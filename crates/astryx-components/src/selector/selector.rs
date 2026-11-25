//! Selector, upstream `Selector/Selector.tsx`: one value from a list, behind
//! a trigger that works as a form field or a toolbar control.
//!
//! - The trigger is a combobox (a button that opens a listbox). Clicking it,
//!   ArrowDown, Enter or Space open the list on the selected option (FR8);
//!   typing on the closed trigger picks the next match like a native
//!   select and announces it.
//! - Open, focus stays on the trigger (or the search field) and the
//!   highlighted option is its active descendant (fork patch R13). The
//!   arrows stop at the ends, Home, End, PageUp and PageDown jump, Enter
//!   picks, Escape and Tab close. The highlight follows its option when
//!   the options change (FR10).
//! - Without a placement or search, the list opens with the selected option
//!   over the trigger, kept inside the window (FR2).
//! - A press inside the list picks the option under its release (FR7): the
//!   highlight follows a held pointer, a mouse released outside closes the
//!   list and a finger leaves it open.
//! - `search` adds a field over the options that filters them by label and
//!   announces the match count (FR9: each query clears the highlight).
//! - The bottom-sheet presentations hold the list in a modal BottomSheet
//!   (FR4).

use std::rc::Rc;

use astryx_i18n::{Arg, current as i18n, use_direction};
use astryx_primitives::editing::{EditorStyle, FieldSettings, TextEditState, text_editor};
use astryx_primitives::typeahead::is_shortcut_chord;
use astryx_primitives::{
    ElementSize, EscapeBehavior, FocusNext, FocusPrevious, LayerAlign, LayerOptions, LayerPosition,
    Placement, Politeness, Typeahead, anchored_layer, announce, control_focus, is_focus_visible,
    position_layer, prefers_reduced_motion, provide, resolve_size,
};
use astryx_theme::{ThemeStyled, current, scope};
use gpui::{
    AnyElement, App, Bounds, ClickEvent, DispatchPhase, ElementId, Entity, FocusHandle, HasPopup,
    IntoElement, KeyDownEvent, Length, MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement,
    Pixels, RenderOnce, Role, ScrollHandle, SharedString, StyleRefinement, Styled, Task, Window,
    div, point, prelude::*, px, relative,
};
use web_time::Instant;

use super::combobox::{Combobox, KeyOutcome};
use super::option::{SelectorOption, SelectorRowLayout};
use super::options::{
    SelectorEntry, SelectorOptionData, Signature, filter_by_query, follow_highlight, matches_query,
    opening_index, selectable_options, signature,
};
use crate::button::{ButtonVariant, IconButton, element_height};
use crate::choice::action::{ActionFn, ChangeFn, Optimistic};
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::focus::{focus_ring, focus_ring_inset};
use crate::icon::{Icon, IconColor, IconDefaultSize, IconSize};
use crate::indicator::CheckIndicator;
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::input::group::{group_context, group_disabled, group_place, inside_member, join};
use crate::item::ItemLayout;
use crate::motion::transition;
use crate::overlay::PopoverPresentation;
use crate::status::{Spinner, SpinnerSize};

/// The most rows that fit the 300px list at the smallest default row.
const LIST_CAP: f32 = 300.;
/// The selected row sits one optical pixel below the trigger's label.
const OPTICAL_OFFSET: f32 = 1.;

type RenderFn = Rc<dyn Fn(&SelectorOptionData, &mut Window, &mut App) -> AnyElement>;
type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type FocusFn = Rc<dyn Fn(&mut Window, &mut App)>;
type KeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;
type Value = Option<SharedString>;

/// Upstream `variant`: the trigger's look.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectorVariant {
    /// The bordered field of a form.
    #[default]
    Input,
    /// A secondary Button's fill, for navs and toolbars beside them.
    Secondary,
    /// A ghost Button's look, for toolbars.
    Ghost,
}

impl SelectorVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            SelectorVariant::Input => "input",
            SelectorVariant::Secondary => "secondary",
            SelectorVariant::Ghost => "ghost",
        }
    }
}

/// Upstream `indicatorPosition`: the logical edge of an option row that
/// carries its selection mark. An empty mark takes no space (FR3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectorIndicatorPosition {
    Start,
    #[default]
    End,
}

/// A Selector's state between frames.
pub(crate) struct SelectorState {
    pub(crate) open: bool,
    /// The opening chose the bottom sheet.
    sheet: bool,
    /// The highlighted option, an index into the shown (filtered) options.
    pub(crate) highlight: Option<usize>,
    /// The option the pointer last entered; it draws no ring.
    hovered: Option<usize>,
    /// The highlight last moved by keyboard input (AR5: it draws the ring).
    keyboard: bool,
    pub(crate) query: String,
    /// The shown options' signature at the last render (FR10).
    list: Signature,
    typeahead: Typeahead,
    /// A press that began inside the list is held (FR7).
    press: bool,
    /// Each shown option's bounds, as of the last frame.
    rows: Vec<Option<Bounds<Pixels>>>,
    listbox: Option<Bounds<Pixels>>,
    /// FR2: the selected (or first) option's center below the list's top.
    overlay: Option<Pixels>,
    shown_at: Option<Instant>,
    side: Placement,
    scroll: ScrollHandle,
    /// Focus enters the search field or the sheet's list once drawn.
    pending_focus: bool,
    /// Scroll the highlight into view on the next frame (a key moved it).
    reveal: bool,
    /// The query whose empty list was announced.
    announced_empty: Option<String>,
    /// The trigger's ring is hidden: a pointer close returned focus to it.
    ring_suppressed: bool,
    default_opened: bool,
    /// The sheet's list, which takes focus there.
    listbox_focus: FocusHandle,
    /// Upstream `onFocus`, refreshed every render, and its subscription.
    on_focus: Option<FocusFn>,
    focus_subscription: Option<gpui::Subscription>,
}

impl SelectorState {
    fn new(cx: &mut App) -> Self {
        SelectorState {
            open: false,
            sheet: false,
            highlight: None,
            hovered: None,
            keyboard: false,
            query: String::new(),
            list: Vec::new(),
            typeahead: Typeahead::new(),
            press: false,
            rows: Vec::new(),
            listbox: None,
            overlay: None,
            shown_at: None,
            side: Placement::Below,
            scroll: ScrollHandle::new(),
            pending_focus: false,
            reveal: false,
            announced_empty: None,
            ring_suppressed: false,
            default_opened: false,
            listbox_focus: cx.focus_handle().tab_stop(true),
            on_focus: None,
            focus_subscription: None,
        }
    }
}

