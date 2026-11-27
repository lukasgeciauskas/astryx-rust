//! Upstream `createSnapDrag.test.ts`: the options only Core passes. A model draws a
//! drag one for one or on a spring, or leaves the element as it is inside a dead
//! zone; a release or a cancel lands where the model says; new snap points re-aim a
//! settle in flight and the cancel of a drag; an empty list stops everything; an x
//! drag can draw into a custom property, with a companion; compositor parts play a
//! settle on one clock and report from its curve. The pan is fed by hand.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{IntoElement, TestAppContext};
use rustryx_primitives::motion::{MotionSpring, SpringDescription, StartMode};

use super::harness::*;
use crate::drag_to_snap_points::compositor_settle::{CompositorPart, OnPass};
use crate::drag_to_snap_points::snap_drag::{
    Companion, CustomProperty, SettleOptions, SnapDragLanding, SnapDragModel, SnapDragTarget,
    SnapElement,
};
use crate::gestures::pan::{PanAxis, PanInput, PanState};

const PROPERTY: &str = "--test-travel";

fn property() -> Option<CustomProperty> {
    Some(CustomProperty {
        name: PROPERTY.into(),
        companion: None,
    })
}

fn travel(rig: &Rig) -> Option<f64> {
    rig.element.property(PROPERTY)
}

fn x_rig(points: &[f64]) -> Rig {
    rig(PanAxis::X, points, |o| o.custom_property = property())
}

fn pass(offset: f64, direction: f64, calls: &Rc<Cell<u32>>) -> Option<OnPass> {
    let calls = calls.clone();
    Some(OnPass {
        offset,
        direction,
        call: Rc::new(move || calls.set(calls.get() + 1)),
    })
}

fn close(a: f64, b: f64, within: f64) -> bool {
    (a - b).abs() <= within
}

use PanInput::Touch;
use PanState::{Began, Cancelled, Changed, Ended};

#[gpui::test]
fn hands_the_pan_its_own_options(cx: &mut TestAppContext) {
    use rustryx_primitives::gestures::host::gesture_host;
    use rustryx_primitives::gestures::{PanOptions, PointerKind};

    use crate::drag_to_snap_points::{AttachedSnapDrag, attach_snap_drag, executor_clock};

    let made: Rc<RefCell<Option<Rc<AttachedSnapDrag>>>> = Rc::default();
    let m = made.clone();
    let cx = host(cx, move |window, cx| {
        let attached = m
            .borrow_mut()
            .get_or_insert_with(|| {
                let mut pan = PanOptions::default();
                pan.base.allowed_pointer_types = Some(vec![PointerKind::Touch, PointerKind::Pen]);
                Rc::new(attach_snap_drag(
                    &SnapElement::new(),
                    &gesture_host(window, cx),
                    crate::drag_to_snap_points::snap_drag::SnapDragOptions {
                        axis: PanAxis::Y,
                        snap_points: vec![0., 100.],
                        flick_threshold: None,
                        on_settle: Rc::new(|_| {}),
                        pan: Some(pan),
                        custom_property: None,
                        compositor_parts: None,
                        model: None,
                        clock: executor_clock(cx),
                    },
                    Rc::new(RefCell::new(Rc::new(|_, _, _| {}))),
                ))
            })
            .clone();
        attached
            .render(target("surface"), window, cx)
            .into_any_element()
    });
    let snap = made.borrow().as_ref().unwrap().snap.clone();
    // A mouse is not among the pointers the pan takes: it moves nothing.
    mouse_drag_y(cx, 60., 4, 16);
    assert_eq!(snap.position(), 0.);
    // A finger is.
    touch(cx, gpui::TouchPhase::Started, 10., 10.);
    for step in 1..=4 {
        wait(cx, 16);
        touch(cx, gpui::TouchPhase::Moved, 10., 10. + 15. * step as f32);
    }
    assert!(snap.position() > 0.);
}

