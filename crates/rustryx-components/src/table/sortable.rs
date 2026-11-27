//! Sorting (upstream `useTableSortable` and `useTableSortableState`).
//!
//! [`TableSortable`] is the headless plugin: a sort control around each
//! sortable column's label, its indicator, the multi-sort priority and the
//! header's sort actions; it reports the next sort state and reorders
//! nothing. [`use_table_sortable_state`] owns that state (or follows a
//! controlled one) and sorts rows locally with the locale's numeric collator.

use std::cmp::Ordering;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, ElementId, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use rustryx_i18n::{Arg, I18n, current as i18n};
use rustryx_primitives::{control_focus, is_focus_visible};
use rustryx_theme::current;

use super::model::{TableColumn, TableColumnAlign, TableRecord, TableValue, iso_date, js_number};
use super::plugin::{HeaderCellProps, TableContextAction, TableContextActions, TablePlugin};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{Icon, IconColor, IconSize, TABLE_SORT_INDICATOR, component_icon};

/// A sort direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            SortDirection::Ascending => "ascending",
            SortDirection::Descending => "descending",
        }
    }
}

/// One key of the sort state: rows sort by it in its direction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortEntry {
    pub sort_key: SharedString,
    pub direction: SortDirection,
}

impl SortEntry {
    pub fn ascending(key: impl Into<SharedString>) -> Self {
        Self {
            sort_key: key.into(),
            direction: SortDirection::Ascending,
        }
    }

    pub fn descending(key: impl Into<SharedString>) -> Self {
        Self {
            sort_key: key.into(),
            direction: SortDirection::Descending,
        }
    }
}

type SortFn = Rc<dyn Fn(Vec<SortEntry>, &mut Window, &mut App)>;

/// The next direction a header's press gives: ascending, descending, then
/// unsorted (or ascending again without an unsorted state).
pub fn next_direction(
    current: Option<SortDirection>,
    allow_unsorted: bool,
) -> Option<SortDirection> {
    match current {
        None => Some(SortDirection::Ascending),
        Some(SortDirection::Ascending) => Some(SortDirection::Descending),
        Some(SortDirection::Descending) => (!allow_unsorted).then_some(SortDirection::Ascending),
    }
}

/// The sort state after a press on `key`'s header: a press replaces the
/// state; with multi-sort, a Shift press steps that key in place, or adds it
/// last.
pub fn next_sort(
    sort: &[SortEntry],
    key: &str,
    multi: bool,
    allow_unsorted: bool,
) -> Vec<SortEntry> {
    let at = sort.iter().position(|e| e.sort_key.as_ref() == key);
    if multi {
        let mut next = sort.to_vec();
        match at {
            Some(i) => match next_direction(Some(sort[i].direction), allow_unsorted) {
                Some(d) => next[i].direction = d,
                None => {
                    next.remove(i);
                }
            },
            None => next.push(SortEntry::ascending(key.to_string())),
        }
        next
    } else {
        match next_direction(at.map(|i| sort[i].direction), allow_unsorted) {
            Some(d) => vec![SortEntry {
                sort_key: key.to_string().into(),
                direction: d,
            }],
            None => Vec::new(),
        }
    }
}

/// The sort plugin (upstream `useTableSortable`): it shows `sort` and
/// reports the next state; sort the rows from it.
#[derive(Clone)]
pub struct TableSortable {
    sort: Rc<Vec<SortEntry>>,
    on_sort_change: SortFn,
    allow_unsorted: bool,
    multi_sort: bool,
}

impl TableSortable {
    pub fn new(
        sort: Vec<SortEntry>,
        on_sort_change: impl Fn(Vec<SortEntry>, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            sort: Rc::new(sort),
            on_sort_change: Rc::new(on_sort_change),
            allow_unsorted: true,
            multi_sort: false,
        }
    }

    /// Upstream `allowUnsortedState`: a third press returns to unsorted (on
    /// by default).
    pub fn allow_unsorted_state(mut self, allow: bool) -> Self {
        self.allow_unsorted = allow;
        self
    }

    /// Upstream `isMultiSortEnabled`: Shift+press adds a key.
    pub fn multi_sort(mut self, multi: bool) -> Self {
        self.multi_sort = multi;
        self
    }

    pub fn sort(&self) -> &[SortEntry] {
        &self.sort
    }
}

