//! Markdown tests, after its spec's FR items and upstream's `Markdown.test.tsx`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    IntoElement, Modifiers, ParentElement, Pixels, Styled, TestAppContext, VisualTestContext, div,
    point, px,
};
use rustryx_markdown::{ParseOptions, parse_markdown_ast};
use rustryx_primitives::{LinkHandler, link_provider};

use super::markdown::{code_fence, fence_language, table_column_floors};
use super::streaming::*;
use super::*;
use crate::content::{CitationSource, CitationVariant};
use crate::segmented::tests::host;
use crate::text::TextDisplay;

type Log = Rc<RefCell<Vec<String>>>;
type Headings = Rc<RefCell<Vec<(u8, Option<String>)>>>;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

/// A Markdown in a 900px column, its links reported to `log`.
fn page(
    cx: &mut TestAppContext,
    log: Log,
    build: impl Fn() -> Markdown + 'static,
) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        let log = log.clone();
        let handler = LinkHandler::new(move |navigation, _, _| {
            log.borrow_mut().push(format!(
                "open {} {}",
                navigation.href,
                navigation.target.as_deref().unwrap_or("-")
            ));
        });
        div()
            .w(px(900.))
            .child(link_provider(handler, build()))
            .into_any_element()
    });
    settle(cx);
    cx
}

fn blocks(source: &str) -> Vec<rustryx_markdown::ast::Block> {
    parse_markdown_ast(source, &ParseOptions::default()).children
}

fn spacing_step(
    cx: &mut VisualTestContext,
    step: impl Fn(&rustryx_theme::Tokens) -> gpui::AbsoluteLength,
) -> Pixels {
    cx.update(|window, app| {
        let theme = rustryx_theme::current(window, app);
        step(theme.tokens()).to_pixels(window.rem_size())
    })
}

/// The space between two blocks' wrappers.
fn gap(cx: &mut VisualTestContext, above: &'static str, below: &'static str) -> Pixels {
    let a = cx.debug_bounds(above).unwrap();
    let b = cx.debug_bounds(below).unwrap();
    b.top() - a.bottom()
}

#[test]
fn root_headings_take_unique_slugs_and_quoted_ones_none() {
    let root = parse_markdown_ast(
        "# Setup\n\n## Setup\n\n> # Quoted\n\n# Hello, World!\n\n# !!!\n\n# !!!",
        &ParseOptions::default(),
    );
    let projection = rustryx_markdown::project_markdown_headings(&root, None);
    let ids: Vec<_> = projection
        .outline
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(
        ids,
        ["setup", "setup-1", "hello-world", "section", "section-1"]
    );
    assert!(
        projection.heading(2).is_none(),
        "the quoted heading has no id"
    );
}

#[test]
fn fr29_fences_pick_the_diff_layer_by_their_first_word() {
    let fence = |source: &str| match blocks(source).remove(0) {
        rustryx_markdown::ast::Block::Code(code) => code_fence(&code),
        other => panic!("not code: {other:?}"),
    };
    assert_eq!(fence("```diff\n-a\n```"), ("plaintext".into(), true));
    assert_eq!(fence("```diff-tsx\n-a\n```"), ("tsx".into(), true));
    assert_eq!(fence("```ts\nconst a = 1;\n```"), ("ts".into(), false));
    assert_eq!(fence("```\nplain\n```"), ("plaintext".into(), false));
    assert_eq!(
        fence("```c++\nint a;\n```"),
        ("c".into(), false),
        "the leading word"
    );
}

#[test]
fn fr26_table_columns_floor_from_their_own_content() {
    let table = |source: &str| match blocks(source).remove(0) {
        rustryx_markdown::ast::Block::Table(table) => table_column_floors(&table),
        other => panic!("not a table: {other:?}"),
    };
    let long = "x".repeat(120);
    assert_eq!(
        table(&format!(
            "| Key | Meaning | Note |\n| --- | --- | --- |\n| id | Stable identifier never reused | {long} |"
        )),
        [4, 15, 24]
    );
    assert_eq!(
        table(
            "| Component name | Accessibility status and remediation owner | X |\n| --- | --- | --- |\n| Button | Pass | 1 |"
        ),
        [14, 21, 4],
        "a header floor up to its cap"
    );
    assert_eq!(
        table("| Status | Owner |\n| --- | --- |"),
        [6, 5],
        "header-only"
    );
    assert_eq!(
        table("| A | B | C |\n| --- | --- | --- |\n|  | middle only |  |"),
        [4, 6, 4],
        "empty cells at the minimum"
    );
}

