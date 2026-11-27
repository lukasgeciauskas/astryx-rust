//! Upstream `hooks/nestedTouchPress.adopters.test.tsx`: a finger on a control inside a
//! container that paints a press paints the control, never the container; a finger on the
//! container's own surface paints the container. The innermost surface takes the press.

use gpui::{
    AnyElement, Bounds, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Styled as _, TestAppContext, TouchPhase, VisualTestContext, div, px,
};

use super::harness::*;
use crate::button::Button;
use crate::containers::{ClickableCard, SelectableCard};
use crate::gestures::PressPaint;
use crate::gestures::press_feedback::painted_surfaces;
use crate::list::ListItem;
use crate::token::Token;

fn inner() -> impl IntoElement {
    // A row that shrinks its one item to the button.
    div().flex().child(
        div()
            .debug_selector(|| "inner".into())
            .child(Button::new("inner", "Inner")),
    )
}

/// Holds a finger at \`at\` past the press onset; the painted surfaces then.
fn finger_paints(cx: &mut VisualTestContext, at: gpui::Point<Pixels>) -> Vec<Bounds<Pixels>> {
    let (x, y) = (f32::from(at.x), f32::from(at.y));
    touch(cx, 1, TouchPhase::Started, x, y);
    wait(cx, 300);
    let painted = cx.update(|window, cx| painted_surfaces(window, cx));
    touch(cx, 1, TouchPhase::Ended, x, y);
    wait(cx, 1000);
    painted
        .into_iter()
        .filter(|(_, p)| *p == PressPaint::On)
        .map(|(b, _)| b)
        .collect()
}

fn area(b: &Bounds<Pixels>) -> f32 {
    f32::from(b.size.width) * f32::from(b.size.height)
}

/// A container drawn at the top left, its inner control somewhere inside.
fn check(cx: &mut TestAppContext, build: impl Fn() -> AnyElement + 'static) {
    let cx = host(cx, move |_, _| {
        div()
            .w(px(360.))
            .p(px(8.))
            .debug_selector(|| "outer".into())
            .child(build())
            .into_any_element()
    });
    let button = cx.debug_bounds("inner").expect("the inner control");
    // A finger on the control paints only the control.
    let painted = finger_paints(cx, button.center());
    assert_eq!(painted.len(), 1, "one surface paints: {painted:?}");
    assert!(
        area(&painted[0]) <= area(&button) + 1.,
        "the control, not the container: {painted:?}"
    );
    // A finger on the container's own surface paints the container.
    let x = if button.left() > px(100.) {
        button.left() - px(40.)
    } else {
        button.right() + px(30.)
    };
    let surface = gpui::point(x, button.center().y);
    let painted = finger_paints(cx, surface);
    assert_eq!(painted.len(), 1, "{painted:?}");
    assert!(
        area(&painted[0]) > area(&button),
        "the container paints its own press"
    );
}

#[gpui::test]
fn a_clickable_card(cx: &mut TestAppContext) {
    check(cx, || {
        ClickableCard::new("card", "Open")
            .on_click(|_, _, _| {})
            .child(div().w(px(300.)).h(px(80.)).child(inner()))
            .into_any_element()
    });
}

#[gpui::test]
fn a_selectable_card(cx: &mut TestAppContext) {
    check(cx, || {
        SelectableCard::new("pick", "Pick", false)
            .on_change(|_, _, _| {})
            .child(div().w(px(300.)).h(px(80.)).child(inner()))
            .into_any_element()
    });
}

#[gpui::test]
fn a_list_row(cx: &mut TestAppContext) {
    check(cx, || {
        div()
            .w(px(340.))
            .child(
                ListItem::new("row", "Row")
                    .on_click(|_, _, _| {})
                    .end_content(inner()),
            )
            .into_any_element()
    });
}

#[gpui::test]
fn the_remove_button_of_a_token(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        div()
            .p(px(8.))
            .child(
                Token::new("tag", "A long token label")
                    .on_click(|_, _, _| {})
                    .on_remove(|_, _, _| {}),
            )
            .into_any_element()
    });
    let token = cx
        .debug_bounds("tag")
        .or_else(|| cx.debug_bounds("tag-token"));
    let _ = token;
    // The remove button sits at the token's end: press near the right edge first.
    let painted_left = finger_paints(cx, gpui::point(px(20.), px(18.)));
    assert_eq!(
        painted_left.len(),
        1,
        "the token paints its own press: {painted_left:?}"
    );
    let token_area = area(&painted_left[0]);
    let right = painted_left[0].right() - px(12.);
    let painted = finger_paints(cx, gpui::point(right, painted_left[0].center().y));
    assert_eq!(painted.len(), 1, "{painted:?}");
    assert!(
        area(&painted[0]) < token_area,
        "the remove button paints, not the token"
    );
}

// --- The chevron of a TreeList row --------------------------------------------

fn tree_row(cx: &mut TestAppContext, own_action: bool) -> (Vec<Bounds<Pixels>>, Bounds<Pixels>) {
    let cx = host(cx, move |_, _| {
        let mut folder = crate::tree_list::TreeListItemData::new("p", "Folder")
            .children([crate::tree_list::TreeListItemData::new("k", "File")]);
        if own_action {
            folder = folder.on_click(|_, _, _| {});
        }
        div()
            .p(px(8.))
            .child(crate::tree_list::TreeList::new("t", vec![folder]).w(px(300.)))
            .into_any_element()
    });
    let chevron = cx.debug_bounds("t-chevron-p").expect("the chevron");
    (finger_paints(cx, chevron.center()), chevron)
}

#[gpui::test]
fn the_chevron_paints_the_row_when_the_rows_surface_toggles(cx: &mut TestAppContext) {
    let (painted, chevron) = tree_row(cx, false);
    assert_eq!(
        painted.len(),
        1,
        "the chevron runs the row's own toggle: {painted:?}"
    );
    assert!(area(&painted[0]) > area(&chevron), "the row paints");
}

#[gpui::test]
fn the_chevron_paints_no_press_when_the_rows_surface_runs_a_different_action(
    cx: &mut TestAppContext,
) {
    let (painted, _) = tree_row(cx, true);
    assert!(painted.is_empty(), "{painted:?}");
}
