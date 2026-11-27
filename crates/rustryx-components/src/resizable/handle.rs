//! ResizeHandle, upstream `Resizable/ResizeHandle.tsx` with `resizeGesture.ts`: a
//! focusable window splitter (APG) with an enlarged grab zone and a grip pill. A
//! pointer drag (inverted under right-to-left), the arrow keys with snap steps, Home,
//! End, Enter and a double-click collapse drive the region it is given.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, CursorStyle, ElementId, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Orientation, Pixels, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Window, canvas, div, prelude::*, px, relative,
};
use rustryx_i18n::{current as i18n, use_direction};
use rustryx_primitives::gestures::VelocityTracker;
use rustryx_primitives::{control_focus, is_focus_visible};
use rustryx_theme::current;

use super::hook::ResizableProps;
use super::model::{KEYBOARD_LARGE_STEP, KEYBOARD_STEP, ResizeDirection, adjacent_snap_point};

/// Upstream `position`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizeHandlePosition {
    /// In flow between its siblings.
    #[default]
    Inline,
    /// Over its parent's end edge (its bottom edge when vertical).
    Overlay,
}

/// Upstream `PillPlacement`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PillPlacement {
    /// The panel's side, flipping when it is collapsed.
    #[default]
    Auto,
    Start,
    End,
    Center,
}

/// Upstream `ResizeHandle`.
#[derive(IntoElement)]
pub struct ResizeHandle {
    id: ElementId,
    direction: ResizeDirection,
    position: ResizeHandlePosition,
    reversed: bool,
    disabled: bool,
    divider: bool,
    always_visible: bool,
    pill_placement: PillPlacement,
    label: Option<SharedString>,
    resizable: Option<ResizableProps>,
    grip: Option<AnyElement>,
    style: StyleRefinement,
}

