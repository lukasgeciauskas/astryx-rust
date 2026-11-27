//! `CircularProgress`: completion as a ring (upstream lab `CircularProgress`).
//!
//! Determinate, an arc from 12 o'clock fills clockwise with the value; a value change
//! sweeps it from wherever it is drawn to the new value on the theme's medium duration
//! and standard easing, in every motion setting, as upstream's stroke transition did.
//! Indeterminate, Spinner's arc turns on Spinner's clock (slower under Reduce Motion).
//! The ring is a progress bar named by its label, which is visually hidden by default.

pub mod examples;

#[cfg(test)]
mod tests;

use std::f32::consts::{PI, TAU};
use std::rc::Rc;
use std::time::Duration;

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use astryx_components::layout::VisuallyHidden;
use astryx_primitives::prefers_reduced_motion;
use astryx_theme::authoring::js::{js_max, js_min, js_number_to_string, js_round};
use astryx_theme::{ThemeStyled, Tokens, current};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ElementId, Hsla, IntoElement, LineCap, ParentElement, PathBuilder,
    PathStyle, Pixels, Role, SharedString, StrokeOptions, StyleRefinement, Window, canvas, div,
    point, px,
};
use web_time::Instant;

/// The ring's diameter (upstream `CircularProgressSize`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CircularProgressSize {
    /// 32 px, a 3 px stroke.
    Sm,
    /// 48 px, a 4 px stroke.
    #[default]
    Md,
    /// 64 px, a 5 px stroke.
    Lg,
}

impl CircularProgressSize {
    /// The (diameter, stroke width) in pixels (upstream `SIZE_CONFIG`).
    pub fn geometry(self) -> (f32, f32) {
        match self {
            CircularProgressSize::Sm => (32., 3.),
            CircularProgressSize::Md => (48., 4.),
            CircularProgressSize::Lg => (64., 5.),
        }
    }

    /// The theme value (`sm`, `md`, `lg`).
    pub fn as_str(self) -> &'static str {
        match self {
            CircularProgressSize::Sm => "sm",
            CircularProgressSize::Md => "md",
            CircularProgressSize::Lg => "lg",
        }
    }
}

/// The fill's color role (upstream `CircularProgressVariant`, an extensible map).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CircularProgressVariant {
    #[default]
    Accent,
    Success,
    Warning,
    Error,
    Neutral,
    /// A variant a theme adds (upstream's `CircularProgressVariantMap` augmentation):
    /// the theme's `circular-progress-fill` rule for it colors the fill, which has no
    /// color of its own.
    Custom(SharedString),
}

impl CircularProgressVariant {
    /// The theme value.
    pub fn as_str(&self) -> &str {
        match self {
            CircularProgressVariant::Accent => "accent",
            CircularProgressVariant::Success => "success",
            CircularProgressVariant::Warning => "warning",
            CircularProgressVariant::Error => "error",
            CircularProgressVariant::Neutral => "neutral",
            CircularProgressVariant::Custom(name) => name,
        }
    }

    /// The fill's stroke (upstream `variantStyles`): one muted track for every
    /// variant, a semantic text role for the fill.
    fn stroke(&self, t: &Tokens) -> Option<Hsla> {
        let c = &t.color;
        Some(match self {
            CircularProgressVariant::Accent => c.accent,
            CircularProgressVariant::Success => c.text_success,
            CircularProgressVariant::Warning => c.text_warning,
            CircularProgressVariant::Error => c.text_error,
            // The secondary text role reaches 3:1 on the track in every bundled theme.
            CircularProgressVariant::Neutral => c.text_secondary,
            CircularProgressVariant::Custom(_) => return None,
        })
    }
}

/// Upstream `formatValueLabel`: the value and the maximum, as text.
pub type FormatValueLabel = Rc<dyn Fn(f64, f64) -> SharedString>;

/// Upstream's default label: the share as a whole percent (`Math.round`), 0% when the
/// maximum is not positive.
pub fn default_format_value_label(value: f64, max: f64) -> SharedString {
    let pct = if max > 0. {
        js_round(value / max * 100.)
    } else {
        0.
    };
    format!("{}%", js_number_to_string(pct)).into()
}

/// What the ring shows for a value and a maximum: a value or maximum that is not a
/// finite number counts as empty progress (upstream treats a NaN from `loaded / total`
/// as 0), and the value is held within 0 and the maximum.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressValues {
    /// The value, within 0 and the maximum (`aria-valuenow`).
    pub value: f64,
    /// The maximum, 0 when it is not finite (`aria-valuemax`).
    pub max: f64,
    /// The filled share, 0 to 1.
    pub percentage: f64,
}

