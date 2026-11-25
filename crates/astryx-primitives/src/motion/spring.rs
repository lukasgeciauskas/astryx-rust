//! Springs, upstream `motion/spring.ts`: every description converts to
//! Core Animation's mass, stiffness and damping, solved in closed form.

use std::f64::consts::TAU;

/// A spring in Core Animation's physical vocabulary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub mass: f64,
    pub stiffness: f64,
    pub damping: f64,
}

/// A spring as its source publishes it (upstream `SpringDescription`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpringDescription {
    /// Mass, stiffness and damping; overdamping is clamped to critical, as
    /// SwiftUI and Core Animation do by default.
    Physics {
        mass: f64,
        stiffness: f64,
        damping: f64,
    },
    /// SwiftUI's `Spring(duration:bounce:)`: 0 is critically damped, up to 1
    /// undamped, down to -1 overdamped.
    DurationBounce { duration: f64, bounce: f64 },
    /// WWDC18's response and damping ratio; overdamping is kept.
    ResponseDamping { response: f64, damping_ratio: f64 },
}

impl SpringDescription {
    /// Upstream `toSpring`.
    pub fn to_spring(self) -> Spring {
        match self {
            Self::Physics {
                mass,
                stiffness,
                damping,
            } => {
                let critical = 2. * (stiffness * mass).sqrt();
                Spring {
                    mass,
                    stiffness,
                    damping: damping.min(critical),
                }
            }
            Self::DurationBounce { duration, bounce } => {
                let b = bounce.clamp(-0.999_999, 1.);
                let stiffness = (TAU / duration).powi(2);
                let damping = if b >= 0. {
                    (1. - b) * 2. * TAU / duration
                } else {
                    2. * TAU / (duration * (1. + b))
                };
                Spring {
                    mass: 1.,
                    stiffness,
                    damping,
                }
            }
            Self::ResponseDamping {
                response,
                damping_ratio,
            } => {
                let omega = TAU / response;
                Spring {
                    mass: 1.,
                    stiffness: omega * omega,
                    damping: 2. * damping_ratio * omega,
                }
            }
        }
    }
}

impl Spring {
    /// The undamped angular frequency, rad/s.
    pub fn natural_frequency(&self) -> f64 {
        (self.stiffness / self.mass).sqrt()
    }

    /// The damping ratio: 1 is critical.
    pub fn damping_ratio(&self) -> f64 {
        self.damping / (2. * (self.stiffness * self.mass).sqrt())
    }
}

/// Below this |ζ − 1| a spring is solved as critically damped.
const CRITICAL_TOLERANCE: f64 = 1e-6;

#[derive(Clone, Copy, Debug)]
enum Regime {
    Under {
        decay: f64,
        omega_d: f64,
        a: f64,
        b: f64,
        bv: f64,
    },
    Critical {
        omega0: f64,
        b: f64,
        peak: f64,
    },
    Over {
        r1: f64,
        r2: f64,
        c1: f64,
        c2: f64,
    },
}

/// A spring released from `from` with a velocity, heading for `to`.
#[derive(Clone, Copy, Debug)]
pub struct SpringCurve {
    pub spring: Spring,
    pub from: f64,
    pub to: f64,
    /// Units per second.
    pub initial_velocity: f64,
    zeta: f64,
    omega0: f64,
    regime: Regime,
}

/// Upstream `solveSpring`.
pub fn solve_spring(spring: Spring, from: f64, to: f64, velocity: f64) -> SpringCurve {
    let omega0 = spring.natural_frequency();
    let zeta = spring.damping_ratio();
    let x0 = from - to;
    let v0 = velocity;
    let regime = if zeta < 1. - CRITICAL_TOLERANCE {
        let decay = zeta * omega0;
        let omega_d = omega0 * (1. - zeta * zeta).sqrt();
        Regime::Under {
            decay,
            omega_d,
            a: x0,
            b: (v0 + decay * x0) / omega_d,
            bv: (decay * v0 + omega0 * omega0 * x0) / omega_d,
        }
    } else if zeta <= 1. + CRITICAL_TOLERANCE {
        let b = v0 + omega0 * x0;
        let peak = if b != 0. {
            (1. / omega0 - x0.abs() / b.abs()).max(0.)
        } else {
            0.
        };
        Regime::Critical { omega0, b, peak }
    } else {
        let root = omega0 * (zeta * zeta - 1.).sqrt();
        let r1 = -zeta * omega0 + root;
        let r2 = -zeta * omega0 - root;
        let c1 = (v0 - r2 * x0) / (r1 - r2);
        Regime::Over {
            r1,
            r2,
            c1,
            c2: x0 - c1,
        }
    };
    SpringCurve {
        spring,
        from,
        to,
        initial_velocity: velocity,
        zeta,
        omega0,
        regime,
    }
}

