//! Upstream's Markdown plugin protocol and canonical AST tests, ported to the parser side:
//! `Markdown/plugins/protocol.test.tsx`, `Markdown/parser.public.test.ts` and
//! `Markdown/ast.test.ts`. Expectations written against upstream's released projection
//! (`parseMarkdown`/`parseInline`: `level`, `content`, `href`, `src`, `bold`) are
//! translated to the canonical tree (`depth`, `value`, `url`, `strong`), checked against
//! upstream's own canonical output (computed with the Node oracle). Upstream's
//! `toMatchObject` is [`assert_matches`]: objects match by subset, arrays by length and
//! element.
//!
//! Skipped upstream cases (React rendering, DOM, SSR streaming, JavaScript-only mechanics):
//!
//! - protocol.test.tsx `refuses an entry that did not come from createMarkdownPlugin`: a
//!   forged entry object; Rust's `MarkdownPlugin` can only come from `create_markdown_plugin`.
//! - protocol.test.tsx `keeps omitted and explicit empty pipelines identical`: compares
//!   rendered HTML; in Rust omitted and empty plugin lists are the same value (`Vec::new()`).
//! - protocol.test.tsx `isolates a suspending renderer behind its node source`: React
//!   Suspense rendering.
//! - protocol.test.tsx `streams readable node fallback and siblings before a renderer
//!   resolves`: SSR streaming (`renderToPipeableStream`).
//! - protocol.test.tsx `accepts plugins created by a duplicate Core copy`: JavaScript module
//!   copies and `Symbol.for` brands; Rust has one crate copy and no brands.
//! - protocol.test.tsx `defuses rejected transform promises in runtime and server
//!   rendering`: promise rejection, `unhandledRejection` and SSR; Rust transforms are
//!   synchronous (a failing transform is covered by
//!   `emits_one_source_free_production_diagnostic_for_plugin_failure`).
//! - protocol.test.tsx `falls back to readable source when an extension renderer throws`:
//!   React rendering of a throwing renderer.
//! - protocol.test.tsx `keeps the last valid tree after mutation, async, or semantic
//!   failure`, its mutation and async parts only: a Rust transform gets `&Root` and cannot
//!   mutate it, and cannot return a promise; the semantic (re-levelled heading) part is
//!   ported.
//! - protocol.test.tsx `keeps syntax identity independent from live transforms` and
//!   `preserves settled identities when finalizing deferred syntax`, their incremental parts:
//!   `parseMarkdownIncremental` is not ported to this crate, so the tokenization count and
//!   object identity (`toBe`) are not checked; the parse identity and the trees are.
//! - protocol.test.tsx: `Object.isFrozen`/`Object.keys` of the capability report and the
//!   `expectTypeOf` assertions (TypeScript types; Rust's types are checked by the compiler).
//! - protocol.test.tsx: every `render(...)`/`renderToString(...)` assertion inside a ported
//!   case (the component crate's concern); the parse-side assertions are ported.
//! - parser.public.test.ts `is a generated public package subpath`: package.json exports
//!   and `'use client'` directives.
//! - parser.public.test.ts `reports plugin capabilities from the server-safe plugin entry,
//!   ...`, its package.json and `'use client'` parts; the capability report is ported.
//! - parser.public.test.ts `exposes legacy and canonical parser results ...`, its legacy
//!   projection, incremental and type parts; the canonical results are ported.
//! - ast.test.ts `preserves optional legacy keys and insertion order exactly`: the
//!   released projection's object keys, not ported to this crate.
//! - ast.test.ts `does not expose canonical fields through released parser results`: the
//!   released projection and incremental state, not ported to this crate.
//! - ast.test.ts: the released-projection (`parseMarkdown`, `range`) parts of the ported
//!   cases.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use rustryx_markdown::ast::{
    Block, ExtensionDisplay, ExtensionNode, Heading, Image, Link, Literal, NodeRef, NodeType,
    Parent, Phrasing, Position, Root, markdown_ast_text_with, visit_markdown_nodes,
};
use rustryx_markdown::plugins::{
    DiagnosticCode, ExtensionRenderer, MarkdownPlugin, PluginCapabilities, PluginDefinition,
    PluginDiagnostic, PluginPhase, SyntaxCapability, SyntaxContribution, TokenizeResult,
    TokenizerInput, Transform, TransformContext, create_markdown_frontmatter,
    create_markdown_plugin, get_markdown_plugin_capabilities, is_markdown_extension_node,
    prepare_markdown_plugins,
};
use rustryx_markdown::{
    OutlineItem, ParseOptions, decode_markdown_character_references, parse_inline_ast,
    parse_markdown_ast, parse_outline_from_markdown, project_markdown_headings,
};
use serde::Serialize;
use serde_json::{Value, json};

// --- Helpers --------------------------------------------------------------------------

fn to_json(value: &impl Serialize) -> Value {
    serde_json::to_value(value).expect("the tree serializes")
}

/// Upstream's `toMatchObject`: every key `expected` names matches (recursively), arrays
/// match by length and element, everything else by equality.
fn matches(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => expected
            .iter()
            .all(|(key, value)| actual.get(key).is_some_and(|found| matches(found, value))),
        (Value::Array(actual), Value::Array(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| matches(actual, expected))
        }
        _ => actual == expected,
    }
}

#[track_caller]
fn assert_matches(actual: &impl Serialize, expected: Value) {
    let actual = to_json(actual);
    assert!(
        matches(&actual, &expected),
        "\nexpected to match:\n{}\nactual:\n{}",
        serde_json::to_string_pretty(&expected).unwrap(),
        serde_json::to_string_pretty(&actual).unwrap()
    );
}

