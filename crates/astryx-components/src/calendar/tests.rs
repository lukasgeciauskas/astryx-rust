//! Calendar tests, numbered after `Calendar.spec.md` and AST-111.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_i18n::date::{DateRange, PlainDate, set_today};
use astryx_i18n::{CalendarSystem, I18n, provider};
use astryx_primitives::Politeness;
use gpui::{IntoElement, Modifiers, TestAppContext, VisualTestContext};

use super::*;
use crate::segmented::tests::{host, press};

fn d(y: i32, m: u8, day: u8) -> PlainDate {
    PlainDate::new(y, m, day).unwrap()
}

type Log = Rc<RefCell<Vec<PlainDate>>>;

/// A single-mode calendar that logs its picks.
fn single(
    cx: &mut TestAppContext,
    configure: impl Fn(Calendar) -> Calendar + 'static,
) -> (&mut VisualTestContext, Log) {
    set_today(Some(d(2026, 11, 25)));
    let log: Log = Rc::default();
    let picks = log.clone();
    let cx = host(cx, move || {
        let picks = picks.clone();
        configure(Calendar::new("cal").on_change(move |date, _, _| picks.borrow_mut().push(date)))
            .into_any_element()
    });
    (cx, log)
}

fn day_center(cx: &mut VisualTestContext, date: PlainDate) -> gpui::Point<gpui::Pixels> {
    let selector: &'static str =
        Box::leak(day_selector(&"cal".into(), date, false).into_boxed_str());
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{date} is shown"))
        .center()
}

fn click_day(cx: &mut VisualTestContext, date: PlainDate) {
    let at = day_center(cx, date);
    cx.simulate_click(at, Modifiers::none());
}

/// Tabs past the month buttons onto the month's tab stop.
fn tab_into_grid(cx: &mut VisualTestContext) {
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "tab");
}

fn heard(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        astryx_primitives::announce::announcement(Politeness::Polite, window, cx).to_string()
    })
}

/// Renders `use_calendar_navigation` and keeps what it returned.
#[derive(gpui::IntoElement)]
struct NavProbe {
    options: CalendarNavigationOptions,
    out: Rc<RefCell<Option<CalendarNavigation>>>,
}

impl gpui::RenderOnce for NavProbe {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        *self.out.borrow_mut() = Some(use_calendar_navigation("n", self.options, window, cx));
        gpui::div()
    }
}

fn probe(
    cx: &mut TestAppContext,
    options: CalendarNavigationOptions,
) -> (
    &mut VisualTestContext,
    Rc<RefCell<Option<CalendarNavigation>>>,
) {
    let out: Rc<RefCell<Option<CalendarNavigation>>> = Rc::default();
    let shown = out.clone();
    let cx = host(cx, move || {
        NavProbe {
            options: options.clone(),
            out: shown.clone(),
        }
        .into_any_element()
    });
    (cx, out)
}

#[gpui::test]
fn fr1_month_labels_stand_alone_in_the_locale(cx: &mut TestAppContext) {
    let (_, nav) = probe(
        cx,
        CalendarNavigationOptions {
            initial_value: Some(d(2026, 11, 2)),
            number_of_months: 2,
            ..Default::default()
        },
    );
    assert_eq!(
        nav.borrow().as_ref().unwrap().month_year_label,
        "November 2026 – December 2026"
    );
    let fr = astryx_i18n::date::format_calendar_month_label(
        CalendarSystem::Gregory,
        &astryx_i18n::date::calendar_month_of(CalendarSystem::Gregory, d(2026, 6, 1)),
        "fr",
        None,
    );
    assert_eq!(fr, "Juin 2026");
}

#[gpui::test]
fn fr2_the_tab_stop_is_the_selected_day(cx: &mut TestAppContext) {
    let (cx, log) = single(cx, |c| c.default_value(d(2026, 11, 10)));
    tab_into_grid(cx);
    press(cx, "enter");
    assert_eq!(*log.borrow(), [d(2026, 11, 10)]);
}

#[gpui::test]
fn fr2_without_a_value_today_holds_the_tab_stop(cx: &mut TestAppContext) {
    let (cx, log) = single(cx, |c| c);
    tab_into_grid(cx);
    press(cx, "enter");
    assert_eq!(*log.borrow(), [d(2026, 11, 25)]);
}

