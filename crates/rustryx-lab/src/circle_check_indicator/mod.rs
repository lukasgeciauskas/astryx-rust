//! `CircleCheckIndicator`: the iOS multiple-selection mark (upstream lab
//! `CircleCheckIndicator`): an empty ring unchecked, a filled circle with a
//! checkmark checked, and a filled circle with a bar for the partial state.
//!
//! Decorative and non-interactive, like every indicator: it has no role, name or tab
//! stop; its owner (a row you lay out) carries those and paints its focus ring. It
//! reacts to the owner's hover and press through `hover_group`, upstream's
//! `indicatorScope`. Its colours fade over the theme's fast duration on every pointer
//! and change at once under Reduce Motion; the fade needs the indicator's `id`, since
//! GPUI keeps state by element id.

pub mod examples;

#[cfg(test)]
mod tests;

use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, Hsla, IntoElement, ParentElement, Pixels, SharedString,
    StyleRefinement, Window, div, px, svg, transparent_black,
};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_components::indicator::{ChoiceSize, IndicatorRequest, IndicatorState};
use rustryx_primitives::{coarse_primary_pointer, prefers_reduced_motion};
use rustryx_theme::color::mix_srgb;
use rustryx_theme::{CubicBezier, current};
use web_time::Instant;

/// UIKit's multiple-selection circle at one text size (upstream `circleCheckTable`
/// and its build-time mirror `circleCheckConsts`), in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleCheckGeometry {
    /// The circle's outer diameter.
    pub diameter: f32,
    /// The empty circle's ring.
    pub stroke: f32,
    /// The checkmark's square ink box.
    pub check_size: f32,
    /// The checkmark's stroke, and the partial state's bar height.
    pub check_stroke: f32,
    /// How far below the circle's centre the checkmark's ink sits.
    pub check_offset_y: f32,
}

impl CircleCheckGeometry {
    /// `md`: the circle at the Large text size (`circleCheckTable.large`).
    pub const LARGE: CircleCheckGeometry = CircleCheckGeometry {
        diameter: 21.8,
        stroke: 1.54,
        check_size: 9.,
        check_stroke: 1.35,
        check_offset_y: 1.,
    };

    /// `sm`: the circle at the xSmall text size (`circleCheckTable.xSmall`). Its check
    /// stroke and offset are derived from Large's: the stroke scaled by the check size
    /// (9 to 8), the offset by the diameter (21.8 to 18), both to hundredths.
    pub const X_SMALL: CircleCheckGeometry = CircleCheckGeometry {
        diameter: 18.,
        stroke: 1.27,
        check_size: 8.,
        check_stroke: 1.2,
        check_offset_y: 0.83,
    };

    /// The geometry of a control size.
    pub fn of(size: ChoiceSize) -> Self {
        match size {
            ChoiceSize::Sm => Self::X_SMALL,
            ChoiceSize::Md => Self::LARGE,
        }
    }
}

/// The checkmark (upstream's path in a 10-unit box): its round caps reach the box's
/// edges, so the ink fills it.
const CHECK_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M0.75 5.25L3.75 9.25L9.25 0.75" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

