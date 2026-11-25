//! Typed dates in a field, upstream `DateInput/useTypedDate.ts`: reading
//! text in the field's calendar, the committed text, the format hint, the
//! calendar description, the rejection messages, and the typing session's
//! state machine that DateInput and DateTimeInput share (AST-111 FR9-FR14,
//! AST-117, AST-118).

use std::rc::Rc;

use astryx_i18n::date::{
    AcceptedDates, DateParse, DateParseOptions, PlainDate, SharedDateFormat, calendar_date_lexicon,
    calendar_display_name, format_calendar_date_hint, format_calendar_date_text,
    nearest_accepted_date, parse_calendar_date, parse_date_input,
};
use astryx_i18n::{CalendarSystem, I18n};
use gpui::SharedString;

use crate::field::EntryErrorBehavior;

/// Upstream `DateInputEntryError` `kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DateEntryErrorKind {
    /// Text that is not a date.
    Unreadable,
    /// Part of a date that Enter or blur cannot finish.
    Incomplete,
    /// A date the calendar does not have.
    Nonexistent,
    /// A date before `min` or after `max`.
    OutOfRange,
    /// A date `date_constraints` rules out.
    Unavailable,
}

/// Upstream `DateInputEntryError`: why the field rejects the typed text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DateInputEntryError {
    pub kind: DateEntryErrorKind,
    /// The typed text.
    pub text: SharedString,
    /// The Gregorian date the text names, for `OutOfRange` and `Unavailable`.
    pub date: Option<PlainDate>,
}

/// The rejection a typed date shows, by its message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypedDateError {
    Unreadable,
    Nonexistent,
    OutOfRange,
}

/// Upstream `DateInputFormat`: how the committed date is written.
#[derive(Clone)]
pub enum DateInputFormat {
    /// A shared Timestamp format, written in the field's calendar.
    Named(SharedDateFormat),
    /// A custom function of the ISO date; display only (FR14).
    Custom(Rc<dyn Fn(PlainDate) -> String>),
}

impl Default for DateInputFormat {
    fn default() -> Self {
        DateInputFormat::Named(SharedDateFormat::DateLong)
    }
}

impl From<SharedDateFormat> for DateInputFormat {
    fn from(format: SharedDateFormat) -> Self {
        DateInputFormat::Named(format)
    }
}

/// Rejects a date outside the bounds or the field's constraints.
pub(crate) type IsDisabled = Rc<dyn Fn(PlainDate) -> bool>;

/// Typed entry in a calendar (upstream `useTypedDate`).
#[derive(Clone)]
pub(crate) struct TypedDate {
    pub calendar: CalendarSystem,
    locale: SharedString,
    format: DateInputFormat,
    min: Option<PlainDate>,
    max: Option<PlainDate>,
    is_disabled: IsDisabled,
    provider_calendar: CalendarSystem,
    i18n: I18n,
}

impl TypedDate {
    pub fn new(
        calendar: CalendarSystem,
        format: DateInputFormat,
        min: Option<PlainDate>,
        max: Option<PlainDate>,
        is_disabled: IsDisabled,
        i18n: &I18n,
    ) -> Self {
        Self {
            calendar,
            locale: i18n.locale().clone(),
            format,
            min,
            max,
            is_disabled,
            provider_calendar: i18n.calendar_system(),
            i18n: i18n.clone(),
        }
    }

    fn is_gregorian(&self) -> bool {
        self.calendar.is_gregorian()
    }

    /// Builds the lexicon ahead of the first keystroke.
    pub fn prepare(&self) {
        calendar_date_lexicon(&self.locale, self.calendar);
    }

