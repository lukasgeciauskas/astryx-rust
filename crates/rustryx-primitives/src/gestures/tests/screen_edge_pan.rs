//! Upstream `__tests__/ScreenEdgePanGestureRecognizer.test.ts`.

use std::cell::Cell;
use std::rc::Rc;

use super::harness::*;
use crate::gestures::screen_edge_pan::PhysicalEdge;
use crate::gestures::*;
use crate::motion::uikit::gestures::screen_edge_pan as EDGE;

/// jsdom's `window.innerWidth`.
const INNER_WIDTH: f64 = 1024.;

const CLAIM: NativeTouchBehavior = NativeTouchBehavior {
    pan_x: false,
    pan_y: true,
    pinch_zoom: true,
    double_tap_zoom: false,
};

fn edge(name: &str) -> ScreenEdgePanGestureRecognizer {
    ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions::named(name))
}

fn edges(name: &str, edges: &[ScreenEdge]) -> ScreenEdgePanGestureRecognizer {
    ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions {
        edges: edges.to_vec(),
        ..ScreenEdgePanOptions::named(name)
    })
}

fn rect(left: f64, width: f64, height: f64) -> ViewRect {
    ViewRect {
        left,
        top: 0.,
        width,
        height,
    }
}

/// A view the recognizer measures outside an arena: `viewAt` / `viewOfSize`.
fn view_at(r: &AnyRecognizer, left: f64, width: f64, height: f64, rtl: bool) {
    r.set_environment(Some(Rc::new(TestEnv {
        width: Cell::new(INNER_WIDTH),
        height: Cell::new(768.),
        rtl: Cell::new(rtl),
        view: Cell::new(Some(rect(left, width, height))),
    })));
}

fn began(h: &Harness, name: &str) -> bool {
    h.log()
        .first()
        .is_some_and(|e| e.starts_with(&format!("{name}:began@")))
}

use ScreenEdge::{Leading, Trailing};

#[test]
fn begins_from_the_leading_band_and_fails_elsewhere_at_touch_down() {
    let h = create_harness();
    let edge = edge("edge");
    h.finger(1)
        .down(10., 300., 0., on(&[&edge]))
        .drag(40., 4., 50., 8);
    assert!(began(&h, "edge"));
    assert_eq!(edge.active_edge(), Some(PhysicalEdge::Left));
    let middle = create_harness();
    let miss = self::edge("miss");
    middle.finger(1).down(60., 300., 0., on(&[&miss]));
    assert_eq!(middle.log(), strings(&["miss:failed@0"]));
}

#[test]
fn is_a_fingers_only_a_mouse_or_a_pen_from_the_band_never_begins_it_fr43() {
    for kind in [PointerKind::Mouse, PointerKind::Pen] {
        let h = create_harness();
        let mut options = ScreenEdgePanOptions::named("edge");
        options.base.allowed_pointer_types = Some(vec![
            PointerKind::Touch,
            PointerKind::Mouse,
            PointerKind::Pen,
        ]);
        let edge = ScreenEdgePanGestureRecognizer::new(options);
        h.finger(1)
            .down(10., 300., 0., on(&[&edge]).pointer(kind))
            .drag(40., 4., 50., 8);
        assert_eq!(edge.state(), GestureState::Possible);
        assert!(h.log().is_empty());
    }
    let h = create_harness();
    let edge = edge("edge");
    h.finger(1)
        .down(10., 300., 0., on(&[&edge]).pointer(PointerKind::Touch))
        .drag(40., 4., 50., 8);
    assert!(began(&h, "edge"));
}

#[test]
fn forgets_the_edge_a_gesture_started_from_once_it_ends() {
    let h = create_harness();
    let edge = edge("edge");
    h.finger(1)
        .down(10., 300., 0., on(&[&edge]))
        .drag(40., 4., 50., 8);
    assert_eq!(edge.active_edge(), Some(PhysicalEdge::Left));
    h.finger(1).up(80.);
    assert_eq!(edge.active_edge(), None);
}

#[test]
fn resolves_the_leading_edge_under_rtl_and_requires_inward_movement() {
    let h = create_harness();
    h.env.rtl.set(true);
    let edge = edge("edge");
    h.finger(1)
        .down(385., 300., 0., on(&[&edge]))
        .drag(-40., 2., 50., 8);
    assert_eq!(edge.active_edge(), Some(PhysicalEdge::Right));
    assert!(began(&h, "edge"));
    let outward = create_harness();
    let back = self::edge("back");
    outward
        .finger(1)
        .down(10., 300., 0., on(&[&back]))
        .drag(-8., 30., 50., 8);
    assert_eq!(back.state(), GestureState::Failed);
}

