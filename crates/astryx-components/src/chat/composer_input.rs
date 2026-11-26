//! ChatComposerInput, upstream `Chat/ChatComposerInput.tsx` with
//! `useChatComposerTokens.ts` and `chatComposerSelection.ts`: the composer's rich text
//! field. Text and atomic tokens share its lines; triggers open suggestion menus at the
//! caret; Enter sends and Shift+Enter breaks the line; the arrows at the draft's ends
//! recall sent messages; long pastes become tokens and files go to `on_files`.
//!
//! Upstream edits a `contentEditable` and portals each token's React content into the
//! span it created (`TokenPortal`). Here the text is the editing engine's
//! (`astryx_primitives::editing`): each token is one object character in the buffer
//! with an id, the input keeps the id's token in its table (the portal's meaning), and
//! the engine draws the token's element in the line at the character's place. The
//! DOM selection helpers of `chatComposerSelection.ts` become buffer offsets.
//!
//! Upstream's `keyboardSurface.ts` only changes how a touch focus reveals the field in a
//! mobile browser; GPUI has no page to scroll, so focus here is a plain focus. The
//! virtual-keyboard lift (`useChatKeyboard`) is carried to the touch milestone (M11).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::editing::{
    EditorStyle, FieldSettings, InlineObjects, KeyboardHints, OBJECT, ObjectId, TextEditState,
    text_editor,
};
use astryx_primitives::{
    EscapeBehavior, LayerAlign, LayerOptions, LayerPosition, Placement, anchored_layer,
    prefers_reduced_motion,
};
use astryx_theme::{ThemeStyled as _, current};
use gpui::Refineable as _;
use gpui::prelude::*;
use gpui::{
    AnyWindowHandle, App, ClipboardEntry, ElementId, Entity, ExternalPaths, FocusHandle,
    Focusable as _, KeyDownEvent, Role, SharedString, StyleRefinement, TextInputAction, Window,
    div, px,
};

use super::composer_token::{ChatComposerToken, ChatPasteAsToken, ChatPastedTextToken};
use super::context::{
    ChatComposerContext, ChatComposerInputControl, ChatValueFn, use_chat_composer_context,
};
use super::trigger_menu::{ChatComposerTrigger, ChatComposerTriggerResult, MenuState, TriggerMenu};
use crate::input::PickedFile;

/// Upstream `onFiles`: the pasted or dropped files, in order.
pub type FilesFn = Rc<dyn Fn(Vec<PickedFile>, &mut Window, &mut App)>;
/// Upstream `onKeyDown`: every key first (after an open trigger menu). Calling
/// `window.prevent_default()` skips the built-in Enter and history handling; calling
/// `cx.stop_propagation()` also keeps the key from the field (no line break).
pub type KeyDownFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;
/// Upstream `onPaste`: told a paste's plain text first; returning true takes it over.
pub type PasteFn = Rc<dyn Fn(&str, &mut Window, &mut App) -> bool>;

/// Upstream `LINE_HEIGHT_PX`.
const LINE_HEIGHT: f32 = 22.;
/// The object id of dictation's interim words (upstream's
/// `data-astryx-dictation-interim` span).
const INTERIM: ObjectId = u64::MAX;
const NBSP: char = '\u{A0}';

/// What the input keeps between frames: the token table (upstream's token portals),
/// the sent-message history and the controlled-value bookkeeping.
pub(crate) struct InputModel {
    tokens: HashMap<ObjectId, ChatComposerToken>,
    next_id: ObjectId,
    history: Vec<String>,
    /// Upstream `historyIndexRef`: `None` while editing the draft.
    history_index: Option<usize>,
    /// The draft set aside while stepping through history.
    draft_before_history: String,
    /// Upstream `pendingEchoValueRef`: the value last emitted, which a controlled
    /// parent will echo back.
    pending_echo: Option<SharedString>,
    /// Bumped on every emitted change (upstream `emitChangeVersionRef`).
    emit_version: u64,
    /// Bumped when the draft is replaced from outside typing; ends an Escape dismissal.
    draft: Rc<Cell<u64>>,
    /// Dictation's interim words, drawn after the text.
    interim: Option<SharedString>,
}

impl InputModel {
    fn new() -> Self {
        InputModel {
            tokens: HashMap::new(),
            next_id: 1,
            history: Vec::new(),
            history_index: None,
            draft_before_history: String::new(),
            pending_echo: None,
            emit_version: 0,
            draft: Rc::new(Cell::new(0)),
            interim: None,
        }
    }

    fn add(&mut self, token: ChatComposerToken) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.tokens.insert(id, token);
        id
    }
}

