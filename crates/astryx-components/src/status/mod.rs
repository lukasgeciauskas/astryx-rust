//! Feedback and status: Spinner, Skeleton, Badge and StatusDot.
//! Upstream: `Spinner/`, `Skeleton/`, `Badge/`, `StatusDot/`.

pub mod examples;

use std::f32::consts::{PI, TAU};
use std::time::Duration;

use astryx_i18n::current as i18n;
use astryx_primitives::{prefers_reduced_motion, provide};
use astryx_theme::{ThemeStyled, Tokens, current};
use gpui::{
    AnyElement, App, Bounds, ElementId, Hsla, IntoElement, Length, ParentElement, PathBuilder,
    Pixels, RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, canvas, div, point,
    prelude::*, px, relative,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{IconDefaultSize, IconSize};
use crate::motion::elapsed;

// ----------------------------------------------------------------------------
// Spinner

/// Upstream `Spinner` `size`: ring diameters 10, 14, 18 and 28 px.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpinnerSize {
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

impl SpinnerSize {
    /// (diameter, stroke width) in pixels, upstream `SIZES`.
    pub fn geometry(self) -> (f32, f32) {
        match self {
            SpinnerSize::Sm => (10., 2.),
            SpinnerSize::Md => (14., 3.),
            SpinnerSize::Lg => (18., 3.),
            SpinnerSize::Xl => (28., 4.),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            SpinnerSize::Sm => "sm",
            SpinnerSize::Md => "md",
            SpinnerSize::Lg => "lg",
            SpinnerSize::Xl => "xl",
        }
    }
}

/// Upstream `shade`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpinnerShade {
    #[default]
    Default,
    /// Light, for dark media.
    OnMedia,
    Subtle,
    /// The surrounding text color.
    Inherit,
}

impl SpinnerShade {
    fn as_str(self) -> &'static str {
        match self {
            SpinnerShade::Default => "default",
            SpinnerShade::OnMedia => "onMedia",
            SpinnerShade::Subtle => "subtle",
            SpinnerShade::Inherit => "inherit",
        }
    }

    /// (arc color, track color) for a shade.
    fn colors(self, t: &Tokens, inherited: Hsla) -> (Hsla, Hsla) {
        match self {
            SpinnerShade::Default => (t.color.accent, t.color.track),
            SpinnerShade::Subtle => (t.color.text_secondary, t.color.track),
            SpinnerShade::OnMedia => (t.color.on_dark, t.color.on_dark.opacity(77. / 255.)),
            SpinnerShade::Inherit => (inherited, inherited.opacity(0.3)),
        }
    }
}

/// The share of the ring the arc covers, upstream `ARC_FRACTION`.
const ARC_FRACTION: f32 = 0.375;

/// A loading indicator for work of unknown length, upstream `Spinner`.
#[derive(IntoElement)]
pub struct Spinner {
    size: SpinnerSize,
    shade: SpinnerShade,
    label: Option<SharedString>,
    aria_label: Option<SharedString>,
    /// Upstream `aria-hidden`: no accessibility node, for a spinner that only
    /// echoes a busy state its control already reports.
    decorative: bool,
    id: ElementId,
    style: StyleRefinement,
}

impl Spinner {
    /// A spinner; `id` names it in the accessibility tree.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Spinner {
            size: SpinnerSize::Md,
            shade: SpinnerShade::Default,
            label: None,
            aria_label: None,
            decorative: false,
            id: id.into(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `size`.
        size: SpinnerSize,
        /// Upstream `shade`.
        shade: SpinnerShade,
        /// Upstream `label`: visible text below the ring, also its accessible name.
        ?label: SharedString,
        /// Upstream `aria-label`: the accessible name. Defaults to the label, else "Loading".
        ?aria_label: SharedString,
    }

    pub(crate) fn decorative(mut self) -> Self {
        self.decorative = true;
        self
    }
}