    /// Reads typed text. In the Gregorian calendar, text only
    /// `parse_date_input` reads still commits at Enter or blur (FR9).
    pub fn parse(&self, text: &str) -> DateParse {
        let is_disabled = self.is_disabled.clone();
        let check = move |d: PlainDate| is_disabled(d);
        let options = DateParseOptions {
            today: None,
            iso_is_gregorian: matches!(
                self.format,
                DateInputFormat::Named(SharedDateFormat::SystemDate)
            ),
            is_disabled: Some(&check),
        };
        let result = parse_calendar_date(
            text,
            &calendar_date_lexicon(&self.locale, self.calendar),
            &options,
        );
        if !self.is_gregorian()
            || !matches!(
                result,
                DateParse::Unreadable | DateParse::Nonexistent { .. }
            )
        {
            return result;
        }
        match parse_date_input(text, &self.locale) {
            None => result,
            Some(date) if (self.is_disabled)(date) => DateParse::OutOfRange {
                date,
                is_complete: false,
            },
            Some(date) => DateParse::Value {
                date,
                is_complete: false,
            },
        }
    }

    /// The committed text of a date.
    pub fn format_committed(&self, value: PlainDate) -> String {
        match &self.format {
            DateInputFormat::Custom(f) => f(value),
            DateInputFormat::Named(format) => {
                format_calendar_date_text(value, *format, &self.locale, self.calendar)
            }
        }
    }

    /// An example date for an empty focused field, `None` in the Gregorian
    /// calendar (AR9): today when the field accepts it, else the nearest
    /// date it accepts.
    pub fn format_hint(&self) -> Option<SharedString> {
        if self.is_gregorian() {
            return None;
        }
        let is_disabled = self.is_disabled.clone();
        let check = move |d: PlainDate| is_disabled(d);
        let example = nearest_accepted_date(
            PlainDate::today(),
            &AcceptedDates {
                min: self.min,
                max: self.max,
                is_disabled: Some(&check),
                search_days: 0,
            },
        );
        let hint = format_calendar_date_hint(example, &self.locale, self.calendar);
        Some(self.i18n.format(
            "@astryx.dateInput.formatHint",
            &[("example", hint.as_str().into())],
        ))
    }

    fn is_provider_calendar(&self) -> bool {
        if self.is_gregorian() {
            self.provider_calendar.is_gregorian()
        } else {
            self.calendar == self.provider_calendar
        }
    }

    /// Names the calendar, `None` when it is the provider's (AR9).
    pub fn calendar_description(&self) -> Option<SharedString> {
        (!self.is_provider_calendar()).then(|| {
            let name = calendar_display_name(self.calendar, &self.locale);
            self.i18n.format(
                "@astryx.dateInput.calendarDescription",
                &[("calendar", name.as_str().into())],
            )
        })
    }

    fn outside_bounds(&self, date: PlainDate) -> bool {
        self.min.is_some_and(|m| date < m) || self.max.is_some_and(|m| date > m)
    }

    /// The message of a rejected result; each kind has its own (AR3).
    /// Whether `min`, `max` or a constraint rules `date` out.
    pub fn is_disabled(&self, date: PlainDate) -> bool {
        (self.is_disabled)(date)
    }

    pub fn error_message(&self, result: &DateParse) -> SharedString {
        match result {
            DateParse::Nonexistent { .. } => {
                let name = calendar_display_name(self.calendar, &self.locale);
                self.i18n.format(
                    "@astryx.dateInput.nonexistentDate",
                    &[
                        (
                            "named",
                            if self.is_provider_calendar() {
                                "no"
                            } else {
                                "yes"
                            }
                            .into(),
                        ),
                        ("calendar", name.as_str().into()),
                    ],
                )
            }
            DateParse::OutOfRange { date, .. } => {
                let format = match &self.format {
                    DateInputFormat::Named(f) => *f,
                    DateInputFormat::Custom(_) => SharedDateFormat::DateLong,
                };
                let write = |d: PlainDate| {
                    format_calendar_date_text(d, format, &self.locale, self.calendar)
                };
                let bounds = if !self.outside_bounds(*date) {
                    "none"
                } else {
                    match (self.min, self.max) {
                        (Some(_), Some(_)) => "both",
                        (Some(_), None) => "min",
                        _ => "max",
                    }
                };
                let min = self.min.map(write).unwrap_or_default();
                let max = self.max.map(write).unwrap_or_default();
                self.i18n.format(
                    "@astryx.dateInput.outOfRangeDate",
                    &[
                        ("bounds", bounds.into()),
                        ("min", min.as_str().into()),
                        ("max", max.as_str().into()),
                    ],
                )
            }
            _ => self.i18n.t("@astryx.dateInput.invalidDate"),
        }
    }

