//! The touch presentations under a coarse primary pointer, after upstream
//! `Layer/holdEntranceInput.test.ts`, `AlertDialog/useAlertMotion.test.tsx` and the
//! menus' `presentation` (`DropdownMenu.doc.mjs`, `useMenuPress.test.tsx`).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement, Pixels, Point,
    Render, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::*;
use crate::button::Button;

type Log = Rc<RefCell<Vec<&'static str>>>;

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    coarse: Option<bool>,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::modality::set_primary_pointer_coarse(coarse, cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    settle(cx);
}

fn finger(cx: &mut VisualTestContext, id: u64, phase: gpui::TouchPhase, p: Point<Pixels>) {
    cx.simulate_event(gpui::TouchEvent {
        id: gpui::TouchId(id),
        phase,
        position: p,
        predicted_position: None,
        force: None,
    });
    settle(cx);
}

fn tap(cx: &mut VisualTestContext, id: u64, p: Point<Pixels>) {
    finger(cx, id, gpui::TouchPhase::Started, p);
    finger(cx, id, gpui::TouchPhase::Ended, p);
}

fn center(cx: &mut VisualTestContext, selector: &'static str) -> Point<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center()
}

/// An open dialog whose content is one button that logs its clicks.
fn dialog(log: &Log) -> impl Fn() -> AnyElement + 'static {
    let log = log.clone();
    move || {
        let log = log.clone();
        Dialog::new("d", true)
            .label("Dialog")
            .child(
                div()
                    .flex()
                    .child(
                        div().debug_selector(|| "d-button".into()).child(
                            Button::new("d-ok", "OK")
                                .on_click(move |_, _, _| log.borrow_mut().push("ok")),
                        ),
                    ),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn holds_the_layer_until_its_entrance_ends_under_a_coarse_primary_pointer(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, Some(true), dialog(&log));
    let at = center(cx, "d-button");
    tap(cx, 1, at);
    assert!(
        log.borrow().is_empty(),
        "a tap during the entrance reaches nothing"
    );
    // The entrance (--duration-medium-max) has ended: the next tap acts.
    wait(cx, 1000);
    let at = center(cx, "d-button");
    tap(cx, 2, at);
    assert_eq!(*log.borrow(), ["ok"]);
}

#[gpui::test]
fn decides_by_each_events_pointer_a_mouse_click_during_the_entrance_passes_ast094_fr35(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let cx = host(cx, Some(true), dialog(&log));
    let at = center(cx, "d-button");
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
    assert_eq!(*log.borrow(), ["ok"], "a mouse is never held");
}

#[gpui::test]
fn holds_nothing_under_a_fine_primary_pointer_ast094_fr35_fr24(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, Some(false), dialog(&log));
    let at = center(cx, "d-button");
    tap(cx, 1, at);
    assert_eq!(
        *log.borrow(),
        ["ok"],
        "a finger on a device whose primary pointer is fine is not held"
    );
}

#[gpui::test]
fn holds_a_finger_that_landed_during_the_hold_to_its_end(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, Some(true), dialog(&log));
    let at = center(cx, "d-button");
    finger(cx, 1, gpui::TouchPhase::Started, at);
    wait(cx, 1000);
    finger(cx, 1, gpui::TouchPhase::Ended, at);
    assert!(
        log.borrow().is_empty(),
        "its release after the hold's end stays held"
    );
    tap(cx, 2, at);
    assert_eq!(*log.borrow(), ["ok"], "the next tap acts");
}

// --- The menus' presentation and press model ------------------------------

fn menu(presentation: Option<PopoverPresentation>) -> impl Fn() -> AnyElement + 'static {
    move || {
        let mut menu = DropdownMenu::new("m")
            .button_label("Menu")
            .entry(MenuItem::new("a", "Alpha"))
            .entry(MenuItem::new("b", "Beta"));
        if let Some(presentation) = presentation {
            menu = menu.presentation(presentation);
        }
        div()
            .debug_selector(|| "m-trigger".into())
            .p(px(40.))
            .child(menu)
            .into_any_element()
    }
}

