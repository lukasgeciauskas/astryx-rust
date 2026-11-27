//! Upstream `utils/pressFeedback.test.ts` in a window: the press reaches the surface the
//! finger landed on (the innermost, never a disabled one, nothing outside every surface);
//! a finger waits only inside a scroller; a scroll, a cancel or travel takes it away and
//! nothing brings it back, until the next press; a finger on a list still gliding is a
//! brake; the release fades on the theme's exit clock; a quick tap answers at the lift; two
//! touches in a row; a Space press on a touch-first device; a mouse press left to GPUI's
//! active state where the pointer is fine.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    FocusHandle, IntoElement, ScrollDelta, ScrollEvent, ScrollSource, TestAppContext, TouchPhase,
    VisualTestContext, div, point, prelude::*, px,
};

use super::harness::*;
use crate::gestures::*;

#[derive(Default)]
struct Case {
    scroller: bool,
    disabled_button: bool,
    surfaces: RefCell<HashMap<&'static str, PressFeedback>>,
    focus: RefCell<Option<FocusHandle>>,
}

/// A row (0..300 by 0..100) holding a button (20..120 by 20..60), in a fixed bar or in a
/// scroller.
fn mount(cx: &mut TestAppContext, case: Case) -> (&mut VisualTestContext, Rc<Case>) {
    let case = Rc::new(case);
    let c = case.clone();
    let cx = host(cx, move |window, cx| {
        let row = use_press_feedback("row", window, cx);
        let focus = c
            .focus
            .borrow_mut()
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let button = use_press_feedback("button", window, cx)
            .disabled(c.disabled_button)
            .space_activates(&focus);
        c.surfaces.borrow_mut().insert("row", row.clone());
        c.surfaces.borrow_mut().insert("button", button.clone());
        let content = row.surface(
            div().id("row").relative().w(px(300.)).h(px(100.)).child(
                button.surface(
                    div()
                        .id("button")
                        .track_focus(&focus)
                        .absolute()
                        .left(px(20.))
                        .top(px(20.))
                        .w(px(100.))
                        .h(px(40.)),
                ),
            ),
        );
        if c.scroller {
            let handle = crate::gestures::scroller::use_scroll_handle("scroller", window, cx);
            div()
                .id("scroller")
                .absolute()
                .left_0()
                .top_0()
                .w(px(300.))
                .h(px(200.))
                .overflow_y_scroll()
                .track_scroll(&handle)
                .child(crate::gestures::scroller::scroll_begin(
                    "scroller",
                    Some(&handle),
                    (false, true),
                ))
                .child(div().h(px(800.)).child(content))
                .child(crate::gestures::scroller::scroll_end())
                .into_any_element()
        } else {
            content.into_any_element()
        }
    });
    (cx, case)
}

fn paint(case: &Case, name: &str) -> PressPaint {
    case.surfaces.borrow()[name].paint()
}

fn fixed() -> Case {
    Case::default()
}

fn scrolling() -> Case {
    Case {
        scroller: true,
        ..Case::default()
    }
}

fn finger_scroll(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_event(ScrollEvent {
        position: point(px(x), px(y)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-20.))),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Touch,
    });
    cx.run_until_parked();
}

#[gpui::test]
fn outside_a_scroller_paints_in_the_frame_the_finger_lands(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    touch(cx, 1, TouchPhase::Started, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::On);
}

#[gpui::test]
fn inside_a_scroller_waits_the_content_touch_delay(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, scrolling());
    touch(cx, 1, TouchPhase::Started, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    wait(cx, 149);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    wait(cx, 1);
    assert_eq!(paint(&case, "row"), PressPaint::On);
}

#[gpui::test]
fn paints_the_innermost_surface_when_they_nest(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    touch(cx, 1, TouchPhase::Started, 50., 40.);
    assert_eq!(paint(&case, "button"), PressPaint::On);
    assert_eq!(paint(&case, "row"), PressPaint::None);
}

#[gpui::test]
fn ignores_a_touch_that_lands_outside_every_pressable(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    touch(cx, 1, TouchPhase::Started, 500., 500.);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    assert_eq!(paint(&case, "button"), PressPaint::None);
}

