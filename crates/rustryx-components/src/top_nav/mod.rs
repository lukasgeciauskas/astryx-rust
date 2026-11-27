//! TopNav and its parts. Upstream: `TopNav/` (component specs `TopNav`, `TopNavHeading`,
//! `TopNavItem`, `TopNavMenu`, `TopNavMegaMenu`, `TopNavMegaMenuItem`,
//! `TopNavMegaMenuFeaturedCard`).

pub mod examples;
mod heading;
mod hover;
mod item;
mod mega;
mod menu;
#[allow(clippy::module_inception)]
mod top_nav;

#[cfg(test)]
mod tests;

pub use heading::TopNavHeading;
pub use item::TopNavItem;
pub use mega::{TopNavMegaMenu, TopNavMegaMenuFeaturedCard, TopNavMegaMenuItem};
pub use menu::{TopNavMenu, TopNavMenuItemData};
pub(crate) use top_nav::TopNavMobileContent;
pub use top_nav::{TopNav, TopNavRenderMode, TopNavSlot};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const NO_AS: &str = "GPUI has no link components; links go through the link provider";

pub const TOP_NAV_DOC: ComponentDoc = ComponentDoc {
    name: "TopNav",
    rust_type: "rustryx::components::top_nav::TopNav",
    props: &[
        p("heading", RustApi::Method("heading")),
        p("startContent", RustApi::Method("start_content")),
        p("children", RustApi::Trait("ParentElement")),
        p("centerContent", RustApi::Method("center_content")),
        p("endContent", RustApi::Method("end_content")),
        p("label", RustApi::Method("label")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const TOP_NAV_HEADING_DOC: ComponentDoc = ComponentDoc {
    name: "TopNavHeading",
    rust_type: "rustryx::components::top_nav::TopNavHeading",
    props: &[
        p("logo", RustApi::Method("logo")),
        p("logoLabel", RustApi::Method("logo_label")),
        p("heading", RustApi::Method("heading")),
        p("headingHref", RustApi::Method("heading_href")),
        p("superheading", RustApi::Method("superheading")),
        p("superheadingHref", RustApi::Method("superheading_href")),
        p("subheading", RustApi::Method("subheading")),
        p("subheadingHref", RustApi::Method("subheading_href")),
        p("headerEndContent", RustApi::Method("header_end_content")),
        p("menu", RustApi::Method("menu")),
        p("as", RustApi::Waived(NO_AS)),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const TOP_NAV_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "TopNavItem",
    rust_type: "rustryx::components::top_nav::TopNavItem",
    props: &[
        p("label", RustApi::Argument("label")),
        p("href", RustApi::Method("href")),
        p("isSelected", RustApi::Method("selected")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isIconOnly", RustApi::Method("icon_only")),
        p("icon", RustApi::Method("icon")),
        p("size", RustApi::Method("size")),
        p("children", RustApi::Trait("ParentElement")),
        p("as", RustApi::Waived(NO_AS)),
    ],
    example: include_str!("examples.rs"),
};

pub const TOP_NAV_MENU_DOC: ComponentDoc = ComponentDoc {
    name: "TopNavMenu",
    rust_type: "rustryx::components::top_nav::TopNavMenu",
    props: &[
        p("label", RustApi::Argument("label")),
        p("items", RustApi::Argument("items")),
        p("delay", RustApi::Method("delay")),
        p("hideDelay", RustApi::Method("hide_delay")),
    ],
    example: include_str!("examples.rs"),
};

pub const TOP_NAV_MEGA_MENU_DOC: ComponentDoc = ComponentDoc {
    name: "TopNavMegaMenu",
    rust_type: "rustryx::components::top_nav::TopNavMegaMenu",
    props: &[
        p("label", RustApi::Argument("label")),
        p("items", RustApi::Method("item")),
        p("featured", RustApi::Method("featured")),
        p("delay", RustApi::Method("delay")),
        p("hideDelay", RustApi::Method("hide_delay")),
        p("onOpenChange", RustApi::Method("on_open_change")),
    ],
    example: include_str!("examples.rs"),
};

pub const TOP_NAV_MEGA_MENU_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "TopNavMegaMenuItem",
    rust_type: "rustryx::components::top_nav::TopNavMegaMenuItem",
    props: &[
        p("title", RustApi::Argument("title")),
        p("description", RustApi::Method("description")),
        p("icon", RustApi::Method("icon")),
        p("href", RustApi::Method("href")),
        p("onClick", RustApi::Method("on_click")),
        p("hasCloseOnSelect", RustApi::Method("close_on_select")),
        p("as", RustApi::Waived(NO_AS)),
    ],
    example: include_str!("examples.rs"),
};

pub const TOP_NAV_MEGA_MENU_FEATURED_CARD_DOC: ComponentDoc = ComponentDoc {
    name: "TopNavMegaMenuFeaturedCard",
    rust_type: "rustryx::components::top_nav::TopNavMegaMenuFeaturedCard",
    props: &[
        p("title", RustApi::Argument("title")),
        p("description", RustApi::Method("description")),
        p("image", RustApi::Method("image")),
        p("imageAlt", RustApi::Method("image_alt")),
        p("linkLabel", RustApi::Method("link_label")),
        p("linkHref", RustApi::Method("link_href")),
        p("children", RustApi::Trait("ParentElement")),
    ],
    example: include_str!("examples.rs"),
};
