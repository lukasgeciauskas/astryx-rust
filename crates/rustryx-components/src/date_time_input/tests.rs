//! DateTimeInput tests, numbered after `DateTimeInput.spec.md`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{IntoElement, TestAppContext, VisualTestContext};
use rustryx_i18n::date::{
    HourFormat, PlainDate, PlainDateTime, PlainTime, set_time_now, set_today,
};

use super::*;
use crate::date_input::DateEntryErrorKind;
use crate::segmented::tests::{host, press};

fn d(y: i32, m: u8, day: u8) -> PlainDate {
    PlainDate::new(y, m, day).unwrap()
}

fn t(h: u8, m: u8) -> PlainTime {
    PlainTime::new(h, m, 0).unwrap()
}

fn at(date: PlainDate, time: PlainTime) -> PlainDateTime {
    PlainDateTime::new(date, time)
}

#[derive(Default)]
struct Log {
    changes: RefCell<Vec<Option<PlainDateTime>>>,
    errors: RefCell<Vec<Option<crate::date_input::DateInputEntryError>>>,
}

fn field(
    cx: &mut TestAppContext,
    initial: Option<PlainDateTime>,
    configure: impl Fn(DateTimeInput) -> DateTimeInput + 'static,
) -> (&mut VisualTestContext, Rc<Log>) {
    set_today(Some(d(2026, 11, 25)));
    set_time_now(Some(t(10, 15)));
    cx.update(rustryx_primitives::editing::init);
    let log = Rc::new(Log::default());
    let value = Rc::new(RefCell::new(initial));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let (log, errors, value) = (log.clone(), log.clone(), value.clone());
            let current = *value.borrow();
            configure(
                DateTimeInput::new("dti", "Meeting", current)
                    .hour_format(HourFormat::H24)
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
    (cx, log)
}

/// Toggle, then the date text.
fn focus_date(cx: &mut VisualTestContext) {
    press(cx, "tab");
    press(cx, "tab");
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    cx.run_until_parked();
}

fn option_shows(cx: &mut VisualTestContext, time: PlainTime) -> bool {
    let selector: &'static str = Box::leak(format!("dti-time-option-{time}").into_boxed_str());
    cx.debug_bounds(selector).is_some()
}

#[gpui::test]
fn fr2_a_typed_date_takes_the_current_time(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    focus_date(cx);
    type_text(cx, "11/20/2026");
    assert_eq!(
        *log.changes.borrow(),
        [Some(at(d(2026, 11, 20), t(10, 15)))]
    );
}

#[gpui::test]
fn fr2_a_time_before_any_date_emits_nothing(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    focus_date(cx);
    press(cx, "tab");
    type_text(cx, "14:30");
    press(cx, "enter");
    assert!(log.changes.borrow().is_empty());
}

#[gpui::test]
fn fr2_a_new_date_keeps_the_time_and_a_typed_time_keeps_the_date(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(at(d(2026, 11, 18), t(14, 30))), |f| f);
    focus_date(cx);
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(rustryx_primitives::editing::SelectAll), cx)
    });
    type_text(cx, "11/19/2026");
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(at(d(2026, 11, 19), t(14, 30))))
    );
    press(cx, "tab");
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(rustryx_primitives::editing::SelectAll), cx)
    });
    type_text(cx, "09:05");
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(at(d(2026, 11, 19), t(9, 5))))
    );
}

#[gpui::test]
fn fr6_a_date_on_the_min_day_clamps_its_time(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f.min(at(d(2026, 11, 20), t(13, 0))));
    focus_date(cx);
    type_text(cx, "11/20/2026");
    assert_eq!(
        *log.changes.borrow(),
        [Some(at(d(2026, 11, 20), t(13, 0)))],
        "10:15 is before min"
    );
}

#[gpui::test]
fn fr7_the_preset_list_opens_moves_and_commits(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(at(d(2026, 11, 18), t(14, 0))), |f| {
        f.time_option_interval(TimeOptionInterval::Thirty)
    });
    focus_date(cx);
    press(cx, "tab");
    assert!(!option_shows(cx, t(14, 0)));
    press(cx, "alt-down");
    cx.run_until_parked();
    assert!(option_shows(cx, t(14, 0)), "opens on the value");
    press(cx, "down");
    press(cx, "enter");
    cx.run_until_parked();
    assert_eq!(
        *log.changes.borrow(),
        [Some(at(d(2026, 11, 18), t(14, 30)))]
    );
    assert!(!option_shows(cx, t(14, 30)), "the list closed");
}

