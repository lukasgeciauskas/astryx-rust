//! The keyboard module's tests, one block per upstream test file: `keyboardModel.test.ts`,
//! `revealBand.test.ts`, `rideKeyboard.test.ts`, and the window cases of
//! `keyboardHeight.test.ts` and `keyboardChromium.test.ts` that GPUI can stand for: the
//! visual viewport and insets the test platform simulates, focus moves, and the guide.
//! Left out as DOM-specific: WebKit's zoomed `innerHeight`, classic scrollbars, iframes,
//! the shared-realm stores of a second copy of Core, `preventScroll` focus, the tap
//! route's synthesized mouse events and caret APIs (GPUI's web build focuses its hidden
//! input without a scroll at the canvas's corner, and places the caret itself).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Bounds, Context, FocusHandle, IntoElement, Render, TestAppContext, VisualTestContext, Window,
    div, point, prelude::*, px, size,
};

use super::*;
use crate::motion::android::keyboard as android;
use crate::motion::{MotionEngine, MotionProperty, MotionValues, cubic_bezier};

// --- keyboardModel.test.ts -------------------------------------------------------------

#[test]
fn fr8_rides_a_keyboard_coming_up_from_its_report_dated_back_to_gboards_start_on_gboards_slide() {
    let mut model = KeyboardModel::new(0.);
    let show = model.report(Some(312.), 1000.).expect("a show");
    let start = 1000. - android::SHOW_REPORT_LAG_MS;
    assert_eq!(show.kind, KeyboardTransitionKind::Show);
    assert_eq!((show.from, show.to, show.start), (0., 312., start));
    assert!(show.animated);
    assert_eq!(show.curve, Some(SHOW_SLIDE));
    let up = cubic_bezier(android::SHOW_CURVE);
    let joined = 312. * up.ease(android::SHOW_REPORT_LAG_MS / android::SHOW_MS);
    assert!((model.presentation(1000.) - joined).abs() < 1e-6);
    assert_eq!(model.presentation(start + android::SHOW_MS), 312.);
}

#[test]
fn rides_to_the_last_keyboards_height_for_the_accessory_bars_report_then_confirms_it() {
    let mut model = KeyboardModel::new(0.);
    model.report(Some(312.), 1000.);
    model.report(Some(0.), 2000.);
    let show = model.report(Some(48.), 3000.).expect("a show");
    assert_eq!(show.to, 312.);
    assert!(show.animated && show.predicted);
    assert!(model.report(Some(312.), 3120.).is_none());
    assert!(!model.transition().predicted);
}

#[test]
fn moves_once_if_the_keyboard_that_comes_is_not_the_last_one() {
    let mut model = KeyboardModel::new(0.);
    model.report(Some(312.), 1000.);
    model.report(Some(0.), 2000.);
    model.report(Some(48.), 3000.);
    let change = model.report(Some(260.), 3120.).expect("a change");
    assert_eq!(change.kind, KeyboardTransitionKind::Change);
    assert_eq!(change.to, 260.);
    assert!(!change.animated);
}

#[test]
fn rides_a_hide_down_from_hide_after_blur_ms_after_a_blur_on_the_hide_curve() {
    let mut model = KeyboardModel::new(0.);
    model.report(Some(312.), 1000.);
    let hide = model.blurred(2000.).expect("a hide");
    let start = 2000. + android::HIDE_AFTER_BLUR_MS;
    assert_eq!(hide.kind, KeyboardTransitionKind::Hide);
    assert_eq!((hide.to, hide.start), (0., start));
    assert!(hide.predicted);
    assert_eq!(hide.curve.map(|c| c.duration_ms), Some(android::HIDE_MS));
    let down = cubic_bezier(android::HIDE_CURVE);
    let half = model.presentation(start + android::HIDE_MS / 2.);
    assert!((half - 312. * (1. - down.ease(0.5))).abs() < 1e-6);
    assert_eq!(model.presentation(start + android::HIDE_MS), 0.);
    // Chrome's report confirms the prediction.
    assert!(model.report(Some(0.), start + 2.).is_none());
}

