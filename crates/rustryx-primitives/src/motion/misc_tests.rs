//! Upstream `motion/reducedMotion.test.ts`, `reducedMotionPlan.test.ts`, and
//! the value cases of `motion/uikit/uikit.test.ts`, `values.test.ts` and
//! `segmented.test.ts`.

use std::collections::BTreeMap;

use super::reduced_motion::*;
use super::reduced_motion_plan::*;
use super::roles::{motion_role_in, motion_role_of};
use super::uikit::{navigation, scroll, segmented, springs, timing, zoom_media};
use super::{
    MotionProperty, SpringDescription, rubber_band_with, settling_duration, solve_decay,
    solve_spring, spring_from_uikit_damping_duration,
};

use MotionProperty::{Opacity, X, Y};

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

// -- reducedMotion.ts ---------------------------------------------------------

#[test]
fn the_fade_is_none_when_motion_is_not_reduced() {
    assert_eq!(reduced_motion_fade(false, true, false), None);
}

#[test]
fn the_fade_is_200_ms_at_most_on_ease_in_out_on_a_coarse_pointer() {
    let fade = reduced_motion_fade(true, true, false).unwrap();
    assert_eq!(fade.duration_ms, timing::REDUCED_MOTION_CROSS_FADE_MS);
    assert_eq!(fade.easing(), "cubic-bezier(0.42, 0, 0.58, 1)");
    assert!(fade.duration_ms > 0. && fade.duration_ms <= 200.);
}

#[test]
fn the_fade_completes_at_once_on_any_other_pointer_unless_a_touch_release_started_it() {
    let instant = reduced_motion_fade(true, false, false).unwrap();
    assert_eq!(
        (instant.duration_ms, instant.easing()),
        (0., "linear".to_string())
    );
    assert_eq!(
        reduced_motion_fade(true, false, true).unwrap().duration_ms,
        timing::REDUCED_MOTION_CROSS_FADE_MS
    );
}

fn fade() -> ReducedMotionFade {
    reduced_motion_fade(true, true, false).unwrap()
}

#[test]
fn a_dip_fades_out_over_half_the_fade_and_holds_0_until_the_change_lands() {
    let dip = ReducedMotionDip::play(fade(), 1., Some(1000.));
    assert_eq!(dip.opacity_at(1000.), 1.);
    assert_eq!(dip.opacity_at(1100.), 0.);
    assert_eq!(dip.opacity_at(5000.), 0.);
    assert!(!dip.is_due(1099.));
    assert!(dip.is_due(1100.));
}

#[test]
fn a_dip_lands_when_the_fade_out_ends_however_late() {
    let dip = ReducedMotionDip::play(fade(), 1., None);
    assert!(!dip.is_due(10_000.));
    let mut dip = dip;
    dip.started(1000.);
    assert!(dip.is_due(10_000.));
}

#[test]
fn a_dip_starts_its_fade_in_where_the_fade_out_ended_counting_the_landing() {
    let land_at = |after: f64| {
        let mut dip = ReducedMotionDip::play(fade(), 1., Some(1000.));
        dip.landed(after);
        dip.fade_in_start().unwrap()
    };
    assert_eq!(land_at(1140.), 1100.);
    assert_eq!(land_at(1100.), 1100.);
    assert_eq!(land_at(1500.), 1450.);
    let mut dip = ReducedMotionDip::play(fade(), 1., Some(1000.));
    dip.landed(1500.);
    assert_eq!(dip.opacity_at(1449.), 0.);
    assert!(close(dip.opacity_at(1550.), 1., 9));
}

#[test]
fn a_dip_starts_its_fade_out_where_the_element_is() {
    let dip = ReducedMotionDip::play(fade(), 0.5, Some(0.));
    assert_eq!(dip.opacity_at(0.), 0.5);
    assert_eq!(dip.opacity_at(100.), 0.);
}

#[test]
fn a_cancelled_dip_lands_nothing() {
    let mut dip = ReducedMotionDip::play(fade(), 1., Some(0.));
    dip.cancel();
    assert!(!dip.is_due(1000.));
    dip.landed(1000.);
    assert_eq!(dip.fade_in_start(), None);
}

