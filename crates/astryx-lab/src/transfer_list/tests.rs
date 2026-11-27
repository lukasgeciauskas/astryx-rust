//! Upstream `TransferList.test.tsx`, `TransferListSelector.test.tsx` and
//! `TransferList.composition.test.tsx`, the parts a GPUI window can drive.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_components::reorderable::{ItemKey, ReorderableOptions, create_reorderable};
use astryx_primitives::announce::{Politeness, announcement};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, Modifiers, Render, SharedString, TestAppContext,
    VisualTestContext, Window, div, px,
};

use super::*;

fn s(values: &[&str]) -> Vec<SharedString> {
    values.iter().map(|v| SharedString::from(*v)).collect()
}

fn options() -> Vec<TransferListOption> {
    vec![
        TransferListOption::new("name", "Name")
            .description("Primary display name")
            .group("Identity"),
        TransferListOption::new("owner", "Owner")
            .description("Person responsible")
            .group("Identity"),
        TransferListOption::new("status", "Status")
            .description("Workflow state")
            .group("Planning"),
        TransferListOption::new("priority", "Priority")
            .description("Relative urgency")
            .group("Planning"),
        TransferListOption::new("locked", "Locked")
            .transfer_disabled(true)
            .disabled_message("Locked stays where it is.")
            .group("Planning"),
    ]
}

#[test]
fn keeps_support_metadata_searchable_with_groups() {
    let o = options();
    let q = normalize_query("  WORKFLOW ");
    assert_eq!(q, "workflow");
    assert_eq!(
        available_options(&o, &[], &q)
            .iter()
            .map(|o| o.value.as_ref())
            .collect::<Vec<_>>(),
        ["status"]
    );
    // A group name matches too.
    let planning: Vec<_> = available_options(&o, &[], "planning")
        .into_iter()
        .map(|o| o.value)
        .collect();
    assert_eq!(planning, s(&["status", "priority", "locked"]));
    let groups = grouped(&available_options(&o, &[], ""));
    assert_eq!(
        groups
            .iter()
            .map(|(g, items)| (g.as_ref(), items.len()))
            .collect::<Vec<_>>(),
        [("Identity", 2), ("Planning", 3)]
    );
}

#[test]
fn filters_both_panels_with_the_shared_search() {
    let o = options();
    let value = s(&["owner", "status"]);
    let selected: Vec<_> = selected_options(&o, &value, "own")
        .into_iter()
        .map(|o| o.value)
        .collect();
    let available: Vec<_> = available_options(&o, &value, "own")
        .into_iter()
        .map(|o| o.value)
        .collect();
    assert_eq!(selected, s(&["owner"]));
    assert!(available.is_empty());
    // Selected rows keep the value's order; a value without an option is not shown.
    let shown: Vec<_> = selected_options(&o, &s(&["status", "ghost", "name"]), "")
        .into_iter()
        .map(|o| o.value)
        .collect();
    assert_eq!(shown, s(&["status", "name"]));
}

#[test]
fn adds_all_transfer_enabled_values_in_option_order_and_clears_only_removable_ones() {
    let o = options();
    assert_eq!(
        add_all(&o, &s(&["status"])),
        s(&["status", "name", "owner", "priority"])
    );
    assert_eq!(
        clear_selected(&o, &s(&["name", "locked", "ghost"])),
        s(&["locked", "ghost"])
    );
}

#[test]
fn a_staged_move_lands_where_reported() {
    let v = s(&["a", "b", "c", "d"]);
    assert_eq!(move_value(&v, "a", 2), s(&["b", "c", "a", "d"]));
    assert_eq!(move_value(&v, "d", 0), s(&["d", "a", "b", "c"]));
    assert_eq!(move_value(&v, "b", 9), s(&["a", "c", "d", "b"]));
}

#[test]
fn the_catalog_names_actions_after_their_option() {
    let i18n = astryx_i18n::I18n::new("en-US");
    let label = |key: &str| i18n.format(key, &[("label", astryx_i18n::Arg::from("Owner"))]);
    assert_eq!(label("@astryx.transferList.addOption"), "Add Owner");
    assert_eq!(label("@astryx.transferList.removeOption"), "Remove Owner");
    let count = i18n.format(
        "@astryx.transferListSelector.triggerLabel",
        &[("count", astryx_i18n::Arg::Num(3.))],
    );
    assert_eq!(count, "3 selected");
}

// In a window.

type BuildFn = Rc<dyn Fn(&Entity<Vec<SharedString>>, &mut Window, &mut App) -> AnyElement>;

