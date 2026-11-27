//! A window for the snap-point drag's tests, its touch and mouse drags and its
//! clock, and a bare snap drag on a manual clock fed pan reports by hand (upstream
//! mocks `createPan` the same way).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Render, TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext, Window, div, point,
    prelude::*, px,
};
use rustryx_primitives::gestures::Point;

use crate::drag_to_snap_points::snap_drag::{
    SnapDrag, SnapDragOptions, SnapElement, WARNINGS, create_snap_drag,
};
use crate::gestures::pan::{PanAxis, PanEvent, PanInput, PanState};

pub(crate) type Build = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

pub(crate) struct Host(pub Build);

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child((self.0)(window, cx))
    }
}

pub(crate) fn host(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    settle(cx);
    cx
}

/// A box at (0, 0), 200 by 200.
pub(crate) fn target(id: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .absolute()
        .left_0()
        .top_0()
        .w(px(200.))
        .h(px(200.))
}

pub(crate) fn settle(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

/// Frames of 16 ms for `ms`.
pub(crate) fn frames(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..ms.div_ceil(16) {
        cx.executor().advance_clock(Duration::from_millis(16));
        settle(cx);
    }
}

/// Moves the clock without a frame.
pub(crate) fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

pub(crate) fn touch(cx: &mut VisualTestContext, phase: TouchPhase, x: f32, y: f32) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: point(px(x), px(y)),
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

/// A finger dragging along `axis` by `d` in `steps` steps of `ms`, then lifting
/// 4 ms after the last.
pub(crate) fn drag(cx: &mut VisualTestContext, axis: PanAxis, d: f32, steps: u32, ms: u64) {
    let at = |s: f32| match axis {
        PanAxis::X => (10. + s, 10.),
        PanAxis::Y => (10., 10. + s),
    };
    touch(cx, TouchPhase::Started, 10., 10.);
    for step in 1..=steps {
        wait(cx, ms);
        let (x, y) = at(d * step as f32 / steps as f32);
        touch(cx, TouchPhase::Moved, x, y);
    }
    wait(cx, 4);
    let (x, y) = at(d);
    touch(cx, TouchPhase::Ended, x, y);
}

/// A mouse drag down by `dy` in `steps` steps of `ms`, then a release.
pub(crate) fn mouse_drag_y(cx: &mut VisualTestContext, dy: f32, steps: u32, ms: u64) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: point(px(10.), px(10.)),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    for step in 1..=steps {
        wait(cx, ms);
        cx.simulate_event(MouseMoveEvent {
            position: point(px(10.), px(10. + dy * step as f32 / steps as f32)),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        });
        cx.run_until_parked();
    }
    wait(cx, 4);
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: point(px(10.), px(10. + dy)),
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
}

/// The development warnings raised since the last call.
pub(crate) fn take_warnings() -> Vec<String> {
    WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

// --- A bare snap drag ---------------------------------------------------------

pub(crate) struct Rig {
    pub snap: SnapDrag,
    pub element: SnapElement,
    pub clock: Rc<Cell<f64>>,
    pub settled: Rc<RefCell<Vec<usize>>>,
}

/// A snap drag on a manual clock, with `configure` to set the options only Core
/// passes.
pub(crate) fn rig(
    axis: PanAxis,
    points: &[f64],
    configure: impl FnOnce(&mut SnapDragOptions),
) -> Rig {
    let clock = Rc::new(Cell::new(1000.));
    let settled: Rc<RefCell<Vec<usize>>> = Rc::default();
    let (c, s) = (clock.clone(), settled.clone());
    let mut options = SnapDragOptions {
        axis,
        snap_points: points.to_vec(),
        flick_threshold: None,
        on_settle: Rc::new(move |index| s.borrow_mut().push(index)),
        pan: None,
        custom_property: None,
        compositor_parts: None,
        model: None,
        clock: Rc::new(move || c.get()),
    };
    configure(&mut options);
    let element = SnapElement::new();
    let snap = create_snap_drag(&element, options);
    Rig {
        snap,
        element,
        clock,
        settled,
    }
}

impl Rig {
    pub fn now(&self) -> f64 {
        self.clock.get()
    }

    /// Animation frames every 16 ms for `ms`.
    pub fn advance(&self, ms: f64) {
        let mut left = ms;
        while left > 0. {
            let step = left.min(16.);
            self.clock.set(self.clock.get() + step);
            left -= step;
            if step >= 16. || left <= 0. {
                self.snap.frame(self.clock.get());
            }
        }
    }

    /// A pan report along x: `total` adds the claim's 8 px.
    pub fn pan(&self, state: PanState, x: f64, velocity: f64, input: PanInput) {
        self.snap.on_pan(&PanEvent {
            state,
            translation: Point::new(x, 0.),
            total_translation: Point::new(x + x.signum() * 8., 0.),
            velocity: Point::new(velocity, 0.),
            input,
        });
    }

    pub fn pan_y(&self, state: PanState, y: f64, velocity: f64, input: PanInput) {
        self.snap.on_pan(&PanEvent {
            state,
            translation: Point::new(0., y),
            total_translation: Point::new(0., y),
            velocity: Point::new(0., velocity),
            input,
        });
    }

    pub fn x(&self) -> f64 {
        self.snap
            .animator
            .get(rustryx_primitives::motion::MotionProperty::X, None)
    }
}
