//! ChatComposer, upstream `Chat/ChatComposer.tsx`: the message-entry shell. It holds
//! the draft (controlled or not), lays out the drawer, header, input, footer and status
//! around a default or custom input, and provides [`ChatComposerContext`] to the input
//! and the send button.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::Refineable as _;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, BoxShadow, ElementId, FocusHandle, SharedString, StyleRefinement, Window, div,
    point, px,
};
use rustryx_primitives::{Politeness, announce, is_focus_visible, provide};
use rustryx_theme::{ThemeStyled as _, current};

use super::composer_input::ChatComposerInput;
use super::context::{
    ChatActionFn, ChatComposerContext, ChatComposerInputControl, ChatDensity, ChatValueFn,
};
use super::send_button::ChatSendButton;
use crate::button::ButtonRadius;
use crate::icon::{Icon, IconSize};

/// Upstream `elevation`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatComposerElevation {
    /// Flat, with a border.
    None,
    /// Raised: low at rest, medium on hover and focus.
    #[default]
    Low,
}

impl ChatComposerElevation {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatComposerElevation::None => "none",
            ChatComposerElevation::Low => "low",
        }
    }
}

/// Upstream `ChatComposerStatus['type']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatComposerStatusType {
    Error,
    Warning,
}

impl ChatComposerStatusType {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatComposerStatusType::Error => "error",
            ChatComposerStatusType::Warning => "warning",
        }
    }
}

/// Upstream `ChatComposerStatus`: feedback above or below the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatComposerStatus {
    pub status_type: ChatComposerStatusType,
    /// Upstream `message`: without one the bar shows and nothing is announced.
    pub message: Option<SharedString>,
}

impl ChatComposerStatus {
    pub fn error(message: impl Into<SharedString>) -> Self {
        ChatComposerStatus {
            status_type: ChatComposerStatusType::Error,
            message: Some(message.into()),
        }
    }

    pub fn warning(message: impl Into<SharedString>) -> Self {
        ChatComposerStatus {
            status_type: ChatComposerStatusType::Warning,
            message: Some(message.into()),
        }
    }
}

/// Upstream `statusPosition`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatComposerStatusPosition {
    Top,
    #[default]
    Bottom,
}

/// What the composer keeps between frames.
#[derive(Default)]
struct ComposerState {
    /// The uncontrolled draft.
    value: SharedString,
    /// The status occasion last seen (FR9): its type while it has a message.
    status: Option<Option<ChatComposerStatusType>>,
    /// Whether the editor had focus last frame, and whether that focus came from the
    /// keyboard (upstream `hasKeyboardEditorFocus`, decided when focus arrives).
    editor_focused: bool,
    keyboard_focus: bool,
}

/// Upstream `ChatComposer`.
///
/// ```ignore
/// ChatComposer::new("composer", |value, _, _| send(value))
///     .stop_shown(streaming)
///     .on_stop(|_, _| stop())
/// ```
#[derive(IntoElement)]
pub struct ChatComposer {
    id: ElementId,
    on_submit: ChatValueFn,
    on_stop: Option<ChatActionFn>,
    stop_shown: bool,
    value: Option<SharedString>,
    on_change: Option<ChatValueFn>,
    placeholder: Option<SharedString>,
    disabled: bool,
    density: ChatDensity,
    elevation: ChatComposerElevation,
    drawer: Option<AnyElement>,
    header_actions: Option<AnyElement>,
    header_context: Option<AnyElement>,
    input: Option<AnyElement>,
    footer_actions: Option<AnyElement>,
    send_actions: Option<AnyElement>,
    send_button: Option<AnyElement>,
    status: Option<ChatComposerStatus>,
    status_position: ChatComposerStatusPosition,
    style: StyleRefinement,
}

