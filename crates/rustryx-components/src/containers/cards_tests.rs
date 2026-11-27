//! Upstream `SelectableCard.test.tsx` (the card's own checkbox) and
//! `SelectableCardRadio.test.tsx` (a card a group renders as a radio through the
//! internal `SelectableCardInputContext`).

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    Context, FocusHandle, Modifiers, Render, TestAppContext, VisualTestContext, div, point, px,
};

use super::*;

type Log = Rc<RefCell<Vec<bool>>>;

/// One card, as a checkbox or (with a group input) as a radio.
struct Host {
    selected: bool,
    disabled: bool,
    radio: Option<FocusHandle>,
    log: Log,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = self.log.clone();
        let card = SelectableCard::new("card", "Test", self.selected)
            .disabled(self.disabled)
            .on_change(move |next, _, _| log.borrow_mut().push(next))
            .width(px(200.))
            .child(div().h(px(60.)).child("Content"));
        let card = match &self.radio {
            Some(focus) => provide_selectable_card_input(
                SelectableCardInput {
                    input_type: SelectableCardInputType::Radio,
                    name: "plan".into(),
                    value: "basic".into(),
                    focus: Some(focus.clone()),
                },
                card,
            )
            .into_any_element(),
            None => card.into_any_element(),
        };
        div().p(px(20.)).child(card)
    }
}

fn mount(
    cx: &mut TestAppContext,
    selected: bool,
    disabled: bool,
    radio: bool,
) -> (&mut VisualTestContext, Log, Option<FocusHandle>) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let log = Log::default();
    let focus = radio.then(|| cx.update(|cx| cx.focus_handle().tab_stop(true)));
    let (_, cx) = cx.add_window_view({
        let (log, focus) = (log.clone(), focus.clone());
        move |_, _| Host {
            selected,
            disabled,
            radio: focus,
            log,
        }
    });
    cx.run_until_parked();
    (cx, log, focus)
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

fn click_surface(cx: &mut VisualTestContext) {
    cx.simulate_click(point(px(60.), px(50.)), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn a_click_on_the_surface_of_an_unselected_card_selects_it(cx: &mut TestAppContext) {
    let (cx, log, _) = mount(cx, false, false, false);
    click_surface(cx);
    assert_eq!(*log.borrow(), [true]);
}

#[gpui::test]
fn a_click_on_the_surface_of_a_selected_card_deselects_it(cx: &mut TestAppContext) {
    let (cx, log, _) = mount(cx, true, false, false);
    click_surface(cx);
    assert_eq!(*log.borrow(), [false]);
}

#[gpui::test]
fn a_disabled_card_ignores_clicks_enter_and_space(cx: &mut TestAppContext) {
    let (cx, log, _) = mount(cx, false, true, false);
    click_surface(cx);
    // It keeps its tab stop.
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "space");
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn enter_toggles_the_checkbox_as_a_second_toggle_key(cx: &mut TestAppContext) {
    let (cx, log, _) = mount(cx, false, false, false);
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), [true]);
    let (cx, log, _) = mount(&mut cx.cx, true, false, false);
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), [false]);
}

#[gpui::test]
fn space_toggles_exactly_once(cx: &mut TestAppContext) {
    let (cx, log, _) = mount(cx, false, false, false);
    press(cx, "tab");
    press(cx, "space");
    assert_eq!(*log.borrow(), [true]);
}

#[gpui::test]
fn a_group_radio_takes_the_groups_focus_and_selects_on_click(cx: &mut TestAppContext) {
    let (cx, log, focus) = mount(cx, false, false, true);
    let focus = focus.unwrap();
    press(cx, "tab");
    assert!(
        cx.update(|window, _| focus.is_focused(window)),
        "the group's focus handle is the card's"
    );
    click_surface(cx);
    assert_eq!(*log.borrow(), [true]);
}

#[gpui::test]
fn a_group_radio_selects_on_enter_and_space_when_unselected(cx: &mut TestAppContext) {
    let (cx, log, _) = mount(cx, false, false, true);
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "space");
    assert_eq!(
        *log.borrow(),
        [true, true],
        "the app keeps its value: each asks again"
    );
}

#[gpui::test]
fn a_group_radio_never_deselects_itself(cx: &mut TestAppContext) {
    // A click or Enter on the selected card does nothing.
    let (cx, log, _) = mount(cx, true, false, true);
    click_surface(cx);
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "space");
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn a_disabled_group_radio_stays_focusable_and_does_not_select(cx: &mut TestAppContext) {
    let (cx, log, focus) = mount(cx, false, true, true);
    let focus = focus.unwrap();
    click_surface(cx);
    press(cx, "tab");
    assert!(cx.update(|window, _| focus.is_focused(window)));
    press(cx, "enter");
    assert!(log.borrow().is_empty());
}