    /// The rejection a result reports to the caller (`dateEntryError`).
    pub fn entry_error(
        &self,
        result: &DateParse,
        committed: bool,
        text: &str,
        behavior: EntryErrorBehavior,
    ) -> Option<DateInputEntryError> {
        let error = typed_date_error(Some(result), committed, behavior)?;
        let text: SharedString = text.to_string().into();
        Some(match result {
            DateParse::Incomplete => DateInputEntryError {
                kind: DateEntryErrorKind::Incomplete,
                text,
                date: None,
            },
            DateParse::OutOfRange { date, .. } => DateInputEntryError {
                kind: if self.outside_bounds(*date) {
                    DateEntryErrorKind::OutOfRange
                } else {
                    DateEntryErrorKind::Unavailable
                },
                text,
                date: Some(*date),
            },
            _ => DateInputEntryError {
                kind: match error {
                    TypedDateError::Unreadable => DateEntryErrorKind::Unreadable,
                    TypedDateError::Nonexistent => DateEntryErrorKind::Nonexistent,
                    TypedDateError::OutOfRange => DateEntryErrorKind::OutOfRange,
                },
                text,
                date: None,
            },
        })
    }
}

/// The rejection a result shows (`typedDateError`): committed text's error,
/// or under `Complete` a complete date's.
pub(crate) fn typed_date_error(
    result: Option<&DateParse>,
    committed: bool,
    behavior: EntryErrorBehavior,
) -> Option<TypedDateError> {
    match result? {
        DateParse::Unreadable | DateParse::Incomplete if committed => {
            Some(TypedDateError::Unreadable)
        }
        DateParse::Nonexistent { is_complete }
            if committed || (*is_complete && behavior == EntryErrorBehavior::Complete) =>
        {
            Some(TypedDateError::Nonexistent)
        }
        DateParse::OutOfRange { is_complete, .. }
            if committed || (*is_complete && behavior == EntryErrorBehavior::Complete) =>
        {
            Some(TypedDateError::OutOfRange)
        }
        _ => None,
    }
}

/// Whether a result is a date typing commits (`isAcceptedDate`).
pub(crate) fn is_accepted(result: &DateParse) -> bool {
    matches!(
        result,
        DateParse::Value {
            is_complete: true,
            ..
        }
    )
}

/// A typing session's state between frames (`useTypedDateField`'s state).
#[derive(Clone, Debug, Default)]
pub(crate) struct TypedState {
    /// The typed text, or `None` while the field shows its value.
    pub pending: Option<String>,
    /// Enter or blur kept rejected text.
    pub rejection_committed: bool,
    /// Counts each Enter or blur that kept rejected text.
    pub kept_commits: u64,
    /// Rejected text whose rejection stays shown while edits leave it wrong.
    pub kept_text: Option<String>,
    pub focused: bool,
    /// The date typing last reported, with the epoch it belongs to.
    pub last_fired: (Option<PlainDate>, u64),
    pub fired_epoch: u64,
    pub prev_value: Option<Option<PlainDate>>,
    pub was_disabled: bool,
    pub replaces: crate::field::entry_report::ValueReplacesEntry<Option<PlainDate>>,
}

impl TypedState {
    pub fn last_fired_date(&self) -> Option<PlainDate> {
        (self.last_fired.1 == self.fired_epoch)
            .then_some(self.last_fired.0)
            .flatten()
    }

    pub fn reset(&mut self) {
        self.pending = None;
        self.rejection_committed = false;
        self.kept_text = None;
    }
}

/// What a frame of the typing session shows.
pub(crate) struct Shown {
    pub text: String,
    pub result: DateParse,
}

