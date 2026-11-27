//! Navigation parts shared by SideNav and TopNav: NavIcon, NavHeadingMenu and
//! NavHeadingMenuItem. Upstream: `NavIcon/`, `NavMenu/`.

mod drawer;
pub mod examples;
mod icon;
mod menu;

#[cfg(test)]
mod tests;

pub(crate) use drawer::{close_drawer_after, drawer_close, nav_item_row};
pub use icon::NavIcon;
pub(crate) use menu::{NavHeadingClose, NavHeadingDismiss, NavHeadingMenuHost};
pub use menu::{NavHeadingMenu, NavHeadingMenuItem, NavHeadingMenuSize};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const NAV_ICON_DOC: ComponentDoc = ComponentDoc {
    name: "NavIcon",
    rust_type: "rustryx::components::nav::NavIcon",
    props: &[p("icon", RustApi::Argument("icon"))],
    example: include_str!("examples.rs"),
};

pub const NAV_HEADING_MENU_DOC: ComponentDoc = ComponentDoc {
    name: "NavHeadingMenu",
    rust_type: "rustryx::components::nav::NavHeadingMenu (rows: NavHeadingMenuItem)",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("size", RustApi::Method("size")),
        p("minWidth", RustApi::Method("min_width")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