#[test]
fn the_css_side_holds_the_same_fade_as_the_table() {
    let parts: Vec<String> = timing::curves::MOVE
        .iter()
        .map(|n| format!("{n}"))
        .collect();
    assert_eq!(
        fade().easing(),
        format!("cubic-bezier({})", parts.join(", "))
    );
    assert!(REDUCED_MOTION_FADE_QUERY.contains("pointer: coarse"));
    assert!(REDUCED_MOTION_INSTANT_QUERY.contains("not (pointer: coarse)"));
}

// -- reducedMotionPlan.ts -----------------------------------------------------------

const MS: f64 = timing::REDUCED_MOTION_CROSS_FADE_MS;

#[derive(Clone, Copy)]
struct View {
    opacity: f64,
    own_opacity: f64,
    writes_own_opacity: bool,
    has_pending_landing: bool,
    has_waiting_dip: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            opacity: 1.,
            own_opacity: 1.,
            writes_own_opacity: false,
            has_pending_landing: false,
            has_waiting_dip: false,
        }
    }
}

impl ReducedMotionView for View {
    fn value_at(&self, property: MotionProperty, _time: f64) -> f64 {
        if property == Opacity {
            self.opacity
        } else {
            0.
        }
    }
    fn opacity_heading(&self) -> f64 {
        1.
    }
    fn own_opacity(&self) -> f64 {
        self.own_opacity
    }
    fn writes_own_opacity(&self) -> bool {
        self.writes_own_opacity
    }
    fn borrowed_opacity(&self) -> Option<f64> {
        None
    }
    fn has_opacity_curve(&self) -> bool {
        false
    }
    fn resting_opacity(&self) -> f64 {
        1.
    }
    fn has_pending_landing(&self) -> bool {
        self.has_pending_landing
    }
    fn has_waiting_dip(&self) -> bool {
        self.has_waiting_dip
    }
}

fn targets(values: &[(MotionProperty, f64)]) -> BTreeMap<MotionProperty, f64> {
    values.iter().copied().collect()
}

fn plan(
    view: View,
    values: &[(MotionProperty, f64)],
    fade: bool,
    role: Option<CrossfadeAs>,
) -> ReducedMotionPlan {
    let targets = targets(values);
    let properties: Vec<MotionProperty> = targets.keys().copied().collect();
    plan_reduced_motion(&view, 0., &targets, &properties, fade, role)
}

fn points(a: f64, b: f64, at: f64) -> Option<Vec<FadePoint>> {
    Some(vec![
        FadePoint { at: 0., value: a },
        FadePoint { at, value: b },
    ])
}

#[test]
fn a_call_naming_the_opacity_a_waiting_dip_returns_to_joins_it() {
    let waiting = View {
        has_pending_landing: true,
        has_waiting_dip: true,
        ..View::default()
    };
    let p = plan(waiting, &[(Opacity, 1.)], true, None);
    assert!(p.joins);
    assert!(p.at_once.is_empty());
    assert_eq!(p.opacity_fade, None);
    assert_eq!(p.landing, None);
}

#[test]
fn a_call_naming_travel_or_another_opacity_does_not_join_a_waiting_dip() {
    let waiting = View {
        has_pending_landing: true,
        has_waiting_dip: true,
        ..View::default()
    };
    assert!(!plan(waiting, &[(Opacity, 1.), (X, 0.)], true, None).joins);
    assert!(!plan(waiting, &[(Opacity, 0.5)], true, None).joins);
}

#[test]
fn without_a_fade_everything_is_written_at_once() {
    let p = plan(View::default(), &[(X, 100.), (Opacity, 0.)], false, None);
    assert_eq!(p.at_once, vec![X, Opacity]);
    assert_eq!(p.opacity_fade, None);
    assert_eq!(p.landing, None);
}

#[test]
fn an_entrance_lands_at_once_and_fades_in() {
    let p = plan(
        View {
            opacity: 0.,
            ..View::default()
        },
        &[(X, 100.), (Opacity, 1.)],
        true,
        None,
    );
    assert_eq!(p.at_once, vec![X]);
    assert_eq!(p.opacity_fade, points(0., 1., MS));
    assert_eq!(p.landing, None);
}

