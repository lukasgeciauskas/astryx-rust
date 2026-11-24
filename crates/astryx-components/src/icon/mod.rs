//! Icon. Upstream: `packages/core/src/Icon/`.

pub mod examples;

use astryx_primitives::use_context;
use astryx_theme::{ResolvedTheme, Tokens, current};
use gpui::{
    App, ElementId, Hsla, IntoElement, Rems, RenderOnce, Role, SharedString, StyleRefinement,
    Styled, Window, prelude::*, rems, svg,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

/// What an icon draws, upstream `icon`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconSource {
    /// A built-in meaning (`"close"`, `"chevronDown"`): the active theme's
    /// artwork for it, else Astryx's core icon.
    Named(SharedString),
    /// Any SVG asset path, upstream's icon component from an app's library.
    Path(SharedString),
}

impl From<&'static str> for IconSource {
    fn from(name: &'static str) -> Self {
        IconSource::Named(name.into())
    }
}

/// Upstream `size`: 12, 16, 20 or 24 px at a 16 px root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconSize {
    Xsm,
    Sm,
    #[default]
    Md,
    Lg,
}

impl IconSize {
    pub fn rems(self) -> Rems {
        match self {
            IconSize::Xsm => rems(0.75),
            IconSize::Sm => rems(1.),
            IconSize::Md => rems(1.25),
            IconSize::Lg => rems(1.5),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            IconSize::Xsm => "xsm",
            IconSize::Sm => "sm",
            IconSize::Md => "md",
            IconSize::Lg => "lg",
        }
    }
}

/// The icon size a container gives the icons inside it, upstream
/// `IconDefaultSizeProvider` (Button sets it, for instance). An explicit
/// `size` wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IconDefaultSize(pub IconSize);

/// Upstream `color`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconColor {
    Primary,
    Secondary,
    Tertiary,
    Disabled,
    Accent,
    Info,
    Success,
    Error,
    Warning,
    /// The surrounding text color. The default.
    #[default]
    Inherit,
    Blue,
    Red,
    Green,
    Gray,
    Cyan,
    Teal,
    Yellow,
    Orange,
    Pink,
    Purple,
}

