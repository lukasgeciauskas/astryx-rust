//! Upstream lab `TransferList/TransferList.tsx`: a controlled dual-panel collection input
//! for choosing and ordering values, with immediate transfers, vertical reordering
//! through Core's List and responsive scroll ownership.

use std::cell::RefCell;
use std::rc::Rc;

use rustryx_components::button::{Button, ButtonVariant, IconButton};
use rustryx_components::icon::{Icon, IconColor, IconSize};
use rustryx_components::input::{InputType, TextInput};
use rustryx_components::item::{Item, ItemDensity};
use rustryx_components::layout::VisuallyHidden;
use rustryx_components::list::{List, ListItem};
use rustryx_components::reorderable::{ItemKey, ReorderMove, Reorderable, ReorderableOptions};

use super::responsive::Responsive;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ElementId, FocusHandle, Hsla, PathBuilder, Pixels, SharedString,
    StyleRefinement, Window, canvas, div, point, px, relative, rems,
};
use rustryx_components::text::{Text, TextColor, TextDisplay, TextType, TextWeight};
use rustryx_i18n::Arg;
use rustryx_primitives::{ElementSize, Politeness, announce, request_focus};
use rustryx_theme::authoring::js::js_trim;

/// Upstream `--transfer-list-panel-min-block-size`: the lab component's own layout
/// variable (no theme token carries it).
const PANEL_MIN_BLOCK_SIZE_REM: f32 = 12.;
/// Upstream `--transfer-list-panel-max-block-size`.
const PANEL_MAX_BLOCK_SIZE_REM: f32 = 20.;
/// Upstream's `@container (max-width: 40rem)`: narrower, the panels stack.
const STACK_BELOW_REM: f32 = 40.;
/// Core List's reorder handle overlaps the row's end padding: a row without one keeps
/// its end content clear of it, so every Remove sits in one column (upstream
/// `clearsHandle`).
const CLEARS_HANDLE_PX: f32 = 20.;

/// One option (upstream `TransferListOption`).
#[derive(Clone, Debug, PartialEq)]
pub struct TransferListOption {
    /// Upstream `value`: stable value written to the controlled value array.
    pub value: SharedString,
    /// Upstream `label`: visible option name and the basis of action labels.
    pub label: SharedString,
    /// Upstream `description`: optional searchable metadata, not rendered in the
    /// default row.
    pub description: Option<SharedString>,
    /// Upstream `group`: optional group heading on the available side.
    pub group: Option<SharedString>,
    /// Upstream `isTransferDisabled`: keeps the option in its list.
    pub is_transfer_disabled: bool,
    /// Upstream `disabledMessage`: why a transfer is unavailable.
    pub disabled_message: Option<SharedString>,
}

impl TransferListOption {
    /// An option with a value and a label.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        TransferListOption {
            value: value.into(),
            label: label.into(),
            description: None,
            group: None,
            is_transfer_disabled: false,
            disabled_message: None,
        }
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `group`.
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// Upstream `isTransferDisabled`.
    pub fn transfer_disabled(mut self, disabled: bool) -> Self {
        self.is_transfer_disabled = disabled;
        self
    }

    /// Upstream `disabledMessage`.
    pub fn disabled_message(mut self, message: impl Into<SharedString>) -> Self {
        self.disabled_message = Some(message.into());
        self
    }
}

/// Which list a row is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferSide {
    Selected,
    Available,
}

impl TransferSide {
    fn as_str(self) -> &'static str {
        match self {
            TransferSide::Selected => "selected",
            TransferSide::Available => "available",
        }
    }
}

/// Upstream `matchesQuery`: whether an option matches a trimmed, lower-cased query (its
/// label, description or group contains it). Case folding is locale-independent.
pub fn matches_query(option: &TransferListOption, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    [
        option.label.as_ref(),
        option.description.as_deref().unwrap_or(""),
        option.group.as_deref().unwrap_or(""),
    ]
    .iter()
    .any(|part| part.to_lowercase().contains(query))
}

/// The query as upstream compares it: trimmed and lower-cased.
pub fn normalize_query(query: &str) -> String {
    js_trim(query).to_lowercase()
}

/// The value's options in value order, matching the query (upstream
/// `selectedOptions`); a value with no option is left out.
pub fn selected_options(
    options: &[TransferListOption],
    value: &[SharedString],
    query: &str,
) -> Vec<TransferListOption> {
    value
        .iter()
        .filter_map(|v| options.iter().rev().find(|o| &o.value == v))
        .filter(|o| matches_query(o, query))
        .cloned()
        .collect()
}

