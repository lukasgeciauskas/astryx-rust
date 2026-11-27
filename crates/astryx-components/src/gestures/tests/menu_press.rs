//! Upstream `hooks/useMenuPress.test.tsx`, in a window, wherever the behavior exists here:
//! the press a trigger click comes from; tracking inside the menu (the release's row, the
//! highlight as focus or a custom callback, disabled rows, the release's modifiers); the
//! row a held finger lights; one gesture, one activation (the stray click and its
//! disarming); a finger's lift taps nothing; release outside and cancellation; the
//! trigger under a mouse and a finger (the UIKit model, the pointer model, a scroll view
//! and its drift allowance); the opening finger's handoff; edge autoscroll; caller content
//! beside the rows. (React-only cases, the shadow root and the iframe clocks have no
//! counterpart here.)

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AnyElement, App, FocusHandle, IntoElement, Modifiers, MouseButton, MouseDownEvent,
    MouseUpEvent, ScrollHandle, SharedString, TestAppContext, TouchPhase, VisualTestContext,
    Window, div, point, prelude::*, px,
};

use super::harness::*;
use crate::gestures::menu_press::*;
use crate::gestures::scroller::{scroll_begin, scroll_end};

#[derive(Default)]
struct Setup {
    delay: Option<f64>,
    custom_highlight: bool,
    trigger_in_scroller: bool,
    many_rows: bool,
    items_only: bool,
}

#[derive(Default)]
struct World {
    open: Cell<bool>,
    log: RefCell<Vec<String>>,
    press: RefCell<Option<MenuPress>>,
    focus: RefCell<Vec<(SharedString, FocusHandle)>>,
    scroll: RefCell<Option<ScrollHandle>>,
}

