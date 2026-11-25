//! The zoom stage's geometry, upstream `Lightbox/zoomStage.ts`: where a
//! zoomed photo may rest, how a pinch or a drag past its limits gives way,
//! the double tap's target, and the drag to dismiss. Pure numbers in stage
//! pixels, measured against UIKit by upstream; the Lightbox drives them.
//!
//! A transform places the photo's fitted box: it draws at `fit.x + x`,
//! `fit.y + y`, `scale` times the fitted size.

/// A point on the stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StagePoint {
    pub x: f64,
    pub y: f64,
}

/// Where the photo's fitted box is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ZoomTransform {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
}

/// A rectangle on the stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The stage and the photo fitted in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ZoomGeometry {
    pub width: f64,
    pub height: f64,
    pub fit: Rect,
    pub min_scale: f64,
    pub max_scale: f64,
    pub double_tap_scale: f64,
}

pub(crate) const IDENTITY: ZoomTransform = ZoomTransform {
    x: 0.,
    y: 0.,
    scale: 1.,
};
const ZOOMED_EPSILON: f64 = 0.001;
/// The keyboard's and the double click's zoom.
pub(crate) const ZOOM_STEP: f64 = 2.;
/// `zoomMedia.overshoot.limitFactor`.
const LIMIT_FACTOR: f64 = 2.;
/// `zoomMedia.viewer.maxPixelZoomMultiple`.
const MAX_PIXEL_ZOOM_MULTIPLE: f64 = 6.;
/// `zoomMedia.viewer.pageGap`: the space between pages as they move.
pub(crate) const PAGE_GAP: f64 = 40.;
/// `zoomMedia.dismiss`.
const SCALE_PER_TRAVEL_SQUARED: f64 = 4.91e-6;
const BACKDROP_ALPHA_PER_TRAVEL_SQUARED: f64 = 1.35e-5;
const CHROME_ALPHA_PER_POINT: f64 = 0.00533;
const CHROME_FADE_START: f64 = 17.8;
const CANCEL_TRAVEL: f64 = 10.;
const PROJECTION_RATE: f64 = 0.998;
/// `scroll.rubberBand.coefficient`: UIScrollView's.
const RUBBER_BAND: f64 = 0.55;

/// Upstream `rubberBand`: how far past an edge a drag of `overflow` shows.
fn rubber_band(overflow: f64, dimension: f64) -> f64 {
    if dimension <= 0. {
        return 0.;
    }
    let x = overflow.abs();
    let banded = (1. - 1. / ((x * RUBBER_BAND) / dimension + 1.)) * dimension;
    banded.copysign(overflow)
}

/// The inverse of [`rubber_band`].
fn rubber_band_inverse(shown: f64, dimension: f64) -> f64 {
    if dimension <= 0. {
        return 0.;
    }
    let y = (shown.abs() / dimension).min(0.999_999);
    let x = (1. / (1. - y) - 1.) * dimension / RUBBER_BAND;
    x.copysign(shown)
}

fn rubber_band_clamp(value: f64, min: f64, max: f64, dimension: f64) -> f64 {
    if value < min {
        min + rubber_band(value - min, dimension)
    } else if value > max {
        max + rubber_band(value - max, dimension)
    } else {
        value
    }
}

fn rubber_band_unclamp(shown: f64, min: f64, max: f64, dimension: f64) -> f64 {
    if shown < min {
        min + rubber_band_inverse(shown - min, dimension)
    } else if shown > max {
        max + rubber_band_inverse(shown - max, dimension)
    } else {
        shown
    }
}

/// The double tap's zoom: filling the stage, or 2× when that is less.
pub(crate) fn double_tap_scale_for(
    width: f64,
    height: f64,
    fit_width: f64,
    fit_height: f64,
) -> f64 {
    if fit_width <= 0. || fit_height <= 0. {
        return ZOOM_STEP;
    }
    ZOOM_STEP.max(width / fit_width).max(height / fit_height)
}

