//! `ChatReasoning`: a compact, collapsible display of a model's reasoning (upstream lab
//! `ChatReasoning`). Collapsed it is one line: a thinking glyph, the label, the duration
//! and an ellipsized preview of string content; a press or Enter/Space on the line shows
//! the full reasoning below it.
//!
//! - The content stays drawn while collapsed, clipped to no height and inert (out of the
//!   accessibility tree and the tab order), so the open and close transition runs: the
//!   height eases between none and the content's own over `--duration-medium`, as
//!   upstream's `grid-template-rows: 0fr` to `1fr`.
//! - While streaming, a light band sweeps across the label (upstream's 4 s shimmer), and
//!   the duration and the preview are hidden.

pub mod examples;

#[cfg(test)]
mod tests;

use std::f32::consts::PI;
use std::rc::Rc;
use std::time::Duration;

use web_time::Instant;

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use astryx_components::focus::focus_ring;
use astryx_i18n::{LogicalStyled, use_direction};
use astryx_primitives::{control_focus, is_focus_visible};
use astryx_theme::ThemeStyled;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, HighlightStyle, Hsla, Pixels, Rgba, Role, SharedString,
    StyleRefinement, StyledText, TextLayout, Transformation, Window, canvas, div, px, radians,
    relative, svg,
};

type ExpandFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type ToggleFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `ThinkingIcon`: a dashed ring with two dots, 14 px.
const THINKING_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 14 14" fill="none"><circle cx="7" cy="7" r="5.5" stroke="currentColor" stroke-width="1.5" stroke-dasharray="3 2"/><circle cx="5.5" cy="7" r="0.75" fill="currentColor"/><circle cx="8.5" cy="7" r="0.75" fill="currentColor"/></svg>"#;

/// Upstream `ChevronDownIcon`: a 12 px chevron pointing down.
const CHEVRON_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M3 4.5L6 7.5L9 4.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

/// Upstream `shimmerKeyframes`' `animationDuration`: one sweep of the band.
const SHIMMER_PERIOD: Duration = Duration::from_secs(4);

/// The label's default (upstream `label`'s `'Thinking'`).
pub const DEFAULT_LABEL: &str = "Thinking";

/// Which parts of the collapsed line show (upstream's render conditions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineParts {
    /// The "·" and the duration: a duration is given and nothing streams.
    pub duration: bool,
    /// The "—" and the ellipsized preview: collapsed, string content, nothing streaming.
    pub preview: bool,
}

/// What the collapsed line shows for these props (upstream's
/// `duration != null && !isStreaming` and `!isExpanded && previewText && !isStreaming`).
/// An empty string previews nothing, as upstream's falsy `previewText`.
pub fn line_parts(
    has_duration: bool,
    preview_text: Option<&str>,
    streaming: bool,
    expanded: bool,
) -> LineParts {
    LineParts {
        duration: has_duration && !streaming,
        preview: !expanded && preview_text.is_some_and(|t| !t.is_empty()) && !streaming,
    }
}

/// Compact collapsible display for model reasoning or thinking (upstream lab
/// `ChatReasoning`).
///
/// ```ignore
/// ChatReasoning::new("reasoning")
///     .duration("12s")
///     .text("Let me work through the constraints on adjacent fields...")
/// ```
#[derive(IntoElement)]
pub struct ChatReasoning {
    id: ElementId,
    text: Option<SharedString>,
    children: Vec<AnyElement>,
    label: SharedString,
    duration: Option<SharedString>,
    streaming: bool,
    expanded: Option<bool>,
    default_expanded: bool,
    on_expanded_change: Option<ExpandFn>,
    style: StyleRefinement,
}

