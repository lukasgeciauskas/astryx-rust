//! The MultiSelector component, upstream `MultiSelector/MultiSelector.tsx`
//! and its `useMultiCombobox` hook.

use std::rc::Rc;

use astryx_i18n::{Arg, current as i18n, use_direction};
use astryx_primitives::editing::TextEditState;
use astryx_primitives::typeahead::is_shortcut_chord;
use astryx_primitives::{
    ElementSize, EscapeBehavior, FocusNext, FocusPrevious, LayerAlign, LayerOptions, OutOfTabOrder,
    Placement, Politeness, Typeahead, anchored_layer, announce, control_focus, is_focus_visible,
    prefers_reduced_motion, provide, resolve_size,
};
use astryx_theme::{ThemeStyled, current, scope};
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Div, ElementId, Entity, FocusHandle, Focusable, HasPopup,
    IntoElement, KeyDownEvent, Length, MouseButton, ParentElement, Pixels, RenderOnce, Role,
    ScrollHandle, SharedString, Stateful, StyleRefinement, Styled, Subscription, Task, Window, div,
    prelude::*, px, relative,
};
use web_time::Instant;

use crate::button::{ButtonVariant, IconButton, element_height};
use crate::choice::action::{ActionFn, ChangeFn, Optimistic};
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconColor, IconDefaultSize, IconSize};
use crate::indicator::{CheckboxIndicator, ChoiceSize, IndicatorState};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::input::group::{group_context, group_disabled, group_place, inside_member, join};
use crate::motion::transition;
use crate::overlay::PopoverPresentation;
use crate::press::ActionCell;
use crate::selector::options::matches_query;
use crate::selector::search_row::{PanelSearch, panel_search};
use crate::selector::{
    SelectorEntry, SelectorIndicatorPosition, SelectorOptionData, SelectorVariant,
    selectable_options, trigger_line,
};
use crate::status::{Badge, Spinner, SpinnerSize};

type ValuesFn = Rc<dyn Fn(Vec<SharedString>, Option<MultiSelectorChange>, &mut Window, &mut App)>;
type RenderFn = Rc<dyn Fn(&SelectorOptionData, &mut Window, &mut App) -> AnyElement>;
type FormatFn = Rc<dyn Fn(&[(SharedString, SharedString)]) -> SharedString>;
type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type FocusFn = Rc<dyn Fn(&mut Window, &mut App)>;
type KeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;
type HandledKeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App) -> bool>;
type TriggerFn = Rc<dyn Fn(MultiTrigger, &mut Window, &mut App) -> AnyElement>;
type ActFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `triggerDisplay`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TriggerDisplay {
    /// "3 selected".
    #[default]
    Count,
    /// The first three labels as a list, then "+N".
    Labels,
    /// Badges up to `max_badges`, then "+N".
    Badges,
}

/// A change that is more than a toggle, upstream `MultiSelectorChange`: a
/// value created from the query (FR9). Every other change passes none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultiSelectorChange {
    Create { query: SharedString },
}

/// What a caller-rendered trigger takes (upstream
/// `MultiSelectorRenderTriggerProps`): pass the control through
/// [`MultiTrigger::attach`].
#[derive(Clone)]
pub struct MultiTrigger {
    /// The control's focus (upstream `ref`): focus returns here on close.
    pub focus: FocusHandle,
    /// Upstream `aria-expanded`.
    pub expanded: bool,
    /// Upstream `aria-busy`.
    pub busy: bool,
    popup: HasPopup,
    toggle: ActFn,
    key: HandledKeyFn,
}

impl MultiTrigger {
    /// Gives `el` the trigger's focus, press, keys and popup states
    /// (upstream: spread the props onto it).
    pub fn attach(self, el: Stateful<Div>) -> Stateful<Div> {
        let (toggle, key) = (self.toggle.clone(), self.key.clone());
        el.track_focus(&self.focus)
            .aria_has_popup(self.popup)
            .aria_expanded(self.expanded)
            .aria_busy(self.busy)
            .on_click(move |event, window, cx| {
                // Enter and Space act on key-down.
                if !event.is_keyboard() {
                    toggle(window, cx)
                }
            })
            .on_key_down(move |event, window, cx| {
                if key(event, window, cx) {
                    cx.stop_propagation();
                }
            })
    }
}

/// Opens, closes and toggles a MultiSelector from code, upstream
/// `MultiSelectorHandle`. Make it with the selector's id in the render that
/// draws the selector, and pass it to [`MultiSelector::controller`].
#[derive(Clone)]
pub struct MultiSelectorController {
    state: Entity<MultiState>,
}

impl MultiSelectorController {
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id: ElementId = id.into();
        MultiSelectorController {
            state: multi_state(&id, window, cx),
        }
    }

    /// Upstream `open()`.
    pub fn open(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| s.request = Some(true));
        window.refresh();
    }

    /// Upstream `close()`.
    pub fn close(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| s.request = Some(false));
        window.refresh();
    }

    /// Upstream `toggle()`.
    pub fn toggle(&self, window: &mut Window, cx: &mut App) {
        let open = self.state.read(cx).open;
        self.state.update(cx, |s, _| s.request = Some(!open));
        window.refresh();
    }

    /// Upstream `isOpen()`.
    pub fn is_open(&self, cx: &App) -> bool {
        self.state.read(cx).open
    }
}

/// One row of the list, in list order.
#[derive(Clone)]
enum Row {
    /// Upstream's create row: `Create "<query>"` (FR9).
    Create(SharedString),
    SelectAll,
    Option(SelectorOptionData),
}

impl Row {
    fn disabled(&self) -> bool {
        matches!(self, Row::Option(o) if o.disabled)
    }

    fn key(&self) -> String {
        match self {
            Row::Create(_) => "create".into(),
            Row::SelectAll => "select-all".into(),
            Row::Option(o) => o.value.to_string(),
        }
    }

    fn has_action(&self) -> bool {
        matches!(self, Row::Option(o) if o.action.is_some())
    }
}

/// How the rows group in the panel.
enum Block {
    Row(usize),
    Divider,
    Section {
        title: Option<SharedString>,
        rows: Vec<usize>,
    },
}

/// Which cell of a grid row the highlight is on (FR10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cell {
    Option,
    Action,
}

pub(crate) struct MultiState {
    open: bool,
    sheet: bool,
    highlight: Option<usize>,
    cell: Cell,
    keyboard: bool,
    query: String,
    /// The selection the list opened with: its options lead (FR18).
    at_open: Vec<SharedString>,
    typeahead: Typeahead,
    pending_focus: bool,
    announced_empty: Option<String>,
    default_opened: bool,
    listbox_focus: FocusHandle,
    /// A controller's open (true) or close (false).
    request: Option<bool>,
    on_open_change: Option<OpenFn>,
    /// Once an option carries an action the popup is a grid while it lives.
    grid: bool,
    reveal: bool,
    scroll: ScrollHandle,
    rows: Vec<Option<Bounds<Pixels>>>,
    cells: Vec<Option<(ActionCell, Bounds<Pixels>)>>,
    shown_at: Option<Instant>,
    side: Placement,
    ring_suppressed: bool,
    on_focus: Option<FocusFn>,
    focus_subscription: Option<Subscription>,
}

