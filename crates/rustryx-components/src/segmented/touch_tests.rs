//! SegmentedControl's press (FR3, FR11-FR14): upstream `useSegmentedTouchTracking.test.ts`
//! (which segment a pointer counts for, and where a lift commits), the face's hysteresis,
//! and the press in a window: a tap, a drag from the selected segment and from another, a
//! lift off the control, a cancel, Escape, and the lift of the selected surface.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    IntoElement, SharedString, TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext,
    point, px,
};

use super::tests::{host, press};
use super::touch::*;
use super::*;

fn spans(layout: &[(f32, f32)], disabled: &[usize]) -> Vec<SegmentSpan> {
    layout
        .iter()
        .enumerate()
        .map(|(i, (left, right))| SegmentSpan {
            left: *left,
            right: *right,
            disabled: disabled.contains(&i),
            ..SegmentSpan::default()
        })
        .collect()
}

// Grid, List, Table: gaps of 4 px, a narrow segment beside a wide one.
const LAYOUT: [(f32, f32); 3] = [(12., 108.), (112., 152.), (156., 314.)];

#[test]
fn counts_a_finger_over_a_segment_for_that_segment_however_near_a_narrower_neighbours_center() {
    let s = spans(&LAYOUT, &[]);
    assert_eq!(segment_target_at(160., &s), Some(2));
    assert_eq!(segment_target_at(60., &s), Some(0));
}

#[test]
fn splits_a_gap_at_its_middle() {
    let s = spans(&LAYOUT, &[]);
    assert_eq!(segment_target_at(109., &s), Some(0));
    assert_eq!(segment_target_at(111., &s), Some(1));
    assert_eq!(segment_target_at(153., &s), Some(1));
    assert_eq!(segment_target_at(155., &s), Some(2));
}

#[test]
fn counts_a_finger_past_either_end_for_the_end_segment_and_a_disabled_one_for_nothing() {
    let s = spans(&LAYOUT, &[]);
    assert_eq!(segment_target_at(-40., &s), Some(0));
    assert_eq!(segment_target_at(400., &s), Some(2));
    let s = spans(&LAYOUT, &[1]);
    assert_eq!(segment_target_at(130., &s), None);
    assert_eq!(segment_target_at(111., &s), None);
    assert_eq!(segment_target_at(109., &s), Some(0));
}

const BOUNDS: Area = Area {
    left: 10.,
    top: 20.,
    right: 316.,
    bottom: 51.,
};
const LIFT_LAYOUT: [(f32, f32); 3] = [(12., 108.), (110., 208.), (210., 314.)];

fn lift_at(x: f32, y: f32, s: &[SegmentSpan]) -> Option<usize> {
    lift_target_at(x, y, &[BOUNDS], s, None)
}

#[test]
fn commits_the_segment_a_lift_inside_the_control_counts_for_the_padding_included() {
    let s = spans(&LIFT_LAYOUT, &[]);
    assert_eq!(lift_at(250., 35., &s), Some(2));
    assert_eq!(lift_at(109., 35., &s), Some(0));
    assert_eq!(lift_at(11., 21., &s), Some(0));
    assert_eq!(lift_at(315., 50., &s), Some(2));
}

#[test]
fn counts_nothing_outside_the_control_and_nothing_over_a_disabled_segment() {
    let s = spans(&LIFT_LAYOUT, &[]);
    assert_eq!(lift_at(5., 35., &s), None);
    assert_eq!(lift_at(330., 35., &s), None);
    assert_eq!(lift_at(250., 10., &s), None);
    assert_eq!(lift_at(250., 90., &s), None);
    assert_eq!(lift_at(150., 35., &spans(&LIFT_LAYOUT, &[1])), None);
}

