//! The text and selection of an editable field, and the edits a field makes:
//! insertion, deletion by grapheme or word, movement, selection, IME
//! composition, and undo and redo. Pure: no GPUI, so every rule is a unit test.
//!
//! Offsets are UTF-8 byte offsets on grapheme boundaries. The IME speaks
//! UTF-16, so the buffer converts.
//!
//! Inline objects (a chat composer's tokens) are atomic: each is one
//! [`OBJECT`] character in the text, and the buffer keeps their ids in text
//! order beside it. Edits keep the ids in step, undo restores them, and caret
//! movement and deletion treat each as one grapheme. Text typed, pasted or set
//! from outside never carries the character; only [`EditBuffer::insert_object`]
//! and [`EditBuffer::set_content`] place objects.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// How far a movement or deletion reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Granularity {
    /// One user-perceived character.
    Grapheme,
    /// To the next word boundary, skipping whitespace (Ctrl+arrow).
    Word,
    /// To the start or end of the logical line (Home/End).
    Line,
    /// To the start or end of the text.
    All,
}

/// Which way a movement goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Backward,
    Forward,
}

/// The character that stands for an inline object in the text (U+FFFC,
/// OBJECT REPLACEMENT CHARACTER).
pub const OBJECT: char = '\u{FFFC}';

/// An inline object's id, chosen by whoever inserts it.
pub type ObjectId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    text: String,
    selection: Range<usize>,
    reversed: bool,
    objects: Vec<ObjectId>,
}

/// The kind of the last edit, so typing and deleting coalesce into one undo
/// step the way text fields do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    /// One character typed; `space` when it was whitespace.
    Typing {
        space: bool,
    },
    Deleting,
    Other,
}

/// An editable text with its selection, upstream's input value and caret.
#[derive(Clone, Debug)]
pub struct EditBuffer {
    text: String,
    selection: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    multiline: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: EditKind,
    /// The inline objects' ids, in the order of their characters in `text`.
    objects: Vec<ObjectId>,
}

/// Undo keeps this many steps.
const UNDO_LIMIT: usize = 200;

impl EditBuffer {
    /// A buffer holding `text`, the caret at its end.
    pub fn new(text: impl Into<String>, multiline: bool) -> Self {
        let text = Self::clean(text.into(), multiline);
        let end = text.len();
        EditBuffer {
            text,
            selection: end..end,
            reversed: false,
            marked: None,
            multiline,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: EditKind::Other,
            objects: Vec::new(),
        }
    }

    /// Single-line fields hold no line breaks: pasted ones become spaces.
    /// Plain text holds no object characters.
    fn clean(text: String, multiline: bool) -> String {
        let text = if text.contains(OBJECT) {
            text.replace(OBJECT, "")
        } else {
            text
        };
        Self::clean_breaks(text, multiline)
    }