#[test]
fn a_drag_5_pt_inward_and_then_down_the_edge_is_the_pages_scroll_fr19() {
    let h = create_harness();
    let edge = edge("edge");
    h.finger(1)
        .down(10., 300., 0., on(&[&edge]))
        .drag(5., 0., 30., 8)
        .drag(0., 20., 50., 8);
    assert_eq!(edge.state(), GestureState::Failed);
    assert!(h.claims().is_empty());
}

#[test]
fn leaves_a_view_wider_than_the_edge_zone_every_touch_behavior_fr18() {
    assert_eq!(
        ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions::default())
            .native_touch_behavior(),
        ALL_NATIVE_TOUCH
    );
    let page = ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions::default());
    view_at(&page, 0., 390., 844., false);
    assert_eq!(page.native_touch_behavior(), ALL_NATIVE_TOUCH);
    assert_eq!(
        ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions {
            edges: vec![Leading, Trailing],
            ..ScreenEdgePanOptions::default()
        })
        .native_touch_behavior(),
        ALL_NATIVE_TOUCH
    );
}

#[test]
fn takes_the_axis_across_its_edge_in_the_edge_zone_fr18() {
    let leading = ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions::default());
    let region = leading.behavior().edge_region;
    view_at(&leading, 0., region, 844., false);
    assert_eq!(leading.native_touch_behavior(), CLAIM);
    let wide = ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions::default());
    view_at(&wide, 0., region + 1., 844., false);
    assert_eq!(wide.native_touch_behavior(), ALL_NATIVE_TOUCH);
}

#[test]
fn takes_a_strip_authored_at_the_edge_region_as_the_edge_zone_though_layout_lays_it_out_at_40_203_px()
 {
    let strip = ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions::default());
    let region = strip.behavior().edge_region;
    view_at(&strip, 0., (region * 64.).ceil() / 64., 844., false);
    assert_eq!(strip.native_touch_behavior(), CLAIM);
    let h = create_harness();
    let edge = edge("edge");
    h.env.view.set(Some(rect(0., 40.203, 844.)));
    let hysteresis = EDGE::HYSTERESIS;
    h.finger(1)
        .down(10., 300., 0., on(&[&edge]))
        .drag(hysteresis - 1., 0., 30., 8);
    assert_eq!(edge.state(), GestureState::Possible);
    h.finger(1).drag(2., 0., 50., 8);
    assert!(began(&h, "edge"));
}

#[test]
fn in_the_edge_zone_still_decides_at_its_hysteresis() {
    let h = create_harness();
    let edge = edge("edge");
    h.env
        .view
        .set(Some(rect(0., edge.behavior().edge_region, 844.)));
    let hysteresis = EDGE::HYSTERESIS;
    h.finger(1)
        .down(10., 300., 0., on(&[&edge]))
        .drag(hysteresis - 1., 0., 30., 8);
    assert_eq!(edge.state(), GestureState::Possible);
    h.finger(1).drag(2., 0., 50., 8);
    assert!(began(&h, "edge"));
}

// --- where an in-band touch decides (FR18, DEC-22) ----------------------------------

#[test]
fn on_an_element_that_reaches_beyond_the_band_at_10_px_before_the_browser_commits() {
    let h = create_harness();
    let edge = edge("edge");
    h.env.view.set(Some(rect(0., 390., 844.)));
    h.finger(1)
        .down(4., 300., 0., on(&[&edge]))
        .drag(9., 0., 30., 9);
    assert_eq!(edge.state(), GestureState::Possible);
    h.finger(1).drag(1., 0., 10., 1);
    assert!(began(&h, "edge"));
}

#[test]
fn on_an_element_within_the_band_at_15_px() {
    let h = create_harness();
    let edge = edge("edge");
    h.env
        .view
        .set(Some(rect(0., edge.behavior().edge_region, 844.)));
    let hysteresis = EDGE::HYSTERESIS;
    h.finger(1)
        .down(4., 300., 0., on(&[&edge]))
        .drag(hysteresis - 1., 0., 30., 14);
    assert_eq!(edge.state(), GestureState::Possible);
    h.finger(1).drag(1., 0., 10., 1);
    assert!(began(&h, "edge"));
}

#[test]
fn from_several_edges_at_10_px_on_an_element_beyond_their_bands() {
    let h = create_harness();
    let edge = edges("edge", &[Leading, Trailing]);
    h.env.view.set(Some(rect(0., 390., 844.)));
    h.finger(1)
        .down(4., 300., 0., on(&[&edge]))
        .drag(9., 0., 30., 9);
    assert_eq!(edge.state(), GestureState::Possible);
    h.finger(1).drag(1., 0., 10., 1);
    assert!(began(&h, "edge"));
}

// --- a pan from both edges (FR18) -------------------------------------------------------

