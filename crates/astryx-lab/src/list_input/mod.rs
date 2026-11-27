//! Upstream lab `ListInput`: a compact editor for short collections of consistent,
//! simple records. It owns the list semantics, the Add and Remove controls, handle-only
//! reordering, the entry motion, focus restoration, announcements and the list, record
//! and field validation placement; the app keeps the controlled data and renders each
//! field with standard Astryx inputs.

pub mod examples;
#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use astryx_components::button::{Button, ButtonVariant, IconButton};
use astryx_components::container_reveal::{
    ContentRevealOptions, UseContainerRevealOptions, use_container_reveal,
};
use astryx_components::containers::EmptyState;
use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use astryx_components::field::{
    Field, FieldStatus, FieldStatusVariant, InputStatus, StatusPlacement,
};
use astryx_components::icon::{Icon, IconSize};
use astryx_components::reorderable::{ItemKey, Reorderable};
use astryx_components::table::ColumnWidth;
use astryx_i18n::Arg;
use astryx_primitives::{
    ElementSize, Politeness, announce, dev_warn_once, prefers_reduced_motion, request_focus,
};
use gpui::prelude::*;
use web_time::Instant;

use crate::transfer_list::responsive::Responsive;
use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, LayoutId,
    Pixels, Point, ScrollHandle, SharedString, Window, div, point, px, relative,
};

/// Core List's reorder handle is 28 px wide for every pointer.
const REORDER_HANDLE_SIZE: f32 = 28.;
/// Upstream's `@container list-input (max-width: 640px)`: narrower, records stack.
const STACK_BELOW_PX: f32 = 640.;
/// A column with no width: a proportional column with a 140 px floor.
const DEFAULT_COLUMN_MIN_PX: f32 = 140.;

/// A record mutation (upstream `ListInputChange`).
#[derive(Clone, Debug, PartialEq)]
pub enum ListInputChange<T> {
    /// A record was added at `index`.
    Add { item: T, index: usize },
    /// The record at `index` was replaced.
    Update {
        item: T,
        previous_item: T,
        index: usize,
        column_key: Option<SharedString>,
    },
    /// The record at `index` was removed.
    Remove { item: T, index: usize },
}

type UpdateFn<T> = Rc<dyn Fn(T, Option<SharedString>, &mut Window, &mut App)>;

/// What a column's renderer gets for one cell (upstream `ListInputRenderContext`).
#[derive(Clone)]
pub struct ListInputRenderContext<T> {
    /// The record.
    pub item: T,
    /// Its visual position.
    pub index: usize,
    /// The cell's accessible label, with the record's position past the first.
    pub label: SharedString,
    /// True: ListInput owns the visible column label.
    pub is_label_hidden: bool,
    /// The field's validation status.
    pub status: Option<InputStatus>,
    /// Pass to the control: status shows in its tooltip (upstream `statusVariant`).
    pub status_placement: StatusPlacement,
    /// Whether the control is disabled.
    pub is_disabled: bool,
    /// Whether a list operation is in progress.
    pub is_loading: bool,
    /// The id to give the cell's control: ListInput focuses the first column's control
    /// of an added record through it (upstream queries the cell for its first focusable
    /// element).
    pub id: ElementId,
    update: UpdateFn<T>,
}

impl<T> ListInputRenderContext<T> {
    /// Upstream `updateItem`: replaces this record in the controlled list. Without a
    /// `column_key`, the change names this column.
    pub fn update_item(
        &self,
        next: T,
        column_key: Option<&str>,
        window: &mut Window,
        cx: &mut App,
    ) {
        (self.update)(
            next,
            column_key.map(|k| SharedString::from(k.to_string())),
            window,
            cx,
        );
    }
}

type RenderInputFn<T> = Rc<dyn Fn(&ListInputRenderContext<T>, &mut Window, &mut App) -> AnyElement>;

/// One column (upstream `ListInputColumn`).
pub struct ListInputColumn<T> {
    /// Upstream `key`: the stable column id, also passed to `field_status`.
    pub key: SharedString,
    /// Upstream `header`: the field label on the first record, repeated when records
    /// stack.
    pub header: SharedString,
    /// Upstream `width`: `proportional()` or `pixel()`; a proportional column with a
    /// 140 px floor by default.
    pub width: Option<ColumnWidth>,
    render: RenderInputFn<T>,
}

impl<T> Clone for ListInputColumn<T> {
    fn clone(&self) -> Self {
        ListInputColumn {
            key: self.key.clone(),
            header: self.header.clone(),
            width: self.width,
            render: self.render.clone(),
        }
    }
}

