//! Column filtering (upstream `useTableFiltering`, `useTableFilterState` and
//! `toSearchFilters`): a column's `filter` names a field of a PowerSearch
//! config, and the plugin renders that operator's control in the header, in
//! a popover behind a funnel button or inline below the label.
//!
//! The `date_absolute` and `time` controls are DateInput and TimeInput, whose
//! ISO values the filter keeps; the other operators have no control.

use std::collections::HashMap;
use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::{ElementSize, LayerAlign, Placement, control_focus, is_focus_visible};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, HasPopup, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, Role, SharedString, Styled, Window, div, prelude::*, px,
};

use super::model::{TableColumn, TableFilterRef, proportional};
use super::plugin::{HeaderCellProps, TablePlugin};
use crate::button::{Button, ButtonVariant};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{Icon, IconColor, IconSize};
use crate::input::{NumberInput, TextInput};
use crate::multi_selector::MultiSelector;
use crate::overlay::{Popover, PopoverPresentation};
use crate::power_search::{
    FilterValue, OperatorValue, PowerSearchConfig, PowerSearchEntity, PowerSearchField,
    PowerSearchFilter, PowerSearchOperator,
};
use crate::search::{SearchAnswer, SearchContext, SearchItem, SearchSource, Searchable};
use crate::selector::{Selector, SelectorEntry, SelectorOptionData};
use crate::tokenizer::Tokenizer;

/// A column's filter value (upstream `TableFilterValue`): text, a number,
/// or a list.
#[derive(Clone, Debug, PartialEq)]
pub enum TableFilterValue {
    Text(SharedString),
    Number(f64),
    List(Vec<SharedString>),
}

impl From<&str> for TableFilterValue {
    fn from(text: &str) -> Self {
        TableFilterValue::Text(text.to_string().into())
    }
}

impl From<SharedString> for TableFilterValue {
    fn from(text: SharedString) -> Self {
        TableFilterValue::Text(text)
    }
}

impl From<f64> for TableFilterValue {
    fn from(n: f64) -> Self {
        TableFilterValue::Number(n)
    }
}

/// Every column's filter (upstream `TableFilterState`): a missing key is no
/// filter.
pub type TableFilterState = HashMap<SharedString, TableFilterValue>;

/// Where the controls show (upstream `TableFilterVariant`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableFilterVariant {
    /// A funnel button in the header opens the control in a popover.
    #[default]
    Popover,
    /// The control below the header's label.
    Inline,
    /// Inline, with a shorter placeholder for unfiltered columns.
    InlineCompact,
}

type ChangeFn = Rc<dyn Fn(SharedString, Option<TableFilterValue>, &mut Window, &mut App)>;
type ClearFn = Rc<dyn Fn(&mut Window, &mut App)>;
type ValueFn = Rc<dyn Fn(Option<TableFilterValue>, &mut Window, &mut App)>;

// ----------------------------------------------------------------------------
// useTableFilterState

/// Upstream `useTableFilterState`'s result.
#[derive(Clone)]
pub struct TableFilterStateHandle {
    /// The current filters, for the plugin.
    pub filters: TableFilterState,
    on_change: ChangeFn,
    clear: ClearFn,
}

impl TableFilterStateHandle {
    /// Upstream `onFilterChange`: `None` removes the column's filter.
    pub fn on_filter_change(
        &self,
    ) -> impl Fn(SharedString, Option<TableFilterValue>, &mut Window, &mut App) + 'static {
        let f = self.on_change.clone();
        move |key, value, window, cx| f(key, value, window, cx)
    }

    /// Upstream `clearAll`: every filter off.
    pub fn clear_all(&self, window: &mut Window, cx: &mut App) {
        (self.clear)(window, cx)
    }
}

