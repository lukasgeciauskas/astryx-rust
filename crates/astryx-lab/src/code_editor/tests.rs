//! Upstream lab `CodeEditor.test.tsx`, the parts a GPUI window can drive, and the
//! editing rules.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use astryx_syntax::SyntaxToken;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, Modifiers, Render, SharedString, TestAppContext,
    VisualTestContext, Window, div, px,
};

use super::*;

// The rules.

#[test]
fn shift_tab_removes_up_to_two_leading_spaces_from_the_caret_line() {
    assert_eq!(
        outdent("  const x = 1;", 14),
        Some(("const x = 1;".into(), 12))
    );
    assert_eq!(
        outdent(" const x = 1;", 13),
        Some(("const x = 1;".into(), 12))
    );
    assert_eq!(outdent("const x = 1;", 12), None);
    // Only the caret's line, in multi-line code.
    assert_eq!(outdent("  a;\n  b;", 7), Some(("  a;\nb;".into(), 5)));
    // A caret inside the indentation stops at the line start.
    assert_eq!(outdent("    x", 1), Some(("  x".into(), 0)));
    // `lastIndexOf('\n', -1)` checks index 0: a caret at 0 before a leading break
    // reads the next line.
    assert_eq!(outdent("\n  y", 0), Some(("\ny".into(), 0)));
    assert_eq!(outdent("", 0), None);
}

#[test]
fn enter_keeps_the_lines_leading_whitespace_up_to_the_caret() {
    assert_eq!(indent_before("    if (x) {", 12), "    ");
    assert_eq!(indent_before("a\n\t  b", 6), "\t  ");
    assert_eq!(indent_before("   ", 2), "  ");
    assert_eq!(indent_before("x", 1), "");
    // JavaScript's `\s` includes a no-break space.
    assert_eq!(indent_before("\u{A0} y", 4), "\u{A0} ");
}

#[test]
fn openers_close_themselves() {
    for (open, close) in [
        ("(", ")"),
        ("[", "]"),
        ("{", "}"),
        ("\"", "\""),
        ("'", "'"),
        ("`", "`"),
    ] {
        assert_eq!(auto_close_pair(open), Some(close));
    }
    assert_eq!(auto_close_pair("<"), None);
}

#[test]
fn line_tokens_become_ranges_of_the_whole_text() {
    let token = |ty: &'static str, start, end| SyntaxToken {
        ty: ty.into(),
        start,
        end,
    };
    let text = "let a\nfn b\nx";
    let lines = vec![
        vec![token("keyword", 0, 3)],
        vec![token("keyword", 0, 2), token("function", 3, 4)],
        vec![token("variable", 0, 9)],
    ];
    let ranges = flat_ranges(text, &lines);
    assert_eq!(
        ranges
            .iter()
            .map(|(r, ty)| (r.clone(), ty.to_string()))
            .collect::<Vec<_>>(),
        [
            (0..3, "keyword".to_string()),
            (6..8, "keyword".into()),
            (9..10, "function".into())
        ],
        "a token past the text is skipped"
    );
}

// In a window.

type BuildFn = Rc<dyn Fn(&Entity<SharedString>, &mut Window, &mut App) -> AnyElement>;

struct Host {
    build: BuildFn,
    value: Entity<SharedString>,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w(px(600.))
            .h(px(600.))
            .child(astryx_components::button::Button::new("before", "before"))
            .child((self.build)(&self.value, window, cx))
            .child(astryx_components::button::Button::new("after", "after"))
    }
}

type Changes = Rc<RefCell<Vec<String>>>;

fn host<'a>(
    cx: &'a mut TestAppContext,
    initial: &str,
    changes: &Changes,
    tweak: impl Fn(CodeEditor) -> CodeEditor + 'static,
) -> (&'a mut VisualTestContext, Entity<SharedString>) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let value = cx.update(|cx| cx.new(|_| SharedString::from(initial.to_string())));
    let (v, log) = (value.clone(), changes.clone());
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        build: Rc::new({
            let (tweak, log) = (Rc::new(tweak), log.clone());
            move |value, _, cx| {
                let (set, log) = (value.clone(), log.clone());
                tweak(
                    CodeEditor::new(
                        "ed",
                        "Edit snippet",
                        value.read(cx).clone(),
                        move |next, _, cx| {
                            log.borrow_mut().push(next.to_string());
                            set.update(cx, |v, cx| {
                                *v = next;
                                cx.notify();
                            })
                        },
                    )
                    .language("typescript"),
                )
                .into_any_element()
            }
        }),
        value: v,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    // Focus the editor, the caret at the end.
    click(cx, "ed-code");
    key(cx, "ctrl-end");
    (cx, value)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(b.center(), Modifiers::none());
    settle(cx);
}

fn key(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(k);
    settle(cx);
}

fn value(cx: &mut VisualTestContext, v: &Entity<SharedString>) -> String {
    v.read_with(cx, |v, _| v.to_string())
}