#[test]
fn rides_a_hide_nothing_predicted_back_down_from_its_report_dated_back_to_gboards_start() {
    let mut model = KeyboardModel::new(0.);
    model.report(Some(312.), 1000.);
    let hide = model.report(Some(0.), 2010.).expect("a hide");
    assert_eq!(hide.kind, KeyboardTransitionKind::Hide);
    assert_eq!(hide.start, 2010. - android::HIDE_REPORT_LAG_MS);
    assert!(hide.animated);
}

#[test]
fn turns_back_up_on_gboards_slide_when_focus_returns_during_a_predicted_hide() {
    let mut model = KeyboardModel::new(0.);
    model.report(Some(312.), 1000.);
    model.blurred(2000.);
    let back = model.focused(2060.).expect("a show");
    assert_eq!(back.kind, KeyboardTransitionKind::Show);
    assert_eq!(back.to, 312.);
    assert!(back.animated);
    assert_eq!(back.curve.map(|c| c.duration_ms), Some(android::SHOW_MS));
}

#[test]
fn a_prediction_the_viewport_never_confirms_gives_way_to_what_it_says() {
    let mut model = KeyboardModel::new(0.);
    model.report(Some(312.), 1000.);
    model.blurred(2000.);
    // Too early: the hide has not ended.
    assert!(model.verify(Some(312.), 2050.).is_none());
    let back = model.verify(Some(312.), 2500.).expect("the viewport wins");
    assert_eq!(back.kind, KeyboardTransitionKind::Show);
    assert_eq!(back.to, 312.);
}

// --- revealBand.test.ts ----------------------------------------------------------------

#[test]
fn scrolls_a_box_below_the_band_up_by_how_far_its_bottom_overhangs() {
    assert_eq!(distance_into_band(500., 540., 0., 451.), 89.);
}

#[test]
fn scrolls_a_box_above_the_band_down_by_how_far_its_top_overhangs() {
    assert_eq!(distance_into_band(-120., -80., 0., 451.), -120.);
    assert_eq!(distance_into_band(40., 80., 64., 451.), -24.);
}

#[test]
fn leaves_a_box_that_fits() {
    assert_eq!(distance_into_band(100., 140., 0., 451.), 0.);
}

#[test]
fn keeps_a_box_taller_than_the_band_by_its_bottom_edge() {
    assert_eq!(distance_into_band(-50., 600., 0., 451.), 149.);
}

#[test]
fn moves_nothing_when_the_band_is_empty() {
    assert_eq!(distance_into_band(500., 540., 300., 300.), 0.);
    assert_eq!(distance_into_band(500., 540., 300., 200.), 0.);
}

// --- rideKeyboard.test.ts --------------------------------------------------------------

fn engine_at(clock: Rc<std::cell::Cell<f64>>) -> MotionEngine {
    MotionEngine::new(crate::motion::EngineOptions {
        now: Some(Rc::new(move || clock.get())),
        ..Default::default()
    })
}

fn slide(curve: KeyboardCurve) -> KeyboardTransition {
    KeyboardTransition {
        kind: KeyboardTransitionKind::Show,
        from: 0.,
        to: 300.,
        start: 1000.,
        velocity: 0.,
        animated: true,
        predicted: false,
        curve: Some(curve),
    }
}