impl World {
    fn log(&self, s: impl Into<String>) {
        self.log.borrow_mut().push(s.into());
    }
    fn logged(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
    fn press(&self) -> MenuPress {
        self.press.borrow().clone().unwrap()
    }
}

const ROWS: [&str; 3] = ["a", "b", "c"];

fn mount(cx: &mut TestAppContext, setup: Setup) -> (&mut VisualTestContext, Rc<World>) {
    let world = Rc::new(World::default());
    let w = world.clone();
    let setup = Rc::new(setup);
    let cx = host(cx, move |window, cx| render(&w, &setup, window, cx));
    (cx, world)
}

fn render(w: &Rc<World>, setup: &Setup, window: &mut Window, cx: &mut App) -> AnyElement {
    let (open, dismiss, highlight) = (w.clone(), w.clone(), w.clone());
    let mut options = MenuPressOptions::default()
        .on_trigger_press(move |pointer, window, _| {
            let was = open.open.get();
            open.open.set(!was);
            open.log(format!("trigger press {pointer:?}"));
            window.refresh();
            !was
        })
        .on_dismiss(move |window, _| {
            dismiss.open.set(false);
            dismiss.log("dismiss");
            window.refresh();
        })
        .press_on_items_only(setup.items_only);
    if let Some(ms) = setup.delay {
        options = options.long_press_delay_ms(ms);
    }
    if setup.custom_highlight {
        options = options.on_highlight(move |row, _, _| {
            highlight.log(format!("highlight {}", row.as_deref().unwrap_or("none")));
        });
    }
    let scroll = w
        .scroll
        .borrow_mut()
        .get_or_insert_with(ScrollHandle::new)
        .clone();
    if setup.many_rows {
        options = options.scroller(scroll.clone());
    }
    let menu_focus = w
        .focus
        .borrow_mut()
        .iter()
        .find(|(k, _)| k == "menu")
        .map(|(_, h)| h.clone());
    let menu_focus = menu_focus.unwrap_or_else(|| {
        let h = cx.focus_handle();
        w.focus.borrow_mut().push(("menu".into(), h.clone()));
        h
    });
    options = options.menu_focus(menu_focus.clone());
    let press = use_menu_press("menu", options, window, cx);
    *w.press.borrow_mut() = Some(press.clone());
    let toggle = w.clone();
    let probe = press.clone();
    let trigger = press.trigger(div().id("trigger").w(px(100.)).h(px(30.)).on_click(
        move |_, window, _| {
            if probe.is_trigger_click_from_press() {
                toggle.log("trigger click from press");
                return;
            }
            toggle.log("trigger click");
            toggle.open.set(!toggle.open.get());
            window.refresh();
        },
    ));
    let trigger: AnyElement = if setup.trigger_in_scroller {
        let handle = crate::gestures::scroller::use_scroll_handle("bar", window, cx);
        div()
            .id("bar")
            .h(px(40.))
            .overflow_y_scroll()
            .track_scroll(&handle)
            .child(scroll_begin("bar", Some(&handle), (false, true)))
            .child(div().h(px(400.)).child(trigger))
            .child(scroll_end())
            .into_any_element()
    } else {
        div().h(px(40.)).child(trigger).into_any_element()
    };
    let mut root = div().absolute().left(px(20.)).top(px(20.)).child(trigger);
    if w.open.get() {
        let mut keys: Vec<SharedString> = ROWS.iter().map(|r| SharedString::from(*r)).collect();
        if setup.many_rows {
            keys = (0..30)
                .map(|i| SharedString::from(format!("r{i}")))
                .collect();
        }
        let mut focus = w.focus.borrow_mut();
        let rows: Vec<AnyElement> = keys
            .iter()
            .map(|key| {
                let handle = match focus.iter().find(|(k, _)| k == key) {
                    Some((_, h)) => h.clone(),
                    None => {
                        let h = cx.focus_handle();
                        focus.push((key.clone(), h.clone()));
                        h
                    }
                };
                let (click, act, stray) = (w.clone(), w.clone(), press.clone());
                let k1 = key.clone();
                let lit = press.lit_row().as_ref() == Some(key);
                let el = div()
                    .id(gpui::ElementId::Name(key.clone()))
                    .track_focus(&handle)
                    .h(px(30.))
                    .flex_none()
                    .w(px(120.))
                    .when(lit, |el| el.debug_selector(|| "lit".into()))
                    .on_click(move |event, _, cx| {
                        if !event.is_keyboard() && stray.take_stray_click(cx) {
                            click.log(format!("swallowed {k1}"));
                            return;
                        }
                        click.log(format!("click {k1}"));
                    });
                if key == "b" {
                    // A disabled row is not one the model picks.
                    return el.into_any_element();
                }
                let k2 = key.clone();
                press
                    .row(
                        MenuPressRow::new(key.clone()).focus(&handle).on_activate(
                            move |release, _, _| {
                                act.log(format!(
                                    "act {k2}{}",
                                    if release.modifiers.shift {
                                        " shift"
                                    } else {
                                        ""
                                    }
                                ))
                            },
                        ),
                        el,
                    )
                    .into_any_element()
            })
            .collect();
        drop(focus);
        let list = div()
            .id("list")
            .track_focus(&menu_focus)
            .flex()
            .flex_col()
            .w(px(140.))
            .h(px(120.))
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .child(div().id("content").h(px(10.)).flex_none().on_click({
                let w = w.clone();
                move |_, _, _| w.log("content click")
            }))
            .children(rows);
        root = root.child(press.menu(list, true));
    }
    root.into_any_element()
}

fn at(x: f32, y: f32) -> gpui::Point<gpui::Pixels> {
    point(px(x), px(y))
}

/// The trigger's centre and each row's.
const TRIGGER: (f32, f32) = (70., 35.);
fn row(i: usize) -> (f32, f32) {
    (80., 60. + 10. + 15. + 30. * i as f32)
}

fn mdown(cx: &mut VisualTestContext, (x, y): (f32, f32), modifiers: Modifiers) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at(x, y),
        modifiers,
        click_count: 1,
        first_mouse: false,
    });
    settle(cx);
}

