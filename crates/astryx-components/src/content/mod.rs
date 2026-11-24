//! Content: Code, Kbd, Blockquote and Citation.
//! Upstream: `Code/`, `Kbd/`, `Blockquote/`, `Citation/`.

pub mod examples;

use astryx_i18n::{I18n, LogicalStyled, current as i18n, use_direction};
use astryx_primitives::use_link_handler;
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, div, img, prelude::*,
};
use smallvec::SmallVec;

use crate::doc::{ComponentDoc, PropMapping, RustApi};

// ----------------------------------------------------------------------------
// Code

/// Upstream `Code` `color`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeColor {
    #[default]
    Primary,
    Secondary,
    Inherit,
}

/// Inline code in prose, upstream `Code`.
#[derive(IntoElement)]
pub struct Code {
    text: SharedString,
    color: CodeColor,
    inherit_size: bool,
    style: StyleRefinement,
}

impl Code {
    /// Upstream `children`: the code.
    pub fn new(text: impl Into<SharedString>) -> Self {
        Code {
            text: text.into(),
            color: CodeColor::Primary,
            inherit_size: false,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `color`.
        color: CodeColor,
        /// Upstream `size="inherit"`: the surrounding font size and line height.
        !inherit_size,
    }
}

impl Styled for Code {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Code {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let mut el = div()
            .flex_none()
            .font_stack(&t.font.family.code)
            .bg(t.color.background_muted)
            .px(t.spacing.s1)
            .py(t.spacing.s0)
            .rounded(t.radius.inner);
        if !self.inherit_size {
            el = el.text_style(&t.text.code);
        }
        el = match self.color {
            CodeColor::Primary => el.text_color(t.color.text_primary),
            CodeColor::Secondary => el.text_color(t.color.text_secondary),
            CodeColor::Inherit => el,
        };
        let color = match self.color {
            CodeColor::Primary => "primary",
            CodeColor::Secondary => "secondary",
            CodeColor::Inherit => "inherit",
        };
        let mut props = vec![("color", color)];
        if self.inherit_size {
            props.push(("size", "inherit"));
        }
        theme.refine(el.style(), "code", &props, None);
        el.style().refine(&self.style);
        el.child(self.text)
    }
}

// ----------------------------------------------------------------------------
// Kbd

/// Whether shortcuts show Apple's modifier symbols, as upstream's
/// `isApplePlatform`.
fn is_apple() -> bool {
    cfg!(any(target_os = "macos", target_os = "ios"))
}

/// How one key is drawn, upstream `getKeyDisplay`.
pub fn key_display(key: &str, apple: bool, i18n: &I18n) -> SharedString {
    if apple {
        let symbol = match key {
            "mod" | "meta" => Some("\u{2318}"),
            "ctrl" => Some("\u{2303}"),
            "alt" => Some("\u{2325}"),
            "shift" => Some("\u{21E7}"),
            _ => None,
        };
        if let Some(symbol) = symbol {
            return symbol.into();
        }
    } else {
        let short = match key {
            "mod" | "ctrl" => Some("@astryx.kbd.key.controlShort"),
            "alt" => Some("@astryx.kbd.key.altShort"),
            "shift" => Some("@astryx.kbd.key.shiftShort"),
            "meta" => Some("@astryx.kbd.key.windowsShort"),
            _ => None,
        };
        if let Some(short) = short {
            return i18n.t(short);
        }
    }
    match key {
        "escape" => i18n.t("@astryx.kbd.key.escapeShort"),
        "enter" => "\u{21B5}".into(),
        "backspace" => "\u{232B}".into(),
        "tab" => "\u{21E5}".into(),
        "up" => "\u{2191}".into(),
        "down" => "\u{2193}".into(),
        "left" => "\u{2190}".into(),
        "right" => "\u{2192}".into(),
        "plus" => "+".into(),
        other => other.to_uppercase().into(),
    }
}

/// A key's spoken name, upstream `getKeyLabel`.
pub fn key_label(key: &str, apple: bool, i18n: &I18n) -> SharedString {
    let message = match key {
        "mod" if apple => "@astryx.kbd.key.command",
        "mod" | "ctrl" => "@astryx.kbd.key.control",
        "alt" => "@astryx.kbd.key.alt",
        "shift" => "@astryx.kbd.key.shift",
        "enter" => "@astryx.kbd.key.enter",
        "backspace" => "@astryx.kbd.key.backspace",
        "escape" => "@astryx.kbd.key.escape",
        "tab" => "@astryx.kbd.key.tab",
        "up" => "@astryx.kbd.key.up",
        "down" => "@astryx.kbd.key.down",
        "left" => "@astryx.kbd.key.left",
        "right" => "@astryx.kbd.key.right",
        "plus" => "@astryx.kbd.key.plus",
        other => return other.to_uppercase().into(),
    };
    i18n.t(message)
}

/// The keys of a `+`-separated shortcut, lowercased with aliases resolved.
pub fn parse_keys(keys: &str) -> Vec<String> {
    keys.split('+')
        .map(|key| key.trim().to_lowercase())
        .map(|key| match key.as_str() {
            "esc" => "escape".into(),
            "return" => "enter".into(),
            _ => key,
        })
        .collect()
}

/// A keyboard shortcut drawn as key caps, upstream `Kbd`.
///
/// ```ignore
/// Kbd::new("mod+k")
/// ```
#[derive(IntoElement)]
pub struct Kbd {
    keys: SharedString,
    id: Option<ElementId>,
    style: StyleRefinement,
}

impl Kbd {
    /// Upstream `keys`: `"mod+k"`; `mod` is ⌘ on Apple platforms, Ctrl elsewhere.
    pub fn new(keys: impl Into<SharedString>) -> Self {
        Kbd {
            keys: keys.into(),
            id: None,
            style: StyleRefinement::default(),
        }
    }

