//! The editing state of one field: an entity holding the buffer, its last
//! layout, and the field's settings. It is GPUI's IME client
//! (`EntityInputHandler`) and handles the editing actions.
//!
//! Adapted from GPUI's `examples/input.rs` (Apache-2.0, Copyright Zed
//! Industries), extended with words, multiline text, undo, masking and a
//! filter.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    Action, App, Bounds, ClipboardItem, Context, EntityInputHandler, FocusHandle, Focusable,
    KeyBinding, KeyBindingContextPredicate, KeyDownEvent, OperatingSystem, Pixels, Point,
    Subscription, UTF16Selection, Window, WrappedLine, actions, point, px,
};
use unicode_segmentation::UnicodeSegmentation;
use web_time::Instant;

use super::buffer::{Direction, EditBuffer, Granularity};

actions!(
    astryx_text,
    [
        /// Deletes the grapheme before the caret, or the selection.
        Backspace,
        /// Deletes the grapheme after the caret, or the selection.
        Delete,
        /// Deletes to the start of the word before the caret.
        DeleteWordBackward,
        /// Deletes to the end of the word after the caret.
        DeleteWordForward,
        Left,
        Right,
        Up,
        Down,
        WordLeft,
        WordRight,
        /// The start of the line.
        Home,
        /// The end of the line.
        End,
        DocumentStart,
        DocumentEnd,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectHome,
        SelectEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        /// Enter: a line break in multiline fields, else the field's Enter callback.
        Enter,
        ShowCharacterPalette,
    ]
);

/// The key context of focused text fields.
pub const KEY_CONTEXT: &str = "AstryxTextInput";

/// Binds the editing keys in text fields: the platform's conventions, Ctrl
/// for shortcuts on Windows and Linux and Cmd on macOS.
pub fn init(cx: &mut App) {
    let mac = cx.platform_info().os == OperatingSystem::Mac;
    let word = if mac { "alt" } else { "ctrl" };
    let mut keys: Vec<(String, Box<dyn Action>)> = vec![
        ("backspace".into(), Box::new(Backspace)),
        ("shift-backspace".into(), Box::new(Backspace)),
        ("delete".into(), Box::new(Delete)),
        (format!("{word}-backspace"), Box::new(DeleteWordBackward)),
        (format!("{word}-delete"), Box::new(DeleteWordForward)),
        ("left".into(), Box::new(Left)),
        ("right".into(), Box::new(Right)),
        ("up".into(), Box::new(Up)),
        ("down".into(), Box::new(Down)),
        (format!("{word}-left"), Box::new(WordLeft)),
        (format!("{word}-right"), Box::new(WordRight)),
        ("home".into(), Box::new(Home)),
        ("end".into(), Box::new(End)),
        ("shift-left".into(), Box::new(SelectLeft)),
        ("shift-right".into(), Box::new(SelectRight)),
        ("shift-up".into(), Box::new(SelectUp)),
        ("shift-down".into(), Box::new(SelectDown)),
        (format!("{word}-shift-left"), Box::new(SelectWordLeft)),
        (format!("{word}-shift-right"), Box::new(SelectWordRight)),
        ("shift-home".into(), Box::new(SelectHome)),
        ("shift-end".into(), Box::new(SelectEnd)),
        ("secondary-a".into(), Box::new(SelectAll)),
        ("secondary-c".into(), Box::new(Copy)),
        ("secondary-x".into(), Box::new(Cut)),
        ("secondary-v".into(), Box::new(Paste)),
        ("secondary-z".into(), Box::new(Undo)),
        ("secondary-shift-z".into(), Box::new(Redo)),
        ("enter".into(), Box::new(Enter)),
        ("shift-enter".into(), Box::new(Enter)),
    ];
    if mac {
        keys.extend([
            ("cmd-left".to_string(), Box::new(Home) as Box<dyn Action>),
            ("cmd-right".into(), Box::new(End)),
            ("cmd-up".into(), Box::new(DocumentStart)),
            ("cmd-down".into(), Box::new(DocumentEnd)),
            ("cmd-shift-left".into(), Box::new(SelectHome)),
            ("cmd-shift-right".into(), Box::new(SelectEnd)),
            ("cmd-shift-up".into(), Box::new(SelectDocumentStart)),
            ("cmd-shift-down".into(), Box::new(SelectDocumentEnd)),
            ("cmd-backspace".into(), Box::new(DeleteWordBackward)),
            ("ctrl-cmd-space".into(), Box::new(ShowCharacterPalette)),
        ]);
    } else {
        keys.extend([
            (
                "ctrl-home".to_string(),
                Box::new(DocumentStart) as Box<dyn Action>,
            ),
            ("ctrl-end".into(), Box::new(DocumentEnd)),
            ("ctrl-shift-home".into(), Box::new(SelectDocumentStart)),
            ("ctrl-shift-end".into(), Box::new(SelectDocumentEnd)),
            ("ctrl-y".into(), Box::new(Redo)),
        ]);
    }
    let context = KeyBindingContextPredicate::parse(KEY_CONTEXT)
        .ok()
        .map(Rc::new);
    let bindings: Vec<KeyBinding> = keys
        .into_iter()
        .filter_map(|(keys, action)| {
            KeyBinding::load(&keys, action, context.clone(), false, None, cx).ok()
        })
        .collect();
    cx.bind_keys(bindings);
}

