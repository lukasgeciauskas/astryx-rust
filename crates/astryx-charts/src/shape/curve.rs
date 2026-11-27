//! d3-shape's curves: how a line or area joins its points, each a port of the state
//! machine in d3-shape 3.2.0's `src/curve`.

use astryx_theme::authoring::js::js_min;

use super::path::ChartPath;

/// How a line or area joins its points (d3-shape's curve factories).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Curve {
    /// Straight segments (d3-shape `curveLinear`).
    #[default]
    Linear,
    /// A cubic spline that keeps monotonic data monotonic in y, for data monotonic in
    /// x (d3-shape `curveMonotoneX`).
    MonotoneX,
    /// A natural cubic spline, its second derivative zero at the ends (d3-shape
    /// `curveNatural`).
    Natural,
    /// Horizontal and vertical segments, the steps halfway between points (d3-shape
    /// `curveStep`).
    Step,
    /// Steps at each point's start: vertical, then horizontal (d3-shape
    /// `curveStepBefore`).
    StepBefore,
    /// Steps at each next point: horizontal, then vertical (d3-shape
    /// `curveStepAfter`).
    StepAfter,
}

/// JavaScript truthiness of a number (`undefined`, which d3 leaves in `_line` for a
/// line, reads here as NaN, as `1 - undefined` does).
fn truthy(x: f64) -> bool {
    x != 0. && !x.is_nan()
}

/// The running state of one curve over one generator call (what d3's curve factory
/// returns for a context).
pub(crate) enum CurveState {
    Linear(Linear),
    MonotoneX(MonotoneX),
    Natural(Natural),
    Step(Step),
}

impl CurveState {
    /// The curve's initial state (d3-shape `curve(context)`).
    pub(crate) fn new(curve: Curve) -> Self {
        match curve {
            Curve::Linear => Self::Linear(Linear {
                line: f64::NAN,
                point: 0,
            }),
            Curve::MonotoneX => Self::MonotoneX(MonotoneX {
                line: f64::NAN,
                point: 0,
                x0: f64::NAN,
                y0: f64::NAN,
                x1: f64::NAN,
                y1: f64::NAN,
                t0: f64::NAN,
            }),
            Curve::Natural => Self::Natural(Natural {
                line: f64::NAN,
                x: Vec::new(),
                y: Vec::new(),
            }),
            Curve::Step | Curve::StepBefore | Curve::StepAfter => Self::Step(Step {
                t: match curve {
                    Curve::StepBefore => 0.,
                    Curve::StepAfter => 1.,
                    _ => 0.5,
                },
                line: f64::NAN,
                point: 0,
                x: f64::NAN,
                y: f64::NAN,
            }),
        }
    }

    /// An area begins (`areaStart`).
    pub(crate) fn area_start(&mut self) {
        *self.line_mut() = 0.;
    }

    /// An area ends (`areaEnd`).
    pub(crate) fn area_end(&mut self) {
        *self.line_mut() = f64::NAN;
    }

    fn line_mut(&mut self) -> &mut f64 {
        match self {
            Self::Linear(c) => &mut c.line,
            Self::MonotoneX(c) => &mut c.line,
            Self::Natural(c) => &mut c.line,
            Self::Step(c) => &mut c.line,
        }
    }

    /// A run of defined points begins (`lineStart`).
    pub(crate) fn line_start(&mut self) {
        match self {
            Self::Linear(c) => c.point = 0,
            Self::MonotoneX(c) => {
                c.x0 = f64::NAN;
                c.x1 = f64::NAN;
                c.y0 = f64::NAN;
                c.y1 = f64::NAN;
                c.t0 = f64::NAN;
                c.point = 0;
            }
            Self::Natural(c) => {
                c.x.clear();
                c.y.clear();
            }
            Self::Step(c) => {
                c.x = f64::NAN;
                c.y = f64::NAN;
                c.point = 0;
            }
        }
    }

