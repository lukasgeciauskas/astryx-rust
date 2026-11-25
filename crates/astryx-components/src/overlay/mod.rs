//! Overlays (M5): Tooltip and InfoTip, Popover, and the rest of the layer
//! family as it lands.

mod alert_dialog;
mod dialog;
mod disclosure;
pub mod examples;
mod hover_card;
mod info_tip;
mod motion;
mod scrim;
mod popover;
mod tooltip;

#[cfg(test)]
mod dialog_tests;
#[cfg(test)]
mod popover_tests;
#[cfg(test)]
mod tests;

pub use alert_dialog::{AlertDialog, AlertDialogController, AlertOptions};
pub(crate) use dialog::focus_scope;
pub use dialog::{
    AutoFocus, Dialog, DialogController, DialogHeader, DialogPosition, DialogPurpose, DialogVariant,
};
pub(crate) use disclosure::disclosure;
pub use hover_card::HoverCard;
pub use info_tip::InfoTip;
pub use scrim::{Overlay, OverlayAlign, OverlayPosition, OverlayScrim, OverlayShowOn};
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

pub const HOVER_CARD_DOC: ComponentDoc = ComponentDoc {
    name: "HoverCard",
    rust_type: "astryx::components::overlay::HoverCard",
    props: &[
        p("children", RustApi::Method("child / text")),
        p("content", RustApi::Method("content / content_fn")),
        p("placement", RustApi::Method("placement")),
        p("alignment", RustApi::Method("alignment")),
        p("delay", RustApi::Method("delay")),
        p("hideDelay", RustApi::Method("hide_delay")),
        p("focusTrigger", RustApi::Method("focus_trigger")),
        p("touchTrigger", RustApi::Method("touch_trigger")),
        p("isEnabled", RustApi::Method("enabled")),
        p("label", RustApi::Method("label")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("hasHoverIndication", RustApi::Method("hover_indication")),
        p("isDefaultOpen", RustApi::Method("default_open")),
        p("isOpen", RustApi::Method("open")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const DIALOG_DOC: ComponentDoc = ComponentDoc {
    name: "Dialog",
    rust_type: "astryx::components::overlay::Dialog",
    props: &[
        p("isOpen", RustApi::Argument("open")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("children", RustApi::Trait("ParentElement")),
        p("width", RustApi::Method("width")),
        p("maxHeight", RustApi::Method("max_height")),
        p("position", RustApi::Method("position")),
        p("variant", RustApi::Method("variant")),
        p("purpose", RustApi::Method("purpose")),
        p("padding", RustApi::Method("padding")),
        p("isInline", RustApi::Method("inline")),
    ],
    example: include_str!("examples.rs"),
};

pub const DIALOG_HEADER_DOC: ComponentDoc = ComponentDoc {
    name: "DialogHeader",
    rust_type: "astryx::components::overlay::DialogHeader",
    props: &[
        p("title", RustApi::Argument("title")),
        p("subtitle", RustApi::Method("subtitle")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("startContent", RustApi::Method("start_content")),
        p("endContent", RustApi::Method("end_content")),
        p(
            "endContentEdgeCompensation",
            RustApi::Waived("the header compensates its end slot from its own layout"),
        ),
        p("hasDivider", RustApi::Method("divider")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_IMPERATIVE_DIALOG_DOC: ComponentDoc = ComponentDoc {
    name: "useImperativeDialog",
    rust_type: "astryx::components::overlay::DialogController",
    props: &[
        p("show", RustApi::Method("show")),
        p("hide", RustApi::Method("hide")),
        p("isOpen", RustApi::Method("is_open")),
        p("element", RustApi::Method("element")),
    ],
    example: include_str!("examples.rs"),
};

pub const ALERT_DIALOG_DOC: ComponentDoc = ComponentDoc {
    name: "AlertDialog",
    rust_type: "astryx::components::overlay::AlertDialog",
    props: &[
        p("isOpen", RustApi::Argument("open")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("title", RustApi::Argument("title")),
        p("description", RustApi::Argument("description")),
        p("actionLabel", RustApi::Argument("action_label")),
        p("onAction", RustApi::Method("on_action")),
        p("cancelLabel", RustApi::Method("cancel_label")),
        p("actionVariant", RustApi::Method("action_variant")),
        p("isActionLoading", RustApi::Method("action_loading")),
        p("width", RustApi::Method("width")),
        p("isInline", RustApi::Method("inline")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_IMPERATIVE_ALERT_DIALOG_DOC: ComponentDoc = ComponentDoc {
    name: "useImperativeAlertDialog",
    rust_type: "astryx::components::overlay::AlertDialogController",
    props: &[
        p("show", RustApi::Method("show")),
        p("hide", RustApi::Method("hide")),
        p("isOpen", RustApi::Method("is_open")),
        p("element", RustApi::Method("element")),
    ],
    example: include_str!("examples.rs"),
};

pub const OVERLAY_DOC: ComponentDoc = ComponentDoc {
    name: "Overlay",
    rust_type: "astryx::components::overlay::Overlay",
    props: &[
        p("content", RustApi::Method("content")),
        p("children", RustApi::Trait("ParentElement")),
        p("showOn", RustApi::Method("show_on")),
        p("isOpen", RustApi::Method("open")),
        p("scrim", RustApi::Method("scrim")),
        p("position", RustApi::Method("position")),
        p("align", RustApi::Method("align")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
    ],
    example: include_str!("examples.rs"),
};
