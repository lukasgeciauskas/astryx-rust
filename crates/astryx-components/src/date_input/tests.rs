//! DateInput tests, numbered after `DateInput.spec.md` (and AST-111, AST-117,
//! AST-118 where they apply).

use std::cell::RefCell;
use std::rc::Rc;

use astryx_i18n::date::{PlainDate, set_today};
use astryx_i18n::{CalendarSystem, I18n, provider};
use std::time::Duration;

use gpui::{IntoElement, ParentElement, TestAppContext, VisualTestContext};

use super::*;
use crate::calendar::day_selector;
use crate::field::{EntryErrorBehavior, FormLayout};
use crate::segmented::tests::{host, press};

fn d(y: i32, m: u8, day: u8) -> PlainDate {
    PlainDate::new(y, m, day).unwrap()
}

#[derive(Default)]
struct Log {
    changes: RefCell<Vec<Option<PlainDate>>>,
    errors: RefCell<Vec<Option<DateInputEntryError>>>,
}

/// A controlled DateInput that keeps the value it reports.
fn field(
    cx: &mut TestAppContext,
    initial: Option<PlainDate>,
    configure: impl Fn(DateInput) -> DateInput + 'static,
) -> (&mut VisualTestContext, Rc<Log>) {
    set_today(Some(d(2026, 11, 25)));
    cx.update(astryx_primitives::editing::init);
    let log = Rc::new(Log::default());
    let value = Rc::new(RefCell::new(initial));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let (log, errors, value) = (log.clone(), log.clone(), value.clone());
            let current = *value.borrow();
            configure(
                DateInput::new("di", "Due", current)
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

/// Tabs past the calendar toggle into the text.
fn focus_input(cx: &mut VisualTestContext) {
    press(cx, "tab");
    press(cx, "tab");
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    cx.run_until_parked();
}

fn calendar_shows(cx: &mut VisualTestContext, date: PlainDate) -> bool {
    let selector: &'static str =
        Box::leak(day_selector(&"di-calendar".into(), date, false).into_boxed_str());
    cx.debug_bounds(selector).is_some()
}

fn last_error(log: &Log) -> Option<DateEntryErrorKind> {
    log.errors
        .borrow()
        .last()
        .cloned()
        .flatten()
        .map(|e| e.kind)
}

#[gpui::test]
fn fr20_a_complete_typed_date_reports_once(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    focus_input(cx);
    type_text(cx, "11/2/202");
    assert!(
        log.changes.borrow().is_empty(),
        "a three-digit year is not a date yet"
    );
    type_text(cx, "6");
    assert_eq!(*log.changes.borrow(), [Some(d(2026, 11, 2))]);
    press(cx, "enter");
    assert_eq!(
        log.changes.borrow().len(),
        1,
        "Enter does not report the same date again"
    );
}

#[gpui::test]
fn fr20_a_two_digit_year_waits_for_enter(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    focus_input(cx);
    type_text(cx, "1/2/45");
    assert!(log.changes.borrow().is_empty());
    press(cx, "enter");
    assert_eq!(*log.changes.borrow(), [Some(d(2045, 1, 2))]);
}

#[gpui::test]
fn fr20_gregorian_text_only_the_fallback_reads_commits_on_enter(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    focus_input(cx);
    type_text(cx, "13/5/2026");
    assert!(log.changes.borrow().is_empty());
    press(cx, "enter");
    assert_eq!(*log.changes.borrow(), [Some(d(2026, 5, 13))]);
}

#[gpui::test]
fn fr25_unreadable_text_stays_and_reports_at_enter(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(d(2026, 3, 1)), |f| f);
    focus_input(cx);
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(astryx_primitives::editing::SelectAll), cx)
    });
    type_text(cx, "next tuesday");
    assert!(
        log.errors.borrow().iter().all(Option::is_none),
        "nothing while typing"
    );
    press(cx, "enter");
    assert_eq!(last_error(&log), Some(DateEntryErrorKind::Unreadable));
    assert!(log.changes.borrow().is_empty(), "the committed value stays");
}

