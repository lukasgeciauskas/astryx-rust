//! `PageControl`: a row of page indicators that pages one step per tap and follows a
//! scrubbing pointer, like UIPageControl (upstream lab `PageControl`).
//!
//! It draws UIPageControl's measured geometry ([`model`]): 9.67 px indicators 8 px
//! apart, 14 px of padding at each end and 25.67 px tall. At most ten indicator slots
//! show. With more pages, the row is a window. Where pages continue past it, the last
//! two slots on that side shrink. The window moves only when the current page would
//! land in a shrunk slot, on UIKit's spring.
//!
//! The app owns the page (`page` and `on_change`). A tap on the trailing side of the
//! current indicator asks for the next page, and on the leading side for the previous
//! one. A finger that travels 16 px scrubs instead. Past the content's edge, the page
//! keeps stepping on its own. A held or scrubbing finger shows a platter behind the
//! indicators and dims the others.
//!
//! Assistive technology gets one adjustable element: a slider named "Page
//! indicators" whose value reads "Page 3 of 5". The arrow keys, Home and End change
//! the page. Every pointer taps and scrubs. With a fine pointer, the row is cropped to
//! the control and opacity changes are instant.

pub mod examples;
pub mod model;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use astryx_components::gestures::touch_hit_area::{HitLinks, hit_area_with};
use astryx_i18n::{Arg, current as i18n, use_direction};
use astryx_primitives::gestures::host::{
    GestureAttachment, after_gesture, gesture_area, gesture_host,
};
use astryx_primitives::gestures::{
    GestureState, LongPressGestureRecognizer, LongPressOptions, PanDirection, PanGestureRecognizer,
    PanOptions, TapGestureRecognizer, TapOptions,
};
use astryx_primitives::motion::cubic_bezier;
use astryx_primitives::motion::reduced_motion::{ReducedMotionFade, reduced_motion_fade};
use astryx_primitives::motion::{
    SpringCurve, settling_duration, solve_spring, spring_from_response,
};
use astryx_primitives::{
    coarse_primary_pointer, control_focus, is_focus_visible, prefers_reduced_motion,
};
use astryx_theme::current;
use gpui::prelude::*;
use gpui::{
    App, Bounds, ElementId, Entity, IntoElement, KeyDownEvent, MouseButton, Pixels, Role,
    SharedString, StyleRefinement, Task, WeakEntity, Window, canvas, div, point, px,
};
use web_time::Instant;

use model::*;

/// What asked for a page (upstream `ChangeKind`): a tap fades the old indicator faster
/// than anything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    /// Upstream `'tap'`.
    Tap,
    /// Upstream `'other'`: a scrub, its jog, a key or an assistive technology.
    Other,
}

type ChangeFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// A row of page indicators under a paging view, as on iOS (upstream lab
/// `PageControl`).
///
/// ```ignore
/// PageControl::new("pages", 5., page).on_change(|page, _, cx| { /* show it */ })
/// ```
#[derive(IntoElement)]
pub struct PageControl {
    id: ElementId,
    total_pages: f64,
    page: f64,
    on_change: Option<ChangeFn>,
    label: Option<SharedString>,
    hides_for_single_page: bool,
    style: StyleRefinement,
}

