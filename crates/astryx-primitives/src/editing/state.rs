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
    Action, AnyElement, App, Autocapitalize, Bounds, ClipboardItem, Context, EntityInputHandler,
    FocusHandle, Focusable, KeyBinding, KeyBindingContextPredicate, KeyDownEvent, OperatingSystem,
    Pixels, Point, Subscription, TextInputAction, TextInputConfiguration, TextInputMode,
    UTF16Selection, Window, WrappedLine, actions, point, px,
};
use unicode_segmentation::UnicodeSegmentation;
use web_time::Instant;

use super::buffer::{Direction, EditBuffer, Granularity, OBJECT, ObjectId};

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

/// Draws an inline object: its element, laid out at its max-content width and
/// one line high.
pub type ObjectRenderFn = Rc<dyn Fn(ObjectId, &mut Window, &mut App) -> AnyElement>;
/// The text an inline object copies as.
pub type ObjectTextFn = Rc<dyn Fn(ObjectId, &App) -> String>;
/// Takes over a paste: given the clipboard's content, the field inserts
/// nothing itself.
pub type PasteOverrideFn = Rc<dyn Fn(ClipboardItem, &mut Window, &mut App)>;

/// How a field shows and copies its inline objects (see
/// [`super::buffer::OBJECT`]): set by its component every render.
#[derive(Clone)]
pub struct InlineObjects {
    pub render: ObjectRenderFn,
    pub text: ObjectTextFn,
}

/// What a no-break space is in the display text: each object shows as one,
/// widened to the object's box.
pub(crate) const OBJECT_STAND_IN: char = '\u{A0}';

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
    /// Upstream `hasSpellCheck`: misspelled words get a wavy underline once
    /// the field has had focus, as in browsers. Never in a masked field.
    pub spell_check: bool,
    /// What a touch keyboard offers for the field.
    pub keyboard: KeyboardHints,
}

/// A field's touch keyboard (upstream `inputMode`, `enterKeyHint`,
/// `autoCapitalize` and `hasAutoCorrect`); unset ones are the platform's
/// defaults for a text field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyboardHints {
    pub input_mode: Option<TextInputMode>,
    pub enter_key_hint: Option<TextInputAction>,
    pub autocapitalize: Option<Autocapitalize>,
    pub autocorrect: Option<bool>,
}

impl FieldSettings {
    /// What the platform's text input is asked for: no assistance in a
    /// masked field; otherwise the hints, else a text field's defaults
    /// (sentence capitals except for addresses, corrections, suggestions).
    pub fn input_configuration(&self) -> TextInputConfiguration {
        let hints = self.keyboard;
        if self.masked {
            return TextInputConfiguration {
                input_mode: hints.input_mode.unwrap_or_default(),
                input_action: hints.enter_key_hint.unwrap_or_default(),
                spellcheck: Some(false),
                ..TextInputConfiguration::default()
            };
        }
        let address = matches!(
            hints.input_mode,
            Some(TextInputMode::Url | TextInputMode::Email)
        );
        TextInputConfiguration {
            autocorrect: hints.autocorrect.unwrap_or(!address),
            autocapitalize: hints.autocapitalize.unwrap_or(if address {
                Autocapitalize::None
            } else {
                Autocapitalize::Sentences
            }),
            suggestions: true,
            input_action: hints.enter_key_hint.unwrap_or_default(),
            spellcheck: Some(self.spell_check),
            input_mode: hints.input_mode.unwrap_or_default(),
        }
    }
}

