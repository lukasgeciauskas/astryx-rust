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
            height: None,
            reduced_motion: true,
            spelling_color: hsla(0., 1., 0.5, 1.),
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

/// The word modifier where the test runs: Option on a Mac, Ctrl elsewhere.
fn word_key(cx: &mut VisualTestContext) -> &'static str {
    if cx.read(|cx| cx.platform_info().os) == gpui::OperatingSystem::Mac {
        "alt"
    } else {
        "ctrl"
    }
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
    let word = word_key(cx);
    cx.simulate_keystrokes(&format!("{word}-backspace"));
    assert_eq!(text(cx, &state), "hello brave ");
    cx.simulate_keystrokes(&format!("{word}-shift-left"));
    cx.simulate_input("big");
    assert_eq!(text(cx, &state), "hello big");
}

#[gpui::test]
fn undo_and_redo_step_through_edits(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "", false);
    cx.simulate_input("one ");
    cx.simulate_input("two");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx, &state), "one ");
    cx.simulate_keystrokes("secondary-shift-z");
    assert_eq!(text(cx, &state), "one two");
    if word_key(cx) == "ctrl" {
        // Windows and Linux also redo with Ctrl+Y.
        cx.simulate_keystrokes("secondary-z ctrl-y");
        assert_eq!(text(cx, &state), "one two");
    }
}

/// On a Mac the fields take macOS's keys: Cmd for shortcuts, Option for words,
/// Cmd with the arrows for line and document ends (checked on any host by
/// reporting a Mac before the keys are bound).
#[gpui::test]
fn text_fields_take_macos_keys_on_a_mac(cx: &mut TestAppContext) {
    cx.set_platform_info(gpui::PlatformInfo {
        os: gpui::OperatingSystem::Mac,
        target: gpui::PlatformTarget::Native,
    });
    let (cx, state, _) = field(cx, "hello brave world", false);
    cx.simulate_keystrokes("alt-backspace");
    assert_eq!(text(cx, &state), "hello brave ");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(cx, &state), "hello brave world");
    cx.simulate_keystrokes("cmd-shift-z cmd-z cmd-left");
    cx.simulate_input("A ");
    assert_eq!(text(cx, &state), "A hello brave world");
    cx.simulate_keystrokes("cmd-right alt-shift-left");
    cx.simulate_input("day");
    assert_eq!(text(cx, &state), "A hello brave day");
    cx.simulate_keystrokes("cmd-a");
    assert_eq!(state.read_with(cx, |s, _| s.buffer().selection()), 0..17);
    // Ctrl does nothing of the sort there.
    cx.simulate_keystrokes("right ctrl-backspace");
    assert_eq!(text(cx, &state), "A hello brave day");
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
    cx.simulate_keystrokes("backspace secondary-a");
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

// ----------------------------------------------------------------------------
// Spelling and the touch keyboard

const AFF: &str = "SET UTF-8
TRY esianrtolcdugmphbyfvkwzESIANRTOLCDUGMPHBYFVKWZ'
";
const DIC: &str = "4
hello
world
the
spell
";

fn dictionary(cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        let checker = crate::spell::DictionaryChecker::new(AFF, DIC).expect("a dictionary");
        crate::spell::set_spell_checker(Some(Rc::new(checker)), cx);
    });
}

fn errors(
    cx: &mut VisualTestContext,
    state: &Entity<TextEditState>,
    focused: bool,
) -> Vec<std::ops::Range<usize>> {
    cx.update(|_, cx| state.update(cx, |s, cx| s.spelling_errors(focused, cx)))
}

#[gpui::test]
fn misspelled_words_show_once_the_field_has_had_focus(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "hello wrold the", false);
    dictionary(cx);
    state.update(cx, |s, _| {
        s.set_settings(FieldSettings {
            spell_check: true,
            ..Default::default()
        })
    });
    assert!(errors(cx, &state, false).is_empty(), "not before focus");
    cx.simulate_keystrokes("home");
    assert_eq!(errors(cx, &state, true), vec![6..11]);
    assert_eq!(errors(cx, &state, false), vec![6..11], "and after blur");
}

#[gpui::test]
fn the_word_being_typed_is_left_alone(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "", false);
    dictionary(cx);
    state.update(cx, |s, _| {
        s.set_settings(FieldSettings {
            spell_check: true,
            ..Default::default()
        })
    });
    cx.simulate_input("the wrol");
    assert!(errors(cx, &state, true).is_empty(), "the caret ends it");
    cx.simulate_input(" ");
    assert_eq!(errors(cx, &state, true), vec![4..8], "a space finishes it");
}

#[gpui::test]
fn masked_and_unchecked_fields_mark_nothing(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "wrold", false);
    dictionary(cx);
    assert!(errors(cx, &state, true).is_empty(), "spell checking is off");
    state.update(cx, |s, _| {
        s.set_settings(FieldSettings {
            spell_check: true,
            masked: true,
            ..Default::default()
        })
    });
    assert!(errors(cx, &state, true).is_empty(), "a password field");
}