fn mup(cx: &mut VisualTestContext, (x, y): (f32, f32), modifiers: Modifiers) {
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at(x, y),
        modifiers,
        click_count: 1,
    });
    settle(cx);
}

fn mmove(cx: &mut VisualTestContext, (x, y): (f32, f32)) {
    mouse_move(cx, x, y);
    settle(cx);
}

fn finger(cx: &mut VisualTestContext, id: u64, phase: TouchPhase, (x, y): (f32, f32)) {
    touch(cx, id, phase, x, y);
    settle(cx);
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn open_by_click(cx: &mut VisualTestContext, w: &World) {
    mdown(cx, TRIGGER, Modifiers::none());
    mup(cx, TRIGGER, Modifiers::none());
    assert!(w.open.get());
    w.log.borrow_mut().clear();
    wait(cx, 1000);
}

// --- The trigger -------------------------------------------------------------

#[gpui::test]
fn a_mouse_press_on_the_trigger_opens_the_menu_and_that_gestures_click_does_not_toggle_it(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(cx, Setup::default());
    mdown(cx, TRIGGER, Modifiers::none());
    assert!(w.open.get(), "a mouse opens on the press");
    mup(cx, TRIGGER, Modifiers::none());
    assert!(w.open.get(), "its release on the trigger leaves it open");
    assert_eq!(
        w.logged(),
        ["trigger press Mouse", "trigger click from press"]
    );
}

#[gpui::test]
fn a_mouse_press_on_the_trigger_of_an_open_menu_closes_it_and_does_not_reopen_it(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    mdown(cx, TRIGGER, Modifiers::none());
    mup(cx, TRIGGER, Modifiers::none());
    assert!(!w.open.get());
    assert_eq!(
        w.logged(),
        ["trigger press Mouse", "trigger click from press"]
    );
}

#[gpui::test]
fn a_click_in_a_later_gesture_is_not_the_presss(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    assert!(
        !w.press().is_trigger_click_from_press(),
        "spent by its own click"
    );
}

#[gpui::test]
fn a_drag_from_the_trigger_into_the_menu_that_releases_over_a_row_acts_on_it(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(cx, Setup::default());
    mdown(cx, TRIGGER, Modifiers::none());
    mmove(cx, row(2));
    mup(cx, row(2), Modifiers::none());
    assert!(w.logged().contains(&"act c".to_string()));
    assert!(
        !w.logged().iter().any(|l| l.starts_with("click")),
        "and nothing clicks again"
    );
}

#[gpui::test]
fn the_opening_release_acts_on_nothing_before_the_settle_time_and_acts_after_it(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(cx, Setup::default());
    mdown(cx, TRIGGER, Modifiers::none());
    // Released over a row without entering it, at once: nothing acts.
    mup(cx, TRIGGER, Modifiers::none());
    assert!(!w.logged().iter().any(|l| l.starts_with("act")));
    // Closed and pressed again, held past the settle time.
    mdown(cx, TRIGGER, Modifiers::none());
    mup(cx, TRIGGER, Modifiers::none());
    mdown(cx, TRIGGER, Modifiers::none());
    wait(cx, 400);
    mmove(cx, row(0));
    mup(cx, row(0), Modifiers::none());
    assert!(w.logged().contains(&"act a".to_string()));
}

#[gpui::test]
fn under_the_uikit_model_a_finger_opens_the_menu_as_it_presses_and_tracks_after_the_handoff(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(cx, Setup::default());
    finger(cx, 1, TouchPhase::Started, TRIGGER);
    assert!(
        w.open.get(),
        "a trigger in no scroll view opens as the finger presses"
    );
    // Without the handoff travel the lift picks nothing and the menu stays.
    finger(cx, 1, TouchPhase::Moved, (TRIGGER.0, TRIGGER.1 + 3.));
    finger(cx, 1, TouchPhase::Ended, (TRIGGER.0, TRIGGER.1 + 3.));
    assert!(w.open.get());
    assert!(!w.logged().iter().any(|l| l.starts_with("act")));
    // Opened again under a finger that travels onto a row: it picks there.
    w.open.set(false);
    settle(cx);
    finger(cx, 2, TouchPhase::Started, TRIGGER);
    finger(cx, 2, TouchPhase::Moved, (TRIGGER.0, TRIGGER.1 + 2.));
    finger(cx, 2, TouchPhase::Moved, row(2));
    assert!(
        cx.debug_bounds("lit").is_some(),
        "the finger lights the row it is over"
    );
    finger(cx, 2, TouchPhase::Ended, row(2));
    assert!(w.logged().contains(&"act c".to_string()));
    assert!(cx.debug_bounds("lit").is_none(), "cleared at the lift");
    assert!(
        !w.logged().iter().any(|l| l.starts_with("click")),
        "the lift taps nothing"
    );
}

#[gpui::test]
fn the_pointer_model_opens_a_held_finger_at_its_delay_and_a_tap_opens_through_the_click(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(
        cx,
        Setup {
            delay: Some(500.),
            ..Setup::default()
        },
    );
    finger(cx, 1, TouchPhase::Started, TRIGGER);
    assert!(!w.open.get(), "the delay first");
    wait(cx, 499);
    assert!(!w.open.get());
    wait(cx, 1);
    assert!(w.open.get(), "held for the delay, it opens");
    finger(cx, 1, TouchPhase::Ended, TRIGGER);
    // The settle rule: a still finger's lift over a row acts once it settled.
    w.open.set(false);
    w.log.borrow_mut().clear();
    settle(cx);
    finger(cx, 2, TouchPhase::Started, TRIGGER);
    wait(cx, 100);
    finger(cx, 2, TouchPhase::Ended, TRIGGER);
    assert_eq!(
        w.logged(),
        ["trigger click"],
        "a tap's click opens the menu"
    );
    assert!(w.open.get());
}

#[gpui::test]
fn inside_a_scroll_view_a_held_finger_opens_once_the_content_touch_delay_passes(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(
        cx,
        Setup {
            trigger_in_scroller: true,
            ..Setup::default()
        },
    );
    finger(cx, 1, TouchPhase::Started, TRIGGER);
    wait(cx, 149);
    assert!(!w.open.get(), "closed at 149 ms");
    wait(cx, 1);
    assert!(w.open.get(), "open at 150 ms");
    finger(cx, 1, TouchPhase::Ended, TRIGGER);
}

#[gpui::test]
fn inside_a_scroll_view_a_finger_that_drifts_past_the_allowance_is_a_scroll(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(
        cx,
        Setup {
            trigger_in_scroller: true,
            ..Setup::default()
        },
    );
    finger(cx, 1, TouchPhase::Started, TRIGGER);
    // 12 pt still holds.
    finger(cx, 1, TouchPhase::Moved, (TRIGGER.0 + 12., TRIGGER.1));
    wait(cx, 200);
    assert!(w.open.get(), "a 12 pt drift still opens");
    finger(cx, 1, TouchPhase::Ended, (TRIGGER.0 + 12., TRIGGER.1));
    w.open.set(false);
    settle(cx);
    finger(cx, 2, TouchPhase::Started, TRIGGER);
    finger(cx, 2, TouchPhase::Moved, (TRIGGER.0 + 14., TRIGGER.1));
    wait(cx, 200);
    assert!(!w.open.get(), "past 13 pt the scroll takes the touch");
    finger(cx, 2, TouchPhase::Ended, (TRIGGER.0 + 14., TRIGGER.1));
}

// --- Tracking inside the menu ------------------------------------------------

#[gpui::test]
fn acts_on_the_row_under_the_release_not_the_row_the_press_began_on(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    mdown(cx, row(0), Modifiers::none());
    mmove(cx, row(2));
    mup(cx, row(2), Modifiers::none());
    assert_eq!(w.logged(), ["act c"]);
}

#[gpui::test]
fn moves_the_highlight_as_focus_and_clears_it_over_a_disabled_row(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    mdown(cx, row(0), Modifiers::none());
    let focused = |cx: &mut VisualTestContext, key: &str| {
        let handle = w
            .focus
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, h)| h.clone())
            .unwrap();
        cx.update(|window, _| handle.is_focused(window))
    };
    assert!(focused(cx, "a"));
    mmove(cx, row(2));
    assert!(focused(cx, "c"));
    // Over the disabled row nothing is highlighted, and its release acts on nothing.
    mmove(cx, row(1));
    assert!(!focused(cx, "c") && !focused(cx, "b"));
    mup(cx, row(1), Modifiers::none());
    assert!(!w.logged().iter().any(|l| l.starts_with("act")));
}

