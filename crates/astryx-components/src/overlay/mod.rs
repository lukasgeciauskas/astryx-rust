//! Overlays (M5): Tooltip and InfoTip, Popover, and the rest of the layer
//! family as it lands.

mod alert_dialog;
mod command_palette;
mod context_menu;
mod dialog;
mod disclosure;
mod drawer;
mod dropdown;
pub mod examples;
mod hover_card;
mod info_tip;
mod lightbox;
mod menu;
mod modal;
mod motion;
mod popover;
mod scrim;
mod sheet;
mod toast;
mod tooltip;
mod zoom_stage;

#[cfg(test)]
mod dialog_tests;
#[cfg(test)]
mod lightbox_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod palette_tests;
#[cfg(test)]
mod popover_tests;
#[cfg(test)]
mod tests;

pub use alert_dialog::{AlertDialog, AlertDialogController, AlertOptions};
pub use command_palette::{
    CommandPalette, CommandPaletteEmpty, CommandPaletteFooter, CommandPaletteGroup,
    CommandPaletteInput, CommandPaletteItem, CommandPaletteList,
};
pub use context_menu::ContextMenu;
pub(crate) use context_menu::{ContextSpec, InvokeFn, with_context_menu};
pub(crate) use context_menu::{context_menu_button, with_row_menu};
pub(crate) use dialog::focus_scope;
pub use dialog::{
    AutoFocus, Dialog, DialogController, DialogHeader, DialogPosition, DialogPurpose, DialogVariant,
};
pub(crate) use disclosure::{Disclosure, disclosure};
pub use drawer::{Drawer, DrawerHeader, DrawerSide};
pub(crate) use dropdown::MenuTrigger;
pub use dropdown::{DropdownMenu, MoreMenu};
pub use hover_card::HoverCard;
pub use info_tip::InfoTip;
pub use lightbox::{
    Lightbox, LightboxHandle, LightboxMedia, LightboxMediaKind, LightboxTrigger, use_lightbox,
};
#[cfg(test)]
pub(crate) use menu::in_safe_triangle;
pub use menu::{
    MenuAction, MenuCheckboxItem, MenuEntry, MenuItem, MenuRadioGroup, MenuRadioItem, MenuSize,
    SubmenuPresentation,
};
pub(crate) use motion::{clearance, entrance, gutter, viewport_cap};
pub(crate) use popover::presents_as_sheet;
pub use popover::{Popover, PopoverController, PopoverPresentation, PopoverRole};
pub use scrim::{Overlay, OverlayAlign, OverlayPosition, OverlayScrim, OverlayShowOn};
pub use sheet::{BottomSheet, BottomSheetSwitcher, SheetHeight, SnapPoint};
pub use toast::{
    Toast, ToastCollision, ToastDismissReason, ToastHandle, ToastOptions, ToastPosition, ToastType,
    ToastViewport, show_toast,
};
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
        p("presentation", RustApi::Method("presentation")),
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

