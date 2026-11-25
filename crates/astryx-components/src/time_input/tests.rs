//! TimeInput tests, numbered after `TimeInput.spec.md`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::date::{HourFormat, PlainTime, set_time_now};
use astryx_i18n::{I18n, provider};
use astryx_primitives::Politeness;
use gpui::{IntoElement, TestAppContext, VisualTestContext};

use super::*;
use crate::field::EntryErrorBehavior;
use crate::segmented::tests::{host, press};

fn t(h: u8, m: u8) -> PlainTime {
    PlainTime::new(h, m, 0).unwrap()
}

#[derive(Default)]
struct Log {
    changes: RefCell<Vec<Option<PlainTime>>>,
    errors: RefCell<Vec<Option<TimeInputEntryError>>>,
}

fn field(
    cx: &mut TestAppContext,
    initial: Option<PlainTime>,
    configure: impl Fn(TimeInput) -> TimeInput + 'static,
) -> (&mut VisualTestContext, Rc<Log>) {
    cx.update(astryx_primitives::editing::init);
    let log = Rc::new(Log::default());
    let value = Rc::new(RefCell::new(initial));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let (log, errors, value) = (log.clone(), log.clone(), value.clone());
            let current = *value.borrow();
            configure(
                TimeInput::new("ti", "Start", current)
                    .on_change(move |v, window, _| {
                        log.changes.borrow_mut().push(v);
                        *value.borrow_mut() = v;
                        window.refresh();
                    })
                    .on_entry_error_change(move |e, _, _| errors.errors.borrow_mut().push(e)),
            )
            .into_any_element()
        }
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    press(cx, "tab");
    (cx, log)
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    cx.run_until_parked();
}

fn heard(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        astryx_primitives::announce::announcement(Politeness::Polite, window, cx).to_string()
    })
}

#[gpui::test]
fn fr3_a_draft_commits_as_soon_as_it_parses_in_range(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f.hour_format(HourFormat::H24));
    type_text(cx, "14:30");
    assert_eq!(log.changes.borrow().last(), Some(&Some(t(14, 30))));
}

#[gpui::test]
fn fr3_the_locale_s_day_periods_read(cx: &mut TestAppContext) {
    cx.update(astryx_primitives::editing::init);
    let log: Rc<RefCell<Vec<Option<PlainTime>>>> = Rc::default();
    let changes = log.clone();
    let cx = host(cx, move || {
        let changes = changes.clone();
        provider(
            I18n::new("ko-KR"),
            TimeInput::new("ti", "Start", None)
                .on_change(move |v, _, _| changes.borrow_mut().push(v)),
        )
        .into_any_element()
    });
    cx.update(|window, _| window.activate_window());
    press(cx, "tab");
    type_text(cx, "오후 2:30");
    assert_eq!(log.borrow().last(), Some(&Some(t(14, 30))));
}

#[gpui::test]
fn fr4_arrows_step_by_the_increment_and_announce(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(t(9, 30)), |f| {
        f.increment(15).hour_format(HourFormat::H12)
    });
    press(cx, "up");
    assert_eq!(log.changes.borrow().last(), Some(&Some(t(9, 45))));
    assert_eq!(heard(cx), "9:45 AM");
    press(cx, "down");
    press(cx, "down");
    assert_eq!(log.changes.borrow().last(), Some(&Some(t(9, 15))));
}

#[gpui::test]
fn fr4_a_step_out_of_range_does_nothing_and_empty_steps_from_now(cx: &mut TestAppContext) {
    set_time_now(Some(t(12, 0)));
    let (cx, log) = field(cx, None, |f| f.max(t(12, 0)));
    press(cx, "up");
    assert!(log.changes.borrow().is_empty(), "12:01 is past max");
    press(cx, "down");
    assert_eq!(log.changes.borrow().last(), Some(&Some(t(11, 59))));
    set_time_now(None);
}

