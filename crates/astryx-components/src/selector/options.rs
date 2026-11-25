//! Selector's options, upstream `Selector/types.ts`, `optionShape.ts`,
//! `selectableOptionEntries.ts` and the option helpers in `Selector.tsx`.
//!
//! An option list holds options, dividers and titled sections. Before
//! anything else reads it (search, keys, rows, value matching), it is
//! flattened to its selectable options with their labels resolved (ORD1).

use std::rc::Rc;

use gpui::{AnyElement, App, SharedString, Window};

use crate::icon::IconSource;

/// Builds an option's secondary control (upstream `action`).
pub type OptionActionFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A selectable option, upstream `SelectorOptionData`.
#[derive(Clone)]
pub struct SelectorOptionData {
    /// Upstream `value`: what `on_change` reports.
    pub value: SharedString,
    /// Upstream `label`, which defaults to the value. Search and typeahead
    /// match it.
    pub label: SharedString,
    /// Upstream `description`: secondary text under the label.
    pub description: Option<SharedString>,
    /// Upstream `disabled`: shown, never picked or highlighted.
    pub disabled: bool,
    /// Upstream `icon`: before the label, in the row and in the trigger.
    pub icon: Option<IconSource>,
    /// Upstream `action`: the option's secondary control. MultiSelector
    /// renders it; Selector shows the option without it.
    pub action: Option<OptionActionFn>,
}

impl SelectorOptionData {
    /// An option whose label is its value.
    pub fn new(value: impl Into<SharedString>) -> Self {
        let value = value.into();
        SelectorOptionData {
            label: value.clone(),
            value,
            description: None,
            disabled: false,
            icon: None,
            action: None,
        }
    }

    /// Upstream `label`.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `disabled`.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `icon`.
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Upstream `action`.
    pub fn action(mut self, f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.action = Some(Rc::new(f));
        self
    }
}

impl std::fmt::Debug for SelectorOptionData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectorOptionData")
            .field("value", &self.value)
            .field("label", &self.label)
            .field("disabled", &self.disabled)
            .finish()
    }
}

impl From<&'static str> for SelectorOptionData {
    fn from(value: &'static str) -> Self {
        SelectorOptionData::new(value)
    }
}

impl From<String> for SelectorOptionData {
    fn from(value: String) -> Self {
        SelectorOptionData::new(value)
    }
}

impl From<SharedString> for SelectorOptionData {
    fn from(value: SharedString) -> Self {
        SelectorOptionData::new(value)
    }
}

/// One entry of an option list, upstream `SelectorOptionType`: an option, a
/// divider or a titled section of options.
#[derive(Clone, Debug)]
pub enum SelectorEntry {
    Option(SelectorOptionData),
    /// Upstream `{type: 'divider'}`.
    Divider,
    /// Upstream `{type: 'section', title, options}`.
    Section {
        title: Option<SharedString>,
        options: Vec<SelectorOptionData>,
    },
}

impl SelectorEntry {
    /// A divider between options.
    pub fn divider() -> Self {
        SelectorEntry::Divider
    }

