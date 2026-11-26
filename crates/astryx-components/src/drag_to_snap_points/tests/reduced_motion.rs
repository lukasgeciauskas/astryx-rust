//! Upstream `createDragToSnapPoints.reducedMotion.test.ts` (design:reduced-motion DR2,
//! DR3): with Reduce Motion on and a fine primary pointer, a touch drag's release
//! cross-fades into the snap point with no travel; a mouse or pen drag's release
//! lands at once, and so does `settle_to`. The pan's reports are fed by hand, each
//! naming the input that drives it.

use astryx_primitives::motion::MotionProperty;

use super::harness::*;
use crate::drag_to_snap_points::snap_drag::SettleOptions;
use crate::gestures::pan::{PanAxis, PanInput, PanState};

fn drawer() -> Rig {
    let r = rig(PanAxis::Y, &[0., 240., 480.], |_| {});
    r.snap.animator.set_reduced_motion(true);
    r
}

/// A drag down by `dy`, released at `vy` px/s, by `input`.
fn release(r: &Rig, dy: f64, vy: f64, input: PanInput) {
    r.pan_y(PanState::Began, 0., 0., input);
    r.pan_y(PanState::Changed, dy, 0., input);
    r.pan_y(PanState::Ended, dy, vy, input);
}

/// Whether the element's opacity dips during the next `ms` (a cross-fade), and
/// whether its offset ever sits between where it was and where it lands (travel).
fn watch(r: &Rig, from: f64, to: f64, ms: f64) -> (bool, bool) {
    let (mut fades, mut travels) = (false, false);
    let mut t = 0.;
    while t < ms {
        r.advance(8.);
        t += 8.;
        if r.snap.animator.shown_opacity(None) < 0.999 {
            fades = true;
        }
        let y = r.snap.animator.get(MotionProperty::Y, None);
        let (lo, hi) = (from.min(to), from.max(to));
        if y > lo + 0.5 && y < hi - 0.5 {
            travels = true;
        }
    }
    (fades, travels)
}

#[test]
fn a_touch_drags_release_cross_fades_into_the_snap_point() {
    let r = drawer();
    release(&r, 300., 0., PanInput::Touch);
    let (fades, travels) = watch(&r, 300., 240., 1000.);
    assert!(fades, "it cross-fades");
    // It goes to its snap point without travel.
    assert!(!travels);
    assert_eq!(*r.settled.borrow(), [1]);
}

#[test]
fn a_mouse_or_pen_drags_release_lands_at_once() {
    for input in [PanInput::Mouse, PanInput::Pen] {
        let r = drawer();
        release(&r, 300., 0., input);
        r.advance(1.);
        assert_eq!(*r.settled.borrow(), [1], "{input:?}");
        assert_eq!(r.snap.animator.get(MotionProperty::Y, None), 240.);
        assert!(r.snap.animator.shown_opacity(None) >= 0.999, "no fade");
    }
}

#[test]
fn settle_to_lands_at_once() {
    let r = drawer();
    r.snap.settle_to(2, SettleOptions::default());
    r.advance(1.);
    assert_eq!(*r.settled.borrow(), [2]);
    assert!(r.snap.animator.shown_opacity(None) >= 0.999, "no fade");
}
