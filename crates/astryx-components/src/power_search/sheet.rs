//! PowerSearch's bottom-sheet presentation, upstream
//! `PowerSearchBottomSheet.tsx` (FR10-FR21): a trigger field that opens a
//! flow of stacked sheets over one draft.
//!
//! - The trigger (FR11): display-only tokens or the placeholder; one button
//!   named by the label and what it shows; the end lane outside it.
//! - Filters (FR12): a row per filter (field and operator, the value in
//!   bold) that opens its value step, a remove action, an empty state,
//!   Clear all (after a confirmation) and Done.
//! - Add filter (FR13): the fields by group, searchable from eight on.
//! - The value step (FR14, FR17): the Operator group, the value's editor
//!   (choice lists for enums and relative dates), Cancel and confirm.
//! - The draft (FR15b): Done replays its edits; a dismissal of a changed
//!   draft asks first (FR15c).

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::{ElementSize, Politeness, announce};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, Entity, IntoElement, ParentElement, Pixels, Role, SharedString,
    Styled, Window, bounds, div, point, prelude::*, px, size,
};

use super::draft::SheetDraft;
use super::format::{format_token_value, relative_date_options};
use super::model::{
    FilterValue, OperatorValue, PowerSearchChangeType, PowerSearchConfig, PowerSearchField,
    PowerSearchFilter,
};
use super::value_editor::{ValueEditorProps, value_editor};
use crate::button::{Button, ButtonVariant, IconButton};
use crate::choice::{CheckboxList, CheckboxListItem, RadioList, RadioListItem};
use crate::containers::EmptyState;
use crate::icon::{Icon, IconColor, IconSize};
use crate::item::ItemDensity;
use crate::list::{List, ListEdge, ListItem};
use crate::overlay::{BottomSheet, Dialog, DialogPurpose, SheetHeight};
use crate::text::{Heading, HeadingLevel};

pub(crate) type ChangeFn =
    Rc<dyn Fn(Vec<PowerSearchFilter>, PowerSearchChangeType, usize, &mut Window, &mut App)>;
pub(crate) type TokenOverrideFn =
    Rc<dyn Fn(&PowerSearchFilter, &mut Window, &mut App) -> Option<AnyElement>>;
type ActFn = Rc<dyn Fn(&mut Window, &mut App)>;
type KeyActFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type IndexFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;
type SuggestionFn = Rc<dyn Fn(super::source::PowerSearchItem, &mut Window, &mut App)>;

/// Upstream `SEARCHABLE_FIELD_COUNT`: the field list searches from here on.
const SEARCHABLE_FIELD_COUNT: usize = 8;

/// The steps of the flow, bottom to top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// Opened on Filters (a field with filters, or a read-only one).
    Filters,
    /// Opened straight on Add filter (an interactive field with none).
    AddFilter,
}

