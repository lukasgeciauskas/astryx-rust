//! `LogStream`: an experimental streaming log viewer (upstream lab `LogStream`).
//!
//! - Monospaced rows of timestamp, level, source and message, with level accents: a
//!   faint error or warning wash on the row, and the level label in its status color.
//! - A row with `detail` is a disclosure button that shows a detail panel below it.
//! - Follow mode pins the scroll to the newest row as rows append. Scrolling away from the
//!   bottom (more than 24 px) unpins it, and a "Jump to latest" button re-pins it.
//! - `Terminal` draws always-dark chrome whatever the color scheme, as a real shell does.
//! - Rows fade in when they first appear (instant under reduced motion). They are not
//!   virtualized: window a stream of thousands of rows in the caller, as upstream says.
//! - The `log` region announces appended rows politely only while following, so a busy
//!   stream never floods a screen reader (upstream's `aria-live` follows the pinning).

pub mod examples;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use web_time::Instant;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, Hsla, Length, Pixels, Point, Role, ScrollHandle, SharedString,
    StyleRefinement, Window, canvas, div, px, relative, rgb,
};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_components::focus::{focus_ring, focus_ring_inset};
use rustryx_components::overlay::Tooltip;
use rustryx_i18n::{LogicalStyled, use_direction};
use rustryx_primitives::{
    Politeness, announce, control_focus, is_focus_visible, prefers_reduced_motion,
};
use rustryx_theme::{ThemeStyled, Tokens};

/// Severity of one log entry (upstream `LogStreamLevel`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogStreamLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl LogStreamLevel {
    /// The level's name, as upstream renders it (and its `data-level`).
    pub fn as_str(self) -> &'static str {
        match self {
            LogStreamLevel::Info => "info",
            LogStreamLevel::Warn => "warn",
            LogStreamLevel::Error => "error",
            LogStreamLevel::Debug => "debug",
        }
    }
}

type DetailFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type RenderEntryFn = Rc<dyn Fn(&LogEntry, &mut Window, &mut App) -> AnyElement>;
type FollowFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// One row in the stream (upstream `LogEntry`); every string is pre-formatted by the
/// caller.
#[derive(Clone)]
pub struct LogEntry {
    /// Upstream `id`: a stable unique key, such as `"req-1042"`.
    pub id: SharedString,
    /// Upstream `timestamp`: pre-formatted, such as `"14:02:11.482"`.
    pub timestamp: SharedString,
    /// Upstream `level`.
    pub level: LogStreamLevel,
    /// Upstream `message`.
    pub message: SharedString,
    /// Upstream `source`: the emitting service, such as `"api-gateway"`.
    pub source: Option<SharedString>,
    /// Upstream `detail`: built when the row is expanded; with it the row is a
    /// disclosure button for this panel.
    pub detail: Option<DetailFn>,
}

impl LogEntry {
    /// An entry of `level` with its `id`, `timestamp` and `message`.
    pub fn new(
        id: impl Into<SharedString>,
        timestamp: impl Into<SharedString>,
        level: LogStreamLevel,
        message: impl Into<SharedString>,
    ) -> Self {
        LogEntry {
            id: id.into(),
            timestamp: timestamp.into(),
            level,
            message: message.into(),
            source: None,
            detail: None,
        }
    }

    /// Upstream `source`.
    pub fn source(mut self, source: impl Into<SharedString>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Upstream `detail`: the panel an expanded row shows.
    pub fn detail(
        mut self,
        detail: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.detail = Some(Rc::new(detail));
        self
    }
}

/// Upstream `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogStreamVariant {
    /// The theme's surface.
    #[default]
    Default,
    /// Always-dark terminal chrome, whatever the color scheme.
    Terminal,
}

impl LogStreamVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            LogStreamVariant::Default => "default",
            LogStreamVariant::Terminal => "terminal",
        }
    }
}

/// Upstream `FOLLOW_THRESHOLD_PX`: the user counts as scrolled away beyond this distance
/// from the bottom.
pub const FOLLOW_THRESHOLD_PX: f32 = 24.;

/// The label's default (upstream `label`'s `'Log stream'`).
pub const DEFAULT_LABEL: &str = "Log stream";

/// The "Jump to latest" text (upstream's literal).
pub const JUMP_TO_LATEST: &str = "Jump to latest \u{2193}";

