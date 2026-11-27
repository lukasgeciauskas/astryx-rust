//! Toast and its viewport, upstream `Toast/`: brief notifications that stack
//! in a corner and leave on their own.
//!
//! - [`show_toast`] (upstream `useToast().show`) queues a toast and returns a
//!   handle that dismisses it; a [`ToastViewport`] rendered once in the
//!   window draws the queue. It draws above every layer, dialogs included.
//! - A toast is an inverted surface (an error toast, the inverted error
//!   color) whose text draws in the opposite media appearance; `spacing-4`
//!   padding, 400px wide at most, a dismiss button at the end.
//! - Info toasts hide after 5 seconds by default; error toasts stay until
//!   dismissed. The countdown pauses while the pointer is over the toast,
//!   focus is in it, or the window is in the background.
//! - A `unique_id` deduplicates: a new toast with the same id replaces the
//!   showing one (`Overwrite`) or is dropped (`Ignore`).
//! - It enters and leaves with a fade and a `spacing-2` drift from its
//!   edge over `--duration-fast`; at most `max_visible` show at once.
//! - Shown, its text is announced: politely, or assertively for an error.

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, ElementId, Entity, Global, IntoElement, ParentElement, RenderOnce,
    SharedString, StyleRefinement, Styled, Task, Window, div, point, prelude::*, px,
};
use rustryx_i18n::{LogicalStyled, current as i18n, use_direction};
use rustryx_primitives::{ElementSize, Politeness, announce, prefers_reduced_motion};
use rustryx_theme::{Appearance, Pointer, ThemeStyled, current, scope};
use web_time::Instant;

use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconSize};

/// Upstream `type`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastType {
    #[default]
    Info,
    /// A failure that needs attention: it stays until dismissed.
    Error,
}

/// Upstream `ToastPosition`: where the viewport stacks toasts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastPosition {
    TopEnd,
    TopStart,
    #[default]
    BottomEnd,
    BottomStart,
}

/// Upstream `collisionBehavior`: what a toast with a showing `unique_id`
/// does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastCollision {
    /// Replaces the showing toast.
    #[default]
    Overwrite,
    /// Is dropped.
    Ignore,
}

/// Why a toast went away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastDismissReason {
    /// Its time ran out.
    Auto,
    /// The user dismissed it.
    Manual,
}

type BuildFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type HideFn = Rc<dyn Fn(ToastDismissReason, &mut Window, &mut App)>;

/// What [`show_toast`] shows, upstream `ToastOptions`.
#[derive(Clone)]
pub struct ToastOptions {
    body: BuildFn,
    text: Option<SharedString>,
    kind: ToastType,
    auto_hide: Option<bool>,
    auto_hide_duration: Option<Duration>,
    end_content: Option<BuildFn>,
    unique_id: Option<SharedString>,
    collision: ToastCollision,
    on_hide: Option<HideFn>,
}

impl ToastOptions {
    /// Upstream `body`: the message.
    pub fn new(body: impl Into<SharedString>) -> Self {
        let text: SharedString = body.into();
        let shown = text.clone();
        ToastOptions {
            body: Rc::new(move |_, _| shown.clone().into_any_element()),
            text: Some(text),
            kind: ToastType::Info,
            auto_hide: None,
            auto_hide_duration: None,
            end_content: None,
            unique_id: None,
            collision: ToastCollision::Overwrite,
            on_hide: None,
        }
    }

    /// Upstream `body` as any element, built each frame it shows;
    /// `announcement` is what assistive technology hears.
    pub fn element(
        announcement: impl Into<SharedString>,
        body: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        ToastOptions {
            body: Rc::new(body),
            text: Some(announcement.into()),
            ..ToastOptions::new("")
        }
    }

    /// Upstream `type`.
    pub fn kind(mut self, kind: ToastType) -> Self {
        self.kind = kind;
        self
    }

    /// Upstream `isAutoHide`: on for info, off for errors, by default.
    pub fn auto_hide(mut self, auto_hide: bool) -> Self {
        self.auto_hide = Some(auto_hide);
        self
    }

    /// Upstream `autoHideDuration`: 5 seconds by default.
    pub fn auto_hide_duration(mut self, duration: Duration) -> Self {
        self.auto_hide_duration = Some(duration);
        self
    }

    /// Upstream `endContent`: an Undo button or a link before the dismiss
    /// button.
    pub fn end_content(
        mut self,
        content: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.end_content = Some(Rc::new(content));
        self
    }

