//! The Tokenizer field, upstream `Tokenizer/Tokenizer.tsx` (inline presentation).

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, ElementId, Focusable, IntoElement, KeyDownEvent, Length, ParentElement,
    Pixels, RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, div, point,
    prelude::*, px,
};
use rustryx_i18n::{current as i18n, use_direction};
use rustryx_primitives::{
    ElementSize, LayerAlign, LayerOptions, LayerPosition, Placement, Politeness, anchored_layer,
    announce, is_focus_visible, provide, resolve_size,
};
use rustryx_theme::{current, scope};

use crate::button::{ButtonVariant, IconButton};
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::icon::{Icon, IconColor, IconDefaultSize, IconSize};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::input::group::{group_context, group_disabled, group_place, inside_member};
use crate::overflow::OverflowList;
use crate::search::{SearchSource, Searchable};
use crate::status::{Badge, Spinner, SpinnerSize};
use crate::token::Token;
use crate::typeahead::{BaseTypeahead, TypeaheadController, TypeaheadMenu};

type ChangeFn<T> = Rc<dyn Fn(Vec<T>, TokenizerChange<T>, &mut Window, &mut App)>;
type RenderTokenFn<T> = Rc<dyn Fn(&T, ActFn, &mut Window, &mut App) -> AnyElement>;
type CreateFn<T> = Rc<dyn Fn(&str) -> T>;
type ActFn = Rc<dyn Fn(&mut Window, &mut App)>;
type RemoveFn<T> = Rc<dyn Fn(&T, &mut Window, &mut App)>;
type BoundsFn = Rc<dyn Fn(gpui::Bounds<Pixels>, &mut App)>;
type RemovableFn<T> = Rc<dyn Fn(&T) -> bool>;
type FocusFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `TokenizerChange`'s `type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenizerChangeKind {
    Add,
    /// An item made from typed text.
    Create,
    Remove,
    Reorder,
}

/// What changed, upstream `TokenizerChange`.
#[derive(Clone, Debug)]
pub struct TokenizerChange<T> {
    pub item: T,
    pub kind: TokenizerChangeKind,
}

/// Upstream `tokenOverflowBehavior`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TokenOverflowBehavior {
    /// Tokens wrap onto more rows.
    #[default]
    None,
    /// Blurred, the tokens fold into one row with a count.
    UnfocusedInline,
    /// Blurred, the field folds into a one-row placeholder; focus opens it
    /// over the placeholder.
    UnfocusedLayer,
}

/// Focuses, blurs, opens and closes a Tokenizer from code, upstream
/// `TokenizerHandle`. Make it in the render that draws the Tokenizer.
#[derive(Clone)]
pub struct TokenizerController(TypeaheadController);

impl TokenizerController {
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        TokenizerController(TypeaheadController::new(id, window, cx))
    }

    /// Upstream `focus()`.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.0.focus(window, cx)
    }

    /// Upstream `blur()`.
    pub fn blur(&self, window: &mut Window, cx: &mut App) {
        self.0.blur(window, cx)
    }

    /// Upstream `open()`.
    pub fn open(&self, window: &mut Window, cx: &mut App) {
        self.0.open(window, cx)
    }

    /// Upstream `close()`.
    pub fn close(&self, window: &mut Window, cx: &mut App) {
        self.0.close(window, cx)
    }
}

/// Several items from a search, as tokens, upstream `Tokenizer`.
///
/// ```ignore
/// Tokenizer::new("tags", "Tags", static_source(tags), chosen.clone())
///     .on_change(|items, _change, _, _| { /* store them */ })
/// ```
#[derive(IntoElement)]
pub struct Tokenizer<T: Searchable> {
    base: BaseTypeahead<T>,
    label: SharedString,
    value: Vec<T>,
    on_change: Option<ChangeFn<T>>,
    max_entries: Option<usize>,
    clear: bool,
    /// Which tokens a remove may take (upstream `TokenizerItemRemovableContext`).
    removable: Option<RemovableFn<T>>,
    /// Clear all's announcement (upstream `TokenizerClearAllMessageContext`).
    clear_all_message: Option<SharedString>,
    /// Reports the input's bounds (upstream's opening-control descriptor,
    /// Tokenizer DEC-1).
    input_bounds: Option<BoundsFn>,
    /// FR31: the opening query.
    opening_query: Option<SharedString>,
    /// FR31: open the sheet as it mounts, with no trigger; called when the
    /// sheet closes.
    sheet_on_mount: Option<ActFn>,
    render_token: Option<RenderTokenFn<T>>,
    disabled: bool,
    disabled_message: Option<SharedString>,
    status: Option<InputStatus>,
    status_placement: StatusPlacement,
    label_hidden: bool,
    description: Option<SharedString>,
    required: bool,
    optional: bool,
    label_tooltip: Option<SharedString>,
    create: Option<CreateFn<T>>,
    start_icon: Option<AnyElement>,
    end_content: Option<AnyElement>,
    width: Option<Length>,
    token_overflow: TokenOverflowBehavior,
    on_focus: Option<FocusFn>,
    on_blur: Option<FocusFn>,
    size: Option<ElementSize>,
    presentation: crate::overlay::PopoverPresentation,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl<T: Searchable> Tokenizer<T> {
    /// Upstream `label`, `searchSource` and `value`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        source: impl SearchSource<T>,
        value: Vec<T>,
    ) -> Self {
        Self::from_base(BaseTypeahead::new(id, source), label, value)
    }

