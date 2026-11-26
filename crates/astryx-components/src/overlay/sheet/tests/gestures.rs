//! Upstream `useSheetGestures.test.ts`'s model suites, on [`SheetGestures`] fed by
//! hand: a 400 px sheet (FR19's release under a finger and under a mouse, FR18's
//! stretch, the detents, the peek and the scrim, the magnet, re-anchoring, and a
//! dismissal the owner refuses).

use astryx_primitives::gestures::PointerKind;

use crate::overlay::sheet::gestures::{Release, SheetDragExit, SheetGestures};

const SHEET_HEIGHT: f64 = 400.;

struct Rig {
    g: SheetGestures,
    dismissed: u32,
    snaps: Vec<f64>,
    scrims: Vec<f64>,
    exits: Vec<Option<SheetDragExit>>,
}

fn setup(snaps: &[f64]) -> Rig {
    let mut g = SheetGestures::default();
    g.can_dismiss = true;
    g.snap_heights = snaps.to_vec();
    g.sheet_height = SHEET_HEIGHT;
    g.reset();
    Rig {
        g,
        dismissed: 0,
        snaps: Vec::new(),
        scrims: Vec::new(),
        exits: Vec::new(),
    }
}

impl Rig {
    fn down(&mut self, pointer: PointerKind, y: f64, t: f64) {
        self.g.begin(pointer, y, t, None, None, 0.);
    }
    fn touch(&mut self, y: f64, t: f64) {
        self.down(PointerKind::Touch, y, t);
    }
    fn mouse(&mut self, y: f64, t: f64) {
        self.down(PointerKind::Mouse, y, t);
    }
    fn mv(&mut self, y: f64, t: f64) -> f64 {
        let moved = self.g.move_to(y, t).expect("dragging");
        self.scrims.push(moved.scrim);
        moved.offset
    }
    fn up(&mut self, y: f64, t: f64) {
        match self.g.end(y, t).expect("dragging") {
            Release::Settle { offset, scrim, .. } => {
                self.snaps.push(SHEET_HEIGHT - self.g.reserve - offset);
                self.scrims.push(scrim);
            }
            Release::Dismiss { exit } => {
                self.dismissed += 1;
                self.exits.push(exit);
            }
        }
    }
    fn settled(&self) -> f64 {
        self.g.settled_offset
    }
    fn last_snap(&self) -> Option<f64> {
        self.snaps.last().copied()
    }
    /// Settles on the 200 px detent with a slow drag.
    fn rest_at_200(&mut self) {
        self.touch(0., 0.);
        self.mv(90., 300.);
        self.mv(180., 700.);
        self.up(180., 1100.);
        assert_eq!(self.last_snap(), Some(200.));
    }
}

// --- A touch release takes UIKit's sheet values (FR19, DEC-4) -------------------------

#[test]
fn fr19_closes_from_the_tallest_detent_on_a_flick_that_lands_nearer_closed_than_any_detent() {
    let mut r = setup(&[200.]);
    // 60 px in 20 ms = 3000 px/s: projected 0.1 s ahead it lands at 360 px.
    r.touch(0., 0.);
    r.mv(30., 10.);
    r.mv(60., 20.);
    r.up(60., 22.);
    assert_eq!(r.dismissed, 1);
}

#[test]
fn fr19_closes_from_the_tallest_detent_on_a_flick_whose_projection_passes_the_next_detent() {
    let mut r = setup(&[200.]);
    // 1500 px/s: lands at 210 px, past the 200 px detent, so on to closed.
    r.touch(0., 0.);
    r.mv(30., 20.);
    r.mv(60., 40.);
    r.up(60., 42.);
    assert_eq!(r.dismissed, 1);
}

#[test]
fn fr19_takes_a_softer_flick_to_the_next_detent_when_its_projection_stops_short() {
    let mut r = setup(&[200.]);
    // 1200 px/s: lands at 180 px, so one position down.
    r.touch(0., 0.);
    r.mv(30., 25.);
    r.mv(60., 50.);
    r.up(60., 52.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.last_snap(), Some(200.));
}