/// Upstream `useTableFilterState`: the filters kept under `id`, starting
/// from `initial`.
pub fn use_table_filter_state(
    id: impl Into<ElementId>,
    initial: Option<TableFilterState>,
    window: &mut Window,
    cx: &mut App,
) -> TableFilterStateHandle {
    let id = id.into();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-filters").into()),
        cx,
        move |_, _| initial.unwrap_or_default(),
    );
    let filters = state.read(cx).clone();
    let (a, b) = (state.clone(), state);
    TableFilterStateHandle {
        filters,
        on_change: Rc::new(move |key, value, window, cx| {
            a.update(cx, |filters, _| match value {
                Some(v) => {
                    filters.insert(key, v);
                }
                None => {
                    filters.remove(&key);
                }
            });
            window.refresh();
        }),
        clear: Rc::new(move |window, cx| {
            b.update(cx, |filters, _| filters.clear());
            window.refresh();
        }),
    }
}

// ----------------------------------------------------------------------------
// PowerSearch resolution

/// Upstream `resolveOperator`: the named operator, else the field's
/// default, else its first.
fn resolve_operator<'a>(
    field: &'a PowerSearchField,
    operator: Option<&str>,
) -> Option<&'a PowerSearchOperator> {
    match operator {
        Some(key) => field.operators.iter().find(|o| o.key.as_ref() == key),
        None => match &field.default_operator {
            Some(key) => field.operators.iter().find(|o| o.key == *key),
            None => field.operators.first(),
        },
    }
}

fn filter_parts(filter: &TableFilterRef) -> (&SharedString, Option<&str>) {
    match filter {
        TableFilterRef::Field(field) => (field, None),
        TableFilterRef::FieldOperator { field, operator } => (field, Some(operator.as_ref())),
    }
}