/// Upstream `serialize`: the text with each token as its value (and dictation's interim
/// words as they read).
fn serialize(editor: &Entity<TextEditState>, model: &Entity<InputModel>, cx: &App) -> String {
    let buffer = editor.read(cx).buffer();
    let model = model.read(cx);
    let ids = buffer.objects();
    let mut nth = 0;
    let mut out = String::with_capacity(buffer.text().len());
    for ch in buffer.text().chars() {
        if ch == OBJECT {
            match ids.get(nth) {
                Some(&INTERIM) => out.push_str(model.interim.as_deref().unwrap_or("")),
                Some(id) => {
                    if let Some(token) = model.tokens.get(id) {
                        out.push_str(token.value());
                    }
                }
                None => {}
            }
            nth += 1;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Whether the draft is empty: no tokens, and nothing but white space.
fn is_empty(editor: &Entity<TextEditState>, model: &Entity<InputModel>, cx: &App) -> bool {
    editor.read(cx).buffer().objects().is_empty() && serialize(editor, model, cx).trim().is_empty()
}

/// Splits a serialized draft into text and tokens again (upstream `restoreValue`): each
/// run between white space that a trigger deserializes to a token of the same value.
fn deserialize(
    value: &str,
    triggers: &[ChatComposerTrigger],
    model: &Entity<InputModel>,
    cx: &mut App,
) -> (String, Vec<ObjectId>) {
    let mut text = String::new();
    let mut ids = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, text: &mut String, ids: &mut Vec<ObjectId>, cx: &mut App| {
        if word.is_empty() {
            return;
        }
        let token = triggers.iter().find_map(|t| {
            t.deserialize
                .as_ref()
                .and_then(|f| f(word))
                .filter(|token| token.value().as_ref() == word.as_str())
        });
        match token {
            Some(token) => {
                ids.push(model.update(cx, |m, _| m.add(token)));
                text.push(OBJECT);
            }
            None => text.push_str(word),
        }
        word.clear();
    };
    for ch in value.chars() {
        if ch.is_whitespace() {
            flush(&mut word, &mut text, &mut ids, cx);
            text.push(ch);
        } else {
            word.push(ch);
        }
    }
    flush(&mut word, &mut text, &mut ids, cx);
    (text, ids)
}

/// The resolved props and parts of one frame, shared by the input's handlers.
struct Parts {
    id: SharedString,
    editor: Entity<TextEditState>,
    model: Entity<InputModel>,
    menu: TriggerMenu,
    triggers: Rc<[ChatComposerTrigger]>,
    on_change: Option<ChatValueFn>,
    on_submit: Option<ChatValueFn>,
    on_key_down: Option<KeyDownFn>,
    on_paste: Option<PasteFn>,
    on_files: Option<FilesFn>,
    paste_as_token: Option<ChatPasteAsToken>,
    history: bool,
    disabled: bool,
    window: AnyWindowHandle,
}

impl Parts {
    /// Upstream `emitChange`: reports the serialized draft (empty when only white
    /// space is left) through the active channel.
    fn emit_change(&self, window: &mut Window, cx: &mut App) {
        if self.disabled {
            return;
        }
        let text = serialize(&self.editor, &self.model, cx);
        let empty = text.trim().is_empty() && self.editor.read(cx).buffer().objects().is_empty();
        let next: SharedString = if empty { "".into() } else { text.into() };
        self.model.update(cx, |m, _| {
            m.pending_echo = Some(next.clone());
            m.emit_version += 1;
        });
        if let Some(f) = &self.on_change {
            f(next, window, cx);
        }
    }

    /// Replaces the draft (a recalled message, a controlled value, a send's clear).
    fn restore(&self, value: &str, report: bool, cx: &mut App) {
        let (text, ids) = deserialize(value, &self.triggers, &self.model, cx);
        self.model.update(cx, |m, _| {
            m.tokens.retain(|id, _| ids.contains(id));
            m.interim = None;
            m.draft.set(m.draft.get() + 1);
        });
        self.editor
            .update(cx, |s, cx| s.set_content(&text, &ids, report, cx));
    }

    /// Upstream `insertToken`: the token and a no-break space after it at the caret,
    /// the caret after the space. Returns the token's id.
    fn insert_token(
        &self,
        token: ChatComposerToken,
        range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<ObjectId> {
        if self.disabled {
            return None;
        }
        let id = self.model.update(cx, |m, _| m.add(token));
        self.editor.update(cx, |s, cx| {
            let range = range.unwrap_or_else(|| s.buffer().selection());
            s.replace_rich(range, &format!("{OBJECT}{NBSP}"), &[id], window, cx);
        });
        Some(id)
    }

    fn insert_text(
        &self,
        text: &str,
        range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.disabled {
            return;
        }
        self.editor.update(cx, |s, cx| {
            let range = range.unwrap_or_else(|| s.buffer().selection());
            s.replace_rich(range, text, &[], window, cx);
        });
    }

    /// Upstream `expandToken`: the token becomes its value as text (with its trailing
    /// no-break space, when that is all that follows it), the caret after it.
    fn expand_token(&self, id: ObjectId, window: &mut Window, cx: &mut App) {
        if self.disabled {
            return;
        }
        let Some(token) = self.model.read(cx).tokens.get(&id).cloned() else {
            return;
        };
        let range = {
            let buffer = self.editor.read(cx).buffer();
            let Some((offset, _)) = buffer.object_offsets().into_iter().find(|(_, o)| *o == id)
            else {
                return;
            };
            let mut end = offset + OBJECT.len_utf8();
            let rest = &buffer.text()[end..];
            if let Some(after) = rest.strip_prefix(NBSP)
                && (after.is_empty() || after.starts_with(['\n', OBJECT]))
            {
                end += NBSP.len_utf8();
            }
            offset..end
        };
        self.model.update(cx, |m, _| {
            m.tokens.remove(&id);
        });
        self.editor.update(cx, |s, cx| {
            s.replace_rich(range, token.value(), &[], window, cx)
        });
    }

    /// Enter: sends the trimmed draft and clears it (FR2).
    fn submit(&self, window: &mut Window, cx: &mut App) {
        let text = serialize(&self.editor, &self.model, cx).trim().to_string();
        if text.is_empty() {
            return;
        }
        if self.history {
            self.model.update(cx, |m, _| {
                m.history.push(text.clone());
                m.history_index = None;
                m.draft_before_history.clear();
            });
        }
        // Submitting ends any trigger query, a dismissed one included.
        self.menu.reset(window, cx);
        if let Some(f) = &self.on_submit {
            f(text.into(), window, cx);
        }
        self.restore("", false, cx);
        self.model
            .update(cx, |m, _| m.pending_echo = Some("".into()));
        if let Some(f) = &self.on_change {
            f("".into(), window, cx);
        }
    }

    /// The arrows at the draft's boundaries step through sent messages (FR3, FR8).
    fn recall(&self, up: bool, window: &mut Window, cx: &mut App) -> bool {
        let (at_start, at_end, collapsed) = {
            let buffer = self.editor.read(cx).buffer();
            let selection = buffer.selection();
            (
                selection.start == 0,
                selection.end == buffer.text().len(),
                buffer.is_collapsed(),
            )
        };
        let can = if up {
            at_start && (collapsed || at_end)
        } else {
            at_end && (collapsed || at_start)
        };
        if !can || self.model.read(cx).history.is_empty() {
            return false;
        }
        let text = serialize(&self.editor, &self.model, cx);
        let (index, history_len) = {
            let m = self.model.read(cx);
            (m.history_index, m.history.len())
        };
        let next = if up {
            if index.is_none() {
                self.model
                    .update(cx, |m, _| m.draft_before_history = text.clone());
            }
            Some(index.map_or(history_len - 1, |i| i.saturating_sub(1)))
        } else {
            match index {
                None => return false,
                Some(i) if i + 1 >= history_len => None,
                Some(i) => Some(i + 1),
            }
        };
        let (value, select) = {
            let m = self.model.read(cx);
            match next {
                Some(i) => (m.history[i].clone(), true),
                None => (
                    m.draft_before_history.clone(),
                    !m.draft_before_history.is_empty(),
                ),
            }
        };
        self.model.update(cx, |m, _| m.history_index = next);
        self.restore(&value, true, cx);
        if select {
            self.editor.update(cx, |s, cx| {
                let len = s.text().len();
                s.select_range(0..len, false, window, cx);
            });
        }
        true
    }

    /// FR18: Backspace with the caret after a token's trailing no-break space, when no
    /// text follows that space, removes the token and the space in one step.
    fn token_backspace(&self, window: &mut Window, cx: &mut App) -> bool {
        let range = {
            let buffer = self.editor.read(cx).buffer();
            if !buffer.is_collapsed() {
                return false;
            }
            let caret = buffer.cursor();
            let text = buffer.text();
            let pair = format!("{OBJECT}{NBSP}");
            let after = &text[caret..];
            if !text[..caret].ends_with(&pair)
                || !(after.is_empty() || after.starts_with(['\n', OBJECT]))
            {
                return false;
            }
            caret - pair.len()..caret
        };
        self.editor
            .update(cx, |s, cx| s.replace_rich(range, "", &[], window, cx));
        true
    }

    /// The field's key handling, in upstream's order (ORD3): the trigger menu, the
    /// caller's `on_key_down`, the token Backspace, Enter, then history.
    fn key_down(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App) {
        if self.disabled {
            return;
        }
        if self.menu.handle_key(&event.keystroke, window, cx) {
            cx.stop_propagation();
            return;
        }
        if let Some(f) = &self.on_key_down {
            f(event, window, cx);
            if window.default_prevented() {
                return;
            }
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if key == "backspace" && !modifiers.modified() && self.token_backspace(window, cx) {
            cx.stop_propagation();
            return;
        }
        if key == "enter" && !modifiers.shift {
            // Never mid-composition: the IME's Enter commits a candidate.
            if self.editor.read(cx).buffer().marked().is_some() {
                return;
            }
            cx.stop_propagation();
            self.submit(window, cx);
            return;
        }
        if self.history && (key == "up" || key == "down") && self.recall(key == "up", window, cx) {
            cx.stop_propagation();
        }
    }

    /// Upstream `handlePaste` (ORD4, ORD5): files first, then the caller's
    /// `on_paste`, then paste-as-token, then plain insertion.
    fn paste(&self, item: gpui::ClipboardItem, window: &mut Window, cx: &mut App) {
        if self.disabled {
            return;
        }
        let files: Vec<PickedFile> = item
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::ExternalPaths(paths) => Some(paths.paths().to_vec()),
                _ => None,
            })
            .flatten()
            .map(PickedFile::from_path)
            .collect();
        if !files.is_empty() {
            if let Some(f) = &self.on_files {
                f(files, window, cx);
            }
            return;
        }
        let Some(text) = item.text() else {
            return;
        };
        let before = self.model.read(cx).emit_version;
        let serialized = serialize(&self.editor, &self.model, cx);
        if let Some(f) = &self.on_paste
            && f(&text, window, cx)
        {
            // Emit once, unless the caller's own edit is about to.
            let edited = serialize(&self.editor, &self.model, cx) != serialized;
            if self.model.read(cx).emit_version == before && !edited {
                self.emit_change(window, cx);
            }
            return;
        }
        if let Some(pat) = &self.paste_as_token
            && pat.on_paste(&text, Some(&self.handle_ref()), window, cx)
        {
            return;
        }
        self.insert_text(&text, None, window, cx);
    }

    fn handle_ref(&self) -> ChatComposerInputHandle {
        // A handle to these parts for paste-as-token, which inserts through one.
        ChatComposerInputHandle {
            inner: Rc::new(RefCell::new(Some(Rc::new(self.shallow())))),
        }
    }

    fn shallow(&self) -> Parts {
        Parts {
            id: self.id.clone(),
            editor: self.editor.clone(),
            model: self.model.clone(),
            menu: self.menu.clone(),
            triggers: self.triggers.clone(),
            on_change: self.on_change.clone(),
            on_submit: self.on_submit.clone(),
            on_key_down: self.on_key_down.clone(),
            on_paste: self.on_paste.clone(),
            on_files: self.on_files.clone(),
            paste_as_token: None,
            history: self.history,
            disabled: self.disabled,
            window: self.window,
        }
    }

    /// Dictation's interim words: drawn dimmed and italic after the text, or cleared.
    fn set_interim(
        &self,
        interim: Option<SharedString>,
        window: Option<&mut Window>,
        cx: &mut App,
    ) {
        let had = self.model.read(cx).interim.is_some();
        self.model.update(cx, |m, _| m.interim = interim.clone());
        let range = {
            let buffer = self.editor.read(cx).buffer();
            buffer
                .object_offsets()
                .into_iter()
                .find(|(_, id)| *id == INTERIM)
                .map(|(offset, _)| offset..offset + OBJECT.len_utf8())
        };
        let selection = self.editor.read(cx).buffer().selection();
        let edit = |s: &mut TextEditState,
                    window: Option<&mut Window>,
                    cx: &mut gpui::Context<TextEditState>| {
            let (range, text, ids): (Range<usize>, String, Vec<ObjectId>) = match (&range, &interim)
            {
                (Some(r), None) => (r.clone(), String::new(), Vec::new()),
                (None, Some(_)) => {
                    let end = s.text().len();
                    (end..end, OBJECT.to_string(), vec![INTERIM])
                }
                _ => return,
            };
            match window {
                Some(window) => {
                    s.replace_rich(range, &text, &ids, window, cx);
                    s.select_range(selection.clone(), false, window, cx);
                }
                None => s.replace_rich_detached(range, &text, &ids, cx),
            }
        };
        if had && interim.is_some() {
            // Only the words changed: redraw.
            self.editor.update(cx, |_, cx| cx.notify());
            return;
        }
        self.editor.update(cx, |s, cx| edit(s, window, cx));
    }
}

/// Upstream `ChatComposerInputHandle` (`handleRef`): drives an input from outside.
/// Create one, give it to [`ChatComposerInput::handle`], and call it after the input
/// has rendered.
#[derive(Clone, Default)]
pub struct ChatComposerInputHandle {
    inner: Rc<RefCell<Option<Rc<Parts>>>>,
}

impl ChatComposerInputHandle {
    pub fn new() -> Self {
        Self::default()
    }

    fn parts(&self) -> Option<Rc<Parts>> {
        self.inner.borrow().clone()
    }

    /// Upstream `insertToken`: a token at the caret, reported as one change. Returns its
    /// id, or `None` when the input is disabled or not drawn yet.
    pub fn insert_token(
        &self,
        token: ChatComposerToken,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<ObjectId> {
        self.parts()?.insert_token(token, None, window, cx)
    }

    /// Upstream `expandToken`: the token becomes its value as editable text.
    pub fn expand_token(&self, id: ObjectId, window: &mut Window, cx: &mut App) {
        if let Some(parts) = self.parts() {
            parts.expand_token(id, window, cx);
        }
    }

    /// Upstream `insertText`: plain text at the caret.
    pub fn insert_text(&self, text: &str, window: &mut Window, cx: &mut App) {
        if let Some(parts) = self.parts() {
            parts.insert_text(text, None, window, cx);
        }
    }

    /// Upstream `focus`: focuses the field, keeping its caret.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        if let Some(parts) = self.parts() {
            let focus = parts.editor.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
    }

    /// The editing state, for tests.
    #[cfg(test)]
    pub(crate) fn editor(&self) -> Option<Entity<TextEditState>> {
        self.parts().map(|p| p.editor.clone())
    }

    /// Upstream `getValue`: the serialized draft.
    pub fn value(&self, cx: &App) -> SharedString {
        self.parts()
            .map(|p| serialize(&p.editor, &p.model, cx).into())
            .unwrap_or_default()
    }
}

impl super::dictation::DictationTarget for ChatComposerInputHandle {
    fn focus(&self, window: Option<&mut Window>, cx: &mut App) {
        let Some(parts) = self.parts() else { return };
        let focus = parts.editor.read(cx).focus_handle(cx);
        match window {
            Some(window) => window.focus(&focus, cx),
            None => {
                parts
                    .window
                    .update(cx, |_, window, cx| window.focus(&focus, cx))
                    .ok();
            }
        }
    }

    fn insert_text(&self, text: &str, cx: &mut App) {
        let Some(parts) = self.parts() else { return };
        let done = parts.window.update(cx, |_, window, cx| {
            parts.insert_text(text, None, window, cx)
        });
        if done.is_err() && !parts.disabled {
            parts.editor.update(cx, |s, cx| {
                let range = s.buffer().selection();
                s.replace_rich_detached(range, text, &[], cx);
            });
        }
    }

    fn set_interim(&self, interim: Option<SharedString>, cx: &mut App) {
        let Some(parts) = self.parts() else { return };
        let window = parts.window;
        let interim2 = interim.clone();
        let done = window.update(cx, |_, window, cx| {
            parts.set_interim(interim2, Some(window), cx)
        });
        if done.is_err() {
            parts.set_interim(interim, None, cx);
        }
    }
}

/// Upstream `ChatComposerInput`.
///
/// ```ignore
/// ChatComposerInput::new("input")
///     .triggers([ChatComposerTrigger::new('@', static_source(people), |item| {
///         ChatComposerToken::badge(format!("@{}", item.id()), item.label()).into()
///     })])
/// ```
#[derive(IntoElement)]
pub struct ChatComposerInput {
    id: ElementId,
    handle: Option<ChatComposerInputHandle>,
    value: Option<SharedString>,
    on_change: Option<ChatValueFn>,
    placeholder: Option<SharedString>,
    max_rows: usize,
    triggers: Vec<ChatComposerTrigger>,
    debounce: Duration,
    history: bool,
    label: Option<SharedString>,
    disabled: Option<bool>,
    on_paste: Option<PasteFn>,
    /// `None`: the default (pastes over 200 characters); `Some(None)`: off.
    paste_as_token: Option<Option<ChatPasteAsToken>>,
    on_files: Option<FilesFn>,
    on_submit: Option<ChatValueFn>,
    on_key_down: Option<KeyDownFn>,
    enter_key_hint: TextInputAction,
    style: StyleRefinement,
}

impl ChatComposerInput {
    pub fn new(id: impl Into<ElementId>) -> Self {
        ChatComposerInput {
            id: id.into(),
            handle: None,
            value: None,
            on_change: None,
            placeholder: None,
            max_rows: 8,
            triggers: Vec::new(),
            debounce: Duration::from_millis(150),
            history: true,
            label: None,
            disabled: None,
            on_paste: None,
            paste_as_token: None,
            on_files: None,
            on_submit: None,
            on_key_down: None,
            enter_key_hint: TextInputAction::Send,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `value`: controlled; wins over a ChatComposer's value (FR4).
        ?value: SharedString,
        /// Upstream `placeholder`: else the composer's, else "Type a message…".
        ?placeholder: SharedString,
        /// Upstream `maxRows`: rows before it scrolls (8).
        max_rows: usize,
        /// Upstream `debounceMs`: the wait before an async trigger search (150 ms).
        debounce: Duration,
        /// Upstream `label`: the accessible name, "Message input" by default.
        ?label: SharedString,
        /// Upstream `isDisabled`: else the composer's.
        ?disabled: bool,
        /// Upstream `enterKeyHint`: the touch keyboard's Return key label (`Send`);
        /// Enter sends whatever it says (FR17).
        enter_key_hint: TextInputAction,
    }

    /// Upstream `hasHistory`: the arrows at the draft's ends recall sent messages
    /// (on by default).
    pub fn history(mut self, history: bool) -> Self {
        self.history = history;
        self
    }

    /// Upstream `handleRef`.
    pub fn handle(mut self, handle: ChatComposerInputHandle) -> Self {
        self.handle = Some(handle);
        self
    }

    /// Upstream `onChange`.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `triggers`: `@` mentions, `/` commands and the like.
    pub fn triggers(mut self, triggers: impl IntoIterator<Item = ChatComposerTrigger>) -> Self {
        self.triggers = triggers.into_iter().collect();
        self
    }

    /// Upstream `onPaste`: told the plain text first; return true to take it over.
    pub fn on_paste(mut self, f: impl Fn(&str, &mut Window, &mut App) -> bool + 'static) -> Self {
        self.on_paste = Some(Rc::new(f));
        self
    }

    /// Upstream `pasteAsToken`: a [`ChatPasteAsToken`] replaces the default (pastes over
    /// 200 characters become tokens).
    pub fn paste_as_token(mut self, paste: ChatPasteAsToken) -> Self {
        self.paste_as_token = Some(Some(paste));
        self
    }

    /// Upstream `pasteAsToken={false}`: long pastes stay text.
    pub fn no_paste_as_token(mut self) -> Self {
        self.paste_as_token = Some(None);
        self
    }

    /// Upstream `onFiles`: pasted or dropped files; never while disabled.
    pub fn on_files(
        mut self,
        f: impl Fn(Vec<PickedFile>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_files = Some(Rc::new(f));
        self
    }

    /// Upstream `onSubmit`: Enter without Shift; else the composer's.
    pub fn on_submit(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    /// Upstream `onKeyDown` (see [`KeyDownFn`]).
    pub fn on_key_down(
        mut self,
        f: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_key_down = Some(Rc::new(f));
        self
    }
}

impl Styled for ChatComposerInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ChatComposerInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let i18n = astryx_i18n::current(cx);
        let composer: Option<ChatComposerContext> = use_chat_composer_context(cx);
        let key = self.id.to_string();

        // ORD1, ORD2: the explicit value and callbacks, then the composer's.
        let controlled = self.value.is_some();
        let value = self
            .value
            .clone()
            .or_else(|| composer.as_ref().map(|c| c.value.clone()));
        let disabled = self
            .disabled
            .or(composer.as_ref().map(|c| c.disabled))
            .unwrap_or(false);
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.chat.composerInput.label"));
        let placeholder = self
            .placeholder
            .clone()
            .or_else(|| composer.as_ref().map(|c| c.placeholder.clone()))
            .unwrap_or_else(|| i18n.t("@astryx.chat.composer.placeholder"));
        let on_change: Option<ChatValueFn> = if controlled {
            self.on_change.clone()
        } else {
            match (
                composer.as_ref().map(|c| c.on_change.clone()),
                self.on_change.clone(),
            ) {
                (Some(a), Some(b)) => Some(Rc::new(
                    move |v: SharedString, w: &mut Window, cx: &mut App| {
                        a(v.clone(), w, cx);
                        b(v, w, cx);
                    },
                )),
                (a, b) => a.or(b),
            }
        };
        let on_submit = self
            .on_submit
            .clone()
            .or_else(|| composer.as_ref().map(|c| c.on_submit.clone()));

        let editor = window.use_keyed_state(
            ElementId::Name(format!("{key}-editor-state").into()),
            cx,
            |window, cx| TextEditState::reporting(String::new(), true, window, cx),
        );
        let model = window.use_keyed_state(
            ElementId::Name(format!("{key}-model").into()),
            cx,
            |_, _| InputModel::new(),
        );
        let menu_state =
            window.use_keyed_state(ElementId::Name(format!("{key}-menu").into()), cx, |_, _| {
                MenuState::default()
            });
        let triggers: Rc<[ChatComposerTrigger]> = self.triggers.clone().into();
        let menu = TriggerMenu {
            state: menu_state.clone(),
            triggers: triggers.clone(),
            debounce: self.debounce,
            editor: editor.clone(),
            draft: model.read(cx).draft.clone(),
        };
        let paste_as_token = match self.paste_as_token.clone() {
            None => Some(super::composer_token::use_chat_paste_as_token(
                Default::default(),
            )),
            Some(choice) => choice,
        };
        let parts = Rc::new(Parts {
            id: key.clone().into(),
            editor: editor.clone(),
            model: model.clone(),
            menu: menu.clone(),
            triggers: triggers.clone(),
            on_change,
            on_submit,
            on_key_down: self.on_key_down.clone(),
            on_paste: self.on_paste.clone(),
            on_files: self.on_files.clone(),
            paste_as_token: paste_as_token.clone(),
            history: self.history,
            disabled,
            window: window.window_handle(),
        });
        if let Some(handle) = &self.handle {
            *handle.inner.borrow_mut() = Some(parts.clone());
        }

        // FR4: a controlled value that is not the echo of the last change replaces the
        // draft; the echo itself leaves the text and caret alone.
        if let Some(value) = &value {
            let echo = model.read(cx).pending_echo.clone();
            if echo.as_ref() == Some(value) {
                model.update(cx, |m, _| m.pending_echo = None);
            } else if serialize(&editor, &model, cx) != value.as_ref() {
                model.update(cx, |m, _| m.pending_echo = None);
                parts.restore(value, false, cx);
            } else {
                model.update(cx, |m, _| m.pending_echo = None);
            }
        }

        // A disabled input ends its trigger query, so late results cannot reopen it.
        if disabled && menu.is_active(cx) {
            menu.reset(window, cx);
        }
        menu_state.update(cx, |s, _| {
            let parts = parts.clone();
            s.on_pick = Some(Rc::new(move |range, result, window, cx| match result {
                ChatComposerTriggerResult::Text(text) => {
                    parts.insert_text(&text, Some(range), window, cx)
                }
                ChatComposerTriggerResult::Token(token) => {
                    parts.insert_token(token, Some(range), window, cx);
                }
            }));
        });

        let paste_tokens = paste_as_token.is_some();
        editor.update(cx, |s, _| {
            s.set_settings(FieldSettings {
                disabled,
                read_only: false,
                masked: false,
                filter: None,
                spell_check: true,
                keyboard: KeyboardHints {
                    enter_key_hint: Some(self.enter_key_hint),
                    ..Default::default()
                },
            });
            let changed = parts.clone();
            s.on_change = (!disabled).then(|| {
                Rc::new(move |_: String, window: &mut Window, cx: &mut App| {
                    changed.emit_change(window, cx);
                    changed.menu.handle_input(window, cx);
                }) as astryx_primitives::editing::ChangeFn
            });
            let keys = parts.clone();
            s.on_key_down = Some(Rc::new(move |event, window, cx| {
                keys.key_down(event, window, cx)
            }));
            let paste = parts.clone();
            s.on_paste_override = Some(Rc::new(move |item, window, cx| {
                paste.paste(item, window, cx)
            }));
            let (render_model, text_model, expand) = (model.clone(), model.clone(), parts.clone());
            let input_key = key.clone();
            let interim_color = t.color.text_secondary;
            s.inline_objects = Some(InlineObjects {
                render: Rc::new(move |id, window, cx| {
                    if id == INTERIM {
                        let words = render_model.read(cx).interim.clone().unwrap_or_default();
                        return div()
                            .italic()
                            .whitespace_nowrap()
                            .text_color(interim_color)
                            .child(words)
                            .into_any_element();
                    }
                    let Some(token) = render_model.read(cx).tokens.get(&id).cloned() else {
                        return div().into_any_element();
                    };
                    let selector = format!("chat-token-{}", token.value());
                    let content = if !token.is_custom()
                        && paste_tokens
                        && token.value().encode_utf16().count() > 200
                    {
                        let expand = expand.clone();
                        ChatPastedTextToken::new(
                            ElementId::Name(format!("{input_key}-pasted-{id}").into()),
                            token.value().clone(),
                        )
                        .on_expand(move |window, cx| expand.expand_token(id, window, cx))
                        .into_any_element()
                    } else {
                        token.content(window, cx)
                    };
                    div()
                        .debug_selector(move || selector)
                        .flex()
                        .items_center()
                        .child(content)
                        .into_any_element()
                }),
                text: Rc::new(move |id, cx| {
                    let m = text_model.read(cx);
                    if id == INTERIM {
                        return m.interim.as_deref().unwrap_or("").to_string();
                    }
                    m.tokens
                        .get(&id)
                        .map(|t| t.value().to_string())
                        .unwrap_or_default()
                }),
            });
        });

        let focus: FocusHandle = editor.read(cx).focus_handle(cx);
        // The composer's shell focuses the input after its draft (FR8).
        if let Some(composer) = &composer {
            let (editor, focus) = (editor.clone(), focus.clone());
            *composer.input_control.borrow_mut() = Some(ChatComposerInputControl {
                focus_handle: Some(focus.clone()),
                focus: Rc::new(move |window, cx| {
                    window.focus(&focus, cx);
                    editor.update(cx, |s, cx| {
                        let end = s.text().len();
                        s.select_range(end..end, false, window, cx);
                    });
                }),
            });
        }

        let empty = is_empty(&editor, &model, cx);
        let line = px(LINE_HEIGHT);
        let style = EditorStyle {
            text_color: t.color.text_primary,
            placeholder: None,
            placeholder_color: t.color.text_secondary,
            selection_color: {
                let mut c = t.color.accent;
                c.a *= 0.3;
                c
            },
            caret_color: t.color.accent,
            multiline: true,
            min_lines: 1,
            max_lines: Some(self.max_rows.max(1)),
            height: None,
            reduced_motion: prefers_reduced_motion(cx),
            spelling_color: t.color.error,
        };
        let editor_key = format!("{key}-editor");
        let has_triggers = !triggers.is_empty();
        let menu_open = !disabled && menu.is_visible(cx);
        let mut field = text_editor(
            ElementId::Name(editor_key.clone().into()),
            &editor,
            style,
            cx,
        )
        .debug_selector(move || editor_key)
        .min_h(line)
        .px(t.spacing.s1)
        .font_stack(&t.font.family.body)
        .text_size(t.text.body.size)
        .line_height(line)
        .text_color(t.color.text_primary)
        .aria_label(label)
        .aria_disabled(disabled);
        // AR1: a multiline textbox, or a combobox once triggers are configured.
        field = if has_triggers {
            field.role(Role::ComboBox).aria_expanded(menu_open)
        } else {
            field.role(Role::MultilineTextInput)
        };
        // A press in the editor keeps the menu open, but may move the caret out of the
        // query: check it again.
        {
            let menu = menu.clone();
            field = field.on_mouse_up(gpui::MouseButton::Left, move |_, window, cx| {
                if !disabled && menu.is_active(cx) {
                    menu.handle_input(window, cx);
                }
            });
        }

        let mut layer = anchored_layer(ElementId::Name(format!("{key}-menu-layer").into()), field);
        if menu_open && let Some(surface) = menu.render(&key, &focus, window, cx) {
            let start = menu_state.read(cx).start;
            let anchor = editor.read(cx).offset_bounds(start + 1);
            let (hide, offset) = (menu.clone(), t.spacing.s1.to_pixels(rem));
            layer = layer
                .escape(EscapeBehavior::Block)
                .position_with(move |bounds, size, viewport, options| {
                    let anchor = anchor.unwrap_or(bounds);
                    let position: LayerPosition =
                        astryx_primitives::position_layer(anchor, size, viewport, options);
                    position
                })
                .surface(
                    surface,
                    LayerOptions {
                        placement: Placement::Above,
                        align: LayerAlign::Start,
                        offset,
                        ..Default::default()
                    },
                    move |window, cx| hide.hide(window, cx),
                );
        }

        let mut root = div()
            .id(self.id.clone())
            .debug_selector({
                let key = key.clone();
                move || key
            })
            .relative()
            .flex()
            .flex_col()
            .min_h(line)
            // A press in the editor is the editor's: the composer body's click-to-focus
            // skips it (upstream skips `[contenteditable]` and tokens).
            .on_click(|_, _, cx| cx.stop_propagation());
        theme.refine(root.style(), "chat-composer-input", &[], None);
        if disabled {
            root = root.opacity(0.5);
        }
        root.style().refine(&self.style);
        // FR6: dropped files go to on_files (never while disabled).
        if !disabled {
            let files = self.on_files.clone();
            root = root.on_drop(move |paths: &ExternalPaths, window, cx| {
                let picked: Vec<PickedFile> = paths
                    .paths()
                    .iter()
                    .cloned()
                    .map(PickedFile::from_path)
                    .collect();
                if !picked.is_empty()
                    && let Some(f) = &files
                {
                    f(picked, window, cx);
                }
            });
        }
        if empty {
            let selector = format!("{key}-placeholder");
            root = root.child(
                div()
                    .debug_selector(move || selector)
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .px(t.spacing.s1)
                    .font_stack(&t.font.family.body)
                    .text_size(t.text.body.size)
                    .line_height(line)
                    .text_color(t.color.text_secondary)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(placeholder),
            );
        }
        root.child(layer)
    }
}
