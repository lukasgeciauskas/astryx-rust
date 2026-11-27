//! `LineClamp`: arbitrary content clamped to a number of lines with an ellipsis
//! (upstream lab `LineClamp`).
//!
//! `Text`'s `max_lines` clamps a Text; LineClamp clamps whatever it wraps and adds no
//! typography of its own. Its children flow as one paragraph (Core's inline flow, the
//! engine Text's paragraphs use): strings, inline Texts and Links share its lines, and
//! any other element is an inline box. The last line kept ends in an ellipsis. A
//! control on a line past the clamp is never laid out, so it is out of the Tab order and
//! the accessibility tree, as upstream makes it inert, and the element that holds focus
//! is never one of them while it stays visible.
//!
//! Truncation is what the flow reports as it lays its lines out at the clamp's width:
//! whether the clamp ended a line in an ellipsis, and the full text of every child
//! (strings, and the text of inline Texts, Headings, Links and spans; upstream reads the
//! element's text content). A clamped root shows that text in a tooltip on hover and
//! keyboard focus, and becomes a tab stop so the tooltip can be reached without a
//! pointer.

pub mod examples;

#[cfg(test)]
mod tests;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Role, StyleRefinement, Window, div, px,
};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_components::overlay::Tooltip;
use rustryx_components::text::{FlowReport, InlineFlow};
use rustryx_primitives::{Placement, control_focus, is_focus_visible};
use rustryx_theme::current;

/// The element LineClamp stands for (upstream `as`): GPUI draws no HTML, so only a
/// paragraph's role differs; the clamp is a block either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineClampElement {
    #[default]
    Div,
    Span,
    /// A paragraph in the accessibility tree.
    P,
}

/// Arbitrary content clamped to `max_lines` lines (upstream lab `LineClamp`).
///
/// ```ignore
/// LineClamp::new("summary", 2).child("Some mixed inline content that may run long.")
/// ```
#[derive(IntoElement)]
pub struct LineClamp {
    id: ElementId,
    max_lines: usize,
    truncate_tooltip: bool,
    placement: Placement,
    element: LineClampElement,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl LineClamp {
    /// Content clamped to `max_lines` lines (upstream `maxLines`; 0 clamps nothing);
    /// `id` keeps what was measured between frames.
    pub fn new(id: impl Into<ElementId>, max_lines: usize) -> Self {
        LineClamp {
            id: id.into(),
            max_lines,
            truncate_tooltip: true,
            placement: Placement::Above,
            element: LineClampElement::Div,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    /// Upstream `hasTruncateTooltip` (on by default): the full text in a tooltip on
    /// hover or keyboard focus while clamped.
    pub fn truncate_tooltip(mut self, shown: bool) -> Self {
        self.truncate_tooltip = shown;
        self
    }

    /// Upstream `truncateTooltipPlacement` (`Above` by default).
    pub fn truncate_tooltip_placement(mut self, placement: Placement) -> Self {
        self.placement = placement;
        self
    }

    /// Upstream `as` (`Div` by default).
    pub fn element(mut self, element: LineClampElement) -> Self {
        self.element = element;
        self
    }
}

impl Styled for LineClamp {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for LineClamp {
    /// Upstream `children`: the content to clamp.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for LineClamp {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        // What the flow measured last frame: whether the clamp cut content off, and the
        // full text content (upstream `useTruncation` over the element's text).
        let measured = window.use_keyed_state(
            ElementId::Name(format!("{}-line-clamp-truncated", self.id).into()),
            cx,
            |_, _| FlowReport::default(),
        );
        let report = measured.read(cx).clone();
        let tooltip = self.truncate_tooltip && self.max_lines > 0;
        let enabled = tooltip && report.truncated && !report.text.is_empty();

        let mut root = div().relative().min_w_0();
        if self.max_lines > 0 {
            // `overflow: clip`: clipped without becoming a scroll container.
            root = root
                .overflow_hidden()
                .line_clamp(self.max_lines)
                .text_ellipsis();
        }
        theme.refine(root.style(), "line-clamp", &[], None);
        root.style().refine(&self.style);
        let selector = format!("{}-line-clamp", self.id);
        let mut root = root.id(self.id.clone()).debug_selector(move || selector);
        if self.element == LineClampElement::P {
            root = root.role(Role::Paragraph);
        }
        // A truncated clamp is a tab stop, so keyboard users can reveal the full text
        // through the tooltip, which opens on focus.
        if enabled {
            let focus = control_focus(&self.id, window, cx);
            if is_focus_visible(&focus, window) {
                root = root.child(rustryx_components::focus::focus_ring(t, px(0.), rem));
            }
            root = root.track_focus(&focus);
        }
        let flow = InlineFlow::new(self.children).on_report(move |report, window, cx| {
            if *measured.read(cx) != report {
                measured.update(cx, |v, _| *v = report);
                window.on_next_frame(|window, _| window.refresh());
            }
        });
        let root = root.child(flow);
        if !tooltip {
            return root.into_any_element();
        }
        Tooltip::new(
            ElementId::Name(format!("{}-tooltip", self.id).into()),
            report.text,
        )
        .enabled(enabled)
        .placement(self.placement)
        .child(root)
        .into_any_element()
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `LineClamp`.
pub const LINE_CLAMP_DOC: ComponentDoc = ComponentDoc {
    name: "LineClamp",
    rust_type: "rustryx::lab::line_clamp::LineClamp",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("maxLines", RustApi::Argument("max_lines")),
        p("hasTruncateTooltip", RustApi::Method("truncate_tooltip")),
        p(
            "truncateTooltipPlacement",
            RustApi::Method("truncate_tooltip_placement"),
        ),
        p("as", RustApi::Method("element")),
    ],
    example: include_str!("examples.rs"),
};