/// The options not in the value, matching the query, in option order (upstream
/// `availableOptions`).
pub fn available_options(
    options: &[TransferListOption],
    value: &[SharedString],
    query: &str,
) -> Vec<TransferListOption> {
    options
        .iter()
        .filter(|o| !value.contains(&o.value) && matches_query(o, query))
        .cloned()
        .collect()
}

/// The available options by group, groups in order of first appearance (upstream
/// `groupedAvailable`); the ungrouped are the group `""`.
pub fn grouped(available: &[TransferListOption]) -> Vec<(SharedString, Vec<TransferListOption>)> {
    let mut groups: Vec<(SharedString, Vec<TransferListOption>)> = Vec::new();
    for option in available {
        let group = option.group.clone().unwrap_or_default();
        match groups.iter_mut().find(|(g, _)| *g == group) {
            Some((_, items)) => items.push(option.clone()),
            None => groups.push((group, vec![option.clone()])),
        }
    }
    groups
}

/// Upstream `addAll`'s additions: every transfer-enabled option not yet in the value,
/// in option order.
pub fn add_all(options: &[TransferListOption], value: &[SharedString]) -> Vec<SharedString> {
    let mut next = value.to_vec();
    for option in options {
        if !option.is_transfer_disabled
            && !next.contains(&option.value)
            && !value.contains(&option.value)
        {
            next.push(option.value.clone());
        }
    }
    next
}

/// Upstream `clearSelected`'s result: the values without an option or with a
/// transfer-disabled one stay.
pub fn clear_selected(options: &[TransferListOption], value: &[SharedString]) -> Vec<SharedString> {
    value
        .iter()
        .filter(|v| option_of(options, v).is_none_or(|o| o.is_transfer_disabled))
        .cloned()
        .collect()
}

/// The option for a value: the last with it, as a `Map` keeps it.
fn option_of<'a>(options: &'a [TransferListOption], value: &str) -> Option<&'a TransferListOption> {
    options.iter().rev().find(|o| o.value.as_ref() == value)
}

/// Upstream `reorderSelected`: a List move among the shown rows (`key` to the shown
/// index `to`) in the value's terms. The shown rows' new order goes back into the
/// places the shown values held, so a value without an option keeps its place; the
/// result is where the moved value lands in the value. `None` when nothing moves.
pub fn reorder_in_value(
    shown: &[SharedString],
    value: &[SharedString],
    key: &str,
    to: usize,
) -> Option<usize> {
    let from = shown.iter().position(|v| v.as_ref() == key)?;
    if from == to || to >= shown.len() {
        return None;
    }
    let mut order = shown.to_vec();
    let moved = order.remove(from);
    order.insert(to, moved);
    let mut next = order.into_iter();
    let next_value: Vec<SharedString> = value
        .iter()
        .map(|item| {
            if shown.contains(item) {
                next.next().unwrap_or_else(|| item.clone())
            } else {
                item.clone()
            }
        })
        .collect();
    next_value.iter().position(|v| v.as_ref() == key)
}

type ChangeFn = Rc<dyn Fn(Vec<SharedString>, &mut Window, &mut App)>;
type HeaderActionFn = Rc<dyn Fn(&mut Window, &mut App)>;
type RenderOptionFn = Rc<dyn Fn(&TransferListOption, &mut Window, &mut App) -> AnyElement>;

/// The list's own state across frames: the search and where focus goes after a
/// transfer.
struct TransferState {
    query: SharedString,
    /// The value the handlers read (upstream `currentValueRef`).
    current: Vec<SharedString>,
    focus_after: Option<(TransferSide, usize)>,
    narrow: bool,
}

/// A controlled dual-panel collection input for choosing and ordering values (upstream
/// lab `TransferList`). Selection, ordering, drafts, persistence and saved views stay
/// the app's; the list owns only its search and gesture state.
#[derive(IntoElement)]
pub struct TransferList {
    id: ElementId,
    label: SharedString,
    label_hidden: bool,
    description: Option<SharedString>,
    options: Rc<[TransferListOption]>,
    disabled: bool,
    value: Vec<SharedString>,
    on_change: Option<ChangeFn>,
    selected_label: Option<SharedString>,
    available_label: Option<SharedString>,
    search: bool,
    search_label: Option<SharedString>,
    search_placeholder: Option<SharedString>,
    reorderable: Option<Reorderable>,
    select_all: bool,
    clear: bool,
    render_option: Option<RenderOptionFn>,
    selected_empty_text: Option<SharedString>,
    available_empty_text: Option<SharedString>,
    no_results_text: Option<SharedString>,
    style: StyleRefinement,
}

