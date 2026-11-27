//! DateRangeInput tests, numbered after `DateRangeInput.spec.md`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{IntoElement, TestAppContext, VisualTestContext, px, size};
use rustryx_i18n::date::{DateRange, PlainDate, set_today};
use rustryx_i18n::{CalendarSystem, I18n};

use super::date_range_input::format_range;
use super::*;
use crate::calendar::day_selector;
use crate::segmented::tests::{host, press};

fn d(y: i32, m: u8, day: u8) -> PlainDate {
    PlainDate::new(y, m, day).unwrap()
}

fn r(start: PlainDate, end: PlainDate) -> DateRange {
    DateRange::new(start, end)
}

#[derive(Default)]
struct Log {
    changes: RefCell<Vec<Option<DateRange>>>,
}

fn field(
    cx: &mut TestAppContext,
    initial: Option<DateRange>,
    configure: impl Fn(DateRangeInput) -> DateRangeInput + 'static,
) -> (&mut VisualTestContext, Rc<Log>) {
    set_today(Some(d(2026, 11, 25)));
    let log = Rc::new(Log::default());
    let value = Rc::new(RefCell::new(initial));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let (log, value) = (log.clone(), value.clone());
            let current = *value.borrow();
            configure(
                DateRangeInput::new("dri", "Stay", current).on_change(move |v, window, _| {
                    log.changes.borrow_mut().push(v);
                    *value.borrow_mut() = v;
                    window.refresh();
                }),
            )
            .into_any_element()
        }
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    (cx, log)
}

fn leak(selector: String) -> &'static str {
    Box::leak(selector.into_boxed_str())
}

fn bounds(cx: &mut VisualTestContext, selector: String) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(leak(selector))
}

fn click(cx: &mut VisualTestContext, selector: String) {
    let at = bounds(cx, selector.clone())
        .unwrap_or_else(|| panic!("nothing at {selector}"))
        .center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
}

fn day(date: PlainDate) -> String {
    day_selector(&"dri-calendar".into(), date, false)
}

fn calendar_shows(cx: &mut VisualTestContext, date: PlainDate) -> bool {
    bounds(cx, day(date)).is_some()
}

fn open(cx: &mut VisualTestContext) {
    click(cx, "dri-trigger".into());
}

#[gpui::test]
fn fr1_a_calendar_range_commits_and_closes(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    open(cx);
    assert!(calendar_shows(cx, d(2026, 11, 10)));
    click(cx, day(d(2026, 11, 10)));
    assert!(
        log.changes.borrow().is_empty(),
        "a start alone emits nothing"
    );
    click(cx, day(d(2026, 11, 12)));
    assert_eq!(
        *log.changes.borrow(),
        [Some(r(d(2026, 11, 10), d(2026, 11, 12)))]
    );
    assert!(!calendar_shows(cx, d(2026, 11, 10)), "the popover closed");
}

#[gpui::test]
fn fr1_clear_emits_none_and_hands_focus_to_the_trigger(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, Some(r(d(2026, 11, 2), d(2026, 11, 6))), |f| f);
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    cx.run_until_parked();
    assert_eq!(*log.changes.borrow(), [None]);
    press(cx, "enter");
    cx.run_until_parked();
    assert!(calendar_shows(cx, d(2026, 11, 10)), "the trigger has focus");
}

#[gpui::test]
fn fr2_a_preset_applies_its_range_and_closes(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| {
        f.presets([DateRangePreset::new("First week", || {
            r(d(2026, 11, 1), d(2026, 11, 7))
        })])
    });
    open(cx);
    click(cx, "dri-preset-0".into());
    assert_eq!(
        *log.changes.borrow(),
        [Some(r(d(2026, 11, 1), d(2026, 11, 7)))]
    );
    assert!(!calendar_shows(cx, d(2026, 11, 10)));
}