#[test]
fn an_exit_fades_out_in_place_then_lands_its_travel() {
    let p = plan(View::default(), &[(X, 100.), (Opacity, 0.)], true, None);
    assert_eq!(p.opacity_fade, points(1., 0., MS));
    let landing = p.landing.unwrap();
    assert_eq!(landing.at, MS);
    assert_eq!(landing.values, targets(&[(X, 100.)]));
    assert_eq!(p.holds, targets(&[(X, 0.)]));
}

#[test]
fn a_move_whose_opacity_rises_from_a_visible_value_dips() {
    let p = plan(
        View {
            opacity: 0.5,
            ..View::default()
        },
        &[(X, 100.), (Opacity, 1.)],
        true,
        None,
    );
    assert_eq!(p.opacity_fade, points(0.5, 0., MS / 2.));
    let landing = p.landing.unwrap();
    assert_eq!(landing.at, MS / 2.);
    assert_eq!(
        landing.fade_in,
        Some(FadeIn {
            to: 1.,
            ms: MS / 2.,
            is_borrowed: false
        })
    );
}

#[test]
fn a_move_whose_opacity_falls_to_a_visible_value_dips() {
    let p = plan(View::default(), &[(X, 100.), (Opacity, 0.5)], true, None);
    let landing = p.landing.unwrap();
    assert_eq!(landing.at, MS / 2.);
    assert_eq!(
        landing.fade_in,
        Some(FadeIn {
            to: 0.5,
            ms: MS / 2.,
            is_borrowed: false
        })
    );
}

#[test]
fn a_move_alone_dips_on_the_own_opacity_it_borrows() {
    let p = plan(
        View {
            own_opacity: 0.6,
            ..View::default()
        },
        &[(X, 100.)],
        true,
        None,
    );
    assert_eq!(p.borrowed, Some(0.6));
    assert_eq!(p.opacity_fade, points(0.6, 0., MS / 2.));
    let landing = p.landing.unwrap();
    assert_eq!(landing.at, MS / 2.);
    assert_eq!(
        landing.fade_in,
        Some(FadeIn {
            to: 0.6,
            ms: MS / 2.,
            is_borrowed: true
        })
    );
}

#[test]
fn a_hidden_element_lends_nothing_and_the_move_lands_at_once() {
    let p = plan(
        View {
            own_opacity: 0.,
            ..View::default()
        },
        &[(X, 100.)],
        true,
        None,
    );
    assert_eq!(p.at_once, vec![X]);
    assert_eq!(p.opacity_fade, None);
    assert_eq!(p.landing, None);
}

#[test]
fn an_opacity_the_engine_writes_is_owned() {
    let p = plan(
        View {
            own_opacity: 0.,
            writes_own_opacity: true,
            ..View::default()
        },
        &[(X, 100.)],
        true,
        None,
    );
    assert_eq!(p.borrowed, None);
    assert_eq!(
        p.landing.unwrap().fade_in,
        Some(FadeIn {
            to: 1.,
            ms: MS / 2.,
            is_borrowed: false
        })
    );
}

#[test]
fn an_exit_by_role_on_a_surface_no_call_owns_restores_the_own_opacity() {
    let p = plan(
        View {
            own_opacity: 0.6,
            ..View::default()
        },
        &[(Y, 40.)],
        true,
        Some(CrossfadeAs::Exit),
    );
    assert_eq!(p.opacity_fade, points(0.6, 0., MS));
    let landing = p.landing.unwrap();
    assert_eq!(landing.restore, Some(0.6));
    assert!(landing.restore_is_borrowed);
}

// -- motion/uikit -----------------------------------------------------------------

#[test]
fn every_spring_of_the_table_settles() {
    let mut entries: Vec<(&str, SpringDescription)> = springs::TABLE.to_vec();
    entries.push(("scroll.bounce", scroll::BOUNCE));
    for (name, description) in entries {
        let settle = settling_duration(description.to_spring());
        assert!(settle > 0.05 && settle < 2., "{name}: {settle}");
    }
}

#[test]
fn the_alert_spring_is_uikits_legacy_derivation_at_0_404_s() {
    let derived = spring_from_uikit_damping_duration(0.404, 1.).unwrap();
    let SpringDescription::Physics {
        stiffness, damping, ..
    } = springs::ALERT_PRESENT
    else {
        panic!("a physical spring");
    };
    assert!(close(derived.stiffness, stiffness, 1));
    assert!(close(derived.damping, damping, 2));
}