    /// A run of defined points ends (`lineEnd`).
    pub(crate) fn line_end(&mut self, path: &mut ChartPath) {
        match self {
            Self::Linear(c) => {
                if truthy(c.line) || (c.line != 0. && c.point == 1) {
                    path.close_path();
                }
                c.line = 1. - c.line;
            }
            Self::MonotoneX(c) => c.line_end(path),
            Self::Natural(c) => c.line_end(path),
            Self::Step(c) => c.line_end(path),
        }
    }

    /// The next defined point (`point(x, y)`).
    pub(crate) fn point(&mut self, path: &mut ChartPath, x: f64, y: f64) {
        match self {
            Self::Linear(c) => match c.point {
                0 => {
                    c.point = 1;
                    if truthy(c.line) {
                        path.line_to(x, y);
                    } else {
                        path.move_to(x, y);
                    }
                }
                1 => {
                    c.point = 2;
                    path.line_to(x, y);
                }
                _ => path.line_to(x, y),
            },
            Self::MonotoneX(c) => c.point(path, x, y),
            Self::Natural(c) => {
                c.x.push(x);
                c.y.push(y);
            }
            Self::Step(c) => c.point(path, x, y),
        }
    }
}

/// `curveLinear`'s state.
pub(crate) struct Linear {
    line: f64,
    point: u8,
}

/// `curveMonotoneX`'s state: the last two points and the last tangent.
pub(crate) struct MonotoneX {
    line: f64,
    point: u8,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    t0: f64,
}

/// `x < 0 ? -1 : 1` (NaN counts as positive).
fn sign(x: f64) -> f64 {
    if x < 0. { -1. } else { 1. }
}

impl MonotoneX {
    // Calculate the slopes of the tangents (Hermite-type interpolation) based on
    // the following paper: Steffen, M. 1990. A Simple Method for Monotonic
    // Interpolation in One Dimension. Astronomy and Astrophysics, Vol. 239, NO.
    // NOV(II), P. 443, 1990.
    fn slope3(&self, x2: f64, y2: f64) -> f64 {
        let h0 = self.x1 - self.x0;
        let h1 = x2 - self.x1;
        // `h0 || h1 < 0 && -0`: a zero span divides by a zero signed like the other.
        let d0 = if truthy(h0) {
            h0
        } else if h1 < 0. {
            -0.
        } else {
            0.
        };
        let d1 = if truthy(h1) {
            h1
        } else if h0 < 0. {
            -0.
        } else {
            0.
        };
        let s0 = (self.y1 - self.y0) / d0;
        let s1 = (y2 - self.y1) / d1;
        let p = (s0 * h1 + s1 * h0) / (h0 + h1);
        let slope = (sign(s0) + sign(s1)) * js_min(js_min(s0.abs(), s1.abs()), 0.5 * p.abs());
        if truthy(slope) { slope } else { 0. }
    }

    // Calculate a one-sided slope.
    fn slope2(&self, t: f64) -> f64 {
        let h = self.x1 - self.x0;
        if truthy(h) {
            (3. * (self.y1 - self.y0) / h - t) / 2.
        } else {
            t
        }
    }

    // According to https://en.wikipedia.org/wiki/Cubic_Hermite_spline#Representations
    // "you can express cubic Hermite interpolation in terms of cubic Bézier curves
    // with respect to the four values p0, p0 + m0 / 3, p1 - m1 / 3, p1".
    fn hermite(&self, path: &mut ChartPath, t0: f64, t1: f64) {
        let (x0, y0, x1, y1) = (self.x0, self.y0, self.x1, self.y1);
        let dx = (x1 - x0) / 3.;
        path.bezier_curve_to(x0 + dx, y0 + dx * t0, x1 - dx, y1 - dx * t1, x1, y1);
    }

    fn line_end(&mut self, path: &mut ChartPath) {
        match self.point {
            2 => path.line_to(self.x1, self.y1),
            3 => self.hermite(path, self.t0, self.slope2(self.t0)),
            _ => {}
        }
        if truthy(self.line) || (self.line != 0. && self.point == 1) {
            path.close_path();
        }
        self.line = 1. - self.line;
    }

