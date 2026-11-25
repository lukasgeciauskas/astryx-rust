//! Table's tests: layout (FR2, FR13, FR24), the empty state (FR8), the
//! plugin pipeline (FR9-FR11), children mode (FR3) and row menus.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, Modifiers, MouseButton,
    ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window, div, px,
};

use super::*;

type Calls = Rc<RefCell<Vec<String>>>;

fn rows() -> Vec<Record> {
    vec![
        Record::new()
            .with("id", "a")
            .with("name", "Ada")
            .with("age", 36),
        Record::new()
            .with("id", "b")
            .with("name", "Grace")
            .with("age", 85),
        Record::new()
            .with("id", "c")
            .with("name", "Alan")
            .with("age", TableValue::Null),
    ]
}

type Build = Box<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Host {
    build: Build,
    width: f32,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w(px(self.width)).child((self.build)(window, cx))
    }
}

fn host(
    cx: &mut TestAppContext,
    width: f32,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        build: Box::new(build),
        width,
    });
    cx.update(|window, _| window.activate_window());
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    cx
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<gpui::Pixels> {
    let s: &'static str = Box::leak(selector.to_string().into_boxed_str());
    cx.debug_bounds(s)
        .unwrap_or_else(|| panic!("no {selector}"))
}

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    let s: &'static str = Box::leak(selector.to_string().into_boxed_str());
    cx.debug_bounds(s).is_some()
}

#[gpui::test]
fn fr2_a_header_and_a_row_per_item(cx: &mut TestAppContext) {
    let cx = host(cx, 600., |_, _| {
        Table::new("t")
            .data(rows())
            .columns([
                TableColumn::new("name").header("Name"),
                TableColumn::new("age").header("Age"),
            ])
            .id_key("id")
            .into_any_element()
    });
    let header = bounds(cx, "t-header-name");
    let first = bounds(cx, "t-row-sa");
    let second = bounds(cx, "t-row-sb");
    assert!(header.bottom() <= first.top() + px(1.));
    assert!(first.bottom() <= second.top() + px(1.));
    // Each row's cells sit under their headers.
    assert_eq!(
        bounds(cx, "t-cell-sb-age").left(),
        bounds(cx, "t-header-age").left()
    );
    assert!(shown(cx, "t-row-sc"));
}

#[gpui::test]
fn fr13_columns_share_the_width_by_their_definitions(cx: &mut TestAppContext) {
    let cx = host(cx, 700., |_, _| {
        Table::new("t")
            .data(rows())
            .columns([
                TableColumn::new("id").width(pixel(100.)),
                TableColumn::new("name").width(proportional(2.)),
                TableColumn::new("age"),
            ])
            .into_any_element()
    });
    let w = |cx: &mut VisualTestContext, k: &str| {
        f32::from(bounds(cx, &format!("t-header-{k}")).size.width)
    };
    assert_eq!(w(cx, "id"), 100.);
    // The other 600 split 2:1.
    assert!((w(cx, "name") - 400.).abs() < 1., "{}", w(cx, "name"));
    assert!((w(cx, "age") - 200.).abs() < 1., "{}", w(cx, "age"));
}

#[gpui::test]
fn fr13_below_its_minimum_the_table_keeps_its_floors_and_scrolls(cx: &mut TestAppContext) {
    let cx = host(cx, 200., |_, _| {
        Table::new("t")
            .data(rows())
            .columns([
                TableColumn::new("id").width(proportional(1.)),
                TableColumn::new("name").width(proportional(1.)),
                TableColumn::new("age"),
            ])
            .into_any_element()
    });
    // Two 120px floors and a widthless column riding at its share: 360.
    let row = bounds(cx, "t-row-n0");
    assert!((f32::from(row.size.width) - 360.).abs() < 1., "{row:?}");
    assert_eq!(f32::from(bounds(cx, "t-header-id").size.width), 120.);
}