#[gpui::test]
fn fr14_a_complete_out_of_range_time_rejects_at_once(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| {
        f.min(t(9, 0)).max(t(17, 0)).hour_format(HourFormat::H12)
    });
    type_text(cx, "8:00 PM");
    cx.run_until_parked();
    let error = log.errors.borrow().last().cloned().flatten().unwrap();
    assert_eq!(error.kind, TimeEntryErrorKind::OutOfRange);
    assert_eq!(error.value, Some(t(20, 0)));
    assert!(
        log.changes
            .borrow()
            .iter()
            .all(|c| c.is_none_or(|t| t.hour < 20))
    );
}

#[gpui::test]
fn fr14_blur_keeps_an_unreadable_draft(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(t(10, 0)), |f| f);
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(astryx_primitives::editing::SelectAll), cx)
    });
    type_text(cx, "lunch");
    assert!(
        log.errors.borrow().iter().all(Option::is_none),
        "nothing while typing"
    );
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    assert_eq!(
        log.errors
            .borrow()
            .last()
            .cloned()
            .flatten()
            .map(|e| e.kind),
        Some(TimeEntryErrorKind::Unreadable)
    );
}

#[gpui::test]
fn fr13_blur_or_enter_waits_for_the_commit(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| {
        f.min(t(9, 0))
            .entry_error_behavior(EntryErrorBehavior::BlurOrEnter)
    });
    type_text(cx, "08:00");
    cx.run_until_parked();
    assert!(log.errors.borrow().iter().all(Option::is_none));
    press(cx, "enter");
    cx.run_until_parked();
    assert_eq!(
        log.errors
            .borrow()
            .last()
            .cloned()
            .flatten()
            .map(|e| e.kind),
        Some(TimeEntryErrorKind::OutOfRange)
    );
}

#[gpui::test]
fn fr8_clear_commits_none(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(t(10, 0)), |f| f.clear(true));
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.changes.borrow(), [None]);
}

#[gpui::test]
fn fr9_a_disabled_field_takes_no_typing_or_steps(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(t(10, 0)), |f| f.disabled(true));
    type_text(cx, "11:00");
    press(cx, "up");
    press(cx, "enter");
    assert!(log.changes.borrow().is_empty());
}

#[gpui::test]
fn fr2_a_change_action_keeps_the_field_busy(cx: &mut TestAppContext) {
    cx.update(astryx_primitives::editing::init);
    let log: Rc<RefCell<Vec<Option<PlainTime>>>> = Rc::default();
    let changes = log.clone();
    let cx = host(cx, move || {
        let changes = changes.clone();
        TimeInput::new("ti", "Start", Some(t(10, 0)))
            .on_change(move |v, _, _| changes.borrow_mut().push(v))
            .change_action(move |_, window, cx| {
                let wait = cx.background_executor().timer(Duration::from_secs(1));
                window.spawn(cx, async move |_| wait.await)
            })
            .into_any_element()
    });
    cx.update(|window, _| window.activate_window());
    press(cx, "tab");
    press(cx, "up");
    assert_eq!(log.borrow().len(), 1);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
}

#[gpui::test]
fn fr5_the_locale_s_hour_cycle_shows(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    type_text(cx, "14:30");
    assert_eq!(log.changes.borrow().last(), Some(&Some(t(14, 30))));
    press(cx, "tab");
    cx.run_until_parked();
    // en-US is twelve-hour: the committed time reads back as "2:30 PM".
    assert_eq!(
        astryx_i18n::date::format_locale_time(t(14, 30), "en", HourFormat::H12, false),
        "2:30 PM"
    );
}

// ---- The bottom-sheet surface (FR7) ----------------------------------------

use crate::date_input::InputPresentation;
use gpui::{Modifiers, ScrollDelta, ScrollEvent, TouchEvent, TouchId, TouchPhase, point, px};

fn sheet(
    cx: &mut TestAppContext,
    initial: Option<PlainTime>,
    configure: impl Fn(TimeInput) -> TimeInput + 'static,
) -> (&mut VisualTestContext, Rc<Log>) {
    field(cx, initial, move |f| {
        configure(f.presentation(InputPresentation::BottomSheet))
    })
}