/// Upstream's `TERM` grays: the terminal variant is always dark on purpose (terminal
/// chrome is a brand surface, as real shells are), so these are fixed values, not theme
/// tokens that would flip with the scheme.
mod term {
    pub const SURFACE: u32 = 0x0a0a0a;
    pub const SURFACE_RAISED: u32 = 0x141417;
    pub const BORDER: u32 = 0x26262a;
    pub const TEXT_DIM: u32 = 0x8b8b94;
    pub const TEXT: u32 = 0xb9b9c0;
    pub const TEXT_BRIGHT: u32 = 0xe8e8ea;
    pub const WARN: u32 = 0xf2c00b;
    pub const ERROR: u32 = 0xff6166;
    /// The detail panel's fill.
    pub const DETAIL: u32 = 0x0e0e10;
    /// The jump button's hover fill.
    pub const JUMP_HOVER: u32 = 0x1d1d21;
}

/// The row's column widths in px (upstream `colsFull`, `colsNoSource`,
/// `colsNoTimestamp`, `colsMessageOnly`): the timestamp (96), level (52) and source
/// (128) columns that show; the message takes the rest.
pub fn columns(timestamps: bool, source: bool) -> &'static [f32] {
    match (timestamps, source) {
        (true, true) => &[96., 52., 128.],
        (true, false) => &[96., 52.],
        (false, true) => &[52., 128.],
        (false, false) => &[52.],
    }
}

/// Whether the viewport counts as at the bottom: within [`FOLLOW_THRESHOLD_PX`] of it
/// (upstream's `scrollHeight - scrollTop - clientHeight`). `offset` is GPUI's scroll
/// offset (negative as content scrolls up) and `max` its largest distance.
pub fn at_bottom(offset: Pixels, max: Pixels) -> bool {
    f32::from(max + offset) <= FOLLOW_THRESHOLD_PX
}

/// A level label's color (upstream `LEVEL_STYLE` and `LEVEL_STYLE_TERMINAL`).
pub fn level_color(level: LogStreamLevel, variant: LogStreamVariant, t: &Tokens) -> Hsla {
    match variant {
        LogStreamVariant::Default => match level {
            LogStreamLevel::Info => t.color.text_secondary,
            LogStreamLevel::Debug => t.color.text_disabled,
            // Plain-surface warning and error labels take their foreground roles.
            LogStreamLevel::Warn => t.color.text_warning,
            LogStreamLevel::Error => t.color.text_error,
        },
        LogStreamVariant::Terminal => match level {
            LogStreamLevel::Info => rgb(term::TEXT).into(),
            LogStreamLevel::Debug => rgb(term::TEXT_DIM).into(),
            LogStreamLevel::Warn => rgb(term::WARN).into(),
            LogStreamLevel::Error => rgb(term::ERROR).into(),
        },
    }
}

/// A message's color (upstream `messageTerminal`, `messageErrorTerminal` and
/// `messageWarnTerminal`): the terminal's own text, warning and error colors; `None`
/// keeps the row's.
pub fn message_color(level: LogStreamLevel, variant: LogStreamVariant) -> Option<Hsla> {
    match (variant, level) {
        (LogStreamVariant::Terminal, LogStreamLevel::Error) => Some(rgb(term::ERROR).into()),
        (LogStreamVariant::Terminal, LogStreamLevel::Warn) => Some(rgb(term::WARN).into()),
        (LogStreamVariant::Terminal, _) => Some(rgb(term::TEXT).into()),
        (LogStreamVariant::Default, _) => None,
    }
}

/// A row's wash (upstream `rowError` and `rowWarn`): `color-mix(in srgb, error 6%,
/// transparent)` and warning at 5%, in the default variant only.
pub fn row_tint(level: LogStreamLevel, variant: LogStreamVariant, t: &Tokens) -> Option<Hsla> {
    let mix = |c: Hsla, share: f32| Hsla {
        a: c.a * share,
        ..c
    };
    match (variant, level) {
        (LogStreamVariant::Default, LogStreamLevel::Error) => Some(mix(t.color.error, 0.06)),
        (LogStreamVariant::Default, LogStreamLevel::Warn) => Some(mix(t.color.warning, 0.05)),
        _ => None,
    }
}