impl Styled for Spinner {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Paints the ring: a full track and an arc turned to `angle`.
fn paint_ring(
    bounds: Bounds<Pixels>,
    diameter: f32,
    stroke: f32,
    angle: f32,
    (arc, track): (Hsla, Hsla),
    window: &mut Window,
) {
    let center = bounds.center();
    let r = px(diameter / 2.);
    let at = |a: f32| point(center.x + r * a.cos(), center.y + r * a.sin());
    let style = |cap| {
        gpui::PathStyle::Stroke(gpui::StrokeOptions {
            line_width: stroke,
            line_cap: cap,
            ..Default::default()
        })
    };
    let mut ring = PathBuilder::stroke(px(stroke)).with_style(style(gpui::LineCap::Butt));
    ring.move_to(at(0.));
    ring.arc_to(point(r, r), px(0.), false, true, at(PI));
    ring.arc_to(point(r, r), px(0.), false, true, at(TAU - 0.0001));
    ring.close();
    if let Ok(path) = ring.build() {
        window.paint_path(path, track);
    }
    // The arc starts at 12 o'clock (`rotate(-90deg)`) and turns clockwise.
    let start = angle - PI / 2.;
    let end = start + TAU * ARC_FRACTION;
    let mut sweep = PathBuilder::stroke(px(stroke)).with_style(style(gpui::LineCap::Round));
    sweep.move_to(at(start));
    sweep.arc_to(point(r, r), px(0.), false, true, at(end));
    if let Ok(path) = sweep.build() {
        window.paint_path(path, arc);
    }
}

impl RenderOnce for Spinner {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let (diameter, stroke) = self.size.geometry();
        let frame = diameter + stroke * 2.;
        let colors = self.shade.colors(t, window.text_style().color);
        // One turn per `--duration-slow-min`; 3 s under reduced motion.
        let period = if prefers_reduced_motion(cx) {
            Duration::from_secs(3)
        } else {
            t.duration.slow_min
        };
        let turns = elapsed(cx).as_secs_f32() / period.as_secs_f32().max(0.001);
        let angle = turns.fract() * TAU;
        window.request_animation_frame();

        // A theme's `spinner` indicator replaces the ring (Indicator FR7).
        let ring = match crate::indicator::theme_indicator(crate::indicator::SPINNER, cx) {
            Some(draw) => div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(frame))
                .child(draw(
                    crate::indicator::IndicatorRequest {
                        state: "busy",
                        size: px(frame),
                    },
                    window,
                    cx,
                ))
                .into_any_element(),
            None => canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    paint_ring(bounds, diameter, stroke, angle, colors, window)
                },
            )
            .flex_none()
            .size(px(frame))
            .into_any_element(),
        };
        let name = self
            .aria_label
            .clone()
            .or_else(|| self.label.clone())
            .unwrap_or_else(|| i18n(cx).t("@astryx.spinner.loading"));
        let mut el = div().flex().flex_col().items_center().gap(t.spacing.s2);
        theme.refine(
            el.style(),
            "spinner",
            &[("size", self.size.as_str()), ("shade", self.shade.as_str())],
            None,
        );
        el.style().refine(&self.style);
        let el = el.id(self.id);
        let el = if self.decorative {
            el
        } else {
            el.role(Role::ProgressIndicator).aria_label(name)
        };
        el.child(ring).children(self.label.map(|label| {
            div()
                .text_style(&t.text.supporting)
                .text_color(t.color.text_secondary)
                .child(label)
        }))
    }
}

// ----------------------------------------------------------------------------
// Skeleton

/// Upstream `Skeleton` `radius`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkeletonRadius {
    None,
    R0,
    R1,
    R2,
    R3,
    R4,
    Rounded,
}