#[gpui::test]
fn fr7_the_list_offers_only_times_within_the_day_s_bounds(cx: &mut TestAppContext) {
    let (cx, _) = field(cx, Some(at(d(2026, 11, 20), t(13, 0))), |f| {
        f.time_option_interval(TimeOptionInterval::Sixty)
            .min(at(d(2026, 11, 20), t(12, 30)))
    });
    focus_date(cx);
    press(cx, "tab");
    press(cx, "alt-down");
    cx.run_until_parked();
    assert!(option_shows(cx, t(13, 0)));
    assert!(!option_shows(cx, t(12, 0)), "before min on the min day");
}

#[gpui::test]
fn fr7_arrows_step_the_time_when_the_list_is_closed(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(at(d(2026, 11, 18), t(14, 0))), |f| {
        f.time_increment(15)
    });
    focus_date(cx);
    press(cx, "tab");
    press(cx, "up");
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(at(d(2026, 11, 18), t(14, 15))))
    );
}

#[gpui::test]
fn fr11_the_date_segment_reports_its_rejections(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    focus_date(cx);
    type_text(cx, "2/30/2026");
    cx.run_until_parked();
    let error = log.errors.borrow().last().cloned().flatten().unwrap();
    assert_eq!(error.kind, DateEntryErrorKind::Nonexistent);
}

#[gpui::test]
fn fr2_clear_emits_none(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(at(d(2026, 11, 18), t(14, 0))), |f| f.clear(true));
    focus_date(cx);
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.changes.borrow(), [None]);
}

#[gpui::test]
fn fr9_a_disabled_field_changes_nothing(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(at(d(2026, 11, 18), t(14, 0))), |f| {
        f.disabled(true)
    });
    focus_date(cx);
    type_text(cx, "11/19/2026");
    press(cx, "tab");
    press(cx, "up");
    assert!(log.changes.borrow().is_empty());
}

// ---- The bottom-sheet surface ----------------------------------------------

use crate::date_input::PickerPresentation;
use gpui::{Modifiers, TouchEvent, TouchId, point, px};
use std::time::Duration;

fn sheet(
    cx: &mut TestAppContext,
    initial: Option<PlainDateTime>,
    configure: impl Fn(DateTimeInput) -> DateTimeInput + 'static,
) -> (&mut VisualTestContext, Rc<Log>) {
    field(cx, initial, move |f| {
        configure(f.presentation(PickerPresentation::BottomSheet))
    })
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

fn bounds_of(cx: &mut VisualTestContext, selector: String) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
}

fn tap(cx: &mut VisualTestContext, selector: String) {
    let at = bounds_of(cx, selector.clone())
        .unwrap_or_else(|| panic!("nothing at {selector}"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn day(date: PlainDate) -> String {
    crate::date_input::month_scroller::day_selector(&"dti-months".into(), date)
}

/// Opens the sheet from a segment (1: the date, 2: the time) and lets it rise.
fn open_from(cx: &mut VisualTestContext, tabs: usize) {
    for _ in 0..tabs {
        press(cx, "tab");
    }
    press(cx, "enter");
    settle(cx);
    wait(cx, 1000);
}

#[gpui::test]
fn fr2_a_tapped_day_keeps_the_time_and_the_sheet_stays_open(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(at(d(2026, 12, 2), t(9, 15))), |f| f);
    open_from(cx, 1);
    tap(cx, day(d(2026, 12, 10)));
    assert_eq!(*log.changes.borrow(), [Some(at(d(2026, 12, 10), t(9, 15)))]);
    assert!(bounds_of(cx, day(d(2026, 12, 11))).is_some(), "still open");
}

#[gpui::test]
fn fr6_a_day_on_the_minimum_s_date_clamps_its_time(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(at(d(2026, 12, 2), t(7, 30))), |f| {
        f.min(at(d(2026, 11, 26), t(8, 0)))
    });
    open_from(cx, 1);
    tap(cx, "dti-prev-month".into());
    wait(cx, 1000);
    tap(cx, day(d(2026, 11, 26)));
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(at(d(2026, 11, 26), t(8, 0))))
    );
}

#[gpui::test]
fn fr2_a_time_before_any_date_waits_for_one(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, None, |f| f);
    open_from(cx, 2);
    tap(cx, "dti-time-minute".into());
    press(cx, "down");
    assert!(log.changes.borrow().is_empty(), "no date yet");
    // Back to the date, by the switch.
    tap(cx, "dti-mode-date".into());
    tap(cx, day(d(2026, 11, 27)));
    assert_eq!(
        *log.changes.borrow(),
        [Some(at(d(2026, 11, 27), t(10, 16)))],
        "with the draft time"
    );
}

