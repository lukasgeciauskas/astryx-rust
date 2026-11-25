//! Stage-7: row and column windowing (the rows virtualization spec's FR
//! items and column windowing).

use gpui::{
    App, Modifiers, ScrollDelta, ScrollEvent, TestAppContext, VisualTestContext, Window, div,
    point, prelude::*, px,
};

use super::tests::{bounds, host, settle, shown};
use super::*;

fn many(n: usize) -> Vec<Record> {
    (0..n)
        .map(|i| {
            Record::new()
                .with("id", format!("r{i}"))
                .with("name", format!("Row {i}"))
        })
        .collect()
}

fn windowed(
    n: usize,
    row_height: f32,
    configure: impl Fn(Table<Record>) -> Table<Record> + 'static,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyElement {
    move |_, _| {
        let table = Table::new("t")
            .data(many(n))
            .columns([TableColumn::new("name")])
            .id_key("id")
            .virtualization(use_table_rows_virtualization(row_height, 400.));
        div().h(px(400.)).child(configure(table)).into_any_element()
    }
}

fn wheel(cx: &mut VisualTestContext, at: &str, dx: f32, dy: f32) {
    let at = bounds(cx, at).center();
    cx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
    settle(cx);
}

#[gpui::test]
fn fr2_only_the_rows_near_the_viewport_mount(cx: &mut TestAppContext) {
    let cx = host(cx, 600., windowed(1000, 40., |t| t));
    assert!(shown(cx, "t-row-sr0") && shown(cx, "t-row-sr5"));
    assert!(!shown(cx, "t-row-sr60") && !shown(cx, "t-row-sr999"));
    // Rows keep the pitch.
    let pitch = f32::from(bounds(cx, "t-row-sr1").top() - bounds(cx, "t-row-sr0").top());
    assert!((pitch - 40.).abs() < 0.5, "{pitch}");
}

#[gpui::test]
fn fr3_scrolling_mounts_the_rows_it_reaches(cx: &mut TestAppContext) {
    let cx = host(cx, 600., windowed(1000, 40., |t| t));
    let first_top = bounds(cx, "t-row-sr0").top();
    wheel(cx, "t-row-sr3", 0., -4000.);
    assert!(!shown(cx, "t-row-sr0"), "the first rows unmount");
    assert!(shown(cx, "t-row-sr100"));
    // The row sits where the full table would put it.
    let expected = f32::from(first_top) + 100. * 40. - 4000.;
    let top = f32::from(bounds(cx, "t-row-sr100").top());
    assert!((top - expected).abs() < 1., "{top} vs {expected}");
}

#[gpui::test]
fn fr17_equal_taller_rows_become_the_pitch(cx: &mut TestAppContext) {
    // A 10 px pitch is shorter than any real row.
    let cx = host(cx, 600., windowed(1000, 10., |t| t));
    let pitch = f32::from(bounds(cx, "t-row-sr1").top() - bounds(cx, "t-row-sr0").top());
    assert!(
        pitch > 20.,
        "rows never measure shorter than their content: {pitch}"
    );
    // The window follows the measured pitch, not the 10 px one.
    assert!(!shown(cx, "t-row-sr40"));
}

#[gpui::test]
fn fr14_custom_cells_and_wrapping_render_every_row(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        600.,
        windowed(120, 40., |t| {
            t.columns([TableColumn::new("name").render_cell(|r: &Record, _, _| {
                div()
                    .child(r.value("name").display_text())
                    .into_any_element()
            })])
        }),
    );
    assert!(shown(cx, "t-row-sr119"), "a custom renderer falls back");
}

#[gpui::test]
fn fr14_explicit_wrap_falls_back(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        600.,
        windowed(120, 40., |t| t.text_overflow(TableTextOverflow::Wrap)),
    );
    assert!(shown(cx, "t-row-sr119"));
}

#[gpui::test]
fn fr6_without_a_valid_identity_every_row_renders(cx: &mut TestAppContext) {
    let cx = host(cx, 600., move |_, _| {
        let mut data = many(120);
        // A duplicate key.
        data[5] = Record::new().with("id", "r4").with("name", "Dup");
        div()
            .h(px(400.))
            .child(
                Table::new("t")
                    .data(data)
                    .columns([TableColumn::new("name")])
                    .id_key("id")
                    .virtualization(use_table_rows_virtualization(40., 400.)),
            )
            .into_any_element()
    });
    assert!(shown(cx, "t-row-sr119"));
}

#[gpui::test]
fn selection_and_row_numbers_keep_rows_windowed(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        600.,
        windowed(1000, 40., |t| {
            t.plugin(
                "selection",
                TableSelection::new(|_: &Record| false, |_, _, _, _| {}, |_, _, _| {}, || false),
            )
            .plugin("rowIndex", TableRowIndex::new())
        }),
    );
    assert!(shown(cx, "t-row-sr0") && !shown(cx, "t-row-sr999"));
}

// ---- Columns ------------------------------------------------------------------------

fn wide(_: &mut Window, _: &mut App) -> gpui::AnyElement {
    let columns: Vec<TableColumn<Record>> = (0..30)
        .map(|i| TableColumn::new(format!("c{i}")).width(pixel(100.)))
        .collect();
    div()
        .w(px(300.))
        .child(
            Table::new("t")
                .data(many(3))
                .columns(columns)
                .id_key("id")
                .plugin("sticky", TableStickyColumns::new().start_keys(["c0"]))
                .virtualization(use_table_columns_virtualization(300.)),
        )
        .into_any_element()
}

#[gpui::test]
fn only_the_columns_near_the_viewport_mount(cx: &mut TestAppContext) {
    let cx = host(cx, 600., wide);
    assert!(shown(cx, "t-header-c1") && shown(cx, "t-cell-sr0-c2"));
    assert!(!shown(cx, "t-header-c20") && !shown(cx, "t-cell-sr0-c20"));
    // An unmounted column keeps its track: the next mounted one sits in place.
    let left = bounds(cx, "t-header-c0").left();
    assert_eq!(bounds(cx, "t-header-c2").left(), left + px(200.));
}

#[gpui::test]
fn scrolling_sideways_mounts_columns_and_keeps_pinned_ones(cx: &mut TestAppContext) {
    let cx = host(cx, 600., wide);
    wheel(cx, "t-cell-sr0-c1", -2000., 0.);
    assert!(shown(cx, "t-header-c20") && shown(cx, "t-cell-sr1-c21"));
    assert!(!shown(cx, "t-header-c5"));
    assert!(shown(cx, "t-header-c0"), "a pinned column stays mounted");
}