/// The rejection the field shows this frame: committed or complete text's,
/// else kept text's while an edit leaves it wrong. A locked field hides it.
pub(crate) fn shown_rejection(
    state: &TypedState,
    typed: &TypedDate,
    locked: bool,
    behavior: EntryErrorBehavior,
) -> Option<Shown> {
    if locked {
        return None;
    }
    let pending = state.pending.as_deref()?;
    let result = (!pending.trim().is_empty()).then(|| typed.parse(pending))?;
    if typed_date_error(Some(&result), state.rejection_committed, behavior).is_some() {
        return Some(Shown {
            text: pending.to_string(),
            result,
        });
    }
    let kept = state.kept_text.as_deref()?;
    if is_accepted(&result) {
        return None;
    }
    let kept_result = typed.parse(kept);
    typed_date_error(Some(&kept_result), true, behavior).map(|_| Shown {
        text: kept.to_string(),
        result: kept_result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> PlainDate {
        PlainDate::new(y, m, day).unwrap()
    }

    fn typed(
        calendar: CalendarSystem,
        min: Option<PlainDate>,
        max: Option<PlainDate>,
    ) -> TypedDate {
        let is_disabled: IsDisabled =
            Rc::new(move |date| min.is_some_and(|m| date < m) || max.is_some_and(|m| date > m));
        TypedDate::new(
            calendar,
            DateInputFormat::default(),
            min,
            max,
            is_disabled,
            &astryx_i18n::I18n::new("en-US"),
        )
    }

    #[test]
    fn fr20_gregorian_text_only_the_fallback_reads_commits_at_enter() {
        let t = typed(CalendarSystem::Gregory, None, None);
        assert_eq!(
            t.parse("13/5/2026"),
            DateParse::Value {
                date: d(2026, 5, 13),
                is_complete: false
            }
        );
        assert!(matches!(
            t.parse("5/13/2026"),
            DateParse::Value {
                is_complete: true,
                ..
            }
        ));
    }

    #[test]
    fn ar3_each_rejection_has_its_own_message() {
        let t = typed(
            CalendarSystem::Gregory,
            Some(d(2026, 1, 1)),
            Some(d(2026, 12, 31)),
        );
        assert_eq!(
            t.error_message(&DateParse::Unreadable).as_ref(),
            "Invalid date"
        );
        assert_eq!(
            t.error_message(&DateParse::Nonexistent { is_complete: true })
                .as_ref(),
            "Enter a real date"
        );
        assert_eq!(
            t.error_message(&DateParse::OutOfRange {
                date: d(2027, 1, 1),
                is_complete: true
            })
            .as_ref(),
            "Enter a date from January 1, 2026 to December 31, 2026"
        );
        let persian = typed(CalendarSystem::Persian, None, None);
        assert_eq!(
            persian
                .error_message(&DateParse::Nonexistent { is_complete: true })
                .as_ref(),
            "Enter a real date (Persian Calendar)"
        );
        assert_eq!(
            persian.calendar_description().as_deref(),
            Some("Dates use the Persian Calendar")
        );
        assert_eq!(t.calendar_description(), None);
    }

    #[test]
    fn fr24_complete_dates_reject_at_once_only_under_complete() {
        let complete = DateParse::Nonexistent { is_complete: true };
        assert_eq!(
            typed_date_error(Some(&complete), false, EntryErrorBehavior::Complete),
            Some(TypedDateError::Nonexistent)
        );
        assert_eq!(
            typed_date_error(Some(&complete), false, EntryErrorBehavior::BlurOrEnter),
            None
        );
        assert_eq!(
            typed_date_error(
                Some(&DateParse::Unreadable),
                false,
                EntryErrorBehavior::Complete
            ),
            None
        );
        assert_eq!(
            typed_date_error(
                Some(&DateParse::Incomplete),
                true,
                EntryErrorBehavior::Complete
            ),
            Some(TypedDateError::Unreadable)
        );
    }
}
