//! Upstream's document-level plugins, ported from
//! `packages/core/src/Markdown/plugins/frontmatter.test.tsx`, `sourceLines.test.tsx` and the
//! projection and parser parts of `headingLinks.test.tsx`: what the canonical parser
//! (`parse_markdown_ast`), the heading projection (`project_markdown_headings`), Outline
//! (`parse_outline_from_markdown`) and the helper factories return.
//!
//! Upstream assertions on rendered stamps (`data-source-line`), renderer props or heading
//! ids in the DOM are ported as the canonical-tree lines or projected ids those come
//! from. Expected values were checked against upstream with the Node oracle.
//!
//! Skipped upstream cases, and why:
//! - frontmatter `keeps renderer and server output free of metadata syntax`: React SSR output.
//! - frontmatter `expectTypeOf` checks (in `removes document-start syntax…` and `returns a
//!   transform plugin…`) and the `Object.hasOwn`/prototype checks of `accepts the decoded
//!   fields record…`: TypeScript types and JavaScript prototype mechanics.
//! - sourceLines `keeps the released DOM and renderer props byte-identical without the
//!   plugin`: DOM and renderer props (the no-plugin tree is covered by
//!   `records_lines_on_canonical_block_nodes_only_with_the_plugin`).
//! - sourceLines `streams to the full-parse lines at every character boundary`:
//!   `parseMarkdownAstIncremental` is not ported.
//! - sourceLines `keeps a settled block and its stamp while more text streams in`: incremental
//!   parsing and node identity (`toBe`), not ported.
//! - sourceLines `runs one parse and tokenizes…`: its parse count is a React/mock concern;
//!   only the tokenizer count is ported.
//! - headingLinks `returns an ordinary public plugin entry`: a TypeScript type check.
//! - headingLinks `carries namespace and URL options across a duplicate compatible Core
//!   copy`: the duplicate module copy is JavaScript module mechanics, and the clipboard part
//!   is DOM; the projected ids, permalink and Outline are ported.
//! - headingLinks `fails soft for %s across Markdown and Outline`, all cases but
//!   `same-name plugin without module configuration`: they forge Symbol-branded, frozen
//!   envelopes, which Rust's opaque `MarkdownPlugin` cannot be built as.
//! - headingLinks `wires the heading row and button to canonical container reveal`: DOM
//!   classes.
//! - headingLinks `copies the canonical URL, shows fixed-space confirmation, announces, and
//!   restores`, `shows and announces a failed copy…`, `shows a failed copy when the
//!   Clipboard API is unavailable`, `ignores modified and non-primary pointer activation`,
//!   `copies on keyboard Enter and Space…`, `keeps one feedback timer…`: clipboard, DOM
//!   events, a11y announcements and timers.
//! - headingLinks `keeps authored heading links valid instead of nesting anchors` and `uses
//!   honest button semantics outside product link routing`: DOM structure and routing.
//! - headingLinks `localizes the copy action and copied names through module keys`: i18n
//!   rendering.
//! - headingLinks `draws the close glyph when %s` (both cases): theme icons and clipboard.
//! - headingLinks `renders one honest sibling copy button…`, `uses a caller-owned namespace…`
//!   and `leaves permalink output to a custom heading renderer…`: only their projected ids
//!   (and Outline) are ported, not the buttons or the container id.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use rustryx_markdown::ast::{
    Block, ExtensionDisplay, ExtensionNode, NodeRef, NodeType, Point, Position, Root,
    visit_markdown_nodes,
};
use rustryx_markdown::plugins::{
    DiagnosticCode, ExtensionRenderer, FenceTransformOptions, FrontmatterFields,
    FrontmatterParseResult, HeadingLinksOptions, MarkdownPlugin, PluginDefinition,
    PluginDiagnostic, PluginPhase, SyntaxCapability, SyntaxContribution, TokenizeResult, Transform,
    create_markdown_fence_transform, create_markdown_frontmatter, create_markdown_heading_links,
    create_markdown_plugin, get_markdown_plugin_capabilities, markdown_source_lines_plugin,
    prepare_markdown_plugins,
};
use rustryx_markdown::{
    OutlineItem, ParseOptions, ProjectedHeading, parse_markdown_ast, parse_outline_from_markdown,
    project_markdown_headings,
};
use serde::de::Error as _;
use serde::ser::Error as _;
use serde_json::{Value, json};

// ---- shared helpers -------------------------------------------------------------------

fn with_plugins(plugins: Vec<MarkdownPlugin>) -> ParseOptions {
    ParseOptions {
        plugins,
        ..ParseOptions::default()
    }
}

/// Every parse diagnostic, collected.
fn collecting(plugins: Vec<MarkdownPlugin>) -> (ParseOptions, Rc<RefCell<Vec<PluginDiagnostic>>>) {
    let sink = Rc::new(RefCell::new(Vec::new()));
    let events = sink.clone();
    let options = ParseOptions {
        plugins,
        on_plugin_diagnostic: Some(Rc::new(move |event: &PluginDiagnostic| {
            events.borrow_mut().push(event.clone())
        })),
        ..ParseOptions::default()
    };
    (options, sink)
}

