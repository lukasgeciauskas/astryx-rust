//! TextInput. Upstream: `TextInput/`.
//!
//! The field is controlled by `value`, as upstream. Its editing state (the
//! caret, selection and undo history) lives in a keyed entity that follows
//! `value` each render.

use std::rc::Rc;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::editing::{
    EditorStyle, FieldSettings, KeyDownFn, KeyboardHints, TextEditState, text_editor,
};
use astryx_primitives::{
    ElementSize, is_focus_visible, prefers_reduced_motion, provide, resolve_size,
};
use astryx_theme::{ThemeStyled, current, scope};
use gpui::{
    AnyElement, App, ElementId, Focusable, KeyDownEvent, Length, Role, SharedString,
    StyleRefinement, Styled, Task, Window, div, prelude::*, px,
};

use super::control::{Chrome, control_box, described_by, status_indicator};
use super::group::{group_context, group_disabled, group_place, inside_member};
use crate::button::{ButtonVariant, IconButton};
use crate::choice::action::{ActionFn, ChangeFn, Optimistic};
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::icon::{Icon, IconColor, IconDefaultSize, IconSize};
use crate::status::{Spinner, SpinnerSize};

/// Upstream `type`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputType {
    #[default]
    Text,
    /// Shows a bullet per character and never copies its text.
    Password,
    Email,
    /// A search field (AR6).
    Search,
    Tel,
    Url,
}

impl InputType {
    pub fn as_str(self) -> &'static str {
        match self {
            InputType::Text => "text",
            InputType::Password => "password",
            InputType::Email => "email",
            InputType::Search => "search",
            InputType::Tel => "tel",
            InputType::Url => "url",
        }
    }

    fn role(self) -> Role {
        match self {
            InputType::Text => Role::TextInput,
            InputType::Password => Role::PasswordInput,
            InputType::Email => Role::EmailInput,
            InputType::Search => Role::SearchInput,
            InputType::Tel => Role::PhoneNumberInput,
            InputType::Url => Role::UrlInput,
        }
    }
}

type EnterFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// A single-line text field with its label, description and validation
/// status, upstream `TextInput`.
#[derive(IntoElement)]
pub struct TextInput {
    id: ElementId,
    label: SharedString,
    value: SharedString,
    input_type: InputType,
    on_change: Option<ChangeFn<SharedString>>,
    change_action: Option<ActionFn<SharedString>>,
    size: Option<ElementSize>,
    label_hidden: bool,
    description: Option<SharedString>,
    optional: bool,
    required: bool,
    on_enter: Option<EnterFn>,
    on_key_down: Option<KeyDownFn>,
    disabled: bool,
    read_only: bool,
    disabled_message: Option<SharedString>,
    loading: bool,
    placeholder: Option<SharedString>,
    label_tooltip: Option<SharedString>,
    start_icon: Option<AnyElement>,
    status: Option<InputStatus>,
    status_placement: StatusPlacement,
    clear: bool,
    auto_focus: bool,
    spell_check: Option<bool>,
    keyboard: KeyboardHints,
    width: Option<Length>,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl TextInput {
    /// Upstream `label` (always rendered for assistive technology) and the
    /// controlled `value`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        TextInput {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            input_type: InputType::Text,
            on_change: None,
            change_action: None,
            size: None,
            label_hidden: false,
            description: None,
            optional: false,
            required: false,
            on_enter: None,
            on_key_down: None,
            disabled: false,
            read_only: false,
            disabled_message: None,
            loading: false,
            placeholder: None,
            label_tooltip: None,
            start_icon: None,
            status: None,
            status_placement: StatusPlacement::Attached,
            clear: false,
            auto_focus: false,
            spell_check: None,
            keyboard: KeyboardHints::default(),
            width: None,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `type`.
        input_type: InputType,
        /// Upstream `size`: explicit, else the nearest size provider, else `md`.
        ?size: ElementSize,
        /// Upstream `isLabelHidden`: hidden visually, still the field's name.
        !label_hidden,
        /// Upstream `description`: between the label and the control.
        ?description: SharedString,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isDisabled`: dimmed and read-only, but still a tab stop.
        !disabled,
        /// Upstream `isReadOnly`: focusable and selectable, never changed.
        !read_only,
        /// Upstream `disabledMessage`: why the field is disabled. It
        /// shown in a tooltip on hover and focus.
        ?disabled_message: SharedString,
        /// Upstream `isLoading`: a spinner, and the field reports busy.
        !loading,
        /// Upstream `placeholder`.
        ?placeholder: SharedString,
        /// Upstream `labelTooltip`: help in an InfoTip after the label.
        ?label_tooltip: SharedString,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `statusVariant`.
        status_placement: StatusPlacement,
        /// Upstream `hasClear`: a clear button while there is a value.
        !clear,
        /// Upstream `hasAutoFocus`: focuses the field when it first renders.
        !auto_focus,
        /// Upstream `width`: the whole field.
        ?width: Length,
    }

    /// Upstream `onChange`: the new text after each edit.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`. The field shows
    /// the new text and is busy until the task settles, then shows `value`.
    pub fn change_action(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `hasSpellCheck`: underlines misspelled words once the field
    /// has had focus. On unless turned off; never in a password field.
    pub fn spell_check(mut self, spell_check: bool) -> Self {
        self.spell_check = Some(spell_check);
        self
    }

    /// Upstream `inputMode`: which touch keyboard the field asks for,
    /// without changing what it accepts.
    pub fn input_mode(mut self, mode: gpui::TextInputMode) -> Self {
        self.keyboard.input_mode = Some(mode);
        self
    }

    /// Upstream `enterKeyHint`: what a touch keyboard's Return key says.
    pub fn enter_key_hint(mut self, hint: gpui::TextInputAction) -> Self {
        self.keyboard.enter_key_hint = Some(hint);
        self
    }

    /// Upstream `autoCapitalize`: when a touch keyboard capitalizes.
    pub fn auto_capitalize(mut self, capitalize: gpui::Autocapitalize) -> Self {
        self.keyboard.autocapitalize = Some(capitalize);
        self
    }

    /// Upstream `hasAutoCorrect`: whether a touch keyboard corrects words.
    pub fn auto_correct(mut self, correct: bool) -> Self {
        self.keyboard.autocorrect = Some(correct);
        self
    }

    /// Upstream `onEnter`: Enter, except when it commits an IME composition.
    pub fn on_enter(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_enter = Some(Rc::new(f));
        self
    }

    /// Upstream `onKeyDown`: every key, before the field handles it.
    pub fn on_key_down(
        mut self,
        f: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_key_down = Some(Rc::new(f));
        self
    }

    /// Upstream `controlXstyle`: styles the bordered control box, after its
    /// own and the theme's styles (`Styled` styles the whole field).
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }

    /// Upstream `startIcon`: drawn small and secondary unless it sets its own
    /// size and color.
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }
}

