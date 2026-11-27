//! The Sankey marks (upstream `SankeyGrid`, `SankeyLink`, `SankeyNode` and
//! `SankeyLabel`): each reads the chart's [`SankeyContext`] and draws over the whole
//! plot, in the order the chart lists them.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, ContentMask, Hsla, PathBuilder, Pixels, Rgba, SharedString, Window, canvas, div,
    linear_color_stop, linear_gradient, px, relative, rgb,
};
use rustryx_theme::ThemeStyled;
use rustryx_theme::authoring::color::{oklch_clamped_hex, parse_color};
use rustryx_theme::authoring::js::{js_max, js_min, js_number_to_string, js_round, js_to_fixed};

use super::{Oklch, SankeyContext, SankeyLinkLayout, SankeyNodeLayout, use_sankey};
use crate::radial::marks::{TextAnchor, at, faded};

/// `oklch(L C H / alpha)` as a GPUI color (a color outside sRGB loses chroma first, as
/// CSS gamut mapping does).
pub fn oklch_color(c: Oklch, alpha: f64) -> Hsla {
    let hex = oklch_clamped_hex(c[0], c[1], c[2]);
    let mut color = parsed(&hex);
    color.a = alpha.clamp(0., 1.) as f32;
    color
}

/// A parsed CSS color as GPUI's, black when it does not parse.
fn parsed(css: &str) -> Hsla {
    match parse_color(css) {
        Some(c) => Rgba {
            r: (c.r / 255.) as f32,
            g: (c.g / 255.) as f32,
            b: (c.b / 255.) as f32,
            a: c.a as f32,
        }
        .into(),
        None => gpui::black(),
    }
}

/// A CSS color the chart takes (`nodeColor`, a flat link color, the grid's): hex,
/// `rgb()` or a basic name; anything else is black.
fn css_color(css: &str) -> Hsla {
    parsed(css)
}

/// Upstream `onSurfaceColor`'s choice: whether text on a bar takes the on-dark color.
/// With a CSS override, a quick check for dark-looking colors (`black`, `#0`–`#3`
/// leads, an `rgb()` red under 80); else the OKLCH lightness below 0.6.
pub fn on_surface_dark(color: Oklch, css_override: Option<&str>) -> bool {
    if let Some(css) = css_override.filter(|c| !c.is_empty()) {
        let c = rustryx_theme::authoring::js::js_trim(&css.to_lowercase()).to_string();
        let b = c.as_bytes();
        let low = |ch: u8| (b'0'..=b'3').contains(&ch);
        let hex_dark = b.first() == Some(&b'#')
            && (b.get(1).is_some_and(|&x| low(x)) || b.get(2).is_some_and(|&x| low(x)));
        // /^rgb\(\s*[0-7]\d?\s*,/
        let rgb_dark = c.strip_prefix("rgb(").is_some_and(|rest| {
            let rest = rest
                .trim_start_matches(|ch: char| rustryx_theme::authoring::js::is_js_whitespace(ch));
            let mut chars = rest.chars();
            match chars.next() {
                Some(d) if ('0'..='7').contains(&d) => {
                    let rest: String = chars.collect();
                    let rest = match rest.chars().next() {
                        Some(d2) if d2.is_ascii_digit() => rest[1..].to_string(),
                        _ => rest,
                    };
                    rest.trim_start_matches(|ch: char| {
                        rustryx_theme::authoring::js::is_js_whitespace(ch)
                    })
                    .starts_with(',')
                }
                _ => false,
            }
        });
        return c == "black" || hex_dark || rgb_dark;
    }
    color[0] < 0.6
}

