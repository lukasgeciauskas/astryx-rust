//! Composer tokens, upstream `ChatComposerInput.tsx` (`ChatComposerToken`,
//! `ChatComposerTokenElement`), `ChatPastedTextToken.tsx`, `pastedTextLabel.ts` and
//! `useChatPasteAsToken.ts`: the atomic chips a composer's text carries, and the
//! paste behavior that turns a long paste into one.

use std::rc::Rc;

use astryx_i18n::Arg;
use astryx_primitives::{ElementSize, LayerAlign, Placement};
use astryx_theme::{ThemeStyled as _, current};
use gpui::Refineable as _;
use gpui::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div, px,
};

use super::composer_input::ChatComposerInputHandle;
use crate::button::{Button, ButtonVariant};
use crate::overlay::HoverCard;
use crate::status::{Badge, BadgeVariant};

/// Renders a token's content (upstream `render`, or a badge's `icon`).
pub type TokenRenderFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

#[derive(Clone)]
enum TokenKind {
    Badge {
        label: SharedString,
        variant: BadgeVariant,
        icon: Option<TokenRenderFn>,
    },
    Custom(TokenRenderFn),
}

/// Upstream `ChatComposerToken`: an inline chip in the composer's text. Its `value` is
/// what the token becomes in the submitted string.
///
/// - [`ChatComposerToken::badge`] (upstream `ChatComposerTokenBadge`): a Badge with a
///   label, variant and icon.
/// - [`ChatComposerToken::custom`] (upstream `ChatComposerTokenCustom`): full control of
///   the rendered content.
#[derive(Clone)]
pub struct ChatComposerToken {
    value: SharedString,
    kind: TokenKind,
}

impl ChatComposerToken {
    /// A badge token: `value` is serialized, `label` shown.
    pub fn badge(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        ChatComposerToken {
            value: value.into(),
            kind: TokenKind::Badge {
                label: label.into(),
                variant: BadgeVariant::Neutral,
                icon: None,
            },
        }
    }

    /// A custom token (upstream `render`): `render` draws its content each frame.
    pub fn custom(
        value: impl Into<SharedString>,
        render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        ChatComposerToken {
            value: value.into(),
            kind: TokenKind::Custom(Rc::new(render)),
        }
    }

    /// The badge's `variant` (no effect on a custom token).
    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        if let TokenKind::Badge { variant: v, .. } = &mut self.kind {
            *v = variant;
        }
        self
    }

    /// The badge's `icon` (no effect on a custom token).
    pub fn icon(mut self, icon: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        if let TokenKind::Badge { icon: i, .. } = &mut self.kind {
            *i = Some(Rc::new(icon));
        }
        self
    }

    /// Upstream `value`: what the token serializes to.
    pub fn value(&self) -> &SharedString {
        &self.value
    }

    /// The badge's label, `None` for a custom token.
    pub fn label(&self) -> Option<&SharedString> {
        match &self.kind {
            TokenKind::Badge { label, .. } => Some(label),
            TokenKind::Custom(_) => None,
        }
    }

    /// Upstream `isCustomToken`.
    pub fn is_custom(&self) -> bool {
        matches!(self.kind, TokenKind::Custom(_))
    }

    /// The token's content: its Badge, or the caller's render (FR2, FR3).
    pub(crate) fn content(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        match &self.kind {
            TokenKind::Badge {
                label,
                variant,
                icon,
            } => {
                let mut badge = Badge::new(label.clone()).variant(*variant);
                if let Some(icon) = icon {
                    badge = badge.icon(icon(window, cx));
                }
                badge.into_any_element()
            }
            TokenKind::Custom(render) => render(window, cx),
        }
    }
}

/// Upstream `ChatComposerTokenElement`: a token as an inline, one-line-high box that
/// centers its Badge or custom content. Upstream marks it `data-astryx-token` with its
/// value and `contentEditable={false}`; here the composer's editing engine keeps a token
/// atomic, and the element carries a `chat-token-<value>` debug selector.
#[derive(IntoElement)]
pub struct ChatComposerTokenElement {
    token: ChatComposerToken,
    style: StyleRefinement,
}

impl ChatComposerTokenElement {
    /// Upstream `token`.
    pub fn new(token: ChatComposerToken) -> Self {
        ChatComposerTokenElement {
            token,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for ChatComposerTokenElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ChatComposerTokenElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let line = window.line_height();
        let selector = format!("chat-token-{}", self.token.value);
        let mut el = div()
            .debug_selector(move || selector)
            .inline_flex()
            .flex_none()
            .items_center()
            .h(line);
        el.style().refine(&self.style);
        el.child(self.token.content(window, cx))
    }
}

/// Upstream `formatPastedTextLabel`: "240 chars", or "3 lines, 302 chars". Characters
/// count UTF-16 units, as JavaScript's `length` does.
pub fn format_pasted_text_label(text: &str, cx: &App) -> SharedString {
    let lines = text.split('\n').count();
    let chars = text.encode_utf16().count();
    astryx_i18n::current(cx).format(
        "@astryx.chat.pastedText.label",
        &[
            ("lines", Arg::Num(lines as f64)),
            ("chars", Arg::Num(chars as f64)),
        ],
    )
}

type ExpandFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `ChatPastedTextToken`: a pasted-text chip. Hovering or focusing it opens a
/// card with the text and an Expand button, which dissolves the token into editable text.
#[derive(IntoElement)]
pub struct ChatPastedTextToken {
    id: ElementId,
    text: SharedString,
    on_expand: Option<ExpandFn>,
}

impl ChatPastedTextToken {
    /// Upstream `text`: the full pasted text.
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        ChatPastedTextToken {
            id: id.into(),
            text: text.into(),
            on_expand: None,
        }
    }

