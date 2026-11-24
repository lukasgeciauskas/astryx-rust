//! Type-to-select in lists and menus, upstream `useTypeahead` and
//! `utils/typeaheadMatch`.
//!
//! Typing jumps to the next item whose label starts with what was typed.
//! The typed text resets after a pause (750 ms by default). Repeating one
//! letter cycles through the items starting with it. Matching ignores case
//! and accents, unless the typed text itself has an accent, which is then
//! tried exactly first.

use std::time::Duration;

use gpui::Keystroke;
use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};
use web_time::Instant;

/// Upstream `resetMs` default.
pub const TYPEAHEAD_RESET: Duration = Duration::from_millis(750);

fn is_combining_mark(c: char) -> bool {
    ('\u{0300}'..='\u{036f}').contains(&c)
}

fn stroked(c: char) -> Option<char> {
    Some(match c {
        'đ' | 'Đ' => 'd',
        'ł' | 'Ł' => 'l',
        'ø' | 'Ø' => 'o',
        'ħ' | 'Ħ' => 'h',
        'ŧ' | 'Ŧ' => 't',
        'ƀ' | 'Ƀ' => 'b',
        'ı' => 'i',
        _ => return None,
    })
}

/// Upstream `foldForTypeahead`: no accents or strokes, lower case.
pub fn fold(text: &str) -> String {
    let nfd = DecomposingNormalizerBorrowed::new_nfd().normalize(text);
    let stripped: String = nfd
        .chars()
        .filter(|c| !is_combining_mark(*c))
        .map(|c| stroked(c).unwrap_or(c))
        .collect();
    ComposingNormalizerBorrowed::new_nfc()
        .normalize(&stripped)
        .to_lowercase()
}

/// Upstream `exactFold`: case folded the Turkish-safe way, accents kept.
fn exact_fold(text: &str) -> String {
    ComposingNormalizerBorrowed::new_nfc()
        .normalize(text)
        .replace('I', "ı")
        .replace('İ', "i")
        .to_lowercase()
}

fn has_mark(typed: &str) -> bool {
    typed.chars().any(|c| {
        matches!(
            c,
            'đ' | 'Đ' | 'ł' | 'Ł' | 'ø' | 'Ø' | 'ħ' | 'Ħ' | 'ŧ' | 'Ŧ' | 'ƀ' | 'Ƀ' | 'ı' | 'İ'
        )
    }) || DecomposingNormalizerBorrowed::new_nfd()
        .normalize(typed)
        .chars()
        .any(is_combining_mark)
}

/// Upstream `nextTypeaheadBuffer`: the typed text after `key`, or `None`
/// when `key` can't start a search. Repeating one character keeps the
/// buffer at that character, so it cycles.
pub fn next_buffer(buffer: &str, key: &str) -> Option<String> {
    if buffer.is_empty() && key != " " && fold(key).is_empty() {
        return None;
    }
    let folded_key = fold(key);
    let repeat = !buffer.is_empty()
        && buffer
            .chars()
            .all(|c| fold(c.encode_utf8(&mut [0; 4])) == folded_key);
    Some(if repeat {
        key.to_string()
    } else {
        format!("{buffer}{key}")
    })
}

/// Upstream `typeaheadPrefixMatch`.
pub fn prefix_match(label: &str, typed: &str, exact: bool) -> bool {
    let label = label.trim();
    if exact {
        return exact_fold(label).starts_with(&exact_fold(typed));
    }
    let typed = fold(typed);
    !typed.is_empty() && fold(label).starts_with(&typed)
}

/// Upstream `findTypeaheadMatch`: the first index in `order` whose label
/// matches, trying an exact match first when the typed text has an accent.
pub fn find_match<'a>(
    order: impl IntoIterator<Item = usize> + Clone,
    label: impl Fn(usize) -> Option<&'a str>,
    typed: &str,
) -> Option<usize> {
    let passes: &[bool] = if has_mark(typed) {
        &[true, false]
    } else {
        &[false]
    };
    passes.iter().find_map(|&exact| {
        order
            .clone()
            .into_iter()
            .find(|&i| label(i).is_some_and(|l| prefix_match(l, typed, exact)))
    })
}

/// Upstream `isShortcutChord`: Ctrl (but not AltGr, which Windows reports
/// as Ctrl+Alt) or the platform key.
pub fn is_shortcut_chord(keystroke: &Keystroke) -> bool {
    let m = &keystroke.modifiers;
    m.platform || (m.control && !m.alt)
}

/// A list's typeahead state, upstream `useTypeahead`. Keep one per list,
/// in `window.use_keyed_state`, and feed it the list's key-downs.
#[derive(Clone, Debug)]
pub struct Typeahead {
    buffer: String,
    last_key: Option<Instant>,
    reset_after: Duration,
}

impl Default for Typeahead {
    fn default() -> Self {
        Self::new()
    }
}

impl Typeahead {
    pub fn new() -> Self {
        Typeahead {
            buffer: String::new(),
            last_key: None,
            reset_after: TYPEAHEAD_RESET,
        }
    }

    /// Upstream `resetMs`: the pause that clears the typed text.
    pub fn reset_after(mut self, pause: Duration) -> Self {
        self.reset_after = pause;
        self
    }