#[track_caller]
fn assert_json(actual: &impl Serialize, expected: Value) {
    let actual = to_json(actual);
    assert_eq!(
        actual,
        expected,
        "\nexpected:\n{}\nactual:\n{}",
        serde_json::to_string_pretty(&expected).unwrap(),
        serde_json::to_string_pretty(&actual).unwrap()
    );
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

/// Options that record every plugin diagnostic.
fn recording(plugins: Vec<MarkdownPlugin>) -> (ParseOptions, Rc<RefCell<Vec<PluginDiagnostic>>>) {
    let diagnostics = Rc::new(RefCell::new(Vec::new()));
    let sink = diagnostics.clone();
    (
        ParseOptions {
            plugins,
            on_plugin_diagnostic: Some(Rc::new(move |diagnostic: &PluginDiagnostic| {
                sink.borrow_mut().push(diagnostic.clone())
            })),
            ..ParseOptions::default()
        },
        diagnostics,
    )
}

fn text(value: &str) -> Phrasing {
    Phrasing::Text(Literal {
        value: value.into(),
        ..Literal::default()
    })
}

fn extension(plugin: &str, name: &str, display: ExtensionDisplay, data: Value) -> ExtensionNode {
    ExtensionNode {
        plugin: plugin.into(),
        name: name.into(),
        data,
        display,
        source: None,
        position: None,
    }
}

fn data_str(node: &ExtensionNode, key: &str) -> String {
    node.data[key].as_str().unwrap_or_default().to_string()
}

fn renderers(entries: Vec<(&str, ExtensionRenderer)>) -> BTreeMap<String, ExtensionRenderer> {
    entries
        .into_iter()
        .map(|(name, renderer)| (name.to_string(), renderer))
        .collect()
}

fn renderer(to_text: impl Fn(&ExtensionNode) -> String + 'static) -> ExtensionRenderer {
    ExtensionRenderer::new(move |node| Ok(to_text(node)))
}

/// JavaScript's `trim`.
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() && c != '\u{85}' || c == '\u{feff}')
}

/// Upstream `mentionDefinition`'s tokenizer: `@{label}`.
fn mention_tokenize(input: &TokenizerInput<'_>) -> Result<TokenizeResult, String> {
    let close = input.source[input.offset + 2..]
        .find('}')
        .map(|at| input.offset + 2 + at);
    let Some(close) = close.filter(|&close| close < input.end) else {
        return Ok(if input.is_final {
            TokenizeResult::NoMatch
        } else {
            TokenizeResult::Defer
        });
    };
    Ok(TokenizeResult::Match {
        end: close + 1,
        node: extension(
            "mentions",
            "mention",
            ExtensionDisplay::Inline,
            json!({ "label": &input.source[input.offset + 2..close] }),
        ),
    })
}

/// Upstream `mentionDefinition`, with its inline tokenizer replaceable.
fn mention_definition(
    tokenize: impl Fn(&TokenizerInput<'_>) -> Result<TokenizeResult, String> + 'static,
) -> PluginDefinition {
    PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![SyntaxContribution::new(["@{"], 80, tokenize)],
            block: vec![],
        }),
        renderers: renderers(vec![(
            "mention",
            renderer(|node| format!("@{}", data_str(node, "label"))),
        )]),
        ..PluginDefinition::new("mentions")
    }
}

fn mention_plugin() -> MarkdownPlugin {
    create_markdown_plugin(mention_definition(mention_tokenize)).unwrap()
}

/// Upstream `calloutDefinition` (and ast.test.ts's `demoNotes`): a `:::note` block.
fn note_block(plugin: &'static str, name: &'static str, max_span: usize) -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![],
            block: vec![SyntaxContribution::new(
                [":::note"],
                max_span,
                move |input| {
                    let close = input.source[input.offset + 7..]
                        .find("\n:::")
                        .map(|at| input.offset + 7 + at);
                    let Some(close) = close.filter(|&close| close + 4 <= input.end) else {
                        return Ok(if input.is_final {
                            TokenizeResult::NoMatch
                        } else {
                            TokenizeResult::Defer
                        });
                    };
                    Ok(TokenizeResult::Match {
                        end: close + 4,
                        node: extension(
                            plugin,
                            name,
                            ExtensionDisplay::Block,
                            json!({ "body": js_trim(&input.source[input.offset + 7..close]) }),
                        ),
                    })
                },
            )],
        }),
        renderers: renderers(vec![(name, renderer(|node| data_str(node, "body")))]),
        ..PluginDefinition::new(plugin)
    })
    .unwrap()
}

fn callout_plugin() -> MarkdownPlugin {
    note_block("callouts", "callout", 500)
}

fn demo_notes() -> MarkdownPlugin {
    note_block("demo-notes", "note", 200)
}

