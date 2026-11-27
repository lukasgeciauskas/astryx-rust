//! Upstream's text-transform helper and soft-breaks plugin tests
//! (`Markdown/plugins/textTransform.test.ts`, `Markdown/plugins/softBreaks.test.tsx`), on
//! the canonical tree. Expectations were checked against upstream's `parseMarkdownAst` and
//! `parseInlineAst` (the released projection's `content`/`href`/`src` read here as
//! `value`/`url`); offsets are UTF-8 bytes.
//!
//! Skipped or partly ported upstream cases:
//!
//! - `finds required substrings in source time linear in its length`: skipped, it spies on
//!   `String.prototype.indexOf` to count scanned units (a work-count budget).
//! - `requires a global pattern and keeps the helper typed as a transform`: the global-flag
//!   check has no Rust form (a `regex::Regex` always scans globally), a non-string
//!   substring (`[42]`) cannot be built, and the `@ts-expect-error`/`expectTypeOf` checks
//!   are TypeScript types; the empty-list and empty-string substring rejections are ported.
//! - `adopts a fresh replacement node and freezes it against later writes`: the
//!   `Object.isFrozen` and late-write `TypeError` checks are JavaScript object mechanics
//!   (Rust hands values over by move); the document and callback count are ported.
//! - `copies a replacement node the callback hands over more than once`: the `not.toBe`
//!   object-identity check has no Rust meaning; the equal documents are ported.
//! - `still rejects an unrepresentable node without freezing the batch`: the
//!   `Object.isFrozen` check is skipped; the `console.warn` is read as the plugin's
//!   transform-phase diagnostic.
//! - `advances declined zero-width matches by Unicode code point`: the `regex` crate has no
//!   lookahead, so `(?=😀)` is written as the empty pattern, which also matches zero-width
//!   before the emoji (and at the end).
//! - `renders hard breaks during server rendering`: skipped, React server rendering and a
//!   TypeScript `expectTypeOf` check.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use regex::Regex;
use rustryx_markdown::ast::{
    Block, ExtensionDisplay, ExtensionNode, Image, Link, Literal, NodeType, Phrasing, Position,
    Root, visit_markdown_nodes,
};
use rustryx_markdown::plugins::{
    ExtensionRenderer, MarkdownPlugin, PluginDefinition, PluginDiagnostic, PluginPhase,
    TextParentType, TextTransformOptions, create_markdown_plugin, create_markdown_text_transform,
    markdown_soft_breaks_plugin,
};
use rustryx_markdown::{ParseOptions, parse_inline_ast, parse_markdown_ast};
use serde_json::{Value, json};

fn text(value: &str) -> Phrasing {
    Phrasing::Text(Literal {
        value: value.into(),
        ..Literal::default()
    })
}

fn with_plugins(plugins: Vec<MarkdownPlugin>) -> ParseOptions {
    ParseOptions {
        plugins,
        ..ParseOptions::default()
    }
}

fn parse(source: &str, plugins: Vec<MarkdownPlugin>) -> Root {
    parse_markdown_ast(source, &with_plugins(plugins))
}

fn to_json(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value).expect("the tree serializes")
}

/// A root of one top-level paragraph spanning `0..end`.
fn paragraph_root(children: Value, end: usize) -> Value {
    json!({
        "type": "root",
        "children": [{
            "type": "paragraph",
            "children": children,
            "position": {"start": {"offset": 0}, "end": {"offset": end}},
        }],
    })
}

/// A text-transform plugin named `name` over `options`, owning no extension nodes.
fn text_plugin(name: &str, options: TextTransformOptions) -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        transform: Some(create_markdown_text_transform(options).expect("valid options")),
        ..PluginDefinition::new(name)
    })
    .expect("a valid plugin")
}

fn required(substrings: &[&str]) -> Option<Vec<String>> {
    Some(substrings.iter().map(|s| s.to_string()).collect())
}