impl ChatComposer {
    /// Upstream `onSubmit`: told the trimmed draft when it is sent.
    pub fn new(
        id: impl Into<ElementId>,
        on_submit: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        ChatComposer {
            id: id.into(),
            on_submit: Rc::new(on_submit),
            on_stop: None,
            stop_shown: false,
            value: None,
            on_change: None,
            placeholder: None,
            disabled: false,
            density: ChatDensity::Balanced,
            elevation: ChatComposerElevation::Low,
            drawer: None,
            header_actions: None,
            header_context: None,
            input: None,
            footer_actions: None,
            send_actions: None,
            send_button: None,
            status: None,
            status_position: ChatComposerStatusPosition::Bottom,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `isStopShown`: the stop button instead of send.
        !stop_shown,
        /// Upstream `value`: a controlled draft.
        ?value: SharedString,
        /// Upstream `placeholder`: "Type a message…" by default.
        ?placeholder: SharedString,
        /// Upstream `isDisabled`: no editing or sending; a shown Stop stays operable.
        !disabled,
        /// Upstream `density`.
        density: ChatDensity,
        /// Upstream `elevation`.
        elevation: ChatComposerElevation,
        /// Upstream `status`.
        ?status: ChatComposerStatus,
        /// Upstream `statusPosition`.
        status_position: ChatComposerStatusPosition,
    }

    /// Upstream `onStop`.
    pub fn on_stop(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_stop = Some(Rc::new(f));
        self
    }

    /// Upstream `onChange`: each new draft.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `drawer`: above the body (a ChatComposerDrawer).
    pub fn drawer(mut self, drawer: impl IntoElement) -> Self {
        self.drawer = Some(drawer.into_any_element());
        self
    }

    /// Upstream `headerActions`: the header's start (small icon buttons).
    pub fn header_actions(mut self, actions: impl IntoElement) -> Self {
        self.header_actions = Some(actions.into_any_element());
        self
    }

    /// Upstream `headerContext`: the header's end (context usage, a ProgressBar).
    pub fn header_context(mut self, context: impl IntoElement) -> Self {
        self.header_context = Some(context.into_any_element());
        self
    }

    /// Upstream `input`: replaces the default ChatComposerInput.
    pub fn input(mut self, input: impl IntoElement) -> Self {
        self.input = Some(input.into_any_element());
        self
    }

    /// Upstream `footerActions`: the footer's start (`md` buttons).
    pub fn footer_actions(mut self, actions: impl IntoElement) -> Self {
        self.footer_actions = Some(actions.into_any_element());
        self
    }

    /// Upstream `sendActions`: before the send button (`md` buttons).
    pub fn send_actions(mut self, actions: impl IntoElement) -> Self {
        self.send_actions = Some(actions.into_any_element());
        self
    }

    /// Upstream `sendButton`: replaces the default ChatSendButton.
    pub fn send_button(mut self, button: impl IntoElement) -> Self {
        self.send_button = Some(button.into_any_element());
        self
    }
}

impl Styled for ChatComposer {
    /// Upstream `xstyle`: the painted body.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ChatComposer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let i18n = rustryx_i18n::current(cx);
        let key = self.id.to_string();
        let placeholder = self
            .placeholder
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.chat.composer.placeholder"));
        let state = window.use_keyed_state(
            ElementId::Name(format!("{key}-composer").into()),
            cx,
            |_, _| ComposerState::default(),
        );
        let control = window
            .use_keyed_state(
                ElementId::Name(format!("{key}-input-control").into()),
                cx,
                |_, _| Rc::new(RefCell::new(None::<ChatComposerInputControl>)),
            )
            .read(cx)
            .clone();
        let body_focus: FocusHandle = window
            .use_keyed_state(
                ElementId::Name(format!("{key}-body-focus").into()),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();

        // FR1, ORD1, ORD2: the draft and its channel.
        let controlled = self.value.is_some();
        let current_value = self
            .value
            .clone()
            .unwrap_or_else(|| state.read(cx).value.clone());
        let update: ChatValueFn = {
            let (state, on_change) = (state.clone(), self.on_change.clone());
            Rc::new(
                move |value: SharedString, window: &mut Window, cx: &mut App| {
                    if !controlled {
                        state.update(cx, |s, _| s.value = value.clone());
                    }
                    if let Some(f) = &on_change {
                        f(value, window, cx);
                    }
                    window.refresh();
                },
            )
        };
        let submit: ChatValueFn = {
            let (update, on_submit, disabled) =
                (update.clone(), self.on_submit.clone(), self.disabled);
            Rc::new(
                move |value: SharedString, window: &mut Window, cx: &mut App| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() || disabled {
                        return;
                    }
                    on_submit(trimmed.to_string().into(), window, cx);
                    update("".into(), window, cx);
                },
            )
        };
        let can_send = !current_value.trim().is_empty() && !self.disabled;
        let context = ChatComposerContext {
            value: current_value,
            on_change: update,
            on_submit: submit,
            placeholder,
            disabled: self.disabled,
            stop_shown: self.stop_shown,
            can_send,
            on_stop: self.on_stop.clone(),
            input_control: control.clone(),
        };

