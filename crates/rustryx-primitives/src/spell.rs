//! Spell checking for text fields: upstream `hasSpellCheck`, which turns the
//! browser's checker on or off.
//!
//! A field asks the installed [`SpellChecker`] for its misspelled words and
//! underlines them with a wavy line, as browsers do. Until an app installs one
//! ([`set_spell_checker`]), the platform's own is used:
//! - Windows: the system checker (`ISpellChecker`), in the user's language,
//!   with the words they added to its dictionary;
//! - macOS: the system checker (`NSSpellChecker`), as Safari and Chrome use
//!   there, in the languages the user chose in System Settings, with the
//!   words they taught it;
//! - Linux: the Hunspell dictionary for `$LANG` from the usual system
//!   folders, read by [`DictionaryChecker`];
//! - the web: none, since browsers do not expose theirs; an app installs a
//!   [`DictionaryChecker`] built from dictionary files it ships or fetches.
//!
//! [`DictionaryChecker`] reads Hunspell `.aff`/`.dic` dictionaries through
//! `spellbook` (MPL-2.0).

use std::cell::RefCell;
use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use gpui::{App, Global};
use unicode_segmentation::UnicodeSegmentation;

/// A spell checker.
pub trait SpellChecker: 'static {
    /// The byte ranges of `text`'s misspelled words.
    fn check(&self, text: &str) -> Vec<Range<usize>>;
    /// Corrections for a misspelled word, best first.
    fn suggest(&self, word: &str) -> Vec<String>;
    /// Accepts `word` from now on: a context menu's "Add to dictionary".
    fn add(&self, word: &str);
}

/// The installed checker, once looked for.
struct Registry(Option<Rc<dyn SpellChecker>>);

impl Global for Registry {}

/// Installs `checker` for every field; `None` turns spell checking off.
pub fn set_spell_checker(checker: Option<Rc<dyn SpellChecker>>, cx: &mut App) {
    cx.set_global(Registry(checker));
}

/// The installed checker; the platform's on first use (see the module docs).
pub fn spell_checker(cx: &mut App) -> Option<Rc<dyn SpellChecker>> {
    if let Some(Registry(checker)) = cx.try_global::<Registry>() {
        return checker.clone();
    }
    let checker = platform_checker();
    cx.set_global(Registry(checker.clone()));
    checker
}

#[cfg(windows)]
fn platform_checker() -> Option<Rc<dyn SpellChecker>> {
    windows_checker::SystemChecker::for_user().map(|c| Rc::new(c) as Rc<dyn SpellChecker>)
}

#[cfg(target_os = "macos")]
fn platform_checker() -> Option<Rc<dyn SpellChecker>> {
    mac_checker::SystemChecker::new().map(|c| Rc::new(c) as Rc<dyn SpellChecker>)
}

#[cfg(all(unix, not(target_os = "macos"), not(target_family = "wasm")))]
fn platform_checker() -> Option<Rc<dyn SpellChecker>> {
    DictionaryChecker::from_system().map(|c| Rc::new(c) as Rc<dyn SpellChecker>)
}

#[cfg(not(any(windows, all(unix, not(target_family = "wasm")))))]
fn platform_checker() -> Option<Rc<dyn SpellChecker>> {
    None
}

/// The words of `text` a checker should look at, with their byte ranges:
/// runs of letters (with inner apostrophes and hyphens), skipping anything
/// with a digit, and links and addresses.
pub fn words(text: &str) -> impl Iterator<Item = (Range<usize>, &str)> {
    text.split_word_bound_indices()
        .filter_map(move |(start, word)| {
            let word = word.trim_matches(|c: char| c == '\'' || c == '\u{2019}');
            if word.is_empty()
                || !word.chars().any(char::is_alphabetic)
                || word.chars().any(|c| c.is_numeric())
            {
                return None;
            }
            let offset = text[start..].find(word).map_or(start, |i| start + i);
            let range = offset..offset + word.len();
            // Inside a link or an address.
            let token_start = text[..range.start]
                .rfind(char::is_whitespace)
                .map_or(0, |i| i + 1);
            let token_end = text[range.end..]
                .find(char::is_whitespace)
                .map_or(text.len(), |i| range.end + i);
            let token = &text[token_start..token_end];
            if token.contains("://") || token.contains('@') || token.starts_with("www.") {
                return None;
            }
            Some((range, word))
        })
}

/// A checker over a Hunspell dictionary.
pub struct DictionaryChecker {
    dictionary: spellbook::Dictionary,
    added: RefCell<HashSet<String>>,
}

impl DictionaryChecker {
    /// From a dictionary's `.aff` and `.dic` files.
    pub fn new(aff: &str, dic: &str) -> Option<Self> {
        Some(DictionaryChecker {
            dictionary: spellbook::Dictionary::new(aff, dic).ok()?,
            added: RefCell::default(),
        })
    }

