//! The sheet's detent geometry, upstream `BottomSheet/snapOffsets.ts`: snap points
//! resolved to visible heights and turned into offsets from fully open (0 = fully
//! open, larger = more collapsed), the peek among them, the scrim's opacity along
//! them and the settle of a mouse release. Pure numbers in px.

use gpui::{Pixels, SharedString};

/// One of upstream's `snapPoints`: an extra height the sheet can rest at, as its
/// visible height.
#[derive(Clone, Debug, PartialEq)]
pub enum SnapPoint {
    /// A fraction of the viewport in (0, 1] (`0.5` is half the screen), so the stop
    /// tracks rotation and window resizes. A number outside that range is ignored.
    Fraction(f32),
    /// An absolute height that does not scale (`'320px'`).
    Length(Pixels),
    /// Upstream's string form: `'<n>%'` or `'<n>px'`. Anything else (a unitless
    /// string, `calc()`, `vh`, `rem`) is ignored with a dev warning.
    Css(SharedString),
}

/// Detents whose resting offsets land within this many px of each other are one stop
/// (the taller one wins).
pub const DETENT_DEDUP_PX: f64 = 48.;

/// Largest share of the fully open sheet the shortest stop may fill and still be a
/// peek. Above it the stop is a working surface.
pub const PEEK_MAX_HEIGHT_RATIO: f64 = 0.25;

/// Minimum scrim opacity at the peek detent: a modal sheet keeps the page inert, so
/// a fully clear backdrop would read as interactive when it isn't.
pub const MIN_PEEK_SCRIM_OPACITY: f64 = 0.3;

/// A snap point string: a positive number with a `px` or `%` unit.
fn parse_css(text: &str) -> Option<(f64, bool)> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    let (number, percent) = match lower.strip_suffix("px") {
        Some(n) => (n, false),
        None => (lower.strip_suffix('%')?, true),
    };
    // Digits with an optional fraction, or a fraction alone.
    let valid = !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit() || c == '.')
        && number.matches('.').count() <= 1
        && number != "."
        && !number.ends_with('.');
    if !valid {
        return None;
    }
    let value: f64 = number.parse().ok()?;
    (value > 0.).then_some((value, percent))
}

/// One snap point as a visible height, `None` when the sheet cannot honor it.
fn parse_snap_point(point: &SnapPoint, viewport: f64) -> Option<f64> {
    match point {
        SnapPoint::Fraction(f) => {
            let f = f64::from(*f);
            (f.is_finite() && f > 0. && f <= 1.).then_some(f * viewport)
        }
        SnapPoint::Length(px) => {
            let v = f64::from(f32::from(*px));
            (v.is_finite() && v > 0.).then_some(v)
        }
        SnapPoint::Css(text) => {
            parse_css(text).map(|(v, percent)| if percent { v / 100. * viewport } else { v })
        }
    }
}

/// Whether a snap point can be resolved at all.
pub fn is_valid_snap_point(point: &SnapPoint) -> bool {
    parse_snap_point(point, 1.).is_some()
}

/// Snap points as candidate visible heights in px, dropping the ones the sheet
/// cannot honor (callers warn about those).
pub fn resolve_snap_points(points: &[SnapPoint], viewport: f64) -> Vec<f64> {
    points
        .iter()
        .filter_map(|p| parse_snap_point(p, viewport))
        .collect()
}

/// The resting offsets from fully open for a sheet `sheet_height` tall and detents
/// of these visible heights: 0 first, ascending, near-equal ones (`dedup`) merged
/// into the taller.
pub fn compute_detent_offsets(sheet_height: f64, detent_heights: &[f64], dedup: f64) -> Vec<f64> {
    let mut ascending: Vec<f64> = std::iter::once(0.)
        .chain(
            detent_heights
                .iter()
                .filter(|h| **h > 0. && **h < sheet_height)
                .map(|h| sheet_height - h),
        )
        .collect();
    ascending.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut deduped: Vec<f64> = Vec::new();
    for offset in ascending {
        match deduped.last() {
            Some(last) if offset - last < dedup => {}
            _ => deduped.push(offset),
        }
    }
    deduped
}