impl<T> ListInputColumn<T> {
    /// A column and upstream `renderInput`, its cell's control.
    pub fn new(
        key: impl Into<SharedString>,
        header: impl Into<SharedString>,
        render_input: impl Fn(&ListInputRenderContext<T>, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        ListInputColumn {
            key: key.into(),
            header: header.into(),
            width: None,
            render: Rc::new(render_input),
        }
    }

    /// Upstream `width`.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = Some(width);
        self
    }
}

/// How a column's track sizes (upstream `resolveColumnTrack`): a fixed width, or a
/// share of the space with a floor.
fn column_track<E: Styled>(el: E, width: Option<ColumnWidth>) -> E {
    match width {
        Some(ColumnWidth::Pixel(w)) => el.flex_none().w(px(w)),
        Some(ColumnWidth::Proportional { value, min_width }) => el
            .flex_basis(px(0.))
            .flex_grow(value)
            .flex_shrink(1.)
            .min_w(px(min_width.unwrap_or(DEFAULT_COLUMN_MIN_PX))),
        None => el
            .flex_basis(px(0.))
            .flex_grow(1.)
            .flex_shrink(1.)
            .min_w(px(DEFAULT_COLUMN_MIN_PX)),
    }
}

type ChangeFn<T> = Rc<dyn Fn(Vec<T>, ListInputChange<T>, &mut Window, &mut App)>;
type KeyFn<T> = Rc<dyn Fn(&T) -> SharedString>;
type ItemStatusFn<T> = Rc<dyn Fn(&T, usize) -> Option<InputStatus>>;
type FieldStatusFn<T> = Rc<dyn Fn(&T, &str, usize) -> Option<InputStatus>>;

/// Where focus goes after a mutation (upstream `PendingFocus`).
#[derive(Clone, Debug, PartialEq)]
enum PendingFocus {
    FirstField(SharedString),
    Remove(SharedString),
    Add,
}

struct ListState {
    pending_focus: Option<PendingFocus>,
    /// A mutation waiting for its value (upstream `pendingMutationMotion`): the
    /// participants' boxes before it and the keys it expects.
    pending_motion: Option<PendingMotion>,
    /// The mutation whose value this frame shows: its participants move from their
    /// boxes before it.
    armed: Option<PendingMotion>,
    /// Every participant's box last frame (`item:{key}`, `action:add`, `state:empty`).
    bounds: HashMap<SharedString, Bounds<Pixels>>,
    /// Participants entering, and when they started.
    entering: HashMap<SharedString, Instant>,
    /// Participants gliding from where they were: the offset and when it started.
    moving: HashMap<SharedString, (Point<Pixels>, Instant)>,
    /// The scroller around Add, for keeping Add under the pointer.
    scroller: Option<ScrollHandle>,
    /// A participant of this frame's mutation changed size.
    size_changed: bool,
    narrow: bool,
}

/// A mutation's motion (upstream `PendingMutationMotion` and `PendingAddScrollAnchor`).
#[derive(Clone)]
struct PendingMotion {
    before: HashMap<SharedString, Bounds<Pixels>>,
    expected: Vec<SharedString>,
    /// Some participant enters: the others keep their place (upstream's
    /// `hasEnteringParticipant`).
    entering_any: bool,
    /// Add's top before a pointer's add, to keep Add under the pointer.
    add_top: Option<Pixels>,
}

const ADD_KEY: &str = "action:add";
const EMPTY_KEY: &str = "state:empty";

fn item_key(key: &str) -> SharedString {
    format!("item:{key}").into()
}

/// A compact editor for short collections of consistent, simple records (upstream lab
/// `ListInput`).
#[derive(IntoElement)]
pub struct ListInput<T: Clone + 'static> {
    id: ElementId,
    label: SharedString,
    description: Option<SharedString>,
    value: Vec<T>,
    on_change: Option<ChangeFn<T>>,
    get_item_key: KeyFn<T>,
    create_item: Rc<dyn Fn() -> T>,
    columns: Vec<ListInputColumn<T>>,
    item_name: Option<SharedString>,
    status: Option<InputStatus>,
    item_status: Option<ItemStatusFn<T>>,
    field_status: Option<FieldStatusFn<T>>,
    reorderable: Option<Reorderable>,
    disabled: bool,
    loading: bool,
    max_items: Option<usize>,
    label_hidden: bool,
    optional: bool,
    required: bool,
}