/// A link ribbon's outline (upstream's `d`): a band from the source's right edge to
/// the target's left edge, its curves pulled `tension` of the way across.
pub fn link_path(link: &SankeyLinkLayout, nodes: &[SankeyNodeLayout], tension: f64) -> String {
    let n = js_number_to_string;
    let (s, t) = (&nodes[link.source], &nodes[link.target]);
    let sx = s.x + s.width;
    let tx = t.x;
    let (sy, ty, lh) = (link.source_y, link.target_y, link.height);
    let dx = (tx - sx) * tension;
    format!(
        "M{},{} C{},{} {},{} {},{} L{},{} C{},{} {},{} {},{} Z",
        n(sx),
        n(sy),
        n(sx + dx),
        n(sy),
        n(tx - dx),
        n(ty),
        n(tx),
        n(ty),
        n(tx),
        n(ty + lh),
        n(tx - dx),
        n(ty + lh),
        n(sx + dx),
        n(sy + lh),
        n(sx),
        n(sy + lh)
    )
}

/// Upstream `defaultFormat`: `12k` from 10,000, `1.2k` from 1,000, else the value
/// as the locale writes it (`number`).
pub fn default_format(value: f64, number: impl Fn(f64) -> String) -> String {
    if value >= 10000. {
        return format!("{}k", js_number_to_string(js_round(value / 1000.)));
    }
    if value >= 1000. {
        return format!("{}k", js_to_fixed(value / 1000., 1));
    }
    number(value)
}

/// Where a node's label goes (upstream `RotatedLabel` or `BesideLabel`).
#[derive(Clone, Debug, PartialEq)]
pub enum LabelPlacement {
    /// On a bar tall enough for the text: turned a quarter counterclockwise and centered.
    Rotated { cx: f64, cy: f64, dark: bool },
    /// Beside a short bar on a surface pill: after it, or before it in the last column.
    Beside {
        label_x: f64,
        cy: f64,
        anchor: TextAnchor,
        pill_x: f64,
        pill_width: f64,
    },
}

/// A node's label (upstream `SankeyLabel`'s per-node computation).
#[derive(Clone, Debug, PartialEq)]
pub struct NodeLabel {
    pub id: SharedString,
    /// `label = value`.
    pub text: SharedString,
    pub placement: LabelPlacement,
    /// The share of the largest column below the node, past the first column.
    pub percent: Option<(f64, f64, SharedString)>,
}

/// The labels: each node's `label = value`, rotated on a bar that fits it (by
/// upstream's 6.5 px per character estimate) and beside the bar otherwise.
pub fn node_labels(
    ctx: &SankeyContext,
    show_percent: bool,
    format: &dyn Fn(f64) -> String,
) -> Vec<NodeLabel> {
    let last_column = ctx.columns.len() as i64 - 1;
    ctx.nodes
        .iter()
        .map(|node| {
            let pct = (node.value / ctx.max_value) * 100.;
            let pct_str = if pct >= 10. {
                format!("{}%", js_number_to_string(js_round(pct)))
            } else {
                format!("{}%", js_to_fixed(pct, 1))
            };
            let text = format!("{} = {}", node.label, format(node.value));
            // `text.length` counts UTF-16 units.
            let length = text.encode_utf16().count() as f64;
            let fits_rotated = node.height >= length * 6.5 + 8.;
            let cx = node.x + ctx.node_width / 2.;
            let cy = node.y + node.height / 2.;
            let placement = if fits_rotated {
                LabelPlacement::Rotated {
                    cx,
                    cy,
                    dark: on_surface_dark(node.color, ctx.node_color.as_deref()),
                }
            } else {
                let last = node.column as i64 == last_column;
                let pill_width = length * 6. + 10.;
                let label_x = if last {
                    node.x - 6.
                } else {
                    node.x + ctx.node_width + 6.
                };
                LabelPlacement::Beside {
                    label_x,
                    cy,
                    anchor: if last {
                        TextAnchor::End
                    } else {
                        TextAnchor::Start
                    },
                    pill_x: if last {
                        label_x - pill_width + 4.
                    } else {
                        label_x - 4.
                    },
                    pill_width,
                }
            };
            let percent = (show_percent && node.column > 0).then(|| {
                (
                    cx,
                    js_min(ctx.height - 2., node.y + node.height + 12.),
                    pct_str.into(),
                )
            });
            NodeLabel {
                id: node.id.clone(),
                text: text.into(),
                placement,
                percent,
            }
        })
        .collect()
}