    fn clean_breaks(text: String, multiline: bool) -> String {
        if multiline {
            text.replace("\r\n", "\n").replace('\r', "\n")
        } else if text.contains(['\n', '\r']) {
            text.replace("\r\n", " ").replace(['\n', '\r'], " ")
        } else {
            text
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_multiline(&self) -> bool {
        self.multiline
    }

    /// The selection, start before end.
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    /// Whether the caret is at the selection's start.
    pub fn is_reversed(&self) -> bool {
        self.reversed
    }

    /// Where the caret is.
    pub fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    /// The end of the selection that stays put while it is extended.
    pub fn anchor(&self) -> usize {
        if self.reversed {
            self.selection.end
        } else {
            self.selection.start
        }
    }

    /// The IME composition, if one is in progress.
    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    /// Whether the selected text is empty.
    pub fn is_collapsed(&self) -> bool {
        self.selection.is_empty()
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.clone()]
    }

    // ------------------------------------------------------------------
    // Outside changes

    /// Replaces the text from outside (a controlled value that changed). The
    /// selection is kept where it still fits; the history restarts, as a
    /// browser's does when a script sets a field's value.
    pub fn set_text(&mut self, text: impl Into<String>) {
        let text = Self::clean(text.into(), self.multiline);
        if text == self.text {
            return;
        }
        self.text = text;
        let clamp = |offset: usize, text: &str| {
            let mut offset = offset.min(text.len());
            while !text.is_char_boundary(offset) {
                offset -= 1;
            }
            offset
        };
        self.selection =
            clamp(self.selection.start, &self.text)..clamp(self.selection.end, &self.text);
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        self.last_edit = EditKind::Other;
        self.objects.clear();
    }

    /// Replaces the content from outside, objects included: `text` holds one
    /// [`OBJECT`] per entry of `objects`, in order (extra characters are
    /// dropped, missing ids are 0). The caret goes to the end and the history
    /// restarts, as with [`Self::set_text`].
    pub fn set_content(&mut self, text: impl Into<String>, objects: &[ObjectId]) {
        let (text, objects) = Self::clean_rich(text.into(), objects, self.multiline);
        self.text = text;
        self.objects = objects;
        let end = self.text.len();
        self.selection = end..end;
        self.reversed = false;
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        self.last_edit = EditKind::Other;
    }

    /// Text that may hold objects, with exactly one id per object character.
    fn clean_rich(text: String, objects: &[ObjectId], multiline: bool) -> (String, Vec<ObjectId>) {
        let mut ids = objects.iter().copied();
        let mut kept = Vec::new();
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            if ch == OBJECT {
                kept.push(ids.next().unwrap_or(0));
            }
            out.push(ch);
        }
        (Self::clean_breaks(out, multiline), kept)
    }

    /// The inline objects' ids, in text order.
    pub fn objects(&self) -> &[ObjectId] {
        &self.objects
    }

    /// Each inline object's offset in the text, with its id.
    pub fn object_offsets(&self) -> Vec<(usize, ObjectId)> {
        self.text
            .match_indices(OBJECT)
            .map(|(i, _)| i)
            .zip(self.objects.iter().copied())
            .collect()
    }

    /// The object whose character starts at `offset`, if any.
    pub fn object_at(&self, offset: usize) -> Option<ObjectId> {
        if !self.text.get(offset..)?.starts_with(OBJECT) {
            return None;
        }
        let nth = self.text[..offset].matches(OBJECT).count();
        self.objects.get(nth).copied()
    }

    /// Keeps the object ids in step with replacing `range` by text holding
    /// `inserted` objects. Call before the text changes.
    fn splice_objects(&mut self, range: &Range<usize>, inserted: &[ObjectId]) {
        if self.objects.is_empty() && inserted.is_empty() {
            return;
        }
        let before = self.text[..range.start].matches(OBJECT).count();
        let within = self.text[range.clone()].matches(OBJECT).count();
        let end = (before + within).min(self.objects.len());
        let before = before.min(end);
        self.objects.splice(before..end, inserted.iter().copied());
    }

    // ------------------------------------------------------------------
    // Boundaries

    /// The grapheme boundary before `offset`.
    pub fn previous_grapheme(&self, offset: usize) -> usize {
        let previous = self
            .text
            .grapheme_indices(true)
            .rev()
            .find_map(|(i, _)| (i < offset).then_some(i))
            .unwrap_or(0);
        // An object is a grapheme of its own: nothing combines with it.
        match self.text[previous..offset].rfind(OBJECT) {
            Some(i) if previous + i + OBJECT.len_utf8() == offset => previous + i,
            Some(i) => previous + i + OBJECT.len_utf8(),
            None => previous,
        }
    }

    /// The grapheme boundary after `offset`.
    pub fn next_grapheme(&self, offset: usize) -> usize {
        let next = self
            .text
            .grapheme_indices(true)
            .find_map(|(i, _)| (i > offset).then_some(i))
            .unwrap_or(self.text.len());
        match self.text[offset..next].find(OBJECT) {
            Some(0) => offset + OBJECT.len_utf8(),
            Some(i) => offset + i,
            None => next,
        }
    }