#[gpui::test]
fn fr1_block_display_renders_blocks_and_inline_display_none(cx: &mut TestAppContext) {
    {
        let cx = page(cx, Log::default(), || {
            Markdown::new("md", "# Title\n\nText")
        });
        assert!(cx.debug_bounds("md-block-0").is_some());
        assert!(cx.debug_bounds("md-block-1-paragraph").is_some());
    }
    let cx = page(cx, Log::default(), || {
        Markdown::new("inline", "**bold** text").display(TextDisplay::Inline)
    });
    assert!(
        cx.debug_bounds("inline-block-0").is_none(),
        "no block anatomy"
    );
}

#[gpui::test]
fn fr3_a_heading_renderer_takes_the_level_and_root_slug(cx: &mut TestAppContext) {
    let seen: Headings = Rc::default();
    let log = seen.clone();
    let _cx = page(cx, Log::default(), move || {
        let log = log.clone();
        Markdown::new("md", "# Alpha\n\n### Beta\n\n##### Deep\n\n> # Quoted")
            .heading_level_start(3)
            .components(MarkdownComponents::new().heading(move |props, _, _| {
                log.borrow_mut()
                    .push((props.level, props.id.map(|id| id.to_string())));
                div().children(props.children).into_any_element()
            }))
    });
    let seen = seen.borrow();
    assert_eq!(
        seen[..4],
        [
            (3, Some("alpha".into())),
            (5, Some("beta".into())),
            (6, Some("deep".into())),
            (3, None),
        ],
        "shifted, capped at 6; quoted headings have no id"
    );
}

#[gpui::test]
fn fr29_a_code_renderer_receives_the_leading_word_and_nothing_new(cx: &mut TestAppContext) {
    let seen: Log = Rc::default();
    let log = seen.clone();
    let _cx = page(cx, Log::default(), move || {
        let log = log.clone();
        Markdown::new(
            "md",
            "```diff\n-a\n```\n\n```diff-tsx\n+b\n```\n\n```\nplain\n```",
        )
        .components(MarkdownComponents::new().code(move |props, _, _| {
            log.borrow_mut()
                .push(format!("{}:{}", props.language, props.code));
            div().into_any_element()
        }))
    });
    assert_eq!(
        seen.borrow()[..3],
        ["diff:-a", "diff:+b", "plaintext:plain"]
    );
}

#[gpui::test]
fn links_open_their_destination_external_ones_in_a_new_tab(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = page(cx, log.clone(), || {
        Markdown::new("md", "[Docs](https://example.com/docs \"Title\")")
    });
    let at = cx.debug_bounds("md-block-0").unwrap().origin + point(px(8.), px(8.));
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
    assert_eq!(
        *log.borrow(),
        ["open https://example.com/docs _blank"],
        "the destination without its title"
    );
}

#[gpui::test]
fn on_link_click_runs_first_and_false_keeps_the_link_from_opening(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let clicks = log.clone();
    let cx = page(cx, log.clone(), move || {
        let clicks = clicks.clone();
        Markdown::new("md", "[Settings](/settings)").on_link_click(move |href, _, _, _| {
            clicks.borrow_mut().push(format!("click {href}"));
            false
        })
    });
    let at = cx.debug_bounds("md-block-0").unwrap().origin + point(px(8.), px(8.));
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
    assert_eq!(*log.borrow(), ["click /settings"]);
}

#[gpui::test]
fn unsafe_destinations_render_as_text(cx: &mut TestAppContext) {
    let seen: Log = Rc::default();
    let log = seen.clone();
    let _cx = page(cx, Log::default(), move || {
        let log = log.clone();
        Markdown::new(
            "md",
            "[bad](javascript:alert(1)) [good](https://example.com) ![x](data:text/html,y)",
        )
        .components(MarkdownComponents::new().link(move |props, _, _| {
            log.borrow_mut().push(props.href.to_string());
            div().children(props.children).into_any_element()
        }))
    });
    let mut seen = seen.borrow().clone();
    seen.dedup();
    assert_eq!(seen, ["https://example.com"], "only the safe link");
}

