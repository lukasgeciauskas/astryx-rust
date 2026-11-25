//! TreeList behavior tests, numbered after the spec's FR and AR items.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    Context, InteractiveElement, IntoElement, Modifiers, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, div, px,
};

use super::*;
use crate::button::Button;

type Log = Rc<RefCell<Vec<String>>>;

struct Host {
    log: Log,
}

fn tree(log: &Log) -> Vec<TreeListItemData> {
    let click = |id: &'static str| {
        let log = log.clone();
        move |_: &gpui::ClickEvent, _: &mut Window, _: &mut gpui::App| {
            log.borrow_mut().push(format!("click {id}"))
        }
    };
    vec![
        TreeListItemData::new("docs", "Docs")
            .expanded(true)
            .children([
                TreeListItemData::new("guide", "Guide").on_click(click("guide")),
                TreeListItemData::new("api", "API").children([TreeListItemData::new(
                    "ref",
                    "Reference",
                )
                .on_click(click("ref"))]),
                TreeListItemData::new("old", "Old")
                    .disabled(true)
                    .on_click(click("old")),
            ]),
        TreeListItemData::new("blog", "Blog")
            .selected(true)
            .on_click(click("blog")),
        TreeListItemData::new("site", "Site").href("https://example.com"),
    ]
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let after = self.log.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .p(px(40.))
            .child(
                div()
                    .debug_selector(|| "before".into())
                    .child(Button::new("before", "Before")),
            )
            .child(
                TreeList::new("t", tree(&self.log))
                    .aria_label("Files")
                    .w(px(300.)),
            )
            .child(
                div().debug_selector(|| "after".into()).child(
                    Button::new("after", "After")
                        .on_click(move |_, _, _| after.borrow_mut().push("after".into())),
                ),
            )
    }
}

fn host(cx: &mut TestAppContext) -> (&mut VisualTestContext, Log) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let log = Log::default();
    let opened = log.clone();
    cx.update(|cx| {
        astryx_primitives::set_link_handler(
            astryx_primitives::LinkHandler::new(move |navigation, _, _| {
                opened.borrow_mut().push(format!(
                    "open {} {:?}",
                    navigation.href,
                    navigation.target.as_deref()
                ));
            }),
            cx,
        )
    });
    let l = log.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Host { log: l });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, log)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .is_some()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    settle(cx);
    let bounds = cx
        .debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .unwrap_or_else(|| panic!("{selector} is not drawn"));
    // Inside the row, away from its chevron.
    let at = gpui::point(bounds.right() - px(20.), bounds.center().y);
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn log_of(log: &Log) -> Vec<String> {
    log.borrow().clone()
}

#[gpui::test]
fn fr1_expanded_branches_show_their_children(cx: &mut TestAppContext) {
    let (cx, _) = host(cx);
    assert!(shown(cx, "t-row-guide"), "docs starts expanded");
    assert!(!shown(cx, "t-row-ref"), "api starts collapsed");
    assert!(shown(cx, "t-chevron-api"));
    assert!(!shown(cx, "t-chevron-guide"), "leaves have no chevron");
}

#[gpui::test]
fn fr7_a_press_runs_the_row_action(cx: &mut TestAppContext) {
    let (cx, log) = host(cx);
    click(cx, "t-row-guide");
    assert_eq!(log_of(&log), ["click guide"], "its on_click");
    click(cx, "t-row-api");
    assert!(shown(cx, "t-row-ref"), "a branch toggles");
    click(cx, "t-row-api");
    assert!(!shown(cx, "t-row-ref"));
    click(cx, "t-row-site");
    assert_eq!(
        log_of(&log).last().unwrap(),
        "open https://example.com None",
        "a link row opens its link"
    );
    click(cx, "t-row-old");
    assert!(
        !log_of(&log).contains(&"click old".to_string()),
        "a disabled row does nothing"
    );
    click(cx, "t-chevron-docs");
    assert!(!shown(cx, "t-row-guide"), "the chevron toggles");
}

#[gpui::test]
fn ar1_one_tab_stop_and_the_tree_keys(cx: &mut TestAppContext) {
    let (cx, log) = host(cx);
    press(cx, "tab"); // Before
    press(cx, "tab"); // the tree's one stop: the selected row
    press(cx, "enter");
    assert_eq!(log_of(&log), ["click blog"]);
    // Up skips the disabled row; Right expands, then enters.
    press(cx, "up");
    press(cx, "right");
    assert!(shown(cx, "t-row-ref"));
    press(cx, "right");
    press(cx, "enter");
    assert_eq!(log_of(&log).last().unwrap(), "click ref");
    // Left climbs to the parent, then collapses it.
    press(cx, "left");
    press(cx, "left");
    assert!(!shown(cx, "t-row-ref"));
    press(cx, "home");
    press(cx, "left");
    assert!(
        !shown(cx, "t-row-guide"),
        "Home reached Docs, Left collapsed it"
    );
    press(cx, "end");
    press(cx, "up");
    press(cx, "enter");
    assert_eq!(log_of(&log).last().unwrap(), "click blog");
    // Typeahead finds a label.
    press(cx, "s");
    press(cx, "enter");
    assert_eq!(
        log_of(&log).last().unwrap(),
        "open https://example.com None"
    );
    // Tab leaves the tree: one stop only.
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(log_of(&log).last().unwrap(), "after");
}

#[gpui::test]
fn ar2_focus_in_a_collapsing_branch_moves_to_its_row(cx: &mut TestAppContext) {
    let (cx, log) = host(cx);
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "home");
    press(cx, "down"); // Guide
    click(cx, "t-chevron-docs");
    assert!(!shown(cx, "t-row-guide"));
    // Focus went to Docs: Down reaches Blog.
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(log_of(&log).last().unwrap(), "click blog");
}

#[gpui::test]
fn fr10_a_theme_disclosure_mark_replaces_the_chevron(cx: &mut TestAppContext) {
    let (cx, _) = host(cx);
    cx.update(|_, cx| {
        crate::indicator::set_theme_indicator(
            crate::indicator::TREE_DISCLOSURE,
            |request, _, _| {
                let name = format!("mark-{}", request.state);
                div()
                    .debug_selector(move || name)
                    .size(request.size)
                    .into_any_element()
            },
            cx,
        )
    });
    assert!(shown(cx, "mark-expanded"), "Docs");
    assert!(shown(cx, "mark-collapsed"), "API");
}