    /// The element id; with one, the shortcut's spoken name is in the
    /// accessibility tree.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl Styled for Kbd {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Kbd {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let i18n = i18n(cx);
        let apple = is_apple();
        let keys = parse_keys(&self.keys);
        let name = keys
            .iter()
            .map(|key| key_label(key, apple, &i18n))
            .reduce(|shortcut, key| {
                i18n.format(
                    "@astryx.kbd.shortcut",
                    &[
                        ("shortcut", shortcut.to_string().into()),
                        ("key", key.to_string().into()),
                    ],
                )
            })
            .unwrap_or_default();
        let caps = keys.iter().map(|key| {
            div()
                .flex()
                .items_center()
                .justify_center()
                .min_w(t.spacing.s5)
                .h(t.spacing.s5)
                .px(t.spacing.s1)
                .rounded(t.radius.inner)
                .bg(t.color.neutral)
                .border_b_2()
                .border_color(t.color.border_emphasized)
                .text_color(t.color.text_primary)
                .font_stack(&t.font.family.body)
                .text_style(&t.text.supporting)
                .font_weight(t.font.weight.medium)
                .child(key_display(key, apple, &i18n))
        });
        let mut el = div()
            .inline_flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.s1);
        theme.refine(el.style(), "kbd", &[], None);
        el.style().refine(&self.style);
        let el = el.children(caps);
        match self.id {
            Some(id) => el
                .id(id)
                .role(Role::Image)
                .aria_label(name)
                .into_any_element(),
            None => el.into_any_element(),
        }
    }
}

// ----------------------------------------------------------------------------
// Blockquote

