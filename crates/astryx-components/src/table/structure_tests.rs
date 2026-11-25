//! Stage-5 plugins: tree data and state, grouped rows and row expansion.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gpui::{
    App, Modifiers, MouseButton, SharedString, TestAppContext, VisualTestContext, Window,
    prelude::*,
};

use super::tests::{bounds, host, rows, settle, shown};
use super::*;

fn click(cx: &mut VisualTestContext, selector: &str) {
    let at = bounds(cx, selector).center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

// ---- Tree -------------------------------------------------------------------

#[derive(Clone)]
struct Node {
    id: &'static str,
    children: Vec<Node>,
}

impl TableRecord for Node {
    fn value(&self, key: &str) -> TableValue {
        match key {
            "id" | "name" => self.id.into(),
            _ => TableValue::Null,
        }
    }
}

fn n(id: &'static str, children: Vec<Node>) -> Node {
    Node { id, children }
}

fn forest() -> Vec<Node> {
    vec![
        n("a", vec![n("a1", vec![n("a1x", vec![])]), n("a2", vec![])]),
        n("b", vec![n("b1", vec![])]),
        n("c", vec![]),
    ]
}

type Seen = Rc<RefCell<Vec<&'static str>>>;

fn tree_table(
    data: Vec<Node>,
    seen: Seen,
    configure: impl Fn(TreeStateConfig<Node>) -> TreeStateConfig<Node> + 'static,
    plugin: impl Fn(TableTreeData<Node>) -> TableTreeData<Node> + 'static,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyElement {
    move |window, cx| {
        let state = use_table_tree_state(
            "tree",
            configure(TreeStateConfig::new(data.clone(), "id", |n: &Node| {
                &n.children[..]
            })),
            window,
            cx,
        );
        *seen.borrow_mut() = state.visible_data.iter().map(|n| n.id).collect();
        Table::new("t")
            .data(state.visible_data.clone())
            .columns([
                TableColumn::new("name").header("Name"),
                TableColumn::new("other"),
            ])
            .id_key("id")
            .plugin("tree", plugin(state.plugin))
            .into_any_element()
    }
}

#[gpui::test]
fn the_tree_shows_expanded_subtrees_only(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let cx = host(
        cx,
        600.,
        tree_table(
            forest(),
            seen.clone(),
            |c| c.default_expanded_ids(["a"]),
            |p| p,
        ),
    );
    assert_eq!(*seen.borrow(), ["a", "a1", "a2", "b", "c"]);
    // Children sit indented under their parent.
    assert!(bounds(cx, "tree-expander-a1").left() > bounds(cx, "tree-expander-a").left());
    // A leaf has no expander.
    assert!(!shown(cx, "tree-expander-a2") && !shown(cx, "tree-expander-c"));
    click(cx, "tree-expander-a1");
    assert_eq!(*seen.borrow(), ["a", "a1", "a1x", "a2", "b", "c"]);
    click(cx, "tree-expander-a");
    assert_eq!(
        *seen.borrow(),
        ["a", "b", "c"],
        "collapsing unmounts the subtree"
    );
}

#[gpui::test]
fn expand_all_and_collapse_all_from_the_header(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let cx = host(
        cx,
        600.,
        tree_table(
            forest(),
            seen.clone(),
            |c| c,
            |p| p.expand_all_control(true),
        ),
    );
    assert_eq!(seen.borrow().len(), 3);
    click(cx, "name-tree-expand-all");
    assert_eq!(seen.borrow().len(), 7, "every expandable row");
    click(cx, "name-tree-expand-all");
    assert_eq!(seen.borrow().len(), 3);
}

#[gpui::test]
fn flat_data_renders_as_without_the_plugin(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let flat = vec![n("x", vec![]), n("y", vec![])];
    let cx = host(
        cx,
        600.,
        tree_table(flat, seen, |c| c, |p| p.expand_all_control(true)),
    );
    assert!(!shown(cx, "tree-expander-x"));
    assert!(!shown(cx, "name-tree-expand-all"));
}

#[gpui::test]
fn a_row_click_toggles_an_expandable_row(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let cx = host(
        cx,
        600.,
        tree_table(
            forest(),
            seen.clone(),
            |c| c,
            |p| p.row_click_expansion(true),
        ),
    );
    click(cx, "t-cell-sb-other");
    assert_eq!(*seen.borrow(), ["a", "b", "b1", "c"]);
    // A leaf stays inert.
    click(cx, "t-cell-sc-other");
    assert_eq!(seen.borrow().len(), 4);
}

#[gpui::test]
fn the_tree_column_follows_its_key(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let cx = host(
        cx,
        600.,
        tree_table(forest(), seen, |c| c.tree_column_key("other"), |p| p),
    );
    assert!(bounds(cx, "tree-expander-a").left() >= bounds(cx, "t-header-other").left());
}

#[gpui::test]
fn cycles_are_skipped_and_siblings_sorted(cx: &mut TestAppContext) {
    let seen = Seen::default();
    // "a" lists itself as its own child.
    let cyclic = vec![n("b", vec![]), n("a", vec![n("a", vec![]), n("z", vec![])])];
    let _cx = host(
        cx,
        600.,
        tree_table(
            cyclic,
            seen.clone(),
            |c| {
                c.default_expanded_ids(["a"])
                    .sort_siblings(|mut v: Vec<Node>| {
                        v.sort_by_key(|n| n.id);
                        v
                    })
            },
            |p| p,
        ),
    );
    assert_eq!(*seen.borrow(), ["a", "z", "b"]);
}

#[gpui::test]
fn a_controlled_tree_reports_and_follows(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let reported: Rc<RefCell<Vec<usize>>> = Rc::default();
    let r = reported.clone();
    let cx = host(
        cx,
        600.,
        tree_table(
            forest(),
            seen.clone(),
            move |c| {
                let r = r.clone();
                c.expanded_ids(["b".into()])
                    .on_expanded_ids_change(move |ids, _, _| r.borrow_mut().push(ids.len()))
            },
            |p| p,
        ),
    );
    assert_eq!(*seen.borrow(), ["a", "b", "b1", "c"]);
    click(cx, "tree-expander-a");
    assert_eq!(*reported.borrow(), [2]);
    // The caller did not store it: the rows follow the caller's set.
    assert_eq!(*seen.borrow(), ["a", "b", "b1", "c"]);
}

// ---- Grouped rows -------------------------------------------------------------

fn grouped_table(order: Vec<&'static str>) -> impl Fn(&mut Window, &mut App) -> gpui::AnyElement {
    move |window, cx| {
        let collapsed =
            window.use_keyed_state("collapsed", cx, |_, _| HashSet::<SharedString>::new());
        let current = collapsed.read(cx).clone();
        let data = vec![
            Record::new()
                .with("id", "a")
                .with("team", "x")
                .with("name", "Ada"),
            Record::new()
                .with("id", "b")
                .with("team", "y")
                .with("name", "Bea"),
            Record::new()
                .with("id", "c")
                .with("team", "x")
                .with("name", "Cy"),
        ];
        let grouped = use_table_grouped_rows(
            GroupedRowsConfig::new(
                data,
                |r: &Record| r.value("team").display_text(),
                current,
                move |key, window, cx| {
                    collapsed.update(cx, |s, _| {
                        if !s.remove(&key) {
                            s.insert(key);
                        }
                    });
                    window.refresh();
                },
            )
            .row_key(|r: &Record| r.value("id").display_text())
            .group_order(order.clone()),
        );
        Table::new("t")
            .data(grouped.data.clone())
            .columns([TableColumn::new("name")])
            .id_key_with(grouped.id_key)
            .plugin("grouped", grouped.plugin)
            .into_any_element()
    }
}

fn top(cx: &mut VisualTestContext, s: &str) -> gpui::Pixels {
    bounds(cx, s).top()
}

#[gpui::test]
fn groups_head_their_rows_in_first_seen_order(cx: &mut TestAppContext) {
    let cx = host(cx, 600., grouped_table(vec![]));
    let order = [
        top(cx, "t-row-gx"),
        top(cx, "t-row-sa"),
        top(cx, "t-row-sc"),
        top(cx, "t-row-gy"),
        top(cx, "t-row-sb"),
    ];
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    // The header spans the table.
    assert_eq!(
        bounds(cx, "t-row-gx").size.width,
        bounds(cx, "t-row-sa").size.width
    );
}

#[gpui::test]
fn group_order_pins_groups_first(cx: &mut TestAppContext) {
    let cx = host(cx, 600., grouped_table(vec!["y"]));
    assert!(top(cx, "t-row-gy") < top(cx, "t-row-gx"));
}

#[gpui::test]
fn a_header_collapses_and_expands_its_group(cx: &mut TestAppContext) {
    let cx = host(cx, 600., grouped_table(vec![]));
    click(cx, "t-row-gx");
    assert!(!shown(cx, "t-row-sa") && !shown(cx, "t-row-sc"));
    assert!(shown(cx, "t-row-gx") && shown(cx, "t-row-sb"));
    // The chevron toggles once (it stops the row's click).
    click(cx, "group-x-toggle");
    assert!(shown(cx, "t-row-sa"));
}

#[gpui::test]
fn all_groups_collapsed_is_not_empty(cx: &mut TestAppContext) {
    let cx = host(cx, 600., grouped_table(vec![]));
    click(cx, "t-row-gx");
    click(cx, "t-row-gy");
    assert!(shown(cx, "t-row-gx"));
    assert!(!shown(cx, "t-empty"), "headers still show");
}

// ---- Row expansion ------------------------------------------------------------

fn expansion_table(row_click: bool) -> impl Fn(&mut Window, &mut App) -> gpui::AnyElement {
    move |window, cx| {
        let open = window.use_keyed_state("open", cx, |_, _| HashSet::<SharedString>::new());
        let current = open.read(cx).clone();
        Table::new("t")
            .data(rows())
            .columns([TableColumn::new("name"), TableColumn::new("age")])
            .id_key("id")
            .plugin(
                "expansion",
                TableRowExpansion::new(
                    current,
                    move |key, window, cx| {
                        open.update(cx, |s, _| {
                            if !s.remove(&key) {
                                s.insert(key);
                            }
                        });
                        window.refresh();
                    },
                    |r: &Record| r.value("id").display_text(),
                    |r, _, _| {
                        gpui::div()
                            .child(format!("About {}", r.value("name").display_text()))
                            .into_any_element()
                    },
                )
                .is_item_expandable(|r: &Record| r.value("id").display_text() != "c")
                .row_click_expansion(row_click),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn the_chevron_opens_a_panel_below_its_row(cx: &mut TestAppContext) {
    let cx = host(cx, 600., expansion_table(false));
    let column = bounds(cx, &format!("t-header-{EXPANSION_COLUMN_KEY}"));
    assert_eq!(f32::from(column.size.width), 40.);
    assert!(column.left() < bounds(cx, "t-header-name").left());
    assert!(
        !shown(cx, "expansion-c"),
        "a row that cannot expand has no chevron"
    );
    click(cx, "expansion-a");
    let panel = bounds(cx, "t-panel-a");
    assert!(panel.top() >= bounds(cx, "t-row-sa").bottom() - gpui::px(1.));
    assert!(panel.bottom() <= bounds(cx, "t-row-sb").top() + gpui::px(1.));
    // The panel's content lines up with the first real column.
    click(cx, "expansion-a");
    assert!(!shown(cx, "t-panel-a"));
}

#[gpui::test]
fn the_rows_menu_toggles_it_too(cx: &mut TestAppContext) {
    let cx = host(cx, 600., expansion_table(false));
    let at = bounds(cx, "t-cell-sb-name").center();
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    settle(cx);
    click(cx, "t-cell-sb-name-menu-list-row-expansion-toggle");
    assert!(shown(cx, "t-panel-b"));
}

#[gpui::test]
fn a_row_click_expands_when_asked(cx: &mut TestAppContext) {
    let cx = host(cx, 600., expansion_table(true));
    click(cx, "t-cell-sb-age");
    assert!(shown(cx, "t-panel-b"));
    click(cx, "t-cell-sc-age");
    assert!(!shown(cx, "t-panel-c"));
}