#[gpui::test]
fn autolinks_only_with_the_gfm_option(cx: &mut TestAppContext) {
    let render = |cx: &mut TestAppContext, autolink: bool| {
        let seen: Log = Rc::default();
        let log = seen.clone();
        let _cx = page(cx, Log::default(), move || {
            let log = log.clone();
            let md = Markdown::new(
                "md",
                "See https://example.com and www.example.org or me@example.com",
            )
            .components(MarkdownComponents::new().link(move |props, _, _| {
                log.borrow_mut().push(props.href.to_string());
                div().children(props.children).into_any_element()
            }));
            if autolink {
                md.autolink(MarkdownAutolink::Gfm)
            } else {
                md
            }
        });
        seen.borrow().clone()
    };
    assert!(render(cx, false).is_empty(), "plain text by default");
    let mut links = render(cx, true);
    links.dedup();
    assert_eq!(
        links[..3],
        [
            "https://example.com",
            "http://www.example.org",
            "mailto:me@example.com"
        ]
    );
}

#[gpui::test]
fn block_margins_collapse_and_follow_the_density(cx: &mut TestAppContext) {
    {
        let cx = page(cx, Log::default(), || {
            Markdown::new("md", "One\n\nTwo\n\n## Three\n\nFour")
        });
        let s3 = spacing_step(cx, |t| t.spacing.s3);
        let s6 = spacing_step(cx, |t| t.spacing.s6);
        assert_eq!(
            gap(cx, "md-block-0", "md-block-1"),
            s3,
            "paragraph to paragraph"
        );
        assert_eq!(
            gap(cx, "md-block-1", "md-block-2"),
            s6,
            "the larger margin wins"
        );
    }
    let cx = page(cx, Log::default(), || {
        Markdown::new("dense", "One\n\nTwo").density(MarkdownDensity::Compact)
    });
    let s1 = spacing_step(cx, |t| t.spacing.s1);
    assert_eq!(gap(cx, "dense-block-0", "dense-block-1"), s1);
}

#[gpui::test]
fn prose_is_capped_at_the_content_width(cx: &mut TestAppContext) {
    let width = |cx: &mut TestAppContext, md: fn() -> Markdown| {
        let cx = page(cx, Log::default(), md);
        cx.debug_bounds("md-block-0-paragraph").unwrap().size.width
    };
    assert_eq!(
        width(cx, || Markdown::new("md", "Text")),
        px(680.),
        "the 680px measure"
    );
    assert_eq!(
        width(cx, || Markdown::new("md", "Text").content_width(px(300.))),
        px(300.)
    );
    assert_eq!(
        width(cx, || Markdown::new("md", "Text")
            .content_width(gpui::Length::Auto)),
        px(900.),
        "no cap"
    );
    assert_eq!(
        width(cx, || Markdown::new("md", "Text").content_width(px(0.))),
        px(900.),
        "zero drops the cap too"
    );
}

#[gpui::test]
fn content_align_centres_capped_prose(cx: &mut TestAppContext) {
    let cx = page(cx, Log::default(), || {
        Markdown::new("md", "Text")
            .content_width(px(300.))
            .content_align(MarkdownContentAlign::Center)
    });
    let column = cx.debug_bounds("md-block-0").unwrap();
    let bounds = cx.debug_bounds("md-block-0-paragraph").unwrap();
    assert_eq!(
        bounds.left() - column.left(),
        px(300.),
        "centred in the 900px column"
    );
}

#[gpui::test]
fn lists_tasks_and_tables_render_their_parts(cx: &mut TestAppContext) {
    let cx = page(cx, Log::default(), || {
        Markdown::new(
            "md",
            "- one\n- [x] done\n  - nested\n\nBetween\n\n- [ ] a\n- [x] b\n\n| a | b |\n|---|:-:|\n| 1 | 2 |",
        )
    });
    // The first list mixes tasks and plain items and stays one List (FR23).
    assert!(cx.debug_bounds("md-block-0-item-0").is_some());
    assert!(
        cx.debug_bounds("md-block-0-item-1-0").is_some(),
        "a nested block"
    );
    assert!(
        cx.debug_bounds("md-block-2-item-1").is_some(),
        "a task list row"
    );
    assert!(cx.debug_bounds("md-block-3-table-header-0-1").is_some());
    assert!(cx.debug_bounds("md-block-3-table-body-0-1").is_some());
}

