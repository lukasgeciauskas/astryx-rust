//! InputGroup tests, numbered by the upstream spec's FR items.

use std::cell::Cell;

use super::*;
use crate::button::Button;

struct GroupHost(Rc<dyn Fn() -> InputGroup>);

impl Render for GroupHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_start()
            .p(px(20.))
            .child((self.0)().label_hidden(true).w(px(300.)))
    }
}

/// A 300px group with a hidden label: its row spans y 20..52; a text cell
/// leads, the input follows, and an 80px Button addon ends it.
fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> InputGroup + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| GroupHost(Rc::new(build)));
    cx.run_until_parked();
    cx
}

fn group(probe: &Probe, disabled: bool, pressed: Rc<Cell<bool>>) -> InputGroup {
    InputGroup::new("g", "Website")
        .disabled(disabled)
        .child(InputGroupText::new("https://"))
        .child(TextInput::new("t", "Subdomain", probe.value()).on_change(probe.store()))
        .child(
            Button::new("go", "Go")
                .w(px(80.))
                .on_click(move |_, _, _| pressed.set(true)),
        )
}

#[gpui::test]
fn grouped_inputs_and_addons_work(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let pressed = Rc::new(Cell::new(false));
    let cx = host(cx, {
        let (probe, pressed) = (probe.clone(), pressed.clone());
        move || group(&probe, false, pressed.clone())
    });
    cx.simulate_click(point(px(150.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    type_text(cx, "astryx");
    assert_eq!(probe.value(), "astryx");
    // The addon fills the row's end.
    cx.simulate_click(point(px(300.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    assert!(pressed.get());
}

#[gpui::test]
fn fr6_a_disabled_group_disables_its_members(cx: &mut TestAppContext) {
    let probe = Probe::new("fixed");
    let pressed = Rc::new(Cell::new(false));
    let cx = host(cx, {
        let (probe, pressed) = (probe.clone(), pressed.clone());
        move || group(&probe, true, pressed.clone())
    });
    cx.simulate_click(point(px(150.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    type_text(cx, "x");
    press(cx, "backspace");
    cx.simulate_click(point(px(300.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    assert!(probe.log().is_empty());
    assert!(!pressed.get());
}

#[gpui::test]
fn fr6_an_enabled_group_never_re_enables_a_disabled_input(cx: &mut TestAppContext) {
    let probe = Probe::new("fixed");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            InputGroup::new("g", "Website")
                .child(InputGroupText::new("https://"))
                .child(
                    TextInput::new("t", "Subdomain", probe.value())
                        .disabled(true)
                        .on_change(probe.store()),
                )
        }
    });
    cx.simulate_click(point(px(150.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    type_text(cx, "x");
    assert!(probe.log().is_empty());
}