/// Whether typing reaches the editor: types a probe and removes it again.
fn editor_has_focus(cx: &mut VisualTestContext, v: &Entity<SharedString>) -> bool {
    let before = value(cx, v);
    cx.simulate_input("~");
    settle(cx);
    let typed = value(cx, v) != before;
    if typed {
        key(cx, "backspace");
    }
    typed
}

#[gpui::test]
fn tab_inserts_two_spaces_and_keeps_focus_in_the_editor(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, v) = host(cx, "", &changes, |e| e);
    key(cx, "tab");
    assert_eq!(*changes.borrow(), ["  "]);
    assert!(editor_has_focus(cx, &v));
}

#[gpui::test]
fn escape_then_tab_moves_focus_out_of_the_editor_without_indenting(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, v) = host(cx, "", &changes, |e| e);
    key(cx, "escape");
    key(cx, "tab");
    assert!(changes.borrow().is_empty());
    assert!(!editor_has_focus(cx, &v));
}

#[gpui::test]
fn escape_then_shift_tab_moves_focus_backward(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, v) = host(cx, "  x", &changes, |e| e);
    key(cx, "escape");
    key(cx, "shift-tab");
    assert!(changes.borrow().is_empty(), "no outdent");
    assert!(!editor_has_focus(cx, &v));
}

#[gpui::test]
fn tab_moves_focus_mode_is_one_shot(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, v) = host(cx, "", &changes, |e| e);
    key(cx, "escape");
    key(cx, "tab");
    assert!(!editor_has_focus(cx, &v));
    click(cx, "ed-code");
    key(cx, "tab");
    assert_eq!(*changes.borrow(), ["  "]);
    assert!(editor_has_focus(cx, &v));
}

#[gpui::test]
fn typing_after_escape_re_arms_tab_indentation(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, v) = host(cx, "", &changes, |e| e);
    key(cx, "escape");
    key(cx, "a");
    key(cx, "tab");
    assert!(editor_has_focus(cx, &v));
    assert_eq!(changes.borrow().last().map(String::as_str), Some("a  "));
}

#[gpui::test]
fn shift_tab_outdents_the_current_line(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, v) = host(cx, "  const x = 1;", &changes, |e| e);
    key(cx, "shift-tab");
    assert_eq!(*changes.borrow(), ["const x = 1;"]);
    assert!(editor_has_focus(cx, &v));
}

#[gpui::test]
fn shift_tab_on_an_unindented_line_changes_nothing_and_keeps_focus(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, v) = host(cx, "const x = 1;", &changes, |e| e);
    key(cx, "shift-tab");
    assert!(changes.borrow().is_empty());
    assert!(editor_has_focus(cx, &v));
}

#[gpui::test]
fn shift_tab_outdents_only_the_current_line_in_multi_line_code(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, _) = host(cx, "  a;\n  b;", &changes, |e| e);
    key(cx, "shift-tab");
    assert_eq!(*changes.borrow(), ["  a;\nb;"]);
}

#[gpui::test]
fn enter_keeps_the_indentation_and_openers_close_themselves(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, v) = host(cx, "    if (x) {", &changes, |e| e);
    key(cx, "enter");
    assert_eq!(value(cx, &v), "    if (x) {\n    ");
    key(cx, "(");
    assert_eq!(value(cx, &v), "    if (x) {\n    ()");
    // The caret sits between the pair.
    key(cx, "a");
    assert_eq!(value(cx, &v), "    if (x) {\n    (a)");
}

#[gpui::test]
fn a_read_only_editor_lets_tab_move_focus_and_takes_no_typing(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, v) = host(cx, "const x = 1;", &changes, |e| e.read_only(true));
    key(cx, "tab");
    cx.simulate_input("z");
    settle(cx);
    assert_eq!(value(cx, &v), "const x = 1;");
    assert!(changes.borrow().is_empty());
}

#[gpui::test]
fn numbers_each_line_and_paints_the_tokens(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let (cx, _) = host(cx, "const a = 1;\nlet b = 2;\n", &changes, |e| {
        e.line_numbers(true)
    });
    let gutter = cx.debug_bounds("ed-gutter").expect("the gutter is drawn");
    let code = cx.debug_bounds("ed-code").unwrap();
    // Three lines, as many numbers: the gutter is as tall as the code.
    assert!(
        (gutter.size.height - code.size.height).abs() < px(2.),
        "{gutter:?} {code:?}"
    );
}

#[gpui::test]
fn a_tall_editor_scrolls_within_its_max_height(cx: &mut TestAppContext) {
    let changes: Changes = Rc::default();
    let code: Vec<String> = (1..=30).map(|i| format!("line {i};")).collect();
    let (cx, _) = host(cx, &code.join("\n"), &changes, |e| e.max_height(px(200.)));
    let root = cx.debug_bounds("ed").unwrap();
    assert!(root.size.height <= px(202.), "{root:?}");
}