#[gpui::test]
fn save_date_moves_on_to_the_time(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(at(d(2026, 12, 2), t(9, 15))), |f| f);
    open_from(cx, 1);
    tap(cx, "dti-save-date".into());
    // The time panel is the one in view: its wheels take the press.
    tap(cx, "dti-time-minute".into());
    press(cx, "down");
    assert_eq!(*log.changes.borrow(), [Some(at(d(2026, 12, 2), t(9, 16)))]);
}

#[gpui::test]
fn fr14_under_a_finger_the_month_wheel_moves_the_date_keeping_its_day_and_time(
    cx: &mut TestAppContext,
) {
    let (cx, log) = sheet(cx, Some(at(d(2027, 1, 31), t(9, 15))), |f| f);
    for phase in [gpui::TouchPhase::Started, gpui::TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(9),
            phase,
            position: point(px(1500.), px(20.)),
            ..Default::default()
        });
    }
    settle(cx);
    open_from(cx, 1);
    tap(cx, "dti-month-year".into());
    wait(cx, 500);
    let month = bounds_of(cx, "dti-wheels-month".into())
        .expect("the wheels")
        .center();
    for phase in [gpui::TouchPhase::Started, gpui::TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: month,
            ..Default::default()
        });
    }
    settle(cx);
    press(cx, "down");
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(at(d(2027, 2, 28), t(9, 15))))
    );
}

#[gpui::test]
fn fr9_a_loading_sheet_field_never_opens(cx: &mut TestAppContext) {
    let (cx, _) = sheet(cx, Some(at(d(2026, 12, 2), t(9, 15))), |f| f.loading(true));
    open_from(cx, 1);
    assert!(bounds_of(cx, day(d(2026, 12, 10))).is_none());
}

#[gpui::test]
fn the_sheet_field_clears(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(at(d(2026, 12, 2), t(9, 15))), |f| f.clear(true));
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    settle(cx);
    assert_eq!(*log.changes.borrow(), [None]);
}

// ---- The native segments (R16) ----------------------------------------------

fn pick(
    cx: &mut VisualTestContext,
    outcome: gpui::NativePickerOutcome,
) -> gpui::NativePickerRequest {
    press(cx, "space");
    cx.run_until_parked();
    let request = cx
        .native_picker_request()
        .expect("a platform picker opened");
    cx.simulate_native_picker(outcome);
    cx.run_until_parked();
    request
}

#[gpui::test]
fn fr3_native_segments_keep_the_time_and_the_date(cx: &mut TestAppContext) {
    cx.set_native_pickers_supported(true);
    let (cx, log) = field(cx, Some(at(d(2026, 12, 2), t(9, 15))), |f| {
        f.presentation(crate::date_input::PickerPresentation::Native)
    });
    press(cx, "tab");
    let request = pick(cx, gpui::NativePickerOutcome::Picked("2026-12-10".into()));
    assert_eq!(request.kind, gpui::NativePickerKind::Date);
    assert_eq!(*log.changes.borrow(), [Some(at(d(2026, 12, 10), t(9, 15)))]);
    press(cx, "tab");
    let request = pick(cx, gpui::NativePickerOutcome::Picked("14:00".into()));
    assert_eq!(request.kind, gpui::NativePickerKind::Time);
    assert_eq!(request.value.as_deref(), Some("09:15"));
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(at(d(2026, 12, 10), t(14, 0))))
    );
}

#[gpui::test]
fn fr2_a_native_time_before_any_date_waits_for_the_date(cx: &mut TestAppContext) {
    cx.set_native_pickers_supported(true);
    let (cx, log) = field(cx, None, |f| {
        f.presentation(crate::date_input::PickerPresentation::Native)
    });
    press(cx, "tab");
    press(cx, "tab");
    pick(cx, gpui::NativePickerOutcome::Picked("09:30".into()));
    assert!(
        log.changes.borrow().is_empty(),
        "no date-time without a date"
    );
    press(cx, "shift-tab");
    pick(cx, gpui::NativePickerOutcome::Picked("2026-12-10".into()));
    assert_eq!(*log.changes.borrow(), [Some(at(d(2026, 12, 10), t(9, 30)))]);
}

#[gpui::test]
fn fr2_adaptive_native_keeps_the_astryx_time_for_presets(cx: &mut TestAppContext) {
    cx.set_native_pickers_supported(true);
    let (cx, _) = field(cx, None, |f| {
        f.native_picker(crate::date_input::LegacyNativePicker::Always)
            .time_option_interval(TimeOptionInterval::Thirty)
    });
    assert!(cx.debug_bounds("dti-native-date-value").is_some());
    assert!(cx.debug_bounds("dti-native-time-value").is_none());
}