#[gpui::test]
fn drives_a_custom_highlight_callback_instead_of_focus(cx: &mut TestAppContext) {
    let (cx, w) = mount(
        cx,
        Setup {
            custom_highlight: true,
            ..Setup::default()
        },
    );
    open_by_click(cx, &w);
    mdown(cx, row(0), Modifiers::none());
    mmove(cx, row(1));
    mmove(cx, row(2));
    assert_eq!(w.logged(), ["highlight a", "highlight none", "highlight c"]);
    mup(cx, row(2), Modifiers::none());
}

#[gpui::test]
fn carries_the_release_modifiers_on_the_activation(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    mdown(cx, row(0), Modifiers::none());
    mup(cx, row(2), shift);
    assert!(w.logged().contains(&"act c shift".to_string()));
}

#[gpui::test]
fn a_held_mouse_lights_no_row_and_a_held_finger_lights_the_row_under_it(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    mdown(cx, row(0), Modifiers::none());
    assert!(
        cx.debug_bounds("lit").is_none(),
        "a mouse keeps the hover tint"
    );
    mup(cx, row(0), Modifiers::none());
    w.open.set(true);
    settle(cx);
    finger(cx, 1, TouchPhase::Started, row(2));
    assert!(cx.debug_bounds("lit").is_some());
    finger(cx, 1, TouchPhase::Ended, row(2));
}

