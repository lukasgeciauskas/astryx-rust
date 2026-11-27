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
        TreeListItemData::new("group", "Group")
            .label_content(|_, _| {
                div()
                    .debug_selector(|| "group-control".into())
                    .child(Button::new("group-button", "Pick"))
                    .into_any_element()
            })
            .children([TreeListItemData::new("member", "Member")]),
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
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let log = Log::default();
    let opened = log.clone();
    cx.update(|cx| {
        rustryx_primitives::set_link_handler(
            rustryx_primitives::LinkHandler::new(move |navigation, _, _| {
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
    press(cx, "end"); // Group
    press(cx, "up"); // Site
    press(cx, "up"); // Blog
    press(cx, "enter");
    assert_eq!(log_of(&log).last().unwrap(), "click blog");
    // Typeahead finds a label.
    press(cx, "s");
    press(cx, "enter");
    assert_eq!(
        log_of(&log).last().unwrap(),
        "open https://example.com None"
    );
    // Tab leaves the rows: one stop among them (the Group row's button,
    // a control of its own, is the next stop).
    press(cx, "tab");
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

#[gpui::test]
fn fr7_a_press_on_a_control_in_a_row_is_the_control_s(cx: &mut TestAppContext) {
    let (cx, _) = host(cx);
    assert!(!shown(cx, "t-row-member"));
    settle(cx);
    // On the button, at the start of its full-width wrapper.
    let b = cx.debug_bounds("group-control").unwrap();
    let at = gpui::point(b.left() + px(12.), b.center().y);
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
    assert!(
        !shown(cx, "t-row-member"),
        "the control's press does not toggle the row"
    );
    click(cx, "t-row-group");
    assert!(
        shown(cx, "t-row-member"),
        "a press elsewhere in the row does"
    );
}

// ---- Motion (FR8, FR9, with AR2) ------------------------------------------------

mod motion {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use gpui::{
        Context, InteractiveElement, IntoElement, Modifiers, ParentElement, Pixels, Render, Styled,
        TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext, Window, div, px,
    };

    use super::{Log, settle, tree};
    use crate::animated_layout::use_animated_layout;
    use crate::tree_list::TreeList;

    struct Page {
        log: Log,
        handle: bool,
        docs_open: Rc<Cell<bool>>,
    }

    impl Render for Page {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let layout = use_animated_layout("area", window, cx);
            let mut items = tree(&self.log);
            items[0] = items[0].clone().expanded(self.docs_open.get());
            let mut list = TreeList::new("t", items).w(px(300.));
            if self.handle {
                list = list.animated_layout(layout.clone());
            }
            let area = div()
                .flex()
                .flex_col()
                .p(px(40.))
                .child(list)
                .child(div().debug_selector(|| "after".into()).h(px(10.)));
            layout.container(area, window, cx)
        }
    }

    fn page(cx: &mut TestAppContext, handle: bool) -> &mut VisualTestContext {
        page_with(cx, handle, Log::default(), Rc::new(Cell::new(true)))
    }

    fn page_with(
        cx: &mut TestAppContext,
        handle: bool,
        log: Log,
        docs_open: Rc<Cell<bool>>,
    ) -> &mut VisualTestContext {
        cx.update(|cx| {
            rustryx_theme::init(cx);
            rustryx_primitives::focus::init(cx);
        });
        let (_, cx) = cx.add_window_view(move |_, _| Page {
            log,
            handle,
            docs_open,
        });
        cx.update(|window, _| window.activate_window());
        settle(cx);
        cx
    }

    fn frames(cx: &mut VisualTestContext, ms: u64) {
        for _ in 0..(ms / 16) {
            cx.executor().advance_clock(Duration::from_millis(16));
            settle(cx);
        }
    }

    fn top(cx: &mut VisualTestContext, selector: &'static str) -> Pixels {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is not drawn"))
            .top()
    }

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let at = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(at, Modifiers::none());
        settle(cx);
    }

    fn tap(cx: &mut VisualTestContext, selector: &'static str) {
        let at = cx.debug_bounds(selector).unwrap().center();
        for phase in [TouchPhase::Started, TouchPhase::Ended] {
            cx.simulate_event(TouchEvent {
                id: TouchId(1),
                phase,
                position: at,
                ..Default::default()
            });
        }
        settle(cx);
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

    #[gpui::test]
    fn fr9_with_a_handle_expanding_glides_the_rows_and_content_after_it(cx: &mut TestAppContext) {
        let cx = page(cx, true);
        let (row, after) = (top(cx, "t-row-old"), top(cx, "after"));
        click(cx, "t-chevron-api");
        frames(cx, 96);
        let (row_mid, after_mid) = (top(cx, "t-row-old") - row, top(cx, "after") - after);
        frames(cx, 600);
        let open = top(cx, "t-row-old") - row;
        assert!(open > px(20.), "the child row's height: {open:?}");
        assert!(
            row_mid > px(0.5) && row_mid < open,
            "the rows glide: {row_mid:?}"
        );
        assert!(
            after_mid > px(0.5) && after_mid < open,
            "so does what follows: {after_mid:?}"
        );
        assert_eq!(top(cx, "after") - after, open);
    }

    #[gpui::test]
    fn fr9_a_collapsing_group_stays_drawn_but_out_of_the_keys_until_it_is_gone(
        cx: &mut TestAppContext,
    ) {
        let log = Log::default();
        let cx = page_with(cx, true, log.clone(), Rc::new(Cell::new(true)));
        // A press on docs' chevron focuses its row and collapses it.
        click(cx, "t-chevron-docs");
        frames(cx, 64);
        assert!(
            cx.debug_bounds("t-row-guide").is_some(),
            "held while it collapses"
        );
        // Down skips the collapsing rows to blog; Enter runs its click.
        press(cx, "down");
        press(cx, "enter");
        assert_eq!(log.borrow().last().map(String::as_str), Some("click blog"));
        frames(cx, 600);
        assert!(cx.debug_bounds("t-row-guide").is_none(), "gone at rest");
    }

    #[gpui::test]
    fn fr8_under_a_finger_without_a_handle_the_tree_animates_inside_itself(
        cx: &mut TestAppContext,
    ) {
        let cx = page(cx, false);
        let (row, after) = (top(cx, "t-row-old"), top(cx, "after"));
        tap(cx, "t-chevron-api");
        frames(cx, 96);
        let row_mid = top(cx, "t-row-old") - row;
        let after_now = top(cx, "after") - after;
        frames(cx, 600);
        let open = top(cx, "t-row-old") - row;
        assert!(open > px(20.));
        assert!(
            row_mid > px(0.5) && row_mid < open,
            "the rows below glide: {row_mid:?}"
        );
        assert_eq!(
            after_now, open,
            "content after the tree takes its place at once"
        );
        assert_eq!(top(cx, "after") - after, open);
    }

    #[gpui::test]
    fn fr8_under_a_mouse_without_a_handle_the_group_appears_at_once(cx: &mut TestAppContext) {
        let cx = page(cx, false);
        let row = top(cx, "t-row-old");
        click(cx, "t-chevron-api");
        let now = top(cx, "t-row-old") - row;
        assert!(now > px(20.), "at once: {now:?}");
    }

    #[gpui::test]
    fn ar2_focus_in_a_collapsing_group_moves_to_its_row_while_it_exits(cx: &mut TestAppContext) {
        let docs_open = Rc::new(Cell::new(true));
        let cx = page_with(cx, true, Log::default(), docs_open.clone());
        // Focus guide, inside docs' group (typeahead from the tab stop).
        press(cx, "tab");
        press(cx, "g");
        press(cx, "u");
        // The items collapse docs while focus is inside its group.
        docs_open.set(false);
        settle(cx);
        frames(cx, 32);
        assert!(cx.debug_bounds("t-row-guide").is_some(), "still exiting");
        // Focus moved to docs: Enter expands it again.
        press(cx, "enter");
        frames(cx, 600);
        assert!(
            cx.debug_bounds("t-row-guide").is_some(),
            "docs expanded again"
        );
    }
    /// A tree whose only control inside it is a button in guide's label, between a button
    /// before the tree and one after it.
    struct Focusable {
        log: Log,
        handle: bool,
    }

    impl Render for Focusable {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            use crate::button::Button;
            use crate::tree_list::TreeListItemData;
            let layout = use_animated_layout("area", window, cx);
            let (inner, before, after) = (self.log.clone(), self.log.clone(), self.log.clone());
            let items = vec![
                TreeListItemData::new("docs", "Docs")
                    .expanded(true)
                    .children([TreeListItemData::new("guide", "Guide").label_content(
                        move |_, _| {
                            let inner = inner.clone();
                            div()
                                .debug_selector(|| "inner".into())
                                .child(Button::new("inner", "Inner").on_click(move |_, _, _| {
                                    inner.borrow_mut().push("inner".into())
                                }))
                                .into_any_element()
                        },
                    )]),
                TreeListItemData::new("blog", "Blog"),
            ];
            let mut list = TreeList::new("t", items).w(px(300.));
            if self.handle {
                list = list.animated_layout(layout.clone());
            }
            let area = div()
                .flex()
                .flex_col()
                .p(px(40.))
                .child(
                    Button::new("before", "Before")
                        .on_click(move |_, _, _| before.borrow_mut().push("before".into())),
                )
                .child(list)
                .child(
                    Button::new("after", "After")
                        .on_click(move |_, _, _| after.borrow_mut().push("after".into())),
                );
            layout.container(area, window, cx)
        }
    }

    fn focusable(cx: &mut TestAppContext, handle: bool) -> (&mut VisualTestContext, Log) {
        cx.update(|cx| {
            rustryx_theme::init(cx);
            rustryx_primitives::focus::init(cx);
        });
        let log = Log::default();
        let l = log.clone();
        let (_, cx) = cx.add_window_view(move |_, _| Focusable { log: l, handle });
        cx.update(|window, _| window.activate_window());
        settle(cx);
        (cx, log)
    }

    #[gpui::test]
    fn ar1_tab_leaves_the_tree_for_the_next_stop_and_shift_tab_for_the_previous(
        cx: &mut TestAppContext,
    ) {
        let (cx, log) = focusable(cx, false);
        // A press on blog focuses it (its row takes the one tab stop).
        click(cx, "t-row-blog");
        press(cx, "tab");
        press(cx, "enter");
        assert_eq!(log.borrow().last().map(String::as_str), Some("after"));
        // From the first row (Home moves the one stop there), Shift-Tab reaches the
        // button before the tree.
        click(cx, "t-row-blog");
        press(cx, "home");
        press(cx, "shift-tab");
        press(cx, "enter");
        assert_eq!(log.borrow().last().map(String::as_str), Some("before"));
    }

    #[gpui::test]
    fn ar1_a_control_in_a_row_s_label_is_a_tab_stop_of_its_own(cx: &mut TestAppContext) {
        // As in the browser: the roving stop is the row; a caller's control inside it
        // keeps its own stop. From docs, Tab reaches guide's button.
        let (cx, log) = focusable(cx, false);
        click(cx, "t-row-blog");
        press(cx, "home");
        press(cx, "tab");
        press(cx, "enter");
        assert_eq!(log.borrow().last().map(String::as_str), Some("inner"));
    }

    #[gpui::test]
    fn fr9_a_collapsing_group_is_inert(cx: &mut TestAppContext) {
        let (cx, log) = focusable(cx, true);
        // A press on docs' chevron focuses its row and collapses it.
        click(cx, "t-chevron-docs");
        frames(cx, 32);
        let inner = cx.debug_bounds("inner").expect("held while it collapses");
        // A press on the collapsing group's control reaches nothing.
        cx.simulate_click(inner.center(), Modifiers::none());
        settle(cx);
        assert!(log.borrow().is_empty(), "{:?}", log.borrow());
        // Tab skips the group's control: from docs it leaves the tree.
        press(cx, "tab");
        press(cx, "enter");
        assert_eq!(*log.borrow(), ["after"]);
    }

    #[gpui::test]
    fn fr8_the_tree_s_own_clip_leaves_its_block_start_edge_alone(cx: &mut TestAppContext) {
        let cx = page(cx, false);
        let (tree, row) = (
            cx.debug_bounds("t-tree").unwrap().top(),
            top(cx, "t-row-docs"),
        );
        tap(cx, "t-chevron-api");
        frames(cx, 64);
        // Mid-motion the rows keep their places at the top.
        assert_eq!(cx.debug_bounds("t-tree").unwrap().top(), tree);
        assert_eq!(top(cx, "t-row-docs"), row);
    }
}
