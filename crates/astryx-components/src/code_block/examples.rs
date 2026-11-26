//! CodeBlock examples: the gallery's Content page.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use super::{CodeBlock, CodeBlockContainer, CodeBlockSize};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};

const TS: &str = "import {useState} from 'react';\n\n// A counter that starts at zero.\nexport function Counter({step = 1}: {step?: number}) {\n  const [count, setCount] = useState(0);\n  return <button onClick={() => setCount(count + step)}>{count}</button>;\n}\n";
const DIFF: &str = "--- a/counter.ts\n+++ b/counter.ts\n@@ -1,3 +1,3 @@\n const start = 0;\n-let count = start;\n+let count = start + 1;\n console.log(count);\n";
const GQL: &str = "query Viewer($first: Int = 10) {\n  viewer { name repositories(first: $first) { totalCount } }\n}\n";

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(label("CodeBlock: a title, line numbers and a highlighted line"))
        .child(
            CodeBlock::new("cb-ts", TS)
                .language("tsx")
                .title("Counter.tsx")
                .line_numbers(true)
                .highlight_lines([5]),
        )
        .child(label(
            "Collapsible: its header folds the body, which eases closed and open (FR5)",
        ))
        .child(
            CodeBlock::new("cb-collapsible", TS)
                .language("tsx")
                .title("Counter.tsx")
                .collapsible(true)
                .collapsible_threshold(5),
        )
        .child(label("A diff over TypeScript, with line numbers"))
        .child(CodeBlock::new("cb-diff", DIFF).language("ts").diff(true).line_numbers(true))
        .child(label("Headerless: the copy button sits beside the first line"))
        .child(CodeBlock::new("cb-cmd", "cargo add astryx"))
        .child(label("A language module (GraphQL), small, in the Dracula preset"))
        .child(
            CodeBlock::new("cb-gql", GQL)
                .language(astryx_syntax::languages::graphql())
                .size(CodeBlockSize::Sm)
                .syntax_theme(astryx_syntax::dracula()),
        )
        .child(label("Section container, wrapped, at full width"))
        .child(
            div().w(px(520.)).child(
                CodeBlock::new("cb-wrap", "const message = 'A long line that wraps instead of scrolling sideways when it reaches the edge of the block';")
                    .language("js")
                    .container(CodeBlockContainer::Section)
                    .wrapped(true)
                    .width(gpui::relative(1.)),
            ),
        )
        .into_any_element()
}