fn bounds_of(cx: &mut VisualTestContext, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
}

fn wheel_shows(cx: &mut VisualTestContext, wheel: &str) -> bool {
    bounds_of(cx, &format!("ti-{wheel}")).is_some()
}

/// A click on a wheel's band: it takes focus and keeps its row.
fn focus_wheel(cx: &mut VisualTestContext, wheel: &str) {
    let at = bounds_of(cx, &format!("ti-{wheel}")).unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

/// A finger's tap somewhere blank: the primary pointer turns coarse.
fn touch_somewhere(cx: &mut VisualTestContext) {
    for phase in [TouchPhase::Started, TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(9),
            phase,
            position: point(px(700.), px(20.)),
            ..Default::default()
        });
    }
    cx.run_until_parked();
}

fn touch(cx: &mut VisualTestContext, phase: TouchPhase, x: f32, y: f32) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: point(px(x), px(y)),
        ..Default::default()
    });
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

/// Opens the sheet and lets it finish rising: until then its hit areas are
/// below the window.
fn open(cx: &mut VisualTestContext) {
    press(cx, "enter");
    settle(cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx);
}

#[gpui::test]
fn fr7_the_sheet_opens_from_the_keyboard_and_escape_closes_it(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(t(6, 45)), |f| f);
    assert!(!wheel_shows(cx, "minute"));
    open(cx);
    assert!(wheel_shows(cx, "hour") && wheel_shows(cx, "minute") && wheel_shows(cx, "meridiem"));
    assert!(!wheel_shows(cx, "second"));
    press(cx, "escape");
    cx.run_until_parked();
    assert!(!wheel_shows(cx, "minute"));
    assert!(
        log.changes.borrow().is_empty(),
        "opening and closing change nothing"
    );
}

#[gpui::test]
fn fr7_under_a_mouse_each_wheel_turns_its_own_part(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(t(6, 45)), |f| f);
    open(cx);
    focus_wheel(cx, "minute");
    assert!(
        log.changes.borrow().is_empty(),
        "a click on the band's row keeps it"
    );
    press(cx, "down");
    assert_eq!(log.changes.borrow().last(), Some(&Some(t(6, 46))));
    press(cx, "pageup");
    assert_eq!(log.changes.borrow().last(), Some(&Some(t(6, 36))));
    focus_wheel(cx, "meridiem");
    press(cx, "down");
    assert_eq!(log.changes.borrow().last(), Some(&Some(t(18, 36))));
}

#[gpui::test]
fn fr7_a_wheel_never_commits_past_a_bound(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(t(11, 20)), |f| f.min(t(5, 0)).max(t(11, 30)));
    open(cx);
    focus_wheel(cx, "minute");
    press(cx, "pagedown");
    assert_eq!(log.changes.borrow().last(), Some(&Some(t(11, 30))));
    press(cx, "pagedown");
    press(cx, "down");
    assert_eq!(
        log.changes.borrow().len(),
        1,
        "nothing enabled lies further"
    );
    focus_wheel(cx, "hour");
    press(cx, "down");
    assert_eq!(log.changes.borrow().len(), 1, "12 PM is past the bound");
}

#[gpui::test]
fn fr7_under_a_finger_the_meridiem_follows_the_hour(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(t(11, 30)), |f| f);
    touch_somewhere(cx);
    press(cx, "tab");
    press(cx, "shift-tab");
    open(cx);
    let hour = bounds_of(cx, "ti-wheels-hour").expect("the finger's wheels");
    touch(
        cx,
        TouchPhase::Started,
        f32::from(hour.center().x),
        f32::from(hour.center().y),
    );
    touch(
        cx,
        TouchPhase::Ended,
        f32::from(hour.center().x),
        f32::from(hour.center().y),
    );
    cx.run_until_parked();
    press(cx, "down");
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(t(12, 30))),
        "11 AM → 12 PM"
    );
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    press(cx, "down");
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(t(13, 30))),
        "12 PM → 1 PM"
    );
}

