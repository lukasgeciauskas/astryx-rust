//! The Rust side of each component's documentation.
//!
//! Upstream documents every component in a `.doc.mjs` file (exported to
//! `spec/manifest.json`). Each ported component adds a [`ComponentDoc`] here
//! that maps every upstream prop onto the Rust API and points at the example
//! code the gallery renders, so `astryx component <Name>` can show an agent
//! both sides and `astryx parity` can tell what is left.

/// How one upstream prop appears in the Rust API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RustApi {
    /// A builder method, such as `.variant(ButtonVariant::Primary)`.
    Method(&'static str),
    /// A constructor argument, such as `label` in `Button::new(id, label)`.
    Argument(&'static str),
    /// Covered by a trait the component implements: `Styled` stands in for
    /// `className`/`xstyle`/`style`, `ParentElement` for `children`.
    Trait(&'static str),
    /// Deliberately not ported, with the reason (a web-only prop, say).
    Waived(&'static str),
    /// Accepted by a builder method whose full behavior lands in a later
    /// milestone, with what is missing.
    Pending(&'static str, &'static str),
}

/// One upstream prop and its Rust counterpart.
#[derive(Clone, Copy, Debug)]
pub struct PropMapping {
    pub prop: &'static str,
    pub rust: RustApi,
}

/// A ported component's Rust documentation.
#[derive(Clone, Copy, Debug)]
pub struct ComponentDoc {
    /// The upstream component name, exactly as in its `.doc.mjs`.
    pub name: &'static str,
    /// The Rust type, such as `astryx::components::Button`.
    pub rust_type: &'static str,
    /// Every upstream prop, in upstream order.
    pub props: &'static [PropMapping],
    /// The source of the component's examples module, which the gallery
    /// renders, so the shown code always compiles.
    pub example: &'static str,
}

/// Every ported component. A component module adds its `DOC` here when it
/// lands; `astryx parity` and the CLI tests read this list.
pub const REGISTRY: &[ComponentDoc] = &[
    crate::text::TEXT_DOC,
    crate::text::HEADING_DOC,
    crate::layout::STACK_DOC,
    crate::layout::CENTER_DOC,
    crate::layout::GRID_DOC,
    crate::layout::DIVIDER_DOC,
    crate::layout::ASPECT_RATIO_DOC,
    crate::layout::VISUALLY_HIDDEN_DOC,
    crate::icon::ICON_DOC,
    crate::status::SPINNER_DOC,
    crate::status::SKELETON_DOC,
    crate::status::BADGE_DOC,
    crate::status::STATUS_DOT_DOC,
    crate::content::CODE_DOC,
    crate::content::KBD_DOC,
    crate::content::BLOCKQUOTE_DOC,
    crate::content::CITATION_DOC,
    crate::people::AVATAR_DOC,
    crate::people::AVATAR_GROUP_DOC,
    crate::containers::CARD_DOC,
    crate::containers::SECTION_DOC,
    crate::containers::EMPTY_STATE_DOC,
    crate::feedback::PROGRESS_BAR_DOC,
    crate::feedback::BANNER_DOC,
    crate::media::THUMBNAIL_DOC,
    crate::time::TIMER_DOC,
    crate::time::TIMESTAMP_DOC,
    crate::button::BUTTON_DOC,
    crate::button::ICON_BUTTON_DOC,
    crate::button::BUTTON_GROUP_DOC,
    crate::button::TOGGLE_BUTTON_DOC,
    crate::button::TOGGLE_BUTTON_GROUP_DOC,
    crate::link::LINK_DOC,
    crate::choice::CHECKBOX_INPUT_DOC,
    crate::choice::SWITCH_DOC,
    crate::field::FIELD_STATUS_DOC,
    crate::indicator::INDICATOR_DOC,
    crate::choice::RADIO_LIST_DOC,
    crate::choice::RADIO_LIST_ITEM_DOC,
    crate::choice::CHECKBOX_LIST_DOC,
    crate::choice::CHECKBOX_LIST_ITEM_DOC,
    crate::field::FIELD_DOC,
    crate::item::ITEM_DOC,
    crate::segmented::SEGMENTED_CONTROL_DOC,
    crate::segmented::SEGMENTED_CONTROL_ITEM_DOC,
    crate::tabs::TAB_LIST_DOC,
    crate::tabs::TAB_DOC,
    crate::tabs::TAB_MENU_DOC,
    crate::toolbar::TOOLBAR_DOC,
    crate::containers::CLICKABLE_CARD_DOC,
    crate::containers::SELECTABLE_CARD_DOC,
    crate::collapsible::COLLAPSIBLE_DOC,
    crate::collapsible::COLLAPSIBLE_GROUP_DOC,
    crate::token::TOKEN_DOC,
    crate::breadcrumbs::BREADCRUMBS_DOC,
    crate::breadcrumbs::BREADCRUMB_ITEM_DOC,
    crate::pagination::PAGINATION_DOC,
    crate::slider::SLIDER_DOC,
    crate::stepper::STEPPER_DOC,
    crate::stepper::STEP_DOC,
    crate::input::TEXT_INPUT_DOC,
    crate::input::TEXT_AREA_DOC,
    crate::input::NUMBER_INPUT_DOC,
    crate::input::INPUT_GROUP_DOC,
    crate::input::INPUT_GROUP_TEXT_DOC,
    crate::field::FORM_LAYOUT_DOC,
    crate::input::FILE_INPUT_DOC,
    crate::overlay::TOOLTIP_DOC,
    crate::overlay::INFO_TIP_DOC,
    crate::overlay::POPOVER_DOC,
    crate::overlay::HOVER_CARD_DOC,
    crate::overlay::DIALOG_DOC,
    crate::overlay::DIALOG_HEADER_DOC,
    crate::overlay::USE_IMPERATIVE_DIALOG_DOC,
    crate::overlay::ALERT_DIALOG_DOC,
    crate::overlay::USE_IMPERATIVE_ALERT_DIALOG_DOC,
    crate::overlay::OVERLAY_DOC,
    crate::overlay::DRAWER_DOC,
    crate::overlay::DRAWER_HEADER_DOC,
    crate::overlay::BOTTOM_SHEET_DOC,
    crate::overlay::BOTTOM_SHEET_SWITCHER_DOC,
    crate::overlay::TOAST_DOC,
    crate::overlay::USE_TOAST_DOC,
    crate::overlay::DROPDOWN_MENU_DOC,
    crate::overlay::DROPDOWN_MENU_ITEM_DOC,
    crate::overlay::DROPDOWN_MENU_CHECKBOX_ITEM_DOC,
    crate::overlay::DROPDOWN_MENU_RADIO_GROUP_DOC,
    crate::overlay::DROPDOWN_MENU_RADIO_ITEM_DOC,
    crate::overlay::DROPDOWN_MENU_GROUP_DOC,
    crate::overlay::DROPDOWN_MENU_DIVIDER_DOC,
    crate::overlay::DROPDOWN_MENU_SUB_MENU_DOC,
    crate::overlay::CONTEXT_MENU_DOC,
    crate::overlay::CONTEXT_MENU_ITEM_DOC,
    crate::overlay::MORE_MENU_DOC,
    crate::overlay::LIGHTBOX_DOC,
    crate::overlay::USE_LIGHTBOX_DOC,
    crate::overlay::COMMAND_PALETTE_DOC,
    crate::overlay::COMMAND_PALETTE_INPUT_DOC,
    crate::overlay::COMMAND_PALETTE_FOOTER_DOC,
    crate::overlay::COMMAND_PALETTE_ITEM_DOC,
    crate::overlay::COMMAND_PALETTE_GROUP_DOC,
    crate::overlay::COMMAND_PALETTE_EMPTY_DOC,
    crate::overlay::COMMAND_PALETTE_LIST_DOC,
    crate::selector::SELECTOR_DOC,
    crate::selector::SELECTOR_OPTION_DOC,
    crate::list::LIST_DOC,
    crate::list::LIST_ITEM_DOC,
    crate::metadata::METADATA_LIST_DOC,
    crate::metadata::METADATA_LIST_ITEM_DOC,
    crate::overflow::OVERFLOW_LIST_DOC,
    crate::typeahead::TYPEAHEAD_DOC,
    crate::typeahead::BASE_TYPEAHEAD_DOC,
    crate::typeahead::TYPEAHEAD_ITEM_DOC,
    crate::tokenizer::TOKENIZER_DOC,
    crate::multi_selector::MULTI_SELECTOR_DOC,
];

/// The registered doc for an upstream component name.
pub fn find(name: &str) -> Option<&'static ComponentDoc> {
    REGISTRY
        .iter()
        .find(|doc| doc.name.eq_ignore_ascii_case(name))
}
