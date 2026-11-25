//! Month indexes for the touch calendar, upstream `DateInput/monthGeometry.ts`:
//! one integer per calendar month, so the scroller pages and the wheels turn
//! by index in any calendar system.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use astryx_i18n::CalendarSystem;
use astryx_i18n::date::{
    CalendarMonth, PlainDate, calendar_add_months, calendar_month_of, calendar_month_of_year,
    calendar_months_between, calendar_months_in_calendar_year, calendar_shift_months,
    calendar_shift_years, calendar_year_month_of,
};

/// Months reachable either way when unbounded (`DEFAULT_MONTH_REACH`).
pub(crate) const DEFAULT_MONTH_REACH: i64 = 600;

/// `value` held to `[min, max]` (`clampIndex`).
pub(crate) fn clamp_index(value: i64, min: i64, max: i64) -> i64 {
    value.max(min).min(max)
}

fn to_index(year: i32, month: u8) -> i64 {
    i64::from(year) * 12 + i64::from(month) - 1
}

fn from_index(index: i64) -> (i32, u8) {
    (
        index.div_euclid(12) as i32,
        (index.rem_euclid(12) + 1) as u8,
    )
}

/// A calendar's months by index (`MonthModel`). Gregorian-aligned
/// calendars count Gregorian years; the others count months from the month
/// that holds 2000-01-01, cached because calendar arithmetic is slow next
/// to a render.
#[derive(Clone)]
pub(crate) struct MonthModel {
    pub calendar: CalendarSystem,
    epoch: Option<CalendarMonth>,
    months: Rc<RefCell<HashMap<i64, CalendarMonth>>>,
}

impl MonthModel {
    /// Upstream `createMonthModel`.
    pub fn new(calendar: CalendarSystem) -> Self {
        let epoch = (!calendar.is_gregorian_aligned())
            .then(|| calendar_month_of(calendar, PlainDate::new(2000, 1, 1).expect("a real date")));
        Self {
            calendar,
            epoch,
            months: Rc::default(),
        }
    }

    /// The index of the calendar month that holds `date`.
    pub fn index_of(&self, date: PlainDate) -> i64 {
        match &self.epoch {
            None => to_index(date.year, date.month),
            Some(epoch) => i64::from(calendar_months_between(
                self.calendar,
                epoch,
                &calendar_month_of(self.calendar, date),
            )),
        }
    }

    /// The calendar month at `index`.
    pub fn month_at(&self, index: i64) -> CalendarMonth {
        match &self.epoch {
            None => {
                let (year, month) = from_index(index);
                calendar_month_of_year(self.calendar, year, month)
            }
            Some(epoch) => {
                if let Some(month) = self.months.borrow().get(&index) {
                    return *month;
                }
                let month = calendar_add_months(self.calendar, epoch, index as i32);
                self.months.borrow_mut().insert(index, month);
                month
            }
        }
    }

    /// The calendar year of `index` and the month's position in it.
    pub fn year_month_at(&self, index: i64) -> (i32, u8) {
        match &self.epoch {
            None => from_index(index),
            Some(_) => {
                let ym = calendar_year_month_of(self.calendar, &self.month_at(index));
                (ym.year, ym.month)
            }
        }
    }

    /// The index of a month position in a calendar year.
    pub fn index_of_year_month(&self, year: i32, month: u8) -> i64 {
        match &self.epoch {
            None => to_index(year, month),
            Some(_) => self.index_of(calendar_month_of_year(self.calendar, year, month).start),
        }
    }

    /// The months in a calendar year.
    pub fn months_in_year(&self, year: i32) -> u8 {
        match &self.epoch {
            None => 12,
            Some(_) => calendar_months_in_calendar_year(self.calendar, year),
        }
    }

    /// The index `years` calendar years on, in the same month; a leap month
    /// the target year lacks becomes its common counterpart.
    pub fn shift_years(&self, index: i64, years: i64) -> i64 {
        match &self.epoch {
            None => index + years * 12,
            Some(_) => self.index_of(calendar_shift_years(
                self.calendar,
                self.month_at(index).start,
                years as i32,
            )),
        }
    }
}

/// `date` moved to the month at `month_index`, keeping its day (the last day
/// of a shorter month), inside the bounds (`dateInMonth`).
pub(crate) fn date_in_month(
    model: &MonthModel,
    date: PlainDate,
    month_index: i64,
    min: Option<PlainDate>,
    max: Option<PlainDate>,
) -> PlainDate {
    let moved = calendar_shift_months(
        model.calendar,
        date,
        (month_index - model.index_of(date)) as i32,
    );
    match (min, max) {
        (Some(min), _) if moved < min => min,
        (_, Some(max)) if moved > max => max,
        _ => moved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> PlainDate {
        PlainDate::new(y, m, day).unwrap()
    }

    #[test]
    fn fr23_a_moved_date_keeps_its_day_or_the_month_s_last() {
        let model = MonthModel::new(CalendarSystem::Gregory);
        let feb = model.index_of(d(2026, 2, 1));
        assert_eq!(
            date_in_month(&model, d(2026, 1, 31), feb, None, None),
            d(2026, 2, 28)
        );
        assert_eq!(
            date_in_month(&model, d(2026, 1, 31), feb, None, Some(d(2026, 2, 10))),
            d(2026, 2, 10)
        );
    }

    #[test]
    fn fr21_calendar_months_round_trip_by_index() {
        for calendar in [
            CalendarSystem::Persian,
            CalendarSystem::Hebrew,
            CalendarSystem::Ethiopic,
        ] {
            let model = MonthModel::new(calendar);
            let start = model.index_of(d(2026, 3, 21));
            for index in start - 30..start + 30 {
                let month = model.month_at(index);
                assert_eq!(model.index_of(month.start), index, "{calendar:?}");
                let (year, position) = model.year_month_at(index);
                assert_eq!(model.index_of_year_month(year, position), index);
            }
        }
    }

    #[test]
    fn fr21_a_hebrew_leap_year_has_13_months() {
        let model = MonthModel::new(CalendarSystem::Hebrew);
        let (year, _) = model.year_month_at(model.index_of(d(2024, 3, 1)));
        assert_eq!(model.months_in_year(year), 13, "5784 is a leap year");
        assert_eq!(model.months_in_year(year + 1), 12);
    }
}
