//! Carousel, upstream `Carousel/Carousel.tsx`, with `useSnapPaging` (`SnapPaging/`): a horizontal
//! row of items that scrolls, with edge fades and previous and next buttons that hide when
//! their direction runs out (FR1-FR4), optional wrap-around, and snapping. GPUI has no CSS
//! scroll snap: after a gesture ends (a trackpad's release, or a wheel at rest) the row
//! settles on the nearest stop (FR6); with snap paging a flick moves at most one page
//! (FR7).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Bounds, ElementId, FocusHandle, IntoElement, ParentElement, Pixels,
    RenderOnce, Role, ScrollDelta, ScrollEvent, ScrollHandle, SharedString, StyleRefinement,
    Styled, TouchPhase, Window, canvas, div, point, prelude::*, px,
};
use rustryx_i18n::current as i18n;
use rustryx_primitives::gestures::VelocityTracker;
use rustryx_primitives::prefers_reduced_motion;
use rustryx_theme::current;

use rustryx_theme::ThemeStyled;
use web_time::Instant;

use super::stops::{nearest_stop, rest_page, snap_stops, step_stop, view_pages};
use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconColor};
use crate::layout::SpacingStep;
use crate::scrollable_area::fade::{FadeScrollbar, scroll_edge_fades};

/// Upstream `useSnapPaging`'s `pages`: what one flick moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SnapPagingPages {
    /// One view, landing on the item start nearest one viewport on.
    #[default]
    View,
    /// One item.
    Item,
}

/// Upstream `SnapPaging`: the value `use_snap_paging` makes for `has_snap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapPaging {
    pages: SnapPagingPages,
}

/// Upstream `useSnapPaging`: paging for Carousel on its snap stops.
pub fn use_snap_paging(pages: SnapPagingPages) -> SnapPaging {
    SnapPaging { pages }
}

/// Upstream `hasSnap`: off, native snap, or snap paging (FR5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CarouselSnap {
    #[default]
    Off,
    Native,
    Paging(SnapPaging),
}

impl From<bool> for CarouselSnap {
    fn from(on: bool) -> Self {
        if on {
            CarouselSnap::Native
        } else {
            CarouselSnap::Off
        }
    }
}

impl From<SnapPaging> for CarouselSnap {
    fn from(paging: SnapPaging) -> Self {
        CarouselSnap::Paging(paging)
    }
}

type MoveFn = Rc<dyn Fn(f32, &mut Window, &mut App)>;
type IndexFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(Default)]
struct Wiring {
    step: Option<MoveFn>,
    to_index: Option<IndexFn>,
    can: (bool, bool),
}

/// Upstream `CarouselHandle` (`handleRef`): drives a Carousel from outside.
#[derive(Clone, Default)]
pub struct CarouselHandle(Rc<RefCell<Wiring>>);

impl CarouselHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scroll_next(&self, window: &mut Window, cx: &mut App) {
        let step = self.0.borrow().step.clone();
        if let Some(step) = step {
            step(1., window, cx);
        }
    }

    pub fn scroll_prev(&self, window: &mut Window, cx: &mut App) {
        let step = self.0.borrow().step.clone();
        if let Some(step) = step {
            step(-1., window, cx);
        }
    }

    pub fn scroll_to(&self, index: usize, window: &mut Window, cx: &mut App) {
        let to = self.0.borrow().to_index.clone();
        if let Some(to) = to {
            to(index, window, cx);
        }
    }

    pub fn can_scroll_next(&self) -> bool {
        self.0.borrow().can.1
    }

    pub fn can_scroll_prev(&self) -> bool {
        self.0.borrow().can.0
    }
}

struct Glide {
    from: f32,
    to: f32,
    start: Instant,
}

struct CarouselState {
    scroll: ScrollHandle,
    focus: FocusHandle,
    items: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    glide: Option<Glide>,
    wheel: u64,
    /// A trackpad or finger gesture: where it began, its velocity, and when.
    gesture: Option<(f32, VelocityTracker, Instant)>,
    /// The edge button a keyboard pressed, for the focus handoff (FR2, FR3).
    pressed: Option<bool>,
    can: Option<(bool, bool)>,
}

