//! `TourStep`: one spotlight step of a [`Tour`](super::Tour) (upstream lab `TourStep`).
//! While active it draws a highlight ring over its target, optionally the dimmed page
//! around it, and a non-modal callout beside the ring; inactive, or outside a Tour, it
//! draws nothing.

use std::panic::Location;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, AvailableSpace, BorderStyle, Bounds, Corners, Element, ElementId, FocusHandle,
    GlobalElementId, Hsla, InspectorElementId, LayoutId, Pixels, Point, Role, SharedString, Style,
    Window, div, px, quad, size,
};
use rustryx_components::button::{Button, ButtonVariant, IconButton};
use rustryx_components::focus::focus_ring;
use rustryx_components::icon::{Icon, IconSize};
use rustryx_components::text::{Heading, HeadingLevel, Text, TextColor, TextType};
use rustryx_i18n::{LogicalStyled, use_direction};
use rustryx_primitives::size::ElementSize;
use rustryx_primitives::{
    LayerAlign, LayerOptions, Placement, is_focus_visible, layer, position_layer, use_context,
};
use rustryx_theme::ThemeStyled;

use super::target::TourTarget;
use super::{TourContext, TourDismissSource};

/// The gap between the target's edge and the highlight ring (upstream
/// `HIGHLIGHT_PADDING`).
pub const HIGHLIGHT_PADDING: f32 = 4.;

/// The step count's text (upstream's `${activeStepIndex + 1} of ${stepCount}`).
pub fn step_count_text(index: usize, count: usize) -> String {
    format!("{} of {count}", index + 1)
}

/// The labels of the step's controls (upstream's literals).
pub mod labels {
    /// The close control's accessible name.
    pub const CLOSE: &str = "Close tour";
    /// The control that returns a step.
    pub const BACK: &str = "Back";
    /// The control that advances.
    pub const NEXT: &str = "Next";
    /// The last step's control, which completes the tour.
    pub const DONE: &str = "Done";
}

/// A single spotlight step within a [`Tour`](super::Tour) (upstream lab `TourStep`).
///
/// ```ignore
/// TourStep::new("save", &save_target, "Save your work").child("Changes save automatically.")
/// ```
#[derive(IntoElement)]
pub struct TourStep {
    id: ElementId,
    target: TourTarget,
    heading: SharedString,
    children: Vec<AnyElement>,
    placement: Placement,
    alignment: LayerAlign,
    test_id: Option<SharedString>,
}

impl TourStep {
    /// A step named `id` (its place among the tour's steps) pointing at `target`
    /// (upstream `targetRef`), headed `heading` (upstream `heading`).
    pub fn new(
        id: impl Into<ElementId>,
        target: &TourTarget,
        heading: impl Into<SharedString>,
    ) -> Self {
        TourStep {
            id: id.into(),
            target: target.clone(),
            heading: heading.into(),
            children: Vec::new(),
            placement: Placement::Below,
            alignment: LayerAlign::Start,
            test_id: None,
        }
    }

    /// Upstream `placement`: the side of the target the callout sits on (`Below` by
    /// default).
    pub fn placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    /// Upstream `alignment`: how the callout lines up along that side (`Start` by
    /// default).
    pub fn alignment(mut self, alignment: LayerAlign) -> Self {
        self.alignment = alignment;
        self
    }

    /// Upstream `data-testid`: the callout's debug selector.
    pub fn test_id(mut self, id: impl Into<SharedString>) -> Self {
        self.test_id = Some(id.into());
        self
    }
}

