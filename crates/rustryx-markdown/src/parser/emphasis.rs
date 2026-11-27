//! Emphasis and strong, upstream `Markdown/parser.ts` ("Emphasis and strong",
//! CommonMark 0.31 §6.2): delimiter runs of `*` and `_`, and the pass that pairs them.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use super::inline::Item;
use crate::ast::{Literal, Parent, Phrasing};
use crate::js;

/// The deepest emphasis and strong may nest (upstream `MAX_EMPHASIS_DEPTH`). Pairs that
/// would nest deeper stay text, so no input builds a tree deep enough to overflow a
/// recursive walk.
const MAX_EMPHASIS_DEPTH: usize = 100;

/// Upstream `UNICODE_WHITESPACE`.
static UNICODE_WHITESPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\t\n\x0C\r\p{Zs}]$").expect("UNICODE_WHITESPACE is valid"));

/// Upstream `UNICODE_PUNCTUATION`.
static UNICODE_PUNCTUATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\p{P}\p{S}]$").expect("UNICODE_PUNCTUATION is valid"));

fn matches_char(regex: &Regex, c: char) -> bool {
    regex.is_match(c.encode_utf8(&mut [0; 4]))
}

/// Upstream `DelimiterRun`: a run of `*` or `_`, what it can do, and how many
/// delimiters are left. Its text is always the delimiter repeated `length` times.
pub(super) struct DelimiterRun {
    character: u8,
    original_length: usize,
    length: usize,
    pub(super) can_open: bool,
    pub(super) can_close: bool,
}

impl DelimiterRun {
    /// The run's text node, as many delimiters as are left.
    fn text(&self) -> Phrasing {
        Phrasing::Text(Literal {
            value: char::from(self.character).to_string().repeat(self.length),
            position: None,
            data: None,
        })
    }
}

/// Upstream `delimiterRun`: the run `text[start..end]` of `character`, left- or
/// right-flanking by the characters around it. An underscore run opens or closes only
/// where it is not inside a word.
pub(super) fn delimiter_run(character: u8, text: &str, start: usize, end: usize) -> DelimiterRun {
    let before = js::char_before(text, start);
    let after = js::char_at(text, end);
    // The start and end of the text count as whitespace.
    let space_before = before.is_none_or(|c| matches_char(&UNICODE_WHITESPACE, c));
    let space_after = after.is_none_or(|c| matches_char(&UNICODE_WHITESPACE, c));
    let punctuation_before = before.is_some_and(|c| matches_char(&UNICODE_PUNCTUATION, c));
    let punctuation_after = after.is_some_and(|c| matches_char(&UNICODE_PUNCTUATION, c));
    let left_flanking = !space_after && (!punctuation_after || space_before || punctuation_before);
    let right_flanking = !space_before && (!punctuation_before || space_after || punctuation_after);
    let star = character == b'*';
    DelimiterRun {
        character,
        original_length: end - start,
        length: end - start,
        can_open: if star {
            left_flanking
        } else {
            left_flanking && (!right_flanking || punctuation_before)
        },
        can_close: if star {
            right_flanking
        } else {
            right_flanking && (!left_flanking || punctuation_after)
        },
    }
}

/// Upstream `Entry`: a place in the doubly linked node list.
struct Entry {
    /// What the place holds: nothing at the list's head, before every node, nor once its
    /// node has moved into an emphasis or strong.
    slot: Option<Item>,
    previous: Option<usize>,
    next: Option<usize>,
}

/// Upstream's `StackRun`: a run on the delimiter stack, a doubly linked list in source
/// order. A run leaves it once used up, paired across, or unable to pair.
struct StackRun {
    previous: Option<usize>,
    next: Option<usize>,
    /// The deepest emphasis among the nodes after this run, up to the next.
    depth_after: usize,
    can_close: bool,
}

/// The strong a pair of runs made last, to keep `***x***` strong outside.
struct LastStrong {
    entry: usize,
    opener: usize,
    closer: usize,
}