impl SkeletonRadius {
    fn token(self, t: &Tokens) -> gpui::AbsoluteLength {
        match self {
            SkeletonRadius::None => px(0.).into(),
            SkeletonRadius::R0 => t.radius.none,
            SkeletonRadius::R1 => t.radius.inner,
            SkeletonRadius::R2 => t.radius.element,
            SkeletonRadius::R3 | SkeletonRadius::R4 => t.radius.container,
            SkeletonRadius::Rounded => t.radius.full,
        }
    }
}

/// Upstream `DELAY_TIME` and `STAGGER_TIME`.
const SKELETON_DELAY: Duration = Duration::from_millis(1000);
const SKELETON_STAGGER: Duration = Duration::from_millis(100);

/// The skeleton's opacity `since` its animation began (after its delay):
/// ten steps from 0.25 to 0.925 and back, upstream `skeletonFade` run
/// `alternate`.
pub fn skeleton_opacity(since: Option<Duration>, period: Duration) -> f32 {
    let Some(since) = since else {
        return 0.25;
    };
    let cycle = since.as_secs_f32() / period.as_secs_f32().max(0.001);
    let forward = (cycle as u64).is_multiple_of(2);
    let progress = if forward {
        cycle.fract()
    } else {
        1. - cycle.fract()
    };
    let step = ((progress * 10.).floor() as u32).min(9);
    0.25 + 0.075 * step as f32
}

/// A pulsing placeholder in the shape of content that is loading, upstream
/// `Skeleton`.
#[derive(IntoElement)]
pub struct Skeleton {
    width: Length,
    height: Length,
    radius: SkeletonRadius,
    index: u32,
    style: StyleRefinement,
}

impl Skeleton {
    pub fn new() -> Self {
        Skeleton {
            width: relative(1.).into(),
            height: relative(1.).into(),
            radius: SkeletonRadius::R3,
            index: 0,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `width`. Defaults to the full width.
        width: Length,
        /// Upstream `height`. Defaults to the full height.
        height: Length,
        /// Upstream `radius`.
        radius: SkeletonRadius,
    }

    /// Upstream `index`: staggers its pulse after others.
    pub fn index(mut self, index: u32) -> Self {
        self.index = index;
        self
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Skeleton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Skeleton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let opacity = if prefers_reduced_motion(cx) {
            0.25
        } else {
            let delay = SKELETON_DELAY + SKELETON_STAGGER * self.index;
            let since = elapsed(cx).checked_sub(delay);
            window.request_animation_frame();
            skeleton_opacity(since, t.duration.medium_max)
        };
        let mut el = div()
            .w(self.width)
            .h(self.height)
            .bg(t.color.skeleton)
            .rounded(self.radius.token(t))
            .opacity(opacity);
        theme.refine(el.style(), "skeleton", &[], None);
        el.style().refine(&self.style);
        el
    }
}

// ----------------------------------------------------------------------------
// Badge

/// Upstream `Badge` `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Error,
    Blue,
    Cyan,
    Green,
    Orange,
    Pink,
    Purple,
    Red,
    Teal,
    Yellow,
}

impl BadgeVariant {
    /// (background, text) colors.
    pub fn colors(self, t: &Tokens) -> (Hsla, Hsla) {
        let c = &t.color;
        match self {
            BadgeVariant::Neutral => (c.neutral, c.text_primary),
            BadgeVariant::Info => (c.info, c.on_info),
            BadgeVariant::Success => (c.success, c.on_success),
            BadgeVariant::Warning => (c.warning, c.on_warning),
            BadgeVariant::Error => (c.error, c.on_error),
            BadgeVariant::Blue => (c.background_blue, c.text_blue),
            BadgeVariant::Cyan => (c.background_cyan, c.text_cyan),
            BadgeVariant::Green => (c.background_green, c.text_green),
            BadgeVariant::Orange => (c.background_orange, c.text_orange),
            BadgeVariant::Pink => (c.background_pink, c.text_pink),
            BadgeVariant::Purple => (c.background_purple, c.text_purple),
            BadgeVariant::Red => (c.background_red, c.text_red),
            BadgeVariant::Teal => (c.background_teal, c.text_teal),
            BadgeVariant::Yellow => (c.background_yellow, c.text_yellow),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            BadgeVariant::Neutral => "neutral",
            BadgeVariant::Info => "info",
            BadgeVariant::Success => "success",
            BadgeVariant::Warning => "warning",
            BadgeVariant::Error => "error",
            BadgeVariant::Blue => "blue",
            BadgeVariant::Cyan => "cyan",
            BadgeVariant::Green => "green",
            BadgeVariant::Orange => "orange",
            BadgeVariant::Pink => "pink",
            BadgeVariant::Purple => "purple",
            BadgeVariant::Red => "red",
            BadgeVariant::Teal => "teal",
            BadgeVariant::Yellow => "yellow",
        }
    }
}

