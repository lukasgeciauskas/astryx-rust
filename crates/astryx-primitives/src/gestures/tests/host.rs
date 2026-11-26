//! The GPUI binding (upstream `GestureHost.ts`, `__tests__/GestureHost.test.ts`
//! in spirit): a window's host feeds GPUI's mouse, touch, wheel, pinch and
//! key input to the recognizers of the areas it paints.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Context, FocusHandle, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PinchEvent, Render, ScrollDelta, ScrollEvent, ScrollSource, TestAppContext, TouchEvent,
    TouchId, VisualTestContext, Window, div, point, prelude::*, px,
};

use crate::gestures::host::{GestureAttachment, gesture_area, gesture_host};
use crate::gestures::*;

type Log = Rc<RefCell<Vec<String>>>;

struct Surface {
    attachment: GestureAttachment,
    clicks: Rc<Cell<u32>>,
    scrolls: Rc<RefCell<Vec<(ScrollSource, f32)>>>,
    focus: FocusHandle,
}

impl Render for Surface {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        let scrolls = self.scrolls.clone();
        div()
            .size_full()
            .on_scroll(move |event: &ScrollEvent, _, _| {
                let delta = event.delta.pixel_delta(px(16.));
                scrolls
                    .borrow_mut()
                    .push((event.source, f32::from(delta.y)));
            })
            .child(gesture_area(
                &self.attachment,
                div()
                    .id("target")
                    .track_focus(&self.focus)
                    .absolute()
                    .left(px(20.))
                    .top(px(20.))
                    .w(px(200.))
                    .h(px(100.))
                    .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
            ))
    }
}

struct Fixture<'a> {
    cx: &'a mut VisualTestContext,
    clicks: Rc<Cell<u32>>,
    scrolls: Rc<RefCell<Vec<(ScrollSource, f32)>>>,
    attachment: GestureAttachment,
    focus: FocusHandle,
}

fn mount<'a>(
    cx: &'a mut TestAppContext,
    build: impl FnOnce(&GestureAttachment, &crate::gestures::host::GestureHost) + 'static,
) -> Fixture<'a> {
    let clicks = Rc::new(Cell::new(0));
    let scrolls = Rc::new(RefCell::new(Vec::new()));
    let (c, s) = (clicks.clone(), scrolls.clone());
    let (view, cx) = cx.add_window_view(move |window, cx| {
        let host = gesture_host(window, cx);
        let attachment = host.attach(&[]);
        build(&attachment, &host);
        Surface {
            attachment,
            clicks: c,
            scrolls: s,
            focus: cx.focus_handle(),
        }
    });
    cx.run_until_parked();
    let (attachment, focus) = view.read_with(cx, |surface, _| {
        (surface.attachment.clone(), surface.focus.clone())
    });
    Fixture {
        cx,
        clicks,
        scrolls,
        attachment,
        focus,
    }
}

fn log_of(recognizer: &AnyRecognizer) -> Log {
    let log: Log = Rc::default();
    let record = log.clone();
    recognizer
        .on_transition(move |r, _from, to| record.borrow_mut().push(format!("{}:{to}", r.name())));
    log
}