fn menu_open(cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds("m-list-a").is_some()
}

#[gpui::test]
fn omitted_a_finger_opens_the_menu_as_it_presses_its_trigger(cx: &mut TestAppContext) {
    let cx = host(cx, Some(true), menu(None));
    let at = point(px(70.), px(55.));
    finger(cx, 1, gpui::TouchPhase::Started, at);
    assert!(menu_open(cx), "UIMenu: the trigger opens as it is pressed");
    finger(cx, 1, gpui::TouchPhase::Ended, at);
}

#[gpui::test]
fn an_explicit_popover_keeps_the_pointer_model_a_finger_opens_after_the_500ms_hold(
    cx: &mut TestAppContext,
) {
    let cx = host(cx, Some(true), menu(Some(PopoverPresentation::Popover)));
    assert!(!menu_open(cx), "closed at first");
    let at = point(px(70.), px(55.));
    finger(cx, 1, gpui::TouchPhase::Started, at);
    assert!(!menu_open(cx), "not as it presses");
    wait(cx, 600);
    assert!(menu_open(cx), "after the 500 ms hold");
    finger(cx, 1, gpui::TouchPhase::Ended, at);
}

#[gpui::test]
fn adaptive_is_the_sheet_only_on_a_compact_touch_screen(cx: &mut TestAppContext) {
    let cx = host(cx, Some(true), menu(Some(PopoverPresentation::Adaptive)));
    // A 1024 x 768 window under a finger is no phone: the menu stays anchored.
    cx.update(|window, cx| {
        assert!(!compact_touch(window, cx));
        assert!(!presents_as_sheet(
            PopoverPresentation::Adaptive,
            window,
            cx
        ));
    });
    cx.simulate_resize(gpui::size(px(393.), px(852.)));
    settle(cx);
    cx.update(|window, cx| {
        assert!(compact_touch(window, cx));
        assert!(presents_as_sheet(PopoverPresentation::Adaptive, window, cx));
    });
}

// --- AlertDialog's touch motion (upstream useAlertMotion) -----------------

fn timing(reduced: bool) -> super::alert_dialog::AlertTiming {
    let linear = rustryx_theme::CubicBezier(0., 0., 1., 1.);
    let cross_fade = Duration::from_millis(200);
    super::alert_dialog::AlertTiming {
        entrance: if reduced {
            (cross_fade, linear)
        } else {
            (Duration::from_millis(400), linear)
        },
        exit: if reduced {
            (cross_fade, linear)
        } else {
            (Duration::from_millis(200), linear)
        },
        reduced,
    }
}

#[test]
fn alert_presents_under_a_finger_from_1_2_and_transparent_holding_input_until_it_has_entered() {
    use super::alert_dialog::AlertMotion;
    let start = web_time::Instant::now();
    let mut m = AlertMotion::default();
    let f = m.frame(true, true, 1.2, timing(false), start);
    let motion = f.motion.expect("the motion drives it");
    assert_eq!((motion.opacity, motion.scale), (0., 1.2));
    assert_eq!(f.hold_until, Some(start + Duration::from_millis(400)));
    let f = m.frame(
        true,
        true,
        1.2,
        timing(false),
        start + Duration::from_millis(200),
    );
    let motion = f.motion.unwrap();
    assert!((motion.opacity - 0.5).abs() < 1e-4 && (motion.scale - 1.1).abs() < 1e-4);
    let f = m.frame(
        true,
        true,
        1.2,
        timing(false),
        start + Duration::from_millis(400),
    );
    assert_eq!(f.motion.map(|m| (m.opacity, m.scale)), Some((1., 1.)));
    assert!(
        !f.moving && f.hold_until.is_none(),
        "entered: input is live"
    );
}

#[test]
fn alert_start_scale_is_never_wider_than_the_window() {
    use super::alert_dialog::AlertMotion;
    assert_eq!(AlertMotion::scale_from(320., 1024.), 1.2);
    assert!((AlertMotion::scale_from(320., 352.) - 1.1).abs() < 1e-4);
    assert_eq!(AlertMotion::scale_from(400., 300.), 1.);
}