/// A status or category tag, upstream `Badge`.
#[derive(IntoElement)]
pub struct Badge {
    label: SharedString,
    variant: BadgeVariant,
    icon: Option<AnyElement>,
    style: StyleRefinement,
}

impl Badge {
    /// Upstream `label`.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Badge {
            label: label.into(),
            variant: BadgeVariant::Neutral,
            icon: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `variant`.
        variant: BadgeVariant,
    }

    /// Upstream `icon`: a leading icon, sized for the badge.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }
}

impl Styled for Badge {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Badge {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let (mut bg, mut fg) = self.variant.colors(t);
        // Inside a solid Button a neutral badge inverts the button's pair, so
        // it keeps the fill's contrast (upstream Button FR2).
        if self.variant == BadgeVariant::Neutral
            && let Some(fill) = astryx_primitives::use_context::<crate::button::ButtonFill>(cx)
        {
            (bg, fg) = (fill.on_fill, fill.fill);
        }
        let mut el = div()
            .inline_flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.s1)
            .h(t.spacing.s5)
            .px(t.spacing.s2)
            .rounded(t.radius.full)
            .bg(bg)
            .text_color(fg)
            .text_style(&t.text.supporting)
            .font_weight(t.font.weight.medium)
            .max_w_full()
            .min_w_0();
        theme.refine(
            el.style(),
            "badge",
            &[("variant", self.variant.as_str())],
            None,
        );
        el.style().refine(&self.style);
        el.children(
            self.icon
                .map(|icon| provide(IconDefaultSize(IconSize::Xsm), icon)),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(self.label),
        )
    }
}

// ----------------------------------------------------------------------------
// StatusDot

/// Upstream `StatusDot` `variant`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusDotVariant {
    Success,
    Warning,
    Error,
    Accent,
    Neutral,
}

impl StatusDotVariant {
    fn colors(self, t: &Tokens) -> (Hsla, Hsla) {
        let c = &t.color;
        match self {
            StatusDotVariant::Success => (c.icon_success, c.background_surface),
            StatusDotVariant::Warning => (c.icon_warning, c.background_surface),
            StatusDotVariant::Error => (c.icon_error, c.background_surface),
            StatusDotVariant::Accent => (c.accent, c.on_accent),
            StatusDotVariant::Neutral => (c.icon_secondary, c.background_surface),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            StatusDotVariant::Success => "success",
            StatusDotVariant::Warning => "warning",
            StatusDotVariant::Error => "error",
            StatusDotVariant::Accent => "accent",
            StatusDotVariant::Neutral => "neutral",
        }
    }
}

/// Upstream `DOT_SIZE`.
const DOT_SIZE: Pixels = px(8.);

/// A small colored dot for a status, upstream `StatusDot`.
#[derive(IntoElement)]
pub struct StatusDot {
    variant: StatusDotVariant,
    label: SharedString,
    pulsing: bool,
    tooltip: Option<SharedString>,
    icon: Option<AnyElement>,
    id: ElementId,
    style: StyleRefinement,
}

