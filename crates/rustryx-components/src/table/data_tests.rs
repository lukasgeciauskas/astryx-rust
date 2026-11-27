//! Stage-4 plugins: pagination, filtering and infinite scroll (the infinite
//! scroll contract's FR items).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, Modifiers, SharedString, Task, TestAppContext, VisualTestContext, Window, div, prelude::*,
    px,
};

use super::tests::{bounds, host, rows, settle, shown};
use super::*;
use crate::power_search::{
    EnumItem, FieldDefinition, FieldKind, FilterValue, PowerSearchConfig,
    create_power_search_config,
};

fn click(cx: &mut VisualTestContext, selector: &str) {
    let at = bounds(cx, selector).center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

// ---- Pagination -----------------------------------------------------------

#[test]
fn paginate_data_slices_a_page() {
    let data: Vec<u32> = (1..=12).collect();
    assert_eq!(paginate_data(&data, 2, 5), [6, 7, 8, 9, 10]);
    assert_eq!(paginate_data(&data, 3, 5), [11, 12]);
    // Page 0 reads as 1, a size of 0 as 1.
    assert_eq!(paginate_data(&data, 0, 0), [1]);
    assert!(paginate_data(&data, 9, 5).is_empty());
}

fn paged(
    position: TablePaginationPosition,
    total: usize,
    more: Option<bool>,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyElement {
    move |_, _| {
        let mut plugin = TablePagination::new(1, |_, _, _| {})
            .total_items(total)
            .page_size(2)
            .position(position);
        if let Some(more) = more {
            plugin = plugin.has_more(more);
        }
        Table::new("t")
            .data(rows())
            .columns([TableColumn::new("name")])
            .plugin("pagination", plugin)
            .into_any_element()
    }
}

#[gpui::test]
fn the_controls_go_where_position_says(cx: &mut TestAppContext) {
    let cx = host(cx, 600., paged(TablePaginationPosition::Below, 6, None));
    assert!(shown(cx, "table-pagination-below") && !shown(cx, "table-pagination-above"));
    assert!(bounds(cx, "table-pagination-below").top() >= bounds(cx, "t-row-n2").bottom());
}

#[gpui::test]
fn both_positions_render_twice(cx: &mut TestAppContext) {
    let cx = host(cx, 600., paged(TablePaginationPosition::Both, 6, None));
    assert!(shown(cx, "table-pagination-above") && shown(cx, "table-pagination-below"));
    assert!(bounds(cx, "table-pagination-above").bottom() <= bounds(cx, "t-header-name").top());
}

#[gpui::test]
fn one_page_shows_no_controls_unless_more_may_come(cx: &mut TestAppContext) {
    let cx = host(cx, 600., paged(TablePaginationPosition::Below, 2, None));
    assert!(!shown(cx, "table-pagination-below"));
}

#[gpui::test]
fn more_to_come_keeps_the_controls(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        600.,
        paged(TablePaginationPosition::Below, 2, Some(true)),
    );
    assert!(shown(cx, "table-pagination-below"));
}

#[gpui::test]
fn position_none_renders_nothing(cx: &mut TestAppContext) {
    let cx = host(cx, 600., paged(TablePaginationPosition::None, 6, None));
    assert!(!shown(cx, "table-pagination-below") && !shown(cx, "table-pagination-above"));
}

// ---- Filtering ------------------------------------------------------------

fn search() -> PowerSearchConfig {
    create_power_search_config(
        [
            FieldDefinition::new("name", FieldKind::String),
            FieldDefinition::new("role", FieldKind::Enum)
                .enum_values([EnumItem::new("a", "A"), EnumItem::new("b", "B")]),
            FieldDefinition::new("age", FieldKind::Number),
            FieldDefinition::new("joined", FieldKind::Date),
            FieldDefinition::new("tags", FieldKind::StringList),
        ],
        None,
    )
    .config
}

fn filtered_columns() -> Vec<TableColumn<Record>> {
    vec![
        TableColumn::new("name")
            .header("Name")
            .filter(TableFilterRef::Field("name".into())),
        TableColumn::new("role").filter(TableFilterRef::FieldOperator {
            field: "role".into(),
            operator: "is_any_of".into(),
        }),
        TableColumn::new("age")
            .width(pixel(80.))
            .filter(TableFilterRef::Field("age".into())),
        TableColumn::new("joined").filter(TableFilterRef::Field("joined".into())),
        TableColumn::new("tags").filter(TableFilterRef::Field("tags".into())),
        TableColumn::new("plain"),
    ]
}

#[test]
fn to_search_filters_maps_values_by_the_operators_kind() {
    let mut filters = TableFilterState::new();
    filters.insert("name".into(), "ada".into());
    filters.insert("role".into(), TableFilterValue::List(vec!["a".into()]));
    filters.insert("age".into(), 36.0.into());
    filters.insert("joined".into(), "2024-03-05".into());
    filters.insert("tags".into(), TableFilterValue::List(vec!["x".into()]));
    filters.insert("plain".into(), "ignored".into());
    let out = to_search_filters(&filters, &filtered_columns(), &search());
    let by = |field: &str| {
        out.iter()
            .find(|f| f.field.as_ref() == field)
            .map(|f| (f.operator.to_string(), f.value.clone()))
    };
    assert_eq!(
        by("name"),
        Some(("contains".into(), FilterValue::String("ada".into())))
    );
    assert_eq!(
        by("role"),
        Some(("is_any_of".into(), FilterValue::EnumList(vec!["a".into()])))
    );
    assert_eq!(by("age"), Some(("equals".into(), FilterValue::Float(36.))));
    // A calendar day reads as UTC midnight.
    assert_eq!(
        by("joined"),
        Some((
            "after".into(),
            FilterValue::DateAbsolute {
                unix_seconds: 1_709_596_800
            }
        ))
    );
    assert_eq!(
        by("tags"),
        Some((
            "is_any_of".into(),
            FilterValue::StringList(vec!["x".into()])
        ))
    );
    assert_eq!(by("plain"), None, "a column without a filter");
    // A value of the wrong kind is skipped.
    let mut wrong = TableFilterState::new();
    wrong.insert("age".into(), "old".into());
    assert!(to_search_filters(&wrong, &filtered_columns(), &search()).is_empty());
}

#[gpui::test]
fn the_filter_state_sets_removes_and_clears(cx: &mut TestAppContext) {
    let seen: Rc<RefCell<Vec<usize>>> = Rc::default();
    let handle: Rc<RefCell<Option<TableFilterStateHandle>>> = Rc::default();
    let (s, h) = (seen.clone(), handle.clone());
    let cx = host(cx, 100., move |window, cx| {
        let state = use_table_filter_state("f", None, window, cx);
        s.borrow_mut().push(state.filters.len());
        *h.borrow_mut() = Some(state);
        div().into_any_element()
    });
    let state = handle.borrow().clone().unwrap();
    let change = state.on_filter_change();
    cx.update(|window, cx| {
        change("name".into(), Some("ada".into()), window, cx);
        change("age".into(), Some(3.0.into()), window, cx);
    });
    settle(cx);
    assert_eq!(seen.borrow().last(), Some(&2));
    cx.update(|window, cx| change("name".into(), None, window, cx));
    settle(cx);
    assert_eq!(seen.borrow().last(), Some(&1));
    let state = handle.borrow().clone().unwrap();
    cx.update(|window, cx| state.clear_all(window, cx));
    settle(cx);
    assert_eq!(seen.borrow().last(), Some(&0));
}

type Changes = Rc<RefCell<Vec<(SharedString, Option<TableFilterValue>)>>>;

fn filter_table(
    variant: TableFilterVariant,
    changes: Changes,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyElement {
    move |window, cx| {
        let state = use_table_filter_state("f", None, window, cx);
        let (log, set) = (changes.clone(), state.on_filter_change());
        Table::new("t")
            .data(rows())
            .columns(filtered_columns())
            .plugin(
                "filter",
                TableFiltering::new(
                    state.filters.clone(),
                    move |key, value, window, cx| {
                        log.borrow_mut().push((key.clone(), value.clone()));
                        set(key, value, window, cx)
                    },
                    search(),
                )
                .variant(variant),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn inline_filters_widen_their_columns_and_sit_below_the_label(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        1200.,
        filter_table(TableFilterVariant::Inline, Changes::default()),
    );
    let slot = bounds(cx, "table-filter-name-inline");
    assert!(slot.top() > bounds(cx, "t-header-name").top());
    // The filtered column without a width takes a proportional one; the
    // pixel column keeps its 80.
    assert_eq!(f32::from(bounds(cx, "t-header-age").size.width), 80.);
    // A date filter's control is a DateInput.
    assert!(shown(cx, "table-filter-joined-inline"));
    // Every column gets a slot: without a filter, upstream's 32 px
    // placeholder (28 px when compact).
    assert_eq!(
        f32::from(bounds(cx, "table-filter-plain-inline").size.height),
        32.
    );
}

#[gpui::test]
fn the_compact_placeholder_is_shorter(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        1200.,
        filter_table(TableFilterVariant::InlineCompact, Changes::default()),
    );
    assert_eq!(
        f32::from(bounds(cx, "table-filter-plain-inline").size.height),
        28.
    );
}

#[gpui::test]
fn typing_in_an_inline_filter_changes_it_live(cx: &mut TestAppContext) {
    let changes = Changes::default();
    let cx = host(
        cx,
        1200.,
        filter_table(TableFilterVariant::Inline, changes.clone()),
    );
    click(cx, "table-filter-name-inline");
    cx.simulate_input("ad");
    settle(cx);
    assert_eq!(
        changes.borrow().last(),
        Some(&("name".into(), Some("ad".into())))
    );
}

#[gpui::test]
fn a_date_filter_takes_a_typed_date_as_iso(cx: &mut TestAppContext) {
    cx.update(rustryx_primitives::editing::init);
    rustryx_i18n::date::set_today(Some(
        rustryx_i18n::date::PlainDate::new(2026, 11, 25).unwrap(),
    ));
    let changes = Changes::default();
    let cx = host(
        cx,
        1200.,
        filter_table(TableFilterVariant::Inline, changes.clone()),
    );
    click(cx, "table-filter-joined-inline");
    cx.simulate_input("11/20/2026");
    settle(cx);
    assert_eq!(
        changes.borrow().last(),
        Some(&("joined".into(), Some("2026-11-20".into())))
    );
}

#[gpui::test]
fn the_popover_applies_its_draft_and_resets(cx: &mut TestAppContext) {
    let changes = Changes::default();
    let cx = host(
        cx,
        1200.,
        filter_table(TableFilterVariant::Popover, changes.clone()),
    );
    assert!(shown(cx, "table-filter-name-trigger"));
    assert!(!shown(cx, "table-filter-plain-trigger"));
    click(cx, "table-filter-name-trigger");
    assert!(shown(cx, "table-filter-name-apply"));
    // Focus enters the control; typing changes only the draft.
    cx.simulate_input("grace");
    settle(cx);
    assert!(changes.borrow().is_empty(), "nothing applies before Apply");
    click(cx, "table-filter-name-apply");
    assert_eq!(*changes.borrow(), [("name".into(), Some("grace".into()))]);
    assert!(!shown(cx, "table-filter-name-apply"), "it closes");
    click(cx, "table-filter-name-trigger");
    click(cx, "table-filter-name-reset");
    assert_eq!(changes.borrow().last(), Some(&("name".into(), None)));
}

// ---- Infinite scroll -------------------------------------------------------

/// The caller's side: rows, flags and the batches asked for.
#[derive(Clone, Default)]
struct Feed {
    calls: Rc<Cell<usize>>,
    rows: Rc<Cell<usize>>,
    loading: Rc<Cell<bool>>,
    more: Rc<Cell<bool>>,
    fail: Rc<Cell<bool>>,
}

const BATCH: Duration = Duration::from_millis(100);

fn feed_table(
    feed: Feed,
    spacer: f32,
    composed: bool,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyElement {
    move |window, cx| {
        let f = feed.clone();
        let plugin = use_table_infinite_scroll(
            "feed",
            feed.more.get(),
            feed.loading.get(),
            move |_, cx| -> Task<Result<(), ()>> {
                f.calls.set(f.calls.get() + 1);
                let fail = f.fail.get();
                let timer = cx.background_executor().timer(BATCH);
                cx.spawn(async move |_| {
                    timer.await;
                    if fail { Err(()) } else { Ok(()) }
                })
            },
            window,
            cx,
        );
        let table = if composed {
            Table::composed("t")
                .plugin("infiniteScroll", plugin)
                .child(TableBody::new())
        } else {
            let data: Vec<Record> = (0..feed.rows.get())
                .map(|i| {
                    Record::new()
                        .with("id", i as i64)
                        .with("name", format!("Row {i}"))
                })
                .collect();
            Table::new("t")
                .data(data)
                .columns([TableColumn::new("name")])
                .id_key("id")
                .empty_state(div().debug_selector(|| "custom-empty".into()).child("None"))
                .plugin("infiniteScroll", plugin)
        };
        div()
            .child(div().h(px(spacer)))
            .child(table)
            .into_any_element()
    }
}

fn feed(rows: usize) -> Feed {
    let feed = Feed::default();
    feed.rows.set(rows);
    feed.more.set(true);
    feed
}

fn finish_batch(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(BATCH);
    settle(cx);
}

#[gpui::test]
fn fr3_fr5_a_visible_sentinel_loads_one_batch_at_a_time(cx: &mut TestAppContext) {
    let f = feed(2);
    let cx = host(cx, 600., feed_table(f.clone(), 0., false));
    assert!(shown(cx, "table-load-more-sentinel"));
    assert_eq!(f.calls.get(), 1, "the sentinel in view asks once");
    settle(cx);
    assert_eq!(f.calls.get(), 1, "the latch holds while it loads");
    // FR5: done, still in view: the next batch, with no scroll needed.
    finish_batch(cx);
    assert_eq!(f.calls.get(), 2);
}

#[gpui::test]
fn fr4_completion_waits_for_the_callers_loading_flag(cx: &mut TestAppContext) {
    let f = feed(2);
    f.loading.set(true);
    let cx = host(cx, 600., feed_table(f.clone(), 0., false));
    assert_eq!(f.calls.get(), 0, "never while the caller loads");
    f.loading.set(false);
    settle(cx);
    assert_eq!(f.calls.get(), 1);
    f.loading.set(true);
    finish_batch(cx);
    assert_eq!(f.calls.get(), 1, "settled, but the caller still loads");
    f.loading.set(false);
    settle(cx);
    assert_eq!(f.calls.get(), 2);
}

#[gpui::test]
fn fr4_a_failure_pauses_automatic_loading_not_the_button(cx: &mut TestAppContext) {
    let f = feed(2);
    f.fail.set(true);
    let cx = host(cx, 600., feed_table(f.clone(), 0., false));
    assert_eq!(f.calls.get(), 1);
    finish_batch(cx);
    finish_batch(cx);
    assert_eq!(f.calls.get(), 1, "paused after the failure");
    f.fail.set(false);
    click(cx, "feed-load-more-button");
    assert_eq!(f.calls.get(), 2, "the button still loads");
    // Its success resumes automatic loading.
    finish_batch(cx);
    assert_eq!(f.calls.get(), 3);
}

#[gpui::test]
fn an_offscreen_sentinel_waits(cx: &mut TestAppContext) {
    let f = feed(2);
    let cx = host(cx, 600., feed_table(f.clone(), 5000., false));
    assert!(shown(cx, "feed-load-more-button"), "the command is there");
    assert_eq!(f.calls.get(), 0);
}

#[gpui::test]
fn fr6_exhaustion_removes_the_command_and_keeps_focus(cx: &mut TestAppContext) {
    let f = feed(2);
    f.loading.set(true);
    let cx = host(cx, 600., feed_table(f.clone(), 0., false));
    // Focus the command (it is busy, so the press loads nothing).
    click(cx, "feed-load-more-button");
    assert_eq!(f.calls.get(), 0);
    f.more.set(false);
    f.loading.set(false);
    settle(cx);
    assert!(!shown(cx, "feed-load-more-button") && !shown(cx, "table-load-more-sentinel"));
    let focused = cx.update(|window, cx| window.focused(cx).is_some());
    assert!(focused, "focus went to the table's region, not nowhere");
}

#[gpui::test]
fn fr10_an_empty_table_loads_without_the_default_message(cx: &mut TestAppContext) {
    let f = feed(0);
    let cx = host(cx, 600., move |window, cx| {
        let plugin = use_table_infinite_scroll(
            "feed",
            f.more.get(),
            true,
            |_, _| -> Task<Result<(), ()>> { Task::ready(Ok(())) },
            window,
            cx,
        );
        Table::new("t")
            .data(Vec::<Record>::new())
            .columns([TableColumn::new("name")])
            .plugin("infiniteScroll", plugin)
            .into_any_element()
    });
    // No "No data" while the first rows load; the command shows.
    assert!(shown(cx, "feed-load-more-button"));
    assert!(!shown(cx, "t-empty"));
}

#[gpui::test]
fn fr10_caller_empty_content_still_shows_while_loading(cx: &mut TestAppContext) {
    let f = feed(0);
    f.loading.set(true);
    let cx = host(cx, 600., feed_table(f, 0., false));
    assert!(shown(cx, "custom-empty"));
}

#[gpui::test]
fn fr13_children_mode_gets_no_surface(cx: &mut TestAppContext) {
    let f = feed(0);
    let cx = host(cx, 600., feed_table(f.clone(), 0., true));
    assert!(!shown(cx, "table-load-more-sentinel"));
    assert_eq!(f.calls.get(), 0);
}
