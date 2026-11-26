//! SideNavCollapseButton, upstream `SideNav/SideNavCollapseButton.tsx`: the ghost
//! icon button that collapses and expands the rail. Inside a SideNav it reads the
//! nav's state; outside, it takes the same controlled config the SideNav gets.

use std::rc::Rc;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{ElementSize, resolve_size, use_context};
use gpui::{AnyElement, App, IntoElement, RenderOnce, SharedString, Window, div};

use super::{SideNavCollapse, SideNavCollapseConfig};
use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconColor, IconSize};

/// Upstream `SideNavCollapseButton`.
#[derive(IntoElement)]
pub struct SideNavCollapseButton {
    collapsible: Option<SideNavCollapseConfig>,
    label: Option<SharedString>,
    size: Option<ElementSize>,
    content: Option<AnyElement>,
}

impl Default for SideNavCollapseButton {
    fn default() -> Self {
        Self::new()
    }
}

impl SideNavCollapseButton {
    pub fn new() -> Self {
        SideNavCollapseButton {
            collapsible: None,
            label: None,
            size: None,
            content: None,
        }
    }

    /// Upstream `collapsible`: the controlled config, for a button outside the SideNav.
    pub fn collapsible(mut self, config: SideNavCollapseConfig) -> Self {
        self.collapsible = Some(config);
        self
    }

    /// Upstream `label`: the button's name.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `size`: from the container (small in a SideNav footer), else medium.
    pub fn size(mut self, size: ElementSize) -> Self {
        self.size = Some(size);
        self
    }

    /// Upstream `children`: replaces the chevron.
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }
}

impl RenderOnce for SideNavCollapseButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let i18n = i18n(cx);
        let rtl = use_direction(cx).is_rtl();
        let state = match &self.collapsible {
            Some(config) => {
                let collapsed = config.is_collapsed.unwrap_or(false);
                let report = config.on_collapsed_change.clone();
                SideNavCollapse {
                    collapsed,
                    collapsible: true,
                    toggle: Rc::new(move |window, cx| {
                        if let Some(f) = &report {
                            f(!collapsed, window, cx);
                        }
                    }),
                }
            }
            None => use_context::<SideNavCollapse>(cx).unwrap_or_else(SideNavCollapse::expanded),
        };
        if !state.collapsible {
            return div().into_any_element();
        }
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let label = self.label.unwrap_or_else(|| {
            if state.collapsed {
                i18n.t("@astryx.sideNavCollapseButton.expandSidebar")
            } else {
                i18n.t("@astryx.sideNavCollapseButton.collapseSidebar")
            }
        });
        // The chevron points toward the edge the rail moves to; it mirrors in RTL.
        let points_start = !state.collapsed != rtl;
        let icon = self.content.unwrap_or_else(|| {
            Icon::new(if points_start {
                "chevronLeft"
            } else {
                "chevronRight"
            })
            .size(IconSize::Sm)
            .color(IconColor::Inherit)
            .into_any_element()
        });
        let toggle = state.toggle.clone();
        let _ = window;
        IconButton::new("side-nav-collapse", label, icon)
            .variant(ButtonVariant::Ghost)
            .size(size)
            .on_click(move |_, window, cx| toggle(window, cx))
            .into_any_element()
    }
}
