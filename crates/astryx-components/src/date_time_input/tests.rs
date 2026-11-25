//! DateTimeInput tests, numbered after `DateTimeInput.spec.md`.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_i18n::date::{HourFormat, PlainDate, PlainDateTime, PlainTime, set_time_now, set_today};
use gpui::{IntoElement, TestAppContext, VisualTestContext};

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
    cx.update(astryx_primitives::editing::init);
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
        window.dispatch_action(Box::new(astryx_primitives::editing::SelectAll), cx)
    });
    type_text(cx, "11/19/2026");
    assert_eq!(
        log.changes.borrow().last(),
        Some(&Some(at(d(2026, 11, 19), t(14, 30))))
    );
    press(cx, "tab");
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(astryx_primitives::editing::SelectAll), cx)
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