impl PageControl {
    /// A control for `total_pages` pages (upstream `totalPages`; a total that is not a
    /// finite number is none) showing `page` (upstream `page`, 1-based: page 1 is the
    /// first page).
    pub fn new(id: impl Into<ElementId>, total_pages: f64, page: f64) -> Self {
        PageControl {
            id: id.into(),
            total_pages,
            page,
            on_change: None,
            label: None,
            hides_for_single_page: false,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onChange`: called with the page, 1-based, that a tap, a click, a scrub,
    /// an arrow key, Home, End or an assistive technology asks for. A scrub calls it each
    /// time the page under the pointer changes, so a quick sweep can skip a page.
    pub fn on_change(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `label`: names the control for assistive technology ("Page indicators",
    /// localized, by default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `hidesForSinglePage`: render nothing when there is only one page.
    pub fn hides_for_single_page(mut self, hides: bool) -> Self {
        self.hides_for_single_page = hides;
        self
    }
}

impl Styled for PageControl {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// An opacity on its way to a target, as a CSS opacity transition draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fade {
    pub from: f64,
    pub to: f64,
    pub start: Instant,
    pub ms: f64,
    pub curve: [f64; 4],
}

impl Fade {
    fn at_rest(value: f64, now: Instant) -> Self {
        Fade {
            from: value,
            to: value,
            start: now,
            ms: 0.,
            curve: [0., 0., 1., 1.],
        }
    }

    /// The opacity at `now`, and whether it still moves.
    pub(crate) fn value(&self, now: Instant) -> (f64, bool) {
        if self.ms <= 0. {
            return (self.to, false);
        }
        let t = now.saturating_duration_since(self.start).as_secs_f64() * 1000. / self.ms;
        if t >= 1. {
            return (self.to, false);
        }
        let eased = cubic_bezier(self.curve).ease(t);
        (self.from + (self.to - self.from) * eased, true)
    }

    /// Heads for `to` from wherever it is now, over `ms` on `curve` (0: at once). A
    /// running transition toward the same value keeps its own duration.
    fn retarget(&mut self, to: f64, ms: f64, curve: [f64; 4], now: Instant) {
        if self.to == to {
            return;
        }
        let (from, _) = self.value(now);
        *self = Fade {
            from,
            to,
            start: now,
            ms,
            curve,
        };
    }
}

/// The window's slide: the strip's offset and every indicator's size on one spring
/// (UIView spring, damping ratio 1, response 0.15), from one start time.
#[derive(Clone, Debug)]
struct Slide {
    from_offset: f64,
    to_offset: f64,
    /// Each rendered page's diameter when the slide started.
    from_sizes: HashMap<usize, f64>,
    /// The old window, for the dots that mount during the slide.
    from_window: usize,
    to_window: usize,
    at: Instant,
    /// Under Reduce Motion the strip's cross-fade carries the whole change: the
    /// offset and the dots' sizes land at its middle.
    fade: Option<ReducedMotionFade>,
}

/// Where a slide stands at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SlideFrame {
    /// Share of the way to the new window, 0 to 1.
    progress: f64,
    /// The strip's opacity.
    opacity: f64,
    /// It has settled.
    done: bool,
}

fn slide_spring() -> SpringCurve {
    let spring =
        spring_from_response(WINDOW_SLIDE_RESPONSE, 1.).expect("the table's spring is valid");
    solve_spring(spring, 0., 1., 0.)
}

impl Slide {
    fn frame(&self, now: Instant) -> SlideFrame {
        let Some(elapsed) = now.checked_duration_since(self.at).map(|d| d.as_secs_f64()) else {
            // It starts windowSlideDelay after the change.
            return SlideFrame {
                progress: 0.,
                opacity: 1.,
                done: false,
            };
        };
        match self.fade {
            Some(fade) => {
                if fade.duration_ms.is_nan() || fade.duration_ms <= 0. {
                    return SlideFrame {
                        progress: 1.,
                        opacity: 1.,
                        done: true,
                    };
                }
                let half = fade.duration_ms / 2.;
                let ms = elapsed * 1000.;
                if ms >= fade.duration_ms {
                    SlideFrame {
                        progress: 1.,
                        opacity: 1.,
                        done: true,
                    }
                } else if ms < half {
                    SlideFrame {
                        progress: 0.,
                        opacity: 1. - fade.ease(ms / half),
                        done: false,
                    }
                } else {
                    SlideFrame {
                        progress: 1.,
                        opacity: fade.ease((ms - half) / half),
                        done: false,
                    }
                }
            }
            None => {
                let spring = slide_spring();
                let settle = settling_duration(spring.spring);
                if elapsed >= settle {
                    SlideFrame {
                        progress: 1.,
                        opacity: 1.,
                        done: true,
                    }
                } else {
                    SlideFrame {
                        progress: spring.value(elapsed),
                        opacity: 1.,
                        done: false,
                    }
                }
            }
        }
    }

    fn offset(&self, progress: f64) -> f64 {
        self.from_offset + (self.to_offset - self.from_offset) * progress
    }

    fn size(&self, pages: usize, page: usize, progress: f64) -> f64 {
        let from = self
            .from_sizes
            .get(&page)
            .copied()
            // Dots that just mounted start at their size in the old window.
            .unwrap_or_else(|| indicator_size(pages, self.from_window, page as isize));
        let to = indicator_size(pages, self.to_window, page as isize);
        from + (to - from) * progress
    }
}

/// A pointer on the control (upstream's `gestureRef`).
#[derive(Clone, Debug, Default)]
struct Gesture {
    start_x: f64,
    last_x: f64,
    scrubbing: bool,
    /// The control's box, read once when the scrub begins: nothing reads layout while
    /// the pointer moves.
    geometry: Option<Bounds<Pixels>>,
    /// With a mouse, the row's dot centres, read once when the scrub begins.
    centres: Vec<(usize, f64)>,
}

/// The page last asked for and what asked (upstream's `requestRef`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pending {
    page: usize,
    kind: ChangeKind,
    sequence: u64,
}

/// The control's state after each render, for gestures and timers (upstream
/// `Latest`).
#[derive(Clone)]
struct Latest {
    current: usize,
    pages: usize,
    window_start: usize,
    touch: bool,
    rtl: bool,
    /// The cropped row's offset as drawn, with a mouse.
    offset: f64,
    on_change: Option<ChangeFn>,
}

/// What the control keeps between frames.
struct PcState {
    window_start: usize,
    interacting: bool,
    pending: Option<Pending>,
    sequence: u64,
    /// The last request's page and kind, for the render that shows its page (a lapsed
    /// request clears `pending` before the app's render can read it).
    asked: Option<(usize, ChangeKind)>,
    gesture: Gesture,
    shown_page: usize,
    shown_window: Option<(usize, usize)>,
    dots: HashMap<usize, Fade>,
    platter: Option<Fade>,
    /// The held look's release under Reduce Motion takes the policy's fade.
    held_release: bool,
    slide: Option<Slide>,
    /// The control's box, as last laid out.
    bounds: Option<Bounds<Pixels>>,
    latest: Latest,
    interact_timer: Option<Task<()>>,
    jog_timer: Option<Task<()>>,
    /// When the pointer crossed the edge or the last jog step was due.
    jog_from: Option<Instant>,
    /// When the pending jog step runs.
    jog_due: Option<Instant>,
}

/// The recognizers that read every pointer on the control: a horizontal pan, a
/// 155 ms hold and a tap.
struct Recognizers {
    attachment: GestureAttachment,
    _pan: PanGestureRecognizer,
    _hold: LongPressGestureRecognizer,
    _tap: TapGestureRecognizer,
}

/// Asks the app for `page` (0-based) unless it is the current page or the one already
/// asked for (upstream `request`). An app that keeps its value never renders the page.
/// Once it has had its chance to, in the next task, the request lapses. Then asking
/// again calls `on_change` again.
fn request(
    state: &Entity<PcState>,
    page: usize,
    kind: ChangeKind,
    window: &mut Window,
    cx: &mut App,
) {
    let (current, pending, on_change) = {
        let s = state.read(cx);
        (s.latest.current, s.pending, s.latest.on_change.clone())
    };
    if page == current || pending.is_some_and(|p| p.page == page) {
        return;
    }
    let sequence = state.update(cx, |s, _| {
        s.sequence += 1;
        s.pending = Some(Pending {
            page,
            kind,
            sequence: s.sequence,
        });
        s.asked = Some((page, kind));
        s.sequence
    });
    if let Some(f) = on_change {
        f(page + 1, window, cx);
    }
    let lapse = state.downgrade();
    window.defer(cx, move |_, cx| {
        let _ = lapse.update(cx, |s, _| {
            if s.pending.is_some_and(|p| p.sequence == sequence) {
                s.pending = None;
            }
        });
    });
}

/// Px along the row (mirrored in RTL) from the window's first slot's centre.
fn along(bounds: Bounds<Pixels>, pages: usize, rtl: bool, x: f64) -> f64 {
    let (left, width) = (f64::from(bounds.origin.x), f64::from(bounds.size.width));
    let row_start = (width - page_control_layout(pages as f64).row_width) / 2.;
    let first_center = row_start + INDICATOR_SIZE / 2.;
    if rtl {
        left + width - first_center - x
    } else {
        x - left - first_center
    }
}

/// Px past the edge of the content: positive past the end, negative past the start.
fn past(bounds: Bounds<Pixels>, rtl: bool, x: f64) -> f64 {
    let (left, width) = (f64::from(bounds.origin.x), f64::from(bounds.size.width));
    let (start, end) = (left + HORIZONTAL_PADDING, left + width - HORIZONTAL_PADDING);
    let beyond = if x > end {
        x - end
    } else if x < start {
        x - start
    } else {
        0.
    };
    if rtl { -beyond } else { beyond }
}

/// Where the strip starts, px from the control's inline start. The cropped row starts
/// inside the padding.
fn strip_start(layout: &PageControlLayout, pages: usize, touch: bool) -> f64 {
    let full_row = pages as f64 * PITCH - INDICATOR_SPACING;
    if touch {
        (layout.width - layout.row_width) / 2.
    } else {
        (layout.width - full_row.min(layout.width - 2. * HORIZONTAL_PADDING)) / 2.
    }
}

/// Page `page`'s dot centre in px from the control's left edge, the strip shifted
/// `offset` px toward its start.
fn dot_center(
    layout: &PageControlLayout,
    pages: usize,
    touch: bool,
    offset: f64,
    page: usize,
    rtl: bool,
) -> f64 {
    let x = strip_start(layout, pages, touch) - offset + page as f64 * PITCH + INDICATOR_SIZE / 2.;
    if rtl { layout.width - x } else { x }
}

fn stop_jog(state: &Entity<PcState>, cx: &mut App) {
    state.update(cx, |s, _| {
        s.jog_timer = None;
        s.jog_from = None;
        s.jog_due = None;
    });
}

/// Past the content's edge of a windowed control, the page steps on its own (upstream
/// `jog`). Each step comes `1 / rate` after the previous one was due, or after the
/// pointer crossed the edge. The rate follows how far past the pointer is now, so
/// moving further brings the next step closer. Steps count from when one was due, so a
/// late timer does not slow the run. After a stall longer than a step, the next one
/// counts from the late step, with no burst.
fn jog(state: &Entity<PcState>, window: &mut Window, cx: &mut App) {
    let (gesture, pages, rtl) = {
        let s = state.read(cx);
        (s.gesture.clone(), s.latest.pages, s.latest.rtl)
    };
    let Some(geometry) = gesture.geometry else {
        return stop_jog(state, cx);
    };
    if !gesture.scrubbing || !page_control_layout(pages as f64).is_windowed {
        return stop_jog(state, cx);
    }
    let beyond = past(geometry, rtl, gesture.last_x);
    let direction = if beyond > 0. {
        1.
    } else if beyond < 0. {
        -1.
    } else {
        0.
    };
    let interval = if direction == 0. {
        f64::INFINITY
    } else {
        jog_interval(beyond.abs(), direction)
    };
    if !interval.is_finite() {
        return stop_jog(state, cx);
    }
    let now = cx.background_executor().now();
    let from = state.update(cx, |s, _| *s.jog_from.get_or_insert(now));
    let due = from + Duration::from_secs_f64(interval);
    {
        let s = state.read(cx);
        if s.jog_timer.is_some()
            && s.jog_due.is_some_and(|d| {
                let gap = if d > due { d - due } else { due - d };
                gap < Duration::from_millis(1)
            })
        {
            return;
        }
    }
    let timer = cx
        .background_executor()
        .timer(due.saturating_duration_since(now));
    let owner = state.downgrade();
    let handle = window.window_handle();
    let task = cx.spawn(async move |cx| {
        timer.await;
        let _ = cx.update_window(handle, |_, window, cx| {
            let Some(owner) = owner.upgrade() else { return };
            let ran_at = cx.background_executor().now();
            let step = owner.update(cx, |s, _| {
                s.jog_timer = None;
                s.jog_due = None;
                s.jog_from = Some(
                    if ran_at.saturating_duration_since(due).as_secs_f64() > interval {
                        ran_at
                    } else {
                        due
                    },
                );
                let g = &s.gesture;
                let beyond = g.geometry.map_or(0., |b| past(b, s.latest.rtl, g.last_x));
                if beyond == 0. {
                    return None;
                }
                let from = s.pending.map_or(s.latest.current, |p| p.page);
                Some(clamp_page(from as f64 + beyond.signum(), s.latest.pages as f64) as usize)
            });
            if let Some(next) = step {
                request(&owner, next, ChangeKind::Other, window, cx);
            }
            jog(&owner, window, cx);
        });
    });
    state.update(cx, |s, _| {
        s.jog_timer = Some(task);
        s.jog_due = Some(due);
    });
}

/// The page under a scrubbing pointer (upstream `scrubTo`).
fn scrub_to(state: &Entity<PcState>, x: f64, window: &mut Window, cx: &mut App) {
    let (gesture, latest) = {
        let s = state.read(cx);
        (s.gesture.clone(), s.latest.clone())
    };
    if !latest.touch {
        // With a mouse: the page whose dot is nearest the pointer, on the full row;
        // nothing jogs past its ends.
        let mut nearest: Option<(usize, f64)> = None;
        for centre in gesture.centres {
            if nearest.is_none_or(|n| (centre.1 - x).abs() < (n.1 - x).abs()) {
                nearest = Some(centre);
            }
        }
        if let Some((page, _)) = nearest {
            request(
                state,
                clamp_page(page as f64, latest.pages as f64) as usize,
                ChangeKind::Other,
                window,
                cx,
            );
        }
        return;
    }
    let Some(geometry) = gesture.geometry else {
        return;
    };
    if past(geometry, latest.rtl, x) == 0. || !page_control_layout(latest.pages as f64).is_windowed
    {
        stop_jog(state, cx);
        let target = scrub_target(
            along(geometry, latest.pages, latest.rtl, x),
            latest.pages,
            latest.window_start,
        );
        request(state, target as usize, ChangeKind::Other, window, cx);
    }
    jog(state, window, cx);
}

/// Shows the held look, the platter and the dimmed indicators: the touch presentation.
fn show_held(state: &Entity<PcState>, cx: &mut App) {
    state.update(cx, |s, cx| {
        if s.latest.touch && !s.interacting {
            s.interacting = true;
            s.held_release = false;
            cx.notify();
        }
    });
}

/// Ends the gesture (upstream `finish`): its timers stop and the held look hides.
fn finish(state: &Entity<PcState>, cx: &mut App) {
    stop_jog(state, cx);
    state.update(cx, |s, cx| {
        s.interact_timer = None;
        s.gesture.scrubbing = false;
        s.gesture.geometry = None;
        s.gesture.centres.clear();
        if s.interacting {
            s.held_release = true;
        }
        s.interacting = false;
        cx.notify();
    });
}

fn begin(state: &Entity<PcState>, start_x: f64, cx: &mut App) {
    state.update(cx, |s, _| {
        s.gesture.start_x = start_x;
        s.gesture.last_x = start_x;
        s.gesture.scrubbing = false;
    });
}

/// The pointer moved to `x` (upstream `track`): 16 px from where it landed, it scrubs.
fn track(state: &Entity<PcState>, x: f64, window: &mut Window, cx: &mut App) {
    let began = state.update(cx, |s, cx| {
        let pages = s.latest.pages;
        s.gesture.last_x = x;
        if s.gesture.scrubbing || (x - s.gesture.start_x).abs() < SCRUB_START_DISTANCE || pages <= 1
        {
            return false;
        }
        s.gesture.scrubbing = true;
        s.gesture.geometry = s.bounds;
        s.gesture.centres = match (s.latest.touch, s.bounds) {
            (false, Some(b)) => {
                let layout = page_control_layout(pages as f64);
                (0..pages)
                    .map(|p| {
                        (
                            p,
                            f64::from(b.origin.x)
                                + dot_center(
                                    &layout,
                                    pages,
                                    false,
                                    s.latest.offset,
                                    p,
                                    s.latest.rtl,
                                ),
                        )
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        cx.notify();
        true
    });
    if began {
        show_held(state, cx);
    }
    if state.read(cx).gesture.scrubbing {
        scrub_to(state, x, window, cx);
    }
}

/// A tap, or a pan or hold that lifts before it scrubs: one page toward the pointer's
/// side of the current indicator (upstream `step`).
fn step(state: &Entity<PcState>, x: f64, window: &mut Window, cx: &mut App) {
    let (latest, bounds) = {
        let s = state.read(cx);
        (s.latest.clone(), s.bounds)
    };
    let Some(bounds) = bounds else { return };
    if latest.pages < 2 {
        return;
    }
    let layout = page_control_layout(latest.pages as f64);
    let target = if latest.touch {
        let slot = latest.current as f64
            - if layout.is_windowed {
                latest.window_start as f64
            } else {
                0.
            };
        tap_target(
            along(bounds, latest.pages, latest.rtl, x),
            slot * PITCH,
            latest.current as f64,
            latest.pages as f64,
            false,
        )
    } else {
        // With a mouse: toward the pointer's side of the current dot, on the row as
        // drawn.
        let centre = f64::from(bounds.origin.x)
            + dot_center(
                &layout,
                latest.pages,
                false,
                latest.offset,
                latest.current,
                latest.rtl,
            );
        tap_target(
            x,
            centre,
            latest.current as f64,
            latest.pages as f64,
            latest.rtl,
        )
    };
    request(state, target as usize, ChangeKind::Tap, window, cx);
}

fn lift(state: &Entity<PcState>, x: f64, window: &mut Window, cx: &mut App) {
    if !state.read(cx).gesture.scrubbing {
        step(state, x, window, cx);
    }
    finish(state, cx);
}

/// Runs `f` with the window and the state, after the recognizer's listener returns.
fn with_state(
    state: &WeakEntity<PcState>,
    f: impl FnOnce(&Entity<PcState>, &mut Window, &mut App) + 'static,
) {
    let state = state.clone();
    after_gesture(move |window, cx| {
        if let Some(state) = state.upgrade() {
            f(&state, window, cx);
        }
    });
}

/// The control's recognizers (upstream `usePageControlTouch`'s three
/// `useGestureRecognizer` calls). A pointer on the control, a finger or a mouse, is
/// decided within 10 px, so a swipe that starts vertical stays the page's scroll:
/// - a horizontal pan, from 10 px, scrubs once the pointer is 16 px from where it
///   landed; a drag that starts vertical fails it;
/// - a pointer held still for 155 ms (UIPageControl's interaction delay) scrubs the
///   same way from there, under a finger on the platter;
/// - a tap steps one page.
///
/// Whichever begins first holds the press and fails the others. The scrub's 16 px
/// count from where the pointer landed, as UIKit counts them.
fn recognizers(state: &Entity<PcState>, window: &mut Window, cx: &mut App) -> Recognizers {
    let host = gesture_host(window, cx);
    let pan = PanGestureRecognizer::new(PanOptions {
        direction: PanDirection::Horizontal,
        ..PanOptions::named("page-control-scrub")
    });
    let hold = LongPressGestureRecognizer::new(LongPressOptions {
        minimum_press_duration: INTERACTION_DELAY_MS,
        ..LongPressOptions::named("page-control-hold")
    });
    let tap = TapGestureRecognizer::new(TapOptions::named("page-control-tap"));
    let attachment = host.attach(&[pan.any(), hold.any(), tap.any()]);
    let weak = state.downgrade();
    {
        let weak = weak.clone();
        pan.add_listener(move |pan| {
            let x = pan.location().x;
            match pan.state() {
                GestureState::Began => {
                    let landing = pan
                        .tracked_touches()
                        .first()
                        .map_or(x - pan.travel_at_began().x, |t| t.start_x());
                    with_state(&weak, move |state, window, cx| {
                        begin(state, landing, cx);
                        // The held look shows 155 ms after the pan began, moving or not.
                        let timer = cx
                            .background_executor()
                            .timer(Duration::from_secs_f64(INTERACTION_DELAY_MS / 1000.));
                        let owner = state.downgrade();
                        let task = cx.spawn(async move |cx| {
                            timer.await;
                            cx.update(|cx| {
                                if let Some(owner) = owner.upgrade() {
                                    owner.update(cx, |s, _| s.interact_timer = None);
                                    show_held(&owner, cx);
                                }
                            });
                        });
                        state.update(cx, |s, _| s.interact_timer = Some(task));
                        track(state, x, window, cx);
                    });
                }
                GestureState::Changed => {
                    with_state(&weak, move |state, window, cx| track(state, x, window, cx))
                }
                GestureState::Ended => {
                    with_state(&weak, move |state, window, cx| lift(state, x, window, cx))
                }
                GestureState::Cancelled => with_state(&weak, |state, _, cx| finish(state, cx)),
                _ => {}
            }
        });
    }
    {
        let weak = weak.clone();
        hold.add_listener(move |hold| {
            let x = hold.location().x;
            match hold.state() {
                GestureState::Began => {
                    let landing = hold.tracked_touches().first().map_or(x, |t| t.start_x());
                    with_state(&weak, move |state, _, cx| {
                        begin(state, landing, cx);
                        show_held(state, cx);
                    });
                }
                GestureState::Changed => {
                    with_state(&weak, move |state, window, cx| track(state, x, window, cx))
                }
                GestureState::Ended => {
                    with_state(&weak, move |state, window, cx| lift(state, x, window, cx))
                }
                GestureState::Cancelled => with_state(&weak, |state, _, cx| finish(state, cx)),
                _ => {}
            }
        });
    }
    tap.add_listener(move |tap| {
        if tap.state() == GestureState::Ended {
            let x = tap.location().x;
            with_state(&weak, move |state, window, cx| step(state, x, window, cx));
        }
    });
    Recognizers {
        attachment,
        _pan: pan,
        _hold: hold,
        _tap: tap,
    }
}

impl RenderOnce for PageControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pages = page_count_of(self.total_pages);
        if pages == 0 || (self.hides_for_single_page && pages == 1) {
            // Nothing to show. The row mounts with its element when pages arrive, so
            // its recognizers attach then.
            return gpui::Empty.into_any_element();
        }
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        // The held platter, shrinking window and fades are the touch presentation,
        // under a finger on any touch device. With a mouse, the control shows the
        // full row, cropped, and takes the same taps and scrubs over it.
        let touch = coarse_primary_pointer(window, cx);
        let reduced = prefers_reduced_motion(cx);
        let now = cx.background_executor().now();
        // Pages count from 1 outside and from 0 inside.
        let current = current_of(self.page, pages);
        let layout = page_control_layout(pages as f64);
        let focus = control_focus(&self.id, window, cx);

        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-page-control", self.id).into()),
            cx,
            move |_, _| PcState {
                // The window moves only when the current page would land in a shrunk
                // slot, so it depends on where it was.
                window_start: window_for(pages, 0, current),
                interacting: false,
                pending: None,
                sequence: 0,
                asked: None,
                gesture: Gesture::default(),
                shown_page: current,
                shown_window: None,
                dots: HashMap::new(),
                platter: None,
                held_release: false,
                slide: None,
                bounds: None,
                latest: Latest {
                    current,
                    pages,
                    window_start: 0,
                    touch,
                    rtl,
                    offset: 0.,
                    on_change: None,
                },
                interact_timer: None,
                jog_timer: None,
                jog_from: None,
                jog_due: None,
            },
        );
        let gestures = {
            let state = state.clone();
            window.use_keyed_state(
                ElementId::Name(format!("{:?}-page-control-gestures", self.id).into()),
                cx,
                move |window, cx| recognizers(&state, window, cx),
            )
        };
        let attachment = gestures.read(cx).attachment.clone();
        attachment.set_rtl(rtl);
        attachment.set_focus(Some(focus.clone()));

        let rendered = state.update(cx, |s, _| {
            if touch {
                s.window_start = window_for(pages, s.window_start, current);
            }
            let scrubbing = s.gesture.scrubbing;
            // With a mouse: the cropped row, every page at full size, shifted at once
            // to keep the current indicator in view. A scrub maps the pointer onto the
            // dots as they lie, so the row holds still until the lift.
            let offset = if touch {
                0.
            } else if scrubbing && s.latest.pages == pages && !s.latest.touch {
                s.latest.offset
            } else {
                cropped_row_offset(pages as f64, current as f64)
            };
            s.latest = Latest {
                current,
                pages,
                window_start: s.window_start,
                touch,
                rtl,
                offset,
                on_change: self.on_change.clone(),
            };

            // What asked for the page the control shows: a tap fades the old indicator
            // faster than anything else. A later change from the app fades as other.
            let old = std::mem::replace(&mut s.shown_page, current);
            let changed = old != current;
            let kind = match s.asked {
                Some((page, kind)) if changed && page == current => kind,
                _ => ChangeKind::Other,
            };
            if changed || s.pending.is_none() {
                s.asked = None;
            }
            if changed {
                s.pending = None;
            }

            // The window's slide: the strip's offset and every indicator's size.
            let (first, last) = if touch && layout.is_windowed {
                rendered_pages(pages, s.window_start)
            } else {
                (0, pages)
            };
            if !touch {
                s.shown_window = None;
                s.slide = None;
            } else {
                let before = s.shown_window.replace((s.window_start, pages));
                match before {
                    Some((was, was_pages)) if was_pages == pages && was != s.window_start => {
                        // The move starts windowSlideDelay after the change, the strip
                        // and every dot from one start time.
                        let running = s
                            .slide
                            .as_ref()
                            .map(|sl| (sl.clone(), sl.frame(now).progress));
                        let mut from_sizes = HashMap::new();
                        let (was_first, was_last) = rendered_pages(pages, was);
                        for page in was_first.min(first)..was_last.max(last) {
                            let size = match &running {
                                Some((sl, p)) => sl.size(pages, page, *p),
                                None => indicator_size(pages, was, page as isize),
                            };
                            if (was_first..was_last).contains(&page) || running.is_some() {
                                from_sizes.insert(page, size);
                            }
                        }
                        let from_offset = running
                            .as_ref()
                            .map_or(was as f64 * PITCH, |(sl, p)| sl.offset(*p));
                        s.slide = Some(Slide {
                            from_offset,
                            to_offset: s.window_start as f64 * PITCH,
                            from_sizes,
                            from_window: was,
                            to_window: s.window_start,
                            at: now + Duration::from_secs_f64(WINDOW_SLIDE_DELAY_MS / 1000.),
                            fade: reduced_motion_fade(reduced, true, false),
                        });
                    }
                    Some((was, was_pages)) if was_pages == pages && was == s.window_start => {}
                    _ => s.slide = None,
                }
            }
            let frame = s.slide.as_ref().map(|sl| sl.frame(now));
            if frame.is_some_and(|f| f.done) {
                s.slide = None;
            }

            // The indicators' opacity. The old current indicator fades, faster after a
            // tap; every other opacity change is a switch. Under Reduce Motion the
            // highlight is a selection colour, so it changes at once, and the held
            // look shows at once and takes the policy's fade on the release. With a
            // mouse, opacity changes are instant.
            let other_alpha = if s.interacting {
                INTERACTING_OTHER_INDICATOR_ALPHA
            } else {
                INACTIVE_ALPHA
            };
            let policy = if touch {
                reduced_motion_fade(reduced, true, false)
            } else {
                None
            };
            let release = std::mem::take(&mut s.held_release);
            for page in first..last {
                let target = if page == current {
                    CURRENT_INDICATOR_ALPHA
                } else {
                    other_alpha
                };
                let fade = s
                    .dots
                    .entry(page)
                    .or_insert_with(|| Fade::at_rest(target, now));
                let (ms, eased) = if !touch {
                    (0., [0., 0., 1., 1.])
                } else if let Some(policy) = policy {
                    if release && !changed {
                        (policy.duration_ms, policy.curve)
                    } else {
                        (0., [0., 0., 1., 1.])
                    }
                } else if changed && page == old {
                    (
                        if kind == ChangeKind::Tap {
                            TAP_FADE_OUT_MS
                        } else {
                            FADE_OUT_MS
                        },
                        FADE_OUT_CURVE,
                    )
                } else {
                    (SWITCH_MS, SWITCH_CURVE)
                };
                fade.retarget(target, ms, eased, now);
            }
            s.dots.retain(|page, _| (first..last).contains(page));
            let platter_target = if touch && s.interacting { 1. } else { 0. };
            let platter = s
                .platter
                .get_or_insert_with(|| Fade::at_rest(platter_target, now));
            let (ms, eased) = match policy {
                Some(policy) if release => (policy.duration_ms, policy.curve),
                Some(_) => (0., [0., 0., 1., 1.]),
                None if touch => (SWITCH_MS, SWITCH_CURVE),
                None => (0., [0., 0., 1., 1.]),
            };
            platter.retarget(platter_target, ms, eased, now);

            // The strip as drawn now.
            let progress = frame.map_or(1., |f| f.progress);
            let strip_offset = match &s.slide {
                Some(sl) => sl.offset(progress),
                None if touch => s.window_start as f64 * PITCH,
                None => offset,
            };
            let mut moving = s.slide.is_some();
            let dots: Vec<(usize, f64, f64, f64)> = (first..last)
                .map(|page| {
                    let size = match (&s.slide, touch) {
                        (Some(sl), _) => sl.size(pages, page, progress),
                        (None, true) => indicator_size(pages, s.window_start, page as isize),
                        // A 10 px box scaled to the 9.67 px indicator.
                        (None, false) => INDICATOR_SIZE,
                    };
                    let (alpha, fading) = s.dots.get(&page).map_or((1., false), |f| f.value(now));
                    moving |= fading;
                    (
                        page,
                        dot_center(&layout, pages, touch, strip_offset, page, rtl),
                        size,
                        alpha,
                    )
                })
                .collect();
            let (platter_alpha, fading) = s.platter.map_or((0., false), |f| f.value(now));
            moving |= fading;
            #[cfg(test)]
            let drawn = Drawn {
                width: layout.width,
                touch,
                interacting: s.interacting,
                offset: strip_offset,
                dots: dots
                    .iter()
                    .map(|&(page, x, size, alpha)| DrawnDot {
                        page,
                        x,
                        size,
                        alpha,
                        fade: s.dots[&page],
                    })
                    .collect(),
                platter: platter_alpha,
                platter_fade: s.platter.expect("set above"),
                label: SharedString::default(),
                value_text: SharedString::default(),
                value_now: current + 1,
                value_max: pages,
            };
            #[cfg(not(test))]
            let drawn = ();
            (
                dots,
                platter_alpha,
                frame.map_or(1., |f| f.opacity),
                moving,
                drawn,
            )
        });
        #[cfg_attr(not(test), allow(unused_variables))]
        let (dots, platter_alpha, strip_opacity, moving, drawn) = rendered;
        if moving {
            window.request_animation_frame();
        }

        let color = t.color.text_primary;
        let pressed = t.color.overlay_pressed;
        let probe = state.downgrade();
        let row = canvas(
            move |bounds, _, cx| {
                let _ = probe.update(cx, |s, _| s.bounds = Some(bounds));
            },
            move |bounds: Bounds<Pixels>, _, window, _| {
                // The held look has no element of its own: the root's platter.
                if platter_alpha > 0. {
                    window.paint_quad(
                        gpui::fill(bounds, pressed.opacity(platter_alpha as f32))
                            .corner_radii(px(9999.)),
                    );
                }
                for &(_, x, size, alpha) in &dots {
                    let shown = alpha * strip_opacity;
                    if size.is_nan() || shown.is_nan() || size <= 0. || shown <= 0. {
                        continue;
                    }
                    let r = size / 2.;
                    let origin =
                        bounds.origin + point(px((x - r) as f32), px((HEIGHT / 2. - r) as f32));
                    let dot = Bounds::new(origin, gpui::size(px(size as f32), px(size as f32)));
                    window.paint_quad(
                        gpui::fill(dot, color.opacity((alpha * strip_opacity) as f32))
                            .corner_radii(px(r as f32)),
                    );
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        let i18n = i18n(cx);
        let label = self
            .label
            .unwrap_or_else(|| i18n.t("@astryx.pagination.pageIndicators"));
        let value_text = i18n.format(
            "@astryx.pagination.pageOfTotal",
            &[
                ("current", Arg::from(current + 1)),
                ("total", Arg::from(pages)),
            ],
        );
        #[cfg(test)]
        DRAWN.with(|d| {
            let mut drawn = drawn;
            drawn.label = label.clone();
            drawn.value_text = value_text.clone();
            d.borrow_mut().insert(self.id.to_string(), drawn);
        });
        let mut root = div()
            .relative()
            .flex_none()
            .w(px(layout.width as f32))
            .h(px(HEIGHT as f32))
            .cursor_default();
        if !touch {
            root = root.overflow_hidden();
        }
        theme.refine(root.style(), "page-control", &[], None);
        root.style().refine(&self.style);
        let selector = format!("{}-page-control", self.id);
        let keys = state.clone();
        let mut root = root
            .id(self.id.clone())
            .debug_selector(move || selector)
            .track_focus(&focus)
            .role(Role::Slider)
            .aria_label(label)
            .aria_orientation(gpui::Orientation::Horizontal)
            .aria_min_numeric_value(1.)
            .aria_max_numeric_value(pages as f64)
            .aria_numeric_value((current + 1) as f64)
            .aria_value(value_text)
            .on_mouse_down(MouseButton::Left, {
                let focus = focus.clone();
                move |_, window, cx| window.focus(&focus, cx)
            })
            .on_key_down(move |e: &KeyDownEvent, window, cx| {
                let (current, pages) = {
                    let l = &keys.read(cx).latest;
                    (l.current as f64, l.pages as f64)
                };
                let next = match e.keystroke.key.as_str() {
                    "right" => current + if rtl { -1. } else { 1. },
                    "left" => current + if rtl { 1. } else { -1. },
                    "up" => current + 1.,
                    "down" => current - 1.,
                    "home" => 0.,
                    "end" => pages - 1.,
                    _ => return,
                };
                cx.stop_propagation();
                request(
                    &keys,
                    clamp_page(next, pages) as usize,
                    ChangeKind::Other,
                    window,
                    cx,
                );
            })
            .child(row);
        // A finger needs a 44 px target: the touch hit area grows the control toward
        // it, as far as its neighbours allow, and a finger on the grown band reaches the
        // control's recognizers. Upstream pauses its measuring while a scrub runs, so
        // nothing reads layout as the finger moves; here the band is measured from the
        // last frame's boxes, which reads no layout, so it stays.
        root = root.children(hit_area_with(
            &self.id,
            true,
            None,
            None,
            HitLinks {
                gestures: Some(&attachment),
                press: None,
            },
            window,
            cx,
        ));
        if is_focus_visible(&focus, window) {
            root = root.child(astryx_components::focus::focus_ring(t, px(0.), rem));
        }
        gesture_area(&attachment, root).into_any_element()
    }
}

#[cfg(test)]
thread_local! {
    /// What each control drew last, by its id, for tests.
    pub(crate) static DRAWN: std::cell::RefCell<HashMap<String, Drawn>> = std::cell::RefCell::default();
}

/// What a control drew, for tests.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct Drawn {
    pub width: f64,
    pub touch: bool,
    pub interacting: bool,
    /// The strip's offset toward its start, px.
    pub offset: f64,
    pub dots: Vec<DrawnDot>,
    pub platter: f64,
    pub platter_fade: Fade,
    pub label: SharedString,
    pub value_text: SharedString,
    pub value_now: usize,
    pub value_max: usize,
}

/// One indicator as drawn, for tests.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct DrawnDot {
    pub page: usize,
    /// Its centre, px from the control's left edge.
    pub x: f64,
    pub size: f64,
    pub alpha: f64,
    pub fade: Fade,
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `PageControl`.
pub const PAGE_CONTROL_DOC: ComponentDoc = ComponentDoc {
    name: "PageControl",
    rust_type: "astryx::lab::page_control::PageControl",
    props: &[
        p("totalPages", RustApi::Argument("total_pages")),
        p("page", RustApi::Argument("page")),
        p("onChange", RustApi::Method("on_change")),
        p("label", RustApi::Method("label")),
        p(
            "hidesForSinglePage",
            RustApi::Method("hides_for_single_page"),
        ),
    ],
    example: include_str!("examples.rs"),
};