impl ChatReasoning {
    /// A reasoning display; `id` keys its uncontrolled state and its parts.
    pub fn new(id: impl Into<ElementId>) -> Self {
        ChatReasoning {
            id: id.into(),
            text: None,
            children: Vec::new(),
            label: DEFAULT_LABEL.into(),
            duration: None,
            streaming: false,
            expanded: None,
            default_expanded: false,
            on_expanded_change: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream string `children`: the reasoning as plain text, which the collapsed line
    /// also previews. Element children (Markdown, say) come through `ParentElement`
    /// and are not previewed.
    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Upstream `label`: the header label (`"Thinking"` by default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// Upstream `duration`: shown after the label, such as `"12s"`.
    pub fn duration(mut self, duration: impl Into<SharedString>) -> Self {
        self.duration = Some(duration.into());
        self
    }

    /// Upstream `isStreaming`: the reasoning is still arriving; the label shimmers and
    /// the duration and the preview hide.
    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    /// Upstream `isExpanded`: controlled; changes are reported through
    /// `on_expanded_change`.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    /// Upstream `defaultIsExpanded`: open at first, uncontrolled (closed by default).
    pub fn default_expanded(mut self, expanded: bool) -> Self {
        self.default_expanded = expanded;
        self
    }

    /// Upstream `onExpandedChange`: told the next state on every toggle.
    pub fn on_expanded_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_expanded_change = Some(Rc::new(f));
        self
    }
}

impl ParentElement for ChatReasoning {
    /// Upstream element `children`: the reasoning as any element.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for ChatReasoning {
    /// Upstream `xstyle`: the root.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The uncontrolled expanded state (upstream `internalExpanded`).
struct Expanded(bool);

/// The content's open fraction over time (upstream's `grid-template-rows` transition).
struct Reveal {
    /// Where the current transition started and is going, as fractions of the content's
    /// height.
    from: f32,
    to: f32,
    since: Option<Instant>,
    /// This transition's length: the full duration for the distance it covers.
    length: Duration,
    /// The content's own height, measured last frame.
    height: Option<Pixels>,
}

impl Reveal {
    fn fraction(&self, now: Instant, ease: impl Fn(f32) -> f32) -> f32 {
        let Some(since) = self.since else {
            return self.to;
        };
        if self.length.is_zero() {
            return self.to;
        }
        let p = (now.saturating_duration_since(since).as_secs_f32() / self.length.as_secs_f32())
            .min(1.);
        self.from + (self.to - self.from) * ease(p)
    }

    fn moving(&self, now: Instant) -> bool {
        self.since
            .is_some_and(|since| now.saturating_duration_since(since) < self.length)
    }
}

/// The shimmer's clock and the label's glyph positions from the last frame.
#[derive(Default)]
struct Shimmer {
    since: Option<Instant>,
    layout: Option<(SharedString, TextLayout)>,
}

/// A color on upstream's shimmer gradient (`linear-gradient(90deg, secondary 0%,
/// disabled 50%, secondary 100%)` at `background-size: 200% 100%`, its position moving
/// from `200% 0` to `-200% 0` over [`SHIMMER_PERIOD`], repeating): `x` is the
/// glyph's place across the label (0 to 1), `elapsed` the time into the sweep.
pub fn shimmer_color(x: f32, elapsed: Duration, secondary: Hsla, disabled: Hsla) -> Hsla {
    let period = SHIMMER_PERIOD.as_secs_f32();
    let t = (elapsed.as_secs_f32() % period) / period;
    // The image is twice the label's width and starts at -2W + 4W·t; it repeats, so the
    // point under x sits at (x + 2 - 4t) / 2 of an image.
    let u = ((x + 2. - 4. * t) / 2.).rem_euclid(1.);
    // Secondary at the image's ends, disabled at its middle.
    let w = 1. - (2. * u - 1.).abs();
    mix_premultiplied(secondary, disabled, w)
}

/// CSS gradient interpolation: sRGB, alpha premultiplied.
fn mix_premultiplied(a: Hsla, b: Hsla, w: f32) -> Hsla {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    let alpha = a.a * (1. - w) + b.a * w;
    if alpha <= 0. {
        return Hsla::transparent_black();
    }
    let channel = |x: f32, y: f32| (x * a.a * (1. - w) + y * b.a * w) / alpha;
    Hsla::from(Rgba {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: alpha,
    })
}

impl RenderOnce for ChatReasoning {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let now = cx.background_executor().now();
        let key = self.id.to_string();
        let default_expanded = self.default_expanded;
        let state = window.use_keyed_state(
            ElementId::Name(format!("{key}-expanded").into()),
            cx,
            move |_, _| Expanded(default_expanded),
        );
        let controlled = self.expanded.is_some();
        let expanded = self.expanded.unwrap_or(state.read(cx).0);

        let toggle: ToggleFn = {
            let state = state.clone();
            let on_change = self.on_expanded_change.clone();
            Rc::new(move |window, cx| {
                let next = !expanded;
                if !controlled {
                    state.update(cx, |s, cx| {
                        s.0 = next;
                        cx.notify();
                    });
                }
                if let Some(f) = &on_change {
                    f(next, window, cx);
                }
                window.refresh();
            })
        };

        // The content's transition (duration-medium, ease-standard), reversible midway.
        let medium = t.duration.medium;
        let ease = t.ease.standard;
        let reveal = window.use_keyed_state(
            ElementId::Name(format!("{key}-reveal").into()),
            cx,
            move |_, _| {
                let at = if default_expanded { 1. } else { 0. };
                Reveal {
                    from: at,
                    to: at,
                    since: None,
                    length: Duration::ZERO,
                    height: None,
                }
            },
        );
        let target = if expanded { 1. } else { 0. };
        reveal.update(cx, |r, _| {
            if r.to != target {
                let current = r.fraction(now, |p| ease.ease(p));
                r.from = current;
                r.to = target;
                r.since = Some(now);
                r.length = medium.mul_f32((target - current).abs());
            }
        });
        let (fraction, moving, measured) = {
            let r = reveal.read(cx);
            (r.fraction(now, |p| ease.ease(p)), r.moving(now), r.height)
        };
        if moving {
            window.request_animation_frame();
        }

        // The header: a button of its own (upstream `role="button"`, `tabIndex={0}`).
        let header_id = ElementId::Name(format!("{key}-header").into());
        let content_name: SharedString = format!("{key}-content").into();
        let focus = control_focus(&header_id, window, cx);
        let parts = line_parts(
            self.duration.is_some(),
            self.text.as_deref(),
            self.streaming,
            expanded,
        );
        let supporting = &t.text.supporting;
        let label_text = self.label.clone();
        let shimmer = window.use_keyed_state(
            ElementId::Name(format!("{key}-shimmer").into()),
            cx,
            |_, _| Shimmer::default(),
        );
        // The sweep starts over each time streaming starts, as a CSS animation restarts.
        if !self.streaming {
            shimmer.update(cx, |s, _| s.since = None);
        }
        let label = if self.streaming {
            let (since, last) =
                shimmer.update(cx, |s, _| (*s.since.get_or_insert(now), s.layout.clone()));
            window.request_animation_frame();
            let elapsed = now.saturating_duration_since(since);
            let (secondary, disabled) = (t.color.text_secondary, t.color.text_disabled);
            // Each glyph takes the gradient at its center: from last frame's layout, else
            // by its place in the string.
            let layout = last.filter(|(text, _)| *text == label_text).map(|(_, l)| l);
            let width = layout.as_ref().map(|l| l.bounds().size.width);
            let count = label_text.chars().count().max(1);
            let highlights: Vec<_> = label_text
                .char_indices()
                .enumerate()
                .map(|(i, (at, ch))| {
                    let end = at + ch.len_utf8();
                    let x = match (&layout, width) {
                        (Some(layout), Some(width)) if width > px(0.) => {
                            let origin = layout.bounds().origin.x;
                            let start = layout.position_for_index(at).map(|p| p.x - origin);
                            let stop = layout.position_for_index(end).map(|p| p.x - origin);
                            match (start, stop) {
                                (Some(a), Some(b)) => ((a + b) / 2.) / width,
                                _ => (i as f32 + 0.5) / count as f32,
                            }
                        }
                        _ => (i as f32 + 0.5) / count as f32,
                    };
                    let color = shimmer_color(x, elapsed, secondary, disabled);
                    (
                        at..end,
                        HighlightStyle {
                            color: Some(color),
                            ..HighlightStyle::default()
                        },
                    )
                })
                .collect();
            let styled = StyledText::new(label_text.clone()).with_highlights(highlights);
            let layout = styled.layout().clone();
            shimmer.update(cx, |s, _| s.layout = Some((label_text.clone(), layout)));
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_style(supporting)
                .font_stack(&t.font.family.body)
                .font_weight(t.font.weight.medium)
                .text_color(secondary)
                .debug_selector(|| "chat-reasoning-label".into())
                .child(styled)
        } else {
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_style(supporting)
                .font_stack(&t.font.family.body)
                .font_weight(t.font.weight.medium)
                .text_color(t.color.text_secondary)
                .debug_selector(|| "chat-reasoning-label".into())
                .child(label_text)
        };
        let separator = |glyph: &'static str| {
            div()
                .flex_none()
                .text_size(supporting.size)
                .line_height(relative(supporting.leading))
                .text_color(t.color.text_disabled)
                .child(glyph)
        };
        let duration = self.duration.clone().filter(|_| parts.duration).map(|d| {
            [
                separator("\u{b7}").into_any_element(),
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_style(supporting)
                    .font_stack(&t.font.family.body)
                    .text_color(t.color.text_disabled)
                    .debug_selector(|| "chat-reasoning-duration".into())
                    .child(d)
                    .into_any_element(),
            ]
        });
        let preview = self.text.clone().filter(|_| parts.preview).map(|text| {
            [
                separator("\u{2014}").into_any_element(),
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_style(supporting)
                    .font_stack(&t.font.family.body)
                    .text_color(t.color.text_disabled)
                    .debug_selector(|| "chat-reasoning-preview".into())
                    .child(text)
                    .into_any_element(),
            ]
        });
        let label_row = div()
            .flex()
            .flex_row_logical(dir)
            .items_center()
            .gap(t.spacing.s1)
            .min_w_0()
            .overflow_hidden()
            .child(label)
            .children(duration.into_iter().flatten())
            .children(preview.into_iter().flatten());

        // The chevron turns half over on duration-fast.
        let turn = {
            let fast = t.duration.fast;
            let state = window.use_keyed_state(
                ElementId::Name(format!("{key}-chevron").into()),
                cx,
                move |_, _| (if expanded { 1f32 } else { 0. }, None::<(f32, Instant)>),
            );
            let target = if expanded { 1. } else { 0. };
            let (turn, moving) = state.update(cx, |(to, from), _| {
                let at = |from: &Option<(f32, Instant)>, to: f32| match from {
                    Some((start, since)) if !fast.is_zero() => {
                        let p = (now.saturating_duration_since(*since).as_secs_f32()
                            / fast.as_secs_f32())
                        .min(1.);
                        (start + (to - start) * ease.ease(p), p < 1.)
                    }
                    _ => (to, false),
                };
                if *to != target {
                    let (current, _) = at(from, *to);
                    *from = Some((current, now));
                    *to = target;
                }
                at(from, *to)
            });
            if moving {
                window.request_animation_frame();
            }
            turn
        };
        let chevron = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(14.))
            .text_color(t.color.text_disabled)
            .child(
                svg()
                    .data(CHEVRON_SVG)
                    .size(px(12.))
                    .text_color(t.color.text_disabled)
                    .with_transformation(Transformation::rotate(radians(PI * turn))),
            );
        let icon = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(16.))
            .text_color(t.color.text_secondary)
            .child(
                svg()
                    .data(THINKING_SVG)
                    .size(px(14.))
                    .text_color(t.color.text_secondary),
            );

