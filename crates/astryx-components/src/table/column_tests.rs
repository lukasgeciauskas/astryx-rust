//! Stage-6 plugins: column settings, sticky columns and column resize.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    App, Modifiers, MouseButton, ScrollDelta, ScrollEvent, SharedString, TestAppContext, Window,
    div, point, prelude::*, px,
};

use super::tests::{bounds, host, rows, settle, shown};
use super::*;

// ---- Column settings ------------------------------------------------------------

type Keys = Rc<RefCell<Vec<Vec<SharedString>>>>;

fn settings(keys: &[&'static str], log: Keys) -> ColumnSettingsConfig {
    ColumnSettingsConfig::new(
        [
            ColumnSettingsOption::new("name", "Name").always_visible(true),
            ColumnSettingsOption::new("age", "Age"),
            ColumnSettingsOption::new("id", "Id"),
        ],
        keys.iter().copied(),
        move |next, _, _| log.borrow_mut().push(next),
    )
}

#[gpui::test]
fn the_settings_show_the_active_columns_in_order(cx: &mut TestAppContext) {
    let cx = host(cx, 600., |_, _| {
        let config = settings(&["age", "name"], Keys::default());
        Table::new("t")
            .data(rows())
            .columns([
                TableColumn::new("name"),
                TableColumn::new("age"),
                TableColumn::new("id"),
            ])
            .plugin("columnSettings", use_table_column_settings(&config))
            .into_any_element()
    });
    assert!(!shown(cx, "t-header-id"));
    assert!(bounds(cx, "t-header-age").left() < bounds(cx, "t-header-name").left());
}

#[gpui::test]
fn the_state_toggles_shows_all_and_resets(cx: &mut TestAppContext) {
    let log = Keys::default();
    let cx = host(cx, 100., |_, _| div().into_any_element());
    let state = use_table_column_settings_state(
        settings(&["name", "age"], log.clone()).default_column_keys(["name", "id"]),
    );
    assert!(state.is_column_active("age") && !state.is_column_active("id"));
    assert!(!state.is_column_toggleable("name"));
    cx.update(|window, cx| {
        state.toggle_column("name", window, cx);
        state.toggle_column("age", window, cx);
        state.toggle_column("id", window, cx);
        state.show_all_columns(window, cx);
        state.reset_to_default(window, cx);
        state.set_active_column_keys(["id"], window, cx);
    });
    let s = |v: &[&str]| {
        v.iter()
            .map(|k| SharedString::from(k.to_string()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        *log.borrow(),
        [
            s(&["name"]),
            s(&["name", "age", "id"]),
            s(&["name", "age", "id"]),
            s(&["name", "id"]),
            s(&["id", "name"]),
        ],
        "always-visible columns never leave"
    );
}

// ---- Sticky columns --------------------------------------------------------------

#[test]
fn the_pinned_runs_are_contiguous_from_their_edges() {
    let columns: Vec<TableColumn<Record>> = ["a", "b", "c", "d", "e"].map(TableColumn::new).into();
    let sticky = TableStickyColumns::new().start_keys(["b"]).end_keys(["d"]);
    let edges: Vec<Option<(PinEdge, bool)>> = (0..5)
        .map(|i| sticky.pin(i, &columns).map(|p| (p.edge, p.boundary)))
        .collect();
    assert_eq!(
        edges,
        [
            Some((PinEdge::Start, false)),
            Some((PinEdge::Start, true)),
            None,
            Some((PinEdge::End, true)),
            Some((PinEdge::End, false)),
        ]
    );
    // A column in both runs pins to the start.
    let both = TableStickyColumns::new().start_keys(["c"]).end_keys(["b"]);
    assert_eq!(both.pin(1, &columns).map(|p| p.edge), Some(PinEdge::Start));
    assert_eq!(both.pin(3, &columns).map(|p| p.edge), Some(PinEdge::End));
}

fn wide_table(_: &mut Window, _: &mut App) -> gpui::AnyElement {
    div()
        .w(px(300.))
        .child(
            Table::new("t")
                .data(rows())
                .columns([
                    TableColumn::new("id").width(pixel(100.)),
                    TableColumn::new("name").width(pixel(200.)),
                    TableColumn::new("age").width(pixel(200.)),
                    TableColumn::new("end").width(pixel(80.)),
                ])
                .id_key("id")
                .plugin(
                    "sticky",
                    TableStickyColumns::new()
                        .start_keys(["id"])
                        .end_keys(["end"]),
                ),
        )
        .into_any_element()
}

#[gpui::test]
fn pinned_cells_hold_their_edges_while_the_table_scrolls(cx: &mut TestAppContext) {
    let cx = host(cx, 600., wide_table);
    let region = bounds(cx, "t-header-id").left();
    // At rest the end column sits at the region's end, over what is under it.
    let end_right = bounds(cx, "t-cell-sa-end").right();
    assert!(
        (f32::from(end_right) - f32::from(region) - 300.).abs() < 1.,
        "{end_right:?}"
    );
    let name_before = bounds(cx, "t-cell-sa-name").left();
    let at = bounds(cx, "t-cell-sa-name").center();
    cx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(-120.), px(0.))),
        modifiers: Modifiers::shift(),
        ..Default::default()
    });
    settle(cx);
    let moved = f32::from(name_before) - f32::from(bounds(cx, "t-cell-sa-name").left());
    assert!(moved > 50., "the table scrolled: {moved}");
    assert_eq!(
        bounds(cx, "t-cell-sa-id").left(),
        region,
        "the start column stays"
    );
    assert!((f32::from(bounds(cx, "t-cell-sa-end").right()) - f32::from(end_right)).abs() < 1.);
}

