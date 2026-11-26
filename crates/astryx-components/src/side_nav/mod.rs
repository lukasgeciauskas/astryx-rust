//! SideNav and its parts. Upstream: `SideNav/` (component specs `SideNav`,
//! `SideNavItem`, `SideNavSection`, `SideNavHeading`, `SideNavCollapseButton`).

mod collapse_button;
pub mod examples;
mod heading;
mod item;
mod section;
#[allow(clippy::module_inception)]
mod side_nav;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui::{App, Window};

pub use collapse_button::SideNavCollapseButton;
pub use heading::SideNavHeading;
pub use item::{SideNavItem, SideNavItemCollapsible};
pub use section::SideNavSection;
pub use side_nav::{SideNav, SideNavCollapseConfig, SideNavResizableConfig};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

/// A callback that acts.
pub(crate) type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// What a SideNav tells its parts: whether its rail presents the collapsed state, its
/// toggle, and its labels' opacity while the rail collapses or expands (FR6).
#[derive(Clone)]
pub(crate) struct SideNavCollapse {
    pub collapsed: bool,
    pub collapsible: bool,
    pub toggle: Action,
    pub labels: f32,
}

impl SideNavCollapse {
    pub(crate) fn expanded() -> Self {
        SideNavCollapse {
            collapsed: false,
            collapsible: false,
            toggle: Rc::new(|_, _| {}),
            labels: 1.,
        }
    }
}

/// What AppShell lends its SideNav (upstream `SideNavShellContext`): its own animated
/// layout handle, whose area holds the rail and the content column that glides beside it
/// (AppShell FR17). Outside AppShell, nothing.
#[derive(Clone)]
pub(crate) struct SideNavShell {
    pub layout: crate::animated_layout::AnimatedLayout,
    /// While the rail expands, the gap its edge has yet to close beside the content
    /// column, which the content surface reaches over; written as the rail renders.
    pub cover: Rc<std::cell::Cell<gpui::Pixels>>,
    /// The rail may draw past its panel's inline edge (a fill shell, whose side region
    /// scrolls only along the block axis): the layout snaps and the edge rides the
    /// content column's glide. An `auto` shell clips its side region, so there the rail's
    /// own box follows its edge.
    pub rides: bool,
}

/// A SideNavItem's nesting depth.
#[derive(Clone, Copy, Default)]
pub(crate) struct ItemDepth(pub usize);

/// Upstream's SideNav render mode: the rail, or (from AppShell and MobileNav) a top
/// bar, a drawer, or a drawer's content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SideNavRenderMode {
    #[default]
    Rail,
    TopBar,
    Drawer,
    DrawerContent,
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const SIDE_NAV_DOC: ComponentDoc = ComponentDoc {
    name: "SideNav",
    rust_type: "astryx::components::side_nav::SideNav",
    props: &[
        p("header", RustApi::Method("header")),
        p("topContent", RustApi::Method("top_content")),
        p("children", RustApi::Trait("ParentElement")),
        p("footer", RustApi::Method("footer")),
        p("footerIcons", RustApi::Method("footer_icons")),
        p(
            "collapsible",
            RustApi::Method("collapsible (collapsible_config)"),
        ),
        p(
            "resizable",
            RustApi::Method("resizable (resizable_config, resizable_region)"),
        ),
        p(
            "handleRef",
            RustApi::Waived(
                "deprecated upstream; give SideNavCollapseButton the same controlled config",
            ),
        ),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const SIDE_NAV_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "SideNavItem",
    rust_type: "astryx::components::side_nav::SideNavItem",
    props: &[
        p("label", RustApi::Argument("label")),
        p(
            "as",
            RustApi::Waived("GPUI has no link components; links go through the link provider"),
        ),
        p("icon", RustApi::Method("icon")),
        p("selectedIcon", RustApi::Method("selected_icon")),
        p("isSelected", RustApi::Method("selected")),
        p("isDisabled", RustApi::Method("disabled")),
        p("href", RustApi::Method("href")),
        p("onClick", RustApi::Method("on_click")),
        p("endContent", RustApi::Method("end_content")),
        p("actions", RustApi::Method("actions")),
        p("children", RustApi::Trait("ParentElement")),
        p("collapsible", RustApi::Method("collapsible")),
        p("size", RustApi::Method("size")),
    ],
    example: include_str!("examples.rs"),
};

pub const SIDE_NAV_SECTION_DOC: ComponentDoc = ComponentDoc {
    name: "SideNavSection",
    rust_type: "astryx::components::side_nav::SideNavSection",
    props: &[
        p("title", RustApi::Argument("title")),
        p("subtitle", RustApi::Method("subtitle")),
        p("children", RustApi::Trait("ParentElement")),
        p("endContent", RustApi::Method("end_content")),
        p("isHeaderHidden", RustApi::Method("header_hidden")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const SIDE_NAV_HEADING_DOC: ComponentDoc = ComponentDoc {
    name: "SideNavHeading",
    rust_type: "astryx::components::side_nav::SideNavHeading",
    props: &[
        p("heading", RustApi::Argument("heading")),
        p("icon", RustApi::Method("icon")),
        p("headingHref", RustApi::Method("heading_href")),
        p("superheading", RustApi::Method("superheading")),
        p("superheadingHref", RustApi::Method("superheading_href")),
        p("subheading", RustApi::Method("subheading")),
        p("subheadingHref", RustApi::Method("subheading_href")),
        p("menu", RustApi::Method("menu")),
        p("headerEndContent", RustApi::Method("header_end_content")),
    ],
    example: include_str!("examples.rs"),
};

pub const SIDE_NAV_COLLAPSE_BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "SideNavCollapseButton",
    rust_type: "astryx::components::side_nav::SideNavCollapseButton",
    props: &[
        p("collapsible", RustApi::Method("collapsible")),
        p(
            "handleRef",
            RustApi::Waived("deprecated upstream; pass collapsible instead"),
        ),
        p("label", RustApi::Method("label")),
        p("size", RustApi::Method("size")),
        p("children", RustApi::Method("content")),
    ],
    example: include_str!("examples.rs"),
};
