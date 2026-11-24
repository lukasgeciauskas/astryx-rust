//! Choice control tests.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use gpui::{Context, Modifiers, Render, TestAppContext, VisualTestContext, Window, div, point, px};

use super::*;

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
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

#[gpui::test]
fn checkboxes_report_the_next_state(cx: &mut TestAppContext) {
    let next = Rc::new(Cell::new(None));
    let cx = host(cx, {
        let next = next.clone();
        move || {
            let next = next.clone();
            CheckboxInput::new("c", "Terms", false)
                .on_change(move |checked, _, _| next.set(Some(checked)))
                .into_any_element()
        }
    });
    click(cx);
    assert_eq!(next.get(), Some(true));
}

#[gpui::test]
fn an_indeterminate_checkbox_checks(cx: &mut TestAppContext) {
    let next = Rc::new(Cell::new(None));
    let cx = host(cx, {
        let next = next.clone();
        move || {
            let next = next.clone();
            CheckboxInput::new("c", "All", IndicatorState::Indeterminate)
                .on_change(move |checked, _, _| next.set(Some(checked)))
                .into_any_element()
        }
    });
    click(cx);
    assert_eq!(next.get(), Some(true));
}

#[gpui::test]
fn space_toggles_but_enter_does_not(cx: &mut TestAppContext) {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let changes = changes.clone();
        move || {
            let changes = changes.clone();
            Switch::new("s", "Wi-Fi", false)
                .on_change(move |on, _, _| changes.borrow_mut().push(on))
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "enter");
    assert!(changes.borrow().is_empty());
    press(cx, "space");
    assert_eq!(*changes.borrow(), [true]);
}

#[gpui::test]
fn disabled_read_only_and_loading_controls_never_change(cx: &mut TestAppContext) {
    let changes = Rc::new(Cell::new(0));
    let mode = Rc::new(Cell::new(0));
    let cx = host(cx, {
        let (changes, mode) = (changes.clone(), mode.clone());
        move || {
            let changes = changes.clone();
            CheckboxInput::new("c", "Terms", false)
                .disabled(mode.get() == 0)
                .read_only(mode.get() == 1)
                .loading(mode.get() == 2)
                .on_change(move |_, _, _| changes.set(changes.get() + 1))
                .into_any_element()
        }
    });
    for m in 0..3 {
        mode.set(m);
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        click(cx);
    }
    assert_eq!(changes.get(), 0);
}

#[gpui::test]
fn a_change_action_keeps_the_control_busy_until_it_settles(cx: &mut TestAppContext) {
    let runs = Rc::new(Cell::new(0));
    let cx = host(cx, {
        let runs = runs.clone();
        move || {
            let runs = runs.clone();
            Switch::new("s", "Sync", false)
                .change_action(move |_, window, cx| {
                    runs.set(runs.get() + 1);
                    let wait = cx.background_executor().timer(Duration::from_secs(1));
                    window.spawn(cx, async move |_| wait.await)
                })
                .into_any_element()
        }
    });
    click(cx);
    click(cx);
    assert_eq!(runs.get(), 1, "busy while the action runs");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    click(cx);
    assert_eq!(runs.get(), 2);
}

#[test]
fn the_switch_thumb_travels_and_grows() {
    assert_eq!(thumb_at(ChoiceSize::Md, 0.), (4., 16.));
    assert_eq!(thumb_at(ChoiceSize::Md, 1.), (18., 20.));
    assert_eq!(thumb_at(ChoiceSize::Sm, 0.), (3., 14.));
    assert_eq!(thumb_at(ChoiceSize::Sm, 1.), (14., 16.));
}