/// Upstream `resolveFilterConfig`: the operator value a column's filter
/// points at.
fn resolve_filter<'a>(
    filter: &TableFilterRef,
    config: &'a PowerSearchConfig,
) -> Option<&'a PowerSearchOperator> {
    let (field, operator) = filter_parts(filter);
    let field = config.find_field(field)?;
    resolve_operator(field, operator)
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `new Date(value).getTime() / 1000` for an ISO date (`YYYY-MM-DD`, read as
/// UTC midnight) or date-time (`YYYY-MM-DDTHH:MM[:SS]`, read as UTC).
fn iso_unix_seconds(value: &str) -> Option<i64> {
    let (date, time) = value.split_once('T').unwrap_or((value, ""));
    let mut parts = date.splitn(3, '-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let time = time.trim_end_matches('Z');
    let mut hms = time.split(':').filter(|s| !s.is_empty());
    let h: i64 = hms.next().map_or(Some(0), |s| s.parse().ok())?;
    let min: i64 = hms.next().map_or(Some(0), |s| s.parse().ok())?;
    let s: f64 = hms.next().map_or(Some(0.), |s| s.parse().ok())?;
    Some(days_from_civil(y, m, d) * 86_400 + h * 3_600 + min * 60 + s.floor() as i64)
}

/// Upstream `tableValueToFilterValue`: the PowerSearch value of a table
/// filter value, when the operator takes that kind.
fn to_filter_value(value: &TableFilterValue, op: &OperatorValue) -> Option<FilterValue> {
    use TableFilterValue as V;
    match (op, value) {
        (OperatorValue::String { .. }, V::Text(t)) => Some(FilterValue::String(t.clone())),
        (OperatorValue::Integer { .. }, V::Number(n)) => Some(FilterValue::Integer(*n as i64)),
        (OperatorValue::Float { .. }, V::Number(n)) => Some(FilterValue::Float(*n)),
        (OperatorValue::Enum { .. }, V::Text(t)) => Some(FilterValue::Enum(t.clone())),
        (OperatorValue::EnumList { .. }, V::List(l)) => Some(FilterValue::EnumList(l.clone())),
        (OperatorValue::DateAbsolute { .. }, V::Text(t)) => {
            iso_unix_seconds(t).map(|unix_seconds| FilterValue::DateAbsolute { unix_seconds })
        }
        (OperatorValue::Time { .. }, V::Text(t)) => Some(FilterValue::Time(t.clone())),
        (OperatorValue::StringList { .. }, V::List(l)) => Some(FilterValue::StringList(l.clone())),
        (OperatorValue::EntityList { .. }, V::List(l)) => Some(FilterValue::EntityList(
            l.iter()
                .map(|id| PowerSearchEntity {
                    id: id.clone(),
                    label: id.clone(),
                    photo: None,
                })
                .collect(),
        )),
        _ => None,
    }
}

/// Upstream `toSearchFilters`: the filters as PowerSearch filters, for
/// `apply_filters` (define filters once, apply them everywhere).
pub fn to_search_filters<T>(
    filters: &TableFilterState,
    columns: &[TableColumn<T>],
    config: &PowerSearchConfig,
) -> Vec<PowerSearchFilter> {
    columns
        .iter()
        .filter_map(|col| {
            let filter = col.filter.as_ref()?;
            let value = filters.get(&col.key)?;
            let (field, _) = filter_parts(filter);
            let operator = resolve_filter(filter, config)?;
            let value = to_filter_value(value, &operator.value)?;
            Some(PowerSearchFilter::new(
                field.clone(),
                operator.key.clone(),
                value,
            ))
        })
        .collect()
}

// ----------------------------------------------------------------------------
// The controls

/// A source that offers what was typed as a new value (upstream's fallback
/// for a list without its own search source).
struct TypedSource;

impl SearchSource<SearchItem> for TypedSource {
    fn search(
        &self,
        query: &str,
        _: &SearchContext<SearchItem>,
        _: &mut App,
    ) -> SearchAnswer<SearchItem> {
        let q = query.trim();
        if q.is_empty() {
            Vec::new().into()
        } else {
            vec![SearchItem::new(q.to_string(), q.to_string())].into()
        }
    }

    fn bootstrap(&self, _: &mut App) -> SearchAnswer<SearchItem> {
        Vec::new().into()
    }
}

/// What one control needs.
struct ControlProps {
    key: SharedString,
    header: SharedString,
    value: Option<TableFilterValue>,
    on_change: ValueFn,
    size: ElementSize,
    clear: bool,
    /// Distinguishes the popover's draft control from an inline one.
    scope: &'static str,
}

impl ControlProps {
    fn id(&self, part: &str) -> ElementId {
        ElementId::Name(format!("table-filter-{}-{}-{part}", self.scope, self.key).into())
    }
}

/// Upstream `FilterControl`: the operator's control, or none.
fn filter_control(
    op: &OperatorValue,
    p: ControlProps,
    window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let t = i18n(cx);
    let label = t.format(
        "@astryx.tableFiltering.filterByColumn",
        &[("header", p.header.as_ref().into())],
    );
    let all = t.t("@astryx.table.filter.allPlaceholder");
    let on_change = p.on_change.clone();
    Some(match op {
        OperatorValue::String { .. } => {
            let text = match &p.value {
                Some(TableFilterValue::Text(t)) => t.clone(),
                _ => SharedString::default(),
            };
            TextInput::new(p.id("text"), label.clone(), text)
                .label_hidden(true)
                .placeholder(label)
                .size(p.size)
                .clear(p.clear)
                .on_change(move |next, window, cx| {
                    on_change(
                        (!next.is_empty()).then_some(TableFilterValue::Text(next)),
                        window,
                        cx,
                    )
                })
                .into_any_element()
        }
        OperatorValue::Integer { min, max, .. } => number_control(
            &p,
            label,
            min.map(|n| n as f64),
            max.map(|n| n as f64),
            Some(1.),
            window,
            cx,
        ),
        OperatorValue::Float { min, max, .. } => {
            number_control(&p, label, *min, *max, None, window, cx)
        }
        OperatorValue::Enum { values } => {
            let current = match &p.value {
                Some(TableFilterValue::Text(t)) if !t.is_empty() => Some(t.clone()),
                _ => None,
            };
            let options: Vec<SelectorEntry> = values
                .iter()
                .map(|v| {
                    SelectorOptionData::new(v.value.clone())
                        .label(v.label.clone())
                        .into()
                })
                .collect();
            Selector::new(p.id("enum"), label, options)
                .label_hidden(true)
                .value(current)
                .placeholder(all)
                .size(p.size)
                .clear(p.clear)
                .on_change(move |next: Option<SharedString>, window, cx| {
                    on_change(
                        next.filter(|v| !v.is_empty()).map(TableFilterValue::Text),
                        window,
                        cx,
                    )
                })
                .into_any_element()
        }
        OperatorValue::EnumList { values } => {
            let current = match &p.value {
                Some(TableFilterValue::List(l)) => l.clone(),
                _ => Vec::new(),
            };
            let options: Vec<SelectorEntry> = values
                .iter()
                .map(|v| {
                    SelectorOptionData::new(v.value.clone())
                        .label(v.label.clone())
                        .into()
                })
                .collect();
            MultiSelector::new(p.id("enum-list"), label, options, current)
                .label_hidden(true)
                .placeholder(all)
                .size(p.size)
                .select_all(true)
                .search(false)
                .clear(p.clear)
                .on_change(move |next: Vec<SharedString>, _, window, cx| {
                    on_change(
                        (!next.is_empty()).then_some(TableFilterValue::List(next)),
                        window,
                        cx,
                    )
                })
                .into_any_element()
        }
        OperatorValue::StringList { search_source, .. }
        | OperatorValue::EntityList { search_source, .. } => {
            let current: Vec<SearchItem> = match &p.value {
                Some(TableFilterValue::List(l)) => l
                    .iter()
                    .map(|v| SearchItem::new(v.clone(), v.clone()))
                    .collect(),
                _ => Vec::new(),
            };
            let source: Rc<dyn SearchSource<SearchItem>> = search_source
                .clone()
                .unwrap_or_else(|| Rc::new(TypedSource));
            Tokenizer::from_rc(p.id("list"), label, source, current)
                .label_hidden(true)
                .size(p.size)
                .clear(p.clear)
                .presentation(PopoverPresentation::Popover)
                .on_change(move |items: Vec<SearchItem>, _, window, cx| {
                    let ids: Vec<SharedString> = items.iter().map(Searchable::id).collect();
                    on_change(
                        (!ids.is_empty()).then_some(TableFilterValue::List(ids)),
                        window,
                        cx,
                    )
                })
                .into_any_element()
        }
        // Upstream `DateFilterControl`: a date reports as soon as it is
        // complete, and the filter keeps its ISO text.
        OperatorValue::DateAbsolute { .. } => {
            let value = match &p.value {
                Some(TableFilterValue::Text(t)) => astryx_i18n::date::PlainDate::from_iso(t),
                _ => None,
            };
            crate::date_input::DateInput::new(p.id("date"), label, value)
                .label_hidden(true)
                .entry_error_behavior(crate::field::EntryErrorBehavior::Complete)
                .size(p.size)
                .clear(p.clear)
                .on_change(move |next, window, cx| {
                    on_change(
                        next.map(|date| TableFilterValue::Text(date.to_string().into())),
                        window,
                        cx,
                    )
                })
                .into_any_element()
        }
        // Upstream `TimeFilterControl`.
        OperatorValue::Time { .. } => {
            let value = match &p.value {
                Some(TableFilterValue::Text(t)) => astryx_i18n::date::PlainTime::from_iso(t),
                _ => None,
            };
            crate::time_input::TimeInput::new(p.id("time"), label, value)
                .label_hidden(true)
                .entry_error_behavior(crate::field::EntryErrorBehavior::Complete)
                .size(p.size)
                .clear(p.clear)
                .on_change(move |next, window, cx| {
                    on_change(
                        next.map(|time| TableFilterValue::Text(time.to_iso(false).into())),
                        window,
                        cx,
                    )
                })
                .into_any_element()
        }
        // Nested, empty, relative and range dates and custom values have no
        // control.
        _ => return None,
    })
}

/// Upstream `NumberFilterControl` with `useLiveNumberFilter`: the filter
/// follows each edit that reads as a number, and a draft rejected at commit
/// puts back the value from when editing began.
fn number_control(
    p: &ControlProps,
    label: SharedString,
    min: Option<f64>,
    max: Option<f64>,
    step: Option<f64>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let value = match &p.value {
        Some(TableFilterValue::Number(n)) => Some(*n),
        _ => None,
    };
    let start = window.use_keyed_state(p.id("number-start"), cx, move |_, _| value);
    let (on_change, on_input, on_error) = (
        p.on_change.clone(),
        p.on_change.clone(),
        p.on_change.clone(),
    );
    let (s1, s2, s3) = (start.clone(), start.clone(), start);
    let mut input = NumberInput::new(p.id("number"), label.clone(), value)
        .label_hidden(true)
        .placeholder(label)
        .size(p.size)
        .clear(p.clear)
        .on_focus(move |_, cx| s1.update(cx, |s, _| *s = value))
        .on_change(move |next, window, cx| {
            s2.update(cx, |s, _| *s = next);
            on_change(next.map(TableFilterValue::Number), window, cx)
        })
        .on_input(move |next, window, cx| {
            if let Some(n) = next
                && Some(n) != value
            {
                on_input(Some(TableFilterValue::Number(n)), window, cx)
            }
        })
        .on_entry_error_change(move |error, window, cx| {
            let begun = *s3.read(cx);
            if error.is_some() && value != begun {
                on_error(begun.map(TableFilterValue::Number), window, cx)
            }
        });
    if let Some(min) = min {
        input = input.min(min);
    }
    if let Some(max) = max {
        input = input.max(max);
    }
    if let Some(step) = step {
        input = input.step(step);
    }
    input.into_any_element()
}

/// The funnel button that opens a column's filter popover (upstream's
/// plain `button`): faint until its header is hovered, it is focused or the
/// column is filtered.
#[derive(IntoElement)]
struct FunnelTrigger {
    id: ElementId,
    label: SharedString,
    active: bool,
    group: SharedString,
}

impl RenderOnce for FunnelTrigger {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let focus = control_focus(&self.id, window, cx);
        let disclosure = crate::overlay::disclosure(cx);
        let radius = t.radius.element.to_pixels(rem);
        let visible = is_focus_visible(&focus, window);
        let selector = format!("{}", self.id);
        let mut el = div()
            .id(self.id)
            .debug_selector(move || selector)
            .track_focus(&focus)
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(radius)
            .cursor_pointer()
            .role(Role::Button)
            .aria_label(self.label)
            .aria_has_popup(HasPopup::Dialog);
        // A 44 px target for touch (upstream's `(pointer: coarse)`).
        if window.last_input_was_touch() {
            el = el.min_w(px(44.)).min_h(px(44.));
        }
        if !self.active && !visible {
            el = el.opacity(0.35).group_hover(self.group, |s| s.opacity(1.));
        }
        if let Some(d) = &disclosure {
            el = el.aria_expanded(d.expanded);
            if let Some(toggle) = d.toggle.clone() {
                el = el.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    toggle(window, cx)
                });
            }
        }
        if visible {
            el = el.child(crate::focus::focus_ring(t, radius, rem));
        }
        el.child(
            Icon::new("funnel")
                .size(IconSize::Xsm)
                .color(if self.active {
                    IconColor::Accent
                } else {
                    IconColor::Secondary
                }),
        )
    }
}