impl ParentElement for TourStep {
    /// Upstream `children`: the step's body.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// A step's own state.
struct StepState {
    /// Active last frame.
    was_active: bool,
    /// Focus moves into the callout after the frame that first draws it.
    pending_focus: bool,
    /// The callout region's focus: not a tab stop (upstream `tabIndex={-1}`).
    region: FocusHandle,
}

type CloseFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// The overlay a step draws while active: the dimmed page (with a backdrop), the
/// highlight ring over the target, and the callout beside it. Placed from the target's
/// box when it is prepainted, after the page, so the ring stays on the target.
struct TourOverlay {
    id: ElementId,
    target: TourTarget,
    callout: Option<AnyElement>,
    options: LayerOptions,
    backdrop: Option<CloseFn>,
    surface: Hsla,
    accent: Hsla,
    dim: Hsla,
    focus: Option<FocusHandle>,
}

struct OverlayFrame {
    backdrop: Option<AnyElement>,
    highlight: AnyElement,
    callout: Option<AnyElement>,
    /// The highlight box and the target's radius, when the target was ever drawn.
    ring: Option<(Bounds<Pixels>, Pixels)>,
}

impl Element for TourOverlay {
    type RequestLayoutState = ();
    type PrepaintState = OverlayFrame;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // No place in the flow: everything is placed against the window in prepaint.
        let style = Style {
            position: gpui::Position::Absolute,
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> OverlayFrame {
        let viewport = Bounds::new(Point::default(), window.viewport_size());
        let target = self.target.frame(window, cx);
        // The highlight box sits HIGHLIGHT_PADDING outside the target; a target never
        // drawn leaves it empty at the window's corner, unpainted.
        let (highlight_bounds, ring) = match &target {
            Some(t) => {
                let b = t.bounds.dilate(px(HIGHLIGHT_PADDING));
                (b, Some((b, t.radius)))
            }
            None => (Bounds::new(viewport.origin, size(px(0.), px(0.))), None),
        };
        let fixed = |b: Bounds<Pixels>| {
            size(
                AvailableSpace::Definite(b.size.width),
                AvailableSpace::Definite(b.size.height),
            )
        };
        // The dimmed overlay catches every press outside the callout, the target's too.
        let backdrop = self.backdrop.clone().map(|close| {
            let mut el = div()
                .id("tour-backdrop")
                .debug_selector(|| "tour-backdrop".into())
                .w(viewport.size.width)
                .h(viewport.size.height)
                .occlude()
                .on_click(move |_, window, cx| close(window, cx))
                .into_any_element();
            el.layout_as_root(fixed(viewport), window, cx);
            el.prepaint_at(viewport.origin, window, cx);
            el
        });
        let mut highlight = div()
            .debug_selector(|| "tour-highlight".into())
            .w(highlight_bounds.size.width)
            .h(highlight_bounds.size.height)
            .into_any_element();
        highlight.layout_as_root(fixed(highlight_bounds), window, cx);
        highlight.prepaint_at(highlight_bounds.origin, window, cx);
        // The callout anchors to the highlight box, not the target, so a press that
        // scales the target leaves it in place.
        let callout = self.callout.take().map(|mut callout| {
            let measured = callout.layout_as_root(
                size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
                window,
                cx,
            );
            let position = position_layer(highlight_bounds, measured, viewport, &self.options);
            callout.prepaint_at(position.origin, window, cx);
            callout
        });
        OverlayFrame {
            backdrop,
            highlight,
            callout,
            ring,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        frame: &mut OverlayFrame,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(backdrop) = frame.backdrop.as_mut() {
            backdrop.paint(window, cx);
        }
        if let Some((b, radius)) = frame.ring {
            // Upstream's rings: `0 0 0 2px surface, 0 0 0 4px accent` and, with a
            // backdrop, `0 0 0 9999px overlay`: spread shadows outside the box, each
            // corner its radius plus the spread, nothing inside the box.
            let grow = |spread: Pixels| {
                let r = if radius > px(0.) {
                    radius + spread
                } else {
                    px(0.)
                };
                (b.dilate(spread), Corners::all(r))
            };
            let ring = |window: &mut Window, spread: Pixels, width: Pixels, color: Hsla| {
                let (bounds, corners) = grow(spread);
                window.paint_quad(quad(
                    bounds,
                    corners,
                    gpui::transparent_black(),
                    width,
                    color,
                    BorderStyle::Solid,
                ));
            };
            if self.backdrop.is_some() {
                let viewport = window.viewport_size();
                let reach = (viewport.width + viewport.height) * 2.;
                ring(window, reach, reach, self.dim);
            }
            ring(window, px(4.), px(2.), self.accent);
            ring(window, px(2.), px(2.), self.surface);
        }
        frame.highlight.paint(window, cx);
        if let Some(callout) = frame.callout.as_mut() {
            callout.paint(window, cx);
        }
        // Focus moves to the step region, after the frame that first draws it, so
        // assistive technology reads the new step.
        if let Some(focus) = self.focus.take() {
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        }
    }
}

impl RenderOnce for TourStep {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Outside a Tour, or while not the active step, a step draws nothing.
        let Some(tour) = use_context::<TourContext>(cx) else {
            return div().hidden().into_any_element();
        };
        tour.frame.borrow_mut().push(self.id.clone());
        let key = self.id.to_string();
        let active = tour.active_step.as_ref() == Some(&self.id);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{key}-tour-step").into()),
            cx,
            |_, cx| StepState {
                was_active: false,
                pending_focus: false,
                region: cx.focus_handle().tab_stop(false),
            },
        );
        if !active {
            state.update(cx, |s, _| s.was_active = false);
            return div().hidden().into_any_element();
        }
        let (region, pending_focus) = state.update(cx, |s, _| {
            if !s.was_active {
                s.was_active = true;
                s.pending_focus = true;
            }
            (s.region.clone(), std::mem::take(&mut s.pending_focus))
        });
        // The tour learns where focus would return from this step.
        let target_focus = self.target.frame(window, cx).and_then(|t| t.focus);
        tour.state.update(cx, |s, _| {
            s.region = Some(region.clone());
            s.return_target =
                target_focus.or_else(|| s.opener.clone().map(super::ReturnTarget::Handle));
        });

        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let first = tour.active_index == 0;
        let last = tour.step_count > 0 && tour.active_index + 1 == tour.step_count;

        let close = {
            let dismiss = tour.dismiss.clone();
            div().debug_selector(|| "tour-close".into()).child(
                IconButton::new(
                    "tour-close",
                    labels::CLOSE,
                    Icon::new("close").size(IconSize::Sm),
                )
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
                .on_click(move |_, window, cx| dismiss(TourDismissSource::Close, window, cx)),
            )
        };
        let header = div()
            .flex()
            .flex_row_logical(dir)
            .gap(t.spacing.s2)
            .items_start()
            .justify_between()
            .child(Heading::new(HeadingLevel::H4, self.heading.clone()))
            .child(close);
        let body = (!self.children.is_empty()).then(|| {
            div()
                .text_style(&t.text.body)
                .font_stack(&t.font.family.body)
                .text_color(t.color.text_primary)
                .children(self.children)
        });
        let count = if tour.step_count_shown && tour.step_count > 0 {
            div()
                .debug_selector(|| "tour-step-count".into())
                .child(
                    Text::new(step_count_text(tour.active_index, tour.step_count))
                        .text_type(TextType::Supporting)
                        .color(TextColor::Secondary),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };
        let back = (!first).then(|| {
            let previous = tour.previous.clone();
            div().debug_selector(|| "tour-back".into()).child(
                Button::new("tour-back", labels::BACK)
                    .variant(ButtonVariant::Ghost)
                    .size(ElementSize::Sm)
                    .on_click(move |_, window, cx| previous(window, cx)),
            )
        });
        let next = {
            let next = tour.next.clone();
            let (label, selector) = if last {
                (labels::DONE, "tour-done")
            } else {
                (labels::NEXT, "tour-next")
            };
            div().debug_selector(move || selector.into()).child(
                Button::new("tour-next", label)
                    .variant(ButtonVariant::Primary)
                    .size(ElementSize::Sm)
                    .on_click(move |_, window, cx| next(window, cx)),
            )
        };
        let footer = div()
            .flex()
            .flex_row_logical(dir)
            .gap(t.spacing.s2)
            .items_center()
            .justify_between()
            .child(count)
            .child(
                div()
                    .flex()
                    .flex_row_logical(dir)
                    .gap(t.spacing.s2)
                    .children(back)
                    .child(next),
            );
        let content = div()
            .flex()
            .flex_col()
            .gap(t.spacing.s3)
            .max_w(px(320.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.s1)
                    .child(header)
                    .children(body),
            )
            .child(footer);
        let selector = self
            .test_id
            .clone()
            .unwrap_or_else(|| format!("tour-step-{key}").into());
        // The callout: the paint of a Popover surface, a region named by its heading.
        let mut callout = div()
            .id(ElementId::Name(format!("{key}-callout").into()))
            .debug_selector(move || selector.to_string())
            .track_focus(&region)
            .occlude()
            .relative()
            .bg(t.color.background_popover)
            .rounded(t.radius.container)
            .shadow_token(&t.shadow.low)
            .p(t.spacing.s3)
            .font_stack(&t.font.family.body)
            .text_color(t.color.text_primary)
            .role(Role::Region)
            .aria_label(self.heading.clone())
            .child(content);
        if is_focus_visible(&region, window) {
            callout = callout.child(focus_ring(t, t.radius.container.to_pixels(rem), rem));
        }

        let options = LayerOptions {
            placement: self.placement,
            align: self.alignment,
            // The layer's anchor offset plus the clearance that keeps the ring uncovered.
            offset: (t.spacing.s1.to_pixels(rem) + t.spacing.s2.to_pixels(rem)),
            rtl: dir.is_rtl(),
            ..LayerOptions::default()
        };
        let backdrop: Option<CloseFn> = tour.backdrop.then(|| {
            let dismiss = tour.dismiss.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                dismiss(TourDismissSource::Backdrop, window, cx)
            }) as CloseFn
        });
        let overlay = TourOverlay {
            id: ElementId::Name(format!("{key}-overlay").into()),
            target: self.target.clone(),
            callout: Some(callout.into_any_element()),
            options,
            backdrop,
            surface: t.color.background_surface,
            accent: t.color.accent,
            dim: t.color.overlay,
            focus: pending_focus.then_some(region),
        };
        // A non-modal layer with no light dismiss: a press on the page or the target
        // leaves the tour running; Escape ends it through the layer stack.
        let escape = tour.dismiss.clone();
        div()
            .id(self.id)
            .absolute()
            .size_0()
            .child(
                layer(overlay, move |window, cx| {
                    escape(TourDismissSource::Escape, window, cx)
                })
                .light_dismiss(false),
            )
            .into_any_element()
    }
}