#[gpui::test]
fn fr20_a_date_the_calendar_lacks_rejects_at_once(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    focus_input(cx);
    type_text(cx, "2/30/2026");
    cx.run_until_parked();
    assert_eq!(last_error(&log), Some(DateEntryErrorKind::Nonexistent));
    assert!(
        log.changes.borrow().is_empty(),
        "never adjusted to another date"
    );
}

#[gpui::test]
fn fr5_a_typed_date_out_of_range_is_rejected(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f.max(d(2026, 12, 31)));
    focus_input(cx);
    type_text(cx, "1/5/2027");
    cx.run_until_parked();
    let error = log.errors.borrow().last().cloned().flatten().unwrap();
    assert_eq!(error.kind, DateEntryErrorKind::OutOfRange);
    assert_eq!(error.date, Some(d(2027, 1, 5)));
    assert!(log.changes.borrow().is_empty());
}

#[gpui::test]
fn fr20_a_date_a_rule_rules_out_is_unavailable(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f.date_constraint(|date| date.day != 13));
    focus_input(cx);
    type_text(cx, "3/13/2026");
    cx.run_until_parked();
    let error = log.errors.borrow().last().cloned().flatten().unwrap();
    assert_eq!(error.kind, DateEntryErrorKind::Unavailable);
    assert!(log.changes.borrow().is_empty());
}

#[gpui::test]
fn fr24_blur_or_enter_waits_for_the_commit(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| {
        f.entry_error_behavior(EntryErrorBehavior::BlurOrEnter)
    });
    focus_input(cx);
    type_text(cx, "2/30/2026");
    cx.run_until_parked();
    assert!(
        log.errors.borrow().iter().all(Option::is_none),
        "no rejection while typing"
    );
    press(cx, "enter");
    assert_eq!(last_error(&log), Some(DateEntryErrorKind::Nonexistent));
}

#[gpui::test]
fn fr24_a_form_layout_sets_the_default(cx: &mut TestAppContext) {
    set_today(Some(d(2026, 11, 25)));
    cx.update(astryx_primitives::editing::init);
    let errors: Rc<RefCell<Vec<Option<DateInputEntryError>>>> = Rc::default();
    let log = errors.clone();
    let cx = host(cx, move || {
        let log = log.clone();
        FormLayout::new("form")
            .entry_error_behavior(EntryErrorBehavior::BlurOrEnter)
            .child(
                DateInput::new("di", "Due", None)
                    .on_entry_error_change(move |e, _, _| log.borrow_mut().push(e)),
            )
            .into_any_element()
    });
    cx.update(|window, _| window.activate_window());
    focus_input(cx);
    type_text(cx, "2/30/2026");
    cx.run_until_parked();
    assert!(errors.borrow().iter().all(Option::is_none));
    press(cx, "enter");
    assert!(errors.borrow().last().cloned().flatten().is_some());
}

#[gpui::test]
fn fr25_a_new_edit_that_reads_as_a_date_clears_the_rejection(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    focus_input(cx);
    type_text(cx, "2/30/2026");
    cx.run_until_parked();
    assert_eq!(last_error(&log), Some(DateEntryErrorKind::Nonexistent));
    press(cx, "backspace");
    press(cx, "backspace");
    press(cx, "backspace");
    press(cx, "backspace");
    press(cx, "backspace");
    press(cx, "backspace");
    press(cx, "backspace");
    type_text(cx, "28/2026");
    cx.run_until_parked();
    assert_eq!(log.errors.borrow().last().cloned().flatten(), None);
    assert_eq!(*log.changes.borrow(), [Some(d(2026, 2, 28))]);
}

#[gpui::test]
fn fr8_arrow_down_opens_the_calendar_and_escape_closes_it(cx: &mut TestAppContext) {
    let (cx, _) = field(cx, Some(d(2026, 11, 18)), |f| f);
    focus_input(cx);
    assert!(!calendar_shows(cx, d(2026, 11, 18)));
    press(cx, "down");
    cx.run_until_parked();
    assert!(
        calendar_shows(cx, d(2026, 11, 18)),
        "the calendar opens on the value"
    );
    press(cx, "escape");
    cx.run_until_parked();
    assert!(!calendar_shows(cx, d(2026, 11, 18)));
}