/// What a field's context menu can offer, from
/// [`TextEditState::context_menu_at`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldMenu {
    /// The misspelled word under the pointer: its range, its text and the
    /// checker's suggestions.
    pub misspelling: Option<(Range<usize>, String, Vec<String>)>,
    pub can_cut: bool,
    pub can_copy: bool,
    pub can_paste: bool,
    pub can_select_all: bool,
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
    /// Inline objects' drawing and copied text.
    pub inline_objects: Option<InlineObjects>,
    /// Takes every paste instead of the field (a chat composer's paste rules):
    /// called after the paste with the clipboard's content.
    pub on_paste_override: Option<PasteOverrideFn>,
    /// A paste waiting for `on_paste_override`.
    paste_waiting: Option<ClipboardItem>,
    /// The misspelled ranges last found, and the text they were found in.
    spelling: Option<(String, Vec<Range<usize>>)>,
    /// Whether the field has had focus: browsers check its text from then on.
    spell_armed: bool,
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
            inline_objects: None,
            on_paste_override: None,
            paste_waiting: None,
            spelling: None,
            spell_armed: false,
            _subscriptions: vec![keys],
        }
    }

    /// The misspelled ranges of the text to underline: none until the field
    /// has had focus, and not the word just typed while the caret ends it.
    pub(crate) fn spelling_errors(&mut self, focused: bool, cx: &mut App) -> Vec<Range<usize>> {
        if !self.settings.spell_check || self.settings.masked {
            return Vec::new();
        }
        self.spell_armed |= focused;
        if !self.spell_armed {
            return Vec::new();
        }
        let stale = self
            .spelling
            .as_ref()
            .is_none_or(|(text, _)| text != self.buffer.text());
        if stale {
            let Some(checker) = crate::spell::spell_checker(cx) else {
                return Vec::new();
            };
            let text = self.buffer.text().to_string();
            let errors = checker.check(&text);
            self.spelling = Some((text, errors));
        }
        let caret = self.buffer.cursor();
        let marked = self.buffer.marked();
        self.spelling
            .as_ref()
            .map(|(_, errors)| {
                errors
                    .iter()
                    .filter(|r| !(focused && r.end == caret))
                    .filter(|r| {
                        marked
                            .as_ref()
                            .is_none_or(|m| r.end <= m.start || r.start >= m.end)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
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
                let (waiting, take_paste) = entity.update(cx, |s, _| {
                    (s.paste_waiting.take(), s.on_paste_override.clone())
                });
                if let (Some(text), Some(f)) = (waiting, take_paste) {
                    f(text, window, cx);
                }
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

    /// Selects all the text (the Select all command).
    pub fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.select_all();
        self.touched(Self::now(cx), window, cx);
    }

    /// Copies the selection to the clipboard, and with `cut` deletes it (the
    /// Copy and Cut commands). A masked field copies nothing.
    pub fn copy(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        // A masked field never gives its text away.
        if self.buffer.is_collapsed() || self.settings.masked {
            return;
        }
        let copied = self.plain_text(self.buffer.selection(), cx);
        cx.write_to_clipboard(ClipboardItem::new_string(copied));
        if cut
            && self.editable()
            && self
                .buffer
                .delete(Granularity::Grapheme, Direction::Backward)
        {
            self.edited(Self::now(cx), window, cx);
        }
    }

    /// Inserts the clipboard's text (the Paste command).
    pub fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        if self.on_paste_override.is_some() {
            if let Some(item) = cx.read_from_clipboard() {
                self.paste_waiting = Some(item);
                cx.notify();
            }
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
    // Editing from the component (reported like the user's own edits)

    /// The text of `range` with each inline object as its copied text.
    pub fn plain_text(&self, range: Range<usize>, cx: &App) -> String {
        let text = &self.buffer.text()[range.clone()];
        let Some(objects) = self
            .inline_objects
            .as_ref()
            .filter(|_| text.contains(OBJECT))
        else {
            return text.to_string();
        };
        let mut out = String::with_capacity(text.len());
        let ids: Vec<(usize, ObjectId)> = self.buffer.object_offsets();
        for (i, ch) in text.char_indices() {
            if ch == OBJECT {
                let at = range.start + i;
                if let Some((_, id)) = ids.iter().find(|(offset, _)| *offset == at) {
                    out.push_str(&(objects.text)(*id, cx));
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    /// Types `text` at the caret (over the selection), as the user would.
    pub fn insert_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.insert(text, window, cx);
    }

    /// Puts inline object `id` at the caret (over the selection).
    pub fn insert_object(&mut self, id: ObjectId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        if self.buffer.insert_object(id) {
            self.edited(Self::now(cx), window, cx);
        }
    }

    /// Replaces `range` with `text` holding `objects` (see
    /// [`EditBuffer::replace_rich`]), the caret after it: one undo step.
    pub fn replace_rich(
        &mut self,
        range: Range<usize>,
        text: &str,
        objects: &[ObjectId],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editable() {
            return;
        }
        if self.buffer.replace_rich(range, text, objects) {
            self.edited(Self::now(cx), window, cx);
        }
    }

    /// Replaces the whole content, objects included (see
    /// [`EditBuffer::set_content`]): a recalled draft. With `report`, it is
    /// reported as a change.
    pub fn set_content(
        &mut self,
        text: &str,
        objects: &[ObjectId],
        report: bool,
        cx: &mut Context<Self>,
    ) {
        self.buffer.set_content(text, objects);
        self.goal_x = None;
        self.caret_since = Some(Self::now(cx));
        if report {
            self.changed = true;
        }
        cx.notify();
    }

    /// [`Self::replace_rich`] from outside any window (speech arriving in the
    /// background): the platform's input method is not told.
    pub fn replace_rich_detached(
        &mut self,
        range: Range<usize>,
        text: &str,
        objects: &[ObjectId],
        cx: &mut Context<Self>,
    ) {
        if !self.editable() {
            return;
        }
        if self.buffer.replace_rich(range, text, objects) {
            self.changed = true;
            self.goal_x = None;
            self.caret_since = Some(Self::now(cx));
            cx.notify();
        }
    }

    /// Where `offset` was last drawn, in window coordinates: a line-high caret.
    pub fn offset_bounds(&self, offset: usize) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let at = self.position_for_offset(offset.min(self.buffer.text().len()))?;
        let origin = layout.bounds.origin - layout.scroll + at;
        Some(Bounds::new(origin, gpui::size(px(1.), layout.line_height)))
    }

    /// Selects `range`, the caret at its end (or start, `reversed`).
    pub fn select_range(
        &mut self,
        range: Range<usize>,
        reversed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.select(range, reversed);
        self.goal_x = None;
        self.touched(Self::now(cx), window, cx);
    }

    // ------------------------------------------------------------------
    // The context menu

    /// Readies a context menu invoked at `position` (window coordinates;
    /// `None` from the keyboard, at the caret), as a browser's text field
    /// does: a press on a misspelled word selects it, a press outside the
    /// selection moves the caret there. Returns what the menu can offer.
    pub fn context_menu_at(
        &mut self,
        position: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> FieldMenu {
        let offset = match position {
            Some(position) => self.offset_for_window(position),
            None => self.buffer.cursor(),
        };
        let misspelling = self.misspelling_at(offset, cx);
        if position.is_some() {
            let selection = self.buffer.selection();
            let inside =
                !self.buffer.is_collapsed() && offset >= selection.start && offset <= selection.end;
            match &misspelling {
                Some((range, _, _)) => self.buffer.select(range.clone(), false),
                None if !inside => self.buffer.move_to(offset),
                None => {}
            }
            self.touched(Self::now(cx), window, cx);
        }
        let editable = self.editable();
        let selected = !self.buffer.is_collapsed() && !self.settings.masked;
        FieldMenu {
            misspelling: misspelling.filter(|_| editable),
            can_cut: selected && editable,
            can_copy: selected,
            can_paste: editable,
            can_select_all: !self.buffer.text().is_empty(),
        }
    }

    /// The misspelled word containing `offset`, with the checker's
    /// suggestions.
    fn misspelling_at(
        &mut self,
        offset: usize,
        cx: &mut App,
    ) -> Option<(Range<usize>, String, Vec<String>)> {
        if !self.settings.spell_check || self.settings.masked {
            return None;
        }
        let checker = crate::spell::spell_checker(cx)?;
        let stale = self
            .spelling
            .as_ref()
            .is_none_or(|(text, _)| text != self.buffer.text());
        if stale {
            let text = self.buffer.text().to_string();
            let errors = checker.check(&text);
            self.spelling = Some((text, errors));
        }
        let (_, errors) = self.spelling.as_ref()?;
        let range = errors
            .iter()
            .find(|r| r.start <= offset && offset <= r.end)?
            .clone();
        let word = self.buffer.text()[range.clone()].to_string();
        let suggestions = checker.suggest(&word);
        Some((range, word, suggestions))
    }

    /// Replaces `range` with `with`, as typing would (undoable, reported):
    /// a spelling suggestion picked from the context menu.
    pub fn replace_range(
        &mut self,
        range: Range<usize>,
        with: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if range.end > self.buffer.text().len() {
            return;
        }
        self.buffer.select(range, false);
        self.insert(with, window, cx);
        cx.notify();
    }

    /// Adds `word` to the spell checker's dictionary, so no field marks it.
    pub fn add_to_dictionary(&mut self, word: &str, cx: &mut Context<Self>) {
        if let Some(checker) = crate::spell::spell_checker(cx) {
            checker.add(word);
        }
        self.spelling = None;
        cx.notify();
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
        } else if self.buffer.objects().is_empty() {
            self.buffer.text().to_string()
        } else {
            self.buffer.text().replace(OBJECT, "\u{A0}")
        }
    }

    /// A buffer offset in the display text.
    pub(crate) fn offset_to_display(&self, offset: usize) -> usize {
        if self.settings.masked {
            self.buffer.text()[..offset].graphemes(true).count() * '•'.len_utf8()
        } else if self.buffer.objects().is_empty() {
            offset
        } else {
            let shrink = OBJECT.len_utf8() - OBJECT_STAND_IN.len_utf8();
            let offset = offset.min(self.buffer.text().len());
            offset - self.buffer.text()[..offset].matches(OBJECT).count() * shrink
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
        } else if self.buffer.objects().is_empty() {
            offset
        } else {
            let mut display = 0;
            for (i, ch) in self.buffer.text().char_indices() {
                if display >= offset {
                    return i;
                }
                display += if ch == OBJECT {
                    OBJECT_STAND_IN.len_utf8()
                } else {
                    ch.len_utf8()
                };
            }
            self.buffer.text().len()
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
    fn text_input_configuration(
        &mut self,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> TextInputConfiguration {
        self.settings.input_configuration()
    }

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
