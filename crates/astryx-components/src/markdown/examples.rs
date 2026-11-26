//! Markdown examples for the gallery.

use std::time::Duration;

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Styled, Window, div, px,
};

use super::markdown_extension_render;
use super::{
    Markdown, MarkdownAutolink, MarkdownComponents, MarkdownContentAlign, MarkdownDensity,
    MarkdownInlinePlugin, MarkdownMathDisplay,
};
use crate::content::{CitationSource, CitationVariant, Code};
use crate::layout::{SpacingStep, Stack};
use crate::outline::{Outline, ParseOutlineFromMarkdownOptions, use_outline_from_markdown};
use crate::status::{Badge, BadgeVariant};
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

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
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
        .child(label("Sources: [id] markers become citation chips, numbered by first use"))
        .child(
            Markdown::new(
                "markdown-citations",
                "Rust's ownership rules prevent data races [rust-book], and GPUI renders on the GPU [gpui]. \
                 Both come up again later [rust-book].",
            )
            .sources([
                (
                    "rust-book",
                    CitationSource {
                        title: Some("The Rust Book".into()),
                        url: Some("https://doc.rust-lang.org/book/".into()),
                        ..Default::default()
                    },
                ),
                (
                    "gpui",
                    CitationSource {
                        title: Some("GPUI".into()),
                        url: Some("https://www.gpui.rs".into()),
                        ..Default::default()
                    },
                ),
            ]),
        )
        .child(
            Markdown::new("markdown-citation-numbers", "Numbered badges instead [rust-book].")
                .sources([("rust-book", CitationSource::default())])
                .citation_style(CitationVariant::Number),
        )
        .child(label("A math renderer turns on $…$ and $$…$$; currency like $5 and $10 stays text"))
        .child(
            Markdown::new(
                "markdown-math",
                "Euler's identity $e^{i\\pi} + 1 = 0$ costs $5 and $10.\n\n$$\n\\sum_{k=1}^{n} k = \\frac{n(n+1)}{2}\n$$",
            )
            .components(MarkdownComponents::new().math(|props, _, _| match props.display {
                MarkdownMathDisplay::Inline => Code::new(props.value).into_any_element(),
                MarkdownMathDisplay::Block => div()
                    .p(px(12.))
                    .rounded(px(6.))
                    .border_1()
                    .child(Code::new(props.value))
                    .into_any_element(),
            })),
        )
        .child(label("An inline plugin: ticket numbers in prose become badges, never in code"))
        .child(
            Markdown::new(
                "markdown-inline-plugins",
                "Fixed in #1204 and #1310; `#1400` is a literal.",
            )
            .inline_plugins([MarkdownInlinePlugin::new(
                fancy_regex::Regex::new(r"#(\d+)").expect("a valid pattern"),
                |m, _, _| {
                    Badge::new(format!("#{}", &m[1]))
                        .variant(BadgeVariant::Info)
                        .into_any_element()
                },
            )]),
        )
        .child(label("Plugins: an @{mention} syntax with its own renderer, and heading links (hover a heading)"))
        .child(plugins_example(window, cx))
        .child(label("Streaming: the reply reveals steadily and its newest text fades in"))
        .child(StreamingDemo { id: "markdown-streaming".into() })
        .into_any_element()
}

const REPLY: &str = "## Streaming replies

Markdown parses the text **as it arrives**: each new span fades in, and a list or \
table appears once its syntax is complete.

- Steady reveal from `use_streaming_text`
- Newest text fades in over the medium duration

Then the reply finishes and settles.";

/// Replays a reply as if it were streaming, every few seconds.
#[derive(IntoElement)]
struct StreamingDemo {
    id: ElementId,
}

impl RenderOnce for StreamingDemo {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let now = cx.background_executor().now();
        let start = window.use_keyed_state(
            ElementId::Name(format!("{}-start", self.id).into()),
            cx,
            |_, cx| cx.background_executor().now(),
        );
        let elapsed = now.duration_since(*start.read(cx));
        // About 40 characters a second, then a pause before it replays.
        let arrived = (elapsed.as_secs_f32() * 40.) as usize;
        if elapsed > Duration::from_secs(14) {
            start.update(cx, |s, _| *s = now);
        }
        let mut end = arrived.min(REPLY.len());
        while !REPLY.is_char_boundary(end) {
            end -= 1;
        }
        window.request_animation_frame();
        // A fixed height, so the page does not grow as the reply arrives.
        div()
            .max_w(px(640.))
            .h(px(300.))
            .child(Markdown::new(self.id, &REPLY[..end]).streaming(end < REPLY.len()))
    }
}

const GUIDE: &str = "## Getting started

Ask @{Ada} or @{Grace} for access.

### Install

Run the installer, then sign in.";

/// A mention syntax plugin, heading links, and the Outline the same plugins give.
fn plugins_example(window: &mut Window, cx: &mut App) -> AnyElement {
    use astryx_markdown::ast::{ExtensionDisplay, ExtensionNode};
    use astryx_markdown::plugins::{
        ExtensionRenderer, HeadingLinksOptions, PluginDefinition, SyntaxCapability,
        SyntaxContribution, TokenizeResult, create_markdown_heading_links, create_markdown_plugin,
    };
    let mentions = create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![SyntaxContribution::new(["@{"], 64, |input| {
                let rest = &input.source[input.offset + 2..input.end];
                let Some(close) = rest.find('}') else {
                    return Ok(if input.is_final {
                        TokenizeResult::NoMatch
                    } else {
                        TokenizeResult::Defer
                    });
                };
                Ok(TokenizeResult::Match {
                    end: input.offset + 2 + close + 1,
                    node: ExtensionNode {
                        plugin: "mentions".into(),
                        name: "mention".into(),
                        data: serde_json::json!({ "name": &rest[..close] }),
                        display: ExtensionDisplay::Inline,
                        source: None,
                        position: None,
                    },
                })
            })],
            block: vec![],
        }),
        renderers: [(
            "mention".to_string(),
            ExtensionRenderer::new(|node| {
                Ok(format!("@{}", node.data["name"].as_str().unwrap_or("")))
            })
            .with_render(markdown_extension_render(|node, _, _| {
                let name = node.data["name"].as_str().unwrap_or("").to_string();
                Ok(Badge::new(format!("@{name}"))
                    .variant(BadgeVariant::Purple)
                    .into_any_element())
            })),
        )]
        .into(),
        ..PluginDefinition::new("mentions")
    })
    .expect("a valid plugin");
    let links = create_markdown_heading_links(HeadingLinksOptions {
        permalink_base_url: Some("https://docs.example/guide".into()),
        ..Default::default()
    })
    .expect("a valid base URL");
    let plugins = vec![mentions, links];
    let items = use_outline_from_markdown(
        "markdown-guide-outline",
        GUIDE,
        ParseOutlineFromMarkdownOptions {
            plugins: plugins.clone(),
            is_final: true,
        },
        window,
        cx,
    );
    div()
        .flex()
        .gap(px(32.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(Markdown::new("markdown-guide", GUIDE).plugins(plugins)),
        )
        .child(
            div()
                .w(px(200.))
                .child(Outline::new("markdown-guide-outline", items)),
        )
        .into_any_element()
}
