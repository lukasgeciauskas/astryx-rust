//! Tokenizer (M6). Upstream: `Tokenizer/`.
//!
//! - The inline field: the selected items as Tokens, then the search field
//!   (BaseTypeahead), one group named by the label. Results leave out what is
//!   chosen (FR13); a pick from the focus list keeps it open on the next
//!   choice (FR14); `create` offers a "Create" row for unmatched text.
//! - Backspace in the empty field removes the last token; a token's remove
//!   button removes it; Clear all empties the field. Each change is
//!   announced.
//! - At `max_entries` the field collapses but stays mounted (FR1).
//! - `token_overflow`: blurred, the tokens fold into one row with a count
//!   (`UnfocusedInline`), or the field folds into a one-row placeholder
//!   and opens over it on focus (`UnfocusedLayer`, FR3).

pub mod examples;
mod field;
mod sheet;

#[cfg(test)]
mod tests;

pub use field::{
    TokenOverflowBehavior, Tokenizer, TokenizerChange, TokenizerChangeKind, TokenizerController,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const TOKENIZER_DOC: ComponentDoc = ComponentDoc {
    name: "Tokenizer",
    rust_type: "rustryx::components::tokenizer::Tokenizer",
    props: &[
        p("label", RustApi::Argument("label")),
        p(
            "searchSource",
            RustApi::Argument("source (Tokenizer::new, or from_rc; source_key)"),
        ),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("placeholder", RustApi::Method("placeholder")),
        p("maxEntries", RustApi::Method("max_entries")),
        p("hasClear", RustApi::Method("clear")),
        p("renderToken", RustApi::Method("render_token")),
        p("renderItem", RustApi::Method("render_item")),
        p("isDisabled", RustApi::Method("disabled")),
        p(
            "htmlName",
            RustApi::Waived("GPUI apps have no HTML forms; read the value from your state"),
        ),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("status", RustApi::Method("status")),
        p("statusVariant", RustApi::Method("status_placement")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isRequired", RustApi::Method("required")),
        p("isOptional", RustApi::Method("optional")),
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("hasEntriesOnFocus", RustApi::Method("entries_on_focus")),
        p("maxMenuItems", RustApi::Method("max_menu_items")),
        p("menuWidth", RustApi::Method("menu_width")),
        p("minQueryLength", RustApi::Method("min_query_length")),
        p("emptySearchText", RustApi::Method("empty_search_text")),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("size", RustApi::Method("size")),
        p("debounceMs", RustApi::Method("debounce")),
        p(
            "hasCreate",
            RustApi::Method("create (makes the item from the typed text)"),
        ),
        p("onChangeQuery", RustApi::Method("on_change_query")),
        p("startIcon", RustApi::Method("start_icon")),
        p("endContent", RustApi::Method("end_content")),
        p(
            "handleRef",
            RustApi::Method("controller (a TokenizerController)"),
        ),
        p("menu", RustApi::Method("menu")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("width", RustApi::Method("width")),
        p("tokenOverflowBehavior", RustApi::Method("token_overflow")),
        p("onFocus", RustApi::Method("on_focus")),
        p("onBlur", RustApi::Method("on_blur")),
        p("containerXstyle", RustApi::Trait("Styled")),
        p("containerClassName", RustApi::Trait("Styled")),
        p("containerStyle", RustApi::Trait("Styled")),
        p("controlXstyle", RustApi::Method("control_style")),
        p("controlClassName", RustApi::Method("control_style")),
        p("controlStyle", RustApi::Method("control_style")),
        p("presentation", RustApi::Method("presentation")),
    ],
    example: include_str!("examples.rs"),
};