/// The value step's staged filter.
#[derive(Clone, Debug, PartialEq)]
struct Staged {
    /// Editing the draft's filter at this index, or creating.
    index: Option<usize>,
    /// Where Cancel from a creation returns focus: the field row, or the
    /// content search (FR16).
    opener: Option<String>,
    field: SharedString,
    operator: Option<SharedString>,
    value: Option<FilterValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Confirm {
    ClearAll,
    Discard,
}

pub(crate) struct FlowState {
    pub open: bool,
    entry: Entry,
    draft: SheetDraft,
    /// Add filter is open (above Filters, or as the entry).
    adding: bool,
    staged: Option<Staged>,
    confirm: Option<Confirm>,
    field_query: String,
    content_query: String,
}

pub(crate) fn flow_state(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<FlowState> {
    window.use_keyed_state(ElementId::Name(format!("{id}-flow").into()), cx, |_, _| {
        FlowState {
            open: false,
            entry: Entry::Filters,
            draft: SheetDraft::default(),
            adding: false,
            staged: None,
            confirm: None,
            field_query: String::new(),
            content_query: String::new(),
        }
    })
}

/// Opens the flow (FR12, FR13): Filters, or Add filter for an interactive
/// field with no filter.
pub(crate) fn open_flow(
    state: &Entity<FlowState>,
    filters: &[PowerSearchFilter],
    interactive: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let entry = if filters.is_empty() && interactive {
        Entry::AddFilter
    } else {
        Entry::Filters
    };
    state.update(cx, |s, _| {
        s.open = true;
        s.entry = entry;
        s.draft = SheetDraft::open(filters);
        s.adding = entry == Entry::AddFilter;
        s.staged = None;
        s.confirm = None;
        s.field_query.clear();
        s.content_query.clear();
    });
    window.refresh();
}

/// What the flow reads.
pub(crate) struct SheetProps {
    pub id: ElementId,
    pub config: Rc<PowerSearchConfig>,
    pub filters: Vec<PowerSearchFilter>,
    pub on_change: ChangeFn,
    pub read_only: bool,
    pub disabled: bool,
    pub clear: bool,
    pub max_menu_items: Option<usize>,
    pub max_search_results: usize,
    pub save_label: Option<SharedString>,
    pub placeholder: SharedString,
}

/// A row's two parts (FR12): "{field} {operator}" and the value, unbudgeted.
pub(crate) fn row_parts(
    config: &PowerSearchConfig,
    filter: &PowerSearchFilter,
    cx: &App,
) -> (String, Option<String>) {
    let t18 = i18n(cx);
    let field = config.find_field(&filter.field);
    let op = config.find_operator(&filter.field, &filter.operator);
    let field_label = field.map_or(filter.field.to_string(), |f| f.label.to_string());
    let op_label = op.map_or(filter.operator.to_string(), |o| {
        o.label_text(cx).to_string()
    });
    let label = if op_label.is_empty() {
        field_label
    } else {
        t18.format(
            "@astryx.powersearch.sheet.fieldOperator",
            &[
                ("field", field_label.as_str().into()),
                ("operator", op_label.as_str().into()),
            ],
        )
        .to_string()
    };
    let value = op.and_then(|o| format_token_value(&o.value, &filter.value, usize::MAX / 4, &t18));
    (label, value)
}

/// The flow's sheets, while open.
pub(crate) fn flow(p: SheetProps, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let t18 = i18n(cx);
    let state = flow_state(&p.id, window, cx);
    let interactive = !p.read_only && !p.disabled;
    // FR11: a field that becomes disabled closes the flow.
    if p.disabled && state.read(cx).open {
        state.update(cx, |s, _| s.open = false);
    }
    let (open, entry, adding, staged, confirm, draft) = {
        let s = state.read(cx);
        (
            s.open,
            s.entry,
            s.adding,
            s.staged.clone(),
            s.confirm,
            s.draft.clone(),
        )
    };
    let config = p.config.clone();

    let close_flow: ActFn = {
        let (state, flow_id) = (state.clone(), p.id.clone());
        Rc::new(move |window, cx| {
            state.update(cx, |s, _| {
                s.open = false;
                s.adding = false;
                s.staged = None;
                s.confirm = None;
            });
            // FR16: after the flow closes, the trigger.
            focus_next_render(format!("{flow_id}-trigger"), cx);
            window.refresh();
        })
    };
    // Done (FR15b): replay the draft onto the latest filters, then close.
    let done: ActFn = {
        let (state, close, on_change, latest) = (
            state.clone(),
            close_flow.clone(),
            p.on_change.clone(),
            p.filters.clone(),
        );
        Rc::new(move |window, cx| {
            let reports = if interactive {
                state.read(cx).draft.replay(&latest)
            } else {
                Vec::new()
            };
            close(window, cx);
            for (list, kind, index) in reports {
                on_change(list, kind, index, window, cx);
            }
        })
    };
    // A dismissal of the bottom sheet (FR15c): ask first while changed.
    let dismiss_bottom: ActFn = {
        let (state, close) = (state.clone(), close_flow.clone());
        Rc::new(move |window, cx| {
            if state.read(cx).draft.changed() {
                state.update(cx, |s, _| s.confirm = Some(Confirm::Discard));
                window.refresh();
            } else {
                close(window, cx);
            }
        })
    };
    let open_fields: ActFn = {
        let state = state.clone();
        Rc::new(move |window, cx| {
            if interactive {
                state.update(cx, |s, _| {
                    s.adding = true;
                    s.field_query.clear();
                });
                window.refresh();
            }
        })
    };
    // Choosing a field (FR13): an empty-only field adds at once; else the
    // value step opens on its default operator.
    let choose_field: KeyActFn = {
        let (state, config, flow_id) = (state.clone(), config.clone(), p.id.clone());
        Rc::new(
            move |key: SharedString, window: &mut Window, cx: &mut App| {
                let Some(field) = config.find_field(&key) else {
                    return;
                };
                let only_empty = field.operators.len() == 1
                    && matches!(field.operators[0].value, OperatorValue::Empty);
                if only_empty {
                    let filter = PowerSearchFilter::new(
                        field.key.clone(),
                        field.operators[0].key.clone(),
                        FilterValue::Empty,
                    );
                    let label = row_label_text(&config, &filter, cx);
                    state.update(cx, |s, _| {
                        s.draft.add(filter);
                        s.adding = false;
                        s.entry = Entry::Filters;
                    });
                    // FR16: the filter's row.
                    let row = state.read(cx).draft.filters.len().saturating_sub(1);
                    focus_next_render(format!("{flow_id}-sheet-row-{row}"), cx);
                    announce(
                        i18n(cx).format(
                            "@astryx.tokenizer.tokenAdded",
                            &[("label", label.as_str().into())],
                        ),
                        Politeness::Polite,
                        window,
                        cx,
                    );
                } else {
                    let op = field.default_op().map(|o| o.key.clone());
                    let value = op
                        .as_ref()
                        .and_then(|o| field.find_operator(o))
                        .and_then(|o| {
                            matches!(o.value, OperatorValue::Empty).then_some(FilterValue::Empty)
                        });
                    state.update(cx, |s, _| {
                        s.staged = Some(Staged {
                            index: None,
                            opener: Some(format!("{flow_id}-sheet-field-{}", field.key)),
                            field: field.key.clone(),
                            operator: op,
                            value,
                        })
                    });
                }
                window.refresh();
            },
        )
    };
    // A content-search suggestion (FR13, FR30): one with a value adds it;
    // any other opens the value step on its field and operator, a "More"
    // row carrying the typed text into a text field.
    let choose_suggestion: SuggestionFn = {
        let (state, config, flow_id) = (state.clone(), config.clone(), p.id.clone());
        Rc::new(
            move |item: super::source::PowerSearchItem, window: &mut Window, cx: &mut App| {
                let data = item.data.clone();
                let Some(field) = config.find_field(&data.field_key) else {
                    return;
                };
                let op = match &data.operator_key {
                    Some(k) => field.find_operator(k),
                    None => field.default_op(),
                };
                if let (Some(value), Some(op)) = (data.filter_value.clone(), op) {
                    let filter = PowerSearchFilter::new(field.key.clone(), op.key.clone(), value);
                    let label = row_label_text(&config, &filter, cx);
                    state.update(cx, |s, _| {
                        s.draft.add(filter);
                        s.adding = false;
                        s.entry = Entry::Filters;
                        s.content_query.clear();
                    });
                    // FR16: the filter's row.
                    let row = state.read(cx).draft.filters.len().saturating_sub(1);
                    focus_next_render(format!("{flow_id}-sheet-row-{row}"), cx);
                    announce(
                        i18n(cx).format(
                            "@astryx.tokenizer.tokenAdded",
                            &[("label", label.as_str().into())],
                        ),
                        Politeness::Polite,
                        window,
                        cx,
                    );
                    window.refresh();
                    return;
                }
                let value = match (op.map(|o| &o.value), &data.more_query) {
                    (Some(OperatorValue::Empty), _) => Some(FilterValue::Empty),
                    (
                        Some(OperatorValue::String {
                            search_source: None,
                            ..
                        }),
                        Some(text),
                    ) => Some(FilterValue::String(text.clone())),
                    _ => None,
                };
                state.update(cx, |s, _| {
                    s.staged = Some(Staged {
                        index: None,
                        opener: Some(format!("{flow_id}-sheet-content-search")),
                        field: field.key.clone(),
                        operator: op.map(|o| o.key.clone()),
                        value,
                    })
                });
                window.refresh();
            },
        )
    };
    let edit_row: IndexFn = {
        let state = state.clone();
        Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
            let Some(filter) = state.read(cx).draft.filters.get(index).cloned() else {
                return;
            };
            state.update(cx, |s, _| {
                s.staged = Some(Staged {
                    index: Some(index),
                    opener: None,
                    field: filter.field.clone(),
                    operator: Some(filter.operator.clone()),
                    value: Some(filter.value.clone()),
                })
            });
            window.refresh();
        })
    };
    let remove_row: IndexFn = {
        let (state, config, flow_id) = (state.clone(), config.clone(), p.id.clone());
        Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
            let Some(filter) = state.read(cx).draft.filters.get(index).cloned() else {
                return;
            };
            let label = row_label_text(&config, &filter, cx);
            state.update(cx, |s, _| s.draft.remove(index));
            // FR16: the nearest remaining remove action, else Add filter.
            let rows = state.read(cx).draft.filters.clone();
            let target = match super::draft::nearest_button_row(&rows, index, |f| {
                interactive && !f.read_only
            }) {
                Some(n) => format!("{flow_id}-sheet-remove-{n}"),
                None if rows.is_empty() => format!("{flow_id}-sheet-empty-add"),
                None => format!("{flow_id}-sheet-add"),
            };
            focus_next_render(target, cx);
            announce(
                i18n(cx).format(
                    "@astryx.tokenizer.tokenRemoved",
                    &[("label", label.as_str().into())],
                ),
                Politeness::Polite,
                window,
                cx,
            );
            window.refresh();
        })
    };

    // ---- Filters (FR12) -------------------------------------------------
    let filters_sheet = {
        let rows = draft.filters.clone();
        let has_rows = !rows.is_empty();
        let header = div()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .pb(t.spacing.s3)
            .child(div().flex_1().min_w_0().child(Heading::new(
                HeadingLevel::H2,
                t18.t("@astryx.powersearch.sheet.filtersTitle"),
            )))
            .when(interactive && has_rows, |el| {
                let open_fields = open_fields.clone();
                el.child(
                    div()
                        .debug_selector({
                            let s = format!("{}-sheet-add", p.id);
                            move || s
                        })
                        .child(
                            IconButton::new(
                                ElementId::Name(format!("{}-sheet-add", p.id).into()),
                                t18.t("@astryx.powersearch.sheet.addFilter"),
                                plus_glyph(px(16.)),
                            )
                            .variant(ButtonVariant::Secondary)
                            .size(ElementSize::Md)
                            .on_click(move |_, window, cx| open_fields(window, cx)),
                        ),
                )
            });
        let body: AnyElement = if has_rows {
            let mut list = List::new(ElementId::Name(format!("{}-sheet-rows", p.id).into()))
                .dividers(true)
                .density(ItemDensity::Spacious)
                .edge(ListEdge::Bleed)
                .label(t18.t("@astryx.powersearch.sheet.filtersTitle"));
            for (index, filter) in rows.iter().enumerate() {
                let (label, value) = row_parts(&config, filter, cx);
                let editable = interactive && !filter.read_only;
                let field_label = config
                    .find_field(&filter.field)
                    .map_or(filter.field.clone(), |f| f.label.clone());
                let one_tap = matches!(
                    config
                        .find_operator(&filter.field, &filter.operator)
                        .map(|o| &o.value),
                    Some(OperatorValue::Empty)
                );
                // One catalog message, "{label} {value}", with the value bold.
                let shown = match &value {
                    Some(v) => {
                        let message = t18
                            .format(
                                "@astryx.powersearch.sheet.labelValue",
                                &[
                                    ("label", label.as_str().into()),
                                    ("value", "\u{E000}".into()),
                                ],
                            )
                            .to_string();
                        let (before, after) = message
                            .split_once('\u{E000}')
                            .unwrap_or((message.as_str(), ""));
                        div()
                            .flex()
                            .flex_wrap()
                            .child(before.to_string())
                            .child(div().font_weight(gpui::FontWeight::BOLD).child(v.clone()))
                            .child(after.to_string())
                    }
                    None => div().child(label.clone()),
                };
                let row_selector = format!("{}-sheet-row-{index}", p.id);
                let mut item = ListItem::new_element(
                    ElementId::Name(format!("{}-sheet-row-{index}", p.id).into()),
                    div().debug_selector(move || row_selector).child(shown),
                );
                if editable {
                    let act = if one_tap {
                        remove_row.clone()
                    } else {
                        edit_row.clone()
                    };
                    item = item.on_click(move |_, window, cx| act(index, window, cx));
                    let remove = remove_row.clone();
                    let name = t18.format(
                        "@astryx.powersearch.sheet.removeFilter",
                        &[("field", field_label.as_ref().into())],
                    );
                    let remove_selector = format!("{}-sheet-remove-{index}", p.id);
                    item = item.end_content(
                        div().debug_selector(move || remove_selector).child(
                            IconButton::new(
                                ElementId::Name(format!("{}-sheet-remove-{index}", p.id).into()),
                                name.clone(),
                                Icon::new("close").size(IconSize::Sm),
                            )
                            .variant(ButtonVariant::Ghost)
                            .size(ElementSize::Md)
                            .tooltip(name)
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                remove(index, window, cx)
                            }),
                        ),
                    );
                }
                list = list.child(item);
            }
            list.into_any_element()
        } else {
            let title = if draft.changed() {
                t18.t("@astryx.powersearch.sheet.noFiltersInDraft")
            } else {
                t18.t("@astryx.powersearch.sheet.noFilters")
            };
            let mut empty = EmptyState::new(title)
                .description(t18.t("@astryx.powersearch.sheet.noFiltersDescription"))
                .heading_level(HeadingLevel::H4)
                .icon(
                    Icon::new("funnel")
                        .size(IconSize::Lg)
                        .color(IconColor::Secondary),
                );
            if interactive {
                let open_fields = open_fields.clone();
                empty = empty.actions(
                    div()
                        .debug_selector({
                            let s = format!("{}-sheet-empty-add", p.id);
                            move || s
                        })
                        .child(
                            Button::new(
                                ElementId::Name(format!("{}-sheet-empty-add", p.id).into()),
                                t18.t("@astryx.powersearch.sheet.addFilter"),
                            )
                            .variant(ButtonVariant::Primary)
                            .size(ElementSize::Lg)
                            .icon(plus_glyph(px(16.)))
                            .on_click(move |_, window, cx| open_fields(window, cx)),
                        ),
                );
            }
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(empty)
                .into_any_element()
        };
        let editable_rows = rows.iter().any(|f| !f.read_only);
        let show_clear = p.clear && interactive && editable_rows;
        let footer = (has_rows || draft.changed()).then(|| {
            let done = done.clone();
            let state_c = state.clone();
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .pt(t.spacing.s3)
                .when(show_clear, |el| {
                    el.child(
                        div()
                            .flex_1()
                            .debug_selector({
                                let s = format!("{}-sheet-clear", p.id);
                                move || s
                            })
                            .child(
                                Button::new(
                                    ElementId::Name(format!("{}-sheet-clear", p.id).into()),
                                    t18.t("@astryx.powersearch.sheet.clearAll"),
                                )
                                .variant(ButtonVariant::Secondary)
                                .size(ElementSize::Lg)
                                .w_full()
                                .on_click(move |_, window, cx| {
                                    state_c.update(cx, |s, _| s.confirm = Some(Confirm::ClearAll));
                                    window.refresh();
                                }),
                            ),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .debug_selector({
                            let s = format!("{}-sheet-done", p.id);
                            move || s
                        })
                        .child(
                            Button::new(
                                ElementId::Name(format!("{}-sheet-done", p.id).into()),
                                t18.t("@astryx.powersearch.sheet.done"),
                            )
                            .variant(ButtonVariant::Primary)
                            .size(ElementSize::Lg)
                            .w_full()
                            .on_click(move |_, window, cx| done(window, cx)),
                        ),
                )
        });
        let dismiss = dismiss_bottom.clone();
        BottomSheet::new(
            ElementId::Name(format!("{}-sheet-filters", p.id).into()),
            open && entry == Entry::Filters,
            t18.t("@astryx.powersearch.sheet.filtersTitle"),
        )
        .height(SheetHeight::Tall)
        .on_open_change(move |open, window, cx| {
            if !open {
                dismiss(window, cx)
            }
        })
        .child(
            div()
                .debug_selector({
                    let s = format!("{}-sheet-filters", p.id);
                    move || s
                })
                .flex()
                .flex_col()
                .min_h_full()
                .child(header)
                .child(div().flex_1().flex().flex_col().child(body))
                .children(footer),
        )
    };

    // ---- Add filter (FR13) ----------------------------------------------
    let fields_sheet = {
        let query = state.read(cx).field_query.clone();
        let searchable = config.fields.len() >= SEARCHABLE_FIELD_COUNT;
        let lower = query.trim().to_lowercase();
        let matches = |f: &PowerSearchField| {
            lower.is_empty()
                || f.label.to_lowercase().contains(&lower)
                || f.typeahead_aliases
                    .iter()
                    .any(|a| a.to_lowercase().contains(&lower))
        };
        let mut shown: Vec<&PowerSearchField> =
            config.fields.iter().filter(|f| matches(f)).collect();
        if !lower.is_empty() {
            shown.truncate(p.max_search_results);
        }
        // Ungrouped first, under "General", then each group in order.
        let mut groups: Vec<Option<SharedString>> = vec![None];
        for f in &shown {
            if let Some(g) = &f.group
                && !groups.contains(&Some(g.clone()))
            {
                groups.push(Some(g.clone()));
            }
        }
        let mut lists: Vec<AnyElement> = Vec::new();
        for group in groups {
            let members: Vec<&&PowerSearchField> =
                shown.iter().filter(|f| f.group == group).collect();
            if members.is_empty() {
                continue;
            }
            let name = group
                .clone()
                .unwrap_or_else(|| t18.t("@astryx.powersearch.sheet.ungroupedFields"));
            let mut list = List::new(ElementId::Name(
                format!("{}-sheet-group-{}", p.id, name).into(),
            ))
            .density(ItemDensity::Spacious)
            .edge(ListEdge::Bleed)
            .label(name.clone())
            .header(Heading::new(HeadingLevel::H3, name.clone()));
            for f in members {
                let only_empty =
                    f.operators.len() == 1 && matches!(f.operators[0].value, OperatorValue::Empty);
                let choose = choose_field.clone();
                let key = f.key.clone();
                let selector = format!("{}-sheet-field-{}", p.id, f.key);
                let icon = f.icon.clone().unwrap_or_else(|| "search".into());
                let mut item = ListItem::new_element(
                    ElementId::Name(format!("{}-sheet-field-{}", p.id, f.key).into()),
                    div()
                        .debug_selector(move || selector)
                        .child(f.label.clone()),
                )
                .start_content(
                    Icon::new(icon)
                        .size(IconSize::Sm)
                        .color(IconColor::Secondary),
                )
                .end_content(
                    div()
                        .text_color(t.color.icon_secondary)
                        .child(if only_empty {
                            plus_glyph(px(16.))
                        } else {
                            Icon::new("chevronRight")
                                .size(IconSize::Sm)
                                .color(IconColor::Secondary)
                                .into_any_element()
                        }),
                )
                .on_click(move |_, window, cx| choose(key.clone(), window, cx));
                if let Some(d) = &f.description {
                    item = item.description(d.clone());
                }
                list = list.child(item);
            }
            lists.push(list.into_any_element());
        }
        let content_search = config
            .content_search_field_key
            .as_ref()
            .and_then(|k| config.find_field(k))
            .and_then(|f| f.default_op())
            .is_some_and(|o| matches!(o.value, OperatorValue::String { .. }));
        let content_query = state.read(cx).content_query.clone();
        let suggestions: Vec<super::source::PowerSearchItem> = if content_search && interactive {
            let max_items = p.max_menu_items.unwrap_or(10);
            if content_query.trim().is_empty() {
                super::source::field_rows(&config)
            } else {
                super::source::sheet_rows(
                    &config,
                    &content_query,
                    p.max_search_results,
                    max_items,
                    cx,
                )
            }
        } else {
            Vec::new()
        };
        let content_box = content_search.then(|| {
            let (state, first, choose) = (
                state.clone(),
                suggestions.first().cloned(),
                choose_suggestion.clone(),
            );
            div()
                .debug_selector({
                    let s = format!("{}-sheet-content-search", p.id);
                    move || s
                })
                .child(
                    crate::input::TextInput::new(
                        ElementId::Name(format!("{}-sheet-content-search", p.id).into()),
                        t18.t("@astryx.powersearch.sheet.search"),
                        content_query.clone(),
                    )
                    .label_hidden(true)
                    .placeholder(p.placeholder.clone())
                    .clear(true)
                    .size(ElementSize::Lg)
                    .disabled(!interactive)
                    .on_change(move |text, window, cx| {
                        state.update(cx, |s, _| s.content_query = text.to_string());
                        window.refresh();
                    })
                    .on_enter(move |window, cx| {
                        if let Some(first) = first.clone() {
                            choose(first, window, cx);
                        }
                    }),
                )
        });
        let content_list = content_search.then(|| -> AnyElement {
            if suggestions.is_empty() {
                if content_query.trim().is_empty() {
                    return div().into_any_element();
                }
                return EmptyState::new(t18.t("@astryx.powersearch.sheet.noResults"))
                    .description(t18.t("@astryx.powersearch.sheet.tryDifferentSearch"))
                    .compact(true)
                    .heading_level(HeadingLevel::H4)
                    .icon(
                        Icon::new("search")
                            .size(IconSize::Lg)
                            .color(IconColor::Secondary),
                    )
                    .into_any_element();
            }
            let mut list = List::new(ElementId::Name(format!("{}-sheet-results", p.id).into()))
                .dividers(true)
                .density(ItemDensity::Spacious)
                .edge(ListEdge::Bleed)
                .label(t18.t("@astryx.powersearch.sheet.searchResults"));
            for item in suggestions.iter() {
                let choose = choose_suggestion.clone();
                let it = item.clone();
                let selector = format!("{}-sheet-result-{}", p.id, item.id);
                list = list.child(
                    ListItem::new_element(
                        ElementId::Name(format!("{}-sheet-result-{}", p.id, item.id).into()),
                        div()
                            .debug_selector(move || selector)
                            .child(item.label.clone()),
                    )
                    .on_click(move |_, window, cx| choose(it.clone(), window, cx)),
                );
            }
            list.into_any_element()
        });
        let search = (searchable && !content_search).then(|| {
            let state = state.clone();
            crate::input::TextInput::new(
                ElementId::Name(format!("{}-sheet-field-search", p.id).into()),
                t18.t("@astryx.powersearch.sheet.searchFields"),
                query.clone(),
            )
            .label_hidden(true)
            .placeholder(t18.t("@astryx.powersearch.sheet.searchFieldsPlaceholder"))
            .on_change(move |text, window, cx| {
                state.update(cx, |s, _| s.field_query = text.to_string());
                window.refresh();
            })
        });
        let empty = (searchable && !lower.is_empty() && shown.is_empty()).then(|| {
            EmptyState::new(t18.t("@astryx.powersearch.sheet.noMatchingFields"))
                .description(t18.t("@astryx.powersearch.sheet.tryDifferentSearch"))
                .compact(true)
                .icon(
                    Icon::new("search")
                        .size(IconSize::Lg)
                        .color(IconColor::Secondary),
                )
        });
        let footer = (entry == Entry::AddFilter).then(|| {
            let done = done.clone();
            div().pt(t.spacing.s3).child(
                Button::new(
                    ElementId::Name(format!("{}-sheet-fields-done", p.id).into()),
                    t18.t("@astryx.powersearch.sheet.done"),
                )
                .variant(ButtonVariant::Primary)
                .size(ElementSize::Lg)
                .w_full()
                .on_click(move |_, window, cx| done(window, cx)),
            )
        });
        let (state_d, dismiss, fields_id) = (state.clone(), dismiss_bottom.clone(), p.id.clone());
        BottomSheet::new(
            ElementId::Name(format!("{}-sheet-fields", p.id).into()),
            open && adding,
            t18.t("@astryx.powersearch.sheet.addFilter"),
        )
        .height(SheetHeight::Tall)
        .on_open_change(move |open, window, cx| {
            if open {
                return;
            }
            // The entry closes the flow; above Filters it reveals them.
            if state_d.read(cx).entry == Entry::AddFilter {
                dismiss(window, cx);
            } else {
                state_d.update(cx, |s, _| s.adding = false);
                // FR16: the Filters header's Add filter, or the empty state's.
                let rows = state_d.read(cx).draft.filters.len();
                focus_next_render(
                    if rows == 0 {
                        format!("{fields_id}-sheet-empty-add")
                    } else {
                        format!("{fields_id}-sheet-add")
                    },
                    cx,
                );
                window.refresh();
            }
        })
        .child(
            div()
                .debug_selector({
                    let s = format!("{}-sheet-fields", p.id);
                    move || s
                })
                .flex()
                .flex_col()
                .gap(t.spacing.s3)
                .child(Heading::new(
                    HeadingLevel::H2,
                    t18.t("@astryx.powersearch.sheet.addFilter"),
                ))
                .children(content_box)
                .children(search)
                .children(content_list)
                .when(!content_search, |el| el.children(empty).children(lists))
                .children(footer),
        )
    };

    // ---- The value step (FR14, FR17) -----------------------------------
    let value_sheet = staged.clone().map(|staged| {
        let field = config.find_field(&staged.field).cloned();
        let field_label = field
            .as_ref()
            .map_or(staged.field.clone(), |f| f.label.clone());
        let op = staged
            .operator
            .as_ref()
            .and_then(|o| field.as_ref().and_then(|f| f.find_operator(o)))
            .cloned();
        let title: SharedString = match (&field, &op) {
            (Some(f), Some(o)) if f.operators.len() == 1 => t18.format(
                "@astryx.powersearch.sheet.fieldOperator",
                &[
                    ("field", f.label.as_ref().into()),
                    ("operator", o.label_text(cx).as_ref().into()),
                ],
            ),
            _ => field_label.clone(),
        };
        let mut sections: Vec<AnyElement> = Vec::new();
        let unavailable = field.is_none() || (staged.operator.is_some() && op.is_none());
        if unavailable {
            sections.push(
                div()
                    .debug_selector({
                        let s = format!("{}-sheet-unavailable", p.id);
                        move || s
                    })
                    .text_color(t.color.text_secondary)
                    .child(t18.t("@astryx.powersearch.sheet.filterUnavailable"))
                    .into_any_element(),
            );
        }
        if let Some(field) = field.clone().filter(|f| f.operators.len() > 1) {
            let state = state.clone();
            let items: Vec<RadioListItem> = field
                .operators
                .iter()
                .map(|o| {
                    RadioListItem::new(
                        o.key.clone(),
                        t18.format(
                            "@astryx.powersearch.sheet.fieldOperator",
                            &[
                                ("field", field.label.as_ref().into()),
                                ("operator", o.label_text(cx).as_ref().into()),
                            ],
                        ),
                    )
                })
                .collect();
            let field_c = field.clone();
            sections.push(
                RadioList::new(
                    ElementId::Name(format!("{}-sheet-operator", p.id).into()),
                    t18.t("@astryx.powersearch.editor.operator"),
                    staged.operator.clone().unwrap_or_default(),
                )
                .items(items)
                .on_change(move |key, window, cx| {
                    let new_kind = field_c.find_operator(&key).map(|o| o.value.kind());
                    state.update(cx, |s, _| {
                        if let Some(st) = s.staged.as_mut() {
                            let old_kind = st
                                .operator
                                .as_ref()
                                .and_then(|o| field_c.find_operator(o))
                                .map(|o| o.value.kind());
                            if new_kind != old_kind {
                                st.value = None;
                            }
                            if new_kind == Some("empty") {
                                st.value = Some(FilterValue::Empty);
                            }
                            st.operator = Some(key.clone());
                        }
                    });
                    window.refresh();
                })
                .into_any_element(),
            );
        }
        if let Some(op) = op.clone().filter(|_| !unavailable) {
            let set_value = {
                let state = state.clone();
                Rc::new(
                    move |value: FilterValue, _: bool, window: &mut Window, cx: &mut App| {
                        state.update(cx, |s, _| {
                            if let Some(st) = s.staged.as_mut() {
                                st.value = Some(value);
                            }
                        });
                        window.refresh();
                    },
                )
            };
            let editor: Option<AnyElement> = match &op.value {
                // FR17: enums and relative dates as the system's lists.
                OperatorValue::Enum { values } => {
                    let set = set_value.clone();
                    Some(
                        RadioList::new(
                            ElementId::Name(format!("{}-sheet-value-{}", p.id, op.key).into()),
                            field_label.clone(),
                            match &staged.value {
                                Some(FilterValue::Enum(v)) => v.clone(),
                                _ => SharedString::default(),
                            },
                        )
                        .items(
                            values
                                .iter()
                                .map(|v| RadioListItem::new(v.value.clone(), v.label.clone())),
                        )
                        .on_change(move |v, window, cx| {
                            set(FilterValue::Enum(v), false, window, cx)
                        })
                        .into_any_element(),
                    )
                }
                OperatorValue::DateRelative {
                    past_allowed,
                    future_allowed,
                } => {
                    let set = set_value.clone();
                    Some(
                        RadioList::new(
                            ElementId::Name(format!("{}-sheet-value-{}", p.id, op.key).into()),
                            field_label.clone(),
                            match &staged.value {
                                Some(FilterValue::DateRelative(v)) => v.clone(),
                                _ => SharedString::default(),
                            },
                        )
                        .items(
                            relative_date_options(*past_allowed, *future_allowed, &t18)
                                .into_iter()
                                .map(|(v, l)| RadioListItem::new(v, l)),
                        )
                        .on_change(move |v, window, cx| {
                            set(FilterValue::DateRelative(v), false, window, cx)
                        })
                        .into_any_element(),
                    )
                }
                OperatorValue::EnumList { values } => {
                    let set = set_value.clone();
                    Some(
                        CheckboxList::new(
                            ElementId::Name(format!("{}-sheet-value-{}", p.id, op.key).into()),
                            field_label.clone(),
                        )
                        .value(match &staged.value {
                            Some(FilterValue::EnumList(v)) => v.clone(),
                            _ => Vec::new(),
                        })
                        .items(
                            values.iter().map(|v| {
                                CheckboxListItem::new(v.label.clone()).value(v.value.clone())
                            }),
                        )
                        .on_change(move |v, window, cx| {
                            set(FilterValue::EnumList(v), false, window, cx)
                        })
                        .into_any_element(),
                    )
                }
                OperatorValue::Nested => {
                    let state = state.clone();
                    let on_value: super::editor::NestedValueFn =
                        Rc::new(move |value, window, cx| {
                            let changed = state
                                .read(cx)
                                .staged
                                .as_ref()
                                .is_some_and(|s| s.value != value);
                            if changed {
                                state.update(cx, |s, _| {
                                    if let Some(st) = s.staged.as_mut() {
                                        st.value = value;
                                    }
                                });
                                window.refresh();
                            }
                        });
                    Some(super::editor::nested_step(
                        &ElementId::Name(format!("{}-sheet-nested-{}", p.id, staged.field).into()),
                        &config,
                        &staged.field,
                        &op.key,
                        staged.value.clone(),
                        !interactive,
                        p.max_menu_items,
                        on_value,
                        window,
                        cx,
                    ))
                }
                _ => value_editor(
                    ValueEditorProps {
                        id: ElementId::Name(
                            format!("{}-sheet-value-{}-{}", p.id, staged.field, op.key).into(),
                        ),
                        op: op.value.clone(),
                        value: staged.value.clone(),
                        on_change: set_value,
                        on_enter: None,
                        max_menu_items: p.max_menu_items,
                        disabled: !interactive,
                        sheet: true,
                        field_label: Some(field_label.clone()),
                        on_unstage: Some({
                            let state = state.clone();
                            Rc::new(move |window: &mut Window, cx: &mut App| {
                                state.update(cx, |s, _| {
                                    if let Some(st) = s.staged.as_mut() {
                                        st.value = None;
                                    }
                                });
                                window.refresh();
                            })
                        }),
                    },
                    window,
                    cx,
                ),
            };
            sections.extend(editor);
        }
        // Several sections sit in muted containers (FR14).
        let many = sections.len() > 1;
        let body = div()
            .flex()
            .flex_col()
            .gap(t.spacing.s4)
            .children(sections.into_iter().map(|s| {
                if many {
                    div()
                        .p(t.spacing.s3)
                        .rounded(t.radius.container)
                        .bg(t.color.background_muted)
                        .child(s)
                        .into_any_element()
                } else {
                    s
                }
            }));
        let complete = !unavailable
            && op.as_ref().is_some_and(|o| {
                matches!(o.value, OperatorValue::Empty)
                    || staged.value.as_ref().is_some_and(|v| !value_is_empty(v))
            });
        let confirm_label = p.save_label.clone().unwrap_or_else(|| {
            if staged.index.is_some() {
                t18.t("@astryx.powersearch.editor.apply")
            } else {
                t18.t("@astryx.powersearch.sheet.addFilter")
            }
        });
        let cancel = {
            let (state, flow_id) = (state.clone(), p.id.clone());
            move |window: &mut Window, cx: &mut App| {
                // FR16: an edit's Cancel returns to its row; a creation's to
                // the field row or the content search that opened it.
                let target = state.read(cx).staged.as_ref().and_then(|s| match s.index {
                    Some(i) => Some(format!("{flow_id}-sheet-row-{i}")),
                    None => s.opener.clone(),
                });
                state.update(cx, |s, _| s.staged = None);
                if let Some(target) = target {
                    focus_next_render(target, cx);
                }
                window.refresh();
            }
        };
        let confirm_action = {
            let (state, config, flow_id) = (state.clone(), config.clone(), p.id.clone());
            let staged = staged.clone();
            move |window: &mut Window, cx: &mut App| {
                let (Some(op), Some(value)) = (
                    staged.operator.clone(),
                    state.read(cx).staged.as_ref().and_then(|s| s.value.clone()),
                ) else {
                    return;
                };
                let filter = PowerSearchFilter::new(staged.field.clone(), op, value);
                let label = row_label_text(&config, &filter, cx);
                let creating = staged.index.is_none();
                state.update(cx, |s, _| {
                    match staged.index {
                        Some(i) => s.draft.edit(i, filter),
                        None => {
                            s.draft.add(filter);
                        }
                    }
                    s.staged = None;
                    // A creation closes Add filter too and shows Filters.
                    if creating {
                        s.adding = false;
                        s.entry = Entry::Filters;
                    }
                });
                if creating {
                    announce(
                        i18n(cx).format(
                            "@astryx.tokenizer.tokenAdded",
                            &[("label", label.as_str().into())],
                        ),
                        Politeness::Polite,
                        window,
                        cx,
                    );
                }
                // FR16: a creation lands on Filters' Add filter; an edit on its row.
                match staged.index {
                    Some(i) => focus_next_render(format!("{flow_id}-sheet-row-{i}"), cx),
                    None => focus_next_render(format!("{flow_id}-sheet-add"), cx),
                }
                window.refresh();
            }
        };
        let cancel_b = cancel.clone();
        let footer = div()
            .flex()
            .gap(t.spacing.s2)
            .pt(t.spacing.s3)
            .child(
                div().flex_1().child(
                    Button::new(
                        ElementId::Name(format!("{}-sheet-cancel", p.id).into()),
                        t18.t("@astryx.powersearch.editor.cancel"),
                    )
                    .variant(ButtonVariant::Secondary)
                    .size(ElementSize::Lg)
                    .w_full()
                    .on_click(move |_, window, cx| cancel_b(window, cx)),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .debug_selector({
                        let s = format!("{}-sheet-confirm", p.id);
                        move || s
                    })
                    .child(
                        Button::new(
                            ElementId::Name(format!("{}-sheet-confirm", p.id).into()),
                            confirm_label,
                        )
                        .variant(ButtonVariant::Primary)
                        .size(ElementSize::Lg)
                        .w_full()
                        .disabled(!complete || !interactive)
                        .on_click(move |_, window, cx| confirm_action(window, cx)),
                    ),
            );
        BottomSheet::new(
            ElementId::Name(format!("{}-sheet-value", p.id).into()),
            open,
            title.clone(),
        )
        .height(SheetHeight::Tall)
        .on_open_change(move |open, window, cx| {
            if !open {
                cancel(window, cx)
            }
        })
        .child(
            div()
                .debug_selector({
                    let s = format!("{}-sheet-value", p.id);
                    move || s
                })
                .flex()
                .flex_col()
                .gap(t.spacing.s3)
                .child(Heading::new(HeadingLevel::H2, title))
                .child(body)
                .child(footer),
        )
    });

    // ---- Confirmations (FR12, FR15c) ------------------------------------
    let confirmation = confirm.map(|which| {
        let (title, description, destroy_label, safe_label) = match which {
            Confirm::ClearAll => (
                t18.t("@astryx.powersearch.sheet.clearAllTitle"),
                t18.t("@astryx.powersearch.sheet.clearAllDescription"),
                t18.t("@astryx.powersearch.sheet.clearAllConfirm"),
                t18.t("@astryx.powersearch.sheet.clearAllCancel"),
            ),
            Confirm::Discard => (
                t18.t("@astryx.powersearch.sheet.discardTitle"),
                t18.t("@astryx.powersearch.sheet.discardDescription"),
                t18.t("@astryx.powersearch.sheet.discardConfirm"),
                t18.t("@astryx.powersearch.sheet.keepEditing"),
            ),
        };
        let safe = {
            let (state, flow_id) = (state.clone(), p.id.clone());
            Rc::new(move |window: &mut Window, cx: &mut App| {
                state.update(cx, |s, _| s.confirm = None);
                // FR16: Keep filters returns to Clear all.
                if which == Confirm::ClearAll {
                    focus_next_render(format!("{flow_id}-sheet-clear"), cx);
                }
                window.refresh();
            }) as ActFn
        };
        let destroy = {
            let (state, close, flow_id) = (state.clone(), close_flow.clone(), p.id.clone());
            Rc::new(move |window: &mut Window, cx: &mut App| match which {
                Confirm::ClearAll => {
                    state.update(cx, |s, _| {
                        s.draft.clear_all();
                        s.confirm = None;
                    });
                    // FR16: the first remaining control, Add filter.
                    let rows = state.read(cx).draft.filters.len();
                    focus_next_render(
                        if rows == 0 {
                            format!("{flow_id}-sheet-empty-add")
                        } else {
                            format!("{flow_id}-sheet-add")
                        },
                        cx,
                    );
                    window.refresh();
                }
                Confirm::Discard => close(window, cx),
            }) as ActFn
        };
        let (safe_a, safe_b) = (safe.clone(), safe.clone());
        Dialog::new(
            ElementId::Name(format!("{}-sheet-confirm-dialog", p.id).into()),
            true,
        )
        .purpose(DialogPurpose::Info)
        .label(title.clone())
        .on_open_change(move |open, window, cx| {
            if !open {
                safe_a(window, cx)
            }
        })
        .child(
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.s3)
                .child(Heading::new(HeadingLevel::H3, title))
                .child(div().text_color(t.color.text_secondary).child(description))
                .child(
                    div()
                        .id(ElementId::Name(
                            format!("{}-sheet-confirm-actions", p.id).into(),
                        ))
                        .flex()
                        .flex_col()
                        .gap(t.spacing.s2)
                        .role(Role::Group)
                        .child(
                            div()
                                .debug_selector({
                                    let s = format!("{}-sheet-destroy", p.id);
                                    move || s
                                })
                                .child(
                                    Button::new(
                                        ElementId::Name(format!("{}-sheet-destroy", p.id).into()),
                                        destroy_label,
                                    )
                                    .variant(ButtonVariant::Destructive)
                                    .size(ElementSize::Lg)
                                    .w_full()
                                    .on_click(move |_, window, cx| destroy(window, cx)),
                                ),
                        )
                        .child(
                            Button::new(
                                ElementId::Name(format!("{}-sheet-safe", p.id).into()),
                                safe_label,
                            )
                            .size(ElementSize::Lg)
                            .w_full()
                            .on_click(move |_, window, cx| safe_b(window, cx)),
                        ),
                ),
        )
        .into_any_element()
    });
    div()
        .display_contents()
        .child(filters_sheet)
        .child(fields_sheet)
        .children(value_sheet)
        .children(confirmation)
        .into_any_element()
}

