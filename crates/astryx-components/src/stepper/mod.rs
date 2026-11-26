//! Stepper and Step. Upstream: `Stepper/`.
//!
//! Both indicator positions, in both orientations:
//! - `separated`: each step's bar sits above (or beside) its label row, which
//!   leads with the indicator;
//! - `on-track`: the indicators sit on one connector running through the
//!   steps, drawn as two segments per step.
//!
//! Steps can be clickable. When the flow advances one step, the fill sweeps
//! the span it crossed. A horizontal stepper narrower than its minimum width
//! per step turns compact: the labels give way to a summary row of the active
//! step under the track, with previous and next controls.

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{Direction, LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{control_focus, is_focus_visible, prefers_reduced_motion};
use astryx_theme::{ResolvedTheme, Tokens, current, scope};
use gpui::{
    AnyElement, App, Div, ElementId, Hsla, IntoElement, ParentElement, Pixels, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Window, canvas, div, prelude::*, px, relative, svg,
};
use smallvec::SmallVec;
use web_time::Instant;

use crate::button::{ButtonVariant, IconButton};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconColor, IconSize};
use crate::item::ItemDensity;
use crate::layout::{Orientation, VisuallyHidden};

/// Upstream Step `status`: the step's semantic color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepStatus {
    Accent,
    Success,
    Warning,
    Error,
}

impl StepStatus {
    fn as_str(self) -> &'static str {
        match self {
            StepStatus::Accent => "accent",
            StepStatus::Success => "success",
            StepStatus::Warning => "warning",
            StepStatus::Error => "error",
        }
    }
}

/// Upstream Step `indicator`.
#[derive(Default)]
pub enum StepIndicator {
    /// A number until completed, then a check; the active step a ring.
    #[default]
    Auto,
    /// Always the number.
    Number,
    /// No indicator.
    None,
    /// Any element.
    Custom(AnyElement),
}

/// Upstream Stepper `indicatorPosition`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IndicatorPosition {
    /// Each step's bar apart from its label row, which leads with the
    /// indicator.
    #[default]
    Separated,
    /// The indicators sit on one connector running through the steps.
    OnTrack,
}

impl IndicatorPosition {
    fn as_str(self) -> &'static str {
        match self {
            IndicatorPosition::Separated => "separated",
            IndicatorPosition::OnTrack => "on-track",
        }
    }
}

/// Upstream `horizontalOptions.collapsedVariant`: what a compact horizontal
/// stepper shows under its track.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CollapsedVariant {
    /// The active step's label, with previous and next controls when the
    /// steps are clickable.
    #[default]
    WithLabelAndControls,
    /// The active step's label alone.
    WithLabel,
    /// The bare track, with no summary row.
    HiddenLabel,
}

/// Upstream `horizontalOptions`: when and how a horizontal stepper collapses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizontalOptions {
    /// Below this width per step the stepper turns compact (upstream
    /// `minimumStepWidth`, 112px by default).
    pub minimum_step_width: Pixels,
    /// What the compact stepper shows (upstream `collapsedVariant`).
    pub collapsed_variant: CollapsedVariant,
}

impl Default for HorizontalOptions {
    fn default() -> Self {
        HorizontalOptions {
            minimum_step_width: px(112.),
            collapsed_variant: CollapsedVariant::WithLabelAndControls,
        }
    }
}

/// Where a step stands against the active one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StepState {
    Completed,
    Active,
    NotStarted,
}

impl StepState {
    fn as_str(self) -> &'static str {
        match self {
            StepState::Completed => "completed",
            StepState::Active => "in-progress",
            StepState::NotStarted => "not-started",
        }
    }
}

pub(crate) fn progress(step: usize, active: usize) -> StepState {
    match step.cmp(&active) {
        std::cmp::Ordering::Less => StepState::Completed,
        std::cmp::Ordering::Equal => StepState::Active,
        std::cmp::Ordering::Greater => StepState::NotStarted,
    }
}

/// The nearest enabled step before (`delta` < 0) or after the active one:
/// where a compact stepper's previous and next controls go.
pub(crate) fn adjacent_enabled(disabled: &[bool], active: usize, delta: isize) -> Option<usize> {
    if delta < 0 {
        (0..active.min(disabled.len()))
            .rev()
            .find(|&i| !disabled[i])
    } else {
        (active + 1..disabled.len()).find(|&i| !disabled[i])
    }
}

// Share of one node-to-node span drawn by the segment arriving at the far
// node; the segment leaving the near node draws the rest. Upstream splits the
// sweep's time the way the length is split, so the two halves move at about
// one speed: evenly in a row, 0.3 down a column, where the arriving half is a
// short cap.
const OT_ARRIVAL_SHARE_H: f32 = 0.5;
const OT_ARRIVAL_SHARE_V: f32 = 0.3;
// A vertical step with content continues its leaving half down the content's
// side; the rail above takes this share of the leaving time.
const OT_RAIL_SHARE_OF_LEAVING: f32 = 0.25;