impl<T: Clone + 'static> ListInput<T> {
    /// Upstream `label`, the controlled `value`, `getItemKey` (a stable key per record,
    /// for focus and reordering), `createItem` (the record Add makes) and `columns`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: Vec<T>,
        get_item_key: impl Fn(&T) -> SharedString + 'static,
        create_item: impl Fn() -> T + 'static,
        columns: Vec<ListInputColumn<T>>,
    ) -> Self {
        ListInput {
            id: id.into(),
            label: label.into(),
            description: None,
            value,
            on_change: None,
            get_item_key: Rc::new(get_item_key),
            create_item: Rc::new(create_item),
            columns,
            item_name: None,
            status: None,
            item_status: None,
            field_status: None,
            reorderable: None,
            disabled: false,
            loading: false,
            max_items: None,
            label_hidden: false,
            optional: false,
            required: false,
        }
    }

    /// Upstream `onChange`: every record mutation, with the next value and its detail.
    pub fn on_change(
        mut self,
        f: impl Fn(Vec<T>, ListInputChange<T>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `itemName`: the singular name in action labels and announcements
    /// ("item").
    pub fn item_name(mut self, name: impl Into<SharedString>) -> Self {
        self.item_name = Some(name.into());
        self
    }

    /// Upstream `status`: the whole list's validation status.
    pub fn status(mut self, status: InputStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Upstream `getItemStatus`: a status shown across one record.
    pub fn item_status(mut self, f: impl Fn(&T, usize) -> Option<InputStatus> + 'static) -> Self {
        self.item_status = Some(Rc::new(f));
        self
    }

    /// Upstream `getFieldStatus`: a status passed to one field's renderer.
    pub fn field_status(
        mut self,
        f: impl Fn(&T, &str, usize) -> Option<InputStatus> + 'static,
    ) -> Self {
        self.field_status = Some(Rc::new(f));
        self
    }

    /// Upstream `reorderable`: the object `create_reorderable` makes, keyed by
    /// `get_item_key`. Each record ends with a reorder handle; its `on_reorder` gets
    /// each move, `to` being the record's index in `value` after it.
    pub fn reorderable(mut self, reorderable: Reorderable) -> Self {
        self.reorderable = Some(reorderable);
        self
    }

    /// Upstream `isDisabled`: disables fields and mutation controls.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `isLoading`: marks the list busy and prevents mutations.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Upstream `maxItems`: reaching it disables, but does not remove, Add.
    pub fn max_items(mut self, max: usize) -> Self {
        self.max_items = Some(max);
        self
    }

    /// Upstream `isLabelHidden`.
    pub fn label_hidden(mut self, hidden: bool) -> Self {
        self.label_hidden = hidden;
        self
    }

    /// Upstream `isOptional`.
    pub fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Upstream `isRequired`.
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }
}

fn part(id: &ElementId, part: impl std::fmt::Display) -> ElementId {
    ElementId::Name(format!("{id}-{part}").into())
}

/// The id ListInput gives a record's control in `column` (`ListInputRenderContext::id`).
pub fn field_id(list: &ElementId, key: &str, column: &str) -> ElementId {
    part(list, format!("field-{key}-{column}"))
}

/// The three lines of a reorder handle, 28 px square (Core List's own).
fn grip(color: gpui::Hsla) -> gpui::Div {
    let bar = || div().w(px(20.)).h(px(2.)).rounded(px(1.)).bg(color);
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .flex_none()
        .w(px(REORDER_HANDLE_SIZE))
        .h(px(REORDER_HANDLE_SIZE))
        .rounded(px(4.))
        .child(bar())
        .child(bar())
        .child(bar())
}

impl<T: Clone + 'static> RenderOnce for ListInput<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens();
        let id = self.id.clone();
        let state = window.use_keyed_state(part(&id, "list-input"), cx, |_, _| {
            Rc::new(RefCell::new(ListState {
                pending_focus: None,
                pending_motion: None,
                armed: None,
                bounds: HashMap::new(),
                entering: HashMap::new(),
                moving: HashMap::new(),
                scroller: None,
                size_changed: false,
                narrow: false,
            }))
        });
        let shared = state.read(cx).clone();
        let keys: Vec<SharedString> = self.value.iter().map(|i| (self.get_item_key)(i)).collect();

        // Keys must be unique and stable; column keys too.
        if let Some(dup) = keys
            .iter()
            .enumerate()
            .find(|(i, k)| keys[..*i].contains(k))
            .map(|(_, k)| k)
        {
            let dup = dup.clone();
            dev_warn_once(&format!("list-input-duplicate-key-{dup}"), move || {
                format!(
                    "ListInput: getItemKey returned duplicate key \"{dup}\". Keys must be unique and stable."
                )
            });
        }
        if let Some(dup) = self
            .columns
            .iter()
            .enumerate()
            .find(|(i, c)| self.columns[..*i].iter().any(|o| o.key == c.key))
            .map(|(_, c)| c.key.clone())
        {
            dev_warn_once(&format!("list-input-duplicate-column-{dup}"), move || {
                format!(
                    "ListInput: columns contains duplicate key \"{dup}\". Column keys must be unique."
                )
            });
        }

        // The motion of a mutation whose value arrived (upstream's layout effect): a
        // participant that was not there enters from the theme's spacing-2, and when none
        // enters, the others glide from where they were. A value that is not the one the
        // mutation made drops its motion.
        let reduced = prefers_reduced_motion(cx);
        let duration = t.duration.fast_max;
        let now = cx.background_executor().now();
        {
            let mut s = shared.borrow_mut();
            s.armed = None;
            s.size_changed = false;
            if let Some(pending) = s.pending_motion.take()
                && pending.expected == keys
            {
                let mut participants: Vec<SharedString> =
                    keys.iter().map(|k| item_key(k)).collect();
                participants.push(ADD_KEY.into());
                if keys.is_empty() {
                    participants.push(EMPTY_KEY.into());
                }
                let entering: Vec<SharedString> = participants
                    .into_iter()
                    .filter(|k| !pending.before.contains_key(k))
                    .collect();
                let motion = !reduced && !duration.is_zero();
                if motion {
                    for key in &entering {
                        s.entering.insert(key.clone(), now);
                    }
                }
                s.armed = Some(PendingMotion {
                    entering_any: !entering.is_empty() || !motion,
                    ..pending
                });
            }
            s.entering
                .retain(|_, started| now.saturating_duration_since(*started) < duration);
            s.moving
                .retain(|_, (_, started)| now.saturating_duration_since(*started) < duration);
            if reduced {
                s.entering.clear();
                s.moving.clear();
            }
        }

        // Focus after a mutation, once its value arrives.
        if let Some(pending) = shared.borrow_mut().pending_focus.take() {
            match &pending {
                PendingFocus::Add => request_focus(&part(&id, "add"), cx),
                PendingFocus::Remove(key) if keys.contains(key) => {
                    request_focus(&part(&id, format!("remove-{key}")), cx)
                }
                PendingFocus::FirstField(key) if keys.contains(key) => {
                    if let Some(column) = self.columns.first() {
                        request_focus(&field_id(&id, key, &column.key), cx);
                    }
                }
                _ => {}
            }
        }

        let last_narrow = shared.borrow().narrow;
        let this = Rc::new(self);
        let group = {
            let (this, shared) = (this.clone(), shared.clone());
            // Narrower than 640 px, the records stack (upstream's container query): the
            // group lays out for the width it had last frame, and again before it paints
            // on a frame whose width crosses the line.
            Responsive::new(
                px(STACK_BELOW_PX),
                last_narrow,
                move |narrow, window, cx| {
                    shared.borrow_mut().narrow = narrow;
                    this.build(narrow, &shared, &keys, window, cx)
                },
            )
        };
        let mut field = Field::new(id.clone(), this.label.clone())
            .label_hidden(this.label_hidden)
            .optional(this.optional)
            .required(this.required)
            .disabled(this.disabled)
            .status_placement(StatusPlacement::Detached);
        if let Some(d) = this.description.clone() {
            field = field.description(d);
        }
        if let Some(s) = this.status.clone() {
            field = field.status(s);
        }
        field.child(group)
    }
}