#[gpui::test]
fn fr8_an_empty_table_says_so_unless_told_otherwise(cx: &mut TestAppContext) {
    let cx = host(cx, 600., |_, _| {
        div()
            .child(
                Table::new("custom")
                    .data(Vec::<Record>::new())
                    .columns([TableColumn::new("name")])
                    .empty_state(
                        div()
                            .debug_selector(|| "nothing".into())
                            .child("Nothing here"),
                    ),
            )
            .child(
                Table::new("off")
                    .data(Vec::<Record>::new())
                    .columns([TableColumn::new("name")])
                    .no_empty_state(),
            )
            .into_any_element()
    });
    assert!(shown(cx, "nothing"));
    assert!(shown(cx, "custom-header-name"), "the header stays");
}

/// A plugin that records its calls and fills each slot.
struct Recorder {
    name: &'static str,
    calls: Calls,
}

impl TablePlugin<Record> for Recorder {
    fn transforms_body(&self) -> bool {
        true
    }

    fn row_model(&self) -> Option<RowModel> {
        Some(RowModel::Flat)
    }

    fn transform_columns(
        &self,
        mut columns: Vec<TableColumn<Record>>,
        _: &mut Window,
        _: &mut App,
    ) -> Vec<TableColumn<Record>> {
        self.calls
            .borrow_mut()
            .push(format!("{}:columns:{}", self.name, columns.len()));
        if self.name == "sort" {
            columns.push(TableColumn::new("extra").header("Extra"));
        }
        columns
    }

    fn transform_header_cell(
        &self,
        mut props: HeaderCellProps,
        column: &TableColumn<Record>,
        _: &[TableColumn<Record>],
        _: &mut Window,
        _: &mut App,
    ) -> HeaderCellProps {
        if self.name == "sort" && column.key.as_ref() == "name" {
            let key = column.key.clone();
            props.before.push(
                div()
                    .debug_selector(move || format!("before-{key}"))
                    .child("[")
                    .into_any_element(),
            );
            props.after.push(
                div()
                    .debug_selector(|| "after".into())
                    .child("^")
                    .into_any_element(),
            );
            props.overlay.push(
                div()
                    .absolute()
                    .debug_selector(|| "overlay".into())
                    .into_any_element(),
            );
            props.below.push(
                div()
                    .debug_selector(|| "below".into())
                    .child("filter")
                    .into_any_element(),
            );
            props.context_menu.push(TableContextActions::List(vec![
                TableContextAction::new("asc", "Sort ascending", |_, _| {}).group("sort"),
            ]));
        }
        props
    }

    fn transform_body_cell(
        &self,
        mut props: BodyCellProps,
        column: &TableColumn<Record>,
        _: &Record,
        _: &[TableColumn<Record>],
        context: &TableRowContext,
        _: &mut Window,
        _: &mut App,
    ) -> BodyCellProps {
        if self.name == "sort" && column.key.as_ref() == "extra" {
            props.content_suppressed = true;
        }
        if self.name == "mine" && column.key.as_ref() == "name" {
            let key = context.key.clone();
            props.context_menu.push(TableContextActions::lazy(move |_| {
                vec![
                    TableContextAction::new("delete", format!("Delete {key:?}"), |_, _| {})
                        .destructive(true),
                ]
            }));
        }
        props
    }

    fn transform_body_row(
        &self,
        mut props: BodyRowProps,
        _: &Record,
        context: &TableRowContext,
        _: &mut Window,
        _: &mut App,
    ) -> BodyRowProps {
        if self.name == "mine" && context.logical_index == 0 {
            props.after_row.push(
                div()
                    .debug_selector(|| "detail".into())
                    .child("Details")
                    .into_any_element(),
            );
        }
        props
    }

    fn transform_scroll_wrapper(
        &self,
        mut props: ScrollWrapperProps,
        _: &TableRenderContext,
        _: &mut Window,
        _: &mut App,
    ) -> ScrollWrapperProps {
        if self.name == "mine" {
            props.after_table.push(
                div()
                    .debug_selector(|| "pager".into())
                    .child("Page 1")
                    .into_any_element(),
            );
        }
        props
    }

    fn transform_table_context(
        &self,
        child: AnyElement,
        _: &mut Window,
        _: &mut App,
    ) -> AnyElement {
        let name = self.name;
        div()
            .debug_selector(move || format!("context-{name}"))
            .p(px(if name == "sort" { 4. } else { 2. }))
            .child(child)
            .into_any_element()
    }
}

