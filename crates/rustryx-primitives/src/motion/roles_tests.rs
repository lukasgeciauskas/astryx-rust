//! Upstream `motion/roles.test.ts` and `motion/bezier.test.ts`.

use super::bezier::{BezierPoints, cubic_bezier};
use super::roles::*;
use super::uikit::{segmented, springs, switch_control, timing};
use super::{MotionSpring, SpringDescription};

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

fn ids() -> Vec<String> {
    list_motion_roles()
        .into_iter()
        .map(|role| role.id)
        .collect()
}

#[test]
fn names_a_role_by_its_table_path_and_its_variable_in_kebab_case() {
    assert!(is_motion_role_id("sheet.present"));
    assert!(is_motion_role_id("timing.curves.move"));
    assert!(!is_motion_role_id("sheet"));
    assert!(!is_motion_role_id("Sheet.present"));
    assert_eq!(
        motion_role_variable("springs.sheetDetent").unwrap(),
        "--motion-springs-sheet-detent"
    );
    assert_eq!(
        motion_role_variable("timing.curves.move").unwrap(),
        "--motion-timing-curves-move"
    );
    assert!(
        motion_role_variable("sheet")
            .unwrap_err()
            .contains("dotted table path")
    );
}

#[test]
fn leaves_a_table_value_fr4_does_not_name_an_internal_constant() {
    let shared = SpringDescription::ResponseDamping {
        response: 0.4,
        damping_ratio: 0.9,
    };
    let leaves = vec![
        ("present".to_string(), TableValue::Spring(shared)),
        ("dismiss".to_string(), TableValue::Spring(shared)),
        ("fadeMs".to_string(), TableValue::Number(200.)),
        (
            "curves.out".to_string(),
            TableValue::Numbers(vec![0., 0., 0.58, 1.]),
        ),
    ];
    register_motion_table("fixture", &leaves, RegisterOptions::default()).unwrap();
    assert_eq!(
        motion_role_of(MotionSpring::role("fixture.present", shared)),
        None
    );
    assert_eq!(motion_role_in("fixture.fadeMs"), None);
    assert_eq!(motion_role_in("fixture.curves.out"), None);
    assert!(!ids().iter().any(|id| id.starts_with("fixture.")));
}

#[test]
fn registers_a_listed_value_and_its_rename() {
    let leaves = vec![
        ("present".to_string(), TableValue::Spring(springs::SETTLE)),
        ("fadeMs".to_string(), TableValue::Number(200.)),
        ("skipMs".to_string(), TableValue::Number(10.)),
    ];
    let options = RegisterOptions {
        roles: &["open", "fadeMs", "skipMs"],
        exclude: &["skipMs"],
        rename: &[("present", "open")],
    };
    register_motion_table("renamed", &leaves, options).unwrap();
    assert_eq!(
        motion_role_in("renamed.present").unwrap().id,
        "renamed.open"
    );
    assert_eq!(
        motion_role("renamed.fadeMs").unwrap().kind,
        MotionRoleKind::Duration
    );
    assert_eq!(motion_role("renamed.skipMs"), None);
    assert!(register_motion_table("Not-a-word", &leaves, options).is_err());
}

/// The roles spec:AST-094 FR4 names and the ones the components' records
/// measure (upstream's generated motion role table).
const EXPECTED_ROLES: [(&str, MotionRoleKind); 37] = {
    use MotionRoleKind::{Curve, Duration, Spring};
    [
        ("picker.tapSpin.durationMs", Duration),
        ("progress.curves.fill", Curve),
        ("progress.fillTrackMs", Duration),
        ("progress.reducedMotionPulseMs", Duration),
        ("segmented.lift", Spring),
        ("segmented.selectionSpring", Spring),
        ("springs.contextMenuDismiss", Spring),
        ("springs.contextMenuLift", Spring),
        ("springs.contextMenuMorph", Spring),
        ("springs.directChange", Spring),
        ("springs.emphasis", Spring),
        ("springs.fingerTracking", Spring),
        ("springs.menuClose", Spring),
        ("springs.menuOpen", Spring),
        ("springs.quickChange", Spring),
        ("springs.settle", Spring),
        ("springs.sheetDismissSwipe", Spring),
        ("springs.sliderDrop", Spring),
        ("springs.sliderLift", Spring),
        ("springs.sliderRelease", Spring),
        ("springs.stateChange", Spring),
        ("springs.subMenuBack", Spring),
        ("springs.subMenuOpen", Spring),
        ("springs.timedChange", Spring),
        ("springs.viewChange", Spring),
        ("switchControl.lift", Spring),
        ("switchControl.minimumLiftMs", Duration),
        ("switchControl.settle", Spring),
        ("switchControl.tracking", Spring),
        ("switchControl.travel", Spring),
        ("switchControl.wellOff", Spring),
        ("switchControl.wellOn", Spring),
        ("timing.curves.entrance", Curve),
        ("timing.curves.exit", Curve),
        ("timing.curves.move", Curve),
        ("timing.curves.viewChange", Curve),
        ("timing.stateChangeMs", Duration),
    ]
};

