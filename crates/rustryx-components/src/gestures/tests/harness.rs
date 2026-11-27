//! A window for the gesture hooks' tests: a view that renders a closure with the window,
//! and touch, mouse and clock helpers.

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Render, TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext, Window, div, point,
    prelude::*, px,
};

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
    cx.run_until_parked();
    cx
}

/// A box at (0, 0), 200 by 100.
pub(crate) fn target(id: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .absolute()
        .left_0()
        .top_0()
        .w(px(200.))
        .h(px(100.))
}

pub(crate) fn touch(cx: &mut VisualTestContext, id: u64, phase: TouchPhase, x: f32, y: f32) {
    cx.simulate_event(TouchEvent {
        id: TouchId(id),
        phase,
        position: point(px(x), px(y)),
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

pub(crate) fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

/// A finger dragging from (10, 10) by `dx`, `dy` in four 16 ms steps, then lifting.
pub(crate) fn drag(cx: &mut VisualTestContext, id: u64, dx: f32, dy: f32, lift: bool) {
    touch(cx, id, TouchPhase::Started, 10., 10.);
    for step in 1..=4 {
        wait(cx, 16);
        let s = step as f32 / 4.;
        touch(cx, id, TouchPhase::Moved, 10. + dx * s, 10. + dy * s);
    }
    if lift {
        wait(cx, 16);
        touch(cx, id, TouchPhase::Ended, 10. + dx, 10. + dy);
    }
}

pub(crate) fn mouse_down(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: point(px(x), px(y)),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
}

pub(crate) fn mouse_move(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_event(MouseMoveEvent {
        position: point(px(x), px(y)),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::default(),
    });
    cx.run_until_parked();
}

pub(crate) fn mouse_up(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: point(px(x), px(y)),
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
}

/// A mouse drag from (10, 10) by `dx`.
pub(crate) fn mouse_drag(cx: &mut VisualTestContext, dx: f32) {
    mouse_down(cx, 10., 10.);
    for step in 1..=4 {
        wait(cx, 16);
        mouse_move(cx, 10. + dx * step as f32 / 4., 10.);
    }
    wait(cx, 16);
    mouse_up(cx, 10. + dx, 10.);
}