impl StatusDot {
    /// Upstream `variant` and `label` (the accessible name).
    pub fn new(
        id: impl Into<ElementId>,
        variant: StatusDotVariant,
        label: impl Into<SharedString>,
    ) -> Self {
        StatusDot {
            variant,
            label: label.into(),
            pulsing: false,
            tooltip: None,
            icon: None,
            id: id.into(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `isPulsing`: a slow pulse, off under reduced motion.
        !pulsing,
        /// Upstream `tooltip`: shown on hover and keyboard focus.
        ?tooltip: SharedString,
    }

    /// Upstream `icon`: a mark drawn in the dot.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }
}

impl Styled for StatusDot {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for StatusDot {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let (bg, ink) = self.variant.colors(t);
        let opacity = if self.pulsing && !prefers_reduced_motion(cx) {
            window.request_animation_frame();
            // 1 → 0.5 → 1 over 2 s.
            let phase = (elapsed(cx).as_secs_f32() / 2.).fract();
            1. - 0.5 * (1. - (phase * 2. - 1.).abs())
        } else {
            1.
        };
        let mut el = div()
            .inline_flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(DOT_SIZE)
            .rounded_full()
            .bg(bg)
            .text_color(ink)
            .opacity(opacity);
        theme.refine(
            el.style(),
            "status-dot",
            &[("variant", self.variant.as_str())],
            None,
        );
        el.style().refine(&self.style);
        let id = self.id.clone();
        let el = el
            .id(self.id)
            .role(Role::Image)
            .aria_label(self.label)
            .children(
                self.icon
                    .map(|icon| provide(IconDefaultSize(IconSize::Xsm), icon)),
            );
        crate::overlay::with_tooltip(&id, self.tooltip, false, el)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const SPINNER_DOC: ComponentDoc = ComponentDoc {
    name: "Spinner",
    rust_type: "astryx::components::status::Spinner",
    props: &[
        p("size", RustApi::Method("size")),
        p("shade", RustApi::Method("shade")),
        p("label", RustApi::Method("label")),
        p("aria-label", RustApi::Method("aria_label")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const SKELETON_DOC: ComponentDoc = ComponentDoc {
    name: "Skeleton",
    rust_type: "astryx::components::status::Skeleton",
    props: &[
        p("width", RustApi::Method("width")),
        p("height", RustApi::Method("height")),
        p("radius", RustApi::Method("radius")),
        p("index", RustApi::Method("index")),
    ],
    example: include_str!("examples.rs"),
};

pub const BADGE_DOC: ComponentDoc = ComponentDoc {
    name: "Badge",
    rust_type: "astryx::components::status::Badge",
    props: &[
        p("variant", RustApi::Method("variant")),
        p("label", RustApi::Argument("label")),
        p("icon", RustApi::Method("icon")),
    ],
    example: include_str!("examples.rs"),
};

pub const STATUS_DOT_DOC: ComponentDoc = ComponentDoc {
    name: "StatusDot",
    rust_type: "astryx::components::status::StatusDot",
    props: &[
        p("variant", RustApi::Argument("variant")),
        p("label", RustApi::Argument("label")),
        p("isPulsing", RustApi::Method("pulsing")),
        p("tooltip", RustApi::Method("tooltip")),
        p("icon", RustApi::Method("icon")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_steps_up_then_back_down() {
        let period = Duration::from_millis(1000);
        assert_eq!(skeleton_opacity(None, period), 0.25);
        let at = |ms: u64| skeleton_opacity(Some(Duration::from_millis(ms)), period);
        assert_eq!(at(0), 0.25);
        assert!((at(150) - 0.325).abs() < 1e-6);
        assert!((at(950) - 0.925).abs() < 1e-6);
        // `alternate`: the second cycle runs backward.
        assert!((at(1050) - 0.925).abs() < 1e-6);
        assert!((at(1950) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn spinner_geometry_matches_upstream() {
        assert_eq!(SpinnerSize::Sm.geometry(), (10., 2.));
        assert_eq!(SpinnerSize::Xl.geometry(), (28., 4.));
    }
}