/// Upstream `todoPlugin`: every `TODO` becomes an inline `todo` extension node.
fn todo_plugin(on_replace: impl Fn(TextParentType) + 'static) -> MarkdownPlugin {
    let transform = create_markdown_text_transform(TextTransformOptions {
        required_substrings: required(&["TODO"]),
        ..TextTransformOptions::new(Regex::new("TODO").unwrap(), move |_, context| {
            on_replace(context.parent_type);
            vec![Phrasing::Extension(ExtensionNode {
                plugin: "todo-labels".into(),
                name: "todo".into(),
                data: json!({"label": "TODO"}),
                display: ExtensionDisplay::Inline,
                source: None,
                position: None,
            })]
        })
    })
    .expect("valid options");
    let renderers: BTreeMap<String, ExtensionRenderer> = [(
        "todo".to_string(),
        ExtensionRenderer::new(|node| Ok(node.data["label"].as_str().unwrap_or("").to_string())),
    )]
    .into();
    create_markdown_plugin(PluginDefinition {
        transform: Some(transform),
        renderers,
        ..PluginDefinition::new("todo-labels")
    })
    .expect("a valid plugin")
}

fn extension_count(root: &Root) -> usize {
    let mut count = 0;
    visit_markdown_nodes(root, NodeType::Extension, &mut |_| count += 1);
    count
}

fn todo_json() -> Value {
    json!({
        "type": "extension",
        "plugin": "todo-labels",
        "name": "todo",
        "display": "inline",
        "data": {"label": "TODO"},
    })
}

// --- createMarkdownTextTransform -------------------------------------------------------

/// Upstream: "replaces eligible prose and preserves protected contexts".
#[test]
fn replaces_eligible_prose_and_preserves_protected_contexts() {
    let parents = Rc::new(RefCell::new(Vec::new()));
    let seen = parents.clone();
    let plugin = todo_plugin(move |parent| seen.borrow_mut().push(parent));
    let source = [
        "TODO **TODO** [TODO](/tasks) `TODO` [source]",
        "",
        "> TODO",
        "",
        "| Item |",
        "| --- |",
        "| TODO |",
        "",
        "```text",
        "TODO",
        "```",
    ]
    .join("\n");
    let root = parse_markdown_ast(
        &source,
        &ParseOptions {
            plugins: vec![plugin],
            source_ids: Some(["source".to_string()].into()),
            ..ParseOptions::default()
        },
    );

    assert_eq!(extension_count(&root), 4);
    assert_eq!(
        *parents.borrow(),
        [
            TextParentType::Paragraph,
            TextParentType::Strong,
            TextParentType::Paragraph,
            TextParentType::TableCell,
        ]
    );
    let tree = to_json(&root);
    assert_eq!(
        tree["children"][0]["children"][4],
        json!({"type": "link", "url": "/tasks", "children": [{"type": "text", "value": "TODO"}]})
    );
    assert_eq!(
        tree["children"][3],
        json!({
            "type": "code",
            "lang": "text",
            "value": "TODO",
            "position": {"start": {"offset": 81}, "end": {"offset": 97}},
        })
    );
    // Upstream's whole canonical tree.
    assert_eq!(
        tree,
        json!({
            "type": "root",
            "children": [
                {
                    "type": "paragraph",
                    "children": [
                        todo_json(),
                        {"type": "text", "value": " "},
                        {"type": "strong", "children": [todo_json()]},
                        {"type": "text", "value": " "},
                        {"type": "link", "url": "/tasks", "children": [{"type": "text", "value": "TODO"}]},
                        {"type": "text", "value": " "},
                        {"type": "inlineCode", "value": "TODO"},
                        {"type": "text", "value": " "},
                        {"type": "citation", "sourceId": "source"},
                    ],
                    "position": {"start": {"offset": 0}, "end": {"offset": 44}},
                },
                {
                    "type": "blockquote",
                    "children": [{"type": "paragraph", "children": [todo_json()]}],
                    "position": {"start": {"offset": 46}, "end": {"offset": 52}},
                },
                {
                    "type": "table",
                    "align": [null],
                    "children": [
                        {"type": "tableRow", "children": [
                            {"type": "tableCell", "children": [{"type": "text", "value": "Item"}]},
                        ]},
                        {"type": "tableRow", "children": [
                            {"type": "tableCell", "children": [todo_json()]},
                        ]},
                    ],
                    "position": {"start": {"offset": 54}, "end": {"offset": 79}},
                },
                {
                    "type": "code",
                    "lang": "text",
                    "value": "TODO",
                    "position": {"start": {"offset": 81}, "end": {"offset": 97}},
                },
            ],
        })
    );
}