/// The value in `offsets` nearest `value` (`offsets` is not empty).
pub fn nearest_offset(value: f64, offsets: &[f64]) -> f64 {
    offsets.iter().copied().fold(offsets[0], |best, o| {
        if (o - value).abs() < (best - value).abs() {
            o
        } else {
            best
        }
    })
}

/// The peek detent's offset: the shortest stop when it shows a quarter of the fully
/// open sheet or less; `None` when the sheet has no peek.
pub fn peek_offset_for(offsets: &[f64], visible_sheet_height: f64) -> Option<f64> {
    if offsets.len() < 2 || visible_sheet_height <= 0. {
        return None;
    }
    let shortest = offsets[offsets.len() - 1];
    let height_at_shortest = visible_sheet_height - shortest;
    (height_at_shortest <= PEEK_MAX_HEIGHT_RATIO * visible_sheet_height).then_some(shortest)
}

/// The scrim's opacity (1 = full) at `offset`: full down to the shortest working
/// stop, then thinning onto a peek (to [`MIN_PEEK_SCRIM_OPACITY`]) or out through
/// the dismiss overshoot toward `dismiss_offset` (to 0).
pub fn scrim_opacity_for_offset(
    offset: f64,
    offsets: &[f64],
    dismiss_offset: f64,
    peek_offset: Option<f64>,
) -> f64 {
    let (fade_start, fade_end, floor) = match peek_offset {
        Some(peek) => (offsets[offsets.len() - 2], peek, MIN_PEEK_SCRIM_OPACITY),
        None => (offsets[offsets.len() - 1], dismiss_offset, 0.),
    };
    if offset <= fade_start {
        return 1.;
    }
    if offset >= fade_end {
        return floor;
    }
    1. - (1. - floor) * ((offset - fade_start) / (fade_end - fade_start))
}

/// A released mouse or pen drag's detent: the nearest, among the detents in the
/// drag's direction from `base` (`dir` 1 down, -1 up, 0 neither), so a committed
/// drag never snaps back past where it started.
pub fn resolve_settle_offset(value: f64, offsets: &[f64], dir: i32, base: f64) -> f64 {
    let candidates: Vec<f64> = match dir {
        1 => offsets.iter().copied().filter(|o| *o >= base).collect(),
        -1 => offsets.iter().copied().filter(|o| *o <= base).collect(),
        _ => Vec::new(),
    };
    if candidates.is_empty() {
        nearest_offset(value, offsets)
    } else {
        nearest_offset(value, &candidates)
    }
}

#[cfg(test)]
mod tests {
    //! Upstream `snapOffsets.test.ts`.

    use super::*;
    use gpui::px;

    fn css(s: &'static str) -> SnapPoint {
        SnapPoint::Css(s.into())
    }

    #[test]
    fn resolve_reads_a_bare_number_as_a_fraction_of_the_viewport() {
        assert_eq!(
            resolve_snap_points(&[SnapPoint::Fraction(0.5), SnapPoint::Fraction(0.92)], 800.)
                .iter()
                .map(|v| v.round())
                .collect::<Vec<_>>(),
            [400., 736.]
        );
    }

    #[test]
    fn resolve_reads_a_percentage_as_the_same_fraction() {
        assert_eq!(
            resolve_snap_points(&[css("50%"), css("92%")], 800.),
            [400., 736.]
        );
    }

    #[test]
    fn resolve_reads_a_px_length_as_an_absolute_height() {
        assert_eq!(resolve_snap_points(&[css("320px")], 800.), [320.]);
        assert_eq!(
            resolve_snap_points(&[SnapPoint::Length(px(320.))], 800.),
            [320.]
        );
    }