fn touch(cx: &mut VisualTestContext, id: u64, phase: gpui::TouchPhase, x: f32, y: f32) {
    cx.simulate_event(TouchEvent {
        id: TouchId(id),
        phase,
        position: point(px(x), px(y)),
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

fn mouse_down(cx: &mut VisualTestContext, button: MouseButton, x: f32, y: f32) {
    cx.simulate_event(MouseDownEvent {
        button,
        position: point(px(x), px(y)),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
}

fn mouse_move(cx: &mut VisualTestContext, x: f32, y: f32, pressed: bool) {
    cx.simulate_event(MouseMoveEvent {
        position: point(px(x), px(y)),
        pressed_button: pressed.then_some(MouseButton::Left),
        modifiers: Modifiers::default(),
    });
    cx.run_until_parked();
}

fn mouse_up(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: point(px(x), px(y)),
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_mouse_drag_drives_a_pan_and_its_click_is_the_arenas(cx: &mut TestAppContext) {
    let pan = PanGestureRecognizer::new(PanOptions::named("pan"));
    let log = log_of(&pan);
    let attach = pan.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    mouse_down(f.cx, MouseButton::Left, 50., 50.);
    mouse_move(f.cx, 60., 50., true);
    mouse_move(f.cx, 90., 50., true);
    assert_eq!(pan.state(), GestureState::Changed);
    assert_eq!(pan.translation(), Point::new(30., 0.));
    mouse_up(f.cx, 90., 50.);
    assert_eq!(
        *log.borrow(),
        ["pan:began", "pan:changed", "pan:ended", "pan:possible"]
    );
    // The drag's click was swallowed: the element under it heard nothing.
    assert_eq!(f.clicks.get(), 0);
}

#[gpui::test]
fn a_mouse_click_nobody_takes_still_clicks(cx: &mut TestAppContext) {
    let pan = PanGestureRecognizer::new(PanOptions::named("pan"));
    let attach = pan.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    mouse_down(f.cx, MouseButton::Left, 50., 50.);
    mouse_up(f.cx, 50., 50.);
    assert_eq!(f.clicks.get(), 1);
    assert_eq!(pan.state(), GestureState::Possible);
}

#[gpui::test]
fn a_touch_on_an_area_is_the_arenas_alone_and_its_allowed_tap_clicks_once(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let record = events.clone();
    let f = mount(cx, move |attachment, host| {
        let mut options = PressTrackerOptions::new(attachment.bounds_reader());
        options.on_control_event = Some(Rc::new(move |event, _: &GestureTouch| {
            record.borrow_mut().push(event)
        }));
        let tracker = PressTracker::new(options, host.clock());
        attachment.set_consumer(Some(tracker.consumer()));
    });
    touch(f.cx, 1, gpui::TouchPhase::Started, 60., 60.);
    assert_eq!(*events.borrow(), [ControlEvent::TouchDown]);
    touch(f.cx, 1, gpui::TouchPhase::Ended, 60., 60.);
    assert_eq!(
        *events.borrow(),
        [ControlEvent::TouchDown, ControlEvent::TouchUpInside]
    );
    // The host replayed the allowed tap to GPUI: one click, with the touch
    // modality.
    assert_eq!(f.clicks.get(), 1);
    f.cx.update(|window, _| assert!(window.last_input_was_touch()));
}

#[gpui::test]
fn a_tap_recognizer_takes_a_finger_and_a_double_tap_defers_then_replays_its_click(
    cx: &mut TestAppContext,
) {
    let double = TapGestureRecognizer::new(TapOptions {
        number_of_taps_required: 2,
        ..TapOptions::named("double")
    });
    let log = log_of(&double);
    let attach = double.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    touch(f.cx, 1, gpui::TouchPhase::Started, 60., 60.);
    touch(f.cx, 1, gpui::TouchPhase::Ended, 60., 60.);
    // The click waits on the double tap.
    assert_eq!(f.clicks.get(), 0);
    f.cx.executor().advance_clock(Duration::from_millis(400));
    f.cx.run_until_parked();
    assert_eq!(*log.borrow(), ["double:failed", "double:possible"]);
    assert_eq!(f.clicks.get(), 1);

    touch(f.cx, 2, gpui::TouchPhase::Started, 60., 60.);
    touch(f.cx, 2, gpui::TouchPhase::Ended, 60., 60.);
    touch(f.cx, 3, gpui::TouchPhase::Started, 61., 61.);
    touch(f.cx, 3, gpui::TouchPhase::Ended, 61., 61.);
    assert!(log.borrow().iter().any(|e| e == "double:ended"));
    // The double tap took both taps' clicks.
    f.cx.executor().advance_clock(Duration::from_millis(400));
    f.cx.run_until_parked();
    assert_eq!(f.clicks.get(), 1);
}

#[gpui::test]
fn a_long_press_begins_on_the_windows_clock(cx: &mut TestAppContext) {
    let press = LongPressGestureRecognizer::new(LongPressOptions::named("press"));
    let attach = press.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    touch(f.cx, 1, gpui::TouchPhase::Started, 60., 60.);
    assert_eq!(press.state(), GestureState::Possible);
    f.cx.executor().advance_clock(Duration::from_millis(499));
    f.cx.run_until_parked();
    assert_eq!(press.state(), GestureState::Possible);
    f.cx.executor().advance_clock(Duration::from_millis(1));
    f.cx.run_until_parked();
    assert_eq!(press.state(), GestureState::Began);
    let host = f.cx.update(gesture_host);
    assert_eq!(host.fingers_down(), 1);
    touch(f.cx, 1, gpui::TouchPhase::Ended, 60., 60.);
    assert_eq!(press.state(), GestureState::Possible);
    assert_eq!(host.fingers_down(), 0);
    assert_eq!(f.clicks.get(), 0);
}

#[gpui::test]
fn a_finger_nobody_claims_becomes_the_windows_scroll(cx: &mut TestAppContext) {
    let pan = PanGestureRecognizer::new(PanOptions {
        direction: PanDirection::Horizontal,
        ..PanOptions::named("pan")
    });
    let log = log_of(&pan);
    let attach = pan.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    touch(f.cx, 1, gpui::TouchPhase::Started, 60., 40.);
    touch(f.cx, 1, gpui::TouchPhase::Moved, 61., 46.);
    touch(f.cx, 1, gpui::TouchPhase::Moved, 61., 70.);
    // A vertical drag on a horizontal pan: the pan fails at its 10 px, and
    // the window (the axis the pan leaves it) takes the finger, which resets
    // the failed pan.
    assert_eq!(*log.borrow(), ["pan:failed", "pan:possible"]);
    touch(f.cx, 1, gpui::TouchPhase::Moved, 61., 80.);
    touch(f.cx, 1, gpui::TouchPhase::Ended, 61., 80.);
    let scrolls = f.scrolls.borrow().clone();
    assert!(
        scrolls
            .iter()
            .all(|(source, _)| *source == ScrollSource::Touch)
    );
    let total: f32 = scrolls.iter().map(|(_, dy)| dy).sum();
    assert!((total - 40.).abs() < 0.01, "{scrolls:?}");
    assert!(log.borrow().iter().any(|e| e == "pan:failed"));
    assert_eq!(f.clicks.get(), 0);
}

#[gpui::test]
fn a_horizontal_drag_on_a_horizontal_pan_stays_the_pans(cx: &mut TestAppContext) {
    let pan = PanGestureRecognizer::new(PanOptions {
        direction: PanDirection::Horizontal,
        ..PanOptions::named("pan")
    });
    let attach = pan.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    touch(f.cx, 1, gpui::TouchPhase::Started, 60., 40.);
    touch(f.cx, 1, gpui::TouchPhase::Moved, 75., 41.);
    touch(f.cx, 1, gpui::TouchPhase::Moved, 120., 42.);
    assert_eq!(pan.state(), GestureState::Changed);
    assert_eq!(pan.translation().x, 50.);
    touch(f.cx, 1, gpui::TouchPhase::Ended, 120., 42.);
    assert!(f.scrolls.borrow().is_empty());
}

#[gpui::test]
fn a_trackpad_scroll_drives_a_pan_that_allows_it_and_scrolls_nothing_under_it(
    cx: &mut TestAppContext,
) {
    let mut options = PanOptions::named("pan");
    options.base.allowed_scroll_types = Some(vec![ScrollType::Continuous]);
    let pan = PanGestureRecognizer::new(options);
    let attach = pan.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    for _ in 0..4 {
        f.cx.simulate_event(ScrollEvent {
            position: point(px(60.), px(60.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(6.))),
            modifiers: Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
            source: ScrollSource::Pointer,
        });
        f.cx.run_until_parked();
    }
    assert!(pan.state().is_active());
    assert!(pan.translation().y > 0.);
    // The wheels the pan took reached no scroller.
    assert!(f.scrolls.borrow().is_empty());
    f.cx.executor().advance_clock(Duration::from_millis(400));
    f.cx.run_until_parked();
    assert_eq!(pan.state(), GestureState::Possible);
}

#[gpui::test]
fn a_mouse_wheel_never_drives_a_pan(cx: &mut TestAppContext) {
    let mut options = PanOptions::named("pan");
    options.base.allowed_scroll_types = Some(vec![ScrollType::Continuous]);
    let pan = PanGestureRecognizer::new(options);
    let attach = pan.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    for _ in 0..4 {
        f.cx.simulate_event(ScrollEvent {
            position: point(px(60.), px(60.)),
            delta: ScrollDelta::Lines(point(0., 1.)),
            modifiers: Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
            source: ScrollSource::Pointer,
        });
        f.cx.run_until_parked();
    }
    assert_eq!(pan.state(), GestureState::Possible);
    assert_eq!(f.scrolls.borrow().len(), 4);
}

#[gpui::test]
fn a_trackpad_pinch_drives_a_pinch(cx: &mut TestAppContext) {
    let pinch = PinchGestureRecognizer::new(PinchOptions::named("pinch"));
    let attach = pinch.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    for (delta, phase) in [
        (0.05, gpui::TouchPhase::Started),
        (0.1, gpui::TouchPhase::Moved),
        (0.1, gpui::TouchPhase::Moved),
    ] {
        f.cx.simulate_event(PinchEvent {
            position: point(px(60.), px(60.)),
            delta,
            modifiers: Modifiers::default(),
            phase,
        });
        f.cx.run_until_parked();
    }
    assert!(pinch.state().is_active());
    assert!(pinch.scale() > 1.);
    f.cx.simulate_event(PinchEvent {
        position: point(px(60.), px(60.)),
        delta: 0.,
        modifiers: Modifiers::default(),
        phase: gpui::TouchPhase::Ended,
    });
    f.cx.run_until_parked();
    assert_eq!(pinch.state(), GestureState::Possible);
}

#[gpui::test]
fn a_right_click_and_the_context_menu_keys_make_context_requests(cx: &mut TestAppContext) {
    let seen: Rc<RefCell<Vec<context_request::ContextRequestKind>>> = Rc::default();
    let record = seen.clone();
    let kept: Rc<RefCell<Option<context_request::ContextRequestAttachment>>> = Rc::default();
    let keep = kept.clone();
    let f = mount(cx, move |attachment, host| {
        let request = context_request::attach_context_request(
            host,
            attachment,
            context_request::ContextRequestOptions::new(move |request| {
                record.borrow_mut().push(request.kind)
            }),
        );
        *keep.borrow_mut() = Some(request);
    });
    mouse_down(f.cx, MouseButton::Right, 60., 60.);
    assert_eq!(
        *seen.borrow(),
        [context_request::ContextRequestKind::Pointer]
    );
    let focus = f.focus.clone();
    f.attachment.set_focus(Some(focus.clone()));
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.executor().advance_clock(Duration::from_millis(1000));
    f.cx.simulate_keystrokes("shift-f10");
    f.cx.run_until_parked();
    assert_eq!(
        *seen.borrow(),
        [
            context_request::ContextRequestKind::Pointer,
            context_request::ContextRequestKind::Keyboard
        ]
    );
    // A held finger requests as it begins.
    touch(f.cx, 1, gpui::TouchPhase::Started, 60., 60.);
    f.cx.executor().advance_clock(Duration::from_millis(600));
    f.cx.run_until_parked();
    assert_eq!(
        seen.borrow().last(),
        Some(&context_request::ContextRequestKind::Hold)
    );
    touch(f.cx, 1, gpui::TouchPhase::Ended, 60., 60.);
    drop(kept);
}

#[gpui::test]
fn the_pointer_the_host_sees_decides_the_coarse_primary_pointer(cx: &mut TestAppContext) {
    let tap = TapGestureRecognizer::new(TapOptions::named("tap"));
    let attach = tap.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    touch(f.cx, 1, gpui::TouchPhase::Started, 60., 60.);
    touch(f.cx, 1, gpui::TouchPhase::Ended, 60., 60.);
    assert!(f.cx.update(|window, cx| crate::coarse_primary_pointer(window, cx)));
    mouse_move(f.cx, 70., 70., false);
    assert!(
        !f.cx
            .update(|window, cx| crate::coarse_primary_pointer(window, cx))
    );
}

#[gpui::test]
fn a_listener_reaches_the_window_through_after_gesture(cx: &mut TestAppContext) {
    let pan = PanGestureRecognizer::new(PanOptions::named("pan"));
    let reached = Rc::new(Cell::new(0));
    let count = reached.clone();
    pan.add_listener(move |pan| {
        let translation = pan.translation();
        let count = count.clone();
        host::after_gesture(move |window, _cx| {
            let _ = window.viewport_size();
            if translation.x >= 0. {
                count.set(count.get() + 1);
            }
        });
    });
    let attach = pan.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    mouse_down(f.cx, MouseButton::Left, 50., 50.);
    mouse_move(f.cx, 70., 50., true);
    mouse_up(f.cx, 70., 50.);
    assert_eq!(reached.get(), 2);
    let _ = f.attachment.key();
}

#[gpui::test]
fn a_two_finger_pinch_by_touch_drives_a_pinch(cx: &mut TestAppContext) {
    let pinch = PinchGestureRecognizer::new(PinchOptions::named("pinch"));
    let log = log_of(&pinch);
    let attach = pinch.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    touch(f.cx, 11, gpui::TouchPhase::Started, 60., 60.);
    touch(f.cx, 12, gpui::TouchPhase::Started, 160., 60.);
    assert_eq!(pinch.number_of_touches(), 2);
    touch(f.cx, 12, gpui::TouchPhase::Moved, 170., 60.);
    assert_eq!(pinch.state(), GestureState::Began);
    // Against the starting span moved by the hysteresis: 110 / 108.
    assert!((pinch.scale() - 110. / 108.).abs() < 1e-5);
    touch(f.cx, 11, gpui::TouchPhase::Moved, 40., 60.);
    assert!((pinch.scale() - 130. / 108.).abs() < 1e-5);
    assert_eq!(pinch.focal_point(), Point::new(105., 60.));
    touch(f.cx, 11, gpui::TouchPhase::Ended, 40., 60.);
    touch(f.cx, 12, gpui::TouchPhase::Ended, 170., 60.);
    assert_eq!(
        *log.borrow(),
        [
            "pinch:began",
            "pinch:changed",
            "pinch:ended",
            "pinch:possible"
        ]
    );
    // GPUI's own recognizer saw neither finger: no tap, no scroll.
    assert_eq!(f.clicks.get(), 0);
    assert!(f.scrolls.borrow().is_empty());
    let host = f.cx.update(gesture_host);
    assert_eq!(host.fingers_down(), 0);
}

#[gpui::test]
fn a_two_finger_pan_by_touch_follows_its_fingers_centroid(cx: &mut TestAppContext) {
    let pan = PanGestureRecognizer::new(PanOptions {
        minimum_number_of_touches: 2,
        ..PanOptions::named("pan")
    });
    let log = log_of(&pan);
    let attach = pan.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    touch(f.cx, 21, gpui::TouchPhase::Started, 60., 60.);
    // One finger is not enough, however far it moves.
    touch(f.cx, 21, gpui::TouchPhase::Moved, 60., 80.);
    assert_eq!(pan.state(), GestureState::Possible);
    touch(f.cx, 22, gpui::TouchPhase::Started, 120., 80.);
    let host = f.cx.update(gesture_host);
    assert_eq!(host.fingers_down(), 2);
    touch(f.cx, 21, gpui::TouchPhase::Moved, 60., 100.);
    touch(f.cx, 22, gpui::TouchPhase::Moved, 120., 100.);
    assert!(pan.state().is_active(), "{:?}", log.borrow());
    touch(f.cx, 21, gpui::TouchPhase::Moved, 60., 120.);
    touch(f.cx, 22, gpui::TouchPhase::Moved, 120., 120.);
    // The centroid travelled 60 px since the first finger landed (the second
    // finger's landing re-based it, with no jump); the translation leaves out
    // the 10 px the hysteresis took.
    assert_eq!(pan.total_translation(), Point::new(0., 60.));
    assert_eq!(pan.translation().y, 50.);
    // Below its minimum, the pan ends; it resets once its last finger lifts.
    touch(f.cx, 22, gpui::TouchPhase::Ended, 120., 120.);
    assert_eq!(pan.state(), GestureState::Ended);
    touch(f.cx, 21, gpui::TouchPhase::Ended, 60., 120.);
    assert_eq!(pan.state(), GestureState::Possible);
    assert!(log.borrow().iter().any(|e| e == "pan:ended"));
    // No window scroll and no click from either finger.
    assert!(f.scrolls.borrow().is_empty());
    assert_eq!(f.clicks.get(), 0);
}

#[gpui::test]
fn a_touch_off_every_area_stays_gpuis(cx: &mut TestAppContext) {
    let tap = TapGestureRecognizer::new(TapOptions::named("tap"));
    let attach = tap.clone();
    let f = mount(cx, move |attachment, _| attachment.add_recognizer(&attach));
    // Outside the area: GPUI's recognizer pans it into a touch scroll.
    touch(f.cx, 31, gpui::TouchPhase::Started, 400., 400.);
    let host = f.cx.update(gesture_host);
    assert_eq!(host.fingers_down(), 1);
    touch(f.cx, 31, gpui::TouchPhase::Moved, 400., 440.);
    touch(f.cx, 31, gpui::TouchPhase::Ended, 400., 440.);
    assert_eq!(host.fingers_down(), 0);
    assert!(
        f.scrolls
            .borrow()
            .iter()
            .any(|(source, _)| *source == ScrollSource::Touch)
    );
    assert_eq!(tap.state(), GestureState::Possible);
}
