//! A typed date segment: the typing session of a date text field
//! (upstream `useTypedDateField`), shared by DateInput and DateTimeInput's
//! date segment. Each owner decides what a typed date and an emptied text
//! commit.

use std::rc::Rc;

use astryx_i18n::date::{DateParse, PlainDate};
use astryx_primitives::{Politeness, announce};
use gpui::{App, ElementId, Entity, SharedString, Window};

use super::typed::{
    DateInputEntryError, Shown, TypedDate, TypedState, is_accepted, shown_rejection,
};
use crate::field::entry_report::{
    CallerErrorPrecedence, EntryReport, ReportFn, ResolveInput, Resolved,
};
use crate::field::{EntryErrorBehavior, InputStatus};

/// A typed date to commit.
pub(crate) type DateFn = Rc<dyn Fn(PlainDate, &mut Window, &mut App)>;
/// Emptied text, committed.
pub(crate) type ClearFn = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Default)]
pub(crate) struct SegmentState {
    pub typed: TypedState,
    last_alert: Option<SharedString>,
}

/// One frame's date segment.
#[derive(Clone)]
pub(crate) struct DateSegment {
    pub state: Entity<SegmentState>,
    pub typed: TypedDate,
    /// The committed date.
    pub value: Option<PlainDate>,
    /// Disabled or busy: typing, Enter and blur do nothing.
    pub locked: bool,
    pub behavior: EntryErrorBehavior,
    pub report: EntryReport<DateInputEntryError>,
    on_date: DateFn,
    on_clear: ClearFn,
    /// Typing completed a new date (the open calendar shows its month).
    on_typed: Option<DateFn>,
}

/// What [`DateSegment::new`] needs.
pub(crate) struct SegmentProps {
    pub typed: TypedDate,
    pub value: Option<PlainDate>,
    pub locked: bool,
    pub behavior: EntryErrorBehavior,
    pub on_date: DateFn,
    pub on_clear: ClearFn,
    pub on_typed: Option<DateFn>,
}

