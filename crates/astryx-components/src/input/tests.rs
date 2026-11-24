//! TextInput tests, numbered by the upstream spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, IntoElement, Modifiers, ParentElement, Render, SharedString, Styled,
    TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::*;

type Log = Rc<RefCell<Vec<String>>>;

/// The text a host stores, and a log of what the field reported.
#[derive(Clone, Default)]
struct Probe {
    value: Rc<RefCell<SharedString>>,
    log: Log,
}

impl Probe {
    fn new(value: &str) -> Self {
        Probe {
            value: Rc::new(RefCell::new(value.into())),
            log: Rc::default(),
        }
    }

    fn value(&self) -> SharedString {
        self.value.borrow().clone()
    }

    fn log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }

    /// A controlled field's change handler: store the text, log it, redraw.
    fn store(&self) -> impl Fn(SharedString, &mut Window, &mut App) + 'static {
        let (value, log) = (self.value.clone(), self.log.clone());
        move |next, window, _| {
            log.borrow_mut().push(next.to_string());
            *value.borrow_mut() = next;
            window.refresh();
        }
    }

    /// Logs without storing: the parent keeps its value.
    fn record(&self) -> impl Fn(SharedString, &mut Window, &mut App) + 'static {
        let log = self.log.clone();
        move |next, _, _| log.borrow_mut().push(next.to_string())
    }

    fn note(&self, entry: &str) {
        self.log.borrow_mut().push(entry.to_string());
    }
}

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().items_start().p(px(20.)).child((self.0)())
    }
}

/// A 300px field with a hidden label: its control spans x 20..320 and
/// y 20..52, so its text line is at y 36.
fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> TextInput + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| {
        Host(Rc::new({
            let build = Rc::new(build);
            move || {
                build()
                    .label_hidden(true)
                    .width(px(300.))
                    .into_any_element()
            }
        }))
    });
    cx.run_until_parked();
    cx
}

fn click_at(cx: &mut VisualTestContext, x: f32) {
    cx.simulate_click(point(px(x), px(36.)), Modifiers::none());
    cx.run_until_parked();
}

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
    cx.run_until_parked();
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    cx.run_until_parked();
}

#[gpui::test]
fn fr3_typing_reports_each_change(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || TextInput::new("t", "Name", probe.value()).on_change(probe.store())
    });
    click_at(cx, 100.);
    type_text(cx, "hi");
    assert_eq!(probe.log(), ["h", "hi"]);
    assert_eq!(probe.value(), "hi");
}

#[gpui::test]
fn fr2_the_field_follows_its_value(cx: &mut TestAppContext) {
    // A parent that keeps its value: each edit starts again from it.
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || TextInput::new("t", "Name", probe.value()).on_change(probe.record())
    });
    click_at(cx, 100.);
    type_text(cx, "a");
    type_text(cx, "b");
    assert_eq!(probe.log(), ["a", "b"]);
}

#[gpui::test]
fn fr3_fr4_an_action_shows_its_text_until_it_settles(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            let log = probe.log.clone();
            TextInput::new("t", "Name", probe.value()).change_action(move |next, window, cx| {
                log.borrow_mut().push(next.to_string());
                let wait = cx.background_executor().timer(Duration::from_secs(1));
                window.spawn(cx, async move |_| wait.await)
            })
        }
    });
    click_at(cx, 100.);
    type_text(cx, "a");
    type_text(cx, "b");
    // The second edit started from the proposed "a", not the value "".
    assert_eq!(probe.log(), ["a", "ab"]);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    // Settled: the value "" shows again.
    type_text(cx, "c");
    assert_eq!(probe.log(), ["a", "ab", "c"]);
}

#[gpui::test]
fn fr5_clear_commits_empty_text_and_keeps_focus(cx: &mut TestAppContext) {
    let probe = Probe::new("abc");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            TextInput::new("t", "Search", probe.value())
                .clear(true)
                .on_change(probe.store())
        }
    });
    // The clear button sits at the end of the control.
    click_at(cx, 300.);
    assert_eq!(probe.log(), [""]);
    type_text(cx, "z");
    assert_eq!(probe.value(), "z", "the field has focus after clearing");
}

#[gpui::test]
fn fr5_fr8_read_only_fields_neither_change_nor_clear(cx: &mut TestAppContext) {
    let probe = Probe::new("fixed");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            TextInput::new("t", "Workspace", probe.value())
                .read_only(true)
                .clear(true)
                .on_change(probe.store())
        }
    });
    click_at(cx, 300.);
    click_at(cx, 100.);
    type_text(cx, "x");
    press(cx, "backspace");
    assert!(probe.log().is_empty());
    assert_eq!(probe.value(), "fixed");
}

#[gpui::test]
fn fr6_disabled_fields_stay_focusable_but_inert(cx: &mut TestAppContext) {
    let probe = Probe::new("abc");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            let (enter, key) = (probe.clone(), probe.clone());
            TextInput::new("t", "Seat", probe.value())
                .disabled(true)
                .clear(true)
                .on_change(probe.store())
                .on_enter(move |_, _| enter.note("<enter>"))
                .on_key_down(move |_, _, _| key.note("<key>"))
        }
    });
    press(cx, "tab");
    assert!(
        cx.update(|window, cx| window.focused(cx).is_some()),
        "a disabled field keeps its tab stop"
    );
    type_text(cx, "x");
    press(cx, "enter");
    press(cx, "backspace");
    click_at(cx, 300.);
    assert!(
        probe.log().is_empty(),
        "no changes, Enter or caller key handlers"
    );
    assert_eq!(probe.value(), "abc");
}

#[gpui::test]
fn fr10_enter_and_keys_reach_the_caller(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            let (enter, key) = (probe.clone(), probe.clone());
            TextInput::new("t", "Name", probe.value())
                .on_change(probe.store())
                .on_enter(move |_, _| enter.note("<enter>"))
                .on_key_down(move |event, _, _| key.note(&format!("key:{}", event.keystroke.key)))
        }
    });
    click_at(cx, 100.);
    press(cx, "enter");
    assert_eq!(probe.log(), ["key:enter", "<enter>"]);
}

#[gpui::test]
fn fr12_a_click_on_the_box_focuses_the_field(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || TextInput::new("t", "Name", probe.value()).on_change(probe.store())
    });
    // The left padding, outside the text.
    click_at(cx, 23.);
    type_text(cx, "ok");
    assert_eq!(probe.value(), "ok");
}

#[path = "area_tests.rs"]
mod area;

#[path = "number_tests.rs"]
mod number;

#[path = "group_tests.rs"]
mod group;