    /// A source kept between renders.
    pub fn from_rc(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        source: Rc<dyn SearchSource<T>>,
        value: Vec<T>,
    ) -> Self {
        Self::from_base(BaseTypeahead::from_rc(id, source), label, value)
    }

    fn from_base(base: BaseTypeahead<T>, label: impl Into<SharedString>, value: Vec<T>) -> Self {
        Tokenizer {
            base,
            label: label.into(),
            value,
            on_change: None,
            max_entries: None,
            clear: false,
            removable: None,
            clear_all_message: None,
            input_bounds: None,
            opening_query: None,
            sheet_on_mount: None,
            render_token: None,
            disabled: false,
            disabled_message: None,
            status: None,
            status_placement: StatusPlacement::Attached,
            label_hidden: false,
            description: None,
            required: false,
            optional: false,
            label_tooltip: None,
            create: None,
            start_icon: None,
            end_content: None,
            width: None,
            token_overflow: TokenOverflowBehavior::None,
            on_focus: None,
            on_blur: None,
            size: None,
            presentation: crate::overlay::PopoverPresentation::Adaptive,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `presentation`: adaptive by default (the sheet on a compact
    /// touch screen, else the inline field).
    pub fn presentation(mut self, presentation: crate::overlay::PopoverPresentation) -> Self {
        self.presentation = presentation;
        self
    }

    rustryx_primitives::setters! {
        /// Upstream `hasClear`: a Clear all button.
        !clear,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `disabledMessage`.
        ?disabled_message: SharedString,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `statusVariant`.
        status_placement: StatusPlacement,
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `labelTooltip`.
        ?label_tooltip: SharedString,
        /// Upstream `width`.
        ?width: Length,
        /// Upstream `tokenOverflowBehavior`.
        token_overflow: TokenOverflowBehavior,
        /// Upstream `size`.
        ?size: ElementSize,
    }

    /// Names the source: a new key is a new source.
    pub fn source_key(mut self, key: impl std::hash::Hash) -> Self {
        self.base = self.base.source_key(key);
        self
    }

    /// Upstream `onChange`: the whole selection and what changed.
    pub fn on_change(
        mut self,
        f: impl Fn(Vec<T>, TokenizerChange<T>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `maxEntries`: at the limit the field collapses.
    pub fn max_entries(mut self, max: usize) -> Self {
        self.max_entries = Some(max);
        self
    }

    /// Locks tokens: no remove path removes one `f` refuses (PowerSearch's
    /// read-only filters).
    pub(crate) fn item_removable(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.removable = Some(Rc::new(f));
        self
    }

    /// FR31: the package-internal opening query (PowerSearch's "More" rows).
    pub(crate) fn opening_query(mut self, query: impl Into<SharedString>) -> Self {
        let query: SharedString = query.into();
        self.base = self.base.opening_query(query.clone());
        self.opening_query = Some(query);
        self
    }

    /// FR31: opens the sheet as the Tokenizer mounts, renders no trigger,
    /// and calls `on_close` when the sheet closes (its owner unmounts it).
    pub(crate) fn open_sheet_on_mount(
        mut self,
        on_close: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.sheet_on_mount = Some(Rc::new(on_close));
        self.presentation = crate::overlay::PopoverPresentation::BottomSheet;
        self
    }

    /// Reports where the input sits each frame (PowerSearch's editor edge).
    pub(crate) fn on_input_bounds(
        mut self,
        f: impl Fn(gpui::Bounds<Pixels>, &mut App) + 'static,
    ) -> Self {
        self.input_bounds = Some(Rc::new(f));
        self
    }

    /// Clear all's announcement in the composing component's words.
    pub(crate) fn clear_all_message(mut self, message: impl Into<SharedString>) -> Self {
        self.clear_all_message = Some(message.into());
        self
    }

    /// Upstream `renderToken`: a token, given its remove action.
    pub fn render_token(
        mut self,
        f: impl Fn(&T, ActFn, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_token = Some(Rc::new(f));
        self
    }

    /// Upstream `renderItem`.
    pub fn render_item(
        mut self,
        f: impl Fn(&T, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.base = self.base.render_item(f);
        self
    }

    /// Upstream `placeholder`: while nothing is chosen.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.base.placeholder = Some(placeholder.into());
        self
    }

    /// Upstream `hasEntriesOnFocus`: the focus list, kept open across picks.
    pub fn entries_on_focus(mut self, on: bool) -> Self {
        self.base.entries_on_focus = on;
        self
    }

    /// Upstream `maxMenuItems`.
    pub fn max_menu_items(mut self, n: usize) -> Self {
        self.base = self.base.max_menu_items(n);
        self
    }

    /// Upstream `menuWidth`.
    pub fn menu_width(mut self, width: impl Into<Pixels>) -> Self {
        self.base.menu_width = Some(width.into());
        self
    }

    /// Upstream `minQueryLength`.
    pub fn min_query_length(mut self, n: usize) -> Self {
        self.base.min_query_length = n;
        self
    }

    /// Upstream `emptySearchText`.
    pub fn empty_search_text(mut self, text: impl Into<SharedString>) -> Self {
        self.base.empty_search_text = Some(text.into());
        self
    }

    /// Upstream `hasAutoFocus`.
    pub fn auto_focus(mut self, on: bool) -> Self {
        self.base.auto_focus = on;
        self
    }

    /// Upstream `debounceMs`.
    pub fn debounce(mut self, debounce: Duration) -> Self {
        self.base.debounce = debounce;
        self
    }

    /// Upstream `hasCreate`: a "Create" row for unmatched text, which
    /// `f` makes into an item.
    pub fn create(mut self, f: impl Fn(&str) -> T + 'static) -> Self {
        self.create = Some(Rc::new(f));
        self
    }

    /// Upstream `onChangeQuery`.
    pub fn on_change_query(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.base = self.base.on_change_query(f);
        self
    }

    /// Upstream `startIcon`.
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `endContent`: at the row's end.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `handleRef`.
    pub fn controller(mut self, controller: TokenizerController) -> Self {
        self.base.controller = Some(controller.0);
        self
    }

    /// Upstream `menu`.
    pub fn menu(mut self, menu: TypeaheadMenu) -> Self {
        self.base.menu = menu;
        self
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.base = self.base.on_open_change(f);
        self
    }

    /// Upstream `onFocus`: focus entered the field.
    pub fn on_focus(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_focus = Some(Rc::new(f));
        self
    }

    /// Upstream `onBlur`: focus left the field.
    pub fn on_blur(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_blur = Some(Rc::new(f));
        self
    }

    /// Upstream `controlXstyle`.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl<T: Searchable> Styled for Tokenizer<T> {
    /// Upstream `containerXstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[derive(Default)]
struct TokenizerState {
    /// Focus was within the field last frame.
    focused: bool,
    on_focus: Option<FocusFn>,
    on_blur: Option<FocusFn>,
}

impl<T: Searchable> RenderOnce for Tokenizer<T> {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.base.id.clone();
        let disabled = self.disabled || group_disabled(cx);
        let place = group_place(cx);
        let group = place.and_then(|_| group_context(cx));
        let dir = use_direction(cx);
        let theme = current(window, cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let t18 = i18n(cx);
        let rem = window.rem_size();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id}-tokenizer").into()),
            cx,
            |_, _| TokenizerState::default(),
        );
        let group_focus = window
            .use_keyed_state(
                ElementId::Name(format!("{id}-tokenizer-group").into()),
                cx,
                |_, cx| cx.focus_handle().tab_stop(false),
            )
            .read(cx)
            .clone();
        let editor = window.use_keyed_state(
            ElementId::Name(format!("{id}-typeahead-input").into()),
            cx,
            |window, cx| {
                rustryx_primitives::editing::TextEditState::reporting("", false, window, cx)
            },
        );
        let input_focus = editor.read(cx).focus_handle(cx);
        let engine = crate::typeahead::engine::engine_of::<T>(&id, window, cx);
        let value = self.value.clone();
        let at_limit = self.max_entries.is_some_and(|m| value.len() >= m);
        // An open sheet keeps its presentation until it closes (FR16): a phone turned to
        // landscape does not swap the surface, or the trigger its focus returns to.
        let sheet_open = super::sheet::draft_state::<T>(&self.base.id, window, cx)
            .read(cx)
            .open;
        if sheet_open || crate::overlay::presents_as_sheet(self.presentation, window, cx) {
            return self.render_sheet_field(disabled, size, window, cx);
        }

        // onFocus and onBlur, as focus enters and leaves the field.
        let focused = group_focus.contains_focused(window, cx) || input_focus.is_focused(window);
        state.update(cx, |s, _| {
            s.on_focus = self.on_focus.clone();
            s.on_blur = self.on_blur.clone();
        });
        if focused != state.read(cx).focused {
            let report = state.update(cx, |s, _| {
                s.focused = focused;
                if focused {
                    s.on_focus.clone()
                } else {
                    s.on_blur.clone()
                }
            });
            if let Some(f) = report {
                window.defer(cx, move |window, cx| f(window, cx));
            }
        }

        let remove: RemoveFn<T> = {
            let (value, report, focus) =
                (value.clone(), self.on_change.clone(), input_focus.clone());
            let removable = self.removable.clone();
            Rc::new(move |item: &T, window: &mut Window, cx: &mut App| {
                if disabled || removable.as_ref().is_some_and(|f| !f(item)) {
                    return;
                }
                let next: Vec<T> = value
                    .iter()
                    .filter(|v| v.id() != item.id())
                    .cloned()
                    .collect();
                if let Some(f) = &report {
                    f(
                        next,
                        TokenizerChange {
                            item: item.clone(),
                            kind: TokenizerChangeKind::Remove,
                        },
                        window,
                        cx,
                    );
                }
                let text = i18n(cx).format(
                    "@astryx.tokenizer.tokenRemoved",
                    &[("label", item.label().as_ref().into())],
                );
                announce(text, Politeness::Polite, window, cx);
                window.focus(&focus, cx);
            })
        };

        // The search: what is chosen is never offered (FR13).
        self.base.exclude = value.iter().map(|v| v.id()).collect();
        self.base.keep_open_on_pick = self.base.entries_on_focus;
        self.base.focusable_disabled = disabled;
        if let Some(factory) = self.create.clone() {
            let query = engine.read(cx).query.trim().to_string();
            let (chosen_ids, chosen_labels): (Vec<SharedString>, Vec<String>) = value
                .iter()
                .map(|v| (v.id(), v.label().to_lowercase()))
                .unzip();
            let at_limit_now = at_limit;
            self.base.query_entries = Some(Rc::new(move |query: &str, results: &[T]| {
                let trimmed = query.trim();
                let lower = trimmed.to_lowercase();
                let exists = trimmed.is_empty()
                    || chosen_ids.iter().any(|i| i.as_ref() == trimmed)
                    || chosen_labels.contains(&lower)
                    || results.iter().any(|r| r.label().to_lowercase() == lower);
                if exists || at_limit_now {
                    return Vec::new();
                }
                vec![factory(trimmed)]
            }));
            self.base.entry_label = (!query.is_empty()).then(|| {
                t18.format(
                    "@astryx.tokenizer.create",
                    &[("value", query.as_str().into())],
                )
            });
        }
        {
            let (value, report, engine) = (value.clone(), self.on_change.clone(), engine.clone());
            let factory = self.create.clone();
            self.base.on_change = Some(Rc::new(
                move |item: Option<T>, window: &mut Window, cx: &mut App| {
                    let Some(item) = item else { return };
                    // A create row is one of the rows appended after the source's.
                    let created = factory.is_some() && {
                        let e = engine.read(cx);
                        let source_end = e.results.len() - e.entry_count;
                        e.results
                            .iter()
                            .position(|r| r.id() == item.id())
                            .is_some_and(|at| at >= source_end)
                    };
                    let item = match (&factory, created) {
                        (Some(f), true) => f(item.label().as_ref()),
                        _ => item,
                    };
                    let mut next = value.clone();
                    next.push(item.clone());
                    let kind = if created {
                        TokenizerChangeKind::Create
                    } else {
                        TokenizerChangeKind::Add
                    };
                    let label = item.label();
                    if let Some(f) = &report {
                        f(next, TokenizerChange { item, kind }, window, cx);
                    }
                    let text = i18n(cx).format(
                        "@astryx.tokenizer.tokenAdded",
                        &[("label", label.as_ref().into())],
                    );
                    announce(text, Politeness::Polite, window, cx);
                },
            ));
        }
        // Backspace in the empty field removes the last token.
        {
            let (value, engine, remove) = (value.clone(), engine.clone(), remove.clone());
            self.base.on_key_down = Some(Rc::new(
                move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                    if event.keystroke.key == "backspace"
                        && engine.read(cx).query.is_empty()
                        && let Some(last) = value.last()
                    {
                        remove(last, window, cx);
                        return true;
                    }
                    false
                },
            ));
        }
        let name: SharedString = match &group {
            Some(group) => format!("{} {}", group.label, self.label).into(),
            None => self.label.clone(),
        };
        self.base.label = Some(name.clone());
        let mut described = described_by(
            self.description.as_ref(),
            self.status
                .as_ref()
                .filter(|_| self.status_placement != StatusPlacement::Tooltip),
            disabled.then_some(self.disabled_message.as_ref()).flatten(),
            &t18,
        );
        if let Some(group) = &group {
            described = group.described.iter().cloned().chain(described).collect();
        }
        if !described.is_empty() {
            self.base.description = Some(described.join(" ").into());
        }
        self.base.required = crate::field::resolve(self.required, self.optional, cx).required;
        let status = self.status.as_ref().map(|s| s.status);
        self.base.invalid = status == Some(StatusType::Error);
        self.base.size = size;
        if !value.is_empty() {
            self.base.placeholder = Some(SharedString::default());
        }
        // FR15: at the limit the list closes.
        if at_limit && engine.read(cx).open {
            let report = engine.update(cx, |e, _| {
                e.open = false;
                e.highlight = None;
                e.on_open_change.clone()
            });
            if let Some(f) = report {
                window.defer(cx, move |window, cx| f(false, window, cx));
            }
        }
        let parts = self.base.parts(window, cx);

        // The tokens.
        let tokens = |window: &mut Window, cx: &mut App| -> Vec<AnyElement> {
            value
                .iter()
                .map(|item| {
                    let act: ActFn = {
                        let (remove, item) = (remove.clone(), item.clone());
                        Rc::new(move |window: &mut Window, cx: &mut App| remove(&item, window, cx))
                    };
                    match &self.render_token {
                        Some(f) => f(item, act, window, cx),
                        None => {
                            let mut token = Token::new(
                                ElementId::Name(format!("{id}-token-{}", item.id()).into()),
                                item.label(),
                            );
                            if !disabled && self.removable.as_ref().is_none_or(|f| f(item)) {
                                token = token.on_remove(move |_, window, cx| act(window, cx));
                            }
                            token.into_any_element()
                        }
                    }
                })
                .collect()
        };
        let chrome = Chrome {
            id: ElementId::Name(format!("{id}-control").into()),
            target: "tokenizer",
            variant: None,
            size,
            status,
            disabled,
            read_only: false,
            focused,
            focus_visible: is_focus_visible(&input_focus, window),
            row: false,
            place,
            dir,
        };
        let part_override = theme.for_part("tokenizer", &chrome.props());
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let folded =
            !focused && self.token_overflow != TokenOverflowBehavior::None && !value.is_empty();
        let mut surface = control_box(&chrome, &part, window, cx)
            .track_focus(&group_focus)
            .role(Role::Group)
            .aria_label(name.clone())
            .items_center()
            .gap(t.spacing.s1)
            .px(t.spacing.s1)
            .py(t.spacing.s1.to_pixels(rem) - px(1.))
            .cursor_text();
        if place.is_none() {
            surface = surface.min_h(crate::button::element_height(size, t));
        }
        surface = if folded || place.is_some() {
            surface.overflow_hidden()
        } else {
            surface.flex_wrap()
        };
        surface.style().refine(&self.control_style);
        if !disabled {
            let focus = input_focus.clone();
            surface = surface.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx);
            });
        }
        // In an InputGroup the start icon, tokens and input scroll in one track
        // beside the end lane, on one line (upstream `TokenTrack`).
        let grouped = place.is_some() && !folded;
        let mut lane: Vec<AnyElement> = Vec::new();
        let has_icon = self.start_icon.is_some();
        if let Some(icon) = self.start_icon.take() {
            let icon = div()
                .flex()
                .flex_none()
                .items_center()
                .text_color(t.color.icon_secondary)
                .child(provide(IconDefaultSize(IconSize::Sm), icon));
            if grouped {
                lane.push(icon.into_any_element());
            } else {
                surface = surface.child(icon);
            }
        }
        if folded {
            // Blurred: one row of tokens and a count of the rest.
            let count_label = t18.clone();
            let mut row = OverflowList::new(ElementId::Name(format!("{id}-folded").into()))
                .gap(crate::layout::SpacingStep::S1)
                .overflow_renderer(move |hidden, _, _| {
                    Badge::new(count_label.format(
                        "@astryx.tokenizer.overflowCount",
                        &[("count", rustryx_i18n::Arg::Num(hidden.len() as f64))],
                    ))
                    .into_any_element()
                });
            for item in &value {
                let label = item.label();
                let token_id = ElementId::Name(format!("{id}-folded-{}", item.id()).into());
                row = row.keyed_item(item.id(), move |_, _| {
                    Token::new(token_id.clone(), label.clone()).into_any_element()
                });
            }
            let folded_selector = format!("{id}-folded");
            surface = surface.child(
                div()
                    .debug_selector(move || folded_selector)
                    .flex_1()
                    .min_w_0()
                    .child(row),
            );
        } else if grouped {
            // Each token's box knows when focus is inside it, for the track's reveal.
            for (item, token) in value.iter().zip(tokens(window, cx)) {
                let wrapper = window
                    .use_keyed_state(
                        ElementId::Name(format!("{id}-track-item-{}", item.id()).into()),
                        cx,
                        |_, cx| cx.focus_handle().tab_stop(false),
                    )
                    .read(cx)
                    .clone();
                lane.push(
                    div()
                        .id(ElementId::Name(
                            format!("{id}-track-item-{}", item.id()).into(),
                        ))
                        .track_focus(&wrapper)
                        .flex_none()
                        .child(token)
                        .into_any_element(),
                );
            }
        } else {
            surface = surface.children(tokens(window, cx));
        }
        // FR1: the field stays mounted, collapsed at the limit.
        let input = if at_limit || folded {
            div()
                .w(px(0.))
                .h(px(0.))
                .overflow_hidden()
                .opacity(0.)
                .child(parts.input)
                .into_any_element()
        } else {
            let report = self.input_bounds.clone();
            div()
                .relative()
                .flex_1()
                // A grouped row cannot wrap: its in-flow lane needs only 40 px.
                .min_w(px(if grouped { 40. } else { 60. }))
                .flex()
                .child(parts.input)
                .children(report.map(|report| {
                    // Where the input sits, for a composing popup's edge rule.
                    gpui::canvas(move |bounds, _, cx| report(bounds, cx), |_, _, _, _| {})
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                }))
                .into_any_element()
        };
        if grouped {
            lane.push(input);
            surface = surface.child(Self::group_track(
                &id,
                lane,
                has_icon,
                &value,
                disabled,
                name.clone(),
                t,
                dir,
                window,
                cx,
            ));
        } else {
            surface = surface.child(input);
        }
        // The end lane.
        let can_remove = |item: &T| self.removable.as_ref().is_none_or(|f| f(item));
        let show_clear = self.clear && !disabled && value.iter().any(can_remove);
        if parts.busy || show_clear || self.end_content.is_some() || status.is_some() {
            let mut end = div()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.spacing.s1)
                .ml_auto();
            if parts.busy {
                end = end.child(
                    div()
                        .debug_selector({
                            let s = format!("{id}-busy");
                            move || s
                        })
                        .child(
                            Spinner::new(ElementId::Name(format!("{id}-spinner").into()))
                                .size(SpinnerSize::Sm),
                        ),
                );
            }
            if show_clear {
                let (value, report, focus) =
                    (value.clone(), self.on_change.clone(), input_focus.clone());
                let (removable, message) = (self.removable.clone(), self.clear_all_message.clone());
                end = end.child(
                    div()
                        .debug_selector({
                            let s = format!("{id}-clear");
                            move || s
                        })
                        .child(
                            IconButton::new(
                                ElementId::Name(format!("{id}-clear").into()),
                                t18.t("@astryx.tokenizer.clearAll"),
                                Icon::new("close")
                                    .size(IconSize::Sm)
                                    .color(IconColor::Secondary),
                            )
                            .variant(ButtonVariant::Ghost)
                            .size(ElementSize::Sm)
                            .tooltip(t18.t("@astryx.tokenizer.clearAll"))
                            .h(px(20.))
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                let locked =
                                    |item: &T| removable.as_ref().is_some_and(|f| !f(item));
                                // Clear all names the last token it removes and
                                // keeps the locked ones.
                                let Some(last) = value.iter().rev().find(|i| !locked(i)).cloned()
                                else {
                                    return;
                                };
                                let kept: Vec<T> =
                                    value.iter().filter(|i| locked(i)).cloned().collect();
                                if let Some(f) = &report {
                                    f(
                                        kept,
                                        TokenizerChange {
                                            item: last,
                                            kind: TokenizerChangeKind::Remove,
                                        },
                                        window,
                                        cx,
                                    );
                                }
                                announce(
                                    message.clone().unwrap_or_else(|| {
                                        i18n(cx).t("@astryx.tokenizer.allRemoved")
                                    }),
                                    Politeness::Polite,
                                    window,
                                    cx,
                                );
                                window.focus(&focus, cx);
                            }),
                        ),
                );
            }
            if let Some(content) = self.end_content.take() {
                end = end.child(content);
            }
            if let Some(status) = status
                .filter(|_| self.status_placement != StatusPlacement::Detached && place.is_none())
            {
                let message = self.status.as_ref().and_then(|s| s.message.clone());
                end = end.child(status_indicator(
                    &id,
                    status,
                    self.status_placement,
                    message.as_ref(),
                    &part,
                    window,
                    cx,
                ));
            }
            surface = surface.child(end);
        }
        let surface = match part_override {
            Some(part) => scope(part, surface).into_any_element(),
            None => surface.into_any_element(),
        };
        let control = (parts.wrap)(surface, window, cx);
        let control = crate::overlay::disabled_reason(
            &id,
            disabled,
            self.disabled_message.as_ref(),
            true,
            control,
        );
        let control = if self.token_overflow == TokenOverflowBehavior::UnfocusedLayer && focused {
            // FR3: open, the field draws over its placeholder in a layer.
            let height = window.use_keyed_state(
                ElementId::Name(format!("{id}-tokenizer-row").into()),
                cx,
                |_, _| px(0.),
            );
            let placeholder = div()
                .w_full()
                .h(crate::button::element_height(size, t).to_pixels(rem));
            let _ = height;
            anchored_layer(
                ElementId::Name(format!("{id}-tokenizer-layer").into()),
                placeholder,
            )
            .light_dismiss(false)
            .match_anchor_width(true)
            .position_with(|anchor, _, _, _| LayerPosition {
                origin: point(anchor.origin.x, anchor.origin.y),
                side: Placement::Below,
            })
            .surface(
                div().w_full().child(control),
                LayerOptions {
                    placement: Placement::Below,
                    align: LayerAlign::Start,
                    offset: px(0.),
                    margin: px(0.),
                    rtl: dir.is_rtl(),
                    priority: 1,
                },
                |_, _| {},
            )
            .into_any_element()
        } else {
            control
        };
        if place.is_some() {
            return inside_member(control).into_any_element();
        }
        let mut field = Field::new(id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(disabled)
            .optional(self.optional)
            .required(self.required)
            .status_placement(self.status_placement);
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
        field.child(control).into_any_element()
    }
}

