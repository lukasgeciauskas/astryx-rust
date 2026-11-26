//! Upstream's semantic-fence and source-decoration acceptance tests
//! (`Markdown/plugins/semanticFence.test.tsx`, `semanticFence.shared.test.tsx` and
//! `Markdown/plugins/sourceDecoration.test.tsx`), ported to the canonical tree. Expectations
//! upstream writes against the released projection (`parseMarkdown`'s `codeblock`
//! `language`/`content`) are read from the canonical `Code` node (`lang`, `value`,
//! `Code::legacy_language`), and what upstream's renderer would show is read from the
//! fence proposal the transform pipeline returns. Offsets are UTF-8 bytes; every source here
//! is ASCII, so they equal upstream's UTF-16 offsets.
//!
//! Upstream's incremental entry points (`parseMarkdownIncremental`,
//! `parseMarkdownAstIncremental`, `trimStreamingArtifacts`) are not ported; the streaming
//! cases parse each prefix with `is_final: false` instead, which is what the incremental
//! parser hands the transforms.
//!
//! Skipped upstream cases:
//! - semanticFence `narrows createNode input and returns the owned extension-node
//!   transform`: TypeScript types only (`expectTypeOf`).
//! - semanticFence `falls back to components.code when a claimed fence renderer throws`:
//!   React rendering and SSR (error boundary, `components.code`).
//! - semanticFence `shows components.code while a claimed fence renderer suspends`:
//!   React Suspense / promise mechanics.
//! - semanticFence `consumes rejected async createNode values in client and SSR`: promise
//!   mechanics (a Rust `create_node` is synchronous by type) and SSR.
//! - semanticFence `falls back when a standard proposal renderer throws`: React rendering
//!   (renderer error boundary).
//! - semanticFence `falls back for descendant errors and during server rendering`: React
//!   rendering and SSR.
//! - semanticFence.shared `renders the fence as the plugin node`: module-instance identity
//!   (`vi.resetModules`, `Symbol.for` keys) and React rendering; Rust has one crate
//!   instance.
//! - sourceDecoration `is observable through the rendered component without changing its
//!   output`: React rendering (`renderToString`); the observer half is covered by
//!   `decorates_positioned_blocks_without_the_caller_opting_into_source_ranges`.
//! - sourceDecoration `freezes every sibling before an untrusted plugin observes the
//!   tree`: `Object.isFrozen` / in-place mutation mechanics; a Rust transform receives
//!   `&Root` and cannot mutate it.
//! - sourceDecoration `copies caller ranges so later mutation cannot change decorations`:
//!   aliasing mechanics; the Rust helper takes its options by value.
//!
//! Partly ported (the rest is outside the parser):
//! - semanticFence `annotates the original code node with frozen typed data`: the
//!   `Object.isFrozen` and identity (`not.toBe`) checks are skipped; the hand-built root
//!   goes through `parse_markdown_ast` (`applyMarkdownTransforms` is crate-private).
//! - semanticFence `renders typed proposal data through the standard renderer map`, `renders
//!   a claimed fence through its plugin and an unclaimed fence through components.code`,
//!   `uses CodeBlock for undeclared or declined fences`, `uses CodeBlock when the standard
//!   renderer is missing`, `matches the complete declared language without prefix
//!   collisions`, `preserves the first proposal and legacy language through node rebuilds`
//!   and `keeps no-plugin and nonmatching-plugin server output identical`: the DOM checks
//!   become checks on the proposal (or its absence) and the renderer map.
//! - semanticFence `falls back to components.code when a claimed fence is declined or its
//!   renderer returns nothing`: only the declined half; a renderer returning nothing is
//!   React rendering.
//! - semanticFence `fails the transform closed for foreign ownership or invalid data`: the
//!   invalid-data half (a function in `data`) cannot be expressed in a `serde_json::Value`.
//! - sourceDecoration `never mutates the document a transform received`: the
//!   `Object.isFrozen` check is skipped.
//! - sourceDecoration `leaves parsed results, rendered output, and heading identity
//!   unchanged`: the released projection drops `data`, so the canonical trees are compared
//!   with the decoration envelope removed; `renderToString` is skipped.
//! - sourceDecoration `rejects invalid options at creation`: a negative or fractional
//!   offset and a function payload cannot be expressed (`usize`, `serde_json::Value`).
//! - sourceDecoration `types as an extension-free transform and reads only its own
//!   envelope`: the `expectTypeOf` checks are skipped.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use astryx_markdown::ast::{
    Block, Code, ExtensionDisplay, ExtensionNode, NodeRef, NodeType, Paragraph, Root,
    visit_markdown_nodes,
};
use astryx_markdown::plugins::{
    DiagnosticCode, ExtensionRenderer, FenceContext, FenceTransformOptions, MarkdownPlugin,
    PluginDefinition, PluginDiagnostic, PluginPhase, SourceDecoration, SourceDecorationOptions,
    SourceDecorationRange, SyntaxCapability, SyntaxContribution, TokenizeResult, Transform,
    create_markdown_fence_transform, create_markdown_plugin, create_markdown_source_decoration,
    get_markdown_source_decorations, prepare_markdown_plugins,
};
use astryx_markdown::{
    ParseOptions, parse_markdown_ast, parse_outline_from_markdown, project_markdown_headings,
};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------

fn options(plugins: &[MarkdownPlugin]) -> ParseOptions {
    ParseOptions {
        plugins: plugins.to_vec(),
        ..ParseOptions::default()
    }
}