#[test]
fn plays_the_eased_slide_the_transition_publishes_where_it_has_one() {
    let clock = Rc::new(std::cell::Cell::new(1000.));
    let engine = engine_at(clock.clone());
    let handle = ride_keyboard(
        &engine,
        MotionValues::new().y(-300.),
        &slide(SHOW_SLIDE),
        1000.,
        false,
    );
    let handle = handle.expect("a played slide");
    let curve = handle.curve(MotionProperty::Y).expect("a curve");
    assert!((curve.duration() - android::SHOW_MS / 1000.).abs() < 1e-9);
    assert_eq!(curve.at(curve.duration()), -300.);
    // Mid-slide on Gboard's curve, from the transition's start.
    let up = cubic_bezier(android::SHOW_CURVE);
    let mid = engine.get(MotionProperty::Y, Some(1000. + android::SHOW_MS / 2.));
    assert!((mid - -300. * up.ease(0.5)).abs() < 1e-6);
}

#[test]
fn plays_the_transitions_own_curve_even_a_show_published_with_the_hide_slide() {
    let clock = Rc::new(std::cell::Cell::new(1000.));
    let engine = engine_at(clock);
    let handle = ride_keyboard(
        &engine,
        MotionValues::new().y(-300.),
        &slide(HIDE_SLIDE),
        1000.,
        false,
    )
    .expect("a played slide");
    let curve = handle.curve(MotionProperty::Y).expect("a curve");
    assert!((curve.duration() - android::HIDE_MS / 1000.).abs() < 1e-9);
}

#[test]
fn lands_at_once_when_the_slide_has_ended_by_the_call_as_a_page_busy_through_the_slide_reads_its_report()
 {
    let late = 1000. + android::SHOW_MS + 50.;
    let clock = Rc::new(std::cell::Cell::new(late));
    let engine = engine_at(clock);
    engine.set(MotionValues::new().y(-120.));
    let handle = ride_keyboard(
        &engine,
        MotionValues::new().y(0.),
        &slide(SHOW_SLIDE),
        late,
        false,
    );
    assert!(handle.is_none());
    assert_eq!(engine.get(MotionProperty::Y, None), 0.);
}

#[test]
fn asks_the_engine_for_its_entrance_under_reduce_motion_where_the_ride_is_an_entrance() {
    let clock = Rc::new(std::cell::Cell::new(1000.));
    let engine = engine_at(clock.clone());
    engine.set_reduced_motion(true);
    engine.set_coarse_pointer(true);
    engine.set(MotionValues::new().y(80.));
    ride_keyboard(
        &engine,
        MotionValues::new().y(0.),
        &slide(SHOW_SLIDE),
        1000.,
        true,
    );
    // Nothing travels: the element lands, and an entrance fades in from nothing.
    clock.set(1001.);
    assert_eq!(engine.get(MotionProperty::Y, Some(1001.)), 0.);
    assert!(engine.shown_opacity(Some(1001.)) < 1.);
}

#[test]
fn lands_at_once_when_the_keyboard_jumps() {
    let clock = Rc::new(std::cell::Cell::new(1000.));
    let engine = engine_at(clock);
    let jump = KeyboardTransition {
        animated: false,
        curve: None,
        ..slide(SHOW_SLIDE)
    };
    assert!(ride_keyboard(&engine, MotionValues::new().y(-300.), &jump, 1000., false).is_none());
    assert_eq!(engine.get(MotionProperty::Y, None), -300.);
}

// --- keyboardHeight.test.ts and keyboardChromium.test.ts, in a window ------------------

#[derive(Default)]
struct Log(RefCell<Vec<String>>);

struct Page {
    field: FocusHandle,
    other: FocusHandle,
    kind: FieldKind,
}

impl Render for Page {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        keyboard_probe(window, cx);
        let (field, kind) = (self.field.clone(), self.kind);
        div()
            .size_full()
            .child(
                div().id("field").track_focus(&self.field).h(px(40.)).child(
                    gpui::canvas(
                        move |bounds, window, cx| {
                            register_field(&field, kind, bounds, true, window, cx)
                        },
                        |_, _, _, _| {},
                    )
                    .size_full(),
                ),
            )
            .child(div().id("other").track_focus(&self.other).h(px(40.)))
    }
}