/// A plugin-authored transform that always returns a new tree.
fn transform_plugin(
    name: &str,
    run: impl Fn(&Root, &TransformContext<'_>) -> Root + 'static,
) -> PluginDefinition {
    PluginDefinition {
        transform: Some(Transform::new(move |root, context| {
            Ok(Some(run(root, context)))
        })),
        ..PluginDefinition::new(name)
    }
}

/// `root.children.map(...)` over the top-level blocks.
fn map_blocks(root: &Root, map: &dyn Fn(Block) -> Block) -> Root {
    Root {
        children: root.children.iter().cloned().map(map).collect(),
        ..root.clone()
    }
}

/// Maps every top-level paragraph's children.
fn map_paragraphs(root: &Root, map: &dyn Fn(Vec<Phrasing>) -> Vec<Phrasing>) -> Root {
    map_blocks(root, &|block| match block {
        Block::Paragraph(mut paragraph) => {
            paragraph.children = map(paragraph.children);
            Block::Paragraph(paragraph)
        }
        other => other,
    })
}

/// Upstream `badgeDefinition`'s transform, with the badge node it appends replaceable.
fn badge_definition(badge: ExtensionNode) -> PluginDefinition {
    PluginDefinition {
        renderers: renderers(vec![("badge", renderer(|node| data_str(node, "label")))]),
        ..transform_plugin("badges", move |root, _| {
            map_paragraphs(root, &|mut children| {
                children.push(Phrasing::Extension(badge.clone()));
                children
            })
        })
    }
}

fn badge_node(label: &str) -> ExtensionNode {
    extension(
        "badges",
        "badge",
        ExtensionDisplay::Inline,
        json!({ "label": label }),
    )
}

fn badge_plugin() -> MarkdownPlugin {
    create_markdown_plugin(badge_definition(badge_node("New"))).unwrap()
}

fn replace_in(children: Vec<Phrasing>, value: &str, replacement: &str) -> Vec<Phrasing> {
    children
        .into_iter()
        .map(|node| match node {
            Phrasing::Text(mut literal) => {
                literal.value = literal.value.replace(value, replacement);
                Phrasing::Text(literal)
            }
            other => other,
        })
        .collect()
}

/// Upstream `replaceText`: replaces text in top-level paragraphs and headings.
fn replace_text(value: &'static str, replacement: &'static str) -> MarkdownPlugin {
    create_markdown_plugin(transform_plugin(
        &format!("replace-{value}-{replacement}"),
        move |root, _| {
            map_blocks(root, &|block| match block {
                Block::Paragraph(mut node) => {
                    node.children = replace_in(node.children, value, replacement);
                    Block::Paragraph(node)
                }
                Block::Heading(mut node) => {
                    node.children = replace_in(node.children, value, replacement);
                    Block::Heading(node)
                }
                other => other,
            })
        },
    ))
    .unwrap()
}

/// A transform that replaces every top-level paragraph's children with `nodes`.
fn set_paragraph(name: &str, nodes: Vec<Phrasing>) -> MarkdownPlugin {
    create_markdown_plugin(transform_plugin(name, move |root, _| {
        map_paragraphs(root, &|_| nodes.clone())
    }))
    .unwrap()
}

fn link(url: &str, children: Vec<Phrasing>) -> Phrasing {
    Phrasing::Link(Link {
        url: url.into(),
        children,
        ..Link::default()
    })
}

fn image(url: &str, alt: &str) -> Phrasing {
    Phrasing::Image(Image {
        url: url.into(),
        alt: alt.into(),
        ..Image::default()
    })
}

/// Upstream `setUrl`.
fn set_url(kind: &str, url: &str) -> MarkdownPlugin {
    let node = if kind == "link" {
        link(url, vec![text("go")])
    } else {
        image(url, "pic")
    };
    set_paragraph(&format!("set-{kind}-url"), vec![node])
}

fn safe_paragraph() -> Value {
    json!([{ "type": "paragraph", "children": [{ "type": "text", "value": "Safe" }] }])
}

// --- protocol.test.tsx: getMarkdownPluginCapabilities (spec:AST-064 FR4) ---------------

/// Upstream: "reports only whether a plugin declares syntax and a transform".
#[test]
fn reports_only_whether_a_plugin_declares_syntax_and_a_transform() {
    let transform_only = create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(|_, _| Ok(None))),
        ..PluginDefinition::new("capability-transform")
    })
    .unwrap();
    let both = create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![SyntaxContribution::new(["%%"], 8, |_| {
                Ok(TokenizeResult::NoMatch)
            })],
            block: vec![],
        }),
        transform: Some(Transform::new(|_, _| Ok(None))),
        renderers: renderers(vec![("mark", renderer(|_| String::new()))]),
        ..PluginDefinition::new("capability-both")
    })
    .unwrap();
    assert_eq!(
        get_markdown_plugin_capabilities(&mention_plugin()),
        PluginCapabilities {
            syntax: true,
            transform: false
        }
    );
    assert_eq!(
        get_markdown_plugin_capabilities(&transform_only),
        PluginCapabilities {
            syntax: false,
            transform: true
        }
    );
    assert_eq!(
        get_markdown_plugin_capabilities(&both),
        PluginCapabilities {
            syntax: true,
            transform: true
        }
    );
}

// --- protocol.test.tsx: Markdown plugin protocol --------------------------------------

/// Upstream: "infers typed inline extensions and keeps protected contexts opaque".
#[test]
fn infers_typed_inline_extensions_and_keeps_protected_contexts_opaque() {
    let nodes = parse_inline_ast(
        "**@{Ada}** [@{Grace}](/people) `@{Linus}`",
        &with_plugins(vec![mention_plugin()]),
    );
    assert_matches(
        &nodes[0],
        json!({
            "type": "strong",
            "children": [{ "type": "extension", "data": { "label": "Ada" } }],
        }),
    );
    assert_matches(
        &nodes[2],
        json!({ "type": "link", "children": [{ "type": "text", "value": "@{Grace}" }] }),
    );
    assert_json(
        &nodes[4],
        json!({ "type": "inlineCode", "value": "@{Linus}" }),
    );

    let forged_provenance = create_markdown_plugin(mention_definition(|input| {
        Ok(match mention_tokenize(input)? {
            TokenizeResult::Match { end, mut node } => {
                node.source = Some("forged".into());
                node.position = Some(Position::offsets(99, 100));
                TokenizeResult::Match { end, node }
            }
            other => other,
        })
    }))
    .unwrap();
    let nodes = parse_inline_ast("@{Ada}", &with_plugins(vec![forged_provenance]));
    assert_matches(
        &nodes[0],
        json!({ "type": "extension", "source": "@{Ada}" }),
    );
    assert!(to_json(&nodes[0]).get("position").is_none());
}

