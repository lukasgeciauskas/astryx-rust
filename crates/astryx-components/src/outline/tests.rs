//! Outline tests, numbered after its spec.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::anchor::anchor;
use gpui::{
    InteractiveElement, IntoElement, Modifiers, ParentElement, ScrollDelta, ScrollEvent,
    ScrollHandle, StatefulInteractiveElement, Styled, TestAppContext, TouchPhase,
    VisualTestContext, div, point, px,
};

use super::*;
use crate::segmented::tests::host;
use crate::text::{Heading, HeadingLevel};

type Log = Rc<RefCell<Vec<String>>>;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..(ms / 16).max(1) {
        cx.executor().advance_clock(Duration::from_millis(16));
        settle(cx);
    }
}

fn items() -> Vec<OutlineItem> {
    vec![
        OutlineItem::new("intro", "Introduction", 1),
        OutlineItem::new("setup", "Setup", 2),
        OutlineItem::new("api", "API", 2),
    ]
}

fn section(id: &'static str, title: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(Heading::new(HeadingLevel::H2, title).id(id))
        .child(div().h(px(600.)))
}

/// An article in a scroller beside its Outline.
fn page(
    cx: &mut TestAppContext,
    log: Log,
    build: impl Fn(Outline) -> Outline + 'static,
) -> (&mut VisualTestContext, ScrollHandle) {
    let handle = ScrollHandle::new();
    let cx = host(cx, {
        let handle = handle.clone();
        move || {
            let (a, b, c) = (log.clone(), log.clone(), log.clone());
            let outline = Outline::new("toc", items())
                .scroll_container(handle.clone())
                .on_navigate_start(move |id, _, _| a.borrow_mut().push(format!("start {id}")))
                .on_navigate_end(move |id, _, _| b.borrow_mut().push(format!("end {id}")))
                .on_active_id_change(move |id, _, _| c.borrow_mut().push(format!("active {id}")));
            div()
                .size_full()
                .flex()
                .child(div().w(px(200.)).child(build(outline)))
                .child(
                    div()
                        .id("scroller")
                        .debug_selector(|| "scroller".into())
                        .h(px(400.))
                        .flex_1()
                        .overflow_y_scroll()
                        .track_scroll(&handle)
                        .child(section("intro", "Introduction"))
                        .child(section("setup", "Setup"))
                        .child(section("api", "API")),
                )
                .into_any_element()
        }
    });
    settle(cx);
    (cx, handle)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn top_of(cx: &mut VisualTestContext, id: &'static str) -> gpui::Pixels {
    cx.update(|window, cx| anchor(id, window, cx).unwrap().bounds.top())
}

#[gpui::test]
fn fr1_fr4_a_link_scrolls_its_heading_to_the_top_and_reports_start_and_end(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let (cx, handle) = page(cx, log.clone(), |o| o);
    assert!(cx.debug_bounds("toc-api").is_some());
    click(cx, "toc-api");
    assert_eq!(log.borrow().first().map(String::as_str), Some("start api"));
    wait(cx, 600);
    let port = handle.bounds();
    assert!(
        (top_of(cx, "api") - port.top()).abs() < px(1.),
        "the heading is at the top"
    );
    assert!(log.borrow().contains(&"end api".to_string()));
    assert!(
        log.borrow().contains(&"active api".to_string()),
        "a click marks it current"
    );
}

#[gpui::test]
fn fr4_the_readers_scroll_ends_the_navigation(cx: &mut TestAppContext) {
    let log = Log::default();
    let (cx, _handle) = page(cx, log.clone(), |o| o);
    click(cx, "toc-api");
    wait(cx, 64);
    assert!(!log.borrow().contains(&"end api".to_string()));
    let at = cx.debug_bounds("scroller").unwrap().center();
    cx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(40.))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
    settle(cx);
    wait(cx, 16);
    let ends = log.borrow().iter().filter(|l| *l == "end api").count();
    assert_eq!(ends, 1, "it ends once, at the interruption");
}

#[gpui::test]
fn fr4_without_scroll_on_click_nothing_scrolls_and_it_ends_at_once(cx: &mut TestAppContext) {
    let log = Log::default();
    let (cx, handle) = page(cx, log.clone(), |o| o.scroll_on_click(false));
    click(cx, "toc-api");
    assert_eq!(handle.offset().y, px(0.));
    assert!(log.borrow().contains(&"end api".to_string()));
}

#[gpui::test]
fn fr5_the_current_heading_follows_the_scroll(cx: &mut TestAppContext) {
    let log = Log::default();
    let (cx, _handle) = page(cx, log.clone(), |o| o);
    let at = cx.debug_bounds("scroller").unwrap().center();
    cx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(-700.))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
    settle(cx);
    assert!(
        log.borrow().contains(&"active setup".to_string()),
        "{:?}",
        log.borrow()
    );
}

#[gpui::test]
fn use_outline_from_dom_lists_the_container_headings_with_ids(cx: &mut TestAppContext) {
    let cx = host(cx, move || {
        // The host's render runs each frame, as a component's would.
        HostOutline.into_any_element()
    });
    settle(cx);
    assert!(cx.debug_bounds("toc-intro").is_some());
    assert!(cx.debug_bounds("toc-api").is_some());
    assert!(
        cx.debug_bounds("toc-untitled").is_none(),
        "a heading without an id is omitted"
    );
}

#[derive(IntoElement)]
struct HostOutline;

impl gpui::RenderOnce for HostOutline {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let source = use_outline_from_container("article", window, cx);
        let items = source.items(cx);
        div()
            .flex()
            .child(div().w(px(200.)).child(Outline::new("toc", items)))
            .child(
                source.container(
                    div()
                        .flex()
                        .flex_col()
                        .child(Heading::new(HeadingLevel::H1, "Introduction").id("intro"))
                        .child(Heading::new(HeadingLevel::H2, "Untitled"))
                        .child(Heading::new(HeadingLevel::H2, "API").id("api")),
                ),
            )
    }
}

#[test]
fn outline_from_markdown_matches_markdowns_heading_ids() {
    use super::{ParseOutlineFromMarkdownOptions, parse_outline_from_markdown};
    let items = parse_outline_from_markdown(
        "# Setup\n\n## Setup\n\n> # Quoted\n\n```\n# not a heading\n```\n\n### **Bold** step",
        &ParseOutlineFromMarkdownOptions::default(),
    );
    let got: Vec<_> = items
        .iter()
        .map(|i| (i.id.to_string(), i.label.to_string(), i.level))
        .collect();
    assert_eq!(
        got,
        [
            ("setup".to_string(), "Setup".to_string(), 1),
            ("setup-1".to_string(), "Setup".to_string(), 2),
            ("bold-step".to_string(), "Bold step".to_string(), 3),
        ],
        "root headings only, code fences skipped"
    );
}