    #[test]
    fn resolve_keeps_the_caller_order_mixing_forms() {
        assert_eq!(
            resolve_snap_points(&[css("120px"), SnapPoint::Fraction(0.5), css("90%")], 800.),
            [120., 400., 720.]
        );
    }

    #[test]
    fn resolve_accepts_fractional_and_unit_cased_lengths_and_surrounding_space() {
        assert_eq!(
            resolve_snap_points(&[css("12.5%"), css("0.5PX"), css(" 80px ")], 800.),
            [100., 0.5, 80.]
        );
    }

    #[test]
    fn resolve_drops_numbers_outside_the_fraction_range() {
        let points: Vec<SnapPoint> = [0., 0.5, 1., 1.2, -0.3, 200.]
            .into_iter()
            .map(SnapPoint::Fraction)
            .collect();
        assert_eq!(resolve_snap_points(&points, 1000.), [500., 1000.]);
    }

    #[test]
    fn resolve_drops_lengths_it_cannot_resolve_by_arithmetic_on_the_viewport() {
        let points = [
            css("50"),
            css("30rem"),
            css("50vh"),
            css("calc(50% - 20px)"),
            css("-10px"),
            css(""),
            css("0px"),
        ];
        assert!(resolve_snap_points(&points, 800.).is_empty());
    }

    #[test]
    fn is_valid_accepts_the_three_supported_forms() {
        assert!(is_valid_snap_point(&SnapPoint::Fraction(0.5)));
        assert!(is_valid_snap_point(&SnapPoint::Fraction(1.)));
        assert!(is_valid_snap_point(&css("50%")));
        assert!(is_valid_snap_point(&css("320px")));
    }

    #[test]
    fn is_valid_rejects_what_resolve_would_silently_drop() {
        assert!(!is_valid_snap_point(&SnapPoint::Fraction(200.)));
        assert!(!is_valid_snap_point(&SnapPoint::Fraction(0.)));
        assert!(!is_valid_snap_point(&css("50")));
        assert!(!is_valid_snap_point(&css("50vh")));
        assert!(!is_valid_snap_point(&css("calc(50% - 20px)")));
    }

    #[test]
    fn detents_always_include_fully_open_first_ascending() {
        assert_eq!(
            compute_detent_offsets(800., &[400., 640.], DETENT_DEDUP_PX),
            [0., 160., 400.]
        );
    }

    #[test]
    fn detents_drop_heights_at_or_taller_than_the_sheet() {
        assert_eq!(
            compute_detent_offsets(800., &[500., 800., 900.], DETENT_DEDUP_PX),
            [0., 300.]
        );
    }

    #[test]
    fn detents_de_dupe_near_equal_stops_keeping_the_taller() {
        assert_eq!(
            compute_detent_offsets(800., &[760., 400.], DETENT_DEDUP_PX),
            [0., 400.]
        );
    }

    #[test]
    fn detents_keep_stops_farther_apart_than_the_dedup_threshold() {
        assert_eq!(
            compute_detent_offsets(800., &[700., 400.], DETENT_DEDUP_PX),
            [0., 100., 400.]
        );
    }

    #[test]
    fn detents_respect_a_custom_dedup_threshold() {
        assert_eq!(
            compute_detent_offsets(800., &[700., 400.], 150.),
            [0., 400.]
        );
    }

    #[test]
    fn a_single_height_sheet_has_just_the_fully_open_detent() {
        assert_eq!(compute_detent_offsets(600., &[], DETENT_DEDUP_PX), [0.]);
    }

    #[test]
    fn nearest_offset_picks_the_closest() {
        assert_eq!(nearest_offset(180., &[0., 200., 400.]), 200.);
        assert_eq!(nearest_offset(90., &[0., 200., 400.]), 0.);
    }

    const OFFSETS: [f64; 3] = [0., 200., 400.];

    #[test]
    fn a_down_drag_never_settles_above_the_starting_detent() {
        assert_eq!(resolve_settle_offset(240., &OFFSETS, 1, 200.), 200.);
    }

