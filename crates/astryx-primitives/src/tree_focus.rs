//! The WAI-ARIA tree keyboard model, upstream `useTreeFocus`: over the
//! visible rows in order, ArrowUp and ArrowDown roam (skipping disabled
//! rows), ArrowRight expands a collapsed row or enters an expanded one,
//! ArrowLeft collapses an expanded row or goes to its parent (both mirrored
//! under right-to-left), Home and End jump, Enter and Space activate. Keep
//! one [`Typeahead`] (reset after 500 ms) for the typed keys.

use std::time::Duration;

use crate::typeahead::Typeahead;

/// Upstream `DEFAULT_TYPEAHEAD_RESET_MS`.
pub const TREE_TYPEAHEAD_RESET: Duration = Duration::from_millis(500);

/// A tree's typeahead with its reset.
pub fn tree_typeahead() -> Typeahead {
    Typeahead::new().reset_after(TREE_TYPEAHEAD_RESET)
}

/// One visible row, as the key model reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// 1 at the top.
    pub level: usize,
    /// `None` for a leaf.
    pub expanded: Option<bool>,
    pub disabled: bool,
}

/// What a key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeKey {
    /// Not a tree key: let it through.
    Ignored,
    /// Handled, nothing to do (a disabled row's Enter, ArrowLeft at the top).
    Handled,
    /// Focus this row (and give it the tab stop).
    Focus(usize),
    /// Expand or collapse this row.
    Toggle(usize),
    /// Enter or Space on this row: run its action; a row without one that
    /// expands toggles.
    Activate(usize),
}

fn enabled_from(rows: &[TreeRow], start: isize, step: isize) -> Option<usize> {
    let mut i = start;
    while i >= 0 && (i as usize) < rows.len() {
        if !rows[i as usize].disabled {
            return Some(i as usize);
        }
        i += step;
    }
    None
}

/// Upstream `handleKeyDown`'s navigation. `key` is GPUI's key name.
pub fn tree_key(rows: &[TreeRow], current: Option<usize>, key: &str, rtl: bool) -> TreeKey {
    if rows.is_empty() {
        return TreeKey::Ignored;
    }
    let key = match (key, rtl) {
        ("left", true) => "right",
        ("right", true) => "left",
        (k, _) => k,
    };
    let focus = |found: Option<usize>| found.map_or(TreeKey::Handled, TreeKey::Focus);
    match key {
        "down" => focus(enabled_from(rows, current.map_or(0, |c| c as isize + 1), 1)),
        "up" => focus(enabled_from(
            rows,
            current.map_or(rows.len() as isize - 1, |c| c as isize - 1),
            -1,
        )),
        "home" => focus(enabled_from(rows, 0, 1)),
        "end" => focus(enabled_from(rows, rows.len() as isize - 1, -1)),
        "right" => {
            let Some(c) = current else {
                return TreeKey::Ignored;
            };
            match rows[c].expanded {
                Some(false) => TreeKey::Toggle(c),
                Some(true) => match rows.get(c + 1) {
                    Some(next) if next.level > rows[c].level => TreeKey::Focus(c + 1),
                    _ => TreeKey::Handled,
                },
                None => TreeKey::Handled,
            }
        }
        "left" => {
            let Some(c) = current else {
                return TreeKey::Ignored;
            };
            if rows[c].expanded == Some(true) {
                return TreeKey::Toggle(c);
            }
            let level = rows[c].level;
            (0..c)
                .rev()
                .find(|i| rows[*i].level < level)
                .map_or(TreeKey::Handled, TreeKey::Focus)
        }
        "enter" | "space" => match current {
            Some(c) if !rows[c].disabled => TreeKey::Activate(c),
            Some(_) => TreeKey::Ignored,
            None => TreeKey::Ignored,
        },
        _ => TreeKey::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<TreeRow> {
        let r = |level, expanded, disabled| TreeRow {
            level,
            expanded,
            disabled,
        };
        vec![
            r(1, Some(true), false),  // 0 Docs
            r(2, None, false),        // 1   Guide
            r(2, Some(false), false), // 2   API
            r(2, None, true),         // 3   Old (disabled)
            r(1, None, false),        // 4 Blog
        ]
    }

    #[test]
    fn arrows_roam_and_skip_disabled_rows() {
        let rows = rows();
        assert_eq!(tree_key(&rows, Some(2), "down", false), TreeKey::Focus(4));
        assert_eq!(tree_key(&rows, Some(4), "up", false), TreeKey::Focus(2));
        assert_eq!(tree_key(&rows, None, "down", false), TreeKey::Focus(0));
        assert_eq!(
            tree_key(&rows, Some(4), "down", false),
            TreeKey::Handled,
            "no wrap"
        );
        assert_eq!(tree_key(&rows, Some(2), "home", false), TreeKey::Focus(0));
        assert_eq!(tree_key(&rows, Some(0), "end", false), TreeKey::Focus(4));
    }

    #[test]
    fn horizontal_arrows_expand_enter_collapse_and_climb() {
        let rows = rows();
        assert_eq!(
            tree_key(&rows, Some(2), "right", false),
            TreeKey::Toggle(2),
            "expands"
        );
        assert_eq!(
            tree_key(&rows, Some(0), "right", false),
            TreeKey::Focus(1),
            "enters"
        );
        assert_eq!(
            tree_key(&rows, Some(0), "left", false),
            TreeKey::Toggle(0),
            "collapses"
        );
        assert_eq!(
            tree_key(&rows, Some(1), "left", false),
            TreeKey::Focus(0),
            "to the parent"
        );
        assert_eq!(tree_key(&rows, Some(4), "left", false), TreeKey::Handled);
        assert_eq!(
            tree_key(&rows, Some(1), "right", true),
            TreeKey::Focus(0),
            "mirrored under RTL"
        );
    }

    #[test]
    fn enter_activates_enabled_rows() {
        let rows = rows();
        assert_eq!(
            tree_key(&rows, Some(1), "enter", false),
            TreeKey::Activate(1)
        );
        assert_eq!(tree_key(&rows, Some(3), "space", false), TreeKey::Ignored);
        assert_eq!(tree_key(&rows, Some(1), "a", false), TreeKey::Ignored);
    }
}