/// Upstream `resolveEmphasis`: pairs the delimiter runs of one inline text into emphasis
/// and strong, as CommonMark's process-emphasis procedure does. Each closer, in order,
/// takes the nearest opener of its character that the rule of three allows: two
/// delimiters for strong when both have two, else one for emphasis; the nodes between
/// become its children. A run left unpaired is text, joined to the text beside it
/// (inside an emphasis it stays its own text node, as upstream leaves it). Runs leave a
/// linked stack as they are used or passed, so the pass is linear in the number of runs.
pub(super) fn resolve_emphasis(items: Vec<Item>, mut runs: Vec<DelimiterRun>) -> Vec<Phrasing> {
    let mut entries = vec![Entry {
        slot: None,
        previous: None,
        next: None,
    }];
    let mut entry_of_run = vec![0; runs.len()];
    for item in items {
        let entry = entries.len();
        if let Item::Run(run) = item {
            entry_of_run[run] = entry;
        }
        entries[entry - 1].next = Some(entry);
        entries.push(Entry {
            slot: Some(item),
            previous: Some(entry - 1),
            next: None,
        });
    }
    let unlink = |entries: &mut Vec<Entry>, entry: usize| {
        let Entry { previous, next, .. } = entries[entry];
        if let Some(previous) = previous {
            entries[previous].next = next;
        }
        if let Some(next) = next {
            entries[next].previous = previous;
        }
    };

    let count = runs.len();
    let mut stack: Vec<StackRun> = (0..count)
        .map(|index| StackRun {
            previous: index.checked_sub(1),
            next: (index + 1 < count).then_some(index + 1),
            depth_after: 0,
            can_close: runs[index].can_close,
        })
        .collect();
    // Takes a run off the stack; what followed it now follows the one before.
    let leave = |stack: &mut Vec<StackRun>, item: usize| {
        let StackRun {
            previous,
            next,
            depth_after,
            ..
        } = stack[item];
        if let Some(previous) = previous {
            stack[previous].next = next;
            stack[previous].depth_after = stack[previous].depth_after.max(depth_after);
        }
        if let Some(next) = next {
            stack[next].previous = previous;
        }
    };
    // Where the search for an opener stops, by source index, for closers of each kind: no
    // opener at or before it can match them.
    let mut openers_bottom: HashMap<(u8, bool, usize), Option<usize>> = HashMap::new();
    let mut last_strong: Option<LastStrong> = None;
    let mut closer_item = (count > 0).then_some(0);
    while let Some(closer) = closer_item {
        if !stack[closer].can_close || runs[closer].length == 0 {
            closer_item = stack[closer].next;
            continue;
        }
        let key = (
            runs[closer].character,
            runs[closer].can_open,
            runs[closer].original_length % 3,
        );
        let bottom = openers_bottom.get(&key).copied().flatten();
        let above_bottom = |item: usize| bottom.is_none_or(|bottom| item > bottom);
        let mut opener_item = stack[closer].previous;
        while let Some(opener) = opener_item.filter(|&opener| above_bottom(opener)) {
            let (candidate, closing) = (&runs[opener], &runs[closer]);
            // The rule of three: a run that can both open and close does not pair with one
            // whose lengths sum to a multiple of three, unless both are.
            let odd_match = (closing.can_open || candidate.can_close)
                && (candidate.original_length + closing.original_length).is_multiple_of(3)
                && !(candidate.original_length.is_multiple_of(3)
                    && closing.original_length.is_multiple_of(3));
            if candidate.character == closing.character
                && candidate.can_open
                && candidate.length > 0
                && !odd_match
            {
                break;
            }
            opener_item = stack[opener].previous;
        }
        let Some(opener) = opener_item.filter(|&opener| above_bottom(opener)) else {
            openers_bottom.insert(key, closer.checked_sub(1));
            let next = stack[closer].next;
            if !runs[closer].can_open {
                leave(&mut stack, closer);
            }
            closer_item = next;
            continue;
        };
        // Runs between the pair can no longer pair: take them off the stack, and note the
        // deepest emphasis among the nodes between.
        let mut inner_depth = stack[opener].depth_after;
        let mut between = stack[opener].next;
        while let Some(item) = between.filter(|&item| item != closer) {
            inner_depth = inner_depth.max(stack[item].depth_after);
            between = stack[item].next;
        }
        stack[opener].next = Some(closer);
        stack[closer].previous = Some(opener);
        stack[opener].depth_after = inner_depth;
        if inner_depth >= MAX_EMPHASIS_DEPTH {
            // Nesting deeper than the cap: both runs stay text.
            let next = stack[closer].next;
            leave(&mut stack, opener);
            stack[closer].can_close = false;
            if !runs[closer].can_open {
                leave(&mut stack, closer);
            }
            closer_item = next;
            continue;
        }
        let (opener_entry, closer_entry) = (entry_of_run[opener], entry_of_run[closer]);
        let used = if runs[opener].length >= 2 && runs[closer].length >= 2 {
            2
        } else {
            1
        };
        let mut child_entries = Vec::new();
        let mut entry = entries[opener_entry].next;
        while let Some(child) = entry.filter(|&child| child != closer_entry) {
            child_entries.push(child);
            entry = entries[child].next;
        }
        let keeps_strong_outside = used == 1
            && child_entries.len() == 1
            && last_strong.as_ref().is_some_and(|last| {
                last.entry == child_entries[0] && last.opener == opener && last.closer == closer
            });
        let children: Vec<Phrasing> = child_entries
            .iter()
            .map(|&child| match entries[child].slot.take() {
                Some(Item::Node(node)) => node,
                Some(Item::Run(run)) => runs[run].text(),
                None => unreachable!("an emphasis takes each node once"),
            })
            .collect();
        let wrapper_entry = entries.len();
        let wrapper = if used == 2 {
            last_strong = Some(LastStrong {
                entry: wrapper_entry,
                opener,
                closer,
            });
            Phrasing::Strong(Parent {
                children,
                position: None,
                data: None,
            })
        } else if keeps_strong_outside {
            let strong_children = match children.into_iter().next() {
                Some(Phrasing::Strong(strong)) => strong.children,
                _ => unreachable!("the last strong is a strong"),
            };
            let emphasis = Phrasing::Emphasis(Parent {
                children: strong_children,
                position: None,
                data: None,
            });
            Phrasing::Strong(Parent {
                children: vec![emphasis],
                position: None,
                data: None,
            })
        } else {
            Phrasing::Emphasis(Parent {
                children,
                position: None,
                data: None,
            })
        };
        entries.push(Entry {
            slot: Some(Item::Node(wrapper)),
            previous: Some(opener_entry),
            next: Some(closer_entry),
        });
        entries[opener_entry].next = Some(wrapper_entry);
        entries[closer_entry].previous = Some(wrapper_entry);
        stack[opener].depth_after = inner_depth + 1;
        runs[opener].length -= used;
        runs[closer].length -= used;
        if runs[opener].length == 0 {
            unlink(&mut entries, opener_entry);
            leave(&mut stack, opener);
        }
        if runs[closer].length == 0 {
            unlink(&mut entries, closer_entry);
            let next = stack[closer].next;
            leave(&mut stack, closer);
            closer_item = next;
        }
    }

    // An unpaired run is text, joined to the text beside it.
    let mut result: Vec<Phrasing> = Vec::new();
    let mut previous_is_leftover = false;
    let mut entry = entries[0].next;
    while let Some(index) = entry {
        entry = entries[index].next;
        let (node, is_leftover) = match entries[index].slot.take() {
            Some(Item::Node(node)) => (node, false),
            Some(Item::Run(run)) => (runs[run].text(), runs[run].length > 0),
            None => unreachable!("the list holds each node once"),
        };
        if let (Phrasing::Text(text), Some(Phrasing::Text(last))) = (&node, result.last_mut())
            && (is_leftover || previous_is_leftover)
        {
            last.value.push_str(&text.value);
            previous_is_leftover = true;
            continue;
        }
        result.push(node);
        previous_is_leftover = is_leftover;
    }
    result
}