/// The fill's sweep. Only a single forward step animates, across the span
/// leaving the step the flow came from; every other segment lands at once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sweep {
    /// The span being swept: span `k` runs from node `k` to node `k + 1`.
    pub span: Option<usize>,
    /// How far the sweep has run, in span durations.
    pub elapsed: f32,
}

impl Sweep {
    /// How much of a segment covering `[offset, offset + share)` of span
    /// `span` is filled, when the segment's target is `filled`.
    pub fn fill(self, span: isize, offset: f32, share: f32, filled: bool) -> f32 {
        if !filled {
            return 0.;
        }
        match self.span {
            Some(swept) if swept as isize == span => {
                if share <= 0. {
                    return if self.elapsed >= offset { 1. } else { 0. };
                }
                ((self.elapsed - offset) / share).clamp(0., 1.)
            }
            _ => 1.,
        }
    }
}

/// What a stepper keeps between frames.
#[derive(Default)]
struct Memory {
    /// The active step at the last render and the one before it.
    seen: Option<(usize, usize)>,
    /// When the active step last changed.
    changed_at: Option<Instant>,
    /// The list's width, for the compact mode.
    width: Option<Pixels>,
    /// Which steps are disabled, read again when a compact control is
    /// pressed, so a stale frame cannot select a newly disabled step.
    disabled: Vec<bool>,
}

/// One step of a [`Stepper`], upstream `Step`.
pub struct Step {
    label: SharedString,
    description: Option<SharedString>,
    status: Option<StepStatus>,
    indicator: StepIndicator,
    disabled: bool,
    optional: bool,
    end_content: Option<AnyElement>,
    density: Option<ItemDensity>,
    children: SmallVec<[AnyElement; 1]>,
}

impl Step {
    /// Upstream `label`. Its index is its place in the stepper.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Step {
            label: label.into(),
            description: None,
            status: None,
            indicator: StepIndicator::Auto,
            disabled: false,
            optional: false,
            end_content: None,
            density: None,
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `status`.
        ?status: StepStatus,
        /// Upstream `indicator`.
        indicator: StepIndicator,
        /// Upstream `isDisabled`: still focusable, never activates.
        !disabled,
        /// Upstream `isOptional`: "• Optional" after the label.
        !optional,
        /// Upstream `density`: else the stepper's.
        ?density: ItemDensity,
    }

    /// Upstream `endContent`: at the end of the label row.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }
}

