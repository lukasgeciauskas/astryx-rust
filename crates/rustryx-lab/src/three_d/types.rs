//! The 3D chart's types (upstream lab `ThreeD/types.ts`): points in data space and on screen,
//! and the camera.

/// A point in data space (upstream `Point3D`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point3D {
    /// Upstream `x`.
    pub x: f64,
    /// Upstream `y`: the vertical axis.
    pub y: f64,
    /// Upstream `z`.
    pub z: f64,
}

/// A point projected to the plot, with its depth for sorting (upstream `ProjectedPoint`):
/// a larger depth is nearer the viewer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProjectedPoint {
    /// Upstream `px`: the horizontal position in the plot, in pixels.
    pub px: f64,
    /// Upstream `py`: the vertical position in the plot, in pixels (down is positive).
    pub py: f64,
    /// Upstream `depth`: the rotated point's distance toward the viewer.
    pub depth: f64,
}

/// The camera (upstream `Camera`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Upstream `azimuth`: the rotation around the vertical axis, in degrees.
    pub azimuth: f64,
    /// Upstream `elevation`: the rotation above the horizontal plane, in degrees.
    pub elevation: f64,
    /// Upstream `distance`: declared upstream for perspective, which the projection does
    /// not use (it is orthographic).
    pub distance: Option<f64>,
}

impl Camera {
    /// A camera at `azimuth` and `elevation` degrees.
    pub fn new(azimuth: f64, elevation: f64) -> Self {
        Camera {
            azimuth,
            elevation,
            distance: None,
        }
    }
}