#[test]
fn alert_close_fades_out_inert_and_stays_presented_until_the_exit_ends() {
    use super::alert_dialog::AlertMotion;
    let start = web_time::Instant::now();
    let mut m = AlertMotion::default();
    m.frame(true, true, 1.2, timing(false), start);
    m.frame(
        true,
        true,
        1.2,
        timing(false),
        start + Duration::from_millis(500),
    );
    let close = start + Duration::from_millis(600);
    let f = m.frame(false, true, 1.2, timing(false), close);
    assert!(f.presented, "the Dialog stays open while the alert leaves");
    assert!(f.motion.is_some_and(|m| m.inert && m.opacity == 1.));
    let f = m.frame(
        false,
        true,
        1.2,
        timing(false),
        close + Duration::from_millis(100),
    );
    assert!(f.motion.is_some_and(|m| (m.opacity - 0.5).abs() < 1e-4));
    let f = m.frame(
        false,
        true,
        1.2,
        timing(false),
        close + Duration::from_millis(200),
    );
    assert!(
        !f.presented && f.motion.is_none(),
        "hidden once the exit ends"
    );
}

#[test]
fn alert_reopened_during_the_exit_fades_back_from_where_it_is() {
    use super::alert_dialog::AlertMotion;
    let start = web_time::Instant::now();
    let mut m = AlertMotion::default();
    m.frame(true, true, 1.2, timing(false), start);
    m.frame(
        true,
        true,
        1.2,
        timing(false),
        start + Duration::from_millis(400),
    );
    let close = start + Duration::from_millis(500);
    m.frame(false, true, 1.2, timing(false), close);
    m.frame(
        false,
        true,
        1.2,
        timing(false),
        close + Duration::from_millis(100),
    );
    let reopen = close + Duration::from_millis(100);
    let f = m.frame(true, true, 1.2, timing(false), reopen);
    let motion = f.motion.unwrap();
    assert!(!motion.inert && (motion.opacity - 0.5).abs() < 1e-4 && motion.scale == 1.);
}

#[test]
fn alert_under_reduce_motion_cross_fades_without_scale() {
    use super::alert_dialog::AlertMotion;
    let start = web_time::Instant::now();
    let mut m = AlertMotion::default();
    let f = m.frame(true, true, 1.2, timing(true), start);
    assert_eq!(f.motion.map(|m| m.scale), Some(1.));
    assert_eq!(f.hold_until, Some(start + Duration::from_millis(200)));
}

#[test]
fn alert_under_a_mouse_keeps_the_theme_entrance() {
    use super::alert_dialog::AlertMotion;
    let mut m = AlertMotion::default();
    let f = m.frame(true, false, 1.2, timing(false), web_time::Instant::now());
    assert!(f.presented && f.motion.is_none() && f.hold_until.is_none());
}

#[test]
fn alert_on_a_phone_defaults_to_the_ios_alert_width() {
    use super::alert_dialog::default_alert_width;
    let phone = TouchPresentation {
        is_touch: true,
        is_compact: true,
    };
    assert_eq!(default_alert_width(phone), 320.);
    assert_eq!(
        default_alert_width(TouchPresentation {
            is_touch: true,
            is_compact: false
        }),
        400.
    );
    assert_eq!(
        default_alert_width(TouchPresentation {
            is_touch: false,
            is_compact: true
        }),
        400.
    );
}

// --- Popover's touch growth (upstream usePopoverSurfaceMotion) ----------------