    /// Upstream `onExpand`.
    pub fn on_expand(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_expand = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for ChatPastedTextToken {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let label = format_pasted_text_label(&self.text, cx);
        let expand = astryx_i18n::current(cx).t("@astryx.chat.pastedText.expand");
        let mut footer = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(t.spacing.s2)
            .child(
                div()
                    .text_style(&t.text.supporting)
                    .text_color(t.color.text_secondary)
                    .child(label.clone()),
            );
        if let Some(on_expand) = self.on_expand.clone() {
            footer = footer.child(
                Button::new(
                    ElementId::Name(format!("{}-expand", self.id).into()),
                    expand,
                )
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
                .on_click(move |_, window, cx| on_expand(window, cx)),
            );
        }
        let preview = div()
            .flex()
            .flex_col()
            .gap(t.spacing.s2)
            .max_w(px(480.))
            .child(
                div()
                    .id(ElementId::Name(format!("{}-preview", self.id).into()))
                    .font_stack(&t.font.family.code)
                    .text_style(&t.text.supporting)
                    .text_color(t.color.text_primary)
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .child(self.text.clone()),
            )
            .child(footer);
        let card = HoverCard::new(self.id.clone())
            .content(preview)
            .placement(Placement::Above)
            .alignment(LayerAlign::Start)
            .hover_indication(false)
            .child(Badge::new(label).variant(BadgeVariant::Neutral));
        // Upstream `useTapStop('mousedown')`: a tap on the token, by finger or pen, asks
        // for its preview, so it stops there. Cancelling the press keeps the composer around
        // the token from taking focus and the caret, which on a touch device raises the
        // keyboard over the preview. The card's own trigger, inside, still sees the tap; a
        // mouse press still places the caret, and hover opens the preview.
        div()
            .debug_selector({
                let selector = format!("{}-tap-stop", self.id);
                move || selector
            })
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                if is_tap(window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            })
            .child(card)
    }
}

/// Whether the press now is a finger's or a pen's (upstream `TOUCH_POINTER_TYPES`): the
/// gesture host hears the contact itself, never GPUI's taps synthesized from a touch.
fn is_tap(window: &mut Window, cx: &mut App) -> bool {
    use astryx_primitives::gestures::PointerKind;
    let host = astryx_primitives::gestures::host::gesture_host(window, cx);
    match host.last_pointer() {
        Some(kind) => matches!(kind, PointerKind::Touch | PointerKind::Pen),
        None => window.last_input_was_touch(),
    }
}

/// Converts pasted text into a token (upstream `toToken`).
pub type ToTokenFn = Rc<dyn Fn(&str, &App) -> ChatComposerToken>;

/// Upstream `UseChatPasteAsTokenOptions`.
#[derive(Clone)]
pub struct UseChatPasteAsTokenOptions {
    /// Upstream `inputRef`: the input the token goes into. `None` inside a
    /// ChatComposerInput's `paste_as_token`, which inserts into itself.
    pub input: Option<ChatComposerInputHandle>,
    /// Upstream `threshold`: pastes longer than this many characters become tokens.
    pub threshold: usize,
    /// Upstream `toToken`: by default a neutral badge with the count label.
    pub to_token: Option<ToTokenFn>,
}

impl Default for UseChatPasteAsTokenOptions {
    fn default() -> Self {
        UseChatPasteAsTokenOptions {
            input: None,
            threshold: 200,
            to_token: None,
        }
    }
}

/// Upstream `UseChatPasteAsTokenReturn`: the paste behavior a ChatComposerInput runs.
#[derive(Clone)]
pub struct ChatPasteAsToken {
    options: UseChatPasteAsTokenOptions,
}

/// Upstream `useChatPasteAsToken`.
pub fn use_chat_paste_as_token(options: UseChatPasteAsTokenOptions) -> ChatPasteAsToken {
    ChatPasteAsToken { options }
}

impl ChatPasteAsToken {
    /// Upstream `onPaste(event, text)`: inserts a token for a paste over the threshold
    /// into `input` (else the options' input) and returns true; a shorter one is left to
    /// plain insertion.
    pub fn on_paste(
        &self,
        text: &str,
        input: Option<&ChatComposerInputHandle>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if text.encode_utf16().count() <= self.options.threshold {
            return false;
        }
        let token = match &self.options.to_token {
            Some(f) => f(text, cx),
            None => ChatComposerToken::badge(text.to_string(), format_pasted_text_label(text, cx))
                .variant(BadgeVariant::Neutral),
        };
        if let Some(input) = self.options.input.as_ref().or(input) {
            input.insert_token(token, window, cx);
        }
        true
    }
}
