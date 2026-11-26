//! Upstream `hooks/menuPressGesture.test.ts` (one test per transition of the press model's
//! state diagram) and `hooks/menuPressValues.test.ts` (the UIKit value).

use astryx_primitives::gestures::Point;

use crate::gestures::menu_press::gesture::{
    MenuPressInput as I, MenuPressOutcome as O, MenuPressState as S, menu_press_transition,
};
use crate::gestures::menu_press::*;

type Row = &'static str;

fn run(events: &[I<Row>]) -> (S<Row>, Vec<O<Row>>) {
    let mut g = S::Idle;
    let mut effects = Vec::new();
    for e in events {
        let t = menu_press_transition(g, e.clone());
        g = t.gesture;
        effects.push(t.effect);
    }
    (g, effects)
}

fn last(events: &[I<Row>]) -> O<Row> {
    run(events).1.pop().unwrap()
}

use MenuPressPointerType::{Mouse, Pen, Touch};

fn down_in_menu(row: Option<Row>, pointer: MenuPressPointerType) -> I<Row> {
    I::Down {
        target: MenuPressTarget::Menu,
        pointer,
        row,
        time: 0.,
    }
}
fn down_on_trigger(pointer: MenuPressPointerType, time: f64) -> I<Row> {
    I::Down {
        target: MenuPressTarget::Trigger,
        pointer,
        row: None,
        time,
    }
}
fn mv(row: Option<Row>, in_menu: bool, time: f64) -> I<Row> {
    I::Move {
        row,
        in_menu,
        time,
        at: None,
    }
}
fn up(row: Option<Row>, in_menu: bool, on_trigger: bool, time: f64) -> I<Row> {
    I::Up {
        row,
        in_menu,
        on_trigger,
        time,
        at: None,
    }
}
fn up_row(row: Row) -> I<Row> {
    up(Some(row), true, false, 20.)
}
fn settle(stray: bool, dismiss: bool) -> O<Row> {
    O::Settle {
        stray,
        dismiss,
        clear: false,
    }
}

#[test]
fn the_handoff_distance_is_the_uikit_tables() {
    assert_eq!(MENU_PRESS_HANDOFF_PX, 10.);
    assert_eq!(
        values::MENU_PRESS_HANDOFF_PX,
        astryx_primitives::motion::uikit::gestures::menu_press::HANDOFF
    );
    assert_eq!(MENU_PRESS_TRIGGER_DRIFT_PX, 13.);
    assert_eq!(MENU_PRESS_SETTLE_MS, 300.);
}

// --- Inside the menu ---

#[test]
fn a_pointer_down_inside_the_menu_tracks_and_highlights_the_row_under_it() {
    let (g, e) = run(&[down_in_menu(Some("A"), Touch)]);
    assert!(matches!(g, S::Tracking { .. }));
    assert_eq!(e[0], O::Highlight("A"));
    let (g, e) = run(&[down_in_menu(None, Touch)]);
    assert!(matches!(g, S::Tracking { .. }));
    assert_eq!(e[0], O::Clear);
}

#[test]
fn tracking_moves_the_highlight_clears_it_over_a_divider_and_ignores_the_same_row() {
    assert_eq!(
        last(&[down_in_menu(Some("A"), Touch), mv(Some("B"), true, 10.)]),
        O::Highlight("B")
    );
    assert_eq!(
        last(&[down_in_menu(Some("A"), Touch), mv(None, true, 10.)]),
        O::Clear
    );
    assert_eq!(
        last(&[down_in_menu(Some("A"), Touch), mv(Some("A"), true, 10.)]),
        O::None
    );
}

#[test]
fn a_release_over_an_enabled_row_acts_on_that_row_not_the_one_pressed() {
    let (g, e) = run(&[
        down_in_menu(Some("A"), Touch),
        mv(Some("B"), true, 10.),
        up_row("B"),
    ]);
    assert_eq!(e.last(), Some(&O::Act("B")));
    assert_eq!(g, S::Idle);
    assert_eq!(
        last(&[down_in_menu(Some("A"), Touch), up_row("A")]),
        O::Act("A")
    );
}

