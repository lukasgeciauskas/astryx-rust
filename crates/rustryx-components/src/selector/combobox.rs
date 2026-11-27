//! The combobox keyboard model, upstream `useCombobox` in
//! `Selector/hooks.ts`, as pure logic: what a key does to an open or closed
//! list. Selector and MultiSelector apply the outcome.
//!
//! - Closed: ArrowDown, Enter and Space open on the selected option or the
//!   first enabled one, ArrowUp on the last enabled one (FR8); a search
//!   list opens with nothing highlighted. Delete and Backspace clear a
//!   clearable value. With search, a printable key opens the list and
//!   seeds the query.
//! - Open: the arrows move among enabled options, stopping at the ends;
//!   Home, End, PageUp and PageDown jump to the first or last; Enter (and
//!   Space without search) picks the highlighted option; Escape and Tab
//!   close.

/// What a key asks of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KeyOutcome {
    /// Not the model's key: it reaches the caller's handler.
    Ignored,
    /// The model's key, with nothing to change (its default is prevented).
    Handled,
    /// Open the list, highlighting this option.
    Open(Option<usize>),
    /// Move the highlight.
    Highlight(Option<usize>),
    /// Pick the option at this index.
    Select(usize),
    /// Close the list (Escape).
    Close,
    /// Close the list and let the key through (Tab).
    CloseAndPass,
    /// Clear the value (Delete and Backspace on a closed clearable list).
    Clear,
    /// Open the list if needed and add the text to the search query.
    Seed(String),
}

/// What the model reads from the list.
pub(crate) struct Combobox<'a> {
    /// Each option's disabled state, in list order.
    pub disabled: &'a [bool],
    /// The opening index for ArrowDown (and every other opening) and for
    /// ArrowUp (upstream `findOpeningIndex`).
    pub opening: (Option<usize>, Option<usize>),
    pub open: bool,
    pub search: bool,
    pub highlight: Option<usize>,
    /// A clear is possible: `hasClear` and a value.
    pub clearable: bool,
}