#[test]
fn draws_the_drag_into_a_custom_property_one_for_one_instead_of_translate() {
    let r = x_rig(&[-100., 0.]);
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -40., 0., Touch);
    assert_eq!(travel(&r), Some(-40.));
    assert_eq!(r.element.translate(), None);
}

#[test]
fn warns_that_a_y_drag_draws_nothing_into_a_custom_property() {
    take_warnings();
    let _r = rig(PanAxis::Y, &[0., 100.], |o| o.custom_property = property());
    assert_eq!(
        take_warnings(),
        ["createSnapDrag: customProperty draws the x axis only, so this y drag draws nothing."]
    );
}

#[test]
fn leaves_the_element_as_it_is_while_the_model_says_so_then_follows_its_targets() {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let r = rig(PanAxis::X, &[-100., 0.], |o| {
        o.custom_property = property();
        o.model = Some(SnapDragModel {
            move_to: Some(Rc::new(move |drag| {
                c.set(c.get() + 1);
                (drag.translation.abs() >= 20.).then_some(SnapDragTarget {
                    offset: drag.translation / 2.,
                    spring: None,
                })
            })),
            ..Default::default()
        });
    });
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -10., 0., Touch);
    assert_eq!(travel(&r), None);
    r.pan(Changed, -60., 0., Touch);
    assert_eq!(travel(&r), Some(-30.));
    assert_eq!(calls.get(), 3);
}

#[test]
fn starts_a_drags_spring_target_on_the_drags_clock_never_waiting_for_a_frame() {
    let spring = MotionSpring::inline(SpringDescription::Physics {
        mass: 1.,
        stiffness: 300.,
        damping: 35.,
    });
    let r = rig(PanAxis::X, &[-100., 0.], |o| {
        o.custom_property = property();
        o.model = Some(SnapDragModel {
            move_to: Some(Rc::new(move |drag| {
                Some(SnapDragTarget {
                    offset: drag.translation,
                    spring: Some(spring),
                })
            })),
            ..Default::default()
        });
    });
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -30., 0., Touch);
    let last = r.snap.calls().last().cloned().unwrap();
    assert_eq!(last.target, -30.);
    assert_eq!(last.spring, spring);
    assert_eq!(last.options.at, Some(r.now()));
}

#[test]
fn lands_a_release_where_the_model_says_with_the_drags_input() {
    let seen = Rc::new(RefCell::new(None));
    let s = seen.clone();
    let r = rig(PanAxis::X, &[-100., 0.], |o| {
        o.custom_property = property();
        o.model = Some(SnapDragModel {
            release: Some(Rc::new(move |release| {
                *s.borrow_mut() = Some(*release);
                Some(SnapDragLanding {
                    index: 0,
                    ..Default::default()
                })
            })),
            ..Default::default()
        });
    });
    r.pan(Began, 0., 0., PanInput::Scroll);
    r.pan(Changed, -30., 0., PanInput::Scroll);
    r.pan(Ended, -30., -400., PanInput::Scroll);
    let release = seen.borrow().unwrap();
    assert_eq!(release.drag.input, PanInput::Scroll);
    assert_eq!(release.drag.translation, -30.);
    assert_eq!(release.velocity, -400.);
    r.advance(2000.);
    assert_eq!(*r.settled.borrow(), [0]);
    assert_eq!(travel(&r), Some(-100.));
}

#[test]
fn lets_the_model_land_a_cancelled_drag_or_nothing() {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let r = rig(PanAxis::X, &[-100., 0.], |o| {
        o.custom_property = property();
        o.model = Some(SnapDragModel {
            cancel: Some(Rc::new(move |_| {
                c.set(c.get() + 1);
                None
            })),
            ..Default::default()
        });
    });
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -30., 0., Touch);
    r.pan(Cancelled, -30., 0., Touch);
    assert_eq!(calls.get(), 1);
    r.advance(2000.);
    assert!(r.settled.borrow().is_empty());
    assert_eq!(travel(&r), Some(-30.));
}