fn parse(source: &str, plugins: &[MarkdownPlugin]) -> Root {
    parse_markdown_ast(source, &options(plugins))
}

/// A parse and every diagnostic it delivered.
fn parse_with_diagnostics(
    source: &str,
    plugins: &[MarkdownPlugin],
) -> (Root, Vec<PluginDiagnostic>) {
    let diagnostics = Rc::new(RefCell::new(Vec::new()));
    let sink = diagnostics.clone();
    let root = parse_markdown_ast(
        source,
        &ParseOptions {
            plugins: plugins.to_vec(),
            on_plugin_diagnostic: Some(Rc::new(move |diagnostic: &PluginDiagnostic| {
                sink.borrow_mut().push(diagnostic.clone())
            })),
            ..ParseOptions::default()
        },
    );
    let diagnostics = diagnostics.borrow().clone();
    (root, diagnostics)
}

fn renderers(name: &str, key: &'static str) -> BTreeMap<String, ExtensionRenderer> {
    [(
        name.to_string(),
        ExtensionRenderer::new(move |node| {
            Ok(node.data[key].as_str().unwrap_or_default().to_string())
        }),
    )]
    .into()
}

fn block_extension(plugin: &str, name: &str, data: Value) -> ExtensionNode {
    ExtensionNode {
        plugin: plugin.into(),
        name: name.into(),
        data,
        display: ExtensionDisplay::Block,
        source: None,
        position: None,
    }
}

/// A plugin whose transform maps every top-level block.
fn map_plugin(name: &str, map: impl Fn(Block) -> Block + 'static) -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(move |root: &Root, _| {
            let mut next = root.clone();
            next.children = next.children.into_iter().map(&map).collect();
            Ok(Some(next))
        })),
        ..PluginDefinition::new(name)
    })
    .unwrap()
}

// ---------------------------------------------------------------------------------------
// Semantic fences
// ---------------------------------------------------------------------------------------

type CreateNode = Rc<dyn Fn(&FenceContext<'_>) -> Result<Option<ExtensionNode>, String>>;

/// Upstream `createDiagramNode`.
fn diagram_node(plugin: &str, context: &FenceContext<'_>) -> ExtensionNode {
    let mut data = json!({ "code": context.code, "language": context.language });
    if let Some(meta) = context.meta {
        data["meta"] = json!(meta);
    }
    block_extension(plugin, "diagram", data)
}

/// Upstream `createFencePlugin` with a custom `createNode`.
fn fence_plugin_with(name: &str, create_node: CreateNode) -> MarkdownPlugin {
    let transform = create_markdown_fence_transform(FenceTransformOptions {
        languages: vec!["diagram".into()],
        create_node,
    })
    .unwrap();
    create_markdown_plugin(PluginDefinition {
        transform: Some(transform),
        renderers: renderers("diagram", "code"),
        ..PluginDefinition::new(name)
    })
    .unwrap()
}

/// Upstream `createFencePlugin` with its default `createNode`.
fn fence_plugin(name: &str) -> MarkdownPlugin {
    let owner = name.to_string();
    fence_plugin_with(
        name,
        Rc::new(move |context| Ok(Some(diagram_node(&owner, context)))),
    )
}

/// A plugin claiming `language` fences as `{plugin}`'s `code` node, counting its
/// `createNode` calls.
fn code_fence_plugin(name: &str, language: &str, calls: Rc<Cell<usize>>) -> MarkdownPlugin {
    let owner = name.to_string();
    let transform = create_markdown_fence_transform(FenceTransformOptions {
        languages: vec![language.into()],
        create_node: Rc::new(move |context| {
            calls.set(calls.get() + 1);
            Ok(Some(block_extension(
                &owner,
                "code",
                json!({ "value": context.code }),
            )))
        }),
    })
    .unwrap();
    create_markdown_plugin(PluginDefinition {
        transform: Some(transform),
        renderers: renderers("code", "value"),
        ..PluginDefinition::new(name)
    })
    .unwrap()
}

fn code_at(root: &Root, index: usize) -> &Code {
    match &root.children[index] {
        Block::Code(code) => code,
        other => panic!("block {index} is not code: {other:?}"),
    }
}

fn all_code(root: &Root) -> Vec<&Code> {
    let mut found = Vec::new();
    visit_markdown_nodes(root, NodeType::Code, &mut |node| {
        if let NodeRef::Code(code) = node {
            found.push(code);
        }
    });
    found
}

fn transform_failures<'a>(
    diagnostics: &'a [PluginDiagnostic],
    plugin: &str,
) -> Vec<&'a PluginDiagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.plugin == plugin && diagnostic.phase == PluginPhase::Transform
        })
        .collect()
}

