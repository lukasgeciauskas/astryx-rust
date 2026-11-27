//! SVGIcon (upstream lab `SVGIcon/`): CSS-variable-driven, multi-variation SVG icons with
//! role-aware rendering and mask-based gaps, the weight-responsive WeightedSVGIcon and its
//! sprite, the weighted-icon build step, and the starter icon sets.
//!
//! Upstream draws one `<svg>` whose layer groups read CSS custom properties that a
//! variation and a size set. Here those properties resolve to [`IconVars`] for a
//! variation, a size and an optional stroke width; each layer group becomes its own
//! small SVG document with the resolved fill, stroke, width, caps and joins (and, in
//! bold, the knockout mask), drawn with GPUI's `svg` element and tinted with the icon
//! color at the group's opacity. How each property maps:
//!
//! | CSS custom property | Rust |
//! |---|---|
//! | `--icon-size` | [`IconVars::size`] (12, 16, 20, 24 px by size) |
//! | `--icon-stroke-width` | [`IconVars::stroke_width`] (by size, or `stroke_width`) |
//! | `--icon-stroke-linecap`, `--icon-stroke-linejoin` | `round` |
//! | `--icon-gap`, `--icon-gap-linejoin` | [`IconVars::gap`] (0.75), `round` |
//! | `--icon-layer-{primary,secondary}-{fill,stroke,opacity}` | [`LayerVars`]'s fill-role paint |
//! | `--icon-layer-{primary,secondary}-stroke-role-*` | [`LayerVars`]'s stroke-role paint and width |
//! | `currentColor` | the icon color ([`SVGIconColor`]) |

pub mod build;
pub mod examples;
#[path = "generated/icons.rs"]
#[rustfmt::skip]
mod generated;
pub mod icons;
pub mod probe;
pub mod weighted;

#[cfg(test)]
mod tests;

use gpui::prelude::*;
use gpui::{App, Hsla, Role, SharedString, StyleRefinement, Window, div, px, svg};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_theme::current;

pub use build::{
    BuildWeightedIconOptions, ICON_WEIGHTS, IconWeight, WeightedDrawing, WeightedIconDef,
    WeightedIconLayer, WeightedIconPath, build_weighted_icon, parse_svg_paths,
};
pub use icons::*;
pub use probe::{effective_icon_weight, layer_opacity, pin_icon_weight, read_weight_probe};
pub use weighted::{WeightedIconSprite, WeightedSVGIcon, weighted_icon_symbol_id};

/// Upstream `SVGIconVariation`: a visual treatment of the same paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SVGIconVariation {
    /// Stroked outlines (the default).
    #[default]
    Linear,
    /// Filled shapes, the secondary layer knocked out of the primary one.
    Bold,
    /// Stroked, the secondary layer and stroke-role details at 40%.
    Twotone,
    /// Filled, the secondary layer and stroke-role details at 40%.
    Bulk,
    /// Stroked, the secondary layer hidden.
    Broken,
}

impl SVGIconVariation {
    /// Upstream's name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Bold => "bold",
            Self::Twotone => "twotone",
            Self::Bulk => "bulk",
            Self::Broken => "broken",
        }
    }
}

/// Upstream `SVGIconSize`, with optical stroke compensation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SVGIconSize {
    /// 12 px, 2 wide strokes.
    Xsm,
    /// 16 px, 1.75 wide strokes.
    Sm,
    /// 20 px, 1.5 wide strokes (the default).
    #[default]
    Md,
    /// 24 px, 1.5 wide strokes.
    Lg,
}

impl SVGIconSize {
    /// Upstream's name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xsm => "xsm",
            Self::Sm => "sm",
            Self::Md => "md",
            Self::Lg => "lg",
        }
    }

    /// Upstream `opticalSize`: the box, in px, and the stroke width.
    pub fn optical(self) -> (f32, f32) {
        match self {
            Self::Xsm => (12., 2.),
            Self::Sm => (16., 1.75),
            Self::Md => (20., 1.5),
            Self::Lg => (24., 1.5),
        }
    }
}

/// Upstream `SVGIconColor`: a semantic color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SVGIconColor {
    /// The default.
    #[default]
    Primary,
    Secondary,
    Disabled,
    Accent,
    Positive,
    Negative,
    Warning,
    /// The surrounding text color.
    Inherit,
}