/// Upstream: "stores a replacement link or image URL in its normalized form".
#[test]
fn stores_a_replacement_link_or_image_url_in_its_normalized_form() {
    let replace_with = |node: Phrasing| {
        text_plugin(
            "url-replacement",
            TextTransformOptions {
                required_substrings: required(&["X"]),
                ..TextTransformOptions::new(Regex::new("X").unwrap(), move |_, _| {
                    vec![node.clone()]
                })
            },
        )
    };
    let link = |url: &str| {
        Phrasing::Link(Link {
            url: url.into(),
            children: vec![text("go")],
            ..Link::default()
        })
    };
    let image = |url: &str| {
        Phrasing::Image(Image {
            url: url.into(),
            alt: "pic".into(),
            ..Image::default()
        })
    };

    assert_eq!(
        to_json(&parse("X", vec![replace_with(link(" \t "))])),
        paragraph_root(
            json!([{"type": "link", "url": "", "children": [{"type": "text", "value": "go"}]}]),
            1
        )
    );
    assert_eq!(
        to_json(&parse("X", vec![replace_with(image(" "))])),
        paragraph_root(json!([{"type": "image", "url": "", "alt": "pic"}]), 1)
    );
    assert_eq!(
        to_json(&parse(
            "X",
            vec![replace_with(link("  https://example.com/x  "))]
        )),
        paragraph_root(
            json!([{
                "type": "link",
                "url": "https://example.com/x",
                "children": [{"type": "text", "value": "go"}],
            }]),
            1
        )
    );
}

/// Upstream: "runs helpers in plugin order and supports removal".
#[test]
fn runs_helpers_in_plugin_order_and_supports_removal() {
    let replace = |name: &str, pattern: &str, value: &'static str| {
        text_plugin(
            name,
            TextTransformOptions {
                required_substrings: required(&[pattern]),
                ..TextTransformOptions::new(Regex::new(pattern).unwrap(), move |_, _| {
                    if value.is_empty() {
                        Vec::new()
                    } else {
                        vec![text(value)]
                    }
                })
            },
        )
    };

    assert_eq!(
        to_json(&parse(
            "A",
            vec![replace("a-to-b", "A", "B"), replace("b-to-c", "B", "C")]
        )),
        paragraph_root(json!([{"type": "text", "value": "C"}]), 1)
    );
    assert_eq!(
        to_json(&parse("Remove me", vec![replace("remove", "Remove ", "")])),
        paragraph_root(json!([{"type": "text", "value": "me"}]), 9)
    );
}

/// Upstream: "clones a hoisted replacement for each claim".
#[test]
fn clones_a_hoisted_replacement_for_each_claim() {
    let replacement = text("done");
    let plugin = text_plugin(
        "hoisted-replacement",
        TextTransformOptions {
            required_substrings: required(&["X"]),
            ..TextTransformOptions::new(Regex::new("X").unwrap(), move |_, _| {
                vec![replacement.clone()]
            })
        },
    );

    assert_eq!(
        to_json(&parse("X X", vec![plugin])),
        paragraph_root(
            json!([
                {"type": "text", "value": "done"},
                {"type": "text", "value": " "},
                {"type": "text", "value": "done"},
            ]),
            3
        )
    );
}

/// Upstream: "does not invoke callbacks for absent or protected-only matches".
#[test]
fn does_not_invoke_callbacks_for_absent_or_protected_only_matches() {
    let replacements = Rc::new(Cell::new(0));
    let counter = replacements.clone();
    let plugin = todo_plugin(move |_| counter.set(counter.get() + 1));

    parse("No work here.", vec![plugin.clone()]);
    assert_eq!(replacements.get(), 0);
    parse("`TODO`\n\n```text\nTODO\n```", vec![plugin]);
    assert_eq!(replacements.get(), 0);
}

/// Upstream: "uses case-insensitive source claims with case-insensitive patterns".
#[test]
fn uses_case_insensitive_source_claims_with_case_insensitive_patterns() {
    let plugin = text_plugin(
        "case-insensitive",
        TextTransformOptions {
            required_substrings: required(&["todo"]),
            ..TextTransformOptions::new(Regex::new("(?i)todo").unwrap(), |_, _| vec![text("done")])
        },
    );
    assert_eq!(
        to_json(&parse("TODO", vec![plugin])),
        paragraph_root(json!([{"type": "text", "value": "done"}]), 4)
    );

    let unicode_case_fold = text_plugin(
        "unicode-case-fold",
        TextTransformOptions {
            required_substrings: required(&["Σ"]),
            ..TextTransformOptions::new(Regex::new("(?i)Σ").unwrap(), |_, _| vec![text("sigma")])
        },
    );
    // `ς` is one UTF-16 unit upstream, two UTF-8 bytes here.
    assert_eq!(
        to_json(&parse("ς", vec![unicode_case_fold])),
        paragraph_root(json!([{"type": "text", "value": "sigma"}]), 2)
    );

    let digits = text_plugin(
        "digits-without-prefilter",
        TextTransformOptions::new(Regex::new(r"\d+").unwrap(), |found, _| {
            vec![text(&format!("[{}]", &found[0]))]
        }),
    );
    assert_eq!(
        to_json(&parse("Item 42", vec![digits])),
        paragraph_root(
            json!([
                {"type": "text", "value": "Item "},
                {"type": "text", "value": "[42]"},
            ]),
            7
        )
    );
}