/// Upstream: `annotates the original code node with frozen typed data`.
#[test]
fn annotates_the_original_code_node_with_frozen_typed_data() {
    let source = "```diagram title=\"Flow\"\nstart --> finish\n```";
    // The upstream root's code node carries `data: {owner: 'source'}`.
    let owner = map_plugin("source-owner", |block| match block {
        Block::Code(code) => Block::Code(Code {
            data: Some(json!({ "owner": "source" })),
            ..code
        }),
        other => other,
    });
    let plugin = fence_plugin("diagram-fences");

    let original = parse(source, std::slice::from_ref(&owner));
    let transformed = parse(source, &[owner, plugin]);
    let code = code_at(&original, 0);
    let transformed_code = code_at(&transformed, 0);

    assert_eq!(code.lang.as_deref(), Some("diagram"));
    assert_eq!(code.meta.as_deref(), Some("title=\"Flow\""));
    assert_eq!(code.value, "start --> finish");
    assert_eq!(code.data, Some(json!({ "owner": "source" })));
    // `toMatchObject(code)`: every field the original has, unchanged.
    assert_eq!(transformed_code.lang, code.lang);
    assert_eq!(transformed_code.meta, code.meta);
    assert_eq!(transformed_code.value, code.value);
    assert_eq!(transformed_code.data, code.data);
    assert_eq!(transformed_code.position, code.position);
    assert!(code.fence_proposal().is_none());
    assert_eq!(
        transformed_code.fence_proposal(),
        Some(&block_extension(
            "diagram-fences",
            "diagram",
            json!({
                "code": "start --> finish",
                "language": "diagram",
                "meta": "title=\"Flow\"",
            }),
        )),
    );
}

/// Upstream: `preserves fence metadata in the canonical tree without changing legacy output`.
#[test]
fn preserves_fence_metadata_in_the_canonical_tree_without_changing_legacy_output() {
    let source = "```diagram title=\"Flow\"\nstart --> finish\n```";
    let root = parse(source, &[]);
    assert_eq!(root.children.len(), 1);
    let code = code_at(&root, 0);
    assert_eq!(code.lang.as_deref(), Some("diagram"));
    assert_eq!(code.meta.as_deref(), Some("title=\"Flow\""));
    assert_eq!(code.value, "start --> finish");
    // Released projection: `{type: 'codeblock', language: 'diagram', content: ...}`.
    assert_eq!(code.legacy_language(), Some("diagram"));
}

/// Upstream: `matches the complete declared language without prefix collisions`.
#[test]
fn matches_the_complete_declared_language_without_prefix_collisions() {
    let prefix_calls = Rc::new(Cell::new(0));
    let exact_calls = Rc::new(Cell::new(0));
    let prefix = code_fence_plugin("c-fences", "c", prefix_calls.clone());
    let exact = code_fence_plugin("cpp-fences", "c++", exact_calls.clone());
    let source = "```c++ title=\"Example\"\nint main() {}\n```";

    let plain = parse(source, &[]);
    let code = code_at(&plain, 0);
    assert_eq!(code.lang.as_deref(), Some("c++"));
    assert_eq!(code.meta.as_deref(), Some("title=\"Example\""));
    // Released projection: `{type: 'codeblock', language: 'c', content: 'int main() {}'}`.
    assert_eq!(code.legacy_language(), Some("c"));
    assert_eq!(code.value, "int main() {}");

    let claimed = parse(source, &[prefix, exact]);
    let code = code_at(&claimed, 0);
    assert_eq!(
        code.fence_proposal(),
        Some(&block_extension(
            "cpp-fences",
            "code",
            json!({ "value": "int main() {}" }),
        )),
    );
    assert_eq!(prefix_calls.get(), 0);
    assert_eq!(exact_calls.get(), 1);
}

/// Upstream: `renders typed proposal data through the standard renderer map`.
#[test]
fn renders_typed_proposal_data_through_the_standard_renderer_map() {
    let plugin = fence_plugin("typed-diagram-fences");
    let root = parse(
        "```diagram title=\"Flow\"\nstart --> finish\n```",
        std::slice::from_ref(&plugin),
    );
    let proposal = code_at(&root, 0).fence_proposal().expect("a proposal");
    assert_eq!(
        proposal.data,
        json!({
            "code": "start --> finish",
            "language": "diagram",
            "meta": "title=\"Flow\"",
        }),
    );
    let prepared = prepare_markdown_plugins(&[plugin]).unwrap();
    let renderer = prepared.renderer(proposal).expect("the standard renderer");
    assert_eq!((renderer.to_text)(proposal).unwrap(), "start --> finish");
}

/// Upstream: `renders a claimed fence through its plugin and an unclaimed fence through
/// components.code`.
#[test]
fn renders_a_claimed_fence_through_its_plugin_and_an_unclaimed_fence_through_components_code() {
    let plugin = fence_plugin("override-fences");
    let root = parse(
        "```diagram\nstart --> finish\n```\n\n```ts\nconst x = 1;\n```",
        &[plugin],
    );
    let codes = all_code(&root);
    assert_eq!(codes.len(), 2);
    assert_eq!(
        codes[0].fence_proposal().map(|node| node.plugin.as_str()),
        Some("override-fences"),
    );
    // Exactly one fence reaches `components.code`: the `ts` one.
    let unclaimed: Vec<_> = codes
        .iter()
        .filter(|code| code.fence_proposal().is_none())
        .map(|code| (code.legacy_language(), code.value.as_str()))
        .collect();
    assert_eq!(unclaimed, vec![(Some("ts"), "const x = 1;")]);
}

/// Upstream: `falls back to components.code when a claimed fence is declined or its renderer
/// returns nothing`.
#[test]
fn falls_back_to_components_code_when_a_claimed_fence_is_declined_or_its_renderer_returns_nothing()
{
    let declining = fence_plugin_with("declining-override-fences", Rc::new(|_| Ok(None)));
    let root = parse("```diagram\nstart --> finish\n```", &[declining]);
    let code = code_at(&root, 0);
    assert!(code.fence_proposal().is_none());
    assert_eq!(code.value, "start --> finish");
}