#[gpui::test]
fn fr3_a_disabled_today_yields_the_stop_and_cannot_be_picked(cx: &mut TestAppContext) {
    let (cx, log) = single(cx, |c| {
        c.date_constraint(|date| date.day != 25 && date.day > 2)
    });
    click_day(cx, d(2026, 11, 25));
    assert!(log.borrow().is_empty(), "a disabled day is not picked");
    tab_into_grid(cx);
    press(cx, "enter");
    assert_eq!(
        *log.borrow(),
        [d(2026, 11, 3)],
        "the month's first enabled day"
    );
}

#[gpui::test]
fn fr4_a_second_pick_before_the_start_swaps_the_range(cx: &mut TestAppContext) {
    set_today(Some(d(2026, 11, 25)));
    let ranges: Rc<RefCell<Vec<DateRange>>> = Rc::default();
    let log = ranges.clone();
    let cx = host(cx, move || {
        let log = log.clone();
        Calendar::new("cal")
            .mode(CalendarMode::Range)
            .on_range_change(move |r, _, _| log.borrow_mut().push(r))
            .into_any_element()
    });
    click_day(cx, d(2026, 11, 20));
    assert!(
        heard(cx).contains("November 20, 2026"),
        "the start is announced"
    );
    click_day(cx, d(2026, 11, 12));
    assert_eq!(
        *ranges.borrow(),
        [DateRange::new(d(2026, 11, 12), d(2026, 11, 20))]
    );
    assert!(
        heard(cx).contains("November 12"),
        "the range is announced in order"
    );
}

#[gpui::test]
fn fr4_escape_or_the_start_again_clears_a_range_in_progress(cx: &mut TestAppContext) {
    set_today(Some(d(2026, 11, 25)));
    let ranges: Rc<RefCell<Vec<DateRange>>> = Rc::default();
    let log = ranges.clone();
    let cx = host(cx, move || {
        let log = log.clone();
        Calendar::new("cal")
            .mode(CalendarMode::Range)
            .min_range_span(3_u32)
            .on_range_change(move |r, _, _| log.borrow_mut().push(r))
            .into_any_element()
    });
    click_day(cx, d(2026, 11, 10));
    // The start again, with a minimum above one day, clears it.
    click_day(cx, d(2026, 11, 10));
    assert!(heard(cx).contains("November 10"));
    click_day(cx, d(2026, 11, 14));
    press(cx, "escape");
    click_day(cx, d(2026, 11, 16));
    click_day(cx, d(2026, 11, 20));
    assert_eq!(
        *ranges.borrow(),
        [DateRange::new(d(2026, 11, 16), d(2026, 11, 20))],
        "only the range picked after Escape completes"
    );
}

#[gpui::test]
fn fr4_spans_bound_the_range_from_its_start(cx: &mut TestAppContext) {
    set_today(Some(d(2026, 11, 25)));
    let ranges: Rc<RefCell<Vec<DateRange>>> = Rc::default();
    let log = ranges.clone();
    let cx = host(cx, move || {
        let log = log.clone();
        Calendar::new("cal")
            .mode(CalendarMode::Range)
            .max_range_span(7_u32)
            .on_range_change(move |r, _, _| log.borrow_mut().push(r))
            .into_any_element()
    });
    click_day(cx, d(2026, 11, 10));
    click_day(cx, d(2026, 11, 17));
    assert!(ranges.borrow().is_empty(), "eight days is past the span");
    click_day(cx, d(2026, 11, 16));
    assert_eq!(
        *ranges.borrow(),
        [DateRange::new(d(2026, 11, 10), d(2026, 11, 16))]
    );
}

#[gpui::test]
fn fr5_arrows_move_by_day_and_week_across_months(cx: &mut TestAppContext) {
    let (cx, log) = single(cx, |c| c.default_value(d(2026, 11, 30)));
    tab_into_grid(cx);
    press(cx, "right");
    press(cx, "enter");
    assert_eq!(
        log.borrow().last(),
        Some(&d(2026, 12, 1)),
        "into the next month"
    );
    press(cx, "up");
    press(cx, "enter");
    assert_eq!(
        log.borrow().last(),
        Some(&d(2026, 11, 24)),
        "back a week, back a month"
    );
    press(cx, "home");
    press(cx, "enter");
    assert_eq!(
        log.borrow().last(),
        Some(&d(2026, 11, 22)),
        "the week's first day"
    );
    press(cx, "end");
    press(cx, "enter");
    assert_eq!(
        log.borrow().last(),
        Some(&d(2026, 11, 28)),
        "the week's last day"
    );
}