#[gpui::test]
fn code_blocks_fit_their_content_at_least_the_measure_wide(cx: &mut TestAppContext) {
    let cx = page(cx, Log::default(), || {
        Markdown::new("md", "```ts\nconst a = 1;\n```")
    });
    assert_eq!(
        cx.debug_bounds("md-block-0-code").unwrap().size.width,
        px(680.),
        "min(680px, 100%), not the whole column"
    );
}

/// Runs a Markdown whose renderers write to a log, and returns the log.
fn rendered(cx: &mut TestAppContext, build: impl Fn(Log) -> Markdown + 'static) -> Vec<String> {
    let seen: Log = Rc::default();
    let log = seen.clone();
    let _cx = page(cx, Log::default(), move || build(log.clone()));
    let mut seen = seen.borrow().clone();
    // Every render logs again; keep the first pass.
    if let Some(repeat) = (1..=seen.len())
        .find(|&n| seen.len().is_multiple_of(n) && seen.chunks(n).all(|c| c == &seen[..n]))
    {
        seen.truncate(repeat);
    }
    seen
}

#[gpui::test]
fn fr7_a_math_renderer_receives_inline_and_display_math(cx: &mut TestAppContext) {
    let seen = rendered(cx, |log| {
        Markdown::new(
            "md",
            "Energy $E=mc^2$ here.\n\n$$\na + b\n$$\n\nPrice $5 and $10.",
        )
        .components(MarkdownComponents::new().math(move |props, _, _| {
            log.borrow_mut()
                .push(format!("{:?}:{}", props.display, props.value));
            div().into_any_element()
        }))
    });
    assert_eq!(
        seen,
        ["Inline:E=mc^2", "Block:a + b"],
        "currency stays text (FR8)"
    );
}

#[gpui::test]
fn citations_number_their_sources_by_first_appearance(cx: &mut TestAppContext) {
    let seen = rendered(cx, |log| {
        Markdown::new("md", "See [b] then [a], again [b], and [c].")
            .sources([
                (
                    "a",
                    CitationSource {
                        title: Some("Alpha".into()),
                        ..Default::default()
                    },
                ),
                (
                    "b",
                    CitationSource {
                        title: Some("Beta".into()),
                        ..Default::default()
                    },
                ),
            ])
            .citation_style(CitationVariant::Number)
            .components(MarkdownComponents::new().citation(move |props, _, _| {
                log.borrow_mut().push(format!(
                    "{} {} {:?}",
                    props.number,
                    props.source.title.unwrap_or_default(),
                    props.variant
                ));
                div().into_any_element()
            }))
    });
    assert_eq!(
        seen,
        ["1 Beta Number", "2 Alpha Number", "1 Beta Number"],
        "[c] is no source"
    );
}

#[gpui::test]
fn inline_plugins_match_prose_only_and_the_first_plugin_wins(cx: &mut TestAppContext) {
    let seen = rendered(cx, |log| {
        let tickets = log.clone();
        let words = log.clone();
        Markdown::new(
            "md",
            "Fixes #12 and #34, not `#56`.\n\n```\n#78\n```\n\n**see #90**",
        )
        .inline_plugins([
            MarkdownInlinePlugin::new(
                fancy_regex::Regex::new(r"#(\d+)").unwrap(),
                move |m, _, _| {
                    tickets.borrow_mut().push(format!("ticket {}", &m[1]));
                    div().into_any_element()
                },
            )
            .end_index(|_, m| {
                let whole = m.get(0).unwrap();
                (whole.as_str() != "#34").then_some(whole.end())
            }),
            MarkdownInlinePlugin::new(fancy_regex::Regex::new(r"#\d").unwrap(), move |m, _, _| {
                words.borrow_mut().push(format!("second {}", &m[0]));
                div().into_any_element()
            }),
        ])
    });
    assert_eq!(
        seen,
        ["ticket 12", "second #3", "ticket 90"],
        "#34 skipped by end_index and left to the next plugin; code is never matched"
    );
}