#[gpui::test]
fn paints_nothing_on_a_disabled_surface_and_does_not_light_the_row_around_it(
    cx: &mut TestAppContext,
) {
    let (cx, case) = mount(
        cx,
        Case {
            disabled_button: true,
            ..Case::default()
        },
    );
    touch(cx, 1, TouchPhase::Started, 50., 40.);
    wait(cx, 200);
    assert_eq!(paint(&case, "button"), PressPaint::None);
    assert_eq!(paint(&case, "row"), PressPaint::None);
}

#[gpui::test]
fn leaves_a_mouse_press_to_gpuis_active_state_where_the_pointer_is_fine(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    mouse_down(cx, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    mouse_up(cx, 200., 80.);
}

#[gpui::test]
fn a_scroll_takes_it_away_and_nothing_brings_it_back(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, scrolling());
    touch(cx, 1, TouchPhase::Started, 200., 50.);
    wait(cx, 150);
    assert_eq!(paint(&case, "row"), PressPaint::On);
    finger_scroll(cx, 200., 50.);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    touch(cx, 1, TouchPhase::Moved, 200., 50.);
    wait(cx, 300);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    touch(cx, 1, TouchPhase::Ended, 200., 50.);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    // The next press, once the list has come to rest, is heard.
    wait(cx, 200);
    touch(cx, 2, TouchPhase::Started, 200., 50.);
    wait(cx, 150);
    assert_eq!(paint(&case, "row"), PressPaint::On);
}

#[gpui::test]
fn clears_on_the_platforms_cancel_and_hears_the_next_press(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    touch(cx, 1, TouchPhase::Started, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::On);
    touch(cx, 1, TouchPhase::Cancelled, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    touch(cx, 2, TouchPhase::Started, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::On);
}

#[gpui::test]
fn paints_nothing_when_the_scroller_claims_inside_the_delay(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, scrolling());
    touch(cx, 1, TouchPhase::Started, 200., 80.);
    wait(cx, 30);
    finger_scroll(cx, 200., 80.);
    wait(cx, 300);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    touch(cx, 1, TouchPhase::Ended, 200., 40.);
    assert_eq!(paint(&case, "row"), PressPaint::None);
}

#[gpui::test]
fn is_a_brake_not_a_press_on_a_list_still_moving(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, scrolling());
    finger_scroll(cx, 200., 150.);
    wait(cx, 50);
    touch(cx, 1, TouchPhase::Started, 200., 50.);
    wait(cx, 200);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    touch(cx, 1, TouchPhase::Ended, 200., 50.);
    // At rest, a press is a press again.
    wait(cx, 200);
    touch(cx, 2, TouchPhase::Started, 200., 50.);
    wait(cx, 150);
    assert_eq!(paint(&case, "row"), PressPaint::On);
}

#[gpui::test]
fn clears_when_the_finger_travels_past_the_slop(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    touch(cx, 1, TouchPhase::Started, 200., 50.);
    touch(cx, 1, TouchPhase::Moved, 200., 55.);
    assert_eq!(paint(&case, "row"), PressPaint::On);
    touch(cx, 1, TouchPhase::Moved, 200., 61.);
    assert_eq!(paint(&case, "row"), PressPaint::None);
}

#[gpui::test]
fn the_release_fades_on_the_themes_exit_clock_then_leaves_nothing(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    touch(cx, 1, TouchPhase::Started, 200., 80.);
    touch(cx, 1, TouchPhase::Ended, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::Fading);
    wait(cx, 174);
    assert_eq!(paint(&case, "row"), PressPaint::Fading);
    wait(cx, 1);
    assert_eq!(paint(&case, "row"), PressPaint::None);
}

#[gpui::test]
fn the_fade_is_no_longer_than_the_reduce_motion_bound(cx: &mut TestAppContext) {
    let (cx, _) = mount(cx, fixed());
    cx.update(|window, cx| {
        let exit = press_feedback::press_fade_ms(window, cx);
        rustryx_primitives::set_reduced_motion(true, cx);
        let reduced = press_feedback::press_fade_ms(window, cx);
        assert_eq!(exit, 175.);
        assert!(reduced <= 200. && reduced <= exit);
    });
}

#[gpui::test]
fn answers_a_tap_too_quick_for_the_delay_at_the_release(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, scrolling());
    touch(cx, 1, TouchPhase::Started, 200., 80.);
    wait(cx, 50);
    touch(cx, 1, TouchPhase::Ended, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::On);
    wait(cx, 100);
    assert_eq!(paint(&case, "row"), PressPaint::Fading);
    wait(cx, 175);
    assert_eq!(paint(&case, "row"), PressPaint::None);
}

