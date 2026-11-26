//! Markdown examples for the gallery.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use super::{Markdown, MarkdownAutolink, MarkdownContentAlign, MarkdownDensity};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextDisplay, TextType};

const SAMPLE: &str = "# Release notes

Astryx renders **Markdown** with its own components: *emphasis*, ~~strikethrough~~, a hard line break\\
`inline code`, and [links](https://example.com) that wrap with the text around them.

## Lists

- Headings, paragraphs and quotes
- Nested lists
  - with their own markers
    - three levels deep
- Code blocks and tables

1. Ordered items
2. Count from their start
3. And nest too

- [x] Task lists are read-only checkboxes
- [ ] Open tasks stay unchecked

> Blockquotes keep their own rule and color.
>
> > And nest.

## Code

```rust
fn main() {
    println!(\"Hello, Markdown\");
}
```

```diff-ts
- const a = 1;
+ const a = 2;
```

## Tables

| Component | Status | Notes |
|:--|:-:|--:|
| Markdown | Ported | Core blocks and inline |
| CodeBlock | Ported | Highlighting, diff, copy |
| Chat | Next | Messages and composer |

---

Character references read as their characters: &copy; &amp; &#x2713;.
";

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(label("Markdown: every block and inline form, at the default density"))
        .child(Markdown::new("markdown-basic", SAMPLE))
        .child(label("Compact density, headings from level 3, prose capped at 420px and centred"))
        .child(
            Markdown::new(
                "markdown-compact",
                "# Section\n\nCompact spacing between blocks.\n\n## Subsection\n\n- One\n- Two\n\nA closing paragraph.",
            )
            .density(MarkdownDensity::Compact)
            .heading_level_start(3)
            .content_width(px(420.))
            .content_align(MarkdownContentAlign::Center),
        )
        .child(label("Inline display inside running text, with GFM autolinks"))
        .child(
            div().max_w(px(560.)).child(
                Text::new("Inline: ").child(
                    Markdown::new(
                        "markdown-inline",
                        "**bold**, *italic* and a bare https://example.com/docs link.",
                    )
                    .display(TextDisplay::Inline)
                    .autolink(MarkdownAutolink::Gfm),
                ),
            ),
        )
        .into_any_element()
}