#[test]
fn a_pan_from_both_edges_outside_every_edge_zone_leaves_the_browser_every_touch_behavior() {
    let edge = edges("edge", &[Leading, Trailing]);
    view_at(&edge, 0., 390., 844., false);
    assert_eq!(edge.native_touch_behavior(), ALL_NATIVE_TOUCH);
}

#[test]
fn a_pan_from_both_edges_inside_either_edge_zone_claims_pan_x_only() {
    let leading = edges("leading", &[Leading, Trailing]);
    let region = leading.behavior().edge_region;
    view_at(&leading, 0., region, 844., false);
    let trailing = edges("trailing", &[Leading, Trailing]);
    view_at(&trailing, INNER_WIDTH - 20., 20., 844., false);
    for edge in [leading, trailing] {
        assert_eq!(edge.native_touch_behavior(), CLAIM);
    }
}

// --- the edge zone (FR18, FR19, DEC-22) -----------------------------------------------

#[test]
fn the_edge_zone_is_a_view_that_lies_within_the_band_at_its_edge_not_merely_a_thin_one() {
    let middle = edge("middle");
    view_at(&middle, 200., 20., 844., false);
    assert_eq!(middle.native_touch_behavior(), ALL_NATIVE_TOUCH);
    let at_leading = edge("leading");
    view_at(&at_leading, 0., 20., 844., false);
    assert_eq!(at_leading.native_touch_behavior(), CLAIM);
    let trailing = edges("trailing", &[Trailing]);
    view_at(&trailing, INNER_WIDTH - 20., 20., 844., false);
    assert_eq!(trailing.native_touch_behavior(), CLAIM);
    let wrong = edges("wrong", &[Trailing]);
    view_at(&wrong, 0., 20., 844., false);
    assert_eq!(wrong.native_touch_behavior(), ALL_NATIVE_TOUCH);
}

#[test]
fn the_edge_zone_is_measured_once_for_both_and_again_on_a_layout_change_which_says_so() {
    let h = create_harness();
    let edge = edge("edge");
    let changes = Rc::new(Cell::new(0));
    let count = changes.clone();
    edge.set_on_touch_behavior_change(Some(Rc::new(move || count.set(count.get() + 1))));
    let hysteresis = EDGE::HYSTERESIS;
    let region = edge.behavior().edge_region;
    h.env.view.set(Some(rect(0., region, 844.)));
    let steps = (hysteresis - 1.) as u32;
    h.finger(1)
        .down(4., 300., 0., on(&[&edge]))
        .drag(hysteresis - 1., 0., 30., steps);
    assert_eq!(edge.state(), GestureState::Possible);
    h.finger(1).up(40.);
    assert!(!edge.native_touch_behavior().pan_x);
    h.env.view.set(Some(rect(0., 390., 844.)));
    assert!(!edge.native_touch_behavior().pan_x);
    h.finger(2)
        .down(4., 300., 100., on(&[&edge]))
        .drag(hysteresis - 1., 0., 30., steps);
    assert_eq!(edge.state(), GestureState::Possible);
    h.finger(2).up(140.);
    assert_eq!(changes.get(), 0);
    edge.layout_changed();
    assert_eq!(changes.get(), 1);
    assert_eq!(edge.native_touch_behavior(), ALL_NATIVE_TOUCH);
    h.finger(3)
        .down(4., 300., 200., on(&[&edge]))
        .drag(9., 0., 30., 9);
    assert_eq!(edge.state(), GestureState::Possible);
    h.finger(3).drag(1., 0., 10., 1);
    assert_eq!(edge.state(), GestureState::Began);
    edge.layout_changed();
    assert_eq!(changes.get(), 1);
}

// --- what the host watches (FR19, FR24) -------------------------------------------------

#[test]
fn needs_the_viewport_only_for_a_pan_that_watches_the_right_side() {
    let cases: [(&[ScreenEdge], bool, bool); 5] = [
        (&[Leading], false, false),
        (&[Trailing], false, true),
        (&[Leading], true, true),
        (&[Trailing], true, false),
        (&[Leading, Trailing], false, true),
    ];
    for (which, rtl, expected) in cases {
        let pan = edges("pan", which);
        view_at(&pan, 0., 20., 844., rtl);
        assert_eq!(pan.watches_viewport(), expected, "{which:?} rtl {rtl}");
    }
}

// --- a writing direction switched in place (DR13) ----------------------------------------