/// Upstream `strokeDasharray` parsed: numbers split by commas or spaces; anything
/// else draws a solid line.
pub(crate) fn dash_pattern(spec: &str) -> Option<Vec<Pixels>> {
    let parts: Vec<f64> = spec
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<f64>())
        .collect::<Result<_, _>>()
        .ok()?;
    if parts.is_empty() || parts.iter().any(|v| *v < 0.) || parts.iter().all(|v| *v == 0.) {
        return None;
    }
    Some(parts.into_iter().map(|v| px(v as f32)).collect())
}

/// Dashed vertical lines at each column, with optional column headers (upstream lab
/// `SankeyGrid`).
#[derive(IntoElement)]
pub struct SankeyGrid {
    dash_array: SharedString,
    color: Option<SharedString>,
    opacity: f64,
    show_headers: bool,
}

impl Default for SankeyGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl SankeyGrid {
    /// A grid of `4 4` dashes in the border color at 0.3 opacity, with headers.
    pub fn new() -> Self {
        SankeyGrid {
            dash_array: "4 4".into(),
            color: None,
            opacity: 0.3,
            show_headers: true,
        }
    }

    /// Upstream `dashArray`: the dash pattern, SVG's `stroke-dasharray`.
    pub fn dash_array(mut self, pattern: impl Into<SharedString>) -> Self {
        self.dash_array = pattern.into();
        self
    }

    /// Upstream `color`: a CSS color for the lines (the theme's border by default).
    pub fn color(mut self, color: impl Into<SharedString>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Upstream `opacity` (0.3 by default).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }

    /// Upstream `showHeaders`: the columns' labels below them (on by default).
    pub fn show_headers(mut self, show: bool) -> Self {
        self.show_headers = show;
        self
    }
}

impl RenderOnce for SankeyGrid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = use_sankey(cx);
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let color = faded(
            self.color
                .as_deref()
                .filter(|c| !c.is_empty())
                .map_or(t.color.border, css_color),
            self.opacity,
        );
        let dashes = dash_pattern(&self.dash_array);
        let xs: Vec<f64> = ctx
            .columns
            .iter()
            .map(|c| c.x + ctx.node_width / 2.)
            .collect();
        let height = ctx.height;
        let headers: Vec<_> = if self.show_headers {
            ctx.columns
                .iter()
                .filter_map(|c| {
                    let label = c.label.clone().filter(|l| !l.is_empty())?;
                    Some((c.x + ctx.node_width / 2., label))
                })
                .collect()
        } else {
            Vec::new()
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        for x in &xs {
                            if !x.is_finite() || !height.is_finite() {
                                continue;
                            }
                            let mut b = PathBuilder::stroke(px(1.));
                            if let Some(d) = &dashes {
                                b = b.dash_array(d);
                            }
                            b.move_to(at(bounds, (*x, 0.)));
                            b.line_to(at(bounds, (*x, height)));
                            if let Ok(path) = b.build() {
                                window.paint_path(path, color);
                            }
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .font_stack(&t.font.family.body)
            .text_size(px(10.))
            .line_height(relative(1.))
            .font_weight(gpui::FontWeight(500.))
            .text_color(t.color.text_secondary)
            .children(
                headers
                    .into_iter()
                    .filter(|(x, _)| x.is_finite())
                    .map(|(x, label)| {
                        baseline_text(
                            x,
                            height - 2.,
                            TextAnchor::Middle,
                            label.to_uppercase().into(),
                        )
                    }),
            )
    }
}

/// A label whose alphabetic baseline sits on `y` (SVG's default for `text`).
fn baseline_text(x: f64, y: f64, anchor: TextAnchor, text: SharedString) -> gpui::Div {
    crate::radial::marks::text_at(x, y, anchor, text).translate(
        relative(match anchor {
            TextAnchor::Start => 0.,
            TextAnchor::Middle => -0.5,
            TextAnchor::End => -1.,
        }),
        relative(-0.8),
    )
}