/// Upstream: "renders inline and top-level block extension nodes" (its parse side: the
/// nodes the renderers receive).
#[test]
fn renders_inline_and_top_level_block_extension_nodes() {
    let root = parse(
        "Hello @{Ada}.\n\n:::note\nRead this\n:::",
        vec![mention_plugin(), callout_plugin()],
    );
    assert_matches(
        &root.children,
        json!([
            {
                "type": "paragraph",
                "children": [
                    { "type": "text", "value": "Hello " },
                    {
                        "type": "extension",
                        "plugin": "mentions",
                        "name": "mention",
                        "display": "inline",
                        "data": { "label": "Ada" },
                        "source": "@{Ada}",
                    },
                    { "type": "text", "value": "." },
                ],
            },
            {
                "type": "extension",
                "plugin": "callouts",
                "name": "callout",
                "display": "block",
                "data": { "body": "Read this" },
                "source": ":::note\nRead this\n:::",
                "position": { "start": { "offset": 15 }, "end": { "offset": 36 } },
            },
        ]),
    );
}

/// Upstream: "rejects duplicate entries and contains invalid tokenizer output".
///
/// Upstream's `parseMarkdown` throws `duplicate name`; the port's parse entry points never
/// fail (FR19), so the throw is `prepare_markdown_plugins`'s error, and the parse (like
/// upstream's `<Markdown>`, which still renders `plain`) runs without plugins and reports
/// the duplicate.
#[test]
fn rejects_duplicate_entries_and_contains_invalid_tokenizer_output() {
    let mention = mention_plugin();
    let error = prepare_markdown_plugins(&[mention.clone(), mention.clone()]).unwrap_err();
    assert!(
        error.message().contains("duplicate name"),
        "{}",
        error.message()
    );
    let (options, diagnostics) = recording(vec![mention.clone(), mention]);
    assert_matches(
        &parse_markdown_ast("plain", &options).children,
        json!([{ "type": "paragraph", "children": [{ "type": "text", "value": "plain" }] }]),
    );
    assert!(
        diagnostics
            .borrow()
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::DuplicateName),
        "{:?}",
        diagnostics.borrow()
    );

    // Upstream's invalid output is a promise; the port's tokenizers fail with `Err`, or
    // return a node that is not their own.
    let failing = create_markdown_plugin(PluginDefinition {
        name: "invalid-syntax".into(),
        ..mention_definition(|_| Err("invalid".into()))
    })
    .unwrap();
    let foreign = create_markdown_plugin(PluginDefinition {
        name: "invalid-syntax".into(),
        ..mention_definition(mention_tokenize)
    })
    .unwrap();
    for invalid in [failing, foreign] {
        assert_json(
            &parse("Hello @{Ada}", vec![invalid]),
            json!({
                "type": "root",
                "children": [{
                    "type": "paragraph",
                    "children": [{ "type": "text", "value": "Hello @{Ada}" }],
                    "position": { "start": { "offset": 0 }, "end": { "offset": 12 } },
                }],
            }),
        );
    }
}

/// Upstream: "renders synthetic extension nodes introduced by transforms".
#[test]
fn renders_synthetic_extension_nodes_introduced_by_transforms() {
    let root = parse("Status: ", vec![badge_plugin()]);
    assert_matches(
        &root.children,
        json!([{
            "type": "paragraph",
            "children": [
                { "type": "text", "value": "Status: " },
                {
                    "type": "extension",
                    "plugin": "badges",
                    "name": "badge",
                    "data": { "label": "New" },
                },
            ],
        }]),
    );
}

/// Upstream: "does not enable block syntax inside blockquotes or list items".
#[test]
fn does_not_enable_block_syntax_inside_blockquotes_or_list_items() {
    let source = "> :::note\n> quoted\n> :::\n\n- :::note\n  listed\n  :::";
    let root = parse(source, vec![callout_plugin()]);
    let serialized = serde_json::to_string(&root).unwrap();
    assert!(
        !serialized.contains(r#""type":"extension""#),
        "{serialized}"
    );
}

/// Upstream: "runs immutable transforms in plugin order".
#[test]
fn runs_immutable_transforms_in_plugin_order() {
    let first = replace_text("original", "first");
    let second = replace_text("first", "second");
    assert_matches(
        &parse("original", vec![first.clone(), second.clone()]).children,
        json!([{ "type": "paragraph", "children": [{ "type": "text", "value": "second" }] }]),
    );
    assert_matches(
        &parse("original", vec![second, first]).children,
        json!([{ "type": "paragraph", "children": [{ "type": "text", "value": "first" }] }]),
    );
}

/// Upstream: "emits one source-free production diagnostic for plugin failure".
///
/// Upstream's console dedupes across parses; the port's callback hears once per parse.
#[test]
fn emits_one_source_free_production_diagnostic_for_plugin_failure() {
    let source = "Private production source";
    let thrown_text = "Private plugin-thrown text";
    let broken = create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(move |_, _| Err(thrown_text.into()))),
        ..PluginDefinition::new("production-failure")
    })
    .unwrap();
    let (options, diagnostics) = recording(vec![broken]);
    let root = parse_markdown_ast(source, &options);
    // Upstream's `rejected-promise` case: the readable source stands.
    assert_matches(
        &root.children,
        json!([{ "type": "paragraph", "children": [{ "type": "text", "value": source }] }]),
    );
    parse_markdown_ast(source, &options);
    let diagnostics = diagnostics.borrow();
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    for diagnostic in diagnostics.iter() {
        assert_eq!(diagnostic.plugin, "production-failure");
        assert_eq!(diagnostic.phase, PluginPhase::Transform);
        assert_eq!(diagnostic.code, DiagnosticCode::TransformFailed);
        let message = diagnostic.to_string();
        assert!(
            message.starts_with("Markdown: plugin \"production-failure\" failed in transform"),
            "{message}"
        );
        assert!(
            message.ends_with("rendered readable fallback."),
            "{message}"
        );
        assert!(!message.contains(source), "{message}");
        assert!(!message.contains(thrown_text), "{message}");
    }
}