#[test]
fn a_release_over_the_chrome_acts_on_nothing_and_keeps_the_menu() {
    assert_eq!(
        last(&[
            down_in_menu(Some("A"), Touch),
            mv(None, true, 10.),
            up(None, true, false, 20.)
        ]),
        settle(true, false)
    );
}

#[test]
fn a_mouse_released_outside_dismisses_and_a_finger_leaves_the_menu_open() {
    assert_eq!(
        last(&[
            down_in_menu(Some("A"), Mouse),
            mv(None, false, 10.),
            up(None, false, false, 20.)
        ]),
        settle(true, true)
    );
    assert_eq!(
        last(&[
            down_in_menu(Some("A"), Touch),
            mv(None, false, 10.),
            up(None, false, false, 20.)
        ]),
        settle(true, false)
    );
}

#[test]
fn a_cancel_acts_on_nothing_and_clears_a_fingers_lit_row_but_not_a_mouses() {
    let (g, e) = run(&[
        down_in_menu(Some("A"), Touch),
        mv(Some("B"), true, 10.),
        I::Cancel,
    ]);
    assert_eq!(g, S::Idle);
    assert_eq!(
        e.last(),
        Some(&O::Settle {
            stray: false,
            dismiss: false,
            clear: true
        })
    );
    assert_eq!(
        last(&[
            down_in_menu(Some("A"), Mouse),
            mv(Some("B"), true, 10.),
            I::Cancel
        ]),
        settle(false, false)
    );
    assert_eq!(
        last(&[
            down_in_menu(Some("A"), Touch),
            mv(None, true, 10.),
            I::Cancel
        ]),
        settle(false, false)
    );
}

#[test]
fn ignores_a_move_or_a_release_with_no_gesture_live() {
    assert_eq!(
        menu_press_transition(S::Idle, mv(Some("A"), true, 10.)).effect,
        O::None
    );
    assert_eq!(menu_press_transition(S::Idle, up_row("A")).effect, O::None);
}

// --- The trigger ---

#[test]
fn a_mouse_press_on_the_trigger_opens_at_once_and_a_finger_opens_nothing_yet() {
    let (g, e) = run(&[down_on_trigger(Mouse, 0.)]);
    assert!(matches!(g, S::Open { .. }));
    assert_eq!(e[0], O::Open);
    let (g, e) = run(&[down_on_trigger(Touch, 0.)]);
    assert!(matches!(g, S::TriggerPress { .. }));
    assert_eq!(e[0], O::None);
}

#[test]
fn a_finger_that_lifts_before_the_delay_is_a_tap_whose_click_opens_the_menu() {
    let (g, e) = run(&[down_on_trigger(Touch, 0.), up(None, false, true, 20.)]);
    assert_eq!(g, S::Idle);
    assert_eq!(e.last(), Some(&settle(false, false)));
}

#[test]
fn the_menu_opens_under_a_held_finger_and_a_cancel_before_ends_the_gesture() {
    let (g, e) = run(&[down_on_trigger(Touch, 0.), I::Opened { time: 500. }]);
    assert!(matches!(g, S::Open { .. }));
    assert_eq!(e.last(), Some(&O::None));
    let (g, e) = run(&[down_on_trigger(Touch, 0.), I::Cancel]);
    assert_eq!(g, S::Idle);
    assert_eq!(e.last(), Some(&settle(false, false)));
}