impl ParentElement for Step {
    /// Upstream `children`: content under the label.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// The color a reached step takes for its status.
fn status_color(status: StepStatus, t: &Tokens) -> (Hsla, Hsla) {
    let c = &t.color;
    match status {
        StepStatus::Accent => (c.accent, c.on_accent),
        StepStatus::Success => (c.success, c.on_success),
        StepStatus::Warning => (c.warning, c.on_warning),
        StepStatus::Error => (c.error, c.on_error),
    }
}

/// Upstream's check, drawn in a 10-unit box.
const CHECK_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M8.5 2.5L4 7.5L1.5 5" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

type ClickFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// A numbered sequence of steps, upstream `Stepper`.
///
/// ```ignore
/// Stepper::new("checkout", 1)
///     .step(Step::new("Cart"))
///     .step(Step::new("Shipping"))
///     .step(Step::new("Payment"))
/// ```
#[derive(IntoElement)]
pub struct Stepper {
    id: ElementId,
    active_step: usize,
    steps: Vec<Step>,
    orientation: Orientation,
    on_step_click: Option<ClickFn>,
    label: Option<SharedString>,
    density: ItemDensity,
    indicator_position: IndicatorPosition,
    horizontal_options: HorizontalOptions,
    connector_gap: Option<Pixels>,
    style: StyleRefinement,
}

impl Stepper {
    /// Upstream `activeStep`: zero-based.
    pub fn new(id: impl Into<ElementId>, active_step: usize) -> Self {
        Stepper {
            id: id.into(),
            active_step,
            steps: Vec::new(),
            orientation: Orientation::Horizontal,
            on_step_click: None,
            label: None,
            density: ItemDensity::Balanced,
            indicator_position: IndicatorPosition::Separated,
            horizontal_options: HorizontalOptions::default(),
            connector_gap: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `orientation`.
        orientation: Orientation,
        /// Upstream `label`: "Progress" by default.
        ?label: SharedString,
        /// Upstream `density`.
        density: ItemDensity,
        /// Upstream `indicatorPosition`.
        indicator_position: IndicatorPosition,
        /// Upstream `horizontalOptions`.
        horizontal_options: HorizontalOptions,
    }

    /// Upstream `--step-connector-gap`: the hole the on-track connector
    /// leaves around each indicator, clamped to `0..=spacing-2`. It never
    /// changes the stepper's size.
    pub fn connector_gap(mut self, gap: impl Into<Pixels>) -> Self {
        self.connector_gap = Some(gap.into());
        self
    }

    /// Upstream `onStepClick`: steps become buttons.
    pub fn on_step_click(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_step_click = Some(Rc::new(f));
        self
    }

    /// Upstream `children`: one step.
    pub fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    /// Upstream `children`: steps.
    pub fn steps(mut self, steps: impl IntoIterator<Item = Step>) -> Self {
        self.steps.extend(steps);
        self
    }
}

impl Styled for Stepper {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Which edge of a connector segment faces an indicator.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Faces {
    Nothing,
    /// The segment leads into the indicator (its end, or its bottom).
    End,
    /// The segment leaves the indicator (its start, or its top).
    Start,
}

/// Paints connector segments (upstream `connectorTrack`): the track in the
/// border color, the accent fill grown over part of it from its start.
struct Connector {
    track: Hsla,
    accent: Hsla,
    dir: Direction,
    gap: Pixels,
}

impl Connector {
    /// A segment box; the caller sizes it. `visible` false keeps its space
    /// and paints nothing (upstream's first and last half segments).
    fn segment(
        &self,
        fill: f32,
        vertical: bool,
        faces: Faces,
        visible: bool,
        rounded: bool,
    ) -> Div {
        let el = div().relative().flex_none();
        if !visible {
            return el;
        }
        let mut paint = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .overflow_hidden()
            .bg(self.track);
        // The hole around the indicator, on the edge that faces it, mirrored
        // per axis and in RTL; a clip, so the layout never moves.
        paint = match (faces, vertical) {
            (Faces::Nothing, _) => paint,
            (Faces::End, true) => paint.bottom(self.gap),
            (Faces::Start, true) => paint.top(self.gap),
            (Faces::End, false) => paint.inset_end(self.dir, self.gap),
            (Faces::Start, false) => paint.inset_start(self.dir, self.gap),
        };
        if rounded {
            paint = paint.rounded_full();
        }
        if fill > 0. {
            let mut fill_el = div().absolute().bg(self.accent);
            fill_el = if vertical {
                fill_el.left_0().right_0().top_0().h(relative(fill))
            } else {
                fill_el
                    .top_0()
                    .bottom_0()
                    .inset_start(self.dir, px(0.))
                    .w(relative(fill))
            };
            if rounded {
                fill_el = fill_el.rounded_full();
            }
            paint = paint.child(fill_el);
        }
        el.child(paint)
    }
}

/// A step's indicator (upstream `indicatorNode`), in the colors of its
/// `step-indicator` theme part.
fn indicator(
    mode: StepIndicator,
    index: usize,
    status: Option<StepStatus>,
    disabled: bool,
    progress: StepState,
    t: &Tokens,
    size: gpui::AbsoluteLength,
) -> Option<AnyElement> {
    let reached = progress != StepState::NotStarted;
    let c = &t.color;
    match mode {
        StepIndicator::None => None,
        StepIndicator::Custom(el) => {
            let color = if disabled {
                c.icon_disabled
            } else {
                match status {
                    Some(status) => status_color(status, t).0,
                    None if reached => c.accent,
                    None => c.icon_secondary,
                }
            };
            Some(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(size)
                    .text_color(color)
                    .child(el)
                    .into_any_element(),
            )
        }
        mode => {
            let glyph = match status {
                Some(s @ (StepStatus::Success | StepStatus::Warning | StepStatus::Error))
                    if matches!(mode, StepIndicator::Auto) && progress != StepState::Active =>
                {
                    Some(s)
                }
                _ => None,
            };
            let number = glyph.is_none()
                && (matches!(mode, StepIndicator::Number) || progress == StepState::NotStarted);
            let el = if let Some(glyph) = glyph {
                let (icon, color) = match glyph {
                    StepStatus::Success => ("success", IconColor::Success),
                    StepStatus::Warning => ("warning", IconColor::Warning),
                    _ => ("error", IconColor::Error),
                };
                let color = if disabled { IconColor::Disabled } else { color };
                Icon::new(icon)
                    .size(IconSize::Sm)
                    .color(color)
                    .into_any_element()
            } else if number {
                let (bg, fg) = if disabled {
                    (c.background_muted, c.text_disabled)
                } else if reached {
                    match status {
                        Some(status) => status_color(status, t),
                        None => (c.accent, c.background_surface),
                    }
                } else {
                    (c.background_muted, c.text_secondary)
                };
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(size)
                    .rounded_full()
                    .bg(bg)
                    .text_color(fg)
                    .text_size(t.font.size.xs)
                    .font_weight(t.font.weight.semibold)
                    .child((index + 1).to_string())
                    .into_any_element()
            } else {
                let color = if disabled { c.icon_disabled } else { c.accent };
                if progress == StepState::Completed {
                    // A filled circle around a check in the surface color.
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(size)
                        .rounded_full()
                        .bg(color)
                        .child(
                            svg()
                                .data(CHECK_SVG)
                                .size(px(10.))
                                .text_color(c.background_surface),
                        )
                        .into_any_element()
                } else {
                    // The active step: a filled dot in a ring.
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(size)
                        .rounded_full()
                        .border_2()
                        .border_color(color)
                        .child(div().size(px(8.)).rounded_full().bg(color))
                        .into_any_element()
                }
            };
            let mut wrap = div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(size);
            if disabled && !number {
                wrap = wrap.opacity(0.5);
            }
            Some(wrap.child(el).into_any_element())
        }
    }
}

impl RenderOnce for Stepper {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let c = &t.color;
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let i18n = i18n(cx);
        let vertical = self.orientation == Orientation::Vertical;
        let on_track = self.indicator_position == IndicatorPosition::OnTrack;
        let id = self.id.clone();
        let count = self.steps.len();
        let active = self.active_step;
        let s = &t.spacing;
        let bar = s.s1;
        let indicator_size = s.s4;
        let gutter = s.s6;