        let on_key = toggle.clone();
        let on_click = toggle.clone();
        let mut header = div()
            .id(header_id)
            .debug_selector(|| "chat-reasoning-header".into())
            .track_focus(&focus)
            .relative()
            .flex()
            .flex_row_logical(dir)
            .items_center()
            .gap(t.spacing.s1_5)
            .cursor_pointer()
            .min_h(px(24.))
            .py(t.spacing.s0_5)
            .role(Role::Button)
            .aria_expanded(expanded)
            .aria_controls([content_name.clone()])
            // Enter and Space toggle on key down, as upstream's `onKeyDown`.
            .on_key_down(move |event, window, cx| {
                let key = event.keystroke.key.as_str();
                if (key == "enter" || key == "space") && !event.keystroke.modifiers.modified() {
                    window.prevent_default();
                    cx.stop_propagation();
                    on_key(window, cx);
                }
            })
            .on_click(move |event, window, cx| {
                // The keyboard toggled on key down already.
                if event.is_keyboard() {
                    return;
                }
                on_click(window, cx);
            })
            .child(icon)
            .child(label_row)
            .child(chevron);
        if is_focus_visible(&focus, window) {
            header = header.child(focus_ring(t, px(0.), rem));
        }

        // The content: always drawn, clipped by its open fraction, inert while collapsed.
        let probe = {
            let reveal = reveal.clone();
            canvas(
                move |bounds, window, cx| {
                    let height = bounds.size.height;
                    let changed = reveal.update(cx, |r, _| {
                        let changed = r.height != Some(height);
                        r.height = Some(height);
                        changed
                    });
                    if changed {
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };
        let padding = div()
            .relative()
            .pt(t.spacing.s2)
            .padding_start(dir, px(16.) + t.spacing.s1_5.to_pixels(rem))
            .text_style(supporting)
            .font_stack(&t.font.family.body)
            .text_color(t.color.text_secondary)
            .children(self.text.clone())
            .children(self.children)
            .child(probe);
        let settled_open = fraction >= 1. && !moving;
        let mut clip = div().min_h_0().overflow_hidden();
        if !settled_open {
            clip = clip.h(measured.unwrap_or_default() * fraction);
        }
        let content = div()
            .id(ElementId::Name(content_name.clone()))
            .debug_selector(|| "chat-reasoning-content".into())
            // A named node, so the header's aria-controls resolves while it is open.
            .role(Role::Group)
            .aria_id(content_name)
            .inert(!expanded)
            .child(clip.child(padding));

        let mut root = div()
            .flex()
            .flex_col()
            .mt(t.spacing.s2)
            .debug_selector(|| "chat-reasoning".into());
        theme.refine(
            root.style(),
            "chat-reasoning",
            &[
                ("expanded", if expanded { "expanded" } else { "" }),
                ("streaming", if self.streaming { "streaming" } else { "" }),
            ],
            None,
        );
        root.style().refine(&self.style);
        root.id(self.id).child(header).child(content)
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `ChatReasoning`.
pub const CHAT_REASONING_DOC: ComponentDoc = ComponentDoc {
    name: "ChatReasoning",
    rust_type: "astryx::lab::chat_reasoning::ChatReasoning",
    props: &[
        p(
            "children",
            RustApi::Method(
                "text (a string, previewed when collapsed) or ParentElement (elements)",
            ),
        ),
        p("label", RustApi::Method("label")),
        p("duration", RustApi::Method("duration")),
        p("isStreaming", RustApi::Method("streaming")),
        p("isExpanded", RustApi::Method("expanded")),
        p("defaultIsExpanded", RustApi::Method("default_expanded")),
        p("onExpandedChange", RustApi::Method("on_expanded_change")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