#[gpui::test]
fn fr15_a_notch_turns_one_row_under_a_finger(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(t(6, 45)), |f| f);
    touch_somewhere(cx);
    press(cx, "tab");
    press(cx, "shift-tab");
    open(cx);
    let minute = bounds_of(cx, "ti-wheels-minute").expect("the finger's wheels");
    cx.simulate_event(ScrollEvent {
        position: minute.center(),
        delta: ScrollDelta::Lines(point(0., -1.)),
        ..Default::default()
    });
    cx.run_until_parked();
    assert_eq!(log.changes.borrow().last(), Some(&Some(t(6, 46))));
}

#[gpui::test]
fn fr13_a_fling_coasts_and_commits_the_row_it_rests_on(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(t(6, 45)), |f| f);
    touch_somewhere(cx);
    press(cx, "tab");
    press(cx, "shift-tab");
    open(cx);
    let minute = bounds_of(cx, "ti-wheels-minute").expect("the finger's wheels");
    let (x, y) = (f32::from(minute.center().x), f32::from(minute.center().y));
    touch(cx, TouchPhase::Started, x, y);
    for step in 1..=6 {
        cx.executor().advance_clock(Duration::from_millis(16));
        touch(cx, TouchPhase::Moved, x, y - 20. * step as f32);
    }
    touch(cx, TouchPhase::Ended, x, y - 120.);
    cx.run_until_parked();
    assert!(
        log.changes.borrow().is_empty(),
        "nothing commits while it coasts"
    );
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    let rested = log
        .changes
        .borrow()
        .last()
        .cloned()
        .flatten()
        .expect("a rest commits");
    // The minutes loop: passing :59 comes to :00 and keeps the hour.
    let travelled = (i64::from(rested.minute) - 45).rem_euclid(60);
    assert!(rested.hour == 6 && travelled > 4, "{rested:?}");
}

#[gpui::test]
fn fr9_a_disabled_sheet_field_never_opens(cx: &mut TestAppContext) {
    let (cx, _) = sheet(cx, Some(t(6, 45)), |f| f.disabled(true));
    open(cx);
    assert!(!wheel_shows(cx, "minute"));
}

#[gpui::test]
fn fr8_the_sheet_field_clears(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(t(6, 45)), |f| f.clear(true));
    press(cx, "tab");
    press(cx, "enter");
    cx.run_until_parked();
    assert_eq!(*log.changes.borrow(), [None]);
}

#[gpui::test]
fn fr13_a_drag_resting_on_a_disabled_row_travels_to_the_nearest_enabled_one(
    cx: &mut TestAppContext,
) {
    let (cx, log) = sheet(cx, Some(t(6, 45)), |f| f.min(t(5, 0)));
    open(cx);
    let hour = bounds_of(cx, "ti-hour").unwrap().center();
    // Three rows down: the column rests on 3 AM, before the minimum.
    cx.simulate_mouse_down(hour, gpui::MouseButton::Left, Modifiers::none());
    settle(cx);
    let below = point(hour.x, hour.y + px(84.));
    cx.simulate_mouse_move(below, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(below, gpui::MouseButton::Left, Modifiers::none());
    settle(cx);
    cx.executor().advance_clock(Duration::from_secs(2));
    settle(cx);
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(t(5, 45))),
        "on to 5 AM"
    );
}