        // What the stepper remembers: the step it came from, and its width.
        let memory = window
            .use_keyed_state(
                ElementId::Name(format!("{id}-stepper").into()),
                cx,
                |_, _| Rc::new(RefCell::new(Memory::default())),
            )
            .read(cx)
            .clone();
        let now = cx.background_executor().now();
        let previous = {
            let mut m = memory.borrow_mut();
            m.disabled = self.steps.iter().map(|s| s.disabled).collect();
            match m.seen {
                Some((current, previous)) if current == active => previous,
                Some((current, _)) => {
                    m.seen = Some((active, current));
                    m.changed_at = Some(now);
                    current
                }
                None => {
                    m.seen = Some((active, active));
                    active
                }
            }
        };
        let changed_at = memory.borrow().changed_at;
        let span_time = if prefers_reduced_motion(cx) {
            Duration::ZERO
        } else {
            t.duration.medium
        };
        let sweep = match changed_at {
            Some(at) if active == previous + 1 && !span_time.is_zero() => {
                let elapsed =
                    now.saturating_duration_since(at).as_secs_f32() / span_time.as_secs_f32();
                if elapsed < 1. {
                    window.request_animation_frame();
                }
                Sweep {
                    span: Some(previous),
                    elapsed,
                }
            }
            _ => Sweep {
                span: None,
                elapsed: 1.,
            },
        };

        // Compact: a horizontal stepper narrower than its minimum per step.
        let options = self.horizontal_options;
        let compact = !vertical
            && count > 0
            && memory
                .borrow()
                .width
                .is_some_and(|w| w > px(0.) && w / (count as f32) < options.minimum_step_width);
        let clickable = self.on_step_click.is_some() && !compact;

        let connector = Connector {
            track: c.border,
            accent: c.accent,
            dir,
            gap: self
                .connector_gap
                .unwrap_or(px(0.))
                .clamp(px(0.), s.s2.to_pixels(rem)),
        };

