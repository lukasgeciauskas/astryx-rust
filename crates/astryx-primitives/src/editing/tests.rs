//! Engine tests: a field in a window, driven by keys, text input and the IME.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, EntityInputHandler, Focusable, IntoElement, Modifiers,
    ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window, div, hsla, point, px,
};

use super::*;

struct Field {
    state: Entity<TextEditState>,
    multiline: bool,
}

impl Render for Field {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let style = EditorStyle {
            text_color: hsla(0., 0., 0., 1.),
            placeholder: Some("Type here".into()),
            placeholder_color: hsla(0., 0., 0.5, 1.),
            selection_color: hsla(0.6, 0.5, 0.5, 0.3),
            caret_color: hsla(0.6, 0.8, 0.5, 1.),
            multiline: self.multiline,
            min_lines: 1,
            max_lines: Some(4),
            reduced_motion: true,
        };
        div()
            .p(px(10.))
            .w(px(300.))
            .child(text_editor("field", &self.state, style, cx))
    }
}

type Log = Rc<RefCell<Vec<String>>>;

fn field<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    multiline: bool,
) -> (&'a mut VisualTestContext, Entity<TextEditState>, Log) {
    cx.update(|cx| {
        init(cx);
        crate::focus::init(cx);
    });
    let log: Log = Rc::default();
    let text = text.to_string();
    let (view, cx) = cx.add_window_view({
        let log = log.clone();
        move |window, cx| {
            let state = cx.new(|cx| TextEditState::reporting(text.clone(), multiline, window, cx));
            let (changes, enters) = (log.clone(), log.clone());
            state.update(cx, |s, _| {
                s.on_change = Some(Rc::new(move |text, _, _| changes.borrow_mut().push(text)));
                s.on_enter = Some(Rc::new(move |_, _| {
                    enters.borrow_mut().push("<enter>".into())
                }));
            });
            Field { state, multiline }
        }
    });
    let state = view.read_with(cx, |field, _| field.state.clone());
    cx.run_until_parked();
    let focus = state.read_with(cx, |s, cx| s.focus_handle(cx));
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.run_until_parked();
    (cx, state, log)
}

fn text(cx: &mut VisualTestContext, state: &Entity<TextEditState>) -> String {
    state.read_with(cx, |s, _| s.text().to_string())
}

#[gpui::test]
fn typing_edits_and_reports_each_change(cx: &mut TestAppContext) {
    let (cx, state, log) = field(cx, "", false);
    cx.simulate_input("hey");
    assert_eq!(text(cx, &state), "hey");
    assert_eq!(log.borrow().last().map(String::as_str), Some("hey"));
    cx.simulate_keystrokes("backspace");
    assert_eq!(text(cx, &state), "he");
}

#[gpui::test]
fn word_keys_and_selection_replace_text(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "hello brave world", false);
    cx.simulate_keystrokes("ctrl-backspace");
    assert_eq!(text(cx, &state), "hello brave ");
    cx.simulate_keystrokes("ctrl-shift-left");
    cx.simulate_input("big");
    assert_eq!(text(cx, &state), "hello big");
}

#[gpui::test]
fn undo_and_redo_step_through_edits(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "", false);
    cx.simulate_input("one ");
    cx.simulate_input("two");
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(text(cx, &state), "one ");
    cx.simulate_keystrokes("ctrl-y");
    assert_eq!(text(cx, &state), "one two");
}

#[gpui::test]
fn enter_is_reported_unless_an_ime_composes(cx: &mut TestAppContext) {
    let (cx, state, log) = field(cx, "", false);
    cx.update(|window, cx| {
        state.update(cx, |s, cx| {
            s.replace_and_mark_text_in_range(None, "ni", None, window, cx)
        });
    });
    cx.simulate_keystrokes("enter");
    assert!(
        !log.borrow().contains(&"<enter>".to_string()),
        "Enter commits the composition"
    );
    cx.update(|window, cx| state.update(cx, |s, cx| s.unmark_text(window, cx)));
    cx.simulate_keystrokes("enter");
    assert!(log.borrow().contains(&"<enter>".to_string()));
}

#[gpui::test]
fn read_only_fields_select_but_never_change(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "fixed", false);
    state.update(cx, |s, _| {
        s.set_settings(FieldSettings {
            read_only: true,
            ..Default::default()
        })
    });
    cx.simulate_input("x");
    cx.simulate_keystrokes("backspace ctrl-a");
    assert_eq!(text(cx, &state), "fixed");
    assert_eq!(state.read_with(cx, |s, _| s.buffer().selection()), 0..5);
}

#[gpui::test]
fn multiline_fields_break_lines_on_enter(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "", true);
    cx.simulate_input("one");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("two");
    assert_eq!(text(cx, &state), "one\ntwo");
    cx.simulate_keystrokes("up");
    cx.simulate_input("!");
    assert_eq!(
        text(cx, &state),
        "one!\ntwo",
        "Up keeps the column on the line above"
    );
}

#[gpui::test]
fn clicking_places_the_caret(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "abc def", false);
    cx.simulate_keystrokes("home");
    cx.simulate_click(point(px(280.), px(15.)), Modifiers::none());
    assert_eq!(
        state.read_with(cx, |s, _| s.buffer().cursor()),
        7,
        "past the end lands at the end"
    );
}
