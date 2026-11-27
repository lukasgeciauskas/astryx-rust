//! A Tall sheet riding the on-screen keyboard: upstream `BottomSheetKeyboard.test.tsx` and
//! `keyboard/useMobileKeyboard.target.test.tsx`, through the visual viewport the test
//! platform shrinks (AST-099 FR3-FR5).

use gpui::{
    Bounds, InteractiveElement as _, IntoElement, ParentElement, Styled, TestAppContext,
    TouchPhase, VisualTestContext, div, point, px, size,
};

use super::harness::*;
use crate::input::TextInput;
use crate::overlay::{BottomSheet, SheetHeight};

fn tall(cx: &mut TestAppContext) -> &mut VisualTestContext {
    let open = flag();
    let flag2 = open.clone();
    let cx = page(cx, move |_, _| {
        BottomSheet::new("s", flag2.get(), "Search")
            .height(SheetHeight::Tall)
            .on_open_change(writer(&flag2))
            .child(
                div()
                    .debug_selector(|| "content".into())
                    .child(div().h(px(760.)))
                    .child(
                        div().debug_selector(|| "field".into()).child(
                            TextInput::new("q", "Query", "")
                                .label_hidden(true)
                                .width(px(240.)),
                        ),
                    )
                    .child(div().h(px(60.))),
            )
            .into_any_element()
    });
    cx.update(|_, cx| rustryx_primitives::editing::init(cx));
    open_sheet(cx, &open);
    cx
}

fn open_sheet(cx: &mut VisualTestContext, open: &std::rc::Rc<std::cell::Cell<bool>>) {
    super::harness::open(cx, open);
}

fn tap_field(cx: &mut VisualTestContext) {
    let bounds = cx.debug_bounds("field").expect("the field");
    let at = point(bounds.left() + px(20.), bounds.center().y);
    touch(cx, 1, TouchPhase::Started, at);
    touch(cx, 1, TouchPhase::Ended, at);
    settle(cx);
}

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
    settle(cx);
}

#[gpui::test]
fn fr3_a_tall_sheet_reveals_its_field_above_the_keyboard_with_the_suggestion_clearance(
    cx: &mut TestAppContext,
) {
    let cx = tall(cx);
    tap_field(cx);
    assert!(cx.update(rustryx_primitives::keyboard::is_text_entry_focused));
    let full = cx.update(|window, _| window.viewport_size());
    let top = f32::from(full.height) - 300.;
    keyboard(cx, 300.);
    // The field's editable text, as upstream reveals the input itself.
    let field = cx
        .update(rustryx_primitives::keyboard::focused_field)
        .unwrap()
        .bounds;
    assert!(
        f32::from(field.bottom()) <= top - 48. + 1.,
        "the field ends {} above the keyboard's top {top}",
        f32::from(field.bottom())
    );
    assert!(f32::from(field.top()) >= 0.);
}

#[gpui::test]
fn fr3_the_body_goes_back_to_its_scroll_once_the_keyboard_has_gone(cx: &mut TestAppContext) {
    let cx = tall(cx);
    let before = cx.debug_bounds("field").unwrap();
    tap_field(cx);
    keyboard(cx, 300.);
    let raised = cx.debug_bounds("field").unwrap();
    assert!(
        raised.top() < before.top(),
        "the body scrolled the field up"
    );
    keyboard(cx, 0.);
    frames(cx, 600);
    let after = cx.debug_bounds("field").unwrap();
    assert!((f32::from(after.top()) - f32::from(before.top())).abs() < 1.);
}

#[gpui::test]
fn a_sheet_that_is_not_tall_leaves_its_field_to_the_fields_own_path(cx: &mut TestAppContext) {
    let open = flag();
    let flag2 = open.clone();
    let cx = page(cx, move |_, _| {
        BottomSheet::new("s", flag2.get(), "Filters")
            .on_open_change(writer(&flag2))
            .child(
                div()
                    .debug_selector(|| "content".into())
                    .child(div().h(px(40.))),
            )
            .into_any_element()
    });
    open_sheet(cx, &open);
    let s = sheet("s");
    assert!(s.open);
    // No keyboard target: nothing registers as inside the sheet's own keyboard path.
    keyboard(cx, 300.);
    assert!(cx.debug_bounds("content").is_some());
}

/// The platform reports a keyboard covering `height` px (natively, iOS and Android).
fn ime(cx: &mut VisualTestContext, height: f32) {
    cx.simulate_insets_change(gpui::WindowInsets {
        ime: gpui::Edges {
            bottom: px(height),
            ..Default::default()
        },
        ..Default::default()
    });
    settle(cx);
    settle(cx);
}

#[gpui::test]
fn fr3_the_platforms_keyboard_inset_reveals_the_field_and_goes_back(cx: &mut TestAppContext) {
    let cx = tall(cx);
    let before = cx.debug_bounds("field").unwrap();
    tap_field(cx);
    let full = cx.update(|window, _| window.viewport_size());
    ime(cx, 300.);
    let field = cx
        .update(rustryx_primitives::keyboard::focused_field)
        .unwrap()
        .bounds;
    assert!(f32::from(field.bottom()) <= f32::from(full.height) - 300. - 48. + 1.);
    ime(cx, 0.);
    frames(cx, 600);
    let after = cx.debug_bounds("field").unwrap();
    assert!((f32::from(after.top()) - f32::from(before.top())).abs() < 1.);
}

#[gpui::test]
fn fr4_dones_blur_parks_focus_on_the_sheet_under_a_finger(cx: &mut TestAppContext) {
    let cx = tall(cx);
    tap_field(cx);
    keyboard(cx, 300.);
    // The keyboard's Done: focus leaves the field for nothing.
    cx.update(|window, cx| window.blur(cx));
    settle(cx);
    assert!(
        cx.update(|window, cx| window.focused(cx).is_some()),
        "focus parked on the sheet"
    );
    assert!(!cx.update(rustryx_primitives::keyboard::is_text_entry_focused));
    // The next tap on the field is a focus move again.
    tap_field(cx);
    assert!(cx.update(rustryx_primitives::keyboard::is_text_entry_focused));
}

#[gpui::test]
fn fr4_a_mouse_keeps_its_own_focus_and_parks_nothing(cx: &mut TestAppContext) {
    let cx = tall(cx);
    let bounds = cx.debug_bounds("field").unwrap();
    cx.simulate_click(
        point(bounds.left() + px(20.), bounds.center().y),
        gpui::Modifiers::none(),
    );
    settle(cx);
    assert!(cx.update(rustryx_primitives::keyboard::is_text_entry_focused));
    cx.update(|window, cx| window.blur(cx));
    settle(cx);
    assert!(cx.update(|window, cx| window.focused(cx).is_none()));
}