#[gpui::test]
fn answers_the_second_tap_while_the_first_is_still_fading(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    touch(cx, 1, TouchPhase::Started, 200., 80.);
    touch(cx, 1, TouchPhase::Ended, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::Fading);
    touch(cx, 2, TouchPhase::Started, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::On);
}

#[gpui::test]
fn paints_nothing_for_a_second_finger_while_the_first_is_still_down(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    touch(cx, 1, TouchPhase::Started, 200., 80.);
    touch(cx, 2, TouchPhase::Started, 50., 40.);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    assert_eq!(paint(&case, "button"), PressPaint::None);
}

#[gpui::test]
fn a_touch_first_device_paints_a_space_press_on_a_focused_button(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    // A finger makes the primary pointer coarse.
    touch(cx, 9, TouchPhase::Started, 500., 500.);
    touch(cx, 9, TouchPhase::Ended, 500., 500.);
    let focus = case.focus.borrow().clone().unwrap();
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.run_until_parked();
    let space = gpui::Keystroke::parse("space").unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.run_until_parked();
    assert_eq!(paint(&case, "button"), PressPaint::On);
    cx.simulate_event(gpui::KeyUpEvent { keystroke: space });
    cx.run_until_parked();
    assert_eq!(paint(&case, "button"), PressPaint::Fading);
    wait(cx, 175);
    assert_eq!(paint(&case, "button"), PressPaint::None);
}

#[gpui::test]
fn no_gesture_is_ever_left_painted(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, scrolling());
    // Jitter inside the slop keeps the press; a cancel in the flash and a scroll in the
    // fade still end at rest.
    touch(cx, 1, TouchPhase::Started, 200., 80.);
    touch(cx, 1, TouchPhase::Moved, 202., 81.);
    wait(cx, 150);
    assert_eq!(paint(&case, "row"), PressPaint::On);
    touch(cx, 1, TouchPhase::Ended, 202., 81.);
    finger_scroll(cx, 200., 80.);
    wait(cx, 400);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    touch(cx, 2, TouchPhase::Started, 200., 80.);
    touch(cx, 2, TouchPhase::Ended, 200., 80.);
    touch(cx, 2, TouchPhase::Cancelled, 200., 80.);
    wait(cx, 400);
    assert_eq!(paint(&case, "row"), PressPaint::None);
}

// --- The device's primary pointer, a control's box, and real scrollers -------------

#[derive(Clone, Copy, PartialEq)]
enum Around {
    Nothing,
    /// A clipping box that does not scroll.
    Clip,
    /// A scroller whose content fits.
    FittingScroller,
}

struct One {
    feedback: RefCell<Option<PressFeedback>>,
}

/// One surface (0..200 by 0..80) that may own a control's box, in \`around\`.
fn one(
    cx: &mut TestAppContext,
    owns_box: bool,
    around: Around,
) -> (&mut VisualTestContext, Rc<One>) {
    let one = Rc::new(One {
        feedback: RefCell::default(),
    });
    let o = one.clone();
    let cx = host(cx, move |window, cx| {
        let feedback = use_press_feedback("one", window, cx).owns_box(owns_box);
        *o.feedback.borrow_mut() = Some(feedback.clone());
        let surface = feedback.surface(div().id("one").w(px(200.)).h(px(80.)));
        match around {
            Around::Nothing => surface.into_any_element(),
            Around::Clip => div()
                .absolute()
                .left_0()
                .top_0()
                .w(px(300.))
                .h(px(100.))
                .overflow_hidden()
                .child(surface)
                .into_any_element(),
            Around::FittingScroller => {
                let handle = crate::gestures::scroller::use_scroll_handle("fits", window, cx);
                div()
                    .id("fits")
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(300.))
                    .h(px(200.))
                    .overflow_y_scroll()
                    .track_scroll(&handle)
                    .child(crate::gestures::scroller::scroll_begin(
                        "fits",
                        Some(&handle),
                        (false, true),
                    ))
                    .child(surface)
                    .child(crate::gestures::scroller::scroll_end())
                    .into_any_element()
            }
        }
    });
    (cx, one)
}

fn one_paint(one: &One) -> PressPaint {
    one.feedback.borrow().as_ref().unwrap().paint()
}