// ---- Column resize ------------------------------------------------------------

type Ends = Rc<RefCell<Vec<HashMap<SharedString, f32>>>>;

fn resizable(ends: Ends, with_columns: bool) -> impl Fn(&mut Window, &mut App) -> gpui::AnyElement {
    move |window, cx| {
        let columns = || {
            vec![
                TableColumn::new("name").width(pixel(200.)),
                TableColumn::new("age").width(pixel(100.)),
                TableColumn::new("id"),
            ]
        };
        let widths =
            window.use_keyed_state("widths", cx, |_, _| HashMap::<SharedString, f32>::new());
        let current = widths.read(cx).clone();
        let ends = ends.clone();
        let mut config = ColumnResizeConfig::new()
            .column_widths(current)
            .on_column_resize_end(move |updates, window, cx| {
                ends.borrow_mut().push(updates.clone());
                widths.update(cx, |w, _| w.extend(updates));
                window.refresh();
            });
        if with_columns {
            config = config.columns(columns());
        }
        let resize = use_table_column_resize("resize", config, window, cx);
        Table::new("t")
            .data(rows())
            .columns(columns())
            .plugin(
                "selection",
                TableSelection::new(|_: &Record| false, |_, _, _, _| {}, |_, _, _| {}, || false),
            )
            .plugin("columnResize", resize)
            .into_any_element()
    }
}

#[gpui::test]
fn fr1_resizable_columns_get_a_boundary(cx: &mut TestAppContext) {
    let cx = host(cx, 800., resizable(Ends::default(), true));
    assert!(shown(cx, "resize-resize-name") && shown(cx, "resize-resize-age"));
    assert!(
        !shown(cx, &format!("resize-resize-{SELECTION_COLUMN_KEY}")),
        "a plugin's column has none"
    );
    assert!(
        !shown(cx, "resize-resize-id"),
        "the last proportional column flexes"
    );
    // FR2: the boundary runs through the body rows.
    let handle = bounds(cx, "resize-resize-name");
    assert!(handle.bottom() >= bounds(cx, "t-row-n2").bottom() - px(1.));
    assert!(
        (f32::from(handle.right()) - f32::from(bounds(cx, "t-header-name").right())).abs() < 1.
    );
}