    /// Upstream `reset`: forgets the typed text.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.last_key = None;
    }

    /// The typed text so far.
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// Feeds a key-down. Returns `None` when the key is not typeahead input
    /// (let it through), else `Some` of the matching index, if any (the key
    /// is consumed either way).
    ///
    /// `current` is the focused or active item; a single typed character
    /// starts after it, so repeating a letter moves on. `disabled` items are
    /// skipped.
    pub fn key<'a>(
        &mut self,
        keystroke: &Keystroke,
        count: usize,
        label: impl Fn(usize) -> Option<&'a str>,
        current: Option<usize>,
        disabled: impl Fn(usize) -> bool,
    ) -> Option<Option<usize>> {
        self.key_at(keystroke, count, label, current, disabled, Instant::now())
    }

    pub(crate) fn key_at<'a>(
        &mut self,
        keystroke: &Keystroke,
        count: usize,
        label: impl Fn(usize) -> Option<&'a str>,
        current: Option<usize>,
        disabled: impl Fn(usize) -> bool,
        now: Instant,
    ) -> Option<Option<usize>> {
        if self
            .last_key
            .is_some_and(|last| now.duration_since(last) >= self.reset_after)
        {
            self.buffer.clear();
        }
        if is_shortcut_chord(keystroke) {
            return None;
        }
        let typed = if keystroke.key == "space" {
            // Space continues a search ("New York"); alone it activates.
            if self.buffer.is_empty() {
                return None;
            }
            " ".to_string()
        } else {
            let text = keystroke
                .key_char
                .clone()
                .unwrap_or_else(|| keystroke.key.clone());
            if text.chars().count() != 1 || text == " " {
                return None;
            }
            text
        };
        let next = next_buffer(&self.buffer, &typed)?;
        if count == 0 {
            return None;
        }
        self.buffer = next;
        self.last_key = Some(now);

        let start = current.unwrap_or(0);
        let offset = usize::from(current.is_some() && self.buffer.chars().count() == 1);
        let order: Vec<usize> = (0..count)
            .map(|i| (start + offset + i) % count)
            .filter(|&i| !disabled(i))
            .collect();
        Some(find_match(order.iter().copied(), label, &self.buffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: &'static str) -> Keystroke {
        let Ok(keystroke) = Keystroke::parse(k) else {
            panic!("not a keystroke: {k}")
        };
        keystroke
    }

    const FRUIT: [&str; 5] = ["Apple", "Apricot", "Banana", "Blueberry", "Cherry"];

    fn feed(
        t: &mut Typeahead,
        k: &'static str,
        current: Option<usize>,
        now: Instant,
    ) -> Option<Option<usize>> {
        t.key_at(
            &key(k),
            FRUIT.len(),
            |i| Some(FRUIT[i]),
            current,
            |_| false,
            now,
        )
    }

    #[test]
    fn folding_drops_case_accents_and_strokes() {
        assert_eq!(fold("Ångström"), "angstrom");
        assert_eq!(fold("Łódź"), "lodz");
        assert_eq!(fold("Øre"), "ore");
    }

    #[test]
    fn typing_builds_a_prefix() {
        let mut t = Typeahead::new();
        let now = Instant::now();
        assert_eq!(feed(&mut t, "a", None, now), Some(Some(0)));
        assert_eq!(feed(&mut t, "p", Some(0), now), Some(Some(0)));
        assert_eq!(feed(&mut t, "r", Some(0), now), Some(Some(1)));
    }

    #[test]
    fn repeating_a_letter_cycles() {
        let mut t = Typeahead::new();
        let now = Instant::now();
        assert_eq!(feed(&mut t, "b", Some(0), now), Some(Some(2)));
        assert_eq!(feed(&mut t, "b", Some(2), now), Some(Some(3)));
        assert_eq!(feed(&mut t, "b", Some(3), now), Some(Some(2)));
    }

    #[test]
    fn a_pause_resets_the_typed_text() {
        let mut t = Typeahead::new();
        let now = Instant::now();
        feed(&mut t, "c", None, now);
        let later = now + TYPEAHEAD_RESET;
        assert_eq!(feed(&mut t, "a", Some(4), later), Some(Some(0)));
        assert_eq!(t.buffer(), "a");
    }

    #[test]
    fn space_only_continues_a_search_and_chords_pass() {
        let mut t = Typeahead::new();
        let now = Instant::now();
        assert_eq!(feed(&mut t, "space", None, now), None);
        assert_eq!(feed(&mut t, "ctrl-a", None, now), None);
        assert_eq!(feed(&mut t, "down", None, now), None);
    }

    #[test]
    fn disabled_items_are_skipped() {
        let mut t = Typeahead::new();
        let found = t.key_at(
            &key("a"),
            FRUIT.len(),
            |i| Some(FRUIT[i]),
            None,
            |i| i == 0,
            Instant::now(),
        );
        assert_eq!(found, Some(Some(1)));
    }

    #[test]
    fn accented_input_prefers_an_exact_match() {
        let labels = ["Ecole", "École"];
        assert_eq!(find_match(0..2, |i| Some(labels[i]), "é"), Some(1));
        assert_eq!(find_match(0..2, |i| Some(labels[i]), "e"), Some(0));
    }
}