fn renderers(
    entries: Vec<(&str, ExtensionRenderer)>,
) -> std::collections::BTreeMap<String, ExtensionRenderer> {
    entries
        .into_iter()
        .map(|(name, renderer)| (name.to_string(), renderer))
        .collect()
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

fn plugin_transform(name: &str, run: impl Fn(&Root) -> Root + 'static) -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(move |root, _| Ok(Some(run(root))))),
        ..PluginDefinition::new(name)
    })
    .unwrap()
}

// ---- frontmatter ----------------------------------------------------------------------

const FRONTMATTER_SOURCE: &str = "---\ntitle: Release notes\ndraft: true\n---\n# Shipped\n\nBody";

/// Upstream's `documentMetadata`: `{title: fields.title ?? '', draft: fields.draft === 'true'}`.
fn document_metadata() -> rustryx_markdown::plugins::MarkdownFrontmatter<Value> {
    create_markdown_frontmatter("document-metadata", |fields: &FrontmatterFields| {
        Ok(json!({
            "title": fields.get("title").cloned().unwrap_or_default(),
            "draft": fields.get("draft").map(String::as_str) == Some("true"),
        }))
    })
    .unwrap()
}

/// Upstream: "removes document-start syntax and exposes typed metadata to later plugins".
#[test]
fn removes_document_start_syntax_and_exposes_typed_metadata_to_later_plugins() {
    let metadata = document_metadata();
    let seen: Rc<RefCell<Option<Value>>> = Rc::new(RefCell::new(None));
    let observer = {
        let metadata = metadata.clone();
        let seen = seen.clone();
        create_markdown_plugin(PluginDefinition {
            transform: Some(Transform::new(move |root, _| {
                *seen.borrow_mut() = metadata.get_metadata(root);
                Ok(None)
            })),
            ..PluginDefinition::new("frontmatter-observer")
        })
        .unwrap()
    };
    let root = parse_markdown_ast(
        FRONTMATTER_SOURCE,
        &with_plugins(vec![metadata.plugin.clone(), observer]),
    );

    let expected = json!({"title": "Release notes", "draft": true});
    assert_eq!(*seen.borrow(), Some(expected.clone()));
    assert_eq!(
        metadata.parse(FRONTMATTER_SOURCE, true).unwrap(),
        FrontmatterParseResult::Match {
            content_start: FRONTMATTER_SOURCE.find("# Shipped").unwrap(),
            metadata: expected,
        }
    );
    assert_eq!(metadata.get_metadata(&root), *seen.borrow());
    assert_eq!(root.children.len(), 2);
    assert!(
        !serde_json::to_string(&root)
            .unwrap()
            .contains("title: Release notes")
    );
}

/// Upstream: "hides unfinished streaming frontmatter and exposes it once closed".
#[test]
fn hides_unfinished_streaming_frontmatter_and_exposes_it_once_closed() {
    let metadata = document_metadata();
    let streaming = ParseOptions {
        is_final: false,
        ..with_plugins(vec![metadata.plugin.clone()])
    };
    let partial = parse_markdown_ast("---\ntitle: Pending", &streaming);
    assert_eq!(metadata.get_metadata(&partial), None);
    assert!(partial.children.is_empty());

    let settled = parse_markdown_ast(FRONTMATTER_SOURCE, &streaming);
    assert_eq!(
        metadata.get_metadata(&settled),
        Some(json!({"title": "Release notes", "draft": true}))
    );
    assert!(matches!(settled.children.first(), Some(Block::Heading(_))));
    match metadata
        .parse("---\r\ntitle: Windows\r\ndraft: false\r\n---\r\nBody", true)
        .unwrap()
    {
        FrontmatterParseResult::Match { metadata, .. } => {
            assert_eq!(metadata, json!({"title": "Windows", "draft": false}))
        }
        other => panic!("expected a frontmatter match, got {other:?}"),
    }
}

/// Upstream: "does not treat non-leading frontmatter syntax as metadata".
#[test]
fn does_not_treat_non_leading_frontmatter_syntax_as_metadata() {
    let metadata = document_metadata();
    let root = parse_markdown_ast(
        &format!("# Intro\n\n{FRONTMATTER_SOURCE}"),
        &with_plugins(vec![metadata.plugin.clone()]),
    );
    assert_eq!(metadata.get_metadata(&root), None);
}

/// Metadata that cannot be represented as JSON-like data (upstream: `new Date()`).
struct Unrepresentable;

impl serde::Serialize for Unrepresentable {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom("not JSON-like data"))
    }
}

impl<'de> serde::Deserialize<'de> for Unrepresentable {
    fn deserialize<D: serde::Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Err(D::Error::custom("not JSON-like data"))
    }
}