impl ResizeHandle {
    pub fn new(id: impl Into<ElementId>) -> Self {
        ResizeHandle {
            id: id.into(),
            direction: ResizeDirection::Horizontal,
            position: ResizeHandlePosition::Inline,
            reversed: false,
            disabled: false,
            divider: false,
            always_visible: true,
            pill_placement: PillPlacement::Auto,
            label: None,
            resizable: None,
            grip: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `direction`: the axis it resizes along.
    pub fn direction(mut self, direction: ResizeDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Upstream `position`.
    pub fn position(mut self, position: ResizeHandlePosition) -> Self {
        self.position = position;
        self
    }

    /// Upstream `isReversed`: it controls a panel on the end side.
    pub fn reversed(mut self, reversed: bool) -> Self {
        self.reversed = reversed;
        self
    }

    /// Upstream `isDisabled`: ignores input, keeps its tab stop.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `hasDivider`: a 1 px line that is the handle.
    pub fn divider(mut self, divider: bool) -> Self {
        self.divider = divider;
        self
    }

    /// Upstream `isAlwaysVisible`: `false` shows the pill only on hover or focus.
    pub fn always_visible(mut self, visible: bool) -> Self {
        self.always_visible = visible;
        self
    }

    /// Upstream `pillPlacement`.
    pub fn pill_placement(mut self, placement: PillPlacement) -> Self {
        self.pill_placement = placement;
        self
    }

    /// Upstream `label`. Default "Resize handle".
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `resizable`: `use_resizable`'s region or its props.
    pub fn resizable(mut self, resizable: impl Into<ResizableProps>) -> Self {
        self.resizable = Some(resizable.into());
        self
    }

    /// Upstream `children`: replaces the grip pill.
    pub fn grip(mut self, grip: impl IntoElement) -> Self {
        self.grip = Some(grip.into_any_element());
        self
    }
}

impl Styled for ResizeHandle {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// A drag in flight.
struct Drag {
    start: Pixels,
    /// -1 under right-to-left on the inline axis.
    rtl: f32,
    tracker: VelocityTracker,
    started_at: std::time::Instant,
    props: ResizableProps,
}

#[derive(Default)]
struct HandleState {
    hovered: bool,
    drag: Option<Drag>,
}

/// One gesture step: start, move, end (upstream `step`).
fn step(props: &ResizableProps, delta: f32, window: &mut Window, cx: &mut App) {
    props.start(window, cx);
    props.move_by(delta, window, cx);
    props.end(0., window, cx);
}

/// Upstream `toggleCollapse`: the region's own collapse and expand, or a resize
/// gesture to the minimum without them.
fn toggle(props: &ResizableProps, window: &mut Window, cx: &mut App) {
    if props.has_collapse_controls() {
        props.toggle(window, cx);
    } else if props.is_collapsed {
        step(props, props.min_size_px, window, cx);
    } else {
        step(props, -props.size, window, cx);
    }
}

impl RenderOnce for ResizeHandle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        let horizontal = self.direction == ResizeDirection::Horizontal;
        let overlay = self.position == ResizeHandlePosition::Overlay;
        let focus = control_focus(&self.id, window, cx);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-handle", self.id).into()),
            cx,
            |_, _| Rc::new(RefCell::new(HandleState::default())),
        );
        let state = state.read(cx).clone();
        let hovered = state.borrow().hovered;
        let dragging = state.borrow().drag.is_some();
        let focused = focus.is_focused(window);
        let interacting = hovered || focused;
        let props = self.resizable.clone();
        if let Some(p) = &props {
            p.register_handle(&focus, cx);
        }
        let inert = self.disabled || props.is_none();
        let collapsed = props.as_ref().is_some_and(|p| p.is_collapsed);
        let pane = props
            .as_ref()
            .map(|p| p.size)
            .filter(|s| !collapsed && *s > 0.);
        if let (Some(p), Some(region)) = (props.as_ref(), props.as_ref().and_then(|p| p.direction))
            && region != self.direction
        {
            let _ = p;
            rustryx_primitives::diagnostics::dev_warn_once("ResizeHandle:direction", || {
                format!(
                    "ResizeHandle: direction=\"{}\" but its useResizable region is \"{}\". They \
                     must match.",
                    self.direction.as_str(),
                    region.as_str()
                )
            });
        }
        let i18n = i18n(cx);
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.resizable.handle.label"));
        let cursor = if inert {
            CursorStyle::Arrow
        } else if horizontal {
            CursorStyle::ResizeLeftRight
        } else {
            CursorStyle::ResizeUpDown
        };

        // The handle.
        let mut handle = div()
            .id(self.id.clone())
            .debug_selector({
                let s = format!("{}", self.id);
                move || s
            })
            .track_focus(&focus)
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .role(Role::Splitter)
            .aria_label(label)
            .aria_orientation(if horizontal {
                Orientation::Vertical
            } else {
                Orientation::Horizontal
            })
            .aria_disabled(self.disabled);
        if let Some(p) = &props {
            let max = p.announced_max();
            let now = p.size.max(p.min_size_px).min(max.unwrap_or(f32::INFINITY));
            handle = handle
                .aria_numeric_value(now as f64)
                .aria_min_numeric_value(p.min_size_px as f64);
            if let Some(max) = max {
                handle = handle.aria_max_numeric_value(max as f64);
            }
            if p.is_collapsed {
                handle = handle.aria_value(i18n.t("@astryx.resizable.collapsed"));
            }
        }
        // A pull past a limit draws the handle past it on the rubber band, over the
        // content, and springs it back after the lift (Resizable FR8).
        let band = props.as_ref().map_or(0., |p| p.handle_offset(window, cx));
        if band != 0. {
            let sign = if self.reversed { -1. } else { 1. };
            let flip = if horizontal && rtl { -1. } else { 1. };
            let shift = px(band * sign * flip);
            handle = if horizontal {
                handle.left(shift)
            } else {
                handle.top(shift)
            };
        }
        let hit = px(16.).min(t.spacing.s4.to_pixels(rem));
        if overlay {
            handle = handle.absolute();
            handle = if horizontal {
                let width = pane.map_or(hit, |p| hit.min(px(p / 2.)));
                let h = handle.top_0().bottom_0().w(width);
                if rtl { h.left_0() } else { h.right_0() }
            } else {
                let height = pane.map_or(hit, |p| hit.min(px(p / 2.)));
                handle.bottom_0().left_0().right_0().h(height)
            };
        } else {
            let line = if self.divider { px(1.) } else { px(0.) };
            handle = if horizontal {
                handle.w(line).h_full().cursor(cursor)
            } else {
                handle.h(line).w_full().cursor(cursor)
            };
            if self.divider {
                handle = handle.bg(if dragging {
                    t.color.border_emphasized
                } else {
                    t.color.border
                });
            }
        }
        theme.refine(
            handle.style(),
            "resize-handle",
            &[("direction", self.direction.as_str())],
            None,
        );
        handle.style().refine(&self.style);

        // The grab zone and the pill take the pointer.
        let grab_handlers = |el: gpui::Stateful<gpui::Div>| -> gpui::Stateful<gpui::Div> {
            let (state_down, state_hover) = (state.clone(), state.clone());
            let props = props.clone();
            let (reversed, disabled) = (self.reversed, self.disabled);
            el.cursor(cursor)
                .on_hover(move |over, window, _| {
                    let mut s = state_hover.borrow_mut();
                    if *over || s.drag.is_none() {
                        s.hovered = *over;
                    }
                    drop(s);
                    window.refresh();
                })
                .on_mouse_down(
                    MouseButton::Left,
                    move |event: &MouseDownEvent, window, cx| {
                        let Some(props) = props.clone() else { return };
                        if disabled {
                            return;
                        }
                        window.prevent_default();
                        cx.stop_propagation();
                        if event.click_count == 2 {
                            if props.collapsible {
                                toggle(&props, window, cx);
                            }
                            return;
                        }
                        let _ = reversed;
                        let pos = if horizontal {
                            event.position.x
                        } else {
                            event.position.y
                        };
                        let mut tracker = VelocityTracker::default();
                        tracker.add(
                            f64::from(f32::from(event.position.x)),
                            f64::from(f32::from(event.position.y)),
                            0.,
                        );
                        props.start_pointer(window, cx);
                        state_down.borrow_mut().drag = Some(Drag {
                            start: pos,
                            rtl: if horizontal && rtl { -1. } else { 1. },
                            tracker,
                            started_at: std::time::Instant::now(),
                            props,
                        });
                        window.refresh();
                    },
                )
        };
        let grab = {
            let mut zone = div()
                .id(ElementId::Name(format!("{}-grab", self.id).into()))
                .debug_selector({
                    let s = format!("{}-grab", self.id);
                    move || s
                })
                .absolute();
            zone = if overlay {
                zone.inset_0()
            } else if horizontal {
                let width = pane.map_or(hit, |p| hit.min(px(p)));
                zone.top_0().bottom_0().w(width).left(-width / 2.)
            } else {
                let height = pane.map_or(hit, |p| hit.min(px(p)));
                zone.left_0().right_0().h(height).top(-height / 2.)
            };
            grab_handlers(zone)
        };
        handle = handle.child(grab);

        // The grip pill (or the caller's grip).
        let side = match self.pill_placement {
            PillPlacement::Auto => match (self.reversed, collapsed) {
                (false, false) | (true, true) => PillPlacement::Start,
                _ => PillPlacement::End,
            },
            placement => placement,
        };
        match self.grip {
            Some(grip) => handle = handle.child(grip),
            None => {
                let shown = self.always_visible || interacting || dragging;
                let gap = t.spacing.s1.to_pixels(rem);
                let (thick, long) = (px(3.), t.spacing.s8.to_pixels(rem));
                let mut pill = div()
                    .id(ElementId::Name(format!("{}-pill", self.id).into()))
                    .debug_selector({
                        let s = format!("{}-pill", self.id);
                        move || s
                    })
                    .absolute()
                    .rounded_full()
                    .bg(if dragging {
                        t.color.border_emphasized
                    } else {
                        t.color.border
                    })
                    .opacity(if shown { 1. } else { 0. });
                let offset = |size: Pixels| match side {
                    PillPlacement::Start => -(size + gap),
                    PillPlacement::End => size + gap,
                    _ => -size / 2.,
                };
                pill = if horizontal {
                    let start = match side {
                        PillPlacement::Start | PillPlacement::End => {
                            if side == PillPlacement::Start {
                                offset(thick)
                            } else {
                                gap
                            }
                        }
                        _ => -thick / 2.,
                    };
                    let start = if rtl && side != PillPlacement::Center {
                        -start - thick
                    } else {
                        start
                    };
                    pill.w(thick)
                        .h(long)
                        .top(relative(0.5))
                        .mt(-long / 2.)
                        .left(start)
                } else {
                    let start = match side {
                        PillPlacement::Start => offset(thick),
                        PillPlacement::End => gap,
                        _ => -thick / 2.,
                    };
                    pill.h(thick)
                        .w(long)
                        .left(relative(0.5))
                        .ml(-long / 2.)
                        .top(start)
                };
                theme.refine(pill.style(), "resize-handle-pill", &[], None);
                if shown {
                    pill = grab_handlers(pill);
                }
                handle = handle.child(pill);
            }
        }

        // Keys (upstream resizeGesture's keyDown).
        if let Some(props) = props.clone() {
            let (reversed, disabled) = (self.reversed, self.disabled);
            handle = handle.on_key_down(move |event: &KeyDownEvent, window, cx| {
                if disabled {
                    return;
                }
                let keys = &event.keystroke;
                let m = keys.modifiers;
                let arrow = matches!(keys.key.as_str(), "left" | "right" | "up" | "down");
                if m.alt || m.control || m.platform || (m.shift && !arrow) {
                    return;
                }
                let sign = if reversed { -1. } else { 1. };
                let key_step = if m.shift {
                    KEYBOARD_LARGE_STEP
                } else {
                    KEYBOARD_STEP
                };
                let flip = if horizontal && rtl { -1. } else { 1. };
                let along = match keys.key.as_str() {
                    "right" | "down" => 1.,
                    "left" | "up" => -1.,
                    _ => 0.,
                };
                match keys.key.as_str() {
                    "left" | "right" | "up" | "down" => {
                        cx.stop_propagation();
                        let grows = along * flip * sign > 0.;
                        if props.is_collapsed {
                            if grows {
                                step(
                                    &props,
                                    (props.min_size_px + key_step).max(props.collapsed_size),
                                    window,
                                    cx,
                                );
                            }
                        } else if !props.snaps.is_empty() {
                            let target = adjacent_snap_point(
                                props.size,
                                props.min_size_px,
                                props.max_size_px,
                                &props.snaps,
                                grows,
                                if m.shift { 2 } else { 1 },
                            );
                            step(&props, target - props.size, window, cx);
                        } else {
                            step(&props, along * key_step * flip * sign, window, cx);
                        }
                    }
                    "home" => {
                        cx.stop_propagation();
                        step(&props, props.min_size_px - props.size, window, cx);
                    }
                    "end" => {
                        cx.stop_propagation();
                        if props.max_size_px.is_finite() {
                            step(&props, props.max_size_px - props.size, window, cx);
                        }
                    }
                    "enter" => {
                        cx.stop_propagation();
                        if props.collapsible {
                            toggle(&props, window, cx);
                        }
                    }
                    _ => {}
                }
            });
        }

        // A drag follows the pointer anywhere in the window until release.
        if dragging {
            let (move_state, up_state) = (state.clone(), state.clone());
            let reversed = self.reversed;
            handle = handle.child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        window.set_window_cursor_style(cursor);
                        let move_state = move_state.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, _, window, cx| {
                            // A move without the button means the release was lost
                            // (upstream's lost pointer capture): the drag cancels.
                            if event.pressed_button != Some(MouseButton::Left) {
                                let drag = move_state.borrow_mut().drag.take();
                                if let Some(drag) = drag {
                                    drag.props.cancel(window, cx);
                                }
                                return;
                            }
                            let (props, delta) = {
                                let mut s = move_state.borrow_mut();
                                let Some(drag) = s.drag.as_mut() else { return };
                                let pos = if horizontal {
                                    event.position.x
                                } else {
                                    event.position.y
                                };
                                let ms = drag.started_at.elapsed().as_secs_f64() * 1000.;
                                drag.tracker.add(
                                    f64::from(f32::from(event.position.x)),
                                    f64::from(f32::from(event.position.y)),
                                    ms,
                                );
                                let sign = if reversed { -1. } else { 1. };
                                (
                                    drag.props.clone(),
                                    f32::from(pos - drag.start) * drag.rtl * sign,
                                )
                            };
                            props.move_by(delta, window, cx);
                        });
                        let up_state = up_state.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, _, window, cx| {
                            if event.button != MouseButton::Left {
                                return;
                            }
                            let Some(drag) = up_state.borrow_mut().drag.take() else {
                                return;
                            };
                            let ms = drag.started_at.elapsed().as_secs_f64() * 1000.;
                            let v = drag.tracker.velocity(Some(ms));
                            let sign = if reversed { -1. } else { 1. };
                            let velocity =
                                (if horizontal { v.x } else { v.y }) as f32 * drag.rtl * sign;
                            drag.props.end(velocity, window, cx);
                            up_state.borrow_mut().hovered = false;
                            window.refresh();
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }
        if is_focus_visible(&focus, window) {
            handle = handle.child(crate::focus::focus_ring(&t, px(0.), rem));
        }
        handle
    }
}
