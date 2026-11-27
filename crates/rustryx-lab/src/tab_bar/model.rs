//! TabBar's UIKit values (upstream `uikitTabBar.ts`) and its pure geometry (upstream
//! `tabBarModel.ts`): the items' widths and centres, the item nearest a finger, and
//! the lens's target and scale.
//!
//! Every value was read from a stock UITabBarController on the iOS 26.2 simulator,
//! iPhone 17 Pro, in points (CSS px). Springs keep UIKit's terms (response in seconds,
//! damping ratio); no value is a motion role, each plays as measured. Coordinates are
//! the capsule's own, in px, before its flex scale: x runs from its left edge, and
//! items go in logical order.

use rustryx_primitives::motion::rubber_band;

/// A UIKit spring as measured: response in seconds and damping ratio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UikitSpring {
    /// Seconds.
    pub response: f64,
    /// 1 is critically damped; below it the spring overshoots.
    pub damping_ratio: f64,
}

/// The capsule that holds the items: 62 pt tall (upstream `barHeight`).
pub const BAR_HEIGHT: f64 = 62.;
/// The capsule's distance from the screen's leading, trailing and bottom edges
/// (upstream `barInset`).
pub const BAR_INSET: f64 = 21.;
/// Without a search tab the capsule is no wider than 16 + 86 pt per tab (upstream
/// `naturalWidth.base`).
pub const NATURAL_WIDTH_BASE: f64 = 16.;
/// Upstream `naturalWidth.perTab`.
pub const NATURAL_WIDTH_PER_TAB: f64 = 86.;
/// Every item's width for n tabs in a capsule P pt wide is 1.0659 × P ÷ n + 0.0436 × P
/// − 14.61 (upstream `itemWidth.perSlot`).
pub const ITEM_WIDTH_PER_SLOT: f64 = 1.0659;
/// Upstream `itemWidth.perBar`.
pub const ITEM_WIDTH_PER_BAR: f64 = 0.0436;
/// Upstream `itemWidth.offset`.
pub const ITEM_WIDTH_OFFSET: f64 = -14.61;
/// An item's height (upstream `itemHeight`).
pub const ITEM_HEIGHT: f64 = 54.;
/// The first and last items' distance from the capsule's edges, and every item's from
/// its top (upstream `itemInset`).
pub const ITEM_INSET: f64 = 4.;
/// The icon's box, centred in the item (upstream `iconSize`).
pub const ICON_SIZE: f64 = 28.;
/// The icon's centre below the item's top (upstream `iconCenterY`).
pub const ICON_CENTER_Y: f64 = 20.;
/// The label's size: 10 pt, medium; semibold selected (upstream `labelSize`).
pub const LABEL_SIZE: f64 = 10.;
/// The label's 12 pt box starts this far below the item's top (upstream `labelTop`).
pub const LABEL_TOP: f64 = 35.;
/// The badge's height and minimum width, a 55 ÷ 3 pt capsule (upstream
/// `badge.height`).
pub const BADGE_HEIGHT: f64 = 18.333;
/// Horizontal padding around the badge's text (upstream `badge.paddingInline`).
pub const BADGE_PADDING_INLINE: f64 = 4.;
/// The badge's text: 12 pt, medium (upstream `badge.fontSize`).
pub const BADGE_FONT_SIZE: f64 = 12.;
/// The badge's leading edge, this far after the item's centre (upstream
/// `badge.offsetX`).
pub const BADGE_OFFSET_X: f64 = 9.;
/// The badge's top, this far below the item's top (upstream `badge.offsetY`).
pub const BADGE_OFFSET_Y: f64 = 2.;
/// Setting, changing or clearing a badge never animates (upstream `badge.animates`).
pub const BADGE_ANIMATES: bool = false;
/// The lens travelling to a landing finger, and the slide of a selection set without a
/// finger (upstream `lensTravel`).
pub const LENS_TRAVEL: UikitSpring = UikitSpring {
    response: 0.4,
    damping_ratio: 0.85,
};
/// The lens chasing a dragging finger (upstream `lensTrack`).
pub const LENS_TRACK: UikitSpring = UikitSpring {
    response: 0.15,
    damping_ratio: 1.,
};
/// The lens settling onto the committed item after the lift (upstream `lensSettle`).
pub const LENS_SETTLE: UikitSpring = UikitSpring {
    response: 0.5,
    damping_ratio: 1.,
};
/// The lift, one progress from rest to lifted (upstream `lift`).
pub const LIFT: UikitSpring = UikitSpring {
    response: 0.25,
    damping_ratio: 1.,
};
/// How far the lifted lens grows past the item on every side (upstream
/// `liftOutset`).
pub const LIFT_OUTSET: f64 = 8.;
/// The selected copy under the lifted lens grows by this factor about each item's
/// centre (upstream `liftMagnification`).
pub const LIFT_MAGNIFICATION: f64 = 1.16;
/// While a finger is on the bar the capsule grows about its centre by this factor
/// (upstream `flexScale`).
pub const FLEX_SCALE: f64 = 1.0488;
/// The flex growing while the finger is down (upstream `flexPress`).
pub const FLEX_PRESS: UikitSpring = UikitSpring {
    response: 0.314,
    damping_ratio: 0.625,
};
/// The flex relaxing after the lift (upstream `flexRelease`).
pub const FLEX_RELEASE: UikitSpring = UikitSpring {
    response: 0.36,
    damping_ratio: 0.6,
};
/// Past the first or last item's centre the lens rubber-bands over this dimension
/// (upstream `lensEdgeDimension`).
pub const LENS_EDGE_DIMENSION: f64 = 10.5;

