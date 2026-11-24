//! Token. Upstream: `Token/`.

use std::rc::Rc;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{
    ElementSize, control_focus, is_focus_visible, provide, resolve_size, use_link_handler,
};
use astryx_theme::color::composite;
use astryx_theme::{Tokens, current};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, Hsla, IntoElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px, relative,
};

use crate::button::element_height;
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconDefaultSize, IconSize};
use crate::layout::VisuallyHidden;

/// Upstream Token `color`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TokenColor {
    #[default]
    Default,
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Cyan,
    Blue,
    Purple,
    Pink,
    Gray,
}

impl TokenColor {
    /// (background, foreground).
    pub fn colors(self, t: &Tokens) -> (Hsla, Hsla) {
        let c = &t.color;
        match self {
            TokenColor::Default => (c.neutral, c.text_primary),
            TokenColor::Red => (c.background_red, c.text_red),
            TokenColor::Orange => (c.background_orange, c.text_orange),
            TokenColor::Yellow => (c.background_yellow, c.text_yellow),
            TokenColor::Green => (c.background_green, c.text_green),
            TokenColor::Teal => (c.background_teal, c.text_teal),
            TokenColor::Cyan => (c.background_cyan, c.text_cyan),
            TokenColor::Blue => (c.background_blue, c.text_blue),
            TokenColor::Purple => (c.background_purple, c.text_purple),
            TokenColor::Pink => (c.background_pink, c.text_pink),
            TokenColor::Gray => (c.background_gray, c.text_gray),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TokenColor::Default => "default",
            TokenColor::Red => "red",
            TokenColor::Orange => "orange",
            TokenColor::Yellow => "yellow",
            TokenColor::Green => "green",
            TokenColor::Teal => "teal",
            TokenColor::Cyan => "cyan",
            TokenColor::Blue => "blue",
            TokenColor::Purple => "purple",
            TokenColor::Pink => "pink",
            TokenColor::Gray => "gray",
        }
    }
}

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A compact chip for metadata, filters and selections, upstream `Token`.
///
/// ```ignore
/// Token::new("tag-design", "Design").color(TokenColor::Blue).on_remove(|_, _, _| {})
/// ```
#[derive(IntoElement)]
pub struct Token {
    id: ElementId,
    label: SharedString,
    size: Option<ElementSize>,
    color: TokenColor,
    icon: Option<AnyElement>,
    disabled: bool,
    on_remove: Option<ClickFn>,
    on_click: Option<ClickFn>,
    href: Option<SharedString>,
    description: Option<SharedString>,
    end_content: Option<AnyElement>,
    label_hidden: bool,
    style: StyleRefinement,
}

impl Token {
    /// Upstream `label`.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Token {
            id: id.into(),
            label: label.into(),
            size: None,
            color: TokenColor::Default,
            icon: None,
            disabled: false,
            on_remove: None,
            on_click: None,
            href: None,
            description: None,
            end_content: None,
            label_hidden: false,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `size`: else the nearest size provider, else `md`.
        ?size: ElementSize,
        /// Upstream `color`.
        color: TokenColor,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `href`: the token is a link.
        ?href: SharedString,
        /// Upstream `description`: an accessible description.
        ?description: SharedString,
        /// Upstream `isLabelHidden`: kept for assistive technology only.
        !label_hidden,
    }

    /// Upstream `icon`: before the label.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `endContent`: after the label, before the remove button.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `onRemove`: shows a remove button.
    pub fn on_remove(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_remove = Some(Rc::new(f));
        self
    }

    /// Upstream `onClick`: the token is pressable.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Styled for Token {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Token {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let (bg, fg) = self.color.colors(t);
        let height = element_height(size, t).to_pixels(rem) - px(8.);
        let interactive = !self.disabled && (self.on_click.is_some() || self.href.is_some());
        let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);

        let icon_size = if size == ElementSize::Sm {
            IconSize::Xsm
        } else {
            IconSize::Sm
        };
        let label = if self.label_hidden {
            VisuallyHidden::new()
                .child(self.label.clone())
                .into_any_element()
        } else {
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(self.label.clone())
                .into_any_element()
        };
        let remove = self
            .on_remove
            .clone()
            .filter(|_| !self.disabled)
            .map(|on_remove| {
                let name = i18n(cx).format(
                    "@astryx.token.remove",
                    &[("label", self.label.to_string().into())],
                );
                let remove_id = ElementId::Name(format!("{:?}-remove", self.id).into());
                let focus = control_focus(&remove_id, window, cx);
                let mut button = div()
                    .id(remove_id)
                    .track_focus(&focus)
                    .relative()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(16.))
                    .margin_end(dir, -t.spacing.s1.to_pixels(rem))
                    .rounded_full()
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .active(move |s| s.bg(pressed))
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        on_remove(event, window, cx);
                    })
                    .child(Icon::new("close").size(IconSize::Xsm));
                if is_focus_visible(&focus, window) {
                    button = button.child(focus_ring(t, px(8.), rem));
                }
                button.role(Role::Button).aria_label(name)
            });

        let mut el = div()
            .relative()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(t.spacing.s1)
            .h(height)
            .px(t.spacing.s2)
            .max_w_full()
            .rounded(t.radius.inner)
            .bg(bg)
            .text_color(fg)
            .text_size(t.text.supporting.size)
            .line_height(relative(t.text.supporting.leading))
            .font_weight(t.font.weight.medium)
            .whitespace_nowrap();
        if self.disabled {
            el = el.opacity(0.5);
        }
        theme.refine(
            el.style(),
            "token",
            &[("size", size.as_str()), ("color", self.color.as_str())],
            None,
        );
        el.style().refine(&self.style);
        let el = el
            .children(
                self.icon
                    .map(|icon| provide(IconDefaultSize(icon_size), icon)),
            )
            .child(label)
            .children(self.end_content)
            .children(remove);

        let mut el = el.id(self.id.clone());
        if interactive {
            let focus = control_focus(&self.id, window, cx);
            let on_click = self.on_click.clone();
            let navigation = self.href.clone().map(|href| (href, use_link_handler(cx)));
            el = el
                .track_focus(&focus)
                .cursor_pointer()
                .hover(move |s| s.bg(composite(bg, hover)))
                .active(move |s| s.bg(composite(bg, pressed)))
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    if let Some(f) = &on_click {
                        f(event, window, cx);
                    }
                    if let Some((href, handler)) = &navigation {
                        handler.open(href, None, window, cx);
                    }
                });
            if is_focus_visible(&focus, window) {
                el = el.child(focus_ring(t, t.radius.inner.to_pixels(rem), rem));
            }
        }
        // A static token is plain text to assistive technology: no role.
        let el = match (interactive, self.href.is_some()) {
            (true, true) => el.role(Role::Link),
            (true, false) => el.role(Role::Button),
            _ => el,
        };
        let el = el.aria_label(self.label);
        match self.description {
            Some(description) => el.aria_description(description),
            None => el,
        }
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const TOKEN_DOC: ComponentDoc = ComponentDoc {
    name: "Token",
    rust_type: "astryx::components::token::Token",
    props: &[
        p("label", RustApi::Argument("label")),
        p("size", RustApi::Method("size")),
        p("color", RustApi::Method("color")),
        p("icon", RustApi::Method("icon")),
        p("isDisabled", RustApi::Method("disabled")),
        p("onRemove", RustApi::Method("on_remove")),
        p("onClick", RustApi::Method("on_click")),
        p("href", RustApi::Method("href")),
        p("description", RustApi::Method("description")),
        p("endContent", RustApi::Method("end_content")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("../containers/examples.rs"),
};