/// Upstream: "rejects duplicate fields and unrepresentable decoded metadata".
#[test]
fn rejects_duplicate_fields_and_unrepresentable_decoded_metadata() {
    let metadata = document_metadata();
    let duplicate = parse_markdown_ast(
        "---\ntitle: One\ntitle: Two\n---\nBody",
        &with_plugins(vec![metadata.plugin.clone()]),
    );
    assert_eq!(metadata.get_metadata(&duplicate), None);

    let invalid = create_markdown_frontmatter("invalid", |_| Ok(Unrepresentable)).unwrap();
    // Upstream: `not.toThrow()`. The port must not panic either.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parse_markdown_ast(
            FRONTMATTER_SOURCE,
            &with_plugins(vec![invalid.plugin.clone()]),
        )
    }));
    assert!(outcome.is_ok());
}

/// Upstream: "accepts the decoded fields record as metadata (identity parse)".
#[test]
fn accepts_the_decoded_fields_record_as_metadata_identity_parse() {
    let fields =
        create_markdown_frontmatter("fields", |record: &FrontmatterFields| Ok(record.clone()))
            .unwrap();
    let source = "---\ntitle: Release notes\n__proto__: kept\n---\n# Shipped";

    let parsed = fields.parse(source, true).unwrap();
    let expected: FrontmatterFields = [
        ("title".to_string(), "Release notes".to_string()),
        ("__proto__".to_string(), "kept".to_string()),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        parsed,
        FrontmatterParseResult::Match {
            content_start: source.find("# Shipped").unwrap(),
            metadata: expected.clone(),
        }
    );
    let FrontmatterParseResult::Match { metadata, .. } = parsed else {
        panic!("expected a frontmatter match");
    };
    // A field named like a prototype key stays an ordinary field.
    assert!(metadata.contains_key("__proto__"));

    let root = parse_markdown_ast(source, &with_plugins(vec![fields.plugin.clone()]));
    assert_eq!(fields.get_metadata(&root), Some(metadata));
    assert!(
        !serde_json::to_string(&root)
            .unwrap()
            .contains("title: Release notes")
    );
}

/// Upstream: "returns a transform plugin that composes in the ordered pipeline".
#[test]
fn returns_a_transform_plugin_that_composes_in_the_ordered_pipeline() {
    let metadata = document_metadata();
    assert!(get_markdown_plugin_capabilities(&metadata.plugin).transform);
    let root = parse_markdown_ast(
        FRONTMATTER_SOURCE,
        &with_plugins(vec![metadata.plugin.clone()]),
    );
    assert!(matches!(
        root.children.first(),
        Some(Block::Heading(heading)) if heading.depth == 1
    ));
}

// ---- source lines ---------------------------------------------------------------------

const LINES_SOURCE: &str = concat!(
    "# Title\n",          // 1
    "\n",                 // 2
    "First paragraph\n",  // 3
    "spans two lines.\n", // 4
    "\n",                 // 5
    "- one\n",            // 6
    "- two\n",            // 7
    "\n",                 // 8
    "> quoted\n",         // 9
    "\n",                 // 10
    "```ts\n",            // 11
    "const x = 1;\n",     // 12
    "```\n",              // 13
    "\n",                 // 14
    "| a | b |\n",        // 15
    "| - | - |\n",        // 16
    "| 1 | 2 |\n",        // 17
    "\n",                 // 18
    "---\n",              // 19
    "\n",                 // 20
    "Last *words* here.", // 21
);

const NESTED: &str = concat!(
    "- outer item\n",               // 1
    "  continues here\n",           // 2
    "\n",                           // 3
    "  second paragraph\n",         // 4
    "\n",                           // 5
    "  - inner one\n",              // 6
    "  - inner two\n",              // 7
    "\n",                           // 8
    "> quote opens\n",              // 9
    "lazy continuation\n",          // 10
    ">\n",                          // 11
    "> - quoted item\n",            // 12
    ">\n",                          // 13
    "> ```\n",                      // 14
    "> fenced\n",                   // 15
    "> ```\n",                      // 16
    "\n",                           // 17
    "[ref]: https://example.com\n", // 18
    "\n",                           // 19
    "See [ref].",                   // 20
);