/// What a Selector's handlers share in one frame.
struct Shared {
    state: Entity<SelectorState>,
    trigger: FocusHandle,
    editor: Option<Entity<TextEditState>>,
    /// Every selectable option.
    items: Rc<Vec<SelectorOptionData>>,
    /// The shown options: those matching the query.
    shown: Rc<Vec<SelectorOptionData>>,
    /// The value shown: an action's while it runs.
    value: Value,
    change: ChangeFn<Value>,
    search: bool,
    clearable: bool,
    /// Disabled or read-only: no value change.
    inert: bool,
    /// The presentation resolves to the sheet now.
    sheet: bool,
    loading: bool,
}

impl Shared {
    fn editor_focus(&self, cx: &App) -> Option<FocusHandle> {
        self.editor
            .as_ref()
            .map(|e| gpui::Focusable::focus_handle(e.read(cx), cx))
    }

    fn selected_index(&self) -> Option<usize> {
        let value = self.value.as_ref()?;
        self.items.iter().position(|i| &i.value == value)
    }

    fn open(&self, highlight: Option<usize>, window: &mut Window, cx: &mut App) {
        if self.inert || self.state.read(cx).open {
            return;
        }
        let now = cx.background_executor().now();
        let keyboard = window.last_input_was_keyboard();
        let (sheet, search) = (self.sheet, self.search);
        self.state.update(cx, |s, _| {
            s.open = true;
            s.sheet = sheet;
            s.highlight = highlight;
            s.hovered = None;
            s.keyboard = keyboard;
            s.shown_at = Some(now);
            s.overlay = None;
            s.listbox = None;
            s.pending_focus = search || sheet;
            s.reveal = highlight.is_some();
            s.ring_suppressed = false;
            s.press = false;
        });
        window.refresh();
    }

    /// Upstream `closeAndReset` and the presentation's hide: the query and
    /// highlight go, and focus that would be lost in the list returns to the
    /// trigger (AR2).
    fn close(&self, window: &mut Window, cx: &mut App) {
        if !self.state.read(cx).open {
            return;
        }
        self.state.update(cx, |s, _| {
            s.open = false;
            s.highlight = None;
            s.hovered = None;
            s.press = false;
            s.query.clear();
            s.typeahead.reset();
            s.overlay = None;
            s.listbox = None;
            s.shown_at = None;
            s.pending_focus = false;
            s.reveal = false;
            s.announced_empty = None;
        });
        if let Some(editor) = &self.editor {
            editor.update(cx, |e, cx| e.set_text("", cx));
        }
        announce("", Politeness::Polite, window, cx);
        let focused = window.focused(cx);
        let listbox = self.state.read(cx).listbox_focus.clone();
        let in_list = match &focused {
            None => true,
            Some(f) => *f == listbox || self.editor_focus(cx).as_ref() == Some(f),
        };
        if in_list || focused.as_ref() == Some(&self.trigger) {
            self.trigger.focus(window, cx);
            let pointer = !window.last_input_was_keyboard();
            self.state.update(cx, |s, _| s.ring_suppressed = pointer);
        }
        window.refresh();
    }

    fn commit(&self, value: Value, window: &mut Window, cx: &mut App) {
        if self.inert {
            return;
        }
        (self.change)(value, window, cx);
    }

    /// Upstream `selectItem` (FR1): report the value and close.
    fn select(&self, index: usize, window: &mut Window, cx: &mut App) {
        let Some(item) = self.shown.get(index) else {
            return;
        };
        if item.disabled || self.inert {
            return;
        }
        self.commit(Some(item.value.clone()), window, cx);
        self.close(window, cx);
    }

    fn clear(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| s.typeahead.reset());
        (self.change)(None, window, cx);
    }

    fn set_highlight(&self, highlight: Option<usize>, window: &mut Window, cx: &mut App) {
        let keyboard = window.last_input_was_keyboard();
        self.state.update(cx, |s, _| {
            s.highlight = highlight;
            s.keyboard = keyboard;
            s.reveal = highlight.is_some();
        });
        window.refresh();
    }

    /// The pointer entered an option: it highlights an enabled one without
    /// scrolling, and draws no ring.
    fn hover(&self, index: usize, window: &mut Window, cx: &mut App) {
        let enabled = self.shown.get(index).is_some_and(|i| !i.disabled);
        let s = self.state.read(cx);
        if !enabled || (s.highlight == Some(index) && !s.keyboard) {
            return;
        }
        let active = s.highlight == Some(index);
        self.state.update(cx, |s, _| {
            if !active {
                s.hovered = Some(index);
            }
            s.highlight = Some(index);
            s.keyboard = false;
            s.reveal = false;
        });
        window.refresh();
    }

    /// The query changed: a new list, so no highlight (FR9), and its count
    /// announced once.
    fn set_query(&self, query: String, window: &mut Window, cx: &mut App) {
        let count = filter_by_query(&self.items, &query).len();
        let empty = query.is_empty();
        self.state.update(cx, |s, _| {
            s.query = query;
            s.highlight = None;
            s.hovered = None;
        });
        if empty || self.loading {
            announce("", Politeness::Polite, window, cx);
        } else if count > 0 {
            let text = i18n(cx).format(
                "@astryx.selector.resultCount",
                &[("count", Arg::Num(count as f64))],
            );
            announce(text, Politeness::Polite, window, cx);
        }
        window.refresh();
    }

    /// The list's keys, upstream `useCombobox`'s `onKeyDown`. Returns whether
    /// the key was handled.
    fn combobox_key(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App) -> bool {
        let disabled: Vec<bool> = self.shown.iter().map(|i| i.disabled).collect();
        let (open, highlight) = {
            let s = self.state.read(cx);
            (s.open, s.highlight)
        };
        let model = Combobox {
            disabled: &disabled,
            opening: (
                opening_index(&self.shown, self.value.as_ref(), false),
                opening_index(&self.shown, self.value.as_ref(), true),
            ),
            open,
            search: self.search,
            highlight,
            clearable: self.clearable,
        };
        let keystroke = &event.keystroke;
        let text = keystroke
            .key_char
            .clone()
            .or_else(|| (keystroke.key.chars().count() == 1).then(|| keystroke.key.clone()));
        let outcome = model.key(
            keystroke.key.as_str(),
            text.as_deref(),
            is_shortcut_chord(keystroke),
        );
        match outcome {
            KeyOutcome::Ignored => false,
            KeyOutcome::Handled => true,
            KeyOutcome::Open(highlight) => {
                self.open(highlight, window, cx);
                true
            }
            KeyOutcome::Highlight(highlight) => {
                self.set_highlight(highlight, window, cx);
                true
            }
            KeyOutcome::Select(index) => {
                self.select(index, window, cx);
                true
            }
            KeyOutcome::Close => {
                self.close(window, cx);
                true
            }
            KeyOutcome::CloseAndPass => {
                self.close(window, cx);
                false
            }
            KeyOutcome::Clear => {
                self.clear(window, cx);
                true
            }
            KeyOutcome::Seed(text) => {
                // Typed before focus reaches the search field: it joins the
                // query, opening the list first.
                self.open(None, window, cx);
                let query = format!("{}{text}", self.state.read(cx).query);
                if let Some(editor) = &self.editor {
                    editor.update(cx, |e, cx| e.set_text(&query, cx));
                }
                self.set_query(query, window, cx);
                true
            }
        }
    }

    /// The trigger's key-down, upstream `handleTriggerKeyDown`: typeahead
    /// first (without search), then the list's keys.
    fn trigger_key(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App) -> bool {
        if self.inert {
            return false;
        }
        self.state.update(cx, |s, _| {
            s.ring_suppressed = false;
            s.hovered = None;
        });
        if !self.search {
            let open = self.state.read(cx).open;
            let current = if open {
                self.state.read(cx).highlight
            } else {
                self.selected_index()
            };
            let items = self.items.clone();
            let found = self.state.update(cx, |s, _| {
                s.typeahead.key(
                    &event.keystroke,
                    items.len(),
                    |i| items.get(i).map(|o| o.label.as_ref()),
                    current,
                    |i| items.get(i).is_some_and(|o| o.disabled),
                )
            });
            if let Some(found) = found {
                if let Some(index) = found {
                    if open {
                        self.set_highlight(Some(index), window, cx);
                    } else if self.value.as_ref() != Some(&items[index].value) {
                        // Like a native select: the value changes in place,
                        // so say what it became.
                        self.commit(Some(items[index].value.clone()), window, cx);
                        announce(items[index].label.clone(), Politeness::Polite, window, cx);
                    }
                }
                return true;
            }
        }
        self.combobox_key(event, window, cx)
    }
}