#[gpui::test]
fn fr9_a_calendar_pick_commits_and_closes(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(d(2026, 11, 18)), |f| f);
    focus_input(cx);
    press(cx, "down");
    cx.run_until_parked();
    let selector: &'static str =
        Box::leak(day_selector(&"di-calendar".into(), d(2026, 11, 20), false).into_boxed_str());
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*log.changes.borrow(), [Some(d(2026, 11, 20))]);
    assert!(!calendar_shows(cx, d(2026, 11, 20)), "the popover closed");
}

#[gpui::test]
fn fr20_typing_shows_the_typed_month_in_the_open_calendar(cx: &mut TestAppContext) {
    let (cx, _) = field(cx, None, |f| f);
    focus_input(cx);
    press(cx, "down");
    cx.run_until_parked();
    type_text(cx, "3/14/2027");
    cx.run_until_parked();
    assert!(calendar_shows(cx, d(2027, 3, 14)));
}

#[gpui::test]
fn fr11_clear_emits_none(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(d(2026, 11, 18)), |f| f.clear(true));
    // Toggle, input, clear.
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.changes.borrow(), [None]);
}

#[gpui::test]
fn fr12_a_disabled_field_takes_no_typing_and_does_not_open(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f.disabled(true));
    focus_input(cx);
    type_text(cx, "11/2/2026");
    press(cx, "enter");
    press(cx, "down");
    cx.run_until_parked();
    assert!(log.changes.borrow().is_empty());
    assert!(!calendar_shows(cx, d(2026, 11, 25)));
}

#[gpui::test]
fn fr7_a_change_action_keeps_the_field_busy(cx: &mut TestAppContext) {
    set_today(Some(d(2026, 11, 25)));
    cx.update(astryx_primitives::editing::init);
    let log: Rc<RefCell<Vec<Option<PlainDate>>>> = Rc::default();
    let changes = log.clone();
    let cx = host(cx, move || {
        let changes = changes.clone();
        DateInput::new("di", "Due", None)
            .on_change(move |v, _, _| changes.borrow_mut().push(v))
            .change_action(move |_, window, cx| {
                let wait = cx.background_executor().timer(Duration::from_secs(1));
                window.spawn(cx, async move |_| wait.await)
            })
            .into_any_element()
    });
    cx.update(|window, _| window.activate_window());
    focus_input(cx);
    type_text(cx, "11/2/2026");
    assert_eq!(*log.borrow(), [Some(d(2026, 11, 2))]);
    // Busy: another complete date is not reported.
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(astryx_primitives::editing::SelectAll), cx)
    });
    type_text(cx, "11/3/2026");
    assert_eq!(log.borrow().len(), 1);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
}

#[gpui::test]
fn ast111_fr9_a_persian_field_reads_persian_dates(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f.calendar(CalendarSystem::Persian));
    focus_input(cx);
    type_text(cx, "8/11/1405");
    assert_eq!(*log.changes.borrow(), [Some(d(2026, 11, 2))]);
}

#[gpui::test]
fn ast111_fr2_the_provider_calendar_applies(cx: &mut TestAppContext) {
    set_today(Some(d(2026, 11, 25)));
    cx.update(astryx_primitives::editing::init);
    let log: Rc<RefCell<Vec<Option<PlainDate>>>> = Rc::default();
    let changes = log.clone();
    let cx = host(cx, move || {
        let changes = changes.clone();
        provider(
            I18n::new("en").calendar(CalendarSystem::Buddhist),
            DateInput::new("di", "Due", None)
                .on_change(move |v, _, _| changes.borrow_mut().push(v)),
        )
        .into_any_element()
    });
    cx.update(|window, _| window.activate_window());
    focus_input(cx);
    type_text(cx, "11/2/2569");
    assert_eq!(*log.borrow(), [Some(d(2026, 11, 2))], "Buddhist year 2569");
}