struct Host {
    build: BuildFn,
    value: Entity<Vec<SharedString>>,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let el = (self.build)(&self.value, window, cx);
        div().w(px(900.)).h(px(800.)).child(el)
    }
}

type Changes = Rc<RefCell<Vec<Vec<SharedString>>>>;

fn host<'a>(
    cx: &'a mut TestAppContext,
    initial: &[&str],
    build: impl Fn(&Entity<Vec<SharedString>>, &mut Window, &mut App) -> AnyElement + 'static,
) -> (&'a mut VisualTestContext, Entity<Vec<SharedString>>) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let initial = s(initial);
    let value = cx.update(|cx| cx.new(|_| initial));
    let v = value.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        build: Rc::new(build),
        value: v,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, value)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    // A popover holds input through its entrance.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    settle(cx);
    let b = cx
        .debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(b.center(), Modifiers::none());
    settle(cx);
}

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .is_some()
}

fn recording(
    changes: &Changes,
    value: &Entity<Vec<SharedString>>,
) -> impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static {
    let (changes, value) = (changes.clone(), value.clone());
    move |next, _, cx| {
        changes.borrow_mut().push(next.clone());
        value.update(cx, |v, cx| {
            *v = next;
            cx.notify();
        });
    }
}

fn list(
    changes: &Changes,
    disabled: bool,
) -> impl Fn(&Entity<Vec<SharedString>>, &mut Window, &mut App) -> AnyElement + 'static {
    let changes = changes.clone();
    move |value, _, cx| {
        TransferList::new("tl", "Columns", options(), value.read(cx).clone())
            .on_change(recording(&changes, value))
            .disabled(disabled)
            .select_all(true)
            .clear(true)
            .into_any_element()
    }
}

#[gpui::test]
fn appends_an_added_value_and_keeps_the_order_when_removing(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, value) = host(cx, &["status", "name"], list(&changes, false));
    click(cx, "tl-available-owner");
    assert_eq!(
        *value.read_with(cx, |v, _| v.clone()),
        s(&["status", "name", "owner"])
    );
    let said = cx.update(|window, cx| announcement(Politeness::Polite, window, cx));
    assert!(said.contains("Owner"), "{said}");
    click(cx, "tl-selected-name");
    assert_eq!(
        *value.read_with(cx, |v, _| v.clone()),
        s(&["status", "owner"])
    );
    assert_eq!(changes.borrow().len(), 2);
}

#[gpui::test]
fn keeps_locked_controls_visible_and_inert(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, value) = host(cx, &["locked"], list(&changes, false));
    assert!(shown(cx, "tl-selected-locked"));
    click(cx, "tl-selected-locked");
    assert_eq!(*value.read_with(cx, |v, _| v.clone()), s(&["locked"]));
    assert!(changes.borrow().is_empty());
}

#[gpui::test]
fn bulk_actions_commit_at_once(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, value) = host(cx, &["status"], list(&changes, false));
    click(cx, "tl-add-all");
    assert_eq!(
        *value.read_with(cx, |v, _| v.clone()),
        s(&["status", "name", "owner", "priority"])
    );
    click(cx, "tl-clear");
    assert!(value.read_with(cx, |v, _| v.is_empty()));
    assert_eq!(changes.borrow().len(), 2);
    assert!(shown(cx, "transfer-list-empty-selected"));
}

#[gpui::test]
fn a_disabled_list_changes_nothing(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, _) = host(cx, &["status"], list(&changes, true));
    click(cx, "tl-available-owner");
    click(cx, "tl-selected-status");
    click(cx, "tl-add-all");
    assert!(changes.borrow().is_empty());
}

#[gpui::test]
fn hands_each_move_to_the_owner(cx: &mut TestAppContext) {
    let moves = Rc::new(RefCell::new(Vec::new()));
    let m = moves.clone();
    let reorderable = create_reorderable(ReorderableOptions::new(move |mv, _, _| {
        m.borrow_mut().push((mv.key.clone(), mv.to));
    }));
    let r = reorderable.clone();
    let (cx, _) = host(cx, &["name", "owner", "status"], move |value, _, cx| {
        TransferList::new("tl", "Columns", options(), value.read(cx).clone())
            .reorderable(r.clone())
            .into_any_element()
    });
    // The owner's start(key) finds the list: reorder mode on Owner, then a move down.
    cx.update(|window, cx| reorderable.start(ItemKey::Text("owner".into()), window, cx));
    settle(cx);
    cx.simulate_keystrokes("down");
    settle(cx);
    assert_eq!(*moves.borrow(), [(ItemKey::Text("owner".into()), 2)]);
}