    /// The start of the word at or before `offset`, skipping the whitespace
    /// between: where Ctrl+Left lands.
    pub fn previous_word(&self, offset: usize) -> usize {
        self.text
            .split_word_bound_indices()
            .rev()
            .find(|(i, word)| *i < offset && !word.trim().is_empty())
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// The end of the word at or after `offset`, skipping the whitespace
    /// before it: where Ctrl+Right lands.
    pub fn next_word(&self, offset: usize) -> usize {
        self.text
            .split_word_bound_indices()
            .map(|(i, word)| (i + word.len(), word))
            .find(|(end, word)| *end > offset && !word.trim().is_empty())
            .map(|(end, _)| end)
            .unwrap_or(self.text.len())
    }

    /// The start of the logical line holding `offset`.
    pub fn line_start(&self, offset: usize) -> usize {
        self.text[..offset].rfind('\n').map_or(0, |i| i + 1)
    }

    /// The end of the logical line holding `offset`.
    pub fn line_end(&self, offset: usize) -> usize {
        self.text[offset..]
            .find('\n')
            .map_or(self.text.len(), |i| offset + i)
    }

    /// The boundary `granularity` reaches from `offset` going `direction`.
    pub fn boundary(&self, offset: usize, granularity: Granularity, direction: Direction) -> usize {
        match (granularity, direction) {
            (Granularity::Grapheme, Direction::Backward) => self.previous_grapheme(offset),
            (Granularity::Grapheme, Direction::Forward) => self.next_grapheme(offset),
            (Granularity::Word, Direction::Backward) => self.previous_word(offset),
            (Granularity::Word, Direction::Forward) => self.next_word(offset),
            (Granularity::Line, Direction::Backward) => self.line_start(offset),
            (Granularity::Line, Direction::Forward) => self.line_end(offset),
            (Granularity::All, Direction::Backward) => 0,
            (Granularity::All, Direction::Forward) => self.text.len(),
        }
    }

    /// The word (or run of whitespace) around `offset`: what a double-click
    /// selects.
    pub fn word_range_at(&self, offset: usize) -> Range<usize> {
        self.text
            .split_word_bound_indices()
            .map(|(i, word)| i..i + word.len())
            .find(|range| {
                range.contains(&offset) || (offset == self.text.len() && range.end == offset)
            })
            .unwrap_or(offset..offset)
    }

    // ------------------------------------------------------------------
    // Selection

    /// Collapses the selection at `offset`.
    pub fn move_to(&mut self, offset: usize) {
        let offset = offset.min(self.text.len());
        self.selection = offset..offset;
        self.reversed = false;
        self.last_edit = EditKind::Other;
    }

    /// Extends the selection from its anchor to `offset`.
    pub fn select_to(&mut self, offset: usize) {
        let offset = offset.min(self.text.len());
        let anchor = self.anchor();
        if offset < anchor {
            self.selection = offset..anchor;
            self.reversed = true;
        } else {
            self.selection = anchor..offset;
            self.reversed = false;
        }
        self.last_edit = EditKind::Other;
    }

    /// Selects `range`, the caret at its end (or start, `reversed`).
    pub fn select(&mut self, range: Range<usize>, reversed: bool) {
        let len = self.text.len();
        self.selection = range.start.min(len)..range.end.min(len);
        self.reversed = reversed && !self.selection.is_empty();
        self.last_edit = EditKind::Other;
    }

    pub fn select_all(&mut self) {
        self.select(0..self.text.len(), false);
    }

    /// The arrow keys: moves (or with `extend`, extends) the caret. A plain
    /// grapheme move over a selection collapses it to that side.
    pub fn move_by(&mut self, granularity: Granularity, direction: Direction, extend: bool) {
        if extend {
            let to = self.boundary(self.cursor(), granularity, direction);
            self.select_to(to);
        } else if !self.is_collapsed() && granularity == Granularity::Grapheme {
            let to = match direction {
                Direction::Backward => self.selection.start,
                Direction::Forward => self.selection.end,
            };
            self.move_to(to);
        } else {
            let to = self.boundary(self.cursor(), granularity, direction);
            self.move_to(to);
        }
    }

    // ------------------------------------------------------------------
    // Edits

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            selection: self.selection.clone(),
            reversed: self.reversed,
            objects: self.objects.clone(),
        }
    }

    fn record(&mut self, kind: EditKind) {
        // Typing (and deleting) coalesce until the kind changes or the caret
        // moves. A word keeps the spaces typed after it; the next word starts a
        // new step.
        let coalesce = match (self.last_edit, kind) {
            (EditKind::Typing { space: previous }, EditKind::Typing { space }) => {
                !previous || space
            }
            (EditKind::Deleting, EditKind::Deleting) => true,
            _ => false,
        };
        if !coalesce {
            self.undo.push(self.snapshot());
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_edit = kind;
    }

    /// Replaces `range` with `text`, the caret after it. Returns whether the
    /// text changed.
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> bool {
        let text = Self::clean(text.to_string(), self.multiline);
        let range = range.start.min(self.text.len())..range.end.min(self.text.len());
        if text.is_empty() && range.is_empty() {
            return false;
        }
        let kind = if text.is_empty() {
            EditKind::Deleting
        } else if range.is_empty() && text.graphemes(true).count() == 1 {
            EditKind::Typing {
                space: text.trim().is_empty(),
            }
        } else {
            EditKind::Other
        };
        self.record(kind);
        self.splice_objects(&range, &[]);
        self.text.replace_range(range.clone(), &text);
        let caret = range.start + text.len();
        self.selection = caret..caret;
        self.reversed = false;
        self.marked = None;
        true
    }

    /// Types `text` over the composition, else the selection.
    pub fn insert(&mut self, text: &str) -> bool {
        let range = self.marked.clone().unwrap_or(self.selection.clone());
        self.replace(range, text)
    }

    /// Puts inline object `id` over the composition, else the selection, the
    /// caret after it: one undo step.
    pub fn insert_object(&mut self, id: ObjectId) -> bool {
        let range = self.marked.clone().unwrap_or(self.selection.clone());
        self.replace_rich(range, &OBJECT.to_string(), &[id])
    }

    /// Replaces `range` with `text`, which may hold objects: one [`OBJECT`]
    /// per id in `objects`, in order. The caret goes after it; one undo step.
    pub fn replace_rich(&mut self, range: Range<usize>, text: &str, objects: &[ObjectId]) -> bool {
        let (text, objects) = Self::clean_rich(text.to_string(), objects, self.multiline);
        let range = range.start.min(self.text.len())..range.end.min(self.text.len());
        if text.is_empty() && range.is_empty() {
            return false;
        }
        self.record(EditKind::Other);
        self.splice_objects(&range, &objects);
        self.text.replace_range(range.clone(), &text);
        let caret = range.start + text.len();
        self.selection = caret..caret;
        self.reversed = false;
        self.marked = None;
        true
    }

    /// Backspace or Delete (with `granularity` for Ctrl): removes the
    /// selection, else up to the boundary. Returns whether anything went.
    pub fn delete(&mut self, granularity: Granularity, direction: Direction) -> bool {
        if !self.is_collapsed() {
            return self.replace(self.selection.clone(), "");
        }
        let cursor = self.cursor();
        let to = self.boundary(cursor, granularity, direction);
        let range = cursor.min(to)..cursor.max(to);
        !range.is_empty() && self.replace(range, "")
    }

    /// The IME's in-progress text: replaces the composition (else the
    /// selection) and marks it; `selected` is the caret within it.
    pub fn replace_and_mark(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
    ) {
        let range = range
            .or(self.marked.clone())
            .unwrap_or(self.selection.clone());
        if self.marked.is_none() {
            self.record(EditKind::Other);
        }
        let text = &Self::clean(text.to_string(), self.multiline);
        self.splice_objects(&range, &[]);
        self.text.replace_range(range.clone(), text);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        self.selection = match selected {
            Some(selected) => {
                let start = range.start + selected.start.min(text.len());
                let end = range.start + selected.end.min(text.len());
                start..end
            }
            None => {
                let end = range.start + text.len();
                end..end
            }
        };
        self.reversed = false;
        self.last_edit = EditKind::Other;
    }

    /// Ends the composition, keeping its text.
    pub fn unmark(&mut self) {
        self.marked = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Steps back one edit. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(previous);
        true
    }

    /// Steps forward again. Returns whether there was a step.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(next);
        true
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.selection = snapshot.selection;
        self.reversed = snapshot.reversed;
        self.objects = snapshot.objects;
        self.marked = None;
        self.last_edit = EditKind::Other;
    }

    // ------------------------------------------------------------------
    // UTF-16, for the IME

    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())]
            .encode_utf16()
            .count()
    }

    pub fn offset_from_utf16(&self, utf16: usize) -> usize {
        let mut count = 0;
        for (i, ch) in self.text.char_indices() {
            if count >= utf16 {
                return i;
            }
            count += ch.len_utf16();
        }
        self.text.len()
    }

    pub fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    pub fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> EditBuffer {
        EditBuffer::new(text, false)
    }

    #[test]
    fn typing_inserts_at_the_caret_and_over_selections() {
        let mut b = buffer("hello");
        b.insert("!");
        assert_eq!(b.text(), "hello!");
        b.select(0..5, false);
        b.insert("bye");
        assert_eq!((b.text(), b.cursor()), ("bye!", 3));
    }

    #[test]
    fn backspace_removes_whole_graphemes() {
        let mut b = buffer("ae\u{301}");
        b.delete(Granularity::Grapheme, Direction::Backward);
        assert_eq!(b.text(), "a", "e + combining accent is one grapheme");
        let mut b = buffer("👍🏽");
        b.delete(Granularity::Grapheme, Direction::Backward);
        assert_eq!(b.text(), "");
    }

    #[test]
    fn word_moves_skip_whitespace_and_punctuation_runs() {
        let b = buffer("hello brave  new world");
        assert_eq!(b.previous_word(22), 17);
        assert_eq!(b.previous_word(17), 13);
        assert_eq!(b.next_word(0), 5);
        assert_eq!(b.next_word(5), 11);
        let mut b = buffer("one two");
        b.delete(Granularity::Word, Direction::Backward);
        assert_eq!(b.text(), "one ");
    }

    #[test]
    fn shift_arrows_extend_and_plain_arrows_collapse() {
        let mut b = buffer("abc");
        b.move_to(1);
        b.move_by(Granularity::Grapheme, Direction::Forward, true);
        b.move_by(Granularity::Grapheme, Direction::Forward, true);
        assert_eq!(b.selection(), 1..3);
        b.move_by(Granularity::Grapheme, Direction::Backward, false);
        assert_eq!(b.selection(), 1..1, "collapses to the selection's start");
        b.move_by(Granularity::All, Direction::Backward, true);
        assert_eq!((b.selection(), b.is_reversed()), (0..1, true));
    }

    #[test]
    fn single_line_buffers_turn_line_breaks_into_spaces() {
        let mut b = buffer("");
        b.insert("a\r\nb\nc");
        assert_eq!(b.text(), "a b c");
        let mut m = EditBuffer::new("", true);
        m.insert("a\r\nb");
        assert_eq!(m.text(), "a\nb");
    }

    #[test]
    fn typing_coalesces_into_one_undo_step() {
        let mut b = buffer("");
        for ch in ["h", "i"] {
            b.insert(ch);
        }
        b.insert(" ");
        b.insert("x");
        assert!(b.undo());
        assert_eq!(b.text(), "hi ", "the word after the space is its own step");
        assert!(b.undo());
        assert_eq!(b.text(), "");
        assert!(b.redo());
        assert_eq!(b.text(), "hi ");
    }

    #[test]
    fn a_caret_move_ends_the_typing_step() {
        let mut b = buffer("");
        b.insert("a");
        b.move_to(0);
        b.insert("b");
        b.undo();
        assert_eq!(b.text(), "a");
    }

    #[test]
    fn outside_changes_keep_the_selection_and_restart_history() {
        let mut b = buffer("hello");
        b.select(1..3, false);
        b.insert("x");
        b.set_text("hi");
        assert_eq!(b.selection(), 2..2);
        assert!(!b.can_undo());
    }

    #[test]
    fn the_ime_marks_its_composition_then_commits_it() {
        let mut b = buffer("a");
        b.replace_and_mark(None, "ni", None);
        assert_eq!((b.text(), b.marked()), ("ani", Some(1..3)));
        b.replace_and_mark(None, "你", None);
        assert_eq!((b.text(), b.marked()), ("a你", Some(1..4)));
        b.insert("你好");
        assert_eq!((b.text(), b.marked()), ("a你好", None));
    }

    #[test]
    fn utf16_offsets_round_trip() {
        let b = buffer("a😀b");
        assert_eq!(b.offset_to_utf16(5), 3);
        assert_eq!(b.offset_from_utf16(3), 5);
        assert_eq!(b.range_from_utf16(&(1..3)), 1..5);
    }

    #[test]
    fn double_click_selects_the_word() {
        let b = buffer("hello world");
        assert_eq!(b.word_range_at(7), 6..11);
        assert_eq!(b.word_range_at(5), 5..6, "the space between");
    }

    #[test]
    fn objects_are_atomic_graphemes_kept_in_step_with_edits() {
        let mut b = EditBuffer::new("", true);
        b.insert("hi ");
        b.insert_object(7);
        b.insert("\u{A0}");
        b.insert_object(8);
        assert_eq!(b.objects(), &[7, 8]);
        let first = b.text().find(OBJECT).unwrap();
        assert_eq!(b.object_at(first), Some(7));
        // The caret steps over an object in one move.
        b.move_to(first);
        b.move_by(Granularity::Grapheme, Direction::Forward, false);
        assert_eq!(b.cursor(), first + OBJECT.len_utf8());
        b.move_by(Granularity::Grapheme, Direction::Backward, false);
        assert_eq!(b.cursor(), first);
        // A combining mark typed after an object does not join it.
        b.move_to(first + OBJECT.len_utf8());
        b.insert("\u{301}");
        assert_eq!(b.next_grapheme(first), first + OBJECT.len_utf8());
        b.delete(Granularity::Grapheme, Direction::Backward);
        // Backspace after an object removes it, and its id.
        b.delete(Granularity::Grapheme, Direction::Backward);
        assert_eq!(b.objects(), &[8]);
        assert_eq!(b.text().matches(OBJECT).count(), 1);
        // Undo brings it back with its id.
        b.undo();
        assert_eq!(b.objects(), &[7, 8]);
    }

    #[test]
    fn plain_text_never_carries_object_characters() {
        let mut b = EditBuffer::new("a\u{FFFC}b", true);
        assert_eq!(b.text(), "ab");
        b.insert("\u{FFFC}");
        assert_eq!(b.text(), "ab");
        b.set_content("x\u{FFFC}y\u{FFFC}", &[3]);
        assert_eq!(b.objects(), &[3, 0]);
        b.select_all();
        b.insert("z");
        assert!(b.objects().is_empty());
    }

    #[test]
    fn home_and_end_reach_the_logical_line() {
        let b = EditBuffer::new("one\ntwo", true);
        assert_eq!(b.line_start(5), 4);
        assert_eq!(b.line_end(5), 7);
        assert_eq!(b.line_end(1), 3);
    }
}
