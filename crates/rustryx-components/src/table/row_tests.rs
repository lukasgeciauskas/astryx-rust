//! Stage-3 plugins: selection with its state and toolbar, row status, row
//! numbers and activation (the activation contract's FR items).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gpui::{
    App, Modifiers, ScrollHandle, SharedString, TestAppContext, VisualTestContext, Window, div,
    prelude::*,
};

use super::tests::{bounds, host, rows, settle, shown};
use super::*;

type Keys = Rc<RefCell<Vec<SharedString>>>;

fn sorted(keys: &HashSet<SharedString>) -> Vec<SharedString> {
    let mut keys: Vec<SharedString> = keys.iter().cloned().collect();
    keys.sort();
    keys
}

fn id_of(r: &Record) -> String {
    r.value("id").display_text().to_string()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let at = bounds(cx, selector).center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn press(cx: &mut VisualTestContext, key: &'static str, held: bool) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: held,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

/// A table with its own selection (row "c" locked), the toolbar above.
fn selection_table(
    seen: Keys,
    start: &'static [&'static str],
    selectable: bool,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyElement {
    move |window, cx| {
        let keys = window.use_keyed_state("keys", cx, |_, _| {
            start
                .iter()
                .map(|k| SharedString::from(*k))
                .collect::<HashSet<_>>()
        });
        let current = keys.read(cx).clone();
        let mut config = SelectionStateConfig::new(rows(), "id")
            .selected_keys(current, move |next, window, cx| {
                keys.update(cx, |k, _| *k = next);
                window.refresh();
            })
            .is_item_enabled(|r: &Record| id_of(r) != "c");
        if !selectable {
            config = config.is_item_selectable(|r: &Record| id_of(r) != "c");
        }
        let sel = use_table_selection_state("sel", config, window, cx);
        *seen.borrow_mut() = sorted(&sel.state.selected_keys);
        div()
            .child(TableSelectionToolbar::new("bar", sel.state.clone()))
            .child(
                Table::new("t")
                    .data(rows())
                    .columns([TableColumn::new("name").header("Name")])
                    .id_key("id")
                    .plugin("selection", sel.plugin),
            )
            .into_any_element()
    }
}

const SEL: &str = SELECTION_COLUMN_KEY;

#[gpui::test]
fn a_row_checkbox_toggles_its_key(cx: &mut TestAppContext) {
    let seen = Keys::default();
    let cx = host(cx, 600., selection_table(seen.clone(), &[], true));
    // The checkbox column comes first, 36 px wide.
    let check = bounds(cx, &format!("t-header-{SEL}"));
    assert!(check.left() < bounds(cx, "t-header-name").left());
    assert_eq!(f32::from(check.size.width), 36.);
    click(cx, &format!("t-cell-sb-{SEL}"));
    assert_eq!(*seen.borrow(), ["b"]);
    click(cx, &format!("t-cell-sb-{SEL}"));
    assert!(seen.borrow().is_empty());
}

#[gpui::test]
fn select_all_takes_the_actionable_rows_and_keeps_frozen_ones(cx: &mut TestAppContext) {
    let seen = Keys::default();
    // "c" is locked (disabled) while selected: frozen.
    let cx = host(cx, 600., selection_table(seen.clone(), &["c"], true));
    click(cx, &format!("t-header-{SEL}"));
    assert_eq!(*seen.borrow(), ["a", "b", "c"]);
    // All selected: the header now deselects, and "c" stays.
    click(cx, &format!("t-header-{SEL}"));
    assert_eq!(*seen.borrow(), ["c"]);
    // A locked row's checkbox never changes.
    click(cx, &format!("t-cell-sc-{SEL}"));
    assert_eq!(*seen.borrow(), ["c"]);
}

#[gpui::test]
fn a_partial_selection_selects_all_from_the_header(cx: &mut TestAppContext) {
    let seen = Keys::default();
    let cx = host(cx, 600., selection_table(seen.clone(), &["a"], true));
    click(cx, &format!("t-header-{SEL}"));
    assert_eq!(*seen.borrow(), ["a", "b"]);
}

#[gpui::test]
fn an_unselectable_row_has_no_checkbox(cx: &mut TestAppContext) {
    let seen = Keys::default();
    let cx = host(cx, 600., selection_table(seen.clone(), &[], false));
    click(cx, &format!("t-cell-sc-{SEL}"));
    assert!(seen.borrow().is_empty());
}

#[gpui::test]
fn the_toolbar_counts_and_clears_the_whole_selection(cx: &mut TestAppContext) {
    let seen = Keys::default();
    let cx = host(cx, 600., selection_table(seen.clone(), &[], true));
    assert!(!shown(cx, "bar-selection-count"), "nothing while empty");
    click(cx, &format!("t-cell-sa-{SEL}"));
    assert!(shown(cx, "bar-selection-count"));
    // Clear takes every key, a frozen one too.
    click(cx, "bar-clear");
    assert!(seen.borrow().is_empty());
    assert!(!shown(cx, "bar-selection-count"));
}

fn row_context(key: Option<RowKey>, status: IdentityStatus, owners: usize) -> TableRowContext {
    TableRowContext {
        key,
        identity_status: status,
        kind: RowKind::Product,
        logical_index: 0,
        logical_row_count: 3,
        header_row_count: 1,
        render: TableRenderContext {
            table: "t".into(),
            block_viewport: false,
            mode: TableMode::Data,
            row_model: RowModel::Flat,
            logical_row_count: 3,
            header_row_count: 1,
            viewport: ScrollHandle::new(),
            viewport_focus: None,
            whole_row_owners: owners,
        },
    }
}

#[gpui::test]
fn fr21_selection_publishes_the_rows_paint(cx: &mut TestAppContext) {
    let cx = host(cx, 100., |_, _| div().into_any_element());
    let data: Rc<[Record]> = rows().into();
    let plugin = |highlight: bool| {
        TableSelection::new(
            |r: &Record| id_of(r) == "b",
            |_, _, _, _| {},
            |_, _, _| {},
            || false,
        )
        .row_highlight(highlight)
    };
    let context = row_context(Some("b".into()), IdentityStatus::Valid, 0);
    cx.update(|window, cx| {
        let item = TableItem::new(data.clone(), 1);
        let on =
            plugin(true).transform_body_row(BodyRowProps::default(), &item, &context, window, cx);
        assert_eq!(
            on.paint,
            RowPaint {
                selected: true,
                highlight: true,
                active: false
            }
        );
        // Without the wash the row stays selected.
        let off =
            plugin(false).transform_body_row(BodyRowProps::default(), &item, &context, window, cx);
        assert!(off.paint.selected && !off.paint.highlight);
        let other = TableItem::new(data.clone(), 0);
        let none =
            plugin(true).transform_body_row(BodyRowProps::default(), &other, &context, window, cx);
        assert!(!none.paint.selected);
    });
}

// ---- Row status and row numbers ---------------------------------------------

#[gpui::test]
fn the_status_gutter_comes_first_and_is_named(cx: &mut TestAppContext) {
    let cx = host(cx, 100., |_, _| div().into_any_element());
    let plugin = use_table_row_status(|r: &Record| {
        (id_of(r) == "a").then(|| TableRowStatus::error("Failed"))
    });
    cx.update(|window, cx| {
        let columns = plugin.transform_columns(vec![TableColumn::new("name")], window, cx);
        assert_eq!(columns[0].key.as_ref(), ROW_STATUS_COLUMN_KEY);
        assert_eq!(columns[0].width, Some(pixel(28.)));
        assert!(!columns[0].resizable);
        let props = HeaderCellProps::default();
        let named = plugin.transform_header_cell(props, &columns[0], &columns, window, cx);
        assert_eq!(named.aria_label.as_deref(), Some("Row status"));
        let props = HeaderCellProps::default();
        let other = plugin.transform_header_cell(props, &columns[1], &columns, window, cx);
        assert_eq!(other.aria_label, None);
    });
    // A semantic status keeps its glyph: an icon is ignored.
    let status = TableRowStatus::success("Done").icon("info");
    assert_eq!(status.label().as_ref(), "Done");
}

#[gpui::test]
fn the_status_column_renders_in_a_table(cx: &mut TestAppContext) {
    let cx = host(cx, 600., |_, _| {
        Table::new("t")
            .data(rows())
            .columns([TableColumn::new("name")])
            .id_key("id")
            .plugin(
                "rowStatus",
                use_table_row_status(|r: &Record| match id_of(r).as_str() {
                    "a" => Some(TableRowStatus::success("Done")),
                    "b" => Some(TableRowStatus::color(TableRowStatusColor::Blue, "Queued")),
                    _ => None,
                }),
            )
            .into_any_element()
    });
    let gutter = bounds(cx, &format!("t-header-{ROW_STATUS_COLUMN_KEY}"));
    assert_eq!(f32::from(gutter.size.width), 28.);
    assert!(gutter.left() < bounds(cx, "t-header-name").left());
}

#[gpui::test]
fn row_numbers_follow_positions_or_the_given_order(cx: &mut TestAppContext) {
    let cx = host(cx, 100., |_, _| div().into_any_element());
    let data: Rc<[Record]> = rows().into();
    // The table shows c, a, b (sorted); the original order is a, b, c.
    let shown_rows: Rc<[Record]> = vec![data[2].clone(), data[0].clone(), data[1].clone()].into();
    cx.update(|window, cx| {
        let by_position = TableRowIndex::<Record>::new();
        let column = TableColumn::new(ROW_INDEX_COLUMN_KEY);
        for i in 0..3 {
            let item = TableItem::new(shown_rows.clone(), i);
            let mut context = row_context(None, IdentityStatus::Valid, 0);
            context.logical_index = i;
            by_position.transform_body_cell(
                BodyCellProps::default(),
                &column,
                &item,
                &[],
                &context,
                window,
                cx,
            );
        }
        let n = |p: &TableRowIndex<Record>, i: usize| {
            p.number_for(&TableItem::new(shown_rows.clone(), i))
        };
        assert_eq!((n(&by_position, 0), n(&by_position, 2)), (Some(1), Some(3)));
        let keyed = TableRowIndex::new().data_keyed(data.clone(), |r: &Record| id_of(r).into());
        assert_eq!((n(&keyed, 0), n(&keyed, 1)), (Some(3), Some(1)));
        let equal = TableRowIndex::new().data(data.clone()).start_from(0);
        assert_eq!((n(&equal, 0), n(&equal, 2)), (Some(2), Some(1)));
        // The same data: each row is its own item.
        let same = TableRowIndex::new().data(shown_rows.clone());
        assert_eq!(n(&same, 0), Some(1));
    });
    let index = TableRowIndex::<Record>::new();
    cx.update(|window, cx| {
        let columns = index.transform_columns(vec![TableColumn::new("name")], window, cx);
        assert_eq!(columns[0].width, Some(pixel(48.)));
        assert_eq!(columns[0].align, Some(TableColumnAlign::End));
        assert_eq!(columns[0].header_text().as_deref(), Some("#"));
    });
}

// ---- Activation -------------------------------------------------------------

type Activations = Rc<RefCell<Vec<RowKey>>>;

/// A plugin that claims the whole row too, or cancels the row's press.
struct Other {
    owner: bool,
    cancel: bool,
}

impl TablePlugin<Record> for Other {
    fn whole_row_owner(&self) -> Option<WholeRowOwner> {
        self.owner.then_some(WholeRowOwner::Expansion)
    }

    fn transform_body_row(
        &self,
        mut props: BodyRowProps,
        _: &TableItem<Record>,
        _: &TableRowContext,
        _: &mut Window,
        _: &mut App,
    ) -> BodyRowProps {
        if self.cancel {
            props.hooks.push(Box::new(|row| {
                row.on_click(|_, window, _| window.prevent_default())
                    .on_key_down(|_, window, _| window.prevent_default())
            }));
        }
        props
    }
}

fn activation_table(
    log: Activations,
    id_key: bool,
    other: Option<(bool, bool)>,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyElement {
    move |_, _| {
        let log = log.clone();
        let mut table = Table::new("t")
            .data(rows())
            .columns([TableColumn::new("name")])
            .plugin(
                "selection",
                TableSelection::new(|_: &Record| false, |_, _, _, _| {}, |_, _, _| {}, || false),
            )
            .plugin(
                "activation",
                TableActivation::new(Some("b"), move |_: &Record, key, _, _| {
                    log.borrow_mut().push(key.clone())
                })
                .is_item_activatable(|r: &Record| id_of(r) != "c"),
            );
        if id_key {
            table = table.id_key("id");
        }
        if let Some((owner, cancel)) = other {
            table = table.plugin("other", Other { owner, cancel });
        }
        table.into_any_element()
    }
}

#[gpui::test]
fn fr3_the_active_row_shows_its_marker(cx: &mut TestAppContext) {
    let log = Activations::default();
    let cx = host(cx, 600., activation_table(log, true, None));
    assert!(shown(cx, "t-row-sb-active"));
    assert!(!shown(cx, "t-row-sa-active") && !shown(cx, "t-row-sc-active"));
}

#[gpui::test]
fn fr5_a_click_activates_with_the_rows_key(cx: &mut TestAppContext) {
    let log = Activations::default();
    let cx = host(cx, 600., activation_table(log.clone(), true, None));
    click(cx, "t-cell-sa-name");
    assert_eq!(*log.borrow(), [RowKey::from("a")]);
    // FR4: an ineligible row never activates.
    click(cx, "t-cell-sc-name");
    assert_eq!(log.borrow().len(), 1);
}

#[gpui::test]
fn fr6_a_nested_control_never_activates_its_row(cx: &mut TestAppContext) {
    let log = Activations::default();
    let cx = host(cx, 600., activation_table(log.clone(), true, None));
    click(cx, &format!("t-cell-sa-{SEL}"));
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn fr5_fr8_enter_and_space_on_the_focused_row_activate_once(cx: &mut TestAppContext) {
    let log = Activations::default();
    let cx = host(cx, 600., activation_table(log.clone(), true, None));
    // Focus the row with a click, then use the keyboard.
    click(cx, "t-cell-sb-name");
    log.borrow_mut().clear();
    press(cx, "enter", false);
    assert_eq!(*log.borrow(), [RowKey::from("b")]);
    // A repeated key never activates.
    press(cx, "enter", true);
    press(cx, "space", true);
    assert_eq!(log.borrow().len(), 1);
    press(cx, "space", false);
    assert_eq!(log.borrow().len(), 2);
    // Tab moves into the row's checkbox: Enter there never activates the row.
    cx.simulate_keystrokes("tab");
    settle(cx);
    press(cx, "enter", false);
    assert_eq!(log.borrow().len(), 2);
}

#[gpui::test]
fn fr5_an_earlier_handlers_prevent_default_cancels(cx: &mut TestAppContext) {
    let log = Activations::default();
    let cx = host(
        cx,
        600.,
        activation_table(log.clone(), true, Some((false, true))),
    );
    click(cx, "t-cell-sa-name");
    press(cx, "enter", false);
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn fr2_without_a_canonical_identity_activation_is_inert(cx: &mut TestAppContext) {
    let log = Activations::default();
    let cx = host(cx, 600., activation_table(log.clone(), false, None));
    click(cx, "t-cell-i0-name");
    assert!(log.borrow().is_empty());
    assert!(!shown(cx, "t-row-i1-active"));
}

#[gpui::test]
fn fr9_a_second_whole_row_owner_neutralizes_it(cx: &mut TestAppContext) {
    let log = Activations::default();
    let cx = host(
        cx,
        600.,
        activation_table(log.clone(), true, Some((true, false))),
    );
    click(cx, "t-cell-sa-name");
    assert!(log.borrow().is_empty());
    assert!(!shown(cx, "t-row-sb-active"));
}

#[gpui::test]
fn fr2_keys_compare_strictly(cx: &mut TestAppContext) {
    let cx = host(cx, 100., |_, _| div().into_any_element());
    let data: Rc<[Record]> = rows().into();
    let plugin = TableActivation::new(Some(RowKey::Number(1)), |_: &Record, _, _, _| {});
    cx.update(|window, cx| {
        let item = TableItem::new(data.clone(), 0);
        // A text key "1" is not the number 1.
        let context = row_context(Some("1".into()), IdentityStatus::Valid, 1);
        let props = plugin.transform_body_row(BodyRowProps::default(), &item, &context, window, cx);
        assert!(!props.paint.active);
        assert!(props.row_action.is_some() && props.tab_stop == Some(true));
        assert_eq!(props.key_shortcuts, ["Enter", "Space"]);
        let context = row_context(Some(RowKey::Number(1)), IdentityStatus::Valid, 1);
        // An earlier `tab_stop(false)` survives (FR7).
        let earlier = BodyRowProps {
            tab_stop: Some(false),
            key_shortcuts: vec!["Enter".into()],
            ..BodyRowProps::default()
        };
        let props = plugin.transform_body_row(earlier, &item, &context, window, cx);
        assert!(props.paint.active);
        assert_eq!(props.tab_stop, Some(false));
        assert_eq!(props.key_shortcuts, ["Enter", "Space"]);
        // A duplicate key is inert.
        let context = row_context(Some(RowKey::Number(1)), IdentityStatus::Duplicate, 1);
        let props = plugin.transform_body_row(BodyRowProps::default(), &item, &context, window, cx);
        assert!(!props.paint.active && props.row_action.is_none());
    });
}