impl DateSegment {
    pub fn new(id: &ElementId, props: SegmentProps, window: &mut Window, cx: &mut App) -> Self {
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id:?}-date-segment").into()),
            cx,
            |_, _| SegmentState::default(),
        );
        let report = EntryReport::<DateInputEntryError>::new(id, window, cx);
        let segment = Self {
            state,
            typed: props.typed,
            value: props.value,
            locked: props.locked,
            behavior: props.behavior,
            report,
            on_date: props.on_date,
            on_clear: props.on_clear,
            on_typed: props.on_typed,
        };
        segment.reconcile(cx);
        segment
    }

    /// A new value drops typed text and retires the reported date; a
    /// re-enabled field shows its value for text typing already reported
    /// (DateInput FR12, AST-118 FR6).
    fn reconcile(&self, cx: &mut App) {
        let (value, locked, typed) = (self.value, self.locked, self.typed.clone());
        self.state.update(cx, |s, _| {
            let t = &mut s.typed;
            let replaced = t.replaces.check(&value, t.kept_commits);
            if t.prev_value != Some(value) {
                let first = t.prev_value.is_none();
                t.prev_value = Some(value);
                if !first {
                    let retires = value != t.last_fired_date();
                    if retires {
                        t.fired_epoch += 1;
                    }
                    if t.pending.is_some() && replaced && (retires || value.is_none()) {
                        t.reset();
                    }
                }
            }
            if locked != t.was_disabled {
                t.was_disabled = locked;
                let reported = t.pending.as_deref().map(|p| typed.parse(p));
                if !locked
                    && !t.focused
                    && matches!(reported, Some(DateParse::Value { date, .. }) if Some(date) == t.last_fired_date())
                {
                    t.fired_epoch += 1;
                    t.reset();
                }
            }
        });
    }

    /// The rejection shown this frame.
    pub fn shown(&self, cx: &App) -> Option<Shown> {
        shown_rejection(
            &self.state.read(cx).typed,
            &self.typed,
            self.locked,
            self.behavior,
        )
    }

    fn current_error(&self, cx: &App) -> Option<DateInputEntryError> {
        self.shown(cx).and_then(|s| {
            self.typed
                .entry_error(&s.result, true, &s.text, self.behavior)
        })
    }

    /// The segment's status and alert this frame; the alert is announced
    /// when it changes.
    pub fn resolve(
        &self,
        status: Option<&InputStatus>,
        status_announced: bool,
        callback: Option<ReportFn<DateInputEntryError>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Resolved {
        let shown = self.shown(cx);
        let resolved = self.report.resolve(
            ResolveInput {
                entry_error: shown.as_ref().and_then(|s| {
                    self.typed
                        .entry_error(&s.result, true, &s.text, self.behavior)
                }),
                rejection_message: shown.as_ref().map(|s| self.typed.error_message(&s.result)),
                status,
                status_announced,
                precedence: CallerErrorPrecedence::WithCallback,
                callback,
            },
            window,
            cx,
        );
        let changed = self.state.update(cx, |s, _| {
            let changed = s.last_alert != resolved.alert;
            s.last_alert = resolved.alert.clone();
            changed
        });
        if changed && let Some(alert) = resolved.alert.clone() {
            window.defer(cx, move |window, cx| {
                announce(alert, Politeness::Assertive, window, cx)
            });
        }
        resolved
    }

    /// Typed text while editing, else the committed date written out.
    pub fn display(&self, shown_value: Option<PlainDate>, cx: &App) -> String {
        match &self.state.read(cx).typed.pending {
            Some(pending) => pending.clone(),
            None => shown_value
                .map(|v| self.typed.format_committed(v))
                .unwrap_or_default(),
        }
    }

    /// The placeholder: the calendar's format hint while focused and empty
    /// with no caller placeholder (AR9), else the caller's or the default.
    pub fn placeholder(
        &self,
        caller: Option<SharedString>,
        default: SharedString,
        display: &str,
        cx: &App,
    ) -> SharedString {
        let focused = self.state.read(cx).typed.focused;
        let hint = (caller.is_none() && focused && display.is_empty())
            .then(|| self.typed.format_hint())
            .flatten();
        hint.or(caller).unwrap_or(default)
    }

    pub fn set_focused(&self, focused: bool, cx: &mut App) {
        self.state.update(cx, |s, _| s.typed.focused = focused);
        if focused {
            self.typed.prepare();
        }
    }

    /// Drops typed text after a date is picked another way.
    pub fn reset(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| s.typed.reset());
        self.report.report(None, window, cx);
    }

    /// Reports a typed date once per typing session.
    fn report_date(&self, date: PlainDate, window: &mut Window, cx: &mut App) -> bool {
        let state = &self.state.read(cx).typed;
        if Some(date) == self.value || Some(date) == state.last_fired_date() {
            return false;
        }
        self.state.update(cx, |s, _| {
            s.typed.last_fired = (Some(date), s.typed.fired_epoch)
        });
        (self.on_date)(date, window, cx);
        true
    }

    /// Upstream `handleChange`: a date only once more typing cannot change it
    /// (AST-111 FR12).
    pub fn typed(&self, text: String, window: &mut Window, cx: &mut App) {
        if self.locked {
            return;
        }
        let result = self.typed.parse(&text);
        let empty = text.trim().is_empty();
        let error = (!empty)
            .then(|| self.typed.entry_error(&result, false, &text, self.behavior))
            .flatten();
        let shown_text = self.shown(cx).map(|s| s.text);
        let current = self.current_error(cx);
        let kept = if error.is_some() || empty || is_accepted(&result) {
            None
        } else {
            shown_text
        };
        let keeps = kept.is_some();
        self.state.update(cx, |s, _| {
            s.typed.pending = Some(text);
            s.typed.rejection_committed = false;
            s.typed.kept_text = kept;
        });
        self.report
            .report(error.or(if keeps { current } else { None }), window, cx);
        if let DateParse::Value {
            date,
            is_complete: true,
        } = result
            && self.report_date(date, window, cx)
            && let Some(f) = &self.on_typed
        {
            f(date, window, cx);
        }
        window.refresh();
    }

    /// Upstream `commit`: Enter or blur.
    pub fn commit(&self, window: &mut Window, cx: &mut App) {
        let Some(pending) = self.state.read(cx).typed.pending.clone() else {
            return;
        };
        if self.locked {
            return;
        }
        if pending.trim().is_empty() {
            (self.on_clear)(window, cx);
            self.reset(window, cx);
            window.refresh();
            return;
        }
        let result = self.typed.parse(&pending);
        if matches!(
            result,
            DateParse::Unreadable
                | DateParse::Incomplete
                | DateParse::Nonexistent { .. }
                | DateParse::OutOfRange { .. }
        ) {
            // Keep the text and say why it cannot commit; never finish or
            // adjust the date (AST-118 FR4).
            self.state.update(cx, |s, _| {
                s.typed.rejection_committed = true;
                s.typed.kept_commits += 1;
                s.typed.kept_text = None;
            });
            let error = self
                .typed
                .entry_error(&result, true, &pending, self.behavior);
            self.report.report(error, window, cx);
            window.refresh();
            return;
        }
        if let DateParse::Value { date, .. } = result {
            self.report_date(date, window, cx);
        }
        self.state
            .update(cx, |s, _| s.typed.last_fired = (None, s.typed.fired_epoch));
        self.reset(window, cx);
        window.refresh();
    }
}
