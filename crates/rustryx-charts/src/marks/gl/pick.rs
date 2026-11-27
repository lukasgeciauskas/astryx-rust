//! The hover pick of `dot_gl_interactive` (upstream's GPU color picking), on the CPU.
//!
//! Upstream draws every point into an offscreen buffer, each in a color that encodes
//! its index, at its size plus `HIT_PADDING`, and reads back the pixel under the
//! pointer: the last point drawn over a pixel wins. This finds that point the way
//! the rasterizer does, from a grid of the points' sprites so a hover stays fast for
//! a hundred thousand points.

use rustryx_theme::authoring::js::js_round;

/// Upstream `HIT_PADDING`: extra diameter, in CSS pixels, each point has in the pick
/// pass only, so hover feels magnetic.
pub(crate) const HIT_PADDING: f64 = 16.;

/// The smallest grid cell, in device pixels, so tiny sprites do not make a huge grid.
const MIN_CELL: f64 = 16.;

/// The pick buffer a set of points draws (upstream's offscreen framebuffer), as a grid
/// of the device pixels each point's sprite covers.
#[derive(Debug, PartialEq)]
pub(crate) struct PickBuffer {
    /// The buffer's size in device pixels (`Math.max(1, Math.round(width * dpr))`).
    pub(crate) tex_w: f64,
    pub(crate) tex_h: f64,
    /// The device pixel ratio the buffer was built with.
    pub(crate) dpr: f64,
    /// The sprite's size in device pixels; `None` when GL draws no points.
    sprite: Option<f32>,
    /// Each point's window position (GL's bottom-left origin), NaN when it does not
    /// rasterize.
    centers: Vec<[f32; 2]>,
    cell: f64,
    cols: usize,
    rows: usize,
    /// The points in each cell, by index (compressed rows: `starts[c]..starts[c + 1]`
    /// of `members`).
    starts: Vec<u32>,
    members: Vec<u32>,
}

impl PickBuffer {
    /// The pick buffer of `positions` (index-aligned with the data, NaN where a point
    /// is missing) in a plot of `width` by `height`, for points of `size` at `dpr`.
    pub(crate) fn new(
        positions: &[[f32; 2]],
        width: f64,
        height: f64,
        size: f64,
        dpr: f64,
    ) -> Self {
        let tex_w = js_round(width * dpr).max(1.);
        let tex_h = js_round(height * dpr).max(1.);
        // `gl.uniform1f(u_size, (size + HIT_PADDING) * dpr)`; GL draws a point at
        // least one pixel wide, and none at a size that is not positive.
        let size_px = ((size + HIT_PADDING) * dpr) as f32;
        let sprite = (size_px > 0.).then(|| size_px.max(1.));
        let (res_w, res_h) = (width as f32, height as f32);
        let (half_w, half_h) = ((tex_w * 0.5) as f32, (tex_h * 0.5) as f32);
        // The vertex shader: `clip = (a_position / u_resolution) * 2.0 - 1.0`, y flipped,
        // then the viewport transform into window pixels.
        let centers: Vec<[f32; 2]> = positions
            .iter()
            .map(|[x, y]| {
                let cx = (x / res_w) * 2. - 1.;
                let cy = -((y / res_h) * 2. - 1.);
                [(cx + 1.) * half_w, (cy + 1.) * half_h]
            })
            .collect();
        let cell = sprite.map_or(MIN_CELL, |s| f64::from(s).max(MIN_CELL));
        // One ring of cells past each edge holds sprites that reach in from outside.
        let cols = (tex_w / cell).floor() as usize + 3;
        let rows = (tex_h / cell).floor() as usize + 3;
        let mut buffer = PickBuffer {
            tex_w,
            tex_h,
            dpr,
            sprite,
            centers,
            cell,
            cols,
            rows,
            starts: Vec::new(),
            members: Vec::new(),
        };
        buffer.bin();
        buffer
    }

    /// The cell a window position falls in, `None` past the ring around the buffer.
    fn cell_of(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        let cx = (x / self.cell).floor() + 1.;
        let cy = (y / self.cell).floor() + 1.;
        (cx >= 0. && cy >= 0. && cx < self.cols as f64 && cy < self.rows as f64)
            .then_some((cx as usize, cy as usize))
    }

