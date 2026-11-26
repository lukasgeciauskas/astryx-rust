//! A page for the sheet's window tests: an opener, sheets built by a closure, and
//! key, mouse, touch and clock helpers. Each sheet's last frame is read through
//! `probe::frame`.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, Render, TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext,
    Window, div, point, prelude::*, px,
};

pub(crate) use crate::overlay::sheet::probe::{SheetFrame, frame};

pub(crate) type Build = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

pub(crate) struct Page(pub Build);

impl Render for Page {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(astryx_primitives::announcer())
            .child((self.0)(window, cx))
    }
}

pub(crate) fn page(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Page(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

pub(crate) fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

/// Frames of 16 ms for `ms`.
pub(crate) fn frames(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..ms.div_ceil(16) {
        cx.executor().advance_clock(Duration::from_millis(16));
        settle(cx);
    }
}

pub(crate) fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

pub(crate) fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

pub(crate) fn touch(cx: &mut VisualTestContext, id: u64, phase: TouchPhase, at: Point<Pixels>) {
    cx.simulate_event(TouchEvent {
        id: TouchId(id),
        phase,
        position: at,
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

/// A finger from `from` by `dy` in `steps` steps of `ms`, lifting 2 ms after the
/// last when `lift`.
pub(crate) fn finger(
    cx: &mut VisualTestContext,
    from: Point<Pixels>,
    dy: f32,
    steps: u32,
    ms: u64,
    lift: bool,
) {
    touch(cx, 1, TouchPhase::Started, from);
    for step in 1..=steps {
        wait(cx, ms);
        let y = from.y + px(dy * step as f32 / steps as f32);
        touch(cx, 1, TouchPhase::Moved, point(from.x, y));
    }
    if lift {
        wait(cx, 2);
        touch(cx, 1, TouchPhase::Ended, point(from.x, from.y + px(dy)));
    }
    settle(cx);
}

/// A mouse drag from `from` by `dy` in `steps` steps of `ms`, then a release.
pub(crate) fn mouse_drag(
    cx: &mut VisualTestContext,
    from: Point<Pixels>,
    dy: f32,
    steps: u32,
    ms: u64,
) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: from,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    for step in 1..=steps {
        wait(cx, ms);
        cx.simulate_event(MouseMoveEvent {
            position: point(from.x, from.y + px(dy * step as f32 / steps as f32)),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        });
        cx.run_until_parked();
    }
    wait(cx, 2);
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: point(from.x, from.y + px(dy)),
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    settle(cx);
}

/// The top centre of the sheet's grab bar, from its content's bounds.
pub(crate) fn handle_of(cx: &mut VisualTestContext, content: &'static str) -> Point<Pixels> {
    let bounds = cx
        .debug_bounds(content)
        .expect("the sheet's content is shown");
    point(bounds.center().x, bounds.top() - px(16.))
}

/// An open flag a test flips, with the sheet's `on_open_change` writing it back.
pub(crate) fn flag() -> Rc<Cell<bool>> {
    Rc::new(Cell::new(false))
}

pub(crate) fn writer(open: &Rc<Cell<bool>>) -> impl Fn(bool, &mut Window, &mut App) + 'static {
    let open = open.clone();
    move |value, window, _| {
        open.set(value);
        window.refresh();
    }
}

/// Opens a sheet and lets its entrance end.
pub(crate) fn open(cx: &mut VisualTestContext, flag: &Rc<Cell<bool>>) {
    flag.set(true);
    settle(cx);
    frames(cx, 1000);
}

pub(crate) fn close(cx: &mut VisualTestContext, flag: &Rc<Cell<bool>>) {
    flag.set(false);
    settle(cx);
}

pub(crate) fn sheet(name: &str) -> SheetFrame {
    frame(name).unwrap_or_else(|| panic!("the sheet {name} is presented"))
}