/// Accepts or rewrites inserted text (a NumberInput keeps digits): given the
/// current text, the byte range the insertion replaces and the text being
/// inserted, returns what to insert.
pub type InsertFilter = Rc<dyn Fn(&str, Range<usize>, &str) -> String>;

/// Told the clipboard text of a paste, before it lands.
pub type PasteFn = Rc<dyn Fn(&str, &mut Window, &mut App)>;
/// Told when the field gains or loses focus.
pub type FocusFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Told the new text after each change the user makes.
pub type ChangeFn = Rc<dyn Fn(String, &mut Window, &mut App)>;
/// Told when Enter is pressed in a single-line field.
pub type EnterFn = Rc<dyn Fn(&mut Window, &mut App)>;
/// Told every key pressed in a focused field, before the field acts on it.
pub type KeyDownFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;

/// The last layout of a field, for hit testing and the IME.
#[derive(Clone)]
pub(crate) struct EditorLayout {
    /// Each logical line's start offset in the display text, and its layout.
    pub lines: Rc<[(usize, WrappedLine)]>,
    pub bounds: Bounds<Pixels>,
    pub line_height: Pixels,
    pub scroll: Point<Pixels>,
}

/// How a field behaves, set by its component each frame.
#[derive(Clone, Default)]
pub struct FieldSettings {
    pub disabled: bool,
    pub read_only: bool,
    /// Shows a bullet per character, as password fields do.
    pub masked: bool,
    pub filter: Option<InsertFilter>,
}

/// The editing state of one text field.
pub struct TextEditState {
    focus: FocusHandle,
    pub(crate) buffer: EditBuffer,
    pub(crate) layout: Option<EditorLayout>,
    pub(crate) settings: FieldSettings,
    selecting: bool,
    goal_x: Option<Pixels>,
    /// When the caret last moved or the text last changed: the caret shows
    /// solid for a moment after, then blinks.
    pub(crate) caret_since: Option<Instant>,
    pub(crate) blink_scheduled: bool,
    /// Whether the last edit came from this field (typing, paste, undo), so
    /// the component reports it.
    changed: bool,
    enter_pressed: bool,
    /// A paste waiting to be reported, before its change.
    pasted: Option<String>,
    /// The component's callbacks, refreshed every render.
    pub on_change: Option<ChangeFn>,
    pub on_enter: Option<EnterFn>,
    pub on_key_down: Option<KeyDownFn>,
    pub on_paste: Option<PasteFn>,
    pub on_focus: Option<FocusFn>,
    pub on_blur: Option<FocusFn>,
    _subscriptions: Vec<Subscription>,
}