#[test]
fn fr19_keeps_a_long_release_under_the_flick_speed_from_closing_above_the_lowest_detent() {
    let mut r = setup(&[200.]);
    r.touch(0., 0.);
    r.mv(40., 50.);
    r.mv(80., 100.);
    r.mv(100., 125.);
    r.up(100., 127.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.last_snap(), Some(200.));
}

#[test]
fn fr19_closes_on_a_flick_at_the_lowest_of_two_detents() {
    let mut r = setup(&[200.]);
    r.rest_at_200();
    r.touch(200., 2000.);
    r.mv(230., 2020.);
    r.mv(260., 2040.);
    r.up(260., 2042.);
    assert_eq!(r.dismissed, 1);
}

#[test]
fn fr7_settles_instead_of_dismissing_on_a_downward_flick_when_dismissal_is_off() {
    let mut r = setup(&[200.]);
    r.g.can_dismiss = false;
    r.mouse(0., 0.);
    r.mv(30., 10.);
    r.mv(60., 20.);
    r.up(60., 22.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 200.);
    // A half-height stop is a working surface: the scrim stays full.
    assert_eq!(r.scrims.last(), Some(&1.));
}

#[test]
fn fr19_expands_to_the_tallest_detent_on_a_fast_upward_flick() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(200., 400.);
    r.up(200., 800.);
    r.mouse(200., 1000.);
    r.mv(170., 1010.);
    r.mv(140., 1020.);
    r.up(140., 1022.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.last_snap(), Some(SHEET_HEIGHT));
    assert_eq!(r.settled(), 0.);
}

// --- A mouse release projects as a thrown scroll view ------------------------------------

#[test]
fn fr19_a_mouse_dismisses_on_a_medium_throw_released_short_of_the_dismiss_line() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(50., 50.);
    r.mv(100., 100.);
    r.mv(150., 150.);
    r.up(150., 152.);
    assert_eq!(r.dismissed, 1);
}

#[test]
fn fr19_a_mouse_lands_a_gentle_throw_on_the_next_detent() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(30., 100.);
    r.mv(60., 200.);
    r.up(60., 202.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 200.);
}

#[test]
fn fr19_keeps_a_throw_whose_last_sample_is_slow() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(60., 20.);
    r.mv(62., 90.);
    r.up(62., 92.);
    assert_eq!(r.dismissed, 1);
}

#[test]
fn fr19_weighs_a_hold_into_the_first_move_after_it() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(0., 250.);
    r.mv(30., 275.);
    r.mv(60., 300.);
    r.up(60., 302.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 200.);
}

#[test]
fn fr19_releases_a_single_move_at_no_speed() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(60., 300.);
    r.up(60., 302.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 0.);
}

#[test]
fn fr19_releases_no_throw_from_a_finger_that_rested_before_lifting() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(60., 20.);
    r.up(60., 400.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 0.);
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(30., 10.);
    r.mv(60., 20.);
    r.up(60., 70.);
    assert_eq!(r.dismissed, 0, "50 ms of rest is past the stillness");
}

#[test]
fn fr19_keeps_a_throw_after_a_30ms_rest_before_the_lift() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(30., 10.);
    r.mv(60., 20.);
    r.up(60., 50.);
    assert_eq!(r.dismissed, 1);
}

#[test]
fn fr7_settles_a_throw_past_the_line_at_the_shortest_detent_when_dismissal_is_off() {
    let mut r = setup(&[200.]);
    r.g.can_dismiss = false;
    r.mouse(0., 0.);
    r.mv(50., 50.);
    r.mv(100., 100.);
    r.up(100., 102.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 200.);
}