impl<T: TableRecord> TablePlugin<T> for TableSortable {
    fn transform_header_cell(
        &self,
        mut props: HeaderCellProps,
        column: &TableColumn<T>,
        _: &[TableColumn<T>],
        window: &mut Window,
        cx: &mut App,
    ) -> HeaderCellProps {
        let Some(key) = column.sort_key_or_key() else {
            return props;
        };
        let at = self.sort.iter().position(|e| e.sort_key == key);
        let direction = at.map(|i| self.sort[i].direction);
        if let Some(d) = direction {
            props.hooks.push(Box::new(move |el| {
                el.aria_sort(match d {
                    SortDirection::Ascending => gpui::SortDirection::Ascending,
                    SortDirection::Descending => gpui::SortDirection::Descending,
                })
            }));
        }
        // The cell is the group whose hover lights the unsorted glyph.
        let group: SharedString = format!("table-sort-{key}").into();
        let g = group.clone();
        props.hooks.push(Box::new(move |el| el.group(g)));
        let rank = (self.multi_sort && self.sort.len() > 1)
            .then(|| at.map(|i| i + 1))
            .flatten();
        let label = props
            .content
            .take()
            .unwrap_or_else(|| key.clone().into_any_element());
        props.content = Some(sort_control(
            self, column, &key, direction, rank, group, label, window, cx,
        ));
        // The header's sort actions, read when its menu opens.
        let (sort, on_change, k) = (self.sort.clone(), self.on_sort_change.clone(), key.clone());
        props
            .context_menu
            .push(TableContextActions::lazy(move |cx| {
                let t = i18n(cx);
                let dir = sort.iter().find(|e| e.sort_key == k).map(|e| e.direction);
                let set = |d: SortDirection| {
                    let (on_change, k) = (on_change.clone(), k.clone());
                    move |window: &mut Window, cx: &mut App| {
                        on_change(
                            vec![SortEntry {
                                sort_key: k.clone(),
                                direction: d,
                            }],
                            window,
                            cx,
                        )
                    }
                };
                let mut actions = vec![
                    TableContextAction::new(
                        "sort-asc",
                        t.t("@astryx.table.sort.ascending"),
                        set(SortDirection::Ascending),
                    )
                    .group("sort")
                    .icon("arrowUp")
                    .checked(dir == Some(SortDirection::Ascending)),
                    TableContextAction::new(
                        "sort-desc",
                        t.t("@astryx.table.sort.descending"),
                        set(SortDirection::Descending),
                    )
                    .group("sort")
                    .icon("arrowDown")
                    .checked(dir == Some(SortDirection::Descending)),
                ];
                if dir.is_some() {
                    let (sort, on_change, k) = (sort.clone(), on_change.clone(), k.clone());
                    actions.push(
                        TableContextAction::new(
                            "sort-clear",
                            t.t("@astryx.table.sort.clear"),
                            move |window, cx| {
                                on_change(
                                    sort.iter().filter(|e| e.sort_key != k).cloned().collect(),
                                    window,
                                    cx,
                                )
                            },
                        )
                        .group("sort-clear")
                        .icon("close"),
                    );
                }
                actions
            }));
        props
    }
}