    /// The system's dictionary for the user's language (`$LC_ALL`,
    /// `$LC_MESSAGES` or `$LANG`, else `en_US`), from the folders Hunspell
    /// reads.
    #[cfg(all(unix, not(target_os = "macos"), not(target_family = "wasm")))]
    pub fn from_system() -> Option<Self> {
        let lang = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|var| std::env::var(var).ok())
            .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
            .unwrap_or_else(|| "en_US".into());
        let lang = lang.split(['.', '@']).next().unwrap_or("en_US").to_string();
        let short = lang.split('_').next().unwrap_or("en").to_string();
        let folders = [
            "/usr/share/hunspell",
            "/usr/share/myspell",
            "/usr/share/myspell/dicts",
            "/usr/local/share/hunspell",
            "/usr/share/hunspell-dictionaries",
        ];
        for name in [lang.as_str(), short.as_str(), "en_US"] {
            for folder in folders {
                let base = std::path::Path::new(folder).join(name);
                let (Ok(aff), Ok(dic)) = (
                    std::fs::read_to_string(base.with_extension("aff")),
                    std::fs::read_to_string(base.with_extension("dic")),
                ) else {
                    continue;
                };
                if let Some(checker) = Self::new(&aff, &dic) {
                    return Some(checker);
                }
            }
        }
        None
    }
}

impl SpellChecker for DictionaryChecker {
    fn check(&self, text: &str) -> Vec<Range<usize>> {
        let added = self.added.borrow();
        words(text)
            .filter(|(_, word)| !added.contains(*word) && !self.dictionary.check(word))
            .map(|(range, _)| range)
            .collect()
    }

    fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.dictionary.suggest(word, &mut out);
        out
    }

    fn add(&self, word: &str) {
        self.added.borrow_mut().insert(word.to_string());
    }
}

/// A UTF-16 offset in `text`, as a byte offset: the system checkers count
/// UTF-16 units.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

#[cfg(target_os = "macos")]
mod mac_checker {
    //! The macOS spell checker (`NSSpellChecker`), the one Safari and Chrome
    //! use there. AppKit's checker runs on the main thread, where GPUI's app
    //! runs; elsewhere (a test's thread) it finds nothing.

    use std::ops::Range;

    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::NSSpellChecker;
    use objc2_foundation::{NSInteger, NSNotFound, NSRange, NSString};

    use super::{SpellChecker, byte_offset};

    pub struct SystemChecker {
        checker: Retained<NSSpellChecker>,
        /// The document tag the fields' words share: words ignored for it are
        /// ignored everywhere in the app.
        tag: NSInteger,
    }

    impl SystemChecker {
        /// The shared checker; `None` off the main thread.
        pub fn new() -> Option<Self> {
            MainThreadMarker::new()?;
            Some(SystemChecker {
                checker: NSSpellChecker::sharedSpellChecker(),
                tag: NSSpellChecker::uniqueSpellDocumentTag(),
            })
        }
    }

    impl SpellChecker for SystemChecker {
        fn check(&self, text: &str) -> Vec<Range<usize>> {
            if MainThreadMarker::new().is_none() {
                return Vec::new();
            }
            let string = NSString::from_str(text);
            let length = text.encode_utf16().count();
            let mut out = Vec::new();
            let mut start = 0usize;
            while start < length {
                // SAFETY: A null word count is allowed.
                let found = unsafe {
                    self.checker
                        .checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                            &string,
                            start as NSInteger,
                            None,
                            false,
                            self.tag,
                            std::ptr::null_mut(),
                        )
                };
                if found.location == NSNotFound as usize
                    || found.length == 0
                    || found.location < start
                {
                    break;
                }
                let end = found.location + found.length;
                out.push(byte_offset(text, found.location)..byte_offset(text, end));
                start = end;
            }
            out
        }

        fn suggest(&self, word: &str) -> Vec<String> {
            if MainThreadMarker::new().is_none() {
                return Vec::new();
            }
            let string = NSString::from_str(word);
            let range = NSRange::new(0, word.encode_utf16().count());
            self.checker
                .guessesForWordRange_inString_language_inSpellDocumentWithTag(
                    range, &string, None, self.tag,
                )
                .map(|guesses| guesses.iter().map(|guess| guess.to_string()).collect())
                .unwrap_or_default()
        }

        fn add(&self, word: &str) {
            if MainThreadMarker::new().is_some() {
                self.checker.learnWord(&NSString::from_str(word));
            }
        }
    }
}

#[cfg(windows)]
mod windows_checker {
    //! The Windows spell checker (`ISpellChecker`, Windows 8 and later).

    use std::ops::Range;