#[test]
fn commits_on_a_segments_own_touch_band_never_on_the_box_around_every_band() {
    let mut s = spans(&LIFT_LAYOUT, &[]);
    s[0].band = Some(Area {
        left: 12.,
        top: 14.,
        right: 108.,
        bottom: 49.,
    });
    s[2].band = Some(Area {
        left: 210.,
        top: 10.,
        right: 314.,
        bottom: 49.,
    });
    let area = [BOUNDS, s[0].band.unwrap(), s[2].band.unwrap()];
    assert_eq!(lift_target_at(60., 15., &area, &s, None), Some(0));
    assert_eq!(lift_target_at(250., 11., &area, &s, None), Some(2));
    assert_eq!(lift_target_at(60., 11., &area, &s, None), None);
}

#[test]
fn commits_the_second_row_segment_a_tap_lands_on_when_a_caller_wraps_the_segments() {
    // Rows of three 96 px segments, 36 px tall, 4 px apart: E sits under B.
    let boxed = |left: f32, top: f32| SegmentSpan {
        left,
        right: left + 96.,
        top: Some(top),
        bottom: Some(top + 36.),
        ..SegmentSpan::default()
    };
    let s = [
        boxed(12., 22.),
        boxed(112., 22.),
        boxed(212., 22.),
        boxed(12., 62.),
        boxed(112., 62.),
    ];
    let track = Area {
        left: 10.,
        top: 20.,
        right: 310.,
        bottom: 102.,
    };
    // The tracker counts the pointer on the center line, the finger where it is.
    let line = (track.top + track.bottom) / 2.;
    assert_eq!(
        lift_target_at(160., line, &[track], &s, Some((160., 80.))),
        Some(4)
    );
    assert_eq!(
        lift_target_at(60., line, &[track], &s, Some((60., 80.))),
        Some(3)
    );
    assert_eq!(
        lift_target_at(160., line, &[track], &s, Some((160., 40.))),
        Some(1)
    );
}

#[test]
fn fr12_the_face_moves_only_past_the_boundary_by_its_hysteresis() {
    // Three 100 px segments, 4 px apart: the boundaries sit at 102 and 206.
    let s = spans(&[(0., 100.), (104., 204.), (208., 308.)], &[]);
    // A quarter of a 100 px segment: 25 px past the boundary.
    assert_eq!(follow_face(0, 110., &s), 0);
    assert_eq!(follow_face(0, 126., &s), 0, "short of 102 + 25");
    assert_eq!(follow_face(0, 127., &s), 1);
    assert_eq!(
        follow_face(1, 80., &s),
        1,
        "back over the boundary, inside the hysteresis"
    );
    assert_eq!(follow_face(1, 77., &s), 0);
    // A center far along moves past several at once.
    assert_eq!(follow_face(0, 290., &s), 2);
    // A disabled segment never takes the face.
    let s = spans(&[(0., 100.), (104., 204.), (208., 308.)], &[1]);
    assert_eq!(follow_face(0, 150., &s), 0);
    // The hysteresis is at least 8 px.
    let narrow = spans(&[(0., 20.), (24., 44.)], &[]);
    assert_eq!(follow_face(0, 28., &narrow), 0);
    assert_eq!(follow_face(0, 30., &narrow), 1);
}

// --- In a window -----------------------------------------------------------

type Log = Rc<RefCell<Vec<SharedString>>>;

fn three(log: Log, value: &'static str) -> gpui::AnyElement {
    SegmentedControl::new("s", "View", value)
        .on_change(move |value, _, _| log.borrow_mut().push(value))
        .item(SegmentedControlItem::new("a", "Alpha"))
        .item(SegmentedControlItem::new("b", "Beta"))
        .item(SegmentedControlItem::new("c", "Gamma"))
        .into_any_element()
}

fn mount<'a>(cx: &'a mut TestAppContext, value: &'static str) -> (&'a mut VisualTestContext, Log) {
    let log: Log = Rc::default();
    let l = log.clone();
    let cx = host(cx, move || three(l.clone(), value));
    (cx, log)
}