        let mut summary: Option<AnyElement> = None;
        let mut steps = Vec::with_capacity(count);
        for (i, mut step) in self.steps.into_iter().enumerate() {
            let progress = progress(i, active);
            let is_active = progress == StepState::Active;
            let disabled = step.disabled;
            let density = step.density.unwrap_or(self.density);
            let block = match density {
                ItemDensity::Compact => s.s1,
                ItemDensity::Balanced => s.s2,
                ItemDensity::Spacious => s.s3,
            };
            let inline = if density == ItemDensity::Spacious {
                s.s3
            } else {
                s.s2
            };

            // Theme parts and their props (upstream themeProps).
            let mut step_props: SmallVec<[(&str, &str); 4]> =
                SmallVec::from_slice(&[("progress", progress.as_str())]);
            if let Some(status) = step.status {
                step_props.push(("status", status.as_str()));
            }
            let description_props = step_props.clone();
            if is_active {
                step_props.push(("selected", "selected"));
            }
            let mut part_props = step_props.clone();
            if disabled {
                part_props.push(("disabled", "disabled"));
            }

            // The indicator, in its part's tokens.
            let part = theme.for_part("step-indicator", &part_props);
            let it = part.as_ref().map(ResolvedTheme::tokens).unwrap_or(t);
            let mode = std::mem::take(&mut step.indicator);
            let indicator = indicator(mode, i, step.status, disabled, progress, it, indicator_size)
                .map(|mut el| {
                    if let Some(style) = el_style(&theme, &part_props) {
                        el = div()
                            .flex_none()
                            .child(el)
                            .map(|mut d| {
                                d.style().refine(&style);
                                d
                            })
                            .into_any_element();
                    }
                    match part.clone() {
                        Some(part) => scope(part, el).into_any_element(),
                        None => el,
                    }
                });
            let has_indicator = indicator.is_some();

            // What a step announces beyond its label (upstream statusText).
            let status_text = match step.status {
                Some(StepStatus::Error) => Some(i18n.t("@astryx.step.status.error")),
                Some(StepStatus::Warning) => Some(i18n.t("@astryx.step.status.warning")),
                Some(StepStatus::Success) => Some(i18n.t("@astryx.step.status.completed")),
                _ if progress == StepState::Completed => {
                    Some(i18n.t("@astryx.step.status.completed"))
                }
                _ => None,
            };

            let label_color = if disabled {
                c.text_disabled
            } else if progress == StepState::NotStarted {
                c.text_secondary
            } else {
                c.text_primary
            };
            let label_text = step.label.clone();
            let label = || {
                let mut el = div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(t.text.body.size)
                    .line_height(relative(t.text.body.leading))
                    .font_weight(if is_active {
                        t.font.weight.semibold
                    } else {
                        t.font.weight.normal
                    })
                    .text_color(label_color)
                    .child(label_text.clone());
                theme.refine(el.style(), "step-label", &part_props, None);
                el
            };
            let optional = |row: Div| {
                if !step.optional {
                    return row;
                }
                let part = |text: SharedString| {
                    div()
                        .flex_none()
                        .text_size(t.text.body.size)
                        .line_height(relative(t.text.body.leading))
                        .text_color(c.text_secondary)
                        .child(text)
                };
                row.child(part("•".into()))
                    .child(part(i18n.t("@astryx.step.optional")))
            };
            let description = |d: &SharedString| {
                let mut el = div()
                    .text_size(t.text.supporting.size)
                    .line_height(s.s4)
                    .text_color(c.text_secondary)
                    .child(d.clone());
                theme.refine(el.style(), "step-description", &description_props, None);
                el
            };
            let status_node = || status_text.clone().map(|s| VisuallyHidden::new().child(s));

            // The accessible name of a clickable step.
            let name = match &status_text {
                Some(status) => i18n.format(
                    "@astryx.step.goToStepWithStatus",
                    &[
                        ("stepNumber", (i + 1).into()),
                        ("label", step.label.to_string().into()),
                        ("status", status.to_string().into()),
                    ],
                ),
                None => i18n.format(
                    "@astryx.step.goToStep",
                    &[
                        ("stepNumber", (i + 1).into()),
                        ("label", step.label.to_string().into()),
                    ],
                ),
            };
            let step_id = ElementId::Name(format!("{id}-step-{i}").into());
            let button_id = ElementId::Name(format!("{id}-step-{i}-button").into());
            let on_click = self.on_step_click.clone();
            let description_text = step.description.clone();
            // The pressable part of a step (upstream's `<button>`): the label
            // area, never the content slot.
            let press = |inner: Div, window: &mut Window, cx: &mut App| -> AnyElement {
                let inner = inner.rounded(t.radius.element);
                let Some(on_click) = on_click.clone().filter(|_| clickable) else {
                    return inner.into_any_element();
                };
                let focus = control_focus(&button_id, window, cx);
                let (hover, pressed) = (c.overlay_hover, c.overlay_pressed);
                let mut el = inner
                    .id(button_id.clone())
                    .relative()
                    .track_focus(&focus)
                    .role(Role::Button)
                    .aria_label(name.clone());
                if let Some(description) = description_text.clone() {
                    el = el.aria_description(description);
                }
                // Disabled the Astryx way: it keeps its tab stop and never
                // activates.
                el = if disabled {
                    el.aria_disabled(true)
                } else {
                    crate::press::touch_press(
                        el,
                        ElementId::Name(format!("{button_id:?}-press").into()),
                        false,
                        gpui::transparent_black(),
                        pressed,
                        window,
                        cx,
                    )
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        on_click(i, window, cx);
                    })
                };
                if is_focus_visible(&focus, window) {
                    el = el.child(focus_ring(t, t.radius.element.to_pixels(rem), rem));
                }
                el.into_any_element()
            };
            // A compact step's name, status and availability, for assistive
            // technology only (upstream compactNameNode).
            let compact_name = compact.then(|| {
                let mut el = div().child(VisuallyHidden::new().child(step.label.clone()));
                if let Some(status) = status_node() {
                    el = el.child(status);
                }
                if disabled {
                    el = el.child(
                        VisuallyHidden::new().child(i18n.t("@astryx.step.status.unavailable")),
                    );
                }
                el
            });
            let first = i == 0;
            let last = i + 1 == count;
            let end_content = step.end_content.take();
            let children = std::mem::take(&mut step.children);
            let has_children = !children.is_empty();