/// Upstream: `uses CodeBlock for undeclared or declined fences`.
#[test]
fn uses_code_block_for_undeclared_or_declined_fences() {
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let plugin = fence_plugin_with(
        "declining-fences",
        Rc::new(move |_| {
            counter.set(counter.get() + 1);
            Ok(None)
        }),
    );

    let root = parse("```text\nplain source\n```", std::slice::from_ref(&plugin));
    assert_eq!(calls.get(), 0);
    let code = code_at(&root, 0);
    assert!(code.fence_proposal().is_none());
    assert_eq!(code.value, "plain source");
    assert_eq!(code.legacy_language(), Some("text"));

    let root = parse("```diagram\nsemantic source\n```", &[plugin]);
    assert_eq!(calls.get(), 1);
    let code = code_at(&root, 0);
    assert!(code.fence_proposal().is_none());
    assert_eq!(code.value, "semantic source");
}

/// Upstream: `uses CodeBlock when the standard renderer is missing`.
#[test]
fn uses_code_block_when_the_standard_renderer_is_missing() {
    let transform = create_markdown_fence_transform(FenceTransformOptions {
        languages: vec!["diagram".into()],
        create_node: Rc::new(|context| Ok(Some(diagram_node("missing-renderer", context)))),
    })
    .unwrap();
    let plugin = create_markdown_plugin(PluginDefinition {
        transform: Some(transform),
        ..PluginDefinition::new("missing-renderer")
    })
    .unwrap();

    let (root, diagnostics) =
        parse_with_diagnostics("```diagram\nstart --> finish\n```", &[plugin]);
    let code = code_at(&root, 0);
    assert!(code.fence_proposal().is_none());
    assert_eq!(code.value, "start --> finish");
    // The helper refuses the node (no registered renderer), so the transform fails closed.
    assert!(!transform_failures(&diagnostics, "missing-renderer").is_empty());
}

/// Upstream: `fails the transform closed for foreign ownership or invalid data`.
#[test]
fn fails_the_transform_closed_for_foreign_ownership_or_invalid_data() {
    let foreign = fence_plugin_with(
        "owned-fences",
        Rc::new(|_| {
            Ok(Some(block_extension(
                "foreign",
                "diagram",
                json!({ "code": "forged", "language": "diagram" }),
            )))
        }),
    );
    let (root, diagnostics) =
        parse_with_diagnostics("```diagram\nreadable source\n```", &[foreign]);
    let code = code_at(&root, 0);
    assert!(code.fence_proposal().is_none());
    assert_eq!(code.value, "readable source");
    // Upstream: `expect(warning).toHaveBeenCalled()`.
    assert!(!transform_failures(&diagnostics, "owned-fences").is_empty());
}

/// Upstream: `keeps no-plugin and nonmatching-plugin server output identical`.
#[test]
fn keeps_no_plugin_and_nonmatching_plugin_server_output_identical() {
    let plugin = fence_plugin("nonmatching-fences");
    let source = "```text\nplain source\n```";
    let with_plugin = parse(source, &[plugin]);
    assert_eq!(with_plugin, parse(source, &[]));
    assert!(code_at(&with_plugin, 0).fence_proposal().is_none());
}

/// Upstream: `preserves the first proposal and legacy language through node rebuilds`.
#[test]
fn preserves_the_first_proposal_and_legacy_language_through_node_rebuilds() {
    let first_calls = Rc::new(Cell::new(0));
    let second_calls = Rc::new(Cell::new(0));
    let first = code_fence_plugin("first-cpp-fences", "c++", first_calls.clone());
    // Rebuilds each code node from its public fields only, as upstream's spread does.
    let rebuild = map_plugin("rebuild-code-nodes", |block| match block {
        Block::Code(code) => Block::Code(Code {
            lang: code.lang,
            meta: code.meta,
            value: code.value,
            data: code.data,
            position: code.position,
            ..Code::default()
        }),
        other => other,
    });
    let second = code_fence_plugin("second-cpp-fences", "c++", second_calls.clone());
    let source = "```c++\nint main() {}\n```";

    let root = parse(source, &[first, rebuild, second]);
    let code = code_at(&root, 0);
    // Released projection: `{type: 'codeblock', language: 'c', content: 'int main() {}'}`.
    assert_eq!(code.legacy_language(), Some("c"));
    assert_eq!(code.value, "int main() {}");
    assert_eq!(
        code.fence_proposal(),
        Some(&block_extension(
            "first-cpp-fences",
            "code",
            json!({ "value": "int main() {}" }),
        )),
    );
    assert_eq!(first_calls.get(), 1);
}

// ---------------------------------------------------------------------------------------
// Source decorations
// ---------------------------------------------------------------------------------------

const SOURCE: &str = "# Release notes\n\nAda shipped the parser.\n\n> Quoted detail";

type Seen = Rc<RefCell<BTreeMap<String, Vec<SourceDecoration>>>>;

fn span_of(text: &str, source: &str) -> (usize, usize) {
    let start = source
        .find(text)
        .unwrap_or_else(|| panic!("{text:?} not in source"));
    (start, start + text.len())
}

fn span(text: &str) -> (usize, usize) {
    span_of(text, SOURCE)
}

fn range((start, end): (usize, usize)) -> SourceDecorationRange {
    SourceDecorationRange {
        start,
        end,
        data: None,
    }
}

