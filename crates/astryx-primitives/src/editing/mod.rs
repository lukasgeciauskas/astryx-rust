//! The text editing engine behind TextInput, TextArea and NumberInput.
//!
//! - [`EditBuffer`]: the text model (graphemes, words, selection, IME
//!   composition, undo), pure and unit-tested.
//! - [`TextEditState`]: one field's entity, GPUI's IME client and the handler
//!   of the editing actions; [`init`] binds their keys.
//! - [`text_editor`]: the field's editable area, ready to put inside a
//!   component's chrome.
//!
//! ```ignore
//! let state = window.use_keyed_state(id, cx, |window, cx| {
//!     TextEditState::reporting(value, false, window, cx)
//! });
//! state.update(cx, |s, cx| {
//!     s.set_text(&value, cx);
//!     s.on_change = Some(Rc::new(|text, window, cx| { /* store it */ }));
//! });
//! text_editor("field", &state, style, cx)
//! ```

pub mod buffer;
mod element;
mod state;

#[cfg(test)]
mod tests;

pub use buffer::{Direction, EditBuffer, Granularity};
pub use element::EditorStyle;
pub use state::{
    Backspace, Copy, Cut, Delete, DeleteWordBackward, DeleteWordForward, DocumentEnd,
    DocumentStart, Down, End, Enter, Home, Left, Paste, Redo, Right, SelectAll, SelectDocumentEnd,
    SelectDocumentStart, SelectDown, SelectEnd, SelectHome, SelectLeft, SelectRight, SelectUp,
    SelectWordLeft, SelectWordRight, ShowCharacterPalette, Undo, Up, WordLeft, WordRight,
};
pub use state::{
    ChangeFn, EnterFn, FieldSettings, InsertFilter, KEY_CONTEXT, KeyDownFn, TextEditState, init,
};

use gpui::{
    App, CursorStyle, ElementId, Entity, Focusable, InteractiveElement, MouseButton, ParentElement,
    Stateful, Styled, Window, div,
};

use element::EditorElement;

