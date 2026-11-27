//! The menu press machine, upstream `hooks/menuPressGesture.ts`: pure, so each transition is
//! tested without a window. Rows are opaque (`T`): the hook feeds in the enabled row under
//! the pointer, or `None` over a divider, a heading, a disabled row or outside the menu.
//!
//! The row under the release acts, and the highlight follows the held pointer, as in macOS
//! and iOS menus:
//! - `Idle`: no gesture is live.
//! - `TriggerPress`: a finger rests on the trigger and the menu is not open yet (a mouse
//!   press opens at once).
//! - `Open`: the menu opened under a held pointer, which does not pick yet.
//! - `Tracking`: the pointer is held and the highlight follows it.
//! - `Withdrawn` (internal): the window lost focus under a finger; tracked to its lift,
//!   nothing it does acts.
//!
//! A finger or a pen that opened the menu picks only after it travels
//! [`MENU_PRESS_HANDOFF_PX`] from the first position the menu sees, as UIMenu hands off.
//! Without positions the settle rule holds: the release of the opening gesture acts only
//! after the pointer entered the menu or the press lasted [`MENU_PRESS_SETTLE_MS`].

use rustryx_primitives::gestures::Point;

use super::values::MENU_PRESS_HANDOFF_PX as UIKIT_HANDOFF;

/// How long the opening press must last before its release may act, ms, unless a
/// finger's handoff applies.
pub const MENU_PRESS_SETTLE_MS: f64 = 300.;

/// How far a finger that opened the menu travels, from the first sample the menu sees,
/// before it picks (UIMenu's scrub handoff), px.
pub const MENU_PRESS_HANDOFF_PX: f64 = UIKIT_HANDOFF;

/// Upstream `MenuPressPointerType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuPressPointerType {
    Mouse,
    Touch,
    Pen,
}

impl MenuPressPointerType {
    /// A finger or a pen: UIKit's touch rules apply.
    pub fn is_finger(self) -> bool {
        self != Self::Mouse
    }
}

/// Where a press began.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuPressTarget {
    Trigger,
    Menu,
}

/// The machine's own state, with the finger's handoff and the withdrawn gesture.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MenuPressState<T> {
    Idle,
    TriggerPress {
        pointer: MenuPressPointerType,
        started_at: f64,
    },
    Open {
        pointer: MenuPressPointerType,
        opened_at: f64,
        /// The first position the menu saw after it opened.
        handoff_from: Option<Point>,
    },
    Tracking {
        pointer: MenuPressPointerType,
        origin: MenuPressTarget,
        row: Option<T>,
    },
    Withdrawn {
        pointer: MenuPressPointerType,
        /// Withdrawn while still a press on the trigger: its tap's click opens the menu.
        from_trigger: bool,
    },
}

impl<T> MenuPressState<T> {
    pub(crate) fn pointer(&self) -> Option<MenuPressPointerType> {
        match self {
            Self::Idle => None,
            Self::TriggerPress { pointer, .. }
            | Self::Open { pointer, .. }
            | Self::Tracking { pointer, .. }
            | Self::Withdrawn { pointer, .. } => Some(*pointer),
        }
    }

    pub(crate) fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }
}

/// One event of the machine.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MenuPressInput<T> {
    Down {
        target: MenuPressTarget,
        pointer: MenuPressPointerType,
        row: Option<T>,
        time: f64,
    },
    /// The menu opened while the trigger press is still held.
    Opened { time: f64 },
    Move {
        row: Option<T>,
        in_menu: bool,
        time: f64,
        /// Window position, for a finger's handoff distance.
        at: Option<Point>,
    },
    Up {
        row: Option<T>,
        in_menu: bool,
        on_trigger: bool,
        time: f64,
        at: Option<Point>,
    },
    /// The platform took the gesture: a cancel, a second pointer, a scroll.
    Cancel,
    /// The window lost focus mid-press.
    Withdraw,
}

