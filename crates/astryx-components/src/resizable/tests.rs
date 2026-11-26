//! Resizable tests, numbered after `Resizable.spec.md` (FR and AR items).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, IntoElement, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::*;
use crate::segmented::tests::press;

type Build = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Host(Build);

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.0)(window, cx)
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::storage::use_memory_storage(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.run_until_parked();
    cx
}

/// A sidebar whose width the region drives, and its handle at x = width.
fn split(
    cx: &mut TestAppContext,
    config: impl Fn() -> UseResizable + 'static,
    handle: impl Fn(ResizeHandle) -> ResizeHandle + 'static,
) -> (&mut VisualTestContext, Rc<Cell<f32>>) {
    let size = Rc::new(Cell::new(0.));
    let seen = size.clone();
    let cx = host(cx, move |window, cx| {
        let region = use_resizable("side", config(), window, cx);
        seen.set(region.size);
        div()
            .flex()
            .w(px(800.))
            .h(px(400.))
            .child(div().w(px(region.size)).flex_none())
            .child(handle(
                ResizeHandle::new("h").resizable(&region).divider(true),
            ))
            .child(div().flex_1())
            .into_any_element()
    });
    (cx, size)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn drag(cx: &mut VisualTestContext, from: f32, to: f32) {
    let y = px(200.);
    cx.simulate_event(MouseDownEvent {
        position: point(px(from), y),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    settle(cx);
    cx.simulate_mouse_move(
        point(px(to), y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    settle(cx);
    cx.simulate_event(MouseUpEvent {
        position: point(px(to), y),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    settle(cx);
}

#[gpui::test]
fn ar1_the_keys_step_and_jump_within_the_bounds(cx: &mut TestAppContext) {
    let (cx, size) = split(
        cx,
        || {
            UseResizable::new()
                .default_size(200.)
                .min_size(100.)
                .max_size(400.)
        },
        |h| h,
    );
    assert_eq!(size.get(), 200.);
    press(cx, "tab");
    press(cx, "right");
    assert_eq!(size.get(), 210.);
    press(cx, "shift-right");
    assert_eq!(size.get(), 260.);
    press(cx, "left");
    assert_eq!(size.get(), 250.);
    press(cx, "home");
    assert_eq!(size.get(), 100.);
    press(cx, "end");
    assert_eq!(size.get(), 400.);
    press(cx, "ctrl-left");
    assert_eq!(
        size.get(),
        400.,
        "a key with a modifier the map does not name passes on"
    );
}

#[gpui::test]
fn ar1_enter_collapses_and_restores_the_previous_size(cx: &mut TestAppContext) {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let log = changes.clone();
    let (cx, size) = split(
        cx,
        move || {
            let log = log.clone();
            UseResizable::new()
                .default_size(260.)
                .collapsible(true)
                .on_collapse_change(move |collapsed, _, _| log.borrow_mut().push(collapsed))
        },
        |h| h,
    );
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(size.get(), 0.);
    press(cx, "enter");
    assert_eq!(size.get(), 260., "restores the size, not the minimum");
    assert_eq!(*changes.borrow(), [true, false]);
}

#[gpui::test]
fn fr3_a_drag_follows_the_pointer_and_a_lift_under_the_threshold_collapses(
    cx: &mut TestAppContext,
) {
    let (cx, size) = split(
        cx,
        || {
            UseResizable::new()
                .default_size(200.)
                .min_size(50.)
                .collapsible(true)
        },
        |h| h,
    );
    drag(cx, 200., 260.);
    assert_eq!(size.get(), 260.);
    drag(cx, 260., 20.);
    assert_eq!(size.get(), 0., "lifted under 40 px it collapses");
    // A drag out of collapse reopens it.
    drag(cx, 0., 120.);
    assert_eq!(size.get(), 120.);
}

#[gpui::test]
fn fr3_a_double_click_collapses_a_collapsible_region(cx: &mut TestAppContext) {
    let (cx, size) = split(
        cx,
        || UseResizable::new().default_size(200.).collapsible(true),
        |h| h,
    );
    cx.simulate_event(MouseDownEvent {
        position: point(px(200.), px(200.)),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    settle(cx);
    assert_eq!(size.get(), 0.);
}

#[gpui::test]
fn fr3_a_disabled_handle_ignores_keys_and_pointer(cx: &mut TestAppContext) {
    let (cx, size) = split(
        cx,
        || UseResizable::new().default_size(200.),
        |h| h.disabled(true),
    );
    press(cx, "tab");
    press(cx, "right");
    drag(cx, 200., 260.);
    assert_eq!(size.get(), 200.);
}

#[gpui::test]
fn fr3_a_reversed_handle_grows_toward_the_start(cx: &mut TestAppContext) {
    let (cx, size) = split(
        cx,
        || UseResizable::new().default_size(200.),
        |h| h.reversed(true),
    );
    press(cx, "tab");
    press(cx, "left");
    assert_eq!(size.get(), 210.);
}

#[gpui::test]
fn fr3_snaps_hold_the_rest_and_the_arrows_step_between_them(cx: &mut TestAppContext) {
    let (cx, size) = split(
        cx,
        || {
            UseResizable::new()
                .default_size(200.)
                .snaps([200., 300., 400.])
        },
        |h| h,
    );
    drag(cx, 200., 270.);
    assert_eq!(size.get(), 300.);
    press(cx, "tab");
    press(cx, "right");
    assert_eq!(size.get(), 400.);
    press(cx, "shift-left");
    assert_eq!(size.get(), 200.);
}

#[gpui::test]
fn fr3_the_size_persists_once_a_person_resizes(cx: &mut TestAppContext) {
    let (cx, size) = split(
        cx,
        || UseResizable::new().default_size(200.).auto_save_id("side"),
        |h| h,
    );
    let stored = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| astryx_primitives::storage::read(cx, "astryx-resizable:side"))
    };
    assert_eq!(stored(cx), None, "nothing before a person resizes");
    drag(cx, 200., 280.);
    assert_eq!(size.get(), 280.);
    let entry = stored(cx).expect("written when the drag ends");
    assert!(entry.contains("280"), "{entry}");
}

#[gpui::test]
fn fr5_a_percentage_resolves_against_the_container(cx: &mut TestAppContext) {
    let size = Rc::new(Cell::new(0.));
    let seen = size.clone();
    let cx = host(cx, move |window, cx| {
        let basis = use_resizable_basis("box", window, cx);
        let region = use_resizable(
            "side",
            UseResizable::new()
                .default_size(ResizableSize::percent(25.))
                .max_size(ResizableSize::percent_at_most(50., 300.))
                .container(basis.clone()),
            window,
            cx,
        );
        seen.set(region.size);
        div()
            .relative()
            .w(px(800.))
            .h(px(100.))
            .child(basis.probe())
            .into_any_element()
    });
    settle(cx);
    assert_eq!(size.get(), 200., "25% of 800");
}

#[gpui::test]
fn fr6_conflicting_bounds_take_the_maximum(cx: &mut TestAppContext) {
    let (_, size) = split(
        cx,
        || {
            UseResizable::new()
                .default_size(200.)
                .min_size(300.)
                .max_size(250.)
        },
        |h| h,
    );
    assert_eq!(size.get(), 250.);
}

#[gpui::test]
fn controlled_collapse_reports_and_waits_for_the_caller(cx: &mut TestAppContext) {
    let requests = Rc::new(RefCell::new(Vec::new()));
    let log = requests.clone();
    let (cx, size) = split(
        cx,
        move || {
            let log = log.clone();
            UseResizable::new()
                .default_size(200.)
                .collapsible(true)
                .is_collapsed(false)
                .on_collapse_change(move |c, _, _| log.borrow_mut().push(c))
        },
        |h| h,
    );
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*requests.borrow(), [true]);
    assert_eq!(
        size.get(),
        200.,
        "nothing changes until the caller passes it"
    );
}

#[gpui::test]
fn ar1_right_to_left_inverts_the_drag_and_the_arrows(cx: &mut TestAppContext) {
    let size = Rc::new(Cell::new(0.));
    let seen = size.clone();
    let cx = host(cx, move |window, cx| {
        let region = use_resizable("side", UseResizable::new().default_size(200.), window, cx);
        seen.set(region.size);
        astryx_i18n::provider(
            astryx_i18n::I18n::new("ar"),
            div()
                .flex()
                .w(px(800.))
                .h(px(400.))
                .child(div().w(px(region.size)).flex_none())
                .child(ResizeHandle::new("h").resizable(&region).divider(true))
                .child(div().flex_1()),
        )
        .into_any_element()
    });
    drag(cx, 200., 260.);
    assert_eq!(size.get(), 140., "the divider follows the pointer in RTL");
    press(cx, "tab");
    press(cx, "right");
    assert_eq!(size.get(), 130., "ArrowRight shrinks a start panel in RTL");
}