impl TransferList {
    /// Upstream `label` (the control's accessible name), `options` (the complete
    /// catalog; values are unique) and `value` (the ordered selection, the single
    /// source of truth).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        options: impl Into<Rc<[TransferListOption]>>,
        value: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        TransferList {
            id: id.into(),
            label: label.into(),
            label_hidden: false,
            description: None,
            options: options.into(),
            disabled: false,
            value: value.into_iter().map(Into::into).collect(),
            on_change: None,
            selected_label: None,
            available_label: None,
            search: false,
            search_label: None,
            search_placeholder: None,
            reorderable: None,
            select_all: false,
            clear: false,
            render_option: None,
            selected_empty_text: None,
            available_empty_text: None,
            no_results_text: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onChange`: the next ordered selection after a committed change.
    pub fn on_change(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `isLabelHidden`: hides the label, keeping its accessible name.
    pub fn label_hidden(mut self, hidden: bool) -> Self {
        self.label_hidden = hidden;
        self
    }

    /// Upstream `description`: guidance below the label.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `isDisabled`: disables every transfer, reorder, Clear, Add all and
    /// search control; the row controls and the search field stay focusable.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `selectedLabel` ("Selected" by default).
    pub fn selected_label(mut self, label: impl Into<SharedString>) -> Self {
        self.selected_label = Some(label.into());
        self
    }

    /// Upstream `availableLabel` ("Available" by default).
    pub fn available_label(mut self, label: impl Into<SharedString>) -> Self {
        self.available_label = Some(label.into());
        self
    }

    /// Upstream `hasSearch`: one search field that filters both panels.
    pub fn search(mut self, search: bool) -> Self {
        self.search = search;
        self
    }

    /// Upstream `searchLabel`: the search's accessible label (`Search {label}` by
    /// default).
    pub fn search_label(mut self, label: impl Into<SharedString>) -> Self {
        self.search_label = Some(label.into());
        self
    }

    /// Upstream `searchPlaceholder` ("Search…" by default).
    pub fn search_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.search_placeholder = Some(placeholder.into());
        self
    }

    /// Upstream `reorderable`: makes the selected options reorderable through Core
    /// List's reorder handle; pass the object `create_reorderable` makes. Its
    /// `is_reorderable(value)` decides which options move; each move goes to its
    /// `on_reorder`, `to` being the value's index in `value` after the move. Paused
    /// while a search filters the panel.
    pub fn reorderable(mut self, reorderable: Reorderable) -> Self {
        self.reorderable = Some(reorderable);
        self
    }

    /// Upstream `hasSelectAll`: an Add all action.
    pub fn select_all(mut self, select_all: bool) -> Self {
        self.select_all = select_all;
        self
    }

    /// Upstream `hasClear`: a Clear action; transfer-disabled values stay.
    pub fn clear(mut self, clear: bool) -> Self {
        self.clear = clear;
        self
    }

    /// Upstream `renderOption`: the row's primary content, keeping the built-in
    /// actions.
    pub fn render_option(
        mut self,
        f: impl Fn(&TransferListOption, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_option = Some(Rc::new(f));
        self
    }

    /// Upstream `selectedEmptyText`.
    pub fn selected_empty_text(mut self, text: impl Into<SharedString>) -> Self {
        self.selected_empty_text = Some(text.into());
        self
    }

    /// Upstream `availableEmptyText`.
    pub fn available_empty_text(mut self, text: impl Into<SharedString>) -> Self {
        self.available_empty_text = Some(text.into());
        self
    }

    /// Upstream `noResultsText`: the empty copy while a search is active.
    pub fn no_results_text(mut self, text: impl Into<SharedString>) -> Self {
        self.no_results_text = Some(text.into());
        self
    }
}

impl Styled for TransferList {
    /// Upstream `xstyle`, `className` and `style` on the root.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn name(id: &ElementId, part: impl std::fmt::Display) -> ElementId {
    ElementId::Name(format!("{id:?}-{part}").into())
}

/// The plus glyph upstream draws inline (the icon registry has no `add`): Lucide's
/// `M5 12h14` and `M12 5v14` in a 24-unit box, 1.5 wide, round caps, in the text color.
fn plus_icon() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        |bounds: Bounds<Pixels>, _, window, _| {
            let color: Hsla = window.text_style().color;
            let unit = f32::from(bounds.size.width) / 24.;
            let at = |x: f32, y: f32| bounds.origin + point(px(x * unit), px(y * unit));
            for (from, to) in [((5., 12.), (19., 12.)), ((12., 5.), (12., 19.))] {
                let mut b = PathBuilder::stroke(px(1.5 * unit));
                b.move_to(at(from.0, from.1));
                b.line_to(at(to.0, to.1));
                if let Ok(path) = b.build() {
                    window.paint_path(path, color);
                }
            }
        },
    )
    .size(IconSize::Sm.rems())
    .flex_none()
}

struct Built {
    shared: Rc<RefCell<TransferState>>,
    options: Rc<[TransferListOption]>,
    on_change: Option<ChangeFn>,
    id: ElementId,
}

impl Built {
    fn commit(&self, next: Vec<SharedString>, window: &mut Window, cx: &mut App) {
        self.shared.borrow_mut().current = next.clone();
        if let Some(f) = &self.on_change {
            f(next, window, cx);
        }
    }

    fn add(&self, option: &TransferListOption, index: usize, window: &mut Window, cx: &mut App) {
        let current = self.shared.borrow().current.clone();
        if option.is_transfer_disabled || current.contains(&option.value) {
            return;
        }
        let mut next = current;
        next.push(option.value.clone());
        let count = next.len();
        self.commit(next, window, cx);
        let text = rustryx_i18n::current(cx).format(
            "@astryx.transferList.announceAdded",
            &[
                ("label", Arg::from(option.label.as_ref())),
                ("count", Arg::Num(count as f64)),
            ],
        );
        announce(text, Politeness::Polite, window, cx);
        self.shared.borrow_mut().focus_after = Some((TransferSide::Available, index));
        window.refresh();
    }

    fn remove(&self, option: &TransferListOption, index: usize, window: &mut Window, cx: &mut App) {
        if option.is_transfer_disabled {
            return;
        }
        let next: Vec<SharedString> = self
            .shared
            .borrow()
            .current
            .iter()
            .filter(|v| **v != option.value)
            .cloned()
            .collect();
        let count = next.len();
        self.commit(next, window, cx);
        let text = rustryx_i18n::current(cx).format(
            "@astryx.transferList.announceRemoved",
            &[
                ("label", Arg::from(option.label.as_ref())),
                ("count", Arg::Num(count as f64)),
            ],
        );
        announce(text, Politeness::Polite, window, cx);
        self.shared.borrow_mut().focus_after = Some((TransferSide::Selected, index));
        window.refresh();
    }

    fn add_all(&self, window: &mut Window, cx: &mut App) {
        let current = self.shared.borrow().current.clone();
        let next = add_all(&self.options, &current);
        let added = next.len() - current.len();
        if added > 0 {
            self.commit(next, window, cx);
            let text = rustryx_i18n::current(cx).format(
                "@astryx.transferList.announceBulkAdded",
                &[("count", Arg::Num(added as f64))],
            );
            announce(text, Politeness::Polite, window, cx);
        }
    }

    fn clear(&self, window: &mut Window, cx: &mut App) {
        let current = self.shared.borrow().current.clone();
        let next = clear_selected(&self.options, &current);
        let removed = current.len() - next.len();
        if removed > 0 {
            self.commit(next, window, cx);
            let text = rustryx_i18n::current(cx).format(
                "@astryx.transferList.announceBulkRemoved",
                &[("count", Arg::Num(removed as f64))],
            );
            announce(text, Politeness::Polite, window, cx);
        }
    }

    fn search(&self, query: SharedString, window: &mut Window, cx: &mut App) {
        let normalized = normalize_query(&query);
        self.shared.borrow_mut().query = query;
        if !normalized.is_empty() {
            let count = self
                .options
                .iter()
                .filter(|o| matches_query(o, &normalized))
                .count();
            let text = rustryx_i18n::current(cx).format(
                "@astryx.transferList.announceSearchResults",
                &[("count", Arg::Num(count as f64))],
            );
            announce(text, Politeness::Polite, window, cx);
        }
        window.refresh();
    }

    fn action_id(&self, side: TransferSide, value: &str) -> ElementId {
        name(&self.id, format!("{}-{value}", side.as_str()))
    }
}

impl RenderOnce for TransferList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id.clone();
        let state = window.use_keyed_state(name(&id, "transfer-list"), cx, |_, _| {
            Rc::new(RefCell::new(TransferState {
                query: SharedString::default(),
                current: Vec::new(),
                focus_after: None,
                narrow: false,
            }))
        });
        let last_narrow = state.read(cx).borrow().narrow;
        let shared = state.read(cx).clone();
        let threshold = window.rem_size() * STACK_BELOW_REM;
        let this = Rc::new(self);
        let first = Rc::new(std::cell::Cell::new(true));
        // Narrower than 40rem, the panels stack (upstream's container query): the list
        // lays out as it did last frame, and on a frame whose width crosses the line it
        // lays its content out again for the new width before it paints.
        Responsive::new(threshold, last_narrow, move |narrow, window, cx| {
            shared.borrow_mut().narrow = narrow;
            this.build(narrow, first.replace(false), window, cx)
        })
    }
}

impl TransferList {
    /// The list's content at `narrow`; `first` is the frame's first build, which moves
    /// focus after a transfer.
    fn build(&self, narrow: bool, first: bool, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let i18n = rustryx_i18n::current(cx);
        let id = self.id.clone();
        let state = window.use_keyed_state(name(&id, "transfer-list"), cx, |_, _| {
            Rc::new(RefCell::new(TransferState {
                query: SharedString::default(),
                current: Vec::new(),
                focus_after: None,
                narrow: false,
            }))
        });
        let shared = state.read(cx).clone();
        // The handlers read the committed value.
        shared.borrow_mut().current = self.value.clone();
        let selected_label = self
            .selected_label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.transferList.selectedLabel"));
        let available_label = self
            .available_label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.transferList.availableLabel"));
        let search_placeholder = self
            .search_placeholder
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.transferList.searchPlaceholder"));
        let selected_empty_text = self
            .selected_empty_text
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.transferList.selectedEmpty"));
        let available_empty_text = self
            .available_empty_text
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.transferList.availableEmpty"));
        let no_results_text = self
            .no_results_text
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.transferList.noResults"));
        let disabled = self.disabled;
        // An option's own message names its own constraint; it does not apply to a
        // disabled list.
        let options: Rc<[TransferListOption]> = if disabled {
            self.options
                .iter()
                .map(|o| TransferListOption {
                    is_transfer_disabled: true,
                    disabled_message: None,
                    ..o.clone()
                })
                .collect::<Vec<_>>()
                .into()
        } else {
            self.options.clone()
        };
        let query = shared.borrow().query.clone();
        let normalized = normalize_query(&query);
        let value = self.value.clone();
        let selected = selected_options(&options, &value, &normalized);
        let available = available_options(&options, &value, &normalized);
        let groups = grouped(&available);
        let built = Rc::new(Built {
            shared: shared.clone(),
            options: options.clone(),
            on_change: self.on_change.clone(),
            id: id.clone(),
        });

        // Reordering is off while a search filters the panel: a hidden row could
        // otherwise move past the shown rows.
        let ordering = self.reorderable.is_some() && normalized.is_empty();
        let owner = self.reorderable.clone();
        let can_move = {
            let owner = owner.clone();
            move |v: &SharedString| {
                owner
                    .as_ref()
                    .is_none_or(|o| o.can_move(&ItemKey::Text(v.clone())))
            }
        };
        // The owner's object, adapted so the List's moves among the shown rows reach the
        // owner in the value's terms. Its collections are the owner's, so the owner's
        // `start(key)` finds this list.
        let shown: Vec<SharedString> = selected.iter().map(|o| o.value.clone()).collect();
        let list_reorderable = owner.as_ref().filter(|_| ordering).map(|owner| {
            let (target, keep, value) = (owner.clone(), owner.clone(), value.clone());
            owner.adapt(
                ReorderableOptions::new(move |m, window, cx| {
                    let ItemKey::Text(key) = &m.key else {
                        return;
                    };
                    if let Some(to) = reorder_in_value(&shown, &value, key, m.to) {
                        target.on_reorder(
                            &ReorderMove {
                                key: m.key.clone(),
                                to,
                            },
                            window,
                            cx,
                        );
                    }
                })
                .is_reorderable(move |key| keep.can_move(key)),
            )
        });

        // Focus after a transfer: the action at the same place on the same side, else
        // the search, else the panel.
        let selected_panel_focus = panel_focus(&id, TransferSide::Selected, window, cx);
        let available_panel_focus = panel_focus(&id, TransferSide::Available, window, cx);
        let search_id = name(&id, "search");
        let pending_focus = if first {
            shared.borrow_mut().focus_after.take()
        } else {
            None
        };
        if let Some((side, index)) = pending_focus {
            let actions: Vec<ElementId> = match side {
                TransferSide::Selected => selected
                    .iter()
                    .filter(|o| !o.is_transfer_disabled)
                    .map(|o| built.action_id(side, &o.value))
                    .collect(),
                TransferSide::Available => groups
                    .iter()
                    .flat_map(|(_, items)| items.iter())
                    .filter(|o| !o.is_transfer_disabled)
                    .map(|o| built.action_id(side, &o.value))
                    .collect(),
            };
            if let Some(action) = actions.get(index.min(actions.len().saturating_sub(1))) {
                request_focus(action, cx);
            } else if self.search {
                request_focus(&search_id, cx);
            } else {
                let panel = match side {
                    TransferSide::Selected => &selected_panel_focus,
                    TransferSide::Available => &available_panel_focus,
                };
                window.focus(panel, cx);
            }
        }

        let disabled_reason = |o: &TransferListOption| {
            o.disabled_message.clone().unwrap_or_else(|| {
                i18n.format(
                    "@astryx.transferList.transferDisabled",
                    &[("label", Arg::from(o.label.as_ref()))],
                )
            })
        };
        let row_label = |o: &TransferListOption, window: &mut Window, cx: &mut App| -> AnyElement {
            match &self.render_option {
                Some(f) => f(o, window, cx),
                None => o.label.clone().into_any_element(),
            }
        };

        // The selected rows.
        let mut selected_rows = Vec::new();
        for (index, option) in selected.iter().enumerate() {
            let built = built.clone();
            let o = option.clone();
            let mut remove = IconButton::new(
                built.action_id(TransferSide::Selected, &option.value),
                i18n.format(
                    "@astryx.transferList.removeOption",
                    &[("label", Arg::from(option.label.as_ref()))],
                ),
                Icon::new("close").size(IconSize::Sm),
            )
            .size(ElementSize::Sm)
            .variant(ButtonVariant::Ghost)
            .disabled(option.is_transfer_disabled)
            .on_click(move |_, window, cx| built.remove(&o, index, window, cx));
            if option.is_transfer_disabled {
                remove = remove.tooltip(disabled_reason(option));
            }
            // A row that keeps its place has no handle: its Remove keeps the movable
            // rows' column.
            let fixed = ordering && !can_move(&option.value);
            let selector = format!("{id}-selected-{}", option.value);
            let end = div()
                .flex()
                .items_center()
                .when(fixed, |d| d.mr(px(CLEARS_HANDLE_PX)))
                .debug_selector(move || selector)
                .child(remove);
            let state = if option.is_transfer_disabled {
                "disabled"
            } else {
                "enabled"
            };
            let reorder = if ordering {
                if fixed { "fixed" } else { "movable" }
            } else {
                ""
            };
            let mut row = ListItem::new_element(
                name(&id, format!("row-{}", option.value)),
                row_label(option, window, cx),
            )
            .item_key(ItemKey::Text(option.value.clone()))
            .reorder_label(option.label.clone())
            .end_content(end)
            .disabled(disabled)
            .px(t.spacing.s3);
            // A disabled list's handle says why the row does not move.
            if disabled && ordering && !fixed {
                row = row.reorder_tooltip(i18n.format(
                    "@astryx.transferList.reorderDisabled",
                    &[("label", Arg::from(option.label.as_ref()))],
                ));
            }
            theme.refine(
                row.style(),
                "transfer-list-item",
                &[("side", "selected"), ("state", state), ("reorder", reorder)],
                None,
            );
            selected_rows.push(row.into_any_element());
        }

        // The available rows, by group.
        let mut available_rows = Vec::new();
        let headings = groups.len() > 1 || groups.iter().any(|(g, _)| !g.is_empty());
        for (gi, (group, items)) in groups.iter().enumerate() {
            if headings {
                let text = if group.is_empty() {
                    i18n.t("@astryx.transferList.ungroupedLabel")
                } else {
                    group.clone()
                };
                available_rows.push(
                    div()
                        .py(t.spacing.s1)
                        .px(t.spacing.s3)
                        .when(gi > 0, |d| d.mt(t.spacing.s1))
                        .child(
                            Text::new(text)
                                .weight(TextWeight::Bold)
                                .color(TextColor::Primary),
                        )
                        .into_any_element(),
                );
            }
            for option in items {
                let index = available
                    .iter()
                    .position(|o| o.value == option.value)
                    .unwrap_or(0);
                let built = built.clone();
                let o = option.clone();
                let mut add = IconButton::new(
                    built.action_id(TransferSide::Available, &option.value),
                    i18n.format(
                        "@astryx.transferList.addOption",
                        &[("label", Arg::from(option.label.as_ref()))],
                    ),
                    plus_icon(),
                )
                .size(ElementSize::Sm)
                .variant(ButtonVariant::Ghost)
                .disabled(option.is_transfer_disabled)
                .on_click(move |_, window, cx| built.add(&o, index, window, cx));
                if option.is_transfer_disabled {
                    add = add.tooltip(disabled_reason(option));
                }
                let state = if option.is_transfer_disabled {
                    "disabled"
                } else {
                    "enabled"
                };
                let selector = format!("{id}-available-{}", option.value);
                let add = div()
                    .flex()
                    .items_center()
                    .debug_selector(move || selector)
                    .child(add);
                let mut row = Item::new(
                    name(&id, format!("option-{}", option.value)),
                    row_label(option, window, cx),
                )
                .density(ItemDensity::Compact)
                .end_content(add)
                .min_w_0()
                .px(t.spacing.s3);
                theme.refine(
                    row.style(),
                    "transfer-list-item",
                    &[("side", "available"), ("state", state)],
                    None,
                );
                available_rows.push(row.into_any_element());
            }
        }

        let empty = |side: TransferSide, text: SharedString| {
            div()
                .flex()
                .items_center()
                .justify_center()
                .when(!narrow, |d| d.min_h(rems(PANEL_MIN_BLOCK_SIZE_REM)))
                .p(t.spacing.s4)
                .text_center()
                .debug_selector(move || format!("transfer-list-empty-{}", side.as_str()))
                .child(
                    Text::new(text)
                        .text_type(TextType::Supporting)
                        .color(TextColor::Secondary),
                )
                .into_any_element()
        };
        let body = |side: TransferSide, content: AnyElement| {
            let mut el = div().id(name(&id, format!("{}-body", side.as_str()))).p_0();
            if !narrow {
                el = el
                    .min_h(rems(PANEL_MIN_BLOCK_SIZE_REM))
                    .max_h(rems(PANEL_MAX_BLOCK_SIZE_REM))
                    .overflow_y_scroll()
                    .overscroll_y_contain();
            }
            el.child(content)
        };
        let header_action =
            |id: ElementId, label: SharedString, enabled: bool, f: HeaderActionFn| {
                let selector = id.to_string();
                let mut button = Button::new(id, label)
                    .size(ElementSize::Sm)
                    .variant(ButtonVariant::Ghost)
                    .disabled(!enabled)
                    .on_click(move |_, window, cx| f(window, cx));
                // A text-link treatment, with a hit target that still clears 24 px.
                button = button
                    .h_auto()
                    .min_h(t.size.element_sm)
                    .py_0()
                    .px_0()
                    .rounded(px(0.))
                    .text_color(t.color.text_accent);
                div().debug_selector(move || selector).child(button)
            };
        let panel_header =
            |label_text: SharedString, heading: ElementId, action: Option<gpui::Div>| {
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(t.spacing.s2)
                    .when(narrow, |d| d.bg(t.color.background_body))
                    .py(t.spacing.s2)
                    .px(t.spacing.s3)
                    .border_b(t.border.width)
                    .border_color(t.color.border)
                    .child(
                        Text::new(label_text)
                            .id(heading)
                            .text_type(TextType::Label)
                            .color(TextColor::Secondary),
                    )
                    .children(action)
            };

        let clear = self.clear.then(|| {
            let enabled = value
                .iter()
                .any(|v| option_of(&options, v).is_some_and(|o| !o.is_transfer_disabled));
            let built = built.clone();
            header_action(
                ElementId::Name(format!("{id}-clear").into()),
                i18n.t("@astryx.transferList.clear"),
                enabled,
                Rc::new(move |w, cx| built.clear(w, cx)),
            )
        });
        let add_all_action = self.select_all.then(|| {
            let enabled = options
                .iter()
                .any(|o| !o.is_transfer_disabled && !value.contains(&o.value));
            let built = built.clone();
            header_action(
                ElementId::Name(format!("{id}-add-all").into()),
                i18n.t("@astryx.transferList.addAll"),
                enabled,
                Rc::new(move |w, cx| built.add_all(w, cx)),
            )
        });

        let selected_content = if selected.is_empty() {
            empty(
                TransferSide::Selected,
                if normalized.is_empty() {
                    selected_empty_text
                } else {
                    no_results_text.clone()
                },
            )
        } else {
            let mut list = List::new(name(&id, "selected-list"))
                .density(ItemDensity::Compact)
                .label(selected_label.clone())
                .header(VisuallyHidden::new().child(selected_label.clone()))
                .children(selected_rows);
            if let Some(r) = list_reorderable {
                list = list.reorderable(r);
            }
            list.into_any_element()
        };
        let available_content = if available.is_empty() {
            empty(
                TransferSide::Available,
                if normalized.is_empty() {
                    available_empty_text
                } else {
                    no_results_text
                },
            )
        } else {
            List::new(name(&id, "available-list"))
                .density(ItemDensity::Compact)
                .label(available_label.clone())
                .header(VisuallyHidden::new().child(available_label.clone()))
                .children(available_rows)
                .into_any_element()
        };

        let panel =
            |side: TransferSide, focus: &FocusHandle, header: gpui::Div, content: AnyElement| {
                let mut el = div()
                    .id(name(&id, format!("{}-panel", side.as_str())))
                    .track_focus(focus)
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .when(!narrow, |d| {
                        d.overflow_hidden()
                            .flex_1()
                            .flex_basis(rems(STACK_BELOW_REM / 2.))
                    })
                    .when(narrow, |d| d.w_full())
                    .role(gpui::Role::Group)
                    .aria_label(match side {
                        TransferSide::Selected => selected_label.clone(),
                        TransferSide::Available => available_label.clone(),
                    })
                    .debug_selector(move || format!("transfer-list-panel-{}", side.as_str()));
                if side == TransferSide::Available {
                    el = if narrow {
                        el.border_t(t.border.width).border_color(t.color.border)
                    } else {
                        el.border_l(t.border.width).border_color(t.color.border)
                    };
                }
                theme.refine(
                    el.style(),
                    "transfer-list-panel",
                    &[("side", side.as_str())],
                    None,
                );
                el.child(header).child(body(side, content))
            };

        let selected_panel = panel(
            TransferSide::Selected,
            &selected_panel_focus,
            panel_header(selected_label.clone(), name(&id, "selected-heading"), clear),
            selected_content,
        );
        let available_panel = panel(
            TransferSide::Available,
            &available_panel_focus,
            panel_header(
                available_label.clone(),
                name(&id, "available-heading"),
                add_all_action,
            ),
            available_content,
        );
        let mut collection = div().flex().when(narrow, |d| d.flex_col()).min_w_0();
        theme.refine(collection.style(), "transfer-list-collection", &[], None);
        let collection = collection.child(selected_panel).child(available_panel);

        let search = self.search.then(|| {
            let built = built.clone();
            let label = self.search_label.clone().unwrap_or_else(|| {
                i18n.format(
                    "@astryx.transferList.searchLabel",
                    &[("label", Arg::from(self.label.as_ref()))],
                )
            });
            div()
                .flex()
                .items_end()
                .gap(t.spacing.s2)
                .min_w_0()
                .p(t.spacing.s3)
                .child(
                    TextInput::new(search_id.clone(), label, query.clone())
                        .input_type(InputType::Search)
                        .label_hidden(true)
                        .placeholder(search_placeholder.clone())
                        .start_icon(
                            Icon::new("search")
                                .size(IconSize::Sm)
                                .color(IconColor::Secondary),
                        )
                        .clear(true)
                        .width(relative(1.))
                        .disabled(disabled)
                        .on_change(move |q, window, cx| built.search(q, window, cx)),
                )
        });

        let heading = div()
            .flex()
            .flex_col()
            .gap(t.spacing.s1)
            .px(t.spacing.s3)
            .child(if self.label_hidden {
                VisuallyHidden::new()
                    .child(self.label.clone())
                    .into_any_element()
            } else {
                Text::new(self.label.clone())
                    .text_type(TextType::Label)
                    .display(TextDisplay::Block)
                    .into_any_element()
            })
            .children(self.description.clone().map(|d| {
                Text::new(d)
                    .text_type(TextType::Supporting)
                    .display(TextDisplay::Block)
            }));

        let mut root = div()
            .id(id.clone())
            .relative()
            .flex()
            .flex_col()
            .min_w_0()
            .role(gpui::Role::Group)
            .aria_label(self.label.clone())
            .debug_selector(|| "transfer-list".into());
        theme.refine(root.style(), "transfer-list", &[], None);
        root.style().refine(&self.style);
        root.child(heading)
            .children(search)
            .child(collection)
            .into_any_element()
    }
}

fn panel_focus(
    id: &ElementId,
    side: TransferSide,
    window: &mut Window,
    cx: &mut App,
) -> FocusHandle {
    window
        .use_keyed_state(
            name(id, format!("{}-panel-focus", side.as_str())),
            cx,
            |_, cx| cx.focus_handle(),
        )
        .read(cx)
        .clone()
}