#[test]
fn open_tracks_once_the_pointer_enters_the_menu() {
    let (g, e) = run(&[down_on_trigger(Mouse, 0.), mv(Some("A"), true, 10.)]);
    assert_eq!(
        g,
        S::Tracking {
            pointer: Mouse,
            origin: MenuPressTarget::Trigger,
            row: Some("A")
        }
    );
    assert_eq!(e.last(), Some(&O::Highlight("A")));
    let (g, e) = run(&[down_on_trigger(Mouse, 0.), mv(None, false, 10.)]);
    assert!(matches!(g, S::Open { .. }));
    assert_eq!(e.last(), Some(&O::None));
}

#[test]
fn the_opening_release_acts_only_once_it_settled() {
    assert_eq!(
        last(&[
            down_on_trigger(Mouse, 0.),
            up(None, false, true, MENU_PRESS_SETTLE_MS - 1.)
        ]),
        settle(true, false)
    );
    assert_eq!(
        last(&[
            down_on_trigger(Mouse, 0.),
            mv(Some("B"), true, 50.),
            up(Some("B"), true, false, 60.)
        ]),
        O::Act("B")
    );
    assert_eq!(
        last(&[
            down_on_trigger(Mouse, 0.),
            up(Some("A"), true, false, MENU_PRESS_SETTLE_MS)
        ]),
        O::Act("A")
    );
    assert_eq!(
        last(&[
            down_on_trigger(Mouse, 0.),
            up(None, false, true, MENU_PRESS_SETTLE_MS)
        ]),
        settle(true, false)
    );
}

#[test]
fn a_settled_mouse_release_outside_dismisses_and_a_finger_leaves_the_menu_open() {
    assert_eq!(
        last(&[
            down_on_trigger(Mouse, 0.),
            up(None, false, false, MENU_PRESS_SETTLE_MS)
        ]),
        settle(true, true)
    );
    assert_eq!(
        last(&[
            down_on_trigger(Touch, 0.),
            I::Opened { time: 500. },
            up(None, false, false, 500. + MENU_PRESS_SETTLE_MS)
        ]),
        settle(true, false)
    );
}

#[test]
fn a_cancel_while_open_ends_the_gesture_and_keeps_the_menu() {
    let (g, e) = run(&[down_on_trigger(Mouse, 0.), I::Cancel]);
    assert_eq!(g, S::Idle);
    assert_eq!(e.last(), Some(&settle(false, false)));
}

// --- The handoff ---

fn held(pointer: MenuPressPointerType) -> Vec<I<Row>> {
    vec![down_on_trigger(pointer, 0.), I::Opened { time: 150. }]
}
fn move_to(x: f64, y: f64, row: Option<Row>) -> I<Row> {
    I::Move {
        row,
        in_menu: row.is_some(),
        time: 400.,
        at: Some(Point::new(x, y)),
    }
}
fn up_at(x: f64, y: f64, row: Option<Row>, time: f64) -> I<Row> {
    I::Up {
        row,
        in_menu: row.is_some(),
        on_trigger: false,
        time,
        at: Some(Point::new(x, y)),
    }
}
fn with(mut base: Vec<I<Row>>, more: &[I<Row>]) -> Vec<I<Row>> {
    base.extend_from_slice(more);
    base
}

#[test]
fn the_opening_finger_lifted_without_moving_picks_nothing_however_long_it_rested() {
    for time in [200., 600., 5000.] {
        let (g, e) = run(&with(held(Touch), &[up_at(200., 182., Some("A"), time)]));
        assert_eq!(g, S::Idle);
        assert_eq!(e.last(), Some(&settle(true, false)));
    }
}