#[test]
fn fr30_listed_languages_claim_their_identifiers() {
    let code = |source: &str| match blocks(source).remove(0) {
        rustryx_markdown::ast::Block::Code(code) => code,
        other => panic!("not code: {other:?}"),
    };
    let listed = [rustryx_syntax::languages::graphql()];
    let name =
        |input: &rustryx_syntax::CodeLanguageInput| rustryx_syntax::language_key(input).to_string();
    let (language, diff) = fence_language(&code("```graphql\nquery { a }\n```"), &listed);
    assert!(matches!(language, rustryx_syntax::CodeLanguageInput::Language(_)) && !diff);
    let (language, _) = fence_language(&code("```graphql\nquery { a }\n```"), &[]);
    assert!(
        matches!(language, rustryx_syntax::CodeLanguageInput::Name(_)),
        "unlisted: the plain name"
    );
    let (language, diff) = fence_language(&code("```diff-graphql\n-a\n```"), &listed);
    assert!(diff && matches!(language, rustryx_syntax::CodeLanguageInput::Language(_)));
    assert_eq!(name(&language), "graphql");
}

/// Renders `use_streaming_text` and logs what it shows.
#[derive(IntoElement)]
struct RevealProbe {
    target: &'static str,
    streaming: bool,
    log: Log,
}

impl gpui::RenderOnce for RevealProbe {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let shown = use_streaming_text(
            "probe",
            self.target,
            self.streaming,
            Default::default(),
            window,
            cx,
        );
        self.log.borrow_mut().push(shown.to_string());
        div().child(shown)
    }
}

#[gpui::test]
fn streaming_text_reveals_steadily_and_snaps_when_streaming_stops(cx: &mut TestAppContext) {
    const TARGET: &str = "The quick brown fox jumps over the lazy dog, again and again and again.";
    let log: Log = Rc::default();
    let streaming = Rc::new(std::cell::Cell::new(true));
    let cx = {
        let (log, streaming) = (log.clone(), streaming.clone());
        host(cx, move || {
            RevealProbe {
                target: TARGET,
                streaming: streaming.get(),
                log: log.clone(),
            }
            .into_any_element()
        })
    };
    let first = log.borrow().last().unwrap().clone();
    assert!(first.len() < TARGET.len(), "starts partway: {first:?}");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(30));
    settle(cx);
    let later = log.borrow().last().unwrap().clone();
    assert!(
        later.len() > first.len() && TARGET.starts_with(&later),
        "grows toward the target"
    );
    streaming.set(false);
    settle(cx);
    assert_eq!(
        log.borrow().last().unwrap(),
        TARGET,
        "the whole text once streaming stops"
    );
}

#[gpui::test]
fn streamed_markdown_renders_its_revealed_text(cx: &mut TestAppContext) {
    let seen = rendered(cx, |log| {
        Markdown::new("md", "**Bold** words, then `code`.")
            .streaming(true)
            .components(MarkdownComponents::new().inline_code(move |value, _, _| {
                log.borrow_mut().push(value.to_string());
                div().into_any_element()
            }))
    });
    // The first ticks reveal the start; the code span arrives later or not yet.
    assert!(seen.len() <= 1);
}

/// An inline `@{name}` mention plugin whose UI renderer logs, or fails for "bad".
fn mentions(log: Log) -> rustryx_markdown::plugins::MarkdownPlugin {
    use rustryx_markdown::ast::{ExtensionDisplay, ExtensionNode};
    use rustryx_markdown::plugins::{
        ExtensionRenderer, PluginDefinition, SyntaxCapability, SyntaxContribution, TokenizeResult,
        create_markdown_plugin,
    };
    create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![SyntaxContribution::new(["@{"], 80, |input| {
                let Some(close) = input.source[input.offset + 2..].find('}') else {
                    return Ok(TokenizeResult::NoMatch);
                };
                let close = input.offset + 2 + close;
                Ok(TokenizeResult::Match {
                    end: close + 1,
                    node: ExtensionNode {
                        plugin: "mentions".into(),
                        name: "mention".into(),
                        data: serde_json::json!({"label": &input.source[input.offset + 2..close]}),
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
                Ok(format!("@{}", node.data["label"].as_str().unwrap_or("")))
            })
            .with_render(markdown_extension_render(move |node, _, _| {
                let label = node.data["label"].as_str().unwrap_or("").to_string();
                if label == "bad" {
                    return Err("no".into());
                }
                log.borrow_mut().push(format!("mention {label}"));
                Ok(div().into_any_element())
            })),
        )]
        .into(),
        ..PluginDefinition::new("mentions")
    })
    .expect("a valid plugin")
}