fn selector(
    changes: &Changes,
    behavior: CommitBehavior,
) -> impl Fn(&Entity<Vec<SharedString>>, &mut Window, &mut App) -> AnyElement + 'static {
    let changes = changes.clone();
    move |value, _, cx| {
        TransferListSelector::new("tls", "Columns", options(), value.read(cx).clone())
            .on_change(recording(&changes, value))
            .commit_behavior(behavior)
            .into_any_element()
    }
}

#[gpui::test]
fn the_selector_commits_each_edit_at_once_by_default_with_no_footer(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, value) = host(cx, &["name"], selector(&changes, CommitBehavior::Immediate));
    click(cx, "tls-control");
    assert!(shown(cx, "tls-content"));
    click(cx, "tls-list-available-owner");
    assert_eq!(
        *value.read_with(cx, |v, _| v.clone()),
        s(&["name", "owner"])
    );
    assert_eq!(changes.borrow().len(), 1);
    assert!(!shown(cx, "tls-apply"));
}

#[gpui::test]
fn a_staged_selector_keeps_edits_in_a_draft_until_apply(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, value) = host(cx, &["name"], selector(&changes, CommitBehavior::Staged));
    click(cx, "tls-control");
    click(cx, "tls-list-available-owner");
    click(cx, "tls-list-available-status");
    assert!(changes.borrow().is_empty(), "nothing commits before Apply");
    assert_eq!(*value.read_with(cx, |v, _| v.clone()), s(&["name"]));
    click(cx, "tls-apply");
    assert_eq!(*changes.borrow(), [s(&["name", "owner", "status"])]);
    assert!(!shown(cx, "tls-content"), "closed");
}

#[gpui::test]
fn cancel_drops_the_draft(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, value) = host(cx, &["name"], selector(&changes, CommitBehavior::Staged));
    click(cx, "tls-control");
    click(cx, "tls-list-available-owner");
    click(cx, "tls-cancel");
    assert!(changes.borrow().is_empty());
    // Reopened, the draft is the applied value again.
    click(cx, "tls-control");
    assert!(shown(cx, "tls-list-available-owner"));
    assert_eq!(*value.read_with(cx, |v, _| v.clone()), s(&["name"]));
}

#[gpui::test]
fn an_unchanged_apply_closes_without_a_change(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, _) = host(cx, &["name"], selector(&changes, CommitBehavior::Staged));
    click(cx, "tls-control");
    click(cx, "tls-apply");
    assert!(changes.borrow().is_empty());
    assert!(!shown(cx, "tls-content"));
}

#[test]
fn a_move_among_the_shown_rows_lands_in_the_values_terms() {
    // A value without an option keeps its place in the order.
    assert_eq!(
        reorder_in_value(&s(&["a", "c"]), &s(&["a", "ghost", "c"]), "c", 0),
        Some(0)
    );
    assert_eq!(
        reorder_in_value(&s(&["a", "b", "c"]), &s(&["x", "a", "b", "c"]), "a", 2),
        Some(3)
    );
    // Nothing moves onto itself or past the end, nor a key that is not shown.
    assert_eq!(
        reorder_in_value(&s(&["a", "b"]), &s(&["a", "b"]), "a", 0),
        None
    );
    assert_eq!(
        reorder_in_value(&s(&["a", "b"]), &s(&["a", "b"]), "a", 5),
        None
    );
    assert_eq!(
        reorder_in_value(&s(&["a", "b"]), &s(&["a", "b"]), "z", 1),
        None
    );
}

#[gpui::test]
fn a_move_reaches_the_owner_in_the_values_terms_around_a_value_without_an_option(
    cx: &mut TestAppContext,
) {
    let moves = Rc::new(RefCell::new(Vec::new()));
    let m = moves.clone();
    let reorderable = create_reorderable(ReorderableOptions::new(move |mv, _, _| {
        m.borrow_mut().push((mv.key.clone(), mv.to));
    }));
    let r = reorderable.clone();
    let (cx, _) = host(
        cx,
        &["name", "ghost", "owner", "status"],
        move |value, _, cx| {
            TransferList::new("tl", "Columns", options(), value.read(cx).clone())
                .reorderable(r.clone())
                .into_any_element()
        },
    );
    // Shown: Name, Owner, Status. Owner down one place lands after Status: index 3.
    cx.update(|window, cx| reorderable.start(ItemKey::Text("owner".into()), window, cx));
    settle(cx);
    cx.simulate_keystrokes("down");
    settle(cx);
    assert_eq!(*moves.borrow(), [(ItemKey::Text("owner".into()), 3)]);
}