#[test]
fn has_every_role_fr4_names_and_exactly_the_generated_table() {
    let mut registered: Vec<(String, MotionRoleKind)> = list_motion_roles()
        .into_iter()
        .filter(|role| !role.id.starts_with("fixture.") && !role.id.starts_with("renamed."))
        .map(|role| (role.id, role.kind))
        .collect();
    registered.sort_by(|a, b| a.0.cmp(&b.0));
    let mut expected: Vec<(String, MotionRoleKind)> = EXPECTED_ROLES
        .iter()
        .map(|(id, kind)| (id.to_string(), *kind))
        .collect();
    expected.extend([
        (
            "zoomMedia.dismiss.dismissPositionSpring".to_string(),
            MotionRoleKind::Spring,
        ),
        (
            "zoomMedia.dismiss.dismissSizeSpring".to_string(),
            MotionRoleKind::Spring,
        ),
    ]);
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(registered, expected);
}

#[test]
fn names_the_shared_roles_for_their_use() {
    let leaves = [
        (springs::role::FINGER_TRACKING, "springs.fingerTracking"),
        (springs::role::EMPHASIS, "springs.emphasis"),
        (springs::role::SETTLE, "springs.settle"),
        (springs::role::QUICK_CHANGE, "springs.quickChange"),
        (springs::role::VIEW_CHANGE, "springs.viewChange"),
        (springs::role::TIMED_CHANGE, "springs.timedChange"),
        (springs::role::STATE_CHANGE, "springs.stateChange"),
        (springs::role::DIRECT_CHANGE, "springs.directChange"),
        (springs::role::MENU_OPEN, "springs.menuOpen"),
        (springs::role::MENU_CLOSE, "springs.menuClose"),
        (
            segmented::role::SELECTION_SPRING,
            "segmented.selectionSpring",
        ),
        (switch_control::role::TRAVEL, "switchControl.travel"),
    ];
    for (leaf, id) in leaves {
        assert_eq!(motion_role_of(leaf).unwrap().id, id);
    }
    for (path, points) in [
        ("timing.curves.viewChange", timing::curves::VIEW_CHANGE),
        ("timing.curves.exit", timing::curves::EXIT),
        ("timing.curves.entrance", timing::curves::ENTRANCE),
        ("timing.curves.move", timing::curves::MOVE),
    ] {
        assert_eq!(
            motion_role_in(path).unwrap().value,
            RoleValue::Curve(points)
        );
    }
    assert_eq!(
        motion_role_in(timing::STATE_CHANGE_MS_ROLE).unwrap().id,
        "timing.stateChangeMs"
    );
    for old in [
        "springs.smooth",
        "springs.bouncy",
        "timing.curves.easeInOut",
        "timing.implicitAnimationMs",
    ] {
        assert!(!ids().contains(&old.to_string()));
    }
}

#[test]
fn leaves_the_keyboards_motion_out() {
    assert_eq!(
        motion_role_of(MotionSpring::role("springs.keyboard", springs::KEYBOARD)),
        None
    );
    assert_eq!(motion_role_in("timing.keyboardNotificationMs"), None);
}

