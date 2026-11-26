//! Button family tests, named after the spec's FR and AR items.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{Context, Modifiers, Render, TestAppContext, VisualTestContext, point, px};

use super::toggle::toggled;
use super::*;

/// A window rendering whatever the closure builds.
struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // A flex row, as buttons sit in real layouts (see conventions:
        // inline components stretch in a block parent).
        div().flex().items_start().p(px(20.)).child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.run_until_parked();
    cx
}

fn click(cx: &mut VisualTestContext) {
    cx.simulate_click(point(px(30.), px(30.)), Modifiers::none());
    cx.run_until_parked();
}

/// A full key press: GPUI fires keyboard clicks on key-up, and
/// `simulate_keystrokes` sends only key-down.
fn press(cx: &mut VisualTestContext, key: &'static str) {
    let Ok(keystroke) = gpui::Keystroke::parse(key) else {
        panic!("not a keystroke: {key}")
    };
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
}

fn theme() -> ResolvedTheme {
    ResolvedTheme::new(
        std::sync::Arc::new(astryx_themes::neutral::theme()),
        astryx_theme::Appearance::Light,
        1280.,
        astryx_theme::Pointer::Fine,
    )
}

#[gpui::test]
fn fr6_on_click_runs_first_and_can_cancel_the_action(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cancel = Rc::new(Cell::new(false));
    let cx = host(cx, {
        let (log, cancel) = (log.clone(), cancel.clone());
        move || {
            let (a, b, cancel) = (log.clone(), log.clone(), cancel.clone());
            Button::new("b", "Go")
                .on_click(move |_, window, _| {
                    a.borrow_mut().push("click");
                    if cancel.get() {
                        window.prevent_default();
                    }
                })
                .click_action(move |_, _, _| {
                    b.borrow_mut().push("action");
                    Task::ready(())
                })
                .into_any_element()
        }
    });
    click(cx);
    assert_eq!(*log.borrow(), ["click", "action"]);
    cancel.set(true);
    click(cx);
    assert_eq!(*log.borrow(), ["click", "action", "click"]);
}

#[gpui::test]
fn fr7_a_pending_action_runs_once(cx: &mut TestAppContext) {
    let runs = Rc::new(Cell::new(0));
    let cx = host(cx, {
        let runs = runs.clone();
        move || {
            let runs = runs.clone();
            Button::new("b", "Sync")
                .click_action(move |_, _, cx| {
                    runs.set(runs.get() + 1);
                    let timer = cx.background_executor().timer(Duration::from_secs(5));
                    cx.spawn(async move |_| timer.await)
                })
                .into_any_element()
        }
    });
    click(cx);
    click(cx);
    assert_eq!(runs.get(), 1, "a second press while pending does nothing");
    cx.executor().advance_clock(Duration::from_secs(6));
    cx.run_until_parked();
    click(cx);
    assert_eq!(runs.get(), 2, "it runs again once settled");
}

#[gpui::test]
fn fr7_interruptible_actions_run_again_while_pending(cx: &mut TestAppContext) {
    let runs = Rc::new(Cell::new(0));
    let cx = host(cx, {
        let runs = runs.clone();
        move || {
            let runs = runs.clone();
            Button::new("b", "Search")
                .interruptible(true)
                .click_action(move |_, _, cx| {
                    runs.set(runs.get() + 1);
                    let timer = cx.background_executor().timer(Duration::from_secs(5));
                    cx.spawn(async move |_| timer.await)
                })
                .into_any_element()
        }
    });
    click(cx);
    click(cx);
    assert_eq!(runs.get(), 2);
}

#[gpui::test]
fn fr10_disabled_and_loading_buttons_never_activate(cx: &mut TestAppContext) {
    let clicks = Rc::new(Cell::new(0));
    let loading = Rc::new(Cell::new(false));
    let cx = host(cx, {
        let (clicks, loading) = (clicks.clone(), loading.clone());
        move || {
            let clicks = clicks.clone();
            Button::new("b", "Delete")
                .disabled(!loading.get())
                .loading(loading.get())
                .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
                .into_any_element()
        }
    });
    click(cx);
    loading.set(true);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    click(cx);
    assert_eq!(clicks.get(), 0);
}

#[gpui::test]
fn ar2_tab_then_enter_or_space_activates(cx: &mut TestAppContext) {
    let clicks = Rc::new(Cell::new(0));
    let cx = host(cx, {
        let clicks = clicks.clone();
        move || {
            let clicks = clicks.clone();
            Button::new("b", "Save")
                .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
                .into_any_element()
        }
    });
    press(cx, "enter");
    assert_eq!(clicks.get(), 0, "nothing is focused yet");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(clicks.get(), 1);
    press(cx, "space");
    assert_eq!(clicks.get(), 2);
}

#[gpui::test]
fn toggle_buttons_report_the_next_state(cx: &mut TestAppContext) {
    let next = Rc::new(Cell::new(None));
    let cx = host(cx, {
        let next = next.clone();
        move || {
            let next = next.clone();
            ToggleButton::new("t", "Bold")
                .pressed(true)
                .on_pressed_change(move |pressed, _, _, _| next.set(Some(pressed)))
                .into_any_element()
        }
    });
    click(cx);
    assert_eq!(next.get(), Some(false));
}