/// What a step asks the hook to do.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MenuPressOutcome<T> {
    None,
    /// A mouse pressed the trigger: open the menu now, under the held pointer.
    Open,
    Highlight(T),
    Clear,
    Act(T),
    /// The gesture ended without a row acting: `stray` says a click for it may still come
    /// and must not act; `dismiss` closes the menu (a mouse released outside it); `clear` takes
    /// a finger's lit row off.
    Settle {
        stray: bool,
        dismiss: bool,
        clear: bool,
    },
    /// The gesture was withdrawn and is still live: stop what drives it.
    Withdraw {
        clear: bool,
    },
}

/// One step's answer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MenuPressTransition<T> {
    pub gesture: MenuPressState<T>,
    pub effect: MenuPressOutcome<T>,
}

fn idle<T>(effect: MenuPressOutcome<T>) -> MenuPressTransition<T> {
    MenuPressTransition {
        gesture: MenuPressState::Idle,
        effect,
    }
}

fn settle<T>(stray: bool, dismiss: bool) -> MenuPressOutcome<T> {
    MenuPressOutcome::Settle {
        stray,
        dismiss,
        clear: false,
    }
}

fn highlight<T>(row: Option<T>) -> MenuPressOutcome<T> {
    match row {
        Some(row) => MenuPressOutcome::Highlight(row),
        None => MenuPressOutcome::Clear,
    }
}

fn same<T>(gesture: MenuPressState<T>) -> MenuPressTransition<T> {
    MenuPressTransition {
        gesture,
        effect: MenuPressOutcome::None,
    }
}

/// A straight-line distance, as UIMenu measures it.
fn passed_handoff(from: Point, to: Point) -> bool {
    (to.x - from.x).hypot(to.y - from.y) >= MENU_PRESS_HANDOFF_PX
}

/// A release while the highlight tracks the pointer: over an enabled row it acts, over
/// the menu's chrome or the trigger it acts on nothing, outside under a mouse it dismisses.
fn release<T>(
    pointer: MenuPressPointerType,
    row: Option<T>,
    in_menu: bool,
    on_trigger: bool,
) -> MenuPressTransition<T> {
    if let Some(row) = row {
        return idle(MenuPressOutcome::Act(row));
    }
    if in_menu || on_trigger {
        return idle(settle(true, false));
    }
    idle(settle(true, pointer == MenuPressPointerType::Mouse))
}

fn withdraw<T>(
    pointer: MenuPressPointerType,
    lit: bool,
    from_trigger: bool,
) -> MenuPressTransition<T> {
    if !pointer.is_finger() {
        return idle(settle(false, false));
    }
    MenuPressTransition {
        gesture: MenuPressState::Withdrawn {
            pointer,
            from_trigger,
        },
        effect: MenuPressOutcome::Withdraw { clear: lit },
    }
}