fn window(
    cx: &mut TestAppContext,
    kind: FieldKind,
) -> (gpui::Entity<Page>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(move |_, cx| Page {
        field: cx.focus_handle(),
        other: cx.focus_handle(),
        kind,
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    (view, cx)
}

const TEXT: FieldKind = FieldKind {
    text_entry: true,
    select: false,
    picker_over_page: false,
};

/// The keyboard covers `height` px of the window's bottom (the visual viewport shrinks),
/// panned `pan` px down.
fn keyboard(cx: &mut VisualTestContext, height: f32, pan: f32) {
    let handle = cx.update(|window, _| window.window_handle());
    let full = cx.update(|window, _| window.viewport_size());
    cx.simulate_window_visual_viewport_change(
        handle,
        Bounds::new(
            point(px(0.), px(pan)),
            size(full.width, full.height - px(height)),
        ),
    );
    cx.run_until_parked();
}

fn record(cx: &mut VisualTestContext, focus: bool) -> (Rc<Log>, KeyboardSubscription) {
    let log = Rc::new(Log::default());
    let sink = log.clone();
    let sub = cx.update(|window, cx| {
        subscribe_keyboard_height(window, cx, focus, move |event, _, _| {
            let line = match event {
                KeyboardHeightEvent::Viewport { height, .. } => format!("viewport {height}"),
                KeyboardHeightEvent::FocusIn { target, .. } => {
                    format!("focusin {}", target.is_text_entry())
                }
                KeyboardHeightEvent::FocusOut {
                    target, related, ..
                } => format!(
                    "focusout {} -> {}",
                    target.is_text_entry(),
                    matches!(related, FocusTarget::Nothing)
                ),
            };
            sink.0.borrow_mut().push(line);
        })
    });
    cx.run_until_parked();
    (log, sub)
}

#[gpui::test]
fn fr2_is_the_layout_viewport_minus_the_visual_viewport(cx: &mut TestAppContext) {
    let (_, cx) = window(cx, TEXT);
    let (log, _sub) = record(cx, false);
    keyboard(cx, 300., 0.);
    assert_eq!(cx.update(get_keyboard_height), 300.);
    assert_eq!(log.0.borrow().as_slice(), ["viewport 300"]);
}

#[gpui::test]
fn fr2_is_not_made_smaller_by_a_page_pan(cx: &mut TestAppContext) {
    let (_, cx) = window(cx, TEXT);
    let (log, _sub) = record(cx, false);
    keyboard(cx, 300., 0.);
    keyboard(cx, 300., 120.);
    assert_eq!(cx.update(get_keyboard_height), 300.);
    // The pan is a report of the same height, which a reader puts back (FR4).
    assert_eq!(log.0.borrow().as_slice(), ["viewport 300", "viewport 300"]);
}

#[gpui::test]
fn fr2_is_0_with_no_keyboard_shown(cx: &mut TestAppContext) {
    let (_, cx) = window(cx, TEXT);
    assert_eq!(cx.update(|window, _| measure_keyboard_height(window)), 0.);
    // Under a pixel is rounding, not a keyboard.
    keyboard(cx, 0.6, 0.);
    assert_eq!(cx.update(|window, _| measure_keyboard_height(window)), 0.);
}

#[gpui::test]
fn fr1_shares_the_listeners_among_subscribers_and_removes_them_with_the_last(
    cx: &mut TestAppContext,
) {
    let (_, cx) = window(cx, TEXT);
    let (a, sub_a) = record(cx, false);
    let (b, sub_b) = record(cx, true);
    assert_eq!(cx.update(listener_count), (2, 1));
    keyboard(cx, 300., 0.);
    // One measurement, the same height for every reader at the same moment.
    assert_eq!(a.0.borrow().as_slice(), ["viewport 300"]);
    assert_eq!(b.0.borrow().as_slice(), ["viewport 300"]);
    drop(sub_a);
    drop(sub_b);
    assert_eq!(cx.update(listener_count), (0, 0));
}

#[gpui::test]
fn fr1_reads_a_keyboard_that_is_already_up_when_the_first_subscriber_attaches(
    cx: &mut TestAppContext,
) {
    let (_, cx) = window(cx, TEXT);
    keyboard(cx, 250., 0.);
    let (log, _sub) = record(cx, false);
    assert_eq!(cx.update(get_keyboard_height), 250.);
    assert!(log.0.borrow().is_empty(), "no change to report");
}

#[gpui::test]
fn listens_to_focus_moves_only_while_a_subscriber_asks_for_them(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, TEXT);
    let (plain, _a) = record(cx, false);
    let (focus, _b) = record(cx, true);
    let field = view.read_with(cx, |p, _| p.field.clone());
    cx.update(|window, cx| window.focus(&field, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    assert!(plain.0.borrow().is_empty());
    assert_eq!(
        focus.0.borrow().as_slice(),
        ["focusin true", "focusout true -> true"]
    );
}

#[gpui::test]
fn the_text_entry_predicate_reads_an_input_mode_none_field_as_no_text_entry(
    cx: &mut TestAppContext,
) {
    let (view, cx) = window(cx, FieldKind::default());
    let (focus, _sub) = record(cx, true);
    let field = view.read_with(cx, |p, _| p.field.clone());
    cx.update(|window, cx| window.focus(&field, cx));
    cx.run_until_parked();
    assert_eq!(focus.0.borrow().as_slice(), ["focusin false"]);
    assert!(!cx.update(is_text_entry_focused));
}

fn steps(cx: &mut TestAppContext) -> (Rc<Log>, KeyboardSubscription, &mut VisualTestContext) {
    cx.update(|cx| {
        set_virtual_keyboard_api(true, cx);
        crate::set_reduced_motion(true, cx);
    });
    let (_, cx) = window(cx, TEXT);
    let (log, sub) = record(cx, false);
    keyboard(cx, 300., 0.);
    (log, sub, cx)
}

#[gpui::test]
fn fr8_reports_the_hide_once_with_the_report_of_none_and_keeps_the_shown_height_until_then(
    cx: &mut TestAppContext,
) {
    let (log, _sub, cx) = steps(cx);
    keyboard(cx, 200., 0.);
    keyboard(cx, 90., 0.);
    assert_eq!(cx.update(get_keyboard_height), 300.);
    keyboard(cx, 0., 0.);
    assert_eq!(log.0.borrow().as_slice(), ["viewport 300", "viewport 0"]);
}

#[gpui::test]
fn fr8_reports_a_lower_keyboard_that_stays_up_after_a_quiet_spell(cx: &mut TestAppContext) {
    let (log, _sub, cx) = steps(cx);
    keyboard(cx, 200., 0.);
    cx.executor().advance_clock(Duration::from_millis(310));
    cx.run_until_parked();
    assert_eq!(log.0.borrow().as_slice(), ["viewport 300", "viewport 200"]);
}

#[gpui::test]
fn fr8_waits_1_s_at_most_in_all_however_long_the_steps_go_on(cx: &mut TestAppContext) {
    let (log, _sub, cx) = steps(cx);
    for (i, height) in [280., 260., 240., 220.].into_iter().enumerate() {
        keyboard(cx, height, 0.);
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
        let _ = i;
    }
    // 1 s after the first step, the height in place is reported.
    assert_eq!(
        log.0.borrow().last().map(String::as_str),
        Some("viewport 220")
    );
}

#[gpui::test]
fn fr8_reports_a_keyboard_that_comes_back_up_at_once(cx: &mut TestAppContext) {
    let (log, _sub, cx) = steps(cx);
    keyboard(cx, 200., 0.);
    keyboard(cx, 300., 0.);
    assert_eq!(log.0.borrow().as_slice(), ["viewport 300", "viewport 300"]);
}

// The guide (keyboardChromium.test.ts).

fn guide_log(
    cx: &mut VisualTestContext,
) -> (Rc<RefCell<Vec<KeyboardTransition>>>, GuideSubscription) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    let sub = cx.update(|window, cx| {
        keyboard_layout_guide(window, cx)
            .subscribe(window, cx, move |t, _, _| sink.borrow_mut().push(*t))
    });
    cx.run_until_parked();
    (log, sub)
}

#[gpui::test]
fn guide_rides_a_keyboard_coming_up_from_its_report(cx: &mut TestAppContext) {
    cx.update(|cx| set_virtual_keyboard_api(true, cx));
    let (_, cx) = window(cx, TEXT);
    let (log, _sub) = guide_log(cx);
    let before = cx.update(|_, cx| keyboard_now(cx));
    keyboard(cx, 312., 0.);
    let show = log.borrow()[0];
    assert_eq!(show.kind, KeyboardTransitionKind::Show);
    assert_eq!(show.to, 312.);
    assert!((show.start - (before - android::SHOW_REPORT_LAG_MS)).abs() < 1.);
    assert_eq!(
        cx.update(|window, cx| keyboard_layout_guide(window, cx).profile()),
        Some(KeyboardProfile::Chromium)
    );
}

#[gpui::test]
fn guide_rides_a_hide_down_from_hide_after_blur_ms_after_our_blur(cx: &mut TestAppContext) {
    cx.update(|cx| set_virtual_keyboard_api(true, cx));
    let (view, cx) = window(cx, TEXT);
    let field = view.read_with(cx, |p, _| p.field.clone());
    cx.update(|window, cx| window.focus(&field, cx));
    cx.run_until_parked();
    let (log, _sub) = guide_log(cx);
    keyboard(cx, 312., 0.);
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    let hide = *log.borrow().last().unwrap();
    assert_eq!(hide.kind, KeyboardTransitionKind::Hide);
    assert!(hide.predicted);
    // Chrome's late report of the keyboard still up starts nothing.
    let count = log.borrow().len();
    keyboard(cx, 200., 0.);
    assert_eq!(log.borrow().len(), count);
    keyboard(cx, 0., 0.);
    assert!(!cx.update(|window, cx| keyboard_layout_guide(window, cx).transition().predicted));
}

#[gpui::test]
fn guide_holds_the_momentary_hide_chrome_reports_while_focus_moves_to_another_field(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| set_virtual_keyboard_api(true, cx));
    let (view, cx) = window(cx, TEXT);
    let field = view.read_with(cx, |p, _| p.field.clone());
    let (log, _sub) = guide_log(cx);
    keyboard(cx, 312., 0.);
    cx.update(|window, cx| window.focus(&field, cx));
    cx.run_until_parked();
    let count = log.borrow().len();
    // Gone and back within the handoff: nothing moves.
    keyboard(cx, 0., 0.);
    keyboard(cx, 312., 0.);
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(log.borrow().len(), count);
    assert_eq!(
        cx.update(|window, cx| keyboard_layout_guide(window, cx).inset()),
        312.
    );
}

#[gpui::test]
fn follows_keyboard_holds_only_under_a_coarse_pointer(cx: &mut TestAppContext) {
    let (_, cx) = window(cx, TEXT);
    assert!(!cx.update(follows_keyboard));
}

// --- useLayerKeyboardClearance.test.tsx ------------------------------------------------

struct Anchored;

impl Render for Anchored {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let full = window.viewport_size();
        let trigger = div()
            .debug_selector(|| "trigger".into())
            .mt(full.height - px(520.))
            .h(px(20.))
            .w(px(80.));
        let surface = div()
            .id("list")
            .debug_selector(|| "surface".into())
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .child(div().h(px(400.)).w(px(80.)).flex_none());
        div()
            .size_full()
            .child(crate::layer::anchored_layer("menu", trigger).surface(
                surface,
                crate::LayerOptions {
                    placement: crate::Placement::Below,
                    ..Default::default()
                },
                |_, _| {},
            ))
    }
}