/// FR16: asks the flow's control named `name` to take focus when it next
/// renders.
fn focus_next_render(name: String, cx: &mut App) {
    astryx_primitives::request_focus(&ElementId::Name(name.into()), cx);
}

/// A filter's whole text, for announcements (AR6).
fn row_label_text(config: &PowerSearchConfig, filter: &PowerSearchFilter, cx: &App) -> String {
    let (label, value) = row_parts(config, filter, cx);
    match value {
        Some(v) => i18n(cx)
            .format(
                "@astryx.powersearch.sheet.labelValue",
                &[
                    ("label", label.as_str().into()),
                    ("value", v.as_str().into()),
                ],
            )
            .to_string(),
        None => label,
    }
}

/// A staged value that completes nothing yet (an empty list or text).
fn value_is_empty(value: &FilterValue) -> bool {
    match value {
        FilterValue::String(s) => s.is_empty(),
        FilterValue::StringList(l) | FilterValue::EnumList(l) => l.is_empty(),
        FilterValue::EntityList(l) => l.is_empty(),
        FilterValue::Enum(v) | FilterValue::DateRelative(v) => v.is_empty(),
        _ => false,
    }
}

/// Upstream's `PlusIcon`: a plus on a 16-unit box (bars 3 to 13, a 1.5
/// stroke), painted in the text color it sits in.
pub(crate) fn plus_glyph(edge: Pixels) -> AnyElement {
    gpui::canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let color = window.text_style().color;
            let unit = edge / 16.;
            let (stroke, from, to) = (unit * 1.5, unit * 3., unit * 13.);
            let mid = edge / 2. - stroke / 2.;
            window.paint_quad(gpui::fill(
                bounds(
                    point(b.origin.x + from, b.origin.y + mid),
                    size(to - from, stroke),
                ),
                color,
            ));
            window.paint_quad(gpui::fill(
                bounds(
                    point(b.origin.x + mid, b.origin.y + from),
                    size(stroke, to - from),
                ),
                color,
            ));
        },
    )
    .flex_none()
    .size(edge)
    .into_any_element()
}