/// Upstream `menuPressTransition`: advances the gesture by one event.
pub(crate) fn menu_press_transition<T: Clone + PartialEq>(
    gesture: MenuPressState<T>,
    event: MenuPressInput<T>,
) -> MenuPressTransition<T> {
    use MenuPressInput as I;
    use MenuPressState as S;
    match gesture {
        S::Idle => match event {
            I::Down {
                target: MenuPressTarget::Menu,
                pointer,
                row,
                ..
            } => MenuPressTransition {
                effect: highlight(row.clone()),
                gesture: S::Tracking {
                    pointer,
                    origin: MenuPressTarget::Menu,
                    row,
                },
            },
            I::Down {
                pointer: MenuPressPointerType::Mouse,
                time,
                ..
            } => MenuPressTransition {
                gesture: S::Open {
                    pointer: MenuPressPointerType::Mouse,
                    opened_at: time,
                    handoff_from: None,
                },
                effect: MenuPressOutcome::Open,
            },
            I::Down { pointer, time, .. } => same(S::TriggerPress {
                pointer,
                started_at: time,
            }),
            _ => same(S::Idle),
        },
        S::TriggerPress {
            pointer,
            started_at,
        } => match event {
            I::Opened { time } => same(S::Open {
                pointer,
                opened_at: time,
                handoff_from: None,
            }),
            // A tap: the trigger's click opens the menu, so it must pass.
            I::Up { .. } | I::Cancel => idle(settle(false, false)),
            I::Withdraw => withdraw(pointer, false, true),
            _ => same(S::TriggerPress {
                pointer,
                started_at,
            }),
        },
        S::Open {
            pointer,
            opened_at,
            handoff_from,
        } => match event {
            I::Move {
                row, in_menu, at, ..
            } => {
                if let (true, Some(at)) = (pointer.is_finger(), at) {
                    // UIMenu counts from the first sample after the open.
                    let Some(from) = handoff_from else {
                        return same(S::Open {
                            pointer,
                            opened_at,
                            handoff_from: Some(at),
                        });
                    };
                    if !passed_handoff(from, at) {
                        return same(S::Open {
                            pointer,
                            opened_at,
                            handoff_from,
                        });
                    }
                } else if !in_menu {
                    return same(S::Open {
                        pointer,
                        opened_at,
                        handoff_from,
                    });
                }
                MenuPressTransition {
                    effect: highlight(row.clone()),
                    gesture: S::Tracking {
                        pointer,
                        origin: MenuPressTarget::Trigger,
                        row,
                    },
                }
            }
            I::Up {
                row,
                in_menu,
                on_trigger,
                time,
                at,
            } => {
                // A finger that never travelled the handoff: the row under it appeared
                // under a press nobody moved. Nothing acts; the trigger's click is spent.
                if pointer.is_finger() && at.is_some() {
                    return idle(settle(true, false));
                }
                if time - opened_at < MENU_PRESS_SETTLE_MS {
                    return idle(settle(true, false));
                }
                release(pointer, row, in_menu, on_trigger)
            }
            I::Cancel => idle(settle(false, false)),
            I::Withdraw => withdraw(pointer, false, false),
            _ => same(S::Open {
                pointer,
                opened_at,
                handoff_from,
            }),
        },
        S::Tracking {
            pointer,
            origin,
            row,
        } => match event {
            I::Move { row: next, .. } => {
                if next == row {
                    return same(S::Tracking {
                        pointer,
                        origin,
                        row,
                    });
                }
                MenuPressTransition {
                    effect: highlight(next.clone()),
                    gesture: S::Tracking {
                        pointer,
                        origin,
                        row: next,
                    },
                }
            }
            I::Up {
                row: at,
                in_menu,
                on_trigger,
                ..
            } => release(pointer, at, in_menu, on_trigger),
            // A row a finger lit loses its highlight; a mouse's stays.
            I::Cancel => idle(MenuPressOutcome::Settle {
                stray: false,
                dismiss: false,
                clear: row.is_some() && pointer.is_finger(),
            }),
            I::Withdraw => withdraw(pointer, row.is_some(), false),
            _ => same(S::Tracking {
                pointer,
                origin,
                row,
            }),
        },
        S::Withdrawn {
            pointer,
            from_trigger,
        } => match event {
            // The withdrawn finger lifts: nothing acts, and its click is swallowed,
            // unless it was a tap on the trigger, whose click opens the menu.
            I::Up { .. } => idle(settle(!from_trigger, false)),
            I::Cancel => idle(settle(false, false)),
            _ => same(S::Withdrawn {
                pointer,
                from_trigger,
            }),
        },
    }
}

// --- The released public shape ----------------------------------------------

/// Upstream `MenuPressGesture`: where a press is, as released.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuPressGesture<T> {
    Idle,
    TriggerPress {
        pointer: MenuPressPointerType,
        started_at: f64,
    },
    Open {
        pointer: MenuPressPointerType,
        opened_at: f64,
    },
    Tracking {
        pointer: MenuPressPointerType,
        origin: MenuPressTarget,
        row: Option<T>,
    },
}