/// The popover's open state and its draft, kept per column.
#[derive(Default)]
struct PopoverDraft {
    open: bool,
    draft: Option<TableFilterValue>,
}

// ----------------------------------------------------------------------------
// The plugin

/// The plugin (upstream `useTableFiltering`).
#[derive(Clone)]
pub struct TableFiltering {
    filters: TableFilterState,
    on_change: ChangeFn,
    variant: TableFilterVariant,
    search_config: Rc<PowerSearchConfig>,
}

impl TableFiltering {
    /// Upstream `filters`, `onFilterChange` (`None` clears) and
    /// `searchConfig`.
    pub fn new(
        filters: TableFilterState,
        on_filter_change: impl Fn(SharedString, Option<TableFilterValue>, &mut Window, &mut App)
        + 'static,
        search_config: impl Into<Rc<PowerSearchConfig>>,
    ) -> Self {
        Self {
            filters,
            on_change: Rc::new(on_filter_change),
            variant: TableFilterVariant::Popover,
            search_config: search_config.into(),
        }
    }

    /// Upstream `variant` (popover by default).
    pub fn variant(mut self, variant: TableFilterVariant) -> Self {
        self.variant = variant;
        self
    }

    fn popover(
        &self,
        key: SharedString,
        header: SharedString,
        op: OperatorValue,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let label = i18n(cx).format(
            "@astryx.tableFiltering.filterByColumn",
            &[("header", header.as_ref().into())],
        );
        let value = self.filters.get(&key).cloned();
        let state = window.use_keyed_state(
            ElementId::Name(format!("table-filter-popover-{key}").into()),
            cx,
            |_, _| PopoverDraft::default(),
        );
        let (open, draft) = {
            let s = state.read(cx);
            (s.open, s.draft.clone())
        };
        let (on_open, seed) = (state.clone(), value.clone());
        let (apply_state, clear_state, draft_state) = (state.clone(), state.clone(), state);
        let (apply, clear) = (self.on_change.clone(), self.on_change.clone());
        let (apply_key, clear_key, trigger_key) = (key.clone(), key.clone(), key.clone());
        let t = i18n(cx);
        let popover = Popover::new(
            ElementId::Name(format!("table-filter-popover-{key}-layer").into()),
            label.clone(),
        )
        .open(open)
        .placement(Placement::Below)
        .alignment(LayerAlign::Start)
        .on_open_change(move |next, window, cx| {
            // Opening seeds the draft from the applied value.
            let seed = seed.clone();
            on_open.update(cx, |s, _| {
                if next {
                    s.draft = seed;
                }
                s.open = next;
            });
            window.refresh();
        })
        .content_fn(move |window, cx| {
            let control = filter_control(
                &op,
                ControlProps {
                    key: key.clone(),
                    header: header.clone(),
                    value: draft,
                    on_change: Rc::new(move |next, window, cx| {
                        draft_state.update(cx, |s, _| s.draft = next);
                        window.refresh();
                    }),
                    size: ElementSize::Md,
                    clear: false,
                    scope: "draft",
                },
                window,
                cx,
            );
            let theme = current(window, cx);
            let gap = theme.tokens().spacing.s2.to_pixels(window.rem_size());
            div()
                .w(px(240.))
                .children(control)
                .child(
                    div()
                        .flex()
                        .gap(gap)
                        .mt(gap)
                        .child(
                            div()
                                .debug_selector({
                                    let s = format!("table-filter-{clear_key}-reset");
                                    move || s
                                })
                                .child(
                                    Button::new(
                                        ElementId::Name(
                                            format!("table-filter-{clear_key}-reset").into(),
                                        ),
                                        t.t("@astryx.table.filter.reset"),
                                    )
                                    .variant(ButtonVariant::Ghost)
                                    .size(ElementSize::Sm)
                                    .on_click(
                                        move |_, window, cx| {
                                            clear(clear_key.clone(), None, window, cx);
                                            clear_state.update(cx, |s, _| s.open = false);
                                            window.refresh();
                                        },
                                    ),
                                ),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .debug_selector({
                                    let s = format!("table-filter-{apply_key}-apply");
                                    move || s
                                })
                                .child(
                                    Button::new(
                                        ElementId::Name(
                                            format!("table-filter-{apply_key}-apply").into(),
                                        ),
                                        t.t("@astryx.table.filter.apply"),
                                    )
                                    .variant(ButtonVariant::Primary)
                                    .size(ElementSize::Sm)
                                    .on_click(
                                        move |_, window, cx| {
                                            let draft = apply_state.read(cx).draft.clone();
                                            apply(apply_key.clone(), draft, window, cx);
                                            apply_state.update(cx, |s, _| s.open = false);
                                            window.refresh();
                                        },
                                    ),
                                ),
                        ),
                )
                .into_any_element()
        })
        .child(FunnelTrigger {
            id: ElementId::Name(format!("table-filter-{trigger_key}-trigger").into()),
            label,
            active: value.is_some(),
            group: group_name(&trigger_key),
        });
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .child(popover)
            .into_any_element()
    }
}