impl IconColor {
    /// The token color, or `None` for `Inherit`.
    pub fn token(self, t: &Tokens) -> Option<Hsla> {
        let c = &t.color;
        Some(match self {
            IconColor::Primary => c.icon_primary,
            IconColor::Secondary | IconColor::Tertiary => c.icon_secondary,
            IconColor::Disabled => c.icon_disabled,
            IconColor::Accent => c.accent,
            IconColor::Info => c.icon_info,
            IconColor::Success => c.icon_success,
            IconColor::Error => c.icon_error,
            IconColor::Warning => c.icon_warning,
            IconColor::Inherit => return None,
            IconColor::Blue => c.icon_blue,
            IconColor::Red => c.icon_red,
            IconColor::Green => c.icon_green,
            IconColor::Gray => c.icon_gray,
            IconColor::Cyan => c.icon_cyan,
            IconColor::Teal => c.icon_teal,
            IconColor::Yellow => c.icon_yellow,
            IconColor::Orange => c.icon_orange,
            IconColor::Pink => c.icon_pink,
            IconColor::Purple => c.icon_purple,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            IconColor::Primary => "primary",
            IconColor::Secondary => "secondary",
            IconColor::Tertiary => "tertiary",
            IconColor::Disabled => "disabled",
            IconColor::Accent => "accent",
            IconColor::Info => "info",
            IconColor::Success => "success",
            IconColor::Error => "error",
            IconColor::Warning => "warning",
            IconColor::Inherit => "inherit",
            IconColor::Blue => "blue",
            IconColor::Red => "red",
            IconColor::Green => "green",
            IconColor::Gray => "gray",
            IconColor::Cyan => "cyan",
            IconColor::Teal => "teal",
            IconColor::Yellow => "yellow",
            IconColor::Orange => "orange",
            IconColor::Pink => "pink",
            IconColor::Purple => "purple",
        }
    }
}

/// The asset path for an icon in a theme: the theme's artwork (its variant
/// first, when asked for), else the core icon. `None` for an unknown name.
pub fn resolve_icon(
    source: &IconSource,
    variant: Option<&str>,
    theme: &ResolvedTheme,
) -> Option<SharedString> {
    match source {
        IconSource::Path(path) => Some(path.clone()),
        IconSource::Named(name) => {
            let themed = variant
                .and_then(|v| theme.theme.icon(&format!("{name}:{v}")))
                .or_else(|| theme.theme.icon(name));
            themed
                .map(|path| SharedString::from(path.to_string()))
                .or_else(|| astryx_icons::core_icon_path(name).map(SharedString::from))
        }
    }
}

/// An icon in the theme's colors and sizes, upstream `Icon`.
///
/// ```ignore
/// Icon::new("close").color(IconColor::Secondary).label("Close")
/// ```
#[derive(IntoElement)]
pub struct Icon {
    source: IconSource,
    color: IconColor,
    size: Option<IconSize>,
    variant: Option<SharedString>,
    label: Option<SharedString>,
    id: Option<ElementId>,
    style: StyleRefinement,
}

impl Icon {
    /// Upstream `icon`: a built-in name or an [`IconSource`].
    pub fn new(icon: impl Into<IconSource>) -> Self {
        Icon {
            source: icon.into(),
            color: IconColor::Inherit,
            size: None,
            variant: None,
            label: None,
            id: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `color`.
        color: IconColor,
        /// Upstream `size`. Defaults to the container's icon size, else `md`.
        ?size: IconSize,
        /// Upstream `variant`: the theme's alternate artwork, such as `filled`.
        ?variant: SharedString,
    }

    /// Upstream `label`: the accessible name of a meaningful standalone
    /// icon. Without one the icon is decorative and left out of the
    /// accessibility tree.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The element id; a labeled icon needs one to be in the accessibility tree.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl Styled for Icon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let size = self
            .size
            .or_else(|| use_context::<IconDefaultSize>(cx).map(|s| s.0))
            .unwrap_or_default();
        let color = self
            .color
            .token(t)
            .or_else(|| window.text_style().color.into())
            .unwrap_or(t.color.icon_primary);
        let path = resolve_icon(&self.source, self.variant.as_deref(), &theme).unwrap_or_default();
        let mut el = svg()
            .path(path)
            .flex_none()
            .size(size.rems())
            .text_color(color);
        theme.refine(
            el.style(),
            "icon",
            &[("size", size.as_str()), ("color", self.color.as_str())],
            None,
        );
        el.style().refine(&self.style);
        match (self.label, self.id) {
            (Some(label), Some(id)) => el
                .id(id)
                .role(Role::Image)
                .aria_label(label)
                .into_any_element(),
            (Some(label), None) => el
                .id(ElementId::Name(label.clone()))
                .role(Role::Image)
                .aria_label(label)
                .into_any_element(),
            _ => el.into_any_element(),
        }
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const ICON_DOC: ComponentDoc = ComponentDoc {
    name: "Icon",
    rust_type: "astryx::components::icon::Icon",
    props: &[
        p(
            "icon",
            RustApi::Argument("icon (a built-in name or IconSource::Path)"),
        ),
        p("color", RustApi::Method("color")),
        p("size", RustApi::Method("size")),
        p("variant", RustApi::Method("variant")),
        p("label", RustApi::Method("label")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
    ],
    example: include_str!("examples.rs"),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> ResolvedTheme {
        ResolvedTheme::new(
            std::sync::Arc::new(astryx_themes::neutral::theme()),
            astryx_theme::Appearance::Light,
            1280.,
            astryx_theme::Pointer::Fine,
        )
    }

    #[test]
    fn names_resolve_through_the_theme_then_core() {
        let theme = theme();
        let close = resolve_icon(&"close".into(), None, &theme).unwrap();
        assert!(close.ends_with(".svg"), "{close}");
        // A variant the theme lacks falls back to the base artwork.
        assert_eq!(
            resolve_icon(&"close".into(), Some("filled"), &theme),
            Some(close)
        );
        assert_eq!(resolve_icon(&"nope".into(), None, &theme), None);
        assert_eq!(
            resolve_icon(&IconSource::Path("my/icon.svg".into()), None, &theme).as_deref(),
            Some("my/icon.svg")
        );
    }

    #[test]
    fn sizes_and_colors() {
        assert_eq!(IconSize::Sm.rems(), rems(1.));
        let theme = theme();
        let t = theme.tokens();
        assert_eq!(IconColor::Error.token(t), Some(t.color.icon_error));
        assert_eq!(IconColor::Inherit.token(t), None);
    }
}