fn range_with((start, end): (usize, usize), data: Value) -> SourceDecorationRange {
    SourceDecorationRange {
        start,
        end,
        data: Some(data),
    }
}

fn hit(name: &str) -> SourceDecoration {
    SourceDecoration {
        name: name.into(),
        data: None,
    }
}

fn hit_with(name: &str, data: Value) -> SourceDecoration {
    SourceDecoration {
        name: name.into(),
        data: Some(data),
    }
}

fn block_key(index: usize, block: &Block) -> String {
    format!("{index}:{}", NodeRef::from(block).node_type().as_str())
}

/// Upstream `observerPlugin`: records each top-level block's decorations, as later plugins
/// and rendering observe them.
fn observer(name: &str, seen: &Seen) -> MarkdownPlugin {
    let seen = seen.clone();
    create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(move |root: &Root, _| {
            let mut seen = seen.borrow_mut();
            seen.clear();
            for (index, block) in root.children.iter().enumerate() {
                seen.insert(
                    block_key(index, block),
                    get_markdown_source_decorations(NodeRef::from(block)),
                );
            }
            Ok(None)
        })),
        ..PluginDefinition::new(name)
    })
    .unwrap()
}

fn decoration(name: &str, ranges: Vec<SourceDecorationRange>) -> Transform {
    create_markdown_source_decoration(SourceDecorationOptions {
        name: name.into(),
        ranges,
    })
    .unwrap()
}

/// Upstream `decorationPlugin`.
fn decoration_plugin(name: &str, transform: Transform) -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        transform: Some(transform),
        ..PluginDefinition::new(name)
    })
    .unwrap()
}

fn new_seen() -> Seen {
    Rc::new(RefCell::new(BTreeMap::new()))
}

fn get(seen: &Seen, key: &str) -> Vec<SourceDecoration> {
    seen.borrow()
        .get(key)
        .cloned()
        .unwrap_or_else(|| panic!("no block {key}; saw {:?}", seen.borrow().keys()))
}

/// Upstream `decorationsFor`.
fn decorations_for(source: &str, ranges: Vec<SourceDecorationRange>, name: &str) -> Seen {
    let seen = new_seen();
    parse(
        source,
        &[
            decoration_plugin(&format!("{name}-decoration"), decoration(name, ranges)),
            observer(&format!("{name}-observer"), &seen),
        ],
    );
    seen
}

fn data_slot(block: &mut Block) -> Option<&mut Option<Value>> {
    match block {
        Block::Heading(node) => Some(&mut node.data),
        Block::Paragraph(node) => Some(&mut node.data),
        Block::Code(node) => Some(&mut node.data),
        Block::Math(node) => Some(&mut node.data),
        Block::Blockquote(node) => Some(&mut node.data),
        Block::List(node) => Some(&mut node.data),
        Block::Table(node) => Some(&mut node.data),
        Block::ThematicBreak(node) => Some(&mut node.data),
        Block::Image(node) => Some(&mut node.data),
        Block::Extension(_) => None,
    }
}

/// The tree without the top-level blocks' `data`, which the released projection drops.
fn without_data(mut root: Root) -> Root {
    for block in &mut root.children {
        if let Some(data) = data_slot(block) {
            *data = None;
        }
    }
    root
}

/// Upstream: `decorates positioned blocks without the caller opting into sourceRanges`.
#[test]
fn decorates_positioned_blocks_without_the_caller_opting_into_source_ranges() {
    let seen = new_seen();
    parse(
        SOURCE,
        &[
            decoration_plugin(
                "covers-paragraph",
                decoration(
                    "review-hit",
                    vec![range_with(
                        span("Ada shipped the parser."),
                        json!({ "reviewer": "ada" }),
                    )],
                ),
            ),
            observer("covers-paragraph-observer", &seen),
        ],
    );

    assert_eq!(
        get(&seen, "1:paragraph"),
        vec![hit_with("review-hit", json!({ "reviewer": "ada" }))],
    );
    assert_eq!(get(&seen, "0:heading"), vec![]);
    assert_eq!(get(&seen, "2:blockquote"), vec![]);
}

/// Upstream: `decorates every block a range touches, including partial overlap`.
#[test]
fn decorates_every_block_a_range_touches_including_partial_overlap() {
    let heading = span("# Release notes");
    let paragraph = span("Ada shipped the parser.");

    // One range covering two whole blocks.
    let spanning = decorations_for(SOURCE, vec![range((heading.0, paragraph.1))], "hit");
    assert_eq!(get(&spanning, "0:heading"), vec![hit("hit")]);
    assert_eq!(get(&spanning, "1:paragraph"), vec![hit("hit")]);
    assert_eq!(get(&spanning, "2:blockquote"), vec![]);

    // A range strictly inside one block still decorates that block.
    let inside = decorations_for(SOURCE, vec![range(span("shipped"))], "hit");
    assert_eq!(get(&inside, "1:paragraph"), vec![hit("hit")]);
    assert_eq!(get(&inside, "0:heading"), vec![]);

    // A range straddling a boundary touches both blocks.
    let straddling = decorations_for(
        SOURCE,
        vec![range((paragraph.1 - 4, span("> Quoted detail").0 + 3))],
        "hit",
    );
    assert_eq!(get(&straddling, "1:paragraph"), vec![hit("hit")]);
    assert_eq!(get(&straddling, "2:blockquote"), vec![hit("hit")]);
}