#[gpui::test]
fn a_clip_that_does_not_scroll_and_a_scroller_whose_content_fits_paint_at_once(
    cx: &mut TestAppContext,
) {
    let (cx, one) = self::one(cx, false, Around::Clip);
    touch(cx, 1, TouchPhase::Started, 50., 40.);
    assert_eq!(one_paint(&one), PressPaint::On, "a clip is no scroller");
    touch(cx, 1, TouchPhase::Ended, 50., 40.);
}

#[gpui::test]
fn a_scroller_whose_content_fits_paints_at_once(cx: &mut TestAppContext) {
    let (cx, one) = self::one(cx, false, Around::FittingScroller);
    // The handle learns the scroller's size on the first frame.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    touch(cx, 1, TouchPhase::Started, 50., 40.);
    assert_eq!(
        one_paint(&one),
        PressPaint::On,
        "nothing to scroll: no delay"
    );
}

#[gpui::test]
fn a_touch_first_device_paints_a_mouse_press_at_once_holds_it_and_fades_it(
    cx: &mut TestAppContext,
) {
    let (cx, one) = self::one(cx, false, Around::Nothing);
    cx.update(|_, cx| press_feedback::set_primary_pointer_coarse(Some(true), cx));
    cx.run_until_parked();
    mouse_down(cx, 50., 40.);
    assert_eq!(one_paint(&one), PressPaint::On);
    // Dragged off, it keeps the paint, as the active state does.
    mouse_move(cx, 600., 500.);
    assert_eq!(one_paint(&one), PressPaint::On);
    mouse_up(cx, 600., 500.);
    assert_eq!(one_paint(&one), PressPaint::Fading);
    wait(cx, 175);
    assert_eq!(one_paint(&one), PressPaint::None);
}

#[gpui::test]
fn a_fine_pointer_leaves_a_mouse_press_to_the_active_state_except_on_a_controls_box(
    cx: &mut TestAppContext,
) {
    let (cx, one) = self::one(cx, true, Around::Nothing);
    cx.update(|_, cx| press_feedback::set_primary_pointer_coarse(Some(false), cx));
    cx.run_until_parked();
    mouse_down(cx, 50., 40.);
    assert_eq!(
        one_paint(&one),
        PressPaint::On,
        "a box paints a mouse press at once"
    );
    mouse_up(cx, 50., 40.);
    assert_eq!(one_paint(&one), PressPaint::Fading, "and fades it");
    wait(cx, 175);
    // Under Reduce Motion the fine pointer's release ends at once.
    cx.update(|_, cx| rustryx_primitives::set_reduced_motion(true, cx));
    mouse_down(cx, 50., 40.);
    mouse_up(cx, 50., 40.);
    assert_eq!(one_paint(&one), PressPaint::None);
}

#[gpui::test]
fn a_fine_pointer_paints_no_mouse_press_on_a_plain_surface(cx: &mut TestAppContext) {
    let (cx, one) = self::one(cx, false, Around::Nothing);
    cx.update(|_, cx| press_feedback::set_primary_pointer_coarse(Some(false), cx));
    cx.run_until_parked();
    mouse_down(cx, 50., 40.);
    assert_eq!(one_paint(&one), PressPaint::None);
    mouse_up(cx, 50., 40.);
}