/// Upstream's `lineSpans`: every node reached through `children` with recorded lines, as
/// `type:start-end`, in document order.
fn line_spans(root: &Root) -> Vec<String> {
    fn walk(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            Value::Object(node) => {
                if let Some(start) = node.get("position").and_then(|p| p["start"].get("line")) {
                    out.push(format!(
                        "{}:{}-{}",
                        node["type"].as_str().unwrap_or_default(),
                        start,
                        node["position"]["end"]["line"]
                    ));
                }
                if let Some(children) = node.get("children") {
                    walk(children, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&serde_json::to_value(root).unwrap(), &mut out);
    out
}

/// Every typed node anywhere in the tree with recorded lines (not only through
/// `children`), as `type:start-end`.
fn all_lines(root: &Root) -> Vec<String> {
    fn walk(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            Value::Object(node) => {
                if let (Some(kind), Some(start)) = (
                    node.get("type").and_then(Value::as_str),
                    node.get("position").and_then(|p| p["start"].get("line")),
                ) {
                    out.push(format!(
                        "{kind}:{start}-{}",
                        node["position"]["end"]["line"]
                    ));
                }
                for (key, child) in node {
                    if key != "position" {
                        walk(child, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&serde_json::to_value(root).unwrap(), &mut out);
    out
}

/// Upstream's `hasAnyLine`.
fn has_any_line(root: &Root) -> bool {
    serde_json::to_string(root).unwrap().contains("\"line\"")
}

fn lines_of(position: Option<&Position>) -> Option<(usize, usize)> {
    let position = position?;
    Some((position.start.line?, position.end.line?))
}

fn with_lines(mut plugins: Vec<MarkdownPlugin>) -> ParseOptions {
    plugins.push(markdown_source_lines_plugin());
    with_plugins(plugins)
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

const NESTED_SPANS: &[&str] = &[
    "list:1-7",
    "listItem:1-7",
    "paragraph:1-2",
    "paragraph:4-4",
    "list:6-7",
    "listItem:6-6",
    "paragraph:6-6",
    "listItem:7-7",
    "paragraph:7-7",
    "blockquote:9-16",
    "paragraph:9-10",
    "list:12-12",
    "listItem:12-12",
    "paragraph:12-12",
    "code:14-16",
    "paragraph:20-20",
];

/// Upstream: "stamps every default block kind with its 1-based inclusive lines" (as the
/// canonical blocks' lines the stamps come from).
#[test]
fn stamps_every_default_block_kind_with_its_1_based_inclusive_lines() {
    let root = parse_markdown_ast(LINES_SOURCE, &with_lines(vec![]));
    assert_eq!(
        line_spans(&root),
        strings(&[
            "heading:1-1",
            "paragraph:3-4",
            "list:6-7",
            "listItem:6-6",
            "paragraph:6-6",
            "listItem:7-7",
            "paragraph:7-7",
            "blockquote:9-9",
            "paragraph:9-9",
            "code:11-13",
            "table:15-17",
            "thematicBreak:19-19",
            "paragraph:21-21",
        ])
    );
}

/// Upstream: "stamps no inline element, table row, or table cell".
#[test]
fn stamps_no_inline_element_table_row_or_table_cell() {
    let root = parse_markdown_ast(LINES_SOURCE, &with_lines(vec![]));
    let stamped: Vec<String> = all_lines(&root)
        .into_iter()
        .filter(|span| {
            ["emphasis:", "text:", "tableRow:", "tableCell:"]
                .iter()
                .any(|kind| span.starts_with(kind))
        })
        .collect();
    assert_eq!(stamped, Vec::<String>::new());
}

/// Upstream: "stamps nested blocks with document lines, past markers, lazy lines, and
/// stripped definitions".
#[test]
fn stamps_nested_blocks_with_document_lines_past_markers_lazy_lines_and_stripped_definitions() {
    let root = parse_markdown_ast(NESTED, &with_lines(vec![]));
    let spans = line_spans(&root);
    let of = |kind: &str| -> Vec<String> {
        spans
            .iter()
            .filter(|span| span.starts_with(kind))
            .map(|span| span[kind.len()..].to_string())
            .collect()
    };
    assert_eq!(of("listItem:"), strings(&["1-7", "6-6", "7-7", "12-12"]));
    assert_eq!(
        of("paragraph:"),
        strings(&["1-2", "4-4", "6-6", "7-7", "9-10", "12-12", "20-20"])
    );
    assert_eq!(of("blockquote:"), strings(&["9-16"]));
    assert_eq!(of("code:"), strings(&["14-16"]));
}

/// Upstream: "counts CRLF line endings and the lines a frontmatter block occupies".
#[test]
fn counts_crlf_line_endings_and_the_lines_a_frontmatter_block_occupies() {
    let frontmatter = create_markdown_frontmatter("metadata", |fields: &FrontmatterFields| {
        Ok(json!({"title": fields.get("title").cloned().unwrap_or_default()}))
    })
    .unwrap();
    let root = parse_markdown_ast(
        "---\r\ntitle: Notes\r\n---\r\n# Heading\r\n\r\nBody\r\nline",
        &with_lines(vec![frontmatter.plugin.clone()]),
    );
    assert_eq!(all_lines(&root), strings(&["heading:4-4", "paragraph:6-7"]));
}

/// Upstream: "gives each components block renderer sourceLines and adds no wrapper" (as the
/// canonical lines each renderer's `sourceLines` come from; inline math has none).
#[test]
fn gives_each_components_block_renderer_source_lines_and_adds_no_wrapper() {
    let source = concat!(
        "# Heading\n",                         // 1
        "\n",                                  // 2
        "Text with $x$ inline.\n",             // 3
        "\n",                                  // 4
        "```\n",                               // 5
        "code\n",                              // 6
        "```\n",                               // 7
        "\n",                                  // 8
        "> quote\n",                           // 9
        "\n",                                  // 10
        "***\n",                               // 11
        "\n",                                  // 12
        "![alt](https://example.com/a.png)\n", // 13
        "\n",                                  // 14
        "$$\n",                                // 15
        "y\n",                                 // 16
        "$$",                                  // 17
    );
    let root = parse_markdown_ast(
        source,
        &ParseOptions {
            math: true,
            ..with_lines(vec![])
        },
    );
    assert_eq!(
        all_lines(&root),
        strings(&[
            "heading:1-1",
            "paragraph:3-3",
            "code:5-7",
            "blockquote:9-9",
            "paragraph:9-9",
            "thematicBreak:11-11",
            "image:13-13",
            "math:15-17",
        ])
    );
}

fn charts() -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        transform: Some(
            create_markdown_fence_transform(FenceTransformOptions {
                languages: vec!["chart".into()],
                create_node: Rc::new(|fence| {
                    Ok(Some(extension(
                        "charts",
                        "chart",
                        ExtensionDisplay::Block,
                        json!({"spec": fence.code}),
                    )))
                }),
            })
            .unwrap(),
        ),
        renderers: renderers(vec![(
            "chart",
            ExtensionRenderer::new(|node| {
                Ok(node.data["spec"].as_str().unwrap_or_default().to_string())
            }),
        )]),
        ..PluginDefinition::new("charts")
    })
    .unwrap()
}

fn diagrams() -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        transform: Some(
            create_markdown_fence_transform(FenceTransformOptions {
                languages: vec!["diagram".into()],
                create_node: Rc::new(|fence| {
                    Ok(Some(extension(
                        "diagrams",
                        "diagram",
                        ExtensionDisplay::Block,
                        json!({"code": fence.code}),
                    )))
                }),
            })
            .unwrap(),
        ),
        renderers: renderers(vec![(
            "diagram",
            ExtensionRenderer::new(|node| {
                Ok(node.data["code"].as_str().unwrap_or_default().to_string())
            }),
        )]),
        ..PluginDefinition::new("diagrams")
    })
    .unwrap()
}

/// A fence's language, lines and whether a fence transform claimed it.
type Fence = (Option<String>, Option<(usize, usize)>, bool);

/// Every code node of the tree, depth first.
fn code_nodes(root: &Root) -> Vec<&rustryx_markdown::ast::Code> {
    let mut codes = Vec::new();
    visit_markdown_nodes(root, NodeType::Code, &mut |node| {
        if let NodeRef::Code(code) = node {
            codes.push(code);
        }
    });
    codes
}

/// Upstream: "gives plugin block renderers and claimed fences their lines through position".
#[test]
fn gives_plugin_block_renderers_and_claimed_fences_their_lines_through_position() {
    let cards = create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![],
            block: vec![SyntaxContribution::new([":::card"], 200, |input| {
                Ok(TokenizeResult::Match {
                    end: input.offset + ":::card".len(),
                    node: extension("cards", "card", ExtensionDisplay::Block, json!({})),
                })
            })],
        }),
        renderers: renderers(vec![(
            "card",
            ExtensionRenderer::new(|_| Ok("card".into())),
        )]),
        ..PluginDefinition::new("cards")
    })
    .unwrap();
    let root = parse_markdown_ast(
        "Intro\n\n:::card\n\n- item\n\n  ```chart\n  bars\n  ```",
        &with_lines(vec![cards, charts()]),
    );

    let mut seen = Vec::new();
    visit_markdown_nodes(&root, NodeType::Extension, &mut |node| {
        if let NodeRef::Extension(card) = node {
            seen.push(format!(
                "{}:{:?}",
                card.name,
                lines_of(card.position.as_ref())
            ));
        }
    });
    for code in code_nodes(&root) {
        if let Some(chart) = code.fence_proposal() {
            seen.push(format!(
                "{}:{:?}",
                chart.name,
                lines_of(chart.position.as_ref())
            ));
        }
    }
    assert_eq!(seen, strings(&["card:Some((3, 3))", "chart:Some((7, 9))"]));
}