/// The iOS multiple-selection mark (upstream lab `CircleCheckIndicator`).
///
/// ```ignore
/// CircleCheckIndicator::new(IndicatorState::Checked).size(ChoiceSize::Sm).hover_group("row")
/// ```
#[derive(IntoElement)]
pub struct CircleCheckIndicator {
    id: Option<ElementId>,
    state: IndicatorState,
    size: ChoiceSize,
    disabled: bool,
    hover_group: Option<SharedString>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl CircleCheckIndicator {
    /// The mark for `state` (upstream `state`): an empty ring, a check or a bar.
    pub fn new(state: impl Into<IndicatorState>) -> Self {
        CircleCheckIndicator {
            id: None,
            state: state.into(),
            size: ChoiceSize::Md,
            disabled: false,
            hover_group: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    /// Upstream `size`: the circle at UIKit's 18 px (`Sm`) or 21.8 px (`Md`, the
    /// default), centred in its host's slot.
    pub fn size(mut self, size: ChoiceSize) -> Self {
        self.size = size;
        self
    }

    /// Upstream `isDisabled`: the disabled look (half opacity, no hover or press). Purely
    /// visual; the owner keeps the disabled semantics.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// The owner row's GPUI group: hovering or pressing the row tints the circle
    /// (upstream `indicatorScope`).
    pub fn hover_group(mut self, group: impl Into<SharedString>) -> Self {
        self.hover_group = Some(group.into());
        self
    }

    /// The element id its colour fade keeps its state under. Without one the colours
    /// change at once.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// The indicator a theme's `checkbox` entry draws for a host's request (upstream
    /// `defineTheme({indicators: {checkbox: CircleCheckIndicator}})`): its state, size,
    /// disabled look, row hover and children come from the host.
    ///
    /// ```ignore
    /// set_theme_indicator(CHECKBOX, |request, _, _| {
    ///     CircleCheckIndicator::from_request(request).into_any_element()
    /// }, cx);
    /// ```
    pub fn from_request(request: IndicatorRequest) -> Self {
        let state = match request.state {
            "checked" => IndicatorState::Checked,
            "indeterminate" => IndicatorState::Indeterminate,
            _ => IndicatorState::Unchecked,
        };
        let mut indicator = CircleCheckIndicator::new(state)
            .size(request.control_size)
            .disabled(request.disabled);
        indicator.hover_group = request.hover_group;
        indicator.children = request.children;
        indicator
    }
}

impl Styled for CircleCheckIndicator {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for CircleCheckIndicator {
    /// Upstream `children`: content drawn inside the circle instead of the mark, such
    /// as a busy spinner.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// The circle's colours: its fill and its inset ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Look {
    pub fill: Hsla,
    pub ring: Hsla,
    pub ring_width: f32,
}

impl Look {
    /// The resting look of a state: a clear centre inside an emphasized ring, or the
    /// accent fill with no ring.
    pub(crate) fn of(
        state: IndicatorState,
        geometry: CircleCheckGeometry,
        t: &rustryx_theme::Tokens,
    ) -> Self {
        if state.is_on() {
            Look {
                fill: t.color.accent,
                ring: transparent_black(),
                ring_width: 0.,
            }
        } else {
            Look {
                fill: transparent_black(),
                ring: t.color.border_emphasized,
                ring_width: geometry.stroke,
            }
        }
    }

    /// The look `t` of the way from `self` to `to`, as CSS interpolates a
    /// `background-color` and an inset `box-shadow` (a missing shadow is a transparent
    /// one of zero width).
    pub(crate) fn mix(self, to: Look, t: f32) -> Look {
        Look {
            fill: mix_srgb(self.fill, to.fill, t),
            ring: mix_srgb(self.ring, to.ring, t),
            ring_width: self.ring_width + (to.ring_width - self.ring_width) * t,
        }
    }
}

/// A colour change in flight: where it started, where it goes, and when.
struct Fade {
    from: Look,
    to: Look,
    start: Option<Instant>,
}

/// The look to draw now, fading from the last one to `target` over `duration` on
/// `ease`. Requests frames while it moves.
fn fade(
    id: &ElementId,
    target: Look,
    duration: Duration,
    ease: CubicBezier,
    window: &mut Window,
    cx: &mut App,
) -> Look {
    let now = cx.background_executor().now();
    let reduced = prefers_reduced_motion(cx);
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-circle-fade").into()),
        cx,
        |_, _| Fade {
            from: target,
            to: target,
            start: None,
        },
    );
    let at = |f: &Fade| -> (Look, bool) {
        match f.start {
            Some(start) if !reduced && !duration.is_zero() => {
                let t = now.saturating_duration_since(start).as_secs_f32() / duration.as_secs_f32();
                if t >= 1. {
                    (f.to, false)
                } else {
                    (f.from.mix(f.to, ease.ease(t)), true)
                }
            }
            _ => (f.to, false),
        }
    };
    let (mut look, mut moving) = at(state.read(cx));
    if state.read(cx).to != target {
        state.update(cx, |f, _| {
            f.from = look;
            f.to = target;
            f.start = Some(now);
        });
        (look, moving) = at(state.read(cx));
    }
    if moving {
        window.request_animation_frame();
    }
    look
}

impl RenderOnce for CircleCheckIndicator {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let geometry = CircleCheckGeometry::of(self.size);
        let resting = Look::of(self.state, geometry, t);
        let look = match &self.id {
            Some(id) => fade(id, resting, t.duration.fast, t.ease.standard, window, cx),
            None => resting,
        };
        let mut el = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(geometry.diameter))
            .rounded_full()
            .bg(look.fill)
            .border(px(look.ring_width))
            .border_color(look.ring)
            // An inherit-shade spinner inside reads this colour.
            .text_color(t.color.on_accent);
        if self.disabled {
            el = el.opacity(0.5);
        } else if let Some(group) = self.hover_group.clone() {
            // Hover tints the ring or the fill, where the device can hover.
            if self.state.is_on() {
                let fill = mix_srgb(t.color.accent, t.color.tint_hover, 0.15);
                el = el.group_hover(group, move |s| s.bg(fill));
            } else {
                let ring = mix_srgb(t.color.border_emphasized, t.color.tint_hover, 0.2);
                el = el.group_hover(group, move |s| s.border_color(ring));
            }
        }
        let checked = match self.state {
            IndicatorState::Unchecked => "",
            IndicatorState::Checked => "checked",
            IndicatorState::Indeterminate => "indeterminate",
        };
        theme.refine(
            el.style(),
            "circle-check-indicator",
            &[
                ("size", self.size.as_str()),
                ("checked", checked),
                ("disabled", if self.disabled { "disabled" } else { "" }),
            ],
            None,
        );
        el.style().refine(&self.style);
        let mut el = el.debug_selector(|| "circle-check-indicator".into());

        // The press paints on the circle itself, on a fine pointer; under a finger the
        // owner's press feedback paints it.
        if !self.disabled
            && let Some(group) = self.hover_group.clone()
            && !coarse_primary_pointer(window, cx)
        {
            let pressed = t.color.overlay_pressed;
            el = el.child(
                div()
                    .id("circle-check-press")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .rounded_full()
                    .group_active(group, move |s| s.bg(pressed)),
            );
        }
        if !self.children.is_empty() {
            return el.children(self.children);
        }
        match self.state {
            IndicatorState::Checked => {
                let mut check = div()
                    .debug_selector(|| "circle-check-indicator-check".into())
                    .relative()
                    .top(px(geometry.check_offset_y))
                    .flex_none()
                    .size(px(geometry.check_size));
                theme.refine(
                    check.style(),
                    "circle-check-indicator-check",
                    &[("size", self.size.as_str())],
                    None,
                );
                el.child(
                    check.child(
                        svg()
                            .data(CHECK_SVG)
                            .size(px(geometry.check_size))
                            .text_color(t.color.on_accent),
                    ),
                )
            }
            IndicatorState::Indeterminate => {
                let mut bar = div()
                    .debug_selector(|| "circle-check-indicator-dash".into())
                    .flex_none()
                    .w(px(geometry.check_size))
                    .h(px(geometry.check_stroke))
                    .rounded_full()
                    .bg(t.color.on_accent);
                theme.refine(
                    bar.style(),
                    "circle-check-indicator-dash",
                    &[("size", self.size.as_str())],
                    None,
                );
                el.child(bar)
            }
            IndicatorState::Unchecked => el,
        }
    }
}

/// The geometry the circle draws for `size`, in pixels (for tests and hosts that size
/// a slot around it).
pub fn circle_diameter(size: ChoiceSize) -> Pixels {
    px(CircleCheckGeometry::of(size).diameter)
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `CircleCheckIndicator`.
pub const CIRCLE_CHECK_INDICATOR_DOC: ComponentDoc = ComponentDoc {
    name: "CircleCheckIndicator",
    rust_type: "rustryx::lab::circle_check_indicator::CircleCheckIndicator",
    props: &[
        p("state", RustApi::Argument("state")),
        p("size", RustApi::Method("size")),
        p("isDisabled", RustApi::Method("disabled")),
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