/// The sort control around a column's label: the label, the indicator and,
/// in a multi-sort, the column's priority. Named by its column's text; the
/// direction is the header's `aria-sort`, the priority its description.
#[allow(clippy::too_many_arguments)]
fn sort_control<T>(
    plugin: &TableSortable,
    column: &TableColumn<T>,
    key: &SharedString,
    direction: Option<SortDirection>,
    rank: Option<usize>,
    group: SharedString,
    label: AnyElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let id = ElementId::Name(format!("table-sort-{key}").into());
    let focus = control_focus(&id, window, cx);
    let focused = is_focus_visible(&focus, window);
    let state = direction.map_or("none", SortDirection::as_str);
    let fallback = match direction {
        Some(SortDirection::Ascending) => "arrowUp",
        Some(SortDirection::Descending) => "arrowDown",
        None => "arrowsUpDown",
    };
    let icon = component_icon(TABLE_SORT_INDICATOR, state, fallback.into(), cx).map(|source| {
        let wrapper = div().flex().flex_none().items_center().h(px(0.)).child(
            Icon::new(source)
                .size(IconSize::Xsm)
                .color(if direction.is_some() {
                    IconColor::Accent
                } else {
                    IconColor::Secondary
                }),
        );
        if direction.is_none() && !focused {
            wrapper.opacity(0.35).group_hover(group, |s| s.opacity(1.))
        } else {
            wrapper
        }
    });
    let rank_selector = format!("table-sort-{key}-rank");
    let rank_el = rank.map(|r| {
        div()
            .debug_selector(move || rank_selector.clone())
            .flex_none()
            .text_size(px(10.))
            .line_height(px(10.))
            .text_color(t.color.text_accent)
            .child(r.to_string())
    });
    let name = column.header_text().unwrap_or_else(|| key.clone());
    let total = plugin.sort.len();
    let mut el = div()
        .id(id)
        .track_focus(&focus)
        .relative()
        .flex()
        .items_center()
        .gap(t.spacing.s1.to_pixels(rem))
        .w_full()
        .min_w_0()
        .rounded(t.radius.inner.to_pixels(rem))
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(name);
    el = match column.align.unwrap_or_default() {
        TableColumnAlign::Start => el,
        TableColumnAlign::Center => el.justify_center(),
        TableColumnAlign::End => el.justify_end(),
    };
    if let Some(r) = rank {
        el = el.aria_description(i18n(cx).format(
            "@astryx.table.sort.priority",
            &[
                ("rank", Arg::Num(r as f64)),
                ("total", Arg::Num(total as f64)),
            ],
        ));
    }
    if focused {
        el = el.child(crate::focus::focus_ring(
            t,
            t.radius.inner.to_pixels(rem),
            rem,
        ));
    }
    let (sort, on_change, k, multi, allow) = (
        plugin.sort.clone(),
        plugin.on_sort_change.clone(),
        key.clone(),
        plugin.multi_sort,
        plugin.allow_unsorted,
    );
    el.child(
        div()
            .min_w_0()
            .overflow_hidden()
            .text_ellipsis()
            .child(label),
    )
    .children(icon)
    .children(rank_el)
    .on_click(move |event, window, cx| {
        cx.stop_propagation();
        let shift = event.modifiers().shift || window.modifiers().shift;
        on_change(next_sort(&sort, &k, shift && multi, allow), window, cx);
    })
    .into_any_element()
}

// ---- useTableSortableState -------------------------------------------------

type Comparator<T> = Rc<dyn Fn(&T, &T) -> Ordering>;

/// What [`use_table_sortable_state`] takes (upstream
/// `UseTableSortableStateConfig`).
pub struct SortableStateConfig<T> {
    data: Rc<[T]>,
    default_sort: Vec<SortEntry>,
    controlled: Option<(Vec<SortEntry>, SortFn)>,
    comparators: Vec<(SharedString, Comparator<T>)>,
    allow_unsorted: bool,
    multi_sort: bool,
}

impl<T> SortableStateConfig<T> {
    /// The rows to sort; they are copied, never changed.
    pub fn new(data: impl Into<Rc<[T]>>) -> Self {
        Self {
            data: data.into(),
            default_sort: Vec::new(),
            controlled: None,
            comparators: Vec::new(),
            allow_unsorted: true,
            multi_sort: false,
        }
    }

    /// Upstream `defaultSort`: the hook's own state to start.
    pub fn default_sort(mut self, sort: Vec<SortEntry>) -> Self {
        self.default_sort = sort;
        self
    }

