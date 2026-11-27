//! `Stat`: a KPI display (upstream lab `Stat`): the metric's name, its value large in
//! tabular numerals, an optional change with a direction glyph colored by sentiment,
//! a supporting line and a slot for a sparkline.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, FontFeatures, Hsla, LineCap, LineJoin, PathBuilder, PathStyle, Pixels,
    SharedString, StrokeOptions, StyleRefinement, Window, canvas, div, point, px,
};
use rustryx_components::layout::VisuallyHidden;
use rustryx_theme::ThemeStyled;

/// A change's trend (upstream `StatDeltaDirection`): picks the glyph and the default
/// sentiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatDeltaDirection {
    Up,
    Down,
    Flat,
}

impl StatDeltaDirection {
    /// The direction's default sentiment: up is good, down is bad.
    pub fn sentiment(self) -> StatDeltaSentiment {
        match self {
            StatDeltaDirection::Up => StatDeltaSentiment::Positive,
            StatDeltaDirection::Down => StatDeltaSentiment::Negative,
            StatDeltaDirection::Flat => StatDeltaSentiment::Neutral,
        }
    }

    /// What a screen reader hears after the change (upstream `DIRECTION_TEXT`).
    pub fn text(self) -> &'static str {
        match self {
            StatDeltaDirection::Up => "trending up",
            StatDeltaDirection::Down => "trending down",
            StatDeltaDirection::Flat => "flat",
        }
    }

    /// The glyph in a 12 × 12 box (upstream `DELTA_GLYPH_PATHS`): an arrow up or down
    /// to the right, or a dash.
    fn glyph(self) -> &'static [&'static [(f32, f32)]] {
        match self {
            StatDeltaDirection::Up => &[
                &[(3.5, 8.5), (8.5, 3.5)],
                &[(4.75, 3.5), (8.5, 3.5), (8.5, 7.25)],
            ],
            StatDeltaDirection::Down => &[
                &[(3.5, 3.5), (8.5, 8.5)],
                &[(4.75, 8.5), (8.5, 8.5), (8.5, 4.75)],
            ],
            StatDeltaDirection::Flat => &[&[(2.5, 6.), (9.5, 6.)]],
        }
    }
}

/// A change's tone (upstream `StatDeltaSentiment`), which overrides the direction's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatDeltaSentiment {
    Positive,
    Negative,
    Neutral,
}

impl StatDeltaSentiment {
    pub fn as_str(self) -> &'static str {
        match self {
            StatDeltaSentiment::Positive => "positive",
            StatDeltaSentiment::Negative => "negative",
            StatDeltaSentiment::Neutral => "neutral",
        }
    }
}

/// A change next to the value (upstream `StatDelta`).
#[derive(Clone, Debug, PartialEq)]
pub struct StatDelta {
    /// Formatted change text, such as "+12.4%".
    pub value: SharedString,
    pub direction: StatDeltaDirection,
    /// For inverted metrics where down is good (error rate, latency).
    pub sentiment: Option<StatDeltaSentiment>,
}

impl StatDelta {
    pub fn new(value: impl Into<SharedString>, direction: StatDeltaDirection) -> Self {
        StatDelta {
            value: value.into(),
            direction,
            sentiment: None,
        }
    }

    pub fn sentiment(mut self, sentiment: StatDeltaSentiment) -> Self {
        self.sentiment = Some(sentiment);
        self
    }

    /// Its sentiment: the one given, else its direction's.
    pub fn resolved_sentiment(&self) -> StatDeltaSentiment {
        self.sentiment.unwrap_or(self.direction.sentiment())
    }
}

/// The value's size (upstream `StatSize`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl StatSize {
    pub fn as_str(self) -> &'static str {
        match self {
            StatSize::Sm => "sm",
            StatSize::Md => "md",
            StatSize::Lg => "lg",
        }
    }
}