#[gpui::test]
fn ast059_a_layer_that_would_run_under_the_keyboard_ends_its_clearance_above_it(
    cx: &mut TestAppContext,
) {
    let (_, cx) = cx.add_window_view(|_, _| Anchored);
    cx.run_until_parked();
    let full = cx.update(|window, _| window.viewport_size());
    let top_before = cx.debug_bounds("surface").unwrap().top();
    keyboard(cx, 300., 0.);
    // A rising keyboard caps the layer once, when it arrives.
    cx.executor().advance_clock(Duration::from_millis(500));
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    let capped = cx.debug_bounds("surface").unwrap();
    let limit = f32::from(full.height) - 300. - crate::layer::LAYER_KEYBOARD_CLEARANCE_PX as f32;
    assert!(
        f32::from(capped.bottom()) <= limit + 1.,
        "the layer ends at {}, its limit {limit}",
        f32::from(capped.bottom())
    );
    assert_eq!(capped.top(), top_before, "it keeps its block-start edge");
    // The keyboard goes: the layer grows back at once.
    keyboard(cx, 0., 0.);
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    assert_eq!(cx.debug_bounds("surface").unwrap().size.height, px(400.));
}

#[test]
fn keyboard_clearance_cap_is_none_when_the_layer_is_clear() {
    use crate::layer::keyboard_clearance_cap;
    assert_eq!(keyboard_clearance_cap(100., 300., 800., 0.), None);
    assert_eq!(keyboard_clearance_cap(100., 300., 800., 400.), None);
    assert_eq!(keyboard_clearance_cap(100., 500., 800., 400.), Some(290.));
    assert_eq!(keyboard_clearance_cap(500., 600., 800., 400.), Some(0.));
}