    /// Upstream `sort` with `onSortChange`: a state the caller owns.
    pub fn sort(
        mut self,
        sort: Vec<SortEntry>,
        on_sort_change: impl Fn(Vec<SortEntry>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.controlled = Some((sort, Rc::new(on_sort_change)));
        self
    }

    /// Upstream `comparators`: an ascending order for one sort key (the
    /// hook flips it for descending).
    pub fn comparator(
        mut self,
        key: impl Into<SharedString>,
        compare: impl Fn(&T, &T) -> Ordering + 'static,
    ) -> Self {
        self.comparators.push((key.into(), Rc::new(compare)));
        self
    }

    pub fn allow_unsorted_state(mut self, allow: bool) -> Self {
        self.allow_unsorted = allow;
        self
    }

    pub fn multi_sort(mut self, multi: bool) -> Self {
        self.multi_sort = multi;
        self
    }
}

/// The state's result (upstream `UseTableSortableStateResult`).
pub struct SortableState<T> {
    /// A sorted copy of the rows.
    pub sorted_data: Rc<[T]>,
    pub sort: Vec<SortEntry>,
    /// The plugin, ready to pass to the table (upstream `sortConfig`).
    pub plugin: TableSortable,
    comparators: Vec<(SharedString, Comparator<T>)>,
    i18n: I18n,
}

impl<T: TableRecord + Clone> SortableState<T> {
    /// The current sort applied to other rows.
    pub fn apply_sort(&self, data: &[T]) -> Vec<T> {
        sort_rows(data, &self.sort, &self.comparators, &self.i18n)
    }
}

fn is_missing(v: &TableValue) -> bool {
    match v {
        TableValue::Number(n) => n.is_nan(),
        other => other.is_missing(),
    }
}

fn sortable_text(v: &TableValue) -> String {
    match v {
        TableValue::Text(t) => t.to_string(),
        TableValue::Number(n) => js_number(*n),
        TableValue::Bool(b) => b.to_string(),
        TableValue::Date(Some(ms)) => iso_date(*ms),
        _ => String::new(),
    }
}

/// The default order of one key (upstream `defaultCompare`): missing values
/// after the rest, numbers by value, everything else by the locale's
/// numeric collation of its text.
pub(crate) fn default_compare(a: &TableValue, b: &TableValue, i18n: &I18n) -> Ordering {
    match (is_missing(a), is_missing(b)) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    if let (TableValue::Number(x), TableValue::Number(y)) = (a, b) {
        return x.partial_cmp(y).unwrap_or(Ordering::Equal);
    }
    i18n.compare(&sortable_text(a), &sortable_text(b))
}

/// Rows sorted by every key of `sort` in turn (a stable sort).
pub(crate) fn sort_rows<T: TableRecord + Clone>(
    data: &[T],
    sort: &[SortEntry],
    comparators: &[(SharedString, Comparator<T>)],
    i18n: &I18n,
) -> Vec<T> {
    let mut rows = data.to_vec();
    if sort.is_empty() {
        return rows;
    }
    rows.sort_by(|a, b| {
        for entry in sort {
            let cmp = match comparators.iter().find(|(k, _)| *k == entry.sort_key) {
                Some((_, f)) => f(a, b),
                None => default_compare(&a.value(&entry.sort_key), &b.value(&entry.sort_key), i18n),
            };
            if cmp != Ordering::Equal {
                return if entry.direction == SortDirection::Ascending {
                    cmp
                } else {
                    cmp.reverse()
                };
            }
        }
        Ordering::Equal
    });
    rows
}

/// A table's sort state with its rows sorted locally (upstream
/// `useTableSortableState`): its own state, or the caller's.
pub fn use_table_sortable_state<T: TableRecord + Clone>(
    id: impl Into<ElementId>,
    config: SortableStateConfig<T>,
    window: &mut Window,
    cx: &mut App,
) -> SortableState<T> {
    let id = id.into();
    let default = config.default_sort.clone();
    let own = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-sort").into()),
        cx,
        move |_, _: &mut Context<Vec<SortEntry>>| default,
    );
    let (sort, on_change): (Vec<SortEntry>, SortFn) = match config.controlled {
        Some((sort, f)) => (sort, f),
        None => {
            let own = own.clone();
            (
                own.read(cx).clone(),
                Rc::new(move |next, window, cx| {
                    own.update(cx, |s, _| *s = next);
                    window.refresh();
                }),
            )
        }
    };
    let i18n = i18n(cx);
    let sorted: Rc<[T]> = sort_rows(&config.data, &sort, &config.comparators, &i18n).into();
    let handler = on_change.clone();
    let plugin = TableSortable::new(sort.clone(), move |next, window, cx| {
        handler(next, window, cx)
    })
    .allow_unsorted_state(config.allow_unsorted)
    .multi_sort(config.multi_sort);
    SortableState {
        sorted_data: sorted,
        sort,
        plugin,
        comparators: config.comparators,
        i18n,
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_SORTABLE_DOC: ComponentDoc = ComponentDoc {
    name: "useTableSortable",
    rust_type: "rustryx::components::table::TableSortable",
    props: &[
        p("sort", RustApi::Argument("sort")),
        p("onSortChange", RustApi::Argument("on_sort_change")),
        p(
            "allowUnsortedState",
            RustApi::Method("allow_unsorted_state"),
        ),
        p("isMultiSortEnabled", RustApi::Method("multi_sort")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_TABLE_SORTABLE_STATE_DOC: ComponentDoc = ComponentDoc {
    name: "useTableSortableState",
    rust_type: "rustryx::components::table::use_table_sortable_state",
    props: &[
        p("config", RustApi::Argument("SortableStateConfig")),
        p(
            "config.data",
            RustApi::Argument("SortableStateConfig::new(data)"),
        ),
        p("config.defaultSort", RustApi::Method("default_sort")),
        p("config.sort", RustApi::Method("sort(sort, on_sort_change)")),
        p(
            "config.onSortChange",
            RustApi::Method("sort(sort, on_sort_change)"),
        ),
        p("config.comparators", RustApi::Method("comparator")),
        p(
            "config.allowUnsortedState",
            RustApi::Method("allow_unsorted_state"),
        ),
        p("config.isMultiSortEnabled", RustApi::Method("multi_sort")),
    ],
    example: include_str!("examples.rs"),
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::Record;

    #[test]
    fn a_press_cycles_ascending_descending_unsorted() {
        let s0: Vec<SortEntry> = Vec::new();
        let s1 = next_sort(&s0, "name", false, true);
        assert_eq!(s1, [SortEntry::ascending("name")]);
        let s2 = next_sort(&s1, "name", false, true);
        assert_eq!(s2, [SortEntry::descending("name")]);
        assert!(next_sort(&s2, "name", false, true).is_empty());
        assert_eq!(
            next_sort(&s2, "name", false, false),
            [SortEntry::ascending("name")]
        );
        // Another column replaces the state.
        assert_eq!(
            next_sort(&s2, "age", false, true),
            [SortEntry::ascending("age")]
        );
    }

    #[test]
    fn shift_adds_keys_and_steps_them_in_place() {
        let s = vec![SortEntry::ascending("name")];
        let s = next_sort(&s, "age", true, true);
        assert_eq!(
            s,
            [SortEntry::ascending("name"), SortEntry::ascending("age")]
        );
        let s = next_sort(&s, "name", true, true);
        assert_eq!(
            s,
            [SortEntry::descending("name"), SortEntry::ascending("age")]
        );
        let s = next_sort(&s, "name", true, true);
        assert_eq!(s, [SortEntry::ascending("age")]);
    }

    #[test]
    fn rows_sort_by_value_with_missing_values_last_ascending() {
        let i18n = I18n::new("en");
        let rows = vec![
            Record::new().with("n", "item 10").with("v", 3),
            Record::new()
                .with("n", "item 2")
                .with("v", TableValue::Null),
            Record::new().with("n", "Item 1").with("v", 1),
        ];
        let by = |key: &str, dir: SortDirection| {
            sort_rows(
                &rows,
                &[SortEntry {
                    sort_key: key.to_string().into(),
                    direction: dir,
                }],
                &[],
                &i18n,
            )
            .iter()
            .map(|r| r.value("n").display_text().to_string())
            .collect::<Vec<_>>()
        };
        // Digits by their value, case after base letter.
        assert_eq!(
            by("n", SortDirection::Ascending),
            ["Item 1", "item 2", "item 10"]
        );
        assert_eq!(
            by("v", SortDirection::Ascending),
            ["Item 1", "item 10", "item 2"]
        );
        // Descending flips the whole order, missing values first (upstream).
        assert_eq!(
            by("v", SortDirection::Descending),
            ["item 2", "item 10", "Item 1"]
        );
    }

    #[test]
    fn a_comparator_orders_its_key_and_ties_fall_to_the_next() {
        let i18n = I18n::new("en");
        let rows = vec![
            Record::new().with("s", "low").with("n", "b"),
            Record::new().with("s", "high").with("n", "a"),
            Record::new().with("s", "low").with("n", "a"),
        ];
        let rank = |r: &Record| match r.value("s").display_text().as_ref() {
            "high" => 0,
            _ => 1,
        };
        let cmp: Comparator<Record> = Rc::new(move |a, b| rank(a).cmp(&rank(b)));
        let sorted = sort_rows(
            &rows,
            &[SortEntry::ascending("s"), SortEntry::ascending("n")],
            &[("s".into(), cmp)],
            &i18n,
        );
        let names: Vec<String> = sorted
            .iter()
            .map(|r| {
                format!(
                    "{}{}",
                    r.value("s").display_text(),
                    r.value("n").display_text()
                )
            })
            .collect();
        assert_eq!(names, ["higha", "lowa", "lowb"]);
    }
}
