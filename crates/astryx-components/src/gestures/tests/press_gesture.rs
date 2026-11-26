//! Upstream `utils/pressGesture.test.ts`: the press state machine, one case per requirement of
//! the touch press model, in the model's order.

use astryx_primitives::motion::uikit::gestures::press as PRESS;

use crate::gestures::press_gesture::*;

fn run(events: &[PressEvent]) -> (Vec<PressPaint>, PressGesture) {
    let mut g = idle_press_gesture();
    let mut paints = Vec::new();
    for e in events {
        let s = press_gesture_step(g, *e);
        g = s.gesture;
        paints.push(s.paint);
    }
    (paints, g)
}

fn down() -> PressEvent {
    at(100., 100.)
}
fn at(x: f64, y: f64) -> PressEvent {
    PressEvent::Down {
        x,
        y,
        delayed: true,
    }
}
fn now(x: f64, y: f64) -> PressEvent {
    PressEvent::Down {
        x,
        y,
        delayed: false,
    }
}
fn mv(x: f64, y: f64) -> PressEvent {
    PressEvent::Move { x, y }
}

use PressEvent::{ButtonDown, Cancel, FadeEnd, FlashEnd, Onset, Up};
use PressPaint::{Fading, None as Bare, On};

#[test]
fn the_clocks_are_the_native_ones() {
    assert_eq!(PRESS_ONSET_MS, 150.);
    assert_eq!(PRESS_SLOP_PX, 10.);
    assert_eq!(PRESS_FADE_MS, 175.);
    assert_eq!(PRESS_FLASH_MS, 100.);
    // One definition each: the UIKit table's measured values.
    assert_eq!(PRESS_ONSET_MS, PRESS::CONTENT_TOUCH_DELAY);
    assert_eq!(PRESS_FLASH_MS, PRESS::QUICK_TAP_FLASH);
    assert_eq!(PRESS_SLOP_PX, PRESS::SLOP);
}

#[test]
fn the_fade_without_a_theme_is_the_exit_tokens_default() {
    let t = astryx_theme::generated::default_theme::LIGHT;
    assert_eq!(t.duration.exit.as_secs_f64() * 1000., PRESS_FADE_MS);
}

#[test]
fn nothing_paints_until_the_press_is_believed() {
    let (paints, g) = run(&[down()]);
    assert_eq!(paints, [Bare]);
    assert_eq!(g.phase, PressPhase::Armed);
    assert_eq!(
        press_gesture_step(idle_press_gesture(), down()).timer,
        Some(PressTimer::Onset)
    );
    // A fling: the scroller claims it inside the delay.
    assert_eq!(run(&[down(), mv(100., 90.), Cancel]).0, [Bare, Bare, Bare]);
}

#[test]
fn a_held_press_paints_instantly_and_completely() {
    let (paints, g) = run(&[down(), Onset]);
    assert_eq!(paints, [Bare, On]);
    assert_eq!(g.phase, PressPhase::Pressed);
}

#[test]
fn travel_cancels_and_a_shaky_hand_does_not() {
    let inside = PRESS_SLOP_PX - 1.;
    let (paints, g) = run(&[down(), Onset, mv(100., 100. + inside)]);
    assert_eq!(paints, [Bare, On, On]);
    assert_eq!(g.phase, PressPhase::Pressed);
    let beyond = PRESS_SLOP_PX + 1.;
    let (paints, g) = run(&[down(), Onset, mv(100., 100. + beyond)]);
    assert_eq!(paints.last(), Some(&Bare));
    assert_eq!(g.phase, PressPhase::Dead);
    let (paints, g) = run(&[down(), mv(100., 100. + beyond)]);
    assert_eq!(paints, [Bare, Bare]);
    assert_eq!(g.phase, PressPhase::Dead);
    // Travel counts from where the finger landed, not the last move.
    let mut creep = vec![down(), Onset];
    let mut y = 101.;
    while y <= 100. + PRESS_SLOP_PX + 2. {
        creep.push(mv(100., y));
        y += 1.;
    }
    assert_eq!(run(&creep).1.phase, PressPhase::Dead);
}

#[test]
fn the_scrollers_claim_cancels_with_no_fade_and_no_timer() {
    let (paints, g) = run(&[down(), Onset, Cancel]);
    assert_eq!(paints, [Bare, On, Bare]);
    assert_eq!(g.phase, PressPhase::Dead);
    let armed = press_gesture_step(idle_press_gesture(), down()).gesture;
    let pressed = press_gesture_step(armed, Onset).gesture;
    assert_eq!(press_gesture_step(pressed, Cancel).timer, None);
}

