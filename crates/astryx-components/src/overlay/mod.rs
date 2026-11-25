//! Overlays (M5): Tooltip and InfoTip, Popover, and the rest of the layer
//! family as it lands.

mod disclosure;
pub mod examples;
mod info_tip;
mod motion;
mod popover;
mod tooltip;

#[cfg(test)]
mod popover_tests;
#[cfg(test)]
mod tests;

pub(crate) use disclosure::disclosure;
pub use info_tip::InfoTip;
pub use popover::{Popover, PopoverController, PopoverRole};
pub(crate) use tooltip::tooltip_description;
pub use tooltip::{FocusTrigger, Tooltip, TooltipContent, TooltipController, TouchTrigger};

use gpui::{AnyElement, ElementId, IntoElement, SharedString};

/// A disabled control's reason, upstream `disabledMessage`: a tooltip above
/// the control on hover and on any focus. Anything else passes through.
pub(crate) fn disabled_reason(
    id: &ElementId,
    disabled: bool,
    message: Option<&SharedString>,
    text_entry: bool,
    control: impl IntoElement,
) -> AnyElement {
    match message.filter(|_| disabled) {
        Some(message) => Tooltip::new(
            ElementId::Name(format!("{id}-disabled-reason").into()),
            message.clone(),
        )
        .focus_trigger(FocusTrigger::Always)
        .text_entry(text_entry)
        .child(control)
        .into_any_element(),
        None => control.into_any_element(),
    }
}

/// Wraps `el` in a tooltip of `text`, if there is one (a `tooltip` prop).
/// `acts`: the element does something when tapped, so a tap leaves the
/// tooltip to hover and focus.
pub(crate) fn with_tooltip(
    id: &ElementId,
    text: Option<SharedString>,
    acts: bool,
    el: impl IntoElement,
) -> AnyElement {
    match text {
        Some(text) => Tooltip::new(ElementId::Name(format!("{id}-tooltip").into()), text)
            .trigger_acts(acts)
            .child(el)
            .into_any_element(),
        None => el.into_any_element(),
    }
}

// ----------------------------------------------------------------------------
// Docs

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const TOOLTIP_DOC: ComponentDoc = ComponentDoc {
    name: "Tooltip",
    rust_type: "astryx::components::overlay::Tooltip",
    props: &[
        p("children", RustApi::Method("child / text")),
        p(
            "anchorRef",
            RustApi::Waived(
                "GPUI has no refs: wrap the anchor with `child` where it renders; elements are rebuilt every frame, so wrapping never remounts it",
            ),
        ),
        p("content", RustApi::Argument("content")),
        p("placement", RustApi::Method("placement")),
        p("alignment", RustApi::Method("alignment")),
        p("delay", RustApi::Method("delay")),
        p("hideDelay", RustApi::Method("hide_delay")),
        p("focusTrigger", RustApi::Method("focus_trigger")),
        p("touchTrigger", RustApi::Method("touch_trigger")),
        p("isEnabled", RustApi::Method("enabled")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("hasHoverIndication", RustApi::Method("hover_indication")),
        p("isDefaultOpen", RustApi::Method("default_open")),
        p("isOpen", RustApi::Method("open")),
    ],
    example: include_str!("examples.rs"),
};

pub const INFO_TIP_DOC: ComponentDoc = ComponentDoc {
    name: "InfoTip",
    rust_type: "astryx::components::overlay::InfoTip",
    props: &[
        p("content", RustApi::Argument("content")),
        p("label", RustApi::Method("label")),
        p("size", RustApi::Method("size")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const POPOVER_DOC: ComponentDoc = ComponentDoc {
    name: "Popover",
    rust_type: "astryx::components::overlay::Popover",
    props: &[
        p("children", RustApi::Method("child")),
        p(
            "anchorRef",
            RustApi::Waived(
                "GPUI has no refs: wrap the anchor with `child` where it renders; elements are rebuilt every frame, so wrapping never remounts it",
            ),
        ),
        p("content", RustApi::Method("content / content_fn")),
        p("placement", RustApi::Method("placement")),
        p("alignment", RustApi::Method("alignment")),
        p("isOpen", RustApi::Method("open")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("isEnabled", RustApi::Method("enabled")),
        p("width", RustApi::Method("width")),
        p("padding", RustApi::Method("padding")),
        p("label", RustApi::Argument("label")),
        p("role", RustApi::Method("role")),
        p("isModal", RustApi::Method("modal")),
        p("hasCloseButton", RustApi::Method("close_button")),
        p("closeButtonLabel", RustApi::Method("close_button_label")),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("hasLightDismiss", RustApi::Method("light_dismiss")),
        p("hasEscapeDismiss", RustApi::Method("escape_dismiss")),
        p(
            "presentation",
            RustApi::Pending(
                "presentation",
                "the sheet presentations land with BottomSheet in M5",
            ),
        ),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