/// Upstream: "supports bounded claim extension and decline".
#[test]
fn supports_bounded_claim_extension_and_decline() {
    let replacements = Rc::new(Cell::new(0));
    let counter = replacements.clone();
    let transform = create_markdown_text_transform(TextTransformOptions {
        required_substrings: required(&["#"]),
        get_end_index: Some(Rc::new(|text: &str, found: &regex::Captures<'_>| {
            let start = found.get(0).unwrap().start();
            Some(text[start..].find(' ').map_or(text.len(), |at| start + at))
        })),
        ..TextTransformOptions::new(Regex::new("#").unwrap(), move |found, _| {
            counter.set(counter.get() + 1);
            let index = found.get(0).unwrap().start();
            vec![text(if index == 0 { "tag" } else { "nested" })]
        })
    })
    .unwrap();
    let plugin = create_markdown_plugin(PluginDefinition {
        transform: Some(transform),
        ..PluginDefinition::new("hash-tags")
    })
    .unwrap();

    assert_eq!(
        to_json(&parse_inline_ast("#one #two", &with_plugins(vec![plugin]))),
        json!([
            {"type": "text", "value": "tag"},
            {"type": "text", "value": " "},
            {"type": "text", "value": "nested"},
        ])
    );
    assert_eq!(replacements.get(), 2);
}

/// Upstream: "supports accepted zero-width insertions at text boundaries".
#[test]
fn supports_accepted_zero_width_insertions_at_text_boundaries() {
    let start = text_plugin(
        "insert-start",
        TextTransformOptions::new(Regex::new("^").unwrap(), |_, _| vec![text("[")]),
    );
    let end = text_plugin(
        "insert-end",
        TextTransformOptions::new(Regex::new("$").unwrap(), |_, _| vec![text("]")]),
    );

    assert_eq!(
        to_json(&parse("value", vec![start])),
        paragraph_root(
            json!([
                {"type": "text", "value": "["},
                {"type": "text", "value": "value"},
            ]),
            5
        )
    );
    assert_eq!(
        to_json(&parse("value", vec![end])),
        paragraph_root(
            json!([
                {"type": "text", "value": "value"},
                {"type": "text", "value": "]"},
            ]),
            5
        )
    );
}