/// One value from a list, upstream `Selector`.
///
/// ```ignore
/// Selector::new("fruit", "Fruit", ["Apple", "Banana", "Orange"])
///     .value(fruit.clone())
///     .placeholder("Select a fruit…")
///     .on_change(|value, _, cx| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct Selector {
    id: ElementId,
    label: SharedString,
    entries: Vec<SelectorEntry>,
    value: Value,
    on_change: Option<ChangeFn<Value>>,
    change_action: Option<ActionFn<Value>>,
    clear: bool,
    search: bool,
    search_placeholder: Option<SharedString>,
    empty_text: Option<SharedString>,
    empty_search_text: Option<SharedString>,
    placeholder: Option<SharedString>,
    size: Option<ElementSize>,
    variant: SelectorVariant,
    disabled: bool,
    read_only: bool,
    disabled_message: Option<SharedString>,
    label_hidden: bool,
    description: Option<SharedString>,
    label_tooltip: Option<SharedString>,
    optional: bool,
    required: bool,
    status: Option<InputStatus>,
    status_placement: Option<StatusPlacement>,
    render_option: Option<RenderFn>,
    render_value: Option<RenderFn>,
    indicator_position: SelectorIndicatorPosition,
    presentation: PopoverPresentation,
    placement: Option<Placement>,
    default_open: bool,
    width: Option<Length>,
    start_icon: Option<AnyElement>,
    loading: bool,
    on_click: Option<ClickFn>,
    on_focus: Option<FocusFn>,
    on_key_down: Option<KeyFn>,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl Selector {
    /// Upstream `label` (always the field's accessible name) and `options`:
    /// strings, [`SelectorOptionData`], dividers and sections.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        options: impl IntoIterator<Item = impl Into<SelectorEntry>>,
    ) -> Self {
        Selector {
            id: id.into(),
            label: label.into(),
            entries: options.into_iter().map(Into::into).collect(),
            value: None,
            on_change: None,
            change_action: None,
            clear: false,
            search: false,
            search_placeholder: None,
            empty_text: None,
            empty_search_text: None,
            placeholder: None,
            size: None,
            variant: SelectorVariant::Input,
            disabled: false,
            read_only: false,
            disabled_message: None,
            label_hidden: false,
            description: None,
            label_tooltip: None,
            optional: false,
            required: false,
            status: None,
            status_placement: None,
            render_option: None,
            render_value: None,
            indicator_position: SelectorIndicatorPosition::End,
            presentation: PopoverPresentation::Popover,
            placement: None,
            default_open: false,
            width: None,
            start_icon: None,
            loading: false,
            on_click: None,
            on_focus: None,
            on_key_down: None,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `hasClear`: a clear (×) button while there is a value;
        /// `on_change` then receives `None` on a clear.
        !clear,
        /// Upstream `hasSearch`: a field over the options that filters them.
        !search,
        /// Upstream `searchPlaceholder` ("Search...").
        ?search_placeholder: SharedString,
        /// Upstream `emptyText` ("No options"): shown, and announced, when
        /// there are no options. Not while loading.
        ?empty_text: SharedString,
        /// Upstream `emptySearchText` ("No results found").
        ?empty_search_text: SharedString,
        /// Upstream `placeholder` ("Select...").
        ?placeholder: SharedString,
        /// Upstream `size`: explicit, else the nearest size provider, else md.
        ?size: ElementSize,
        /// Upstream `variant`.
        variant: SelectorVariant,
        /// Upstream `isDisabled`: dimmed and inert, but still a tab stop.
        !disabled,
        /// Upstream `isReadOnly`: the value stays visible and focusable, with
        /// no list, clear button or chevron. `disabled` wins over it.
        !read_only,
        /// Upstream `disabledMessage`: why it is disabled, on hover and focus.
        ?disabled_message: SharedString,
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`: helper text under the label.
        ?description: SharedString,
        /// Upstream `labelTooltip`: an InfoTip after the label.
        ?label_tooltip: SharedString,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `statusVariant`: attached for the input variant,
        /// detached for the others.
        ?status_placement: StatusPlacement,
        /// Upstream `indicatorPosition`.
        indicator_position: SelectorIndicatorPosition,
        /// Upstream `presentation`.
        presentation: PopoverPresentation,
        /// Upstream `placement`: omit it to put the selected option over the
        /// trigger.
        ?placement: Placement,
        /// Upstream `isDefaultOpen`: open on the first render.
        !default_open,
        /// Upstream `width`: the whole field.
        ?width: Length,
        /// Upstream `isLoading`: a spinner, the trigger busy, no empty text.
        !loading,
    }

    /// Upstream `value`: the selected value, or `None` for the placeholder.
    pub fn value(mut self, value: impl Into<Option<SharedString>>) -> Self {
        self.value = value.into();
        self
    }

    /// Upstream `onChange`: the picked value (`None` after a clear).
    pub fn on_change(mut self, f: impl Fn(Value, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`. The trigger
    /// shows the new value and is busy until the task settles.
    pub fn change_action(
        mut self,
        f: impl Fn(Value, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `renderOption`: each option's row content.
    pub fn render_option(
        mut self,
        f: impl Fn(&SelectorOptionData, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_option = Some(Rc::new(f));
        self
    }

    /// Upstream `renderValue`: the selected option inside the closed trigger.
    pub fn render_value(
        mut self,
        f: impl Fn(&SelectorOptionData, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_value = Some(Rc::new(f));
        self
    }

    /// Upstream `startIcon`: before the value, over the selected option's
    /// own icon (ORD2).
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `onClick`: the trigger was clicked, before it toggles.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `onFocus`: the trigger took focus.
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

    /// Upstream `controlXstyle`: the trigger's box, after its own and the
    /// theme's styles (`Styled` styles the whole field).
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl Styled for Selector {
    /// Upstream `containerXstyle`: the field (label, control and status).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The selector's keyed state.
pub(crate) fn selector_state(
    id: &ElementId,
    window: &mut Window,
    cx: &mut App,
) -> Entity<SelectorState> {
    window.use_keyed_state(
        ElementId::Name(format!("{id}-selector").into()),
        cx,
        |_, cx| SelectorState::new(cx),
    )
}

/// The trigger's one-line box (upstream `sizeStyles`): the line, capped to
/// what the size token holds, and the padding that centers it.
fn trigger_line(
    size: ElementSize,
    t: &astryx_theme::Tokens,
    rem: Pixels,
    capped: bool,
) -> (Pixels, Pixels) {
    let height = element_height(size, t).to_pixels(rem);
    let border = t.border.width.to_pixels(rem);
    let font = t.text.label.size.to_pixels(rem);
    let s5 = t.spacing.s5.to_pixels(rem);
    let line = if capped {
        s5.min((height - border * 2.).max(font * 1.5))
            .max(px(20.))
            .max(rem)
    } else {
        s5.max(px(20.)).max(rem)
    };
    let pad = ((height - line - border * 2.) / 2.).max(px(0.));
    (line, pad)
}

/// Where the list goes so its selected option sits over the trigger
/// (FR2): centered on the trigger's center, one optical pixel down, kept
/// inside the window above its gutter.
fn overlay_position(
    center: Pixels,
) -> impl Fn(Bounds<Pixels>, gpui::Size<Pixels>, Bounds<Pixels>, &LayerOptions) -> LayerPosition {
    move |anchor, size, viewport, options| {
        let normal = position_layer(anchor, size, viewport, options);
        let anchor_center = anchor.origin.y + anchor.size.height / 2.;
        let desired = anchor_center - center - px(OPTICAL_OFFSET);
        let max_top = (viewport.bottom() - size.height - options.margin).max(viewport.top());
        let top = desired.max(viewport.top()).min(max_top);
        LayerPosition {
            origin: point(normal.origin.x, top),
            side: Placement::Below,
        }
    }
}

/// After a frame: scroll a key's highlight into view, then measure where
/// the selected option sits in the list (FR2).
fn after_frame(
    state: &Entity<SelectorState>,
    target: usize,
    measure: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let changed = state.update(cx, |s, _| {
        let mut delta = px(0.);
        let mut changed = false;
        if s.reveal {
            s.reveal = false;
            if let Some(row) = s.highlight.and_then(|h| s.rows.get(h).copied().flatten()) {
                let before = s.scroll.offset().y;
                let viewport = s.scroll.bounds();
                let mut offset = s.scroll.offset();
                if row.top() < viewport.top() {
                    offset.y += viewport.top() - row.top();
                } else if row.bottom() > viewport.bottom() {
                    offset.y -= row.bottom() - viewport.bottom();
                }
                s.scroll.set_offset(offset);
                delta = s.scroll.offset().y - before;
                changed = delta != px(0.);
            }
        }
        if measure
            && s.overlay.is_none()
            && let (Some(row), Some(list)) = (s.rows.get(target).copied().flatten(), s.listbox)
        {
            s.overlay = Some(row.top() + delta - list.top() + row.size.height / 2.);
            changed = true;
        }
        changed
    });
    if changed {
        window.refresh();
    }
}

impl RenderOnce for Selector {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // An enclosing InputGroup's disabled state, never re-enabling.
        self.disabled |= group_disabled(cx);
        let place = group_place(cx);
        let group = place.and_then(|_| group_context(cx));
        let dir = use_direction(cx);
        let rtl = dir.is_rtl();
        let theme = current(window, cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let t18 = i18n(cx);
        let rem = window.rem_size();
        let read_only = self.read_only && !self.disabled;
        let inert = self.disabled || read_only;
        let input = self.variant == SelectorVariant::Input;
        // Secondary and ghost selectors float their status message.
        let status_placement = match self.status_placement.unwrap_or(StatusPlacement::Attached) {
            StatusPlacement::Attached if !input => StatusPlacement::Detached,
            placement => placement,
        };
        let placeholder = self
            .placeholder
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.selector.placeholder"));
        let search_placeholder = self
            .search_placeholder
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.selector.searchPlaceholder"));
        let empty_text = self
            .empty_text
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.selector.empty"));
        let empty_search_text = self
            .empty_search_text
            .clone()
            .unwrap_or_else(|| t18.t("@astryx.selector.emptySearchResults"));
        let required = crate::field::resolve(self.required, self.optional, cx);

        // The change pipeline: on_change, then the action with the new value
        // shown and the trigger busy until it settles.
        let optimistic = Optimistic::<Value>::new(&self.id, window, cx);
        let value = optimistic.shown(self.value.clone(), cx);
        let busy = self.loading || value != self.value;
        optimistic.announce_busy(busy, t18.t("@astryx.spinner.loading"), window, cx);
        let change: ChangeFn<Value> =
            Rc::new(optimistic.change(self.on_change.clone(), self.change_action.clone()));

        let state = selector_state(&self.id, window, cx);
        let trigger_id = ElementId::Name(format!("{}-trigger", self.id).into());
        let trigger = control_focus(&trigger_id, window, cx);
        let editor = self.search.then(|| {
            window.use_keyed_state(
                ElementId::Name(format!("{}-search", self.id).into()),
                cx,
                |window, cx| TextEditState::reporting("", false, window, cx),
            )
        });

        let items = Rc::new(selectable_options(&self.entries));
        let query = state.read(cx).query.clone();
        let searching = self.search && !query.is_empty();
        let shown = Rc::new(if self.search {
            filter_by_query(&items, &query)
        } else {
            items.to_vec()
        });
        // FR10: the highlight follows its option when the options change.
        let list = signature(&shown);
        if state.read(cx).list != list {
            state.update(cx, |s, _| {
                if let Some(at) = s.highlight {
                    let followed = follow_highlight(&s.list, &list, at);
                    if let Some(hovered) = s.hovered {
                        s.hovered = if hovered == at {
                            followed
                        } else {
                            let value = s.list.get(hovered).map(|(v, _)| v.clone());
                            value
                                .and_then(|v| list.iter().position(|(x, _)| *x == v))
                                .filter(|moved| Some(*moved) != followed)
                        };
                    }
                    s.highlight = followed;
                }
                s.list = list.clone();
            });
        }
        state.update(cx, |s, _| s.rows.resize(shown.len(), None));

        let sheet_now = crate::overlay::presents_as_sheet(self.presentation, window);
        let shared = Rc::new(Shared {
            state: state.clone(),
            trigger: trigger.clone(),
            editor: editor.clone(),
            items: items.clone(),
            shown: shown.clone(),
            value: value.clone(),
            change,
            search: self.search,
            clearable: self.clear && value.is_some(),
            inert,
            sheet: sheet_now,
            loading: self.loading,
        });

        if self.default_open && !state.read(cx).default_opened {
            state.update(cx, |s, _| s.default_opened = true);
            shared.open(None, window, cx);
        }
        // FR6: read-only closes an open list.
        if read_only && state.read(cx).open {
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

        let (open, highlight, keyboard, hovered, opened_sheet, overlay, ring_suppressed) = {
            let s = state.read(cx);
            (
                s.open,
                s.highlight.filter(|h| *h < shown.len()),
                s.keyboard,
                s.hovered,
                s.sheet,
                s.overlay,
                s.ring_suppressed,
            )
        };
        let sheet = if open { opened_sheet } else { sheet_now };
        let selected = value
            .as_ref()
            .and_then(|v| items.iter().find(|i| &i.value == v))
            .cloned();
        let status = self.status.as_ref().map(|s| s.status);

        // ---- The trigger -------------------------------------------------
        let focused = trigger.is_focused(window);
        let focus_visible = is_focus_visible(&trigger, window);
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
        let part_override = theme.for_part("selector", &props);
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let height = element_height(size, t).to_pixels(rem);
        let border = t.border.width.to_pixels(rem);
        let (line, pad) = trigger_line(size, t, rem, input && place.is_none());
        let control_id = ElementId::Name(format!("{}-control", self.id).into());
        let mut control = if input {
            let chrome = Chrome {
                id: control_id,
                target: "selector",
                variant: Some("input"),
                size,
                status,
                disabled: self.disabled,
                read_only,
                focused: focused && !ring_suppressed,
                focus_visible: focus_visible && !ring_suppressed,
                row: false,
                place,
                dir,
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
                let pressed = astryx_theme::color::composite(fill, t.color.overlay_pressed);
                el = el
                    .hover(move |s| s.bg(hover))
                    .active(move |s| s.bg(pressed));
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
                el = join(el, place, t.radius.element.to_pixels(rem), border, dir).flex_1();
                el.style().size.height = Some(relative(1.).into());
            }
            part.refine(el.style(), "selector", &props, None);
            el
        };
        control = control
            .track_focus(&trigger)
            .items_center()
            .justify_between()
            .gap(t.spacing.s2)
            .px(t.spacing.s3)
            .font_stack(&t.font.family.body)
            .text_size(t.text.label.size)
            .line_height(line)
            .text_color(if selected.is_some() {
                t.color.text_primary
            } else {
                t.color.text_secondary
            });
        if place.is_some() {
            control = control
                .py(px(0.))
                .min_h(px(0.))
                .line_height(t.spacing.s5.to_pixels(rem).max(px(20.)).max(rem));
        } else {
            control = control.py(pad).min_h(height);
        }
        if input && place.is_none() {
            control = control.w_full();
        }
        if inert {
            control = control.cursor_default();
        } else {
            control = control.cursor_pointer();
        }
        control.style().refine(&self.control_style);
        if !input && focus_visible && !ring_suppressed {
            control = control.child(focus_ring(t, t.radius.element.to_pixels(rem), rem));
        }

        // The value: the caller's rendering, or the option's icon and label,
        // or the placeholder.
        if let Some(icon) = self.start_icon.take() {
            control = control.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .text_color(t.color.icon_secondary)
                    .child(provide(IconDefaultSize(IconSize::Sm), icon)),
            );
        } else if let Some(icon) = selected.as_ref().and_then(|s| s.icon.clone()) {
            control = control.child(
                div().flex().flex_none().items_center().child(
                    Icon::new(icon)
                        .size(IconSize::Sm)
                        .color(IconColor::Secondary),
                ),
            );
        }
        let value_text: SharedString = selected
            .as_ref()
            .map(|s| s.label.clone())
            .unwrap_or_else(|| placeholder.clone());
        control = match (&selected, &self.render_value) {
            (Some(option), Some(render)) => {
                let content = render(option, window, cx);
                let mut wrap = div().flex_1().min_w_0().overflow_hidden().child(content);
                if place.is_some() {
                    wrap = wrap.max_h(relative(1.));
                    control.child(provide(SelectorRowLayout(ItemLayout::Inline), wrap))
                } else {
                    control.child(wrap)
                }
            }
            _ => control.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(value_text.clone()),
            ),
        };
        if busy {
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
        if self.clear && value.is_some() && !inert {
            let label = t18.format(
                "@astryx.selector.clearLabel",
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
        if let Some(status) = status.filter(|_| status_placement != StatusPlacement::Detached) {
            let message = self.status.as_ref().and_then(|s| s.message.clone());
            control = control.child(status_indicator(
                &self.id,
                status,
                status_placement,
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
                "selector-indicator-icon",
                &[("state", if open { "expanded" } else { "collapsed" })],
                None,
            );
            control = control.child(chevron);
        }

        // Its name, value and states (AR1, AR4).
        let editable_search = self.search && !read_only;
        let name: SharedString = match &group {
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
        if let Some(group) = &group {
            described = group.described.iter().cloned().chain(described).collect();
        }
        if read_only {
            described.push(t18.t("@astryx.input.readOnly"));
        }
        // A search trigger is a button: its name carries the value, and its
        // description the required state its search field reports.
        // The name here is the bare label, never its marker.
        if editable_search && required.required {
            described.push(t18.t("@astryx.field.required"));
        }
        control = if editable_search {
            control
                .role(Role::Button)
                .aria_label(SharedString::from(format!("{name} {value_text}")))
        } else {
            control
                .role(Role::ComboBox)
                .aria_label(name)
                .aria_value(value_text.clone())
        };
        control = control
            .aria_expanded(open && !read_only)
            .aria_read_only(read_only)
            .aria_disabled(self.disabled)
            .aria_required(required.required && !editable_search)
            .aria_invalid(status == Some(StatusType::Error))
            .aria_busy(busy);
        if !read_only {
            control = control.aria_has_popup(if sheet {
                HasPopup::Dialog
            } else {
                HasPopup::Listbox
            });
        }
        if !described.is_empty() {
            control = control.aria_description(SharedString::from(described.join(" ")));
        }

        // Its handlers.
        {
            let trigger = trigger.clone();
            control = control.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                trigger.focus(window, cx);
            });
        }
        {
            let (shared, on_click, disabled) =
                (shared.clone(), self.on_click.clone(), self.disabled);
            control = control.on_click(move |event, window, cx| {
                // Enter and Space act on key-down.
                if event.is_keyboard() || disabled {
                    return;
                }
                if let Some(f) = &on_click {
                    f(event, window, cx);
                }
                if shared.inert {
                    return;
                }
                if shared.state.read(cx).open {
                    shared.close(window, cx);
                } else {
                    let highlight = (!shared.search)
                        .then(|| opening_index(&shared.shown, shared.value.as_ref(), false))
                        .flatten();
                    shared.open(highlight, window, cx);
                }
            });
        }
        {
            let (shared, on_key, disabled) =
                (shared.clone(), self.on_key_down.clone(), self.disabled);
            control = control.on_key_down(move |event, window, cx| {
                if shared.trigger_key(event, window, cx) {
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

        let control = match part_override {
            Some(part) => scope(part, control).into_any_element(),
            None => control.into_any_element(),
        };

        // ---- The list ----------------------------------------------------
        let surface = (open && !read_only).then(|| {
            let owner = if self.search {
                shared.editor_focus(cx).unwrap_or_else(|| trigger.clone())
            } else if sheet {
                state.read(cx).listbox_focus.clone()
            } else {
                trigger.clone()
            };
            let ctx = ListCtx {
                owner,
                highlight,
                keyboard,
                hovered,
                searching,
                query: query.clone(),
                size,
                sheet,
                empty: if searching {
                    empty_search_text.clone()
                } else {
                    empty_text.clone()
                },
                search_placeholder: search_placeholder.clone(),
                required: required.required,
            };
            let rows = self.rows(&shared, &ctx, window, cx);
            // FR5: the empty state is announced from what it says, once.
            if shown.is_empty() && !self.loading {
                let text = if searching {
                    empty_search_text.clone()
                } else {
                    empty_text.clone()
                };
                if state.read(cx).announced_empty.as_deref() != Some(query.as_str()) {
                    state.update(cx, |s, _| s.announced_empty = Some(query.clone()));
                    announce(text, Politeness::Polite, window, cx);
                }
            } else if state.read(cx).announced_empty.is_some() {
                state.update(cx, |s, _| s.announced_empty = None);
            }
            self.panel(&shared, &ctx, rows, window, cx)
        });

        let overlay_mode = self.placement.is_none() && !self.search;
        let mut layer = anchored_layer(
            ElementId::Name(format!("{}-layer", self.id).into()),
            control,
        )
        .light_dismiss(true)
        .escape(EscapeBehavior::Close)
        .match_anchor_width(true);
        let mut sheet_el = None;
        match surface {
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
                theme.refine(surface.style(), "selector-popup", &[], None);
                let mut surface =
                    crate::overlay::entrance(surface, since, side, rtl, t, window, cx);
                // Hidden until the selected option is measured (FR2).
                if overlay_mode && overlay.is_none() {
                    surface = surface.opacity(0.);
                }
                let options = LayerOptions {
                    placement: self.placement.unwrap_or(Placement::Below),
                    align: LayerAlign::Start,
                    offset: if overlay_mode {
                        px(0.)
                    } else {
                        crate::overlay::clearance(t, window)
                    },
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
                if overlay_mode && let Some(center) = overlay {
                    layer = layer.position_with(overlay_position(center));
                }
            }
            panel => {
                if opened_sheet || sheet {
                    let shared = shared.clone();
                    let mut bottom = crate::overlay::BottomSheet::new(
                        ElementId::Name(format!("{}-sheet", self.id).into()),
                        open && sheet,
                        self.label.clone(),
                    )
                    .on_open_change(move |open, window, cx| {
                        if !open {
                            shared.close(window, cx);
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

        // A disabled selector's reason, on hover and focus.
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
        if let Some(tooltip) = self.label_tooltip {
            field = field.label_tooltip(tooltip);
        }
        if let Some(width) = self.width {
            field = field.width(width);
        }
        field.style().refine(&self.style);
        field.child(content).into_any_element()
    }
}

/// What the list's rows and panel read in one frame.
struct ListCtx {
    /// The element whose active descendant the highlighted option is.
    owner: FocusHandle,
    highlight: Option<usize>,
    keyboard: bool,
    hovered: Option<usize>,
    searching: bool,
    query: String,
    size: ElementSize,
    sheet: bool,
    empty: SharedString,
    search_placeholder: SharedString,
    required: bool,
}

impl Selector {
    /// The rows in list order: options, dividers and sections (a search
    /// hides dividers and sections with no match), or the empty state.
    fn rows(
        &self,
        shared: &Rc<Shared>,
        ctx: &ListCtx,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        let theme = current(window, cx);
        let t = theme.tokens();
        if shared.shown.is_empty() {
            // FR5: nothing to claim while the options are loading.
            if self.loading {
                return Vec::new();
            }
            let empty_selector = format!("{}-empty", self.id);
            let mut empty = div()
                .debug_selector(move || empty_selector)
                .p(t.spacing.s3)
                .text_center()
                .text_color(t.color.text_secondary)
                .text_size(t.text.label.size)
                .line_height(relative(t.text.label.leading))
                .child(ctx.empty.clone());
            theme.refine(empty.style(), "selector-empty-state", &[], None);
            return vec![empty.into_any_element()];
        }
        let mut flat = 0usize;
        let mut out = Vec::new();
        for (at, entry) in self.entries.iter().enumerate() {
            match entry {
                SelectorEntry::Divider => {
                    if !ctx.searching {
                        out.push(
                            div()
                                .my(t.spacing.s1)
                                .child(crate::layout::Divider::new())
                                .into_any_element(),
                        );
                    }
                }
                SelectorEntry::Section { title, options } => {
                    let mut rows = Vec::new();
                    for option in options {
                        if ctx.searching && !matches_query(option, &ctx.query) {
                            continue;
                        }
                        rows.push(self.row(shared, ctx, flat, window, cx));
                        flat += 1;
                    }
                    if rows.is_empty() {
                        continue;
                    }
                    let heading = title.clone().map(|title| {
                        let mut heading = div()
                            .py(t.spacing.s1)
                            .px(t.spacing.s2)
                            .text_size(t.text.supporting.size)
                            .line_height(relative(t.text.supporting.leading))
                            .text_color(t.color.text_secondary)
                            .child(title);
                        theme.refine(heading.style(), "selector-section-heading", &[], None);
                        heading
                    });
                    let mut group = div()
                        .id(ElementId::Name(format!("{}-section-{at}", self.id).into()))
                        .flex()
                        .flex_col()
                        .role(Role::Group);
                    if let Some(title) = title {
                        group = group.aria_label(title.clone());
                    }
                    out.push(group.children(heading).children(rows).into_any_element());
                }
                SelectorEntry::Option(option) => {
                    if ctx.searching && !matches_query(option, &ctx.query) {
                        continue;
                    }
                    out.push(self.row(shared, ctx, flat, window, cx));
                    flat += 1;
                }
            }
        }
        out
    }

    /// One option row (upstream `SelectorOptionRow`).
    fn row(
        &self,
        shared: &Rc<Shared>,
        ctx: &ListCtx,
        index: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let Some(item) = shared.shown.get(index).cloned() else {
            return div().into_any_element();
        };
        let highlighted = ctx.highlight == Some(index);
        // AR5: the ring after keyboard input only.
        let ring = highlighted && ctx.keyboard && ctx.hovered != Some(index);
        let selected = self.value.as_ref() == Some(&item.value);
        let (pad_y, pad_x) = match ctx.size {
            ElementSize::Sm => (t.spacing.s1, t.spacing.s2),
            ElementSize::Md => (t.spacing.s1_5, t.spacing.s2),
            ElementSize::Lg => (t.spacing.s2, t.spacing.s2),
        };
        let radius = t.radius.element.to_pixels(rem);
        // FR3: an empty mark takes no space.
        let check_selector = format!("{}-check-{}", self.id, item.value);
        let mark = selected.then(|| {
            let mut mark = div()
                .debug_selector(move || check_selector)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .min_w(gpui::rems(1.))
                .child(CheckIndicator::new(true).disabled(item.disabled));
            theme.refine(mark.style(), "selector-check", &[], None);
            mark
        });
        let content = div()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .flex_1()
            .min_w_0()
            .child(match &self.render_option {
                Some(render) => render(&item, window, cx),
                None => {
                    let mut option = SelectorOption::new(item.label.clone());
                    if let Some(icon) = item.icon.clone() {
                        option = option.icon(icon);
                    }
                    if let Some(description) = item.description.clone() {
                        option = option.description(description);
                    }
                    option.into_any_element()
                }
            });
        let selector_name = format!("{}-option-{}", self.id, item.value);
        let mut el = div()
            .id(ElementId::Name(format!("{}-item-{index}", self.id).into()))
            .debug_selector(move || selector_name)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(t.spacing.s2)
            .w_full()
            .py(pad_y)
            .px(pad_x)
            .rounded(radius)
            .text_size(t.text.label.size)
            .line_height(relative(t.text.label.leading))
            .text_color(t.color.text_primary)
            .role(Role::ListBoxOption)
            .aria_label(item.label.clone())
            .aria_selected(selected)
            .aria_disabled(item.disabled);
        el = match self.indicator_position {
            SelectorIndicatorPosition::Start => el.children(mark).child(content),
            SelectorIndicatorPosition::End => el.child(content).children(mark),
        };
        if highlighted {
            let marker = format!("{}-highlighted-{}", self.id, item.value);
            el = el
                .bg(t.color.overlay_hover)
                .aria_active_descendant_of(&ctx.owner)
                .child(div().absolute().size_0().debug_selector(move || marker));
        }
        if ring {
            let marker = format!("{}-ring-{}", self.id, item.value);
            el = el.child(focus_ring_inset(t, radius, rem).debug_selector(move || marker));
        }
        if selected {
            el = el.font_weight(t.font.weight.medium);
        }
        el = if item.disabled {
            el.opacity(0.5)
        } else {
            el.cursor_pointer()
        };
        // Its bounds, for the press model and the overlay measure.
        {
            let state = shared.state.clone();
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
        {
            let shared = shared.clone();
            el = el.on_mouse_move(move |_, window, cx| shared.hover(index, window, cx));
        }
        {
            let shared = shared.clone();
            el = el.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                shared.state.update(cx, |s, _| s.press = true);
                shared.hover(index, window, cx);
            });
        }
        let mut props = vec![("size", ctx.size.as_str())];
        if highlighted {
            props.push(("highlighted", "highlighted"));
        }
        if selected {
            props.push(("selected", "selected"));
        }
        if item.disabled {
            props.push(("disabled", "disabled"));
        }
        theme.refine(el.style(), "selector-option-row", &props, None);
        el.into_any_element()
    }

    /// The listbox, under a search field when there is one.
    fn panel(
        &self,
        shared: &Rc<Shared>,
        ctx: &ListCtx,
        rows: Vec<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let cap = crate::overlay::viewport_cap(t, window);
        let border = t.border.width.to_pixels(rem);
        // The input trigger's text inset includes its border; the list
        // mirrors it so rows line up with the value.
        let pad_x = if !ctx.sheet && self.variant == SelectorVariant::Input {
            t.spacing.s1.to_pixels(rem) + border
        } else {
            t.spacing.s1.to_pixels(rem)
        };
        let state = shared.state.clone();
        let (scroll, listbox_focus) = {
            let s = state.read(cx);
            (s.scroll.clone(), s.listbox_focus.clone())
        };
        let overlay_mode = self.placement.is_none() && !self.search && !ctx.sheet;
        let target = shared
            .value
            .as_ref()
            .and_then(|v| shared.shown.iter().position(|i| &i.value == v))
            .unwrap_or(0);
        let probe = {
            let (state, shared) = (state.clone(), shared.clone());
            gpui::canvas(
                {
                    let state = state.clone();
                    move |bounds, _, cx| state.update(cx, |s, _| s.listbox = Some(bounds))
                },
                move |_, _, window, cx| {
                    // The press model (FR7): the release picks the enabled
                    // option under it; a mouse released outside closes the
                    // list, a finger leaves it open. Registered while the
                    // list is drawn, since a press and its release can come
                    // between two frames.
                    {
                        let up = shared.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble
                                || event.button != MouseButton::Left
                                || !up.state.read(cx).press
                            {
                                return;
                            }
                            up.state.update(cx, |s, _| s.press = false);
                            let (row, inside) = {
                                let s = up.state.read(cx);
                                let inside = s.listbox.is_some_and(|b| b.contains(&event.position));
                                let row = inside
                                    .then(|| {
                                        s.rows.iter().position(|b| {
                                            b.is_some_and(|b| b.contains(&event.position))
                                        })
                                    })
                                    .flatten();
                                (row, inside)
                            };
                            match row {
                                Some(index) if up.shown.get(index).is_some_and(|i| !i.disabled) => {
                                    up.select(index, window, cx)
                                }
                                _ if !inside && !window.last_input_was_touch() => {
                                    up.close(window, cx)
                                }
                                _ => {}
                            }
                        });
                        let held = shared.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble || !held.state.read(cx).press {
                                return;
                            }
                            let row = {
                                let s = held.state.read(cx);
                                s.listbox
                                    .is_some_and(|b| b.contains(&event.position))
                                    .then(|| {
                                        s.rows.iter().position(|b| {
                                            b.is_some_and(|b| b.contains(&event.position))
                                        })
                                    })
                                    .flatten()
                                    .filter(|&i| held.shown.get(i).is_some_and(|o| !o.disabled))
                            };
                            match row {
                                Some(index) => held.hover(index, window, cx),
                                // Nothing is highlighted off an enabled row.
                                None if held.state.read(cx).highlight.is_some() => {
                                    held.state.update(cx, |s, _| s.highlight = None);
                                    window.refresh();
                                }
                                None => {}
                            }
                        });
                    }
                    let (reveal, unmeasured) = {
                        let s = state.read(cx);
                        (s.reveal, s.overlay.is_none())
                    };
                    if reveal || (overlay_mode && unmeasured) {
                        let state = state.clone();
                        window.defer(cx, move |window, cx| {
                            after_frame(&state, target, overlay_mode, window, cx)
                        });
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };
        let list_selector = format!("{}-listbox", self.id);
        let mut listbox = div()
            .id(ElementId::Name(format!("{}-listbox", self.id).into()))
            .debug_selector(move || list_selector)
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .min_h_0()
            .max_h(px(LIST_CAP).min(cap.height))
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .py(t.spacing.s1)
            .px(pad_x)
            .role(Role::ListBox)
            .aria_label(self.label.clone())
            .on_mouse_down(MouseButton::Left, {
                let state = state.clone();
                move |_, _, cx| state.update(cx, |s, _| s.press = true)
            })
            .children(rows)
            .child(probe);
        // The sheet's list takes focus and the trigger's keys.
        if ctx.sheet && !self.search {
            let shared = shared.clone();
            listbox = listbox
                .track_focus(&listbox_focus)
                .on_key_down(move |event, window, cx| {
                    if shared.trigger_key(event, window, cx) {
                        cx.stop_propagation();
                    }
                })
                .child(focus_probe(&state, listbox_focus.clone()));
        }
        let Some(editor) = shared.editor.clone() else {
            return listbox.into_any_element();
        };

        // The search row: a magnifier, a borderless field and a clear button.
        let search_focus = gpui::Focusable::focus_handle(editor.read(cx), cx);
        {
            let (on_change, on_key) = (shared.clone(), shared.clone());
            editor.update(cx, |e, _| {
                e.set_settings(FieldSettings {
                    spell_check: false,
                    ..FieldSettings::default()
                });
                e.on_change = Some(Rc::new(
                    move |text: String, window: &mut Window, cx: &mut App| {
                        on_change.set_query(text, window, cx)
                    },
                ));
                e.on_key_down = Some(Rc::new(
                    move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                        let key = event.keystroke.key.as_str();
                        if matches!(
                            key,
                            "down" | "up" | "pageup" | "pagedown" | "enter" | "escape"
                        ) && on_key.combobox_key(event, window, cx)
                        {
                            cx.stop_propagation();
                        }
                    },
                ));
            });
        }
        let t18 = i18n(cx);
        let search_label = t18.t("@astryx.selector.searchOptions");
        let style = EditorStyle {
            text_color: t.color.text_primary,
            placeholder: Some(ctx.search_placeholder.clone()),
            placeholder_color: t.color.text_secondary,
            selection_color: crate::input::selection_color(t),
            caret_color: t.color.text_primary,
            multiline: false,
            min_lines: 1,
            max_lines: Some(1),
            height: None,
            reduced_motion: prefers_reduced_motion(cx),
            spelling_color: t.color.error,
        };
        let field = text_editor("query", &editor, style, cx)
            .flex_1()
            .min_w_0()
            .font_stack(&t.font.family.body)
            .text_size(t.text.label.size)
            .line_height(relative(t.text.label.leading))
            .role(Role::ComboBox)
            .aria_label(search_label.clone())
            .aria_expanded(true)
            .aria_required(ctx.required);
        let clear = (!ctx.query.is_empty()).then(|| {
            let (shared, editor, focus) = (shared.clone(), editor.clone(), search_focus.clone());
            IconButton::new(
                ElementId::Name(format!("{}-search-clear", self.id).into()),
                t18.format(
                    "@astryx.textInput.clearLabel",
                    &[("label", search_label.as_ref().into())],
                ),
                Icon::new("close")
                    .size(IconSize::Sm)
                    .color(IconColor::Secondary),
            )
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .h(px(20.))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                editor.update(cx, |e, cx| e.set_text("", cx));
                shared.set_query(String::new(), window, cx);
                window.focus(&focus, cx);
            })
        });
        let mut input_box = div()
            .relative()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .w_full()
            .py(t.spacing.s1_5)
            .px(t.spacing.s2)
            .rounded(t.radius.element)
            .child(
                Icon::new("search")
                    .size(IconSize::Sm)
                    .color(IconColor::Secondary),
            )
            .child(field)
            .children(clear);
        // The keyboard's focus ring, inset (the field sits inside the panel).
        if is_focus_visible(&search_focus, window) {
            input_box = input_box.shadow(vec![gpui::BoxShadow {
                color: t.color.accent,
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(2.),
                inset: true,
            }]);
        }
        let mut row = div()
            .id(ElementId::Name(format!("{}-search-row", self.id).into()))
            .flex_none()
            .py(t.spacing.s1)
            .px(pad_x)
            .child(input_box)
            .child(focus_probe(&state, search_focus.clone()));
        theme.refine(row.style(), "selector-search", &[], None);
        // Tab: to the clear button while a query shows it, else out of the
        // closing list (from the trigger, as upstream's focus return does).
        {
            let (shared, focus) = (shared.clone(), search_focus.clone());
            row = row.on_action(move |_: &FocusNext, window, cx| {
                let has_query = !shared.state.read(cx).query.is_empty();
                if !(focus.is_focused(window) && has_query) {
                    shared.close(window, cx);
                }
                cx.propagate();
            });
        }
        {
            let (shared, focus) = (shared.clone(), search_focus.clone());
            row = row.on_action(move |_: &FocusPrevious, window, cx| {
                if focus.is_focused(window) {
                    shared.close(window, cx);
                }
                cx.propagate();
            });
        }
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
/// the list asked for focus (the search field, the sheet's list).
fn focus_probe(state: &Entity<SelectorState>, target: FocusHandle) -> AnyElement {
    let state = state.clone();
    gpui::canvas(
        |_, _, _| {},
        move |_, _, window, cx| {
            if !state.read(cx).pending_focus {
                return;
            }
            let (state, target) = (state.clone(), target.clone());
            window.defer(cx, move |window, cx| {
                if state.read(cx).pending_focus {
                    state.update(cx, |s, _| s.pending_focus = false);
                    target.focus(window, cx);
                }
            });
        },
    )
    .absolute()
    .size_0()
    .into_any_element()
}
