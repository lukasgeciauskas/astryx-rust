//! The refresh's state machine (upstream lab `RefreshControl/refreshModel.ts`):
//! UIRefreshControl's rules as a pure reducer that returns the effects to run.

// The rules, as UIKit runs them:
// - A pull triggers once, with the finger down, when its overscroll reaches
//   the trigger distance. Only overscroll that a finger drives triggers, never
//   a momentum overshoot.
// - A release settles onto the refreshing inset while refreshing, and back to
//   the top otherwise. A cancelled pull settles the same way.
// - A programmatic begin puts the control in the refreshing state. It calls
//   the refresh handler only when it stands for the user's request, not when
//   the app reports a refresh that it started.
// - Ending a refresh that is not running does nothing.
//
// One refresh runs at a time: a pull while refreshing only stretches.

/// The refresh's state (upstream `RefreshState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshState {
    /// A refresh is in progress (`UIRefreshControl.isRefreshing`).
    pub is_refreshing: bool,
    /// A finger is pulling the content.
    pub is_pulling: bool,
    /// The current pull has triggered (its mapping turns one to one).
    pub has_triggered: bool,
}

/// Upstream `INITIAL_REFRESH_STATE`: at rest, nothing running.
pub const INITIAL_REFRESH_STATE: RefreshState = RefreshState {
    is_refreshing: false,
    is_pulling: false,
    has_triggered: false,
};

/// Who asked for a programmatic begin (upstream `source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeginSource {
    /// The user's request (`refresh()`): the handler runs.
    User,
    /// The app reports a refresh it started (a controlled `isRefreshing`).
    Program,
}

/// Where a release settles (upstream `'inset' | 'top'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettleTarget {
    /// Onto the refreshing inset.
    Inset,
    /// Back to the top.
    Top,
}

/// What happens to the refresh (upstream `RefreshEvent`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RefreshEvent {
    /// A finger starts pulling.
    PullStart,
    /// The pull moved to `overscroll`; `trigger` is the trigger distance for this
    /// scroll owner.
    PullMove { overscroll: f64, trigger: f64 },
    /// The finger lifted.
    PullEnd,
    /// The pull was taken away.
    PullCancel,
    /// A refresh starts without a pull.
    Begin(BeginSource),
    /// The refresh ends.
    End,
}

/// What the refresh asks the touch layer and the wiring to do (upstream
/// `RefreshEffect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshEffect {
    /// The pull reached the trigger: spin, announce, call the handler.
    Trigger,
    /// A refresh started without a pull; `call_handler` for the user's request.
    Begin { call_handler: bool },
    /// The finger let go: settle onto the inset or back to the top.
    Settle(SettleTarget),
    /// The refresh ended: remove the inset and hide the indicator.
    End,
}

/// Upstream `reduceRefresh`: the next state and the effects to run, in order.
pub fn reduce_refresh(
    state: RefreshState,
    event: RefreshEvent,
) -> (RefreshState, Vec<RefreshEffect>) {
    match event {
        RefreshEvent::PullStart => (
            RefreshState {
                is_pulling: true,
                has_triggered: false,
                ..state
            },
            Vec::new(),
        ),
        RefreshEvent::PullMove {
            overscroll,
            trigger,
        } => {
            // One refresh at a time: a pull while refreshing only stretches.
            if !state.is_pulling
                || state.has_triggered
                || state.is_refreshing
                || overscroll < trigger
            {
                return (state, Vec::new());
            }
            (
                RefreshState {
                    is_refreshing: true,
                    has_triggered: true,
                    ..state
                },
                vec![RefreshEffect::Trigger],
            )
        }
        RefreshEvent::PullEnd | RefreshEvent::PullCancel => {
            if !state.is_pulling {
                return (state, Vec::new());
            }
            let to = if state.is_refreshing {
                SettleTarget::Inset
            } else {
                SettleTarget::Top
            };
            (
                RefreshState {
                    is_pulling: false,
                    ..state
                },
                vec![RefreshEffect::Settle(to)],
            )
        }
        RefreshEvent::Begin(source) => {
            if state.is_refreshing {
                return (state, Vec::new());
            }
            (
                RefreshState {
                    is_refreshing: true,
                    ..state
                },
                vec![RefreshEffect::Begin {
                    call_handler: source == BeginSource::User,
                }],
            )
        }
        RefreshEvent::End => {
            if !state.is_refreshing {
                return (state, Vec::new());
            }
            (
                RefreshState {
                    is_refreshing: false,
                    ..state
                },
                vec![RefreshEffect::End],
            )
        }
    }
}