#[test]
fn a_sheets_detent_spring_is_its_presentation_spring_as_rendered() {
    assert!(close(
        springs::SHEET_DETENT.to_spring().natural_frequency(),
        springs::SHEET_PRESENT.to_spring().natural_frequency(),
        6
    ));
}

#[test]
fn scroll_to_top_as_a_decay_lands_on_the_top_along_the_measured_curve() {
    let measured = [1257., 768., 475., 296., 186., 118.];
    let coast = solve_decay(0., 9.7 * 2000., Some(scroll::SCROLL_TO_TOP_RATE));
    assert!(close(coast.rest, 2000., -1));
    for (i, remaining) in measured.iter().enumerate() {
        assert!((2000. - coast.value((i + 1) as f64 * 0.05) - remaining).abs() < 30.);
    }
}

#[test]
fn the_rubber_band_reproduces_uiscrollviews_released_overscroll() {
    for (overflow, shown) in [(90., 47.), (290., 135.), (590., 236.67)] {
        assert!(
            (rubber_band_with(overflow, 874., scroll::RUBBER_BAND_COEFFICIENT) - shown).abs() < 0.5
        );
    }
}

#[test]
fn names_each_platform_preset_for_its_use() {
    let rd = |response, damping_ratio| SpringDescription::ResponseDamping {
        response,
        damping_ratio,
    };
    let db = |duration, bounce| SpringDescription::DurationBounce { duration, bounce };
    assert_eq!(springs::FINGER_TRACKING, rd(0.15, 0.86));
    assert_eq!(springs::EMPHASIS, db(0.5, 0.3));
    assert_eq!(springs::SETTLE, db(0.5, 0.));
    assert_eq!(springs::QUICK_CHANGE, db(0.5, 0.15));
    assert_eq!(
        springs::VIEW_CHANGE,
        SpringDescription::Physics {
            mass: 3.,
            stiffness: 1000.,
            damping: 500.
        }
    );
    assert_eq!(springs::TIMED_CHANGE, db(0.5, 0.));
    assert_eq!(springs::STATE_CHANGE, rd(0.55, 1.));
    assert_eq!(springs::DIRECT_CHANGE, rd(0.5, 0.825));
    assert_eq!(timing::STATE_CHANGE_MS, 250.);
    assert_eq!(timing::curves::VIEW_CHANGE, [0.25, 0.1, 0.25, 1.]);
    assert_eq!(timing::curves::MOVE, [0.42, 0., 0.58, 1.]);
    assert_eq!(timing::curves::EXIT, [0.42, 0., 1., 1.]);
    assert_eq!(timing::curves::ENTRANCE, [0., 0., 0.58, 1.]);
    for name in [
        "smooth",
        "snappy",
        "bouncy",
        "interactive",
        "uikitDefault",
        "swiftUIDefault",
    ] {
        assert!(!springs::TABLE.iter().any(|(key, _)| *key == name));
    }
}

#[test]
fn rows_measured_on_one_spring_stay_equal() {
    use springs::*;
    let groups: [&[SpringDescription]; 6] = [
        &[VIEW_CHANGE, SHEET_PRESENT, SHEET_DISMISS, BANNER],
        &[NAVIGATION_PUSH, NAVIGATION_POP],
        &[SHEET_RELEASE_FAST, SHEET_DISMISS_SWIPE],
        &[
            ALERT_PRESENT,
            ALERT_DISMISS,
            ACTION_SHEET_PRESENT,
            ACTION_SHEET_DISMISS,
        ],
        &[MENU_OPEN, CONTEXT_MENU_LIFT],
        &[MENU_CLOSE, CONTEXT_MENU_DISMISS],
    ];
    for group in groups {
        for row in &group[1..] {
            assert_eq!(*row, group[0]);
        }
    }
}

#[test]
fn uikits_default_spring_has_the_documented_damping_ratio_before_clamping() {
    let SpringDescription::Physics {
        mass,
        stiffness,
        damping,
    } = springs::VIEW_CHANGE
    else {
        panic!("a physical spring");
    };
    assert!(close(damping / (2. * (stiffness * mass).sqrt()), 4.56, 2));
}

#[test]
fn deceleration_rates_are_per_millisecond_factors_fast_below_normal() {
    const { assert!(scroll::DECELERATION_FAST < scroll::DECELERATION_NORMAL) };
    const { assert!(scroll::DECELERATION_NORMAL < 1.) };
}