// ---- The bottom-sheet surface ----------------------------------------------

use crate::date_input::PickerPresentation;
use gpui::{
    Modifiers, ScrollDelta, ScrollEvent, ScrollSource, TouchEvent, TouchId, TouchPhase, point, px,
};

fn sheet(
    cx: &mut TestAppContext,
    initial: Option<PlainDate>,
    configure: impl Fn(DateInput) -> DateInput + 'static,
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

fn sheet_day(date: PlainDate) -> String {
    super::month_scroller::day_selector(&"di-months".into(), date)
}

/// Whether `date`'s day sits inside the scrollport: its month is shown.
fn shows_month_of(cx: &mut VisualTestContext, date: PlainDate) -> bool {
    let (Some(port), Some(day)) = (
        bounds_of(cx, "di-months".into()),
        bounds_of(cx, sheet_day(date)),
    ) else {
        return false;
    };
    day.left() >= port.left() - px(0.5) && day.right() <= port.right() + px(0.5)
}

fn tap(cx: &mut VisualTestContext, selector: String) {
    let at = bounds_of(cx, selector.clone())
        .unwrap_or_else(|| panic!("nothing at {selector}"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

/// Tab to the field and open its sheet from the keyboard; it rises.
fn open_sheet(cx: &mut VisualTestContext) {
    press(cx, "tab");
    press(cx, "down");
    settle(cx);
    wait(cx, 1000);
}

fn touch_somewhere(cx: &mut VisualTestContext) {
    for phase in [TouchPhase::Started, TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(9),
            phase,
            position: point(px(1500.), px(20.)),
            ..Default::default()
        });
    }
    settle(cx);
}

#[gpui::test]
fn fr8_the_sheet_opens_from_the_keyboard_on_the_value_s_month(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(d(2026, 12, 4)), |f| f);
    assert!(bounds_of(cx, sheet_day(d(2026, 12, 4))).is_none());
    open_sheet(cx);
    assert!(shows_month_of(cx, d(2026, 12, 4)));
    assert!(log.changes.borrow().is_empty());
}

#[gpui::test]
fn fr9_a_tapped_day_commits_and_the_sheet_stays_open(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(d(2026, 11, 18)), |f| f);
    open_sheet(cx);
    tap(cx, sheet_day(d(2026, 11, 20)));
    assert_eq!(*log.changes.borrow(), [Some(d(2026, 11, 20))]);
    assert!(shows_month_of(cx, d(2026, 11, 20)), "still open");
    tap(cx, sheet_day(d(2026, 11, 21)));
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(d(2026, 11, 21))),
        "a second tap corrects"
    );
    tap(cx, "di-save".into());
    wait(cx, 1000);
    assert!(
        bounds_of(cx, sheet_day(d(2026, 11, 21))).is_none(),
        "Save closes"
    );
    assert_eq!(log.changes.borrow().len(), 2, "and commits nothing");
}

#[gpui::test]
fn fr5_a_day_outside_the_bounds_refuses_to_commit(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(d(2026, 11, 18)), |f| {
        f.min(d(2026, 11, 10))
            .date_constraint(|date| date != d(2026, 11, 20))
    });
    open_sheet(cx);
    tap(cx, sheet_day(d(2026, 11, 5)));
    tap(cx, sheet_day(d(2026, 11, 20)));
    assert!(log.changes.borrow().is_empty());
}

#[gpui::test]
fn fr22_an_arrow_at_the_range_end_hands_its_focus_to_the_other(cx: &mut TestAppContext) {
    let (cx, _) = sheet(cx, Some(d(2026, 11, 20)), |f| {
        f.min(d(2026, 11, 1)).max(d(2026, 12, 31))
    });
    open_sheet(cx);
    // The previous arrow is hidden at the first month: Tab reaches the next.
    tap(cx, "di-month-year".into());
    tap(cx, "di-month-year".into());
    press(cx, "tab");
    press(cx, "enter");
    wait(cx, 1000);
    assert!(shows_month_of(cx, d(2026, 12, 4)), "on to December");
    // The next arrow hid; Enter now presses the previous one.
    press(cx, "enter");
    wait(cx, 1000);
    assert!(shows_month_of(cx, d(2026, 11, 4)), "back to November");
}