#[gpui::test]
fn fr5_arrows_skip_disabled_days_and_stop_at_the_bounds(cx: &mut TestAppContext) {
    let (cx, log) = single(cx, |c| {
        c.default_value(d(2026, 11, 13))
            .max(d(2026, 11, 20))
            .date_constraint(|date| date.day != 14 && date.day != 15)
    });
    tab_into_grid(cx);
    press(cx, "right");
    press(cx, "enter");
    assert_eq!(
        log.borrow().last(),
        Some(&d(2026, 11, 16)),
        "over the disabled days"
    );
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(
        log.borrow().last(),
        Some(&d(2026, 11, 16)),
        "a week on is past max: nothing moves"
    );
}

#[gpui::test]
fn fr9_page_keys_step_calendar_months_and_years(cx: &mut TestAppContext) {
    let (cx, log) = single(cx, |c| c.default_value(d(2026, 1, 31)));
    tab_into_grid(cx);
    press(cx, "pagedown");
    press(cx, "enter");
    assert_eq!(
        log.borrow().last(),
        Some(&d(2026, 2, 28)),
        "clamped to February"
    );
    press(cx, "shift-pagedown");
    press(cx, "enter");
    assert_eq!(log.borrow().last(), Some(&d(2027, 2, 28)), "a year on");
    press(cx, "pageup");
    press(cx, "enter");
    assert_eq!(log.borrow().last(), Some(&d(2027, 1, 28)));
}

#[gpui::test]
fn fr9_page_keys_step_persian_months(cx: &mut TestAppContext) {
    // 8 Aban 1405; a month on is 8 Azar.
    let (cx, log) = single(cx, |c| {
        c.calendar(CalendarSystem::Persian)
            .default_value(d(2026, 10, 30))
    });
    tab_into_grid(cx);
    press(cx, "pagedown");
    press(cx, "enter");
    assert_eq!(log.borrow().last(), Some(&d(2026, 11, 29)));
}

#[gpui::test]
fn fr9_a_year_step_turns_adar_i_into_adar(cx: &mut TestAppContext) {
    // 15 Adar I 5784 (a leap year); 5785 has one Adar.
    let (cx, log) = single(cx, |c| {
        c.calendar(CalendarSystem::Hebrew)
            .default_value(d(2024, 2, 24))
    });
    tab_into_grid(cx);
    press(cx, "shift-pagedown");
    press(cx, "enter");
    let picked = *log.borrow().last().unwrap();
    assert_eq!(
        astryx_i18n::date::calendar_month_code(CalendarSystem::Hebrew, picked),
        "M06"
    );
    assert_eq!(
        astryx_i18n::date::calendar_day_of_month(CalendarSystem::Hebrew, picked),
        15
    );
}

#[gpui::test]
fn fr6_month_buttons_stop_at_the_bounds(cx: &mut TestAppContext) {
    set_today(Some(d(2026, 11, 25)));
    let months: Log = Rc::default();
    let log = months.clone();
    let cx = host(cx, move || {
        let log = log.clone();
        Calendar::new("cal")
            .min(d(2026, 11, 3))
            .max(d(2026, 12, 15))
            .on_focus_date_change(move |date, _, _| log.borrow_mut().push(date))
            .into_any_element()
    });
    press(cx, "tab");
    press(cx, "enter");
    assert!(months.borrow().is_empty(), "previous is past min");
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "enter");
    assert_eq!(
        *months.borrow(),
        [d(2026, 12, 1)],
        "next once, then past max"
    );
}

#[gpui::test]
fn fr7_a_changed_month_is_announced_the_first_is_not(cx: &mut TestAppContext) {
    let (cx, _) = single(cx, |c| c.default_value(d(2026, 11, 10)));
    assert_eq!(heard(cx), "");
    tab_into_grid(cx);
    press(cx, "pagedown");
    assert_eq!(heard(cx), "December 2026");
}

#[gpui::test]
fn fr8_the_grid_shows_calendar_months_and_values_stay_gregorian(cx: &mut TestAppContext) {
    let (cx, log) = single(cx, |c| {
        c.calendar(CalendarSystem::Persian)
            .default_value(d(2026, 10, 30))
    });
    // Aban 1405 starts on October 23.
    click_day(cx, d(2026, 10, 23));
    assert_eq!(*log.borrow(), [d(2026, 10, 23)]);
}

