//! Tests for Collapsible, the pressable cards and Token.

use std::cell::{Cell, RefCell};

use gpui::{Modifiers, TestAppContext, point, px};

use super::*;
use crate::button::Button;
use crate::containers::{ClickableCard, SelectableCard};
use crate::segmented::tests::{host, press};
use crate::token::Token;

fn click_at(cx: &mut gpui::VisualTestContext, x: f32, y: f32) {
    cx.simulate_click(point(px(x), px(y)), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn the_trigger_toggles_an_uncontrolled_collapsible(cx: &mut TestAppContext) {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let changes = changes.clone();
        move || {
            let changes = changes.clone();
            div()
                .w(px(300.))
                .child(
                    Collapsible::new("c", "Details")
                        .on_open_change(move |open, _, _| changes.borrow_mut().push(open))
                        .child("Body"),
                )
                .into_any_element()
        }
    });
    click_at(cx, 40., 30.);
    click_at(cx, 40., 30.);
    assert_eq!(
        *changes.borrow(),
        [false, true],
        "open by default, then closed, then open"
    );
}

#[gpui::test]
fn a_single_group_keeps_one_item_open(cx: &mut TestAppContext) {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let changes = changes.clone();
        move || {
            let changes = changes.clone();
            div()
                .w(px(300.))
                .child(
                    CollapsibleGroup::new("g", SelectionMode::Single)
                        .value(["a"])
                        .on_change(move |value, _, _| changes.borrow_mut().push(value))
                        .item(Collapsible::new("a", "A").value("a").child("A body"))
                        .item(Collapsible::new("b", "B").value("b").child("B body")),
                )
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*changes.borrow(), [vec![SharedString::from("b")]]);
}

#[gpui::test]
fn a_cards_nested_button_keeps_its_click(cx: &mut TestAppContext) {
    let (card, button) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let cx = host(cx, {
        let (card, button) = (card.clone(), button.clone());
        move || {
            let (card, button) = (card.clone(), button.clone());
            ClickableCard::new("card", "Open")
                .width(px(300.))
                .on_click(move |_, _, _| card.set(card.get() + 1))
                .child(
                    div().h(px(80.)).child(
                        Button::new("inner", "Inner")
                            .on_click(move |_, _, _| button.set(button.get() + 1)),
                    ),
                )
                .into_any_element()
        }
    });
    // The card's padding is about 16 px: the button sits near its corner.
    click_at(cx, 50., 50.);
    assert_eq!(
        (card.get(), button.get()),
        (0, 1),
        "the button's click stays its own"
    );
    click_at(cx, 250., 90.);
    assert_eq!(card.get(), 1, "the surface activates the card");
}

#[gpui::test]
fn selectable_cards_toggle_with_space_but_not_enter(cx: &mut TestAppContext) {
    let next = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let next = next.clone();
        move || {
            let next = next.clone();
            SelectableCard::new("s", "Pro", false)
                .on_change(move |selected, _, _| next.borrow_mut().push(selected))
                .child("Pro")
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "space");
    assert_eq!(*next.borrow(), [true]);
}

#[gpui::test]
fn removing_a_token_does_not_click_it(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let (a, b) = (log.clone(), log.clone());
            Token::new("t", "Design")
                .on_click(move |_, _, _| a.borrow_mut().push("click"))
                .on_remove(move |_, _, _| b.borrow_mut().push("remove"))
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["remove"]);
}

#[gpui::test]
fn static_tokens_render(cx: &mut TestAppContext) {
    // A token with no click, link or remove has no role (GPUI rejects the
    // generic container role).
    let cx = host(cx, || Token::new("t", "Static").into_any_element());
    cx.run_until_parked();
}