#[gpui::test]
fn dragging_a_boundary_reports_the_new_widths(cx: &mut TestAppContext) {
    let ends = Ends::default();
    let cx = host(cx, 800., resizable(ends.clone(), true));
    let at = bounds(cx, "resize-resize-name").center();
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    settle(cx);
    cx.simulate_mouse_move(
        at + point(px(30.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    settle(cx);
    // Live: the column follows the pointer before the drag ends.
    assert!((f32::from(bounds(cx, "t-header-name").size.width) - 230.).abs() < 1.);
    cx.simulate_mouse_up(
        at + point(px(30.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    let ends = ends.borrow();
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].get("name"), Some(&230.));
    assert_eq!(ends[0].get("age"), Some(&100.));
    assert!(
        !ends[0].contains_key("id"),
        "the last column is not reported"
    );
}

#[gpui::test]
fn a_pixel_column_never_goes_below_its_width_as_minimum(cx: &mut TestAppContext) {
    let ends = Ends::default();
    let cx = host(cx, 800., resizable(ends.clone(), true));
    let at = bounds(cx, "resize-resize-age").center();
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        at - point(px(60.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        at - point(px(60.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert_eq!(ends.borrow()[0].get("age"), Some(&100.));
}

#[gpui::test]
fn fr8_the_arrow_keys_resize_a_focused_boundary(cx: &mut TestAppContext) {
    let ends = Ends::default();
    let cx = host(cx, 800., resizable(ends.clone(), true));
    // Tab reaches a boundary: a Right arrow there widens its column by 10.
    for _ in 0..8 {
        cx.simulate_keystrokes("tab");
        settle(cx);
        cx.simulate_keystrokes("right");
        settle(cx);
        if !ends.borrow().is_empty() {
            break;
        }
    }
    let ends = ends.borrow();
    assert_eq!(ends.len(), 1, "one boundary took the key");
    let grew = ends[0].get("name") == Some(&210.) || ends[0].get("age") == Some(&110.);
    assert!(grew, "{:?}", ends[0]);
}

// ---- Scroll-edge fades (AST-025 FR22) -----------------------------------------

fn wide_plain(_: &mut Window, _: &mut App) -> gpui::AnyElement {
    div()
        .w(px(300.))
        .child(
            Table::new("t")
                .data(rows())
                .columns([
                    TableColumn::new("id").width(pixel(100.)),
                    TableColumn::new("name").width(pixel(200.)),
                    TableColumn::new("age").width(pixel(200.)),
                ])
                .id_key("id"),
        )
        .into_any_element()
}

fn touch(cx: &mut gpui::VisualTestContext) {
    for phase in [gpui::TouchPhase::Started, gpui::TouchPhase::Ended] {
        cx.simulate_event(gpui::TouchEvent {
            id: gpui::TouchId(9),
            phase,
            position: point(px(590.), px(590.)),
            ..Default::default()
        });
    }
    settle(cx);
}

#[gpui::test]
fn ast025_fr22_a_scrolling_table_fades_its_hidden_edge_under_a_finger(cx: &mut TestAppContext) {
    let cx = host(cx, 600., wide_plain);
    settle(cx);
    assert!(
        cx.debug_bounds("t-scroll-fade-end").is_none(),
        "with a mouse the scrollbar shows the overflow"
    );
    touch(cx);
    assert!(cx.debug_bounds("t-scroll-fade-end").is_some());
    assert!(
        cx.debug_bounds("t-scroll-fade-start").is_none(),
        "nothing hides at the start"
    );
}

#[gpui::test]
fn ast025_fr22_pinned_columns_turn_their_edge_fades_off(cx: &mut TestAppContext) {
    let cx = host(cx, 600., wide_table);
    settle(cx);
    touch(cx);
    assert!(cx.debug_bounds("t-scroll-fade-end").is_none());
}

#[gpui::test]
fn dec4_under_a_coarse_primary_pointer_the_boundary_takes_no_pointer(cx: &mut TestAppContext) {
    let ends = Ends::default();
    let cx = host(cx, 800., resizable(ends.clone(), true));
    cx.update(|_, cx| astryx_primitives::modality::set_primary_pointer_coarse(Some(true), cx));
    settle(cx);
    let at = bounds(cx, "resize-resize-name").center();
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    settle(cx);
    cx.simulate_mouse_move(
        at + point(px(30.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    settle(cx);
    cx.simulate_mouse_up(
        at + point(px(30.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert!(ends.borrow().is_empty(), "no drag starts");
    assert!((f32::from(bounds(cx, "t-header-name").size.width) - 200.).abs() < 1.);
}
