//! Group press tracker tests, after upstream `hooks/actionGroupTracker.test.ts`,
//! `hooks/useActionGroupTracking.test.tsx` and the ButtonGroup and ToggleButtonGroup
//! tracking tests.

use std::cell::RefCell;
use std::time::Duration;

use gpui::{
    AnyElement, Context, InteractiveElement as _, Pixels, Point, Render, TestAppContext,
    VisualTestContext, div, point, px,
};

use super::*;
use crate::button::{Button, ButtonGroup};

fn key(s: &str) -> SharedString {
    SharedString::from(s.to_string())
}

fn target(k: &str, left: f32, top: f32, right: f32, bottom: f32) -> ActionTarget {
    ActionTarget {
        key: key(k),
        rect: HitRect::new(left, top, right, bottom),
    }
}

/// Cancel and Delete side by side, 100 x 40 each.
fn actions() -> Vec<ActionTarget> {
    vec![
        target("cancel", 0., 0., 100., 40.),
        target("delete", 100., 0., 200., 40.),
    ]
}

fn plain() -> TrackerOptions {
    TrackerOptions::default()
}

#[test]
fn begin_highlights_the_action_under_the_point() {
    let mut t = ActionGroupTracker::default();
    let change = t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    assert_eq!(
        change,
        Some(HighlightChange {
            next: Some(key("cancel")),
            previous: None
        })
    );
    assert!(t.is_tracking());
    assert_eq!(t.began_on(), Some(&key("cancel")));
}

#[test]
fn move_follows_the_finger_the_action_it_leaves_is_cut_not_faded() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    let change = t.move_to(150., 20., &plain());
    assert_eq!(
        change,
        Some(HighlightChange {
            next: Some(key("delete")),
            previous: Some(key("cancel"))
        })
    );
    let now = Instant::now();
    assert_eq!(t.paint_of(&key("cancel"), now), None, "cut, not faded");
    assert_eq!(t.paint_of(&key("delete"), now), Some(TrackerPaint::On));
}

#[test]
fn end_fades_the_action_under_the_finger_and_runs_it_once() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 150.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    let now = Instant::now();
    assert_eq!(t.end(now, 200., &plain()), Some(key("delete")));
    assert!(matches!(
        t.paint_of(&key("delete"), now),
        Some(TrackerPaint::Fading { .. })
    ));
    assert_eq!(t.end(now, 200., &plain()), None, "it runs once");
}

#[test]
fn a_lift_with_no_action_under_the_finger_runs_nothing() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    t.move_to(50., 80., &plain());
    assert_eq!(t.end(Instant::now(), 200., &plain()), None);
}

#[test]
fn cancel_cuts_the_highlight_reports_it_gone_and_runs_nothing() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    assert_eq!(
        t.cancel(&plain()),
        Some(HighlightChange {
            next: None,
            previous: Some(key("cancel"))
        })
    );
    assert_eq!(t.paint_of(&key("cancel"), Instant::now()), None);
    assert_eq!(t.end(Instant::now(), 200., &plain()), None);
}

#[test]
fn an_inert_press_is_taken_but_paints_and_runs_nothing() {
    let mut t = ActionGroupTracker::default();
    assert_eq!(
        t.begin(
            PressBegin {
                x: 50.,
                y: 20.,
                inert: true,
                landed: None
            },
            actions(),
            None,
            &plain()
        ),
        None
    );
    assert!(t.is_tracking());
    assert_eq!(t.end(Instant::now(), 200., &plain()), None);
}

#[test]
fn where_several_actions_hold_the_point_the_one_the_press_landed_on_wins() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 100.,
            y: 20.,
            inert: false,
            landed: Some(key("delete")),
        },
        actions(),
        None,
        &plain(),
    );
    assert_eq!(t.highlighted(), Some(&key("delete")));
}

#[test]
fn offset_by_shifts_the_cached_rects_when_the_content_moves_under_a_still_finger() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    t.offset_by(100., 0.);
    t.move_to(150., 20., &plain());
    assert_eq!(t.highlighted(), Some(&key("cancel")));
}

#[test]
fn reports_highlight_changes_and_can_leave_the_paint_to_its_owner() {
    let options = TrackerOptions {
        paints_highlight: Some(Rc::new(|_: &SharedString| false)),
        ..TrackerOptions::default()
    };
    let mut t = ActionGroupTracker::default();
    assert!(
        t.begin(
            PressBegin {
                x: 50.,
                y: 20.,
                inert: false,
                landed: None
            },
            actions(),
            None,
            &options
        )
        .is_some()
    );
    assert_eq!(t.paint_of(&key("cancel"), Instant::now()), None);
}

#[test]
fn a_press_that_begins_while_another_is_tracked_takes_the_paint_off_the_first_one() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    t.begin(
        PressBegin {
            x: 150.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    assert_eq!(t.paint_of(&key("cancel"), Instant::now()), None);
    assert_eq!(
        t.paint_of(&key("delete"), Instant::now()),
        Some(TrackerPaint::On)
    );
}

#[test]
fn times_the_release_fade_by_the_given_exit_duration_ast094_fr5() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    let now = Instant::now();
    t.end(now, 200., &plain());
    assert!(
        t.paint_of(&key("cancel"), now + Duration::from_millis(199))
            .is_some()
    );
    assert_eq!(
        t.paint_of(&key("cancel"), now + Duration::from_millis(200)),
        None
    );
}