// --- One gesture, one activation ---------------------------------------------

#[gpui::test]
fn a_tap_on_a_row_acts_once_though_the_platform_also_reports_a_click(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    mdown(cx, row(2), Modifiers::none());
    mup(cx, row(2), Modifiers::none());
    assert_eq!(w.logged(), ["act c", "swallowed c"]);
    // A finger's tap: its lift is claimed, so the platform makes no click at all.
    w.log.borrow_mut().clear();
    finger(cx, 1, TouchPhase::Started, row(0));
    finger(cx, 1, TouchPhase::Ended, row(0));
    assert_eq!(w.logged(), ["act a"]);
}

#[gpui::test]
fn the_swallower_is_disarmed_after_its_window_and_at_the_next_press(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    let press = w.press();
    // A release on the menu's chrome acts on nothing; the click it may still send must not act.
    mdown(cx, row(0), Modifiers::none());
    mmove(cx, row(1));
    mup(cx, row(1), Modifiers::none());
    wait(cx, 450);
    assert!(
        !cx.update(|_, cx| press.take_stray_click(cx)),
        "the short window ran out"
    );
    mdown(cx, row(0), Modifiers::none());
    mmove(cx, row(1));
    mup(cx, row(1), Modifiers::none());
    mdown(cx, (600., 500.), Modifiers::none());
    assert!(
        !cx.update(|_, cx| press.take_stray_click(cx)),
        "the next press disarms it"
    );
    mup(cx, (600., 500.), Modifiers::none());
}