#[test]
fn measures_its_edges_again_at_touch_down_when_the_view_turns_rtl_with_no_resize_and_announces_the_change()
 {
    let width = INNER_WIDTH;
    let h = create_harness();
    h.env.width.set(width);
    h.env.view.set(Some(rect(0., width, 844.)));
    let pan = edges("edge", &[Leading]);
    let announced = Rc::new(Cell::new(0));
    let count = announced.clone();
    pan.set_on_touch_behavior_change(Some(Rc::new(move || count.set(count.get() + 1))));
    view_at(&pan, 0., width, 844., false);
    assert_eq!(pan.native_touch_behavior(), ALL_NATIVE_TOUCH);
    h.finger(1)
        .down(width - 4., 300., 0., on(&[&pan]))
        .drag(-20., 0., 40., 8);
    assert!(!h.log().iter().any(|l| l.starts_with("edge:began")));
    h.finger(1).up(60.);
    assert_eq!(announced.get(), 0);
    h.env.rtl.set(true);
    let before = h.log().len();
    h.finger(2)
        .down(width - 4., 300., 200., on(&[&pan]))
        .drag(-20., 0., 40., 8);
    assert!(
        h.log()[before..]
            .iter()
            .any(|l| l.starts_with("edge:began"))
    );
    assert_eq!(pan.active_edge(), Some(PhysicalEdge::Right));
    assert_eq!(announced.get(), 1);
    h.finger(2).up(260.);
}

#[test]
fn decides_the_touch_that_measures_again_at_10_px_and_the_next_at_15_px_fr19() {
    let width = INNER_WIDTH;
    let h = create_harness();
    h.env.width.set(width);
    let pan = edges("edge", &[Leading]);
    let region = pan.behavior().edge_region;
    view_at(&pan, width - region, region, 844., false);
    h.env.view.set(Some(rect(width - region, region, 844.)));
    assert_eq!(pan.native_touch_behavior(), ALL_NATIVE_TOUCH);
    h.env.rtl.set(true);
    h.finger(1)
        .down(width - 4., 300., 0., on(&[&pan]))
        .drag(-9., 0., 30., 9);
    assert_eq!(pan.state(), GestureState::Possible);
    h.finger(1).drag(-1., 0., 10., 1);
    assert_eq!(pan.state(), GestureState::Began);
    h.finger(1).up(60.);
    assert!(!pan.native_touch_behavior().pan_x);
    let hysteresis = EDGE::HYSTERESIS;
    let steps = (hysteresis - 1.) as u32;
    h.finger(2)
        .down(width - 4., 300., 200., on(&[&pan]))
        .drag(-(hysteresis - 1.), 0., 30., steps);
    assert_eq!(pan.state(), GestureState::Possible);
    h.finger(2).drag(-1., 0., 10., 1);
    assert_eq!(pan.state(), GestureState::Began);
    h.finger(2).up(260.);
}

// --- each writing direction and edge (FR18, FR19, DEC-13, DEC-22) -------------------------

#[test]
fn each_writing_direction_and_edge_touch_action_and_the_touch_down_decision_agree() {
    let hysteresis = EDGE::HYSTERESIS;
    for rtl in [false, true] {
        for edge in [Leading, Trailing] {
            let physical_left = (edge == Leading) != rtl;
            let width = INNER_WIDTH;
            let h = create_harness();
            h.env.rtl.set(rtl);
            h.env.width.set(width);
            let left = if physical_left { 0. } else { width - 20. };
            h.env.view.set(Some(rect(left, 20., 844.)));
            let pan = edges("edge", &[edge]);
            view_at(&pan, left, 20., 844., rtl);
            assert_eq!(pan.native_touch_behavior(), CLAIM, "{rtl} {edge:?}");
            let x = if physical_left { 4. } else { width - 4. };
            let inward = if physical_left { 1. } else { -1. };
            let steps = (hysteresis - 1.) as u32;
            h.finger(1).down(x, 300., 0., on(&[&pan])).drag(
                inward * (hysteresis - 1.),
                0.,
                30.,
                steps,
            );
            assert_eq!(pan.state(), GestureState::Possible, "{rtl} {edge:?}");
            h.finger(1).drag(inward * 2., 0., 20., 2);
            assert!(pan.state().is_active(), "{rtl} {edge:?}");
            h.finger(1).up(80.);
            h.env.view.set(Some(rect(0., width, 844.)));
            view_at(&pan, 0., width, 844., rtl);
            pan.layout_changed();
            assert_eq!(
                pan.native_touch_behavior(),
                ALL_NATIVE_TOUCH,
                "{rtl} {edge:?}"
            );
            h.finger(2)
                .down(x, 300., 200., on(&[&pan]))
                .drag(inward * 9., 0., 30., 9);
            assert_eq!(pan.state(), GestureState::Possible, "{rtl} {edge:?}");
            h.finger(2).drag(inward, 0., 10., 1);
            assert!(pan.state().is_active(), "{rtl} {edge:?}");
            h.finger(2).up(280.);
        }
    }
}