    use windows::Win32::Globalization::{
        CORRECTIVE_ACTION_NONE, GetUserDefaultLocaleName, ISpellChecker, ISpellCheckerFactory,
        SpellCheckerFactory,
    };
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
    use windows::core::{HSTRING, PWSTR};

    use super::{SpellChecker, byte_offset};

    pub struct SystemChecker {
        checker: ISpellChecker,
    }

    impl SystemChecker {
        /// The checker for the user's language, else English; `None` where
        /// COM is not set up or no language is supported.
        pub fn for_user() -> Option<Self> {
            let mut buffer = [0u16; 85];
            let len = unsafe { GetUserDefaultLocaleName(&mut buffer) };
            let user = (len > 1).then(|| String::from_utf16_lossy(&buffer[..len as usize - 1]));
            let factory: ISpellCheckerFactory =
                unsafe { CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER) }
                    .ok()?;
            for tag in user.iter().map(String::as_str).chain(["en-US"]) {
                let tag = HSTRING::from(tag);
                let supported = unsafe { factory.IsSupported(&tag) }.ok()?;
                if supported.as_bool()
                    && let Ok(checker) = unsafe { factory.CreateSpellChecker(&tag) }
                {
                    return Some(SystemChecker { checker });
                }
            }
            None
        }
    }

    impl SpellChecker for SystemChecker {
        fn check(&self, text: &str) -> Vec<Range<usize>> {
            let Ok(errors) = (unsafe { self.checker.Check(&HSTRING::from(text)) }) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            loop {
                let mut error = None;
                if unsafe { errors.Next(&mut error) }.is_err() {
                    break;
                }
                let Some(error) = error else { break };
                let (Ok(start), Ok(len), Ok(action)) = (
                    unsafe { error.StartIndex() },
                    unsafe { error.Length() },
                    unsafe { error.CorrectiveAction() },
                ) else {
                    continue;
                };
                if action == CORRECTIVE_ACTION_NONE {
                    continue;
                }
                // The checker counts UTF-16 units.
                let end = byte_offset(text, (start + len) as usize);
                let start = byte_offset(text, start as usize);
                out.push(start..end);
            }
            out
        }

        fn suggest(&self, word: &str) -> Vec<String> {
            let Ok(list) = (unsafe { self.checker.Suggest(&HSTRING::from(word)) }) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            loop {
                let mut item = [PWSTR::null()];
                let mut fetched = 0;
                let ok = unsafe { list.Next(&mut item, Some(&mut fetched)) };
                if ok.is_err() || fetched == 0 {
                    break;
                }
                let s = unsafe { item[0].to_string() }.unwrap_or_default();
                unsafe { CoTaskMemFree(Some(item[0].0 as *const _)) };
                out.push(s);
            }
            out
        }

        fn add(&self, word: &str) {
            unsafe { self.checker.Add(&HSTRING::from(word)) }.ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFF: &str = "SET UTF-8
TRY esianrtolcdugmphbyfvkwzESIANRTOLCDUGMPHBYFVKWZ'
";
    const DIC: &str = "5
hello
world
the
spell
don't
";

    #[test]
    fn words_skip_numbers_links_and_addresses() {
        let text = "Read the docs at https://example.com or mail me@example.com by 5pm, don't wait";
        let found: Vec<&str> = words(text).map(|(_, w)| w).collect();
        assert_eq!(
            found,
            [
                "Read", "the", "docs", "at", "or", "mail", "by", "don't", "wait"
            ]
        );
        for (range, word) in words(text) {
            assert_eq!(&text[range], word);
        }
    }

    #[test]
    fn utf16_offsets_become_byte_offsets() {
        let text = "née 😀 wrold";
        // "née " is four UTF-16 units and five bytes; the emoji two and four.
        assert_eq!(byte_offset(text, 4), 5);
        assert_eq!(byte_offset(text, 7), 10);
        assert_eq!(byte_offset(text, 12), text.len());
    }

    #[test]
    fn a_dictionary_marks_unknown_words_and_suggests() {
        let checker = DictionaryChecker::new(AFF, DIC).expect("a dictionary");
        assert_eq!(checker.check("hello wrold, the spel"), [6..11, 17..21]);
        assert!(checker.suggest("wrold").iter().any(|s| s == "world"));
        checker.add("wrold");
        assert_eq!(checker.check("hello wrold"), Vec::<Range<usize>>::new());
    }

    /// The system checker, where Windows has one for the user's language.
    #[cfg(windows)]
    #[test]
    fn the_windows_checker_marks_misspellings() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .ok();
        let Some(checker) = windows_checker::SystemChecker::for_user() else {
            return;
        };
        let text = "This sentense has a mistaek.";
        let found: Vec<&str> = checker.check(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(found, ["sentense", "mistaek"]);
        assert!(checker.suggest("sentense").iter().any(|s| s == "sentence"));
    }
}