    #[test]
    fn a_down_drag_past_the_midpoint_moves_to_the_next_lower_detent() {
        assert_eq!(resolve_settle_offset(320., &OFFSETS, 1, 200.), 400.);
    }

    #[test]
    fn an_up_drag_never_settles_below_the_starting_detent() {
        assert_eq!(resolve_settle_offset(160., &OFFSETS, -1, 200.), 200.);
    }

    #[test]
    fn an_up_drag_past_the_midpoint_expands_to_the_next_higher_detent() {
        assert_eq!(resolve_settle_offset(80., &OFFSETS, -1, 200.), 0.);
    }

    #[test]
    fn with_no_direction_the_settle_picks_the_plain_nearest() {
        assert_eq!(resolve_settle_offset(180., &OFFSETS, 0, 200.), 200.);
    }

    #[test]
    fn the_peek_is_the_shortest_stop_when_that_stop_is_a_sliver() {
        assert_eq!(peek_offset_for(&[0., 400., 700.], 800.), Some(700.));
    }

    #[test]
    fn no_peek_when_the_shortest_stop_is_a_working_height() {
        assert_eq!(peek_offset_for(&[0., 400.], 800.), None);
    }

    #[test]
    fn no_peek_without_a_collapsed_stop() {
        assert_eq!(peek_offset_for(&[0.], 800.), None);
    }

    #[test]
    fn the_quarter_mark_itself_is_a_peek() {
        assert_eq!(peek_offset_for(&[0., 600.], 800.), Some(600.));
    }

    #[test]
    fn no_peek_without_a_measured_sheet() {
        assert_eq!(peek_offset_for(&[0., 600.], 0.), None);
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn with_a_peek_the_scrim_is_full_at_or_above_the_mid_detent() {
        let o = [0., 100., 200.];
        for offset in [0., 100., 60.] {
            assert_eq!(scrim_opacity_for_offset(offset, &o, 280., Some(200.)), 1.);
        }
    }

    #[test]
    fn with_a_peek_the_scrim_fades_linearly_from_the_mid_detent_toward_the_floor() {
        let o = [0., 100., 200.];
        assert!(near(
            scrim_opacity_for_offset(150., &o, 280., Some(200.)),
            0.65
        ));
    }

    #[test]
    fn with_a_peek_the_scrim_thins_to_the_floor_at_or_below_the_peek() {
        let o = [0., 100., 200.];
        assert!(near(
            scrim_opacity_for_offset(200., &o, 280., Some(200.)),
            0.3
        ));
        assert!(near(
            scrim_opacity_for_offset(240., &o, 280., Some(200.)),
            0.3
        ));
    }

    #[test]
    fn without_a_peek_the_scrim_stays_full_at_every_stop() {
        let o = [0., 100.];
        assert_eq!(scrim_opacity_for_offset(0., &o, 160., None), 1.);
        assert_eq!(scrim_opacity_for_offset(100., &o, 160., None), 1.);
    }

    #[test]
    fn without_a_peek_the_scrim_fades_only_across_the_dismiss_overshoot() {
        let o = [0., 100.];
        assert!(near(scrim_opacity_for_offset(130., &o, 160., None), 0.5));
        assert_eq!(scrim_opacity_for_offset(160., &o, 160., None), 0.);
    }

    #[test]
    fn a_single_detent_scrim_stays_full_until_the_drag_collapses_it() {
        assert_eq!(scrim_opacity_for_offset(0., &[0.], 160., None), 1.);
        assert!(near(scrim_opacity_for_offset(80., &[0.], 160., None), 0.5));
        assert_eq!(scrim_opacity_for_offset(160., &[0.], 160., None), 0.);
    }

    #[test]
    fn the_dedup_threshold_is_a_sane_positive_number() {
        const { assert!(DETENT_DEDUP_PX > 0.) };
    }
}
