//! ScrollableArea, upstream `ScrollableArea/` (component spec
//! `ScrollableArea.spec.md`; system spec AST-025): one scroll viewport with a real
//! content box, over `astryx_primitives::scroll_area`.

use astryx_i18n::{LogicalStyled, use_direction};
use astryx_primitives::scroll_area::{
    ScrollAxis, ScrollKeyboardAccess, ScrollOverscroll, ScrollRole, ScrollStickyContainment,
    UseScrollableAreaOptions, use_scrollable_area,
};
use astryx_primitives::{coarse_primary_pointer, use_context};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px,
};

use crate::layout::{
    ContainerChild, ContainerCorners, ContainerPadding, Padding, Sizing, padding_setters,
    size_setters,
};

/// Upstream `ScrollableArea`.
#[derive(IntoElement)]
pub struct ScrollableArea {
    id: ElementId,
    label: SharedString,
    axis: ScrollAxis,
    role: ScrollRole,
    overscroll: ScrollOverscroll,
    sticky_containment: ScrollStickyContainment,
    size: Sizing,
    padding: Padding,
    full_bleed: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl ScrollableArea {
    /// `label` names the viewport when it joins the tab order.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        ScrollableArea {
            id: id.into(),
            label: label.into(),
            axis: ScrollAxis::Block,
            role: ScrollRole::Group,
            overscroll: ScrollOverscroll::Allow,
            sticky_containment: ScrollStickyContainment::WhenScrollable,
            size: Sizing::default(),
            padding: Padding::default(),
            full_bleed: false,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `axis`: the logical axes that scroll (block by default).
    pub fn axis(mut self, axis: ScrollAxis) -> Self {
        self.axis = axis;
        self
    }

    /// Upstream `role`: `group` (the default) or a `region` landmark.
    pub fn role(mut self, role: ScrollRole) -> Self {
        self.role = role;
        self
    }

    /// Upstream `overscroll`: `Contain` keeps a scroll at an edge here.
    pub fn overscroll(mut self, overscroll: ScrollOverscroll) -> Self {
        self.overscroll = overscroll;
        self
    }

    /// Upstream `stickyContainment`. GPUI has no sticky positioning, so both values
    /// behave alike (KNOWN_ISSUES).
    pub fn sticky_containment(mut self, containment: ScrollStickyContainment) -> Self {
        self.sticky_containment = containment;
        self
    }

    /// Upstream `isFullBleed`: the viewport reaches past its container's padding.
    pub fn full_bleed(mut self, full_bleed: bool) -> Self {
        self.full_bleed = full_bleed;
        self
    }

    padding_setters!();

    size_setters!();
}

impl ParentElement for ScrollableArea {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for ScrollableArea {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ScrollableArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let dir = use_direction(cx);
        let rem = window.rem_size();
        let coarse = coarse_primary_pointer(window, cx);
        let area = use_scrollable_area(
            &self.id,
            UseScrollableAreaOptions {
                axis: self.axis,
                keyboard_access: ScrollKeyboardAccess::Viewport {
                    label: self.label.clone(),
                    role: self.role,
                },
                overscroll: self.overscroll,
                sticky_containment: self.sticky_containment,
            },
            window,
            cx,
        );

        // The content box applies its padding and publishes it, zero by default (FR5).
        let [start, end, top, bottom] = self.padding.resolve();
        let len = |step: Option<crate::layout::SpacingStep>| {
            step.map(|s| s.token(&t).to_pixels(rem)).unwrap_or(px(0.))
        };
        let published = ContainerPadding {
            inline_start: len(start),
            inline_end: len(end),
            block_start: len(top),
            block_end: len(bottom),
        };
        let content =
            self.padding
                .apply(area.content(div()), &t, dir)
                .child(astryx_primitives::provide(
                    // Anchors inside scroll with this region (Outline's scroll root).
                    astryx_primitives::anchor::ScrollRoot(area.scroll_handle().clone()),
                    crate::layout::host_children(
                        self.children,
                        published,
                        ContainerCorners::default(),
                    ),
                ));

        let mut viewport = div().relative().max_w_full().max_h_full();
        viewport = self.size.apply(viewport);
        theme.refine(
            viewport.style(),
            "scrollable-area",
            &[("axis", self.axis.as_str())],
            None,
        );
        // Only the viewport consumes the inherited inset (FR5).
        if self.full_bleed {
            let pad = use_context::<ContainerPadding>(cx).unwrap_or_default();
            let place = use_context::<ContainerChild>(cx).unwrap_or_default();
            viewport = viewport
                .margin_start(dir, -pad.inline_start)
                .margin_end(dir, -pad.inline_end);
            viewport.style().max_size.width = None;
            if place.first {
                viewport = viewport.mt(-pad.block_start);
            }
            if place.last {
                viewport = viewport.mb(-pad.block_end);
            }
        }
        viewport.style().refine(&self.style);
        let mut viewport = area.viewport(viewport);
        // A fine pointer's thumb takes the neutral color on a clear track; a coarse
        // pointer keeps the platform's indicator (FR3).
        if !coarse {
            viewport = viewport.scrollbar_color(t.color.neutral, gpui::transparent_black());
        }
        let focus_visible = area.is_focus_visible(window);
        // The focus ring sits over the visible area.
        let mut overlay = super::fade::visible_box(area.scroll_handle());
        if focus_visible {
            overlay = overlay.child(
                crate::focus::focus_ring_inset(&t, px(0.), rem).debug_selector({
                    let id = format!("{}-focus-ring", self.id);
                    move || id.clone()
                }),
            );
        }
        let viewport = viewport.child(content).child(overlay);
        viewport.debug_selector({
            let id = format!("{}", self.id);
            move || id.clone()
        })
    }
}
