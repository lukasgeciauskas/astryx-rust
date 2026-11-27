//! Typed times, upstream `TimeInput/useTypedTime.ts`: the draft of a time
//! field and its rejection, shared by TimeInput and DateTimeInput's time
//! segment (AST-117, AST-118).

use gpui::SharedString;
use rustryx_i18n::date::{
    HourFormat, PlainTime, is_locale_time_complete, is_time_in_range, parse_locale_time,
};

use crate::field::EntryErrorBehavior;
use crate::field::entry_report::ValueReplacesEntry;

/// Upstream `TimeInputEntryError` `kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeEntryErrorKind {
    /// Text that is not a time.
    Unreadable,
    /// A time before `min` or after `max`.
    OutOfRange,
}

/// Upstream `TimeInputEntryError`: why the field rejects the typed text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TimeInputEntryError {
    pub kind: TimeEntryErrorKind,
    /// The typed text.
    pub text: SharedString,
    /// The time the text names, for `OutOfRange`.
    pub value: Option<PlainTime>,
}

/// What a draft reads as.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Draft {
    /// The time the text names, when the field accepts it.
    pub time: Option<PlainTime>,
    pub rejection: Option<TimeInputEntryError>,
    /// Under `Complete`, a complete rejected time shows while typed.
    pub complete: bool,
}

/// What reads a draft: the locale, the hour format, seconds and bounds.
#[derive(Clone, Debug)]
pub(crate) struct TimeRules {
    pub locale: SharedString,
    pub hour_format: HourFormat,
    pub seconds: bool,
    pub min: Option<PlainTime>,
    pub max: Option<PlainTime>,
}

impl TimeRules {
    /// Upstream `readDraft`.
    pub fn read(&self, text: &str) -> Draft {
        if text.trim().is_empty() {
            return Draft::default();
        }
        let shared: SharedString = text.to_string().into();
        let Some(time) = parse_locale_time(text, &self.locale, self.seconds) else {
            return Draft {
                time: None,
                rejection: Some(TimeInputEntryError {
                    kind: TimeEntryErrorKind::Unreadable,
                    text: shared,
                    value: None,
                }),
                complete: false,
            };
        };
        if !is_time_in_range(time, self.min, self.max) {
            return Draft {
                time: None,
                rejection: Some(TimeInputEntryError {
                    kind: TimeEntryErrorKind::OutOfRange,
                    text: shared,
                    value: Some(time),
                }),
                complete: is_locale_time_complete(
                    text,
                    &self.locale,
                    self.hour_format,
                    self.seconds,
                ),
            };
        }
        Draft {
            time: Some(time),
            rejection: None,
            complete: true,
        }
    }
}

/// How a draft was last committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    Enter,
    Blur,
}

/// A native control's rejection: none, waiting for a commit, or shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SurfaceRejection {
    #[default]
    None,
    Held,
    Shown,
}

/// A time field's typing state between frames (`useTypedTime`'s state).
#[derive(Clone, Debug, Default)]
pub(crate) struct TypedTimeState {
    pub pending: Option<String>,
    pub committed_by: Option<Trigger>,
    pub kept_commits: u64,
    pub kept_rejection: Option<TimeInputEntryError>,
    pub surface: SurfaceRejection,
    /// The time typing last committed, so its echo through `value` keeps
    /// the draft.
    pub last_committed: Option<Option<PlainTime>>,
    pub was_disabled: bool,
    pub had_typed_field: Option<bool>,
    pub replaces: ValueReplacesEntry<Option<PlainTime>>,
}

impl TypedTimeState {
    pub fn drop_draft(&mut self) {
        self.pending = None;
        self.committed_by = None;
        self.kept_rejection = None;
    }

    /// Reconciles the draft with this frame's props: a value from outside,
    /// disabling, and leaving the typed surface drop it; the echo of a time
    /// typing committed keeps it.
    pub fn reconcile(&mut self, value: Option<PlainTime>, disabled: bool, typed_field: bool) {
        let new_value = self.replaces.check(&value, self.kept_commits);
        let echo = self.last_committed == Some(value);
        if new_value {
            self.surface = SurfaceRejection::None;
        }
        if new_value && !echo {
            self.last_committed = None;
            if self.pending.is_some() {
                self.drop_draft();
            }
        }
        if disabled != self.was_disabled {
            self.was_disabled = disabled;
            if disabled {
                self.drop_draft();
                self.surface = SurfaceRejection::None;
            }
        }
        if self.had_typed_field != Some(typed_field) {
            if self.had_typed_field.is_some() {
                self.surface = SurfaceRejection::None;
            }
            self.had_typed_field = Some(typed_field);
        }
        if !typed_field && self.pending.is_some() {
            self.drop_draft();
        }
    }

    /// The rejection to report and whether one is revealed, this frame. Also
    /// keeps a revealed rejection through edits that leave the draft wrong.
    pub fn resolve(
        &mut self,
        rules: &TimeRules,
        locked: bool,
        behavior: EntryErrorBehavior,
    ) -> (Option<TimeInputEntryError>, bool) {
        let reveals_complete = behavior == EntryErrorBehavior::Complete;
        let draft = self
            .pending
            .as_deref()
            .map(|p| rules.read(p))
            .unwrap_or_default();
        let fresh = draft
            .rejection
            .clone()
            .filter(|_| self.committed_by.is_some() || (reveals_complete && draft.complete));
        match (&draft.rejection, &fresh) {
            (None, _) => self.kept_rejection = None,
            (Some(_), Some(fresh)) if self.kept_rejection.as_ref() != Some(fresh) => {
                self.kept_rejection = Some(fresh.clone());
            }
            _ => {}
        }
        let entry_error = if locked || draft.rejection.is_none() {
            None
        } else {
            fresh.or(self.kept_rejection.clone())
        };
        let surface_shown = self.surface_shown(locked, behavior);
        let rejected = entry_error.is_some() || surface_shown;
        (entry_error, rejected)
    }