#[test]
fn refuses_a_spring_that_never_settles() {
    let spring = |s| RoleValue::Spring(s);
    let err = format_motion_role_value(
        MotionRoleKind::Spring,
        spring(SpringDescription::DurationBounce {
            duration: 0.5,
            bounce: 1.,
        }),
    )
    .unwrap_err();
    assert!(err.to_string().contains("must settle"));
    let err = format_motion_role_value(
        MotionRoleKind::Spring,
        spring(SpringDescription::ResponseDamping {
            response: 0.5,
            damping_ratio: 0.,
        }),
    )
    .unwrap_err();
    assert!(err.to_string().contains("must settle"));
    assert_eq!(
        parse_motion_role_value(MotionRoleKind::Spring, "0.5s 1"),
        None
    );
}

#[test]
fn makes_the_roles_of_the_shared_tables_and_keeps_every_other_value_internal() {
    assert_eq!(
        motion_role_of(springs::role::VIEW_CHANGE).unwrap().id,
        "springs.viewChange"
    );
    assert_eq!(
        motion_role_in("timing.curves.move").unwrap().kind,
        MotionRoleKind::Curve
    );
    assert_eq!(
        motion_role_in("timing.stateChangeMs").unwrap().kind,
        MotionRoleKind::Duration
    );
    assert_eq!(motion_role("springs.sheetPresent"), None);
    assert_eq!(springs::SHEET_PRESENT, springs::VIEW_CHANGE);
    assert_eq!(motion_role("scroll.bounce"), None);
    assert_eq!(motion_role_in("timing.press.flashMs"), None);
    assert_eq!(motion_role_in("navigation.transitionMs"), None);
    assert_eq!(motion_role_in("timing.reducedMotionCrossFadeMs"), None);
    assert_eq!(motion_role("springs.menuReducedMotion"), None);
    assert_eq!(motion_role_in("springs.keyboard"), None);
    assert_eq!(motion_role_in("timing.press.scrollBrakeMs"), None);
    assert_eq!(motion_role_in("scroll.delaysContentTouchesMs"), None);
    assert_eq!(motion_role_in("navigation.parallax"), None);
    assert!(!ids().iter().any(|id| id.starts_with("gestures.")));
    let mut variables: Vec<String> = list_motion_roles()
        .into_iter()
        .map(|role| role.variable)
        .collect();
    let count = variables.len();
    variables.sort();
    variables.dedup();
    assert_eq!(variables.len(), count);
}

#[test]
fn writes_every_table_value_in_css_form_and_reads_back_the_same_motion() {
    for role in list_motion_roles() {
        let text = format_motion_role_value(role.kind, role.value).unwrap();
        let back = parse_motion_role_value(role.kind, &text)
            .unwrap_or_else(|| panic!("{}: {text}", role.id));
        match (role.value, back) {
            (RoleValue::Spring(a), RoleValue::Spring(b)) => {
                let (a, b) = (a.to_spring(), b.to_spring());
                assert!(
                    close(b.natural_frequency() / a.natural_frequency(), 1., 5),
                    "{}",
                    role.id
                );
                assert!(
                    close(b.damping_ratio(), a.damping_ratio(), 5),
                    "{}",
                    role.id
                );
            }
            (value, back) => assert_eq!(back, value, "{}", role.id),
        }
    }
    let rd = SpringDescription::ResponseDamping {
        response: 0.5,
        damping_ratio: 0.85,
    };
    assert_eq!(
        format_motion_role_value(MotionRoleKind::Spring, RoleValue::Spring(rd)).unwrap(),
        "0.5s 0.15"
    );
    assert_eq!(
        format_motion_role_value(MotionRoleKind::Duration, RoleValue::DurationMs(250.)).unwrap(),
        "250ms"
    );
    assert_eq!(
        format_motion_role_value(
            MotionRoleKind::Curve,
            RoleValue::Curve([0.42, 0., 0.58, 1.])
        )
        .unwrap(),
        "cubic-bezier(0.42, 0, 0.58, 1)"
    );
}