/// Upstream `Carousel`.
#[derive(IntoElement)]
pub struct Carousel {
    id: ElementId,
    children: Vec<AnyElement>,
    gap: SpacingStep,
    buttons: bool,
    edge_fade: bool,
    looping: bool,
    snap: CarouselSnap,
    padding: Option<SpacingStep>,
    label: Option<SharedString>,
    handle: Option<CarouselHandle>,
    style: StyleRefinement,
}

impl Carousel {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Carousel {
            id: id.into(),
            children: Vec::new(),
            gap: SpacingStep::S1,
            buttons: true,
            edge_fade: true,
            looping: false,
            snap: CarouselSnap::Off,
            padding: None,
            label: None,
            handle: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `gap`: 0 to 4. Default 1.
    pub fn gap(mut self, gap: SpacingStep) -> Self {
        self.gap = gap;
        self
    }

    /// Upstream `hasButtons`. Default true.
    pub fn buttons(mut self, buttons: bool) -> Self {
        self.buttons = buttons;
        self
    }

    /// Upstream `hasEdgeFade`. Default true.
    pub fn edge_fade(mut self, fade: bool) -> Self {
        self.edge_fade = fade;
        self
    }

    /// Upstream `hasLoop`: next at the end goes to the start, and back.
    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Upstream `hasSnap`: `true`, or `use_snap_paging(..)`.
    pub fn snap(mut self, snap: impl Into<CarouselSnap>) -> Self {
        self.snap = snap.into();
        self
    }

    /// Upstream `padding`: inline padding, and the snap stops' scroll padding.
    pub fn padding(mut self, padding: SpacingStep) -> Self {
        self.padding = Some(padding);
        self
    }

    /// Upstream `aria-label`: default "Carousel".
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `handleRef`.
    pub fn handle(mut self, handle: CarouselHandle) -> Self {
        self.handle = Some(handle);
        self
    }
}

impl ParentElement for Carousel {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Carousel {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The row's position (0 at the start), its end, and its stops.
fn geometry(state: &CarouselState, padding: f32) -> (f32, f32, Vec<f32>, f32, f32) {
    let port = state.scroll.bounds();
    let at = -f32::from(state.scroll.offset().x);
    let end = f32::from(state.scroll.max_offset().x);
    let items = state.items.borrow();
    let starts: Vec<f32> = items
        .iter()
        .map(|b| f32::from(b.left() - port.left()) + at)
        .collect();
    let item = items.first().map_or(0., |b| f32::from(b.size.width));
    (
        at,
        end,
        snap_stops(&starts, padding, end),
        f32::from(port.size.width),
        item,
    )
}

/// Glides the row to `to` (at once under reduced motion).
fn glide(state: &gpui::Entity<CarouselState>, to: f32, window: &mut Window, cx: &mut App) {
    let now = cx.background_executor().now();
    let (scroll, at) = {
        let s = state.read(cx);
        (s.scroll.clone(), -f32::from(s.scroll.offset().x))
    };
    if prefers_reduced_motion(cx) {
        scroll.set_offset(point(px(-to), scroll.offset().y));
        state.update(cx, |s, _| s.glide = None);
    } else {
        state.update(cx, |s, _| {
            s.glide = Some(Glide {
                from: at,
                to,
                start: now,
            })
        });
    }
    window.refresh();
}

/// Settles a released gesture on its stop (FR6) or page (FR7).
fn settle(
    state: &gpui::Entity<CarouselState>,
    snap: CarouselSnap,
    pad: f32,
    release: Option<(f32, f64)>,
    window: &mut Window,
    cx: &mut App,
) {
    let (at, _end, stops, width, _) = geometry(state.read(cx), pad);
    let velocity = release.map_or(0., |(_, v)| v as f32);
    let projected = at + velocity * 0.25;
    let to = match (snap, release) {
        (CarouselSnap::Paging(paging), Some((start, _))) => {
            let anchor = stops
                .iter()
                .position(|s| (s - nearest_stop(&stops, start)).abs() < 0.5)
                .unwrap_or(0);
            let pages = match paging.pages {
                SnapPagingPages::Item => stops.clone(),
                SnapPagingPages::View => view_pages(&stops, anchor, width),
            };
            rest_page(&pages, start, projected)
        }
        _ => nearest_stop(&stops, projected),
    };
    if (to - at).abs() > 0.5 {
        glide(state, to, window, cx);
    }
}

impl RenderOnce for Carousel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let i18n = i18n(cx);
        let dir = rustryx_i18n::use_direction(cx);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-carousel", self.id).into()),
            cx,
            |_, cx| CarouselState {
                scroll: ScrollHandle::new(),
                focus: cx.focus_handle(),
                items: Rc::default(),
                glide: None,
                wheel: 0,
                gesture: None,
                pressed: None,
                can: None,
            },
        );
        let padding = self.padding.map_or(px(0.), |p| p.token(&t).to_pixels(rem));
        let pad = f32::from(padding);