/// An experimental streaming log viewer (upstream lab `LogStream`).
///
/// ```ignore
/// LogStream::new("build-log", entries)
///     .max_height(px(480.))
///     .following(following)
///     .on_follow_change(|following, _, cx| ..)
/// ```
#[derive(IntoElement)]
pub struct LogStream {
    id: ElementId,
    entries: Vec<LogEntry>,
    variant: LogStreamVariant,
    following: Option<bool>,
    on_follow_change: Option<FollowFn>,
    max_height: Option<Length>,
    timestamps: bool,
    label: SharedString,
    render_entry: Option<RenderEntryFn>,
    style: StyleRefinement,
}

impl LogStream {
    /// A log of `entries` (upstream `entries`: oldest first; live tails append at the
    /// end); `id` keys its scroll, follow and expansion state.
    pub fn new(id: impl Into<ElementId>, entries: impl IntoIterator<Item = LogEntry>) -> Self {
        LogStream {
            id: id.into(),
            entries: entries.into_iter().collect(),
            variant: LogStreamVariant::Default,
            following: None,
            on_follow_change: None,
            max_height: None,
            timestamps: true,
            label: DEFAULT_LABEL.into(),
            render_entry: None,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `variant`.
        variant: LogStreamVariant,
        /// Upstream `isFollowing`: controlled follow pinning; uncontrolled (and at first
        /// unpinned) when unset.
        ?following: bool,
        /// Upstream `maxHeight`: the scroll area's height before it scrolls.
        ?max_height: Length,
        /// Upstream `hasTimestamps`: the timestamp column (on by default).
        !timestamps,
        /// Upstream `label`: the log region's accessible name (`"Log stream"`).
        label: SharedString,
    }

    /// Upstream `onFollowChange`: told when pinning changes (a scroll away from the
    /// bottom, or "Jump to latest").
    pub fn on_follow_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_follow_change = Some(Rc::new(f));
        self
    }

    /// Upstream `renderEntry`: replaces the default row for every entry.
    pub fn render_entry(
        mut self,
        f: impl Fn(&LogEntry, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_entry = Some(Rc::new(f));
        self
    }
}

impl Styled for LogStream {
    /// Upstream `xstyle`: the root.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a stream keeps between frames.
struct StreamState {
    /// Upstream `internalFollowing`: the uncontrolled pinning.
    following: bool,
    /// Upstream `isAtBottom`: updated from scroll events only.
    at_bottom: bool,
    /// Upstream `expandedIds`.
    expanded: HashSet<SharedString>,
    scroll: ScrollHandle,
    /// The scroll offset last frame ended at; a change is a scroll event.
    last_offset: Option<Point<Pixels>>,
    /// The pinning effect's dependencies when it last ran (upstream
    /// `[following, entries.length]`).
    pinned_for: Option<(bool, usize)>,
    /// When each row first appeared, for its fade in; `None` until the first frame.
    seen: Option<HashMap<SharedString, Instant>>,
}

/// The row's fade in (upstream `@starting-style` with `--duration-fast-min` ease-out).
fn ease_out(p: f32) -> f32 {
    rustryx_theme::CubicBezier(0., 0., 0.58, 1.).ease(p)
}

impl RenderOnce for LogStream {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let now = cx.background_executor().now();
        let key = self.id.to_string();
        let terminal = self.variant == LogStreamVariant::Terminal;
        let initial_following = self.following.unwrap_or(false);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{key}-stream").into()),
            cx,
            move |_, _| StreamState {
                following: initial_following,
                at_bottom: true,
                expanded: HashSet::new(),
                scroll: ScrollHandle::new(),
                last_offset: None,
                pinned_for: None,
                seen: None,
            },
        );
        let controlled = self.following.is_some();
        let following = self.following.unwrap_or(state.read(cx).following);
        let set_following: FollowFn = {
            let state = state.clone();
            let on_change = self.on_follow_change.clone();
            Rc::new(move |next, window, cx| {
                if next == following {
                    return;
                }
                if !controlled {
                    state.update(cx, |s, cx| {
                        s.following = next;
                        cx.notify();
                    });
                }
                if let Some(f) = &on_change {
                    f(next, window, cx);
                }
                window.refresh();
            })
        };