fn group_name(key: &str) -> SharedString {
    format!("table-filter-header-{key}").into()
}

/// The header's text, else the column's key (upstream `getHeaderString`).
fn header_string<T>(column: &TableColumn<T>) -> SharedString {
    column.header_text().unwrap_or_else(|| column.key.clone())
}

/// Upstream `useTableFiltering`.
pub fn use_table_filtering(
    filters: TableFilterState,
    on_filter_change: impl Fn(SharedString, Option<TableFilterValue>, &mut Window, &mut App) + 'static,
    search_config: impl Into<Rc<PowerSearchConfig>>,
) -> TableFiltering {
    TableFiltering::new(filters, on_filter_change, search_config)
}

impl<T> TablePlugin<T> for TableFiltering {
    fn transform_columns(
        &self,
        columns: Vec<TableColumn<T>>,
        _: &mut Window,
        _: &mut App,
    ) -> Vec<TableColumn<T>> {
        if self.variant == TableFilterVariant::Popover {
            return columns;
        }
        // Inline controls need room: a filtered column without a width
        // takes a proportional one, with its minimum.
        columns
            .into_iter()
            .map(|col| {
                if col.filter.is_some() && col.width.is_none() {
                    col.width(proportional(1.))
                } else {
                    col
                }
            })
            .collect()
    }