#[gpui::test]
fn fr13_plugin_syntax_renders_through_its_renderer_and_falls_back_on_failure(
    cx: &mut TestAppContext,
) {
    let seen = rendered(cx, |log| {
        let diagnostics = log.clone();
        Markdown::new("md", "Hi @{Ada} and @{bad}.")
            .plugins([mentions(log.clone())])
            .on_plugin_diagnostic(move |d| diagnostics.borrow_mut().push(d.to_string()))
    });
    assert_eq!(seen[0], "mention Ada");
    assert!(
        seen.iter()
            .any(|s| s.contains("\"mentions\" failed in render (render-failed)")),
        "a failing renderer reports and shows its text: {seen:?}"
    );
}

#[gpui::test]
fn heading_links_give_every_heading_an_id_and_copy_its_permalink(cx: &mut TestAppContext) {
    use rustryx_markdown::plugins::{HeadingLinksOptions, create_markdown_heading_links};
    let links = create_markdown_heading_links(HeadingLinksOptions {
        permalink_base_url: Some("https://docs.example/page".into()),
        ..Default::default()
    })
    .unwrap();
    let seen = rendered(cx, {
        let links = links.clone();
        move |log| {
            Markdown::new("md", "# Intro\n\n> ## Quoted")
                .plugins([links.clone()])
                .components(MarkdownComponents::new().heading(move |props, _, _| {
                    log.borrow_mut()
                        .push(format!("{:?}", props.id.map(|id| id.to_string())));
                    div().into_any_element()
                }))
        }
    });
    assert_eq!(
        seen,
        ["Some(\"intro\")", "Some(\"quoted\")"],
        "nested headings too"
    );

    let cx = page(cx, Log::default(), move || {
        Markdown::new("md", "# Intro").plugins([links.clone()])
    });
    // Rest the pointer on the control (it shows once the row is hovered), then press it.
    let copy = cx.debug_bounds("md-block-0-heading-links-copy").unwrap();
    cx.simulate_mouse_move(copy.center(), None, Modifiers::none());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    settle(cx);
    cx.simulate_click(copy.center(), Modifiers::none());
    settle(cx);
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(copied.as_deref(), Some("https://docs.example/page#intro"));
}

#[gpui::test]
fn source_lines_reach_custom_renderers(cx: &mut TestAppContext) {
    let seen = rendered(cx, |log| {
        Markdown::new("md", "One\n\nTwo\nlines")
            .plugins([rustryx_markdown::plugins::markdown_source_lines_plugin()])
            .components(MarkdownComponents::new().paragraph(move |props, _, _| {
                log.borrow_mut().push(format!(
                    "{:?}",
                    props.source_lines.map(|l| (l.start, l.end))
                ));
                div().into_any_element()
            }))
    });
    assert_eq!(seen, ["Some((1, 1))", "Some((3, 4))"]);
}

#[gpui::test]
fn heading_links_take_24_px_where_any_pointer_is_coarse(cx: &mut TestAppContext) {
    use rustryx_markdown::plugins::{HeadingLinksOptions, create_markdown_heading_links};
    let links = create_markdown_heading_links(HeadingLinksOptions {
        permalink_base_url: Some("https://docs.example/page".into()),
        ..Default::default()
    })
    .unwrap();
    let cx = page(cx, Log::default(), move || {
        Markdown::new("md", "# Intro").plugins([links.clone()])
    });
    // A fine device with no touch screen: the small element size.
    cx.update(|_, cx| rustryx_primitives::modality::set_primary_pointer_coarse(Some(false), cx));
    settle(cx);
    let fine = cx
        .debug_bounds("md-block-0-heading-links-copy")
        .unwrap()
        .size
        .width;
    let small = cx.update(|window, cx| {
        rustryx_theme::current(window, cx)
            .tokens()
            .size
            .element_sm
            .to_pixels(window.rem_size())
    });
    assert_eq!(fine, small);
    cx.update(|_, cx| rustryx_primitives::modality::set_primary_pointer_coarse(Some(true), cx));
    settle(cx);
    let coarse = cx
        .debug_bounds("md-block-0-heading-links-copy")
        .unwrap()
        .size
        .width;
    assert_eq!(coarse, px(24.), "COARSE_TARGET");
}
