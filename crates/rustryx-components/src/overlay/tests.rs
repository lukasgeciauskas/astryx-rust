//! Tooltip and InfoTip tests, numbered by the upstream spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, Context, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, point, px,
};

use super::*;
use crate::button::Button;

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_start()
            .pt(px(120.))
            .pl(px(120.))
            .child((self.0)())
    }
}

/// The trigger sits at x 120, y 120 in a 1024 × 768 window.
fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx
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

fn shown(cx: &mut VisualTestContext) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.debug_bounds("t-surface")
}

fn hover(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_mouse_move(point(px(x), px(y)), None, Modifiers::none());
    cx.run_until_parked();
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn button_tip() -> AnyElement {
    Tooltip::new("t", "Save the draft")
        .child(Button::new("b", "Save").w(px(80.)))
        .into_any_element()
}

#[gpui::test]
fn fr3_hover_opens_after_the_delay_and_leaving_closes(cx: &mut TestAppContext) {
    let cx = host(cx, button_tip);
    hover(cx, 140., 130.);
    wait(cx, 150);
    assert!(shown(cx).is_none(), "not before the 200 ms delay");
    wait(cx, 100);
    let surface = shown(cx).expect("open after the delay");
    // Above the trigger, centered on it.
    assert!(surface.bottom() <= px(120.));
    hover(cx, 600., 600.);
    wait(cx, 150);
    assert!(shown(cx).is_none(), "closed after the hover bridge");
}

#[gpui::test]
fn fr4_keyboard_focus_opens_at_once(cx: &mut TestAppContext) {
    let cx = host(cx, button_tip);
    press(cx, "tab");
    assert!(shown(cx).is_some());
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    assert!(shown(cx).is_none(), "focus leaving closes it");
}

#[gpui::test]
fn fr4_pointer_focus_opens_nothing(cx: &mut TestAppContext) {
    let cx = host(cx, button_tip);
    cx.simulate_click(point(px(140.), px(130.)), Modifiers::none());
    cx.run_until_parked();
    // Clicking also hovered it, but the press closed the preview (FR6).
    wait(cx, 300);
    assert!(shown(cx).is_none());
}

#[gpui::test]
fn fr6_escape_closes(cx: &mut TestAppContext) {
    let cx = host(cx, button_tip);
    press(cx, "tab");
    assert!(shown(cx).is_some());
    press(cx, "escape");
    assert!(shown(cx).is_none());
}

#[gpui::test]
fn fr7_a_controlled_tooltip_reports_and_holds(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let log = log.clone();
            Tooltip::new("t", "Held")
                .open(true)
                .on_open_change(move |open, _, _| log.borrow_mut().push(open))
                .child(Button::new("b", "Save"))
                .into_any_element()
        }
    });
    assert!(shown(cx).is_some());
    press(cx, "escape");
    assert!(shown(cx).is_some(), "held open");
    assert_eq!(*log.borrow(), [false]);
}

#[gpui::test]
fn fr7_disabled_tooltips_never_open(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        Tooltip::new("t", "Off")
            .enabled(false)
            .child(Button::new("b", "Save"))
            .into_any_element()
    });
    press(cx, "tab");
    hover(cx, 140., 130.);
    wait(cx, 300);
    assert!(shown(cx).is_none());
}

#[gpui::test]
fn fr9_infotip_pins_on_press(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        InfoTip::new("t", "Editors can change this.").into_any_element()
    });
    cx.simulate_click(point(px(128.), px(128.)), Modifiers::none());
    cx.run_until_parked();
    assert!(shown(cx).is_some(), "a press pins it");
    // The pointer leaving keeps a pinned tip.
    hover(cx, 600., 600.);
    wait(cx, 300);
    assert!(shown(cx).is_some());
    // A press outside closes it.
    cx.simulate_click(point(px(600.), px(600.)), Modifiers::none());
    cx.run_until_parked();
    assert!(shown(cx).is_none());
}

#[gpui::test]
fn fr9_infotip_toggles_from_the_keyboard(cx: &mut TestAppContext) {
    let cx = host(cx, || InfoTip::new("t", "Help").into_any_element());
    press(cx, "tab");
    assert!(shown(cx).is_some(), "focus previews it");
    press(cx, "enter");
    assert!(shown(cx).is_some(), "Enter pins it");
    press(cx, "enter");
    assert!(shown(cx).is_none(), "the same press closes it");
}

#[gpui::test]
fn text_triggers_take_focus(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        Tooltip::new("t", "Coordinated Universal Time")
            .text("UTC")
            .into_any_element()
    });
    press(cx, "tab");
    assert!(shown(cx).is_some());
}

// ----------------------------------------------------------------------------
// HoverCard, numbered by its spec's FR items.

fn card() -> AnyElement {
    HoverCard::new("t")
        .label("Ada Lovelace")
        .content(
            div()
                .flex()
                .gap(px(8.))
                .child(Button::new("follow", "Follow"))
                .child(Button::new("message", "Message")),
        )
        .child(Button::new("b", "Ada").w(px(80.)))
        .into_any_element()
}

#[gpui::test]
fn card_fr4_hover_opens_after_300ms_and_the_pointer_may_cross_onto_it(cx: &mut TestAppContext) {
    let cx = host(cx, card);
    hover(cx, 140., 130.);
    wait(cx, 250);
    assert!(shown(cx).is_none(), "not before the 300 ms delay");
    wait(cx, 100);
    let surface = shown(cx).expect("open after the delay");
    assert!(surface.bottom() <= px(120.), "above the trigger");
    // Crossing onto the card within the hide delay keeps it.
    hover(cx, 140., 119.);
    let inside = surface.center();
    hover(cx, inside.x.into(), inside.y.into());
    wait(cx, 300);
    assert!(shown(cx).is_some(), "the card holds itself open");
    hover(cx, 900., 700.);
    wait(cx, 150);
    assert!(shown(cx).is_some(), "the 200 ms hide delay");
    wait(cx, 100);
    assert!(shown(cx).is_none());
}

#[gpui::test]
fn card_fr6_escape_from_inside_returns_focus_to_the_trigger(cx: &mut TestAppContext) {
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let (follow, trigger) = (log.clone(), log.clone());
            HoverCard::new("t")
                .label("Ada Lovelace")
                .content(
                    Button::new("follow", "Follow")
                        .on_click(move |_, _, _| follow.borrow_mut().push("follow")),
                )
                .child(
                    Button::new("b", "Ada")
                        .w(px(80.))
                        .on_click(move |_, _, _| trigger.borrow_mut().push("trigger")),
                )
                .into_any_element()
        }
    });
    press(cx, "tab");
    assert!(shown(cx).is_some(), "keyboard focus opens it");
    // Into the card, whose tab stops come after the page's.
    press(cx, "tab");
    assert!(shown(cx).is_some(), "focus inside holds it open");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["follow"], "focus was in the card");
    press(cx, "escape");
    assert!(shown(cx).is_none());
    press(cx, "enter");
    assert_eq!(
        *log.borrow(),
        ["follow", "trigger"],
        "focus was back on the trigger"
    );
}