    /// Upstream `uniqueID`.
    pub fn unique_id(mut self, id: impl Into<SharedString>) -> Self {
        self.unique_id = Some(id.into());
        self
    }

    /// Upstream `collisionBehavior`.
    pub fn collision(mut self, collision: ToastCollision) -> Self {
        self.collision = collision;
        self
    }

    /// Upstream `onHide`.
    pub fn on_hide(
        mut self,
        f: impl Fn(ToastDismissReason, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_hide = Some(Rc::new(f));
        self
    }
}

struct Entry {
    id: u64,
    options: ToastOptions,
    /// Bumped when an overwrite replaces it: its countdown restarts.
    generation: u64,
    /// When it started leaving.
    exiting_since: Option<Instant>,
    announced: bool,
}

#[derive(Default)]
struct ToastQueue {
    entries: Vec<Entry>,
    next_id: u64,
}

impl Global for ToastQueue {}

/// Dismisses the toast it came from, upstream's `show` return value.
#[derive(Clone)]
pub struct ToastHandle(u64);

impl ToastHandle {
    pub fn dismiss(&self, window: &mut Window, cx: &mut App) {
        dismiss(self.0, ToastDismissReason::Manual, window, cx);
    }
}

/// Shows a toast, upstream `useToast().show`. A [`ToastViewport`] in the
/// window draws it.
pub fn show_toast(options: ToastOptions, window: &mut Window, cx: &mut App) -> ToastHandle {
    let queue = cx.default_global::<ToastQueue>();
    if let Some(unique) = &options.unique_id
        && let Some(existing) = queue
            .entries
            .iter_mut()
            .find(|e| e.exiting_since.is_none() && e.options.unique_id.as_ref() == Some(unique))
    {
        let id = existing.id;
        if options.collision == ToastCollision::Overwrite {
            existing.options = options;
            existing.generation += 1;
            existing.announced = false;
        }
        window.refresh();
        return ToastHandle(id);
    }
    let id = queue.next_id;
    queue.next_id += 1;
    queue.entries.push(Entry {
        id,
        options,
        generation: 0,
        exiting_since: None,
        announced: false,
    });
    window.refresh();
    ToastHandle(id)
}

fn dismiss(id: u64, reason: ToastDismissReason, window: &mut Window, cx: &mut App) {
    let now = cx.background_executor().now();
    let on_hide = cx.try_global::<ToastQueue>().and_then(|q| {
        q.entries
            .iter()
            .find(|e| e.id == id && e.exiting_since.is_none())
            .map(|e| e.options.on_hide.clone())
    });
    let Some(on_hide) = on_hide else { return };
    if let Some(entry) = cx
        .default_global::<ToastQueue>()
        .entries
        .iter_mut()
        .find(|e| e.id == id)
    {
        entry.exiting_since = Some(now);
    }
    if let Some(f) = on_hide {
        f(reason, window, cx);
    }
    window.refresh();
}

/// A toast's countdown between frames.
#[derive(Default)]
struct Countdown {
    generation: u64,
    remaining: Option<Duration>,
    running_since: Option<Instant>,
    hovered: bool,
    timer: Option<Task<()>>,
}

type DismissFn = Rc<dyn Fn(ToastDismissReason, &mut Window, &mut App)>;

/// One toast, upstream `Toast`; normally drawn by a [`ToastViewport`].
#[derive(IntoElement)]
pub struct Toast {
    id: ElementId,
    body: AnyElement,
    kind: ToastType,
    auto_hide: bool,
    auto_hide_duration: Duration,
    exiting: bool,
    end_content: Option<AnyElement>,
    on_dismiss: Option<DismissFn>,
    generation: u64,
    style: StyleRefinement,
}

impl Toast {
    /// Upstream `body`.
    pub fn new(id: impl Into<ElementId>, body: impl IntoElement) -> Self {
        Toast {
            id: id.into(),
            body: body.into_any_element(),
            kind: ToastType::Info,
            auto_hide: true,
            auto_hide_duration: Duration::from_secs(5),
            exiting: false,
            end_content: None,
            on_dismiss: None,
            generation: 0,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `type`.
        kind: ToastType,
        /// Upstream `isAutoHide`.
        auto_hide: bool,
        /// Upstream `isExiting`: it plays its exit.
        !exiting,
    }

    /// Upstream `autoHideDuration`.
    pub fn auto_hide_duration(mut self, duration: Duration) -> Self {
        self.auto_hide_duration = duration;
        self
    }

    /// Upstream `endContent`.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `onDismiss`.
    pub fn on_dismiss(
        mut self,
        f: impl Fn(ToastDismissReason, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }

    fn generation(mut self, generation: u64) -> Self {
        self.generation = generation;
        self
    }
}

impl Styled for Toast {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Toast {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let state: Entity<Countdown> = window.use_keyed_state(
            ElementId::Name(format!("{}-countdown", self.id).into()),
            cx,
            |_, _| Countdown::default(),
        );
        let focus = window
            .use_keyed_state(
                ElementId::Name(format!("{}-focus", self.id).into()),
                cx,
                |_, cx| cx.focus_handle().tab_stop(false),
            )
            .read(cx)
            .clone();

        // The countdown: it runs while nothing holds it.
        let now = cx.background_executor().now();
        let held = state.read(cx).hovered
            || focus.contains_focused(window, cx)
            || !window.is_window_active();
        let on_dismiss = self.on_dismiss.clone();
        let (generation, duration) = (self.generation, self.auto_hide_duration);
        state.update(cx, |s, _| {
            if s.generation != generation {
                *s = Countdown {
                    generation,
                    hovered: s.hovered,
                    ..Countdown::default()
                };
            }
            if !self.auto_hide || self.exiting {
                s.timer = None;
                s.running_since = None;
                return;
            }
            let remaining = *s.remaining.get_or_insert(duration);
            match (held, s.running_since) {
                (true, Some(since)) => {
                    s.remaining = Some(remaining.saturating_sub(now - since));
                    s.running_since = None;
                    s.timer = None;
                }
                (false, None) => s.running_since = Some(now),
                _ => {}
            }
        });
        let needs_timer = {
            let s = state.read(cx);
            s.running_since.is_some() && s.timer.is_none()
        };
        if needs_timer && let Some(remaining) = state.read(cx).remaining {
            let timer = cx.background_executor().timer(remaining);
            let on_dismiss = on_dismiss.clone();
            let weak = state.downgrade();
            let task = window.spawn(cx, async move |cx| {
                timer.await;
                cx.update(|window, cx| {
                    if weak.upgrade().is_some()
                        && let Some(f) = &on_dismiss
                    {
                        f(ToastDismissReason::Auto, window, cx);
                    }
                })
                .ok();
            });
            state.update(cx, |s, _| s.timer = Some(task));
        }

        let error = self.kind == ToastType::Error;
        let surface = if error {
            t.color.background_error_inverted
        } else {
            t.color.background_inverted
        };
        // The text draws in the inverted surface's media appearance.
        let media = match theme.appearance {
            Appearance::Light => Appearance::Dark,
            Appearance::Dark => Appearance::Light,
        };
        let width = f32::from(window.viewport_size().width);
        let media_theme = theme.on_surface(media, width, Pointer::Fine);
        let label = i18n(cx).t("@astryx.toast.dismiss");
        let close_dismiss = on_dismiss.clone();
        let close = IconButton::new(
            ElementId::Name(format!("{}-dismiss", self.id).into()),
            label,
            Icon::new("close").size(IconSize::Sm),
        )
        .variant(ButtonVariant::Ghost)
        .size(ElementSize::Sm)
        .on_click(move |_, window, cx| {
            if let Some(f) = &close_dismiss {
                f(ToastDismissReason::Manual, window, cx);
            }
        });
        let body_line = t.text.body.size.to_pixels(window.rem_size()) * t.text.body.leading;
        let inner = div()
            .flex()
            .items_start()
            .gap(t.spacing.s3)
            .w_full()
            .text_color(media_theme.tokens().color.text_primary)
            .child(div().flex_1().min_w_0().child(self.body))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(t.spacing.s2)
                    .h(body_line)
                    .mr(-t.spacing.s1.to_pixels(window.rem_size()))
                    .children(self.end_content)
                    .child(close),
            );
        let mut el = div()
            .id(self.id.clone())
            .track_focus(&focus)
            .occlude()
            .w(px(400.))
            .max_w_full()
            .p(t.spacing.s4)
            .rounded(t.radius.container)
            .shadow_token(&t.shadow.med)
            .bg(surface)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .on_hover({
                let state = state.clone();
                move |hovered, window, cx| {
                    state.update(cx, |s, _| s.hovered = *hovered);
                    window.refresh();
                }
            })
            .child(scope(media_theme, inner));
        theme.refine(
            el.style(),
            "toast",
            &[("type", if error { "error" } else { "info" })],
            None,
        );
        el.style().refine(&self.style);
        el
    }
}

/// Draws the toast queue in a corner of the window, upstream
/// `ToastViewport`. Render one per window.
#[derive(IntoElement)]
pub struct ToastViewport {
    position: ToastPosition,
    max_visible: usize,
}

impl ToastViewport {
    pub fn new() -> Self {
        ToastViewport {
            position: ToastPosition::BottomEnd,
            max_visible: 5,
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `position`.
        position: ToastPosition,
    }

    /// Upstream `maxVisible`: 0 shows every toast.
    pub fn max_visible(mut self, max: usize) -> Self {
        self.max_visible = max;
        self
    }
}

impl Default for ToastViewport {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for ToastViewport {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let now = cx.background_executor().now();
        let fade = if prefers_reduced_motion(cx) {
            Duration::ZERO
        } else {
            t.duration.fast
        };
        // Drop toasts that finished leaving.
        cx.default_global::<ToastQueue>().entries.retain(|e| {
            e.exiting_since
                .is_none_or(|at| now.saturating_duration_since(at) < fade)
        });
        let mut moving = false;
        let top = matches!(
            self.position,
            ToastPosition::TopEnd | ToastPosition::TopStart
        );
        let drift = t.spacing.s2.to_pixels(window.rem_size());