#[test]
fn lands_a_cancelled_drag_on_the_new_snap_points_after_they_change() {
    let r = x_rig(&[-200., -100., 0.]);
    r.snap.set_snap_points(&[-80., 0.]);
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -30., 0., Touch);
    r.pan(Cancelled, -30., 0., Touch);
    r.advance(3000.);
    assert_eq!(r.settled.borrow().last(), Some(&1));
    assert_eq!(r.x(), 0.);
}

#[test]
fn lands_a_cancelled_drag_on_the_new_snap_points_when_they_change_mid_drag() {
    let r = x_rig(&[-200., -100., 0.]);
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -30., 0., Touch);
    r.snap.set_snap_points(&[-80., 0.]);
    r.pan(Cancelled, -30., 0., Touch);
    r.advance(3000.);
    assert_eq!(r.settled.borrow().last(), Some(&1));
    assert_eq!(r.x(), 0.);
}

#[test]
fn re_aims_a_settle_in_flight_at_the_new_snap_point_nearest_where_it_was_heading() {
    let r = x_rig(&[-200., -100., 0.]);
    r.snap.settle_to(1, SettleOptions::default());
    r.advance(100.);
    assert!(r.x() < 0. && r.x() > -100., "partway: {}", r.x());
    // -100 is gone: the nearest new point to it is -120.
    r.snap.set_snap_points(&[-120., 0.]);
    r.advance(3000.);
    assert_eq!(*r.settled.borrow(), [0]);
    assert!(close(r.x(), -120., 0.1));
}

#[test]
fn calls_a_re_aimed_settles_on_pass_once_when_it_had_already_called_back() {
    let calls = Rc::new(Cell::new(0));
    let r = x_rig(&[-200., -100., 0.]);
    r.snap.settle_to(
        0,
        SettleOptions {
            on_pass: pass(-20., -1., &calls),
            ..Default::default()
        },
    );
    r.advance(150.);
    assert_eq!(calls.get(), 1);
    assert!(r.x() > -150.);
    // -200 is gone: the settle re-aims at -150, still past the offset.
    r.snap.set_snap_points(&[-150., 0.]);
    r.advance(3000.);
    assert_eq!(calls.get(), 1);
    assert_eq!(r.settled.borrow().last(), Some(&0));
    assert!(close(r.x(), -150., 0.1));
}

fn dead_zone_rig(points: &[f64]) -> Rig {
    rig(PanAxis::X, points, |o| {
        o.custom_property = property();
        o.model = Some(SnapDragModel {
            move_to: Some(Rc::new(|drag| {
                (drag.translation.abs() >= 20.).then_some(SnapDragTarget {
                    offset: drag.start + drag.translation,
                    spring: None,
                })
            })),
            ..Default::default()
        });
    })
}

#[test]
fn re_aims_a_settle_that_a_drag_inside_its_dead_zone_has_not_caught() {
    let r = dead_zone_rig(&[-200., -100., 0.]);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(50.);
    // A finger held 5 px into the 20 px dead zone: the settle runs on.
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -5., 0., Touch);
    r.snap.set_snap_points(&[-80., 0.]);
    r.advance(3000.);
    assert_eq!(r.settled.borrow().last(), Some(&0));
    assert!(close(r.x(), -80., 0.1));
}

#[test]
fn lands_a_cancelled_drag_that_caught_a_settle_on_its_target_after_the_snap_points_change() {
    let r = x_rig(&[-200., -100., 0.]);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(3000.);
    r.snap.settle_to(2, SettleOptions::default());
    r.advance(30.);
    // The drag catches the settle short of -100, on its way to 0.
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -1., 0., Touch);
    assert!(r.x() < -100.);
    r.snap.set_snap_points(&[-200., -100., 0.]);
    r.pan(Cancelled, -1., 0., Touch);
    r.advance(3000.);
    assert_eq!(r.settled.borrow().last(), Some(&2));
    assert!(close(r.x(), 0., 0.1));
}

