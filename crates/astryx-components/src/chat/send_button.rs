//! ChatSendButton, upstream `Chat/ChatSendButton.tsx`: the composer's circular send
//! button, which turns into a stop button while a response streams.

use std::rc::Rc;

use astryx_primitives::ElementSize;
use astryx_theme::{Declaration, ResolvedTheme, StyleProp, current};
use gpui::Refineable as _;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, IntoElement, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, div,
};

use super::context::{ChatActionFn, ChatComposerContext, use_chat_composer_context};
use crate::button::{Button, ButtonVariant};
use crate::containers::Elevation;
use crate::icon::{Icon, IconSource};

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Upstream `ChatSendButtonProps['size']`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatSendButtonSize {
    Sm,
    #[default]
    Md,
}

impl ChatSendButtonSize {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatSendButtonSize::Sm => "sm",
            ChatSendButtonSize::Md => "md",
        }
    }

    /// The Button size it delegates to.
    pub fn element_size(self) -> ElementSize {
        match self {
            ChatSendButtonSize::Sm => ElementSize::Sm,
            ChatSendButtonSize::Md => ElementSize::Md,
        }
    }
}

/// Upstream `ChatSendButton`. Inside a ChatComposer it reads the composer's state;
/// every value it reads can be set explicitly for standalone use (FR5).
///
/// ```ignore
/// ChatSendButton::new("send").disabled(draft.is_empty()).on_send(|window, cx| submit(window, cx))
/// ```
#[derive(IntoElement)]
pub struct ChatSendButton {
    id: ElementId,
    stop_shown: Option<bool>,
    disabled: Option<bool>,
    on_send: Option<ChatActionFn>,
    on_stop: Option<ChatActionFn>,
    send_icon: Option<AnyElement>,
    stop_icon: Option<AnyElement>,
    size: ChatSendButtonSize,
    on_click: Option<ClickFn>,
    style: StyleRefinement,
}

impl ChatSendButton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        ChatSendButton {
            id: id.into(),
            stop_shown: None,
            disabled: None,
            on_send: None,
            on_stop: None,
            send_icon: None,
            stop_icon: None,
            size: ChatSendButtonSize::Md,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `isStopShown`: the stop button instead of send. Else the composer's
        /// state, else false.
        ?stop_shown: bool,
        /// Upstream `isDisabled`: disables send (never stop). Else the inverse of the
        /// composer's `canSend`; outside a composer, disabled.
        ?disabled: bool,
        /// Upstream `size`, `md` by default.
        size: ChatSendButtonSize,
    }

    /// Upstream `onSend`: else the composer's `onSubmit` with its current value.
    pub fn on_send(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_send = Some(Rc::new(f));
        self
    }

    /// Upstream `onStop`: else the composer's `onStop`.
    pub fn on_stop(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_stop = Some(Rc::new(f));
        self
    }

    /// Upstream `sendIcon`: else the `arrowUp` icon.
    pub fn send_icon(mut self, icon: impl IntoElement) -> Self {
        self.send_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `stopIcon`: else the `stop` icon.
    pub fn stop_icon(mut self, icon: impl IntoElement) -> Self {
        self.stop_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `onClick`: runs after the send or stop action, never instead of it.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Styled for ChatSendButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The resolved state: whether stop shows, and whether the button is disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SendState {
    pub stop: bool,
    pub disabled: bool,
}

/// ORD1 and ORD3: explicit props, then the composer, then the defaults. Stop is never
/// disabled.
pub(crate) fn resolve(
    stop_shown: Option<bool>,
    disabled: Option<bool>,
    context: Option<&ChatComposerContext>,
) -> SendState {
    let stop = stop_shown
        .or(context.map(|c| c.stop_shown))
        .unwrap_or(false);
    let disabled = disabled.unwrap_or(!context.is_some_and(|c| c.can_send));
    SendState {
        stop,
        disabled: !stop && disabled,
    }
}

/// The button's translated name, variant and default icon for a state.
pub(crate) fn presentation(stop: bool) -> (&'static str, ButtonVariant, &'static str) {
    if stop {
        (
            "@astryx.chatSendButton.stop",
            ButtonVariant::Secondary,
            "stop",
        )
    } else {
        (
            "@astryx.chatSendButton.send",
            ButtonVariant::Primary,
            "arrowUp",
        )
    }
}

/// The component's own root style over the Button's (FR7): circular unless a theme
/// rounds the button itself, never shrinking, then the `chat-send-button` target.
pub(crate) fn root_style(
    theme: &ResolvedTheme,
    variant: ButtonVariant,
    size: ElementSize,
) -> StyleRefinement {
    let t = theme.tokens();
    let button_props = [
        ("variant", variant.as_str()),
        ("size", size.as_str()),
        ("elevation", Elevation::None.as_str()),
    ];
    // Upstream sets the button's radius variable, which a theme's `button` radius
    // still overrides.
    let themed_radius = theme
        .declarations("button", &button_props, None)
        .any(|d| matches!(d, Declaration::Style(StyleProp::Radius(_))));
    let mut el = div().flex_shrink_0();
    if !themed_radius {
        el = el.rounded(t.radius.full);
    }
    theme.refine(el.style(), "chat-send-button", &[], None);
    el.style().clone()
}

impl RenderOnce for ChatSendButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let context = use_chat_composer_context(cx);
        let state = resolve(self.stop_shown, self.disabled, context.as_ref());
        let (label_key, variant, default_icon) = presentation(state.stop);
        let label: SharedString = astryx_i18n::current(cx).t(label_key);
        let action: Option<ChatActionFn> = if state.stop {
            self.on_stop
                .or_else(|| context.as_ref().and_then(|c| c.on_stop.clone()))
        } else {
            self.on_send.or_else(|| {
                context.as_ref().map(|c| {
                    let (submit, value) = (c.on_submit.clone(), c.value.clone());
                    Rc::new(move |window: &mut Window, cx: &mut App| {
                        submit(value.clone(), window, cx)
                    }) as ChatActionFn
                })
            })
        };
        let icon = if state.stop {
            self.stop_icon
        } else {
            self.send_icon
        }
        .unwrap_or_else(|| Icon::new(IconSource::Named(default_icon.into())).into_any_element());
        let size = self.size.element_size();
        let on_click = self.on_click;
        let mut button = Button::new(self.id, label)
            .variant(variant)
            .size(size)
            .icon(icon)
            .icon_only(true)
            .disabled(state.disabled)
            // The state action first, then the caller's click (ORD2); a disabled send
            // runs neither, since the Button never activates (AR3).
            .on_click(move |event, window, cx| {
                if let Some(action) = &action {
                    action(window, cx);
                }
                if let Some(f) = &on_click {
                    f(event, window, cx);
                }
            });
        let root = root_style(&theme, variant, size);
        button.style().refine(&root);
        button.style().refine(&self.style);
        button
    }
}