// --- The native insets path (iOS and Android report the keyboard's inset themselves) ---

/// The platform reports a keyboard covering `height` px of the window's bottom; the visual
/// viewport stands still.
fn ime(cx: &mut VisualTestContext, height: f32) {
    cx.simulate_insets_change(gpui::WindowInsets {
        ime: gpui::Edges {
            bottom: px(height),
            ..Default::default()
        },
        ..Default::default()
    });
    cx.run_until_parked();
}

#[gpui::test]
fn fr2_reads_the_platforms_own_keyboard_inset(cx: &mut TestAppContext) {
    let (_, cx) = window(cx, TEXT);
    let (log, _sub) = record(cx, false);
    ime(cx, 280.);
    assert_eq!(cx.update(|window, _| measure_keyboard_height(window)), 280.);
    assert_eq!(cx.update(get_keyboard_height), 280.);
    ime(cx, 0.);
    assert_eq!(log.0.borrow().as_slice(), ["viewport 280", "viewport 0"]);
}

#[gpui::test]
fn fr2_takes_the_larger_of_the_inset_and_the_visual_viewports_shortfall(cx: &mut TestAppContext) {
    let (_, cx) = window(cx, TEXT);
    let (_log, _sub) = record(cx, false);
    keyboard(cx, 200., 0.);
    ime(cx, 260.);
    assert_eq!(cx.update(get_keyboard_height), 260.);
}

#[gpui::test]
fn ast059_the_platforms_keyboard_inset_caps_an_anchored_layer(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Anchored);
    cx.run_until_parked();
    let full = cx.update(|window, _| window.viewport_size());
    ime(cx, 300.);
    cx.executor().advance_clock(Duration::from_millis(500));
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    let capped = cx.debug_bounds("surface").unwrap();
    let limit = f32::from(full.height) - 300. - crate::layer::LAYER_KEYBOARD_CLEARANCE_PX as f32;
    assert!(f32::from(capped.bottom()) <= limit + 1.);
}