            let mut li = if on_track {
                let block_px = block.to_pixels(rem);
                let arrival = if vertical {
                    OT_ARRIVAL_SHARE_V
                } else {
                    OT_ARRIVAL_SHARE_H
                };
                let leaving = 1. - arrival;
                let content_seg = vertical && has_children;
                let rail_share = leaving
                    * if content_seg {
                        OT_RAIL_SHARE_OF_LEAVING
                    } else {
                        1.
                    };
                let before_fill = sweep.fill(i as isize - 1, leaving, arrival, i <= active);
                let rail_fill = sweep.fill(i as isize, 0., rail_share, i < active);
                let content_fill =
                    sweep.fill(i as isize, rail_share, leaving - rail_share, i < active);
                let faces = |f: Faces| if has_indicator { f } else { Faces::Nothing };
                let connector_part = |mut el: Div| {
                    theme.refine(el.style(), "step-connector", &[], None);
                    el
                };

                // The label line: the label, its status, Optional, endContent.
                let label_line = |centered: bool| {
                    let mut row = div().flex().flex_row_logical(dir).items_center();
                    row = if centered {
                        row.justify_center().flex_wrap().gap(s.s1)
                    } else {
                        row.gap(s.s2)
                    };
                    row = row.child(label()).children(status_node());
                    optional(row)
                };

                if vertical {
                    let lead = connector_part(
                        connector
                            .segment(before_fill, true, faces(Faces::End), !first, false)
                            .w(bar)
                            .h(block),
                    );
                    let rail = connector_part(
                        connector
                            .segment(rail_fill, true, faces(Faces::Start), !last, false)
                            .w(bar)
                            .flex_grow(1.)
                            .flex_shrink(1.)
                            .flex_basis(relative(0.))
                            .min_h(s.s2),
                    );
                    let column = div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .w(indicator_size)
                        .flex_none()
                        .self_stretch()
                        .my(-block_px)
                        .child(lead)
                        .children(indicator)
                        .child(rail);
                    let body = div()
                        .flex()
                        .flex_col()
                        .justify_start()
                        .flex_1()
                        .gap(s.s0_5)
                        .min_w_0()
                        .child(label_line(false).children(end_content))
                        .children(step.description.as_ref().map(description));
                    let row = div()
                        .flex()
                        .flex_row_logical(dir)
                        .items_stretch()
                        .gap(s.s2)
                        .w_full()
                        .p(block)
                        .child(column)
                        .child(body);
                    let content = has_children.then(|| {
                        let seg_inset =
                            block_px + (indicator_size.to_pixels(rem) - bar.to_pixels(rem)) / 2.;
                        div()
                            .relative()
                            .child(connector_part(
                                connector
                                    .segment(content_fill, true, Faces::Nothing, !last, false)
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .inset_start(dir, seg_inset)
                                    .w(bar),
                            ))
                            .child(
                                div()
                                    .pt(s.s2)
                                    .padding_start(dir, block_px + gutter.to_pixels(rem))
                                    .padding_end(dir, block)
                                    .children(children),
                            )
                    });
                    div()
                        .flex()
                        .flex_col()
                        .relative()
                        .child(press(row, window, cx))
                        .children(content)
                } else {
                    let lead = connector_part(
                        connector
                            .segment(before_fill, false, faces(Faces::End), !first, false)
                            .h(bar)
                            .flex_1()
                            .min_w(s.s2),
                    );
                    let rail = connector_part(
                        connector
                            .segment(rail_fill, false, faces(Faces::Start), !last, false)
                            .h(bar)
                            .flex_1()
                            .min_w(s.s2),
                    );
                    let track = div()
                        .flex()
                        .flex_row_logical(dir)
                        .items_center()
                        .w_full()
                        .child(lead)
                        .children(indicator)
                        .child(rail);
                    let mut wrap = div()
                        .flex()
                        .flex_col()
                        .items_stretch()
                        .w_full()
                        .py(block)
                        .child(track);
                    let mut end_content = end_content;
                    if compact {
                        // The indicator stays on the rail; the summary row
                        // repeats the label alone (FR13).
                        if is_active && options.collapsed_variant != CollapsedVariant::HiddenLabel {
                            let row = optional(
                                div()
                                    .flex()
                                    .flex_row_logical(dir)
                                    .items_center()
                                    .gap(s.s2)
                                    .min_w_0()
                                    .child(label()),
                            )
                            .children(end_content.take());
                            summary = Some(summary_root(
                                row,
                                step.description.as_ref().map(description),
                                &theme,
                                &step_props,
                                t,
                            ));
                        }
                    } else {
                        wrap = wrap.child(
                            div()
                                .flex()
                                .flex_col()
                                .items_stretch()
                                .gap(s.s0_5)
                                .text_center()
                                .mt(block)
                                .child(label_line(true).children(end_content))
                                .children(step.description.as_ref().map(description)),
                        );
                    }
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(press(wrap, window, cx))
                        .children(compact_name)
                        .children(
                            (!compact && has_children).then(|| div().pt(s.s2).children(children)),
                        )
                }
            } else {
                // Separated: the bar, then the label area.
                let filled = progress != StepState::NotStarted;
                let fill = sweep.fill(i as isize - 1, 0., 1., filled);
                let mut bar_el = connector.segment(fill, vertical, Faces::Nothing, true, true);
                bar_el = if vertical {
                    bar_el.w(bar).self_stretch()
                } else {
                    bar_el.w_full().h(bar).mb(s.s0_5)
                };
                theme.refine(bar_el.style(), "step-bar", &[], None);

                let row = |indicator: Option<AnyElement>, status: bool, end: Option<AnyElement>| {
                    let row = div()
                        .flex()
                        .flex_row_logical(dir)
                        .items_center()
                        .gap(s.s2)
                        .min_w_0()
                        .children(indicator)
                        .child(label())
                        .children(status.then(status_node).flatten());
                    optional(row).children(end.map(|c| {
                        div()
                            .flex_none()
                            .margin_start(dir, gpui::Length::Auto)
                            .child(c)
                    }))
                };
                let description_row = |indent: bool| {
                    step.description.as_ref().map(|d| {
                        div()
                            .when(indent, |el| el.padding_start(dir, gutter))
                            .child(description(d))
                    })
                };
                let content = (!compact && has_children).then(|| {
                    let start = inline.to_pixels(rem)
                        + if has_indicator {
                            gutter.to_pixels(rem)
                        } else {
                            px(0.)
                        };
                    div()
                        .pt(s.s2)
                        .padding_start(dir, start)
                        .padding_end(dir, inline)
                        .children(children)
                });
                let area = if compact {
                    if is_active && options.collapsed_variant != CollapsedVariant::HiddenLabel {
                        // The summary keeps the active indicator beside its
                        // label (FR13).
                        summary = Some(summary_root(
                            row(indicator, false, end_content),
                            step.description.as_ref().map(description),
                            &theme,
                            &step_props,
                            t,
                        ));
                    }
                    compact_name.map(IntoElement::into_any_element)
                } else {
                    let inner = div()
                        .flex()
                        .flex_col()
                        .items_stretch()
                        .w_full()
                        .py(block)
                        .px(inline)
                        .child(row(indicator, true, end_content))
                        .children(description_row(has_indicator));
                    Some(press(inner, window, cx))
                };
                if vertical {
                    div()
                        .flex()
                        .flex_row_logical(dir)
                        .items_stretch()
                        .relative()
                        .gap(s.s0_5)
                        .child(bar_el)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .children(area)
                                .children(content),
                        )
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .items_stretch()
                        .flex_1()
                        .min_w_0()
                        .child(bar_el)
                        .children(area)
                        .children(content)
                }
            };
            theme.refine(li.style(), "step", &step_props, None);
            steps.push(
                li.id(step_id)
                    .role(Role::ListItem)
                    .when(is_active, |el| el.aria_current(gpui::AriaCurrent::Step))
                    .into_any_element(),
            );
        }