        // Announce what newly shows.
        let to_announce: Vec<(SharedString, bool)> = {
            let queue = cx.default_global::<ToastQueue>();
            queue
                .entries
                .iter_mut()
                .filter(|e| !e.announced && e.exiting_since.is_none())
                .filter_map(|e| {
                    e.announced = true;
                    e.options
                        .text
                        .clone()
                        .filter(|t| !t.is_empty())
                        .map(|t| (t, e.options.kind == ToastType::Error))
                })
                .collect()
        };
        for (text, assertive) in to_announce {
            let politeness = if assertive {
                Politeness::Assertive
            } else {
                Politeness::Polite
            };
            announce(text, politeness, window, cx);
        }

        let entries: Vec<(u64, ToastOptions, u64, Option<Instant>)> = cx
            .try_global::<ToastQueue>()
            .map(|q| {
                let visible = if self.max_visible == 0 {
                    q.entries.len()
                } else {
                    self.max_visible
                };
                let skip = q.entries.len().saturating_sub(visible);
                q.entries
                    .iter()
                    .skip(skip)
                    .map(|e| (e.id, e.options.clone(), e.generation, e.exiting_since))
                    .collect()
            })
            .unwrap_or_default();
        if entries.is_empty() {
            return div().into_any_element();
        }
        let toasts = entries
            .into_iter()
            .map(|(id, options, generation, exiting)| {
                let element_id = ElementId::NamedInteger("toast".into(), id);
                let kind = options.kind;
                let auto_hide = options.auto_hide.unwrap_or(kind == ToastType::Info);
                let duration = options.auto_hide_duration.unwrap_or(Duration::from_secs(5));
                // Entering from, and leaving toward, its edge.
                let shown_at = window
                    .use_keyed_state(
                        ElementId::NamedInteger("toast-shown".into(), id),
                        cx,
                        move |_, _| now,
                    )
                    .read(cx)
                    .to_owned();
                let progress = |from: Instant| {
                    if fade.is_zero() {
                        1.
                    } else {
                        (now.saturating_duration_since(from).as_secs_f32() / fade.as_secs_f32())
                            .min(1.)
                    }
                };
                let shown = match exiting {
                    Some(at) => 1. - t.ease.standard.ease(progress(at)),
                    None => t.ease.standard.ease(progress(shown_at)),
                };
                if shown < 1. && (exiting.is_some() || progress(shown_at) < 1.) {
                    moving = true;
                }
                let offset = drift * (1. - shown) * if top { -1. } else { 1. };
                let mut toast = Toast::new(element_id, (options.body)(window, cx))
                    .kind(kind)
                    .auto_hide(auto_hide)
                    .auto_hide_duration(duration)
                    .exiting(exiting.is_some())
                    .generation(generation)
                    .on_dismiss(move |reason, window, cx| dismiss(id, reason, window, cx));
                if let Some(end) = &options.end_content {
                    toast = toast.end_content(end(window, cx));
                }
                div()
                    .opacity(shown)
                    .mt(offset.max(px(0.)))
                    .mb((-offset).max(px(0.)))
                    .child(toast)
            })
            .collect::<Vec<_>>();
        if moving {
            window.request_animation_frame();
        }