impl SVGIconColor {
    /// Upstream's name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Disabled => "disabled",
            Self::Accent => "accent",
            Self::Positive => "positive",
            Self::Negative => "negative",
            Self::Warning => "warning",
            Self::Inherit => "inherit",
        }
    }

    /// Upstream `iconColorStyles`: the theme's icon color role for this color.
    pub fn resolve(self, window: &Window, cx: &App) -> Hsla {
        let theme = current(window, cx);
        let c = &theme.tokens().color;
        match self {
            Self::Primary => c.icon_primary,
            Self::Secondary => c.icon_secondary,
            Self::Disabled => c.icon_disabled,
            Self::Accent => c.accent,
            Self::Positive => c.icon_success,
            Self::Negative => c.icon_error,
            Self::Warning => c.icon_warning,
            Self::Inherit => window.text_style().color,
        }
    }
}

/// Upstream `IconShapeRole`: how a shape behaves across variations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum IconShapeRole {
    /// A closed shape: stroked in linear, filled in bold (the default).
    #[default]
    Fill,
    /// A line or detail stroke: always stroked.
    Stroke,
}

/// Upstream `IconShape['type']`: the SVG element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconShapeType {
    Path,
    Circle,
    Rect,
    Line,
    Polyline,
    Polygon,
}

impl IconShapeType {
    /// The element's tag.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Circle => "circle",
            Self::Rect => "rect",
            Self::Line => "line",
            Self::Polyline => "polyline",
            Self::Polygon => "polygon",
        }
    }

    /// The element for a tag.
    pub fn from_tag(tag: &str) -> Option<Self> {
        Some(match tag {
            "path" => Self::Path,
            "circle" => Self::Circle,
            "rect" => Self::Rect,
            "line" => Self::Line,
            "polyline" => Self::Polyline,
            "polygon" => Self::Polygon,
            _ => return None,
        })
    }
}

/// Upstream `IconShape`: one element of an icon layer.
#[derive(Clone, Debug, PartialEq)]
pub struct IconShape {
    /// The SVG element.
    pub ty: IconShapeType,
    /// Its attributes (`d`, `cx`, `cy`, `r`, `points`, `x1`, ...), in order.
    pub attrs: Vec<(SharedString, SharedString)>,
    /// How it behaves across variations (`Fill` when unset).
    pub role: Option<IconShapeRole>,
}

impl IconShape {
    /// A shape of `ty` with `attrs`.
    pub fn new(
        ty: IconShapeType,
        attrs: impl IntoIterator<Item = (impl Into<SharedString>, impl Into<SharedString>)>,
    ) -> Self {
        IconShape {
            ty,
            attrs: attrs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            role: None,
        }
    }

    /// Upstream `role`.
    pub fn role(mut self, role: IconShapeRole) -> Self {
        self.role = Some(role);
        self
    }
}

/// Upstream `SVGIconDef`: an icon's geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct SVGIconDef {
    /// The display name.
    pub name: SharedString,
    /// The view box (`0 0 24 24` when unset).
    pub view_box: Option<SharedString>,
    /// The primary layer: the main outlines.
    pub primary: Vec<IconShape>,
    /// The secondary layer: detail elements.
    pub secondary: Option<Vec<IconShape>>,
}

/// A paint of a layer group: `none`, or `currentColor` (the icon color).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconPaint {
    None,
    CurrentColor,
}

impl IconPaint {
    fn svg(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CurrentColor => "black",
        }
    }
}

/// One layer's resolved `--icon-layer-*` properties.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerVars {
    /// Fill-role shapes: fill, stroke and opacity.
    pub fill: IconPaint,
    pub stroke: IconPaint,
    pub opacity: f32,
    /// Stroke-role shapes: fill, stroke, opacity and stroke width.
    pub stroke_role_fill: IconPaint,
    pub stroke_role_stroke: IconPaint,
    pub stroke_role_opacity: f32,
    pub stroke_role_width: f32,
}

/// Every resolved icon property for a variation, a size and an optional stroke width
/// (upstream `iconVars` as `variations` and `opticalSize` set them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconVars {
    /// `--icon-size`, px.
    pub size: f32,
    /// `--icon-stroke-width`: fill-role strokes.
    pub stroke_width: f32,
    /// `--icon-gap`: the knockout border in bold.
    pub gap: f32,
    pub primary: LayerVars,
    pub secondary: LayerVars,
}

