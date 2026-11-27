//! The filter editor, upstream `PowerSearchEditPopover.tsx` (and its public
//! wrapper `PowerSearchFilterEditor`): the field, the operator and the value
//! in one row, then Delete (when editing), Cancel and Apply. A nested
//! operator edits its sub-filters as a TreeList.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, Entity, IntoElement, KeyDownEvent, ParentElement, RenderOnce,
    SharedString, Styled, Window, div, prelude::*, px,
};
use rustryx_i18n::current as i18n;

use super::model::{
    FilterValue, OperatorValue, PartialFilter, PowerSearchConfig, PowerSearchFilter,
};
use super::value_editor::{ValueEditorProps, value_editor};
use crate::button::{Button, ButtonVariant, IconButton};
use crate::icon::{Icon, IconSize};
use crate::selector::{Selector, SelectorEntry, SelectorOptionData};
use crate::tree_list::{TreeList, TreeListItemData};
use rustryx_primitives::ElementSize;

pub(crate) type SaveFn = Rc<dyn Fn(Option<PowerSearchFilter>, &mut Window, &mut App)>;
pub(crate) type CancelFn = Rc<dyn Fn(&mut Window, &mut App)>;
/// Reports a nested group's value while it is complete, else none.
pub(crate) type NestedValueFn = Rc<dyn Fn(Option<FilterValue>, &mut Window, &mut App)>;

/// Whether the editor adds a filter or edits one (upstream `mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorMode {
    Create,
    Edit,
}

/// A filter being edited, with its sub-filters for a nested operator
/// (upstream `EditablePartialFilter`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Draft {
    pub field: SharedString,
    pub operator: Option<SharedString>,
    pub value: Option<FilterValue>,
    pub subs: Option<Vec<Draft>>,
}

impl Draft {
    fn from_filter(config: &PowerSearchConfig, f: &PowerSearchFilter) -> Draft {
        let nested = matches!(
            config
                .find_operator(&f.field, &f.operator)
                .map(|o| &o.value),
            Some(OperatorValue::Nested)
        );
        Draft {
            field: f.field.clone(),
            operator: Some(f.operator.clone()),
            value: Some(f.value.clone()),
            subs: match (&f.value, nested) {
                (FilterValue::Nested(list), true) => {
                    Some(list.iter().map(|s| Draft::from_filter(config, s)).collect())
                }
                _ => None,
            },
        }
    }

    fn from_partial(config: &PowerSearchConfig, p: &PartialFilter) -> Draft {
        match (&p.operator, &p.value) {
            (Some(op), Some(value)) => Draft::from_filter(
                config,
                &PowerSearchFilter::new(p.field.clone(), op.clone(), value.clone()),
            ),
            _ => Draft {
                field: p.field.clone(),
                operator: p.operator.clone(),
                value: p.value.clone(),
                subs: None,
            },
        }
    }

    /// Upstream `isEditableFilterComplete`.
    fn complete(&self, config: &PowerSearchConfig) -> bool {
        let Some(op) = &self.operator else {
            return false;
        };
        match config.find_operator(&self.field, op).map(|o| &o.value) {
            Some(OperatorValue::Nested) => self
                .subs
                .as_ref()
                .is_some_and(|s| !s.is_empty() && s.iter().all(|d| d.complete(config))),
            _ => self.value.is_some(),
        }
    }

    /// Upstream `editableToCompleteFilter`.
    fn to_filter(&self, config: &PowerSearchConfig) -> Option<PowerSearchFilter> {
        let op = self.operator.clone()?;
        match config.find_operator(&self.field, &op).map(|o| &o.value) {
            Some(OperatorValue::Nested) => {
                let subs = self
                    .subs
                    .as_ref()
                    .map(|s| s.iter().filter_map(|d| d.to_filter(config)).collect())
                    .unwrap_or_default();
                Some(PowerSearchFilter::new(
                    self.field.clone(),
                    op,
                    FilterValue::Nested(subs),
                ))
            }
            _ => Some(PowerSearchFilter::new(
                self.field.clone(),
                op,
                self.value.clone()?,
            )),
        }
    }