        // The list.
        let mut list = div().flex().w_full();
        list = match (vertical, on_track) {
            (false, false) => list.flex_row_logical(dir).items_start().gap(s.s1),
            (true, false) => list.flex_col().gap(s.s1),
            (false, true) => list.flex_row_logical(dir).items_start(),
            (true, true) => list.flex_col(),
        };
        theme.refine(
            list.style(),
            "stepper",
            &[
                ("orientation", self.orientation.as_str()),
                ("indicatorPosition", self.indicator_position.as_str()),
            ],
            None,
        );
        let name = self
            .label
            .unwrap_or_else(|| i18n.t("@astryx.stepper.label"));
        let list = list
            .id(self.id.clone())
            .role(Role::List)
            .aria_label(name)
            .children(steps);
        if vertical {
            let mut list = list;
            list.style().refine(&self.style);
            return list.into_any_element();
        }

        // A horizontal stepper measures itself for the compact mode.
        let probe = {
            let memory = memory.clone();
            canvas(
                move |bounds, window, _| {
                    let mut m = memory.borrow_mut();
                    if m.width != Some(bounds.size.width) {
                        m.width = Some(bounds.size.width);
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };
        let list = list.relative().child(probe);

        // The summary row under a compact track (upstream stepper-summary).
        let summary_row = (compact && options.collapsed_variant != CollapsedVariant::HiddenLabel)
            .then(|| {
                let shows_controls = options.collapsed_variant
                    == CollapsedVariant::WithLabelAndControls
                    && self.on_step_click.is_some();
                let control = |delta: isize| -> Option<AnyElement> {
                    let on_click = self.on_step_click.clone().filter(|_| shows_controls)?;
                    let target = adjacent_enabled(&memory.borrow().disabled, active, delta);
                    let (key, icon, part) = if delta < 0 {
                        ("@astryx.stepper.previousStep", "chevronLeft", "previous")
                    } else {
                        ("@astryx.stepper.nextStep", "chevronRight", "next")
                    };
                    let name = i18n.t(key);
                    let memory = memory.clone();
                    Some(
                        IconButton::new(
                            ElementId::Name(format!("{id}-{part}").into()),
                            name.clone(),
                            Icon::new(icon).mirror_in_rtl(dir),
                        )
                        .variant(ButtonVariant::Ghost)
                        .tooltip(name)
                        .disabled(target.is_none())
                        .on_click(move |_, window, cx| {
                            // Read again: a step may have been disabled since.
                            let target = adjacent_enabled(&memory.borrow().disabled, active, delta);
                            if let Some(target) = target {
                                on_click(target, window, cx);
                            }
                        })
                        .into_any_element(),
                    )
                };
                let mut row = div()
                    .flex()
                    .flex_row_logical(dir)
                    .items_center()
                    .gap(s.s2)
                    .justify_between()
                    .min_w_0()
                    .children(control(-1))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_grow(1.)
                            .min_w_0()
                            .text_center()
                            .children(summary),
                    )
                    .children(control(1));
                theme.refine(row.style(), "stepper-summary", &[], None);
                row
            });

        let mut frame = div()
            .flex()
            .flex_col()
            .w_full()
            .child(list)
            .children(summary_row);
        theme.refine(frame.style(), "stepper-frame", &[], None);
        frame.style().refine(&self.style);
        frame.into_any_element()
    }
}

/// The active step in a compact summary (upstream summaryRoot).
fn summary_root(
    row: Div,
    description: Option<Div>,
    theme: &ResolvedTheme,
    step_props: &[(&str, &str)],
    t: &Tokens,
) -> AnyElement {
    let mut el = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(t.spacing.s0_5)
        .min_w_0()
        .child(row)
        .children(description);
    theme.refine(el.style(), "step", step_props, None);
    el.into_any_element()
}

/// The style declarations of a step's `step-indicator` part, if any.
fn el_style(theme: &ResolvedTheme, props: &[(&str, &str)]) -> Option<StyleRefinement> {
    let mut style = StyleRefinement::default();
    theme.refine(&mut style, "step-indicator", props, None);
    (style != StyleRefinement::default()).then_some(style)
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const STEPPER_DOC: ComponentDoc = ComponentDoc {
    name: "Stepper",
    rust_type: "astryx::components::stepper::Stepper",
    props: &[
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("activeStep", RustApi::Argument("active_step")),
        p("children", RustApi::Method("step / steps")),
        p("orientation", RustApi::Method("orientation")),
        p("onStepClick", RustApi::Method("on_step_click")),
        p("label", RustApi::Method("label")),
        p("density", RustApi::Method("density")),
        p("indicatorPosition", RustApi::Method("indicator_position")),
        p("horizontalOptions", RustApi::Method("horizontal_options")),
    ],
    example: include_str!("../pagination/examples.rs"),
};

pub const STEP_DOC: ComponentDoc = ComponentDoc {
    name: "Step",
    rust_type: "astryx::components::stepper::Step",
    props: &[
        p(
            "step",
            RustApi::Waived("a step's index is its place in the stepper"),
        ),
        p("label", RustApi::Argument("label")),
        p("description", RustApi::Method("description")),
        p("children", RustApi::Trait("ParentElement")),
        p("status", RustApi::Method("status")),
        p("indicator", RustApi::Method("indicator")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isOptional", RustApi::Method("optional")),
        p("endContent", RustApi::Method("end_content")),
        p("density", RustApi::Method("density")),
    ],
    example: include_str!("../pagination/examples.rs"),
};