fn center(cx: &mut VisualTestContext, selector: &'static str) -> (f32, f32) {
    let b = cx.debug_bounds(selector).expect(selector);
    (
        f32::from(b.origin.x + b.size.width / 2.),
        f32::from(b.origin.y + b.size.height / 2.),
    )
}

fn finger(cx: &mut VisualTestContext, id: u64, phase: TouchPhase, (x, y): (f32, f32)) {
    cx.simulate_event(TouchEvent {
        id: TouchId(id),
        phase,
        position: point(px(x), px(y)),
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
    // Draws the frame the time reached, as the screen would.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

/// A finger from `from` to `to` in eight 16 ms steps, held there.
fn drag(cx: &mut VisualTestContext, id: u64, from: (f32, f32), to: (f32, f32)) {
    finger(cx, id, TouchPhase::Started, from);
    for step in 1..=8 {
        wait(cx, 16);
        let s = step as f32 / 8.;
        let at = (from.0 + (to.0 - from.0) * s, from.1 + (to.1 - from.1) * s);
        finger(cx, id, TouchPhase::Moved, at);
    }
}

/// A tap far from the control, so the window reads its primary pointer as coarse.
fn touch_first(cx: &mut VisualTestContext) {
    finger(cx, 99, TouchPhase::Started, (700., 500.));
    finger(cx, 99, TouchPhase::Ended, (700., 500.));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

#[gpui::test]
fn fr3_a_tap_selects_through_the_segments_click(cx: &mut TestAppContext) {
    let (cx, log) = mount(cx, "a");
    let b = center(cx, "s-b");
    finger(cx, 1, TouchPhase::Started, b);
    finger(cx, 1, TouchPhase::Ended, b);
    wait(cx, 50);
    assert_eq!(*log.borrow(), [SharedString::from("b")]);
}

#[gpui::test]
fn fr13_a_drag_from_the_selected_segment_carries_the_lens_and_commits_the_nearest_at_the_lift(
    cx: &mut TestAppContext,
) {
    let (cx, log) = mount(cx, "a");
    let (a, c) = (center(cx, "s-a"), center(cx, "s-c"));
    drag(cx, 1, a, c);
    wait(cx, 400);
    // The lens followed the finger: it shows over Gamma.
    let lens = center(cx, "s-lens");
    assert!(
        (lens.0 - c.0).abs() < 16.,
        "the lens at {lens:?}, the finger at {c:?}"
    );
    assert!(log.borrow().is_empty(), "nothing commits before the lift");
    finger(cx, 1, TouchPhase::Ended, c);
    wait(cx, 50);
    assert_eq!(*log.borrow(), [SharedString::from("c")]);
}

#[gpui::test]
fn fr13_a_drag_from_another_segment_commits_where_it_lifts(cx: &mut TestAppContext) {
    let (cx, log) = mount(cx, "a");
    let (b, c) = (center(cx, "s-b"), center(cx, "s-c"));
    drag(cx, 1, b, c);
    finger(cx, 1, TouchPhase::Ended, c);
    wait(cx, 50);
    assert_eq!(*log.borrow(), [SharedString::from("c")]);
}

#[gpui::test]
fn fr3_a_lift_off_the_control_commits_the_segment_under_its_horizontal_position(
    cx: &mut TestAppContext,
) {
    let (cx, log) = mount(cx, "a");
    let (a, b) = (center(cx, "s-a"), center(cx, "s-b"));
    drag(cx, 1, a, b);
    // Off the control, below it, still over Beta's horizontal span.
    for step in 1..=4 {
        wait(cx, 16);
        finger(cx, 1, TouchPhase::Moved, (b.0, b.1 + 30. * step as f32));
    }
    finger(cx, 1, TouchPhase::Ended, (b.0, b.1 + 120.));
    wait(cx, 50);
    assert_eq!(*log.borrow(), [SharedString::from("b")]);
}

#[gpui::test]
fn fr3_a_cancel_commits_nothing_and_the_lens_goes_back(cx: &mut TestAppContext) {
    let (cx, log) = mount(cx, "a");
    let (a, c) = (center(cx, "s-a"), center(cx, "s-c"));
    drag(cx, 1, a, c);
    finger(cx, 1, TouchPhase::Cancelled, c);
    wait(cx, 1000);
    assert!(log.borrow().is_empty());
    // At rest, the selected segment paints its own surface again.
    assert!(cx.debug_bounds("s-lens").is_none());
}

#[gpui::test]
fn fr3_escape_cancels_the_press_and_the_lift_commits_nothing(cx: &mut TestAppContext) {
    let (cx, log) = mount(cx, "a");
    press(cx, "tab");
    let (a, c) = (center(cx, "s-a"), center(cx, "s-c"));
    drag(cx, 1, a, c);
    press(cx, "escape");
    finger(cx, 1, TouchPhase::Ended, c);
    wait(cx, 50);
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn fr14_a_press_on_the_selected_segment_lifts_the_lens_under_the_touch_presentation(
    cx: &mut TestAppContext,
) {
    let (cx, _) = mount(cx, "a");
    touch_first(cx);
    let segment = cx.debug_bounds("s-a").unwrap();
    let a = center(cx, "s-a");
    finger(cx, 1, TouchPhase::Started, a);
    wait(cx, 500);
    let lens = cx.debug_bounds("s-lens").expect("a lifted lens");
    let grew = f32::from(lens.size.width - segment.size.width);
    assert!((grew - 24.).abs() < 1., "the lens grew by {grew}");
    assert!(f32::from(lens.size.height - segment.size.height) > 15.);
    // It settles back at the release.
    finger(cx, 1, TouchPhase::Ended, a);
    wait(cx, 1500);
    assert!(cx.debug_bounds("s-lens").is_none());
}

#[gpui::test]
fn fr14_nothing_lifts_under_a_fine_pointer_or_reduce_motion(cx: &mut TestAppContext) {
    let (cx, _) = mount(cx, "a");
    let segment = cx.debug_bounds("s-a").unwrap();
    let a = center(cx, "s-a");
    cx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position: point(px(a.0), px(a.1)),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    wait(cx, 500);
    if let Some(lens) = cx.debug_bounds("s-lens") {
        assert_eq!(lens.size, segment.size, "a mouse press lifts nothing");
    }
    cx.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Left,
        position: point(px(a.0), px(a.1)),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    wait(cx, 1000);
    // Reduce Motion under a finger.
    cx.update(|_, cx| rustryx_primitives::set_reduced_motion(true, cx));
    touch_first(cx);
    finger(cx, 1, TouchPhase::Started, a);
    wait(cx, 500);
    if let Some(lens) = cx.debug_bounds("s-lens") {
        assert_eq!(lens.size, segment.size, "Reduce Motion lifts nothing");
    }
    finger(cx, 1, TouchPhase::Ended, a);
}

#[gpui::test]
fn fr13_a_mouse_drags_the_same_way(cx: &mut TestAppContext) {
    let (cx, log) = mount(cx, "a");
    let (a, c) = (center(cx, "s-a"), center(cx, "s-c"));
    let mouse = |cx: &mut VisualTestContext, at: (f32, f32), phase: u8| {
        let position = point(px(at.0), px(at.1));
        match phase {
            0 => cx.simulate_event(gpui::MouseDownEvent {
                button: gpui::MouseButton::Left,
                position,
                modifiers: gpui::Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }),
            1 => cx.simulate_event(gpui::MouseMoveEvent {
                position,
                pressed_button: Some(gpui::MouseButton::Left),
                modifiers: gpui::Modifiers::default(),
            }),
            _ => cx.simulate_event(gpui::MouseUpEvent {
                button: gpui::MouseButton::Left,
                position,
                modifiers: gpui::Modifiers::default(),
                click_count: 1,
            }),
        }
        cx.run_until_parked();
    };
    mouse(cx, a, 0);
    for step in 1..=8 {
        wait(cx, 16);
        let s = step as f32 / 8.;
        mouse(cx, (a.0 + (c.0 - a.0) * s, a.1), 1);
    }
    mouse(cx, c, 2);
    wait(cx, 50);
    assert_eq!(*log.borrow(), [SharedString::from("c")]);
}

#[gpui::test]
fn fr11_a_held_unselected_segment_paints_the_pressed_overlay_under_a_finger(
    cx: &mut TestAppContext,
) {
    let (cx, _) = mount(cx, "a");
    let segment = cx.debug_bounds("s-b").unwrap();
    let b = center(cx, "s-b");
    finger(cx, 1, TouchPhase::Started, b);
    wait(cx, 200);
    let painted =
        cx.update(|window, cx| crate::gestures::press_feedback::painted_surfaces(window, cx));
    assert!(
        painted
            .iter()
            .any(|(bounds, paint)| *bounds == segment && *paint == crate::gestures::PressPaint::On),
        "the held segment paints its press: {painted:?}"
    );
    // The selected segment lifts instead: it paints no overlay.
    finger(cx, 1, TouchPhase::Cancelled, b);
    let selected = cx.debug_bounds("s-a").unwrap();
    let a = center(cx, "s-a");
    finger(cx, 2, TouchPhase::Started, a);
    wait(cx, 200);
    let painted =
        cx.update(|window, cx| crate::gestures::press_feedback::painted_surfaces(window, cx));
    assert!(painted.iter().all(|(bounds, _)| *bounds != selected));
    finger(cx, 2, TouchPhase::Ended, a);
}

/// The lens's centres over the half second after a drag from Alpha to Gamma lifts, its
/// moves `step` ms apart.
fn settle_after_drag(cx: &mut TestAppContext, step: u64) -> (Vec<f32>, f32) {
    // A parent that adopts the new value.
    let log: Log = Rc::default();
    let value = Rc::new(RefCell::new(SharedString::from("a")));
    let (l, v) = (log.clone(), value.clone());
    let cx = host(cx, move || {
        let (l, store) = (l.clone(), v.clone());
        SegmentedControl::new("s", "View", v.borrow().clone())
            .on_change(move |value, window, _| {
                l.borrow_mut().push(value.clone());
                *store.borrow_mut() = value;
                window.refresh();
            })
            .item(SegmentedControlItem::new("a", "Alpha"))
            .item(SegmentedControlItem::new("b", "Beta"))
            .item(SegmentedControlItem::new("c", "Gamma"))
            .into_any_element()
    });
    touch_first(cx);
    let (a, c) = (center(cx, "s-a"), center(cx, "s-c"));
    finger(cx, 1, TouchPhase::Started, a);
    for i in 1..=8 {
        wait(cx, step);
        let s = i as f32 / 8.;
        finger(cx, 1, TouchPhase::Moved, (a.0 + (c.0 - a.0) * s, a.1));
    }
    finger(cx, 1, TouchPhase::Ended, c);
    assert_eq!(*log.borrow(), [SharedString::from("c")]);
    let mut centres = Vec::new();
    for _ in 0..30 {
        wait(cx, 16);
        if let Some(lens) = cx.debug_bounds("s-lens") {
            centres.push(f32::from(lens.origin.x + lens.size.width / 2.));
        }
    }
    (centres, c.0)
}

#[gpui::test]
fn fr13_a_flicks_release_settles_from_its_velocity(cx: &mut TestAppContext) {
    let (centres, target) = settle_after_drag(cx, 4);
    let past = centres.iter().copied().fold(f32::NEG_INFINITY, f32::max) - target;
    assert!(
        past > 1.,
        "a flick carries the lens past its segment and back: {past}"
    );
}

#[gpui::test]
fn fr13_a_slow_release_settles_without_overshoot(cx: &mut TestAppContext) {
    let (centres, target) = settle_after_drag(cx, 120);
    let past = centres.iter().copied().fold(f32::NEG_INFINITY, f32::max) - target;
    assert!(past <= 1., "a resting release settles in place: {past}");
}