        let viewport = window.viewport_size();
        let gutter = t.spacing.s4.to_pixels(window.rem_size());
        let (edge_top, edge_bottom, edge_start, edge_end) = toast_viewport_insets(
            gutter,
            window.insets().safe_area,
            dir.is_rtl(),
            rustryx_primitives::modality::coarse_primary_pointer_now(window, cx),
        );
        let mut column = div()
            .absolute()
            .flex()
            .gap(t.spacing.s2)
            .pt(edge_top)
            .pb(edge_bottom)
            .padding_start(dir, edge_start)
            .padding_end(dir, edge_end)
            .max_w(viewport.width);
        column = if top {
            column.top_0().flex_col_reverse()
        } else {
            column.bottom_0().flex_col()
        };
        column = match self.position {
            ToastPosition::TopEnd | ToastPosition::BottomEnd => {
                column.inset_end(dir, px(0.)).items_end()
            }
            ToastPosition::TopStart | ToastPosition::BottomStart => {
                column.inset_start(dir, px(0.)).items_start()
            }
        };
        let label = i18n(cx).t("@astryx.toast.viewport");
        let column = column
            .id("astryx-toast-viewport")
            .role(gpui::Role::Region)
            .aria_label(label)
            .children(toasts);
        // A positioner over the window that takes no presses of its own,
        // drawn above every layer.
        let positioner = div()
            .relative()
            .w(viewport.width)
            .h(viewport.height)
            .child(column);
        gpui::deferred(
            gpui::anchored()
                .position(point(px(0.), px(0.)))
                .child(positioner),
        )
        .with_priority(500)
        .into_any_element()
    }
}

/// The toasts showing now, for tests.
#[cfg(test)]
pub(crate) fn queued(cx: &App) -> usize {
    cx.try_global::<ToastQueue>().map_or(0, |q| {
        q.entries
            .iter()
            .filter(|e| e.exiting_since.is_none())
            .count()
    })
}

/// Upstream ToastViewport's safe-area padding: the top and inline sides keep the
/// gutter or the system's inset, whichever is larger; the bottom keeps the gutter plus
/// the inset, and under a coarse pointer at least 34 px of it (a Face ID iPhone's
/// bottom inset, which Safari reports only with `viewport-fit=cover`). Returns (top,
/// bottom, inline start, inline end).
pub(crate) fn toast_viewport_insets(
    gutter: gpui::Pixels,
    safe: gpui::Edges<gpui::Pixels>,
    rtl: bool,
    coarse: bool,
) -> (gpui::Pixels, gpui::Pixels, gpui::Pixels, gpui::Pixels) {
    let (start, end) = if rtl {
        (safe.right, safe.left)
    } else {
        (safe.left, safe.right)
    };
    let bottom = if coarse {
        safe.bottom.max(px(34.))
    } else {
        safe.bottom
    };
    (
        gutter.max(safe.top),
        gutter + bottom,
        gutter.max(start),
        gutter.max(end),
    )
}

#[cfg(test)]
mod safe_area_tests {
    use super::toast_viewport_insets;
    use gpui::{Edges, px};

    #[test]
    fn keeps_the_gutter_or_the_safe_area_and_under_a_coarse_pointer_clears_34_px_at_the_bottom() {
        let none = Edges::default();
        assert_eq!(
            toast_viewport_insets(px(16.), none, false, false),
            (px(16.), px(16.), px(16.), px(16.))
        );
        // Under a finger the bottom clears 34 px even where no inset is reported.
        assert_eq!(toast_viewport_insets(px(16.), none, false, true).1, px(50.));
        let notch = Edges {
            top: px(47.),
            right: px(0.),
            bottom: px(34.),
            left: px(20.),
        };
        let (top, bottom, start, end) = toast_viewport_insets(px(16.), notch, false, false);
        assert_eq!(
            (top, bottom, start, end),
            (px(47.), px(50.), px(20.), px(16.))
        );
        // A larger reported inset wins over the 34 px floor; RTL swaps the inline sides.
        let tall = Edges {
            bottom: px(48.),
            ..notch
        };
        let (_, bottom, start, end) = toast_viewport_insets(px(16.), tall, true, true);
        assert_eq!((bottom, start, end), (px(64.), px(16.), px(20.)));
    }
}
