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
fn selectable_cards_toggle_with_space_and_with_enter(cx: &mut TestAppContext) {
    // Upstream SelectableCard: a native checkbox reacts to Space, and Enter is wired up
    // as an additional toggle key ("calls onChange with true when Enter is pressed").
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
    // The app keeps its value, so each asks for the selected state.
    assert_eq!(*next.borrow(), [true, true]);
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

// ----------------------------------------------------------------------------
// FR10 (AST-105) and AR4

mod motion {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Duration;

    use gpui::{
        InteractiveElement as _, IntoElement, ParentElement, Pixels, Styled, TestAppContext,
        VisualTestContext, div, px,
    };

    use crate::animated_layout::use_animated_layout;
    use crate::button::Button;
    use crate::collapsible::Collapsible;
    use crate::segmented::tests::{host, press};

    fn settle(cx: &mut VisualTestContext) {
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update(|window, _| window.refresh());
        }
        cx.run_until_parked();
    }

    fn frames(cx: &mut VisualTestContext, ms: u64) {
        for _ in 0..(ms / 16) {
            cx.executor().advance_clock(Duration::from_millis(16));
            settle(cx);
        }
    }

    fn top(cx: &mut VisualTestContext, selector: &'static str) -> Pixels {
        cx.debug_bounds(selector).unwrap().top()
    }

    #[derive(Clone)]
    struct Setup {
        /// Controlled when set.
        open: Option<Rc<Cell<bool>>>,
        handle: bool,
        log: Rc<RefCell<Vec<bool>>>,
        clicks: Rc<RefCell<Vec<&'static str>>>,
    }

    #[derive(IntoElement)]
    struct Page(Setup);

    impl gpui::RenderOnce for Page {
        fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
            let layout = use_animated_layout("area", window, cx);
            let log = self.0.log.clone();
            let (inner, after) = (self.0.clicks.clone(), self.0.clicks.clone());
            let mut c = Collapsible::new("c", "Details")
                .default_open(false)
                .on_open_change(move |open, _, _| log.borrow_mut().push(open))
                .child(
                    div().debug_selector(|| "body".into()).h(px(100.)).child(
                        div().debug_selector(|| "inner".into()).child(
                            Button::new("inner", "Inner")
                                .on_click(move |_, _, _| inner.borrow_mut().push("inner")),
                        ),
                    ),
                );
            if let Some(open) = &self.0.open {
                c = c.open(open.get());
            }
            if self.0.handle {
                c = c.animated_layout(layout.clone());
            }
            let area = div()
                .w(px(300.))
                .flex()
                .flex_col()
                .child(c)
                .child(div().debug_selector(|| "after".into()).h(px(10.)))
                .child(
                    Button::new("after-button", "After")
                        .on_click(move |_, _, _| after.borrow_mut().push("after")),
                );
            layout.container(area, window, cx)
        }
    }

    fn page(cx: &mut TestAppContext, setup: Setup) -> &mut VisualTestContext {
        let cx = host(cx, move || Page(setup.clone()).into_any_element());
        settle(cx);
        cx
    }

    fn setup(open: Option<bool>, handle: bool) -> Setup {
        Setup {
            open: open.map(|o| Rc::new(Cell::new(o))),
            handle,
            log: Rc::default(),
            clicks: Rc::default(),
        }
    }

    fn toggle(cx: &mut VisualTestContext) {
        press(cx, "tab");
        press(cx, "enter");
        settle(cx);
    }

    #[gpui::test]
    fn fr10_with_a_handle_the_content_after_glides_as_it_opens_and_closes(cx: &mut TestAppContext) {
        let cx = page(cx, setup(None, true));
        let base = top(cx, "after");
        toggle(cx);
        frames(cx, 96);
        let mid = top(cx, "after") - base;
        assert!(mid > px(1.) && mid < px(100.), "gliding open: {mid:?}");
        frames(cx, 600);
        let open = top(cx, "after") - base;
        assert!(open > px(100.), "open: {open:?}");

        // Closing holds the content while it plays its exit, then hides it.
        press(cx, "enter");
        settle(cx);
        frames(cx, 64);
        assert!(cx.debug_bounds("body").is_some(), "held while it closes");
        let closing = top(cx, "after") - base;
        assert!(
            closing > px(1.) && closing < open,
            "gliding closed: {closing:?}"
        );
        frames(cx, 600);
        assert_eq!(top(cx, "after"), base);
        assert!(cx.debug_bounds("body").is_none(), "hidden after its exit");
    }

    #[gpui::test]
    fn fr10_without_a_handle_nothing_changes(cx: &mut TestAppContext) {
        let cx = page(cx, setup(None, false));
        let base = top(cx, "after");
        toggle(cx);
        assert!(top(cx, "after") - base > px(100.), "at once");
        press(cx, "enter");
        settle(cx);
        assert_eq!(top(cx, "after"), base);
        assert!(cx.debug_bounds("body").is_none(), "gone at once");
    }

    #[gpui::test]
    fn fr10_an_owner_set_open_glides(cx: &mut TestAppContext) {
        let s = setup(Some(false), true);
        let open = s.open.clone().unwrap();
        let cx = page(cx, s);
        let base = top(cx, "after");
        open.set(true);
        settle(cx);
        frames(cx, 96);
        let mid = top(cx, "after") - base;
        assert!(mid > px(1.) && mid < px(100.), "gliding: {mid:?}");
        frames(cx, 600);
        assert!(top(cx, "after") - base > px(100.));
    }

    #[gpui::test]
    fn fr10_reduced_motion_with_a_mouse_changes_at_once(cx: &mut TestAppContext) {
        cx.update(|cx| astryx_primitives::set_reduced_motion(true, cx));
        let cx = page(cx, setup(None, true));
        let base = top(cx, "after");
        toggle(cx);
        assert!(top(cx, "after") - base > px(100.));
    }

    #[gpui::test]
    fn fr10_closing_content_is_inert_while_it_conceals(cx: &mut TestAppContext) {
        let s = setup(None, true);
        let clicks = s.clicks.clone();
        let cx = page(cx, s);
        toggle(cx);
        frames(cx, 600);
        // Close from the trigger, which keeps focus; the content conceals.
        press(cx, "enter");
        settle(cx);
        frames(cx, 32);
        assert!(cx.debug_bounds("inner").is_some(), "held while it conceals");
        // A press on its button reaches nothing.
        let at = cx.debug_bounds("inner").unwrap().center();
        cx.simulate_click(at, gpui::Modifiers::none());
        settle(cx);
        assert!(clicks.borrow().is_empty(), "{:?}", clicks.borrow());
        // Tab skips it: the next stop is the button after the Collapsible.
        press(cx, "tab");
        press(cx, "enter");
        settle(cx);
        assert_eq!(*clicks.borrow(), ["after"]);
    }

    fn focus_leaves_closing_content(cx: &mut TestAppContext, handle: bool) {
        let s = setup(Some(true), handle);
        let (open, log) = (s.open.clone().unwrap(), s.log.clone());
        let cx = page(cx, s);
        // The trigger, then the button inside the content.
        press(cx, "tab");
        press(cx, "tab");
        // The owner closes it while focus is inside.
        open.set(false);
        settle(cx);
        frames(cx, 600);
        // Enter now activates the trigger: focus moved there.
        press(cx, "enter");
        settle(cx);
        assert_eq!(*log.borrow(), [true]);
    }

    #[gpui::test]
    fn ar4_focus_inside_closing_content_moves_to_the_trigger(cx: &mut TestAppContext) {
        focus_leaves_closing_content(cx, false);
    }

    #[gpui::test]
    fn ar4_focus_inside_content_held_by_its_exit_moves_to_the_trigger(cx: &mut TestAppContext) {
        focus_leaves_closing_content(cx, true);
    }
}