#[gpui::test]
fn fr8_the_provider_calendar_applies_and_a_prop_wins(cx: &mut TestAppContext) {
    set_today(Some(d(2026, 11, 25)));
    let log: Log = Rc::default();
    let picks = log.clone();
    let cx = host(cx, move || {
        let picks = picks.clone();
        provider(
            I18n::new("en").calendar(CalendarSystem::Persian),
            Calendar::new("cal")
                .default_value(d(2026, 10, 30))
                .on_change(move |date, _, _| picks.borrow_mut().push(date)),
        )
        .into_any_element()
    });
    tab_into_grid(cx);
    press(cx, "pagedown");
    press(cx, "enter");
    assert_eq!(*log.borrow(), [d(2026, 11, 29)], "a Persian month on");
}

#[test]
fn fr10_week_numbers_are_iso_8601_by_the_rows_thursday() {
    // A Persian row from Saturday 2026-10-24 to Friday 2026-10-30: its
    // Thursday, October 29, is in ISO week 44.
    let first = d(2026, 10, 24);
    let thursday = first.add_days((4 - first.weekday().index() as i64).rem_euclid(7));
    assert_eq!(thursday, d(2026, 10, 29));
    assert_eq!(thursday.iso_week_number(), 44);
}

#[gpui::test]
fn fr11_navigation_steps_and_reports_calendar_months(cx: &mut TestAppContext) {
    let (cx, out) = probe(
        cx,
        CalendarNavigationOptions {
            initial_value: Some(d(2026, 10, 30)),
            calendar: CalendarSystem::Persian,
            ..Default::default()
        },
    );
    let nav = out.borrow().clone().unwrap();
    assert_eq!(nav.month_year_label, "Aban 1405 AP");
    assert_eq!(nav.visible_calendar_months[0].month, 8);
    assert_eq!(nav.base_month, d(2026, 10, 23));
    cx.update(|window, cx| nav.navigate_month(1, Some(d(2026, 10, 30)), Some(1), window, cx));
    cx.run_until_parked();
    let nav = out.borrow().clone().unwrap();
    assert_eq!(nav.month_year_label, "Azar 1405 AP");
    assert_eq!(nav.pending_focus, Some(d(2026, 10, 31)));
}

#[gpui::test]
fn the_handle_navigates_to_a_month(cx: &mut TestAppContext) {
    set_today(Some(d(2026, 11, 25)));
    let handle = CalendarHandle::new();
    let shown = handle.clone();
    let log: Log = Rc::default();
    let picks = log.clone();
    let cx = host(cx, move || {
        let picks = picks.clone();
        Calendar::new("cal")
            .handle(shown.clone())
            .on_change(move |date, _, _| picks.borrow_mut().push(date))
            .into_any_element()
    });
    cx.update(|window, cx| handle.navigate_to(d(2027, 3, 1), window, cx));
    cx.run_until_parked();
    click_day(cx, d(2027, 3, 9));
    assert_eq!(*log.borrow(), [d(2027, 3, 9)]);
}

#[gpui::test]
fn ar1_arrows_follow_the_reading_direction(cx: &mut TestAppContext) {
    set_today(Some(d(2026, 11, 25)));
    let log: Log = Rc::default();
    let picks = log.clone();
    let cx = host(cx, move || {
        let picks = picks.clone();
        provider(
            I18n::new("ar"),
            Calendar::new("cal")
                .default_value(d(2026, 11, 10))
                .on_change(move |date, _, _| picks.borrow_mut().push(date)),
        )
        .into_any_element()
    });
    tab_into_grid(cx);
    press(cx, "left");
    press(cx, "enter");
    assert_eq!(*log.borrow(), [d(2026, 11, 11)], "left is forward in RTL");
}

#[gpui::test]
fn two_months_keep_one_pick_per_date(cx: &mut TestAppContext) {
    let (cx, log) = single(cx, |c| c.number_of_months(2).default_value(d(2026, 11, 30)));
    tab_into_grid(cx);
    press(cx, "right");
    press(cx, "enter");
    assert_eq!(
        *log.borrow(),
        [d(2026, 12, 1)],
        "the real December 1, in the second grid"
    );
}
