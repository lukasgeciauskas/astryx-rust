//! Popover tests, numbered by the upstream spec's FR and AR items.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, Modifiers, ParentElement, Render,
    Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::*;
use crate::button::Button;

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_start()
            .pt(px(120.))
            .pl(px(120.))
            .child((self.0)())
            .child(Button::new("after", "After"))
    }
}

type Log = Rc<RefCell<Vec<&'static str>>>;

/// The trigger sits at x 120, y 120 in a 1024 × 768 window.
fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

/// Lets a few frames pass: focus moves after the surface is drawn.
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

fn open(cx: &mut VisualTestContext) -> bool {
    settle(cx);
    cx.debug_bounds("p-content").is_some()
}

fn popover(log: Log, light_dismiss: bool) -> impl Fn() -> AnyElement + 'static {
    move || {
        let (first, second) = (log.clone(), log.clone());
        Popover::new("p", "Filters")
            .light_dismiss(light_dismiss)
            .content(
                div()
                    .debug_selector(|| "p-content".into())
                    .flex()
                    .gap(px(8.))
                    .child(
                        Button::new("first", "First")
                            .on_click(move |_, _, _| first.borrow_mut().push("first")),
                    )
                    .child(
                        Button::new("second", "Second")
                            .on_click(move |_, _, _| second.borrow_mut().push("second")),
                    ),
            )
            .child(Button::new("trigger", "Filters"))
            .into_any_element()
    }
}

#[gpui::test]
fn fr1_the_trigger_toggles_the_popover(cx: &mut TestAppContext) {
    let cx = host(cx, popover(Log::default(), true));
    assert!(!open(cx));
    cx.simulate_click(point(px(130.), px(130.)), Modifiers::none());
    assert!(open(cx), "a press opens it");
    cx.simulate_click(point(px(130.), px(130.)), Modifiers::none());
    assert!(
        !open(cx),
        "a second press on the trigger closes it, not reopens it"
    );
}

#[gpui::test]
fn fr2_focus_enters_the_first_control_and_tab_stays_inside(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, popover(log.clone(), true));
    press(cx, "tab");
    press(cx, "enter");
    assert!(open(cx));
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["first"], "focus went to the first control");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["first", "second"]);
    // The hidden close button ends the region, then Tab wraps (AR3).
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(
        *log.borrow(),
        ["first", "second", "first"],
        "Tab wrapped inside"
    );
    assert!(open(cx));
}

#[gpui::test]
fn ar3_the_close_button_closes_it(cx: &mut TestAppContext) {
    let cx = host(cx, popover(Log::default(), true));
    press(cx, "tab");
    press(cx, "enter");
    assert!(open(cx));
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    assert!(!open(cx), "the close button was the third stop");
}

#[gpui::test]
fn ar4_escape_closes_and_returns_focus_to_the_trigger(cx: &mut TestAppContext) {
    let cx = host(cx, popover(Log::default(), true));
    press(cx, "tab");
    press(cx, "enter");
    assert!(open(cx));
    press(cx, "escape");
    assert!(!open(cx));
    press(cx, "enter");
    assert!(open(cx), "focus was back on the trigger");
}

#[gpui::test]
fn a_press_outside_closes_it_unless_light_dismiss_is_off(cx: &mut TestAppContext) {
    let cx = host(cx, popover(Log::default(), true));
    cx.simulate_click(point(px(130.), px(130.)), Modifiers::none());
    assert!(open(cx));
    cx.simulate_click(point(px(900.), px(700.)), Modifiers::none());
    assert!(!open(cx));
}

#[gpui::test]
fn without_light_dismiss_only_the_trigger_closes_it(cx: &mut TestAppContext) {
    let cx = host(cx, popover(Log::default(), false));
    cx.simulate_click(point(px(130.), px(130.)), Modifiers::none());
    assert!(open(cx));
    cx.simulate_click(point(px(900.), px(700.)), Modifiers::none());
    assert!(open(cx), "a press outside leaves it open");
    cx.simulate_click(point(px(130.), px(130.)), Modifiers::none());
    assert!(!open(cx));
}

#[gpui::test]
fn a_controlled_popover_only_asks(cx: &mut TestAppContext) {
    let asked: Rc<RefCell<Vec<bool>>> = Rc::default();
    let cx = host(cx, {
        let asked = asked.clone();
        move || {
            let asked = asked.clone();
            Popover::new("p", "Filters")
                .open(false)
                .on_open_change(move |open, _, _| asked.borrow_mut().push(open))
                .content(div().debug_selector(|| "p-content".into()).child("Hi"))
                .child(Button::new("trigger", "Filters"))
                .into_any_element()
        }
    });
    cx.simulate_click(point(px(130.), px(130.)), Modifiers::none());
    assert_eq!(*asked.borrow(), [true]);
    assert!(!open(cx), "the caller keeps it closed");
}

#[gpui::test]
fn a_popover_without_controls_focuses_its_surface(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        Popover::new("p", "About")
            .close_button(false)
            .content(
                div()
                    .debug_selector(|| "p-content".into())
                    .child("Read only"),
            )
            .child(Button::new("trigger", "About"))
            .into_any_element()
    });
    press(cx, "tab");
    press(cx, "enter");
    assert!(open(cx));
    // Focus sits on the surface: Enter there does not reach the trigger.
    press(cx, "enter");
    assert!(open(cx));
    press(cx, "escape");
    assert!(!open(cx));
}
