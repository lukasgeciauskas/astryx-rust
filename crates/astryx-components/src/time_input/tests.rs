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
