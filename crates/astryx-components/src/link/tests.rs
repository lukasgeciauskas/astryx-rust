//! Link tests, named after the spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_primitives::{LinkHandler, link_provider};
use gpui::{Context, Modifiers, Render, TestAppContext, VisualTestContext, Window, div, point, px};

use super::*;

type Log = Rc<RefCell<Vec<String>>>;

struct Host(Rc<dyn Fn() -> Link>, Log);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = self.1.clone();
        let handler = LinkHandler::new(move |navigation, _, _| {
            log.borrow_mut().push(format!(
                "open {} {}",
                navigation.href,
                navigation.target.as_deref().unwrap_or("-")
            ));
        });
        div()
            .flex()
            .items_start()
            .p(px(20.))
            .child(link_provider(handler, (self.0)()))
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn(Log) -> Link + 'static,
) -> (&mut VisualTestContext, Log) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let log: Log = Rc::default();
    let view_log = log.clone();
    let (_, cx) = cx.add_window_view(move |_, _| {
        let build_log = view_log.clone();
        Host(Rc::new(move || build(build_log.clone())), view_log.clone())
    });
    cx.run_until_parked();
    (cx, log)
}

fn click(cx: &mut VisualTestContext) {
    cx.simulate_click(point(px(25.), px(28.)), Modifiers::none());
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
fn fr1_on_click_runs_before_navigation(cx: &mut TestAppContext) {
    let (cx, log) = host(cx, |log| {
        Link::new("l", "Docs")
            .href("https://example.com/docs")
            .on_click(move |_, _, _| log.borrow_mut().push("click".into()))
    });
    click(cx);
    assert_eq!(*log.borrow(), ["click", "open https://example.com/docs -"]);
}

#[gpui::test]
fn fr2_external_links_open_in_a_new_tab(cx: &mut TestAppContext) {
    let (cx, log) = host(cx, |_| {
        Link::new("l", "GitHub")
            .href("https://github.com")
            .external(true)
    });
    click(cx);
    assert_eq!(*log.borrow(), ["open https://github.com _blank"]);
}

#[gpui::test]
fn fr4_rejected_destinations_never_navigate(cx: &mut TestAppContext) {
    let (cx, log) = host(cx, |_| Link::new("l", "Run").href("javascript:alert(1)"));
    click(cx);
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn fr5_disabled_links_never_navigate_or_click(cx: &mut TestAppContext) {
    let (cx, log) = host(cx, |log| {
        Link::new("l", "Docs")
            .href("https://example.com")
            .disabled(true)
            .on_click(move |_, _, _| log.borrow_mut().push("click".into()))
    });
    click(cx);
    press(cx, "tab");
    press(cx, "enter");
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn anchors_take_enter_and_button_links_take_space_too(cx: &mut TestAppContext) {
    let (cx, log) = host(cx, |_| Link::new("l", "Docs").href("https://example.com"));
    press(cx, "tab");
    press(cx, "space");
    assert!(log.borrow().is_empty(), "space does not follow a link");
    press(cx, "enter");
    assert_eq!(log.borrow().len(), 1);
}

#[gpui::test]
fn button_links_take_space(cx: &mut TestAppContext) {
    let (cx, log) = host(cx, |log| {
        Link::new("l", "Details").on_click(move |_, _, _| log.borrow_mut().push("click".into()))
    });
    press(cx, "tab");
    press(cx, "space");
    assert_eq!(*log.borrow(), ["click"]);
}

#[test]
fn fr1_forms() {
    assert_eq!(Link::new("a", "A").href("/x").form(), LinkForm::Anchor);
    assert_eq!(
        Link::new("a", "A").href("/x").disabled(true).form(),
        LinkForm::DisabledAnchor
    );
    assert_eq!(Link::new("a", "A").form(), LinkForm::Button);
}
