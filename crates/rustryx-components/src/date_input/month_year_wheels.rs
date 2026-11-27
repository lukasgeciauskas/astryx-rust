//! The month and year wheels of the touch calendar, upstream
//! `DateInput/MonthYearWheels.tsx`: the months of the shown calendar year
//! (leap months by name) and the years the range reaches, with era labels
//! (DateInput FR21).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    App, ElementId, IntoElement, RenderOnce, SharedString, Styled, Window, div, prelude::*, px,
};
use rustryx_i18n::current as i18n;
use rustryx_i18n::date::{
    calendar_year_spans_eras, format_calendar_month_name, format_calendar_year_label,
};
use rustryx_theme::current;

use super::month_model::{MonthModel, clamp_index};
use crate::wheel::options::PANE_HEIGHT;
use crate::wheel::{Wheel, WheelOption};

pub(crate) type MonthFn = Rc<dyn Fn(i64, &mut Window, &mut App)>;

/// Month and year wheels, bounded by the scroller's range.
#[derive(IntoElement)]
pub(crate) struct MonthYearWheels {
    pub id: ElementId,
    pub model: MonthModel,
    pub month_index: i64,
    pub min_index: i64,
    pub max_index: i64,
    pub on_change: MonthFn,
    pub month_label: SharedString,
    pub year_label: SharedString,
    pub active: bool,
    /// The day the picker keeps: a year with an era change takes its era
    /// from this day of the selected month.
    pub kept_day: u8,
}

type YearKey = (String, String, i32, i32);
/// Each year of a range: its number, its label and whether it spans eras.
type Years = Rc<Vec<(i32, String, bool)>>;

thread_local! {
    /// Each range's years, labeled: era labels are slow to work out.
    static YEARS: RefCell<HashMap<YearKey, Years>> = RefCell::default();
}

fn years(model: &MonthModel, locale: &str, first: i32, last: i32) -> Years {
    let key = (
        model.calendar.identifier().to_string(),
        locale.to_string(),
        first,
        last,
    );
    if let Some(years) = YEARS.with(|y| y.borrow().get(&key).cloned()) {
        return years;
    }
    let years: Rc<Vec<_>> = Rc::new(
        (first..=last)
            .map(|year| {
                (
                    year,
                    format_calendar_year_label(model.calendar, year, locale, None),
                    calendar_year_spans_eras(model.calendar, year, locale),
                )
            })
            .collect(),
    );
    YEARS.with(|y| y.borrow_mut().insert(key, years.clone()));
    years
}

impl RenderOnce for MonthYearWheels {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let locale = i18n(cx).locale().clone();
        let model = self.model.clone();
        let (year, month) = model.year_month_at(self.month_index);

        // Months outside the range stay on the wheel, disabled: a list whose
        // length changed with the range would jump under the finger.
        let month_options: Rc<[WheelOption]> = (1..=model.months_in_year(year))
            .map(|position| {
                let candidate = model.index_of_year_month(year, position);
                WheelOption::new(
                    i64::from(position),
                    format_calendar_month_name(model.calendar, &model.month_at(candidate), &locale),
                )
                .disabled(candidate < self.min_index || candidate > self.max_index)
            })
            .collect();
        let first = model.year_month_at(self.min_index).0;
        let last = model.year_month_at(self.max_index).0;
        let year_options: Rc<[WheelOption]> = years(&model, &locale, first, last)
            .iter()
            .map(|(value, label, spans)| {
                // A year with an era change takes the era of the shown date.
                let label = if *spans {
                    format_calendar_year_label(
                        model.calendar,
                        *value,
                        &locale,
                        Some((month, self.kept_day)),
                    )
                } else {
                    label.clone()
                };
                WheelOption::new(i64::from(*value), label)
            })
            .collect();

        let (month_model, year_model) = (model.clone(), model.clone());
        let (on_month, on_year) = (self.on_change.clone(), self.on_change.clone());
        let (index, min, max) = (self.month_index, self.min_index, self.max_index);
        div()
            .flex()
            .w_full()
            .h(px(PANE_HEIGHT as f32))
            .gap(t.spacing.s2)
            .child(
                Wheel::new(
                    ElementId::Name(format!("{}-month", self.id).into()),
                    self.month_label,
                    month_options,
                    i64::from(month),
                    move |next, window, cx| {
                        on_month(
                            month_model.index_of_year_month(year, next as u8),
                            window,
                            cx,
                        )
                    },
                )
                .active(self.active),
            )
            .child(
                Wheel::new(
                    ElementId::Name(format!("{}-year", self.id).into()),
                    self.year_label,
                    year_options,
                    i64::from(year),
                    move |next, window, cx| {
                        // The same month of the new year, or its common
                        // counterpart; a year change may leave the range.
                        let candidate = year_model.shift_years(index, next - i64::from(year));
                        on_year(clamp_index(candidate, min, max), window, cx)
                    },
                )
                .active(self.active),
            )
    }
}
