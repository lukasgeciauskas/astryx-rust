//! Cubic-bézier timing functions, upstream `motion/bezier.ts`: the curve
//! CSS and Core Animation define by two control points, solved for x by
//! Newton's method with a bisection fallback, as browser engines do.

/// Control points `[x1, y1, x2, y2]` (upstream `BezierPoints`).
pub type BezierPoints = [f64; 4];

/// The timing function of a set of control points: time progress to eased
/// progress (upstream `cubicBezier`'s returned function).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezierEasing {
    ax: f64,
    bx: f64,
    cx: f64,
    ay: f64,
    by: f64,
    cy: f64,
}

/// The timing function for `points` (upstream `cubicBezier`).
pub fn cubic_bezier([x1, y1, x2, y2]: BezierPoints) -> CubicBezierEasing {
    let cx = 3. * x1;
    let bx = 3. * (x2 - x1) - cx;
    let cy = 3. * y1;
    let by = 3. * (y2 - y1) - cy;
    CubicBezierEasing {
        ax: 1. - cx - bx,
        bx,
        cx,
        ay: 1. - cy - by,
        by,
        cy,
    }
}

impl CubicBezierEasing {
    fn sample_x(&self, t: f64) -> f64 {
        ((self.ax * t + self.bx) * t + self.cx) * t
    }

    fn sample_y(&self, t: f64) -> f64 {
        ((self.ay * t + self.by) * t + self.cy) * t
    }

    fn slope_x(&self, t: f64) -> f64 {
        (3. * self.ax * t + 2. * self.bx) * t + self.cx
    }

    fn solve_t(&self, x: f64) -> f64 {
        let mut t = x;
        for _ in 0..8 {
            let error = self.sample_x(t) - x;
            if error.abs() < 1e-7 {
                return t;
            }
            let slope = self.slope_x(t);
            if slope.abs() < 1e-6 {
                break;
            }
            t -= error / slope;
        }
        let (mut lo, mut hi) = (0., 1.);
        t = x;
        for _ in 0..60 {
            let value = self.sample_x(t);
            if (value - x).abs() < 1e-7 {
                break;
            }
            if value < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) / 2.;
        }
        t
    }

    /// The eased progress at time progress `x`, clamped to [0, 1].
    pub fn ease(&self, x: f64) -> f64 {
        if x <= 0. {
            0.
        } else if x >= 1. {
            1.
        } else {
            self.sample_y(self.solve_t(x))
        }
    }
}