        // FR9: a status announces once per occasion; one already shown when the
        // composer first draws is not announced.
        let occasion = self
            .status
            .as_ref()
            .map(|s| s.message.as_ref().map(|_| s.status_type))
            .and_then(|o| o);
        let previous = state.read(cx).status;
        if previous != Some(occasion) {
            state.update(cx, |s, _| s.status = Some(occasion));
            if let (Some(_), Some(status_type)) = (previous, occasion)
                && let Some(message) = self.status.as_ref().and_then(|s| s.message.clone())
            {
                let politeness = match status_type {
                    ChatComposerStatusType::Error => Politeness::Assertive,
                    ChatComposerStatusType::Warning => Politeness::Polite,
                };
                announce(message, politeness, window, cx);
            }
        }

        let radius = t.radius.chat.to_pixels(rem);
        let padding = match self.density {
            ChatDensity::Compact => t.spacing.s2,
            _ => t.spacing.s3,
        }
        .to_pixels(rem);
        let gap = match self.density {
            ChatDensity::Compact => t.spacing.s1,
            _ => t.spacing.s2,
        };

        let status = self.status.clone().map(|status| {
            let (bg, fg, icon) = match status.status_type {
                ChatComposerStatusType::Error => {
                    (t.color.error_muted, t.color.on_error_muted, "error")
                }
                ChatComposerStatusType::Warning => {
                    (t.color.warning_muted, t.color.on_warning_muted, "warning")
                }
            };
            let selector = format!("{key}-status");
            let mut bar = div()
                .debug_selector(move || selector)
                .relative()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .px(t.spacing.s4)
                .font_stack(&t.font.family.body)
                .text_style(&t.text.supporting)
                .bg(bg)
                .text_color(fg)
                .child(Icon::new(icon).size(IconSize::Md))
                .children(status.message.clone());
            bar = match self.status_position {
                ChatComposerStatusPosition::Top => bar
                    .pt(padding)
                    .pb(padding + radius)
                    .mb(-radius)
                    .rounded_t(radius),
                ChatComposerStatusPosition::Bottom => bar
                    .pt(padding + radius)
                    .pb(padding)
                    .mt(-radius)
                    .rounded_b(radius),
            };
            bar
        });