/// Upstream: "keeps the last valid tree after mutation, async, or semantic failure" (its
/// semantic part: a transform that re-levels a source heading).
#[test]
fn keeps_the_last_valid_tree_after_mutation_async_or_semantic_failure() {
    let heading_change = create_markdown_plugin(transform_plugin("heading-change", |root, _| {
        map_blocks(root, &|block| match block {
            Block::Heading(mut heading) => {
                heading.depth = 6;
                Block::Heading(heading)
            }
            other => other,
        })
    }))
    .unwrap();
    let (options, diagnostics) = recording(vec![heading_change]);
    assert_json(
        &parse_markdown_ast("# Safe", &options),
        json!({
            "type": "root",
            "children": [{
                "type": "heading",
                "depth": 1,
                "children": [{ "type": "text", "value": "Safe" }],
                "position": { "start": { "offset": 0 }, "end": { "offset": 6 } },
            }],
        }),
    );
    assert!(
        diagnostics.borrow().iter().any(|diagnostic| {
            diagnostic.plugin == "heading-change"
                && diagnostic.code == DiagnosticCode::InvalidDocument
        }),
        "{:?}",
        diagnostics.borrow()
    );
}

/// Upstream: "allows source heading removal and synthetic headings without changing
/// surviving depth".
#[test]
fn allows_source_heading_removal_and_synthetic_headings_without_changing_surviving_depth() {
    let add_heading = create_markdown_plugin(transform_plugin("add-heading", |root, _| {
        let mut next = root.clone();
        next.children.push(Block::Heading(Heading {
            depth: 2,
            children: vec![text("Generated")],
            ..Heading::default()
        }));
        next
    }))
    .unwrap();
    let remove_heading =
        create_markdown_plugin(transform_plugin("remove-heading", |root, _| Root {
            children: root
                .children
                .iter()
                .filter(|block| !matches!(block, Block::Heading(_)))
                .cloned()
                .collect(),
            ..root.clone()
        }))
        .unwrap();
    assert_matches(
        &parse("# Source", vec![add_heading]).children,
        json!([
            { "type": "heading", "depth": 1 },
            { "type": "heading", "depth": 2, "children": [{ "value": "Generated" }] },
        ]),
    );
    assert_matches(
        &parse("# Removed\n\nSurviving body.", vec![remove_heading]).children,
        json!([{
            "type": "paragraph",
            "children": [{ "type": "text", "value": "Surviving body." }],
        }]),
    );
}

/// Upstream: "rejects unsafe destinations, forged provenance, and foreign deletion".
#[test]
fn rejects_unsafe_destinations_forged_provenance_and_foreign_deletion() {
    let unsafe_destination = set_paragraph(
        "unsafe-destination",
        vec![link("javascript:alert(1)", vec![text("unsafe")])],
    );
    let nested_link = set_paragraph(
        "nested-link",
        vec![link(
            "/outer",
            vec![Phrasing::Strong(Parent {
                children: vec![link("/inner", vec![text("nested")])],
                ..Parent::default()
            })],
        )],
    );
    let forged_source = create_markdown_plugin(badge_definition(ExtensionNode {
        source: Some("forged".into()),
        ..badge_node("forged")
    }))
    .unwrap();
    let delete_foreign = create_markdown_plugin(transform_plugin("delete-foreign", |root, _| {
        map_paragraphs(root, &|mut children| {
            children.retain(|node| !matches!(node, Phrasing::Extension(_)));
            children
        })
    }))
    .unwrap();

    assert_matches(
        &parse("Safe", vec![unsafe_destination]).children,
        safe_paragraph(),
    );
    // Upstream checks the frozen-node variant's rendered href; the parse side: a padded
    // URL is trimmed.
    assert_matches(
        &parse("Safe", vec![set_url("link", "  https://example.com/x  ")]).children,
        json!([{ "type": "paragraph", "children": [{ "type": "link", "url": "https://example.com/x" }] }]),
    );
    assert_matches(
        &parse("Safe", vec![set_url("link", " \t ")]).children,
        json!([{ "type": "paragraph", "children": [{ "type": "link", "url": "" }] }]),
    );
    assert_matches(
        &parse("Safe", vec![set_url("link", "  /docs  ")]).children,
        json!([{ "type": "paragraph", "children": [{ "type": "link", "url": "/docs" }] }]),
    );
    assert_matches(
        &parse("Safe", vec![set_url("image", " ")]).children,
        json!([{ "type": "paragraph", "children": [{ "type": "image", "url": "" }] }]),
    );
    for (kind, node) in [
        (
            "link",
            link("data:image/png;base64,iVBORw0KGgo=", vec![text("unsafe")]),
        ),
        (
            "image",
            image("data:image/png;base64,iVBORw0KGgo=", "unsafe"),
        ),
    ] {
        let data_destination = set_paragraph(&format!("data-{kind}"), vec![node]);
        assert_matches(
            &parse("Safe", vec![data_destination]).children,
            safe_paragraph(),
        );
    }
    assert_matches(&parse("Safe", vec![nested_link]).children, safe_paragraph());
    let identity_replacement = replace_text("missing", "unchanged");
    assert_matches(
        &parse(
            "![pic](https://example.com/pic.png)",
            vec![identity_replacement.clone()],
        )
        .children,
        json!([{ "type": "image", "url": "https://example.com/pic.png" }]),
    );
    assert_matches(
        &parse("[empty]()", vec![identity_replacement]).children,
        json!([{ "type": "paragraph", "children": [{ "type": "link", "url": "" }] }]),
    );
    assert_matches(
        &parse("Safe", vec![forged_source]).children,
        safe_paragraph(),
    );
    assert_matches(
        &parse("@{Ada}", vec![mention_plugin(), delete_foreign]).children,
        json!([{
            "type": "paragraph",
            "children": [{ "type": "extension", "plugin": "mentions", "name": "mention" }],
        }]),
    );
}