fn multi_state(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<MultiState> {
    window.use_keyed_state(
        ElementId::Name(format!("{id}-multi").into()),
        cx,
        |_, cx| MultiState {
            open: false,
            sheet: false,
            highlight: None,
            cell: Cell::Option,
            keyboard: false,
            query: String::new(),
            at_open: Vec::new(),
            typeahead: Typeahead::new(),
            pending_focus: false,
            announced_empty: None,
            default_opened: false,
            listbox_focus: cx.focus_handle().tab_stop(false),
            request: None,
            on_open_change: None,
            grid: false,
            reveal: false,
            scroll: ScrollHandle::new(),
            rows: Vec::new(),
            cells: Vec::new(),
            shown_at: None,
            side: Placement::Below,
            ring_suppressed: false,
            on_focus: None,
            focus_subscription: None,
        },
    )
}

/// Upstream `sortedItems` and the panel's grouping: the create row, the
/// select-all row, then the options matching `query`, each run between
/// dividers and each section with the `chosen` options first. Dividers
/// hide while searching; a section with no match hides.
fn build_rows(
    entries: &[SelectorEntry],
    query: &str,
    chosen: &[SharedString],
    create: Option<SharedString>,
    select_all: bool,
) -> (Vec<Row>, Vec<Block>, usize) {
    let mut rows = Vec::new();
    let mut blocks = Vec::new();
    if let Some(query) = create {
        rows.push(Row::Create(query));
        blocks.push(Block::Row(rows.len() - 1));
    }
    if select_all {
        rows.push(Row::SelectAll);
        blocks.push(Block::Row(rows.len() - 1));
    }
    let leading = rows.len();
    let chosen_first = |items: Vec<SelectorOptionData>| {
        let (mut first, rest): (Vec<_>, Vec<_>) =
            items.into_iter().partition(|o| chosen.contains(&o.value));
        first.extend(rest);
        first
    };
    let mut pending: Vec<SelectorOptionData> = Vec::new();
    let flush =
        |pending: &mut Vec<SelectorOptionData>, rows: &mut Vec<Row>, blocks: &mut Vec<Block>| {
            for o in chosen_first(std::mem::take(pending)) {
                rows.push(Row::Option(o));
                blocks.push(Block::Row(rows.len() - 1));
            }
        };
    let searching = !query.is_empty();
    for entry in entries {
        match entry {
            SelectorEntry::Option(o) => {
                if matches_query(o, query) {
                    pending.push(o.clone());
                }
            }
            SelectorEntry::Divider => {
                flush(&mut pending, &mut rows, &mut blocks);
                if !searching {
                    blocks.push(Block::Divider);
                }
            }
            SelectorEntry::Section { title, options } => {
                flush(&mut pending, &mut rows, &mut blocks);
                let matched: Vec<SelectorOptionData> = options
                    .iter()
                    .filter(|o| matches_query(o, query))
                    .cloned()
                    .collect();
                if matched.is_empty() {
                    continue;
                }
                let mut indices = Vec::new();
                for o in chosen_first(matched) {
                    rows.push(Row::Option(o));
                    indices.push(rows.len() - 1);
                }
                blocks.push(Block::Section {
                    title: title.clone(),
                    rows: indices,
                });
            }
        }
    }
    flush(&mut pending, &mut rows, &mut blocks);
    (rows, blocks, leading)
}

/// Upstream `hasLabel`: an option is labelled `query`, ignoring case.
fn has_label(options: &[SelectorOptionData], query: &str) -> bool {
    let wanted = query.to_lowercase();
    options.iter().any(|o| o.label.to_lowercase() == wanted)
}

/// Several values from a list, upstream `MultiSelector`.
///
/// ```ignore
/// MultiSelector::new("columns", "Columns", ["Name", "Owner", "Status"], shown.clone())
///     .on_change(|values, _, _, _| { /* store them */ })
/// ```
#[derive(IntoElement)]
pub struct MultiSelector {
    id: ElementId,
    label: SharedString,
    entries: Vec<SelectorEntry>,
    value: Vec<SharedString>,
    on_change: Option<ValuesFn>,
    change_action: Option<ActionFn<Vec<SharedString>>>,
    placeholder: Option<SharedString>,
    size: Option<ElementSize>,
    variant: SelectorVariant,
    trigger_display: TriggerDisplay,
    format_value: Option<FormatFn>,
    max_badges: usize,
    select_all: bool,
    select_all_label: Option<SharedString>,
    search: bool,
    search_placeholder: Option<SharedString>,
    empty_text: Option<SharedString>,
    empty_search_text: Option<SharedString>,
    create: bool,
    disabled: bool,
    read_only: bool,
    disabled_message: Option<SharedString>,
    label_hidden: bool,
    description: Option<SharedString>,
    optional: bool,
    required: bool,
    loading: bool,
    status: Option<InputStatus>,
    status_placement: Option<StatusPlacement>,
    render_option: Option<RenderFn>,
    indicator_position: SelectorIndicatorPosition,
    presentation: PopoverPresentation,
    width: Option<Length>,
    start_icon: Option<AnyElement>,
    clear: bool,
    default_open: bool,
    render_trigger: Option<TriggerFn>,
    controller: Option<MultiSelectorController>,
    on_open_change: Option<OpenFn>,
    on_click: Option<ClickFn>,
    on_focus: Option<FocusFn>,
    on_key_down: Option<KeyFn>,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl MultiSelector {
    /// Upstream `label`, `options` and `value`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        options: impl IntoIterator<Item = impl Into<SelectorEntry>>,
        value: Vec<SharedString>,
    ) -> Self {
        MultiSelector {
            id: id.into(),
            label: label.into(),
            entries: options.into_iter().map(Into::into).collect(),
            value,
            on_change: None,
            change_action: None,
            placeholder: None,
            size: None,
            variant: SelectorVariant::Input,
            trigger_display: TriggerDisplay::Count,
            format_value: None,
            max_badges: 3,
            select_all: false,
            select_all_label: None,
            search: false,
            search_placeholder: None,
            empty_text: None,
            empty_search_text: None,
            create: false,
            disabled: false,
            read_only: false,
            disabled_message: None,
            label_hidden: false,
            description: None,
            optional: false,
            required: false,
            loading: false,
            status: None,
            status_placement: None,
            render_option: None,
            indicator_position: SelectorIndicatorPosition::Start,
            presentation: PopoverPresentation::Popover,
            width: None,
            start_icon: None,
            clear: false,
            default_open: false,
            render_trigger: None,
            controller: None,
            on_open_change: None,
            on_click: None,
            on_focus: None,
            on_key_down: None,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `placeholder`.
        ?placeholder: SharedString,
        /// Upstream `size`.
        ?size: ElementSize,
        /// Upstream `variant`.
        variant: SelectorVariant,
        /// Upstream `triggerDisplay`.
        trigger_display: TriggerDisplay,
        /// Upstream `hasSelectAll`.
        !select_all,
        /// Upstream `selectAllLabel`.
        ?select_all_label: SharedString,
        /// Upstream `hasSearch`.
        !search,
        /// Upstream `searchPlaceholder`.
        ?search_placeholder: SharedString,
        /// Upstream `emptyText`.
        ?empty_text: SharedString,
        /// Upstream `emptySearchText`.
        ?empty_search_text: SharedString,
        /// Upstream `hasCreate`: with search, a Create row (FR9).
        !create,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `isReadOnly`.
        !read_only,
        /// Upstream `disabledMessage`.
        ?disabled_message: SharedString,
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isLoading`.
        !loading,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `statusVariant`.
        ?status_placement: StatusPlacement,
        /// Upstream `indicatorPosition`: the checkbox's edge (start by
        /// default).
        indicator_position: SelectorIndicatorPosition,
        /// Upstream `presentation`.
        presentation: PopoverPresentation,
        /// Upstream `width`.
        ?width: Length,
        /// Upstream `hasClear`.
        !clear,
        /// Upstream `isDefaultOpen`.
        !default_open,
    }

    /// Upstream `maxBadges`: 3 by default.
    pub fn max_badges(mut self, n: usize) -> Self {
        self.max_badges = n;
        self
    }

    /// Upstream `onChange`: the values, and a create's descriptor.
    pub fn on_change(
        mut self,
        f: impl Fn(Vec<SharedString>, Option<MultiSelectorChange>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`.
    pub fn change_action(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `formatValue`: the trigger's text from (value, label) pairs.
    pub fn format_value(
        mut self,
        f: impl Fn(&[(SharedString, SharedString)]) -> SharedString + 'static,
    ) -> Self {
        self.format_value = Some(Rc::new(f));
        self
    }

    /// Upstream `renderOption`.
    pub fn render_option(
        mut self,
        f: impl Fn(&SelectorOptionData, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_option = Some(Rc::new(f));
        self
    }

    /// Upstream `startIcon`.
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `renderTrigger`: the caller's control, which
    /// [`MultiTrigger::attach`] wires; no field renders (FR8).
    pub fn render_trigger(
        mut self,
        f: impl Fn(MultiTrigger, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_trigger = Some(Rc::new(f));
        self
    }

    /// Upstream `handleRef`.
    pub fn controller(mut self, controller: MultiSelectorController) -> Self {
        self.controller = Some(controller);
        self
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `onClick`: after the built-in toggle.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `onFocus`.
    pub fn on_focus(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_focus = Some(Rc::new(f));
        self
    }

    /// Upstream `onKeyDown`: keys the built-in model leaves alone.
    pub fn on_key_down(
        mut self,
        f: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_key_down = Some(Rc::new(f));
        self
    }

    /// Upstream `controlXstyle`.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl Styled for MultiSelector {
    /// Upstream `containerXstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What the handlers share in one frame.
struct Shared {
    state: Entity<MultiState>,
    trigger: FocusHandle,
    editor: Option<Entity<TextEditState>>,
    rows: Rc<Vec<Row>>,
    /// Every option, sections flattened (upstream `selectableItems`).
    options: Rc<Vec<SelectorOptionData>>,
    value: Vec<SharedString>,
    change: ChangeFn<(Vec<SharedString>, Option<MultiSelectorChange>)>,
    search: bool,
    clearable: bool,
    inert: bool,
    sheet: bool,
    external: bool,
    grid: bool,
    rtl: bool,
    loading: bool,
    can_create: bool,
    select_all_label: SharedString,
}

impl Shared {
    /// Upstream `onOpen` and the presentation's show. False when it
    /// cannot open.
    fn open(&self, highlight: Option<usize>, window: &mut Window, cx: &mut App) -> bool {
        if self.inert {
            return false;
        }
        if self.state.read(cx).open {
            return true;
        }
        let now = cx.background_executor().now();
        let keyboard = window.last_input_was_keyboard();
        let value = self.value.clone();
        let focus_inside = self.search || self.sheet || self.external;
        let sheet = self.sheet;
        let report = self.state.update(cx, |s, _| {
            s.open = true;
            s.sheet = sheet;
            s.highlight = highlight;
            s.cell = Cell::Option;
            s.keyboard = keyboard;
            s.at_open = value;
            s.query.clear();
            s.pending_focus = focus_inside;
            s.reveal = highlight.is_some();
            s.shown_at = Some(now);
            s.announced_empty = None;
            s.ring_suppressed = false;
            s.on_open_change.clone()
        });
        if let Some(f) = report {
            f(true, window, cx);
        }
        window.refresh();
        true
    }

    /// Upstream `closeAndReset` and the presentation's hide.
    fn close(&self, window: &mut Window, cx: &mut App) {
        if !self.state.read(cx).open {
            return;
        }
        let report = self.state.update(cx, |s, _| {
            s.open = false;
            s.highlight = None;
            s.cell = Cell::Option;
            s.query.clear();
            s.typeahead.reset();
            s.announced_empty = None;
            s.pending_focus = false;
            s.reveal = false;
            s.shown_at = None;
            s.on_open_change.clone()
        });
        if let Some(editor) = &self.editor {
            editor.update(cx, |e, cx| e.set_text("", cx));
        }
        announce("", Politeness::Polite, window, cx);
        // FR15: focus that would be lost returns to the trigger; a caller's
        // control always takes it back (FR8).
        let focused = window.focused(cx);
        let listbox = self.state.read(cx).listbox_focus.clone();
        let editor_focus = self.editor.as_ref().map(|e| e.read(cx).focus_handle(cx));
        let lost = match &focused {
            None => true,
            Some(f) => *f == listbox || editor_focus.as_ref() == Some(f) || *f == self.trigger,
        };
        if lost || self.external {
            self.trigger.focus(window, cx);
            let pointer = !window.last_input_was_keyboard();
            self.state.update(cx, |s, _| s.ring_suppressed = pointer);
        }
        if let Some(f) = report {
            f(false, window, cx);
        }
        window.refresh();
    }

    /// Upstream `announceSelection` (FR14).
    fn announce_selection(&self, next: &[SharedString], window: &mut Window, cx: &mut App) {
        let total = self.options.len();
        let count = next
            .iter()
            .filter(|v| self.options.iter().any(|o| &o.value == *v))
            .count();
        let t18 = i18n(cx);
        let text = if count == 0 {
            t18.t("@astryx.multiSelector.selectionCleared")
        } else if total > 0 && count == total {
            t18.t("@astryx.multiSelector.allSelected")
        } else {
            t18.format(
                "@astryx.multiSelector.selectionCount",
                &[
                    ("count", Arg::Num(count as f64)),
                    ("total", Arg::Num(total as f64)),
                ],
            )
        };
        announce(text, Politeness::Polite, window, cx);
    }

    /// A toggle, select-all or clear: report it and say what it became.
    fn commit(&self, next: Vec<SharedString>, window: &mut Window, cx: &mut App) {
        if self.inert {
            return;
        }
        (self.change)((next.clone(), None), window, cx);
        self.announce_selection(&next, window, cx);
    }

    fn clear(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| s.typeahead.reset());
        self.commit(Vec::new(), window, cx);
    }

    /// Upstream `handleNavigableToggle`: the row's toggle.
    fn activate(&self, index: usize, window: &mut Window, cx: &mut App) {
        match self.rows.get(index) {
            Some(Row::Option(o)) if !o.disabled => {
                let mut next = self.value.clone();
                if let Some(at) = next.iter().position(|v| *v == o.value) {
                    next.remove(at);
                } else {
                    next.push(o.value.clone());
                }
                self.commit(next, window, cx);
            }
            Some(Row::SelectAll) => self.select_all(window, cx),
            Some(Row::Create(query)) => self.create(query.clone(), window, cx),
            _ => {}
        }
    }

    /// Upstream `handleSelectAll`: every enabled option the query shows.
    fn select_all(&self, window: &mut Window, cx: &mut App) {
        let enabled: Vec<SharedString> = self
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Option(o) if !o.disabled => Some(o.value.clone()),
                _ => None,
            })
            .collect();
        let all = !enabled.is_empty() && enabled.iter().all(|v| self.value.contains(v));
        let next: Vec<SharedString> = if all {
            self.value
                .iter()
                .filter(|v| !enabled.contains(v))
                .cloned()
                .collect()
        } else {
            let mut next = self.value.clone();
            next.extend(enabled.into_iter().filter(|v| !self.value.contains(v)));
            next
        };
        self.commit(next, window, cx);
    }

    /// Upstream `commitCreate` (FR9): the query becomes a value.
    fn create(&self, query: SharedString, window: &mut Window, cx: &mut App) {
        if self.inert {
            return;
        }
        let mut next = self.value.clone();
        if !next.contains(&query) {
            next.push(query.clone());
        }
        self.set_query(String::new(), window, cx);
        if let Some(editor) = &self.editor {
            editor.update(cx, |e, cx| e.set_text("", cx));
        }
        let text = i18n(cx).format(
            "@astryx.multiSelector.optionCreated",
            &[("label", query.as_ref().into())],
        );
        announce(text, Politeness::Polite, window, cx);
        (self.change)(
            (next, Some(MultiSelectorChange::Create { query })),
            window,
            cx,
        );
    }

    /// Upstream `handleSearchChange` (FR14, FR16): a new query clears the
    /// highlight and says how many options match.
    fn set_query(&self, text: String, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| {
            s.query = text.clone();
            s.highlight = None;
            s.cell = Cell::Option;
            s.announced_empty = None;
        });
        let t18 = i18n(cx);
        if text.is_empty() || self.loading {
            announce("", Politeness::Polite, window, cx);
        } else {
            let count = self
                .options
                .iter()
                .filter(|o| matches_query(o, &text))
                .count();
            if count > 0 {
                let said = t18.format(
                    "@astryx.multiSelector.resultCount",
                    &[("count", Arg::Num(count as f64))],
                );
                announce(said, Politeness::Polite, window, cx);
            } else if let Some(query) = self.create_query(&text) {
                let said = t18.format(
                    "@astryx.multiSelector.createOption",
                    &[("query", query.as_ref().into())],
                );
                announce(said, Politeness::Polite, window, cx);
            }
            // Otherwise the empty panel says it, once drawn.
        }
        window.refresh();
    }

    /// Upstream `getCreateQuery`.
    fn create_query(&self, query: &str) -> Option<SharedString> {
        let trimmed = query.trim();
        (self.can_create && !trimmed.is_empty() && !has_label(&self.options, trimmed))
            .then(|| SharedString::from(trimmed.to_string()))
    }

    fn enabled(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|i| !self.rows[*i].disabled())
            .collect()
    }

    /// Upstream `findOpeningIndex`: never a disabled option.
    fn opening_index(&self, from_end: bool) -> Option<usize> {
        let enabled = self.enabled();
        if from_end {
            enabled.last().copied()
        } else {
            enabled.first().copied()
        }
    }

    fn set_highlight(&self, highlight: Option<usize>, window: &mut Window, cx: &mut App) {
        let keyboard = window.last_input_was_keyboard();
        self.state.update(cx, |s, _| {
            s.highlight = highlight;
            s.cell = Cell::Option;
            s.keyboard = keyboard;
            s.reveal = highlight.is_some();
        });
        window.refresh();
    }

    fn set_cell(&self, cell: Cell, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| {
            s.cell = cell;
            s.keyboard = true;
        });
        window.refresh();
    }

    /// Hovering a row's cell (upstream `onItemMouseEnter`).
    fn hover(&self, index: usize, cell: Cell, window: &mut Window, cx: &mut App) {
        let row = &self.rows[index];
        if cell == Cell::Option && row.disabled() {
            return;
        }
        let same = {
            let s = self.state.read(cx);
            s.highlight == Some(index) && s.cell == cell && !s.keyboard
        };
        if !same {
            self.state.update(cx, |s, _| {
                s.highlight = Some(index);
                s.cell = cell;
                s.keyboard = false;
            });
            window.refresh();
        }
    }

    /// Enter on the action cell presses its control (FR10).
    fn activate_action(&self, index: usize, window: &mut Window, cx: &mut App) {
        let cell = self.state.read(cx).cells.get(index).cloned().flatten();
        if let Some((cell, bounds)) = cell {
            cell.activate(bounds, window, cx);
        }
    }

    /// Upstream `useMultiCombobox`'s keys (FR10, FR12, FR18). True when
    /// the key was handled (a caller's `on_key_down` then sees nothing).
    fn key(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App) -> bool {
        if self.inert {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let (open, highlight, cell) = {
            let s = self.state.read(cx);
            (s.open, s.highlight.filter(|h| *h < self.rows.len()), s.cell)
        };
        // Without search, typing jumps to a match; Space stays the toggle.
        if !self.search && key != "space" && !is_shortcut_chord(&event.keystroke) {
            let labels: Vec<SharedString> =
                self.rows.iter().map(|r| self.row_label(r, cx)).collect();
            let rows = self.rows.clone();
            let current = if open { highlight } else { None };
            let found = self.state.update(cx, |s, _| {
                s.typeahead.key(
                    &event.keystroke,
                    labels.len(),
                    |i| labels.get(i).map(|l| l.as_ref()),
                    current,
                    |i| rows.get(i).is_some_and(|r| r.disabled()),
                )
            });
            if let Some(found) = found {
                if let Some(index) = found
                    && (open || self.open(Some(index), window, cx))
                {
                    self.set_highlight(Some(index), window, cx);
                }
                return true;
            }
        }
        // In a grid the inline arrows move between a row's cells, following
        // the visual direction (FR10).
        if self.grid
            && open
            && let Some(h) = highlight
        {
            let (inline_end, inline_start) = if self.rtl {
                ("left", "right")
            } else {
                ("right", "left")
            };
            if key == inline_end {
                if cell == Cell::Option && self.rows[h].has_action() {
                    self.set_cell(Cell::Action, window, cx);
                    return true;
                }
                return false;
            }
            if key == inline_start {
                if cell == Cell::Action {
                    self.set_cell(Cell::Option, window, cx);
                    return true;
                }
                return false;
            }
        }
        let enabled = self.enabled();
        let pos = highlight.and_then(|h| enabled.iter().position(|&i| i == h));
        match key {
            "down" => {
                if !open {
                    self.open(self.opening_index(false), window, cx);
                } else {
                    let next = pos.map_or(0, |p| (p + 1).min(enabled.len().saturating_sub(1)));
                    self.set_highlight(enabled.get(next).copied().or(highlight), window, cx);
                }
                true
            }
            "up" => {
                if !open {
                    self.open(self.opening_index(true), window, cx);
                } else {
                    let prev = pos.map_or(0, |p| p.saturating_sub(1));
                    self.set_highlight(enabled.get(prev).copied().or(highlight), window, cx);
                }
                true
            }
            "enter" | "space" => {
                if key == "space" && self.search {
                    return false;
                }
                if open && let Some(h) = highlight {
                    if self.grid && cell == Cell::Action {
                        self.activate_action(h, window, cx);
                    } else {
                        self.activate(h, window, cx);
                    }
                } else if !open {
                    let h = if self.search {
                        None
                    } else {
                        self.opening_index(false)
                    };
                    self.open(h, window, cx);
                }
                true
            }
            "escape" if open => {
                self.close(window, cx);
                true
            }
            "home" | "end" | "pageup" | "pagedown" => {
                if open && !enabled.is_empty() {
                    let first = matches!(key, "home" | "pageup");
                    let at = if first {
                        enabled[0]
                    } else {
                        enabled[enabled.len() - 1]
                    };
                    self.set_highlight(Some(at), window, cx);
                }
                true
            }
            "delete" | "backspace" if !self.search && !open && self.clearable => {
                self.clear(window, cx);
                true
            }
            _ => false,
        }
    }

    /// The search field's keys: Enter with nothing lit creates (FR9); the
    /// list's keys go to the list; the rest edit the query.
    fn search_key(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App) -> bool {
        // An input method's composition keeps its keys.
        if let Some(editor) = &self.editor
            && editor.read(cx).buffer().marked().is_some()
        {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let highlight = self.state.read(cx).highlight;
        if key == "enter" && highlight.is_none() {
            let query = self.state.read(cx).query.clone();
            if let Some(query) = self.create_query(&query) {
                self.create(query, window, cx);
                return true;
            }
        }
        if matches!(
            key,
            "down" | "up" | "pageup" | "pagedown" | "enter" | "escape"
        ) {
            return self.key(event, window, cx);
        }
        if self.grid && highlight.is_some() && matches!(key, "left" | "right") {
            return self.key(event, window, cx);
        }
        false
    }

    /// A row's text: its label, or the select-all and create rows' own.
    fn row_label(&self, row: &Row, cx: &App) -> SharedString {
        match row {
            Row::Option(o) => o.label.clone(),
            Row::SelectAll => self.select_all_label.clone(),
            Row::Create(q) => i18n(cx).format(
                "@astryx.multiSelector.createOption",
                &[("query", q.as_ref().into())],
            ),
        }
    }

    fn toggle_open(&self, window: &mut Window, cx: &mut App) {
        if self.state.read(cx).open {
            self.close(window, cx);
        } else {
            let h = if self.search {
                None
            } else {
                self.opening_index(false)
            };
            self.open(h, window, cx);
        }
    }
}

/// The closed trigger's text (upstream `renderTriggerContent`): the
/// placeholder, the caller's `format_value`, the count, or the first three
/// labels as a list ending in "+N". Badges show the labels as text here,
/// for the trigger's accessible value.
pub(crate) fn trigger_text(
    display: TriggerDisplay,
    selected: &[(SharedString, SharedString)],
    format: Option<&FormatFn>,
    placeholder: SharedString,
    cx: &App,
) -> SharedString {
    let t18 = i18n(cx);
    if selected.is_empty() {
        return placeholder;
    }
    if let Some(f) = format.filter(|_| display != TriggerDisplay::Badges) {
        return f(selected);
    }
    match display {
        TriggerDisplay::Count => t18.format(
            "@astryx.multiSelector.selectedCount",
            &[("count", Arg::Num(selected.len() as f64))],
        ),
        TriggerDisplay::Labels => {
            let mut parts: Vec<String> = selected
                .iter()
                .take(3)
                .map(|(_, l)| l.to_string())
                .collect();
            if selected.len() > 3 {
                let more = t18.format(
                    "@astryx.multiSelector.overflowCount",
                    &[("count", Arg::Num((selected.len() - 3) as f64))],
                );
                parts.push(more.to_string());
            }
            t18.conjunction(parts.iter().map(|s| s.as_str())).into()
        }
        TriggerDisplay::Badges => selected
            .iter()
            .map(|(_, l)| l.as_ref())
            .collect::<Vec<_>>()
            .join(", ")
            .into(),
    }
}

/// After a frame: scroll a key's highlight into view.
fn reveal(state: &Entity<MultiState>, window: &mut Window, cx: &mut App) {
    let changed = state.update(cx, |s, _| {
        if !s.reveal {
            return false;
        }
        s.reveal = false;
        let Some(row) = s.highlight.and_then(|h| s.rows.get(h).copied().flatten()) else {
            return false;
        };
        let viewport = s.scroll.bounds();
        let mut offset = s.scroll.offset();
        let before = offset.y;
        if row.top() < viewport.top() {
            offset.y += viewport.top() - row.top();
        } else if row.bottom() > viewport.bottom() {
            offset.y -= row.bottom() - viewport.bottom();
        }
        s.scroll.set_offset(offset);
        s.scroll.offset().y != before
    });
    if changed {
        window.refresh();
    }
}

impl RenderOnce for MultiSelector {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // An enclosing InputGroup's disabled state, never re-enabling.
        self.disabled |= group_disabled(cx);
        let place = group_place(cx);
        let group = place.and_then(|_| group_context(cx));
        let dir = use_direction(cx);
        let rtl = dir.is_rtl();
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let t18 = i18n(cx);
        let read_only = self.read_only && !self.disabled;
        let inert = self.disabled || read_only;
        let input = self.variant == SelectorVariant::Input;
        let status_placement = match self.status_placement.unwrap_or(StatusPlacement::Attached) {
            StatusPlacement::Attached if !input => StatusPlacement::Detached,
            p => p,
        };

        // The change pipeline: on_change, then changeAction with the new
        // values shown while it runs.
        let optimistic = Optimistic::<Vec<SharedString>>::new(&self.id, window, cx);
        let value = optimistic.shown(self.value.clone(), cx);
        let busy = self.loading || optimistic.pending(cx);
        optimistic.announce_busy(busy, t18.t("@astryx.spinner.loading"), window, cx);
        let report = self.on_change.clone();
        let pipeline = Rc::new(optimistic.change(None, self.change_action.clone()));
        let change: ChangeFn<(Vec<SharedString>, Option<MultiSelectorChange>)> =
            Rc::new(move |(next, c), window, cx| {
                if let Some(f) = &report {
                    f(next.clone(), c, window, cx);
                }
                pipeline(next, window, cx);
            });

        let state = match &self.controller {
            Some(c) => c.state.clone(),
            None => multi_state(&self.id, window, cx),
        };
        state.update(cx, |s, _| s.on_open_change = self.on_open_change.clone());
        let trigger = control_focus(
            &ElementId::Name(format!("{}-trigger", self.id).into()),
            window,
            cx,
        );
        let editor = self.search.then(|| {
            window.use_keyed_state(
                ElementId::Name(format!("{}-search", self.id).into()),
                cx,
                |window, cx| TextEditState::reporting("", false, window, cx),
            )
        });
        let options: Rc<Vec<SelectorOptionData>> = Rc::new(selectable_options(&self.entries));
        // FR10: a grid once any option carries an action, for good.
        if options.iter().any(|o| o.action.is_some()) && !state.read(cx).grid {
            state.update(cx, |s, _| s.grid = true);
        }
        let (open, query, at_open, grid) = {
            let s = state.read(cx);
            (s.open, s.query.clone(), s.at_open.clone(), s.grid)
        };
        let chosen = if open { at_open } else { value.clone() };
        let can_create = self.create && self.search && !self.loading;
        let create_query = {
            let trimmed = query.trim();
            (can_create && !trimmed.is_empty() && !has_label(&options, trimmed))
                .then(|| SharedString::from(trimmed.to_string()))
        };
        let (rows, blocks, leading) = build_rows(
            &self.entries,
            &query,
            &chosen,
            create_query.clone(),
            self.select_all,
        );
        state.update(cx, |s, _| {
            s.rows.resize(rows.len(), None);
            s.cells.resize(rows.len(), None);
        });
        let rows = Rc::new(rows);
        let sheet_now = crate::overlay::presents_as_sheet(self.presentation, window, cx);
        let select_all_label = self
            .select_all_label
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.multiSelector.selectAll"));
        let shared = Rc::new(Shared {
            state: state.clone(),
            trigger: trigger.clone(),
            editor: editor.clone(),
            rows: rows.clone(),
            options: options.clone(),
            value: value.clone(),
            change,
            search: self.search,
            clearable: self.clear && !value.is_empty(),
            inert,
            sheet: sheet_now,
            external: self.render_trigger.is_some(),
            grid,
            rtl,
            loading: self.loading,
            can_create,
            select_all_label: select_all_label.clone(),
        });
        // A controller's request, the default open, and a read-only or
        // disabled field closing its list (FR7).
        if let Some(open) = state.update(cx, |s, _| s.request.take()) {
            if open {
                shared.open(None, window, cx);
            } else {
                shared.close(window, cx);
            }
        }
        if self.default_open && !state.read(cx).default_opened {
            state.update(cx, |s, _| s.default_opened = true);
            let h = if self.search {
                None
            } else {
                shared.opening_index(false)
            };
            shared.open(h, window, cx);
        }
        if inert && state.read(cx).open {
            shared.close(window, cx);
        }
        if state.read(cx).ring_suppressed && !trigger.is_focused(window) {
            state.update(cx, |s, _| s.ring_suppressed = false);
        }
        // Upstream onFocus.
        state.update(cx, |s, _| s.on_focus = self.on_focus.clone());
        if self.on_focus.is_some() && state.read(cx).focus_subscription.is_none() {
            let (weak, focus) = (state.downgrade(), trigger.clone());
            let subscription = window.on_focus_in(&trigger, cx, move |window, cx| {
                // The trigger itself, not its clear button.
                if !focus.is_focused(window) {
                    return;
                }
                let f = weak.upgrade().and_then(|s| s.read(cx).on_focus.clone());
                if let Some(f) = f {
                    f(window, cx);
                }
            });
            state.update(cx, |s, _| s.focus_subscription = Some(subscription));
        }
        let (open, highlight, cell, keyboard, opened_sheet) = {
            let s = state.read(cx);
            (
                s.open,
                s.highlight.filter(|h| *h < rows.len()),
                s.cell,
                s.keyboard,
                s.sheet,
            )
        };
        // The presentation an opening chose holds until its sheet has played its exit: a
        // sheet that closes after the pointer turned fine still slides away (upstream
        // `useSelectorPresentation`'s presentation holds).
        let sheet = if open {
            opened_sheet
        } else {
            (opened_sheet
                && crate::overlay::is_sheet_presented(&ElementId::Name(
                    format!("{}-sheet", self.id).into(),
                )))
                || sheet_now
        };
        let popup_kind = if sheet {
            HasPopup::Dialog
        } else if grid {
            HasPopup::Grid
        } else {
            HasPopup::Listbox
        };
        let key_fn: HandledKeyFn = {
            let shared = shared.clone();
            Rc::new(move |event, window, cx| shared.key(event, window, cx))
        };
        let toggle: ActFn = {
            let shared = shared.clone();
            Rc::new(move |window, cx| shared.toggle_open(window, cx))
        };

        // The panel (FR2, FR5): one tree for the popup and the sheet.
        let listbox_focus = state.read(cx).listbox_focus.clone();
        let list_owns_keys = !self.search && (sheet || self.render_trigger.is_some());
        let owner = match &editor {
            Some(editor) => editor.read(cx).focus_handle(cx),
            None if list_owns_keys => listbox_focus.clone(),
            None => trigger.clone(),
        };
        let panel = (open && !read_only).then(|| {
            self.panel(
                PanelArgs {
                    shared: &shared,
                    blocks: &blocks,
                    leading,
                    owner: &owner,
                    highlight,
                    cell,
                    keyboard,
                    size,
                    query: &query,
                    select_all_label: &select_all_label,
                    list_owns_keys,
                    has_create: create_query.is_some(),
                },
                window,
                cx,
            )
        });

        // The trigger: the caller's control (FR8), or the field's own.
        let status = self.status.as_ref().map(|s| s.status);
        let trigger_el: AnyElement = match self.render_trigger.clone() {
            Some(render) => render(
                MultiTrigger {
                    focus: trigger.clone(),
                    expanded: open,
                    busy,
                    popup: popup_kind,
                    toggle: toggle.clone(),
                    key: key_fn.clone(),
                },
                window,
                cx,
            ),
            None => self.trigger_box(
                TriggerArgs {
                    shared: &shared,
                    trigger: &trigger,
                    toggle: &toggle,
                    key_fn: &key_fn,
                    options: &options,
                    value: &value,
                    open,
                    busy,
                    popup: popup_kind,
                    size,
                    status,
                    status_placement,
                    place,
                    dir,
                    group: group.as_ref(),
                },
                window,
                cx,
            ),
        };

        // The surface: an anchored popup, or a bottom sheet.
        let mut layer = anchored_layer(
            ElementId::Name(format!("{}-layer", self.id).into()),
            trigger_el,
        )
        .light_dismiss(true)
        .escape(EscapeBehavior::Close)
        .match_anchor_width(true);
        let mut sheet_el = None;
        match panel {
            Some(panel) if !sheet => {
                let theme = current(window, cx);
                let t = theme.tokens();
                let cap = crate::overlay::viewport_cap(t, window);
                let (since, side) = {
                    let s = state.read(cx);
                    (
                        s.shown_at.unwrap_or_else(|| cx.background_executor().now()),
                        s.side,
                    )
                };
                let mut surface = div()
                    .id(ElementId::Name(format!("{}-popup", self.id).into()))
                    .occlude()
                    .relative()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .max_w(cap.width)
                    .max_h(cap.height)
                    .overflow_hidden()
                    .bg(t.color.background_popover)
                    .rounded(t.radius.container)
                    .shadow_token(&t.shadow.low)
                    .font_stack(&t.font.family.body)
                    .text_color(t.color.text_primary)
                    .child(panel);
                theme.refine(surface.style(), "multi-selector-popup", &[], None);
                let surface = crate::overlay::entrance(surface, since, side, rtl, t, window, cx);
                let options = LayerOptions {
                    placement: Placement::Below,
                    align: LayerAlign::Start,
                    offset: crate::overlay::clearance(t, window),
                    margin: crate::overlay::gutter(t, window),
                    rtl,
                    priority: 1,
                };
                let (dismiss, placed) = (shared.clone(), state.clone());
                layer = layer
                    .surface(surface, options, move |window, cx| {
                        dismiss.close(window, cx)
                    })
                    .on_placed(move |position, _, cx| {
                        placed.update(cx, |s, _| s.side = position.side);
                    });
            }
            panel => {
                if opened_sheet || sheet {
                    let close = shared.clone();
                    let mut bottom = crate::overlay::BottomSheet::new(
                        ElementId::Name(format!("{}-sheet", self.id).into()),
                        open && sheet,
                        self.label.clone(),
                    )
                    .on_open_change(move |open, window, cx| {
                        if !open {
                            close.close(window, cx);
                        }
                    });
                    if let Some(panel) = panel {
                        let theme = current(window, cx);
                        let t = theme.tokens();
                        bottom = bottom.child(
                            div()
                                .w_full()
                                .pt(t.spacing.s4)
                                .px(t.spacing.s1)
                                .flex()
                                .flex_col()
                                .child(div().mb(t.spacing.s2).mx(t.spacing.s3).child(
                                    crate::text::Heading::new(
                                        crate::text::HeadingLevel::H3,
                                        self.label.clone(),
                                    ),
                                ))
                                .child(panel),
                        );
                    }
                    sheet_el = Some(bottom.into_any_element());
                }
            }
        }
        // FR8: a caller's control renders no Field, status or clear action.
        if self.render_trigger.is_some() {
            return div()
                .display_contents()
                .child(layer)
                .children(sheet_el)
                .into_any_element();
        }
        // FR11: a disabled field's reason, on hover and focus.
        let control = crate::overlay::disabled_reason(
            &self.id,
            self.disabled,
            self.disabled_message.as_ref(),
            false,
            layer,
        );
        let content = div().display_contents().child(control).children(sheet_el);
        if place.is_some() {
            return inside_member(content).into_any_element();
        }
        let mut field = Field::new(self.id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(self.disabled)
            .optional(self.optional)
            .required(self.required)
            .status_placement(status_placement);
        if let Some(status) = self.status {
            field = field.status(status);
        }
        if let Some(width) = self.width {
            field = field.width(width);
        }
        field.style().refine(&self.style);
        field.child(content).into_any_element()
    }
}

/// What the built-in trigger reads.
struct TriggerArgs<'a> {
    shared: &'a Rc<Shared>,
    trigger: &'a FocusHandle,
    toggle: &'a ActFn,
    key_fn: &'a HandledKeyFn,
    options: &'a [SelectorOptionData],
    value: &'a [SharedString],
    open: bool,
    busy: bool,
    popup: HasPopup,
    size: ElementSize,
    status: Option<StatusType>,
    status_placement: StatusPlacement,
    place: Option<crate::input::group::GroupPlace>,
    dir: astryx_i18n::Direction,
    group: Option<&'a crate::input::group::GroupContext>,
}

/// What the panel reads.
struct PanelArgs<'a> {
    shared: &'a Rc<Shared>,
    blocks: &'a [Block],
    leading: usize,
    owner: &'a FocusHandle,
    highlight: Option<usize>,
    cell: Cell,
    keyboard: bool,
    size: ElementSize,
    query: &'a str,
    select_all_label: &'a SharedString,
    list_owns_keys: bool,
    has_create: bool,
}

impl MultiSelector {
    /// The built-in trigger (FR1, FR13): the bordered or button-like control
    /// with the value, a busy spinner, the clear button, and the status or
    /// disclosure icon.
    fn trigger_box(&mut self, a: TriggerArgs<'_>, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = current(window, cx);
        let t18 = i18n(cx);
        let rem = window.rem_size();
        let shared = a.shared;
        let read_only = self.read_only && !self.disabled;
        let inert = shared.inert;
        let input = self.variant == SelectorVariant::Input;
        let (open, place, size, status) = (a.open, a.place, a.size, a.status);
        let ring_suppressed = shared.state.read(cx).ring_suppressed;
        let focused = a.trigger.is_focused(window);
        let focus_visible = is_focus_visible(a.trigger, window);
        let mut props: Vec<(&'static str, &'static str)> =
            vec![("variant", self.variant.as_str()), ("size", size.as_str())];
        if let Some(status) = status {
            props.push(("status", status.as_str()));
        }
        if self.disabled {
            props.push(("disabled", "disabled"));
        }
        if read_only {
            props.push(("readonly", "readonly"));
        }
        let part_override = theme.for_part("multi-selector", &props);
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let height = element_height(size, t).to_pixels(rem);
        let border = t.border.width.to_pixels(rem);
        let (line, pad) = trigger_line(size, t, rem, input && place.is_none());
        let control_id = ElementId::Name(format!("{}-control", self.id).into());
        let mut control = if input {
            let chrome = Chrome {
                id: control_id.clone(),
                target: "multi-selector",
                variant: Some("input"),
                size,
                status,
                disabled: self.disabled,
                read_only,
                focused: focused && !ring_suppressed,
                focus_visible: focus_visible && !ring_suppressed,
                row: false,
                place,
                dir: a.dir,
            };
            control_box(&chrome, &part, window, cx)
        } else {
            let fill = match self.variant {
                SelectorVariant::Secondary => t.color.neutral,
                _ => gpui::transparent_black(),
            };
            let mut el = div()
                .id(control_id.clone())
                .debug_selector({
                    let id = control_id.to_string();
                    move || id
                })
                .relative()
                .flex()
                .flex_row()
                .self_start()
                .rounded(t.radius.element)
                .bg(fill)
                .font_weight(t.font.weight.medium);
            if !inert {
                let hover = astryx_theme::color::composite(fill, t.color.overlay_hover);
                el = el.hover(move |s| s.bg(hover));
                // A finger's press paints as UIKit tracks a control (upstream `usePressFeedback`).
                el = crate::press::touch_press(
                    el,
                    ElementId::Name(format!("{control_id:?}-trigger-press").into()),
                    false,
                    fill,
                    t.color.overlay_pressed,
                    window,
                    cx,
                );
                // The press scale, at once as the list opens.
                if !open && !prefers_reduced_motion(cx) && !window.last_input_was_touch() {
                    el = el.active(|s| s.scale(0.98));
                }
            }
            if self.disabled {
                el = el.opacity(0.5);
            }
            if self.variant == SelectorVariant::Secondary
                && let Some(place) = place
            {
                el = join(el, place, t.radius.element.to_pixels(rem), border, a.dir).flex_1();
                el.style().size.height = Some(relative(1.).into());
            }
            part.refine(el.style(), "multi-selector", &props, None);
            el
        };
        let empty = a.value.is_empty();
        control = control
            .track_focus(a.trigger)
            .items_center()
            .justify_between()
            .gap(t.spacing.s2)
            .px(t.spacing.s3)
            .font_stack(&t.font.family.body)
            .text_size(t.text.label.size)
            .line_height(line)
            .text_color(if empty {
                t.color.text_secondary
            } else {
                t.color.text_primary
            });
        if place.is_some() {
            control = control
                .py(px(0.))
                .min_h(px(0.))
                .line_height(t.spacing.s5.to_pixels(rem).max(px(20.)).max(rem));
        } else {
            // FR13: the shared element height, growing with the text.
            control = control.py(pad).min_h(height);
        }
        if input && place.is_none() {
            control = control.w_full();
        }
        control = if inert {
            control.cursor_default()
        } else {
            control.cursor_pointer()
        };
        control.style().refine(&self.control_style);
        if !input && focus_visible && !ring_suppressed {
            control = control.child(focus_ring(t, t.radius.element.to_pixels(rem), rem));
        }

        // The start icon, then the value.
        if let Some(icon) = self.start_icon.take() {
            control = control.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .text_color(t.color.icon_secondary)
                    .child(provide(IconDefaultSize(IconSize::Sm), icon)),
            );
        }
        let selected: Vec<(SharedString, SharedString)> = a
            .value
            .iter()
            .map(|v| {
                let label = a
                    .options
                    .iter()
                    .find(|o| &o.value == v)
                    .map_or(v.clone(), |o| o.label.clone());
                (v.clone(), label)
            })
            .collect();
        let placeholder = self
            .placeholder
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.multiSelector.selectPlaceholder"));
        let text = trigger_text(
            self.trigger_display,
            &selected,
            self.format_value.as_ref(),
            placeholder,
            cx,
        );
        let value_selector = format!("{}-value", self.id);
        if self.trigger_display == TriggerDisplay::Badges && !selected.is_empty() {
            let extra = selected.len().saturating_sub(self.max_badges);
            control = control.child(
                div()
                    .debug_selector(move || value_selector)
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .flex_wrap()
                    .items_center()
                    .gap(t.spacing.s1)
                    .overflow_hidden()
                    .children(
                        selected
                            .iter()
                            .take(self.max_badges)
                            .map(|(_, l)| Badge::new(l.clone())),
                    )
                    .when(extra > 0, |el| {
                        el.child(
                            div()
                                .flex_none()
                                .text_size(t.text.label.size)
                                .line_height(relative(t.text.label.leading))
                                .text_color(t.color.text_secondary)
                                .font_weight(t.font.weight.medium)
                                .child(t18.format(
                                    "@astryx.multiSelector.overflowCount",
                                    &[("count", Arg::Num(extra as f64))],
                                )),
                        )
                    }),
            );
        } else {
            control = control.child(
                div()
                    .debug_selector(move || value_selector)
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(text.clone()),
            );
        }
        // FR11: busy shows a spinner, ordinary paint.
        if a.busy {
            let busy_selector = format!("{}-busy", self.id);
            control = control.child(
                div()
                    .flex()
                    .flex_none()
                    .debug_selector(move || busy_selector)
                    .child(
                        Spinner::new(ElementId::Name(format!("{}-spinner", self.id).into()))
                            .size(SpinnerSize::Sm)
                            .decorative(),
                    ),
            );
        }
        // The clear button (FR6, FR7): never read-only or disabled.
        if self.clear && !empty && !inert {
            let label = t18.format(
                "@astryx.multiSelector.clearAll",
                &[("label", self.label.as_ref().into())],
            );
            let shared = shared.clone();
            let clear_selector = format!("{}-clear", self.id);
            control = control.child(
                div()
                    .flex()
                    .flex_none()
                    .debug_selector(move || clear_selector)
                    .child(
                        IconButton::new(
                            ElementId::Name(format!("{}-clear", self.id).into()),
                            label,
                            Icon::new("close")
                                .size(IconSize::Sm)
                                .color(IconColor::Secondary),
                        )
                        .variant(ButtonVariant::Ghost)
                        .size(ElementSize::Sm)
                        .tooltip(t18.t("@astryx.inputClearButton.tooltip"))
                        .h(px(20.))
                        .on_click(move |event, window, cx| {
                            cx.stop_propagation();
                            shared.clear(window, cx);
                            // A keyboard press leaves focus on a button that goes away.
                            if event.is_keyboard() {
                                shared.trigger.focus(window, cx);
                            }
                        }),
                    ),
            );
        }
        if let Some(status) = status.filter(|_| a.status_placement != StatusPlacement::Detached) {
            let message = self.status.as_ref().and_then(|s| s.message.clone());
            control = control.child(status_indicator(
                &self.id,
                status,
                a.status_placement,
                message.as_ref(),
                &part,
                window,
                cx,
            ));
        } else if !read_only {
            let turn = transition(
                format!("{}-chevron", self.id),
                open,
                t.duration.fast,
                t.ease.standard,
                window,
                cx,
            );
            let mut chevron = Icon::new("chevronDown")
                .size(IconSize::Sm)
                .color(IconColor::Secondary)
                .rotation(std::f32::consts::PI * turn);
            part.refine(
                chevron.style(),
                "multi-selector-indicator-icon",
                &[("state", if open { "expanded" } else { "collapsed" })],
                None,
            );
            control = control.child(chevron);
        }

        // Its name, value and states.
        let editable_search = self.search && !read_only;
        let name: SharedString = match a.group {
            Some(group) => format!("{} {}", group.label, self.label).into(),
            None => self.label.clone(),
        };
        let mut described = described_by(
            self.description.as_ref(),
            self.status.as_ref(),
            self.disabled
                .then_some(self.disabled_message.as_ref())
                .flatten(),
            &t18,
        );
        if let Some(group) = a.group {
            described = group.described.iter().cloned().chain(described).collect();
        }
        if read_only {
            described.push(t18.t("@astryx.input.readOnly"));
        }
        if editable_search && self.required {
            described.push(t18.t("@astryx.field.required"));
        }
        control = if editable_search {
            control
                .role(Role::Button)
                .aria_label(SharedString::from(format!("{name} {text}")))
        } else {
            control
                .role(Role::ComboBox)
                .aria_label(name)
                .aria_value(text.clone())
        };
        control = control
            .aria_expanded(open && !read_only)
            .aria_read_only(read_only)
            .aria_disabled(self.disabled)
            .aria_required(self.required && !editable_search)
            .aria_invalid(status == Some(StatusType::Error))
            .aria_busy(a.busy);
        if !read_only {
            control = control.aria_has_popup(a.popup);
        }
        if !described.is_empty() {
            control = control.aria_description(SharedString::from(described.join(" ")));
        }

        // Its handlers: the built-in toggle, then the caller's (FR13).
        {
            let trigger = a.trigger.clone();
            control = control.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                trigger.focus(window, cx);
            });
        }
        {
            let (toggle, on_click, disabled) =
                (a.toggle.clone(), self.on_click.clone(), self.disabled);
            control = control.on_click(move |event, window, cx| {
                // FR11: a disabled trigger's press reaches no handler.
                if disabled {
                    cx.stop_propagation();
                    return;
                }
                // Enter and Space act on key-down.
                if event.is_keyboard() {
                    return;
                }
                toggle(window, cx);
                if let Some(f) = &on_click {
                    f(event, window, cx);
                }
            });
        }
        {
            let (key_fn, on_key, disabled) =
                (a.key_fn.clone(), self.on_key_down.clone(), self.disabled);
            control = control.on_key_down(move |event, window, cx| {
                if key_fn(event, window, cx) {
                    cx.stop_propagation();
                } else if !disabled && let Some(f) = &on_key {
                    f(event, window, cx);
                }
            });
        }
        for back in [false, true] {
            let shared = shared.clone();
            let close = move |window: &mut Window, cx: &mut App| {
                shared.close(window, cx);
                cx.propagate();
            };
            control = if back {
                control.on_action(move |_: &FocusPrevious, window, cx| close(window, cx))
            } else {
                control.on_action(move |_: &FocusNext, window, cx| close(window, cx))
            };
        }
        match part_override {
            Some(part) => scope(part, control).into_any_element(),
            None => control.into_any_element(),
        }
    }

    /// The panel content (FR2, FR5): the search row, the rows in their
    /// groups, and the empty state.
    fn panel(&self, a: PanelArgs<'_>, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let t18 = i18n(cx);
        let cap = crate::overlay::viewport_cap(t, window);
        let shared = a.shared;
        let state = shared.state.clone();
        let rows = shared.rows.clone();
        let grid = shared.grid;
        let radius = t.radius.element.to_pixels(rem);
        let choice = match a.size {
            ElementSize::Sm => ChoiceSize::Sm,
            _ => ChoiceSize::Md,
        };
        // Select-all covers the enabled options the query shows.
        let enabled: Vec<&SharedString> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Option(o) if !o.disabled => Some(&o.value),
                _ => None,
            })
            .collect();
        let all = !enabled.is_empty() && enabled.iter().all(|v| shared.value.contains(v));
        let some = enabled.iter().any(|v| shared.value.contains(v));
        let select_all_state = if all {
            IndicatorState::Checked
        } else if some {
            IndicatorState::Indeterminate
        } else {
            IndicatorState::Unchecked
        };

        let row_el = |index: usize, window: &mut Window, cx: &mut App| -> AnyElement {
            let row = &rows[index];
            let key = row.key();
            let lit = a.highlight == Some(index);
            let on_action = lit && grid && a.cell == Cell::Action;
            let disabled = row.disabled();
            let (mark, selected) = match row {
                Row::SelectAll => (Some(select_all_state), all),
                Row::Create(_) => (None, false),
                Row::Option(o) => {
                    let on = shared.value.contains(&o.value);
                    (Some(IndicatorState::from(on)), on)
                }
            };
            let partial =
                matches!(row, Row::SelectAll) && select_all_state == IndicatorState::Indeterminate;
            let label: SharedString = match row {
                Row::SelectAll => a.select_all_label.clone(),
                _ => shared.row_label(row, cx),
            };
            let checkbox = mark.map(|state| {
                div()
                    .flex()
                    .flex_none()
                    .when(
                        self.indicator_position == SelectorIndicatorPosition::End,
                        |el| el.ml_auto(),
                    )
                    .child(
                        CheckboxIndicator::new(state)
                            .size(choice)
                            .disabled(disabled),
                    )
            });
            let body: AnyElement = match (row, &self.render_option) {
                (Row::Option(o), Some(render)) => render(o, window, cx),
                _ => div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(label.clone())
                    .into_any_element(),
            };
            let (start, end) = match self.indicator_position {
                SelectorIndicatorPosition::Start => (checkbox, None),
                SelectorIndicatorPosition::End => (None, checkbox),
            };
            let (pad_y, pad_x) = match (row, a.size) {
                (Row::SelectAll, ElementSize::Sm) => (t.spacing.s0_5, t.spacing.s1),
                (Row::SelectAll, _) => (t.spacing.s1, t.spacing.s2),
                (_, ElementSize::Sm) => (t.spacing.s1, t.spacing.s1),
                (_, ElementSize::Md) => (t.spacing.s1_5, t.spacing.s2),
                (_, ElementSize::Lg) => (t.spacing.s2, t.spacing.s2),
            };
            let selector = format!("{}-option-{key}", self.id);
            let mut el = div()
                .id(ElementId::Name(format!("{}-row-{index}", self.id).into()))
                .debug_selector(move || selector)
                .relative()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .w_full()
                .py(pad_y)
                .px(pad_x)
                .rounded(radius)
                .font_stack(&t.font.family.body)
                .text_size(t.text.label.size)
                .line_height(relative(t.text.label.leading))
                .font_weight(t.font.weight.medium)
                .text_color(t.color.text_primary)
                .role(if grid { Role::Row } else { Role::ListBoxOption })
                .aria_label(if partial {
                    t18.format(
                        "@astryx.multiSelector.selectAllPartiallySelected",
                        &[("label", a.select_all_label.as_ref().into())],
                    )
                } else {
                    label.clone()
                })
                .aria_selected(selected)
                .aria_disabled(disabled);
            if lit {
                el = el.bg(t.color.overlay_hover);
                if !on_action {
                    el = el.aria_active_descendant_of(a.owner);
                }
                let marker = format!("{}-highlighted-{key}", self.id);
                el = el.child(div().absolute().size_0().debug_selector(move || marker));
            }
            if disabled {
                el = el
                    .opacity(0.5)
                    .text_color(t.color.text_disabled)
                    .cursor_default();
            } else {
                el = el.cursor_pointer();
            }
            let mut states: Vec<(&'static str, &'static str)> = vec![("size", a.size.as_str())];
            if matches!(row, Row::SelectAll) {
                states.push(("select-all", "select-all"));
            }
            if lit {
                states.push(("highlighted", "highlighted"));
            }
            if selected {
                states.push(("selected", "selected"));
            }
            if disabled {
                states.push(("disabled", "disabled"));
            }
            theme.refine(el.style(), "multi-selector-option", &states, None);
            // Its bounds, for scrolling a key's highlight into view.
            {
                let state = state.clone();
                el = el.child(
                    gpui::canvas(
                        move |bounds, _, cx| {
                            state.update(cx, |s, _| {
                                if let Some(slot) = s.rows.get_mut(index) {
                                    *slot = Some(bounds);
                                }
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                );
            }
            let content = |el: Stateful<Div>| el.children(start).child(body).children(end);
            if !grid {
                let (press, hover) = (shared.clone(), shared.clone());
                el = content(el);
                if !disabled {
                    el = el
                        .on_mouse_move(move |_, window, cx| {
                            hover.hover(index, Cell::Option, window, cx)
                        })
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            press.activate(index, window, cx)
                        });
                }
                return el.into_any_element();
            }
            // FR10: two cells, the option's content and its action.
            let (press, hover) = (shared.clone(), shared.clone());
            let mut option_cell = div()
                .id(ElementId::Name(
                    format!("{}-row-{index}-option", self.id).into(),
                ))
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .flex_1()
                .min_w_0()
                .self_stretch()
                .role(Role::GridCell)
                .on_mouse_move(move |_, window, cx| hover.hover(index, Cell::Option, window, cx));
            if !disabled {
                option_cell = option_cell.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    press.activate(index, window, cx)
                });
            }
            let option_cell = content(option_cell);
            let action = match row {
                Row::Option(o) => o.action.clone(),
                _ => None,
            };
            let cell_selector = format!("{}-action-{key}", self.id);
            let mut action_cell = div()
                .id(ElementId::Name(
                    format!("{}-row-{index}-action", self.id).into(),
                ))
                .debug_selector(move || cell_selector)
                .relative()
                .flex()
                .flex_none()
                .items_center()
                .rounded(radius)
                .role(Role::GridCell);
            if on_action {
                action_cell = action_cell.aria_active_descendant_of(a.owner);
                let marker = format!("{}-highlighted-{key}-action", self.id);
                action_cell =
                    action_cell.child(div().absolute().size_0().debug_selector(move || marker));
                if a.keyboard {
                    action_cell = action_cell.child(focus_ring(t, radius, rem));
                }
            }
            if let Some(action) = action {
                let registry = ActionCell::default();
                let hover = shared.clone();
                action_cell = action_cell
                    .on_mouse_move(move |_, window, cx| {
                        hover.hover(index, Cell::Action, window, cx)
                    })
                    // No action control is a tab stop; Enter on the cell
                    // presses it.
                    .child(provide(
                        OutOfTabOrder,
                        provide(registry.clone(), action(window, cx)),
                    ));
                let state = state.clone();
                action_cell = action_cell.child(
                    gpui::canvas(
                        move |bounds, _, cx| {
                            state.update(cx, |s, _| {
                                if let Some(slot) = s.cells.get_mut(index) {
                                    *slot = Some((registry.clone(), bounds));
                                }
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                );
            }
            el.child(option_cell).child(action_cell).into_any_element()
        };

        // The rows, in their groups (upstream `renderItems`).
        let mut children: Vec<AnyElement> = (0..a.leading).map(|i| row_el(i, window, cx)).collect();
        let real = rows.len() - a.leading;
        let empty = real == 0 && !shared.loading && !a.has_create;
        if empty {
            let text = if a.query.is_empty() {
                self.empty_text
                    .clone()
                    .unwrap_or_else(|| t18.t("@astryx.multiSelector.empty"))
            } else {
                self.empty_search_text
                    .clone()
                    .unwrap_or_else(|| t18.t("@astryx.multiSelector.emptySearchResults"))
            };
            // The empty panel is how the search ended: say it once.
            if state.read(cx).announced_empty.as_deref() != Some(a.query) {
                let query = a.query.to_string();
                state.update(cx, |s, _| s.announced_empty = Some(query));
                announce(text.clone(), Politeness::Polite, window, cx);
            }
            let selector = format!("{}-empty", self.id);
            let mut e = div()
                .debug_selector(move || selector)
                .p(t.spacing.s3)
                .text_center()
                .text_color(t.color.text_secondary)
                .font_stack(&t.font.family.body)
                .text_size(t.text.label.size)
                .line_height(relative(t.text.label.leading))
                .child(text);
            theme.refine(e.style(), "multi-selector-empty-state", &[], None);
            children.push(e.into_any_element());
        } else {
            for (at, block) in a.blocks.iter().enumerate() {
                match block {
                    Block::Row(i) if *i < a.leading => {}
                    Block::Row(i) => children.push(row_el(*i, window, cx)),
                    Block::Divider => children.push(
                        div()
                            .my(t.spacing.s1)
                            .child(crate::layout::Divider::new())
                            .into_any_element(),
                    ),
                    Block::Section {
                        title,
                        rows: indices,
                    } => {
                        let mut section = div()
                            .id(ElementId::Name(format!("{}-section-{at}", self.id).into()))
                            .flex()
                            .flex_col()
                            .role(if grid { Role::RowGroup } else { Role::Group });
                        if let Some(title) = title {
                            let mut heading = div()
                                .py(t.spacing.s1)
                                .px(t.spacing.s2)
                                .font_stack(&t.font.family.body)
                                .text_size(t.text.supporting.size)
                                .line_height(relative(t.text.supporting.leading))
                                .text_color(t.color.text_secondary)
                                .child(title.clone());
                            theme.refine(
                                heading.style(),
                                "multi-selector-section-heading",
                                &[],
                                None,
                            );
                            section = section.aria_label(title.clone()).child(heading);
                        }
                        let items: Vec<AnyElement> =
                            indices.iter().map(|i| row_el(*i, window, cx)).collect();
                        children.push(section.children(items).into_any_element());
                    }
                }
            }
        }

        let listbox_focus = state.read(cx).listbox_focus.clone();
        let list_selector = format!("{}-listbox", self.id);
        let scroll = state.read(cx).scroll.clone();
        let reveal_probe = {
            let state = state.clone();
            gpui::canvas(
                |_, _, _| {},
                move |_, _, window, cx| {
                    if state.read(cx).reveal {
                        let state = state.clone();
                        window.defer(cx, move |window, cx| reveal(&state, window, cx));
                    }
                },
            )
            .absolute()
            .size_0()
        };
        let mut listbox = div()
            .id(ElementId::Name(format!("{}-listbox", self.id).into()))
            .debug_selector(move || list_selector)
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .min_h_0()
            .max_h(px(300.).min(cap.height))
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .p(t.spacing.s1)
            .role(if grid { Role::Grid } else { Role::ListBox })
            .aria_label(self.label.clone())
            .aria_multiselectable(true)
            .child(crate::gestures::scroller::scroll_begin(
                ElementId::Name(format!("{}-listbox", self.id).into()),
                Some(&scroll),
                (false, true),
            ))
            .children(children)
            .child(crate::gestures::scroller::scroll_end())
            .child(reveal_probe);
        if a.list_owns_keys {
            let s = shared.clone();
            listbox = listbox
                .track_focus(&listbox_focus)
                .on_key_down(move |event, window, cx| {
                    if s.key(event, window, cx) {
                        cx.stop_propagation();
                    }
                })
                .child(focus_probe(&state, listbox_focus.clone()));
        }
        let Some(editor) = shared.editor.clone() else {
            return listbox.into_any_element();
        };
        let search_focus = editor.read(cx).focus_handle(cx);
        let row = panel_search(
            PanelSearch {
                id: self.id.clone(),
                editor,
                has_query: !a.query.is_empty(),
                label: t18.t("@astryx.multiSelector.searchOptions"),
                placeholder: self
                    .search_placeholder
                    .clone()
                    .unwrap_or_else(|| t18.t("@astryx.multiSelector.searchPlaceholder")),
                required: self.required,
                pad_x: t.spacing.s1.to_pixels(rem),
                target: "multi-selector-search",
                on_query: {
                    let shared = shared.clone();
                    Rc::new(move |text, window, cx| shared.set_query(text, window, cx))
                },
                on_key: {
                    let shared = shared.clone();
                    Rc::new(move |event, window, cx| shared.search_key(event, window, cx))
                },
                on_close: {
                    let shared = shared.clone();
                    Rc::new(move |window, cx| shared.close(window, cx))
                },
                focus_probe: focus_probe(&state, search_focus),
            },
            window,
            cx,
        );
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_h_0()
            .max_h(cap.height)
            .child(row)
            .child(crate::layout::Divider::new())
            .child(listbox)
            .into_any_element()
    }
}

/// Moves focus to `target` right after the frame that first draws it, when
/// the list asked for focus (the search field, or a list that owns keys).
fn focus_probe(state: &Entity<MultiState>, target: FocusHandle) -> AnyElement {
    let state = state.clone();
    gpui::canvas(
        |_, _, _| {},
        move |_, _, window, cx| {
            if state.read(cx).pending_focus {
                let (state, target) = (state.clone(), target.clone());
                window.defer(cx, move |window, cx| {
                    state.update(cx, |s, _| s.pending_focus = false);
                    target.focus(window, cx);
                });
            }
        },
    )
    .absolute()
    .size_0()
    .into_any_element()
}
