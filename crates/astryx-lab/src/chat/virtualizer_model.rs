//! The geometry of ChatVirtualizer (upstream's `recompute`, `indexAt`, `computeWindow`,
//! `targetFor`, `anchorAt` and the size buckets' `settle`): the rows' offsets from their
//! measured sizes and estimates, the window of rows near the viewport, and the scroll
//! position a declaration asks for. Pure: the component reads and writes the scroller.

use std::collections::HashMap;

use gpui::SharedString;

/// "Did the declarative target land?": a convergence tolerance, px, deliberately not the
/// same knob as `end_threshold` (upstream `CONVERGE_EPSILON_PX`).
pub const CONVERGE_EPSILON_PX: f64 = 4.;
/// At this many real samples a bucket's own data and its inherited baseline carry equal
/// weight (upstream `PRIOR_SAMPLES`).
pub const PRIOR_SAMPLES: f64 = 5.;
/// The bucket every row feeds (upstream `GLOBAL_BUCKET`).
pub const GLOBAL_BUCKET: &str = "\u{0}global";

/// Where every row sits (upstream `Geo`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geo {
    /// Each row's top, px from the content block's top.
    pub offsets: Vec<f64>,
    /// The content block's height.
    pub total: f64,
    /// Each row's key, in order.
    pub keys: Vec<SharedString>,
    /// Each key's row.
    pub index: HashMap<SharedString, usize>,
}

/// The rows' offsets from each row's size: its measurement, else `estimate(i)` (upstream
/// `recompute`).
pub fn recompute(
    keys: Vec<SharedString>,
    measured: &HashMap<SharedString, f64>,
    estimate: impl Fn(usize) -> f64,
) -> Geo {
    let mut offsets = Vec::with_capacity(keys.len());
    let mut index = HashMap::with_capacity(keys.len());
    let mut y = 0.;
    for (i, key) in keys.iter().enumerate() {
        index.insert(key.clone(), i);
        offsets.push(y);
        y += measured.get(key).copied().unwrap_or_else(|| estimate(i));
    }
    Geo {
        offsets,
        total: y,
        keys,
        index,
    }
}

/// The last row whose offset is at most `y` (upstream `indexAt`); 0 for an empty list.
pub fn index_at(offsets: &[f64], y: f64) -> usize {
    let (mut lo, mut hi) = (0isize, offsets.len() as isize - 1);
    while lo < hi {
        let mid = (lo + hi + 1) >> 1;
        if offsets[mid as usize] <= y {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo.max(0) as usize
}

/// The rows to render, `start..=end` (upstream `Win`); `end` is -1 for none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Win {
    /// The first row rendered.
    pub start: usize,
    /// The last row rendered; -1 for none.
    pub end: isize,
}

impl Win {
    /// No rows.
    pub const EMPTY: Win = Win { start: 0, end: -1 };
}

/// The window for a scroll position (upstream `computeWindow`): the rows from
/// `overscan_top` above the viewport to `overscan_bottom` below it. `origin` is where the
/// content block starts in the scroller. When the query reaches past the content, the
/// last row is included by construction: a zero-height final row sits at `total`, which
/// the search can never select.
pub fn compute_window(
    geo: &Geo,
    scroll_top: f64,
    client_height: f64,
    origin: f64,
    overscan_top: f64,
    overscan_bottom: f64,
) -> Win {
    if geo.keys.is_empty() {
        return Win::EMPTY;
    }
    let top = scroll_top - origin;
    let start = index_at(&geo.offsets, (top - overscan_top).max(0.));
    let bottom_edge = top + client_height + overscan_bottom;
    let end = if bottom_edge >= geo.total {
        geo.keys.len() - 1
    } else {
        index_at(&geo.offsets, bottom_edge)
    };
    Win {
        start,
        end: end as isize,
    }
}

/// What the scroll position keeps (upstream `Mode`).
#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    /// Be `distance` px above the bottom edge; 0 follows the end.
    End { distance: f64 },
    /// Hold row `key` at `viewport_offset` px from the viewport's top.
    Anchor {
        key: SharedString,
        viewport_offset: f64,
    },
}

/// The scroll position a mode asks for, clamped to the scrollable range (upstream
/// `targetFor`); none when the anchored row is not in the data. `below` is content past
/// the block.
pub fn target_for(
    mode: &Mode,
    geo: &Geo,
    client_height: f64,
    origin: f64,
    below: f64,
) -> Option<f64> {
    let raw = match mode {
        Mode::End { distance } => origin + geo.total + below - client_height - distance,
        Mode::Anchor {
            key,
            viewport_offset,
        } => {
            let idx = *geo.index.get(key)?;
            origin + geo.offsets[idx] - viewport_offset
        }
    };
    // JavaScript's Math.max(0, Math.min(raw, max)).
    let max = origin + geo.total + below - client_height;
    let min = if raw.is_nan() || max.is_nan() {
        f64::NAN
    } else {
        raw.min(max)
    };
    Some(if min.is_nan() { f64::NAN } else { min.max(0.) })
}

/// The anchor at a scroll position (upstream `anchorAt`): the row at the viewport's top
/// and how far its top is from it.
pub fn anchor_at(geo: &Geo, scroll_top: f64, origin: f64) -> Mode {
    let y = scroll_top - origin;
    let idx = index_at(&geo.offsets, y);
    Mode::Anchor {
        key: geo.keys.get(idx).cloned().unwrap_or_default(),
        // An empty list has no row there: upstream reads `undefined - y`, NaN.
        viewport_offset: geo.offsets.get(idx).copied().unwrap_or(f64::NAN) - y,
    }
}

/// A running size average (upstream `Bucket`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bucket {
    /// The measured sizes' sum, px.
    pub sum: f64,
    /// How many rows were measured.
    pub count: f64,
    /// The price unmeasured rows of this bucket take (0: not set yet).
    pub eff: f64,
}

/// A bucket's price after a pass (upstream's `settle`): shrinkage toward the inherited
/// `baseline`, which counts as `PRIOR_SAMPLES` virtual samples, repriced only while
/// scrolling is `quiet` and the mean has moved more than 10 %. Returns the price and
/// whether it changed the geometry.
pub fn settle(bucket: Option<&mut Bucket>, baseline: f64, quiet: bool) -> (f64, bool) {
    let Some(b) = bucket.filter(|b| b.count != 0.) else {
        return (baseline, false);
    };
    let raw = (b.sum + PRIOR_SAMPLES * baseline) / (b.count + PRIOR_SAMPLES);
    if b.eff == 0. {
        b.eff = if baseline > 0. { baseline } else { raw };
    }
    let mut changed = false;
    if quiet && (raw - b.eff).abs() > b.eff * 0.1 {
        b.eff = raw;
        changed = true;
    }
    (b.eff, changed)
}

/// A bucket's price for an unmeasured row (upstream `effectiveAvg`): its own, or the
/// baseline before it has one.
pub fn effective_avg(bucket: Option<&Bucket>, baseline: f64) -> f64 {
    match bucket {
        Some(b) if b.count != 0. && b.eff != 0. => b.eff,
        _ => baseline,
    }
}