/// A field's editable area: the text with its caret and selection, focusable,
/// with the editing keys and the mouse wired to `state`.
pub fn text_editor(
    id: impl Into<ElementId>,
    state: &Entity<TextEditState>,
    style: EditorStyle,
    cx: &App,
) -> Stateful<gpui::Div> {
    let focus = state.read(cx).focus_handle(cx);
    let s = state.clone();
    macro_rules! on {
        ($el:expr, $action:ty, |$state:ident, $window:ident, $cx:ident| $body:expr) => {{
            let s = s.clone();
            $el.on_action(move |_: &$action, $window: &mut Window, cx: &mut App| {
                s.update(cx, |$state, $cx| $body);
            })
        }};
    }
    use Direction::{Backward, Forward};
    use Granularity::{All, Grapheme, Line, Word};
    let mut el = div()
        .id(id)
        .key_context(KEY_CONTEXT)
        .track_focus(&focus)
        .cursor(CursorStyle::IBeam)
        .w_full();
    el = on!(el, Backspace, |st, window, cx| st
        .delete(Grapheme, Backward, window, cx));
    el = on!(el, Delete, |st, window, cx| st
        .delete(Grapheme, Forward, window, cx));
    el = on!(el, DeleteWordBackward, |st, window, cx| st
        .delete(Word, Backward, window, cx));
    el = on!(el, DeleteWordForward, |st, window, cx| st
        .delete(Word, Forward, window, cx));
    el = on!(el, Left, |st, window, cx| st
        .move_caret(Grapheme, Backward, false, window, cx));
    el = on!(el, Right, |st, window, cx| st
        .move_caret(Grapheme, Forward, false, window, cx));
    el = on!(el, WordLeft, |st, window, cx| st
        .move_caret(Word, Backward, false, window, cx));
    el = on!(el, WordRight, |st, window, cx| st
        .move_caret(Word, Forward, false, window, cx));
    el = on!(el, Home, |st, window, cx| st
        .move_caret(Line, Backward, false, window, cx));
    el = on!(el, End, |st, window, cx| st
        .move_caret(Line, Forward, false, window, cx));
    el = on!(el, DocumentStart, |st, window, cx| st
        .move_caret(All, Backward, false, window, cx));
    el = on!(el, DocumentEnd, |st, window, cx| st
        .move_caret(All, Forward, false, window, cx));
    el = on!(el, SelectLeft, |st, window, cx| st
        .move_caret(Grapheme, Backward, true, window, cx));
    el = on!(el, SelectRight, |st, window, cx| st
        .move_caret(Grapheme, Forward, true, window, cx));
    el = on!(el, SelectWordLeft, |st, window, cx| st
        .move_caret(Word, Backward, true, window, cx));
    el = on!(el, SelectWordRight, |st, window, cx| st
        .move_caret(Word, Forward, true, window, cx));
    el = on!(el, SelectHome, |st, window, cx| st
        .move_caret(Line, Backward, true, window, cx));
    el = on!(el, SelectEnd, |st, window, cx| st
        .move_caret(Line, Forward, true, window, cx));
    el = on!(el, SelectDocumentStart, |st, window, cx| st
        .move_caret(All, Backward, true, window, cx));
    el = on!(el, SelectDocumentEnd, |st, window, cx| st
        .move_caret(All, Forward, true, window, cx));
    el = on!(el, SelectAll, |st, window, cx| st.select_all(window, cx));
    el = on!(el, Copy, |st, window, cx| st.copy(false, window, cx));
    el = on!(el, Cut, |st, window, cx| st.copy(true, window, cx));
    el = on!(el, Paste, |st, window, cx| st.paste(window, cx));
    el = on!(el, Undo, |st, window, cx| st.undo(false, window, cx));
    el = on!(el, Redo, |st, window, cx| st.undo(true, window, cx));
    el = on!(el, Enter, |st, window, cx| st.enter(window, cx));
    if style.multiline {
        el = on!(el, Up, |st, window, cx| st
            .move_vertically(Backward, false, window, cx));
        el = on!(el, Down, |st, window, cx| st
            .move_vertically(Forward, false, window, cx));
        el = on!(el, SelectUp, |st, window, cx| st
            .move_vertically(Backward, true, window, cx));
        el = on!(el, SelectDown, |st, window, cx| st
            .move_vertically(Forward, true, window, cx));
    } else {
        // Up and Down go to the start and end of a single line, as platform fields do.
        el = on!(el, Up, |st, window, cx| st
            .move_caret(All, Backward, false, window, cx));
        el = on!(el, Down, |st, window, cx| st
            .move_caret(All, Forward, false, window, cx));
        el = on!(el, SelectUp, |st, window, cx| st
            .move_caret(All, Backward, true, window, cx));
        el = on!(el, SelectDown, |st, window, cx| st
            .move_caret(All, Forward, true, window, cx));
    }
    el = el.on_action(
        |_: &ShowCharacterPalette, window: &mut Window, _: &mut App| {
            window.show_character_palette();
        },
    );
    let (down, moved, up, up_out) = (s.clone(), s.clone(), s.clone(), s.clone());
    let focus_on_down = focus.clone();
    el.on_mouse_down(MouseButton::Left, move |event, window, cx| {
        window.focus(&focus_on_down, cx);
        down.update(cx, |st, cx| {
            st.mouse_down(
                event.position,
                event.click_count,
                event.modifiers.shift,
                window,
                cx,
            )
        });
    })
    .on_mouse_move(move |event, window, cx| {
        if event.dragging() {
            moved.update(cx, |st, cx| st.mouse_drag(event.position, window, cx));
        }
    })
    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
        up.update(cx, |st, _| st.mouse_up())
    })
    .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
        up_out.update(cx, |st, _| st.mouse_up())
    })
    .child(EditorElement {
        state: state.clone(),
        style,
    })
}