/// Upstream `MenuPressEvent`.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuPressEvent<T> {
    Down {
        target: MenuPressTarget,
        pointer: MenuPressPointerType,
        row: Option<T>,
        time: f64,
    },
    Opened {
        time: f64,
    },
    Move {
        row: Option<T>,
        in_menu: bool,
        time: f64,
    },
    Up {
        row: Option<T>,
        in_menu: bool,
        on_trigger: bool,
        time: f64,
    },
    Cancel,
}

/// Upstream `MenuPressEffect`.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuPressEffect<T> {
    None,
    Open,
    Highlight(T),
    Clear,
    Act(T),
    Settle { stray: bool, dismiss: bool },
}

/// Upstream `menuPressStep`: the machine as released, one step from a gesture and an
/// event to the next gesture and the effect. The settle rule holds: the released events
/// carry no positions.
pub fn menu_press_step<T: Clone + PartialEq>(
    gesture: MenuPressGesture<T>,
    event: MenuPressEvent<T>,
) -> (MenuPressGesture<T>, MenuPressEffect<T>) {
    let state = match gesture {
        MenuPressGesture::Idle => MenuPressState::Idle,
        MenuPressGesture::TriggerPress {
            pointer,
            started_at,
        } => MenuPressState::TriggerPress {
            pointer,
            started_at,
        },
        MenuPressGesture::Open { pointer, opened_at } => MenuPressState::Open {
            pointer,
            opened_at,
            handoff_from: None,
        },
        MenuPressGesture::Tracking {
            pointer,
            origin,
            row,
        } => MenuPressState::Tracking {
            pointer,
            origin,
            row,
        },
    };
    let input = match event {
        MenuPressEvent::Down {
            target,
            pointer,
            row,
            time,
        } => MenuPressInput::Down {
            target,
            pointer,
            row,
            time,
        },
        MenuPressEvent::Opened { time } => MenuPressInput::Opened { time },
        MenuPressEvent::Move { row, in_menu, time } => MenuPressInput::Move {
            row,
            in_menu,
            time,
            at: None,
        },
        MenuPressEvent::Up {
            row,
            in_menu,
            on_trigger,
            time,
        } => MenuPressInput::Up {
            row,
            in_menu,
            on_trigger,
            time,
            at: None,
        },
        MenuPressEvent::Cancel => MenuPressInput::Cancel,
    };
    let next = menu_press_transition(state, input);
    let gesture = match next.gesture {
        // Unreachable from the released events, which carry no withdraw.
        MenuPressState::Idle | MenuPressState::Withdrawn { .. } => MenuPressGesture::Idle,
        MenuPressState::TriggerPress {
            pointer,
            started_at,
        } => MenuPressGesture::TriggerPress {
            pointer,
            started_at,
        },
        MenuPressState::Open {
            pointer, opened_at, ..
        } => MenuPressGesture::Open { pointer, opened_at },
        MenuPressState::Tracking {
            pointer,
            origin,
            row,
        } => MenuPressGesture::Tracking {
            pointer,
            origin,
            row,
        },
    };
    let effect = match next.effect {
        MenuPressOutcome::None | MenuPressOutcome::Withdraw { .. } => MenuPressEffect::None,
        MenuPressOutcome::Open => MenuPressEffect::Open,
        MenuPressOutcome::Highlight(row) => MenuPressEffect::Highlight(row),
        MenuPressOutcome::Clear => MenuPressEffect::Clear,
        MenuPressOutcome::Act(row) => MenuPressEffect::Act(row),
        MenuPressOutcome::Settle { stray, dismiss, .. } => {
            MenuPressEffect::Settle { stray, dismiss }
        }
    };
    (gesture, effect)
}
