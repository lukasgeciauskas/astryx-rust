//! What upstream lab's `webgl.ts` gives the GL marks that applies to GPUI, which draws them
//! with its own GPU quads and paths, with no WebGL context: the color conversion, the
//! supersampling ratio, the sprite size compensation and premultiplied blending.

use gpui::{Hsla, Rgba};

/// Upstream `POINT_SIZE_COMPENSATION`: a point sprite's visible circle ends at 0.48 of the
/// sprite, where its smoothstep edge starts, so sprites are drawn 1/0.96 as large to
/// match an SVG circle's radius.
pub const POINT_SIZE_COMPENSATION: f64 = 1.0 / 0.96;

/// A color's red, green and blue as upstream's `hexToGL` reads them: each 8-bit channel
/// over 255.
pub fn hex_to_gl(color: Hsla) -> [f64; 3] {
    let c = color.to_rgb();
    [c.r, c.g, c.b].map(|v| (f64::from(v.clamp(0., 1.)) * 255.).round() / 255.)
}

/// Upstream `getCanvasDPR`: twice the display's pixel ratio (2 when unknown), the
/// supersampling of the marks' canvases.
pub fn canvas_dpr(scale_factor: f32) -> f64 {
    let base = f64::from(scale_factor);
    (if base > 0. { base } else { 2. }) * 2.
}

/// The color a premultiplied fragment `(rgb * a, a)` leaves, as upstream's canvases blend it
/// (`ONE, ONE_MINUS_SRC_ALPHA`, channels clamped to 0 to 1): an alpha past 1 brightens the
/// color, and one at or below 0 draws nothing (`None`).
pub(crate) fn premultiplied(rgb: [f64; 3], a: f64) -> Option<Hsla> {
    if a.is_nan() || a <= 0. {
        return None;
    }
    let alpha = a.min(1.);
    let [r, g, b] = rgb.map(|c| (c * a).clamp(0., 1.) / alpha);
    Some(
        Rgba {
            r: r as f32,
            g: g as f32,
            b: b as f32,
            a: alpha as f32,
        }
        .into(),
    )
}

/// An opaque color from GL floats.
pub(crate) fn rgb_color([r, g, b]: [f64; 3]) -> Hsla {
    Rgba {
        r: r as f32,
        g: g as f32,
        b: b as f32,
        a: 1.,
    }
    .into()
}
