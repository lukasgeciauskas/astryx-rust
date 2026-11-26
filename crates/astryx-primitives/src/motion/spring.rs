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

/// A spring parameter out of its range (upstream's `RangeError`).
#[derive(Clone, Debug, PartialEq)]
pub struct SpringError(pub String);

impl std::fmt::Display for SpringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SpringError {}

fn positive_finite(name: &str, value: f64) -> Result<(), SpringError> {
    if value.is_finite() && value > 0. {
        Ok(())
    } else {
        Err(SpringError(format!(
            "spring: {name} must be a positive number, got {value}"
        )))
    }
}

/// SwiftUI's (duration, bounce) (upstream `DurationBounce`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DurationBounce {
    /// The perceptual duration, s.
    pub duration: f64,
    /// 0 is critically damped, up to 1 undamped, down to -1 overdamped.
    pub bounce: f64,
}

/// WWDC18's (response, dampingRatio) (upstream `ResponseDamping`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResponseDamping {
    /// Period of the undamped spring, s.
    pub response: f64,
    /// Damping as a fraction of critical damping.
    pub damping_ratio: f64,
}

/// SwiftUI's `Spring(duration:bounce:)`: the WWDC23 equations, bounce
/// clamped to (-1, 1] (upstream `springFromDurationBounce`).
pub fn spring_from_duration_bounce(duration: f64, bounce: f64) -> Result<Spring, SpringError> {
    positive_finite("duration", duration)?;
    if !bounce.is_finite() {
        return Err(SpringError(format!(
            "spring: bounce must be finite, got {bounce}"
        )));
    }
    Ok(SpringDescription::DurationBounce { duration, bounce }.to_spring())
}

/// SwiftUI's `Spring(response:dampingRatio:)`; overdamping is kept
/// (upstream `springFromResponse`).
pub fn spring_from_response(response: f64, damping_ratio: f64) -> Result<Spring, SpringError> {
    positive_finite("response", response)?;
    if !(damping_ratio.is_finite() && damping_ratio >= 0.) {
        return Err(SpringError(format!(
            "spring: dampingRatio must be ≥ 0, got {damping_ratio}"
        )));
    }
    Ok(SpringDescription::ResponseDamping {
        response,
        damping_ratio,
    }
    .to_spring())
}

/// `CASpringAnimation`'s mass, stiffness and damping; an overdamped spring
/// is critical unless `allow_overdamping` (upstream `springFromPhysics`).
pub fn spring_from_physics(
    mass: f64,
    stiffness: f64,
    damping: f64,
    allow_overdamping: bool,
) -> Result<Spring, SpringError> {
    positive_finite("mass", mass)?;
    positive_finite("stiffness", stiffness)?;
    if !(damping.is_finite() && damping >= 0.) {
        return Err(SpringError(format!(
            "spring: damping must be ≥ 0, got {damping}"
        )));
    }
    let critical = 2. * (stiffness * mass).sqrt();
    Ok(Spring {
        mass,
        stiffness,
        damping: if allow_overdamping {
            damping
        } else {
            damping.min(critical)
        },
    })
}

impl Spring {
    /// The undamped period, s (upstream `responseOf`).
    pub fn response(&self) -> f64 {
        TAU / self.natural_frequency()
    }

    /// The spring as SwiftUI's (duration, bounce) (upstream `durationBounceOf`).
    pub fn duration_bounce(&self) -> DurationBounce {
        let zeta = self.damping_ratio();
        DurationBounce {
            duration: self.response(),
            bounce: if zeta <= 1. {
                1. - zeta
            } else {
                1. / zeta - 1.
            },
        }
    }

    /// The spring as (response, dampingRatio) (upstream `responseDampingOf`).
    pub fn response_damping(&self) -> ResponseDamping {
        ResponseDamping {
            response: self.response(),
            damping_ratio: self.damping_ratio(),
        }
    }
}