    /// A field with its default operator and no value.
    fn fresh(config: &PowerSearchConfig, field: &str) -> Draft {
        let f = config.find_field(field);
        let op = f.and_then(|f| f.default_op());
        Draft {
            field: field.to_string().into(),
            operator: op.map(|o| o.key.clone()),
            value: None,
            subs: matches!(op.map(|o| &o.value), Some(OperatorValue::Nested)).then(Vec::new),
        }
    }

    fn at_mut(&mut self, path: &[usize]) -> Option<&mut Draft> {
        match path.split_first() {
            None => Some(self),
            Some((i, rest)) => self.subs.as_mut()?.get_mut(*i)?.at_mut(rest),
        }
    }
}

/// The editor's state, kept while one open lasts.
pub(crate) struct EditorState {
    pub draft: Draft,
    /// Upstream `timezoneID`, for the nested rows' date editors.
    pub timezone_id: Option<SharedString>,
    /// The root, from which focus steps to the value editor once.
    root: gpui::FocusHandle,
    focused: bool,
}

/// Edits one filter, upstream `PowerSearchFilterEditor` (and the popover's
/// content). `key` names the open: a new key starts a fresh draft.
#[derive(IntoElement)]
pub struct PowerSearchFilterEditor {
    id: ElementId,
    config: Rc<PowerSearchConfig>,
    filter: PartialFilter,
    mode: EditorMode,
    on_save: SaveFn,
    on_cancel: CancelFn,
    save_label: Option<SharedString>,
    max_menu_items: Option<usize>,
    read_only: bool,
    opening_query: Option<SharedString>,
    timezone_id: Option<SharedString>,
}

impl PowerSearchFilterEditor {
    /// Upstream `config`, `filter`, `mode`, `onSave` (a filter, or none
    /// to delete it) and `onCancel`.
    pub fn new(
        id: impl Into<ElementId>,
        config: Rc<PowerSearchConfig>,
        filter: PartialFilter,
        mode: EditorMode,
        on_save: impl Fn(Option<PowerSearchFilter>, &mut Window, &mut App) + 'static,
        on_cancel: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        PowerSearchFilterEditor {
            id: id.into(),
            config,
            filter,
            mode,
            on_save: Rc::new(on_save),
            on_cancel: Rc::new(on_cancel),
            timezone_id: None,
            save_label: None,
            max_menu_items: None,
            read_only: false,
            opening_query: None,
        }
    }

    /// A "More" row's typed text (FR29): a text field's value, else the
    /// search editor's opening query.
    pub(crate) fn opening_query(mut self, query: impl Into<SharedString>) -> Self {
        self.opening_query = Some(query.into());
        self
    }

    /// Upstream `saveButtonLabel`: "Apply" by default.
    pub fn save_label(mut self, label: impl Into<SharedString>) -> Self {
        self.save_label = Some(label.into());
        self
    }

    /// The value typeaheads' limit (upstream `maxMenuItems`).
    pub fn max_menu_items(mut self, n: usize) -> Self {
        self.max_menu_items = Some(n);
        self
    }

    /// Upstream `isReadOnly`.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Upstream `timezoneID`: the zone date values read and write in
    /// (FR25, FR26).
    pub fn timezone_id(mut self, zone: impl Into<SharedString>) -> Self {
        self.timezone_id = Some(zone.into());
        self
    }
}

fn field_options(config: &PowerSearchConfig) -> Vec<SelectorEntry> {
    config
        .fields
        .iter()
        .map(|f| {
            SelectorEntry::Option(SelectorOptionData::new(f.key.clone()).label(f.label.clone()))
        })
        .collect()
}