#[test]
fn stops_a_settle_where_it_is_when_the_snap_points_empty_and_reports_nothing() {
    let r = x_rig(&[-200., 0.]);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(50.);
    let at = r.x();
    assert!(at < 0. && at > -200.);
    r.snap.set_snap_points(&[]);
    r.advance(3000.);
    assert!(r.settled.borrow().is_empty());
    assert!(close(r.x(), at, 0.1));
}

#[test]
fn ignores_new_snap_points_after_destroy() {
    let r = x_rig(&[-200., 0.]);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(50.);
    let at = r.x();
    r.snap.destroy();
    r.snap.set_snap_points(&[-80., 0.]);
    r.advance(3000.);
    assert!(r.settled.borrow().is_empty());
    assert!(close(r.x(), at, 0.1));
}

#[test]
fn ends_a_drag_in_progress_when_the_snap_points_empty() {
    let r = x_rig(&[-100., 0.]);
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -30., 0., Touch);
    r.snap.set_snap_points(&[]);
    r.snap.set_snap_points(&[-100., 0.]);
    // The same touch moves on and lifts: the ended drag draws and lands nothing.
    r.pan(Changed, -60., 0., Touch);
    r.pan(Ended, -60., -500., Touch);
    r.advance(3000.);
    assert!(r.settled.borrow().is_empty());
    assert!(close(r.x(), -30., 0.1));
}

#[test]
fn lands_a_cancel_on_the_re_aimed_point_after_a_dead_zone_drag() {
    let r = dead_zone_rig(&[-200., -100., 0.]);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(50.);
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -5., 0., Touch);
    r.snap.set_snap_points(&[-80., 0.]);
    r.pan(Cancelled, -5., 0., Touch);
    r.advance(3000.);
    assert_eq!(r.settled.borrow().last(), Some(&0));
    assert!(close(r.x(), -80., 0.1));
}

#[test]
fn does_nothing_for_settle_to_after_the_snap_points_empty_with_a_dev_warning() {
    let r = x_rig(&[-100., 0.]);
    r.snap.set_snap_points(&[]);
    take_warnings();
    assert!(r.snap.settle_to(0, SettleOptions::default()).is_none());
    assert_eq!(take_warnings().len(), 1);
    r.advance(100.);
    assert!(r.settled.borrow().is_empty());
}

#[test]
fn starts_a_settle_asked_for_in_an_input_events_own_task_at_the_next_frame() {
    let r = x_rig(&[-100., 0.]);
    r.snap.settle_to(
        0,
        SettleOptions {
            next_frame: true,
            ..Default::default()
        },
    );
    let last = r.snap.calls().last().cloned().unwrap();
    assert_eq!(last.target, -100.);
    assert_eq!(last.options.start, Some(StartMode::NextFrame));
    r.snap.settle_to(1, SettleOptions::default());
    assert_eq!(r.snap.calls().last().unwrap().options.start, None);
}

#[test]
fn settles_on_a_snap_point_with_the_spring_and_velocity_it_is_given_and_returns_its_motion() {
    let r = x_rig(&[0., -100.]);
    assert_eq!(r.snap.snap_points(), [-100., 0.]);
    let handle = r.snap.settle_to(
        0,
        SettleOptions {
            spring: Some(MotionSpring::inline(SpringDescription::Physics {
                mass: 1.,
                stiffness: 600.,
                damping: 49.,
            })),
            velocity: Some(-300.),
            ..Default::default()
        },
    );
    assert!(handle.is_some());
    assert_eq!(r.snap.calls().last().unwrap().velocity, Some(-300.));
    r.advance(2000.);
    assert_eq!(*r.settled.borrow(), [0]);
    r.snap.set_snap_points(&[0., -50.]);
    assert_eq!(r.snap.snap_points(), [-50., 0.]);
}

// --- A settle that passes an offset calls back -----------------------------------