#[test]
fn the_first_sample_after_the_open_is_where_the_distance_counts_from() {
    let (g, e) = run(&with(held(Touch), &[move_to(200., 189., Some("A"))]));
    assert!(matches!(g, S::Open { handoff_from: Some(p), .. } if p == Point::new(200., 189.)));
    assert_eq!(e.last(), Some(&O::None));
    let (g, _) = run(&with(
        held(Touch),
        &[
            move_to(200., 189., Some("A")),
            move_to(200., 189. + MENU_PRESS_HANDOFF_PX - 1., Some("A")),
        ],
    ));
    assert!(matches!(g, S::Open { .. }));
    let (g, e) = run(&with(
        held(Touch),
        &[
            move_to(200., 189., Some("A")),
            move_to(200., 189. + MENU_PRESS_HANDOFF_PX, Some("B")),
        ],
    ));
    assert_eq!(
        g,
        S::Tracking {
            pointer: Touch,
            origin: MenuPressTarget::Trigger,
            row: Some("B")
        }
    );
    assert_eq!(e.last(), Some(&O::Highlight("B")));
}

#[test]
fn counts_from_the_first_sample_not_where_the_finger_was_at_the_open() {
    let samples = [189.33, 196.67, 200.33];
    let phases: Vec<bool> = (0..samples.len())
        .map(|i| {
            let moves: Vec<I<Row>> = samples[..=i]
                .iter()
                .map(|y| move_to(200., *y, Some("A")))
                .collect();
            matches!(run(&with(held(Touch), &moves)).0, S::Tracking { .. })
        })
        .collect();
    assert_eq!(phases, [false, false, true]);
    let (g, e) = run(&with(
        held(Touch),
        &[
            move_to(200., 191., Some("A")),
            move_to(200., 194., Some("A")),
            up_at(200., 194., Some("A"), 600.),
        ],
    ));
    assert_eq!(g, S::Idle);
    assert_eq!(e.last(), Some(&settle(true, false)));
}

#[test]
fn measures_a_straight_line_distance_as_uimenu_does() {
    let tracks = |dx: f64, dy: f64| {
        matches!(
            run(&with(
                held(Touch),
                &[
                    move_to(100., 100., Some("A")),
                    move_to(100. + dx, 100. + dy, Some("A"))
                ]
            ))
            .0,
            S::Tracking { .. }
        )
    };
    assert!(!tracks(0., 9.));
    assert!(!tracks(7., 7.));
    assert!(tracks(0., 10.));
    assert!(tracks(10., 0.));
    assert!(tracks(9., 9.));
    assert!(tracks(0., -10.));
}

#[test]
fn after_the_handoff_the_lift_acts_on_the_row_under_it() {
    assert_eq!(
        last(&with(
            held(Touch),
            &[
                move_to(200., 189., Some("A")),
                move_to(200., 230., Some("A")),
                move_to(200., 280., Some("B")),
                up_at(200., 292., Some("B"), 600.)
            ]
        )),
        O::Act("B")
    );
    assert_eq!(
        last(&with(
            held(Touch),
            &[
                move_to(200., 189., Some("A")),
                move_to(200., 195., Some("A")),
                up_at(200., 195., Some("A"), 600.)
            ]
        )),
        settle(true, false)
    );
}

#[test]
fn a_pen_follows_the_finger_rules_and_a_mouse_keeps_the_settle_rule() {
    assert_eq!(
        last(&with(held(Pen), &[up_at(200., 182., Some("A"), 600.)])),
        settle(true, false)
    );
    let mut m = vec![down_on_trigger(Mouse, 0.)];
    m.push(I::Move {
        row: Some("A"),
        in_menu: true,
        time: 40.,
        at: Some(Point::new(200., 189.)),
    });
    let (g, e) = run(&m);
    assert!(matches!(g, S::Tracking { row: Some("A"), .. }));
    assert_eq!(e.last(), Some(&O::Highlight("A")));
    assert_eq!(
        last(&[
            down_on_trigger(Mouse, 0.),
            up_at(200., 182., Some("A"), MENU_PRESS_SETTLE_MS)
        ]),
        O::Act("A")
    );
}

// --- The window loses focus ---