/// Upstream: "narrows visitor callbacks by node kind".
#[test]
fn narrows_visitor_callbacks_by_node_kind() {
    let headings = Rc::new(Cell::new(0));
    let seen = headings.clone();
    let plugin = create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(move |root, _| {
            visit_markdown_nodes(root, NodeType::Heading, &mut |node| {
                if let NodeRef::Heading(heading) = node {
                    let depth: u8 = heading.depth;
                    assert!((1..=6).contains(&depth));
                    seen.set(seen.get() + 1);
                }
            });
            visit_markdown_nodes(root, NodeType::Extension, &mut |node| {
                if is_markdown_extension_node(node, "mentions", "mention")
                    && let NodeRef::Extension(extension) = node
                {
                    assert!(extension.data["label"].is_string());
                }
            });
            Ok(None)
        })),
        ..PluginDefinition::new("typed-visitor")
    })
    .unwrap();
    assert_eq!(parse("# Heading", vec![plugin]).children.len(), 1);
    assert_eq!(headings.get(), 1);
}

/// Upstream: "keeps syntax identity independent from live transforms" (without the
/// incremental parser: the two plugin lists share one parse identity, and the new
/// transform runs).
#[test]
fn keeps_syntax_identity_independent_from_live_transforms() {
    let plugin_with_transform = |replacement: &'static str| {
        create_markdown_plugin(PluginDefinition {
            transform: Some(Transform::new(move |root, _| {
                Ok(Some(map_paragraphs(root, &|children| {
                    children
                        .into_iter()
                        .map(|node| match node {
                            Phrasing::Text(mut literal) => {
                                literal.value = literal.value.replacen("Tail", replacement, 1);
                                Phrasing::Text(literal)
                            }
                            other => other,
                        })
                        .collect()
                })))
            })),
            ..mention_definition(mention_tokenize)
        })
        .unwrap()
    };
    let first = with_plugins(vec![plugin_with_transform("First")]);
    let second = with_plugins(vec![plugin_with_transform("Second")]);
    assert_eq!(first.parse_identity(), second.parse_identity());
    let updated = parse_markdown_ast("@{Ada}\n\nTail", &second);
    assert_matches(
        updated.children.last().unwrap(),
        json!({ "type": "paragraph", "children": [{ "type": "text", "value": "Second" }] }),
    );
}

/// Upstream: "preserves settled identities when finalizing deferred syntax" (without the
/// incremental parser: the settled block is equal, not identical).
#[test]
fn preserves_settled_identities_when_finalizing_deferred_syntax() {
    let streaming = parse_markdown_ast(
        "First\n\n@{",
        &ParseOptions {
            is_final: false,
            ..with_plugins(vec![mention_plugin()])
        },
    );
    let final_root = parse_markdown_ast(
        "First\n\n@{",
        &ParseOptions {
            is_final: true,
            ..with_plugins(vec![mention_plugin()])
        },
    );
    assert_eq!(final_root.children[0], streaming.children[0]);
    assert_matches(
        &final_root.children[1],
        json!({ "type": "paragraph", "children": [{ "type": "text", "value": "@{" }] }),
    );
}

/// Upstream: "uses transformed heading text for both Markdown and Outline".
#[test]
fn uses_transformed_heading_text_for_both_markdown_and_outline() {
    let plugin = replace_text("Draft", "Final");
    let source = "# Draft";
    let outline = parse_outline_from_markdown(source, std::slice::from_ref(&plugin), true);
    assert_eq!(
        outline,
        vec![OutlineItem {
            id: "final".into(),
            label: "Final".into(),
            level: 1
        }]
    );
    // Markdown's side: the id its heading renders with.
    let prepared = prepare_markdown_plugins(std::slice::from_ref(&plugin)).unwrap();
    let root = parse(source, vec![plugin]);
    let projection = project_markdown_headings(&root, Some(&prepared));
    let heading = projection.heading(0).expect("the heading is identified");
    assert_eq!(heading.id, "final");
    assert_eq!(heading.label, "Final");
}

