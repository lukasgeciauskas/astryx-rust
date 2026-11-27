//! The CodeEditor gallery (upstream's `Lab/CodeEditor` stories).

use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Heading, HeadingLevel};
use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, div, px};

use super::{CodeEditor, CodeEditorSize};

const DEFAULT_CODE: &str = "function greet(name: string): string {
  const message = `Hello, ${name}!`;
  console.log(message);
  return message;
}";

/// Upstream's `ControlledEditor`: a CodeEditor that owns its value, with line
/// numbers, in TypeScript unless told otherwise.
fn controlled(
    id: &'static str,
    initial: &str,
    window: &mut Window,
    cx: &mut App,
    tweak: impl FnOnce(CodeEditor) -> CodeEditor,
) -> AnyElement {
    let initial = SharedString::from(initial.to_string());
    let value = window.use_keyed_state(id, cx, move |_, _| initial);
    let set = value.clone();
    tweak(
        CodeEditor::new(
            id,
            "Code editor",
            value.read(cx).clone(),
            move |next, _, cx| {
                set.update(cx, |v, cx| {
                    *v = next;
                    cx.notify();
                })
            },
        )
        .language("typescript")
        .line_numbers(true),
    )
    .into_any_element()
}

fn story(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(Heading::new(HeadingLevel::H3, title))
        .child(div().w(px(560.)).child(body))
        .into_any_element()
}

/// The CodeEditor examples: TypeScript, JSON and Python, a placeholder, read-only, a
/// maximum height, and the small size.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let lines: Vec<String> = (1..=30).map(|i| format!("const line{i} = {i};")).collect();
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(story("Default", controlled("code-editor-default", DEFAULT_CODE, window, cx, |e| e)))
        .child(story(
            "JSON editor",
            controlled(
                "code-editor-json",
                "{\n  \"name\": \"my-app\",\n  \"version\": \"1.0.0\",\n  \"settings\": {\n    \"port\": 3000,\n    \"debug\": false\n  }\n}",
                window,
                cx,
                |e| e.language("json").line_numbers(true),
            ),
        ))
        .child(story(
            "Python editor",
            controlled(
                "code-editor-python",
                "def fibonacci(n: int) -> list[int]:\n    \"\"\"Generate Fibonacci sequence.\"\"\"\n    if n <= 0:\n        return []\n    fib = [0, 1]\n    for i in range(2, n):\n        fib.append(fib[-1] + fib[-2])\n    return fib[:n]\n\nresult = fibonacci(10)\nprint(result)",
                window,
                cx,
                |e| e.language("python").line_numbers(true),
            ),
        ))
        .child(story(
            "With placeholder",
            controlled("code-editor-placeholder", "", window, cx, |e| {
                e.placeholder("Type your code here...").language("typescript").line_numbers(false)
            }),
        ))
        .child(story(
            "Read only",
            controlled("code-editor-read-only", DEFAULT_CODE, window, cx, |e| e.read_only(true).line_numbers(true)),
        ))
        .child(story(
            "With max height",
            controlled("code-editor-max-height", &lines.join("\n"), window, cx, |e| {
                e.language("typescript").line_numbers(true).max_height(px(200.))
            }),
        ))
        .child(story(
            "Small size",
            controlled("code-editor-small", DEFAULT_CODE, window, cx, |e| e.size(CodeEditorSize::Sm).line_numbers(true)),
        ))
        .into_any_element()
}