#[test]
fn refuses_invalid_values_and_reads_malformed_css_as_unset() {
    use MotionRoleKind::{Curve, Duration, Spring};
    assert!(
        format_motion_role_value(Duration, RoleValue::DurationMs(-1.))
            .unwrap_err()
            .to_string()
            .contains("≥ 0")
    );
    assert!(
        format_motion_role_value(Curve, RoleValue::Curve([2., 0., 0.5, 1.]))
            .unwrap_err()
            .to_string()
            .contains("x1 and x2")
    );
    assert!(
        format_motion_role_value(Spring, RoleValue::DurationMs(0.5))
            .unwrap_err()
            .to_string()
            .contains("spring is")
    );
    for never in [
        SpringDescription::DurationBounce {
            duration: 0.5,
            bounce: 1.,
        },
        SpringDescription::DurationBounce {
            duration: 0.5,
            bounce: 1.5,
        },
        SpringDescription::ResponseDamping {
            response: 0.5,
            damping_ratio: 0.,
        },
        SpringDescription::Physics {
            mass: 1.,
            stiffness: 100.,
            damping: 0.,
        },
    ] {
        let err = format_motion_role_value(Spring, RoleValue::Spring(never)).unwrap_err();
        assert!(err.to_string().contains("must settle"), "{never:?}");
    }
    assert_eq!(parse_motion_role_value(Spring, "0.5s 1"), None);
    assert_eq!(parse_motion_role_value(Spring, ""), None);
    assert_eq!(parse_motion_role_value(Spring, "fast"), None);
    assert_eq!(
        parse_motion_role_value(Spring, "0.4s 0.2"),
        Some(RoleValue::Spring(SpringDescription::DurationBounce {
            duration: 0.4,
            bounce: 0.2
        }))
    );
    assert_eq!(
        parse_motion_role_value(Duration, " 0.3s "),
        Some(RoleValue::DurationMs(300.))
    );
    assert_eq!(parse_motion_role_value(Curve, "ease-in"), None);
}

#[test]
fn parse_css_easing_reads_keywords_and_cubic_bezier() {
    assert_eq!(parse_css_easing("linear"), Some([0., 0., 1., 1.]));
    assert_eq!(parse_css_easing(" ease-out "), Some([0., 0., 0.58, 1.]));
    assert_eq!(
        parse_css_easing("cubic-bezier(0.24, 1, 0.4, 1)"),
        Some([0.24, 1., 0.4, 1.])
    );
}

#[test]
fn parse_css_easing_gives_none_for_what_is_not_one_curve() {
    assert_eq!(parse_css_easing("steps(4)"), None);
    assert_eq!(parse_css_easing(""), None);
    assert_eq!(parse_css_easing("cubic-bezier(2, 0, 0.5, 1)"), None);
}

const CURVES: [BezierPoints; 6] = [
    timing::curves::VIEW_CHANGE,
    timing::curves::MOVE,
    timing::curves::EXIT,
    timing::curves::ENTRANCE,
    [0.24, 1., 0.4, 1.],
    [0.3, 0., 0.6, 0.6],
];

#[test]
fn cubic_bezier_matches_the_parametric_curve() {
    for points in CURVES {
        let [x1, y1, x2, y2] = points;
        let ease = cubic_bezier(points);
        let axis = |a: f64, b: f64, t: f64| {
            3. * a * t * (1. - t).powi(2) + 3. * b * t * t * (1. - t) + t.powi(3)
        };
        for i in 0..=256 {
            let t = f64::from(i) / 256.;
            assert!(
                close(ease.ease(axis(x1, x2, t)), axis(y1, y2, t), 5),
                "{points:?} {t}"
            );
        }
    }
}

#[test]
fn cubic_bezier_runs_from_0_to_1_and_clamps_outside() {
    for points in CURVES {
        let ease = cubic_bezier(points);
        assert_eq!(ease.ease(0.), 0.);
        assert_eq!(ease.ease(1.), 1.);
        assert_eq!(ease.ease(-0.5), 0.);
        assert_eq!(ease.ease(1.5), 1.);
    }
}

#[test]
fn ease_in_out_is_symmetric_about_its_midpoint() {
    let ease = cubic_bezier(timing::curves::MOVE);
    assert!(close(ease.ease(0.5), 0.5, 9));
    let mut x = 0.;
    while x <= 0.5 {
        assert!(close(ease.ease(x) + ease.ease(1. - x), 1., 6));
        x += 0.05;
    }
}