#[test]
fn a_fingers_gesture_is_withdrawn_its_lit_row_clears_and_nothing_it_does_highlights() {
    let (g, e) = run(&[
        down_in_menu(Some("A"), Touch),
        I::Withdraw,
        mv(Some("B"), true, 10.),
    ]);
    assert_eq!(
        g,
        S::Withdrawn {
            pointer: Touch,
            from_trigger: false
        }
    );
    assert_eq!(e[1..], [O::Withdraw { clear: true }, O::None]);
}

#[test]
fn the_withdrawn_fingers_release_acts_on_nothing_and_a_cancel_ends_it_quietly() {
    let (g, e) = run(&[down_in_menu(Some("A"), Touch), I::Withdraw, up_row("B")]);
    assert_eq!(g, S::Idle);
    assert_eq!(e.last(), Some(&settle(true, false)));
    let (g, e) = run(&[down_in_menu(Some("A"), Touch), I::Withdraw, I::Cancel]);
    assert_eq!(g, S::Idle);
    assert_eq!(e.last(), Some(&settle(false, false)));
}

#[test]
fn a_finger_on_the_trigger_or_one_that_opened_the_menu_is_withdrawn_with_nothing_to_clear() {
    let (g, e) = run(&[down_on_trigger(Touch, 0.), I::Withdraw]);
    assert!(matches!(g, S::Withdrawn { .. }));
    assert_eq!(e.last(), Some(&O::Withdraw { clear: false }));
    let (g, e) = run(&[
        down_on_trigger(Pen, 0.),
        I::Opened { time: 500. },
        I::Withdraw,
    ]);
    assert!(matches!(g, S::Withdrawn { .. }));
    assert_eq!(e.last(), Some(&O::Withdraw { clear: false }));
    // A tap on the trigger the blur withdrew lets its click through.
    let (g, e) = run(&[
        down_on_trigger(Touch, 0.),
        I::Withdraw,
        up(None, false, true, 20.),
    ]);
    assert_eq!(g, S::Idle);
    assert_eq!(e.last(), Some(&settle(false, false)));
}

#[test]
fn a_mouses_gesture_ends_as_a_cancel_keeping_its_click() {
    let (g, e) = run(&[down_in_menu(Some("A"), Mouse), I::Withdraw]);
    assert_eq!(g, S::Idle);
    assert_eq!(e.last(), Some(&settle(false, false)));
}

// --- The released machine ---

#[test]
fn the_released_machine_keeps_the_settle_rule_and_its_shapes() {
    let (g, _) = menu_press_step::<Row>(
        MenuPressGesture::Idle,
        MenuPressEvent::Down {
            target: MenuPressTarget::Trigger,
            pointer: Touch,
            row: None,
            time: 0.,
        },
    );
    let (g, _) = menu_press_step(g, MenuPressEvent::Opened { time: 500. });
    assert_eq!(
        g,
        MenuPressGesture::Open {
            pointer: Touch,
            opened_at: 500.
        }
    );
    let (g, _) = menu_press_step(
        g,
        MenuPressEvent::Move {
            row: Some("B"),
            in_menu: true,
            time: 1000.,
        },
    );
    let (_, effect) = menu_press_step(
        g,
        MenuPressEvent::Up {
            row: Some("B"),
            in_menu: true,
            on_trigger: false,
            time: 1100.,
        },
    );
    assert_eq!(effect, MenuPressEffect::Act("B"));
    // No settle clear in the released shape.
    let (g, _) = menu_press_step(
        MenuPressGesture::Idle,
        MenuPressEvent::Down {
            target: MenuPressTarget::Menu,
            pointer: Touch,
            row: Some("A"),
            time: 0.,
        },
    );
    let (g, _) = menu_press_step(
        g,
        MenuPressEvent::Move {
            row: Some("B"),
            in_menu: true,
            time: 10.,
        },
    );
    let (_, effect) = menu_press_step(g, MenuPressEvent::Cancel);
    assert_eq!(
        effect,
        MenuPressEffect::Settle {
            stray: false,
            dismiss: false
        }
    );
}