        // A glide in progress.
        let duration = Duration::from_millis(300);
        let gliding = state
            .read(cx)
            .glide
            .as_ref()
            .map(|g| (g.from, g.to, g.start));
        if let Some((from, to, start)) = gliding {
            let now = cx.background_executor().now();
            let p = (now.saturating_duration_since(start).as_secs_f32() / duration.as_secs_f32())
                .min(1.);
            let x = from + (to - from) * t.ease.standard.ease(p);
            let scroll = state.read(cx).scroll.clone();
            scroll.set_offset(point(px(-x), scroll.offset().y));
            if p < 1. {
                window.request_animation_frame();
            } else {
                state.update(cx, |s, _| s.glide = None);
            }
        }

        let (at, end, stops, width, item) = geometry(state.read(cx), pad);
        let overflow = end > 0.5;
        let (over_start, over_end) = (overflow && at > 0.5, overflow && at < end - 0.5);
        let looping = self.looping && overflow;
        let (can_start, can_end) = if looping {
            (true, true)
        } else {
            (over_start, over_end)
        };

        // FR2, FR3: a keyboard-pressed edge button that runs out hands focus to the row.
        let previous = state.read(cx).can;
        state.update(cx, |s, _| s.can = Some((can_start, can_end)));
        if let (Some(prev), Some(side)) = (previous, state.read(cx).pressed) {
            let (was, is) = if side {
                (prev.1, can_end)
            } else {
                (prev.0, can_start)
            };
            if was && !is {
                let focus = state.read(cx).focus.clone();
                state.update(cx, |s, _| s.pressed = None);
                if window.focused(cx).is_none() {
                    window.focus(&focus, cx);
                }
            }
        }

        // The edge step (FR7): about one view, onto a stop; or around the loop.
        let snap = self.snap;
        let step: MoveFn = {
            let state = state.clone();
            Rc::new(move |direction, window, cx| {
                let (at, end, stops, width, item) = geometry(state.read(cx), pad);
                let to = if looping && direction > 0. && at >= end - 0.5 {
                    0.
                } else if looping && direction < 0. && at <= 0.5 {
                    end
                } else {
                    match snap {
                        CarouselSnap::Paging(paging) => {
                            let pages = match paging.pages {
                                SnapPagingPages::Item => stops.clone(),
                                SnapPagingPages::View => {
                                    let anchor = stops
                                        .iter()
                                        .position(|s| (s - nearest_stop(&stops, at)).abs() < 0.5)
                                        .unwrap_or(0);
                                    view_pages(&stops, anchor, width)
                                }
                            };
                            step_stop(&pages, at, width, 0., direction)
                        }
                        CarouselSnap::Native => step_stop(&stops, at, width, item, direction),
                        CarouselSnap::Off => {
                            (at + direction * (width - item / 2.).max(item)).clamp(0., end)
                        }
                    }
                };
                glide(&state, to, window, cx);
            })
        };
        let to_index: IndexFn = {
            let state = state.clone();
            Rc::new(move |index, window, cx| {
                let (at, end, stops, _, _) = geometry(state.read(cx), pad);
                let start = {
                    let s = state.read(cx);
                    let port = s.scroll.bounds();
                    let items = s.items.borrow();
                    items
                        .get(index.min(items.len().saturating_sub(1)))
                        .map(|b| f32::from(b.left() - port.left()) + at - pad)
                };
                if let Some(start) = start {
                    let to = if snap == CarouselSnap::Off {
                        start.clamp(0., end)
                    } else {
                        nearest_stop(&stops, start)
                    };
                    glide(&state, to, window, cx);
                }
            })
        };
        if let Some(handle) = &self.handle {
            *handle.0.borrow_mut() = Wiring {
                step: Some(step.clone()),
                to_index: Some(to_index),
                can: (can_start, can_end),
            };
        }

