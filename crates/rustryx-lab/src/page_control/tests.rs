//! Upstream `pageControlModel.test.ts` (the page-control math against UIPageControl's
//! measurements) and `PageControl.test.tsx` (the semantics, the keyboard, the measured
//! geometry and alphas, the width cap, the window, the fades and the pointers), plus
//! the jog and the window's slide from `PageControl.interaction.browser.spec.ts`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, Context, Modifiers, MouseButton, Render, TestAppContext, TouchEvent, TouchId,
    TouchPhase, VisualTestContext, div, point,
};
use rustryx_i18n::{Direction, I18n};

use super::*;

fn close(a: f64, b: f64, digits: i32) -> bool {
    // Vitest's toBeCloseTo: |a - b| < 10^-digits / 2.
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

// ---------------------------------------------------------------------------
// pageControlModel.test.ts

#[test]
fn layout_matches_uipagecontrols_measured_width_for_5_pages() {
    let layout = page_control_layout(5.);
    assert!(close(layout.row_width, 80.35, 1));
    assert!(close(layout.width, 108.3, 0));
    assert_eq!(layout.slots, 5);
    assert!(!layout.is_windowed);
}

#[test]
fn layout_caps_the_width_at_161_3_for_50_pages_and_shows_ten_slots() {
    let layout = page_control_layout(50.);
    assert_eq!(layout.width, 161.3);
    assert_eq!(layout.slots, 10);
    assert!(layout.is_windowed);
}

#[test]
fn layout_is_empty_without_pages() {
    assert_eq!(
        page_control_layout(0.),
        PageControlLayout {
            width: 28.,
            slots: 0,
            row_width: 0.,
            is_windowed: false,
        }
    );
}

/// The sizes of the ten shown slots, rounded to 0.01 pt.
fn shown(count: usize, window_start: usize) -> Vec<f64> {
    (0..10)
        .map(|i| {
            js_round(indicator_size(count, window_start, (window_start + i) as isize) * 100.) / 100.
        })
        .collect()
}

use rustryx_theme::authoring::js::js_round;

#[test]
fn window_page_0_pages_0_to_7_full_then_7_33_and_4_31() {
    let start = window_for(50, 0, 0);
    assert_eq!(start, 0);
    assert_eq!(
        shown(50, start),
        [9.67, 9.67, 9.67, 9.67, 9.67, 9.67, 9.67, 9.67, 7.33, 4.31]
    );
    assert_eq!(indicator_size(50, start, 10), 0.);
}

#[test]
fn window_page_10_from_page_0_the_current_page_in_the_last_full_slot_edges_shrunk_on_both_sides() {
    let start = window_for(50, 0, 10);
    assert_eq!(start, 3);
    assert_eq!(
        shown(50, start),
        [4.31, 7.33, 9.67, 9.67, 9.67, 9.67, 9.67, 9.67, 7.33, 4.31]
    );
    assert_eq!(full_pages(50, start), (5, 10));
}

#[test]
fn window_page_25_from_page_10_the_same_shape_the_current_page_last() {
    let start = window_for(50, 3, 25);
    assert_eq!(full_pages(50, start).1, 25);
}

#[test]
fn window_page_49_eight_full_slots_ending_at_the_current_page() {
    let start = window_for(50, 18, 49);
    assert_eq!(start, 40);
    assert_eq!(
        shown(50, start),
        [4.31, 7.33, 9.67, 9.67, 9.67, 9.67, 9.67, 9.67, 9.67, 9.67]
    );
}

#[test]
fn window_moves_only_when_the_current_page_would_land_in_a_shrunk_slot() {
    assert_eq!(window_for(50, 3, 7), 3);
    assert_eq!(window_for(50, 3, 11), 4);
    assert_eq!(window_for(50, 3, 4), 2);
}

#[test]
fn a_control_with_ten_pages_or_fewer_never_windows() {
    assert_eq!(window_for(10, 0, 9), 0);
    assert_eq!(shown(10, 0), [9.67; 10]);
}

#[test]
fn a_tap_on_the_trailing_side_moves_forward_one_page_the_leading_side_back() {
    assert_eq!(tap_target(60., 50., 2., 5., false), 3.);
    assert_eq!(tap_target(40., 50., 2., 5., false), 1.);
}

#[test]
fn tap_target_mirrors_in_rtl() {
    assert_eq!(tap_target(40., 50., 2., 5., true), 3.);
    assert_eq!(tap_target(60., 50., 2., 5., true), 1.);
}

#[test]
fn tap_target_stops_at_the_first_and_last_pages() {
    assert_eq!(tap_target(60., 50., 4., 5., false), 4.);
    assert_eq!(tap_target(40., 50., 0., 5., false), 0.);
}

#[test]
fn scrub_switches_at_the_midpoint_between_indicator_centres() {
    // The measured switch points (192.3 / 210.3 / 229.0 on a 5-page control whose
    // first centre is at 166), relative to the first indicator's centre.
    assert_eq!(scrub_target(192.3 - 166. - 0.5, 5, 0), 1.);
    assert_eq!(scrub_target(192.3 - 166. + 0.5, 5, 0), 2.);
    assert_eq!(scrub_target(210.3 - 166. - 0.5, 5, 0), 2.);
    assert_eq!(scrub_target(210.3 - 166. + 0.5, 5, 0), 3.);
    assert_eq!(scrub_target(1.5 * PITCH + 2.5 * PITCH, 5, 0), 4.);
}

#[test]
fn scrub_stays_within_the_pages() {
    assert_eq!(scrub_target(-40., 5, 0), 0.);
    assert_eq!(scrub_target(400., 5, 0), 4.);
}

#[test]
fn scrub_in_a_window_stays_within_the_pages_shown_at_full_size() {
    // Window at 3 (page 10 current): full pages 5–10.
    assert_eq!(scrub_target(0., 50, 3), 5.);
    assert_eq!(scrub_target(5. * PITCH, 50, 3), 8.);
    assert_eq!(scrub_target(9. * PITCH, 50, 3), 10.);
}

#[test]
fn jog_does_not_step_inside_the_content() {
    assert_eq!(jog_interval(0., 1.), f64::INFINITY);
    assert_eq!(jog_interval(-5., 1.), f64::INFINITY);
}

#[test]
fn jog_steps_faster_the_further_past_the_edge_at_most_15_pages_a_second() {
    // The least-squares fit of the measured run: 266 ms at 19 pt, 66 ms at 76 pt.
    assert!(close(jog_interval(19., 1.), 0.304, 2));
    assert!(close(jog_interval(45.7, 1.), 0.126, 2));
    assert!(close(jog_interval(76., 1.), 0.076, 2));
    assert!(close(jog_interval(110., 1.), 1. / 15., 5));
}

#[test]
fn a_run_toward_the_first_page_steps_slower_than_toward_the_last() {
    // spec:AST-094 FR1: 10 pages a second 77 pt past the edge toward the first page,
    // 13.3 toward the last.
    assert!(close(1. / jog_interval(77., -1.), 10., 0));
    assert!(close(1. / jog_interval(77., 1.), 13.3, 1));
    assert!(close(jog_interval(150., -1.), 1. / 15., 5));
}

#[test]
fn the_page_count_and_the_current_page_read_any_number() {
    assert_eq!(page_count_of(5.9), 5);
    assert_eq!(page_count_of(-3.), 0);
    assert_eq!(page_count_of(f64::NAN), 0);
    assert_eq!(page_count_of(f64::INFINITY), 0);
    assert_eq!(current_of(3., 5), 2);
    assert_eq!(current_of(99., 5), 4);
    assert_eq!(current_of(-4., 5), 0);
    assert_eq!(current_of(f64::NAN, 5), 0);
    // JavaScript's Math.round: 2.5 rounds up.
    assert_eq!(current_of(3.5, 5), 3);
}

#[test]
fn with_a_mouse_the_cropped_row_keeps_the_current_indicator_in_view() {
    // A row that fits is not shifted; 5 × 17.67 − 8 lands a rounding error past the
    // 80.35 px the control shows, as it does in JavaScript.
    assert!(cropped_row_offset(5., 2.) < 1e-12);
    assert_eq!(cropped_row_offset(4., 2.), 0.);
    assert_eq!(cropped_row_offset(50., 0.), 0.);
    assert!(close(cropped_row_offset(50., 5.), 26.535, 6));
    // Pinned at the end.
    assert!(close(
        cropped_row_offset(50., 49.),
        50. * PITCH - 8. - 133.3,
        6
    ));
}

#[test]
fn inactive_dots_reach_3_to_1_on_cores_default_page_background() {
    // WCAG 1.4.11: Core's light defaults, --color-text-primary #0A1317 on
    // --color-background-body #F1F4F7.
    fn contrast_over(fg: [f64; 3], bg: [f64; 3], alpha: f64) -> f64 {
        let luminance = |rgb: [f64; 3]| {
            let [r, g, b] = rgb.map(|value| {
                let v = value / 255.;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            0.2126 * r + 0.7152 * g + 0.0722 * b
        };
        let mut mixed = [0.; 3];
        for i in 0..3 {
            mixed[i] = fg[i] * alpha + bg[i] * (1. - alpha);
        }
        (luminance(bg) + 0.05) / (luminance(mixed) + 0.05)
    }
    let (fg, bg) = ([10., 19., 23.], [241., 244., 247.]);
    assert!(contrast_over(fg, bg, INACTIVE_ALPHA) >= 3.);
    assert!(
        contrast_over(fg, bg, OTHER_INDICATOR_ALPHA) < 3.,
        "UIKit's 45 % falls short"
    );
}

// ---------------------------------------------------------------------------
// PageControl.test.tsx

type Log = Rc<RefCell<Vec<usize>>>;

/// The app: a control it pages (`follows`) or one whose value it keeps.
struct Owner {
    total: f64,
    page: f64,
    follows: bool,
    label: Option<&'static str>,
    hides: bool,
    rtl: bool,
    log: Log,
}

impl Owner {
    fn new(total: f64, page: f64) -> Self {
        Owner {
            total,
            page,
            follows: false,
            label: None,
            hides: false,
            rtl: false,
            log: Log::default(),
        }
    }
}

impl Render for Owner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (log, follows, view) = (self.log.clone(), self.follows, cx.entity());
        let mut control = PageControl::new("pc", self.total, self.page)
            .hides_for_single_page(self.hides)
            .on_change(move |page, _, cx| {
                log.borrow_mut().push(page);
                if follows {
                    view.update(cx, |o, cx| {
                        o.page = page as f64;
                        cx.notify();
                    });
                }
            });
        if let Some(label) = self.label {
            control = control.label(label);
        }
        let el = div().p(px(20.)).child(control);
        let el: AnyElement = if self.rtl {
            rustryx_i18n::provider(I18n::new("en").dir(Direction::Rtl), el).into_any_element()
        } else {
            el.into_any_element()
        };
        el
    }
}

fn mount(cx: &mut TestAppContext, owner: Owner) -> (Entity<Owner>, &mut VisualTestContext, Log) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let log = owner.log.clone();
    DRAWN.with(|d| d.borrow_mut().clear());
    let (view, cx) = cx.add_window_view(move |_, _| owner);
    cx.run_until_parked();
    (view, cx, log)
}