#[gpui::test]
fn swiping_pages_to_the_next_month(cx: &mut TestAppContext) {
    let (cx, _) = sheet(cx, Some(d(2026, 11, 18)), |f| f);
    open_sheet(cx);
    let port = bounds_of(cx, "di-months".into()).unwrap();
    cx.simulate_event(ScrollEvent {
        position: port.center(),
        delta: ScrollDelta::Pixels(point(-port.size.width * 0.7, px(0.))),
        source: ScrollSource::Touch,
        touch_phase: TouchPhase::Ended,
        ..Default::default()
    });
    settle(cx);
    wait(cx, 1000);
    assert!(shows_month_of(cx, d(2026, 12, 10)));
}

#[gpui::test]
fn fr23_under_a_finger_the_month_wheel_moves_the_date_keeping_its_day(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(d(2026, 1, 31)), |f| f);
    touch_somewhere(cx);
    open_sheet(cx);
    tap(cx, "di-month-year".into());
    wait(cx, 500);
    let month = bounds_of(cx, "di-wheels-month".into())
        .expect("the wheels")
        .center();
    for phase in [TouchPhase::Started, TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: month,
            ..Default::default()
        });
    }
    settle(cx);
    press(cx, "down");
    assert_eq!(log.changes.borrow().last(), Some(&Some(d(2026, 2, 28))));
}

#[gpui::test]
fn fr23_with_a_mouse_the_wheels_move_only_the_calendar(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(d(2026, 1, 31)), |f| f);
    open_sheet(cx);
    tap(cx, "di-month-year".into());
    wait(cx, 500);
    tap(cx, "di-wheels-month".into());
    press(cx, "down");
    wait(cx, 500);
    assert!(log.changes.borrow().is_empty());
    tap(cx, "di-done".into());
    wait(cx, 500);
    assert!(
        shows_month_of(cx, d(2026, 2, 10)),
        "the calendar is on February"
    );
}

#[gpui::test]
fn fr11_the_sheet_field_clears(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(d(2026, 11, 18)), |f| f.clear(true));
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    settle(cx);
    assert_eq!(*log.changes.borrow(), [None]);
}

#[gpui::test]
fn fr12_a_disabled_sheet_field_never_opens(cx: &mut TestAppContext) {
    let (cx, _) = sheet(cx, Some(d(2026, 11, 18)), |f| f.disabled(true));
    open_sheet(cx);
    assert!(bounds_of(cx, sheet_day(d(2026, 11, 18))).is_none());
}

#[gpui::test]
fn reset_clears_the_date_and_returns_to_today_s_month(cx: &mut TestAppContext) {
    let (cx, log) = sheet(cx, Some(d(2027, 2, 4)), |f| f);
    open_sheet(cx);
    assert!(shows_month_of(cx, d(2027, 2, 4)));
    // Focus enters on the month-and-year button: the arrows, then Reset.
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    wait(cx, 1000);
    assert_eq!(*log.changes.borrow(), [None]);
    assert!(shows_month_of(cx, d(2026, 11, 25)), "today's month");
}

// ---- The native surface (R16) ------------------------------------------------

use gpui::{NativePickerKind, NativePickerOutcome};

fn native(
    cx: &mut TestAppContext,
    initial: Option<PlainDate>,
    supported: bool,
    configure: impl Fn(DateInput) -> DateInput + 'static,
) -> (&mut VisualTestContext, Rc<Log>) {
    cx.set_native_pickers_supported(supported);
    field(cx, initial, move |f| {
        configure(f.presentation(PickerPresentation::Native))
    })
}

fn alerted(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        astryx_primitives::announce::announcement(
            astryx_primitives::Politeness::Assertive,
            window,
            cx,
        )
        .to_string()
    })
}