#[gpui::test]
fn a_click_with_no_tracked_gesture_behind_it_acts_as_always(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    let handle = w
        .focus
        .borrow()
        .iter()
        .find(|(k, _)| k == "a")
        .map(|(_, h)| h.clone())
        .unwrap();
    cx.update(|window, cx| handle.focus(window, cx));
    let enter = gpui::Keystroke::parse("enter").unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: enter.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke: enter });
    settle(cx);
    assert_eq!(w.logged(), ["click a"], "a keyboard activation passes");
}

// --- Release outside and cancellation ----------------------------------------

#[gpui::test]
fn a_mouse_released_outside_dismisses_and_a_finger_released_outside_leaves_it_open(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    finger(cx, 1, TouchPhase::Started, row(0));
    finger(cx, 1, TouchPhase::Moved, (600., 500.));
    finger(cx, 1, TouchPhase::Ended, (600., 500.));
    assert!(w.open.get(), "a finger leaves it open");
    mdown(cx, row(0), Modifiers::none());
    mmove(cx, (600., 500.));
    mup(cx, (600., 500.), Modifiers::none());
    assert!(!w.open.get(), "a mouse dismisses");
    assert!(!w.logged().iter().any(|l| l.starts_with("act")));
}

#[gpui::test]
fn a_cancel_ends_the_gesture_without_acting_and_clears_the_lit_row(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    finger(cx, 1, TouchPhase::Started, row(0));
    assert!(cx.debug_bounds("lit").is_some());
    finger(cx, 1, TouchPhase::Cancelled, row(0));
    assert!(cx.debug_bounds("lit").is_none());
    assert!(w.logged().is_empty());
    assert!(!w.press().is_active());
}

#[gpui::test]
fn a_second_finger_ends_the_gesture_and_starts_no_other(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    finger(cx, 1, TouchPhase::Started, row(0));
    finger(cx, 2, TouchPhase::Started, row(2));
    assert!(!w.press().is_active());
    finger(cx, 2, TouchPhase::Moved, row(0));
    finger(cx, 2, TouchPhase::Ended, row(0));
    finger(cx, 1, TouchPhase::Ended, row(0));
    assert!(w.logged().is_empty(), "{:?}", w.logged());
    // The next press acts as usual.
    finger(cx, 3, TouchPhase::Started, row(2));
    finger(cx, 3, TouchPhase::Ended, row(2));
    assert_eq!(w.logged(), ["act c"]);
}

#[gpui::test]
fn ignores_a_pointer_of_another_kind_while_one_is_tracked_and_ends_its_gesture(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(cx, Setup::default());
    open_by_click(cx, &w);
    finger(cx, 1, TouchPhase::Started, row(0));
    mdown(cx, row(2), Modifiers::none());
    assert!(!w.press().is_active(), "a second pointer ends it");
    mup(cx, row(2), Modifiers::none());
    finger(cx, 1, TouchPhase::Ended, row(0));
    assert!(!w.logged().iter().any(|l| l.starts_with("act")));
}

// --- Edge autoscroll and caller content ----------------------------------------

#[gpui::test]
fn a_pointer_resting_near_the_bottom_edge_scrolls_the_menu_and_stops_when_it_leaves(
    cx: &mut TestAppContext,
) {
    let (cx, w) = mount(
        cx,
        Setup {
            many_rows: true,
            ..Setup::default()
        },
    );
    open_by_click(cx, &w);
    let scroll = w.scroll.borrow().clone().unwrap();
    mdown(cx, row(0), Modifiers::none());
    // The list spans y 60..180; its bottom zone is the last 24 px.
    mmove(cx, (80., 175.));
    wait(cx, 200);
    let scrolled = -f32::from(scroll.offset().y);
    assert!(scrolled > 20., "it scrolled down: {scrolled}");
    mmove(cx, (80., 120.));
    let stopped = -f32::from(scroll.offset().y);
    wait(cx, 200);
    assert_eq!(
        -f32::from(scroll.offset().y),
        stopped,
        "it stops outside the zone"
    );
    mup(cx, (80., 120.), Modifiers::none());
}