/// Upstream: "keeps the lines when a claimed fence falls back to the host code renderer or
/// the default code block" (as the fences' canonical lines, claimed or not).
#[test]
fn keeps_the_lines_when_a_claimed_fence_falls_back_to_the_host_code_renderer_or_the_default_code_block()
 {
    let source = "Intro\n\n```diagram\na --> b\n```\n\n```ts\nconst x = 1;\n```";
    let root = parse_markdown_ast(source, &with_lines(vec![diagrams()]));
    let fences: Vec<Fence> = code_nodes(&root)
        .into_iter()
        .map(|code| {
            (
                code.lang.clone(),
                lines_of(code.position.as_ref()),
                code.fence_proposal().is_some(),
            )
        })
        .collect();
    assert_eq!(
        fences,
        vec![
            (Some("diagram".to_string()), Some((3, 5)), true),
            (Some("ts".to_string()), Some((7, 9)), false),
        ]
    );
    let diagram = code_nodes(&root)[0].fence_proposal().cloned().unwrap();
    assert_eq!(diagram.data, json!({"code": "a --> b"}));
}

/// Upstream: "records lines on canonical block nodes only with the plugin".
#[test]
fn records_lines_on_canonical_block_nodes_only_with_the_plugin() {
    let with = parse_markdown_ast(NESTED, &with_lines(vec![]));
    assert_eq!(line_spans(&with), strings(NESTED_SPANS));

    let without = parse_markdown_ast(NESTED, &ParseOptions::default());
    assert!(!has_any_line(&without));
    assert!(!has_any_line(&parse_markdown_ast(
        NESTED,
        &with_plugins(vec![])
    )));
}