/// Upstream: `is observable through Outline without changing heading identity`.
#[test]
fn is_observable_through_outline_without_changing_heading_identity() {
    let seen = new_seen();
    let plugins = [
        decoration_plugin(
            "outline-decoration",
            decoration("search-hit", vec![range(span("# Release notes"))]),
        ),
        observer("outline-observer", &seen),
    ];

    assert_eq!(
        parse_outline_from_markdown(SOURCE, &plugins, true),
        parse_outline_from_markdown(SOURCE, &[], true),
    );
    assert_eq!(get(&seen, "0:heading"), vec![hit("search-hit")]);
}

/// Upstream: `exposes decorations only on the settled snapshot, never present then absent`.
#[test]
fn exposes_decorations_only_on_the_settled_snapshot_never_present_then_absent() {
    let source = [
        "# Title",
        "",
        "Intro prose here.",
        "",
        "| Item | Detail |",
        "| --- | --- |",
        "| 1 | two |",
        "",
        "Tail prose.",
    ]
    .join("\n");

    for target in [
        "Intro prose here.",
        "| Item | Detail |",
        "| 1 | two |",
        "Tail prose.",
    ] {
        let seen = new_seen();
        let plugins = vec![
            decoration_plugin(
                "streaming-decoration",
                decoration("hit", vec![range(span_of(target, &source))]),
            ),
            observer("streaming-observer", &seen),
        ];

        let mut presence = Vec::new();
        for length in 1..=source.len() {
            let is_final = length == source.len();
            seen.borrow_mut().clear();
            parse_markdown_ast(
                &source[..length],
                &ParseOptions {
                    plugins: plugins.clone(),
                    is_final,
                    ..ParseOptions::default()
                },
            );
            presence.push(seen.borrow().values().any(|entries| !entries.is_empty()));
        }

        assert!(
            presence[..presence.len() - 1].iter().all(|shown| !shown),
            "{target:?}: decorations shown before the source settled",
        );
        assert!(
            presence[presence.len() - 1],
            "{target:?}: no decoration once final"
        );
    }
}

/// Upstream: `decorates blocks an earlier transform reordered`.
#[test]
fn decorates_blocks_an_earlier_transform_reordered() {
    let heading = span("# Release notes");
    let quote = span("> Quoted detail");
    let reverse = create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(|root: &Root, _| {
            let mut next = root.clone();
            next.children.reverse();
            Ok(Some(next))
        })),
        ..PluginDefinition::new("reverse-blocks")
    })
    .unwrap();
    let seen = new_seen();
    parse(
        SOURCE,
        &[
            reverse,
            decoration_plugin(
                "reordered",
                decoration(
                    "hit",
                    vec![range(heading), range_with(quote, json!({ "tail": true }))],
                ),
            ),
            observer("reordered-observer", &seen),
        ],
    );

    // Reversed order: blockquote, paragraph, heading.
    assert_eq!(
        get(&seen, "0:blockquote"),
        vec![hit_with("hit", json!({ "tail": true }))]
    );
    assert_eq!(get(&seen, "1:paragraph"), vec![]);
    assert_eq!(get(&seen, "2:heading"), vec![hit("hit")]);
}

/// Upstream: `resolves overlapping, duplicate, and unordered ranges deterministically`.
#[test]
fn resolves_overlapping_duplicate_and_unordered_ranges_deterministically() {
    let paragraph = span("Ada shipped the parser.");
    let wide = (0, paragraph.1);
    let run = || {
        let seen = new_seen();
        parse(
            SOURCE,
            &[
                decoration_plugin(
                    "overlaps",
                    decoration(
                        "hit",
                        vec![
                            range_with(paragraph, json!({ "rank": 2 })),
                            range(wide),
                            range_with(paragraph, json!({ "rank": 1 })),
                            // Exact duplicate of the first range.
                            range_with(paragraph, json!({ "rank": 2 })),
                            range(paragraph),
                        ],
                    ),
                ),
                observer("overlaps-observer", &seen),
            ],
        );
        get(&seen, "1:paragraph")
    };

    assert_eq!(
        run(),
        vec![
            // The wide range starts first.
            hit("hit"),
            // Then the three paragraph-aligned ranges, payload-free first.
            hit("hit"),
            hit_with("hit", json!({ "rank": 1 })),
            hit_with("hit", json!({ "rank": 2 })),
        ],
    );
    assert_eq!(run(), run());
}

/// Upstream: `records each range once per block however many blocks it sweeps`.
#[test]
fn records_each_range_once_per_block_however_many_blocks_it_sweeps() {
    let sections = 200;
    let source = (0..sections)
        .map(|index| format!("Section {index} prose."))
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut ranges = vec![range((0, source.len()))];
    ranges.extend(
        (0..sections).map(|index| range(span_of(&format!("Section {index} prose."), &source))),
    );
    let seen = decorations_for(&source, ranges, "hit");

    assert_eq!(seen.borrow().len(), sections);
    for index in 0..sections {
        // The document-wide range and exactly this block's own range.
        assert_eq!(
            get(&seen, &format!("{index}:paragraph")),
            vec![hit("hit"), hit("hit")]
        );
    }
}

/// Upstream: `appends later plugins after earlier ones`.
#[test]
fn appends_later_plugins_after_earlier_ones() {
    let paragraph = span("Ada shipped the parser.");
    let seen = new_seen();
    parse(
        SOURCE,
        &[
            decoration_plugin("first-pass", decoration("first", vec![range(paragraph)])),
            decoration_plugin("second-pass", decoration("second", vec![range(paragraph)])),
            observer("order-observer", &seen),
        ],
    );

    assert_eq!(get(&seen, "1:paragraph"), vec![hit("first"), hit("second")]);
}

