//! Slider. Upstream: `Slider/`.
//!
//! Thumbs travel inside a box inset by half a thumb, so positions are plain
//! fractions. A press on the track moves the nearest thumb there and drags
//! it; arrows, Page Up/Down and Home/End move the focused thumb. The
//! `tooltip` value display shows the value over the thumb on hover, focus
//! and drag.

#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::rc::Rc;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{control_focus, is_focus_visible};
use astryx_theme::color::mix_srgb;
use astryx_theme::current;
use gpui::{
    App, Bounds, DispatchPhase, ElementId, IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, canvas, div,
    prelude::*, px, relative,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::field::{Field, InputStatus, StatusPlacement};
use crate::focus::focus_ring;
use crate::layout::Orientation;

/// Upstream `TRACK_SIZE` and `THUMB_SIZE`.
const TRACK: f32 = 4.;
const THUMB: f32 = 20.;

/// Upstream `value`: one thumb, or a range of two.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderValue {
    Single(f64),
    Range(f64, f64),
}

impl From<f64> for SliderValue {
    fn from(value: f64) -> Self {
        SliderValue::Single(value)
    }
}

impl From<(f64, f64)> for SliderValue {
    fn from((start, end): (f64, f64)) -> Self {
        SliderValue::Range(start, end)
    }
}

impl SliderValue {
    fn thumbs(self) -> Vec<f64> {
        match self {
            SliderValue::Single(v) => vec![v],
            SliderValue::Range(a, b) => vec![a, b],
        }
    }
}

/// Upstream `valueDisplay`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ValueDisplay {
    /// A tooltip over the thumb, on hover, focus and drag.
    #[default]
    Tooltip,
    /// The value as text after the track.
    Text,
    /// No value shown.
    None,
}

/// Upstream `marks`: a tick, with an optional label.
#[derive(Clone, Debug, PartialEq)]
pub struct SliderMark {
    pub value: f64,
    pub label: Option<SharedString>,
}

