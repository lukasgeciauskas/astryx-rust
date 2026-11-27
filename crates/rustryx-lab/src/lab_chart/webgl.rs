//! What upstream lab's `webgl.ts` gives the GL marks that applies to GPUI, which draws them
//! with its own GPU quads and paths, with no WebGL context: rustryx-charts' color conversion,
//! supersampling ratio, sprite size compensation and premultiplied blending.

/// Upstream `hexToGL`: a color's 8-bit red, green and blue over 255.
pub use rustryx_charts::marks::gl_rgb as hex_to_gl;
/// Upstream `getCanvasDPR` and `POINT_SIZE_COMPENSATION`.
pub use rustryx_charts::marks::{POINT_SIZE_COMPENSATION, canvas_dpr};
pub(crate) use rustryx_charts::marks::{premultiplied, rgb_color};