/// Upstream's `variations` and `opticalSize`, with `iconVars`'s defaults for what a
/// variation leaves unset (a stroke-role width of 1.5, a gap of 0.75).
pub fn icon_vars(
    variation: SVGIconVariation,
    size: SVGIconSize,
    stroke_width: Option<f32>,
) -> IconVars {
    use IconPaint::{CurrentColor as C, None as N};
    let layer = |fill, stroke, opacity, sr_opacity, sr_width| LayerVars {
        fill,
        stroke,
        opacity,
        stroke_role_fill: N,
        stroke_role_stroke: C,
        stroke_role_opacity: sr_opacity,
        stroke_role_width: sr_width,
    };
    let (primary, secondary) = match variation {
        SVGIconVariation::Linear => (layer(N, C, 1., 1., 1.5), layer(N, C, 1., 1., 1.5)),
        SVGIconVariation::Bold => (layer(C, N, 1., 1., 2.), layer(C, N, 1., 1., 2.5)),
        SVGIconVariation::Twotone => (layer(N, C, 1., 0.4, 1.5), layer(N, C, 0.4, 0.4, 1.5)),
        SVGIconVariation::Bulk => (layer(C, N, 1., 0.4, 1.5), layer(C, N, 0.4, 0.4, 1.5)),
        SVGIconVariation::Broken => (layer(N, C, 1., 1., 1.5), layer(N, C, 0., 0., 1.5)),
    };
    let (px_size, optical) = size.optical();
    IconVars {
        size: px_size,
        stroke_width: stroke_width.unwrap_or(optical),
        gap: 0.75,
        primary,
        secondary,
    }
}

