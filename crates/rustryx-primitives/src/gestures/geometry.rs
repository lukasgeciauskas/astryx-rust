//! The geometry recognizers share, upstream `gestures/geometry.ts`.

use std::f64::consts::PI;

use super::types::{GestureTouch, Point};

/// The centroid of `touches`, leaving out the ids in `excluding`.
pub fn centroid<'a>(
    touches: impl IntoIterator<Item = &'a GestureTouch>,
    excluding: &[u64],
) -> Point {
    let mut x = 0.;
    let mut y = 0.;
    let mut count = 0.;
    for touch in touches {
        if excluding.contains(&touch.id()) {
            continue;
        }
        let at = touch.position();
        x += at.x;
        y += at.y;
        count += 1.;
    }
    if count == 0. {
        Point::ZERO
    } else {
        Point::new(x / count, y / count)
    }
}

/// The distance between two points.
pub fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// How far a touch is from where it landed.
pub fn travel(touch: &GestureTouch) -> f64 {
    distance(touch.position(), touch.start())
}

/// The angle of the vector from `a` to `b`, radians.
pub fn angle_between(a: Point, b: Point) -> f64 {
    (b.y - a.y).atan2(b.x - a.x)
}

/// `angle` shifted by whole turns to lie within half a turn of `reference`.
pub fn unwrap_angle(angle: f64, reference: f64) -> f64 {
    let mut result = angle;
    while result - reference > PI {
        result -= 2. * PI;
    }
    while result - reference < -PI {
        result += 2. * PI;
    }
    result
}

/// `TAP_WOBBLE` (upstream `gestures/tapWobble.ts`): how far a tap's finger
/// drifts, px. A pan or a pull leaves moves inside it to the window, so a tap
/// keeps its click.
pub const TAP_WOBBLE: f64 = 5.;

/// `Math.sign`: -1, 0 or 1 (0 for zero, unlike `f64::signum`).
pub fn sign(value: f64) -> f64 {
    if value > 0. {
        1.
    } else if value < 0. {
        -1.
    } else {
        0.
    }
}