/// Upstream: "composes with a transform that rebuilds nested blocks, which keep their lines".
#[test]
fn composes_with_a_transform_that_rebuilds_nested_blocks_which_keep_their_lines() {
    let copy = plugin_transform("test-rebuild", |root| root.clone());
    let (mut options, events) = collecting(vec![copy]);
    options.plugins.push(markdown_source_lines_plugin());
    let root = parse_markdown_ast(NESTED, &options);
    assert_eq!(*events.borrow(), Vec::<PluginDiagnostic>::new());
    assert_eq!(
        line_spans(&root),
        line_spans(&parse_markdown_ast(NESTED, &with_lines(vec![])))
    );
}

fn line_point(line: usize) -> Point {
    Point {
        line: Some(line),
        ..Point::default()
    }
}

fn transform_failure(plugin: &str) -> PluginDiagnostic {
    PluginDiagnostic {
        plugin: plugin.to_string(),
        phase: PluginPhase::Transform,
        code: DiagnosticCode::InvalidDocument,
        severity: DiagnosticCode::InvalidDocument.severity(),
    }
}

/// Upstream: "refuses a transform that shifts a nested block to other lines".
#[test]
fn refuses_a_transform_that_shifts_a_nested_block_to_other_lines() {
    fn shift(blocks: &mut [Block]) {
        for block in blocks {
            match block {
                Block::Paragraph(paragraph)
                    if paragraph.position.and_then(|p| p.start.line) == Some(12) =>
                {
                    paragraph.position = Some(Position {
                        start: line_point(13),
                        end: line_point(13),
                    });
                }
                Block::Blockquote(quote) => shift(&mut quote.children),
                Block::List(list) => {
                    for item in &mut list.children {
                        shift(&mut item.children);
                    }
                }
                _ => {}
            }
        }
    }
    let forge = plugin_transform("test-shift-lines", |root| {
        let mut next = root.clone();
        shift(&mut next.children);
        next
    });
    let (mut options, events) = collecting(vec![forge]);
    options.plugins.push(markdown_source_lines_plugin());
    let root = parse_markdown_ast(NESTED, &options);
    assert_eq!(
        *events.borrow(),
        vec![transform_failure("test-shift-lines")]
    );
    assert_eq!(line_spans(&root), strings(NESTED_SPANS));
}

/// Upstream: "refuses a transform that moves a top-level block to other lines while keeping
/// its offsets".
#[test]
fn refuses_a_transform_that_moves_a_top_level_block_to_other_lines_while_keeping_its_offsets() {
    let forge = plugin_transform("test-move-lines-keep-offsets", |root| {
        let mut next = root.clone();
        for block in &mut next.children {
            if let Block::Paragraph(paragraph) = block
                && let Some(position) = paragraph.position.as_mut()
                && position.start.line == Some(20)
            {
                position.start.line = Some(19);
                position.end.line = Some(19);
            }
        }
        next
    });
    let (mut options, events) = collecting(vec![forge]);
    options.plugins.push(markdown_source_lines_plugin());
    let root = parse_markdown_ast(NESTED, &options);
    assert_eq!(
        *events.borrow(),
        vec![transform_failure("test-move-lines-keep-offsets")]
    );
    assert_eq!(line_spans(&root), strings(NESTED_SPANS));
}

/// Upstream: "runs one parse and tokenizes each source position once, with or without the
/// plugin" (the tokenizer count; the parse count is the React component's).
#[test]
fn runs_one_parse_and_tokenizes_each_source_position_once_with_or_without_the_plugin() {
    let calls = Rc::new(Cell::new(0usize));
    let marks = {
        let calls = calls.clone();
        create_markdown_plugin(PluginDefinition {
            parse_key: Some("v1".into()),
            syntax: Some(SyntaxCapability {
                inline: vec![SyntaxContribution::new(["!!"], 4, move |input| {
                    calls.set(calls.get() + 1);
                    Ok(TokenizeResult::Match {
                        end: input.offset + 2,
                        node: extension("marks", "mark", ExtensionDisplay::Inline, json!({})),
                    })
                })],
                block: vec![],
            }),
            renderers: renderers(vec![("mark", ExtensionRenderer::new(|_| Ok("!".into())))]),
            ..PluginDefinition::new("marks")
        })
        .unwrap()
    };
    let source = format!("{NESTED}\n\nA !! mark, and - !! in a list:\n\n- item !!\n  > quote !!");

    parse_markdown_ast(&source, &with_plugins(vec![marks.clone()]));
    let baseline = calls.replace(0);
    let root = parse_markdown_ast(&source, &with_lines(vec![marks]));

    assert!(baseline > 0);
    assert_eq!(calls.get(), baseline);
    assert!(
        line_spans(&root)
            .iter()
            .any(|span| span.starts_with("listItem:"))
    );
}

