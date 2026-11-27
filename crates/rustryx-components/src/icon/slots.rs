//! A theme's component icons (upstream `ComponentIcon` and
//! `defineTheme({componentIcons})`): a component's themeable icon, named by
//! why it is there (`table-sort-indicator`), which a theme may draw with
//! another icon, or hide, for each of the component's states.
//!
//! ```ignore
//! set_component_icon(TABLE_SORT_INDICATOR, |state| match state {
//!     "none" => ComponentIconChoice::Hidden,
//!     _ => ComponentIconChoice::Fallback,
//! }, cx);
//! ```

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{App, Global};

use super::IconSource;

/// A sortable Table column's indicator; states `ascending`, `descending`
/// and `none`.
pub const TABLE_SORT_INDICATOR: &str = "table-sort-indicator";

/// Row expansion's chevron; states `expanded` and `collapsed`.
pub const TABLE_ROW_EXPAND: &str = "table-row-expand";

/// What a theme draws for a slot in a state.
#[derive(Clone, Debug, PartialEq)]
pub enum ComponentIconChoice {
    /// The component's own icon.
    Fallback,
    /// Another icon.
    Icon(IconSource),
    /// Nothing.
    Hidden,
}

type SlotFn = Rc<dyn Fn(&str) -> ComponentIconChoice>;

#[derive(Default)]
struct ComponentIcons(HashMap<&'static str, SlotFn>);

impl Global for ComponentIcons {}

/// Gives `slot` the theme's choice, by the component's state.
pub fn set_component_icon(
    slot: &'static str,
    choose: impl Fn(&str) -> ComponentIconChoice + 'static,
    cx: &mut App,
) {
    cx.default_global::<ComponentIcons>()
        .0
        .insert(slot, Rc::new(choose));
    cx.refresh_windows();
}

/// Removes the theme's choice for `slot`.
pub fn clear_component_icon(slot: &'static str, cx: &mut App) {
    if cx
        .try_global::<ComponentIcons>()
        .is_some_and(|i| i.0.contains_key(slot))
    {
        cx.global_mut::<ComponentIcons>().0.remove(slot);
        cx.refresh_windows();
    }
}

/// The icon a component draws in `slot` and `state`: the theme's, else its
/// `fallback`; `None` when the theme hides it (upstream `useComponentIcon`).
pub(crate) fn component_icon(
    slot: &str,
    state: &str,
    fallback: IconSource,
    cx: &App,
) -> Option<IconSource> {
    let choice = cx
        .try_global::<ComponentIcons>()
        .and_then(|i| i.0.get(slot).cloned())
        .map_or(ComponentIconChoice::Fallback, |f| f(state));
    match choice {
        ComponentIconChoice::Fallback => Some(fallback),
        ComponentIconChoice::Icon(icon) => Some(icon),
        ComponentIconChoice::Hidden => None,
    }
}