/// Upstream: "advances declined zero-width matches by Unicode code point".
#[test]
fn advances_declined_zero_width_matches_by_unicode_code_point() {
    let replacements = Rc::new(Cell::new(0));
    let declining = |name: &str, pattern: &str| {
        let counter = replacements.clone();
        text_plugin(
            name,
            TextTransformOptions {
                required_substrings: required(&["😀"]),
                get_end_index: Some(Rc::new(|_: &str, _: &regex::Captures<'_>| None)),
                ..TextTransformOptions::new(Regex::new(pattern).unwrap(), move |_, _| {
                    counter.set(counter.get() + 1);
                    vec![text("unexpected")]
                })
            },
        )
    };
    // Upstream `(?=😀)`: the regex crate has no lookahead; the empty pattern matches
    // zero-width before the emoji too.
    let astral = declining("astral-zero-width", "");
    let end = declining("end-zero-width", "$");

    // `😀` is two UTF-16 units upstream, four UTF-8 bytes here.
    assert_eq!(
        to_json(&parse("😀", vec![astral, end])),
        paragraph_root(json!([{"type": "text", "value": "😀"}]), 4)
    );
    assert_eq!(replacements.get(), 0);
}

/// Upstream: "requires a global pattern and keeps the helper typed as a transform" (the
/// required-substring rejections; see the module comment).
#[test]
fn requires_a_global_pattern_and_keeps_the_helper_typed_as_a_transform() {
    let options = |substrings: Vec<String>| TextTransformOptions {
        required_substrings: Some(substrings),
        ..TextTransformOptions::new(Regex::new("TODO").unwrap(), |_, _| vec![text("done")])
    };
    let Err(empty) = create_markdown_text_transform(options(Vec::new())) else {
        panic!("an empty list is rejected");
    };
    assert!(empty.message().contains("requiredSubstring"));
    let Err(blank) = create_markdown_text_transform(options(vec![String::new()])) else {
        panic!("an empty substring is rejected");
    };
    assert!(blank.message().contains("requiredSubstrings"));
    assert!(create_markdown_text_transform(options(vec!["TODO".into()])).is_ok());
}

/// Upstream: "adopts a fresh replacement node and freezes it against later writes" (the
/// document; freezing is JavaScript object mechanics).
#[test]
fn adopts_a_fresh_replacement_node_and_freezes_it_against_later_writes() {
    let handed_over = Rc::new(Cell::new(0));
    let counter = handed_over.clone();
    let plugin = text_plugin(
        "fresh-nodes",
        TextTransformOptions {
            required_substrings: required(&["TODO"]),
            ..TextTransformOptions::new(Regex::new("TODO").unwrap(), move |_, _| {
                counter.set(counter.get() + 1);
                vec![text("done")]
            })
        },
    );

    let root = parse("TODO and TODO again", vec![plugin]);
    assert_eq!(handed_over.get(), 2);
    assert_eq!(
        to_json(&root),
        paragraph_root(
            json!([
                {"type": "text", "value": "done"},
                {"type": "text", "value": " and "},
                {"type": "text", "value": "done"},
                {"type": "text", "value": " again"},
            ]),
            19
        )
    );
}

/// Upstream: "copies a replacement node the callback hands over more than once".
#[test]
fn copies_a_replacement_node_the_callback_hands_over_more_than_once() {
    let shared = text("done");
    let plugin = text_plugin(
        "shared-node",
        TextTransformOptions {
            required_substrings: required(&["TODO"]),
            ..TextTransformOptions::new(Regex::new("TODO").unwrap(), move |_, _| {
                vec![shared.clone()]
            })
        },
    );

    let root = parse("TODO and TODO again", vec![plugin.clone()]);
    let Some(Block::Paragraph(paragraph)) = root.children.first() else {
        panic!("Expected a paragraph");
    };
    assert_eq!(
        to_json(&paragraph.children[0]),
        json!({"type": "text", "value": "done"})
    );
    assert_eq!(
        to_json(&paragraph.children[2]),
        json!({"type": "text", "value": "done"})
    );

    // Both documents agree with a run whose callback allocates per match.
    let per_match = text_plugin(
        "per-match-node",
        TextTransformOptions {
            required_substrings: required(&["TODO"]),
            ..TextTransformOptions::new(Regex::new("TODO").unwrap(), |_, _| vec![text("done")])
        },
    );
    assert_eq!(
        to_json(&parse("TODO and TODO again", vec![plugin])),
        to_json(&parse("TODO and TODO again", vec![per_match]))
    );
}

/// Upstream: "still rejects an unrepresentable node without freezing the batch".
#[test]
fn still_rejects_an_unrepresentable_node_without_freezing_the_batch() {
    let rejected = Phrasing::Text(Literal {
        value: "ok".into(),
        position: Some(Position::offsets(0, 2)),
        ..Literal::default()
    });
    let plugin = text_plugin(
        "forged-provenance",
        TextTransformOptions {
            required_substrings: required(&["TODO"]),
            ..TextTransformOptions::new(Regex::new("TODO").unwrap(), move |_, _| {
                vec![rejected.clone()]
            })
        },
    );
    let diagnostics = Rc::new(RefCell::new(Vec::<PluginDiagnostic>::new()));
    let sink = diagnostics.clone();

    // Authored provenance is refused and the document keeps its source text.
    let root = parse_markdown_ast(
        "TODO",
        &ParseOptions {
            plugins: vec![plugin],
            on_plugin_diagnostic: Some(Rc::new(move |diagnostic: &PluginDiagnostic| {
                sink.borrow_mut().push(diagnostic.clone())
            })),
            ..ParseOptions::default()
        },
    );
    assert_eq!(
        to_json(&root),
        to_json(&parse_markdown_ast("TODO", &ParseOptions::default()))
    );
    // Upstream's `console.warn`: the failure is reported against the plugin's transform.
    assert!(
        diagnostics
            .borrow()
            .iter()
            .any(|d| d.plugin == "forged-provenance" && d.phase == PluginPhase::Transform),
        "{:?}",
        diagnostics.borrow()
    );
}

// --- markdownSoftBreaksPlugin ----------------------------------------------------------

const SOURCE: &str = "First line\nSecond **emphasized** line\n\n[Linked\nlabel](/docs) and `inline code`.\n\n```text\nfenced\ncode\n```";

fn soft_breaks(source: &str) -> Root {
    parse(source, vec![markdown_soft_breaks_plugin()])
}

fn alpha_beta(first: &str) -> Value {
    json!([
        {"type": "text", "value": first},
        {"type": "break"},
        {"type": "text", "value": "Beta"},
    ])
}

/// Upstream: "turns eligible LF, CRLF, and lone CR endings into break nodes".
#[test]
fn turns_eligible_lf_crlf_and_lone_cr_endings_into_break_nodes() {
    for (source, first) in [
        ("Alpha\nBeta", "Alpha"),
        ("Alpha\r\nBeta", "Alpha"),
        ("Alpha\rBeta", "Alpha"),
        ("Alpha \nBeta", "Alpha "),
        ("Alpha\t\nBeta", "Alpha\t"),
        ("Alpha  \nBeta", "Alpha"),
    ] {
        assert_eq!(
            to_json(&soft_breaks(source)),
            paragraph_root(alpha_beta(first), source.len()),
            "{source:?}"
        );
    }
}

/// Upstream: "preserves the no-plugin path and opaque contexts".
#[test]
fn preserves_the_no_plugin_path_and_opaque_contexts() {
    assert_eq!(
        soft_breaks("No line ending"),
        parse_markdown_ast("No line ending", &ParseOptions::default())
    );

    let baseline = parse_markdown_ast(SOURCE, &ParseOptions::default());
    let transformed = soft_breaks(SOURCE);
    let breaks = |root: &Root| {
        let mut count = 0;
        visit_markdown_nodes(root, NodeType::Break, &mut |_| count += 1);
        count
    };
    assert_eq!(breaks(&baseline), 0);
    assert!(breaks(&transformed) > 0);
    assert_eq!(
        to_json(&transformed),
        json!({
            "type": "root",
            "children": [
                {
                    "type": "paragraph",
                    "children": [
                        {"type": "text", "value": "First line"},
                        {"type": "break"},
                        {"type": "text", "value": "Second "},
                        {"type": "strong", "children": [{"type": "text", "value": "emphasized"}]},
                        {"type": "text", "value": " line"},
                    ],
                    "position": {"start": {"offset": 0}, "end": {"offset": 37}},
                },
                {
                    "type": "paragraph",
                    "children": [
                        {
                            "type": "link",
                            "url": "/docs",
                            "children": [
                                {"type": "text", "value": "Linked"},
                                {"type": "break"},
                                {"type": "text", "value": "label"},
                            ],
                        },
                        {"type": "text", "value": " and "},
                        {"type": "inlineCode", "value": "inline code"},
                        {"type": "text", "value": "."},
                    ],
                    "position": {"start": {"offset": 39}, "end": {"offset": 79}},
                },
                {
                    "type": "code",
                    "lang": "text",
                    "value": "fenced\ncode",
                    "position": {"start": {"offset": 81}, "end": {"offset": 104}},
                },
            ],
        })
    );
}

/// Upstream: "is deterministic and converges across streaming prefixes".
#[test]
fn is_deterministic_and_converges_across_streaming_prefixes() {
    let streaming = |source: &str| {
        parse_markdown_ast(
            source,
            &ParseOptions {
                plugins: vec![markdown_soft_breaks_plugin()],
                is_final: false,
                ..ParseOptions::default()
            },
        )
    };
    let snapshots: Vec<Root> = ["Alpha", "Alpha\n", "Alpha\nBeta"]
        .into_iter()
        .map(streaming)
        .collect();

    assert_eq!(snapshots[0], streaming("Alpha"));
    assert_eq!(
        to_json(&snapshots[1]),
        paragraph_root(json!([{"type": "text", "value": "Alpha"}]), 5)
    );
    assert_eq!(snapshots[2], soft_breaks("Alpha\nBeta"));
    assert_eq!(
        to_json(&snapshots[2]),
        paragraph_root(alpha_beta("Alpha"), 10)
    );
}