#[gpui::test]
fn fr2_a_preset_past_a_bound_or_a_span_is_disabled(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| {
        f.max(d(2026, 11, 30))
            .max_range_span(7)
            .date_constraint(|date| date != d(2026, 11, 16))
            .presets([
                DateRangePreset::new("Past max", || r(d(2026, 11, 28), d(2026, 12, 2))),
                DateRangePreset::new("Too long", || r(d(2026, 11, 1), d(2026, 11, 10))),
                DateRangePreset::new("Blocked end", || r(d(2026, 11, 12), d(2026, 11, 16))),
                DateRangePreset::new("Fine", || r(d(2026, 11, 2), d(2026, 11, 8))),
            ])
    });
    open(cx);
    for index in 0..3 {
        click(cx, format!("dri-preset-{index}"));
        assert!(
            log.changes.borrow().is_empty(),
            "preset {index} is disabled"
        );
        assert!(
            calendar_shows(cx, d(2026, 11, 10)),
            "the popover stays open"
        );
    }
    click(cx, "dri-preset-3".into());
    assert_eq!(
        *log.changes.borrow(),
        [Some(r(d(2026, 11, 2), d(2026, 11, 8)))]
    );
}

#[gpui::test]
fn fr2_spans_count_both_ends(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| {
        f.min_range_span(3).presets([
            DateRangePreset::new("Two days", || r(d(2026, 11, 2), d(2026, 11, 3))),
            DateRangePreset::new("Three days", || r(d(2026, 11, 2), d(2026, 11, 4))),
        ])
    });
    open(cx);
    click(cx, "dri-preset-0".into());
    assert!(log.changes.borrow().is_empty());
    click(cx, "dri-preset-1".into());
    assert_eq!(log.changes.borrow().len(), 1);
}

#[gpui::test]
fn fr4_escape_drops_a_range_in_progress_before_it_closes(cx: &mut TestAppContext) {
    let (cx, log) = field(cx, None, |f| f);
    open(cx);
    click(cx, day(d(2026, 11, 10)));
    press(cx, "escape");
    cx.run_until_parked();
    assert!(
        calendar_shows(cx, d(2026, 11, 10)),
        "the first Escape only drops the start"
    );
    click(cx, day(d(2026, 11, 12)));
    assert!(
        log.changes.borrow().is_empty(),
        "the next press starts a new range"
    );
    press(cx, "escape");
    press(cx, "escape");
    cx.run_until_parked();
    assert!(!calendar_shows(cx, d(2026, 11, 10)), "then Escape closes");
}

#[gpui::test]
fn fr5_presets_sit_beside_the_calendar_or_stack_above_it(cx: &mut TestAppContext) {
    let (cx, _) = field(cx, None, |f| {
        f.number_of_months(1)
            .presets([DateRangePreset::new("First week", || {
                r(d(2026, 11, 1), d(2026, 11, 7))
            })])
    });
    cx.simulate_resize(size(px(1200.), px(900.)));
    cx.run_until_parked();
    open(cx);
    let preset = bounds(cx, "dri-preset-0".into()).unwrap();
    let first = bounds(cx, day(d(2026, 11, 1))).unwrap();
    assert!(preset.right() <= first.left(), "side by side when they fit");
    press(cx, "escape");
    cx.run_until_parked();

    cx.simulate_resize(size(px(340.), px(900.)));
    cx.run_until_parked();
    open(cx);
    let preset = bounds(cx, "dri-preset-0".into()).unwrap();
    let first = bounds(cx, day(d(2026, 11, 1))).unwrap();
    assert!(preset.bottom() <= first.top(), "stacked above when narrow");
}

#[gpui::test]
fn fr6_the_trigger_writes_the_range_in_the_calendar(_cx: &mut TestAppContext) {
    let i18n = I18n::new("en");
    let today = d(2026, 11, 25);
    let this_year = Some(r(d(2026, 11, 19), d(2026, 11, 25)));
    let text = format_range(this_year, CalendarSystem::Gregory, "en", today, &i18n);
    assert!(text.contains("Nov 19") && text.contains("Nov 25"), "{text}");
    assert!(
        !text.contains("2026"),
        "this year's range drops the year: {text}"
    );
    let across = Some(r(d(2026, 12, 28), d(2027, 1, 3)));
    let text = format_range(across, CalendarSystem::Gregory, "en", today, &i18n);
    assert!(text.contains("2026") && text.contains("2027"), "{text}");
    let text = format_range(this_year, CalendarSystem::Persian, "en", today, &i18n);
    assert!(text.contains("Azar") && !text.contains("Nov"), "{text}");
}

#[gpui::test]
fn ast085_a_disabled_field_does_not_open(cx: &mut TestAppContext) {
    let (cx, _) = field(cx, None, |f| f.disabled(true));
    open(cx);
    assert!(!calendar_shows(cx, d(2026, 11, 10)));
}
