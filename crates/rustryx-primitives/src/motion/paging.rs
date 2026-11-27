//! UIKit's paging settle, upstream `motion/paging.ts`: the settle a paging
//! scroll view plays onto a page, and `setContentOffset(_:animated:)`.
//! Pure math read by the pager.

use std::rc::Rc;

use super::bezier::{BezierPoints, cubic_bezier};
use super::uikit::paging;
use super::{FnCurve, MotionCurve};

/// The velocity a paging settle starts at: the release velocity, zeroed
/// below `release.velocityFloor` (upstream `pagingSettleVelocity`).
pub fn paging_settle_velocity(velocity: f64) -> f64 {
    if velocity.abs() < paging::release::VELOCITY_FLOOR {
        0.
    } else {
        velocity
    }
}

/// A paging settle: the curve and where it ends (upstream `PagingCurve`).
#[derive(Clone, Debug)]
pub struct PagingCurve {
    pub from: f64,
    pub to: f64,
    /// Integration steps the settle took.
    pub steps: usize,
    /// Length, s.
    pub duration: f64,
    steps_per_second: f64,
    xs: Rc<Vec<f64>>,
    vs: Rc<Vec<f64>>,
}

impl PagingCurve {
    fn read(&self, table: &[f64], t: f64) -> f64 {
        if t.is_nan() || t <= 0. {
            return table[0];
        }
        let last = table.len() - 1;
        let s = t * self.steps_per_second;
        if s >= last as f64 {
            return table[last];
        }
        let i = s.floor() as usize;
        let f = s - i as f64;
        table[i] + (table[i + 1] - table[i]) * f
    }
}

impl MotionCurve for PagingCurve {
    fn duration(&self) -> f64 {
        self.duration
    }
    fn at(&self, t: f64) -> f64 {
        self.read(&self.xs, t)
    }
    fn velocity(&self, t: f64) -> f64 {
        self.read(&self.vs, t) * self.steps_per_second
    }
}

/// UIKit's paging settle from `from` (px) at `velocity` (px/s) onto `to`, at
/// the theme's `tempo` (upstream `solvePaging`): it steps
/// `paging.integrator` `stepsPerSecond` times a second and ends exactly on
/// `to`. At a tempo each step lasts tempo times as long and carries tempo
/// times the release's distance.
pub fn solve_paging(from: f64, velocity: f64, to: f64, tempo: f64) -> PagingCurve {
    use paging::integrator::{FRICTION, REST_DISTANCE, REST_SPEED, SPRING_PULL, STEPS_PER_SECOND};
    let pace = if tempo.is_finite() && tempo > 0. {
        tempo
    } else {
        1.
    };
    let steps_per_second = STEPS_PER_SECOND / pace;
    let start_velocity = velocity / 1000. * pace;
    let mut xs = vec![from];
    let mut vs = vec![start_velocity];
    let (mut x, mut v) = (from, start_velocity);
    let max_steps = (STEPS_PER_SECOND * 10.) as usize;
    let mut steps = 0;
    while steps < max_steps {
        if (to - x).abs() < REST_DISTANCE && v.abs() < REST_SPEED {
            break;
        }
        v = (v + (to - x) * SPRING_PULL) * FRICTION;
        x += v;
        steps += 1;
        xs.push(x);
        vs.push(v);
    }
    let last = xs.len() - 1;
    xs[last] = to;
    vs[last] = 0.;
    PagingCurve {
        from,
        to,
        steps,
        duration: steps as f64 / steps_per_second,
        steps_per_second,
        xs: Rc::new(xs),
        vs: Rc::new(vs),
    }
}

/// `setContentOffset(_:animated:)` from `from` to `to` over `duration_ms` on
/// `curve` (the table's by default), keeping a retargeted move's `velocity`
/// (px/s) through a Hermite term (upstream `solveAnimatedPageScroll`).
pub fn solve_animated_page_scroll(
    from: f64,
    to: f64,
    duration_ms: Option<f64>,
    curve: Option<BezierPoints>,
    velocity: Option<f64>,
) -> FnCurve {
    let duration = duration_ms.unwrap_or(paging::ANIMATED_SCROLL_MS) / 1000.;
    let distance = to - from;
    let ease = cubic_bezier(curve.unwrap_or(paging::ANIMATED_SCROLL_CURVE));
    let progress = move |u: f64| ease.ease(u.clamp(0., 1.));
    let start_slope = progress(1e-4) / 1e-4;
    let carry = velocity.map_or(0., |v| v * duration - distance * start_slope);
    let at = Rc::new(move |t: f64| {
        let u = (t / duration).clamp(0., 1.);
        from + distance * progress(u) + carry * u * (1. - u) * (1. - u)
    });
    let value = at.clone();
    FnCurve::new(
        duration,
        move |t| at(t),
        move |t| {
            if t <= 0. {
                velocity.unwrap_or(0.)
            } else if t >= duration {
                0.
            } else {
                let h = 1e-4f64.min(t / 2.).min((duration - t) / 2.);
                (value(t + h) - value(t - h)) / (2. * h)
            }
        },
    )
}