        // Pin to the newest row while following (upstream's effect on following and the
        // entry count); it lands after layout, so rapid appends never queue animations.
        let deps = (following, self.entries.len());
        let scroll = state.update(cx, |s, _| {
            if s.pinned_for != Some(deps) {
                s.pinned_for = Some(deps);
                if following {
                    s.scroll.scroll_to_bottom();
                    s.at_bottom = true;
                }
            }
            s.scroll.clone()
        });

        // Rows: when each first appeared (they fade in), and what to announce.
        let reduced = prefers_reduced_motion(cx);
        let fade = if reduced {
            Duration::ZERO
        } else {
            t.duration.fast_min
        };
        let has_source = self.entries.iter().any(|e| e.source.is_some());
        let (appeared, fresh) = state.update(cx, |s, _| {
            let first_frame = s.seen.is_none();
            let seen = s.seen.get_or_insert_with(HashMap::new);
            let mut fresh = Vec::new();
            let mut appeared = HashMap::new();
            for entry in &self.entries {
                let at = *seen.entry(entry.id.clone()).or_insert_with(|| {
                    if !first_frame {
                        fresh.push(entry.id.clone());
                    }
                    now
                });
                appeared.insert(entry.id.clone(), at);
            }
            seen.retain(|id, _| appeared.contains_key(id));
            (appeared, fresh)
        });
        // The log region is polite only while following; unfollowed it is silent.
        if following && !fresh.is_empty() {
            let text: Vec<String> = self
                .entries
                .iter()
                .filter(|e| fresh.contains(&e.id))
                .map(|e| {
                    let mut parts: Vec<&str> = Vec::new();
                    if self.timestamps {
                        parts.push(&e.timestamp);
                    }
                    parts.push(e.level.as_str());
                    if let Some(source) = &e.source {
                        parts.push(source);
                    }
                    parts.push(&e.message);
                    parts.join(" ")
                })
                .collect();
            announce(text.join("\n"), Politeness::Polite, window, cx);
        }
        let mut fading = false;
        let opacity = |id: &SharedString, fading: &mut bool| -> f32 {
            let Some(at) = appeared.get(id) else {
                return 1.;
            };
            if fade.is_zero() {
                return 1.;
            }
            let p = now.saturating_duration_since(*at).as_secs_f32() / fade.as_secs_f32();
            if p < 1. {
                *fading = true;
            }
            ease_out(p.min(1.))
        };