/// The widest a capsule of `count` tabs gets without a search tab, px (upstream
/// `naturalWidth`).
pub fn natural_width(count: f64) -> f64 {
    NATURAL_WIDTH_BASE + NATURAL_WIDTH_PER_TAB * count
}

/// The width every item (and the resting lens) has in a capsule `width` px wide, never
/// wider than the capsule's inside (upstream `itemWidth`).
pub fn item_width(width: f64, count: f64) -> f64 {
    let value = (ITEM_WIDTH_PER_SLOT * width) / count.max(1.)
        + ITEM_WIDTH_PER_BAR * width
        + ITEM_WIDTH_OFFSET;
    // JavaScript's Math.min: NaN wins.
    let inside = width - 2. * ITEM_INSET;
    if value.is_nan() || inside.is_nan() {
        f64::NAN
    } else {
        value.min(inside)
    }
}

/// The centres of `count` items in a capsule `width` px wide, in logical order
/// (upstream `itemCenters`): the first and last sit `ITEM_INSET` from the edges and
/// the others are spread evenly between them. Right to left, the first is on the right.
pub fn item_centers(width: f64, count: usize, rtl: bool) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    let size = item_width(width, count as f64);
    let first = ITEM_INSET + size / 2.;
    let centers: Vec<f64> = if count == 1 {
        vec![width / 2.]
    } else {
        (0..count)
            .map(|i| first + ((width - 2. * first) * i as f64) / (count - 1) as f64)
            .collect()
    };
    if rtl {
        centers.into_iter().map(|c| width - c).collect()
    } else {
        centers
    }
}

/// The item nearest a finger at `x` (upstream `nearestIndex`): the one whose centre is
/// nearest, the earlier one on a tie. Float centres can differ by rounding; a
/// difference under 1e-9 px counts as a tie.
pub fn nearest_index(x: f64, centers: &[f64]) -> usize {
    let mut best = 0;
    let mut best_distance = f64::INFINITY;
    for (index, center) in centers.iter().enumerate() {
        let distance = (x - center).abs();
        if distance < best_distance - 1e-9 {
            best = index;
            best_distance = distance;
        }
    }
    best
}

/// Where the lens heads for a finger at `x` (upstream `lensTarget`): the finger itself
/// between the first and last centres, rubber-banded past them.
pub fn lens_target(x: f64, centers: &[f64]) -> f64 {
    if centers.is_empty() {
        return x;
    }
    // JavaScript's Math.min / Math.max over the centres: NaN wins.
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for &c in centers {
        if c.is_nan() || min.is_nan() {
            min = f64::NAN;
            max = f64::NAN;
            continue;
        }
        min = min.min(c);
        max = max.max(c);
    }
    if x < min {
        return min + rubber_band(x - min, LENS_EDGE_DIMENSION);
    }
    if x > max {
        return max + rubber_band(x - max, LENS_EDGE_DIMENSION);
    }
    x
}

/// The lifted lens's scale on each axis for items `width` px wide: the item grown by
/// `LIFT_OUTSET` on every side (upstream `liftScale`).
pub fn lift_scale(width: f64) -> (f64, f64) {
    (
        (width + 2. * LIFT_OUTSET) / width,
        (ITEM_HEIGHT + 2. * LIFT_OUTSET) / ITEM_HEIGHT,
    )
}

/// The transform that puts the selected copy inside the lens back where the items are,
/// for a lens moved by `x` and scaled by (`scale_x`, `scale_y`) about its centre
/// (upstream `copyTransform`): `(x, scale_x, scale_y)`.
pub fn copy_transform(x: f64, scale_x: f64, scale_y: f64) -> (f64, f64, f64) {
    (-x / scale_x, 1. / scale_x, 1. / scale_y)
}

/// More than this many tabs and the bar shows the first four and More (upstream
/// `MAX_TABS`).
pub const MAX_TABS: usize = 5;