    fn point(&mut self, path: &mut ChartPath, x: f64, y: f64) {
        let mut t1 = f64::NAN;
        // Ignore coincident points.
        if x == self.x1 && y == self.y1 {
            return;
        }
        match self.point {
            0 => {
                self.point = 1;
                if truthy(self.line) {
                    path.line_to(x, y);
                } else {
                    path.move_to(x, y);
                }
            }
            1 => self.point = 2,
            2 => {
                self.point = 3;
                t1 = self.slope3(x, y);
                self.hermite(path, self.slope2(t1), t1);
            }
            _ => {
                t1 = self.slope3(x, y);
                self.hermite(path, self.t0, t1);
            }
        }
        self.x0 = self.x1;
        self.x1 = x;
        self.y0 = self.y1;
        self.y1 = y;
        self.t0 = t1;
    }
}

/// `curveNatural`'s state: the run's points, drawn when it ends.
pub(crate) struct Natural {
    line: f64,
    x: Vec<f64>,
    y: Vec<f64>,
}

impl Natural {
    fn line_end(&mut self, path: &mut ChartPath) {
        let (x, y) = (&self.x, &self.y);
        let n = x.len();
        if n > 0 {
            if truthy(self.line) {
                path.line_to(x[0], y[0]);
            } else {
                path.move_to(x[0], y[0]);
            }
            if n == 2 {
                path.line_to(x[1], y[1]);
            } else if n > 2 {
                let (px0, px1) = control_points(x);
                let (py0, py1) = control_points(y);
                for i0 in 0..n - 1 {
                    let i1 = i0 + 1;
                    path.bezier_curve_to(px0[i0], py0[i0], px1[i0], py1[i0], x[i1], y[i1]);
                }
            }
        }
        if truthy(self.line) || (self.line != 0. && n == 1) {
            path.close_path();
        }
        self.line = 1. - self.line;
        self.x.clear();
        self.y.clear();
    }
}

// See https://www.particleincell.com/2012/bezier-splines/ for derivation.
fn control_points(x: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = x.len() - 1;
    let mut a = vec![0.; n];
    let mut b = vec![0.; n];
    let mut r = vec![0.; n];
    a[0] = 0.;
    b[0] = 2.;
    r[0] = x[0] + 2. * x[1];
    for i in 1..n - 1 {
        a[i] = 1.;
        b[i] = 4.;
        r[i] = 4. * x[i] + 2. * x[i + 1];
    }
    a[n - 1] = 2.;
    b[n - 1] = 7.;
    r[n - 1] = 8. * x[n - 1] + x[n];
    for i in 1..n {
        let m = a[i] / b[i - 1];
        b[i] -= m;
        r[i] -= m * r[i - 1];
    }
    a[n - 1] = r[n - 1] / b[n - 1];
    for i in (0..n - 1).rev() {
        a[i] = (r[i] - a[i + 1]) / b[i];
    }
    b[n - 1] = (x[n] + a[n - 1]) / 2.;
    for i in 0..n - 1 {
        b[i] = 2. * x[i + 1] - a[i + 1];
    }
    (a, b)
}

/// The step curves' state: where between points the step falls (`t`) and the last
/// point.
pub(crate) struct Step {
    t: f64,
    line: f64,
    point: u8,
    x: f64,
    y: f64,
}

impl Step {
    fn line_end(&mut self, path: &mut ChartPath) {
        if 0. < self.t && self.t < 1. && self.point == 2 {
            path.line_to(self.x, self.y);
        }
        if truthy(self.line) || (self.line != 0. && self.point == 1) {
            path.close_path();
        }
        if self.line >= 0. {
            self.t = 1. - self.t;
            self.line = 1. - self.line;
        }
    }

    fn point(&mut self, path: &mut ChartPath, x: f64, y: f64) {
        match self.point {
            0 => {
                self.point = 1;
                if truthy(self.line) {
                    path.line_to(x, y);
                } else {
                    path.move_to(x, y);
                }
            }
            _ => {
                if self.point == 1 {
                    self.point = 2;
                }
                if self.t <= 0. {
                    path.line_to(self.x, y);
                    path.line_to(x, y);
                } else {
                    let x1 = self.x * (1. - self.t) + x * self.t;
                    path.line_to(x1, self.y);
                    path.line_to(x1, y);
                }
            }
        }
        self.x = x;
        self.y = y;
    }
}