#[test]
fn dispose_finishes_a_pending_release_fade() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    t.end(Instant::now(), 200., &plain());
    t.dispose();
    assert_eq!(t.paint_of(&key("cancel"), Instant::now()), None);
}

#[test]
fn expands_each_actions_hit_area_by_the_given_margin() {
    let options = TrackerOptions {
        highlight_expansion: 10.,
        ..TrackerOptions::default()
    };
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 48.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &options,
    );
    assert_eq!(t.highlighted(), Some(&key("cancel")));
    t.move_to(50., 51., &options);
    assert_eq!(t.highlighted(), None);
}

#[test]
fn resolves_the_target_with_the_owners_rule_from_the_rects_read_at_the_begin() {
    let options = TrackerOptions {
        resolve_target: Some(Rc::new(|_, _, targets: &[ActionTarget]| {
            targets.last().map(|t| t.key.clone())
        })),
        ..TrackerOptions::default()
    };
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 5.,
            y: 5.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &options,
    );
    assert_eq!(t.highlighted(), Some(&key("delete")));
}

#[test]
fn remeasure_reads_the_targets_again_and_resolves_the_fingers_last_position() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &plain(),
    );
    let moved = vec![target("delete", 0., 0., 100., 40.)];
    t.remeasure(moved, None, &plain());
    assert_eq!(t.highlighted(), Some(&key("delete")));
}

#[test]
fn counts_the_finger_on_the_row_a_drift_off_it_keeps_choosing_along_x() {
    let options = TrackerOptions {
        tracks_along: Some(TrackAxis::X),
        ..TrackerOptions::default()
    };
    let band = HitRect::new(0., 0., 200., 40.);
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        Some(band),
        &options,
    );
    t.move_to(150., 300., &options);
    assert_eq!(
        t.highlighted(),
        Some(&key("delete")),
        "a drift off keeps choosing"
    );
    // Past the row's ends, the end action counts.
    t.move_to(900., 20., &options);
    assert_eq!(t.highlighted(), Some(&key("delete")));
    assert_eq!(t.end(Instant::now(), 0., &options), Some(key("delete")));
}

#[test]
fn without_an_axis_a_drift_off_the_group_leaves_no_action() {
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        Some(HitRect::new(0., 0., 200., 40.)),
        &plain(),
    );
    t.move_to(150., 300., &plain());
    assert_eq!(t.highlighted(), None);
}

#[test]
fn escape_cancels_the_press_in_a_one_axis_group_and_leaves_a_group_without_one_alone() {
    let options = TrackerOptions {
        tracks_along: Some(TrackAxis::X),
        ..TrackerOptions::default()
    };
    let band = HitRect::new(0., 0., 200., 40.);
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        Some(band),
        &options,
    );
    assert!(t.escape(&options), "handled");
    assert_eq!(t.paint_of(&key("cancel"), Instant::now()), None);
    assert_eq!(t.end(Instant::now(), 0., &options), None);
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        Some(band),
        &plain(),
    );
    assert!(!t.escape(&plain()));
}

#[test]
fn a_paint_function_decides_action_by_action_which_take_the_press_paint() {
    let options = TrackerOptions {
        paints_highlight: Some(Rc::new(|k: &SharedString| k.as_ref() == "delete")),
        ..TrackerOptions::default()
    };
    let mut t = ActionGroupTracker::default();
    t.begin(
        PressBegin {
            x: 50.,
            y: 20.,
            inert: false,
            landed: None,
        },
        actions(),
        None,
        &options,
    );
    assert_eq!(t.paint_of(&key("cancel"), Instant::now()), None);
    t.move_to(150., 20., &options);
    assert_eq!(
        t.paint_of(&key("delete"), Instant::now()),
        Some(TrackerPaint::On)
    );
}

#[test]
fn reads_a_row_on_its_center_line_clamped_to_its_ends_and_a_column_across_x() {
    let band = HitRect::new(0., 0., 200., 40.);
    assert_eq!(on_center_line(&band, TrackAxis::X, -5., 300.), (0., 20.));
    assert_eq!(on_center_line(&band, TrackAxis::Y, 500., -9.), (100., 0.));
}

#[test]
fn resolve_nearest_action_lets_a_disabled_neighbour_count_for_nearness_and_resolve_to_none() {
    let targets = vec![target("a", 0., 0., 24., 24.)];
    let disabled = [HitRect::new(30., 0., 54., 24.)];
    assert_eq!(
        resolve_nearest_action(26., 12., &targets, &disabled),
        Some(key("a"))
    );
    assert_eq!(resolve_nearest_action(29., 12., &targets, &disabled), None);
}

// --- The hook, through a ButtonGroup ------------------------------------------

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(40.)).child((self.0)())
    }
}

type Log = Rc<RefCell<Vec<&'static str>>>;