/// Two scrollers side by side (0..300 and 400..700), each holding a surface.
fn two_scrollers(
    cx: &mut TestAppContext,
) -> (&mut VisualTestContext, Rc<RefCell<Vec<PressFeedback>>>) {
    let surfaces: Rc<RefCell<Vec<PressFeedback>>> = Rc::default();
    let s = surfaces.clone();
    let cx = host(cx, move |window, cx| {
        let mut list = Vec::new();
        let mut root = div().size_full();
        for (i, left) in [(0usize, 0.), (1, 400.)] {
            let name = if i == 0 { "left" } else { "right" };
            let handle = crate::gestures::scroller::use_scroll_handle(name, window, cx);
            let feedback = use_press_feedback(
                gpui::ElementId::Name(format!("{name}-row").into()),
                window,
                cx,
            );
            list.push(feedback.clone());
            root = root.child(
                div()
                    .id(name)
                    .absolute()
                    .left(px(left))
                    .top_0()
                    .w(px(300.))
                    .h(px(200.))
                    .overflow_y_scroll()
                    .track_scroll(&handle)
                    .child(crate::gestures::scroller::scroll_begin(
                        name,
                        Some(&handle),
                        (false, true),
                    ))
                    .child(
                        div()
                            .h(px(800.))
                            .child(feedback.surface(div().id(name).h(px(100.)))),
                    )
                    .child(crate::gestures::scroller::scroll_end()),
            );
        }
        *s.borrow_mut() = list;
        root.into_any_element()
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    (cx, surfaces)
}

#[gpui::test]
fn a_scroll_in_another_scroller_neither_brakes_nor_takes_the_press(cx: &mut TestAppContext) {
    let (cx, surfaces) = two_scrollers(cx);
    let right = |s: &Rc<RefCell<Vec<PressFeedback>>>| s.borrow()[1].paint();
    // A finger drives the left scroller; a moment later a finger presses on the right.
    finger_scroll(cx, 100., 50.);
    wait(cx, 20);
    touch(cx, 1, TouchPhase::Started, 500., 50.);
    wait(cx, 150);
    assert_eq!(
        right(&surfaces),
        PressPaint::On,
        "no brake from another scroller"
    );
    // A scroll in the left one leaves the right press alone.
    finger_scroll(cx, 100., 50.);
    assert_eq!(right(&surfaces), PressPaint::On);
    touch(cx, 1, TouchPhase::Ended, 500., 50.);
}

#[gpui::test]
fn does_not_repaint_a_later_row_while_the_finger_stays_down(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, scrolling());
    touch(cx, 1, TouchPhase::Started, 200., 50.);
    wait(cx, 150);
    finger_scroll(cx, 200., 50.);
    touch(cx, 1, TouchPhase::Moved, 60., 40.);
    wait(cx, 300);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    assert_eq!(paint(&case, "button"), PressPaint::None);
    touch(cx, 1, TouchPhase::Ended, 60., 40.);
}

#[gpui::test]
fn a_release_that_matches_no_held_press_leaves_the_held_finger_alone(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    touch(cx, 1, TouchPhase::Started, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::On);
    touch(cx, 7, TouchPhase::Ended, 200., 80.);
    mouse_up(cx, 200., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::On);
    touch(cx, 1, TouchPhase::Ended, 200., 80.);
}

fn key(cx: &mut VisualTestContext, k: &'static str, down: bool) {
    let keystroke = gpui::Keystroke::parse(k).unwrap();
    if down {
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke,
            is_held: false,
            prefer_character_input: false,
        });
    } else {
        cx.simulate_event(gpui::KeyUpEvent { keystroke });
    }
    cx.run_until_parked();
}

#[gpui::test]
fn on_a_touch_first_device_enter_paints_nothing_and_focus_leaving_ends_a_space_press(
    cx: &mut TestAppContext,
) {
    let (cx, case) = mount(cx, fixed());
    cx.update(|_, cx| press_feedback::set_primary_pointer_coarse(Some(true), cx));
    let focus = case.focus.borrow().clone().unwrap();
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.run_until_parked();
    key(cx, "enter", true);
    assert_eq!(
        paint(&case, "button"),
        PressPaint::None,
        "Enter is no press"
    );
    key(cx, "enter", false);
    key(cx, "space", true);
    assert_eq!(paint(&case, "button"), PressPaint::On);
    // Focus leaves before the key comes up: the press ends with its fade.
    cx.update(|window, cx| window.blur(cx));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(paint(&case, "button"), PressPaint::Fading);
    wait(cx, 200);
    assert_eq!(paint(&case, "button"), PressPaint::None);
}

#[gpui::test]
fn a_fingers_press_and_a_mouse_press_keep_their_own_slots(cx: &mut TestAppContext) {
    let (cx, case) = mount(cx, fixed());
    cx.update(|_, cx| press_feedback::set_primary_pointer_coarse(Some(true), cx));
    cx.run_until_parked();
    touch(cx, 1, TouchPhase::Started, 250., 80.);
    assert_eq!(paint(&case, "row"), PressPaint::On);
    mouse_down(cx, 50., 40.);
    assert_eq!(
        paint(&case, "button"),
        PressPaint::On,
        "the mouse paints its own surface"
    );
    assert_eq!(
        paint(&case, "row"),
        PressPaint::On,
        "the finger keeps its own"
    );
    mouse_up(cx, 50., 40.);
    touch(cx, 1, TouchPhase::Ended, 250., 80.);
    wait(cx, 300);
    assert_eq!(paint(&case, "row"), PressPaint::None);
    assert_eq!(paint(&case, "button"), PressPaint::None);
}