impl<T: Clone + 'static> ListInput<T> {
    /// The group at `narrow`: the records and Add.
    fn build(
        &self,
        narrow: bool,
        shared: &Rc<RefCell<ListState>>,
        keys: &[SharedString],
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens();
        let i18n = astryx_i18n::current(cx);
        let id = self.id.clone();
        let item_name = self.item_name.clone().unwrap_or_else(|| "item".into());
        let duration = t.duration.fast_max;
        let ease = t.ease.standard;
        let now = cx.background_executor().now();
        let list_gap =
            if narrow { t.spacing.s8 } else { t.spacing.s2 }.to_pixels(window.rem_size());
        let mutations_disabled = self.disabled || self.loading;
        let reached_max = self.max_items.is_some_and(|max| self.value.len() >= max);
        let attached = self
            .reorderable
            .as_ref()
            .map(|r| r.attach(part(&id, "records"), window, cx));
        let handle_cell = attached.is_some();
        let rem = window.rem_size();
        let gap = t.spacing.s1.to_pixels(rem);
        let control_width = t.size.element_md.to_pixels(rem)
            + if handle_cell {
                gap + px(REORDER_HANDLE_SIZE)
            } else {
                px(0.)
            };

        let value = Rc::new(self.value.clone());
        let on_change = self.on_change.clone();
        let get_key = self.get_item_key.clone();
        let count = value.len();

        let mut records: Vec<AnyElement> = Vec::new();
        for (index, item) in value.iter().enumerate() {
            let key = keys[index].clone();
            let item_status = self.item_status.as_ref().and_then(|f| f(item, index));
            let mut fields = div().flex().min_w_0().flex_1();
            fields = if narrow {
                fields.flex_col().gap(t.spacing.s2)
            } else {
                fields.flex_row().items_end().gap(t.spacing.s1)
            };
            for column in &self.columns {
                let status = self
                    .field_status
                    .as_ref()
                    .and_then(|f| f(item, &column.key, index));
                let repeated = index != 0;
                let cell_label: SharedString = if repeated {
                    i18n.format(
                        "@astryx.listInput.fieldLabelWithPosition",
                        &[
                            ("header", Arg::from(column.header.as_ref())),
                            ("itemName", Arg::from(item_name.as_ref())),
                            ("position", Arg::Num((index + 1) as f64)),
                            ("total", Arg::Num(count as f64)),
                        ],
                    )
                } else {
                    column.header.clone()
                };
                let update: UpdateFn<T> = {
                    let (value, on_change, item, column_key) = (
                        value.clone(),
                        on_change.clone(),
                        item.clone(),
                        column.key.clone(),
                    );
                    Rc::new(
                        move |next: T, change_key: Option<SharedString>, window, cx| {
                            if mutations_disabled {
                                return;
                            }
                            let mut next_value = (*value).clone();
                            next_value[index] = next.clone();
                            if let Some(f) = &on_change {
                                f(
                                    next_value,
                                    ListInputChange::Update {
                                        item: next,
                                        previous_item: item.clone(),
                                        index,
                                        column_key: Some(
                                            change_key.unwrap_or_else(|| column_key.clone()),
                                        ),
                                    },
                                    window,
                                    cx,
                                );
                            }
                        },
                    )
                };
                let context = ListInputRenderContext {
                    item: item.clone(),
                    index,
                    label: cell_label,
                    is_label_hidden: true,
                    status,
                    status_placement: StatusPlacement::Tooltip,
                    is_disabled: mutations_disabled,
                    is_loading: self.loading,
                    id: field_id(&id, &key, &column.key),
                    update,
                };
                let content = (column.render)(&context, window, cx);
                // The column's label shows on the first record, and on every record
                // once they stack. Assistive technology reads the control's own label.
                let show_label = !repeated || narrow;
                let label = div()
                    .when(!show_label, |d| d.hidden())
                    .mb(if narrow { t.spacing.s1 } else { t.spacing.s0_5 })
                    .text_color(t.color.text_primary)
                    .text_size(t.text.label.size)
                    .line_height(relative(t.text.label.leading))
                    .font_weight(t.font.weight.medium)
                    .child(column.header.clone());
                let cell = div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(label)
                    .child(div().min_w_0().child(content));
                fields = fields.child(if narrow {
                    cell.w_full()
                } else {
                    column_track(cell, column.width)
                });
            }

            // Remove, and the handle when reorderable, revealed on hover or focus.
            let reveal = use_container_reveal(
                part(&id, format!("row-{key}")),
                UseContainerRevealOptions::default(),
                window,
                cx,
            );
            let remove_label = i18n.format(
                "@astryx.listInput.removeItem",
                &[
                    ("itemName", Arg::from(item_name.as_ref())),
                    ("position", Arg::Num((index + 1) as f64)),
                ],
            );
            let tooltip = if mutations_disabled {
                i18n.format(
                    "@astryx.listInput.removeUnavailable",
                    &[("itemName", Arg::from(item_name.as_ref()))],
                )
            } else {
                remove_label.clone()
            };
            let remove = {
                let (value, on_change, get_key, shared) = (
                    value.clone(),
                    on_change.clone(),
                    get_key.clone(),
                    shared.clone(),
                );
                let (item_name, removed_key) = (item_name.clone(), key.clone());
                let selector = format!("{id}-remove-{key}");
                div().debug_selector(move || selector).child(
                    IconButton::new(
                        part(&id, format!("remove-{key}")),
                        remove_label,
                        Icon::new("close").size(IconSize::Sm),
                    )
                    .variant(ButtonVariant::Ghost)
                    .size(ElementSize::Md)
                    .disabled(mutations_disabled)
                    .tooltip(tooltip)
                    .on_click(move |_, window, cx| {
                        if mutations_disabled {
                            return;
                        }
                        let Some(index) = value.iter().position(|c| get_key(c) == removed_key)
                        else {
                            return;
                        };
                        let removed = value[index].clone();
                        let next: Vec<T> = value
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| *i != index)
                            .map(|(_, v)| v.clone())
                            .collect();
                        {
                            let mut s = shared.borrow_mut();
                            s.pending_focus =
                                Some(match next.get(index.min(next.len().saturating_sub(1))) {
                                    Some(focus) if !next.is_empty() => {
                                        PendingFocus::Remove(get_key(focus))
                                    }
                                    _ => PendingFocus::Add,
                                });
                            s.pending_motion = Some(PendingMotion {
                                before: s.bounds.clone(),
                                expected: next.iter().map(|v| get_key(v)).collect(),
                                entering_any: false,
                                add_top: None,
                            });
                        }
                        if let Some(f) = &on_change {
                            f(
                                next,
                                ListInputChange::Remove {
                                    item: removed,
                                    index,
                                },
                                window,
                                cx,
                            );
                        }
                        let text = astryx_i18n::current(cx).format(
                            "@astryx.listInput.announceRemoved",
                            &[
                                ("itemName", Arg::from(item_name.as_ref())),
                                ("position", Arg::Num((index + 1) as f64)),
                            ],
                        );
                        announce(text, Politeness::Polite, window, cx);
                    }),
                )
            };
            let handle = attached.as_ref().and_then(|a| {
                let item_key = ItemKey::Text(key.clone());
                let movable = self
                    .reorderable
                    .as_ref()
                    .is_some_and(|r| r.can_move(&item_key));
                movable.then(|| {
                    let grip = grip(if mutations_disabled {
                        t.color.text_disabled
                    } else {
                        t.color.text_secondary
                    });
                    // A disabled or loading list keeps its handles focusable but inert.
                    a.handle_disabled(item_key, grip, mutations_disabled)
                })
            });
            let controls = reveal
                .content(ContentRevealOptions {
                    layout_preserved: true,
                    ..ContentRevealOptions::default()
                })
                .flex()
                .items_center()
                .justify_start()
                .gap(t.spacing.s1)
                .min_h(t.size.element_md)
                .w(control_width)
                .flex_none()
                .child(remove)
                .children(handle);
            let status_el = item_status
                .as_ref()
                .and_then(|s| s.message.clone().map(|m| (s.status, m)))
                .map(|(kind, message)| {
                    FieldStatus::new(part(&id, format!("item-{index}-status")), kind, message)
                        .variant(FieldStatusVariant::Detached)
                        .when(!narrow, |s| s.mt(t.spacing.s1))
                        .mr(control_width + gap)
                });
            let row = reveal
                .container(Default::default())
                .flex()
                .flex_row()
                .when(narrow, |d| d.items_start())
                .when(!narrow, |d| d.items_end())
                .gap(t.spacing.s1)
                .min_w_0()
                .child(fields)
                .child(controls);
            let entering = shared.borrow().entering.get(&item_key(&key)).copied();
            let mut record = div()
                .flex()
                .flex_col()
                .min_w_0()
                .relative()
                .debug_selector({
                    let s = format!("{id}-record-{key}");
                    move || s
                })
                .child(row)
                .children(status_el);
            if let Some(started) = entering {
                // Entry: up from the theme's spacing-2 on --ease-standard.
                let elapsed = cx
                    .background_executor()
                    .now()
                    .saturating_duration_since(started);
                let progress = (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0., 1.);
                let offset = f32::from(t.spacing.s2.to_pixels(rem))
                    * (1. - t.ease.standard.ease(progress as f32));
                record = record.translate_y(px(offset));
                window.request_animation_frame();
            }
            let record = Glide::new(item_key(&key), shared.clone(), duration, ease, record);
            records.push(match &attached {
                Some(a) => a.item(
                    index,
                    ItemKey::Text(key.clone()),
                    i18n.format(
                        "@astryx.listInput.itemName",
                        &[
                            ("itemName", Arg::from(item_name.as_ref())),
                            ("position", Arg::Num((index + 1) as f64)),
                        ],
                    ),
                    record,
                    cx,
                ),
                None => record.into_any_element(),
            });
        }

        let records_box = div().flex().flex_col().gap(list_gap).min_w_0();
        let list = div()
            .id(part(&id, "list"))
            .min_w_0()
            .role(gpui::Role::List)
            .aria_label(self.label.clone());
        let list = if value.is_empty() {
            let entering = shared.borrow().entering.get(EMPTY_KEY).copied();
            let mut empty = div().w_full();
            if let Some(started) = entering {
                let elapsed = now.saturating_duration_since(started);
                let progress = (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0., 1.);
                let offset = f32::from(t.spacing.s2.to_pixels(rem))
                    * (1. - t.ease.standard.ease(progress as f32));
                empty = empty.translate_y(px(offset));
                window.request_animation_frame();
            }
            list.child(Glide::new(
                EMPTY_KEY.into(),
                shared.clone(),
                duration,
                ease,
                empty.child(
                    EmptyState::new(i18n.format(
                        "@astryx.listInput.emptyTitle",
                        &[("itemName", Arg::from(item_name.as_ref()))],
                    ))
                    .description(i18n.format(
                        "@astryx.listInput.emptyDescription",
                        &[("itemName", Arg::from(item_name.as_ref()))],
                    ))
                    .compact(true),
                ),
            ))
            .into_any_element()
        } else {
            list.child(match &attached {
                Some(a) => a.container(records_box.children(records), count),
                None => records_box.children(records).into_any_element(),
            })
            .into_any_element()
        };

        let add = {
            let (value, on_change, get_key, create, shared) = (
                value.clone(),
                on_change.clone(),
                get_key.clone(),
                self.create_item.clone(),
                shared.clone(),
            );
            let item_name = item_name.clone();
            let selector = format!("{id}-add");
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(t.spacing.s1)
                .mt(t.spacing.s2)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .debug_selector(move || selector)
                        .child(
                            Button::new(
                                part(&id, "add"),
                                i18n.format(
                                    "@astryx.listInput.addItem",
                                    &[("itemName", Arg::from(item_name.as_ref()))],
                                ),
                            )
                            .variant(ButtonVariant::Secondary)
                            .size(ElementSize::Md)
                            .width(relative(1.))
                            .disabled(self.disabled || reached_max)
                            .loading(self.loading)
                            .on_click(move |event, window, cx| {
                                if mutations_disabled || reached_max {
                                    return;
                                }
                                let item = create();
                                let mut next = (*value).clone();
                                next.push(item.clone());
                                let key = get_key(&item);
                                {
                                    let mut s = shared.borrow_mut();
                                    s.pending_focus = Some(PendingFocus::FirstField(key));
                                    // A pointer's add keeps Add under the pointer: the new
                                    // record's room scrolls away with the same frame, as
                                    // tall as the last record and its gap, and what is left
                                    // is scrolled once the record lays out.
                                    let add_top = (!event.is_keyboard())
                                        .then(|| s.bounds.get(ADD_KEY).map(|b| b.origin.y))
                                        .flatten();
                                    if add_top.is_some()
                                        && let Some(last) =
                                            value.last().map(|v| item_key(&get_key(v)))
                                        && let Some(room) =
                                            s.bounds.get(&last).map(|b| b.size.height + list_gap)
                                        && let Some(scroller) = s.scroller.clone()
                                    {
                                        scroll_by(&scroller, room);
                                    }
                                    s.pending_motion = Some(PendingMotion {
                                        before: s.bounds.clone(),
                                        expected: next.iter().map(|v| get_key(v)).collect(),
                                        entering_any: true,
                                        add_top,
                                    });
                                }
                                let index = value.len();
                                if let Some(f) = &on_change {
                                    f(next, ListInputChange::Add { item, index }, window, cx);
                                }
                                let text = astryx_i18n::current(cx).format(
                                    "@astryx.listInput.announceAdded",
                                    &[
                                        ("itemName", Arg::from(item_name.as_ref())),
                                        ("position", Arg::Num((index + 1) as f64)),
                                    ],
                                );
                                announce(text, Politeness::Polite, window, cx);
                            }),
                        ),
                )
                .child(div().flex_none().w(control_width))
        };
        let add = Glide::new(ADD_KEY.into(), shared.clone(), duration, ease, add);

        let state_name = if self.loading {
            "loading"
        } else if self.disabled {
            "disabled"
        } else {
            ""
        };
        let mut group = div()
            .id(part(&id, "group"))
            .relative()
            .flex()
            .flex_col()
            .min_w_0()
            .w_full()
            .role(gpui::Role::Group)
            .aria_label(self.label.clone())
            .debug_selector(|| "list-input".into());
        theme.refine(
            group.style(),
            "list-input",
            &[
                ("state", state_name),
                ("reorderable", if handle_cell { "true" } else { "" }),
            ],
            None,
        );
        group.child(list).child(add).into_any_element()
    }
}