/// Opens the platform picker from the focused field and answers it.
fn pick(cx: &mut VisualTestContext, outcome: NativePickerOutcome) {
    press(cx, "space");
    cx.run_until_parked();
    cx.simulate_native_picker(outcome);
    cx.run_until_parked();
}

#[gpui::test]
fn fr2_without_a_platform_picker_native_is_the_pointer_field(cx: &mut TestAppContext) {
    let (cx, log) = native(cx, None, false, |f| f);
    focus_input(cx);
    type_text(cx, "11/20/2026");
    assert_eq!(*log.changes.borrow(), [Some(d(2026, 11, 20))]);
}

#[gpui::test]
fn fr5_a_native_pick_commits_through_the_bounds(cx: &mut TestAppContext) {
    let (cx, log) = native(cx, Some(d(2026, 11, 18)), true, |f| {
        f.min(d(2026, 11, 1)).max(d(2026, 12, 31))
    });
    press(cx, "tab");
    press(cx, "space");
    cx.run_until_parked();
    let request = cx
        .native_picker_request()
        .expect("the platform picker opened");
    assert_eq!(request.kind, NativePickerKind::Date);
    assert_eq!(request.value.as_deref(), Some("2026-11-18"));
    assert_eq!(request.min.as_deref(), Some("2026-11-01"));
    assert_eq!(request.max.as_deref(), Some("2026-12-31"));
    cx.simulate_native_picker(NativePickerOutcome::Picked("2026-11-20".into()));
    cx.run_until_parked();
    assert_eq!(*log.changes.borrow(), [Some(d(2026, 11, 20))]);
    // Dismissing changes nothing; the picker's clear clears.
    pick(cx, NativePickerOutcome::Dismissed);
    assert_eq!(log.changes.borrow().len(), 1);
    pick(cx, NativePickerOutcome::Cleared);
    assert_eq!(log.changes.borrow().last(), Some(&None));
    // Native control takes no typed text, so nothing is reported.
    assert!(log.errors.borrow().is_empty());
}

#[gpui::test]
fn fr5_a_refused_date_is_held_until_enter(cx: &mut TestAppContext) {
    let (cx, log) = native(cx, Some(d(2026, 11, 18)), true, |f| f.max(d(2026, 12, 31)));
    press(cx, "tab");
    pick(cx, NativePickerOutcome::Picked("2027-01-05".into()));
    assert!(
        log.changes.borrow().is_empty(),
        "a date past max never commits"
    );
    assert_eq!(alerted(cx), "", "the refusal waits for Enter or blur");
    press(cx, "enter");
    assert_ne!(alerted(cx), "", "Enter shows it");
    assert!(
        cx.native_picker_request().is_none(),
        "Enter commits, it does not open"
    );
}

#[gpui::test]
fn fr5_blur_shows_a_held_refusal_and_a_new_value_ends_it(cx: &mut TestAppContext) {
    let (cx, log) = native(cx, Some(d(2026, 11, 18)), true, |f| {
        let rule: Rc<dyn Fn(PlainDate) -> bool> = Rc::new(|date: PlainDate| date.day != 13);
        f.date_constraints(vec![rule])
    });
    press(cx, "tab");
    pick(cx, NativePickerOutcome::Picked("2026-11-13".into()));
    assert_eq!(alerted(cx), "");
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    assert_ne!(alerted(cx), "", "blur shows the refusal");
    press(cx, "tab");
    pick(cx, NativePickerOutcome::Picked("2026-11-14".into()));
    assert_eq!(*log.changes.borrow(), [Some(d(2026, 11, 14))]);
}

#[gpui::test]
fn fr12_a_disabled_native_field_never_opens_its_picker(cx: &mut TestAppContext) {
    let (cx, _) = native(cx, Some(d(2026, 11, 18)), true, |f| f.disabled(true));
    press(cx, "tab");
    press(cx, "space");
    cx.run_until_parked();
    assert!(cx.native_picker_request().is_none());
}