        let widths = columns(self.timestamps, has_source);
        let expanded_ids = state.read(cx).expanded.clone();
        let count = self.entries.len();
        let mut rows: Vec<AnyElement> = Vec::with_capacity(count);
        for (i, entry) in self.entries.iter().enumerate() {
            let alpha = opacity(&entry.id, &mut fading);
            if let Some(render) = &self.render_entry {
                rows.push(
                    div()
                        .id(ElementId::Name(format!("{key}-entry-{}", entry.id).into()))
                        .opacity(alpha)
                        .child(render(entry, window, cx))
                        .into_any_element(),
                );
                continue;
            }
            let expandable = entry.detail.is_some();
            let expanded = expandable && expanded_ids.contains(&entry.id);
            let last = i + 1 == count && !expanded;
            let row_id = ElementId::Name(format!("{key}-row-{}", entry.id).into());
            let mut cells: Vec<AnyElement> = Vec::new();
            let mut widths_iter = widths.iter();
            if self.timestamps {
                let w = *widths_iter.next().unwrap_or(&96.);
                cells.push(
                    div()
                        .flex_none()
                        .w(px(w))
                        .whitespace_nowrap()
                        .font_features(gpui::FontFeatures(std::sync::Arc::new(vec![(
                            "tnum".into(),
                            1,
                        )])))
                        .text_color(if terminal {
                            rgb(term::TEXT_DIM).into()
                        } else {
                            t.color.text_secondary
                        })
                        .child(entry.timestamp.clone())
                        .into_any_element(),
                );
            }
            let level_width = *widths_iter.next().unwrap_or(&52.);
            cells.push(
                div()
                    .flex_none()
                    .w(px(level_width))
                    .text_size(t.font.size.xs)
                    .font_weight(t.font.weight.semibold)
                    .text_color(level_color(entry.level, self.variant, t))
                    .debug_selector({
                        let name =
                            format!("log-stream-level-{}-{}", entry.id, entry.level.as_str());
                        move || name
                    })
                    // Upstream `textTransform: uppercase`.
                    .child(entry.level.as_str().to_uppercase())
                    .into_any_element(),
            );
            if has_source {
                let w = *widths_iter.next().unwrap_or(&128.);
                let source = entry.source.clone().unwrap_or_default();
                let cell = div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(if terminal {
                        rgb(term::TEXT_DIM).into()
                    } else {
                        t.color.text_secondary
                    })
                    .child(source.clone());
                // Upstream's `title`: the full source on hover.
                let cell: AnyElement = if source.is_empty() {
                    cell.into_any_element()
                } else {
                    Tooltip::new(
                        ElementId::Name(format!("{key}-source-{}", entry.id).into()),
                        source,
                    )
                    .child(cell)
                    .into_any_element()
                };
                cells.push(
                    div()
                        .flex_none()
                        .w(px(w))
                        .min_w_0()
                        .child(cell)
                        .into_any_element(),
                );
            }
            let message_color = message_color(entry.level, self.variant);
            let mut message = div()
                .flex_1()
                .min_w_0()
                .debug_selector({
                    let name = format!("log-stream-message-{}", entry.id);
                    move || name
                })
                .child(entry.message.clone());
            if let Some(color) = message_color {
                message = message.text_color(color);
            }
            cells.push(message.into_any_element());

            let mut row = div()
                .id(row_id.clone())
                .debug_selector({
                    let name = format!("log-stream-row-{}", entry.id);
                    move || name
                })
                .relative()
                .flex()
                .flex_row_logical(dir)
                .items_baseline()
                .gap(t.spacing.s3)
                .px(t.spacing.s3)
                .line_height(relative(1.7))
                .text_color(if terminal {
                    rgb(term::TEXT).into()
                } else {
                    t.color.text_primary
                })
                .opacity(alpha);
            row = if terminal {
                row.py(px(1.))
            } else {
                let row = row.py(t.spacing.s1).border_color(t.color.border);
                if last {
                    row
                } else {
                    row.border_b(t.border.width)
                }
            };
            if let Some(tint) = row_tint(entry.level, self.variant, t) {
                row = row.bg(tint);
            }
            if !expandable {
                rows.push(row.children(cells).into_any_element());
                continue;
            }
            let focus = control_focus(&row_id, window, cx);
            let hover = if terminal {
                rgb(term::SURFACE_RAISED).into()
            } else {
                t.color.overlay_hover
            };
            let toggle_state = state.clone();
            let id = entry.id.clone();
            let mut row = row
                .w_full()
                .cursor_pointer()
                .track_focus(&focus)
                .role(Role::Button)
                .aria_expanded(expanded)
                .hover(move |s| s.bg(hover))
                .on_click(move |_, window, cx| {
                    toggle_state.update(cx, |s, cx| {
                        if !s.expanded.remove(&id) {
                            s.expanded.insert(id.clone());
                        }
                        cx.notify();
                    });
                    window.refresh();
                })
                .children(cells);
            if is_focus_visible(&focus, window) {
                row = row.child(focus_ring_inset(t, px(0.), rem));
            }
            rows.push(row.into_any_element());
            if expanded && let Some(detail) = &entry.detail {
                let mut panel = div()
                    .debug_selector({
                        let name = format!("log-stream-detail-{}", entry.id);
                        move || name
                    })
                    .p(t.spacing.s3)
                    .border_b(t.border.width);
                panel = if terminal {
                    panel
                        .bg(rgb(term::DETAIL))
                        .border_color(rgb(term::BORDER))
                        .text_color(rgb(term::TEXT))
                } else {
                    panel
                        .bg(t.color.background_muted)
                        .border_color(t.color.border)
                };
                rows.push(panel.child(detail(window, cx)).into_any_element());
            }
        }
        if fading {
            window.request_animation_frame();
        }

        let label = self.label.clone();
        let mut scroller = div()
            .id(ElementId::Name(format!("{key}-log").into()))
            .debug_selector(|| "log-stream-scroller".into())
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .role(Role::Log)
            .aria_label(label)
            .children(rows);
        if let Some(max) = self.max_height {
            scroller = scroller.max_h(max);
        }