/// Upstream: "passes matching transform finality to Markdown-derived outlines".
#[test]
fn passes_matching_transform_finality_to_markdown_derived_outlines() {
    let plugin = create_markdown_plugin(transform_plugin("finality-label", |root, context| {
        let value = if context.is_final() { "Final" } else { "Draft" };
        map_blocks(root, &|block| match block {
            Block::Heading(mut heading) => {
                heading.children = vec![text(value)];
                Block::Heading(heading)
            }
            other => other,
        })
    }))
    .unwrap();
    let item = |id: &str, label: &str| OutlineItem {
        id: id.into(),
        label: label.into(),
        level: 1,
    };
    assert_eq!(
        parse_outline_from_markdown("# Pending", std::slice::from_ref(&plugin), false),
        vec![item("draft", "Draft")]
    );
    assert_eq!(
        parse_outline_from_markdown("# Pending", std::slice::from_ref(&plugin), true),
        vec![item("final", "Final")]
    );
}

// --- parser.public.test.ts ------------------------------------------------------------

/// Upstream: "exports the character reference decoder Markdown renders with (spec:AST-061
/// DEC-5)".
#[test]
fn exports_the_character_reference_decoder_markdown_renders_with() {
    assert_eq!(
        decode_markdown_character_references("&copy; &unknown;"),
        "© &unknown;"
    );
}

/// Upstream: "reports plugin capabilities from the server-safe plugin entry, and renders
/// one node only from the client-only renderer entry (spec:AST-064 DEC-6)" (its capability
/// report).
#[test]
fn reports_plugin_capabilities_from_the_server_safe_plugin_entry() {
    let transform_only = create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(|_, _| Ok(None))),
        ..PluginDefinition::new("server-capabilities")
    })
    .unwrap();
    assert_eq!(
        get_markdown_plugin_capabilities(&transform_only),
        PluginCapabilities {
            syntax: false,
            transform: true
        }
    );
}

/// Upstream: "exposes legacy and canonical parser results from a server-only entry point"
/// (its canonical results).
#[test]
fn exposes_legacy_and_canonical_parser_results_from_a_server_only_entry_point() {
    let block_ast = parse_markdown_ast("# Heading\n\nParagraph", &ParseOptions::default());
    let inline_ast = parse_inline_ast("**bold**", &ParseOptions::default());
    assert_matches(
        &block_ast,
        json!({
            "type": "root",
            "children": [{ "type": "heading", "depth": 1 }, { "type": "paragraph" }],
        }),
    );
    assert_matches(&inline_ast, json!([{ "type": "strong" }]));
}

/// Upstream: "runs typed plugins through the server-safe parser boundary".
#[test]
fn runs_typed_plugins_through_the_server_safe_parser_boundary() {
    let frontmatter = create_markdown_frontmatter("server-document-metadata", |fields| {
        Ok(json!({ "title": fields.get("title").map_or("Untitled", String::as_str) }))
    })
    .unwrap();
    let title = Rc::new(RefCell::new(None::<String>));
    let observed = title.clone();
    let reader = frontmatter.clone();
    let observer = create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(move |root, _| {
            *observed.borrow_mut() = reader
                .get_metadata(root)
                .and_then(|metadata| metadata["title"].as_str().map(String::from));
            Ok(None)
        })),
        ..PluginDefinition::new("server-metadata-observer")
    })
    .unwrap();
    let root = parse(
        "---\ntitle: Server proof\n---\n# Body",
        vec![frontmatter.plugin.clone(), observer],
    );
    assert_eq!(title.borrow().as_deref(), Some("Server proof"));
    assert_matches(&root.children, json!([{ "type": "heading", "depth": 1 }]));
}

// --- ast.test.ts: canonical Markdown AST ----------------------------------------------

/// Upstream: "uses MDAST-aligned names and fields for every built-in structure".
#[test]
fn uses_mdast_aligned_names_and_fields_for_every_built_in_structure() {
    let ast = parse_markdown_ast(
        "# Title\n\n**bold** and *emphasis* with `code` and [link](/docs).\n\n- item\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\n```ts\nconst value = 1;\n```\n\n---",
        &ParseOptions::default(),
    );
    assert_matches(
        &ast,
        json!({
            "type": "root",
            "children": [
                {
                    "type": "heading",
                    "depth": 1,
                    "children": [{ "type": "text", "value": "Title" }],
                },
                {
                    "type": "paragraph",
                    "children": [
                        { "type": "strong" },
                        { "type": "text" },
                        { "type": "emphasis" },
                        { "type": "text" },
                        { "type": "inlineCode", "value": "code" },
                        { "type": "text" },
                        { "type": "link", "url": "/docs" },
                        { "type": "text", "value": "." },
                    ],
                },
                {
                    "type": "list",
                    "ordered": false,
                    "children": [{ "type": "listItem" }],
                },
                {
                    "type": "table",
                    "align": [null, null],
                    "children": [
                        {
                            "type": "tableRow",
                            "children": [{ "type": "tableCell" }, { "type": "tableCell" }],
                        },
                        {
                            "type": "tableRow",
                            "children": [{ "type": "tableCell" }, { "type": "tableCell" }],
                        },
                    ],
                },
                { "type": "code", "lang": "ts", "value": "const value = 1;" },
                { "type": "thematicBreak" },
            ],
        }),
    );
}