    fn transform_header_cell(
        &self,
        mut props: HeaderCellProps,
        column: &TableColumn<T>,
        _: &[TableColumn<T>],
        window: &mut Window,
        cx: &mut App,
    ) -> HeaderCellProps {
        let header = header_string(column);
        let op = column
            .filter
            .as_ref()
            .and_then(|f| resolve_filter(f, &self.search_config))
            .map(|o| o.value.clone());
        let key = column.key.clone();
        if self.variant == TableFilterVariant::Popover {
            let Some(op) = op else {
                return props;
            };
            let group = group_name(&key);
            props.hooks.push(Box::new(move |el| el.group(group)));
            props.after.push(self.popover(key, header, op, window, cx));
            return props;
        }
        let theme = current(window, cx);
        let gap = theme.tokens().spacing.s1.to_pixels(window.rem_size());
        let on_change = self.on_change.clone();
        let change_key = key.clone();
        let control = op.and_then(|op| {
            filter_control(
                &op,
                ControlProps {
                    key: key.clone(),
                    header,
                    value: self.filters.get(&key).cloned(),
                    on_change: Rc::new(move |next, window, cx| {
                        on_change(change_key.clone(), next, window, cx)
                    }),
                    size: ElementSize::Sm,
                    clear: true,
                    scope: "inline",
                },
                window,
                cx,
            )
        });
        // Upstream's inline slot is a block column: its control spans the cell.
        let selector = format!("table-filter-{key}-inline");
        let slot = div()
            .debug_selector(move || selector)
            .flex()
            .flex_col()
            .w_full()
            .gap(gap)
            .mt(gap)
            .min_w_0();
        let slot = match control {
            Some(control) => slot.child(control),
            // Keeps the headers' labels level.
            None => slot.child(
                div().h(px(if self.variant == TableFilterVariant::InlineCompact {
                    28.
                } else {
                    32.
                })),
            ),
        };
        props.below.push(slot.into_any_element());
        props
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_FILTERING_DOC: ComponentDoc = ComponentDoc {
    name: "useTableFiltering",
    rust_type: "astryx::components::table::TableFiltering",
    props: &[
        p("filters", RustApi::Argument("filters")),
        p("onFilterChange", RustApi::Argument("on_filter_change")),
        p("variant", RustApi::Method("variant")),
        p("searchConfig", RustApi::Argument("search_config")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_TABLE_FILTER_STATE_DOC: ComponentDoc = ComponentDoc {
    name: "useTableFilterState",
    rust_type: "astryx::components::table::use_table_filter_state",
    props: &[p("initialState", RustApi::Argument("initial"))],
    example: include_str!("examples.rs"),
};