/// One drawn layer group: its SVG document and the opacity the icon color takes.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerDocument {
    /// Which group: `primary-fill`, `primary-stroke`, `secondary-fill` or
    /// `secondary-stroke`.
    pub group: &'static str,
    /// The group as a standalone SVG document, painted in black.
    pub svg: String,
    /// The group's opacity.
    pub opacity: f32,
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// React's attribute names: a camelCase prop (`strokeWidth`) is its kebab-case
/// attribute, but for the names SVG spells in camelCase itself.
fn attribute_name(name: &str) -> String {
    if matches!(name, "viewBox" | "pathLength" | "preserveAspectRatio")
        || !name.chars().any(|c| c.is_ascii_uppercase())
    {
        return name.to_string();
    }
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn shape_markup(shape: &IconShape, extra: &[(&str, String)]) -> String {
    let mut out = format!("<{}", shape.ty.tag());
    for (name, value) in &shape.attrs {
        out.push_str(&format!(" {}=\"{}\"", attribute_name(name), escape(value)));
    }
    for (name, value) in extra {
        out.push_str(&format!(" {name}=\"{}\"", escape(value)));
    }
    out.push_str("/>");
    out
}

fn number(n: f32) -> String {
    rustryx_theme::authoring::js::js_number_to_string(f64::from(n))
}

/// Upstream `splitByRole`: fill-role shapes (the default) and stroke-role shapes.
pub fn split_by_role(shapes: &[IconShape]) -> (Vec<IconShape>, Vec<IconShape>) {
    let mut fill = Vec::new();
    let mut stroke = Vec::new();
    for shape in shapes {
        if shape.role == Some(IconShapeRole::Stroke) {
            stroke.push(shape.clone());
        } else {
            fill.push(shape.clone());
        }
    }
    (fill, stroke)
}

/// Upstream `needsMask`: bold, with primary fill shapes and a secondary layer.
pub fn needs_mask(
    variation: SVGIconVariation,
    primary_fill: &[IconShape],
    secondary: &[IconShape],
) -> bool {
    variation == SVGIconVariation::Bold && !primary_fill.is_empty() && !secondary.is_empty()
}

/// Upstream `SVGIcon`'s rendering, as one SVG document per layer group in drawing
/// order: the primary fill-role group (masked in bold when there is a secondary
/// layer), the primary stroke-role group, then the secondary groups (left out in bold
/// with a mask, where every secondary shape is a knockout). An empty group draws
/// nothing.
pub fn layer_documents(
    icon: &SVGIconDef,
    vars: &IconVars,
    variation: SVGIconVariation,
) -> Vec<LayerDocument> {
    let view_box = icon.view_box.as_deref().unwrap_or("0 0 24 24");
    let secondary = icon.secondary.clone().unwrap_or_default();
    let has_secondary = !secondary.is_empty();
    let (primary_fill, primary_stroke) = split_by_role(&icon.primary);
    let (secondary_fill, secondary_stroke) = if has_secondary {
        split_by_role(&secondary)
    } else {
        (Vec::new(), Vec::new())
    };
    let all_secondary: Vec<IconShape> = secondary_fill
        .iter()
        .chain(secondary_stroke.iter())
        .cloned()
        .collect();
    let use_mask = needs_mask(variation, &primary_fill, &all_secondary);
    let caps = "stroke-linecap=\"round\" stroke-linejoin=\"round\"";
    let document = |defs: String, group: String| {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{}\">{defs}{group}</svg>",
            escape(view_box)
        )
    };
    let mut out = Vec::new();
    let fill_group = |shapes: &[IconShape], layer: &LayerVars, mask: bool| {
        let body: String = shapes.iter().map(|s| shape_markup(s, &[])).collect();
        format!(
            "<g fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" {caps}{}>{body}</g>",
            layer.fill.svg(),
            layer.stroke.svg(),
            number(vars.stroke_width),
            if mask { " mask=\"url(#knockout)\"" } else { "" }
        )
    };
    let stroke_group = |shapes: &[IconShape], layer: &LayerVars| {
        let body: String = shapes.iter().map(|s| shape_markup(s, &[])).collect();
        format!(
            "<g fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" {caps}>{body}</g>",
            layer.stroke_role_fill.svg(),
            layer.stroke_role_stroke.svg(),
            number(layer.stroke_role_width)
        )
    };
    if !primary_fill.is_empty() {
        // The mask: a full white plate, each secondary shape a black knockout. A
        // stroke-role knockout covers its visible stroke and the gap; a fill-role one
        // takes the gap as its border.
        let defs = if use_mask {
            let knockouts: String = all_secondary
                .iter()
                .map(|shape| {
                    let role = shape.role.unwrap_or_default();
                    let width = match role {
                        IconShapeRole::Stroke => vars.stroke_width + vars.gap * 2.,
                        IconShapeRole::Fill => vars.gap,
                    };
                    shape_markup(
                        shape,
                        &[
                            (
                                "fill",
                                if role == IconShapeRole::Fill {
                                    "black"
                                } else {
                                    "none"
                                }
                                .to_string(),
                            ),
                            ("stroke", "black".to_string()),
                            ("stroke-width", number(width)),
                            ("stroke-linecap", "round".to_string()),
                            ("stroke-linejoin", "round".to_string()),
                        ],
                    )
                })
                .collect();
            format!(
                "<defs><mask id=\"knockout\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"24\" height=\"24\"><rect width=\"24\" height=\"24\" fill=\"white\"/>{knockouts}</mask></defs>"
            )
        } else {
            String::new()
        };
        out.push(LayerDocument {
            group: "primary-fill",
            svg: document(defs, fill_group(&primary_fill, &vars.primary, use_mask)),
            opacity: vars.primary.opacity,
        });
    }
    if !primary_stroke.is_empty() {
        out.push(LayerDocument {
            group: "primary-stroke",
            svg: document(String::new(), stroke_group(&primary_stroke, &vars.primary)),
            opacity: vars.primary.stroke_role_opacity,
        });
    }
    if has_secondary && !use_mask {
        if !secondary_fill.is_empty() {
            out.push(LayerDocument {
                group: "secondary-fill",
                svg: document(
                    String::new(),
                    fill_group(&secondary_fill, &vars.secondary, false),
                ),
                opacity: vars.secondary.opacity,
            });
        }
        if !secondary_stroke.is_empty() {
            out.push(LayerDocument {
                group: "secondary-stroke",
                svg: document(
                    String::new(),
                    stroke_group(&secondary_stroke, &vars.secondary),
                ),
                opacity: vars.secondary.stroke_role_opacity,
            });
        }
    }
    out
}