#[test]
fn on_pass_calls_back_once_on_the_first_frame_whose_presented_value_has_passed_the_offset() {
    let calls = Rc::new(Cell::new(0));
    let r = x_rig(&[-200., 0.]);
    let handle = r
        .snap
        .settle_to(
            0,
            SettleOptions {
                on_pass: pass(-150., -1., &calls),
                ..Default::default()
            },
        )
        .unwrap();
    // Not before the curve gets there.
    r.advance(16.);
    let now = r.now();
    let passed_at = (0..=3000)
        .find(|t| {
            handle.value(
                rustryx_primitives::motion::MotionProperty::X,
                Some(now + *t as f64),
            ) <= -150.
        })
        .unwrap();
    assert!(passed_at > 16);
    assert_eq!(calls.get(), 0);
    r.advance(3000.);
    assert_eq!(calls.get(), 1);
}

#[test]
fn on_pass_calls_back_on_the_next_frame_when_the_element_is_past_the_offset_already() {
    let r = x_rig(&[-200., 0.]);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(3000.);
    let calls = Rc::new(Cell::new(0));
    r.snap.settle_to(
        0,
        SettleOptions {
            on_pass: pass(-150., -1., &calls),
            ..Default::default()
        },
    );
    assert_eq!(calls.get(), 0);
    r.advance(20.);
    assert_eq!(calls.get(), 1);
}

#[test]
fn on_pass_calls_back_once_for_a_settle_that_starts_past_the_offset_and_lands_short() {
    let r = x_rig(&[-200., -100., 0.]);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(3000.);
    let calls = Rc::new(Cell::new(0));
    // From -200, past -150, back to -100: the first frame is past it.
    r.snap.settle_to(
        1,
        SettleOptions {
            on_pass: pass(-150., -1., &calls),
            ..Default::default()
        },
    );
    r.advance(3000.);
    assert_eq!(calls.get(), 1);
    assert_eq!(travel(&r), Some(-100.));
}

#[test]
fn on_pass_never_calls_back_for_a_settle_that_lands_short_is_replaced_or_is_destroyed() {
    let r = x_rig(&[-200., -100., 0.]);
    let short = Rc::new(Cell::new(0));
    r.snap.settle_to(
        1,
        SettleOptions {
            on_pass: pass(-150., -1., &short),
            ..Default::default()
        },
    );
    r.advance(3000.);
    assert_eq!(short.get(), 0);
    let replaced = Rc::new(Cell::new(0));
    r.snap.settle_to(
        0,
        SettleOptions {
            on_pass: pass(-190., -1., &replaced),
            ..Default::default()
        },
    );
    r.advance(16.);
    r.snap.settle_to(2, SettleOptions::default());
    r.advance(3000.);
    assert_eq!(replaced.get(), 0);
    let destroyed = Rc::new(Cell::new(0));
    r.snap.settle_to(
        0,
        SettleOptions {
            on_pass: pass(-50., -1., &destroyed),
            ..Default::default()
        },
    );
    r.snap.destroy();
    r.advance(3000.);
    assert_eq!(destroyed.get(), 0);
}

#[test]
fn on_pass_never_calls_back_for_a_settle_a_drag_catches() {
    let r = x_rig(&[-200., 0.]);
    let calls = Rc::new(Cell::new(0));
    r.snap.settle_to(
        0,
        SettleOptions {
            on_pass: pass(-190., -1., &calls),
            ..Default::default()
        },
    );
    r.advance(16.);
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, 10., 0., Touch);
    r.advance(3000.);
    assert_eq!(calls.get(), 0);
}

// --- A companion drawn from the travel ------------------------------------------

const COMPANION: &str = "--test-companion";

fn companion_rig() -> Rig {
    rig(PanAxis::X, &[-200., 0.], |o| {
        o.custom_property = Some(CustomProperty {
            name: PROPERTY.into(),
            companion: Some(Companion {
                name: COMPANION.into(),
                map: Rc::new(|x| (-x / 100.).clamp(0., 1.)),
            }),
        });
    })
}