impl Styled for TextInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The text selection's highlight: the accent at the strength of the hover
/// ring (`--shadow-inset-hover`).
pub(crate) fn selection_color(t: &astryx_theme::Tokens) -> gpui::Hsla {
    let mut color = t.color.accent;
    color.a *= 0.3;
    color
}

impl RenderOnce for TextInput {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // FR6: an enclosing InputGroup's disabled state, never re-enabling.
        self.disabled |= group_disabled(cx);
        let place = group_place(cx);
        let group = place.and_then(|_| group_context(cx));
        let dir = use_direction(cx);
        let theme = current(window, cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let i18n = i18n(cx);

        // FR3, FR4: onChange, then the action with the proposed text shown
        // and the field busy until it settles.
        let optimistic = Optimistic::<SharedString>::new(&self.id, window, cx);
        let shown = optimistic.shown(self.value.clone(), cx);
        let busy = self.loading || shown != self.value;
        optimistic.announce_busy(busy, i18n.t("@astryx.spinner.loading"), window, cx);
        let change: ChangeFn<SharedString> =
            Rc::new(optimistic.change(self.on_change, self.change_action));

        let masked = self.input_type == InputType::Password;
        let editable = !self.disabled && !self.read_only;
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-editor", self.id).into()),
            cx,
            |window, cx| TextEditState::reporting(shown.to_string(), false, window, cx),
        );
        state.update(cx, |s, cx| {
            s.set_text(&shown, cx);
            s.set_settings(FieldSettings {
                disabled: self.disabled,
                read_only: self.read_only,
                masked,
                filter: None,
                spell_check: self.spell_check.unwrap_or(true),
                // The type picks the keyboard, as a browser's does.
                keyboard: KeyboardHints {
                    input_mode: self.keyboard.input_mode.or(match self.input_type {
                        InputType::Email => Some(gpui::TextInputMode::Email),
                        InputType::Url => Some(gpui::TextInputMode::Url),
                        InputType::Tel => Some(gpui::TextInputMode::Tel),
                        InputType::Search => Some(gpui::TextInputMode::Search),
                        InputType::Text | InputType::Password => None,
                    }),
                    ..self.keyboard
                },
            });
            s.on_change = editable.then(|| {
                let change = change.clone();
                Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                    change(text.into(), window, cx)
                }) as astryx_primitives::editing::ChangeFn
            });
            s.on_enter = self.on_enter.clone().filter(|_| !self.disabled);
            // FR10: every key reaches onKeyDown first; a disabled field runs
            // no caller handlers (FR6).
            s.on_key_down = self.on_key_down.clone().filter(|_| !self.disabled);
        });
        let focus = state.read(cx).focus_handle(cx);
        if self.auto_focus {
            let done = window.use_keyed_state(
                ElementId::Name(format!("{:?}-autofocused", self.id).into()),
                cx,
                |_, _| false,
            );
            if !*done.read(cx) {
                done.update(cx, |done, _| *done = true);
                let focus = focus.clone();
                window.defer(cx, move |window, cx| window.focus(&focus, cx));
            }
        }

        let status = self.status.as_ref().map(|s| s.status);
        let chrome = Chrome {
            id: ElementId::Name(format!("{}-control", self.id).into()),
            target: "text-input",
            variant: None,
            size,
            status,
            disabled: self.disabled,
            read_only: self.read_only,
            focused: focus.is_focused(window),
            focus_visible: is_focus_visible(&focus, window),
            row: true,
            place,
            dir,
        };
        let part_override = theme.for_part("text-input", &chrome.props());
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let mut control = control_box(&chrome, &part, window, cx);
        control.style().refine(&self.control_style);
        // FR12: a click on the box outside the text focuses the field.
        if !self.disabled {
            let focus = focus.clone();
            control = control.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            });
        }
        // The browser's text-field context menu.
        control = super::field_menu::with_field_menu(
            control,
            &self.id,
            &state,
            &focus,
            self.disabled,
            window,
            cx,
        );
        if let Some(icon) = self.start_icon {
            control = control.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .text_color(t.color.icon_secondary)
                    .child(provide(IconDefaultSize(IconSize::Sm), icon)),
            );
        }

        let described = described_by(
            self.description.as_ref(),
            self.status.as_ref(),
            self.disabled
                .then_some(self.disabled_message.as_ref())
                .flatten(),
            &i18n,
        );
        // FR2: a grouped field is named by the group and itself, and
        // described by the group first.
        let (name, described) = match &group {
            Some(group) => (
                SharedString::from(format!("{} {}", group.label, self.label)),
                group.described.iter().cloned().chain(described).collect(),
            ),
            None => (self.label.clone(), described),
        };

        let style = EditorStyle {
            text_color: t.color.text_primary,
            placeholder: self.placeholder.clone(),
            placeholder_color: t.color.text_secondary,
            selection_color: selection_color(t),
            caret_color: t.color.text_primary,
            multiline: false,
            min_lines: 1,
            max_lines: Some(1),
            height: None,
            reduced_motion: prefers_reduced_motion(cx),
            spelling_color: t.color.error,
        };
        let mut editor = text_editor("editor", &state, style, cx)
            .flex_1()
            .min_w_0()
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .role(self.input_type.role())
            .aria_label(name)
            .aria_disabled(self.disabled)
            .aria_read_only(!editable)
            .aria_required(crate::field::resolve(self.required, self.optional, cx).required)
            .aria_invalid(status == Some(StatusType::Error))
            .aria_busy(busy);
        if !masked {
            editor = editor.aria_value(shown.clone());
        }
        if let Some(placeholder) = &self.placeholder {
            editor = editor.aria_placeholder(placeholder.clone());
        }
        if !described.is_empty() {
            editor = editor.aria_description(SharedString::from(described.join(" ")));
        }
        control = control.child(editor);

        // End controls, in flow after the text (FR15): the spinner inward of
        // the clear button, so the button keeps its place when busy.
        if busy {
            control = control.child(Spinner::new("spinner").size(SpinnerSize::Sm).decorative());
        }
        if self.clear && !self.value.is_empty() && editable {
            let label = i18n.format(
                "@astryx.textInput.clearLabel",
                &[("label", self.label.as_ref().into())],
            );
            let clear = change.clone();
            let focus = focus.clone();
            control = control.child(
                IconButton::new(
                    "clear",
                    label,
                    Icon::new("close")
                        .size(IconSize::Sm)
                        .color(IconColor::Secondary),
                )
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
                .tooltip(i18n.t("@astryx.inputClearButton.tooltip"))
                .h(px(20.))
                // FR5: commit '' through the change pipeline, then focus the field.
                .on_click(move |_, window, cx| {
                    clear(SharedString::default(), window, cx);
                    window.focus(&focus, cx);
                }),
            );
        }
        if let Some(status) = status
            && self.status_placement != StatusPlacement::Detached
            && place.is_none()
        {
            let message = self.status.as_ref().and_then(|s| s.message.clone());
            control = control.child(status_indicator(
                &self.id,
                status,
                self.status_placement,
                message.as_ref(),
                &part,
                window,
                cx,
            ));
        }

        let control = match part_override {
            Some(part) => scope(part, control).into_any_element(),
            None => control.into_any_element(),
        };
        // FR7: a disabled field's reason, on hover and focus.
        let control = crate::overlay::disabled_reason(
            &self.id,
            self.disabled,
            self.disabled_message.as_ref(),
            true,
            control,
        );
        // In a group the control is the member: no Field of its own.
        if place.is_some() {
            return inside_member(control).into_any_element();
        }
        let mut field = Field::new(self.id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(self.disabled)
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