    /// Upstream `setSurfaceRejected`: the native picker chose a time outside
    /// the bounds, or one inside them. Under `Complete` it shows at once.
    pub fn set_surface_rejected(&mut self, rejected: bool, behavior: EntryErrorBehavior) {
        self.surface = if !rejected {
            SurfaceRejection::None
        } else if behavior == EntryErrorBehavior::Complete
            || self.surface == SurfaceRejection::Shown
        {
            SurfaceRejection::Shown
        } else {
            SurfaceRejection::Held
        };
    }

    /// Upstream `commitSurface`: a held native rejection shows at Enter or
    /// blur.
    pub fn commit_surface(&mut self) {
        if self.surface == SurfaceRejection::Held {
            self.surface = SurfaceRejection::Shown;
        }
    }

    /// A native control's rejection shows.
    pub fn surface_shown(&self, locked: bool, behavior: EntryErrorBehavior) -> bool {
        !locked
            && (self.surface == SurfaceRejection::Shown
                || (behavior == EntryErrorBehavior::Complete
                    && self.surface == SurfaceRejection::Held))
    }

    /// Upstream `handleChange`: the time to commit, if the draft names an
    /// accepted one other than `value`, and the rejection to report.
    pub fn change(
        &mut self,
        text: String,
        rules: &TimeRules,
        value: Option<PlainTime>,
        behavior: EntryErrorBehavior,
    ) -> (Option<PlainTime>, Option<Option<TimeInputEntryError>>) {
        let next = rules.read(&text);
        self.pending = Some(text);
        self.committed_by = None;
        let report = (next.rejection.is_none()
            || (behavior == EntryErrorBehavior::Complete && next.complete))
            .then(|| next.rejection.clone());
        match next.time {
            Some(time) if Some(time) != value => {
                self.last_committed = Some(Some(time));
                (Some(time), report)
            }
            _ => (None, report),
        }
    }

    /// Upstream `commit`: `Some(Some(time))` or `Some(None)` (emptied) to
    /// commit, `None` for nothing; and the rejection to report.
    pub fn commit(
        &mut self,
        trigger: Trigger,
        rules: &TimeRules,
        value: Option<PlainTime>,
    ) -> (
        Option<Option<PlainTime>>,
        Option<Option<TimeInputEntryError>>,
    ) {
        let Some(pending) = self.pending.clone() else {
            return (None, None);
        };
        let current = rules.read(&pending);
        if let Some(rejection) = current.rejection {
            self.committed_by = Some(if self.committed_by == Some(Trigger::Blur) {
                Trigger::Blur
            } else {
                trigger
            });
            self.kept_commits += 1;
            return (None, Some(Some(rejection)));
        }
        let mut report = None;
        if trigger == Trigger::Blur {
            self.drop_draft();
            report = Some(None);
        }
        if current.time == value {
            return (None, report);
        }
        self.last_committed = Some(current.time);
        (Some(current.time), report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u8, m: u8) -> PlainTime {
        PlainTime::new(h, m, 0).unwrap()
    }

    fn rules() -> TimeRules {
        TimeRules {
            locale: "en-US".into(),
            hour_format: HourFormat::H12,
            seconds: false,
            min: Some(t(9, 0)),
            max: Some(t(17, 0)),
        }
    }

    #[test]
    fn fr3_a_parsed_in_range_draft_commits_while_typing() {
        let mut s = TypedTimeState::default();
        let (time, report) = s.change(
            "2:30 PM".into(),
            &rules(),
            None,
            EntryErrorBehavior::Complete,
        );
        assert_eq!(time, Some(t(14, 30)));
        assert_eq!(report, Some(None));
    }

    #[test]
    fn fr14_a_complete_out_of_range_time_shows_at_once() {
        let mut s = TypedTimeState::default();
        let (time, report) = s.change(
            "8:00 PM".into(),
            &rules(),
            None,
            EntryErrorBehavior::Complete,
        );
        assert_eq!(time, None);
        assert_eq!(
            report.flatten().map(|e| e.kind),
            Some(TimeEntryErrorKind::OutOfRange)
        );
        let (_, report) = s.change(
            "8:00 PM".into(),
            &rules(),
            None,
            EntryErrorBehavior::BlurOrEnter,
        );
        assert_eq!(report, None, "blur-or-enter waits");
    }

    #[test]
    fn fr14_blur_keeps_an_unreadable_draft() {
        let mut s = TypedTimeState::default();
        s.change("lunch".into(), &rules(), None, EntryErrorBehavior::Complete);
        let (commit, report) = s.commit(Trigger::Blur, &rules(), None);
        assert_eq!(commit, None);
        assert_eq!(
            report.flatten().map(|e| e.kind),
            Some(TimeEntryErrorKind::Unreadable)
        );
        assert_eq!(s.pending.as_deref(), Some("lunch"));
        let (error, rejected) = s.resolve(&rules(), false, EntryErrorBehavior::Complete);
        assert!(rejected && error.is_some());
    }

    #[test]
    fn fr3_blur_empties_to_none() {
        let mut s = TypedTimeState::default();
        s.change(
            String::new(),
            &rules(),
            Some(t(10, 0)),
            EntryErrorBehavior::Complete,
        );
        let (commit, _) = s.commit(Trigger::Blur, &rules(), Some(t(10, 0)));
        assert_eq!(commit, Some(None));
    }
}