impl TextEditState {
    pub fn new(text: impl Into<String>, multiline: bool, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        // Key bindings stop a key before element key listeners see it, so
        // `on_key_down` intercepts keystrokes instead: every key reaches it
        // first while this field has focus (upstream onKeyDown).
        let weak = cx.weak_entity();
        let keys = cx.intercept_keystrokes({
            let focus = focus.clone();
            move |event, window, cx| {
                if !focus.is_focused(window) {
                    return;
                }
                let Some(f) = weak
                    .upgrade()
                    .and_then(|state| state.read(cx).on_key_down.clone())
                else {
                    return;
                };
                let key_down = KeyDownEvent {
                    keystroke: event.keystroke.clone(),
                    is_held: false,
                    prefer_character_input: false,
                };
                f(&key_down, window, cx);
            }
        });
        TextEditState {
            focus,
            buffer: EditBuffer::new(text, multiline),
            layout: None,
            settings: FieldSettings::default(),
            selecting: false,
            goal_x: None,
            caret_since: None,
            blink_scheduled: false,
            changed: false,
            enter_pressed: false,
            pasted: None,
            on_change: None,
            on_enter: None,
            on_key_down: None,
            on_paste: None,
            on_focus: None,
            on_blur: None,
            _subscriptions: vec![keys],
        }
    }

    /// A field's state that reports its changes: after each notify, a user
    /// change calls `on_change` with the text, and Enter calls `on_enter`.
    /// Use it as the init of `window.use_keyed_state`.
    pub fn reporting(
        text: impl Into<String>,
        multiline: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let entity = cx.entity();
        window
            .observe(&entity, cx, |entity, window, cx| {
                let (pasted, on_paste) =
                    entity.update(cx, |s, _| (s.pasted.take(), s.on_paste.clone()));
                if let (Some(text), Some(f)) = (pasted, on_paste) {
                    f(&text, window, cx);
                }
                let (changed, enter, text, on_change, on_enter) = entity.update(cx, |s, _| {
                    (
                        s.take_changed(),
                        s.take_enter(),
                        s.text().to_string(),
                        s.on_change.clone(),
                        s.on_enter.clone(),
                    )
                });
                if changed && let Some(f) = on_change {
                    f(text, window, cx);
                }
                if enter && let Some(f) = on_enter {
                    f(window, cx);
                }
            })
            .detach();
        let mut this = Self::new(text, multiline, cx);
        // Focus callbacks run after the focus change settles, outside this
        // entity's update, so they may read the field.
        let focus = this.focus.clone();
        this._subscriptions
            .push(cx.on_focus(&focus, window, |this, window, cx| {
                // Tabbing into a single-line field selects its text, as
                // platform fields do.
                if !this.buffer.is_multiline() && window.last_input_was_keyboard() {
                    this.buffer.select_all();
                    cx.notify();
                }
                if let Some(f) = this.on_focus.clone() {
                    window.defer(cx, move |window, cx| f(window, cx));
                }
            }));
        this._subscriptions
            .push(cx.on_blur(&focus, window, |this, window, cx| {
                if let Some(f) = this.on_blur.clone() {
                    window.defer(cx, move |window, cx| f(window, cx));
                }
            }));
        this
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    pub fn buffer(&self) -> &EditBuffer {
        &self.buffer
    }

    /// Where the text area was last drawn, in window coordinates.
    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.layout.as_ref().map(|layout| layout.bounds)
    }

