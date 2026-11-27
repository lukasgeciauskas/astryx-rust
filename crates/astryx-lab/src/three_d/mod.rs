//! Upstream lab `ThreeD`: 3D charts projected to the plane (`<ThreeDChart><ThreeDScatter/>…</ThreeDChart>`).
//! [`ThreeDChart`] owns the camera and the projection of the unit cube; its marks
//! ([`ThreeDScatter`], [`ThreeDBar`], [`ThreeDSurface`], [`ThreeDScatterGl`]) and chrome ([`ThreeDGrid`],
//! [`ThreeDAxis`]) draw far to near with GPUI paths and quads.

mod axis;
mod bar;
mod chart;
mod context;
mod grid;
mod projection;
mod scatter;
mod scatter_gl;
mod sort;
mod surface;
mod types;

/// The gallery's 3D chart examples (upstream's stories).
pub mod examples;

#[cfg(test)]
mod tests;

pub use axis::{ThreeDAxis, axis_geometry};
pub use bar::{Bar3D, ThreeDBar, bar_geometry};
pub use chart::{
    KEYBOARD_STEP_DEGREES, ThreeDChart, accessible_label, camera_after_drag, camera_after_key,
    clamp_elevation, compute_domain, rotation_step,
};
pub use context::{ThreeDContext, use_3d};
pub use grid::{GridLine3D, ThreeDGrid, grid_lines};
pub use projection::{Projection, normalize};
pub use scatter::{ScatterPoint3D, ThreeDScatter, depth_factor, scatter_points};
pub use scatter_gl::{GlPoint, ThreeDScatterGl, gl_points, gl_positions};
pub use surface::{FaceStyle, SurfaceFace, ThreeDSurface, face_style, lerp_color, surface_faces};
pub use types::{Camera, Point3D, ProjectedPoint};

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};

pub(crate) use sort::js_sort;

/// Sorts far to near as upstream's `sort((a, b) => a.depth - b.depth)` does.
pub(crate) fn sort_by_depth<T: Clone>(items: &mut [T], depth: impl Fn(&T) -> f64) {
    js_sort(items, |a, b| depth(a) - depth(b));
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const EXAMPLE: &str = include_str!("examples.rs");

/// Upstream lab `ThreeDChart` (its doc lists no props; these are `ThreeDChartProps` in order).
pub const THREE_D_CHART_DOC: ComponentDoc = ComponentDoc {
    name: "ThreeDChart",
    rust_type: "astryx::lab::three_d::ThreeDChart",
    props: &[
        p("data", RustApi::Argument("data")),
        p("xKey", RustApi::Argument("x_key")),
        p("yKey", RustApi::Argument("y_key")),
        p("zKey", RustApi::Argument("z_key")),
        p("xDomain", RustApi::Method("x_domain")),
        p("yDomain", RustApi::Method("y_domain")),
        p("zDomain", RustApi::Method("z_domain")),
        p("height", RustApi::Method("height")),
        p("azimuth", RustApi::Method("azimuth")),
        p("elevation", RustApi::Method("elevation")),
        p("interactive", RustApi::Method("interactive")),
        p("autoRotate", RustApi::Method("auto_rotate")),
        p("label", RustApi::Method("label")),
        p("children", RustApi::Trait("ParentElement")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ThreeDBar` (`ThreeDBarProps` in order).
pub const THREE_D_BAR_DOC: ComponentDoc = ComponentDoc {
    name: "ThreeDBar",
    rust_type: "astryx::lab::three_d::ThreeDBar",
    props: &[
        p("color", RustApi::Argument("color")),
        p("barWidth", RustApi::Method("bar_width")),
        p("barDepth", RustApi::Method("bar_depth")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ThreeDScatter` (`ThreeDScatterProps` in order).
pub const THREE_D_SCATTER_DOC: ComponentDoc = ComponentDoc {
    name: "ThreeDScatter",
    rust_type: "astryx::lab::three_d::ThreeDScatter",
    props: &[
        p("color", RustApi::Argument("color")),
        p("radius", RustApi::Method("radius")),
        p("opacity", RustApi::Method("opacity")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ThreeDScatterGL` (`ThreeDScatterGLProps` in order).
pub const THREE_D_SCATTER_GL_DOC: ComponentDoc = ComponentDoc {
    name: "ThreeDScatterGL",
    rust_type: "astryx::lab::three_d::ThreeDScatterGl",
    props: &[
        p("color", RustApi::Argument("color")),
        p("size", RustApi::Method("size")),
        p("opacity", RustApi::Method("opacity")),
    ],
    example: EXAMPLE,
};

/// Upstream lab `ThreeDSurface` (`ThreeDSurfaceProps` in order).
pub const THREE_D_SURFACE_DOC: ComponentDoc = ComponentDoc {
    name: "ThreeDSurface",
    rust_type: "astryx::lab::three_d::ThreeDSurface",
    props: &[
        p("colorRange", RustApi::Argument("color_range")),
        p("opacity", RustApi::Method("opacity")),
        p("wireframe", RustApi::Method("wireframe")),
    ],
    example: EXAMPLE,
};