#[gpui::test]
fn a_staged_selector_moves_its_draft_and_the_owners_start_finds_its_list(cx: &mut TestAppContext) {
    let moves = Rc::new(RefCell::new(Vec::new()));
    let m = moves.clone();
    let reorderable = create_reorderable(ReorderableOptions::new(move |mv, _, _| {
        m.borrow_mut().push((mv.key.clone(), mv.to));
    }));
    let changes: Changes = Rc::default();
    let (c, r) = (changes.clone(), reorderable.clone());
    let (cx, _) = host(cx, &["name", "owner", "status"], move |value, _, cx| {
        TransferListSelector::new("tls", "Columns", options(), value.read(cx).clone())
            .on_change(recording(&c, value))
            .commit_behavior(CommitBehavior::Staged)
            .reorderable(r.clone())
            .into_any_element()
    });
    click(cx, "tls-control");
    cx.update(|window, cx| reorderable.start(ItemKey::Text("name".into()), window, cx));
    settle(cx);
    cx.simulate_keystrokes("down");
    settle(cx);
    assert!(
        moves.borrow().is_empty(),
        "a staged move goes to the draft, not the owner"
    );
    assert!(changes.borrow().is_empty());
    click(cx, "tls-apply");
    assert_eq!(*changes.borrow(), [s(&["owner", "name", "status"])]);
}

#[gpui::test]
fn an_immediate_selector_drops_a_move_while_busy(cx: &mut TestAppContext) {
    let moves = Rc::new(RefCell::new(Vec::new()));
    let m = moves.clone();
    let reorderable = create_reorderable(ReorderableOptions::new(move |mv, _, _| {
        m.borrow_mut().push((mv.key.clone(), mv.to));
    }));
    let changes: Changes = Rc::default();
    let (c, r) = (changes.clone(), reorderable.clone());
    let (cx, _) = host(cx, &["name", "owner"], move |value, _, cx| {
        TransferListSelector::new("tls", "Columns", options(), value.read(cx).clone())
            .on_change(recording(&c, value))
            .change_action(|_, _, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(60))
            })
            .reorderable(r.clone())
            .into_any_element()
    });
    click(cx, "tls-control");
    // A transfer starts the action: the selector is busy for a minute.
    click(cx, "tls-list-available-status");
    assert_eq!(changes.borrow().len(), 1);
    cx.update(|window, cx| reorderable.start(ItemKey::Text("name".into()), window, cx));
    settle(cx);
    cx.simulate_keystrokes("down");
    settle(cx);
    assert!(moves.borrow().is_empty(), "no move while busy");
}

#[gpui::test]
fn an_immediate_selector_hands_a_move_to_the_owner(cx: &mut TestAppContext) {
    let moves = Rc::new(RefCell::new(Vec::new()));
    let m = moves.clone();
    let reorderable = create_reorderable(ReorderableOptions::new(move |mv, _, _| {
        m.borrow_mut().push((mv.key.clone(), mv.to));
    }));
    let r = reorderable.clone();
    let (cx, _) = host(cx, &["name", "owner"], move |value, _, cx| {
        TransferListSelector::new("tls", "Columns", options(), value.read(cx).clone())
            .reorderable(r.clone())
            .into_any_element()
    });
    click(cx, "tls-control");
    cx.update(|window, cx| reorderable.start(ItemKey::Text("name".into()), window, cx));
    settle(cx);
    cx.simulate_keystrokes("down");
    settle(cx);
    assert_eq!(*moves.borrow(), [(ItemKey::Text("name".into()), 1)]);
}

struct Narrow;

impl Render for Narrow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .child(TransferList::new("tl", "Columns", options(), s(&["name"])))
    }
}

#[gpui::test]
fn a_narrow_list_stacks_its_panels_on_its_first_frame(cx: &mut TestAppContext) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(|_, _| Narrow);
    cx.run_until_parked();
    let selected = cx
        .debug_bounds("transfer-list-panel-selected")
        .expect("drawn");
    let available = cx
        .debug_bounds("transfer-list-panel-available")
        .expect("drawn");
    assert!(
        available.origin.y > selected.origin.y,
        "stacked: {selected:?} {available:?}"
    );
    assert_eq!(available.size.width, selected.size.width);
}