impl<T: Searchable> Tokenizer<T> {
    /// The grouped row's track (upstream `TokenTrack` and its in-group styles): one
    /// line that scrolls inline under a hidden scrollbar, its hidden edges faded
    /// (AST-025 FR22), and a token whose remove button takes keyboard focus brought
    /// into view. A disabled field has no remove buttons, so the track itself is
    /// the tab stop while it overflows.
    #[allow(clippy::too_many_arguments)]
    fn group_track(
        id: &ElementId,
        lane: Vec<AnyElement>,
        has_icon: bool,
        value: &[T],
        disabled: bool,
        name: SharedString,
        t: &rustryx_theme::Tokens,
        dir: rustryx_i18n::Direction,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        use crate::scrollable_area::fade::{FadeScrollbar, scroll_edge_fades, visible_box};
        use rustryx_primitives::scroll_area::{
            ScrollAxis, ScrollKeyboardAccess, ScrollRole, UseScrollableAreaOptions,
            use_scrollable_area,
        };
        let track_id = ElementId::Name(format!("{id}-track").into());
        let access = if disabled {
            ScrollKeyboardAccess::Viewport {
                label: name,
                role: ScrollRole::Group,
            }
        } else {
            ScrollKeyboardAccess::Content
        };
        let area = use_scrollable_area(
            &track_id,
            UseScrollableAreaOptions::new(ScrollAxis::Inline, access),
            window,
            cx,
        );
        // Keyboard focus inside a token (its remove button) reveals it, once per move.
        let keyboard = window
            .focused(cx)
            .is_some_and(|focus| is_focus_visible(&focus, window));
        let focused_token = value.iter().position(|item| {
            let wrapper = window
                .use_keyed_state(
                    ElementId::Name(format!("{id}-track-item-{}", item.id()).into()),
                    cx,
                    |_, cx| cx.focus_handle().tab_stop(false),
                )
                .read(cx)
                .clone();
            keyboard && wrapper.contains_focused(window, cx)
        });
        let revealed = window.use_keyed_state(
            ElementId::Name(format!("{id}-track-revealed").into()),
            cx,
            |_, _| None::<usize>,
        );
        if *revealed.read(cx) != focused_token {
            revealed.update(cx, |r, _| *r = focused_token);
            if let Some(index) = focused_token {
                area.scroll_handle()
                    .scroll_to_item(index + usize::from(has_icon));
            }
        }
        let rem = window.rem_size();
        let mut fades = scroll_edge_fades(
            &format!("{id}-track"),
            area.state.inline,
            FadeScrollbar::Hidden,
            false,
            area.is_focus_visible(window),
            (false, false),
            // Upstream's clear band, so a fade never paints into a token's focus ring.
            &rustryx_effects::edge_fade_stops(
                t.spacing.s1.to_pixels(rem) - px(1.),
                t.spacing.s6.to_pixels(rem),
            ),
            dir,
            window.supports_offscreen_layers(),
            t,
        );
        let selector = format!("{id}-track");
        let viewport = area
            .viewport(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(t.spacing.s1)
                    .self_stretch()
                    .flex_1()
                    .min_w_0(),
            )
            .debug_selector(move || selector)
            // A scrollbar would take most of the row; the fades show the overflow.
            .scrollbar_none()
            .children(lane)
            .child(visible_box(area.scroll_handle()).children(std::mem::take(&mut fades.edges)));
        fades.mask(viewport).into_any_element()
    }