#[test]
fn a_cancelled_gesture_is_dead() {
    let (paints, g) = run(&[
        down(),
        Onset,
        mv(100., 200.),
        mv(100., 200.),
        mv(100., 201.),
    ]);
    assert_eq!(paints[2..], [Bare, Bare, Bare]);
    assert_eq!(g.phase, PressPhase::Dead);
    assert_eq!(
        run(&[down(), Onset, mv(100., 200.), mv(100., 100.)])
            .0
            .last(),
        Some(&Bare)
    );
    assert_eq!(run(&[down(), Onset, Cancel, Up]).0.last(), Some(&Bare));
    // A late onset after the scroll took it.
    assert_eq!(run(&[down(), Cancel, Onset]).0, [Bare, Bare, Bare]);
    // Only a new touch revives it.
    let (paints, g) = run(&[down(), Cancel, at(120., 300.), Onset]);
    assert_eq!(paints.last(), Some(&On));
    assert_eq!(g.phase, PressPhase::Pressed);
    // The lift ends it, so the next press is heard.
    assert_eq!(
        run(&[down(), Onset, mv(100., 300.), Up]).1.phase,
        PressPhase::Idle
    );
}

#[test]
fn a_press_after_a_scroll_paints_the_way_the_first_one_did() {
    let (paints, g) = run(&[down(), Onset, mv(100., 400.), Up, at(100., 420.), Onset]);
    assert_eq!(paints.last(), Some(&On));
    assert_eq!(g.phase, PressPhase::Pressed);
}

#[test]
fn the_paint_does_not_survive_its_gesture() {
    let (paints, g) = run(&[down(), Onset, Cancel, Up]);
    assert_eq!(paints.last(), Some(&Bare));
    assert_eq!(g.phase, PressPhase::Idle);
}

#[test]
fn a_tap_too_quick_for_the_delay_still_answers() {
    let (paints, g) = run(&[down(), Up]);
    assert_eq!(paints, [Bare, On]);
    assert_eq!(g.phase, PressPhase::Flash);
    let armed = press_gesture_step(idle_press_gesture(), down()).gesture;
    let flash = press_gesture_step(armed, Up);
    assert_eq!(flash.timer, Some(PressTimer::Flash));
    let fading = press_gesture_step(flash.gesture, FlashEnd);
    assert_eq!(fading.paint, Fading);
    assert_eq!(fading.timer, Some(PressTimer::Fade));
    // Not clipped by the cancel a platform sends after the tap.
    assert_eq!(run(&[down(), Up, Cancel]).0.last(), Some(&On));
}

#[test]
fn the_release_fades() {
    let (paints, g) = run(&[down(), Onset, Up, FadeEnd]);
    assert_eq!(paints, [Bare, On, Fading, Bare]);
    assert_eq!(g.phase, PressPhase::Idle);
    let armed = press_gesture_step(idle_press_gesture(), down()).gesture;
    let pressed = press_gesture_step(armed, Onset).gesture;
    assert_eq!(
        press_gesture_step(pressed, Up).timer,
        Some(PressTimer::Fade)
    );
}

#[test]
fn on_a_touch_first_device_a_mouse_button_or_the_space_key_presses_at_once() {
    let step = press_gesture_step(idle_press_gesture(), ButtonDown);
    assert_eq!(step.paint, On);
    assert_eq!(step.timer, None);
    assert_eq!(step.gesture.phase, PressPhase::ButtonHeld);
    let (paints, g) = run(&[
        ButtonDown,
        mv(100. + PRESS_SLOP_PX * 10., 100.),
        mv(-500., -500.),
        Onset,
    ]);
    assert_eq!(paints, [On, On, On, On]);
    assert_eq!(g.phase, PressPhase::ButtonHeld);
    let release = press_gesture_step(step.gesture, Up);
    assert_eq!(release.paint, Fading);
    assert_eq!(release.timer, Some(PressTimer::Fade));
    let (paints, g) = run(&[ButtonDown, Up, FadeEnd]);
    assert_eq!(paints, [On, Fading, Bare]);
    assert_eq!(g.phase, PressPhase::Idle);
    let (paints, g) = run(&[ButtonDown, Cancel]);
    assert_eq!(paints, [On, Bare]);
    assert_eq!(g.phase, PressPhase::Dead);
    assert_eq!(run(&[ButtonDown, Up, ButtonDown]).0, [On, Fading, On]);
}

#[test]
fn a_control_with_no_scroller_above_it_does_not_wait() {
    let step = press_gesture_step(idle_press_gesture(), now(100., 100.));
    assert_eq!(step.paint, On);
    assert_eq!(step.gesture.phase, PressPhase::Pressed);
    assert_eq!(step.timer, None);
    assert_eq!(
        run(&[now(100., 100.), mv(100., 100. + PRESS_SLOP_PX + 1.)]).0,
        [On, Bare]
    );
    assert_eq!(run(&[now(100., 100.), Cancel]).0, [On, Bare]);
    let (paints, g) = run(&[now(100., 100.), Up]);
    assert_eq!(paints, [On, Fading]);
    assert_eq!(g.phase, PressPhase::Fading);
    assert_eq!(press_gesture_step(idle_press_gesture(), down()).paint, Bare);
}