/// Upstream: `leaves parsed results, rendered output, and heading identity unchanged`.
#[test]
fn leaves_parsed_results_rendered_output_and_heading_identity_unchanged() {
    let decorated = decoration_plugin(
        "invariance",
        decoration(
            "hit",
            vec![
                range(span("# Release notes")),
                range(span("Ada shipped the parser.")),
            ],
        ),
    );

    let plain = parse(SOURCE, &[]);
    let with_decoration = parse(SOURCE, std::slice::from_ref(&decorated));
    // The decorations are there, and they are all that changed.
    assert!(
        !get_markdown_source_decorations(NodeRef::from(&with_decoration.children[0])).is_empty()
    );
    assert_eq!(without_data(with_decoration.clone()), plain);

    let ranged = |plugins: &[MarkdownPlugin]| {
        parse_markdown_ast(
            SOURCE,
            &ParseOptions {
                source_ranges: true,
                plugins: plugins.to_vec(),
                ..ParseOptions::default()
            },
        )
    };
    assert_eq!(
        without_data(ranged(std::slice::from_ref(&decorated))),
        ranged(&[])
    );

    // Heading identity.
    assert_eq!(
        project_markdown_headings(&with_decoration, None),
        project_markdown_headings(&plain, None),
    );
    assert_eq!(
        parse_outline_from_markdown(SOURCE, &[decorated], true),
        parse_outline_from_markdown(SOURCE, &[], true),
    );
}

/// Upstream: `never mutates the document a transform received`.
#[test]
fn never_mutates_the_document_a_transform_received() {
    let paragraph = span("Ada shipped the parser.");
    let before: Rc<RefCell<Vec<Vec<SourceDecoration>>>> = Rc::default();
    let after: Rc<RefCell<Vec<Vec<SourceDecoration>>>> = Rc::default();
    let capture = |name: &str, sink: &Rc<RefCell<Vec<Vec<SourceDecoration>>>>| {
        let sink = sink.clone();
        create_markdown_plugin(PluginDefinition {
            transform: Some(Transform::new(move |root: &Root, _| {
                sink.borrow_mut()
                    .push(get_markdown_source_decorations(NodeRef::from(
                        &root.children[1],
                    )));
                Ok(None)
            })),
            ..PluginDefinition::new(name)
        })
        .unwrap()
    };

    parse(
        SOURCE,
        &[
            capture("mutation-before", &before),
            decoration_plugin(
                "mutation-decorate",
                decoration("hit", vec![range(paragraph)]),
            ),
            capture("mutation-after", &after),
        ],
    );

    assert_eq!(*before.borrow(), vec![vec![]]);
    assert_eq!(*after.borrow(), vec![vec![hit("hit")]]);
}

/// Upstream: `leaves a prior streamed snapshot untouched when a later chunk decorates`.
#[test]
fn leaves_a_prior_streamed_snapshot_untouched_when_a_later_chunk_decorates() {
    let source = "Alpha prose.\n\nBeta prose.";
    let target = span_of("Beta prose.", source);
    let plugins = vec![decoration_plugin(
        "snapshot-decoration",
        decoration("hit", vec![range(target)]),
    )];

    let partial = parse_markdown_ast(
        &source[..target.0],
        &ParseOptions {
            plugins: plugins.clone(),
            is_final: false,
            ..ParseOptions::default()
        },
    );
    let snapshot = partial.clone();
    let full = parse_markdown_ast(
        source,
        &ParseOptions {
            plugins,
            is_final: true,
            ..ParseOptions::default()
        },
    );

    assert_eq!(partial, snapshot);
    assert!(
        partial
            .children
            .iter()
            .all(|block| get_markdown_source_decorations(NodeRef::from(block)).is_empty())
    );
    // The final parse decorates the target block.
    assert_eq!(
        get_markdown_source_decorations(NodeRef::from(&full.children[1])),
        vec![hit("hit")],
    );
}

/// Upstream: `leaves plugin extension nodes to their owner`.
#[test]
fn leaves_plugin_extension_nodes_to_their_owner() {
    let source = ":::note\nOwned by the plugin\n:::\n\nAda shipped it.";
    let notes = create_markdown_plugin(PluginDefinition {
        parse_key: Some("v1".into()),
        syntax: Some(SyntaxCapability {
            inline: vec![],
            block: vec![SyntaxContribution::new([":::note"], 200, |input| {
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
                    node: block_extension(
                        "decoration-notes",
                        "note",
                        json!({ "body": input.source[input.offset + 7..close].trim() }),
                    ),
                })
            })],
        }),
        renderers: renderers("note", "body"),
        ..PluginDefinition::new("decoration-notes")
    })
    .unwrap();

    let seen = new_seen();
    let root = parse(
        source,
        &[
            notes,
            decoration_plugin(
                "extension-ownership",
                decoration("hit", vec![range((0, source.len()))]),
            ),
            observer("extension-ownership-observer", &seen),
        ],
    );

    assert_eq!(get(&seen, "0:extension"), vec![]);
    assert_eq!(get(&seen, "1:paragraph"), vec![hit("hit")]);
    match &root.children[0] {
        Block::Extension(node) => {
            assert_eq!(node.data, json!({ "body": "Owned by the plugin" }));
        }
        other => panic!("not an extension node: {other:?}"),
    }
}

