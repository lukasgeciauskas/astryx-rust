//! A Core field above the on-screen keyboard (AST-099 FR9's scrolling content), upstream
//! `keyboard/keyboardTarget.test.ts`'s `keepFieldAboveKeyboard` cases that GPUI can stand
//! for: the test platform shrinks the visual viewport as a keyboard does, and a touch makes
//! the primary pointer coarse. Left out as DOM-specific: `position: fixed` containers and
//! the app's docked bars (GPUI has no fixed positioning), the browser's own focus reveal and
//! tap route, Firefox's folded reveal and Safari's blur timing.

use std::time::Duration;

use gpui::{
    AnyElement, Bounds, Context, IntoElement, Modifiers, ParentElement, Render, ScrollHandle,
    Styled, TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext, Window, div, point,
    prelude::*, px, size,
};

use super::TextInput;
use crate::gestures::scroller::{scroll_begin, scroll_end};

struct Page {
    handle: ScrollHandle,
    in_target: bool,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let field = div().debug_selector(|| "field".into()).child(
            TextInput::new("name", "Name", "")
                .label_hidden(true)
                .width(px(300.)),
        );
        let field: AnyElement = if self.in_target {
            rustryx_primitives::keyboard::provide_keyboard_target(
                rustryx_primitives::keyboard::KeyboardTargetId(7),
                field,
                cx,
            )
            .into_any_element()
        } else {
            field.into_any_element()
        };
        div().size_full().child(
            div()
                .id("scroller")
                .h(px(600.))
                .overflow_y_scroll()
                .track_scroll(&self.handle)
                .child(scroll_begin("scroller", Some(&self.handle), (false, true)))
                .child(div().h(px(500.)))
                .child(field)
                .child(div().h(px(1500.)))
                .child(scroll_end()),
        )
    }
}