#[gpui::test]
fn fr14_a_wheel_another_turned_turns_onto_its_row(cx: &mut TestAppContext) {
    let (cx, _) = sheet(cx, Some(t(11, 30)), |f| f);
    touch_somewhere(cx);
    press(cx, "tab");
    press(cx, "shift-tab");
    open(cx);
    let hour = bounds_of(cx, "ti-wheels-hour").unwrap().center();
    touch(
        cx,
        TouchPhase::Started,
        f32::from(hour.x),
        f32::from(hour.y),
    );
    touch(cx, TouchPhase::Ended, f32::from(hour.x), f32::from(hour.y));
    settle(cx);
    let band = bounds_of(cx, "ti-wheels-meridiem").unwrap().center().y;
    press(cx, "down");
    cx.executor().advance_clock(Duration::from_millis(60));
    settle(cx);
    let pm = bounds_of(cx, "ti-wheels-meridiem-option-1")
        .unwrap()
        .center()
        .y;
    assert!(pm > band + px(2.), "PM is still on its way to the band");
    cx.executor().advance_clock(Duration::from_secs(2));
    settle(cx);
    let pm = bounds_of(cx, "ti-wheels-meridiem-option-1")
        .unwrap()
        .center()
        .y;
    assert!((pm - band).abs() < px(0.5), "then rests under it");
}

// ---- The native surface (R16) ------------------------------------------------

fn alerted(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        astryx_primitives::announce::announcement(Politeness::Assertive, window, cx).to_string()
    })
}

fn pick(cx: &mut VisualTestContext, outcome: gpui::NativePickerOutcome) {
    press(cx, "space");
    cx.run_until_parked();
    cx.simulate_native_picker(outcome);
    cx.run_until_parked();
}

#[gpui::test]
fn fr2_a_forced_native_opens_even_with_seconds(cx: &mut TestAppContext) {
    cx.set_native_pickers_supported(true);
    let (cx, _) = field(cx, Some(t(6, 45)), |f| {
        f.presentation(InputPresentation::Native).seconds(true)
    });
    press(cx, "space");
    cx.run_until_parked();
    let request = cx.native_picker_request().expect("forced native opens");
    // The picker steps whole minutes, as upstream's `step=60`.
    assert!(!request.seconds);
    assert_eq!(request.value.as_deref(), Some("06:45"));
}

#[gpui::test]
fn fr2_the_deprecated_always_keeps_the_typed_field_with_seconds(cx: &mut TestAppContext) {
    cx.set_native_pickers_supported(true);
    let (cx, log) = field(cx, None, |f| {
        f.native_picker(crate::date_input::LegacyNativePicker::Always)
            .seconds(true)
    });
    press(cx, "space");
    cx.run_until_parked();
    assert!(cx.native_picker_request().is_none());
    type_text(cx, "9:30:15 AM");
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(PlainTime::new(9, 30, 15).unwrap()))
    );
}

#[gpui::test]
fn fr6_a_native_time_commits_within_its_bounds(cx: &mut TestAppContext) {
    cx.set_native_pickers_supported(true);
    let (cx, log) = field(cx, Some(t(9, 0)), |f| {
        f.presentation(InputPresentation::Native)
            .min(t(8, 0))
            .max(t(17, 0))
    });
    pick(cx, gpui::NativePickerOutcome::Picked("10:30".into()));
    assert_eq!(*log.changes.borrow(), [Some(t(10, 30))]);
    // Under `Complete` (the default) a time past max shows at once.
    pick(cx, gpui::NativePickerOutcome::Picked("18:00".into()));
    assert_eq!(log.changes.borrow().len(), 1, "past max is refused");
    assert_ne!(alerted(cx), "");
}

#[gpui::test]
fn fr13_under_blur_or_enter_a_native_rejection_waits(cx: &mut TestAppContext) {
    cx.set_native_pickers_supported(true);
    let (cx, log) = field(cx, Some(t(9, 0)), |f| {
        f.presentation(InputPresentation::Native)
            .max(t(17, 0))
            .entry_error_behavior(EntryErrorBehavior::BlurOrEnter)
    });
    pick(cx, gpui::NativePickerOutcome::Picked("18:00".into()));
    assert!(log.changes.borrow().is_empty());
    assert_eq!(alerted(cx), "");
    press(cx, "enter");
    assert_ne!(alerted(cx), "", "Enter shows it");
}
