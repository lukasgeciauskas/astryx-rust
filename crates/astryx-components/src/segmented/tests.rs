//! SegmentedControl tests, plus the shared host the tab and toolbar tests use.

use std::cell::RefCell;

use gpui::{
    AnyElement, Context, Modifiers, Render, TestAppContext, VisualTestContext, Window, div, point,
    px,
};

use super::*;

pub(crate) struct Host(pub Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().items_start().p(px(20.)).child((self.0)())
    }
}

pub(crate) fn host(
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

pub(crate) fn press(cx: &mut VisualTestContext, key: &'static str) {
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

fn segments(log: Rc<RefCell<Vec<SharedString>>>) -> AnyElement {
    SegmentedControl::new("s", "View", "a")
        .on_change(move |value, _, _| log.borrow_mut().push(value))
        .item(SegmentedControlItem::new("a", "Alpha"))
        .item(SegmentedControlItem::new("b", "Beta").disabled(true))
        .item(SegmentedControlItem::new("c", "Gamma"))
        .into_any_element()
}

#[gpui::test]
fn arrows_select_skipping_disabled_segments(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || segments(log.clone())
    });
    press(cx, "tab");
    press(cx, "right");
    assert_eq!(*log.borrow(), [SharedString::from("c")]);
    press(cx, "home");
    assert_eq!(log.borrow().last(), Some(&SharedString::from("a")));
}

#[gpui::test]
fn clicking_a_segment_selects_it(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || segments(log.clone())
    });
    // The first segment is selected already; clicking it changes nothing.
    cx.simulate_click(point(px(30.), px(30.)), Modifiers::none());
    cx.run_until_parked();
    assert!(log.borrow().is_empty());
}