    /// Syncs a controlled value: replaces the text if it differs.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.buffer.text() != text {
            // A field whose whole text was selected keeps it selected, as when
            // a formatted value gives way to its editable form on focus.
            let all = !self.buffer.text().is_empty()
                && self.buffer.selection() == (0..self.buffer.text().len());
            self.buffer.set_text(text);
            if all {
                self.buffer.select_all();
            }
            cx.notify();
        }
    }

    pub fn set_settings(&mut self, settings: FieldSettings) {
        self.settings = settings;
    }

    /// Takes the field's pending change report: whether the user changed the
    /// text since the last call.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Takes a pending Enter (single-line fields).
    pub fn take_enter(&mut self) -> bool {
        std::mem::take(&mut self.enter_pressed)
    }

    fn editable(&self) -> bool {
        !self.settings.disabled && !self.settings.read_only
    }

    fn touched(&mut self, now: Instant, window: &Window, cx: &mut Context<Self>) {
        self.caret_since = Some(now);
        window.notify_text_selection_changed(&self.focus);
        cx.notify();
    }

    fn edited(&mut self, now: Instant, window: &Window, cx: &mut Context<Self>) {
        self.changed = true;
        self.goal_x = None;
        self.caret_since = Some(now);
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }

    fn now(cx: &App) -> Instant {
        cx.background_executor().now()
    }

    // ------------------------------------------------------------------
    // Actions

    pub(crate) fn delete(
        &mut self,
        granularity: Granularity,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editable() {
            return;
        }
        if self.buffer.delete(granularity, direction) {
            self.edited(Self::now(cx), window, cx);
        }
    }

    pub(crate) fn move_caret(
        &mut self,
        granularity: Granularity,
        direction: Direction,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.move_by(granularity, direction, extend);
        self.goal_x = None;
        self.touched(Self::now(cx), window, cx);
    }

    /// Up and Down: the same x on the visual line above or below, else the
    /// text's start or end.
    pub(crate) fn move_vertically(
        &mut self,
        direction: Direction,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(position) = self.position_for_offset(self.buffer.cursor()) else {
            return;
        };
        let Some(layout) = &self.layout else { return };
        let x = *self.goal_x.get_or_insert(position.x);
        let y = match direction {
            Direction::Backward => position.y - layout.line_height,
            Direction::Forward => position.y + layout.line_height,
        };
        let target = if y < px(0.) {
            0
        } else {
            self.offset_for_local(point(x, y + layout.line_height / 2.))
        };
        if extend {
            self.buffer.select_to(target);
        } else {
            self.buffer.move_to(target);
        }
        self.goal_x = Some(x);
        self.touched(Self::now(cx), window, cx);
    }

    pub(crate) fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_all();
        self.touched(Self::now(cx), window, cx);
    }

    pub(crate) fn copy(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        // A masked field never gives its text away.
        if self.buffer.is_collapsed() || self.settings.masked {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.buffer.selected_text().to_string(),
        ));
        if cut
            && self.editable()
            && self
                .buffer
                .delete(Granularity::Grapheme, Direction::Backward)
        {
            self.edited(Self::now(cx), window, cx);
        }
    }

    pub(crate) fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.pasted = Some(text.clone());
            self.insert(&text, window, cx);
            cx.notify();
        }
    }

    pub(crate) fn undo(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let done = if redo {
            self.buffer.redo()
        } else {
            self.buffer.undo()
        };
        if done {
            self.edited(Self::now(cx), window, cx);
        }
    }

    pub(crate) fn enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Enter commits an IME composition; it never reaches the field then
        // (upstream's IME guard). A disabled field cancels it.
        if self.buffer.marked().is_some() || self.settings.disabled {
            return;
        }
        if self.buffer.is_multiline() {
            self.insert("\n", window, cx);
        } else {
            self.enter_pressed = true;
            cx.notify();
        }
    }

    fn insert(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let text = match &self.settings.filter {
            Some(filter) => {
                let range = self
                    .buffer
                    .marked()
                    .unwrap_or_else(|| self.buffer.selection());
                filter(self.buffer.text(), range, text)
            }
            None => text.to_string(),
        };
        if self.buffer.insert(&text) {
            self.edited(Self::now(cx), window, cx);
        }
    }

    // ------------------------------------------------------------------
    // Mouse

    pub(crate) fn mouse_down(
        &mut self,
        position: Point<Pixels>,
        clicks: usize,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.offset_for_window(position);
        match clicks {
            2 => {
                let word = self.buffer.word_range_at(offset);
                self.buffer.select(word, false);
            }
            n if n >= 3 => {
                let line = if self.buffer.is_multiline() {
                    self.buffer.line_start(offset)..self.buffer.line_end(offset)
                } else {
                    0..self.buffer.text().len()
                };
                self.buffer.select(line, false);
            }
            _ if shift => self.buffer.select_to(offset),
            _ => self.buffer.move_to(offset),
        }
        self.selecting = true;
        self.goal_x = None;
        self.touched(Self::now(cx), window, cx);
    }

    pub(crate) fn mouse_drag(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selecting {
            let offset = self.offset_for_window(position);
            self.buffer.select_to(offset);
            self.touched(Self::now(cx), window, cx);
        }
    }

    pub(crate) fn mouse_up(&mut self) {
        self.selecting = false;
    }

    // ------------------------------------------------------------------
    // Display text (masking) and positions

    /// The text as shown: bullets for a masked field.
    pub(crate) fn display_text(&self) -> String {
        if self.settings.masked {
            "•".repeat(self.buffer.text().graphemes(true).count())
        } else {
            self.buffer.text().to_string()
        }
    }

    /// A buffer offset in the display text.
    pub(crate) fn offset_to_display(&self, offset: usize) -> usize {
        if self.settings.masked {
            self.buffer.text()[..offset].graphemes(true).count() * '•'.len_utf8()
        } else {
            offset
        }
    }

    /// A display offset in the buffer.
    pub(crate) fn display_to_offset(&self, offset: usize) -> usize {
        if self.settings.masked {
            let nth = offset / '•'.len_utf8();
            self.buffer
                .text()
                .grapheme_indices(true)
                .nth(nth)
                .map_or(self.buffer.text().len(), |(i, _)| i)
        } else {
            offset
        }
    }

    /// Where `offset` is, relative to the text's top-left (before scrolling).
    pub(crate) fn position_for_offset(&self, offset: usize) -> Option<Point<Pixels>> {
        let layout = self.layout.as_ref()?;
        let display = self.offset_to_display(offset);
        let mut y = px(0.);
        for (start, line) in layout.lines.iter() {
            let end = start + line.len();
            if display <= end {
                let p = line.position_for_index(display - start, layout.line_height)?;
                return Some(point(p.x, y + p.y));
            }
            y += line.size(layout.line_height).height;
        }
        None
    }

    /// The offset nearest a point relative to the text's top-left.
    pub(crate) fn offset_for_local(&self, local: Point<Pixels>) -> usize {
        let Some(layout) = &self.layout else {
            return 0;
        };
        let mut y = px(0.);
        for (start, line) in layout.lines.iter() {
            let height = line.size(layout.line_height).height;
            if local.y < y + height || std::ptr::eq(line, &layout.lines.last().unwrap().1) {
                let within = point(
                    local.x.max(px(0.)),
                    (local.y - y).max(px(0.)).min(height - px(0.5)),
                );
                let index = match line.closest_index_for_position(within, layout.line_height) {
                    Ok(i) | Err(i) => i,
                };
                return self.display_to_offset(start + index.min(line.len()));
            }
            y += height;
        }
        self.buffer.text().len()
    }

    /// The offset nearest a window position.
    pub(crate) fn offset_for_window(&self, position: Point<Pixels>) -> usize {
        let Some(layout) = &self.layout else {
            return 0;
        };
        let local = position - layout.bounds.origin + layout.scroll;
        self.offset_for_local(local)
    }
}

