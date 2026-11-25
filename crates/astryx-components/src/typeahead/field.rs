//! Typeahead, upstream `Typeahead/Typeahead.tsx`: BaseTypeahead in a field.
//!
//! - The input surface holds the field, or, with a value and not editing, a
//!   Token over the collapsed field; the Token is the tab stop, named by the
//!   label and the value (FR1, FR8).
//! - Clicking the Token edits: the field shows the value's label, selected,
//!   and searches it. Leaving without a pick restores the Token; Escape with
//!   the list hidden restores it and focuses it (FR8).
//! - Backspace or Delete on the Token clears (with `clear`) or edits
//!   (FR12). The clear button reports `None` and announces it.
//! - The end lane: the busy spinner, the clear button, the status glyph
//!   (FR15).

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{
    ElementSize, Politeness, announce, is_focus_visible, provide, resolve_size,
};
use astryx_theme::{current, scope};
use gpui::{
    AnyElement, App, ElementId, FocusHandle, Focusable, IntoElement, KeyDownEvent, Length,
    ParentElement, RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, div,
    prelude::*, px,
};

use super::base::{BaseTypeahead, TypeaheadController, TypeaheadMenu};
use crate::button::{ButtonVariant, IconButton};
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconColor, IconDefaultSize, IconSize};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::input::group::{group_context, group_disabled, group_place, inside_member};
use crate::search::{SearchSource, Searchable};
use crate::status::{Spinner, SpinnerSize};
use crate::token::Token;

type ChangeFn<T> = Rc<dyn Fn(Option<T>, &mut Window, &mut App)>;
type ActFn = Rc<dyn Fn(&mut Window, &mut App)>;
type TrailingKeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;