impl Combobox<'_> {
    fn enabled(&self) -> Vec<usize> {
        self.disabled
            .iter()
            .enumerate()
            .filter(|(_, d)| !**d)
            .map(|(i, _)| i)
            .collect()
    }

    fn open_at(&self, from_end: bool) -> KeyOutcome {
        if self.search {
            KeyOutcome::Open(None)
        } else if from_end {
            KeyOutcome::Open(self.opening.1)
        } else {
            KeyOutcome::Open(self.opening.0)
        }
    }

    /// Upstream `onKeyDown`. `text` is the key's typed character, if any;
    /// `chord` whether a shortcut modifier is held.
    pub fn key(&self, key: &str, text: Option<&str>, chord: bool) -> KeyOutcome {
        let enabled = self.enabled();
        let position = self
            .highlight
            .and_then(|h| enabled.iter().position(|&i| i == h));
        match key {
            "down" => {
                if !self.open {
                    return self.open_at(false);
                }
                let next = position
                    .map_or(0, |p| p + 1)
                    .min(enabled.len().saturating_sub(1));
                KeyOutcome::Highlight(enabled.get(next).copied().or(self.highlight))
            }
            "up" => {
                if !self.open {
                    return self.open_at(true);
                }
                let prev = position.map_or(0, |p| p.saturating_sub(1));
                KeyOutcome::Highlight(enabled.get(prev).copied().or(self.highlight))
            }
            "space" if self.search => KeyOutcome::Ignored,
            "enter" | "space" => {
                if self.open {
                    match self.highlight {
                        Some(h) if self.disabled.get(h) == Some(&false) => KeyOutcome::Select(h),
                        _ => KeyOutcome::Handled,
                    }
                } else {
                    self.open_at(false)
                }
            }
            "escape" if self.open => KeyOutcome::Close,
            "tab" if self.open => KeyOutcome::CloseAndPass,
            "home" | "pageup" => match (self.open, enabled.first()) {
                (true, Some(&first)) => KeyOutcome::Highlight(Some(first)),
                _ => KeyOutcome::Handled,
            },
            "end" | "pagedown" => match (self.open, enabled.last()) {
                (true, Some(&last)) => KeyOutcome::Highlight(Some(last)),
                _ => KeyOutcome::Handled,
            },
            "delete" | "backspace" if !self.search && !self.open && self.clearable => {
                KeyOutcome::Clear
            }
            _ => match text {
                Some(text) if self.search && !chord && text.chars().count() == 1 && text != " " => {
                    KeyOutcome::Seed(text.to_string())
                }
                _ => KeyOutcome::Ignored,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(disabled: &[bool], open: bool, highlight: Option<usize>) -> Combobox<'_> {
        Combobox {
            disabled,
            opening: (Some(1), Some(2)),
            open,
            search: false,
            highlight,
            clearable: false,
        }
    }

    #[test]
    fn closed_keys_open_on_the_opening_option() {
        let d = [true, false, false, true];
        let m = model(&d, false, None);
        assert_eq!(m.key("down", None, false), KeyOutcome::Open(Some(1)));
        assert_eq!(m.key("up", None, false), KeyOutcome::Open(Some(2)));
        assert_eq!(m.key("enter", None, false), KeyOutcome::Open(Some(1)));
        assert_eq!(m.key("space", Some(" "), false), KeyOutcome::Open(Some(1)));
        assert_eq!(m.key("escape", None, false), KeyOutcome::Ignored);
        assert_eq!(m.key("tab", None, false), KeyOutcome::Ignored);
    }

    #[test]
    fn open_arrows_stop_at_the_enabled_ends() {
        let d = [true, false, false, true];
        let m = model(&d, true, Some(2));
        assert_eq!(m.key("down", None, false), KeyOutcome::Highlight(Some(2)));
        assert_eq!(m.key("up", None, false), KeyOutcome::Highlight(Some(1)));
        let m = model(&d, true, Some(1));
        assert_eq!(m.key("up", None, false), KeyOutcome::Highlight(Some(1)));
        // Nothing highlighted: both arrows start at the first enabled option.
        let m = model(&d, true, None);
        assert_eq!(m.key("down", None, false), KeyOutcome::Highlight(Some(1)));
        assert_eq!(m.key("up", None, false), KeyOutcome::Highlight(Some(1)));
        assert_eq!(m.key("end", None, false), KeyOutcome::Highlight(Some(2)));
        assert_eq!(m.key("pageup", None, false), KeyOutcome::Highlight(Some(1)));
    }

    #[test]
    fn enter_picks_only_an_enabled_highlight() {
        let d = [false, true];
        assert_eq!(
            model(&d, true, Some(0)).key("enter", None, false),
            KeyOutcome::Select(0)
        );
        assert_eq!(
            model(&d, true, Some(1)).key("enter", None, false),
            KeyOutcome::Handled
        );
        assert_eq!(
            model(&d, true, None).key("enter", None, false),
            KeyOutcome::Handled
        );
        assert_eq!(
            model(&d, true, Some(0)).key("escape", None, false),
            KeyOutcome::Close
        );
        assert_eq!(
            model(&d, true, None).key("tab", None, false),
            KeyOutcome::CloseAndPass
        );
    }

    #[test]
    fn search_lists_open_unhighlighted_and_seed_the_query() {
        let d = [false, false];
        let mut m = model(&d, false, None);
        m.search = true;
        assert_eq!(m.key("down", None, false), KeyOutcome::Open(None), "FR8");
        assert_eq!(m.key("space", Some(" "), false), KeyOutcome::Ignored);
        assert_eq!(m.key("a", Some("a"), false), KeyOutcome::Seed("a".into()));
        assert_eq!(
            m.key("a", Some("a"), true),
            KeyOutcome::Ignored,
            "a shortcut"
        );
        m.clearable = true;
        assert_eq!(m.key("backspace", None, false), KeyOutcome::Ignored);
    }

    #[test]
    fn delete_clears_a_closed_clearable_value() {
        let d = [false];
        let mut m = model(&d, false, None);
        assert_eq!(m.key("delete", None, false), KeyOutcome::Ignored);
        m.clearable = true;
        assert_eq!(m.key("delete", None, false), KeyOutcome::Clear);
        assert_eq!(m.key("backspace", None, false), KeyOutcome::Clear);
        m.open = true;
        assert_eq!(m.key("backspace", None, false), KeyOutcome::Ignored);
    }
}