/// The greatest zoom: six times the zoom that shows one image pixel per
/// device pixel, and never less than the double tap's.
pub(crate) fn max_scale_for(
    natural_width: f64,
    fit_width: f64,
    device_pixel_ratio: f64,
    double_tap_scale: f64,
) -> f64 {
    if fit_width <= 0. || natural_width <= 0. || device_pixel_ratio <= 0. {
        return double_tap_scale;
    }
    let pixel_scale = natural_width / (fit_width * device_pixel_ratio);
    double_tap_scale.max(MAX_PIXEL_ZOOM_MULTIPLE * pixel_scale)
}

pub(crate) fn is_zoomed(geometry: &ZoomGeometry, transform: &ZoomTransform) -> bool {
    transform.scale > geometry.min_scale + ZOOMED_EPSILON
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AxisRange {
    pub min: f64,
    pub max: f64,
}

fn axis_range(stage: f64, fit_start: f64, fit_size: f64, scale: f64) -> AxisRange {
    let size = fit_size * scale;
    if size <= stage {
        let centred = (stage - size) / 2. - fit_start;
        return AxisRange {
            min: centred,
            max: centred,
        };
    }
    AxisRange {
        min: stage - size - fit_start,
        max: -fit_start,
    }
}

/// Where a photo at `scale` may rest: centred on an axis it does not fill,
/// with no gap at either edge on one it does.
pub(crate) fn rest_range(geometry: &ZoomGeometry, scale: f64) -> (AxisRange, AxisRange) {
    let fit = geometry.fit;
    (
        axis_range(geometry.width, fit.x, fit.width, scale),
        axis_range(geometry.height, fit.y, fit.height, scale),
    )
}

pub(crate) fn resting_transform(geometry: &ZoomGeometry, t: ZoomTransform) -> ZoomTransform {
    let (x, y) = rest_range(geometry, t.scale);
    ZoomTransform {
        x: t.x.clamp(x.min, x.max),
        y: t.y.clamp(y.min, y.max),
        scale: t.scale,
    }
}

/// Past a zoom limit the zoom gives way, approaching twice the maximum or
/// half the minimum.
pub(crate) fn overshoot_scale(geometry: &ZoomGeometry, scale: f64) -> f64 {
    let f = LIMIT_FACTOR;
    let (m, big_m) = (geometry.min_scale, geometry.max_scale);
    if scale > big_m {
        big_m * (f - ((f - 1.) * big_m) / scale)
    } else if scale < m {
        m / (f - ((f - 1.) * scale) / m)
    } else {
        scale
    }
}

/// The inverse of [`overshoot_scale`]: the pinch scale that shows `shown`.
pub(crate) fn pinch_scale_for(geometry: &ZoomGeometry, shown: f64) -> f64 {
    let f = LIMIT_FACTOR;
    let (m, big_m) = (geometry.min_scale, geometry.max_scale);
    if shown > big_m {
        let gap = f * big_m - shown;
        if gap > 0. {
            ((f - 1.) * big_m * big_m) / gap
        } else {
            f64::INFINITY
        }
    } else if shown < m {
        (((f - m / shown) * m) / (f - 1.)).max(0.)
    } else {
        shown
    }
}

/// The transform at `scale` that shows the photo point under `anchor` (in
/// `from`) at `point`.
pub(crate) fn transform_keeping_point(
    geometry: &ZoomGeometry,
    from: &ZoomTransform,
    anchor: StagePoint,
    point: StagePoint,
    scale: f64,
) -> ZoomTransform {
    let fit = geometry.fit;
    let u = (anchor.x - fit.x - from.x) / from.scale;
    let v = (anchor.y - fit.y - from.y) / from.scale;
    ZoomTransform {
        x: point.x - fit.x - u * scale,
        y: point.y - fit.y - v * scale,
        scale,
    }
}

/// A drag's transform shown: past an edge, UIScrollView's rubber band.
pub(crate) fn dragged_transform(geometry: &ZoomGeometry, raw: ZoomTransform) -> ZoomTransform {
    let (x, y) = rest_range(geometry, raw.scale);
    ZoomTransform {
        x: if x.min == x.max {
            x.min
        } else {
            rubber_band_clamp(raw.x, x.min, x.max, geometry.width)
        },
        y: if y.min == y.max {
            y.min
        } else {
            rubber_band_clamp(raw.y, y.min, y.max, geometry.height)
        },
        scale: raw.scale,
    }
}

/// The inverse of [`dragged_transform`], so a drag continues from a photo
/// shown past its edge.
pub(crate) fn raw_for_dragged(geometry: &ZoomGeometry, shown: ZoomTransform) -> ZoomTransform {
    let (x, y) = rest_range(geometry, shown.scale);
    ZoomTransform {
        x: if x.min == x.max {
            shown.x
        } else {
            rubber_band_unclamp(shown.x, x.min, x.max, geometry.width)
        },
        y: if y.min == y.max {
            shown.y
        } else {
            rubber_band_unclamp(shown.y, y.min, y.max, geometry.height)
        },
        scale: shown.scale,
    }
}

/// A double tap: zoomed, back to the fitted size; else to the double tap's
/// zoom about `point`, kept inside the stage.
pub(crate) fn double_tap_target(
    geometry: &ZoomGeometry,
    from: &ZoomTransform,
    point: StagePoint,
) -> ZoomTransform {
    if is_zoomed(geometry, from) {
        return resting_transform(
            geometry,
            ZoomTransform {
                scale: geometry.min_scale,
                ..IDENTITY
            },
        );
    }
    let scale = geometry
        .double_tap_scale
        .max(geometry.min_scale)
        .min(geometry.max_scale);
    resting_transform(
        geometry,
        transform_keeping_point(geometry, from, point, point, scale),
    )
}

/// A double click: as a double tap, at 2×.
pub(crate) fn double_click_target(
    geometry: &ZoomGeometry,
    from: &ZoomTransform,
    point: StagePoint,
) -> ZoomTransform {
    double_tap_target(
        &ZoomGeometry {
            double_tap_scale: ZOOM_STEP,
            ..*geometry
        },
        from,
        point,
    )
}

/// The keyboard's zoom in: 2× about the stage's centre.
pub(crate) fn released_zoom_target(geometry: &ZoomGeometry, from: &ZoomTransform) -> ZoomTransform {
    double_click_target(
        geometry,
        from,
        StagePoint {
            x: geometry.width / 2.,
            y: geometry.height / 2.,
        },
    )
}

/// A pinch's release: back inside the limits about its last centroid.
pub(crate) fn release_target(
    geometry: &ZoomGeometry,
    shown: ZoomTransform,
    centroid: StagePoint,
) -> ZoomTransform {
    let limit = shown.scale.max(geometry.min_scale).min(geometry.max_scale);
    if limit == shown.scale {
        return resting_transform(geometry, shown);
    }
    resting_transform(
        geometry,
        transform_keeping_point(geometry, &shown, centroid, centroid, limit),
    )
}

/// A pan by `dx`, `dy`, inside the resting bounds.
pub(crate) fn panned_by(
    geometry: &ZoomGeometry,
    from: &ZoomTransform,
    dx: f64,
    dy: f64,
) -> ZoomTransform {
    resting_transform(
        geometry,
        ZoomTransform {
            x: from.x + dx,
            y: from.y + dy,
            scale: from.scale,
        },
    )
}

/// The transform after the stage resizes: a zoomed photo keeps its zoom
/// relative to the fitted size and the point at the centre.
pub(crate) fn transform_for_resize(
    before: &ZoomGeometry,
    after: &ZoomGeometry,
    t: &ZoomTransform,
) -> ZoomTransform {
    if !is_zoomed(before, t) {
        return ZoomTransform {
            scale: after.min_scale,
            ..IDENTITY
        };
    }
    let ratio = if after.fit.width > 0. {
        before.fit.width / after.fit.width
    } else {
        1.
    };
    let scale = (t.scale * ratio).max(after.min_scale).min(after.max_scale);
    let u = (before.width / 2. - before.fit.x - t.x) / t.scale / before.fit.width;
    let v = (before.height / 2. - before.fit.y - t.y) / t.scale / before.fit.height;
    resting_transform(
        after,
        ZoomTransform {
            x: after.width / 2. - after.fit.x - u * after.fit.width * scale,
            y: after.height / 2. - after.fit.y - v * after.fit.height * scale,
            scale,
        },
    )
}

/// A drag to dismiss: the photo follows the finger and shrinks with the
/// travel down.
pub(crate) fn dismiss_transform(geometry: &ZoomGeometry, dx: f64, dy: f64) -> ZoomTransform {
    let travel = dy.max(0.);
    let scale = (1. - SCALE_PER_TRAVEL_SQUARED * travel * travel).max(0.);
    ZoomTransform {
        x: dx + ((1. - scale) * geometry.fit.width) / 2.,
        y: dy + ((1. - scale) * geometry.fit.height) / 2.,
        scale,
    }
}

/// The backdrop's opacity during a drag to dismiss.
pub(crate) fn dismiss_backdrop_alpha(travel: f64) -> f64 {
    let t = travel.max(0.);
    (1. - BACKDROP_ALPHA_PER_TRAVEL_SQUARED * t * t).clamp(0., 1.)
}

/// The controls' opacity during a drag to dismiss.
pub(crate) fn dismiss_chrome_alpha(travel: f64) -> f64 {
    (1. - CHROME_ALPHA_PER_POINT * (travel - CHROME_FADE_START)).clamp(0., 1.)
}

/// Whether a drag to dismiss released at `travel`, moving `velocity_y`
/// (px/s), closes the viewer: its projected travel passes the commit point
/// (UIKit's deceleration projection between rest and twice the cancel
/// travel).
pub(crate) fn dismiss_commits(travel: f64, velocity_y: f64) -> bool {
    let seconds = PROJECTION_RATE / (1. - PROJECTION_RATE) / 1000.;
    let projected = travel + velocity_y * seconds;
    let (rest, commit) = (0., 2. * CANCEL_TRAVEL);
    (projected - commit).abs() < (projected - rest).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Upstream's lab stage: a 402 × 874 screen showing a 3000 × 2000 photo
    /// fitted 402 × 268 at its vertical centre, on a 3× screen.
    fn landscape() -> ZoomGeometry {
        let double_tap_scale = double_tap_scale_for(402., 874., 402., 268.);
        ZoomGeometry {
            width: 402.,
            height: 874.,
            fit: Rect {
                x: 0.,
                y: (874. - 268.) / 2.,
                width: 402.,
                height: 268.,
            },
            min_scale: 1.,
            max_scale: max_scale_for(3000., 402., 3., double_tap_scale),
            double_tap_scale,
        }
    }

    fn close(a: f64, b: f64, digits: i32) -> bool {
        (a - b).abs() < 0.5 * 10f64.powi(-digits)
    }

    #[test]
    fn overshoot_gives_way_as_measured() {
        let g = ZoomGeometry {
            max_scale: 4.,
            ..landscape()
        };
        for (pinch, zoom) in [(4.5, 4.444), (5., 4.8), (5.5, 5.091), (6., 5.333)] {
            assert!(close(overshoot_scale(&g, pinch), zoom, 3));
        }
        for (pinch, zoom) in [(0.85, 0.870), (0.7, 0.769), (0.474, 0.655), (0.323, 0.596)] {
            assert!(close(overshoot_scale(&g, pinch), zoom, 3), "{pinch}");
        }
        assert!(overshoot_scale(&g, 1e6) < 8.);
        for pinch in [0.5, 2., 6.] {
            assert!(close(
                pinch_scale_for(&g, overshoot_scale(&g, pinch)),
                pinch,
                9
            ));
        }
    }

    #[test]
    fn the_double_tap_fills_the_screen_or_zooms_two_times() {
        assert!(close(
            double_tap_scale_for(402., 874., 402., 268.),
            3.261,
            3
        ));
        assert_eq!(double_tap_scale_for(402., 874., 402., 603.), 2.);
        let g = landscape();
        let target = double_tap_target(&g, &IDENTITY, StagePoint { x: 201., y: 437. });
        assert!(close(target.scale, 874. / 268., 9));
        assert!(
            close(g.fit.y + target.y, 0., 9),
            "the photo fills the screen"
        );
        let zoomed = ZoomTransform {
            x: -100.,
            y: -300.,
            scale: 3.,
        };
        assert_eq!(
            double_tap_target(&g, &zoomed, StagePoint { x: 50., y: 50. }).scale,
            1.
        );
    }

    #[test]
    fn the_maximum_is_six_times_one_pixel_per_device_pixel() {
        assert!(close(max_scale_for(3000., 402., 3., 3.261), 14.93, 2));
        assert!(close(max_scale_for(2000., 402., 3., 2.), 9.95, 2));
        assert_eq!(max_scale_for(200., 402., 3., 3.261), 3.261);
    }

    #[test]
    fn resting_places_centre_or_reach_the_edges() {
        let g = landscape();
        let (x, y) = rest_range(&g, 2.);
        assert_eq!(
            x,
            AxisRange {
                min: 402. - 804.,
                max: 0.
            }
        );
        assert_eq!(y.min, y.max);
        assert!(close(g.fit.y + y.min + 536. / 2., 874. / 2., 9));
        assert_eq!(
            resting_transform(
                &g,
                ZoomTransform {
                    x: 50.,
                    y: 0.,
                    scale: 2.
                }
            )
            .x,
            0.
        );
        let from = ZoomTransform {
            x: -10.,
            y: 0.,
            scale: 1.15,
        };
        assert_eq!(panned_by(&g, &from, 50., 0.).x, 0.);
        assert!(close(panned_by(&g, &from, -50., 0.).x, -60., 9));
    }

    #[test]
    fn a_drag_past_an_edge_rubber_bands_and_inverts() {
        let g = landscape();
        let raw = ZoomTransform {
            x: -200.,
            y: 0.,
            scale: 2.,
        };
        assert_eq!(dragged_transform(&g, raw).x, -200.);
        let past = ZoomTransform {
            x: 100.,
            y: 0.,
            scale: 2.,
        };
        let shown = dragged_transform(&g, past);
        assert!(shown.x > 0. && shown.x < 100., "it gives way");
        assert!(close(raw_for_dragged(&g, shown).x, 100., 6));
    }

    #[test]
    fn a_point_stays_under_the_centroid_as_the_zoom_changes() {
        let g = landscape();
        let from = ZoomTransform {
            x: -50.,
            y: -20.,
            scale: 2.,
        };
        let centroid = StagePoint { x: 150., y: 400. };
        let next = transform_keeping_point(&g, &from, centroid, centroid, 4.2);
        let u = (centroid.x - g.fit.x - from.x) / from.scale;
        assert!(close(g.fit.x + next.x + u * next.scale, centroid.x, 9));
        assert_eq!(next.scale, 4.2);
    }

    #[test]
    fn a_drag_down_shrinks_the_photo_and_commits_past_its_projection() {
        let g = landscape();
        let t = dismiss_transform(&g, 0., 100.);
        assert!(t.scale < 1. && t.scale > 0.9);
        assert!(dismiss_backdrop_alpha(100.) < 1.);
        assert_eq!(dismiss_chrome_alpha(10.), 1.);
        assert!(!dismiss_commits(5., 0.), "a short, slow drag springs back");
        assert!(dismiss_commits(30., 0.), "past twice the cancel travel");
        assert!(dismiss_commits(2., 200.), "a quick flick down");
        assert!(!dismiss_commits(40., -200.), "moving back up");
    }
}