    /// A section of options under `title`.
    pub fn section(
        title: impl Into<SharedString>,
        options: impl IntoIterator<Item = impl Into<SelectorOptionData>>,
    ) -> Self {
        let title: SharedString = title.into();
        SelectorEntry::Section {
            title: (!title.is_empty()).then_some(title),
            options: options.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<SelectorOptionData> for SelectorEntry {
    fn from(option: SelectorOptionData) -> Self {
        SelectorEntry::Option(option)
    }
}

impl From<&'static str> for SelectorEntry {
    fn from(value: &'static str) -> Self {
        SelectorEntry::Option(value.into())
    }
}

impl From<String> for SelectorEntry {
    fn from(value: String) -> Self {
        SelectorEntry::Option(value.into())
    }
}

impl From<SharedString> for SelectorEntry {
    fn from(value: SharedString) -> Self {
        SelectorEntry::Option(value.into())
    }
}

/// Upstream `getSelectableOptions`: every option, sections flattened and
/// dividers skipped.
pub fn selectable_options(entries: &[SelectorEntry]) -> Vec<SelectorOptionData> {
    let mut out = Vec::new();
    for entry in entries {
        match entry {
            SelectorEntry::Option(option) => out.push(option.clone()),
            SelectorEntry::Section { options, .. } => out.extend(options.iter().cloned()),
            SelectorEntry::Divider => {}
        }
    }
    out
}

/// Upstream `optionMatchesQuery`: the label contains the query, ignoring
/// case. An empty query matches everything.
pub(crate) fn matches_query(option: &SelectorOptionData, query: &str) -> bool {
    query.is_empty() || option.label.to_lowercase().contains(&query.to_lowercase())
}

/// Upstream `filterOptionsByQuery`.
pub(crate) fn filter_by_query(
    items: &[SelectorOptionData],
    query: &str,
) -> Vec<SelectorOptionData> {
    if query.is_empty() {
        return items.to_vec();
    }
    items
        .iter()
        .filter(|item| matches_query(item, query))
        .cloned()
        .collect()
}

/// What the highlight follows: each option's value and whether it is
/// enabled (upstream `optionSignature`). New option objects with the same
/// values and states give the same signature.
pub(crate) type Signature = Vec<(SharedString, bool)>;

pub(crate) fn signature(items: &[SelectorOptionData]) -> Signature {
    items
        .iter()
        .map(|item| (item.value.clone(), item.disabled))
        .collect()
}

/// Upstream `followHighlight` (FR10): the index in `next` of the option
/// highlighted at `index` in `previous`. When that option is gone or
/// disabled, the next remaining enabled option after it; with none after
/// it, the previous one. `None` when none remains.
pub(crate) fn follow_highlight(
    previous: &Signature,
    next: &Signature,
    index: usize,
) -> Option<usize> {
    let enabled_index_of = |value: Option<&SharedString>| {
        value.and_then(|value| {
            next.iter()
                .position(|(v, disabled)| v == value && !*disabled)
        })
    };
    if let Some(stayed) = enabled_index_of(previous.get(index).map(|(v, _)| v)) {
        return Some(stayed);
    }
    for after in index + 1..previous.len() {
        if let Some(found) = enabled_index_of(previous.get(after).map(|(v, _)| v)) {
            return Some(found);
        }
    }
    // A highlight past the end of its list (a hover after it shrank) starts
    // from the list's own last option.
    (0..index.min(previous.len()))
        .rev()
        .find_map(|before| enabled_index_of(previous.get(before).map(|(v, _)| v)))
}

/// Upstream `enabledEdgeIndex`: the first (or last) enabled option.
pub(crate) fn enabled_edge(items: &[SelectorOptionData], from_end: bool) -> Option<usize> {
    if from_end {
        items.iter().rposition(|item| !item.disabled)
    } else {
        items.iter().position(|item| !item.disabled)
    }
}

/// Upstream `findOpeningIndex` (FR8): the selected option when it is
/// enabled, else the first (or, for ArrowUp, the last) enabled option.
/// Opening never lands on a disabled option.
pub(crate) fn opening_index(
    items: &[SelectorOptionData],
    value: Option<&SharedString>,
    from_end: bool,
) -> Option<usize> {
    if let Some(selected) = value.and_then(|v| items.iter().position(|item| &item.value == v))
        && !items[selected].disabled
    {
        return Some(selected);
    }
    enabled_edge(items, from_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(items: &[(&'static str, bool)]) -> Signature {
        items.iter().map(|(v, d)| ((*v).into(), *d)).collect()
    }

    #[test]
    fn options_normalize_and_flatten() {
        let entries = vec![
            SelectorEntry::from("Apple"),
            SelectorEntry::divider(),
            SelectorEntry::section(
                "Citrus",
                [
                    SelectorOptionData::new("lemon").label("Lemon"),
                    SelectorOptionData::new("lime"),
                ],
            ),
        ];
        let flat = selectable_options(&entries);
        let labels: Vec<_> = flat.iter().map(|o| o.label.to_string()).collect();
        assert_eq!(
            labels,
            ["Apple", "Lemon", "lime"],
            "a label defaults to the value"
        );
        assert_eq!(flat[0].value, "Apple");
    }

    #[test]
    fn search_matches_labels_ignoring_case() {
        let items = selectable_options(&["Apple".into(), "Pineapple".into(), "Cherry".into()]);
        let found: Vec<_> = filter_by_query(&items, "APP")
            .into_iter()
            .map(|o| o.value)
            .collect();
        assert_eq!(found, ["Apple", "Pineapple"]);
        assert_eq!(filter_by_query(&items, "").len(), 3);
    }

    // FR8: the opening highlight.
    #[test]
    fn opening_lands_on_the_enabled_selection_or_an_enabled_edge() {
        let items = vec![
            SelectorOptionData::new("a").disabled(true),
            SelectorOptionData::new("b"),
            SelectorOptionData::new("c"),
            SelectorOptionData::new("d").disabled(true),
        ];
        let c: SharedString = "c".into();
        let a: SharedString = "a".into();
        assert_eq!(opening_index(&items, Some(&c), false), Some(2));
        assert_eq!(opening_index(&items, None, false), Some(1), "first enabled");
        assert_eq!(
            opening_index(&items, None, true),
            Some(2),
            "ArrowUp: last enabled"
        );
        assert_eq!(
            opening_index(&items, Some(&a), false),
            Some(1),
            "a disabled selection is skipped"
        );
        let none = vec![SelectorOptionData::new("x").disabled(true)];
        assert_eq!(opening_index(&none, None, false), None);
    }

    // FR10: the highlight follows its option by value.
    #[test]
    fn the_highlight_follows_its_option() {
        let before = sig(&[("a", false), ("b", false), ("c", false)]);
        // Reordered: b stays highlighted.
        let reordered = sig(&[("c", false), ("b", false), ("a", false)]);
        assert_eq!(follow_highlight(&before, &reordered, 1), Some(1));
        let moved = sig(&[("b", false), ("a", false), ("c", false)]);
        assert_eq!(follow_highlight(&before, &moved, 1), Some(0));
        // Removed: the next one after it follows.
        let removed = sig(&[("a", false), ("c", false)]);
        assert_eq!(follow_highlight(&before, &removed, 1), Some(1));
        // The last removed: the previous one follows.
        let last_removed = sig(&[("a", false), ("b", false)]);
        assert_eq!(follow_highlight(&before, &last_removed, 2), Some(1));
        // A disabled option in the way is skipped.
        let disabled = sig(&[("a", false), ("b", true), ("c", true)]);
        assert_eq!(follow_highlight(&before, &disabled, 1), Some(0));
        // No old option left: nothing.
        let fresh = sig(&[("x", false)]);
        assert_eq!(follow_highlight(&before, &fresh, 1), None);
        // A highlight past the end starts from the last option.
        assert_eq!(follow_highlight(&before, &removed, 7), Some(1));
    }
}