/// Scrolls a scroller's content by `delta` (down for a positive one), within its range.
fn scroll_by(scroller: &ScrollHandle, delta: Pixels) {
    let offset = scroller.offset();
    let max = scroller.max_offset().y;
    let top = (-offset.y + delta).max(px(0.));
    // A container that grows with the new record may scroll past its old range.
    let top = if max > px(0.) {
        top.min(max + delta.max(px(0.)))
    } else {
        top
    };
    scroller.set_offset(point(offset.x, -top));
}

/// A participant of the collection motion (upstream's `data-list-input-motion-key`
/// element): it records its box each frame, and after a mutation it glides from the box
/// it had before (upstream's FLIP animation). Add also keeps itself under the pointer
/// that added a record.
struct Glide {
    key: SharedString,
    shared: Rc<RefCell<ListState>>,
    duration: std::time::Duration,
    ease: astryx_theme::CubicBezier,
    child: AnyElement,
}

impl Glide {
    fn new(
        key: SharedString,
        shared: Rc<RefCell<ListState>>,
        duration: std::time::Duration,
        ease: astryx_theme::CubicBezier,
        child: impl IntoElement,
    ) -> Self {
        Glide {
            key,
            shared,
            duration,
            ease,
            child: child.into_any_element(),
        }
    }
}