    /// FR16-FR17: the sheet presentation's field: a trigger showing the
    /// tokens (display-only) or the placeholder, and the sheet.
    fn render_sheet_field(
        mut self,
        disabled: bool,
        size: ElementSize,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let id = self.base.id.clone();
        let dir = use_direction(cx);
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let draft = super::sheet::draft_state::<T>(&id, window, cx);
        let open = draft.read(cx).open;
        let status = self.status.as_ref().map(|s| s.status);
        let on_open: ActFn = {
            let (draft, value) = (draft.clone(), self.value.clone());
            Rc::new(move |window: &mut Window, cx: &mut App| {
                if !disabled {
                    super::sheet::open_sheet(&draft, &value, window, cx);
                }
            })
        };
        let content: AnyElement = if self.value.is_empty() {
            div()
                .text_color(t.color.text_secondary)
                .child(
                    self.base
                        .placeholder
                        .clone()
                        .unwrap_or_else(|| i18n(cx).t("@astryx.tokenizer.searchPlaceholder")),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_wrap()
                .gap(t.spacing.s1)
                .children(self.value.iter().map(|item| {
                    match &self.render_token {
                        Some(f) => {
                            let act: ActFn = Rc::new(|_, _| {});
                            f(item, act, window, cx)
                        }
                        None => Token::new(
                            ElementId::Name(format!("{id}-sheet-token-{}", item.id()).into()),
                            item.label(),
                        )
                        .into_any_element(),
                    }
                }))
                .into_any_element()
        };
        let chrome = Chrome {
            id: ElementId::Name(format!("{id}-control").into()),
            target: "tokenizer",
            variant: None,
            size,
            status,
            disabled,
            read_only: false,
            focused: false,
            focus_visible: false,
            row: false,
            place: None,
            dir,
        };
        let mut surface = control_box(&chrome, &theme, window, cx)
            .items_center()
            .gap(t.spacing.s1)
            .px(t.spacing.s1)
            .py(t.spacing.s1.to_pixels(rem) - px(1.))
            .min_h(crate::button::element_height(size, t));
        {
            let open = on_open.clone();
            surface = surface.on_click(move |_, window, cx| open(window, cx));
        }
        surface = surface.child(super::sheet::SheetTrigger {
            id: ElementId::Name(format!("{id}-trigger").into()),
            label: self.label.clone(),
            content,
            disabled,
            open,
            on_open: on_open.clone(),
        });
        if let Some(content) = self.end_content.take() {
            surface = surface.child(
                div()
                    .flex_none()
                    .ml_auto()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(content),
            );
        }
        if let Some(status) = status.filter(|_| self.status_placement != StatusPlacement::Detached)
        {
            let message = self.status.as_ref().and_then(|s| s.message.clone());
            surface = surface.child(status_indicator(
                &id,
                status,
                self.status_placement,
                message.as_ref(),
                &theme,
                window,
                cx,
            ));
        }
        let sheet = super::sheet::sheet(
            super::sheet::SheetProps {
                id: id.clone(),
                label: self.label.clone(),
                source: self.base.source.clone(),
                max_entries: self.max_entries,
                create: self.create.clone(),
                max_menu_items: self.base.max_menu_items,
                min_query_length: self.base.min_query_length,
                debounce: self.base.debounce,
                empty_search_text: self.base.empty_search_text.clone(),
                on_change: self.on_change.clone(),
                on_change_query: self.base.on_change_query.clone(),
                disabled,
                opening_query: self.opening_query.clone(),
            },
            window,
            cx,
        );
        // FR31: opened as it mounts, no trigger; the owner hears the close.
        if let Some(on_close) = self.sheet_on_mount.clone() {
            let mounted = window.use_keyed_state(
                ElementId::Name(format!("{id}-sheet-mounted").into()),
                cx,
                |_, _| false,
            );
            if !*mounted.read(cx) {
                mounted.update(cx, |m, _| *m = true);
                let (draft, value) = (draft.clone(), self.value.clone());
                window.defer(cx, move |window, cx| {
                    super::sheet::open_sheet(&draft, &value, window, cx)
                });
            } else if !open {
                window.defer(cx, move |window, cx| on_close(window, cx));
            }
            return sheet;
        }
        let control = crate::overlay::disabled_reason(
            &id,
            disabled,
            self.disabled_message.as_ref(),
            false,
            surface,
        );
        let mut field = Field::new(id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(disabled)
            .optional(self.optional)
            .required(self.required)
            .status_placement(self.status_placement);
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
        div()
            .display_contents()
            .child(field.child(control))
            .child(sheet)
            .into_any_element()
    }
}