fn operator_options(config: &PowerSearchConfig, field: &str, cx: &App) -> Vec<SelectorEntry> {
    config
        .find_field(field)
        .map(|f| {
            f.operators
                .iter()
                .map(|o| {
                    SelectorEntry::Option(
                        SelectorOptionData::new(o.key.clone()).label(o.label_text(cx)),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

impl RenderOnce for PowerSearchFilterEditor {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let t18 = i18n(cx);
        let config = self.config.clone();
        let state: Entity<EditorState> = {
            let (config, filter, opening) = (
                config.clone(),
                self.filter.clone(),
                self.opening_query.clone(),
            );
            window.use_keyed_state(
                ElementId::Name(format!("{}-editor", self.id).into()),
                cx,
                move |_, cx| {
                    let mut draft = Draft::from_partial(&config, &filter);
                    let text_field = draft
                        .operator
                        .as_ref()
                        .and_then(|o| config.find_operator(&draft.field, o))
                        .is_some_and(|o| {
                            matches!(
                                o.value,
                                OperatorValue::String {
                                    search_source: None,
                                    ..
                                }
                            )
                        });
                    if draft.value.is_none()
                        && text_field
                        && let Some(q) = &opening
                    {
                        draft.value = Some(FilterValue::String(q.clone()));
                    }
                    EditorState {
                        draft,
                        timezone_id: None,
                        root: cx.focus_handle(),
                        focused: false,
                    }
                },
            )
        };
        let draft = state.read(cx).draft.clone();
        {
            let zone = self.timezone_id.clone();
            state.update(cx, |s, _| s.timezone_id = zone);
        }
        let op = draft
            .operator
            .as_ref()
            .and_then(|o| config.find_operator(&draft.field, o))
            .cloned();
        let (on_save, on_cancel) = (self.on_save.clone(), self.on_cancel.clone());
        let read_only = self.read_only;

        // Saving: a complete draft; an \`empty\` operator saves when chosen.
        let save: CancelFn = {
            let (state, config, on_save) = (state.clone(), config.clone(), on_save.clone());
            Rc::new(move |window, cx| {
                let draft = state.read(cx).draft.clone();
                if draft.complete(&config)
                    && let Some(filter) = draft.to_filter(&config)
                {
                    on_save(Some(filter), window, cx);
                }
            })
        };
        let save_if_empty = {
            let (config, on_save) = (config.clone(), on_save.clone());
            move |field: &SharedString,
                  operator: Option<&SharedString>,
                  window: &mut Window,
                  cx: &mut App| {
                if let Some(op) = operator
                    && let Some(OperatorValue::Empty) =
                        config.find_operator(field, op).map(|o| &o.value)
                {
                    let filter =
                        PowerSearchFilter::new(field.clone(), op.clone(), FilterValue::Empty);
                    let on_save = on_save.clone();
                    window.defer(cx, move |window, cx| on_save(Some(filter), window, cx));
                }
            }
        };
        let on_field = {
            let (state, config, save_if_empty) =
                (state.clone(), config.clone(), save_if_empty.clone());
            move |field: Option<SharedString>, window: &mut Window, cx: &mut App| {
                let Some(field) = field else { return };
                let before = state.read(cx).draft.clone();
                let fresh = Draft::fresh(&config, &field);
                let changed = fresh.field != before.field || fresh.operator != before.operator;
                state.update(cx, |s, _| s.draft = fresh.clone());
                if changed {
                    save_if_empty(&fresh.field, fresh.operator.as_ref(), window, cx);
                }
                window.refresh();
            }
        };
        let on_operator = {
            let (state, config, save_if_empty) =
                (state.clone(), config.clone(), save_if_empty.clone());
            move |operator: Option<SharedString>, window: &mut Window, cx: &mut App| {
                let Some(operator) = operator else { return };
                let before = state.read(cx).draft.clone();
                let new_op = config
                    .find_operator(&before.field, &operator)
                    .map(|o| o.value.kind());
                let old_op = before
                    .operator
                    .as_ref()
                    .and_then(|o| config.find_operator(&before.field, o))
                    .map(|o| o.value.kind());
                let keep = new_op.is_some() && new_op == old_op;
                let nested = new_op == Some("nested");
                state.update(cx, |s, _| {
                    s.draft.operator = Some(operator.clone());
                    if !keep {
                        s.draft.value = None;
                    }
                    s.draft.subs = if nested {
                        Some(s.draft.subs.clone().unwrap_or_default())
                    } else {
                        None
                    };
                });
                if before.operator.as_ref() != Some(&operator) {
                    save_if_empty(&before.field, Some(&operator), window, cx);
                }
                window.refresh();
            }
        };
        let complete = draft.complete(&config);

        // Enter saves (not on a button), Escape cancels.
        let on_key = {
            let (save, cancel) = (save.clone(), on_cancel.clone());
            let state = state.clone();
            let config = config.clone();
            move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| match event
                .keystroke
                .key
                .as_str()
            {
                "enter"
                    if state.read(cx).draft.complete(&config) && !window.default_prevented() =>
                {
                    save(window, cx);
                    cx.stop_propagation();
                }
                "escape" if !window.default_prevented() => {
                    cancel(window, cx);
                    cx.stop_propagation();
                }
                _ => {}
            }
        };

        let field_selector = Selector::new(
            ElementId::Name(format!("{}-field", self.id).into()),
            t18.t("@astryx.powersearch.editor.field"),
            field_options(&config),
        )
        .label_hidden(true)
        .disabled(read_only)
        .size(ElementSize::Md)
        .value(Some(draft.field.clone()))
        .on_change(on_field);
        let operators = operator_options(&config, &draft.field, cx);
        let empty = matches!(op.as_ref().map(|o| &o.value), Some(OperatorValue::Empty));
        let operator_count = operators.len();
        let nested = matches!(op.as_ref().map(|o| &o.value), Some(OperatorValue::Nested));

        let footer = {
            let left: AnyElement = if !read_only && self.mode == EditorMode::Edit {
                let on_save = on_save.clone();
                let delete_selector = format!("{}-delete", self.id);
                div()
                    .debug_selector(move || delete_selector)
                    .child(
                        Button::new(
                            ElementId::Name(format!("{}-delete", self.id).into()),
                            t18.t("@astryx.powersearch.editor.delete"),
                        )
                        .variant(ButtonVariant::Ghost)
                        .size(ElementSize::Sm)
                        .on_click(move |_, window, cx| on_save(None, window, cx)),
                    )
                    .into_any_element()
            } else {
                div().into_any_element()
            };
            let cancel = on_cancel.clone();
            let save_btn = save.clone();
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(t.spacing.s2)
                .p(t.spacing.s3)
                .pt(px(0.))
                .child(left)
                .child(
                    div()
                        .flex()
                        .gap(t.spacing.s2)
                        .child(
                            Button::new(
                                ElementId::Name(format!("{}-cancel", self.id).into()),
                                t18.t("@astryx.powersearch.editor.cancel"),
                            )
                            .variant(ButtonVariant::Ghost)
                            .size(ElementSize::Sm)
                            .on_click(move |_, window, cx| cancel(window, cx)),
                        )
                        .child(
                            div()
                                .debug_selector({
                                    let s = format!("{}-apply", self.id);
                                    move || s
                                })
                                .child(
                                    Button::new(
                                        ElementId::Name(format!("{}-apply", self.id).into()),
                                        self.save_label.clone().unwrap_or_else(|| {
                                            t18.t("@astryx.powersearch.editor.apply")
                                        }),
                                    )
                                    .variant(ButtonVariant::Primary)
                                    .size(ElementSize::Sm)
                                    .disabled(!complete)
                                    .on_click(move |_, window, cx| save_btn(window, cx)),
                                ),
                        ),
                )
        };

        let body: AnyElement = if nested {
            let tree = nested_tree(
                &self.id,
                &config,
                &state,
                &draft,
                read_only,
                self.max_menu_items,
                on_operator.clone(),
                true,
                window,
                cx,
            );
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.s2)
                .child(
                    div()
                        .flex()
                        .gap(t.spacing.s2)
                        .child(div().flex_1().min_w_0().child(field_selector)),
                )
                .child(tree)
                .into_any_element()
        } else {
            let mut row = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(t.spacing.s2)
                .child(div().flex_1().min_w(px(120.)).child(field_selector));
            if (operators.len() > 1 || !empty) && !operators.is_empty() {
                row = row.child(
                    div().flex_1().flex_shrink_0().max_w_full().child(
                        Selector::new(
                            ElementId::Name(format!("{}-operator", self.id).into()),
                            t18.t("@astryx.powersearch.editor.operator"),
                            operators,
                        )
                        .label_hidden(true)
                        .disabled(read_only)
                        .size(ElementSize::Md)
                        .value(draft.operator.clone())
                        .on_change(on_operator),
                    ),
                );
            }
            if let Some(op) = op.as_ref().filter(|_| !empty) {
                let on_value: super::value_editor::ValueChangeFn = {
                    let (state, config, on_save) = (state.clone(), config.clone(), on_save.clone());
                    Rc::new(
                        move |value: FilterValue,
                              save_now: bool,
                              window: &mut Window,
                              cx: &mut App| {
                            state.update(cx, |s, _| s.draft.value = Some(value.clone()));
                            let d = state.read(cx).draft.clone();
                            if save_now && let Some(op) = d.operator.clone() {
                                let _ = &config;
                                on_save(
                                    Some(PowerSearchFilter::new(d.field, op, value)),
                                    window,
                                    cx,
                                );
                            }
                            window.refresh();
                        },
                    )
                };
                let editor = value_editor(
                    ValueEditorProps {
                        id: ElementId::Name(
                            format!("{}-value-{}-{}", self.id, draft.field, op.key).into(),
                        ),
                        timezone_id: self.timezone_id.clone(),
                        op: op.value.clone(),
                        value: draft.value.clone(),
                        on_change: on_value,
                        on_enter: Some(save.clone()),
                        max_menu_items: self.max_menu_items,
                        disabled: read_only,
                        sheet: false,
                        field_label: None,
                        on_unstage: None,
                        opening_query: self.opening_query.clone(),
                    },
                    window,
                    cx,
                );
                row = row.children(editor.map(|e| div().flex_grow(2.).min_w_0().child(e)));
            }
            row.into_any_element()
        };
        // Upstream's ValueEditorCell: once drawn, focus goes to the value
        // editor (else the field selector): from the root, one stop per
        // control before it.
        let operator_shown = !nested && (operator_count > 1 || !empty) && operator_count > 0;
        let value_shown = !nested && !empty && op.is_some();
        let steps = 1 + usize::from(operator_shown && value_shown) + usize::from(value_shown);
        let root = state.read(cx).root.clone();
        let focus_probe = (!state.read(cx).focused).then(|| {
            let (state, root) = (state.clone(), root.clone());
            gpui::canvas(
                |_, _, _| {},
                move |_, _, window, cx| {
                    let (state, root) = (state.clone(), root.clone());
                    window.defer(cx, move |window, cx| {
                        if state.read(cx).focused {
                            return;
                        }
                        state.update(cx, |s, _| s.focused = true);
                        root.focus(window, cx);
                        for _ in 0..steps {
                            window.focus_next(cx);
                        }
                    });
                },
            )
            .absolute()
            .size_0()
        });
        let selector = format!("{}-editor", self.id);
        div()
            .id(ElementId::Name(format!("{}-editor-root", self.id).into()))
            .track_focus(&root)
            .children(focus_probe)
            .debug_selector(move || selector)
            .overflow_hidden()
            .on_key_down(on_key)
            .child(div().p(t.spacing.s4).child(body))
            .when(!empty, |el| el.child(footer))
    }
}

/// The nested editor: a root row with the group operator, then a row per
/// sub-filter (field, operator, value, remove) and "+ Add filter".
#[allow(clippy::too_many_arguments)]
fn nested_tree(
    id: &ElementId,
    config: &Rc<PowerSearchConfig>,
    state: &Entity<EditorState>,
    draft: &Draft,
    read_only: bool,
    max_menu_items: Option<usize>,
    on_operator: impl Fn(Option<SharedString>, &mut Window, &mut App) + Clone + 'static,
    root_picker: bool,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let t18 = i18n(cx);
    fn rows(
        id: &ElementId,
        config: &Rc<PowerSearchConfig>,
        state: &Entity<EditorState>,
        subs: &[Draft],
        parent: Vec<usize>,
        read_only: bool,
        max_menu_items: Option<usize>,
        cx: &App,
    ) -> Vec<TreeListItemData> {
        let t18 = i18n(cx);
        let mut items: Vec<TreeListItemData> = subs
            .iter()
            .enumerate()
            .map(|(i, sub)| {
                let mut path = parent.clone();
                path.push(i);
                let key = path
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join("-");
                let label = config
                    .find_field(&sub.field)
                    .map_or(sub.field.clone(), |f| f.label.clone());
                let mut item = TreeListItemData::new(format!("filter-{key}"), label).expanded(true);
                {
                    let (config, state, path, sub, id) = (
                        config.clone(),
                        state.clone(),
                        path.clone(),
                        sub.clone(),
                        id.clone(),
                    );
                    item = item.label_content(move |window, cx| {
                        sub_row(
                            &id,
                            &config,
                            &state,
                            &path,
                            &sub,
                            read_only,
                            max_menu_items,
                            window,
                            cx,
                        )
                    });
                }
                if !read_only {
                    let (state, path) = (state.clone(), path.clone());
                    let remove = t18.t("@astryx.powersearch.editor.removeFilter");
                    let rid = ElementId::Name(format!("{id}-remove-{key}").into());
                    item = item.end_content(move |_, _| {
                        let (state, path) = (state.clone(), path.clone());
                        IconButton::new(
                            rid.clone(),
                            remove.clone(),
                            Icon::new("close").size(IconSize::Sm),
                        )
                        .variant(ButtonVariant::Ghost)
                        .size(ElementSize::Sm)
                        .on_click(move |_, window, cx| {
                            state.update(cx, |s, _| {
                                let (last, parent) = path.split_last().unwrap();
                                if let Some(list) =
                                    s.draft.at_mut(parent).and_then(|d| d.subs.as_mut())
                                    && *last < list.len()
                                {
                                    list.remove(*last);
                                }
                            });
                            window.refresh();
                        })
                        .into_any_element()
                    });
                }
                if let Some(children) = &sub.subs {
                    let mut kids = rows(
                        id,
                        config,
                        state,
                        children,
                        path.clone(),
                        read_only,
                        max_menu_items,
                        cx,
                    );
                    if !read_only {
                        kids.push(add_row(id, config, state, path.clone(), cx));
                    }
                    item = item.children(kids);
                }
                item
            })
            .collect();
        if parent.is_empty() && !read_only {
            items.push(add_row(id, config, state, Vec::new(), cx));
        }
        items
    }
    let children = rows(
        id,
        config,
        state,
        draft.subs.as_deref().unwrap_or(&[]),
        Vec::new(),
        read_only,
        max_menu_items,
        cx,
    );
    let operators = operator_options(config, &draft.field, cx);
    let current = draft.operator.clone();
    let group_label: SharedString = config
        .find_field(&draft.field)
        .and_then(|f| {
            current
                .as_ref()
                .and_then(|o| f.find_operator(o))
                .or(f.operators.first())
        })
        .map(|o| o.label_text(cx))
        .unwrap_or_else(|| t18.t("@astryx.powersearch.editor.group"));
    let mut root = TreeListItemData::new("nested-root", group_label.clone())
        .expanded(true)
        .children(children);
    if root_picker && operators.len() > 1 {
        let (id, on_operator) = (id.clone(), on_operator.clone());
        root = root.label_content(move |_, cx| {
            Selector::new(
                ElementId::Name(format!("{id}-group-operator").into()),
                i18n(cx).t("@astryx.powersearch.editor.groupOperator"),
                operators.clone(),
            )
            .label_hidden(true)
            .disabled(read_only)
            .size(ElementSize::Md)
            .value(current.clone())
            .on_change(on_operator.clone())
            .into_any_element()
        });
    }
    TreeList::new(ElementId::Name(format!("{id}-nested").into()), [root])
        .aria_label(group_label)
        .into_any_element()
}

fn add_row(
    id: &ElementId,
    config: &Rc<PowerSearchConfig>,
    state: &Entity<EditorState>,
    parent: Vec<usize>,
    cx: &App,
) -> TreeListItemData {
    let key = parent
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join("-");
    let label = i18n(cx).t("@astryx.powersearch.editor.addFilter");
    let (config, state, bid) = (
        config.clone(),
        state.clone(),
        ElementId::Name(format!("{id}-add-{key}").into()),
    );
    let text = label.clone();
    TreeListItemData::new(format!("{key}-add"), label).label_content(move |_, _| {
        let (config, state, parent) = (config.clone(), state.clone(), parent.clone());
        Button::new(bid.clone(), text.clone())
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .on_click(move |_, window, cx| {
                let Some(first) = config.fields.first() else {
                    return;
                };
                let fresh = Draft::fresh(&config, &first.key);
                state.update(cx, |s, _| {
                    if let Some(d) = s.draft.at_mut(&parent) {
                        d.subs.get_or_insert_with(Vec::new).push(fresh);
                    }
                });
                window.refresh();
            })
            .into_any_element()
    })
}

/// One sub-filter's row (upstream `NestedSubFilterRow`).
#[allow(clippy::too_many_arguments)]
fn sub_row(
    id: &ElementId,
    config: &Rc<PowerSearchConfig>,
    state: &Entity<EditorState>,
    path: &[usize],
    sub: &Draft,
    read_only: bool,
    max_menu_items: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = rustryx_theme::current(window, cx);
    let t = theme.tokens();
    let t18 = i18n(cx);
    let key = path
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join("-");
    let op = sub
        .operator
        .as_ref()
        .and_then(|o| config.find_operator(&sub.field, o))
        .cloned();
    let update = {
        let (state, path) = (state.clone(), path.to_vec());
        move |f: &dyn Fn(&mut Draft), window: &mut Window, cx: &mut App| {
            state.update(cx, |s, _| {
                if let Some(d) = s.draft.at_mut(&path) {
                    f(d);
                }
            });
            window.refresh();
        }
    };
    let on_field = {
        let (config, update) = (config.clone(), update.clone());
        move |field: Option<SharedString>, window: &mut Window, cx: &mut App| {
            if let Some(field) = field {
                let fresh = Draft::fresh(&config, &field);
                update(&|d| *d = fresh.clone(), window, cx);
            }
        }
    };
    let on_operator = {
        let (config, update, field, old) = (
            config.clone(),
            update.clone(),
            sub.field.clone(),
            op.as_ref().map(|o| o.value.kind()),
        );
        move |operator: Option<SharedString>, window: &mut Window, cx: &mut App| {
            let Some(operator) = operator else { return };
            let new = config
                .find_operator(&field, &operator)
                .map(|o| o.value.kind());
            let keep = new.is_some() && new == old;
            let nested = new == Some("nested");
            update(
                &|d| {
                    d.operator = Some(operator.clone());
                    if !keep {
                        d.value = None;
                    }
                    d.subs = if nested {
                        Some(d.subs.clone().unwrap_or_default())
                    } else {
                        None
                    };
                },
                window,
                cx,
            );
        }
    };
    let operators = operator_options(config, &sub.field, cx);
    let mut row = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.s2)
        .w_full()
        .child(
            div().flex_none().w(px(200.)).max_w_full().child(
                Selector::new(
                    ElementId::Name(format!("{id}-sub-field-{key}").into()),
                    t18.t("@astryx.powersearch.editor.field"),
                    field_options(config),
                )
                .label_hidden(true)
                .disabled(read_only)
                .size(ElementSize::Md)
                .value(Some(sub.field.clone()))
                .on_change(on_field),
            ),
        );
    if !operators.is_empty() {
        row = row.child(
            div().flex_none().w(px(180.)).max_w_full().child(
                Selector::new(
                    ElementId::Name(format!("{id}-sub-operator-{key}").into()),
                    t18.t("@astryx.powersearch.editor.operator"),
                    operators,
                )
                .label_hidden(true)
                .disabled(read_only)
                .size(ElementSize::Md)
                .value(sub.operator.clone())
                .on_change(on_operator),
            ),
        );
    }
    if let Some(op) =
        op.filter(|o| !matches!(o.value, OperatorValue::Empty | OperatorValue::Nested))
    {
        let on_value: super::value_editor::ValueChangeFn = {
            let update = update.clone();
            Rc::new(
                move |value: FilterValue, _: bool, window: &mut Window, cx: &mut App| {
                    update(&|d| d.value = Some(value.clone()), window, cx);
                },
            )
        };
        let editor = value_editor(
            ValueEditorProps {
                id: ElementId::Name(
                    format!("{id}-sub-value-{key}-{}-{}", sub.field, op.key).into(),
                ),
                timezone_id: state.read(cx).timezone_id.clone(),
                op: op.value.clone(),
                value: sub.value.clone(),
                on_change: on_value,
                on_enter: None,
                max_menu_items,
                disabled: read_only,
                sheet: false,
                field_label: None,
                on_unstage: None,
                opening_query: None,
            },
            window,
            cx,
        );
        row = row.children(editor.map(|e| div().flex_grow(2.).flex_shrink(1.).min_w_0().child(e)));
    }
    row.into_any_element()
}

/// The value step's nested editor (FR14): the sub-filter tree, its root
/// reading the operator's label (the step's Operator group picks it). It
/// reports a complete group as the value, else none.
#[allow(clippy::too_many_arguments)]
pub(crate) fn nested_step(
    id: &ElementId,
    config: &Rc<PowerSearchConfig>,
    field: &SharedString,
    operator: &SharedString,
    initial: Option<FilterValue>,
    read_only: bool,
    max_menu_items: Option<usize>,
    timezone_id: Option<SharedString>,
    on_value: NestedValueFn,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let state: Entity<EditorState> = {
        let (config, field, operator) = (config.clone(), field.clone(), operator.clone());
        window.use_keyed_state(
            ElementId::Name(format!("{id}-nested-step").into()),
            cx,
            move |_, cx| EditorState {
                draft: Draft::from_partial(
                    &config,
                    &PartialFilter {
                        field,
                        operator: Some(operator),
                        value: initial,
                        read_only: false,
                    },
                ),
                timezone_id: None,
                root: cx.focus_handle(),
                focused: true,
            },
        )
    };
    state.update(cx, |s, _| s.timezone_id = timezone_id);
    if state.read(cx).draft.operator.as_ref() != Some(operator) {
        state.update(cx, |s, _| {
            s.draft.operator = Some(operator.clone());
            s.draft.subs.get_or_insert_with(Vec::new);
        });
    }
    let draft = state.read(cx).draft.clone();
    // The step stages the group while it is complete.
    let value = draft
        .complete(config)
        .then(|| draft.to_filter(config))
        .flatten()
        .map(|f| f.value);
    let report = on_value.clone();
    window.defer(cx, move |window, cx| report(value, window, cx));
    nested_tree(
        id,
        config,
        &state,
        &draft,
        read_only,
        max_menu_items,
        |_, _, _| {},
        false,
        window,
        cx,
    )
}