pub const DRAWER_DOC: ComponentDoc = ComponentDoc {
    name: "Drawer",
    rust_type: "astryx::components::overlay::Drawer",
    props: &[
        p("isOpen", RustApi::Argument("open")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("label", RustApi::Argument("label")),
        p("children", RustApi::Trait("ParentElement")),
        p("side", RustApi::Method("side")),
        p("width", RustApi::Method("width")),
        p("experimental", RustApi::Method("full_width_on_mobile")),
        p("hasScrim", RustApi::Method("scrim")),
        p("purpose", RustApi::Method("purpose")),
        p("padding", RustApi::Method("padding")),
    ],
    example: include_str!("examples.rs"),
};

pub const DRAWER_HEADER_DOC: ComponentDoc = ComponentDoc {
    name: "DrawerHeader",
    rust_type: "astryx::components::overlay::DrawerHeader",
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

pub const BOTTOM_SHEET_DOC: ComponentDoc = ComponentDoc {
    name: "BottomSheet",
    rust_type: "astryx::components::overlay::BottomSheet",
    props: &[
        p("isOpen", RustApi::Argument("open")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p(
            "finalFocusRef",
            RustApi::Waived("GPUI has no refs: focus returns to what had it when the sheet opened"),
        ),
        p("purpose", RustApi::Method("purpose")),
        p("sheetId", RustApi::Method("sheet_id")),
        p("label", RustApi::Argument("label")),
        p("children", RustApi::Trait("ParentElement")),
        p("padding", RustApi::Method("padding")),
        p("height", RustApi::Method("height")),
        p("snapPoints", RustApi::Method("snap_points")),
        p("hasScrim", RustApi::Method("scrim")),
    ],
    example: include_str!("examples.rs"),
};

pub const BOTTOM_SHEET_SWITCHER_DOC: ComponentDoc = ComponentDoc {
    name: "BottomSheetSwitcher",
    rust_type: "astryx::components::overlay::BottomSheetSwitcher",
    props: &[
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p(
            "onCancel",
            RustApi::Waived("Escape reaches the active sheet, which asks to close per its purpose"),
        ),
        p("activeSheet", RustApi::Argument("active_sheet")),
        p(
            "onActiveSheetChange",
            RustApi::Method("on_active_sheet_change"),
        ),
        p("hasScrim", RustApi::Method("scrim")),
        p("children", RustApi::Method("sheet")),
    ],
    example: include_str!("examples.rs"),
};

pub const TOAST_DOC: ComponentDoc = ComponentDoc {
    name: "Toast",
    rust_type: "astryx::components::overlay::Toast",
    props: &[
        p("body", RustApi::Argument("body")),
        p("type", RustApi::Method("kind")),
        p("isAutoHide", RustApi::Method("auto_hide")),
        p("autoHideDuration", RustApi::Method("auto_hide_duration")),
        p("isExiting", RustApi::Method("exiting")),
        p("endContent", RustApi::Method("end_content")),
        p("onDismiss", RustApi::Method("on_dismiss")),
        p(
            "renderContent",
            RustApi::Waived("compose the body freely; the dismiss button stays at the end"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_TOAST_DOC: ComponentDoc = ComponentDoc {
    name: "useToast",
    rust_type: "astryx::components::overlay::show_toast",
    props: &[p(
        "show",
        RustApi::Method("show_toast(ToastOptions, window, cx)"),
    )],
    example: include_str!("examples.rs"),
};

const NO_REFS: &str = "GPUI elements have no refs";
const NO_REL: &str = "GPUI opens URLs without a referrer";
const ROW_STYLE: &str = "rows are themed with dropdown-menu-item rules";
const TEST_IDS: &str = "GPUI tests find elements by id";

pub const DROPDOWN_MENU_DOC: ComponentDoc = ComponentDoc {
    name: "DropdownMenu",
    rust_type: "astryx::components::overlay::DropdownMenu",
    props: &[
        p(
            "button",
            RustApi::Method(
                "button_label / button_variant / button_size / button_icon / icon_only / button",
            ),
        ),
        p("renderTrigger", RustApi::Method("trigger")),
        p("items", RustApi::Method("items")),
        p("presentation", RustApi::Method("presentation")),
        p("isMenuOpen", RustApi::Method("open")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("menuWidth", RustApi::Method("menu_width")),
        p("menuMaxHeight", RustApi::Method("menu_max_height")),
        p("header", RustApi::Method("header")),
        p("footer", RustApi::Method("footer")),
        p("placement", RustApi::Method("placement")),
        p("alignment", RustApi::Method("alignment")),
        p("onClick", RustApi::Method("on_click")),
        p("hasChevron", RustApi::Method("chevron")),
        p("children", RustApi::Method("entry")),
    ],
    example: include_str!("examples.rs"),
};

pub const DROPDOWN_MENU_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "DropdownMenuItem",
    rust_type: "astryx::components::overlay::MenuItem",
    props: &[
        p("icon", RustApi::Method("icon")),
        p("label", RustApi::Argument("label")),
        p("description", RustApi::Method("description")),
        p("onClick", RustApi::Method("on_click")),
        p("href", RustApi::Method("href")),
        p("target", RustApi::Method("target")),
        p("rel", RustApi::Waived(NO_REL)),
        p("isDisabled", RustApi::Method("disabled")),
        p("endContent", RustApi::Method("end_content")),
        p("hasCloseOnSelect", RustApi::Method("close_on_select")),
        p("variant", RustApi::Method("destructive")),
        p("xstyle", RustApi::Waived(ROW_STYLE)),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};

pub const DROPDOWN_MENU_CHECKBOX_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "DropdownMenuCheckboxItem",
    rust_type: "astryx::components::overlay::MenuCheckboxItem",
    props: &[
        p("label", RustApi::Argument("label")),
        p("description", RustApi::Method("description")),
        p("icon", RustApi::Method("icon")),
        p("value", RustApi::Argument("checked")),
        p("onChange", RustApi::Method("on_change")),
        p("isDisabled", RustApi::Method("disabled")),
        p("hasCloseOnSelect", RustApi::Method("close_on_select")),
        p("ref", RustApi::Waived(NO_REFS)),
        p("endContent", RustApi::Method("end_content")),
    ],
    example: include_str!("examples.rs"),
};

pub const DROPDOWN_MENU_RADIO_GROUP_DOC: ComponentDoc = ComponentDoc {
    name: "DropdownMenuRadioGroup",
    rust_type: "astryx::components::overlay::MenuRadioGroup",
    props: &[
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("label", RustApi::Method("title")),
        p("hasCloseOnSelect", RustApi::Method("close_on_select")),
        p("ref", RustApi::Waived(NO_REFS)),
        p("children", RustApi::Method("item")),
    ],
    example: include_str!("examples.rs"),
};

pub const DROPDOWN_MENU_RADIO_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "DropdownMenuRadioItem",
    rust_type: "astryx::components::overlay::MenuRadioItem",
    props: &[
        p("value", RustApi::Argument("value")),
        p("label", RustApi::Argument("label")),
        p("description", RustApi::Method("description")),
        p("icon", RustApi::Method("icon")),
        p("isDisabled", RustApi::Method("disabled")),
        p("endContent", RustApi::Method("end_content")),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};

pub const DROPDOWN_MENU_GROUP_DOC: ComponentDoc = ComponentDoc {
    name: "DropdownMenuGroup",
    rust_type: "astryx::components::overlay::MenuEntry::section",
    props: &[
        p("title", RustApi::Argument("title")),
        p("children", RustApi::Argument("entries")),
        p(
            "xstyle",
            RustApi::Waived("sections are themed with dropdown-menu-section-heading rules"),
        ),
        p("className", RustApi::Waived(ROW_STYLE)),
        p("style", RustApi::Waived(ROW_STYLE)),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};

pub const DROPDOWN_MENU_DIVIDER_DOC: ComponentDoc = ComponentDoc {
    name: "DropdownMenuDivider",
    rust_type: "astryx::components::overlay::MenuEntry::Divider",
    props: &[p(
        "xstyle",
        RustApi::Waived("dividers are themed with dropdown-menu-divider rules"),
    )],
    example: include_str!("examples.rs"),
};

pub const DROPDOWN_MENU_SUB_MENU_DOC: ComponentDoc = ComponentDoc {
    name: "DropdownMenuSubMenu",
    rust_type: "astryx::components::overlay::MenuItem",
    props: &[
        p("label", RustApi::Argument("label")),
        p("icon", RustApi::Method("icon")),
        p("description", RustApi::Method("description")),
        p("children", RustApi::Method("submenu")),
        p("isDisabled", RustApi::Method("disabled")),
        p("hasSpinner", RustApi::Method("spinner")),
        p("menuWidth", RustApi::Method("submenu_width")),
        p("onOpenChange", RustApi::Method("on_submenu_open_change")),
        p("presentation", RustApi::Method("submenu_presentation")),
        p("delay", RustApi::Method("submenu_delay")),
        p("hideDelay", RustApi::Method("submenu_hide_delay")),
        p("header", RustApi::Method("submenu_header")),
        p("footer", RustApi::Method("submenu_footer")),
        p("menuDataTestId", RustApi::Waived(TEST_IDS)),
        p("xstyle", RustApi::Waived(ROW_STYLE)),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};

pub const CONTEXT_MENU_DOC: ComponentDoc = ComponentDoc {
    name: "ContextMenu",
    rust_type: "astryx::components::overlay::ContextMenu",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("items", RustApi::Method("items")),
        p("menuContent", RustApi::Method("entry")),
        p("menuWidth", RustApi::Method("menu_width")),
        p("size", RustApi::Method("size")),
        p("label", RustApi::Method("label")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("presentation", RustApi::Method("presentation")),
        p("isDisabled", RustApi::Method("disabled")),
        p("triggerXstyle", RustApi::Trait("Styled")),
        p("triggerAs", RustApi::Method("inline")),
    ],
    example: include_str!("examples.rs"),
};

pub const CONTEXT_MENU_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "ContextMenuItem",
    rust_type: "astryx::components::overlay::MenuItem",
    props: &[
        p("icon", RustApi::Method("icon")),
        p("label", RustApi::Argument("label")),
        p("description", RustApi::Method("description")),
        p("onClick", RustApi::Method("on_click")),
        p("href", RustApi::Method("href")),
        p("target", RustApi::Method("target")),
        p("rel", RustApi::Waived(NO_REL)),
        p("isDisabled", RustApi::Method("disabled")),
        p("endContent", RustApi::Method("end_content")),
        p("hasCloseOnSelect", RustApi::Method("close_on_select")),
        p("variant", RustApi::Method("destructive")),
        p("xstyle", RustApi::Waived(ROW_STYLE)),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};

pub const MORE_MENU_DOC: ComponentDoc = ComponentDoc {
    name: "MoreMenu",
    rust_type: "astryx::components::overlay::MoreMenu",
    props: &[
        p("items", RustApi::Argument("entries")),
        p("label", RustApi::Method("label")),
        p("variant", RustApi::Method("variant")),
        p("size", RustApi::Method("size")),
        p("icon", RustApi::Method("icon")),
        p("isDisabled", RustApi::Method("disabled")),
        p("placement", RustApi::Method("placement")),
        p("alignment", RustApi::Method("alignment")),
        p("presentation", RustApi::Method("presentation")),
        p("isMenuOpen", RustApi::Method("open")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

const VIDEO: &str = "video plays through MediaPlayer's decoder (M13); until then a video item opens in the system player";

pub const LIGHTBOX_DOC: ComponentDoc = ComponentDoc {
    name: "Lightbox",
    rust_type: "astryx::components::overlay::Lightbox",
    props: &[
        p("isOpen", RustApi::Argument("open")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("media", RustApi::Argument("media")),
        p("index", RustApi::Method("index")),
        p("onIndexChange", RustApi::Method("on_index_change")),
        p("hasZoom", RustApi::Method("has_zoom")),
        p("defaultIndex", RustApi::Method("default_index")),
        p("hasAutoPlay", RustApi::Pending("has_auto_play", VIDEO)),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_LIGHTBOX_DOC: ComponentDoc = ComponentDoc {
    name: "useLightbox",
    rust_type: "astryx::components::overlay::use_lightbox",
    props: &[
        p(
            "options",
            RustApi::Method("use_lightbox(id, media, window, cx)"),
        ),
        p("options.media", RustApi::Argument("media")),
        p("options.hasZoom", RustApi::Method("element(cx).has_zoom")),
        p(
            "options.hasAutoPlay",
            RustApi::Pending("element(cx).has_auto_play", VIDEO),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const COMMAND_PALETTE_DOC: ComponentDoc = ComponentDoc {
    name: "CommandPalette",
    rust_type: "astryx::components::overlay::CommandPalette",
    props: &[
        p("isOpen", RustApi::Argument("open")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p(
            "searchSource",
            RustApi::Argument("source (new / from_rc; source_key)"),
        ),
        p("input", RustApi::Method("input")),
        p("footer", RustApi::Method("footer")),
        p("renderItem", RustApi::Method("render_item")),
        p(
            "emptySearchText",
            RustApi::Method("empty_search_text / empty_search"),
        ),
        p(
            "emptyBootstrapText",
            RustApi::Method("empty_bootstrap_text / empty_bootstrap"),
        ),
        p("value", RustApi::Method("value")),
        p("onValueChange", RustApi::Method("on_value_change")),
        p("label", RustApi::Method("label")),
        p("width", RustApi::Method("width")),
        p("maxHeight", RustApi::Method("max_height")),
        p("isInline", RustApi::Method("inline")),
    ],
    example: include_str!("examples.rs"),
};

pub const COMMAND_PALETTE_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "CommandPaletteInput",
    rust_type: "astryx::components::overlay::CommandPaletteInput",
    props: &[
        p("placeholder", RustApi::Method("placeholder")),
        p("label", RustApi::Method("label")),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("endContent", RustApi::Method("end_content")),
        p("value", RustApi::Method("value")),
        p("onValueChange", RustApi::Method("on_value_change")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const COMMAND_PALETTE_FOOTER_DOC: ComponentDoc = ComponentDoc {
    name: "CommandPaletteFooter",
    rust_type: "astryx::components::overlay::CommandPaletteFooter",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const COMMAND_PALETTE_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "CommandPaletteItem",
    rust_type: "astryx::components::overlay::CommandPaletteItem",
    props: &[
        p("value", RustApi::Argument("value")),
        p("children", RustApi::Argument("content")),
        p("onSelect", RustApi::Method("on_select")),
        p("isHighlighted", RustApi::Method("highlighted")),
        p("isSelected", RustApi::Method("selected")),
        p("isDisabled", RustApi::Method("disabled")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const COMMAND_PALETTE_GROUP_DOC: ComponentDoc = ComponentDoc {
    name: "CommandPaletteGroup",
    rust_type: "astryx::components::overlay::CommandPaletteGroup",
    props: &[
        p("heading", RustApi::Argument("heading")),
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const COMMAND_PALETTE_EMPTY_DOC: ComponentDoc = ComponentDoc {
    name: "CommandPaletteEmpty",
    rust_type: "astryx::components::overlay::CommandPaletteEmpty",
    props: &[p("children", RustApi::Argument("content"))],
    example: include_str!("examples.rs"),
};

pub const COMMAND_PALETTE_LIST_DOC: ComponentDoc = ComponentDoc {
    name: "CommandPaletteList",
    rust_type: "astryx::components::overlay::CommandPaletteList",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("label", RustApi::Method("label")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