#[test]
fn fr19_lands_a_flick_on_the_next_detent_when_an_early_move_arrives_late() {
    let mut r = setup(&[100.]);
    r.touch(0., 0.);
    for k in 1..=7 {
        r.mv(9.6 * f64::from(k), 8. * f64::from(k));
    }
    r.mv(76.8, 134.);
    r.mv(86.4, 142.);
    r.mv(96., 150.);
    r.up(96., 152.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.last_snap(), Some(100.));
}

#[test]
fn fr19_reads_a_lift_at_a_new_position_as_the_last_move() {
    let mut r = setup(&[100.]);
    r.touch(0., 0.);
    r.mv(9., 10.);
    r.mv(18., 20.);
    r.up(48., 30.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.last_snap(), Some(100.));
}

#[test]
fn fr19_takes_a_fast_short_flick_from_the_tallest_of_two_detents_to_the_next_one_down() {
    let mut r = setup(&[200.]);
    r.touch(0., 0.);
    r.mv(10., 8.);
    r.mv(20., 16.);
    r.up(20., 18.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.last_snap(), Some(200.));
}

#[test]
fn fr19_closes_on_a_fast_short_flick_at_the_lowest_detent() {
    let mut r = setup(&[]);
    r.touch(0., 0.);
    r.mv(10., 5.);
    r.mv(20., 10.);
    r.up(20., 12.);
    assert_eq!(r.dismissed, 1);
}

#[test]
fn fr19_keeps_a_short_release_under_the_flick_speed() {
    let mut r = setup(&[]);
    r.touch(0., 0.);
    r.mv(8., 10.);
    r.mv(20., 25.);
    r.up(20., 27.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 0.);
}

#[test]
fn fr19_expands_to_the_next_detent_up_on_a_fast_short_flick() {
    let mut r = setup(&[200.]);
    r.rest_at_200();
    r.touch(400., 2000.);
    r.mv(390., 2005.);
    r.mv(380., 2010.);
    r.up(380., 2012.);
    assert_eq!(r.last_snap(), Some(400.));
}

#[test]
fn fr19_hands_the_panel_the_distance_left_and_the_release_speed_after_a_drag_closes_it() {
    let mut r = setup(&[]);
    r.touch(0., 0.);
    r.mv(10., 5.);
    r.mv(20., 10.);
    r.up(20., 12.);
    assert_eq!(r.dismissed, 1);
    // The owner takes the close.
    r.g.answer(false);
    let exit = r.g.drag_exit().expect("a drag's close");
    assert_eq!(exit.distance, 380.);
    assert!((exit.velocity - 2000.).abs() < 1., "{}", exit.velocity);
    assert!(r.g.is_dismissed_by_touch_drag());
}

#[test]
fn fr19_keeps_the_stylesheet_exit_for_a_close_that_no_drag_started() {
    let mut r = setup(&[]);
    r.g.answer(false);
    assert_eq!(r.g.drag_exit(), None);
}

#[test]
fn fr19_springs_a_quick_short_mouse_drag_back_at_the_lowest_detent() {
    let mut r = setup(&[]);
    r.mouse(0., 0.);
    r.mv(20., 10.);
    r.mv(40., 20.);
    r.up(40., 22.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 0.);
}

#[test]
fn fr19_gives_a_mouse_drag_that_closes_the_sheet_the_stylesheet_exit() {
    let mut r = setup(&[]);
    r.mouse(0., 0.);
    r.mv(100., 50.);
    r.mv(200., 100.);
    r.up(200., 102.);
    assert_eq!(r.dismissed, 1);
    r.g.answer(false);
    assert_eq!(r.g.drag_exit(), None);
    assert!(!r.g.is_dismissed_by_touch_drag());
}

#[test]
fn fr19_never_stops_a_faster_touch_release_short_of_a_slower_one() {
    let land = |ms: f64| {
        let mut r = setup(&[200., 60.]);
        r.touch(0., 0.);
        r.mv(30., ms / 2.);
        r.mv(60., ms);
        r.up(60., ms + 2.);
        r.settled()
    };
    assert_eq!(land(60.), 200.);
    assert_eq!(land(55.), 200.);
}