impl ProgressValues {
    /// Upstream's `safeValue`, `safeMax`, `clampedValue` and `percentage`.
    pub fn new(value: f64, max: f64) -> Self {
        let safe_value = if value.is_finite() { value } else { 0. };
        let safe_max = if max.is_finite() { max } else { 0. };
        // `Math.min(Math.max(0, safeValue), safeMax)`: a negative maximum wins.
        let clamped = js_min(js_max(0., safe_value), safe_max);
        let percentage = if safe_max > 0. {
            clamped / safe_max
        } else {
            0.
        };
        ProgressValues {
            value: clamped,
            max: safe_max,
            percentage,
        }
    }
}

/// Each half-ring's turn and the leading cap's for a filled share `p` (0 to 1), in
/// degrees, and which parts show (upstream `fillTurns`). A half parked at the edge
/// of its half is hidden, so its antialiased end never leaves a hairline at 6 o'clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillTurns {
    pub right: f64,
    pub left: f64,
    pub cap: f64,
    pub right_shown: bool,
    pub left_shown: bool,
    pub caps: bool,
}

/// Upstream `fillTurns`.
pub fn fill_turns(p: f64) -> FillTurns {
    let turn = 360. * p;
    FillTurns {
        right: js_min(js_max(turn, 0.), 180.),
        left: js_min(js_max(turn, 180.), 360.),
        cap: turn,
        right_shown: p > 0.,
        left_shown: p > 0.5,
        caps: p > 0.,
    }
}

/// A circular progress indicator (upstream lab `CircularProgress`).
///
/// ```ignore
/// CircularProgress::new("upload", "Upload progress").value(75.).value_label(true)
/// CircularProgress::new("loading", "Loading").indeterminate(true)
/// ```
#[derive(IntoElement)]
pub struct CircularProgress {
    id: ElementId,
    label: SharedString,
    value: f64,
    max: f64,
    label_hidden: bool,
    value_label: bool,
    format_value_label: Option<FormatValueLabel>,
    size: CircularProgressSize,
    variant: CircularProgressVariant,
    indeterminate: bool,
    disabled: bool,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl CircularProgress {
    /// A ring named `label` (upstream `label`, required for accessibility); `id` keeps
    /// where its arc is drawn between frames.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        CircularProgress {
            id: id.into(),
            label: label.into(),
            value: 0.,
            max: 100.,
            label_hidden: true,
            value_label: false,
            format_value_label: None,
            size: CircularProgressSize::Md,
            variant: CircularProgressVariant::Accent,
            indeterminate: false,
            disabled: false,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    /// Upstream `value` (0 by default); ignored when indeterminate.
    pub fn value(mut self, value: f64) -> Self {
        self.value = value;
        self
    }

    /// Upstream `max` (100 by default).
    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    /// Upstream `isLabelHidden` (true by default): the label still names the ring.
    pub fn label_hidden(mut self, hidden: bool) -> Self {
        self.label_hidden = hidden;
        self
    }

    /// Upstream `hasValueLabel`: the formatted value in the ring's center, unless
    /// indeterminate or given children.
    pub fn value_label(mut self, shown: bool) -> Self {
        self.value_label = shown;
        self
    }

    /// Upstream `formatValueLabel`: the value label and `aria-valuetext`
    /// (`Math.round(value / max * 100)%` by default).
    pub fn format_value_label(
        mut self,
        format: impl Fn(f64, f64) -> SharedString + 'static,
    ) -> Self {
        self.format_value_label = Some(Rc::new(format));
        self
    }

    /// Upstream `size` (`Md` by default).
    pub fn size(mut self, size: CircularProgressSize) -> Self {
        self.size = size;
        self
    }

    /// Upstream `variant` (`Accent` by default).
    pub fn variant(mut self, variant: CircularProgressVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Upstream `isIndeterminate`: a turning arc for progress of unknown length.
    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }

    /// Upstream `isDisabled`: the neutral fill, and the whole ring at half opacity.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for CircularProgress {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for CircularProgress {
    /// Upstream `children`: the ring's center content, over the value label.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// Where the arc is drawn across frames (upstream's `drawn` ref and its running
/// animation): the value at rest, or a sweep from `from` to `to` begun at `start`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sweep {
    pub from: f64,
    pub to: f64,
    pub start: Option<Instant>,
}

impl Sweep {
    /// Where the sweep has reached on its easing at `now`, and whether it runs on.
    pub(crate) fn at(
        &self,
        now: Instant,
        duration: Duration,
        ease: astryx_theme::CubicBezier,
    ) -> (f64, bool) {
        match self.start {
            Some(start) if !duration.is_zero() => {
                let t = now.saturating_duration_since(start).as_secs_f32() / duration.as_secs_f32();
                if t >= 1. {
                    (self.to, false)
                } else {
                    (
                        self.from + (self.to - self.from) * f64::from(ease.ease(t)),
                        true,
                    )
                }
            }
            _ => (self.to, false),
        }
    }
}

/// The sweep after a render asks for `target` (`None` while indeterminate): a new
/// value sweeps from wherever the arc is drawn now (mid-change, the running change's
/// own curve says where); while indeterminate no value is drawn, so the next value
/// draws at once instead of sweeping from the value held before.
pub(crate) fn next_sweep(
    current: Option<Sweep>,
    target: Option<f64>,
    now: Instant,
    duration: Duration,
    ease: astryx_theme::CubicBezier,
) -> Option<Sweep> {
    match (target, current) {
        (None, _) => None,
        (Some(p), None) => Some(Sweep {
            from: p,
            to: p,
            start: None,
        }),
        (Some(p), Some(s)) if s.to != p => Some(Sweep {
            from: s.at(now, duration, ease).0,
            to: p,
            start: Some(now),
        }),
        (_, current) => current,
    }
}

/// The share to draw now, sweeping toward `target` from wherever the arc is drawn.
fn drawn_share(
    id: &ElementId,
    target: Option<f64>,
    duration: Duration,
    ease: astryx_theme::CubicBezier,
    window: &mut Window,
    cx: &mut App,
) -> Option<f64> {
    let now = cx.background_executor().now();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-circular-sweep").into()),
        cx,
        move |_, _| next_sweep(None, target, now, duration, ease),
    );
    let sweep = state.update(cx, |sweep, _| {
        *sweep = next_sweep(*sweep, target, now, duration, ease);
        *sweep
    });
    let (shown, moving) = sweep.map_or((None, false), |s| {
        let (p, moving) = s.at(now, duration, ease);
        (Some(p), moving)
    });
    if moving {
        window.request_animation_frame();
    }
    shown
}