/// How the links are colored (upstream `SankeyLinkColor`).
#[derive(Clone, Debug, PartialEq)]
pub enum SankeyLinkColor {
    /// An even transition from the source's color to the target's (the default).
    Gradient,
    /// A transition leaning toward one end: 0.2 holds the source's color longer, 0.8
    /// the target's (upstream `{gradient: bias}`).
    Biased(f64),
    /// Each link in its source node's color.
    Source,
    /// Each link in its target node's color.
    Target,
    /// One CSS color for every link.
    Flat(SharedString),
}

/// A gradient link's stop offsets, 0 to 1 from the source's edge (upstream
/// `GradientDef`): the colors hold to 0.15 either side of `bias`, then cross.
pub fn gradient_offsets(bias: f64) -> [f64; 4] {
    [0., js_max(0., bias - 0.15), js_min(1., bias + 0.15), 1.]
}

/// The flow ribbons (upstream lab `SankeyLink`); place it before [`SankeyNode`] so the
/// nodes draw on top.
#[derive(IntoElement)]
pub struct SankeyLink {
    color: SankeyLinkColor,
    opacity: f64,
    tension: f64,
}

impl Default for SankeyLink {
    fn default() -> Self {
        Self::new()
    }
}

impl SankeyLink {
    /// Gradient ribbons at 0.7 opacity and 0.5 tension.
    pub fn new() -> Self {
        SankeyLink {
            color: SankeyLinkColor::Gradient,
            opacity: 0.7,
            tension: 0.5,
        }
    }

    /// Upstream `color` (`Gradient` by default).
    pub fn color(mut self, color: SankeyLinkColor) -> Self {
        self.color = color;
        self
    }

    /// Upstream `opacity`: the fills' opacity (0.7 by default).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }

    /// Upstream `tension`: 0 is straight, 1 the most curve (0.5 by default).
    pub fn tension(mut self, tension: f64) -> Self {
        self.tension = tension;
        self
    }
}

/// A ribbon's fill.
enum RibbonFill {
    Solid(Hsla),
    /// The gradient's stops from the source's edge to the target's: offsets 0 to 1.
    Stops([(Hsla, f32); 4]),
}

fn ribbon(
    bounds: Bounds<Pixels>,
    link: &SankeyLinkLayout,
    nodes: &[SankeyNodeLayout],
    tension: f64,
) -> Option<gpui::Path> {
    let (s, t) = (&nodes[link.source], &nodes[link.target]);
    let sx = s.x + s.width;
    let tx = t.x;
    let (sy, ty, lh) = (link.source_y, link.target_y, link.height);
    let dx = (tx - sx) * tension;
    if [sx, tx, sy, ty, lh, dx].iter().any(|v| !v.is_finite()) {
        return None;
    }
    let mut b = PathBuilder::fill();
    b.move_to(at(bounds, (sx, sy)));
    b.cubic_bezier_to(
        at(bounds, (tx, ty)),
        at(bounds, (sx + dx, sy)),
        at(bounds, (tx - dx, ty)),
    );
    b.line_to(at(bounds, (tx, ty + lh)));
    b.cubic_bezier_to(
        at(bounds, (sx, sy + lh)),
        at(bounds, (tx - dx, ty + lh)),
        at(bounds, (sx + dx, sy + lh)),
    );
    b.close();
    b.build().ok()
}