fn host(
    cx: &mut TestAppContext,
    coarse: bool,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::modality::set_primary_pointer_coarse(Some(coarse), cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn finger(cx: &mut VisualTestContext, id: u64, phase: gpui::TouchPhase, p: Point<Pixels>) {
    cx.simulate_event(gpui::TouchEvent {
        id: gpui::TouchId(id),
        phase,
        position: p,
        predicted_position: None,
        force: None,
    });
    settle(cx);
}

fn group(log: &Log, disabled_delete: bool) -> impl Fn() -> AnyElement + 'static {
    let log = log.clone();
    move || {
        let (a, b) = (log.clone(), log.clone());
        ButtonGroup::new("g", "Actions")
            .child(
                div().debug_selector(|| "g-cancel".into()).child(
                    Button::new("g-cancel-button", "Cancel")
                        .on_click(move |_, _, _| a.borrow_mut().push("cancel")),
                ),
            )
            .child(
                div().debug_selector(|| "g-delete".into()).child(
                    Button::new("g-delete-button", "Delete")
                        .disabled(disabled_delete)
                        .on_click(move |_, _, _| b.borrow_mut().push("delete")),
                ),
            )
            .into_any_element()
    }
}

fn center(cx: &mut VisualTestContext, s: &'static str) -> Point<Pixels> {
    cx.debug_bounds(s).expect("drawn").center()
}

#[gpui::test]
fn moves_the_highlight_with_the_finger_and_runs_the_action_it_lifts_on(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, true, group(&log, false));
    let (from, to) = (center(cx, "g-delete"), center(cx, "g-cancel"));
    finger(cx, 1, gpui::TouchPhase::Started, from);
    finger(
        cx,
        1,
        gpui::TouchPhase::Moved,
        point(from.x - px(20.), from.y),
    );
    finger(cx, 1, gpui::TouchPhase::Moved, to);
    finger(cx, 1, gpui::TouchPhase::Ended, to);
    assert_eq!(
        *log.borrow(),
        ["cancel"],
        "the lift runs the action under it, once"
    );
}

#[gpui::test]
fn paints_the_touched_action_in_the_touch_down_event_and_runs_it_once_at_the_lift(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let cx = host(cx, true, group(&log, false));
    let at = center(cx, "g-cancel");
    finger(cx, 1, gpui::TouchPhase::Started, at);
    assert!(log.borrow().is_empty());
    finger(cx, 1, gpui::TouchPhase::Ended, at);
    cx.executor().advance_clock(Duration::from_millis(500));
    settle(cx);
    assert_eq!(*log.borrow(), ["cancel"]);
}

#[gpui::test]
fn ends_the_gesture_with_nothing_acting_when_a_second_finger_lands_family_action_groups_fr3(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let cx = host(cx, true, group(&log, false));
    let (a, b) = (center(cx, "g-cancel"), center(cx, "g-delete"));
    finger(cx, 1, gpui::TouchPhase::Started, a);
    finger(cx, 2, gpui::TouchPhase::Started, b);
    finger(cx, 1, gpui::TouchPhase::Ended, a);
    finger(cx, 2, gpui::TouchPhase::Ended, b);
    cx.executor().advance_clock(Duration::from_millis(500));
    settle(cx);
    assert!(log.borrow().is_empty(), "nothing acts: {:?}", log.borrow());
}

#[gpui::test]
fn skips_a_disabled_action_a_lift_on_it_runs_nothing(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, true, group(&log, true));
    let (from, to) = (center(cx, "g-cancel"), center(cx, "g-delete"));
    finger(cx, 1, gpui::TouchPhase::Started, from);
    finger(cx, 1, gpui::TouchPhase::Moved, to);
    finger(cx, 1, gpui::TouchPhase::Ended, to);
    cx.executor().advance_clock(Duration::from_millis(500));
    settle(cx);
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn tracks_nothing_under_a_fine_primary_pointer_a_tap_keeps_its_own_click(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, false, group(&log, false));
    let (from, to) = (center(cx, "g-delete"), center(cx, "g-cancel"));
    finger(cx, 1, gpui::TouchPhase::Started, from);
    finger(cx, 1, gpui::TouchPhase::Moved, to);
    finger(cx, 1, gpui::TouchPhase::Ended, to);
    cx.executor().advance_clock(Duration::from_millis(500));
    settle(cx);
    assert!(
        !log.borrow().contains(&"cancel"),
        "no slide across the members without the touch presentation"
    );
}

#[gpui::test]
fn a_finger_just_past_the_groups_edge_lands_on_its_hit_layer_and_runs_the_nearest_member(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let cx = host(cx, true, group(&log, false));
    let b = center(cx, "g-cancel");
    let bottom = cx.debug_bounds("g-cancel").unwrap().bottom();
    let at = point(b.x, bottom + px(3.));
    finger(cx, 1, gpui::TouchPhase::Started, at);
    finger(cx, 1, gpui::TouchPhase::Ended, at);
    cx.executor().advance_clock(Duration::from_millis(500));
    settle(cx);
    assert_eq!(*log.borrow(), ["cancel"]);
}