impl Focusable for TextEditState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for TextEditState {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.buffer.range_from_utf16(&range_utf16);
        actual_range.replace(self.buffer.range_to_utf16(&range));
        Some(self.buffer.text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.buffer.range_to_utf16(&self.buffer.selection()),
            reversed: self.buffer.is_reversed(),
        })
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.buffer.range_from_utf16(&range_utf16);
        self.buffer.select(range, false);
        self.touched(Self::now(cx), window, cx);
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.buffer.offset_to_utf16(self.buffer.text().len()))
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buffer
            .marked()
            .map(|range| self.buffer.range_to_utf16(&range))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.unmark();
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editable() {
            return;
        }
        if let Some(range) = range_utf16 {
            let range = self.buffer.range_from_utf16(&range);
            self.buffer.select(range, false);
        }
        self.insert(text, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editable() {
            return;
        }
        let range = range_utf16.map(|r| self.buffer.range_from_utf16(&r));
        // The selection within the composition comes in UTF-16 of the new text.
        let selected = selected_utf16.map(|r| {
            let to_utf8 = |utf16: usize| {
                let mut count = 0;
                for (i, ch) in text.char_indices() {
                    if count >= utf16 {
                        return i;
                    }
                    count += ch.len_utf16();
                }
                text.len()
            };
            to_utf8(r.start)..to_utf8(r.end)
        });
        self.buffer.replace_and_mark(range, text, selected);
        self.changed = true;
        self.caret_since = Some(Self::now(cx));
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let range = self.buffer.range_from_utf16(&range_utf16);
        let start = self.position_for_offset(range.start)?;
        let end = self.position_for_offset(range.end)?;
        let origin = layout.bounds.origin - layout.scroll;
        Some(Bounds::from_corners(
            origin + start,
            origin + point(end.x, end.y + layout.line_height),
        ))
    }

    fn selection_bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        self.bounds_for_range(range_utf16, bounds, window, cx)
            .into_iter()
            .collect()
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.layout.as_ref()?;
        let offset = self.offset_for_window(point);
        Some(self.buffer.offset_to_utf16(offset))
    }
}