#[test]
fn fr19_closes_on_a_slow_touch_drag_only_past_halfway_from_the_lowest_detent_to_closed() {
    let drag = |to: f64| {
        let mut r = setup(&[200.]);
        r.rest_at_200();
        r.touch(200., 2000.);
        let step = (to - 200.) / 10.;
        for i in 1..=10 {
            let i = f64::from(i);
            r.mv(200. + step * i, 2000. + step / 0.225 * i);
        }
        r.up(to, 2000. + step / 0.225 * 10. + 2.);
        r.dismissed
    };
    assert_eq!(drag(290.), 0);
    assert_eq!(drag(310.), 1);
}

#[test]
fn fr19_cancels_a_touch_release_thrown_back_toward_where_the_drag_began() {
    // AST-094 FR37.
    let mut r = setup(&[]);
    r.touch(0., 0.);
    for i in 1..=9 {
        r.mv(f64::from(i) * 40., f64::from(i) * 200.);
    }
    r.mv(345., 1815.);
    r.mv(330., 1830.);
    r.up(330., 1832.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 0.);
}

#[test]
fn fr19_springs_a_gentle_touch_throw_back_where_uikits_sheet_does() {
    let mut r = setup(&[200.]);
    r.touch(0., 0.);
    r.mv(30., 100.);
    r.mv(60., 200.);
    r.up(60., 202.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 0.);
}

#[test]
fn fr19_settles_to_the_nearest_detent_on_a_slow_drag() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(90., 300.);
    r.mv(180., 700.);
    r.up(180., 1100.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 200.);
}

// --- Detents, the peek, the layout split and the scrim -------------------------------------

#[test]
fn fr18_excludes_the_reserve_below_the_window_from_visible_detent_heights() {
    let mut r = setup(&[200.]);
    r.g.reserve = 48.;
    // 352 px visible: the 200 px detent rests at offset 152.
    assert_eq!(r.g.detents(SHEET_HEIGHT).offsets, [0., 152.]);
    r.mouse(0., 0.);
    r.mv(150., 700.);
    r.up(150., 1400.);
    assert_eq!(r.settled(), 152.);
    assert_eq!(r.last_snap(), Some(200.));
}

#[test]
fn a_peek_detent_keeps_the_full_layout_height() {
    let mut r = setup(&[80.]);
    // 80 of 400 px is a fifth: a peek.
    assert_eq!(r.g.detents(SHEET_HEIGHT).peek, Some(320.));
    r.mouse(0., 0.);
    r.mv(160., 700.);
    r.mv(320., 1400.);
    r.up(320., 2100.);
    assert_eq!(r.settled(), 320.);
    assert_eq!(
        r.g.settled_layout_offset, 0.,
        "it slides, it does not reflow"
    );
}

#[test]
fn a_working_height_detent_resizes_the_scrolling_area() {
    let mut r = setup(&[200.]);
    r.rest_at_200();
    assert_eq!(r.g.settled_layout_offset, 200.);
    // The settle keeps the drag's split until it lands.
    assert!(r.g.is_settling());
    r.g.complete_settle();
    assert_eq!(r.g.layout_offset(), 200.);
}

#[test]
fn the_dismiss_overshoot_reads_the_visible_shortest_detent() {
    let r = setup(&[200.]);
    // The line is 40% of the 200 px shortest stop past it: 280 px.
    let at = |offset: f64| r.g.scrim_at(offset, SHEET_HEIGHT);
    assert_eq!(at(200.), 1.);
    assert!((at(240.) - 0.5).abs() < 1e-9);
    assert_eq!(at(280.), 0.);
}