#[test]
fn writes_the_companion_with_the_travel_one_for_one() {
    let r = companion_rig();
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -40., 0., Touch);
    assert_eq!(travel(&r), Some(-40.));
    assert_eq!(r.element.property(COMPANION), Some(0.4));
    r.pan(Changed, -150., 0., Touch);
    assert_eq!(r.element.property(COMPANION), Some(1.));
}

#[test]
fn settles_the_companion_with_the_travel_in_the_same_frames() {
    let r = companion_rig();
    r.snap.settle_to(0, SettleOptions::default());
    for _ in 0..30 {
        r.advance(16.);
        let x = travel(&r).unwrap_or(0.);
        let companion = r.element.property(COMPANION).unwrap_or(0.);
        assert!(close(companion, (-x / 100.).clamp(0., 1.), 1e-4));
    }
}

// --- On the compositor ------------------------------------------------------------

/// A root and a child part: the root follows the value, the child mirrors it.
fn parts_rig(root: &SnapElement, child: &SnapElement) -> Rig {
    let (root, child) = (root.clone(), child.clone());
    rig(PanAxis::X, &[-100., 0.], move |o| {
        o.custom_property = property();
        o.compositor_parts = Some(Rc::new(move || {
            vec![
                CompositorPart {
                    element: root.clone(),
                    offset: Rc::new(|v| v),
                },
                CompositorPart {
                    element: child.clone(),
                    offset: Rc::new(|v| -v),
                },
            ]
        }));
    })
}

#[test]
fn plays_a_settle_as_one_translate_per_part_from_its_offset_back_to_none_while_the_drawn_value_holds_the_end()
 {
    let (root, child) = (SnapElement::new(), SnapElement::new());
    let r = parts_rig(&root, &child);
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -40., 0., Touch);
    assert_eq!(travel(&r), Some(-40.));
    assert!(r.snap.settle_to(1, SettleOptions::default()).is_none());
    // The end at once: 0, which draws as no value at all.
    assert_eq!(travel(&r), None);
    assert_eq!(root.composited(), Some((-40., 0.)));
    assert_eq!(child.composited(), Some((40., 0.)));
    // One clock: the parts mirror each other on every frame.
    for _ in 0..10 {
        r.advance(16.);
        let (a, b) = (root.composited(), child.composited());
        assert_eq!(a.map(|p| -p.0), b.map(|p| p.0));
    }
    r.advance(3000.);
    assert_eq!(root.composited(), None, "back to none");
    assert_eq!(child.composited(), None);
}

#[test]
fn plays_a_role_spring_at_the_themes_tempo_as_the_animator_does() {
    let duration_at = |tempo: f64| {
        let (root, child) = (SnapElement::new(), SnapElement::new());
        let r = parts_rig(&root, &child);
        r.snap.animator.set_tempo(tempo);
        r.pan(Began, 0., 0., Touch);
        r.pan(Changed, -40., 0., Touch);
        r.snap.settle_to(1, SettleOptions::default());
        r.snap.compositor_duration().unwrap()
    };
    let at_one = duration_at(1.);
    // Twice the tempo: the same spring takes about twice as long.
    assert!(duration_at(2.) > at_one * 1.6);
}

#[test]
fn finish_settle_ends_a_compositor_settle_at_its_end_now() {
    let (root, child) = (SnapElement::new(), SnapElement::new());
    let r = parts_rig(&root, &child);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(48.);
    assert!(r.settled.borrow().is_empty());
    r.snap.finish_settle();
    assert_eq!(root.composited(), None);
    assert_eq!(child.composited(), None);
    assert_eq!(*r.settled.borrow(), [0]);
    assert_eq!(r.snap.position(), -100.);
    r.advance(3000.);
    assert_eq!(*r.settled.borrow(), [0]);
    // With nothing in flight, it does nothing.
    r.snap.finish_settle();
    assert_eq!(*r.settled.borrow(), [0]);
}