impl RenderOnce for SankeyLink {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = use_sankey(cx);
        let opacity = self.opacity;
        let fills: Vec<RibbonFill> = ctx
            .links
            .iter()
            .map(|link| {
                let (s, t) = (&ctx.nodes[link.source], &ctx.nodes[link.target]);
                match &self.color {
                    SankeyLinkColor::Flat(css) => RibbonFill::Solid(faded(css_color(css), opacity)),
                    SankeyLinkColor::Source => RibbonFill::Solid(oklch_color(s.color, opacity)),
                    SankeyLinkColor::Target => RibbonFill::Solid(oklch_color(t.color, opacity)),
                    SankeyLinkColor::Gradient | SankeyLinkColor::Biased(_) => {
                        let bias = match self.color {
                            SankeyLinkColor::Biased(b) => b,
                            _ => 0.5,
                        };
                        // SVG reads an unusable offset as 0 and never lets one fall
                        // behind the stop before it.
                        let mut last = 0f64;
                        let o = gradient_offsets(bias).map(|o| {
                            last = last.max(if o.is_finite() { o.clamp(0., 1.) } else { 0. });
                            last as f32
                        });
                        RibbonFill::Stops([
                            (oklch_color(s.color, opacity), o[0]),
                            (oklch_color(s.color, opacity * 0.9), o[1]),
                            (oklch_color(t.color, opacity * 0.9), o[2]),
                            (oklch_color(t.color, opacity), o[3]),
                        ])
                    }
                }
            })
            .collect();
        let ctx = Rc::new(ctx);
        let tension = self.tension;
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                for (link, fill) in ctx.links.iter().zip(&fills) {
                    match fill {
                        RibbonFill::Solid(color) => {
                            if let Some(path) = ribbon(bounds, link, &ctx.nodes, tension) {
                                window.paint_path(path, *color);
                            }
                        }
                        RibbonFill::Stops(stops) => {
                            // GPUI's gradients take two stops: the ribbon draws in runs
                            // across its width, each masked to its pair of stops.
                            let (s, t) = (&ctx.nodes[link.source], &ctx.nodes[link.target]);
                            let (sx, tx) = (s.x + s.width, t.x);
                            if !sx.is_finite() || !tx.is_finite() || sx == tx {
                                continue;
                            }
                            // Toward the target, whichever side it is on.
                            let angle = if tx > sx { 90. } else { 270. };
                            for pair in stops.windows(2) {
                                let (a, b) = (pair[0], pair[1]);
                                if b.1 <= a.1 {
                                    continue;
                                }
                                let x0 = sx + (tx - sx) * f64::from(a.1);
                                let x1 = sx + (tx - sx) * f64::from(b.1);
                                let mask = Bounds::from_corners(
                                    at(bounds, (x0.min(x1), -1e6)),
                                    at(bounds, (x0.max(x1), 1e6)),
                                );
                                if let Some(path) = ribbon(bounds, link, &ctx.nodes, tension) {
                                    window.with_content_mask(
                                        Some(ContentMask { bounds: mask }),
                                        |window| {
                                            window.paint_path(
                                                path,
                                                linear_gradient(
                                                    angle,
                                                    linear_color_stop(a.0, a.1),
                                                    linear_color_stop(b.0, b.1),
                                                ),
                                            );
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

/// The node bars, with an optional glow (upstream lab `SankeyNode`).
#[derive(IntoElement)]
pub struct SankeyNode {
    glow: bool,
}

impl Default for SankeyNode {
    fn default() -> Self {
        Self::new()
    }
}

impl SankeyNode {
    /// Bars with their glow.
    pub fn new() -> Self {
        SankeyNode { glow: true }
    }

    /// Upstream `glow`: a soft halo behind each bar (on by default).
    pub fn glow(mut self, glow: bool) -> Self {
        self.glow = glow;
        self
    }
}

fn round_rect(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    (x, y, w, h): (f64, f64, f64, f64),
    r: f64,
    color: Hsla,
) {
    if [x, y, w, h].iter().any(|v| !v.is_finite()) || w <= 0. || h <= 0. {
        return;
    }
    let r = r.min(w / 2.).min(h / 2.);
    window.paint_quad(
        gpui::fill(
            Bounds::new(at(bounds, (x, y)), gpui::size(px(w as f32), px(h as f32))),
            color,
        )
        .corner_radii(px(r as f32)),
    );
}

impl RenderOnce for SankeyNode {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = use_sankey(cx);
        let override_color = ctx
            .node_color
            .as_deref()
            .filter(|c| !c.is_empty())
            .map(css_color);
        let bars: Vec<_> = ctx
            .nodes
            .iter()
            .map(|n| {
                let fill = override_color.unwrap_or_else(|| oklch_color(n.color, 0.9));
                let glow = match override_color {
                    Some(c) => faded(c, 0.12),
                    None => oklch_color(n.color, 0.12),
                };
                ((n.x, n.y, n.width, n.height), fill, glow)
            })
            .collect();
        let glow = self.glow;
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                for ((x, y, w, h), fill, glow_fill) in &bars {
                    if glow {
                        round_rect(
                            window,
                            bounds,
                            (x - 3., y - 1., w + 6., h + 2.),
                            4.,
                            *glow_fill,
                        );
                    }
                    round_rect(window, bounds, (*x, *y, *w, *h), 1.5, *fill);
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

type FormatFn = Rc<dyn Fn(f64) -> String>;

/// The node labels (upstream lab `SankeyLabel`): rotated on a tall bar, beside a short
/// one on a surface pill (before it in the last column), with the node's share below.
#[derive(IntoElement)]
pub struct SankeyLabel {
    show_percent: bool,
    format_value: Option<FormatFn>,
}

impl Default for SankeyLabel {
    fn default() -> Self {
        Self::new()
    }
}

impl SankeyLabel {
    /// Labels with percentages and the compact default format.
    pub fn new() -> Self {
        SankeyLabel {
            show_percent: true,
            format_value: None,
        }
    }

    /// Upstream `showPercent`: the share below each node past the first column (on by
    /// default).
    pub fn show_percent(mut self, show: bool) -> Self {
        self.show_percent = show;
        self
    }

    /// Upstream `formatValue`: how a value reads (`12k`, `1.2k`, else the locale's
    /// number, by default).
    pub fn format_value(mut self, f: impl Fn(f64) -> String + 'static) -> Self {
        self.format_value = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for SankeyLabel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = use_sankey(cx);
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let i18n = rustryx_i18n::current(cx);
        let format: FormatFn = self
            .format_value
            .unwrap_or_else(|| Rc::new(move |v| default_format(v, |n| i18n.number(n))));
        let labels = node_labels(&ctx, self.show_percent, &*format);
        // Upstream's `var(--color-text-tertiary, #8e8ea0)`: no theme defines it, so the
        // fallback shows.
        let tertiary: Hsla = rgb(0x8e8ea0).into();
        let surface = faded(t.color.background_surface, 0.9);
        let pills: Vec<_> = labels
            .iter()
            .filter_map(|l| match l.placement {
                LabelPlacement::Beside {
                    cy,
                    pill_x,
                    pill_width,
                    ..
                } => Some((pill_x, cy - 8., pill_width, 16.)),
                _ => None,
            })
            .collect();
        let mut texts = Vec::new();
        for l in labels {
            match l.placement {
                LabelPlacement::Rotated { cx, cy, dark } => {
                    if cx.is_finite() && cy.is_finite() {
                        texts.push(
                            div()
                                .absolute()
                                .left(px(cx as f32))
                                .top(px(cy as f32))
                                .translate(relative(-0.5), relative(-0.5))
                                .rotate_degrees(-90.)
                                .whitespace_nowrap()
                                .font_weight(gpui::FontWeight(600.))
                                .text_size(px(10.))
                                .text_color(if dark {
                                    t.color.on_dark
                                } else {
                                    t.color.on_light
                                })
                                .child(l.text.clone())
                                .into_any_element(),
                        );
                    }
                }
                LabelPlacement::Beside {
                    label_x,
                    cy,
                    anchor,
                    ..
                } => {
                    if label_x.is_finite() && cy.is_finite() {
                        texts.push(
                            crate::radial::marks::text_at(label_x, cy, anchor, l.text.clone())
                                .font_weight(gpui::FontWeight(600.))
                                .text_size(px(10.))
                                .text_color(t.color.text_primary)
                                .into_any_element(),
                        );
                    }
                }
            }
            if let Some((x, y, pct)) = l.percent
                && x.is_finite()
                && y.is_finite()
            {
                texts.push(
                    baseline_text(x, y, TextAnchor::Middle, pct)
                        .font_weight(gpui::FontWeight(500.))
                        .text_size(px(9.))
                        .text_color(tertiary)
                        .into_any_element(),
                );
            }
        }
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .font_stack(&t.font.family.body)
            .line_height(relative(1.))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        for (x, y, w, h) in &pills {
                            round_rect(window, bounds, (*x, *y, *w, *h), 3., surface);
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .children(texts)
    }
}