/// Draws SVG documents stacked in a `size` box, each tinted with `color` at its
/// opacity.
pub(crate) fn stacked(size: f32, docs: Vec<(String, f32)>, color: Hsla) -> gpui::Div {
    let mut root = div().relative().flex_none().size(px(size));
    for (doc, opacity) in docs {
        if opacity <= 0. {
            continue;
        }
        let tint = Hsla {
            a: color.a * opacity,
            ..color
        };
        root = root.child(
            svg()
                .data(doc.as_bytes())
                .absolute()
                .top_0()
                .left_0()
                .size(px(size))
                .text_color(tint),
        );
    }
    root
}

/// Upstream lab `SVGIcon`: a CSS-variable-driven SVG icon with role-aware rendering.
/// Each shape's role decides how it behaves across variations; in bold, fill-role
/// shapes that overlap get a knockout mask for clean gaps.
///
/// ```ignore
/// SVGIcon::new(bell_icon()).variation(SVGIconVariation::Twotone).color(SVGIconColor::Accent)
/// ```
#[derive(IntoElement)]
pub struct SVGIcon {
    icon: SVGIconDef,
    variation: SVGIconVariation,
    size: SVGIconSize,
    color: SVGIconColor,
    stroke_width: Option<f32>,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl SVGIcon {
    /// Upstream `icon`: the definition holding the path data.
    pub fn new(icon: SVGIconDef) -> Self {
        SVGIcon {
            icon,
            variation: SVGIconVariation::Linear,
            size: SVGIconSize::Md,
            color: SVGIconColor::Primary,
            stroke_width: None,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `variation` (`Linear` by default).
    pub fn variation(mut self, variation: SVGIconVariation) -> Self {
        self.variation = variation;
        self
    }

    /// Upstream `size` (`Md` by default), with optical stroke compensation.
    pub fn size(mut self, size: SVGIconSize) -> Self {
        self.size = size;
        self
    }

    /// Upstream `color` (`Primary` by default).
    pub fn color(mut self, color: SVGIconColor) -> Self {
        self.color = color;
        self
    }

    /// Upstream `strokeWidth`: overrides the stroke width (bypasses the optical
    /// compensation).
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = Some(width);
        self
    }

    /// Upstream `aria-label`: names the icon, which is then an image; unnamed icons
    /// are decorative (upstream `iconA11yProps`).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Styled for SVGIcon {
    /// The icon box's style (upstream's extra `svg` props and `style`).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SVGIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let vars = icon_vars(self.variation, self.size, self.stroke_width);
        let color = self.color.resolve(window, cx);
        let docs = layer_documents(&self.icon, &vars, self.variation)
            .into_iter()
            .map(|d| (d.svg, d.opacity))
            .collect();
        let selector = format!("svg-icon-{}", self.icon.name);
        let mut root = stacked(vars.size, docs, color).debug_selector(move || selector);
        theme.refine(
            root.style(),
            "svg-icon",
            &[
                ("variation", self.variation.as_str()),
                ("size", self.size.as_str()),
                ("color", self.color.as_str()),
            ],
            None,
        );
        root.style().refine(&self.style);
        let root = root.id(gpui::ElementId::Name(
            format!("svg-icon-{}", self.icon.name).into(),
        ));
        match self.label {
            Some(label) => root.role(Role::Image).aria_label(label).into_any_element(),
            None => root.into_any_element(),
        }
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `SVGIcon`'s Rust documentation (its upstream doc lists no props).
pub const SVG_ICON_DOC: ComponentDoc = ComponentDoc {
    name: "SVGIcon",
    rust_type: "rustryx::lab::svg_icon::SVGIcon",
    props: &[],
    example: include_str!("examples.rs"),
};

/// Upstream lab `WeightedSVGIcon`'s Rust documentation.
pub const WEIGHTED_SVG_ICON_DOC: ComponentDoc = ComponentDoc {
    name: "WeightedSVGIcon",
    rust_type: "rustryx::lab::svg_icon::WeightedSVGIcon",
    props: &[
        p("icon", RustApi::Argument("icon (WeightedIconDef)")),
        p("size", RustApi::Method("size")),
        p("color", RustApi::Method("color")),
        p("weight", RustApi::Method("weight")),
        p("sprite", RustApi::Method("sprite")),
    ],
    example: include_str!("examples.rs"),
};