fn page(cx: &mut TestAppContext, in_target: bool) -> (ScrollHandle, &mut VisualTestContext) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
    });
    let handle = ScrollHandle::new();
    let h = handle.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Page {
        handle: h,
        in_target,
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    (handle, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn touch(cx: &mut VisualTestContext, phase: TouchPhase, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: at,
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

/// A finger's tap on the field: the pointer is coarse, and the field has focus.
fn tap_field(cx: &mut VisualTestContext) {
    let bounds = cx.debug_bounds("field").expect("the field");
    let at = point(bounds.left() + px(20.), bounds.center().y);
    touch(cx, TouchPhase::Started, at);
    touch(cx, TouchPhase::Ended, at);
    settle(cx);
}

/// A mouse's click on the field.
fn click_field(cx: &mut VisualTestContext) {
    let bounds = cx.debug_bounds("field").expect("the field");
    let at = point(bounds.left() + px(20.), bounds.center().y);
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

/// The keyboard covers `height` px of the window's bottom.
fn keyboard(cx: &mut VisualTestContext, height: f32) {
    let handle = cx.update(|window, _| window.window_handle());
    let full = cx.update(|window, _| window.viewport_size());
    cx.simulate_window_visual_viewport_change(
        handle,
        Bounds::new(
            point(px(0.), px(0.)),
            size(full.width, full.height - px(height)),
        ),
    );
    settle(cx);
}

fn scroll_top(handle: &ScrollHandle) -> f32 {
    -f32::from(handle.offset().y)
}

/// The keyboard height that puts its top at `top`.
fn keyboard_with_top(cx: &mut VisualTestContext, top: f32) -> f32 {
    let full = cx.update(|window, _| window.viewport_size());
    f32::from(full.height) - top
}

/// Where the focused field's editable text was drawn.
fn editor_bounds(cx: &mut VisualTestContext) -> Bounds<gpui::Pixels> {
    cx.update(rustryx_primitives::keyboard::focused_field)
        .expect("a focused field")
        .bounds
}

fn focused(cx: &mut VisualTestContext) -> bool {
    cx.update(rustryx_primitives::keyboard::is_text_entry_focused)
}

#[gpui::test]
fn fr9_scrolls_its_own_scroller_when_it_sits_in_one(cx: &mut TestAppContext) {
    let (handle, cx) = page(cx, false);
    tap_field(cx);
    assert!(focused(cx), "the tap focused the field");
    let editor = editor_bounds(cx);
    let height = keyboard_with_top(cx, 400.);
    keyboard(cx, height);
    // The field ends FIELD_OFFSET (10 px) above the keyboard's top, in the report itself.
    let expected = f32::from(editor.bottom()) - (400. - 10.);
    assert!(
        (scroll_top(&handle) - expected).abs() < 1.,
        "scrolled {}",
        scroll_top(&handle)
    );
    assert!((f32::from(editor_bounds(cx).bottom()) - 390.).abs() < 1.);
}

#[gpui::test]
fn reveals_at_a_change_of_the_keyboards_height_not_at_a_report_of_the_same_height(
    cx: &mut TestAppContext,
) {
    let (handle, cx) = page(cx, false);
    tap_field(cx);
    let height = keyboard_with_top(cx, 400.);
    keyboard(cx, height);
    // The person scrolls the field away; a pan reports the same height.
    handle.set_offset(point(px(0.), px(0.)));
    settle(cx);
    let window = cx.update(|window, _| window.window_handle());
    let full = cx.update(|window, _| window.viewport_size());
    cx.simulate_window_visual_viewport_change(
        window,
        Bounds::new(
            point(px(0.), px(20.)),
            size(full.width, full.height - px(height)),
        ),
    );
    settle(cx);
    assert_eq!(
        scroll_top(&handle),
        0.,
        "the field stays where the person put it"
    );
}

#[gpui::test]
fn scrolls_the_page_back_to_its_place_before_the_reveal_once_the_keyboard_has_gone_on_the_smooth_scroll(
    cx: &mut TestAppContext,
) {
    let (handle, cx) = page(cx, false);
    tap_field(cx);
    let height = keyboard_with_top(cx, 400.);
    keyboard(cx, height);
    assert!(scroll_top(&handle) > 0.);
    keyboard(cx, 0.);
    // On the smooth scroll: partway first, then home.
    cx.executor().advance_clock(Duration::from_millis(100));
    settle(cx);
    let partway = scroll_top(&handle);
    assert!(partway > 0., "a smooth scroll, not a jump");
    cx.executor().advance_clock(Duration::from_millis(400));
    settle(cx);
    assert_eq!(scroll_top(&handle), 0.);
}

#[gpui::test]
fn scrolls_back_at_once_under_reduce_motion(cx: &mut TestAppContext) {
    cx.update(|cx| rustryx_primitives::set_reduced_motion(true, cx));
    let (handle, cx) = page(cx, false);
    tap_field(cx);
    let height = keyboard_with_top(cx, 400.);
    keyboard(cx, height);
    assert!(scroll_top(&handle) > 0.);
    keyboard(cx, 0.);
    assert_eq!(scroll_top(&handle), 0.);
}

#[gpui::test]
fn keeps_a_scroll_the_person_made_while_the_keyboard_showed_and_puts_nothing_back(
    cx: &mut TestAppContext,
) {
    let (handle, cx) = page(cx, false);
    tap_field(cx);
    let height = keyboard_with_top(cx, 400.);
    keyboard(cx, height);
    handle.set_offset(point(px(0.), px(-300.)));
    settle(cx);
    keyboard(cx, 0.);
    cx.executor().advance_clock(Duration::from_millis(500));
    settle(cx);
    assert_eq!(scroll_top(&handle), 300.);
}

#[gpui::test]
fn moves_nothing_inside_a_keyboard_target_which_keeps_its_own_fields_clear(
    cx: &mut TestAppContext,
) {
    let (handle, cx) = page(cx, true);
    tap_field(cx);
    let height = keyboard_with_top(cx, 400.);
    keyboard(cx, height);
    assert_eq!(scroll_top(&handle), 0.);
}

#[gpui::test]
fn attaches_nothing_under_a_mouse_or_a_trackpad(cx: &mut TestAppContext) {
    let (handle, cx) = page(cx, false);
    click_field(cx);
    assert!(focused(cx));
    let height = keyboard_with_top(cx, 400.);
    keyboard(cx, height);
    assert_eq!(scroll_top(&handle), 0.);
}