// ---- heading links: projection and Outline --------------------------------------------

fn heading_links(prefix: Option<&str>, base: Option<&str>) -> MarkdownPlugin {
    create_markdown_heading_links(HeadingLinksOptions {
        heading_id_prefix: prefix.map(String::from),
        permalink_base_url: base.map(String::from),
    })
    .unwrap()
}

/// Every heading's projection (depth first; `None` for one without an id) and the Outline.
fn project(
    source: &str,
    options: ParseOptions,
) -> (Vec<Option<ProjectedHeading>>, Vec<OutlineItem>) {
    let prepared = prepare_markdown_plugins(&options.plugins).unwrap();
    let root = parse_markdown_ast(source, &options);
    let projection = project_markdown_headings(&root, Some(&prepared));
    let mut count = 0;
    visit_markdown_nodes(&root, NodeType::Heading, &mut |_| count += 1);
    let headings = (0..count)
        .map(|index| projection.heading(index).cloned())
        .collect();
    (headings, projection.outline)
}

fn heading(id: &str, label: &str, permalink: Option<&str>) -> Option<ProjectedHeading> {
    Some(ProjectedHeading {
        id: id.into(),
        label: label.into(),
        permalink_url: permalink.map(String::from),
    })
}

fn outline_item(id: &str, label: &str, level: u8) -> OutlineItem {
    OutlineItem {
        id: id.into(),
        label: label.into(),
        level,
    }
}

fn ids(headings: &[Option<ProjectedHeading>]) -> Vec<Option<String>> {
    headings
        .iter()
        .map(|heading| heading.as_ref().map(|heading| heading.id.clone()))
        .collect()
}

/// Upstream: "carries namespace and URL options across a duplicate compatible Core copy"
/// (the projected ids, permalink and Outline).
#[test]
fn carries_namespace_and_url_options_across_a_duplicate_compatible_core_copy() {
    let portable = heading_links(Some("portable"), Some("/reader?document=42#stale"));
    let source = "> # Nested\n\n# Root";
    let (headings, outline) = project(source, with_plugins(vec![portable.clone()]));
    assert_eq!(
        headings,
        vec![
            heading(
                "portable--nested",
                "Nested",
                Some("/reader?document=42#portable--nested")
            ),
            heading(
                "portable--root",
                "Root",
                Some("/reader?document=42#portable--root")
            ),
        ]
    );
    assert_eq!(outline, vec![outline_item("portable--root", "Root", 1)]);
    assert_eq!(
        parse_outline_from_markdown(source, &[portable], true),
        vec![outline_item("portable--root", "Root", 1)]
    );
}

/// Upstream: "fails soft for %s across Markdown and Outline", the case "same-name plugin
/// without module configuration".
#[test]
fn fails_soft_for_same_name_plugin_without_module_configuration_across_markdown_and_outline() {
    let incompatible = plugin_transform("heading-links", |root| root.clone());
    let source = "> # Nested\n\n# Root";
    let (headings, _) = project(source, with_plugins(vec![incompatible.clone()]));
    assert_eq!(headings, vec![None, heading("root", "Root", None)]);
    assert_eq!(
        parse_outline_from_markdown(source, &[incompatible], true),
        vec![outline_item("root", "Root", 1)]
    );
}

/// Upstream: "does not treat another valid plugin as heading-links configuration".
#[test]
fn does_not_treat_another_valid_plugin_as_heading_links_configuration() {
    let other = plugin_transform("other-plugin", |root| root.clone());
    let (headings, _) = project("> # Nested\n\n# Root", with_plugins(vec![other]));
    assert_eq!(headings, vec![None, heading("root", "Root", None)]);
}

/// Upstream: "links every semantic heading level".
#[test]
fn links_every_semantic_heading_level() {
    let source = (1..=6)
        .map(|level| format!("{} Level {level}", "#".repeat(level)))
        .collect::<Vec<_>>()
        .join("\n\n");
    let (headings, _) = project(&source, with_plugins(vec![heading_links(None, None)]));
    let expected: Vec<Option<ProjectedHeading>> = (1..=6)
        .map(|level| {
            heading(
                &format!("level-{level}"),
                &format!("Level {level}"),
                Some(&format!("#level-{level}")),
            )
        })
        .collect();
    assert_eq!(headings, expected);
}

