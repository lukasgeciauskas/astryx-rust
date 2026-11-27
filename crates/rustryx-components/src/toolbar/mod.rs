//! Toolbar. Upstream: `Toolbar/`.
//!
//! A bar of controls in start, center and end lanes. It is one tab stop:
//! the arrows move among its controls (every control built on
//! `control_focus` joins its roving scope), and its size reaches them.

#[cfg(test)]
mod tests;

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*,
};
use rustryx_i18n::{LogicalStyled, use_direction};
use rustryx_primitives::{ElementSize, ListKeys, RovingScope, provide};
use rustryx_theme::current;

use crate::containers::{Section, SectionSide, SectionVariant};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::layout::{Orientation, SpacingStep};

/// A bar of contextual actions, upstream `Toolbar`.
///
/// ```ignore
/// Toolbar::new("format", "Formatting")
///     .start_content(ButtonGroup::new("style", "Style").child(..))
///     .end_content(IconButton::new("more", "More", Icon::new("moreHorizontal")))
/// ```
#[derive(IntoElement)]
pub struct Toolbar {
    id: ElementId,
    label: SharedString,
    start_content: Option<AnyElement>,
    center_content: Option<AnyElement>,
    end_content: Option<AnyElement>,
    size: ElementSize,
    gap: SpacingStep,
    orientation: Orientation,
    variant: SectionVariant,
    dividers: Vec<SectionSide>,
    style: StyleRefinement,
}

impl Toolbar {
    /// Upstream `label`: the toolbar's accessible name.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Toolbar {
            id: id.into(),
            label: label.into(),
            start_content: None,
            center_content: None,
            end_content: None,
            size: ElementSize::Md,
            gap: SpacingStep::S1,
            orientation: Orientation::Horizontal,
            variant: SectionVariant::Transparent,
            dividers: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `size`: reaches every control inside.
        size: ElementSize,
        /// Upstream `gap`: between controls.
        gap: SpacingStep,
        /// Upstream `orientation`.
        orientation: Orientation,
        /// Upstream `variant`: the Section surface.
        variant: SectionVariant,
    }

    /// Upstream `startContent`.
    pub fn start_content(mut self, content: impl IntoElement) -> Self {
        self.start_content = Some(content.into_any_element());
        self
    }

    /// Upstream `centerContent`: centered between the side lanes.
    pub fn center_content(mut self, content: impl IntoElement) -> Self {
        self.center_content = Some(content.into_any_element());
        self
    }

    /// Upstream `endContent`.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `dividers`: the sides with a border.
    pub fn dividers(mut self, sides: impl IntoIterator<Item = SectionSide>) -> Self {
        self.dividers = sides.into_iter().collect();
        self
    }
}

impl Styled for Toolbar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Toolbar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let vertical = self.orientation == Orientation::Vertical;
        let keys = if vertical {
            ListKeys::vertical()
        } else {
            ListKeys::horizontal()
        }
        .rtl(dir.is_rtl());
        let scope = RovingScope::begin(&self.id, keys, window, cx);
        let hint = crate::keyboard_hint::use_keyboard_hint(
            &self.id,
            crate::keyboard_hint::KeyboardHintOptions::new(if vertical {
                crate::keyboard_hint::KeyboardHintOrientation::Vertical
            } else {
                crate::keyboard_hint::KeyboardHintOrientation::Horizontal
            }),
            scope.contains_focus(window),
            window,
            cx,
        );
        let gap = self.gap.token(t);
        let lane = |content: Option<AnyElement>| {
            div()
                .flex()
                .items_center()
                .gap(gap)
                .when(!vertical, |el| el.flex_row().flex_wrap())
                .when(vertical, |el| el.flex_col().items_stretch())
                .children(content)
        };

        let mut bar = div().flex().gap(gap).min_h(t.size.element_sm);
        bar = if vertical {
            bar.flex_col().items_stretch()
        } else {
            bar.flex_row().flex_wrap().items_center()
        };
        bar = match self.center_content {
            Some(center) => bar
                .child(lane(self.start_content).flex_1().min_w_0())
                .child(lane(Some(center)).justify_center().flex_none())
                .child(lane(self.end_content).flex_1().min_w_0().justify_end()),
            None => match (self.start_content, self.end_content) {
                (Some(start), Some(end)) => bar
                    .justify_between()
                    .child(lane(Some(start)))
                    .child(lane(Some(end))),
                (Some(start), None) => bar.child(lane(Some(start)).flex_1()),
                (None, Some(end)) => {
                    bar.child(lane(Some(end)).margin_start(dir, gpui::Length::Auto))
                }
                (None, None) => bar,
            },
        };
        theme.refine(
            bar.style(),
            "toolbar",
            &[("size", self.size.as_str())],
            None,
        );
        let bar = bar
            .id(self.id.clone())
            .on_key_down(scope.key_down_listener())
            .role(Role::Toolbar)
            .aria_label(self.label);
        let bar = hint.wrap(bar, window, cx);

        let mut section = Section::new()
            .variant(self.variant)
            .padding_block(SpacingStep::S2)
            .dividers(self.dividers);
        section.style().refine(&self.style);
        provide(scope, provide(self.size, section.child(bar)))
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const TOOLBAR_DOC: ComponentDoc = ComponentDoc {
    name: "Toolbar",
    rust_type: "rustryx::components::toolbar::Toolbar",
    props: &[
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("startContent", RustApi::Method("start_content")),
        p("centerContent", RustApi::Method("center_content")),
        p("endContent", RustApi::Method("end_content")),
        p("label", RustApi::Argument("label")),
        p("size", RustApi::Method("size")),
        p("gap", RustApi::Method("gap")),
        p("orientation", RustApi::Method("orientation")),
        p("variant", RustApi::Method("variant")),
        p("dividers", RustApi::Method("dividers")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("../segmented/examples.rs"),
};