/// A quotation with an inline-start rule, upstream `Blockquote`.
#[derive(IntoElement)]
pub struct Blockquote {
    cite: Option<AnyElement>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl Blockquote {
    pub fn new() -> Self {
        Blockquote {
            cite: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    /// Upstream `cite`: the attribution after the quote.
    pub fn cite(mut self, cite: impl IntoElement) -> Self {
        self.cite = Some(cite.into_any_element());
        self
    }
}

impl Default for Blockquote {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Blockquote {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Blockquote {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Blockquote {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let mut el = div()
            .flex()
            .flex_col()
            .border_start(dir, t.spacing.s0_5)
            .border_color(t.color.border_emphasized)
            .padding_start(dir, t.spacing.s4)
            .text_color(t.color.text_secondary);
        theme.refine(el.style(), "blockquote", &[], None);
        el.style().refine(&self.style);
        el.children(self.children).children(self.cite.map(|cite| {
            div()
                .mt(t.spacing.s2)
                .text_style(&t.text.supporting)
                .child(cite)
        }))
    }
}

// ----------------------------------------------------------------------------
// Citation

/// Upstream `CitationSource`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CitationSource {
    pub title: Option<SharedString>,
    /// Opened through the link handler when the citation is clicked.
    pub url: Option<SharedString>,
    /// An image (favicon) shown before the title.
    pub src: Option<SharedString>,
    /// A built-in icon name shown before the title, when there is no image.
    pub icon: Option<SharedString>,
}

/// Upstream `Citation` `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CitationVariant {
    /// A chip with the source's title.
    #[default]
    Label,
    /// A compact numbered badge.
    Number,
}

/// An inline reference to a source, upstream `Citation`.
#[derive(IntoElement)]
pub struct Citation {
    id: ElementId,
    source: CitationSource,
    number: u32,
    variant: CitationVariant,
    style: StyleRefinement,
}

impl Citation {
    /// Upstream `source` and `number`.
    pub fn new(id: impl Into<ElementId>, source: CitationSource, number: u32) -> Self {
        Citation {
            id: id.into(),
            source,
            number,
            variant: CitationVariant::Label,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `variant`.
        variant: CitationVariant,
    }
}

impl Styled for Citation {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Citation {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let i18n = i18n(cx);
        let title: SharedString = self
            .source
            .title
            .clone()
            .unwrap_or_else(|| self.number.to_string().into());
        let href = self
            .source
            .url
            .clone()
            .filter(|url| astryx_primitives::link::is_safe_url(url));
        let name = i18n.format(
            "@astryx.citation.label",
            &[
                ("number", self.number.into()),
                ("title", title.to_string().into()),
            ],
        );
        let hover_bg = t.color.overlay_hover;
        let hover_fg = t.color.text_primary;
        let mut el = div().id(self.id).inline_flex().flex_none().items_center();
        el = match self.variant {
            CitationVariant::Number => el
                .justify_center()
                .min_w(t.spacing.s5)
                .h(t.spacing.s5)
                .px(t.spacing.s1)
                .rounded(t.radius.full)
                .bg(t.color.accent_muted)
                .text_style(&t.text.supporting)
                .font_weight(t.font.weight.semibold)
                .text_color(t.color.text_secondary)
                .child(self.number.to_string()),
            CitationVariant::Label => {
                let icon = if let Some(src) = self.source.src.clone() {
                    Some(img(src).size(t.spacing.s3).into_any_element())
                } else {
                    self.source.icon.clone().map(|name| {
                        crate::icon::Icon::new(crate::icon::IconSource::Named(name))
                            .size(crate::icon::IconSize::Xsm)
                            .into_any_element()
                    })
                };
                el.gap(t.spacing.s1)
                    .h(t.spacing.s5)
                    .px(t.spacing.s2)
                    .margin_start(dir, t.spacing.s0_5)
                    .rounded(t.radius.element)
                    .border_1()
                    .border_color(t.color.border)
                    .text_style(&t.text.supporting)
                    .text_color(t.color.text_secondary)
                    .when(icon.is_some(), |el| el.padding_start(dir, t.spacing.s0_5))
                    .children(icon.map(|icon| {
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(t.spacing.s4)
                            .rounded_full()
                            .overflow_hidden()
                            .bg(t.color.background_surface)
                            .border_1()
                            .border_color(t.color.border)
                            .child(icon)
                    }))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(title),
                    )
            }
        };
        let variant = match self.variant {
            CitationVariant::Label => "label",
            CitationVariant::Number => "number",
        };
        theme.refine(el.style(), "citation", &[("variant", variant)], None);
        el.style().refine(&self.style);
        let el = el.aria_label(name);
        match href {
            Some(href) => {
                let handler = use_link_handler(cx);
                el.role(Role::Link)
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
                    .on_click(move |_, window, cx| {
                        handler.open(&href, Some("_blank".into()), window, cx)
                    })
                    .into_any_element()
            }
            None => el.role(Role::Group).into_any_element(),
        }
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CODE_DOC: ComponentDoc = ComponentDoc {
    name: "Code",
    rust_type: "astryx::components::content::Code",
    props: &[
        p("children", RustApi::Argument("text")),
        p("color", RustApi::Method("color")),
        p("size", RustApi::Method("inherit_size")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
        p(
            "data-testid",
            RustApi::Waived("GPUI tests find elements by id"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const KBD_DOC: ComponentDoc = ComponentDoc {
    name: "Kbd",
    rust_type: "astryx::components::content::Kbd",
    props: &[
        p("keys", RustApi::Argument("keys")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
    ],
    example: include_str!("examples.rs"),
};

pub const BLOCKQUOTE_DOC: ComponentDoc = ComponentDoc {
    name: "Blockquote",
    rust_type: "astryx::components::content::Blockquote",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("cite", RustApi::Method("cite")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const CITATION_DOC: ComponentDoc = ComponentDoc {
    name: "Citation",
    rust_type: "astryx::components::content::Citation",
    props: &[
        p("source", RustApi::Argument("source")),
        p("number", RustApi::Argument("number")),
        p("variant", RustApi::Method("variant")),
    ],
    example: include_str!("examples.rs"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kbd_keys_follow_upstream() {
        let en = astryx_i18n::english();
        assert_eq!(parse_keys("Mod + Esc"), ["mod", "escape"]);
        assert_eq!(key_display("mod", true, &en), "\u{2318}");
        assert_eq!(key_display("k", false, &en), "K");
        assert_eq!(key_display("enter", false, &en), "\u{21B5}");
        assert_eq!(key_label("mod", true, &en), en.t("@astryx.kbd.key.command"));
        assert_eq!(
            key_label("mod", false, &en),
            en.t("@astryx.kbd.key.control")
        );
        assert_ne!(key_display("ctrl", false, &en), "CTRL");
    }
}
