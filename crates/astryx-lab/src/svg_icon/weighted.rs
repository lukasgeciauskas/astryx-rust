//! WeightedSVGIcon (upstream lab `SVGIcon/WeightedSVGIcon.tsx`): an SVG icon whose
//! drawing follows the surrounding font weight, inline or from a sprite. An icon holds
//! every drawing; the one whose weight range holds the current weight shows. The weight
//! is the inherited text style's ([`super::probe`]); `weight` pins it, and
//! [`super::pin_icon_weight`] pins a subtree.
//!
//! Sprite mode: render [`WeightedIconSprite`] once with the icons a page uses, then
//! pass `sprite` to each icon, which then draws its sprite's symbol (none while no
//! sprite holds it, as a `<use>` of a missing symbol).

use std::collections::HashMap;
use std::rc::Rc;

use astryx_theme::current;
use gpui::prelude::*;
use gpui::{App, Global, Role, SharedString, StyleRefinement, Window, div};

use super::build::{IconWeight, WeightedIconDef, WeightedIconLayer};
use super::probe::{effective_icon_weight, layer_opacity, pinned_icon_weight, read_weight_probe};
use super::{SVGIconColor, SVGIconSize, stacked};

/// Upstream `weightedIconSymbolId`: the id of an icon's sprite symbol.
pub fn weighted_icon_symbol_id(icon: &WeightedIconDef) -> String {
    format!("astryx-weighted-icon-{}", icon.name)
}

/// The symbols every sprite on screen holds, by id.
#[derive(Default)]
pub(crate) struct Sprites(pub(crate) HashMap<String, Rc<WeightedIconDef>>);

impl Global for Sprites {}

/// One layer's drawing as a standalone SVG document, painted in black.
pub(crate) fn layer_document(icon: &WeightedIconDef, layer: &WeightedIconLayer) -> String {
    let view_box = icon.view_box.as_deref().unwrap_or("0 0 24 24");
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{view_box}\"><g fill=\"black\">"
    );
    for path in &layer.paths {
        out.push_str(&format!("<path d=\"{}\"", path.d.replace('"', "&quot;")));
        if path.even_odd == Some(true) {
            out.push_str(" fill-rule=\"evenodd\"");
        }
        out.push_str("/>");
    }
    out.push_str("</g></svg>");
    out
}

/// Upstream `WeightedIconSprite`: holds each icon's drawings once for the icons that
/// pass `sprite`. Render it once per page, before or after the icons; it draws nothing.
#[derive(IntoElement)]
pub struct WeightedIconSprite {
    icons: Vec<WeightedIconDef>,
}

impl WeightedIconSprite {
    /// Upstream `icons`: every icon the page references with `sprite`.
    pub fn new(icons: impl IntoIterator<Item = WeightedIconDef>) -> Self {
        WeightedIconSprite {
            icons: icons.into_iter().collect(),
        }
    }
}

impl RenderOnce for WeightedIconSprite {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let sprites = cx.default_global::<Sprites>();
        let mut added = false;
        for icon in self.icons {
            let id = weighted_icon_symbol_id(&icon);
            if sprites.0.get(&id).is_none_or(|held| **held != icon) {
                sprites.0.insert(id, Rc::new(icon));
                added = true;
            }
        }
        // Icons drawn before the sprite this frame find their symbol on the next one.
        if added {
            window.refresh();
        }
        div()
            .absolute()
            .size_0()
            .overflow_hidden()
            .debug_selector(|| "weighted-icon-sprite".into())
    }
}

/// Upstream lab `WeightedSVGIcon`: an SVG icon whose drawing follows the surrounding
/// font weight.
///
/// ```ignore
/// Text::new("Alerts").weight(TextWeight::Bold).child(WeightedSVGIcon::new(bell_weighted_icon()).color(SVGIconColor::Inherit))
/// ```
#[derive(IntoElement)]
pub struct WeightedSVGIcon {
    icon: WeightedIconDef,
    size: SVGIconSize,
    color: SVGIconColor,
    weight: Option<IconWeight>,
    sprite: bool,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl WeightedSVGIcon {
    /// Upstream `icon`: one drawing per weight range.
    pub fn new(icon: WeightedIconDef) -> Self {
        WeightedSVGIcon {
            icon,
            size: SVGIconSize::Md,
            color: SVGIconColor::Primary,
            weight: None,
            sprite: false,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `size` (`Md` by default).
    pub fn size(mut self, size: SVGIconSize) -> Self {
        self.size = size;
        self
    }

    /// Upstream `color` (`Primary` by default).
    pub fn color(mut self, color: SVGIconColor) -> Self {
        self.color = color;
        self
    }

    /// Upstream `weight`: pins the drawing to one weight instead of following the
    /// surrounding font weight.
    pub fn weight(mut self, weight: IconWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    /// Upstream `sprite`: draws the icon's symbol from a [`WeightedIconSprite`].
    pub fn sprite(mut self, sprite: bool) -> Self {
        self.sprite = sprite;
        self
    }

    /// Upstream `aria-label`: names the icon, which is then an image; unnamed icons
    /// are decorative.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Styled for WeightedSVGIcon {
    /// The icon's style (upstream `className` and `style`).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The weight a weighted icon draws at here: the pin (its own, then an ancestor's),
/// else the surrounding text's.
pub(crate) fn current_icon_weight(
    pinned: Option<IconWeight>,
    window: &Window,
    cx: &App,
) -> IconWeight {
    let pinned = pinned.map(f32::from).or_else(|| pinned_icon_weight(cx));
    effective_icon_weight(pinned, read_weight_probe(window.text_style().font_weight.0))
}

impl RenderOnce for WeightedSVGIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let (size, _) = self.size.optical();
        let color = self.color.resolve(window, cx);
        let weight = current_icon_weight(self.weight, window, cx);
        let icon: Option<Rc<WeightedIconDef>> = if self.sprite {
            cx.try_global::<Sprites>()
                .and_then(|s| s.0.get(&weighted_icon_symbol_id(&self.icon)).cloned())
        } else {
            Some(Rc::new(self.icon.clone()))
        };
        let docs: Vec<(String, f32)> = icon
            .map(|icon| {
                icon.layers
                    .iter()
                    .map(|layer| {
                        (
                            layer_document(&icon, layer),
                            layer_opacity(layer.from, layer.to, weight),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let selector = format!("weighted-svg-icon-{}", self.icon.name);
        let mut root = stacked(size, docs, color).debug_selector(move || selector);
        theme.refine(
            root.style(),
            "weighted-svg-icon",
            &[("size", self.size.as_str()), ("color", self.color.as_str())],
            None,
        );
        root.style().refine(&self.style);
        let root = root.id(gpui::ElementId::Name(
            format!("weighted-svg-icon-{}", self.icon.name).into(),
        ));
        match self.label {
            Some(label) => root.role(Role::Image).aria_label(label).into_any_element(),
            None => root.into_any_element(),
        }
    }
}