/// A KPI display (upstream lab `Stat`).
///
/// ```ignore
/// Stat::new("Error rate", "0.42%")
///     .delta(StatDelta::new("-0.08%", StatDeltaDirection::Down).sentiment(StatDeltaSentiment::Positive))
///     .description("vs. previous 30 days")
/// ```
#[derive(IntoElement)]
pub struct Stat {
    label: SharedString,
    value: AnyElement,
    delta: Option<StatDelta>,
    description: Option<SharedString>,
    media: Option<AnyElement>,
    size: StatSize,
    style: StyleRefinement,
}

impl Stat {
    /// A stat named `label` (upstream `label`) showing `value` (upstream `value`, a
    /// formatted string such as "1.2M", or any element).
    pub fn new(label: impl Into<SharedString>, value: impl IntoElement) -> Self {
        Stat {
            label: label.into(),
            value: value.into_any_element(),
            delta: None,
            description: None,
            media: None,
            size: StatSize::Md,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `delta`.
    pub fn delta(mut self, delta: StatDelta) -> Self {
        self.delta = Some(delta);
        self
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `media`: a sparkline or trend glyph below the text.
    pub fn media(mut self, media: impl IntoElement) -> Self {
        self.media = Some(media.into_any_element());
        self
    }

    /// Upstream `size` (`Md` by default).
    pub fn size(mut self, size: StatSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for Stat {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The direction glyph, stroked in `color` 1.5 px wide with round ends.
fn glyph(direction: StatDeltaDirection, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let stroke = StrokeOptions {
                line_width: 1.5,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..StrokeOptions::default()
            };
            for segment in direction.glyph() {
                let mut b = PathBuilder::stroke(px(1.5)).with_style(PathStyle::Stroke(stroke));
                for (i, (x, y)) in segment.iter().enumerate() {
                    let at = bounds.origin + point(px(*x), px(*y));
                    if i == 0 { b.move_to(at) } else { b.line_to(at) }
                }
                if let Ok(path) = b.build() {
                    window.paint_path(path, color);
                }
            }
        },
    )
    .flex_none()
    .size(px(12.))
}

impl RenderOnce for Stat {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let tabular = FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]));
        let size = match self.size {
            StatSize::Sm => t.font.size.xl,
            StatSize::Md => t.font.size.s2xl,
            StatSize::Lg => t.font.size.s3xl,
        };
        let value = div()
            .font_stack(&t.font.family.heading)
            .font_weight(t.font.weight.semibold)
            .text_size(size)
            .line_height(gpui::DefiniteLength::Fraction(t.text.heading_2.leading))
            .text_color(t.color.text_primary)
            .font_features(tabular.clone())
            .child(self.value);
        let delta = self.delta.map(|delta| {
            let sentiment = delta.resolved_sentiment();
            let color = match sentiment {
                StatDeltaSentiment::Positive => t.color.text_success,
                StatDeltaSentiment::Negative => t.color.text_error,
                StatDeltaSentiment::Neutral => t.color.text_secondary,
            };
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s1)
                .text_style(&t.text.supporting)
                .font_weight(t.font.weight.medium)
                .font_features(tabular)
                .whitespace_nowrap()
                .text_color(color)
                .child(glyph(delta.direction, color))
                .child(delta.value)
                .child(VisuallyHidden::new().child(format!("({})", delta.direction.text())))
        });
        let mut root = div()
            .flex()
            .flex_col()
            .items_start()
            .gap(t.spacing.s1)
            .min_w_0()
            .font_stack(&t.font.family.body);
        theme.refine(root.style(), "stat", &[("size", self.size.as_str())], None);
        root.style().refine(&self.style);
        root.child(
            div()
                .text_style(&t.text.supporting)
                .font_weight(t.font.weight.medium)
                .text_color(t.color.text_secondary)
                .child(self.label),
        )
        .child(
            div()
                .flex()
                .items_baseline()
                .gap(t.spacing.s2)
                .min_w_0()
                .child(value)
                .children(delta),
        )
        .children(self.description.map(|d| {
            div()
                .text_style(&t.text.supporting)
                .text_color(t.color.text_secondary)
                .child(d)
        }))
        .children(
            self.media
                .map(|m| div().mt(t.spacing.s1).self_stretch().min_w_0().child(m)),
        )
    }
}