/// Upstream: `stores decorations in a versioned Core-owned envelope`.
#[test]
fn stores_decorations_in_a_versioned_core_owned_envelope() {
    let root = parse(
        SOURCE,
        &[decoration_plugin(
            "envelope-shape",
            decoration("hit", vec![range(span("Ada shipped the parser."))]),
        )],
    );

    assert_eq!(
        NodeRef::from(&root.children[1]).data(),
        Some(&json!({
            "astryx:sourceDecorations": {
                "kind": "astryx.markdown.sourceDecorations",
                "version": 1,
                "entries": [{ "name": "hit" }],
            },
        })),
    );
}

/// Upstream: `never merges into foreign data and reports the skip once`.
#[test]
fn never_merges_into_foreign_data_and_reports_the_skip_once() {
    let seen = new_seen();
    // Two foreign shapes: an opaque scalar, and an unbranded array parked at the envelope
    // key by another plugin.
    let foreign = map_plugin("foreign-data", |block| match block {
        Block::Heading(mut heading) => {
            heading.data = Some(json!("opaque"));
            Block::Heading(heading)
        }
        Block::Blockquote(mut quote) => {
            quote.data = Some(json!({ "astryx:sourceDecorations": [{ "name": "forged" }] }));
            Block::Blockquote(quote)
        }
        other => other,
    });

    let (_, diagnostics) = parse_with_diagnostics(
        SOURCE,
        &[
            foreign,
            decoration_plugin(
                "foreign-data-decoration",
                decoration("hit", vec![range((0, SOURCE.len()))]),
            ),
            observer("foreign-data-observer", &seen),
        ],
    );

    assert_eq!(get(&seen, "0:heading"), vec![]);
    assert_eq!(get(&seen, "2:blockquote"), vec![]);
    assert_eq!(get(&seen, "1:paragraph"), vec![hit("hit")]);
    // Upstream: one warning, naming the decorating plugin.
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].plugin, "foreign-data-decoration");
    assert_eq!(diagnostics[0].phase, PluginPhase::Transform);
    assert_eq!(diagnostics[0].code, DiagnosticCode::TransformReport);
}

/// Upstream: `keeps unrelated data on a node it decorates`.
#[test]
fn keeps_unrelated_data_on_a_node_it_decorates() {
    let paragraph = span("Ada shipped the parser.");
    let annotate = map_plugin("annotate-first", |block| match block {
        Block::Paragraph(mut node) => {
            node.data = Some(json!({ "owner": "ada" }));
            Block::Paragraph(node)
        }
        other => other,
    });

    let root = parse(
        SOURCE,
        &[
            annotate,
            decoration_plugin(
                "annotate-decoration",
                decoration("hit", vec![range(paragraph)]),
            ),
        ],
    );

    assert_eq!(
        NodeRef::from(&root.children[1]).data(),
        Some(&json!({
            "owner": "ada",
            "astryx:sourceDecorations": {
                "kind": "astryx.markdown.sourceDecorations",
                "version": 1,
                "entries": [{ "name": "hit" }],
            },
        })),
    );
}

/// Upstream: `rejects invalid options at creation`.
#[test]
fn rejects_invalid_options_at_creation() {
    let create = |name: &str, ranges: Vec<SourceDecorationRange>| {
        create_markdown_source_decoration(SourceDecorationOptions {
            name: name.into(),
            ranges,
        })
    };
    assert!(create(" ", vec![]).is_err());
    assert!(create("hit", vec![range((4, 4))]).is_err());
}

/// Upstream: `treats an empty range list as a no-op transform`.
#[test]
fn treats_an_empty_range_list_as_a_no_op_transform() {
    let seen = decorations_for(SOURCE, vec![], "hit");
    assert!(seen.borrow().values().all(Vec::is_empty));
}

/// Upstream: `types as an extension-free transform and reads only its own envelope`.
#[test]
fn types_as_an_extension_free_transform_and_reads_only_its_own_envelope() {
    // A decoration plugin introduces no extension nodes, so it needs no renderers.
    let plugin = decoration_plugin("typed-decoration", decoration("hit", vec![range((0, 4))]));
    assert_eq!(plugin.name(), "typed-decoration");

    let read = |data: Option<Value>| {
        get_markdown_source_decorations(NodeRef::Paragraph(&Paragraph {
            data,
            ..Paragraph::default()
        }))
    };
    assert_eq!(read(None), vec![]);
    assert_eq!(read(Some(json!("opaque"))), vec![]);
    assert_eq!(
        read(Some(json!({ "decorations": [{ "name": "x" }] }))),
        vec![]
    );
    assert_eq!(
        read(Some(
            json!({ "astryx:sourceDecorations": [{ "name": "x" }] })
        )),
        vec![]
    );
    assert_eq!(
        read(Some(json!({
            "astryx:sourceDecorations": {
                "kind": "astryx.markdown.sourceDecorations",
                "version": 2,
                "entries": [{ "name": "x" }],
            },
        }))),
        vec![],
    );
    assert_eq!(
        read(Some(json!({
            "astryx:sourceDecorations": {
                "kind": "astryx.markdown.sourceDecorations",
                "version": 1,
                "entries": [{ "name": "x" }],
            },
        }))),
        vec![hit("x")],
    );
}
