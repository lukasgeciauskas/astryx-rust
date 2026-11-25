//! Dates and times in every calendar system (AST-111), upstream's
//! `utils/plainDate.ts`, `calendarSystem.ts`, `calendarDateParser.ts`,
//! `dateParser.ts`, `timeParser.ts` and `timeFormat.ts`.
//!
//! Values are Gregorian [`PlainDate`]s and [`PlainTime`]s in every
//! calendar; a [`CalendarSystem`](crate::CalendarSystem) changes only how
//! they are shown and typed. ICU4X does the calendar arithmetic (upstream's
//! `Temporal`) and writes the names and patterns (upstream's `Intl`).

mod calendar;
mod format;
mod gregorian;
mod parse;
mod plain;
mod text;
mod time;

pub use calendar::{
    CalendarDateFields, CalendarMonth, CalendarYearMonth, calendar_add_months,
    calendar_date_from_fields, calendar_day_of_month, calendar_era_year, calendar_month_code,
    calendar_month_from_year_month, calendar_month_of, calendar_month_of_year,
    calendar_months_between, calendar_months_in_calendar_year, calendar_months_in_year,
    calendar_shift_months, calendar_shift_years, calendar_year_month_of, calendar_year_of,
    calendar_year_spans_eras, format_calendar_date, format_calendar_date_to_parts,
    format_calendar_month_label, format_calendar_month_name, format_calendar_year_label,
};
pub use format::{
    DatePart, DateStyle, MAX_ABBREVIATED_HEADER_LENGTH, PartKind, SharedDateFormat, Width,
    calendar_display_name, format_date, format_date_to_parts, format_day_number,
    format_shared_date, format_standalone_month_year, join_parts, week_field_name,
    weekday_header_names, weekday_names,
};
pub use gregorian::{is_locale_day_first, parse_date_input};
pub use parse::{
    CalendarDateLexicon, DateParse, DateParseOptions, calendar_date_lexicon,
    format_calendar_date_hint, format_calendar_date_text, parse_calendar_date,
};
pub use plain::{
    AcceptedDates, DateRange, InvalidDate, PlainDate, Weekday, days_in_month,
    nearest_accepted_date, set_today,
};
pub use text::normalize_date_text;
pub use time::{
    HourFormat, PlainTime, adjust_time, clamp_time, compare_time, format_display_time_12h,
    format_display_time_24h, format_locale_time, is_locale_time_complete, is_time_in_range,
    parse_locale_time, parse_time_input, resolve_hour_format,
};