/// Search-as-you-type for one item, upstream `Typeahead`.
///
/// ```ignore
/// Typeahead::new("assignee", "Assignee", static_source(users))
///     .value(assignee.clone())
///     .on_change(|user, _, _| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct Typeahead<T: Searchable> {
    base: BaseTypeahead<T>,
    label: SharedString,
    on_change: Option<ChangeFn<T>>,
    on_key_down: Option<TrailingKeyFn>,
    label_hidden: bool,
    description: Option<SharedString>,
    required: bool,
    optional: bool,
    status: Option<InputStatus>,
    status_placement: StatusPlacement,
    label_tooltip: Option<SharedString>,
    start_icon: Option<AnyElement>,
    clear: bool,
    disabled_message: Option<SharedString>,
    size: Option<ElementSize>,
    width: Option<Length>,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl<T: Searchable> Typeahead<T> {
    /// Upstream `label` and `searchSource`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        source: impl SearchSource<T>,
    ) -> Self {
        Self::from_base(BaseTypeahead::new(id, source), label)
    }

    /// A source kept between renders (see [`BaseTypeahead::from_rc`]).
    pub fn from_rc(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        source: Rc<dyn SearchSource<T>>,
    ) -> Self {
        Self::from_base(BaseTypeahead::from_rc(id, source), label)
    }

    fn from_base(base: BaseTypeahead<T>, label: impl Into<SharedString>) -> Self {
        Typeahead {
            base,
            label: label.into(),
            on_change: None,
            on_key_down: None,
            label_hidden: false,
            description: None,
            required: false,
            optional: false,
            status: None,
            status_placement: StatusPlacement::Attached,
            label_tooltip: None,
            start_icon: None,
            clear: true,
            disabled_message: None,
            size: None,
            width: None,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    /// Names the source: a new key is a new source.
    pub fn source_key(mut self, key: impl std::hash::Hash) -> Self {
        self.base = self.base.source_key(key);
        self
    }

    /// Upstream `value`: the selected item.
    pub fn value(mut self, value: Option<T>) -> Self {
        self.base.value = value;
        self
    }

    /// Upstream `onChange`: the picked item, or `None` after a clear.
    pub fn on_change(mut self, f: impl Fn(Option<T>, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
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

    /// Upstream `placeholder`.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.base.placeholder = Some(placeholder.into());
        self
    }

    /// Upstream `hasEntriesOnFocus`.
    pub fn entries_on_focus(mut self, on: bool) -> Self {
        self.base.entries_on_focus = on;
        self
    }

    /// Upstream `maxMenuItems`.
    pub fn max_menu_items(mut self, n: usize) -> Self {
        self.base = self.base.max_menu_items(n);
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
    /// BaseTypeahead FR18's package-internal opening query.
    pub(crate) fn opening_query(mut self, query: impl Into<SharedString>) -> Self {
        self.base = self.base.opening_query(query);
        self
    }

    pub fn debounce(mut self, debounce: Duration) -> Self {
        self.base.debounce = debounce;
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

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.base = self.base.on_open_change(f);
        self
    }

    /// Upstream `menu`.
    pub fn menu(mut self, menu: TypeaheadMenu) -> Self {
        self.base.menu = menu;
        self
    }

    /// Upstream `handleRef`.
    pub fn controller(mut self, controller: TypeaheadController) -> Self {
        self.base.controller = Some(controller);
        self
    }

    /// Upstream `onKeyDown`: after the built-in keys, once per key.
    pub fn on_key_down(
        mut self,
        f: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_key_down = Some(Rc::new(f));
        self
    }

    /// Upstream `isDisabled`: still a tab stop, read-only, opens nothing.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.base.focusable_disabled = disabled;
        self
    }

    astryx_primitives::setters! {
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `statusVariant`.
        status_placement: StatusPlacement,
        /// Upstream `labelTooltip`.
        ?label_tooltip: SharedString,
        /// Upstream `hasClear`: on by default.
        !clear,
        /// Upstream `disabledMessage`.
        ?disabled_message: SharedString,
        /// Upstream `size`.
        ?size: ElementSize,
        /// Upstream `width`: the whole field.
        ?width: Length,
    }

    /// Upstream `startIcon`.
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `controlXstyle`: the input surface.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl<T: Searchable> Styled for Typeahead<T> {
    /// Upstream `containerXstyle`: the field.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Typeahead's own state.
#[derive(Default)]
struct FieldState {
    editing: bool,
    /// Focus the token next frame (after a pick).
    focus_token: bool,
}

impl<T: Searchable> RenderOnce for Typeahead<T> {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.base.id.clone();
        let disabled = self.base.focusable_disabled || group_disabled(cx);
        self.base.focusable_disabled = disabled;
        let place = group_place(cx);
        let group = place.and_then(|_| group_context(cx));
        let dir = use_direction(cx);
        let theme = current(window, cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let t18 = i18n(cx);
        let rem = window.rem_size();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id}-typeahead-field").into()),
            cx,
            |_, _| FieldState::default(),
        );
        let token_focus: FocusHandle = astryx_primitives::control_focus(
            &ElementId::Name(format!("{id}-token").into()),
            window,
            cx,
        );
        let engine = super::base::engine::<T>(&id, window, cx);
        let editor = window.use_keyed_state(
            ElementId::Name(format!("{id}-typeahead-input").into()),
            cx,
            |window, cx| {
                astryx_primitives::editing::TextEditState::reporting("", false, window, cx)
            },
        );
        let input_focus = editor.read(cx).focus_handle(cx);
        let value = self.base.value.clone();
        // Leaving the field without a pick restores the Token.
        if state.read(cx).editing
            && !input_focus.is_focused(window)
            && !token_focus.is_focused(window)
            && !engine.read(cx).open
        {
            state.update(cx, |s, _| s.editing = false);
            editor.update(cx, |e, cx| e.set_text("", cx));
        }
        if state.read(cx).focus_token {
            state.update(cx, |s, _| s.focus_token = false);
            let focus = token_focus.clone();
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        }
        let editing = state.read(cx).editing;
        let show_token = value.is_some() && !editing;

        // Edit mode: the value's label, selected, and searched.
        let enter_edit: ActFn = {
            let (state, editor, engine, focus) = (
                state.clone(),
                editor.clone(),
                engine.clone(),
                input_focus.clone(),
            );
            let value = value.clone();
            let report = self.base.on_change_query.clone();
            Rc::new(move |window, cx| {
                let Some(value) = &value else { return };
                if disabled {
                    return;
                }
                state.update(cx, |s, _| s.editing = true);
                let label = value.label();
                if let Some(f) = &report {
                    f(label.clone(), window, cx);
                }
                editor.update(cx, |e, cx| e.set_text(&label, cx));
                engine.update(cx, |e, _| e.query = label.to_string());
                window.focus(&focus, cx);
                let editor = editor.clone();
                window.defer(cx, move |window, cx| {
                    editor.update(cx, |e, cx| e.select_all(window, cx));
                });
                window.refresh();
            })
        };
        // A clear: None, announced, and focus in the empty field.
        let clear: ActFn = {
            let (state, editor, focus, report) = (
                state.clone(),
                editor.clone(),
                input_focus.clone(),
                self.on_change.clone(),
            );
            let label = value.as_ref().map(|v| v.label());
            Rc::new(move |window, cx| {
                editor.update(cx, |e, cx| e.set_text("", cx));
                state.update(cx, |s, _| s.editing = false);
                if let Some(f) = &report {
                    f(None, window, cx);
                }
                let text = i18n(cx).format(
                    "@astryx.typeahead.selectionCleared",
                    &[("label", label.clone().unwrap_or_default().as_ref().into())],
                );
                announce(text, Politeness::Polite, window, cx);
                window.focus(&focus, cx);
            })
        };
        // A pick: report, restore the Token and focus it.
        {
            let (state, report) = (state.clone(), self.on_change.clone());
            self.base.on_change = Some(Rc::new(
                move |item: Option<T>, window: &mut Window, cx: &mut App| {
                    state.update(cx, |s, _| {
                        s.editing = false;
                        s.focus_token = item.is_some();
                    });
                    if let Some(f) = &report {
                        f(item, window, cx);
                    }
                    window.refresh();
                },
            ));
        }
        // FR8: Escape in edit mode with the list hidden restores the Token.
        {
            let (state, engine, token) = (state.clone(), engine.clone(), token_focus.clone());
            let editor = editor.clone();
            self.base.on_key_down = Some(Rc::new(
                move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                    if event.keystroke.key == "escape"
                        && state.read(cx).editing
                        && !engine.read(cx).open
                    {
                        state.update(cx, |s, _| s.editing = false);
                        editor.update(cx, |e, cx| e.set_text("", cx));
                        window.focus(&token, cx);
                        return true;
                    }
                    false
                },
            ));
        }
        self.base.trailing_key = self.on_key_down.clone().filter(|_| !disabled);
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

        let parts = self.base.parts(window, cx);
        let focused = input_focus.is_focused(window) || token_focus.is_focused(window);
        let chrome = Chrome {
            id: ElementId::Name(format!("{id}-control").into()),
            target: "typeahead",
            variant: None,
            size,
            status,
            disabled,
            read_only: false,
            focused,
            focus_visible: is_focus_visible(&input_focus, window)
                || is_focus_visible(&token_focus, window),
            row: false,
            place,
            dir,
        };
        let part_override = theme.for_part("typeahead", &chrome.props());
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let mut surface = control_box(&chrome, &part, window, cx)
            .items_center()
            .gap(t.spacing.s1)
            .py(t.spacing.s1.to_pixels(rem) - px(1.))
            .cursor_text();
        if place.is_none() {
            surface = surface.min_h(crate::button::element_height(size, t));
        }
        surface.style().refine(&self.control_style);
        {
            let (focus, enter) = (input_focus.clone(), enter_edit.clone());
            surface = surface.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                if show_token {
                    enter(window, cx);
                } else {
                    window.focus(&focus, cx);
                }
            });
        }
        if let Some(icon) = self.start_icon.take() {
            surface = surface.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .text_color(t.color.icon_secondary)
                    .child(provide(IconDefaultSize(IconSize::Sm), icon)),
            );
        }
        // The content lane: the field, and the Token over it.
        let mut lane = div()
            .relative()
            .flex()
            .items_center()
            .self_stretch()
            .flex_1()
            .min_w_0();
        // The collapsed field leaves the tab order: the Token is the stop.
        lane = if show_token {
            lane.child(
                div()
                    .flex_1()
                    .min_w(px(60.))
                    .h(t.text.body.size.to_pixels(rem) * t.text.body.leading),
            )
        } else {
            lane.child(parts.input)
        };
        if let Some(value) = &value
            && show_token
        {
            let token_name: SharedString = format!("{} {}", name, value.label()).into();
            let (enter, clear_token) = (enter_edit.clone(), clear.clone());
            let enter_key = enter_edit.clone();
            let has_clear = self.clear;
            let ring = is_focus_visible(&token_focus, window);
            let mut token = div()
                .id(ElementId::Name(format!("{id}-token").into()))
                .debug_selector({
                    let s = format!("{id}-token");
                    move || s
                })
                .absolute()
                .top_0()
                .bottom_0()
                .left(-(t.spacing.s1.to_pixels(rem) - px(1.)))
                .flex()
                .items_center()
                .track_focus(&token_focus)
                .role(Role::Button)
                .aria_label(token_name)
                .aria_disabled(disabled)
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    enter(window, cx);
                })
                .on_key_down(move |event, window, cx| {
                    if disabled {
                        return;
                    }
                    // FR12: Backspace or Delete clears, or edits.
                    if matches!(event.keystroke.key.as_str(), "backspace" | "delete") {
                        cx.stop_propagation();
                        if has_clear {
                            clear_token(window, cx);
                        } else {
                            enter_key(window, cx);
                        }
                    }
                })
                .child(Token::new(
                    ElementId::Name(format!("{id}-token-chip").into()),
                    value.label(),
                ));
            if ring {
                token = token.child(focus_ring(t, t.radius.element.to_pixels(rem), rem));
            }
            lane = lane.child(token);
        }
        surface = surface.child(lane);
        // The end lane.
        let show_clear = self.clear && (value.is_some() || editing) && !disabled;
        if parts.busy || show_clear || status.is_some() {
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
            if show_clear && value.is_some() {
                let clear = clear.clone();
                let label = t18.format(
                    "@astryx.typeahead.clearSelection",
                    &[("label", self.label.as_ref().into())],
                );
                end = end.child(
                    div()
                        .debug_selector({
                            let s = format!("{id}-clear");
                            move || s
                        })
                        .child(
                            IconButton::new(
                                ElementId::Name(format!("{id}-clear").into()),
                                label,
                                Icon::new("close")
                                    .size(IconSize::Sm)
                                    .color(IconColor::Secondary),
                            )
                            .variant(ButtonVariant::Ghost)
                            .size(ElementSize::Sm)
                            .tooltip(t18.t("@astryx.inputClearButton.tooltip"))
                            .h(px(20.))
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                clear(window, cx)
                            }),
                        ),
                );
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