#[test]
fn curves_are_valid_cubic_bezier_control_points() {
    for [x1, _, x2, _] in [
        timing::curves::VIEW_CHANGE,
        timing::curves::MOVE,
        timing::curves::EXIT,
        timing::curves::ENTRANCE,
    ] {
        assert!(x1 >= 0. && x2 <= 1.);
    }
}

#[test]
fn no_role_reads_the_navigation_or_zoom_constants_that_are_not_motion() {
    assert_eq!(motion_role_in("navigation.pushVelocity"), None);
    assert_eq!(navigation::TRANSITION_MS, 350.);
    assert!(motion_role_of(zoom_media::role::DISMISS_SIZE_SPRING).is_some());
}

/// UIKit lab capture `b5-segmented`: (from, to, samples (ms, pt)).
#[allow(clippy::type_complexity)]
const SEGMENTED: [(f64, f64, [[f64; 2]; 9]); 2] = [
    (
        50.,
        250.,
        [
            [0.0, 50.],
            [69.1, 117.81],
            [135.6, 191.14],
            [206.4, 231.12],
            [267.7, 246.72],
            [333.3, 250.89],
            [403.2, 251.17],
            [470.0, 250.67],
            [536.6, 250.27],
        ],
    ),
    (
        250.,
        50.,
        [
            [0.0, 250.],
            [71.9, 184.42],
            [139.9, 110.35],
            [204.5, 69.55],
            [272.0, 53.5],
            [340.1, 49.15],
            [406.5, 48.82],
            [471.1, 49.32],
            [538.5, 49.72],
        ],
    ),
];

#[test]
fn the_segmented_selection_spring_is_the_fit_of_the_capture() {
    let spring = segmented::SELECTION_SPRING.to_spring();
    assert!(close(spring.response(), 0.4, 6));
    assert!(close(spring.damping_ratio(), 0.85, 6));
}

#[test]
fn the_segmented_selection_spring_matches_the_measured_slides_within_a_point() {
    for (from, to, samples) in SEGMENTED {
        let curve = solve_spring(segmented::SELECTION_SPRING.to_spring(), from, to, 0.);
        let mut best = (f64::INFINITY, f64::INFINITY);
        let mut offset = -6.;
        while offset <= 6. {
            let mut worst: f64 = 0.;
            let mut squares = 0.;
            for [t, x] in samples {
                let error = (curve.value((t + offset).max(0.) / 1000.) - x).abs();
                worst = worst.max(error);
                squares += error * error;
            }
            let rms = (squares / samples.len() as f64).sqrt();
            if rms < best.1 {
                best = (worst, rms);
            }
            offset += 0.25;
        }
        assert!(best.1 < 0.75 && best.0 < 1.5, "{from}→{to}: {best:?}");
    }
}

#[test]
fn the_segmented_slide_overshoots_like_uikit() {
    let curve = solve_spring(segmented::SELECTION_SPRING.to_spring(), 50., 250., 0.);
    let mut peak: f64 = 0.;
    for i in 0..=1000 {
        peak = peak.max(curve.value(f64::from(i) / 1000.));
    }
    assert!(peak - 250. > 0.9 && peak - 250. < 1.6);
    assert!((curve.value(0.6) - 250.).abs() < 0.5);
}

#[test]
fn the_segmented_roles_and_internal_constants() {
    assert_eq!(
        motion_role_of(springs::role::FINGER_TRACKING).unwrap().id,
        "springs.fingerTracking"
    );
    assert_eq!(
        motion_role_of(segmented::role::SELECTION_SPRING)
            .unwrap()
            .id,
        "segmented.selectionSpring"
    );
    assert_eq!(
        motion_role_of(segmented::role::LIFT).unwrap().id,
        "segmented.lift"
    );
    assert_eq!(motion_role_in("segmented.dragOvershoot"), None);
    assert_eq!(motion_role_in("segmented.liftGrowth"), None);
    assert_eq!(segmented::LIFT_GROWTH, [24., 16.]);
    assert_eq!(
        segmented::LIFT,
        SpringDescription::ResponseDamping {
            response: 0.25,
            damping_ratio: 1.
        }
    );
    assert_eq!(motion_role_in("springs.segmentedThumb"), None);
}