        // Snapping after a gesture (FR6, FR7); Shift with the wheel scrolls sideways.
        let on_wheel = {
            let state = state.clone();
            move |event: &ScrollEvent, window: &mut Window, cx: &mut App| {
                let (dx, dy) = match event.delta {
                    ScrollDelta::Pixels(d) => (f32::from(d.x), f32::from(d.y)),
                    ScrollDelta::Lines(d) => (d.x * 20., d.y * 20.),
                };
                let scroll = state.read(cx).scroll.clone();
                if event.modifiers.shift && dx == 0. && dy != 0. {
                    let max = f32::from(scroll.max_offset().x);
                    let x = (f32::from(scroll.offset().x) + dy).clamp(-max, 0.);
                    scroll.set_offset(point(px(x), scroll.offset().y));
                    cx.stop_propagation();
                    window.refresh();
                }
                if snap == CarouselSnap::Off {
                    return;
                }
                let now = cx.background_executor().now();
                let at = -f32::from(scroll.offset().x);
                match event.touch_phase {
                    TouchPhase::Started => {
                        let mut tracker = VelocityTracker::default();
                        tracker.add(f64::from(at), 0., 0.);
                        state.update(cx, |s, _| {
                            s.glide = None;
                            s.gesture = Some((at, tracker, now));
                        });
                    }
                    TouchPhase::Moved if state.read(cx).gesture.is_some() => {
                        state.update(cx, |s, _| {
                            if let Some((_, tracker, start)) = s.gesture.as_mut() {
                                let ms =
                                    now.saturating_duration_since(*start).as_secs_f64() * 1000.;
                                tracker.add(f64::from(at), 0., ms);
                            }
                        });
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        let release = state.update(cx, |s, _| {
                            s.gesture.take().map(|(start_at, tracker, start)| {
                                let ms = now.saturating_duration_since(start).as_secs_f64() * 1000.;
                                (start_at, tracker.velocity(Some(ms)).x)
                            })
                        });
                        settle(&state, snap, pad, release, window, cx);
                    }
                    // A wheel has no phases: it settles once it rests (native snap).
                    TouchPhase::Moved => {
                        if state.read(cx).glide.is_some() {
                            return;
                        }
                        let generation = state.update(cx, |s, _| {
                            s.wheel += 1;
                            s.wheel
                        });
                        let state = state.clone();
                        let timer = cx.background_executor().timer(Duration::from_millis(150));
                        window
                            .spawn(cx, async move |cx| {
                                timer.await;
                                cx.update(|window, cx| {
                                    let s = state.read(cx);
                                    if s.wheel == generation
                                        && s.gesture.is_none()
                                        && s.glide.is_none()
                                    {
                                        settle(&state, CarouselSnap::Native, pad, None, window, cx);
                                    }
                                })
                                .ok();
                            })
                            .detach();
                    }
                }
            }
        };