/// Upstream: "shares one depth-first allocator while Outline remains root-only".
#[test]
fn shares_one_depth_first_allocator_while_outline_remains_root_only() {
    let links = heading_links(None, None);
    let source = "> # Quoted\n\n# Quoted";
    let (headings, _) = project(source, with_plugins(vec![links.clone()]));
    assert_eq!(
        ids(&headings),
        vec![Some("quoted".to_string()), Some("quoted-1".to_string())]
    );
    assert_eq!(
        parse_outline_from_markdown(source, &[links], true),
        vec![outline_item("quoted-1", "Quoted", 1)]
    );
}

/// Upstream: "reserves emitted ids across natural numeric-suffix collisions".
#[test]
fn reserves_emitted_ids_across_natural_numeric_suffix_collisions() {
    let links = heading_links(None, None);
    let (headings, _) = project(
        "# Foo\n\n# Foo\n\n# Foo-1",
        with_plugins(vec![links.clone()]),
    );
    assert_eq!(
        ids(&headings),
        vec![
            Some("foo".to_string()),
            Some("foo-1".to_string()),
            Some("foo-1-1".to_string())
        ]
    );
    let outline: Vec<String> =
        parse_outline_from_markdown("# Foo-1\n\n# Foo\n\n# Foo", &[links], true)
            .into_iter()
            .map(|item| item.id)
            .collect();
    assert_eq!(outline, strings(&["foo-1", "foo", "foo-2"]));
}

/// Upstream: "normalizes Unicode letters and numbers while stripping emoji".
#[test]
fn normalizes_unicode_letters_and_numbers_while_stripping_emoji() {
    let (headings, _) = project(
        "# Ｈｅｌｌｏ Привет 你好 😄 １２３",
        with_plugins(vec![heading_links(None, None)]),
    );
    assert_eq!(
        ids(&headings),
        vec![Some("hello-привет-你好-123".to_string())]
    );
}

/// Upstream: "uses a caller-owned namespace across Markdown and Outline".
#[test]
fn uses_a_caller_owned_namespace_across_markdown_and_outline() {
    let source = "# Overview\n\n## Details";
    let namespaced = heading_links(Some("article"), None);
    let (headings, _) = project(source, with_plugins(vec![namespaced.clone()]));
    assert_eq!(
        ids(&headings),
        vec![
            Some("article--overview".to_string()),
            Some("article--details".to_string())
        ]
    );
    assert_eq!(
        parse_outline_from_markdown(source, &[namespaced], true),
        vec![
            outline_item("article--overview", "Overview", 1),
            outline_item("article--details", "Details", 2),
        ]
    );
}

/// Upstream: "renders one honest sibling copy button for every owned heading" (every
/// heading, at every depth, gets an id and a permalink).
#[test]
fn renders_one_honest_sibling_copy_button_for_every_owned_heading() {
    let (headings, _) = project(
        "# Overview\n\n> ## Nested details",
        with_plugins(vec![heading_links(None, None)]),
    );
    assert_eq!(
        headings,
        vec![
            heading("overview", "Overview", Some("#overview")),
            heading("nested-details", "Nested details", Some("#nested-details")),
        ]
    );
}

/// Upstream: "uses a stable section fallback when the label has no slug text".
#[test]
fn uses_a_stable_section_fallback_when_the_label_has_no_slug_text() {
    let (headings, _) = project(
        "# [cite]",
        ParseOptions {
            source_ids: Some(BTreeSet::from(["cite".to_string()])),
            ..with_plugins(vec![heading_links(None, None)])
        },
    );
    assert_eq!(headings, vec![heading("section", "", Some("#section"))]);
}

/// Upstream: "keeps namespaces inside copied URLs and sanitizes optional bases".
#[test]
fn keeps_namespaces_inside_copied_urls_and_sanitizes_optional_bases() {
    let namespace = ["java", "script:alert(1)"].concat();
    let routed = heading_links(Some(&namespace), Some("/reader?document=42#stale"));
    let (headings, _) = project("# Overview", with_plugins(vec![routed]));
    assert_eq!(
        headings
            .first()
            .cloned()
            .flatten()
            .and_then(|heading| heading.permalink_url),
        Some(format!("/reader?document=42#{namespace}--overview"))
    );
    for unsafe_base in ["javascript:alert(1)", "https://%"] {
        let error = create_markdown_heading_links(HeadingLinksOptions {
            permalink_base_url: Some(unsafe_base.into()),
            ..HeadingLinksOptions::default()
        })
        .expect_err(unsafe_base);
        assert!(
            error.message().contains("safe, parseable navigation URL"),
            "{unsafe_base}: {}",
            error.message()
        );
    }
}

/// Upstream: "leaves permalink output to a custom heading renderer at every depth" (the
/// ids a custom renderer receives).
#[test]
fn leaves_permalink_output_to_a_custom_heading_renderer_at_every_depth() {
    let (headings, _) = project(
        "> # Nested\n\n# Root",
        with_plugins(vec![heading_links(None, None)]),
    );
    assert_eq!(
        ids(&headings),
        vec![Some("nested".to_string()), Some("root".to_string())]
    );
}
