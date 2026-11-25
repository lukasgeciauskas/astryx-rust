//! The status, alert and caller report of a field that rejects typed text,
//! upstream `Field/useEntryErrorReport.ts` and `useValueReplacesEntry.ts`
//! (AST-117, AST-118). DateInput, TimeInput and DateTimeInput share it; each
//! keeps its own rules for when text is rejected and what the message says.

use std::rc::Rc;

use gpui::{App, ElementId, Entity, SharedString, Window};

use super::{InputStatus, StatusType};

/// A rejection a field reports to its caller.
pub(crate) trait EntryError: Clone + PartialEq + 'static {}

impl<T: Clone + PartialEq + 'static> EntryError for T {}

/// A caller's `onEntryErrorChange`.
pub(crate) type ReportFn<E> = Rc<dyn Fn(Option<E>, &mut Window, &mut App)>;

/// When a caller `error` status with a message shows in place of the
/// field's own message.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallerErrorPrecedence {
    Always,
    WithCallback,
}

pub(crate) struct ReportState<E> {
    /// The rejection the caller last received.
    heard: Option<E>,
    /// The rejection the callback was last called with.
    reported: Option<E>,
    /// The latest callback, for the report when the field goes away.
    callback: Option<ReportFn<E>>,
    /// The error message the status showed while no rejection stood, and
    /// whether the alert may repeat it.
    record: (Option<SharedString>, bool),
    recorded_input: Option<(bool, Option<SharedString>)>,
}

/// One field's entry-error report, kept between frames.
#[derive(Clone)]
pub(crate) struct EntryReport<E: EntryError> {
    state: Entity<ReportState<E>>,
}

/// What a frame reports: its rejection and message, the caller's status
/// and callback.
pub(crate) struct ResolveInput<'a, E> {
    pub entry_error: Option<E>,
    pub rejection_message: Option<SharedString>,
    pub status: Option<&'a InputStatus>,
    /// Whether the shown status announces itself (a Field shows it).
    pub status_announced: bool,
    pub precedence: CallerErrorPrecedence,
    pub callback: Option<ReportFn<E>>,
}

/// What a frame's report resolves to.
pub(crate) struct Resolved {
    /// The status the field shows.
    pub status: Option<InputStatus>,
    /// The message the field's alert announces, which no shown status does.
    pub alert: Option<SharedString>,
}