#[test]
fn a_downward_drag_from_a_middle_detent_never_snaps_back_up_past_it() {
    let mut r = setup(&[300., 100.]);
    // Detents at 0, 100 and 300 px offsets.
    r.mouse(0., 0.);
    r.mv(50., 500.);
    r.mv(100., 1000.);
    r.up(100., 1500.);
    assert_eq!(r.settled(), 100.);
    r.mouse(100., 2000.);
    r.mv(130., 2500.);
    r.up(130., 3000.);
    assert_eq!(r.settled(), 100.);
}

#[test]
fn fr19_dismisses_when_dragged_well_below_the_shortest_detent_or_rebounds_when_it_cannot() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(150., 1000.);
    r.mv(300., 2000.);
    r.up(300., 3000.);
    assert_eq!(r.dismissed, 1);
    let mut r = setup(&[200.]);
    r.g.can_dismiss = false;
    r.mouse(0., 0.);
    r.mv(150., 1000.);
    r.mv(300., 2000.);
    r.up(300., 3000.);
    assert_eq!(r.dismissed, 0);
    assert_eq!(r.settled(), 200.);
}

#[test]
fn the_scrim_fades_from_full_toward_the_peek_floor_and_holds_there_at_the_peek() {
    let mut r = setup(&[200., 80.]);
    // Detents 0, 200, 320 (the peek).
    r.mouse(0., 0.);
    r.mv(200., 1000.);
    r.mv(260., 1500.);
    let fading = *r.scrims.last().unwrap();
    assert!(fading < 1. && fading > 0.3, "{fading}");
    r.mv(320., 2000.);
    r.up(320., 2500.);
    assert_eq!(r.settled(), 320.);
    assert!((r.scrims.last().unwrap() - 0.3).abs() < 1e-9, "still modal");
}

#[test]
fn the_live_drag_clicks_into_a_nearby_detent() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    // 190 px down is 10 px off the 200 px detent: the magnet pulls it most of the way.
    let at = r.mv(190., 500.);
    assert!(at > 190. && at < 200., "{at}");
    // Far from any detent the drag follows one for one.
    let at = r.mv(100., 1000.);
    assert_eq!(at, 100.);
}

#[test]
fn a_drag_above_its_base_restores_the_full_layout_height() {
    let mut r = setup(&[200.]);
    r.rest_at_200();
    r.g.complete_settle();
    r.touch(200., 2000.);
    r.mv(150., 2100.);
    assert_eq!(r.g.layout_offset(), 0.);
    r.mv(260., 2200.);
    assert_eq!(r.g.layout_offset(), 200.);
}

#[test]
fn an_interrupted_drag_returns_to_its_detent_and_its_scrim() {
    let mut r = setup(&[200., 60.]);
    r.mouse(0., 0.);
    r.mv(330., 1000.);
    let scrim = r.g.cancel().expect("was dragging");
    assert_eq!(scrim, 1.);
    assert_eq!(r.g.active_offset, 0.);
    assert!(!r.g.is_dragging());
}

// --- The stretch past the top (FR18, DEC-3) -------------------------------------------------

#[test]
fn fr18_a_finger_stretches_on_the_rubber_band_the_same_on_every_pull_with_no_cap() {
    let pull = |r: &mut Rig, by: f64, t: f64| {
        r.touch(400., t);
        r.mv(400. - by / 2., t + 200.);
        let at = r.mv(400. - by, t + 400.);
        r.up(400. - by, t + 900.);
        -at
    };
    let mut r = setup(&[]);
    r.g.reserve = 160.;
    let first = pull(&mut r, 120., 0.);
    let second = pull(&mut r, 120., 2000.);
    let third = pull(&mut r, 120., 4000.);
    assert!(first > 48., "past the mouse's cap: {first}");
    assert!(first < 120., "resisted: {first}");
    assert_eq!(first, second);
    assert_eq!(second, third);
    // The reserve covers the whole stretch: it never passes it.
    let far = pull(&mut r, 2000., 6000.);
    assert!(far <= 160., "{far}");
}