/// What the trigger field reads (FR11).
pub(crate) struct TriggerProps {
    pub id: ElementId,
    pub config: Rc<PowerSearchConfig>,
    pub filters: Vec<PowerSearchFilter>,
    pub label: SharedString,
    pub label_hidden: bool,
    pub placeholder: SharedString,
    pub disabled: bool,
    pub read_only: bool,
    pub disabled_message: Option<SharedString>,
    pub size: Option<ElementSize>,
    pub max_token_length: usize,
    pub start_icon: Option<AnyElement>,
    pub end_lane: Option<AnyElement>,
    pub status: Option<crate::field::InputStatus>,
    pub status_placement: Option<crate::field::StatusPlacement>,
    pub token_override: TokenOverrideFn,
}

/// The sheet presentation's field (FR11): display-only tokens, or the
/// placeholder, in one button that opens the flow; the end lane after it.
pub(crate) fn trigger_field(p: TriggerProps, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let t18 = i18n(cx);
    let state = flow_state(&p.id, window, cx);
    let open = state.read(cx).open;
    let size = astryx_primitives::resolve_size(p.size, ElementSize::Md, cx);
    let focus = astryx_primitives::control_focus(
        &ElementId::Name(format!("{}-trigger", p.id).into()),
        window,
        cx,
    );
    let interactive = !p.read_only && !p.disabled;
    // The tokens, display-only: the same words as the popover's.
    let mut texts: Vec<String> = Vec::new();
    let tokens: Vec<AnyElement> = p
        .filters
        .iter()
        .enumerate()
        .map(|(index, filter)| {
            if let Some(custom) = (p.token_override)(filter, window, cx) {
                return custom;
            }
            let field = p.config.find_field(&filter.field);
            let op = p.config.find_operator(&filter.field, &filter.operator);
            let field_label = field.map_or(filter.field.to_string(), |f| f.label.to_string());
            let op_label = op.map(|o| o.label_text(cx).to_string()).unwrap_or_default();
            let label = super::format::format_token_label(&field_label, &op_label, &t18);
            let budget = p
                .max_token_length
                .saturating_sub(field_label.chars().count() + op_label.chars().count())
                .max(10);
            let value = op.and_then(|o| format_token_value(&o.value, &filter.value, budget, &t18));
            let full = match &value {
                Some(v) => super::format::format_token_text(&label, v, &t18),
                None => label.clone(),
            };
            texts.push(full.clone());
            let parts = value.is_none() || super::format::token_text_keeps_parts(&t18);
            let mut token = crate::token::Token::new(
                ElementId::Name(format!("{}-trigger-token-{index}", p.id).into()),
                if parts { label } else { full.clone() },
            )
            .naming(full.clone(), full)
            .size(size)
            .disabled(p.disabled);
            if parts && let Some(v) = value {
                token = token.end_content(div().font_weight(gpui::FontWeight::NORMAL).child(v));
            }
            token.into_any_element()
        })
        .collect();
    let shown_text: String = if p.filters.is_empty() {
        p.placeholder.to_string()
    } else {
        texts.join(" ")
    };
    let selector = format!("{}-trigger", p.id);
    let filters_at_open = p.filters.clone();
    let mut trigger = div()
        .id(ElementId::Name(format!("{}-trigger", p.id).into()))
        .debug_selector(move || selector)
        .flex()
        .flex_1()
        .min_w_0()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.s1)
        .track_focus(&focus)
        .role(Role::Button)
        .aria_label(SharedString::from(format!("{} {}", p.label, shown_text)))
        .aria_has_popup(gpui::HasPopup::Dialog)
        .aria_expanded(open)
        .aria_disabled(p.disabled)
        .children(p.start_icon.map(|i| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .text_color(t.color.icon_secondary)
                .child(i)
        }))
        .children(tokens)
        .when(p.filters.is_empty(), |el| {
            el.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(t.color.text_secondary)
                    .child(p.placeholder.clone()),
            )
        });
    if !p.disabled {
        let state = state.clone();
        trigger = trigger.cursor_pointer().on_click(move |_, window, cx| {
            open_flow(&state, &filters_at_open, interactive, window, cx)
        });
    }
    let chrome = crate::input::control::Chrome {
        id: ElementId::Name(format!("{}-trigger-control", p.id).into()),
        target: "tokenizer",
        variant: None,
        size,
        status: p.status.as_ref().map(|s| s.status),
        disabled: p.disabled,
        read_only: false,
        focused: focus.is_focused(window),
        focus_visible: astryx_primitives::is_focus_visible(&focus, window),
        row: false,
        place: None,
        dir: astryx_i18n::use_direction(cx),
    };
    let control = crate::input::control::control_box(&chrome, &theme, window, cx)
        .items_center()
        .gap(t.spacing.s1)
        .px(t.spacing.s2)
        .py(t.spacing.s1)
        .min_h(crate::button::element_height(size, t))
        .child(trigger)
        .children(p.end_lane.map(|lane| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.spacing.s2)
                .ml_auto()
                .child(lane)
        }));
    let control = crate::overlay::disabled_reason(
        &p.id,
        p.disabled,
        p.disabled_message.as_ref(),
        false,
        control,
    );
    let mut field = crate::field::Field::new(p.id.clone(), p.label.clone())
        .label_hidden(p.label_hidden)
        .disabled(p.disabled);
    if let Some(status) = p.status {
        field = field.status(status);
    }
    if let Some(placement) = p.status_placement {
        field = field.status_placement(placement);
    }
    field.child(control).into_any_element()
}