impl SpringCurve {
    fn displacement(&self, t: f64) -> f64 {
        let x0 = self.from - self.to;
        match self.regime {
            Regime::Under {
                decay,
                omega_d,
                a,
                b,
                ..
            } => (-decay * t).exp() * (a * (omega_d * t).cos() + b * (omega_d * t).sin()),
            Regime::Critical { omega0, b, .. } => (x0 + b * t) * (-omega0 * t).exp(),
            Regime::Over { r1, r2, c1, c2 } => c1 * (r1 * t).exp() + c2 * (r2 * t).exp(),
        }
    }

    fn envelope(&self, t: f64) -> f64 {
        let x0 = self.from - self.to;
        match self.regime {
            Regime::Under { decay, a, b, .. } => (a.abs() + b.abs()) * (-decay * t).exp(),
            Regime::Critical { omega0, b, .. } => (x0.abs() + b.abs() * t) * (-omega0 * t).exp(),
            Regime::Over { r1, r2, c1, c2 } => {
                c1.abs() * (r1 * t).exp() + c2.abs() * (r2 * t).exp()
            }
        }
    }

    /// The value `t` seconds after release.
    pub fn value(&self, t: f64) -> f64 {
        self.to + self.displacement(t)
    }

    /// The velocity `t` seconds after release, units/s.
    pub fn velocity(&self, t: f64) -> f64 {
        let x0 = self.from - self.to;
        let v0 = self.initial_velocity;
        match self.regime {
            Regime::Under {
                decay, omega_d, bv, ..
            } => (-decay * t).exp() * (v0 * (omega_d * t).cos() - bv * (omega_d * t).sin()),
            Regime::Critical { omega0, b, .. } => {
                let _ = x0;
                (v0 - omega0 * b * t) * (-omega0 * t).exp()
            }
            Regime::Over { r1, r2, c1, c2 } => r1 * c1 * (r1 * t).exp() + r2 * c2 * (r2 * t).exp(),
        }
    }

    /// Seconds after which |value − to| stays below `threshold` for good;
    /// infinite for an undamped spring that moves.
    pub fn settle_time(&self, threshold: f64) -> f64 {
        if threshold.is_nan() || threshold <= 0. {
            return f64::INFINITY;
        }
        let x0 = self.from - self.to;
        if self.zeta == 0. {
            return if x0 == 0. && self.initial_velocity == 0. {
                0.
            } else {
                f64::INFINITY
            };
        }
        let start = match self.regime {
            Regime::Critical { peak, .. } => peak,
            _ => 0.,
        };
        if self.envelope(start) <= threshold {
            return 0.;
        }
        if let Regime::Under { decay, .. } = self.regime {
            return (self.envelope(0.) / threshold).ln() / decay;
        }
        let (mut lo, mut hi) = (start, start.max(1. / self.omega0));
        while self.envelope(hi) > threshold {
            lo = hi;
            hi *= 2.;
            if hi > 1e6 {
                return f64::INFINITY;
            }
        }
        for _ in 0..100 {
            if hi - lo <= 1e-12 {
                break;
            }
            let mid = (lo + hi) / 2.;
            if self.envelope(mid) > threshold {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        hi
    }
}

/// Core Animation's settling epsilon.
pub const SETTLING_EPSILON: f64 = 0.001;

/// Upstream `settlingDuration`: seconds until a unit move from rest stays
/// within `SETTLING_EPSILON` of its target.
pub fn settling_duration(spring: Spring) -> f64 {
    solve_spring(spring, 0., 1., 0.).settle_time(SETTLING_EPSILON)
}