fn popover() -> impl Fn() -> AnyElement + 'static {
    || {
        div()
            .flex()
            .p(px(40.))
            .child(
                Popover::new("p", "Filters")
                    .content(
                        div()
                            .debug_selector(|| "p-content".into())
                            .w(px(120.))
                            .h(px(80.)),
                    )
                    .child(
                        div()
                            .debug_selector(|| "p-trigger".into())
                            .child(Button::new("p-trigger-button", "Filters")),
                    ),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn grows_the_popover_out_of_its_source_under_a_finger_and_shrinks_it_back_as_it_closes(
    cx: &mut TestAppContext,
) {
    let cx = host(cx, Some(true), popover());
    let at = center(cx, "p-trigger");
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
    assert!(cx.debug_bounds("p-content").is_some(), "it opens");
    // While it grows, no finger reaches anything.
    cx.update(|window, cx| {
        assert!(rustryx_primitives::gestures::input_hold::is_holding(
            window,
            &gpui::ElementId::Name("p-grow-hold".into()),
            cx
        ));
    });
    wait(cx, 1000);
    cx.update(|window, cx| {
        assert!(!rustryx_primitives::gestures::input_hold::is_holding(
            window,
            &gpui::ElementId::Name("p-grow-hold".into()),
            cx
        ));
    });
    // Closing: it shrinks back into its trigger, still drawn, then is gone.
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
    assert!(cx.debug_bounds("p-content").is_some(), "the shrink plays");
    wait(cx, 1000);
    assert!(cx.debug_bounds("p-content").is_none(), "then it is gone");
}

#[gpui::test]
fn a_fine_primary_pointer_keeps_the_layers_own_entrance_and_closes_at_once(
    cx: &mut TestAppContext,
) {
    let cx = host(cx, Some(false), popover());
    let at = center(cx, "p-trigger");
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
    assert!(cx.debug_bounds("p-content").is_some());
    cx.update(|window, cx| {
        assert!(!rustryx_primitives::gestures::input_hold::is_holding(
            window,
            &gpui::ElementId::Name("p-grow-hold".into()),
            cx
        ));
    });
    wait(cx, 1000);
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
    assert!(cx.debug_bounds("p-content").is_none());
}

// --- The touch hit area (upstream utils/touchHitArea.ts, hitArea.test.ts) -----

fn small_button(log: &Log) -> impl Fn() -> AnyElement + 'static {
    let log = log.clone();
    move || {
        let log = log.clone();
        div()
            .flex()
            .p(px(80.))
            .child(
                div().debug_selector(|| "hit-button".into()).child(
                    crate::button::IconButton::new("hit", "Close", crate::icon::Icon::new("close"))
                        .size(rustryx_primitives::ElementSize::Sm)
                        .on_click(move |_, _, _| log.borrow_mut().push("close")),
                ),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn a_small_buttons_target_grows_toward_44_px_under_a_coarse_primary_pointer(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let cx = host(cx, Some(true), small_button(&log));
    let b = cx.debug_bounds("hit-button").unwrap();
    assert!(b.size.height < px(44.), "a small button");
    let just_below = point(b.center().x, b.bottom() + px(3.));
    cx.simulate_click(just_below, gpui::Modifiers::none());
    settle(cx);
    assert_eq!(
        *log.borrow(),
        ["close"],
        "a tap just past its edge is its own"
    );
}

#[gpui::test]
fn measures_nothing_where_the_primary_pointer_is_fine(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, Some(false), small_button(&log));
    let b = cx.debug_bounds("hit-button").unwrap();
    cx.simulate_click(
        point(b.center().x, b.bottom() + px(3.)),
        gpui::Modifiers::none(),
    );
    settle(cx);
    assert!(
        log.borrow().is_empty(),
        "the exact box under a fine pointer"
    );
}

fn two_buttons(log: &Log) -> impl Fn() -> AnyElement + 'static {
    let log = log.clone();
    move || {
        let (a, b) = (log.clone(), log.clone());
        div()
            .flex()
            .gap(px(8.))
            .p(px(80.))
            .child(
                div().debug_selector(|| "hit-a".into()).child(
                    crate::button::IconButton::new(
                        "hit-a-button",
                        "A",
                        crate::icon::Icon::new("close"),
                    )
                    .size(rustryx_primitives::ElementSize::Sm)
                    .on_click(move |_, _, _| a.borrow_mut().push("a")),
                ),
            )
            .child(
                div().debug_selector(|| "hit-b".into()).child(
                    crate::button::IconButton::new(
                        "hit-b-button",
                        "B",
                        crate::icon::Icon::new("close"),
                    )
                    .size(rustryx_primitives::ElementSize::Sm)
                    .on_click(move |_, _, _| b.borrow_mut().push("b")),
                ),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn stops_at_half_the_gap_to_a_neighbour_so_a_tap_in_the_gap_goes_to_the_nearer_button(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let cx = host(cx, Some(true), two_buttons(&log));
    let a = cx.debug_bounds("hit-a").unwrap();
    // 3 px past A's end, nearer A than B (8 px apart).
    cx.simulate_click(
        point(a.right() + px(3.), a.center().y),
        gpui::Modifiers::none(),
    );
    settle(cx);
    // 5 px past it, nearer B.
    cx.simulate_click(
        point(a.right() + px(5.), a.center().y),
        gpui::Modifiers::none(),
    );
    settle(cx);
    assert_eq!(*log.borrow(), ["a", "b"]);
}

fn checkbox(log: &Log) -> impl Fn() -> AnyElement + 'static {
    let log = log.clone();
    move || {
        let log = log.clone();
        div()
            .flex()
            .p(px(80.))
            .child(
                div().debug_selector(|| "hit-checkbox".into()).child(
                    crate::choice::CheckboxInput::new("hit-check", "Remember me", false)
                        .on_change(move |_, _, _| log.borrow_mut().push("toggle")),
                ),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn a_checkbox_rows_target_grows_toward_44_px_under_a_coarse_primary_pointer(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let cx = host(cx, Some(true), checkbox(&log));
    let b = cx.debug_bounds("hit-checkbox").unwrap();
    assert!(b.size.height < px(44.));
    cx.simulate_click(
        point(b.center().x, b.bottom() + px(3.)),
        gpui::Modifiers::none(),
    );
    settle(cx);
    assert_eq!(
        *log.borrow(),
        ["toggle"],
        "a tap just below the row is the row's"
    );
}

#[gpui::test]
fn a_finger_on_the_grown_area_shows_the_buttons_press_paint(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, Some(true), small_button(&log));
    let b = cx.debug_bounds("hit-button").unwrap();
    let at = point(b.center().x, b.bottom() + px(3.));
    finger(cx, 1, gpui::TouchPhase::Started, at);
    wait(cx, 200);
    let painted =
        cx.update(|window, cx| crate::gestures::press_feedback::painted_surfaces(window, cx));
    assert!(
        painted
            .iter()
            .any(|(_, paint)| *paint == crate::gestures::PressPaint::On),
        "the press shows: {painted:?}"
    );
    finger(cx, 1, gpui::TouchPhase::Ended, at);
    assert_eq!(*log.borrow(), ["close"]);
}

#[gpui::test]
fn any_pointer_coarse_holds_for_a_declared_touch_device_and_a_window_that_saw_a_finger(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let cx = host(cx, Some(false), small_button(&log));
    cx.update(|window, cx| {
        assert!(!rustryx_primitives::modality::any_pointer_coarse(
            window, cx
        ))
    });
    cx.update(|_, cx| rustryx_primitives::modality::set_primary_pointer_coarse(Some(true), cx));
    cx.update(|window, cx| assert!(rustryx_primitives::modality::any_pointer_coarse(window, cx)));
    // Undeclared, a window that has seen a finger counts.
    cx.update(|_, cx| rustryx_primitives::modality::set_primary_pointer_coarse(None, cx));
    let b = cx.debug_bounds("hit-button").unwrap();
    finger(cx, 1, gpui::TouchPhase::Started, b.center());
    finger(cx, 1, gpui::TouchPhase::Ended, b.center());
    cx.update(|window, cx| {
        rustryx_primitives::coarse_primary_pointer(window, cx);
        assert!(rustryx_primitives::modality::any_pointer_coarse(window, cx));
    });
}