/// UIKit's relative initial spring velocity (upstream `relativeVelocity`).
pub fn relative_velocity(velocity: f64, from: f64, to: f64) -> f64 {
    velocity / (to - from)
}

/// The inverse of [`relative_velocity`] (upstream `absoluteVelocity`).
pub fn absolute_velocity(relative: f64, from: f64, to: f64) -> f64 {
    relative * (to - from)
}

/// A spring chasing a target that moves at a constant speed (upstream
/// `SpringRampCurve`).
#[derive(Clone, Copy, Debug)]
pub struct SpringRampCurve {
    pub spring: Spring,
    /// How far behind the moving target the spring settles: c·slope ÷ k.
    pub lag: f64,
    target: f64,
    slope: f64,
    free: SpringCurve,
}

impl SpringRampCurve {
    /// The value `t` seconds after the start.
    pub fn value(&self, t: f64) -> f64 {
        self.target + self.slope * t - self.lag + self.free.value(t)
    }

    /// The velocity `t` seconds after the start, units/s.
    pub fn velocity(&self, t: f64) -> f64 {
        self.slope + self.free.velocity(t)
    }
}

/// A spring whose target moves as `target + slope·t`, from `from` at
/// `velocity` (upstream `solveSpringTowardRamp`).
pub fn solve_spring_toward_ramp(
    spring: Spring,
    from: f64,
    velocity: f64,
    target: f64,
    slope: f64,
) -> SpringRampCurve {
    let lag = spring.damping * slope / spring.stiffness;
    let free = solve_spring(spring, from - target + lag, 0., velocity - slope);
    SpringRampCurve {
        spring,
        lag,
        target,
        slope,
        free,
    }
}

/// `CASpringAnimation.settlingDuration` with UIKit's relative initial
/// velocity and an epsilon (upstream `settlingDuration` with options).
pub fn settling_duration_with(spring: Spring, initial_velocity: f64, epsilon: f64) -> f64 {
    solve_spring(spring, 0., 1., initial_velocity).settle_time(epsilon)
}

/// ω₀ · duration UIKit's legacy spring derivation lands on, by damping
/// ratio (measured: UIKit lab, iOS 26.2 simulator, Oct 23 2026).
const UIKIT_LEGACY_OMEGA_DURATION: [(f64, f64); 19] = [
    (0.1, 46.102),
    (0.15, 33.4801),
    (0.2, 26.5936),
    (0.25, 22.2149),
    (0.3, 19.1698),
    (0.35, 16.9236),
    (0.4, 15.1966),
    (0.45, 13.8275),
    (0.5, 12.7169),
    (0.55, 11.8001),
    (0.6, 11.0335),
    (0.65, 10.3869),
    (0.7, 9.8396),
    (0.75, 9.3779),
    (0.8, 8.9943),
    (0.85, 8.6897),
    (0.9, 8.4808),
    (0.95, 8.4425),
    (1., 9.2334),
];

/// UIKit's legacy `animate(withDuration:usingSpringWithDamping:…)`: mass 1,
/// the ratio clamped to (0, 1], ω₀ = c(ζ) ÷ duration interpolated from
/// UIKit's reads (upstream `springFromUIKitDampingDuration`).
pub fn spring_from_uikit_damping_duration(
    duration: f64,
    damping_ratio: f64,
) -> Result<Spring, SpringError> {
    positive_finite("duration", duration)?;
    let zeta = damping_ratio.clamp(1e-3, 1.);
    let table = UIKIT_LEGACY_OMEGA_DURATION;
    let mut c = table[0].1 * table[0].0 / zeta;
    for pair in table.windows(2) {
        let ((z0, c0), (z1, c1)) = (pair[0], pair[1]);
        if zeta >= z0 && zeta <= z1 {
            c = c0 + (c1 - c0) * (zeta - z0) / (z1 - z0);
            break;
        }
    }
    let omega0 = c / duration;
    Ok(Spring {
        mass: 1.,
        stiffness: omega0 * omega0,
        damping: 2. * zeta * omega0,
    })
}