#[test]
fn fr18_a_mouse_or_pen_keeps_its_own_resisted_stretch_capped_at_48_px() {
    for pointer in [PointerKind::Mouse, PointerKind::Pen] {
        let mut r = setup(&[]);
        r.g.reserve = 160.;
        r.down(pointer, 400., 0.);
        let small = r.mv(380., 200.);
        assert!((small + 20. * 0.35).abs() < 1e-9, "{small}");
        let big = r.mv(100., 400.);
        assert_eq!(big, -48.);
        r.up(100., 900.);
    }
}

#[test]
fn fr19_dismisses_a_single_height_sheet_on_a_slow_drag_past_the_floor() {
    let mut r = setup(&[]);
    r.mouse(0., 0.);
    r.mv(100., 1000.);
    r.mv(200., 2000.);
    r.up(200., 3000.);
    assert_eq!(r.dismissed, 1);
}

// --- A dismissal the owner refuses -----------------------------------------------------------

#[test]
fn fr21_a_refused_dismissal_restores_the_full_scrim_of_a_sheet_resting_fully_open() {
    let mut r = setup(&[]);
    r.mouse(0., 0.);
    r.mv(150., 1000.);
    r.mv(300., 2000.);
    assert!(*r.scrims.last().unwrap() < 1.);
    r.up(300., 3000.);
    assert_eq!(r.dismissed, 1);
    // The owner keeps it open: the sheet springs back, its scrim with it.
    assert_eq!(r.g.answer(true), Some(1.));
    assert_eq!(r.g.active_offset, 0.);
    assert_eq!(r.g.drag_exit(), None);
}

#[test]
fn fr21_a_refused_dismissal_restores_the_thinned_scrim_of_a_sheet_resting_at_its_peek() {
    let mut r = setup(&[200., 80.]);
    r.mouse(0., 0.);
    r.mv(160., 1000.);
    r.mv(320., 2000.);
    r.up(320., 3000.);
    assert_eq!(r.settled(), 320.);
    r.mouse(320., 4000.);
    r.mv(380., 5000.);
    r.mv(400., 6000.);
    r.up(400., 7000.);
    assert_eq!(r.dismissed, 1);
    let scrim = r.g.answer(true).unwrap();
    assert!((scrim - 0.3).abs() < 1e-9);
}

#[test]
fn fr21_a_dismissal_the_owner_takes_leaves_the_thinned_scrim_alone() {
    let mut r = setup(&[]);
    r.touch(0., 0.);
    r.mv(10., 5.);
    r.mv(20., 10.);
    r.up(20., 12.);
    assert_eq!(r.g.answer(false), None);
    // Answered once: a later frame changes nothing.
    assert_eq!(r.g.answer(false), None);
}

// --- Re-anchoring (FR19, a resize) -----------------------------------------------------------

#[test]
fn a_resize_re_resolves_the_settled_detent_against_the_new_window() {
    let mut r = setup(&[200.]);
    r.rest_at_200();
    r.g.complete_settle();
    // The window grows: the same detent, a fraction of it, is taller now.
    r.g.snap_heights = vec![300.];
    let (offset, _) = r.g.reanchor(600., 0.).expect("moved");
    assert_eq!(offset, 300.);
    assert_eq!(r.g.settled_index, 1);
}

#[test]
fn a_resize_keeps_the_peek_detent_on_screen_when_the_window_shrinks() {
    let mut r = setup(&[80.]);
    r.mouse(0., 0.);
    r.mv(160., 700.);
    r.mv(320., 1400.);
    r.up(320., 2100.);
    r.g.complete_settle();
    r.g.snap_heights = vec![40.];
    let (offset, scrim) = r.g.reanchor(200., 0.).expect("moved");
    assert_eq!(offset, 160.);
    assert!((scrim - 0.3).abs() < 1e-9);
}

#[test]
fn a_resize_leaves_a_drag_in_progress_alone() {
    let mut r = setup(&[200.]);
    r.mouse(0., 0.);
    r.mv(50., 100.);
    assert_eq!(r.g.reanchor(600., 0.), None);
}