fn drawn() -> Drawn {
    DRAWN
        .with(|d| d.borrow().get("pc").cloned())
        .expect("the control drew")
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    cx.run_until_parked();
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
    // Draws the frame the time reached, as the screen would.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

/// The control's box in the window.
fn control(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.debug_bounds("pc-page-control")
        .expect("the control is drawn")
}

/// Page `page`'s dot centre in the window, as drawn, and the control's vertical centre.
fn dot(cx: &mut VisualTestContext, page: usize) -> (f32, f32) {
    let b = control(cx);
    let x = drawn()
        .dots
        .iter()
        .find(|d| d.page == page)
        .expect("the dot is drawn")
        .x;
    (
        f32::from(b.origin.x) + x as f32,
        f32::from(b.origin.y) + HEIGHT as f32 / 2.,
    )
}

fn mouse_down(cx: &mut VisualTestContext, (x, y): (f32, f32)) {
    cx.simulate_mouse_down(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
}

fn mouse_move(cx: &mut VisualTestContext, (x, y): (f32, f32)) {
    cx.simulate_mouse_move(
        point(px(x), px(y)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.run_until_parked();
}

fn mouse_up(cx: &mut VisualTestContext, (x, y): (f32, f32)) {
    cx.simulate_mouse_up(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
}

fn click(cx: &mut VisualTestContext, (x, y): (f32, f32)) {
    cx.simulate_click(point(px(x), px(y)), Modifiers::none());
    cx.run_until_parked();
}

fn finger(cx: &mut VisualTestContext, phase: TouchPhase, (x, y): (f32, f32)) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: point(px(x), px(y)),
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

/// A tap far from the control, so the window reads its primary pointer as coarse.
fn as_touch(cx: &mut VisualTestContext) {
    cx.simulate_event(TouchEvent {
        id: TouchId(99),
        phase: TouchPhase::Started,
        position: point(px(700.), px(500.)),
        predicted_position: None,
        force: None,
    });
    cx.simulate_event(TouchEvent {
        id: TouchId(99),
        phase: TouchPhase::Ended,
        position: point(px(700.), px(500.)),
        predicted_position: None,
        force: None,
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(drawn().touch, "the touch presentation is on");
}

#[gpui::test]
fn is_one_slider_named_page_indicators_that_reads_page_3_of_5(cx: &mut TestAppContext) {
    let (_, cx, log) = mount(cx, Owner::new(5., 3.));
    let d = drawn();
    assert_eq!(d.label.as_ref(), "Page indicators");
    assert_eq!(d.value_text.as_ref(), "Page 3 of 5");
    assert_eq!((d.value_now, d.value_max), (3, 5));
    // One tab stop.
    press(cx, "tab");
    press(cx, "right");
    assert_eq!(*log.borrow(), [4]);
}

#[gpui::test]
fn takes_a_custom_label(cx: &mut TestAppContext) {
    let mut owner = Owner::new(5., 3.);
    owner.label = Some("Onboarding");
    let _ = mount(cx, owner);
    assert_eq!(drawn().label.as_ref(), "Onboarding");
}

#[gpui::test]
fn arrow_keys_home_and_end_ask_for_a_page(cx: &mut TestAppContext) {
    let (_, cx, log) = mount(cx, Owner::new(5., 3.));
    press(cx, "tab");
    for (key, page) in [
        ("right", 4),
        ("left", 2),
        ("up", 4),
        ("down", 2),
        ("home", 1),
        ("end", 5),
    ] {
        press(cx, key);
        assert_eq!(log.borrow().last(), Some(&page), "{key}");
    }
}

#[gpui::test]
fn mirrors_the_horizontal_arrows_in_rtl(cx: &mut TestAppContext) {
    let mut owner = Owner::new(5., 3.);
    owner.rtl = true;
    let (_, cx, log) = mount(cx, owner);
    press(cx, "tab");
    press(cx, "right");
    assert_eq!(log.borrow().last(), Some(&2));
    press(cx, "left");
    assert_eq!(log.borrow().last(), Some(&4));
}

#[gpui::test]
fn asks_for_nothing_past_the_first_or_last_page(cx: &mut TestAppContext) {
    let (_, cx, log) = mount(cx, Owner::new(5., 5.));
    press(cx, "tab");
    press(cx, "right");
    press(cx, "end");
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn draws_uipagecontrols_geometry_with_the_others_at_48_percent(cx: &mut TestAppContext) {
    let (_, cx, _) = mount(cx, Owner::new(5., 3.));
    let d = drawn();
    assert_eq!(d.width, 108.35);
    let b = control(cx);
    // Layout rounds boxes to whole pixels.
    assert!((f64::from(b.size.height) - 25.67).abs() <= 0.5);
    assert!((f64::from(b.size.width) - 108.35).abs() <= 0.5);
    assert_eq!(d.dots.len(), 5);
    assert!(d.dots.iter().all(|dot| (dot.size - 9.67).abs() < 1e-9));
    assert_eq!(
        d.dots.iter().map(|dot| dot.alpha).collect::<Vec<_>>(),
        [0.48, 0.48, 1., 0.48, 0.48]
    );
    // The strip is centred: the first centre 14 px in, then every 17.67 px.
    assert!(close(d.dots[0].x, 14. + 4.835, 6));
    assert!(close(d.dots[4].x - d.dots[0].x, 4. * PITCH, 6));
}

#[gpui::test]
fn never_grows_wider_than_161_3_and_under_a_finger_renders_the_window_around_the_current_page(
    cx: &mut TestAppContext,
) {
    let (_, cx, _) = mount(cx, Owner::new(50., 26.));
    as_touch(cx);
    let d = drawn();
    assert_eq!(d.width, 161.3);
    // From page 1, page 26 (dot 25) sits in the window's last full slot. The window
    // starts at dot 18, with dots 20–25 full. Three slots on each side render for the
    // slide.
    assert_eq!(d.dots.first().map(|dot| dot.page), Some(15));
    assert_eq!(d.dots.last().map(|dot| dot.page), Some(30));
    let size = |page: usize| d.dots.iter().find(|dot| dot.page == page).unwrap().size;
    assert_eq!(
        (size(18), size(19), size(20), size(25), size(26), size(27)),
        (4.31, 7.33, 9.67, 9.67, 7.33, 4.31)
    );
}

#[gpui::test]
fn with_a_mouse_a_long_control_is_the_full_row_of_pages_cropped(cx: &mut TestAppContext) {
    let (_, cx, _) = mount(cx, Owner::new(50., 26.));
    let d = drawn();
    assert_eq!(d.width, 161.3);
    assert!(!d.touch);
    assert_eq!(d.dots.len(), 50);
    assert!(d.dots.iter().all(|dot| dot.size == INDICATOR_SIZE));
    // No fades and no shrinking slots with a mouse.
    assert!(d.dots.iter().all(|dot| dot.fade.ms == 0.));
    assert!(close(d.offset, cropped_row_offset(50., 25.), 9));
    let _ = control(cx);
}

#[gpui::test]
fn under_a_finger_the_old_indicator_fades_on_the_tables_measured_250_ms_and_curve(
    cx: &mut TestAppContext,
) {
    let (view, cx, _) = mount(cx, Owner::new(5., 3.));
    as_touch(cx);
    view.update(cx, |o, cx| {
        o.page = 4.;
        cx.notify();
    });
    cx.run_until_parked();
    let d = drawn();
    let old = d.dots[2].fade;
    assert_eq!(
        (old.from, old.to, old.ms),
        (1., INACTIVE_ALPHA, FADE_OUT_MS)
    );
    assert_eq!(old.curve, [0., 0., 0.58, 1.]);
    // Every other opacity change is a switch.
    let new = d.dots[3].fade;
    assert_eq!((new.to, new.ms), (1., SWITCH_MS));
    assert_eq!(new.curve, [0.42, 0., 0.58, 1.]);
    // The fade runs on: half way, the old dot is between its alphas.
    wait(cx, 125);
    let alpha = drawn().dots[2].alpha;
    assert!(alpha < 1. && alpha > INACTIVE_ALPHA, "{alpha}");
    wait(cx, 200);
    assert_eq!(drawn().dots[2].alpha, INACTIVE_ALPHA);
}

#[gpui::test]
fn under_a_finger_a_tap_fades_the_old_indicator_faster(cx: &mut TestAppContext) {
    let mut owner = Owner::new(5., 3.);
    owner.follows = true;
    let (_, cx, log) = mount(cx, owner);
    as_touch(cx);
    let (x, y) = dot(cx, 2);
    finger(cx, TouchPhase::Started, (x + 6., y));
    finger(cx, TouchPhase::Ended, (x + 6., y));
    wait(cx, 16);
    assert_eq!(*log.borrow(), [4]);
    let old = drawn().dots[2].fade;
    assert_eq!((old.to, old.ms), (INACTIVE_ALPHA, TAP_FADE_OUT_MS));
}

#[gpui::test]
fn under_a_finger_and_reduce_motion_the_current_indicator_changes_at_once(cx: &mut TestAppContext) {
    let (view, cx, _) = mount(cx, Owner::new(5., 3.));
    cx.update(|_, cx| rustryx_primitives::set_reduced_motion(true, cx));
    as_touch(cx);
    view.update(cx, |o, cx| {
        o.page = 4.;
        cx.notify();
    });
    cx.run_until_parked();
    let d = drawn();
    assert_eq!(d.dots.len(), 5);
    assert!(d.dots.iter().all(|dot| dot.fade.ms == 0.));
    assert_eq!(d.dots[2].alpha, INACTIVE_ALPHA);
    assert_eq!(d.dots[3].alpha, 1.);
}

#[gpui::test]
fn under_a_finger_and_reduce_motion_the_held_look_shows_at_once_and_hides_on_the_policys_fade(
    cx: &mut TestAppContext,
) {
    let (_, cx, _) = mount(cx, Owner::new(5., 3.));
    cx.update(|_, cx| rustryx_primitives::set_reduced_motion(true, cx));
    as_touch(cx);
    let (x, y) = dot(cx, 0);
    finger(cx, TouchPhase::Started, (x, y));
    wait(cx, 300);
    let d = drawn();
    assert!(d.interacting);
    assert_eq!((d.platter, d.platter_fade.ms), (1., 0.));
    assert!(d.dots.iter().all(|dot| dot.fade.ms == 0.));
    assert_eq!(d.dots[0].alpha, INTERACTING_OTHER_INDICATOR_ALPHA);
    finger(cx, TouchPhase::Ended, (x, y));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let d = drawn();
    assert!(!d.interacting);
    assert_eq!((d.platter_fade.to, d.platter_fade.ms), (0., 200.));
    assert_eq!(d.platter_fade.curve, [0.42, 0., 0.58, 1.]);
    for page in [0, 1, 3, 4] {
        let fade = d.dots[page].fade;
        assert_eq!(
            (fade.to, fade.ms, fade.curve),
            (INACTIVE_ALPHA, 200., [0.42, 0., 0.58, 1.])
        );
    }
}

#[gpui::test]
fn with_a_mouse_a_click_on_the_trailing_side_asks_for_the_next_page_on_the_leading_side_for_the_previous(
    cx: &mut TestAppContext,
) {
    let (_, cx, log) = mount(cx, Owner::new(5., 3.));
    let (x, y) = dot(cx, 2);
    click(cx, (x + 6., y));
    assert_eq!(log.borrow().last(), Some(&4));
    click(cx, (x - 6., y));
    assert_eq!(log.borrow().last(), Some(&2));
}

#[gpui::test]
fn with_a_mouse_a_press_and_release_pages_once(cx: &mut TestAppContext) {
    // Through the tap recognizer: the click that follows does not page again.
    let (_, cx, log) = mount(cx, Owner::new(5., 3.));
    let (x, y) = dot(cx, 2);
    mouse_down(cx, (x + 6., y));
    mouse_up(cx, (x + 6., y));
    assert_eq!(*log.borrow(), [4]);
    click(cx, (x + 6., y));
    assert_eq!(*log.borrow(), [4, 4], "a second click is a second tap");
}

#[gpui::test]
fn with_a_mouse_a_drag_scrubs_to_the_dot_under_the_pointer(cx: &mut TestAppContext) {
    // spec:AST-094 FR43.
    let (_, cx, log) = mount(cx, Owner::new(5., 3.));
    let (x, y) = dot(cx, 2);
    mouse_down(cx, (x, y));
    for i in 1..=8 {
        wait(cx, 16);
        mouse_move(cx, (x + 4. * i as f32, y));
    }
    mouse_up(cx, (x + 32., y));
    assert!(log.borrow().contains(&4));
    assert_eq!(log.borrow().last(), Some(&5));
}

#[gpui::test]
fn with_a_mouse_a_scrub_on_a_cropped_long_row_holds_the_row_still_until_the_lift(
    cx: &mut TestAppContext,
) {
    let mut owner = Owner::new(50., 6.);
    owner.follows = true;
    let (_, cx, _) = mount(cx, owner);
    let rest = drawn().offset;
    let (x, y) = dot(cx, 5);
    mouse_down(cx, (x, y));
    for i in 1..=12 {
        wait(cx, 16);
        mouse_move(cx, (x + 4. * i as f32, y));
        assert_eq!(drawn().offset, rest);
    }
    assert_eq!(drawn().value_now, 9);
    mouse_up(cx, (x + 48., y));
    assert_ne!(drawn().offset, rest);
}

#[gpui::test]
fn with_a_mouse_a_press_held_past_155_ms_shows_no_platter_and_dims_nothing(
    cx: &mut TestAppContext,
) {
    let (_, cx, _) = mount(cx, Owner::new(5., 3.));
    let rest = drawn().dots[0].alpha;
    let (x, y) = dot(cx, 2);
    mouse_down(cx, (x + 6., y));
    wait(cx, 300);
    let d = drawn();
    assert!(!d.interacting);
    assert_eq!(d.platter, 0.);
    assert_eq!(d.dots[0].alpha, rest);
    mouse_up(cx, (x + 6., y));
}

#[gpui::test]
fn under_a_finger_a_held_finger_shows_the_platter_and_dims_the_other_dots(cx: &mut TestAppContext) {
    let (_, cx, _) = mount(cx, Owner::new(5., 3.));
    as_touch(cx);
    let (x, y) = dot(cx, 2);
    finger(cx, TouchPhase::Started, (x, y));
    wait(cx, 100);
    assert!(!drawn().interacting, "not before 155 ms");
    wait(cx, 100);
    let d = drawn();
    assert!(d.interacting);
    assert_eq!(d.dots[0].fade.to, INTERACTING_OTHER_INDICATOR_ALPHA);
    assert_eq!(d.platter_fade.to, 1.);
    finger(cx, TouchPhase::Ended, (x, y));
    assert!(!drawn().interacting);
}

#[gpui::test]
fn under_a_coarse_pointer_a_hit_area_grows_the_25_67_px_tall_control_toward_44_px(
    cx: &mut TestAppContext,
) {
    let (_, cx, log) = mount(cx, Owner::new(5., 3.));
    as_touch(cx);
    // Measured from the frame before.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let b = control(cx);
    let (x, _) = dot(cx, 2);
    let top = f32::from(b.origin.y);
    // (44 − 25.67) / 2 = 9.17 px above the control still reaches it; 12 px does not.
    finger(cx, TouchPhase::Started, (x + 6., top - 8.));
    finger(cx, TouchPhase::Ended, (x + 6., top - 8.));
    wait(cx, 50);
    assert_eq!(*log.borrow(), [4]);
    finger(cx, TouchPhase::Started, (x + 6., top - 12.));
    finger(cx, TouchPhase::Ended, (x + 6., top - 12.));
    wait(cx, 50);
    assert_eq!(*log.borrow(), [4]);
}

#[gpui::test]
fn with_a_mouse_the_control_takes_no_grown_hit_area(cx: &mut TestAppContext) {
    let (_, cx, log) = mount(cx, Owner::new(5., 3.));
    let b = control(cx);
    let (x, _) = dot(cx, 2);
    click(cx, (x + 6., f32::from(b.origin.y) - 4.));
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn under_a_finger_a_vertical_swipe_that_starts_on_the_dots_pages_nothing(cx: &mut TestAppContext) {
    let (_, cx, log) = mount(cx, Owner::new(5., 3.));
    as_touch(cx);
    let (x, y) = dot(cx, 2);
    let (x, y) = (x + 6., y);
    finger(cx, TouchPhase::Started, (x, y));
    for i in 1..=30 {
        wait(cx, 8);
        let s = i as f32 / 30.;
        finger(cx, TouchPhase::Moved, (x + 2. * s, y - 300. * s));
    }
    finger(cx, TouchPhase::Ended, (x + 2., y - 300.));
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn under_a_finger_a_tap_pages_once(cx: &mut TestAppContext) {
    // The click the platform sends after a tap does not page again.
    let (_, cx, log) = mount(cx, Owner::new(5., 3.));
    as_touch(cx);
    let (x, y) = dot(cx, 2);
    finger(cx, TouchPhase::Started, (x + 6., y));
    finger(cx, TouchPhase::Ended, (x + 6., y));
    wait(cx, 50);
    assert_eq!(*log.borrow(), [4]);
}

#[gpui::test]
fn under_a_finger_a_scrub_follows_the_finger_from_16_px(cx: &mut TestAppContext) {
    let mut owner = Owner::new(5., 1.);
    owner.follows = true;
    let (_, cx, log) = mount(cx, owner);
    as_touch(cx);
    let (x, y) = dot(cx, 0);
    finger(cx, TouchPhase::Started, (x, y));
    for i in 1..=4 {
        wait(cx, 16);
        finger(cx, TouchPhase::Moved, (x + 3.5 * i as f32, y));
    }
    assert!(log.borrow().is_empty(), "14 px is no scrub yet");
    for to in [PITCH, 2. * PITCH, 3. * PITCH + 2.].map(|to| to as f32) {
        wait(cx, 16);
        finger(cx, TouchPhase::Moved, (x + to, y));
    }
    finger(cx, TouchPhase::Ended, (x + 3. * PITCH as f32 + 2., y));
    assert_eq!(
        *log.borrow(),
        [2, 3, 4],
        "the page under the finger, no tap at the lift"
    );
}

#[gpui::test]
fn past_the_windows_edge_a_scrub_keeps_stepping_on_its_own(cx: &mut TestAppContext) {
    let mut owner = Owner::new(50., 1.);
    owner.follows = true;
    let (_, cx, log) = mount(cx, owner);
    as_touch(cx);
    let b = control(cx);
    let (x, y) = dot(cx, 0);
    finger(cx, TouchPhase::Started, (x, y));
    // Out past the trailing edge: 77 px past the content, 13.3 pages a second.
    let end = f32::from(b.origin.x + b.size.width) - HORIZONTAL_PADDING as f32;
    let target = end + 77.;
    for i in 1..=8 {
        wait(cx, 8);
        finger(cx, TouchPhase::Moved, (x + (target - x) * i as f32 / 8., y));
    }
    let scrubbed = log.borrow().len();
    wait(cx, 750);
    let stepped = log.borrow().len() - scrubbed;
    // 750 ms at 1 / 13.32 s a step: 9 or 10 steps, one page at a time.
    assert!((9..=10).contains(&stepped), "{stepped} steps");
    let pages = log.borrow()[scrubbed - 1..].to_vec();
    assert!(pages.windows(2).all(|w| w[1] == w[0] + 1), "{pages:?}");
    finger(cx, TouchPhase::Ended, (target, y));
    let after = log.borrow().len();
    wait(cx, 500);
    assert_eq!(log.borrow().len(), after, "the lift stops the jog");
}

#[gpui::test]
fn the_window_slides_on_uikits_spring_50_ms_after_the_page_changes(cx: &mut TestAppContext) {
    let (view, cx, _) = mount(cx, Owner::new(50., 8.));
    as_touch(cx);
    let rest = drawn().offset;
    assert_eq!(rest, 0.);
    // Page 9 (dot 8) would land in a shrunk slot: the window moves one slot on.
    view.update(cx, |o, cx| {
        o.page = 9.;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(drawn().offset, 0., "it waits windowSlideDelay");
    wait(cx, 66);
    let mid = drawn().offset;
    assert!(mid > 0. && mid < PITCH, "{mid}");
    wait(cx, 1000);
    assert!(close(drawn().offset, PITCH, 6));
    let d = drawn();
    let size = |page: usize| d.dots.iter().find(|dot| dot.page == page).unwrap().size;
    assert_eq!(
        (size(1), size(2), size(3), size(9), size(10)),
        (4.31, 7.33, 9.67, 7.33, 4.31)
    );
}

#[gpui::test]
fn clicks_work_when_the_control_first_renders_with_no_pages_and_the_pages_arrive_later(
    cx: &mut TestAppContext,
) {
    let (view, cx, log) = mount(cx, Owner::new(0., 1.));
    assert!(DRAWN.with(|d| d.borrow().get("pc").is_none()));
    assert!(cx.debug_bounds("pc-page-control").is_none());
    view.update(cx, |o, cx| {
        o.total = 5.;
        o.page = 3.;
        cx.notify();
    });
    cx.run_until_parked();
    let (x, y) = dot(cx, 2);
    click(cx, (x + 6., y));
    assert_eq!(*log.borrow(), [4]);
}

#[gpui::test]
fn a_page_the_app_declines_can_be_asked_for_again(cx: &mut TestAppContext) {
    // The app keeps its value: no render follows on_change.
    let (_, cx, log) = mount(cx, Owner::new(5., 3.));
    let (x, y) = dot(cx, 2);
    click(cx, (x + 6., y));
    click(cx, (x + 6., y));
    assert_eq!(log.borrow().len(), 2);
    press(cx, "tab");
    // Within one task, the page asked for is asked for once.
    cx.update(|window, cx| {
        let right = gpui::Keystroke::parse("right").unwrap();
        window.dispatch_keystroke(right.clone(), cx);
        window.dispatch_keystroke(right, cx);
    });
    cx.run_until_parked();
    assert_eq!(log.borrow().len(), 3);
    // Once it lapsed, asking again calls on_change again.
    press(cx, "right");
    assert_eq!(log.borrow().len(), 4);
}

#[gpui::test]
fn a_long_control_already_draws_its_resting_offset_and_sizes_on_its_first_frame(
    cx: &mut TestAppContext,
) {
    let (_, _, _) = mount(cx, Owner::new(50., 27.));
    let d = drawn();
    assert!(d.offset > 0.);
    // A 10 px box scaled to 0.967.
    assert!(d.dots.iter().all(|dot| close(dot.size / 10., 0.967, 9)));
}

#[gpui::test]
fn with_hides_for_single_page_renders_nothing_for_one_page(cx: &mut TestAppContext) {
    let mut owner = Owner::new(1., 1.);
    owner.hides = true;
    let (_, cx, _) = mount(cx, owner);
    assert!(DRAWN.with(|d| d.borrow().get("pc").is_none()));
    assert!(cx.debug_bounds("pc-page-control").is_none());
}

#[gpui::test]
fn without_hides_for_single_page_one_page_draws_one_dot(cx: &mut TestAppContext) {
    let (_, _, _) = mount(cx, Owner::new(1., 1.));
    let d = drawn();
    assert_eq!(d.dots.len(), 1);
    assert_eq!(d.width, 37.67);
}