/// The ring's stroke style.
fn stroke(width: f32, cap: LineCap) -> PathStyle {
    PathStyle::Stroke(StrokeOptions {
        line_width: width,
        line_cap: cap,
        ..StrokeOptions::default()
    })
}

/// Strokes the arc of radius `r` about `center` from angle `start` clockwise by
/// `sweep` radians (a full turn is a closed ring).
fn stroke_arc(
    window: &mut Window,
    center: gpui::Point<Pixels>,
    r: f32,
    width: f32,
    (start, sweep): (f32, f32),
    cap: LineCap,
    color: Hsla,
) {
    if sweep.is_nan() || r.is_nan() || sweep <= 0. || r <= 0. {
        return;
    }
    let at = |a: f32| point(center.x + px(r * a.cos()), center.y + px(r * a.sin()));
    let radii = point(px(r), px(r));
    let mut path = PathBuilder::stroke(px(width)).with_style(stroke(width, cap));
    if sweep >= TAU {
        path.move_to(at(0.));
        path.arc_to(radii, px(0.), false, true, at(PI));
        path.arc_to(radii, px(0.), false, true, at(TAU - 0.0001));
        path.close();
    } else {
        path.move_to(at(start));
        // Half turns at a time, so no single arc is ambiguous.
        let half = sweep.min(PI);
        path.arc_to(radii, px(0.), false, true, at(start + half));
        if sweep > PI {
            path.arc_to(radii, px(0.), false, true, at(start + sweep));
        }
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

impl RenderOnce for CircularProgress {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let (diameter, stroke_width) = self.size.geometry();
        let radius = (diameter - stroke_width) / 2.;
        let values = ProgressValues::new(self.value, self.max);
        let value_text = match &self.format_value_label {
            Some(format) => format(values.value, values.max),
            None => default_format_value_label(values.value, values.max),
        };

        // The fill's color: the variant's, the neutral role when disabled, or what the
        // theme's fill rule gives it.
        let fill_variant = if self.disabled {
            "disabled"
        } else {
            self.variant.as_str()
        };
        let mut fill_style = StyleRefinement::default();
        theme.refine(
            &mut fill_style,
            "circular-progress-fill",
            &[("variant", fill_variant)],
            None,
        );
        let fill_color = fill_style.text.color.or(if self.disabled {
            Some(t.color.text_secondary)
        } else {
            self.variant.stroke(t)
        });
        let mut track_style = StyleRefinement::default();
        theme.refine(&mut track_style, "circular-progress-track", &[], None);
        let track_color = track_style.text.color.unwrap_or(t.color.background_muted);

        let drawn = drawn_share(
            &self.id,
            (!self.indeterminate).then_some(values.percentage),
            t.duration.medium,
            t.ease.standard,
            window,
            cx,
        );
        // Spinner's rotor: one turn per `--duration-slow-min`, 3 s under reduced motion.
        let angle = if self.indeterminate {
            let period = if prefers_reduced_motion(cx) {
                Duration::from_secs(3)
            } else {
                t.duration.slow_min
            };
            let turns = astryx_components::motion::elapsed(cx).as_secs_f32()
                / period.as_secs_f32().max(0.001);
            window.request_animation_frame();
            Some(turns.fract() * TAU)
        } else {
            None
        };
        let ring = canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                let center = bounds.center();
                stroke_arc(
                    window,
                    center,
                    radius,
                    stroke_width,
                    (0., TAU),
                    LineCap::Butt,
                    track_color,
                );
                let Some(fill) = fill_color else {
                    return;
                };
                // The arcs start at 12 o'clock and turn clockwise.
                match (angle, drawn) {
                    (Some(angle), _) => {
                        let start = angle - PI / 2.;
                        stroke_arc(
                            window,
                            center,
                            radius,
                            stroke_width,
                            (start, TAU * SPINNER_ARC_FRACTION),
                            LineCap::Round,
                            fill,
                        );
                    }
                    (None, Some(p)) if p > 0. => {
                        let sweep = TAU * (p.min(1.) as f32);
                        stroke_arc(
                            window,
                            center,
                            radius,
                            stroke_width,
                            (-PI / 2., sweep),
                            LineCap::Round,
                            fill,
                        );
                    }
                    _ => {}
                }
            },
        )
        .flex_none()
        .size(px(diameter));

        let show_value_label = self.value_label && !self.indeterminate;
        let center: Option<AnyElement> = if !self.children.is_empty() {
            Some(div().children(self.children).into_any_element())
        } else if show_value_label {
            Some(
                div()
                    .text_style(&t.text.supporting)
                    .font_weight(t.font.weight.normal)
                    .text_color(if self.disabled {
                        t.color.text_disabled
                    } else {
                        t.color.text_secondary
                    })
                    .child(value_text.clone())
                    .into_any_element(),
            )
        } else {
            None
        };

        let mut progress = div()
            .id(ElementId::Name(format!("{}-ring", self.id).into()))
            .relative()
            .flex()
            .flex_none()
            .child(ring)
            .children(center.map(|content| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(content)
            }))
            .role(Role::ProgressIndicator)
            .aria_label(self.label.clone());
        if !self.indeterminate {
            progress = progress
                .aria_min_numeric_value(0.)
                .aria_max_numeric_value(values.max)
                .aria_numeric_value(values.value)
                .aria_value(value_text);
        }
        if self.disabled {
            // A disabled ring dims as a whole, center content included.
            progress = progress.opacity(0.5);
        }

        let label: AnyElement = if self.label_hidden {
            VisuallyHidden::new().child(self.label).into_any_element()
        } else {
            div()
                .text_style(&t.text.supporting)
                .font_weight(t.font.weight.medium)
                .text_color(if self.disabled {
                    t.color.text_disabled
                } else {
                    t.color.text_secondary
                })
                .child(self.label)
                .into_any_element()
        };

        let mut root = div()
            .relative()
            .inline_flex()
            .flex_none()
            .items_center()
            .justify_center();
        if !self.label_hidden {
            root = root.flex_col().gap(t.spacing.s1);
        }
        theme.refine(
            root.style(),
            "circular-progress",
            &[
                ("variant", self.variant.as_str()),
                ("size", self.size.as_str()),
            ],
            None,
        );
        root.style().refine(&self.style);
        root.id(self.id).child(label).child(progress)
    }
}

/// Spinner's arc: the share of the ring it covers (upstream `SPINNER_ARC_FRACTION`).
pub const SPINNER_ARC_FRACTION: f32 = 0.375;

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `CircularProgress`.
pub const CIRCULAR_PROGRESS_DOC: ComponentDoc = ComponentDoc {
    name: "CircularProgress",
    rust_type: "astryx::lab::circular_progress::CircularProgress",
    props: &[
        p("value", RustApi::Method("value")),
        p("max", RustApi::Method("max")),
        p("label", RustApi::Argument("label")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("hasValueLabel", RustApi::Method("value_label")),
        p("formatValueLabel", RustApi::Method("format_value_label")),
        p("children", RustApi::Trait("ParentElement")),
        p("size", RustApi::Method("size")),
        p("variant", RustApi::Method("variant")),
        p("isIndeterminate", RustApi::Method("indeterminate")),
        p("isDisabled", RustApi::Method("disabled")),
        p(
            "data-testid",
            RustApi::Waived("GPUI elements are found by their id, which the constructor takes"),
        ),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