#[gpui::test]
fn toggle_groups_own_their_members_state(cx: &mut TestAppContext) {
    let next = Rc::new(RefCell::new(None));
    let cx = host(cx, {
        let next = next.clone();
        move || {
            let next = next.clone();
            ToggleButtonGroup::new("g", "Align", SelectionMode::Single)
                .value(["right"])
                .on_change(move |value, _, _| *next.borrow_mut() = Some(value))
                .child(ToggleButton::new("left", "Left").value("left"))
                .child(ToggleButton::new("right", "Right").value("right"))
                .into_any_element()
        }
    });
    click(cx);
    assert_eq!(*next.borrow(), Some(vec![SharedString::from("left")]));
}

#[test]
fn fr3_icons_follow_the_button_size() {
    assert_eq!(icon_size_for(ElementSize::Sm), IconSize::Sm);
    assert_eq!(icon_size_for(ElementSize::Md), IconSize::Sm);
    assert_eq!(icon_size_for(ElementSize::Lg), IconSize::Md);
}

#[test]
fn icon_only_buttons_are_square() {
    let theme = theme();
    let rem = px(16.);
    let mut el = Button::new("i", "Close").icon_only(true).root(
        &theme,
        ElementSize::Md,
        None,
        astryx_i18n::Direction::Ltr,
        rem,
        None,
    );
    let height = theme.tokens().size.element_md;
    assert_eq!(el.style().size.height, Some(Length::from(height)));
    assert_eq!(el.style().size.width, None, "the width follows the height");
    assert_eq!(el.style().aspect_ratio, Some(1.));
}

#[test]
fn group_members_round_only_the_outer_corners() {
    let theme = theme();
    let rem = px(16.);
    let ltr = astryx_i18n::Direction::Ltr;
    let member = |first, last| GroupMember {
        orientation: Orientation::Horizontal,
        first,
        last,
        disabled: false,
    };
    let radius: gpui::AbsoluteLength = theme.tokens().radius.element;
    let zero: gpui::AbsoluteLength = px(0.).into();
    let corners = |first, last| {
        let mut el = Button::new("a", "A").root(
            &theme,
            ElementSize::Md,
            Some(member(first, last)),
            ltr,
            rem,
            None,
        );
        let c = el.style().corner_radii.clone();
        let divider = el.style().border_widths.left.is_some();
        (c.top_left, c.top_right, divider)
    };
    assert_eq!(corners(true, false), (Some(radius), Some(zero), false));
    assert_eq!(corners(false, false), (Some(zero), Some(zero), true));
    assert_eq!(corners(false, true), (Some(zero), Some(radius), true));
}

#[test]
fn toggle_groups_select_like_upstream() {
    let v = |s: &[&str]| {
        s.iter()
            .map(|s| SharedString::from(s.to_string()))
            .collect::<Vec<_>>()
    };
    let a: SharedString = "a".into();
    assert_eq!(toggled(SelectionMode::Single, &v(&["b"]), &a), v(&["a"]));
    assert_eq!(toggled(SelectionMode::Single, &v(&["a"]), &a), v(&[]));
    assert_eq!(
        toggled(SelectionMode::Multiple, &v(&["b"]), &a),
        v(&["b", "a"])
    );
    assert_eq!(
        toggled(SelectionMode::Multiple, &v(&["a", "b"]), &a),
        v(&["b"])
    );
}

#[test]
fn fr8_pending_actions_reveal_the_spinner_after_the_delay() {
    let delay = Duration::from_millis(310);
    let at = |ms| Some(Duration::from_millis(ms));
    assert!(!spinner_revealed(false, true, false, delay, at(100)));
    assert!(spinner_revealed(false, true, false, delay, at(310)));
    assert!(
        spinner_revealed(true, false, false, delay, None),
        "loading shows at once"
    );
    assert!(
        spinner_revealed(false, true, false, Duration::ZERO, at(0)),
        "reduced motion shows at once"
    );
    assert!(!spinner_revealed(false, false, false, delay, None));
}

/// A block parent, unlike the flex host above.
struct BlockHost(Rc<dyn Fn() -> AnyElement>);

impl Render for BlockHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(20.)).w(px(400.)).child((self.0)())
    }
}

#[gpui::test]
fn buttons_in_a_block_flow_side_by_side(cx: &mut TestAppContext) {
    let clicks = Rc::new(RefCell::new(Vec::new()));
    cx.update(astryx_theme::init);
    let (_, cx) = cx.add_window_view({
        let clicks = clicks.clone();
        move |_, _| {
            let clicks = clicks.clone();
            BlockHost(Rc::new(move || {
                let (a, b) = (clicks.clone(), clicks.clone());
                div()
                    .child(
                        IconButton::new("a", "First", crate::icon::Icon::new("close"))
                            .on_click(move |_, _, _| a.borrow_mut().push("first")),
                    )
                    .child(
                        IconButton::new("b", "Second", crate::icon::Icon::new("close"))
                            .on_click(move |_, _, _| b.borrow_mut().push("second")),
                    )
                    .into_any_element()
            }))
        }
    });
    cx.run_until_parked();
    // Inline boxes (upstream `inline-flex`): the 32 px squares share the first
    // line instead of each stretching across it.
    cx.simulate_click(point(px(68.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*clicks.borrow(), ["second"]);
}