impl SliderMark {
    pub fn new(value: f64) -> Self {
        SliderMark { value, label: None }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// The value range and stepping, shared by the pointer and the keys.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Scale {
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl Scale {
    /// The position of `value` along the track, 0 to 1.
    pub fn fraction(&self, value: f64) -> f32 {
        if self.max <= self.min {
            return 0.;
        }
        (((value - self.min) / (self.max - self.min)).clamp(0., 1.)) as f32
    }

    /// The nearest step to `value`, within the bounds.
    pub fn snap(&self, value: f64) -> f64 {
        let steps = ((value - self.min) / self.step).round();
        (self.min + steps * self.step).clamp(self.min, self.max)
    }

    /// The value at `fraction` along the track, snapped.
    pub fn at(&self, fraction: f32) -> f64 {
        self.snap(self.min + f64::from(fraction.clamp(0., 1.)) * (self.max - self.min))
    }
}

/// A thumb's next value after `key`, or `None` for other keys. `rtl`
/// mirrors the horizontal arrows.
pub(crate) fn key_value(scale: Scale, value: f64, key: &str, rtl: bool) -> Option<f64> {
    let page = (scale.max - scale.min) / 10.;
    let page = (page / scale.step).round().max(1.) * scale.step;
    let (forward, back) = if rtl {
        ("left", "right")
    } else {
        ("right", "left")
    };
    Some(match key {
        k if k == forward || k == "up" => scale.snap(value + scale.step),
        k if k == back || k == "down" => scale.snap(value - scale.step),
        "pageup" => scale.snap(value + page),
        "pagedown" => scale.snap(value - page),
        "home" => scale.min,
        "end" => scale.max,
        _ => return None,
    })
}

/// Moves thumb `index` of `values` to `to`, keeping the range ordered and
/// at least `gap` apart (upstream `minStepsBetweenThumbs`).
pub(crate) fn move_thumb(values: &[f64], index: usize, to: f64, gap: f64) -> Vec<f64> {
    let mut next = values.to_vec();
    next[index] = match (index, values.len()) {
        (0, 2) => to.min(values[1] - gap),
        (1, 2) => to.max(values[0] + gap),
        _ => to,
    };
    next
}

/// The thumb a press at `to` moves: the nearer one, and of a tie the start
/// thumb unless the press lies past the end thumb.
pub(crate) fn nearest_thumb(values: &[f64], to: f64) -> usize {
    match values {
        [start, end] if (to - end).abs() < (to - start).abs() || to > *end => 1,
        _ => 0,
    }
}

type ChangeFn = Rc<dyn Fn(SliderValue, &mut Window, &mut App)>;
type FormatFn = Rc<dyn Fn(f64) -> SharedString>;

#[derive(Default)]
struct DragState {
    /// The travel box as last painted.
    bounds: Cell<Option<Bounds<Pixels>>>,
    /// The thumb being dragged.
    dragging: Cell<Option<usize>>,
}

/// A numeric value or range picked by dragging, upstream `Slider`.
///
/// ```ignore
/// Slider::new("volume", "Volume", volume).on_change(|value, _, _| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct Slider {
    id: ElementId,
    label: SharedString,
    value: SliderValue,
    on_change: Option<ChangeFn>,
    on_change_end: Option<ChangeFn>,
    scale: Scale,
    orientation: Orientation,
    format_value: Option<FormatFn>,
    value_display: ValueDisplay,
    marks: Vec<SliderMark>,
    min_steps_between: f64,
    disabled: bool,
    disabled_message: Option<SharedString>,
    optional: bool,
    required: bool,
    label_hidden: bool,
    description: Option<SharedString>,
    status: Option<InputStatus>,
    label_tooltip: Option<SharedString>,
    width: Option<gpui::Length>,
    style: StyleRefinement,
}

impl Slider {
    /// Upstream `label` and `value` (an `f64`, or `(start, end)`).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SliderValue>,
    ) -> Self {
        Slider {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            on_change: None,
            on_change_end: None,
            scale: Scale {
                min: 0.,
                max: 100.,
                step: 1.,
            },
            orientation: Orientation::Horizontal,
            format_value: None,
            value_display: ValueDisplay::Tooltip,
            marks: Vec::new(),
            min_steps_between: 0.,
            disabled: false,
            disabled_message: None,
            optional: false,
            required: false,
            label_hidden: false,
            description: None,
            status: None,
            label_tooltip: None,
            width: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `orientation`.
        orientation: Orientation,
        /// Upstream `valueDisplay`.
        value_display: ValueDisplay,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `disabledMessage`: the reason, in a tooltip on hover and focus.
        ?disabled_message: SharedString,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `labelTooltip`: an InfoTip beside the label.
        ?label_tooltip: SharedString,
        /// Upstream `width`: the whole field.
        ?width: gpui::Length,
    }

    /// Upstream `min`.
    pub fn min(mut self, min: f64) -> Self {
        self.scale.min = min;
        self
    }

    /// Upstream `max`.
    pub fn max(mut self, max: f64) -> Self {
        self.scale.max = max;
        self
    }

    /// Upstream `step` (positive).
    pub fn step(mut self, step: f64) -> Self {
        if step > 0. {
            self.scale.step = step;
        }
        self
    }

    /// Upstream `minStepsBetweenThumbs`.
    pub fn min_steps_between_thumbs(mut self, steps: f64) -> Self {
        self.min_steps_between = steps.max(0.);
        self
    }

    /// Upstream `marks`.
    pub fn marks(mut self, marks: impl IntoIterator<Item = SliderMark>) -> Self {
        self.marks = marks.into_iter().collect();
        self
    }

    /// Upstream `formatValue`: the shown and announced text of a value.
    pub fn format_value(mut self, f: impl Fn(f64) -> SharedString + 'static) -> Self {
        self.format_value = Some(Rc::new(f));
        self
    }

    /// Upstream `onChange`: during a drag and on every key.
    pub fn on_change(mut self, f: impl Fn(SliderValue, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `onChangeEnd`: when a drag ends, and after a key.
    pub fn on_change_end(
        mut self,
        f: impl Fn(SliderValue, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change_end = Some(Rc::new(f));
        self
    }
}

impl Styled for Slider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The value the thumbs make, in the slider's shape.
fn as_value(single: bool, values: &[f64]) -> SliderValue {
    if single {
        SliderValue::Single(values[0])
    } else {
        SliderValue::Range(values[0], values[1])
    }
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let rtl = dir.is_rtl();
        let vertical = self.orientation == Orientation::Vertical;
        let scale = self.scale;
        let single = matches!(self.value, SliderValue::Single(_));
        let values: Rc<[f64]> = self.value.thumbs().into();
        let gap = self.min_steps_between * scale.step;
        let disabled = self.disabled;
        let i18n = i18n(cx);
        let format: FormatFn = match &self.format_value {
            Some(f) => f.clone(),
            None => {
                let i18n = i18n.clone();
                Rc::new(move |v| i18n.number(v).into())
            }
        };

        let state = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-drag", self.id).into()),
                cx,
                |_, _| Rc::new(DragState::default()),
            )
            .read(cx)
            .clone();

        // The fraction along the travel box at a window position.
        let fraction_at = move |bounds: Bounds<Pixels>, position: Point<Pixels>| -> f32 {
            if vertical {
                let h = bounds.size.height.as_f32().max(1.);
                (bounds.bottom() - position.y).as_f32() / h
            } else {
                let w = bounds.size.width.as_f32().max(1.);
                let f = (position.x - bounds.left()).as_f32() / w;
                if rtl { 1. - f } else { f }
            }
        };

        let on_change = self.on_change.clone();
        let on_change_end = self.on_change_end.clone();
        let fractions: Vec<f32> = values.iter().map(|&v| scale.fraction(v)).collect();
        let (fill_from, fill_to) = if single {
            (0., fractions[0])
        } else {
            (fractions[0], fractions[1])
        };

        // The track runs the full length; the travel box (inset by half a
        // thumb) holds the fill, thumbs, marks and labels, so their ends and
        // centers land at plain fractions. A single fill reaches back to the
        // track's start.
        let half = px(THUMB / 2.);
        // Disabled, the whole control dims; the track keeps its color.
        let track_color = t.color.track;
        let track = if vertical {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px((THUMB - TRACK) / 2.))
                .w(px(TRACK))
        } else {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px((THUMB - TRACK) / 2.))
                .h(px(TRACK))
        }
        .rounded_full()
        .bg(track_color);
        let fill = {
            let fill = div().absolute().rounded_full().bg(t.color.accent);
            if vertical {
                let mut f = fill
                    .left(px((THUMB - TRACK) / 2.))
                    .w(px(TRACK))
                    .bottom(relative(fill_from))
                    .top(relative(1. - fill_to));
                if single {
                    f = f.mb(-half);
                }
                f
            } else {
                let mut f = fill
                    .top(px((THUMB - TRACK) / 2.))
                    .h(px(TRACK))
                    .inset_start(dir, relative(fill_from))
                    .inset_end(dir, relative(1. - fill_to));
                if single {
                    f = f.margin_start(dir, -half);
                }
                f
            }
        };

        let marks = self.marks.iter().map(|mark| {
            let f = scale.fraction(mark.value);
            let filled = f >= fill_from - f32::EPSILON && f <= fill_to + f32::EPSILON;
            let color = if filled {
                t.color.accent
            } else {
                t.color.track
            };
            let tick = div().absolute().rounded_full().bg(color);
            let tick = if vertical {
                tick.w(px(8.))
                    .h(px(2.))
                    .left(px((THUMB - 8.) / 2.))
                    .bottom(relative(f))
                    .mb(px(-1.))
            } else {
                tick.w(px(2.))
                    .h(px(8.))
                    .top(px((THUMB - 8.) / 2.))
                    .inset_start(dir, relative(f))
                    .margin_start(dir, px(-1.))
            };
            let label = mark.label.clone().map(|label| {
                let l = div()
                    .absolute()
                    .whitespace_nowrap()
                    .text_size(t.text.supporting.size)
                    .line_height(relative(t.text.supporting.leading))
                    .text_color(t.color.text_secondary)
                    .child(label);
                if vertical {
                    l.left(px(THUMB / 2. + 4. + THUMB / 2.))
                        .bottom(relative(f))
                        .translate_y(relative(0.5))
                } else {
                    l.top(px(THUMB / 2. + 4. + THUMB / 2.))
                        .inset_start(dir, relative(f))
                        .translate_x(relative(if rtl { 0.5 } else { -0.5 }))
                }
            });
            div().children([tick]).children(label)
        });

        let value_display = self.value_display;
        let dragging_now = state.dragging.get();
        let thumbs = values.iter().enumerate().map(|(i, &v)| {
            let f = fractions[i];
            let thumb_id = ElementId::Name(format!("{:?}-thumb-{i}", self.id).into());
            let focus = control_focus(&thumb_id, window, cx);
            let hover = mix_srgb(t.color.accent, t.color.tint_hover, 0.15);
            let mut thumb = div()
                .id(thumb_id)
                .track_focus(&focus)
                .absolute()
                .size(px(THUMB))
                .rounded_full()
                .bg(if disabled {
                    t.color.background_muted
                } else {
                    t.color.accent
                });
            thumb = if vertical {
                thumb.left_0().bottom(relative(f)).mb(-half)
            } else {
                thumb
                    .top_0()
                    .inset_start(dir, relative(f))
                    .margin_start(dir, -half)
            };
            if !disabled {
                let values = values.clone();
                let (on_change, on_change_end) = (on_change.clone(), on_change_end.clone());
                thumb = thumb
                    .hover(move |s| s.bg(hover))
                    .on_key_down(move |event, window, cx| {
                        let m = &event.keystroke.modifiers;
                        if m.control || m.alt || m.platform {
                            return;
                        }
                        let Some(to) = key_value(scale, values[i], &event.keystroke.key, rtl)
                        else {
                            return;
                        };
                        cx.stop_propagation();
                        let next = as_value(single, &move_thumb(&values, i, to, gap));
                        if let Some(f) = &on_change {
                            f(next, window, cx);
                        }
                        if let Some(f) = &on_change_end {
                            f(next, window, cx);
                        }
                    });
            }
            if is_focus_visible(&focus, window) {
                thumb = thumb.child(focus_ring(t, px(THUMB / 2.), rem));
            }
            let name: SharedString = match (single, i) {
                (true, _) => self.label.clone(),
                (false, 0) => i18n.t("@astryx.slider.minimumThumb"),
                (false, _) => i18n.t("@astryx.slider.maximumThumb"),
            };
            let thumb = thumb
                .role(Role::Slider)
                .aria_label(name)
                .aria_disabled(disabled)
                .aria_numeric_value(v)
                .aria_min_numeric_value(scale.min)
                .aria_max_numeric_value(scale.max)
                .aria_numeric_value_step(scale.step)
                .aria_value(format(v));
            if value_display != ValueDisplay::Tooltip {
                return thumb.into_any_element();
            }
            // Upstream SliderValueTooltip: the value over the thumb, at once on
            // hover and any focus; a drag holds its own thumb's open and
            // closes the other's.
            crate::overlay::Tooltip::new(
                ElementId::Name(format!("{}-value-{i}", self.id).into()),
                format(v),
            )
            .delay(std::time::Duration::ZERO)
            .focus_trigger(crate::overlay::FocusTrigger::Always)
            .placement(if vertical {
                astryx_primitives::Placement::Start
            } else {
                astryx_primitives::Placement::Above
            })
            .open(dragging_now.map(|d| d == i))
            .trigger_acts(false)
            .press_closes(false)
            .child(thumb)
            .into_any_element()
        });

        // Records where the travel box landed, and while a thumb is
        // dragged, follows the pointer anywhere in the window.
        let paint_state = state.clone();
        let drag_values = values.clone();
        let (drag_change, drag_end) = (on_change.clone(), on_change_end.clone());
        let tracker = canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                paint_state.bounds.set(Some(bounds));
                let Some(index) = paint_state.dragging.get() else {
                    return;
                };
                let (state, values, change) = (
                    paint_state.clone(),
                    drag_values.clone(),
                    drag_change.clone(),
                );
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble || !e.dragging() {
                        return;
                    }
                    let to = scale.at(fraction_at(bounds, e.position));
                    let next = as_value(single, &move_thumb(&values, index, to, gap));
                    if let Some(f) = &change {
                        f(next, window, cx);
                    }
                    let _ = &state;
                });
                let (state, values, end) =
                    (paint_state.clone(), drag_values.clone(), drag_end.clone());
                window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble || e.button != MouseButton::Left {
                        return;
                    }
                    state.dragging.set(None);
                    if let Some(f) = &end {
                        f(as_value(single, &values), window, cx);
                    }
                    window.refresh();
                });
            },
        )
        .absolute()
        .size_full();

        let travel = div()
            .absolute()
            .when(vertical, |el| el.left_0().right_0().top(half).bottom(half))
            .when(!vertical, |el| el.top_0().bottom_0().left(half).right(half))
            .child(tracker)
            .child(fill)
            .children(marks)
            .children(thumbs);

        let mut container = div()
            .id(ElementId::Name(format!("{:?}-track", self.id).into()))
            .relative()
            .flex_none();
        container = if vertical {
            container.w(px(THUMB)).h(px(160.))
        } else {
            container.h(px(THUMB)).flex_1().w_full()
        };
        if disabled {
            container = container.opacity(0.5);
        } else {
            let state = state.clone();
            let values = values.clone();
            let change = on_change.clone();
            container = container.cursor_pointer().on_mouse_down(
                MouseButton::Left,
                move |e, window, cx| {
                    let Some(bounds) = state.bounds.get() else {
                        return;
                    };
                    let to = scale.at(fraction_at(bounds, e.position));
                    let index = nearest_thumb(&values, to);
                    state.dragging.set(Some(index));
                    let next = as_value(single, &move_thumb(&values, index, to, gap));
                    if let Some(f) = &change {
                        f(next, window, cx);
                    }
                    window.refresh();
                },
            );
        }
        let container = container.child(track).child(travel);
        // Upstream disabledMessage: the reason over the track.
        let container = crate::overlay::disabled_reason(
            &self.id,
            disabled,
            self.disabled_message.as_ref(),
            false,
            container,
        );

        let text = (self.value_display == ValueDisplay::Text).then(|| {
            let shown: SharedString = match self.value {
                SliderValue::Single(v) => format(v),
                SliderValue::Range(a, b) => i18n.format(
                    "@astryx.slider.rangeValue",
                    &[
                        ("start", format(a).to_string().into()),
                        ("end", format(b).to_string().into()),
                    ],
                ),
            };
            // Reserve the widest of the bounds' texts, so the row never shifts.
            let widest = format(scale.min).len().max(format(scale.max).len());
            let reserve = if single { widest } else { widest * 2 + 3 };
            div()
                .flex_none()
                .text_size(t.text.label.size)
                .line_height(relative(t.text.label.leading))
                .text_color(t.color.text_primary)
                .whitespace_nowrap()
                .min_w(t.text.label.size.to_pixels(rem) * (reserve as f32 * 0.6))
                .text_right()
                .child(shown)
        });
        let has_labels = self.marks.iter().any(|m| m.label.is_some());
        let mut row = div().flex().items_center().gap(t.spacing.s2);
        row = if vertical {
            row.flex_col()
        } else {
            row.flex_row()
        };
        if has_labels && !vertical {
            let line = t.text.supporting.size.to_pixels(rem) * t.text.supporting.leading;
            row = row.pb(px(4.) + line);
        }
        theme.refine(
            row.style(),
            "slider",
            &[("orientation", self.orientation.as_str())],
            None,
        );
        row.style().refine(&self.style);
        let row = row.child(container).children(text);

        let mut field = Field::new(
            ElementId::Name(format!("{:?}-field", self.id).into()),
            self.label,
        )
        .label_hidden(self.label_hidden)
        .optional(self.optional)
        .required(self.required)
        .disabled(disabled)
        .status_placement(StatusPlacement::Detached);
        if let Some(description) = self.description {
            field = field.description(description);
        }
        if let Some(status) = self.status {
            field = field.status(status);
        }
        if let Some(width) = self.width {
            field = field.width(width);
        }
        if let Some(tooltip) = self.label_tooltip.clone() {
            field = field.label_tooltip(tooltip);
        }
        field.child(row)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const SLIDER_DOC: ComponentDoc = ComponentDoc {
    name: "Slider",
    rust_type: "astryx::components::slider::Slider",
    props: &[
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("onChangeEnd", RustApi::Method("on_change_end")),
        p("min", RustApi::Method("min")),
        p("max", RustApi::Method("max")),
        p("step", RustApi::Method("step")),
        p("orientation", RustApi::Method("orientation")),
        p("formatValue", RustApi::Method("format_value")),
        p("valueDisplay", RustApi::Method("value_display")),
        p("marks", RustApi::Method("marks")),
        p(
            "minStepsBetweenThumbs",
            RustApi::Method("min_steps_between_thumbs"),
        ),
        p("isDisabled", RustApi::Method("disabled")),
        p("htmlName", RustApi::Waived("GPUI has no HTML forms")),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("status", RustApi::Method("status")),
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("width", RustApi::Method("width")),
        p(
            "containerXstyle",
            RustApi::Waived("Styled reaches the slider row"),
        ),
        p(
            "containerClassName",
            RustApi::Waived("Styled reaches the slider row"),
        ),
        p(
            "containerStyle",
            RustApi::Waived("Styled reaches the slider row"),
        ),
        p("controlXstyle", RustApi::Trait("Styled")),
        p("controlClassName", RustApi::Trait("Styled")),
        p("controlStyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("../choice/examples.rs"),
};
