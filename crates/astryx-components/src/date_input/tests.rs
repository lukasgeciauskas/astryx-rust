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