    /// Sorts the points into cells, keeping each cell's points in index order.
    fn bin(&mut self) {
        let cells = self.cols * self.rows;
        let mut counts = vec![0u32; cells + 1];
        let mut homes: Vec<Option<usize>> = Vec::with_capacity(self.centers.len());
        for [x, y] in &self.centers {
            let home = if self.sprite.is_some() && x.is_finite() && y.is_finite() {
                self.cell_of(f64::from(*x), f64::from(*y))
                    .map(|(cx, cy)| cy * self.cols + cx)
            } else {
                None
            };
            if let Some(c) = home {
                counts[c + 1] += 1;
            }
            homes.push(home);
        }
        for c in 0..cells {
            counts[c + 1] += counts[c];
        }
        let mut next = counts.clone();
        let mut members = vec![0u32; counts[cells] as usize];
        for (i, home) in homes.iter().enumerate() {
            if let Some(c) = *home {
                members[next[c] as usize] = i as u32;
                next[c] += 1;
            }
        }
        self.starts = counts;
        self.members = members;
    }

    /// Whether point `i`'s sprite covers the fragment centered at (`fx`, `fy`), as the
    /// pick fragment shader decides: `gl_PointCoord - 0.5` within a radius of 0.5.
    fn covers(&self, i: usize, fx: f32, fy: f32, sprite: f32) -> bool {
        let [xw, yw] = self.centers[i];
        let s = 0.5 + (fx - xw) / sprite;
        let t = 0.5 - (fy - yw) / sprite;
        let (u, v) = (s - 0.5, t - 0.5);
        // `if (dot(coord, coord) > 0.25) discard;` (a NaN comparison discards nothing).
        let d = u * u + v * v;
        d.is_nan() || d <= 0.25
    }

    /// The index of the last point drawn over device pixel (`x`, `y`), counted from the
    /// top left, before its color is read back.
    pub(crate) fn top_point(&self, x: f64, y: f64) -> Option<usize> {
        let sprite = self.sprite?;
        // The pixel's center, in GL's bottom-left window coordinates (`readPixels` at
        // `texH - 1 - py`).
        let fx = (x + 0.5) as f32;
        let fy = ((self.tex_h - 1. - y) + 0.5) as f32;
        let (cx, cy) = (
            (f64::from(fx) / self.cell).floor() + 1.,
            (f64::from(fy) / self.cell).floor() + 1.,
        );
        let mut best: Option<usize> = None;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (gx, gy) = (cx + f64::from(dx), cy + f64::from(dy));
                if gx < 0. || gy < 0. || gx >= self.cols as f64 || gy >= self.rows as f64 {
                    continue;
                }
                let c = gy as usize * self.cols + gx as usize;
                let cell = &self.members[self.starts[c] as usize..self.starts[c + 1] as usize];
                // Highest index first: the first that covers the pixel is the cell's last
                // drawn.
                for &i in cell.iter().rev() {
                    let i = i as usize;
                    if best.is_some_and(|b| b >= i) {
                        break;
                    }
                    if self.covers(i, fx, fy, sprite) {
                        best = Some(i);
                        break;
                    }
                }
            }
        }
        best
    }

    /// The point index the pixel reads as (upstream `colorToIndex` of the pixel): the
    /// color encodes `index + 1` in 24 bits, so black, the cleared background, reads
    /// as -1, and indices past 16,777,214 wrap.
    pub(crate) fn pixel_index(&self, x: f64, y: f64) -> i64 {
        match self.top_point(x, y) {
            Some(i) => ((i as i64 + 1) & 0xff_ffff) - 1,
            None => -1,
        }
    }

    /// The hovered index at a pointer position in plot pixels (upstream
    /// `handleMouseMove`): -1 off the buffer or over no point.
    pub(crate) fn hover(&self, local_x: f64, local_y: f64) -> i64 {
        let px = (local_x * self.dpr).floor();
        let py = (local_y * self.dpr).floor();
        if !(px >= 0. && py >= 0. && px < self.tex_w && py < self.tex_h) {
            return -1;
        }
        self.pixel_index(px, py)
    }

    /// The index the pixel reads as, from every point in draw order (for tests).
    #[cfg(test)]
    pub(crate) fn pixel_index_brute_force(&self, x: f64, y: f64) -> i64 {
        let Some(sprite) = self.sprite else {
            return -1;
        };
        let fx = (x + 0.5) as f32;
        let fy = ((self.tex_h - 1. - y) + 0.5) as f32;
        let top = (0..self.centers.len()).rev().find(|&i| {
            let [xw, yw] = self.centers[i];
            xw.is_finite() && yw.is_finite() && self.covers(i, fx, fy, sprite)
        });
        top.map_or(-1, |i| ((i as i64 + 1) & 0xff_ffff) - 1)
    }
}
