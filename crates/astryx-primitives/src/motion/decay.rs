//! UIScrollView's momentum coast, upstream `motion/decay.ts`.

use super::uikit::scroll;

fn decay_constant(rate: f64) -> f64 {
    debug_assert!(rate > 0. && rate < 1., "decay: rate must be in (0, 1)");
    -1000. * rate.ln()
}

/// How far a coast at `velocity` units/s travels before it stops.
pub fn decay_rest_distance(velocity: f64, rate: f64) -> f64 {
    velocity / decay_constant(rate)
}

/// A coast released at `from` with `initial_velocity` units/s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecayCurve {
    pub from: f64,
    pub initial_velocity: f64,
    /// Per-millisecond velocity factor.
    pub rate: f64,
    /// Where the coast comes to rest.
    pub rest: f64,
    lambda: f64,
}

/// Upstream `solveDecay`; `rate` defaults to `scroll::DECELERATION_NORMAL`.
pub fn solve_decay(from: f64, velocity: f64, rate: Option<f64>) -> DecayCurve {
    let rate = rate.unwrap_or(scroll::DECELERATION_NORMAL);
    let lambda = decay_constant(rate);
    DecayCurve {
        from,
        initial_velocity: velocity,
        rate,
        rest: from + velocity / lambda,
        lambda,
    }
}

impl DecayCurve {
    /// The value `t` seconds after release.
    pub fn value(&self, t: f64) -> f64 {
        self.from + self.initial_velocity * -(-self.lambda * t).exp_m1() / self.lambda
    }

    /// The velocity `t` seconds after release.
    pub fn velocity(&self, t: f64) -> f64 {
        self.initial_velocity * (-self.lambda * t).exp()
    }

    /// Seconds until the value stays within `threshold` of the rest point.
    pub fn settle_time(&self, threshold: f64) -> f64 {
        let remaining = self.initial_velocity.abs() / self.lambda;
        if threshold.is_nan() || threshold <= 0. {
            f64::INFINITY
        } else if remaining <= threshold {
            0.
        } else {
            (remaining / threshold).ln() / self.lambda
        }
    }

    /// Seconds until the speed falls to `speed`.
    pub fn time_to_speed(&self, speed: f64) -> f64 {
        let speed0 = self.initial_velocity.abs();
        if speed0 <= speed {
            0.
        } else if speed > 0. {
            (speed0 / speed).ln() / self.lambda
        } else {
            f64::INFINITY
        }
    }

    /// Seconds until the coast reaches `target`, or `None` when it never
    /// does (behind the release point, or at or past the rest point).
    pub fn time_to_reach(&self, target: f64) -> Option<f64> {
        if self.initial_velocity == 0. {
            return (target == self.from).then_some(0.);
        }
        let fraction = self.lambda * (target - self.from) / self.initial_velocity;
        if !(0. ..1.).contains(&fraction) {
            return None;
        }
        Some(-(-fraction).ln_1p() / self.lambda)
    }
}