        let label = self
            .label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.carousel.label"));
        let total = self.children.len();
        let items = state.read(cx).items.clone();
        items.borrow_mut().clear();
        let focus = state.read(cx).focus.clone().tab_stop(overflow);
        let scroll = state.read(cx).scroll.clone();
        let mut scroller = div()
            .id(ElementId::Name(format!("{}-scroller", self.id).into()))
            .debug_selector({
                let s = format!("{}-scroller", self.id);
                move || s
            })
            .track_focus(&focus)
            .track_scroll(&scroll)
            .flex()
            .items_center()
            .w_full()
            .overflow_x_scroll()
            .scrollbar_none()
            .overscroll_x_contain()
            .gap(self.gap.token(&t))
            .px(padding)
            .role(Role::Group)
            .aria_label(label.clone())
            .children(self.children.into_iter().enumerate().map(|(i, child)| {
                let items = items.clone();
                let selector = format!("{}-item-{i}", self.id);
                div()
                    .id(ElementId::Name(format!("{}-item-{i}", self.id).into()))
                    .debug_selector(move || selector)
                    .relative()
                    .flex()
                    .flex_none()
                    .role(Role::Group)
                    .aria_label(i18n.format(
                        "@astryx.carousel.slideLabel",
                        &[("current", (i + 1).into()), ("total", total.into())],
                    ))
                    .child(child)
                    .child(
                        canvas(
                            move |bounds, _, _| items.borrow_mut().push(bounds),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
            }));
        theme.refine(
            scroller.style(),
            "carousel-scroller",
            &[
                (
                    "snap",
                    if snap == CarouselSnap::Off {
                        ""
                    } else {
                        "snap"
                    },
                ),
                ("edgeFade", if self.edge_fade { "edge-fade" } else { "" }),
            ],
            None,
        );

        // The edge fades follow the overflow (both while looping).
        let fades = if self.edge_fade {
            let inline = rustryx_primitives::scroll_area::ScrollAxisState {
                is_scrollable: overflow,
                at_start: !(looping || over_start),
                at_end: !(looping || over_end),
            };
            scroll_edge_fades(
                &format!("{}-fade", self.id),
                inline,
                FadeScrollbar::Hidden,
                false,
                false,
                (false, false),
                // Upstream's own: clear at the edge, 30% at 2px, shown from spacing-1.
                &[
                    rustryx_effects::edge_stop(px(0.), 0.),
                    rustryx_effects::edge_stop(px(2.), 0.3),
                    rustryx_effects::edge_stop(t.spacing.s1.to_pixels(rem), 1.),
                ],
                dir,
                window.supports_offscreen_layers(),
                &t,
            )
        } else {
            Default::default()
        };

        // FR1, FR4: the edge buttons; one that ran out is hidden and takes no tab stop.
        let buttons = self.buttons.then(|| {
            let pill = |end_side: bool, can: bool| {
                let size = t.size.element_md;
                let gutter = px(4.);
                let el = div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(t.color.background_popover)
                    .shadow_token(&t.shadow.med)
                    .when(!end_side, |d| d.ml(gutter))
                    .when(end_side, |d| d.mr(gutter));
                if !can {
                    return el.opacity(0.).size(size).into_any_element();
                }
                let selector = format!("{}-{}", self.id, if end_side { "next" } else { "prev" });
                let el = el.debug_selector(move || selector);
                let (state, step) = (state.clone(), step.clone());
                let (label, icon) = if end_side {
                    (i18n.t("@astryx.carousel.scrollRight"), "chevronRight")
                } else {
                    (i18n.t("@astryx.carousel.scrollLeft"), "chevronLeft")
                };
                el.child(
                    IconButton::new(
                        ElementId::Name(
                            format!("{}-{}", self.id, if end_side { "next" } else { "prev" })
                                .into(),
                        ),
                        label,
                        Icon::new(icon).color(IconColor::Inherit),
                    )
                    .variant(ButtonVariant::Ghost)
                    .on_click(move |event, window, cx| {
                        if event.is_keyboard() {
                            state.update(cx, |s, _| s.pressed = Some(end_side));
                        }
                        step(if end_side { 1. } else { -1. }, window, cx);
                    }),
                )
                .into_any_element()
            };
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .right_0()
                .flex()
                .items_center()
                .justify_between()
                .child(pill(false, can_start))
                .child(pill(true, can_end))
        });

        // The wheel listener runs in the capture phase, before the row's own scrolling,
        // which stops an event it consumes.
        let capture = {
            let scroll = state.read(cx).scroll.clone();
            let on_wheel = Rc::new(on_wheel);
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let (scroll, on_wheel) = (scroll.clone(), on_wheel.clone());
                    window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                        if phase == gpui::DispatchPhase::Capture
                            && scroll.bounds().contains(&event.position)
                        {
                            on_wheel(event, window, cx);
                        }
                    });
                },
            )
            .absolute()
            .size_0()
        };
        let mut root = div()
            .id(self.id.clone())
            .debug_selector({
                let s = format!("{}", self.id);
                move || s
            })
            .relative()
            .flex()
            .items_center()
            .min_w_0()
            .max_w_full()
            .overflow_hidden()
            .role(Role::Region)
            .aria_label(label)
            .child(fades.mask(scroller))
            .child(capture)
            .children(fades.edges)
            .children(buttons);
        theme.refine(root.style(), "carousel", &[], None);
        root.style().refine(&self.style);
        let _ = (end, stops, width, item, at);
        root
    }
}