        // A change of the scroll offset is a scroll event (upstream `onScroll`): it
        // updates whether the viewport sits at the bottom, and a scroll away from the
        // tail unpins. Re-pinning is the "Jump to latest" button's, never a scroll's.
        let probe = {
            let state = state.clone();
            let scroll = scroll.clone();
            let set_following = set_following.clone();
            canvas(
                move |_, window, cx| {
                    let offset = scroll.offset();
                    let moved = state.update(cx, |s, _| {
                        let moved = s.last_offset.is_some_and(|last| last != offset);
                        s.last_offset = Some(offset);
                        moved
                    });
                    if !moved {
                        return;
                    }
                    let bottom = at_bottom(offset.y, scroll.max_offset().y);
                    let state = state.clone();
                    let set_following = set_following.clone();
                    window.defer(cx, move |window, cx| {
                        state.update(cx, |s, cx| {
                            if s.at_bottom != bottom {
                                s.at_bottom = bottom;
                                cx.notify();
                            }
                        });
                        if following && !bottom {
                            set_following(false, window, cx);
                        }
                        window.refresh();
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_0()
        };

        let show_jump = !following && !state.read(cx).at_bottom && !self.entries.is_empty();
        let jump = show_jump.then(|| {
            let jump_id = ElementId::Name(format!("{key}-jump").into());
            let focus = control_focus(&jump_id, window, cx);
            let (bg, hover, border, color) = if terminal {
                (
                    rgb(term::SURFACE_RAISED).into(),
                    rgb(term::JUMP_HOVER).into(),
                    rgb(term::BORDER).into(),
                    rgb(term::TEXT_BRIGHT).into(),
                )
            } else {
                (
                    t.color.background_surface,
                    t.color.background_muted,
                    t.color.border_emphasized,
                    t.color.text_primary,
                )
            };
            let state = state.clone();
            let scroll = scroll.clone();
            let set_following = set_following.clone();
            let mut button = div()
                .id(jump_id)
                .debug_selector(|| "log-stream-jump".into())
                .track_focus(&focus)
                .absolute()
                .bottom(t.spacing.s3)
                .inset_end(dir, t.spacing.s3)
                .py(t.spacing.s1)
                .px(t.spacing.s3)
                .rounded(t.radius.full)
                .border(t.border.width)
                .border_color(border)
                .bg(bg)
                .hover(move |s| s.bg(hover))
                .text_color(color)
                .font_stack(&t.font.family.code)
                .text_size(t.font.size.sm)
                .font_weight(t.font.weight.medium)
                .cursor_pointer()
                .shadow_token(&t.shadow.med)
                .role(Role::Button)
                .on_click(move |_, window, cx| {
                    set_following(true, window, cx);
                    state.update(cx, |s, cx| {
                        s.scroll.scroll_to_bottom();
                        s.at_bottom = true;
                        cx.notify();
                    });
                    scroll.scroll_to_bottom();
                    window.refresh();
                })
                .child(JUMP_TO_LATEST);
            if is_focus_visible(&focus, window) {
                button = button.child(focus_ring(t, t.radius.full.to_pixels(rem), rem));
            }
            button
        });

        let mut root = div()
            .relative()
            .flex()
            .flex_col()
            .rounded(t.radius.element)
            .border(t.border.width)
            .overflow_hidden()
            .font_stack(&t.font.family.code)
            .text_size(t.font.size.sm)
            .debug_selector(|| "log-stream".into());
        root = if terminal {
            root.border_color(rgb(term::BORDER)).bg(rgb(term::SURFACE))
        } else {
            root.border_color(t.color.border)
                .bg(t.color.background_surface)
        };
        theme.refine(
            root.style(),
            "log-stream",
            &[("variant", self.variant.as_str())],
            None,
        );
        root.style().refine(&self.style);
        root.id(self.id).child(scroller).child(probe).children(jump)
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `LogStream`.
pub const LOG_STREAM_DOC: ComponentDoc = ComponentDoc {
    name: "LogStream",
    rust_type: "rustryx::lab::log_stream::LogStream",
    props: &[
        p("entries", RustApi::Argument("entries (LogEntry)")),
        p("variant", RustApi::Method("variant")),
        p("isFollowing", RustApi::Method("following")),
        p("onFollowChange", RustApi::Method("on_follow_change")),
        p("maxHeight", RustApi::Method("max_height")),
        p("hasTimestamps", RustApi::Method("timestamps")),
        p("label", RustApi::Method("label")),
        p("renderEntry", RustApi::Method("render_entry")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