/// Upstream: "represents remaining built-ins without legacy aliases".
#[test]
fn represents_remaining_built_ins_without_legacy_aliases() {
    let ast = parse_markdown_ast(
        "> quote\n\n![Block](/block.png)\n\nParagraph ![Inline](/inline.png) [src] $x$  \nnext\n\n$$\ny\n$$",
        &ParseOptions {
            math: true,
            source_ids: Some(["src".to_string()].into()),
            ..ParseOptions::default()
        },
    );
    assert_matches(
        &ast.children,
        json!([
            {
                "type": "blockquote",
                "children": [
                    { "type": "paragraph", "children": [{ "type": "text", "value": "quote" }] },
                ],
            },
            { "type": "image", "url": "/block.png", "alt": "Block" },
            {
                "type": "paragraph",
                "children": [
                    { "type": "text", "value": "Paragraph " },
                    { "type": "image", "url": "/inline.png", "alt": "Inline" },
                    { "type": "text", "value": " " },
                    { "type": "citation", "sourceId": "src" },
                    { "type": "text", "value": " " },
                    { "type": "inlineMath", "value": "x" },
                    { "type": "break" },
                    { "type": "text", "value": "next" },
                ],
            },
            { "type": "math", "value": "y" },
        ]),
    );
    assert_matches(
        &parse_markdown_ast("```\nplain\n```", &ParseOptions::default()).children[0],
        json!({ "type": "code", "lang": null, "value": "plain" }),
    );
}

/// Upstream: "keeps lazy continuation text and source provenance in its owning block".
#[test]
fn keeps_lazy_continuation_text_and_source_provenance_in_its_owning_block() {
    let source = "> quoted\ncontinued lazily";
    let ast = parse_markdown_ast(source, &ParseOptions::default());
    assert_eq!(ast.children.len(), 1);
    assert_matches(
        &ast.children[0],
        json!({
            "type": "blockquote",
            "position": { "start": { "offset": 0 }, "end": { "offset": source.len() } },
            "children": [{
                "type": "paragraph",
                "children": [{ "type": "text", "value": "quoted\ncontinued lazily" }],
            }],
        }),
    );
    let Block::Blockquote(quote) = &ast.children[0] else {
        panic!("Expected blockquote");
    };
    let Some(Block::Paragraph(paragraph)) = quote.children.first() else {
        panic!("Expected paragraph");
    };
    assert_eq!(
        markdown_ast_text_with(&paragraph.children, &|_| String::new()),
        "quoted\ncontinued lazily"
    );

    let list_source = "- listed\ncontinued lazily";
    let list = &parse_markdown_ast(list_source, &ParseOptions::default()).children[0];
    assert_matches(
        list,
        json!({
            "type": "list",
            "position": { "start": { "offset": 0 }, "end": { "offset": list_source.len() } },
            "children": [{
                "type": "listItem",
                "children": [{
                    "type": "paragraph",
                    "children": [{ "type": "text", "value": "listed\ncontinued lazily" }],
                }],
            }],
        }),
    );
}

/// Upstream: "lets block plugin syntax interrupt a lazy continuation".
#[test]
fn lets_block_plugin_syntax_interrupt_a_lazy_continuation() {
    let ast = parse("> quoted\n:::note\nBody\n:::", vec![demo_notes()]);
    assert_matches(
        &ast.children,
        json!([
            { "type": "blockquote" },
            {
                "type": "extension",
                "plugin": "demo-notes",
                "name": "note",
                "data": { "body": "Body" },
            },
        ]),
    );
}

/// Upstream: "keeps source provenance canonical while projecting the released range" (its
/// canonical part).
#[test]
fn keeps_source_provenance_canonical_while_projecting_the_released_range() {
    let ast = parse_markdown_ast(
        "# Heading\n\nParagraph",
        &ParseOptions {
            source_ranges: true,
            ..ParseOptions::default()
        },
    );
    let Block::Heading(heading) = &ast.children[0] else {
        panic!("Expected heading");
    };
    assert_eq!(heading.position, Some(Position::offsets(0, 9)));
}

fn top_level_positions(root: &Root) -> Vec<Option<Position>> {
    root.children
        .iter()
        .map(|block| {
            to_json(block).get("position").map(|position| {
                let offset = |point: &str| {
                    position[point]["offset"]
                        .as_u64()
                        .map(|offset| offset as usize)
                };
                Position::offsets(offset("start").unwrap(), offset("end").unwrap())
            })
        })
        .collect()
}

/// Upstream: "authors canonical positions without the sourceRanges option".
#[test]
fn authors_canonical_positions_without_the_source_ranges_option() {
    let source = "# Heading\n\nParagraph\n\n> Quoted";
    let ast = parse_markdown_ast(source, &ParseOptions::default());
    assert_eq!(
        top_level_positions(&ast),
        vec![
            Some(Position::offsets(0, 9)),
            Some(Position::offsets(11, 20)),
            Some(Position::offsets(22, 30)),
        ]
    );
    let with_ranges = parse_markdown_ast(
        source,
        &ParseOptions {
            source_ranges: true,
            ..ParseOptions::default()
        },
    );
    assert_eq!(top_level_positions(&with_ranges), top_level_positions(&ast));

    // Nested blocks remain a top-level-only contract.
    let Block::Blockquote(quote) = &ast.children[2] else {
        panic!("Expected blockquote");
    };
    let Block::Paragraph(paragraph) = &quote.children[0] else {
        panic!("Expected paragraph");
    };
    assert_eq!(paragraph.position, None);
}

/// Upstream: "keeps canonical positions off released extension nodes" (its canonical part).
#[test]
fn keeps_canonical_positions_off_released_extension_nodes() {
    let source = ":::note\nBody\n:::\n\nTail prose.";
    let ast = parse(source, vec![demo_notes()]);
    let Block::Extension(note) = &ast.children[0] else {
        panic!("Expected extension");
    };
    assert_eq!(note.position, Some(Position::offsets(0, 16)));
    assert_eq!(note.source.as_deref(), Some(":::note\nBody\n:::"));
}
