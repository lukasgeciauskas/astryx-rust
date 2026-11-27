//! The projection from the normalized unit cube to the plot (upstream `ThreeDChart`'s `project`
//! memo, and the same math in `ThreeDScatterGL`'s vertex shader): centered, turned by the
//! azimuth around the vertical axis and by the elevation around the horizontal one, then
//! scaled to 35% of the plot's smaller side, orthographically.

use super::types::{Camera, ProjectedPoint};
use crate::trig::{js_cos, js_sin};

/// JavaScript's `Math.min` of two numbers: NaN wins.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// A camera's projection onto a plot of a size (upstream's `project` closure).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    /// Upstream `cosAz`: the cosine of the azimuth.
    pub cos_az: f64,
    /// Upstream `sinAz`: the sine of the azimuth.
    pub sin_az: f64,
    /// Upstream `cosEl`: the cosine of the elevation.
    pub cos_el: f64,
    /// Upstream `sinEl`: the sine of the elevation.
    pub sin_el: f64,
    /// Pixels per unit of the cube: 0.35 of the smaller side.
    pub scale: f64,
    /// Upstream `cx`: the plot's horizontal center.
    pub cx: f64,
    /// Upstream `cy`: the plot's vertical center.
    pub cy: f64,
}

impl Projection {
    /// The projection of `camera` onto a `width` by `height` plot. The angles become radians as
    /// upstream turns them (`(deg * PI) / 180`), and their cosines and sines are V8's
    /// `Math.cos` and `Math.sin` to the bit.
    pub fn new(camera: &Camera, width: f64, height: f64) -> Self {
        let az = (camera.azimuth * std::f64::consts::PI) / 180.;
        let el = (camera.elevation * std::f64::consts::PI) / 180.;
        Self::from_trig(
            js_cos(az),
            js_sin(az),
            js_cos(el),
            js_sin(el),
            width,
            height,
        )
    }

    /// The projection from the angles' cosines and sines.
    pub fn from_trig(
        cos_az: f64,
        sin_az: f64,
        cos_el: f64,
        sin_el: f64,
        width: f64,
        height: f64,
    ) -> Self {
        Projection {
            cos_az,
            sin_az,
            cos_el,
            sin_el,
            scale: js_min(width, height) * 0.35,
            cx: width / 2.,
            cy: height / 2.,
        }
    }

    /// Upstream `project(nx, ny, nz)`: a point of the unit cube on screen, with its depth.
    pub fn project(&self, nx: f64, ny: f64, nz: f64) -> ProjectedPoint {
        let (x, y, z) = (nx - 0.5, ny - 0.5, nz - 0.5);
        let x1 = x * self.cos_az + z * self.sin_az;
        let z1 = -x * self.sin_az + z * self.cos_az;
        let y1 = y * self.cos_el - z1 * self.sin_el;
        let z2 = y * self.sin_el + z1 * self.cos_el;
        ProjectedPoint {
            px: self.cx + x1 * self.scale,
            py: self.cy - y1 * self.scale,
            depth: z2,
        }
    }

    /// The vertex shader's projection (`ThreeDScatterGL`'s `VERT`) in the GPU's 32-bit floats, from
    /// a position stored as one (a `Float32Array`) and the uniforms upstream sets.
    pub fn project_gl(&self, position: [f32; 3]) -> ProjectedPoint {
        let (cos_az, sin_az, cos_el, sin_el) = (
            self.cos_az as f32,
            self.sin_az as f32,
            self.cos_el as f32,
            self.sin_el as f32,
        );
        let (scale, cx, cy) = (self.scale as f32, self.cx as f32, self.cy as f32);
        let [x, y, z] = position.map(|v| v - 0.5);
        let x1 = x * cos_az + z * sin_az;
        let z1 = -x * sin_az + z * cos_az;
        let y1 = y * cos_el - z1 * sin_el;
        let z2 = y * sin_el + z1 * cos_el;
        ProjectedPoint {
            px: f64::from(cx + x1 * scale),
            py: f64::from(cy - y1 * scale),
            depth: f64::from(z2),
        }
    }
}

/// Upstream `normalize(value, domain)`: a value's place in its domain, 0.5 when the domain is
/// a single value.
pub fn normalize(value: f64, domain: [f64; 2]) -> f64 {
    let range = domain[1] - domain[0];
    if range == 0. {
        0.5
    } else {
        (value - domain[0]) / range
    }
}