impl<E: EntryError> EntryReport<E> {
    pub fn new(id: &ElementId, window: &mut Window, cx: &mut App) -> Self {
        let handle = window.window_handle();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id:?}-entry-report").into()),
            cx,
            move |_, cx| {
                // The rejection ends with the field (FR20): a field that goes
                // away while it stands reports `None`.
                cx.on_release(move |state: &mut ReportState<E>, cx| {
                    if state.reported.is_some()
                        && let Some(callback) = state.callback.clone()
                    {
                        cx.defer(move |cx| {
                            handle
                                .update(cx, |_, window, cx| callback(None, window, cx))
                                .ok();
                        });
                    }
                })
                .detach();
                ReportState {
                    heard: None,
                    reported: None,
                    callback: None,
                    record: (None, true),
                    recorded_input: None,
                }
            },
        );
        Self { state }
    }

    /// Reports a rejection a handler causes, in the same update.
    pub fn report(&self, error: Option<E>, window: &mut Window, cx: &mut App) {
        let callback = self.state.read(cx).callback.clone();
        let Some(callback) = callback else {
            return;
        };
        let changed = self.state.update(cx, |s, _| {
            s.heard = error.clone();
            let changed = s.reported != error;
            if changed {
                s.reported = error.clone();
            }
            changed
        });
        if changed {
            callback(error, window, cx);
        }
    }

    /// This frame's status and alert. A rejection a prop change caused
    /// reaches the caller after the frame; until then the caller's status
    /// stays, so the field never flashes its own message first.
    pub fn resolve(
        &self,
        input: ResolveInput<'_, E>,
        window: &mut Window,
        cx: &mut App,
    ) -> Resolved {
        let ResolveInput {
            entry_error,
            rejection_message,
            status,
            status_announced,
            precedence,
            callback,
        } = input;
        let has_callback = callback.is_some();
        self.state.update(cx, |s, _| s.callback = callback.clone());
        let (heard, reported) = {
            let s = self.state.read(cx);
            (s.heard.clone(), s.reported.clone())
        };
        let caller_behind = has_callback && entry_error != heard;
        let caller_message = (precedence == CallerErrorPrecedence::Always || has_callback)
            .then(|| match status {
                Some(InputStatus {
                    status: StatusType::Error,
                    message: Some(message),
                }) => Some(message.clone()),
                _ => None,
            })
            .flatten();
        let field_status =
            if rejection_message.is_none() || caller_behind || caller_message.is_some() {
                status.cloned()
            } else {
                rejection_message.clone().map(InputStatus::error)
            };

        // A rejection that starts under the caller's unchanged error message
        // changes no status, so the alert announces that message once.
        let shown_error = match &field_status {
            Some(InputStatus {
                status: StatusType::Error,
                message,
            }) => message.clone(),
            _ => None,
        };
        let input = (rejection_message.is_some(), shown_error.clone());
        let record = self.state.update(cx, |s, _| {
            if has_callback && s.recorded_input.as_ref() != Some(&input) {
                s.recorded_input = Some(input.clone());
                if rejection_message.is_none() {
                    s.record = (shown_error.clone(), true);
                } else if s.record.1 && s.record.0 != shown_error {
                    s.record.1 = false;
                }
            }
            s.record.clone()
        });

        let mut alert = None;
        if let Some(message) = &rejection_message
            && !caller_behind
        {
            match &caller_message {
                None => alert = (!status_announced).then(|| message.clone()),
                Some(caller) if has_callback => {
                    let repeated = record.1 && record.0.as_ref() == Some(caller);
                    alert = (!status_announced || repeated).then(|| caller.clone());
                }
                Some(_) => alert = Some(message.clone()),
            }
        }

        // Syncs the caller with a rejection a prop change starts, changes or
        // ends.
        if has_callback && (entry_error != reported || entry_error != heard) {
            let state = self.state.clone();
            window.defer(cx, move |window, cx| {
                let (changed, callback) = state.update(cx, |s, _| {
                    let changed = s.reported != entry_error;
                    if changed {
                        s.reported = entry_error.clone();
                    }
                    s.heard = entry_error.clone();
                    (changed, s.callback.clone())
                });
                if changed && let Some(callback) = callback {
                    callback(entry_error, window, cx);
                }
                window.refresh();
            });
        }
        Resolved {
            status: field_status,
            alert,
        }
    }
}

/// Whether a new `value` replaces a field's kept entry this frame (upstream
/// `useValueReplacesEntry`): a value that arrives with the Enter or blur
/// that kept rejected text does not, so a caller can restore its own value.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ValueReplacesEntry<V> {
    rendered: Option<(V, u64)>,
}

impl<V: Clone + PartialEq> ValueReplacesEntry<V> {
    pub fn check(&mut self, value: &V, keep_signal: u64) -> bool {
        let Some((last_value, last_keep)) = &self.rendered else {
            self.rendered = Some((value.clone(), keep_signal));
            return false;
        };
        let new_value = last_value != value;
        let kept_now = *last_keep != keep_signal;
        if !new_value && !kept_now {
            return false;
        }
        self.rendered = Some((value.clone(), keep_signal));
        new_value && !kept_now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_with_a_kept_commit_does_not_replace_the_entry() {
        let mut replaces = ValueReplacesEntry::default();
        assert!(!replaces.check(&1, 0));
        assert!(replaces.check(&2, 0), "a new value");
        assert!(!replaces.check(&2, 0), "once per value");
        assert!(!replaces.check(&3, 1), "the same update as a kept commit");
        assert!(!replaces.check(&3, 2), "a kept commit alone");
        assert!(replaces.check(&4, 2));
    }
}