#[test]
fn the_touch_keyboard_follows_the_field() {
    use gpui::{Autocapitalize, TextInputAction, TextInputMode};
    let plain = FieldSettings {
        spell_check: true,
        ..Default::default()
    }
    .input_configuration();
    assert_eq!(plain.autocapitalize, Autocapitalize::Sentences);
    assert!(plain.autocorrect && plain.suggestions);
    assert_eq!(plain.spellcheck, Some(true));
    assert_eq!(plain.input_mode, TextInputMode::Text);

    let email = FieldSettings {
        keyboard: KeyboardHints {
            input_mode: Some(TextInputMode::Email),
            enter_key_hint: Some(TextInputAction::Next),
            ..Default::default()
        },
        ..Default::default()
    }
    .input_configuration();
    assert_eq!(email.autocapitalize, Autocapitalize::None);
    assert!(!email.autocorrect);
    assert_eq!(email.input_action, TextInputAction::Next);

    let password = FieldSettings {
        masked: true,
        spell_check: true,
        ..Default::default()
    }
    .input_configuration();
    assert!(!password.suggestions && !password.autocorrect);
    assert_eq!(password.spellcheck, Some(false));
}

// ----------------------------------------------------------------------------
// Inline objects

/// A field whose objects are 40px boxes, copying as `<id>`.
fn with_objects(cx: &mut VisualTestContext, state: &Entity<TextEditState>) {
    state.update(cx, |s, _| {
        s.inline_objects = Some(InlineObjects {
            render: Rc::new(|id, _, _| {
                div()
                    .debug_selector(move || format!("object-{id}"))
                    .w(px(40.))
                    .h(px(10.))
                    .into_any_element()
            }),
            text: Rc::new(|id, _| format!("<{id}>")),
        })
    });
}

fn x_of(cx: &mut VisualTestContext, state: &Entity<TextEditState>, offset: usize) -> gpui::Pixels {
    state.read_with(cx, |s, _| s.position_for_offset(offset).unwrap().x)
}

#[gpui::test]
fn an_object_takes_its_box_width_in_the_line(cx: &mut TestAppContext) {
    let (cx, state, log) = field(cx, "", true);
    with_objects(cx, &state);
    cx.simulate_input("ab");
    cx.update(|window, cx| state.update(cx, |s, cx| s.insert_object(9, window, cx)));
    cx.simulate_input("cd");
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let object = buffer::OBJECT.len_utf8();
    let (before, after) = (x_of(cx, &state, 2), x_of(cx, &state, 2 + object));
    assert!(
        (after - before - px(40.)).abs() < px(0.5),
        "the caret steps over the 40px box: {before:?} -> {after:?}"
    );
    let drawn = cx.debug_bounds("object-9").expect("the object is drawn");
    assert!(
        (drawn.origin.x - (px(10.) + before)).abs() < px(0.5),
        "at its character"
    );
    assert_eq!(
        log.borrow().last().map(String::as_str),
        Some("ab\u{FFFC}cd")
    );

    // Backspace removes the whole object; copying gives its text.
    cx.simulate_keystrokes("left left backspace");
    assert_eq!(text(cx, &state), "abcd");
    assert!(state.read_with(cx, |s, _| s.buffer().objects().is_empty()));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(
        state.read_with(cx, |s, _| s.buffer().objects().to_vec()),
        vec![9]
    );
    cx.simulate_keystrokes("secondary-a secondary-c");
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(copied.as_deref(), Some("ab<9>cd"));
}

#[gpui::test]
fn lines_with_objects_wrap_at_their_widened_width(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "", true);
    with_objects(cx, &state);
    cx.simulate_input("one two three four five six seven ");
    for id in 0..4 {
        cx.update(|window, cx| state.update(cx, |s, cx| s.insert_object(id, window, cx)));
        cx.simulate_input("\u{A0}word ");
    }
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let end = state.read_with(cx, |s, _| s.text().len());
    let last = state.read_with(cx, |s, _| s.position_for_offset(end).unwrap());
    assert!(last.y > px(0.), "it wrapped onto more rows: {last:?}");
    let rows = state.read_with(cx, |s, _| {
        let layout = s.layout.as_ref().unwrap();
        layout
            .lines
            .iter()
            .map(|(_, l)| l.wrap_boundaries().len() + 1)
            .sum::<usize>()
    });
    for id in 0..4 {
        let b = cx
            .debug_bounds(Box::leak(format!("object-{id}").into_boxed_str()))
            .unwrap();
        assert!(b.right() <= px(311.), "object {id} fits its row: {b:?}");
    }
    assert!(rows >= 2);
}

#[gpui::test]
fn a_multiline_field_that_keeps_lines_whole_grows_as_wide_as_its_longest(cx: &mut TestAppContext) {
    let long = "x".repeat(120);
    let (cx, state, _) = field(cx, &format!("short\n{long}"), true);
    let wrapped = state.read_with(cx, |s, _| s.bounds().unwrap());
    state.update(cx, |s, cx| {
        s.no_wrap = true;
        cx.notify();
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let whole = state.read_with(cx, |s, _| s.bounds().unwrap());
    assert!(
        whole.size.width > wrapped.size.width,
        "{whole:?} {wrapped:?}"
    );
    assert!(
        whole.size.height < wrapped.size.height,
        "two rows, not wrapped ones"
    );
}

#[gpui::test]
fn highlights_paint_ranges_and_ignore_ranges_past_the_text(cx: &mut TestAppContext) {
    let (cx, state, _) = field(cx, "let a = 1;", false);
    state.update(cx, |s, cx| {
        s.highlights = vec![
            (0..3, hsla(0.6, 1., 0.5, 1.)),
            (4..400, hsla(0., 1., 0.5, 1.)),
        ];
        cx.notify();
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    // The field still draws (a stale range past the text is skipped), and editing
    // keeps working.
    cx.simulate_input("x");
    assert_eq!(text(cx, &state), "let a = 1;x");
}
