//! NavIcon and NavHeadingMenu tests, numbered after their specs.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, InteractiveElement, IntoElement, Modifiers, MouseButton, MouseDownEvent,
    MouseUpEvent, ParentElement, Styled, TestAppContext, VisualTestContext, div, point, px,
};

use super::*;
use crate::segmented::tests::{host, press};

type Log = Rc<RefCell<Vec<&'static str>>>;

fn menu(log: Log) -> impl Fn() -> AnyElement + 'static {
    move || {
        let (a, b, c) = (log.clone(), log.clone(), log.clone());
        let dismissed = log.clone();
        astryx_primitives::provide(
            menu::NavHeadingDismiss(Rc::new(move |_, _| dismissed.borrow_mut().push("dismiss"))),
            div().w(px(240.)).child(
                NavHeadingMenu::new("m")
                    .child(
                        NavHeadingMenuItem::new("dash", "Dashboard")
                            .on_click(move |_, _| a.borrow_mut().push("dashboard")),
                    )
                    .child(
                        NavHeadingMenuItem::new("off", "Offline")
                            .disabled(true)
                            .on_click(move |_, _| b.borrow_mut().push("offline")),
                    )
                    .child(
                        NavHeadingMenuItem::new("set", "Settings")
                            .on_click(move |_, _| c.borrow_mut().push("settings")),
                    ),
            ),
        )
        .into_any_element()
    }
}

fn mouse(cx: &mut VisualTestContext, down: bool, selector: &'static str) {
    let at = cx.debug_bounds(selector).expect(selector).center();
    if down {
        cx.simulate_event(MouseDownEvent {
            position: at,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
    } else {
        cx.simulate_event(MouseUpEvent {
            position: at,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
        });
    }
    cx.run_until_parked();
}

fn ring(cx: &mut VisualTestContext, id: &'static str) -> bool {
    let selector: &'static str = Box::leak(format!("{id}-focus-ring").into_boxed_str());
    cx.debug_bounds(selector).is_some()
}

#[gpui::test]
fn nav_icon_fr1_a_circle_the_size_of_a_control(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .debug_selector(|| "box".into())
            .child(NavIcon::new("home"))
            .into_any_element()
    });
    let b = cx.debug_bounds("box").unwrap();
    assert!(b.size.height > px(20.), "{b:?}");
}

#[gpui::test]
fn fr5_fr6_the_keys_move_wrap_and_choose_and_a_disabled_row_stays_inert(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, menu(log.clone()));
    // A click chooses and puts focus on the row.
    mouse(cx, true, "dash");
    mouse(cx, false, "dash");
    assert_eq!(*log.borrow(), ["dashboard"]);
    press(cx, "down");
    assert!(ring(cx, "off"), "a disabled row is reached");
    press(cx, "enter");
    assert_eq!(log.borrow().len(), 1, "and does nothing");
    press(cx, "down");
    press(cx, "down");
    assert!(ring(cx, "dash"), "the arrows wrap");
    press(cx, "end");
    assert!(ring(cx, "set"));
    press(cx, "home");
    assert!(ring(cx, "dash"));
    press(cx, "s");
    assert!(ring(cx, "set"), "typing moves to the matching row");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["dashboard", "settings"]);
}

#[gpui::test]
fn fr8_a_press_slides_across_the_rows_and_a_lift_outside_dismisses(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, menu(log.clone()));
    mouse(cx, true, "dash");
    let to = cx.debug_bounds("set").unwrap().center();
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    mouse(cx, false, "set");
    assert_eq!(
        *log.borrow(),
        ["settings"],
        "the row under the lift acts, once"
    );
    mouse(cx, true, "dash");
    cx.simulate_event(MouseUpEvent {
        position: point(px(600.), px(500.)),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    assert_eq!(*log.borrow(), ["settings", "dismiss"]);
}

fn finger(
    cx: &mut VisualTestContext,
    id: u64,
    phase: gpui::TouchPhase,
    at: gpui::Point<gpui::Pixels>,
) {
    cx.simulate_event(gpui::TouchEvent {
        id: gpui::TouchId(id),
        phase,
        position: at,
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn fr8_a_finger_slides_across_the_rows_and_lifts_on_the_one_that_acts(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, menu(log.clone()));
    let (dash, set) = (
        cx.debug_bounds("dash").unwrap().center(),
        cx.debug_bounds("set").unwrap().center(),
    );
    finger(cx, 1, gpui::TouchPhase::Started, dash);
    finger(cx, 1, gpui::TouchPhase::Moved, set);
    finger(cx, 1, gpui::TouchPhase::Ended, set);
    assert_eq!(
        *log.borrow(),
        ["settings"],
        "the row under the lift acts, once"
    );
    // A finger released off the rows leaves the menu: nothing acts, nothing dismisses.
    finger(cx, 2, gpui::TouchPhase::Started, dash);
    finger(cx, 2, gpui::TouchPhase::Moved, point(px(600.), px(500.)));
    finger(cx, 2, gpui::TouchPhase::Ended, point(px(600.), px(500.)));
    assert_eq!(*log.borrow(), ["settings"]);
    // A second finger ends the press with nothing acting.
    finger(cx, 3, gpui::TouchPhase::Started, dash);
    finger(cx, 4, gpui::TouchPhase::Started, set);
    finger(cx, 3, gpui::TouchPhase::Ended, dash);
    finger(cx, 4, gpui::TouchPhase::Ended, set);
    assert_eq!(*log.borrow(), ["settings"]);
}

#[gpui::test]
fn fr8_a_press_on_a_disabled_row_acts_on_nothing(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, menu(log.clone()));
    let off = cx.debug_bounds("off").unwrap().center();
    finger(cx, 1, gpui::TouchPhase::Started, off);
    finger(cx, 1, gpui::TouchPhase::Ended, off);
    mouse(cx, true, "off");
    mouse(cx, false, "off");
    assert!(log.borrow().is_empty());
}