impl Element for Glide {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let now = cx.background_executor().now();
        let offset = {
            let mut s = self.shared.borrow_mut();
            if let Some(armed) = s.armed.clone() {
                let before = armed.before.get(&self.key).copied();
                // A size change lands everything at once (upstream's `hasSizeChange`):
                // the participants after it in order see it.
                if let Some(before) = before
                    && ((before.size.width - bounds.size.width).abs() >= px(0.5)
                        || (before.size.height - bounds.size.height).abs() >= px(0.5))
                {
                    s.size_changed = true;
                }
                // Appended rows already carry the orientation cue; the others keep their
                // place while one enters.
                if !armed.entering_any
                    && !s.size_changed
                    && let Some(before) = before
                {
                    let delta = before.origin - bounds.origin;
                    if delta.x.abs() >= px(0.5) || delta.y.abs() >= px(0.5) {
                        s.moving.insert(self.key.clone(), (delta, now));
                    }
                }
                // What the scroll at the add left over: Add back where the pointer is.
                if self.key.as_ref() == ADD_KEY
                    && let Some(top) = armed.add_top
                    && let Some(scroller) = s.scroller.clone()
                {
                    let left = bounds.origin.y - top;
                    if left.abs() >= px(0.5) {
                        scroll_by(&scroller, left);
                        window.refresh();
                    }
                }
            }
            if self.key.as_ref() == ADD_KEY {
                s.scroller = astryx_primitives::keyboard::nearest_scroller().map(|(h, _)| h);
            }
            s.bounds.insert(self.key.clone(), bounds);
            s.moving.get(&self.key).map(|(delta, started)| {
                let progress = (now.saturating_duration_since(*started).as_secs_f64()
                    / self.duration.as_secs_f64())
                .clamp(0., 1.) as f32;
                *delta * (1. - self.ease.ease(progress))
            })
        };
        match offset {
            Some(offset) => {
                window.request_animation_frame();
                window.with_element_offset(offset, |window| self.child.prepaint(window, cx));
            }
            None => {
                self.child.prepaint(window, cx);
            }
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `ListInput`.
pub const LIST_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "ListInput",
    rust_type: "astryx::lab::list_input::ListInput",
    props: &[
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("getItemKey", RustApi::Argument("get_item_key")),
        p("createItem", RustApi::Argument("create_item")),
        p("columns", RustApi::Argument("columns (ListInputColumn)")),
        p("itemName", RustApi::Method("item_name")),
        p("description", RustApi::Method("description")),
        p("status", RustApi::Method("status")),
        p("getItemStatus", RustApi::Method("item_status")),
        p("getFieldStatus", RustApi::Method("field_status")),
        p("reorderable", RustApi::Method("reorderable")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isLoading", RustApi::Method("loading")),
        p("maxItems", RustApi::Method("max_items")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
    ],
    example: include_str!("examples.rs"),
};