#[gpui::test]
fn a_press_on_caller_content_beside_the_rows_stays_that_contents(cx: &mut TestAppContext) {
    let (cx, w) = mount(
        cx,
        Setup {
            items_only: true,
            ..Setup::default()
        },
    );
    open_by_click(cx, &w);
    // The content strip sits above the first row.
    mdown(cx, (80., 65.), Modifiers::none());
    assert!(
        !w.press().is_active(),
        "the model takes no press off its rows"
    );
    mup(cx, (80., 65.), Modifiers::none());
    assert_eq!(w.logged(), ["content click"]);
    assert!(w.open.get());
}

// --- The window loses focus mid-press (spec:AST-094 FR28) ------------------------

#[gpui::test]
fn after_the_window_loses_focus_mid_press_a_fingers_lift_acts_on_nothing(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    cx.update(|window, _| window.activate_window());
    settle(cx);
    open_by_click(cx, &w);
    finger(cx, 1, TouchPhase::Started, row(0));
    assert!(cx.debug_bounds("lit").is_some());
    cx.deactivate_window();
    settle(cx);
    assert!(
        cx.debug_bounds("lit").is_none(),
        "the withdrawn finger's lit row clears"
    );
    assert!(
        w.press().is_active(),
        "withdrawn, not ended: tracked to its lift"
    );
    // Nothing it does highlights or acts, and its lift's click is the gesture's.
    finger(cx, 1, TouchPhase::Moved, row(2));
    assert!(cx.debug_bounds("lit").is_none());
    finger(cx, 1, TouchPhase::Ended, row(2));
    assert!(w.logged().is_empty(), "{:?}", w.logged());
    assert!(!w.press().is_active());
    assert!(
        cx.update(|_, cx| w.press().take_stray_click(cx)),
        "its click is swallowed"
    );
    // The next press acts as usual.
    cx.update(|window, _| window.activate_window());
    settle(cx);
    finger(cx, 2, TouchPhase::Started, row(2));
    finger(cx, 2, TouchPhase::Ended, row(2));
    assert_eq!(w.logged(), ["act c"]);
}

#[gpui::test]
fn a_press_ends_a_withdrawn_gesture_whose_lift_the_window_never_saw(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    cx.update(|window, _| window.activate_window());
    settle(cx);
    open_by_click(cx, &w);
    finger(cx, 1, TouchPhase::Started, row(0));
    cx.deactivate_window();
    settle(cx);
    cx.update(|window, _| window.activate_window());
    settle(cx);
    // The finger lifted where the window could not see it: the next press ends the
    // withdrawn gesture, and its click acts as the row's own (FR5); the press after it is
    // the model's again.
    finger(cx, 2, TouchPhase::Started, row(2));
    finger(cx, 2, TouchPhase::Ended, row(2));
    assert_eq!(w.logged(), ["click c"]);
    finger(cx, 3, TouchPhase::Started, row(2));
    finger(cx, 3, TouchPhase::Ended, row(2));
    assert_eq!(w.logged(), ["click c", "act c"]);
}

#[gpui::test]
fn a_mouse_press_the_blur_interrupts_ends_as_a_cancel_and_keeps_its_click(cx: &mut TestAppContext) {
    let (cx, w) = mount(cx, Setup::default());
    cx.update(|window, _| window.activate_window());
    settle(cx);
    open_by_click(cx, &w);
    mdown(cx, row(0), Modifiers::none());
    cx.deactivate_window();
    settle(cx);
    assert!(!w.press().is_active(), "a mouse's gesture ends");
    mup(cx, row(0), Modifiers::none());
    assert_eq!(
        w.logged(),
        ["click a"],
        "nothing acted for the gesture; its click passes"
    );
}