        // The body: the painted, rounded surface.
        let editor_focus = control
            .borrow()
            .as_ref()
            .and_then(|c| c.focus_handle.clone());
        // FR8: focus arriving in the editor by keyboard rings the body until a press in
        // it or the focus leaves; typing never adds the ring.
        let editor_focused = editor_focus.as_ref().is_some_and(|f| f.is_focused(window));
        let arrived = editor_focused && !state.read(cx).editor_focused;
        let by_keyboard = editor_focus
            .as_ref()
            .is_some_and(|f| is_focus_visible(f, window));
        state.update(cx, |s, _| {
            if arrived {
                s.keyboard_focus = by_keyboard;
            }
            if !editor_focused {
                s.keyboard_focus = false;
            }
            s.editor_focused = editor_focused;
        });
        let keyboard_ring = state.read(cx).keyboard_focus;
        let focus_within = body_focus.contains_focused(window, cx);
        let border = t.border.width.to_pixels(rem);
        let body_key = format!("{key}-body");
        let mut body = div()
            .id(ElementId::Name(body_key.clone().into()))
            .debug_selector(move || body_key)
            .track_focus(&body_focus)
            .relative()
            .flex()
            .flex_col()
            .gap(gap)
            .rounded(radius)
            .bg(t.color.background_popover)
            .cursor(if self.disabled {
                gpui::CursorStyle::Arrow
            } else {
                gpui::CursorStyle::IBeam
            });
        body = match self.elevation {
            ChatComposerElevation::Low => {
                let shadow = if focus_within {
                    &t.shadow.med
                } else {
                    &t.shadow.low
                };
                let hover = t.shadow.med.clone();
                body = body.p(padding).shadow_token(shadow);
                if self.disabled {
                    body
                } else {
                    body.hover(move |s| s.shadow_token(&hover))
                }
            }
            ChatComposerElevation::None => {
                let ring = |color: gpui::Hsla| {
                    vec![BoxShadow {
                        color,
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(2.),
                        inset: true,
                    }]
                };
                body = body
                    .p(padding - border)
                    .border(border)
                    .border_color(if focus_within {
                        t.color.accent
                    } else {
                        t.color.border_emphasized
                    });
                if focus_within {
                    body.shadow(ring(t.color.accent_muted))
                } else if self.disabled {
                    body
                } else {
                    let mut hover = t.color.border_emphasized;
                    hover.a *= 0.3;
                    let hover = ring(hover);
                    body.hover(move |s| s.shadow(hover))
                }
            }
        };
        theme.refine(
            body.style(),
            "chat-composer",
            &[
                ("density", self.density.as_str()),
                ("elevation", self.elevation.as_str()),
            ],
            None,
        );
        body.style().refine(&self.style);
        // A press in the body is pointer focus: no ring.
        {
            let state = state.clone();
            body = body.capture_any_mouse_down(move |_, window, cx| {
                if state.read(cx).keyboard_focus {
                    state.update(cx, |s, _| s.keyboard_focus = false);
                    window.refresh();
                }
            });
        }
        // FR8: a click on the body's own space focuses the input after its draft.
        if !self.disabled {
            let control = control.clone();
            body = body.on_click(move |_, window, cx| {
                let focus = control.borrow().as_ref().map(|c| c.focus.clone());
                if let Some(focus) = focus {
                    focus(window, cx);
                }
            });
        }
        if keyboard_ring {
            let ring = format!("{key}-ring");
            body =
                body.child(crate::focus::focus_ring(t, radius, rem).debug_selector(move || ring));
        }
        if self.header_actions.is_some() || self.header_context.is_some() {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(t.spacing.s2)
                    .min_h(px(28.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.spacing.s1)
                            .children(self.header_actions),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.spacing.s2)
                            .ml_auto()
                            .text_style(&t.text.supporting)
                            .text_color(t.color.text_secondary)
                            .children(self.header_context),
                    ),
            );
        }
        let input = self.input.unwrap_or_else(|| {
            ChatComposerInput::new(ElementId::Name(format!("{key}-input").into()))
                .into_any_element()
        });
        body = body.child(div().flex().flex_col().child(input));
        // A disabled composer takes no presses, except a shown Stop (FR4): the cover
        // goes under the footer's end then.
        let interruptible = self.disabled && self.stop_shown;
        let cover = || {
            div()
                .id("cover")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .cursor_default()
        };
        if interruptible {
            body = body.child(cover());
        }
        let footer_left = div()
            .relative()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.s1)
            .min_w_0()
            .children(self.footer_actions);
        let footer_right = div()
            .flex()
            .items_center()
            .gap(t.spacing.s1)
            .flex_shrink_0()
            .children(self.send_actions)
            .child({
                let selector = format!("{key}-send");
                div().debug_selector(move || selector).flex().child(
                    self.send_button.unwrap_or_else(|| {
                        ChatSendButton::new(ElementId::Name(format!("{key}-send").into()))
                            .into_any_element()
                    }),
                )
            });
        let footer_key = format!("{key}-footer");
        body = body.child(
            div()
                .debug_selector(move || footer_key)
                .flex()
                .items_center()
                .justify_between()
                .gap(t.spacing.s2)
                .min_h(px(32.))
                .child(if interruptible {
                    footer_left.child(cover())
                } else {
                    footer_left
                })
                .child(footer_right),
        );
        if self.disabled && !interruptible {
            body = body.child(cover());
        }

        let mut root = div()
            .id(self.id.clone())
            .debug_selector({
                let key = key.clone();
                move || key
            })
            .relative()
            .flex()
            .flex_col();
        if self.disabled {
            root = root.opacity(0.6);
        }
        let (top, bottom) = match self.status_position {
            ChatComposerStatusPosition::Top => (status, None),
            ChatComposerStatusPosition::Bottom => (None, status),
        };
        // Each part paints under the next, as upstream stacks them: the status and
        // the drawer tuck under the body by one corner radius.
        root = root
            .children(top)
            .children(self.drawer)
            .child(div().flex().flex_col_reverse().children(bottom).child(body));
        // Buttons in the composer follow its corner (upstream `--_button-radius`).
        let concentric = ButtonRadius((radius - padding).max(px(0.)));
        provide(context, provide(concentric, root))
    }
}