#[gpui::test]
fn fr9_fr10_fr11_the_pipeline_runs_in_order_and_composes(cx: &mut TestAppContext) {
    let calls: Calls = Rc::default();
    let c = calls.clone();
    let cx = host(cx, 700., move |_, _| {
        Table::new("t")
            .data(rows())
            .columns([
                TableColumn::new("name").header("Name"),
                TableColumn::new("age").header("Age"),
            ])
            .id_key("id")
            .plugin(
                "mine",
                Recorder {
                    name: "mine",
                    calls: c.clone(),
                },
            )
            .plugin(
                "sort",
                Recorder {
                    name: "sort",
                    calls: c.clone(),
                },
            )
            .into_any_element()
    });
    // FR9: the known name first; FR10: the later plugin sees the column the
    // earlier one added.
    let order: Vec<String> = calls.borrow().iter().take(2).cloned().collect();
    assert_eq!(order, ["sort:columns:2", "mine:columns:3"]);
    assert!(shown(cx, "t-header-extra"));
    // FR11: the header's slots.
    for s in ["before-name", "after", "overlay", "below"] {
        assert!(shown(cx, s), "{s}");
    }
    let label = bounds(cx, "before-name");
    assert!(
        bounds(cx, "after").left() > label.right(),
        "after follows the label"
    );
    assert!(
        bounds(cx, "below").top() > label.bottom() - px(1.),
        "below sits under it"
    );
    // A detail row after the first row; chrome after the table.
    assert!(bounds(cx, "detail").top() >= bounds(cx, "t-row-sa").bottom() - px(1.));
    assert!(shown(cx, "pager"));
    // FR11: the first plugin's provider is the outermost.
    let (outer, inner) = (bounds(cx, "context-sort"), bounds(cx, "context-mine"));
    assert!(outer.left() < inner.left() && outer.size.width > inner.size.width);
}

#[gpui::test]
fn a_rows_menu_gathers_its_cells_actions(cx: &mut TestAppContext) {
    let calls: Calls = Rc::default();
    let c = calls.clone();
    let cx = host(cx, 700., move |_, _| {
        Table::new("t")
            .data(rows())
            .columns([
                TableColumn::new("name").header("Name"),
                TableColumn::new("age").header("Age"),
            ])
            .id_key("id")
            .plugin(
                "mine",
                Recorder {
                    name: "mine",
                    calls: c.clone(),
                },
            )
            .into_any_element()
    });
    let at = bounds(cx, "t-cell-sb-age").center();
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    assert!(
        shown(cx, "t-row-sb-menu-list-delete"),
        "the row's menu, from the name cell's action"
    );
}

#[gpui::test]
fn fr3_fr24_composed_rows_share_columns_that_fit_their_content(cx: &mut TestAppContext) {
    let cx = host(cx, 700., |_, _| {
        Table::composed("c")
            .child(
                TableHeader::new().child(
                    TableRow::new()
                        .child(TableHeaderCell::new().child("Item"))
                        .child(TableHeaderCell::new().child("Qty")),
                ),
            )
            .child(
                TableBody::new().child(
                    TableRow::new()
                        .child(TableCell::new().child("A long item name that needs room"))
                        .child(TableCell::new().child("1")),
                ),
            )
            .child(
                TableFooter::new()
                    .child(TableRow::new().child(TableCell::new().col_span(2).child("Total"))),
            )
            .into_any_element()
    });
    let (h0, h1) = (bounds(cx, "c-header-0-0"), bounds(cx, "c-header-0-1"));
    let (b0, b1) = (bounds(cx, "c-body-0-0"), bounds(cx, "c-body-0-1"));
    assert_eq!(h1.left(), b1.left(), "the rows share columns");
    assert_eq!(h0.size.width, b0.size.width);
    assert!(
        b0.size.width > b1.size.width,
        "the long item's column is wider"
    );
    // The footer's cell spans both columns.
    let total = bounds(cx, "c-footer-0-0");
    assert_eq!(total.left(), h0.left());
    assert_eq!(total.right(), h1.right());
}