#[test]
fn finish_settle_leaves_a_settle_the_animator_draws_alone() {
    let r = x_rig(&[-100., 0.]);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(48.);
    let midway = r.snap.position();
    r.snap.finish_settle();
    assert!(r.settled.borrow().is_empty());
    assert_eq!(r.snap.position(), midway);
    r.advance(3000.);
    assert_eq!(*r.settled.borrow(), [0]);
}

#[test]
fn a_compositor_settle_reports_once_its_curve_ends_and_calls_on_pass_once() {
    let (root, child) = (SnapElement::new(), SnapElement::new());
    let r = parts_rig(&root, &child);
    let calls = Rc::new(Cell::new(0));
    r.snap.settle_to(
        0,
        SettleOptions {
            on_pass: pass(-50., -1., &calls),
            ..Default::default()
        },
    );
    assert_eq!(calls.get(), 0);
    r.advance(48.);
    assert!(r.settled.borrow().is_empty());
    let mut passed_at = r.snap.position();
    for _ in 0..120 {
        if calls.get() > 0 {
            break;
        }
        passed_at = r.snap.position();
        r.advance(16.);
    }
    assert_eq!(calls.get(), 1);
    assert!(passed_at > -50.);
    assert!(r.settled.borrow().is_empty());
    r.advance(3000.);
    assert_eq!(calls.get(), 1);
    assert_eq!(*r.settled.borrow(), [0]);
}

#[test]
fn a_compositor_settle_asked_for_in_an_input_events_task_starts_at_the_next_frame() {
    let (root, child) = (SnapElement::new(), SnapElement::new());
    let r = parts_rig(&root, &child);
    r.snap.settle_to(
        0,
        SettleOptions {
            next_frame: true,
            ..Default::default()
        },
    );
    assert_eq!(root.composited(), None);
    assert_ne!(travel(&r), Some(-100.));
    r.advance(16.);
    assert!(root.composited().is_some());
    assert_eq!(travel(&r), Some(-100.));
}

#[test]
fn a_drag_catches_a_compositor_settle_where_it_is_on_screen() {
    let (root, child) = (SnapElement::new(), SnapElement::new());
    let r = parts_rig(&root, &child);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(96.);
    let on_screen = r.snap.position();
    assert!(on_screen < 0. && on_screen > -100.);
    // The drawn value holds the end; the parts draw the rest.
    assert_eq!(travel(&r), Some(-100.));
    r.pan(Began, 0., 0., Touch);
    r.pan(Changed, -10., 0., Touch);
    assert_eq!(root.composited(), None);
    assert_eq!(child.composited(), None);
    assert!(close(travel(&r).unwrap(), on_screen - 10., 1e-3));
    r.advance(3000.);
    assert!(r.settled.borrow().is_empty());
}

#[test]
fn reduce_motion_keeps_the_animators_settle() {
    let (root, child) = (SnapElement::new(), SnapElement::new());
    let r = parts_rig(&root, &child);
    r.snap.animator.set_reduced_motion(true);
    assert!(r.snap.settle_to(0, SettleOptions::default()).is_some());
    assert_eq!(root.composited(), None);
}

#[test]
fn a_destroy_holds_the_element_where_the_compositor_settle_is() {
    let (root, child) = (SnapElement::new(), SnapElement::new());
    let r = parts_rig(&root, &child);
    r.snap.settle_to(0, SettleOptions::default());
    r.advance(96.);
    let on_screen = r.snap.position();
    r.snap.destroy();
    assert!(close(travel(&r).unwrap(), on_screen, 1e-3));
}

// --- A y drag on the element's translate ---------------------------------------------

#[test]
fn a_y_drag_draws_the_elements_translate() {
    let r = rig(PanAxis::Y, &[0., 240.], |_| {});
    r.pan_y(Began, 0., 0., Touch);
    r.pan_y(Changed, 30., 0., Touch);
    assert_eq!(r.element.translate(), Some((0., 30.)));
    let _ = r.element.draw(gpui::div()).into_any_element();
}
