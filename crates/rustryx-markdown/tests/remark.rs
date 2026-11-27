//! Upstream `Markdown/remark.test.tsx`, the Remark adapter (`createMarkdownRemarkTransform`),
//! ported onto `rustryx_markdown::remark`: the canonical tree goes to a mutable mdast tree
//! (`serde_json::Value`), a synchronous transformer runs over it with an isolated
//! [`RemarkFile`], and the result comes back only if it round-trips. Upstream's
//! `runAdapter` calls the transform directly with a `report` collector; here the transform's
//! `Err` is that single report (`Ok(None)` is upstream's `result === root` with no report).
//!
//! Skipped upstream cases (no Rust analog, or not parser-side):
//! - "leaves fenced code copyable and renders adapted output in the browser and on the
//!   server": React/SSR rendering; its parser half is
//!   `leaves_fenced_code_copyable_parser_side`.
//! - "shares transformed heading identity with Outline" and "lets a transform remove a
//!   heading ...": the rendered DOM half is skipped; the heading projection stands in for
//!   the rendered heading's id.
//! - "rejects reading file.%s" (11 keys), "rejects reading an arbitrary key without echoing
//!   its name", "rejects writing file.%s" (3 keys), "rejects defining or deleting a file
//!   property", "lets a plugin feature-detect instead of failing": Proxy guards on the file;
//!   `RemarkFile` has no such members, so the type system refuses them at compile time.
//! - "lets the original verdict stand when a plugin steals the rejection class", "never
//!   echoes a rejection forged from the stolen class in a later run": JS error classes; a
//!   Rust transformer's `Err` text is never reported (covered by
//!   `keeps_a_thrown_error_message_off_the_diagnostic`).
//! - "fails the run when the plugin catches %s" (reading/writing a prohibited key,
//!   defining/deleting a property), "keeps the first verdict when a plugin catches a guard
//!   and then fails": Proxy guards; the sticky `file.fail()` verdict is covered instead.
//! - "fails the attach when the plugin catches a processor access", "fails the run when the
//!   plugin catches a processor access at run time": there is no processor or attacher in
//!   the Rust API.
//! - "rejects output when a hostile then accessor trips a guard late", "rejects output when
//!   a guard fires while the result is being validated": JS accessors/thenables; a
//!   `serde_json::Value` has no getters.
//! - Rejection matrix: "an asynchronous transformer", "a callback-style transformer", "a
//!   parser or compiler plugin that registers processor state", "a plugin that returns no
//!   transformer", "a plugin that returns a promise from its attacher": the Rust transformer
//!   is a synchronous closure with no attacher or processor.
//! - Rejection matrix: "unrepresentable node data", "unrepresentable file data": a function
//!   cannot be put in a `serde_json::Value`; JSON data is always representable.
//! - "stores a whitespace or padded URL ..." : the rendered half (no `<a>`/`<img>` for a
//!   blank URL) is skipped; the canonical URLs are checked.
//! - "renders readable source after a rejected transform": the rendering half is skipped;
//!   the parse keeping the untransformed tree and the diagnostic are checked.
//! - "rejects a plugin value that is not a function": TypeScript/JS typing; a Rust
//!   transformer is a closure by type.
//! - "accepts a conventional Unified plugin at the callsite", "still rejects an actual
//!   promise from a conventional plugin": TypeScript type fixtures and promises.
//! - "a refused promise is never left unhandled" (all six cases): promise mechanics.
//! - "stays out of the Markdown entry point and parser modules", "is published as its own
//!   entry point": JS module and package-manifest layout.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use regex::Regex;
use rustryx_markdown::ast::{Block, ExtensionDisplay, ExtensionNode, Phrasing, Root};
use rustryx_markdown::plugins::{
    DiagnosticCode, ExtensionRenderer, MarkdownPlugin, PluginDefinition, PluginDiagnostic,
    PluginPhase, SyntaxCapability, SyntaxContribution, TokenizeResult, TransformContext,
    TransformResult, create_markdown_plugin, prepare_markdown_plugins,
};
use rustryx_markdown::remark::{REMARK_ORIGIN_KEY, RemarkFile, create_markdown_remark_transform};
use rustryx_markdown::{
    ParseOptions, parse_inline_ast, parse_markdown_ast, parse_outline_from_markdown,
    project_markdown_headings,
};
use serde_json::{Map, Value, json};

type Object = Map<String, Value>;

const PREFIX: &str = "Remark adapter: ";
const UNSUPPORTED: &str = "Remark adapter: the plugin reported the document as unsupported";
const THREW: &str = "Remark adapter: the plugin threw an error";

/// Upstream `visit`: a minimal unist-style walk; children are read after the visitor runs.
fn visit(node: &mut Value, visitor: &mut dyn FnMut(&mut Value)) {
    visitor(node);
    if let Some(Value::Array(children)) = node.get_mut("children") {
        for child in children {
            visit(child, visitor);
        }
    }
}

/// Upstream `mutateFirst`.
fn mutate_first(tree: &mut Value, node_type: &str, mutate: impl FnOnce(&mut Object)) {
    let mut mutate = Some(mutate);
    visit(tree, &mut |node| {
        if node["type"] == node_type
            && let Some(mutate) = mutate.take()
            && let Value::Object(object) = node
        {
            mutate(object);
        }
    });
}

fn children(tree: &mut Value) -> &mut Vec<Value> {
    tree["children"].as_array_mut().expect("a children array")
}

/// Upstream `remarkStrongTerm`: wraps a prose term in `strong`, leaving code, link
/// destinations and other protected values alone.
fn strong_term(term: &'static str) -> impl Fn(Value, &mut RemarkFile) -> Result<Value, String> {
    move |mut tree, file| {
        let mut matches = 0;
        visit(&mut tree, &mut |node| {
            if node["type"] == "strong" {
                return;
            }
            let Some(Value::Array(nodes)) = node.get_mut("children") else {
                return;
            };
            *nodes = std::mem::take(nodes)
                .into_iter()
                .flat_map(|candidate| {
                    let Some(value) = candidate
                        .get("value")
                        .and_then(Value::as_str)
                        .filter(|_| candidate["type"] == "text")
                    else {
                        return vec![candidate];
                    };
                    let parts: Vec<&str> = value.split(term).collect();
                    if parts.len() == 1 {
                        return vec![candidate];
                    }
                    matches += parts.len() - 1;
                    let mut out = Vec::new();
                    for (index, part) in parts.iter().enumerate() {
                        if index > 0 {
                            out.push(json!({"type": "strong", "children": [{"type": "text", "value": term}]}));
                        }
                        if !part.is_empty() {
                            out.push(json!({"type": "text", "value": part}));
                        }
                    }
                    out
                })
                .collect();
        });
        if matches > 0 {
            file.message(format!("wrapped {matches} term(s)"), None);
        }
        Ok(tree)
    }
}

fn plugin_with_transform(
    name: &str,
    transformer: impl Fn(Value, &mut RemarkFile) -> Result<Value, String> + 'static,
) -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        transform: Some(create_markdown_remark_transform(transformer)),
        ..PluginDefinition::new(name)
    })
    .unwrap()
}

fn strong_term_plugin() -> MarkdownPlugin {
    plugin_with_transform("remark-strong-term", strong_term("Astryx"))
}

/// Upstream `renameTermPlugin` (`remarkRenameTerm` from "Astryx" to "Astryx Design").
fn rename_term_plugin() -> MarkdownPlugin {
    plugin_with_transform("remark-rename-term", |mut tree, _| {
        visit(&mut tree, &mut |node| {
            if node["type"] == "text"
                && let Some(value) = node["value"].as_str()
            {
                node["value"] = value.replace("Astryx", "Astryx Design").into();
            }
        });
        Ok(tree)
    })
}

fn remove_first_heading_plugin() -> MarkdownPlugin {
    plugin_with_transform("remark-remove-first-heading", |mut tree, _| {
        let nodes = children(&mut tree);
        if let Some(index) = nodes.iter().position(|node| node["type"] == "heading") {
            nodes.remove(index);
        }
        Ok(tree)
    })
}

/// Upstream `notePlugin`: a `:::note` block extension.
fn note_plugin() -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
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
                    node: ExtensionNode {
                        plugin: "notes".into(),
                        name: "note".into(),
                        data: json!({"body": input.source[input.offset + 7..close].trim()}),
                        display: ExtensionDisplay::Block,
                        source: None,
                        position: None,
                    },
                })
            })],
        }),
        renderers: BTreeMap::from([(
            "note".to_string(),
            ExtensionRenderer::new(|node| Ok(node.data["body"].as_str().unwrap_or("").into())),
        )]),
        ..PluginDefinition::new("notes")
    })
    .unwrap()
}

/// Upstream `runAdapter`'s options.
struct Adapter {
    display: ExtensionDisplay,
    source_ids: Option<BTreeSet<String>>,
    plugins: Vec<MarkdownPlugin>,
}

impl Default for Adapter {
    fn default() -> Self {
        Adapter {
            display: ExtensionDisplay::Block,
            source_ids: None,
            plugins: Vec::new(),
        }
    }
}

fn ids(ids: &[&str]) -> Option<BTreeSet<String>> {
    Some(ids.iter().map(|id| id.to_string()).collect())
}

/// Upstream `runAdapter`: parses `source` (block, or one paragraph of inline content), then
/// runs the adapted transformer over it directly.
fn run_adapter(
    source: &str,
    transformer: impl Fn(Value, &mut RemarkFile) -> Result<Value, String> + 'static,
    options: Adapter,
) -> (Root, TransformResult) {
    let root = match options.display {
        ExtensionDisplay::Inline => Root {
            children: vec![Block::Paragraph(rustryx_markdown::ast::Paragraph {
                children: parse_inline_ast(
                    source,
                    &ParseOptions {
                        source_ids: options.source_ids.clone(),
                        ..ParseOptions::default()
                    },
                ),
                ..Default::default()
            })],
            ..Root::default()
        },
        ExtensionDisplay::Block => parse_markdown_ast(
            source,
            &ParseOptions {
                source_ranges: true,
                source_ids: options.source_ids.clone(),
                plugins: options.plugins,
                ..ParseOptions::default()
            },
        ),
    };
    let transform = create_markdown_remark_transform(transformer);
    let result = transform.run(&root, &TransformContext::new(source, true, options.display));
    (root, result)
}

fn run_block(
    source: &str,
    transformer: impl Fn(Value, &mut RemarkFile) -> Result<Value, String> + 'static,
) -> TransformResult {
    run_adapter(source, transformer, Adapter::default()).1
}

fn reason(message: &str) -> TransformResult {
    Err(format!("{PREFIX}{message}"))
}

const EVERY_NODE_SOURCE: &str = "# Heading with `code` and *emphasis*

Prose with **strong**, ~~struck~~, [a link](/docs), ![alt](/logo.png), a
citation [ref], $x + y$ inline math, and a hard break.  
Second line.

![standalone](/hero.png)

> Quoted prose
>
> - nested item

1) First
2) Second

- [ ] open task
- [x] done task

| Left | Right |
| :--- | ----: |
| a    | b     |

```ts title=\"example.ts\" {1,3}
const value = 1;
```

```
no language, no meta
```

$$
a^2 + b^2
$$

---

:::note
An owned extension node.
:::";

const FENCE_SOURCE: &str =
    "```ts title=\"example.ts\" {1,3}\nconst value = 1;\n```\n\n```\nbare fence\n```";

fn json_of(root: &Root) -> Value {
    serde_json::to_value(root).unwrap()
}

// ---- Remark adapter — supported round trip ----

/// "returns the identical document when every supported node is untouched"
#[test]
fn returns_the_identical_document_when_every_supported_node_is_untouched() {
    let root = parse_markdown_ast(
        EVERY_NODE_SOURCE,
        &ParseOptions {
            math: true,
            source_ranges: true,
            source_ids: ids(&["ref"]),
            plugins: vec![note_plugin()],
            ..ParseOptions::default()
        },
    );
    let transform = create_markdown_remark_transform(|tree, _| Ok(tree));
    let result = transform.run(
        &root,
        &TransformContext::new(EVERY_NODE_SOURCE, true, ExtensionDisplay::Block),
    );
    assert_eq!(result, Ok(None));
}

/// "exposes the supported subset as a mutable mdast tree"
#[test]
fn exposes_the_supported_subset_as_a_mutable_mdast_tree() {
    let seen = Rc::new(RefCell::new(BTreeSet::new()));
    let codes = Rc::new(RefCell::new(Vec::new()));
    let (s, c) = (seen.clone(), codes.clone());
    let result = run_block(EVERY_NODE_SOURCE, move |mut tree, _| {
        visit(&mut tree, &mut |node| {
            s.borrow_mut()
                .insert(node["type"].as_str().unwrap().to_string());
            if node["type"] == "code" {
                c.borrow_mut().push(node.clone());
            }
        });
        Ok(tree)
    });
    assert_eq!(result, Ok(None));
    let expected: BTreeSet<String> = [
        "root",
        "heading",
        "paragraph",
        "text",
        "inlineCode",
        "emphasis",
        "strong",
        "delete",
        "link",
        "image",
        "break",
        "blockquote",
        "list",
        "listItem",
        "table",
        "tableRow",
        "tableCell",
        "code",
        "thematicBreak",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(*seen.borrow(), expected);
    let codes = codes.borrow();
    assert_eq!(codes[0]["type"], "code");
    assert_eq!(codes[0]["lang"], "ts");
    assert_eq!(codes[0]["meta"], "title=\"example.ts\" {1,3}");
    assert_eq!(codes[0]["value"], "const value = 1;");
    assert_eq!(codes[1]["lang"], Value::Null);
    assert_eq!(codes[1]["meta"], Value::Null);
    assert_eq!(codes[1]["value"], "no language, no meta");
}

/// "applies a compatible transform and keeps protected contexts literal"
#[test]
fn applies_a_compatible_transform_and_keeps_protected_contexts_literal() {
    let root = parse_markdown_ast(
        "Astryx ships `Astryx` and [Astryx](/astryx).",
        &ParseOptions {
            plugins: vec![strong_term_plugin()],
            ..ParseOptions::default()
        },
    );
    assert_eq!(
        json_of(&root),
        json!({"type": "root", "children": [{
            "type": "paragraph",
            "children": [
                {"type": "strong", "children": [{"type": "text", "value": "Astryx"}]},
                {"type": "text", "value": " ships "},
                {"type": "inlineCode", "value": "Astryx"},
                {"type": "text", "value": " and "},
                {"type": "link", "url": "/astryx", "children": [
                    {"type": "strong", "children": [{"type": "text", "value": "Astryx"}]},
                ]},
                {"type": "text", "value": "."},
            ],
            "position": {"start": {"offset": 0}, "end": {"offset": 44}},
        }]})
    );
}

/// The parser half of "leaves fenced code copyable and renders adapted output in the
/// browser and on the server".
#[test]
fn leaves_fenced_code_copyable_parser_side() {
    let root = parse_markdown_ast(
        "Astryx docs\n\n```txt\nAstryx stays literal\n```",
        &ParseOptions {
            plugins: vec![strong_term_plugin()],
            ..ParseOptions::default()
        },
    );
    assert_eq!(
        json_of(&root),
        json!({"type": "root", "children": [
            {
                "type": "paragraph",
                "children": [
                    {"type": "strong", "children": [{"type": "text", "value": "Astryx"}]},
                    {"type": "text", "value": " docs"},
                ],
                "position": {"start": {"offset": 0}, "end": {"offset": 11}},
            },
            {
                "type": "code",
                "lang": "txt",
                "value": "Astryx stays literal",
                "position": {"start": {"offset": 13}, "end": {"offset": 44}},
            },
        ]})
    );
}

/// "shares transformed heading identity with Outline"
#[test]
fn shares_transformed_heading_identity_with_outline() {
    let source = "# Astryx release\n\nBody.";
    let plugin = rename_term_plugin();
    let outline = parse_outline_from_markdown(source, std::slice::from_ref(&plugin), true);
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].label, "Astryx Design release");
    assert_eq!(outline[0].level, 1);

    let root = parse_markdown_ast(
        source,
        &ParseOptions {
            plugins: vec![plugin.clone()],
            ..ParseOptions::default()
        },
    );
    let Block::Heading(heading) = &root.children[0] else {
        panic!("a heading first");
    };
    assert_eq!(heading.depth, 1);
    assert!(matches!(
        heading.children.as_slice(),
        [Phrasing::Text(text)] if text.value == "Astryx Design release"
    ));
    let prepared = prepare_markdown_plugins(&[plugin]).unwrap();
    let projection = project_markdown_headings(&root, Some(&prepared));
    assert_eq!(projection.headings[&0].id, outline[0].id);
    assert_eq!(projection.headings[&0].label, "Astryx Design release");
}

/// "lets a transform remove a heading while Markdown and Outline keep surviving identity"
#[test]
fn lets_a_transform_remove_a_heading_while_markdown_and_outline_keep_surviving_identity() {
    let source = "# Removed\n\n## Kept\n\nBody.";
    let plugin = remove_first_heading_plugin();
    let outline = parse_outline_from_markdown(source, std::slice::from_ref(&plugin), true);
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].label, "Kept");
    assert_eq!(outline[0].level, 2);

    let root = parse_markdown_ast(
        source,
        &ParseOptions {
            plugins: vec![plugin.clone()],
            ..ParseOptions::default()
        },
    );
    let headings: Vec<_> = root
        .children
        .iter()
        .filter_map(|block| match block {
            Block::Heading(heading) => Some(heading),
            _ => None,
        })
        .collect();
    assert_eq!(headings.len(), 1);
    assert_eq!(headings[0].depth, 2);
    let prepared = prepare_markdown_plugins(&[plugin]).unwrap();
    let projection = project_markdown_headings(&root, Some(&prepared));
    assert_eq!(projection.headings.len(), 1);
    assert_eq!(projection.headings[&0].label, "Kept");
    assert_eq!(projection.headings[&0].id, outline[0].id);
}

/// "reports exactly one fixed diagnostic that cannot carry plugin text"
#[test]
fn reports_exactly_one_fixed_diagnostic_that_cannot_carry_plugin_text() {
    let result = run_block("Astryx confidential-token", |mut tree, file| {
        file.message("saw confidential-token at line 1", None);
        file.message("saw confidential-token again", None);
        children(&mut tree).push(json!({"type": "html", "value": "<b>x</b>"}));
        Ok(tree)
    });
    assert_eq!(result, reason("raw HTML has no Astryx representation"));
    assert!(!result.unwrap_err().contains("confidential-token"));
}

/// "keeps a plugin-authored failure reason off the diagnostic"
#[test]
fn keeps_a_plugin_authored_failure_reason_off_the_diagnostic() {
    let result = run_block("Astryx confidential-token", |_, file| {
        Err(file.fail("cannot handle \"confidential-token\"", None))
    });
    assert_eq!(result, Err(UNSUPPORTED.to_string()));
}

/// "keeps a thrown error message off the diagnostic"
#[test]
fn keeps_a_thrown_error_message_off_the_diagnostic() {
    let result = run_block("Astryx confidential-token", |_, _| {
        Err("failed parsing \"confidential-token\"".to_string())
    });
    assert_eq!(result, Err(THREW.to_string()));
}

/// "keeps a successful run quiet even when the plugin records messages"
#[test]
fn keeps_a_successful_run_quiet_even_when_the_plugin_records_messages() {
    let result = run_block("Astryx prose", |tree, file| {
        file.message("informational", None);
        Ok(tree)
    });
    assert_eq!(result, Ok(None));
}

/// "makes provenance tamper-proof rather than trusting the plugin" (upstream's one
/// non-writable symbol marker is the one [`REMARK_ORIGIN_KEY`] token; tampering with it is
/// rejected rather than trusted).
#[test]
fn makes_provenance_tamper_proof_rather_than_trusting_the_plugin() {
    let markers = Rc::new(RefCell::new(Vec::new()));
    let seen = markers.clone();
    let result = run_block("Prose.", move |tree, _| {
        let paragraph = tree["children"][0].as_object().unwrap();
        seen.borrow_mut().extend(
            paragraph
                .keys()
                .filter(|key| key.as_str() == REMARK_ORIGIN_KEY)
                .cloned(),
        );
        Ok(tree)
    });
    assert_eq!(result, Ok(None));
    assert_eq!(markers.borrow().len(), 1);

    let tampered = run_block("Prose.", |mut tree, _| {
        tree["children"][0][REMARK_ORIGIN_KEY] = "999999999:0".into();
        Ok(tree)
    });
    assert_eq!(
        tampered,
        reason("a \"paragraph\" node carries forged provenance")
    );
}

/// "gives every invocation a fresh tree and isolated file data"
#[test]
fn gives_every_invocation_a_fresh_tree_and_isolated_file_data() {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let seen = observed.clone();
    let transform = create_markdown_remark_transform(move |mut tree, file| {
        seen.borrow_mut().push(file.data.get("runs").cloned());
        file.data.insert("runs".into(), "first".into());
        children(&mut tree).push(json!({"type": "thematicBreak"}));
        Ok(tree)
    });
    let source = "Body text";
    let root = parse_markdown_ast(source, &ParseOptions::default());
    let context = TransformContext::new(source, true, ExtensionDisplay::Block);

    let first = transform.run(&root, &context).unwrap().unwrap();
    let second = transform.run(&root, &context).unwrap().unwrap();

    assert_eq!(*observed.borrow(), vec![None, None]);
    assert_eq!(root.children.len(), 1);
    assert_eq!(first.children.len(), 2);
    assert_eq!(second, first);
}

// ---- Remark adapter — fenced code metadata ----

/// "round-trips a parsed fence untouched, metadata included"
#[test]
fn round_trips_a_parsed_fence_untouched_metadata_included() {
    assert_eq!(run_block(FENCE_SOURCE, |tree, _| Ok(tree)), Ok(None));
}

/// "keeps the released code projection for a fence it rewrites"
#[test]
fn keeps_the_released_code_projection_for_a_fence_it_rewrites() {
    let plugin = plugin_with_transform("uppercase-code", |mut tree, _| {
        mutate_first(&mut tree, "code", |node| {
            let upper = node["value"].as_str().unwrap().to_uppercase();
            node.insert("value".into(), upper.into());
        });
        Ok(tree)
    });
    let root = parse_markdown_ast(
        FENCE_SOURCE,
        &ParseOptions {
            plugins: vec![plugin],
            ..ParseOptions::default()
        },
    );
    assert_eq!(
        json_of(&root),
        json!({"type": "root", "children": [
            {
                "type": "code",
                "lang": "ts",
                "meta": "title=\"example.ts\" {1,3}",
                "value": "CONST VALUE = 1;",
                "position": {"start": {"offset": 0}, "end": {"offset": 51}},
            },
            {
                "type": "code",
                "lang": null,
                "value": "bare fence",
                "position": {"start": {"offset": 53}, "end": {"offset": 71}},
            },
        ]})
    );
}

/// "carries metadata into and back out of a transform"
#[test]
fn carries_metadata_into_and_back_out_of_a_transform() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let metas = seen.clone();
    let result = run_block(FENCE_SOURCE, move |mut tree, _| {
        visit(&mut tree, &mut |node| {
            if node["type"] == "code" {
                metas.borrow_mut().push(node["meta"].clone());
            }
        });
        mutate_first(&mut tree, "code", |node| {
            node.insert("meta".into(), "title=\"renamed.ts\"".into());
        });
        Ok(tree)
    });
    assert_eq!(
        *seen.borrow(),
        vec![json!("title=\"example.ts\" {1,3}"), Value::Null]
    );
    let root = result.unwrap().expect("a changed document");
    let Block::Code(code) = &root.children[0] else {
        panic!("a code block first");
    };
    assert_eq!(code.lang.as_deref(), Some("ts"));
    assert_eq!(code.meta.as_deref(), Some("title=\"renamed.ts\""));
}

/// "rejects metadata Astryx cannot represent"
#[test]
fn rejects_metadata_astryx_cannot_represent() {
    let result = run_block(FENCE_SOURCE, |mut tree, _| {
        mutate_first(&mut tree, "code", |node| {
            node.insert("meta".into(), json!({"title": "example.ts"}));
        });
        Ok(tree)
    });
    assert_eq!(
        result,
        reason("a code node requires a string or null \"meta\"")
    );
}

// ---- Remark adapter — the constrained file fails closed ----

/// "serves the whole documented surface" (`toString()` is `value()` in Rust).
#[test]
fn serves_the_whole_documented_surface() {
    let observed = Rc::new(RefCell::new(Map::new()));
    let seen = observed.clone();
    let result = run_block("Document source.", move |tree, file| {
        let mut seen = seen.borrow_mut();
        seen.insert("value".into(), file.value().into());
        seen.insert("messages".into(), file.messages().len().into());
        file.data.insert("note".into(), "kept".into());
        seen.insert("data".into(), Value::Object(file.data.clone()));
        file.message("advisory", None);
        seen.insert("afterMessage".into(), file.messages().len().into());
        Ok(tree)
    });
    assert_eq!(
        Value::Object(observed.borrow().clone()),
        json!({
            "value": "Document source.",
            "data": {"note": "kept"},
            "messages": 0,
            "afterMessage": 1,
        })
    );
    assert_eq!(result, Ok(None));
}

// ---- Remark adapter — a plugin cannot talk its way out of a failure ----

/// "fails the run when the plugin swallows its own fail()"
#[test]
fn fails_the_run_when_the_plugin_swallows_its_own_fail() {
    let result = run_block("Prose.", |tree, file| {
        let _ = file.fail("cannot handle this document", None);
        Ok(tree)
    });
    assert_eq!(result, Err(UNSUPPORTED.to_string()));
}

/// "keeps the verdict when the plugin swallows fail() and returns a tree"
#[test]
fn keeps_the_verdict_when_the_plugin_swallows_fail_and_returns_a_tree() {
    let result = run_block("Prose.", |mut tree, file| {
        let _ = file.fail("cannot handle this document", None);
        // A changed, otherwise valid tree does not undo the verdict either.
        children(&mut tree).push(json!({"type": "thematicBreak"}));
        Ok(tree)
    });
    assert_eq!(result, Err(UNSUPPORTED.to_string()));
}

/// "keeps the verdict when the plugin swallows fail() then throws its own error"
#[test]
fn keeps_the_verdict_when_the_plugin_swallows_fail_then_throws_its_own_error() {
    let result = run_block("Prose.", |_, file| {
        let _ = file.fail("cannot handle this document", None);
        Err("a different, friendlier-sounding message".to_string())
    });
    assert_eq!(result, Err(UNSUPPORTED.to_string()));
}

/// "never echoes a rewritten message on a real rejection" (the real rejection here is
/// `fail()`'s returned error, rewritten before it is returned).
#[test]
fn never_echoes_a_rewritten_message_on_a_real_rejection() {
    let result = run_block("Prose.", |_, file| {
        let mut error = file.fail("cannot handle this document", None);
        error.clear();
        error.push_str("everything is fine");
        Err(error)
    });
    assert_eq!(result, Err(UNSUPPORTED.to_string()));
}

/// "ignores a message a plugin marks fatal by hand" (`messages()` is read-only in Rust,
/// so only the hand-marked message half applies).
#[test]
fn ignores_a_message_a_plugin_marks_fatal_by_hand() {
    let result = run_block("Prose.", |tree, file| {
        file.message("advisory", None).fatal = true;
        assert!(file.messages()[0].fatal);
        Ok(tree)
    });
    assert_eq!(result, Ok(None));
}

// ---- Remark adapter — rejection matrix ----

const REJECTION_SOURCE: &str = "# Astryx heading\n\nProse with [a link](/docs).";

/// Upstream's `it.each(rejectionCases)` body: the last valid document stands (an `Err`),
/// with one report naming the adapter's contract, never the document's source.
fn assert_rejected(
    source: Option<&str>,
    options: Adapter,
    transformer: impl Fn(Value, &mut RemarkFile) -> Result<Value, String> + 'static,
    pattern: &str,
) {
    let (_, result) = run_adapter(source.unwrap_or(REJECTION_SOURCE), transformer, options);
    let Err(report) = result else {
        panic!("expected a rejection matching {pattern:?}, got {result:?}");
    };
    assert!(report.starts_with(PREFIX), "{report}");
    assert!(
        Regex::new(pattern).unwrap().is_match(&report),
        "{report:?} does not match {pattern:?}"
    );
    assert!(!report.contains("Astryx heading"), "{report}");
}

fn block_rejected(
    transformer: impl Fn(Value, &mut RemarkFile) -> Result<Value, String> + 'static,
    pattern: &str,
) {
    assert_rejected(None, Adapter::default(), transformer, pattern);
}

fn mutating(
    node_type: &'static str,
    mutate: impl Fn(&mut Object) + 'static,
) -> impl Fn(Value, &mut RemarkFile) -> Result<Value, String> + 'static {
    move |mut tree, _| {
        mutate_first(&mut tree, node_type, &mutate);
        Ok(tree)
    }
}

/// "keeps the last valid document after raw HTML"
#[test]
fn keeps_the_last_valid_document_after_raw_html() {
    block_rejected(
        |mut tree, _| {
            children(&mut tree).push(json!({"type": "html", "value": "<b>x</b>"}));
            Ok(tree)
        },
        "raw HTML",
    );
}

/// "keeps the last valid document after an unsupported node kind"
#[test]
fn keeps_the_last_valid_document_after_an_unsupported_node_kind() {
    block_rejected(
        |mut tree, _| {
            children(&mut tree)
                .push(json!({"type": "footnoteDefinition", "identifier": "a", "children": []}));
            Ok(tree)
        },
        r#""footnoteDefinition" is outside the supported MDAST subset"#,
    );
}

/// "keeps the last valid document after a forged source position"
#[test]
fn keeps_the_last_valid_document_after_a_forged_source_position() {
    block_rejected(
        |mut tree, _| {
            children(&mut tree).push(json!({
                "type": "paragraph",
                "children": [{"type": "text", "value": "forged"}],
                "position": {"start": {"offset": 0}, "end": {"offset": 4}},
            }));
            Ok(tree)
        },
        "cannot carry a source position",
    );
}

/// "keeps the last valid document after a shifted source position"
#[test]
fn keeps_the_last_valid_document_after_a_shifted_source_position() {
    block_rejected(
        mutating("paragraph", |node| {
            node.insert(
                "position".into(),
                json!({"start": {"offset": 999}, "end": {"offset": 1200}}),
            );
        }),
        "position must stay exactly as Core authored it",
    );
}

/// "keeps the last valid document after a shifted line or column on an unchanged offset"
#[test]
fn keeps_the_last_valid_document_after_a_shifted_line_or_column_on_an_unchanged_offset() {
    block_rejected(
        mutating("paragraph", |node| {
            let start = node["position"]["start"]["offset"].clone();
            let end = node["position"]["end"]["offset"].clone();
            node.insert(
                "position".into(),
                json!({
                    "start": {"line": 99, "column": 1, "offset": start},
                    "end": {"line": 99, "column": 9, "offset": end},
                }),
            );
        }),
        "position must stay exactly as Core authored it",
    );
}

/// "keeps the last valid document after a removed source position"
#[test]
fn keeps_the_last_valid_document_after_a_removed_source_position() {
    block_rejected(
        mutating("paragraph", |node| {
            node.remove("position");
        }),
        "position must stay exactly as Core authored it",
    );
}

/// "keeps the last valid document after provenance rebuilt from a copied position"
#[test]
fn keeps_the_last_valid_document_after_provenance_rebuilt_from_a_copied_position() {
    block_rejected(
        |mut tree, _| {
            for node in children(&mut tree) {
                if node["type"] == "paragraph" {
                    *node = json!({
                        "type": "paragraph",
                        "position": node["position"].clone(),
                        "children": [{"type": "text", "value": "rebuilt"}],
                    });
                }
            }
            Ok(tree)
        },
        "cannot carry a source position",
    );
}

/// "keeps the last valid document after a changed source heading depth"
#[test]
fn keeps_the_last_valid_document_after_a_changed_source_heading_depth() {
    block_rejected(
        mutating("heading", |node| {
            node.insert("depth".into(), 4.into());
        }),
        "heading's depth cannot change",
    );
}

/// "keeps the last valid document after a retyped source node"
#[test]
fn keeps_the_last_valid_document_after_a_retyped_source_node() {
    block_rejected(
        mutating("paragraph", |node| {
            node.insert("type".into(), "blockquote".into());
        }),
        "cannot be retyped",
    );
}

/// "keeps the last valid document after a duplicated source node"
#[test]
fn keeps_the_last_valid_document_after_a_duplicated_source_node() {
    block_rejected(
        |mut tree, _| {
            let nodes = children(&mut tree);
            let first = nodes[0].clone();
            nodes.push(first);
            Ok(tree)
        },
        "cannot be duplicated",
    );
}

/// "keeps the last valid document after an authored Astryx extension node"
#[test]
fn keeps_the_last_valid_document_after_an_authored_astryx_extension_node() {
    block_rejected(
        |mut tree, _| {
            children(&mut tree).push(json!({
                "type": "extension",
                "plugin": "notes",
                "name": "note",
                "display": "block",
                "data": {"body": "forged"},
            }));
            Ok(tree)
        },
        r#""extension" node cannot be authored or changed"#,
    );
}

/// "keeps the last valid document after a rewritten Astryx citation"
#[test]
fn keeps_the_last_valid_document_after_a_rewritten_astryx_citation() {
    assert_rejected(
        Some("Prose with a citation [ref]."),
        Adapter {
            source_ids: ids(&["ref"]),
            ..Adapter::default()
        },
        mutating("citation", |node| {
            node.insert("sourceId".into(), "other".into());
        }),
        r#""citation" node cannot be authored or changed"#,
    );
}

/// "keeps the last valid document after an edited Astryx extension node's data"
#[test]
fn keeps_the_last_valid_document_after_an_edited_astryx_extension_nodes_data() {
    assert_rejected(
        Some(":::note\nOwned body.\n:::"),
        Adapter {
            plugins: vec![note_plugin()],
            ..Adapter::default()
        },
        mutating("extension", |node| {
            node.insert("data".into(), json!({"body": "rewritten"}));
        }),
        r#""extension" node cannot be authored or changed"#,
    );
}

/// "keeps the last valid document after an edited Astryx extension node's authored source"
#[test]
fn keeps_the_last_valid_document_after_an_edited_astryx_extension_nodes_authored_source() {
    assert_rejected(
        Some(":::note\nOwned body.\n:::"),
        Adapter {
            plugins: vec![note_plugin()],
            ..Adapter::default()
        },
        mutating("extension", |node| {
            node.insert("source".into(), ":::note\nforged\n:::".into());
        }),
        r#""extension" node cannot be authored or changed"#,
    );
}

/// "keeps the last valid document after a raw-markup data channel"
#[test]
fn keeps_the_last_valid_document_after_a_raw_markup_data_channel() {
    block_rejected(
        mutating("paragraph", |node| {
            node.insert("data".into(), json!({"hName": "section"}));
        }),
        r#"raw-markup data channel "hName""#,
    );
}

/// "keeps the last valid document after non-string fence metadata"
#[test]
fn keeps_the_last_valid_document_after_non_string_fence_metadata() {
    assert_rejected(
        Some("```ts\nconst a = 1;\n```"),
        Adapter::default(),
        mutating("code", |node| {
            node.insert("meta".into(), 42.into());
        }),
        r#"requires a string or null "meta""#,
    );
}

/// "keeps the last valid document after an unrepresentable link title"
#[test]
fn keeps_the_last_valid_document_after_an_unrepresentable_link_title() {
    block_rejected(
        mutating("link", |node| {
            node.insert("title".into(), "Docs".into());
        }),
        r#""title" has no Astryx representation"#,
    );
}

/// "keeps the last valid document after an unsupported field on a supported node"
#[test]
fn keeps_the_last_valid_document_after_an_unsupported_field_on_a_supported_node() {
    block_rejected(
        mutating("paragraph", |node| {
            node.insert("hProperties".into(), json!({"className": "x"}));
        }),
        r#"unsupported field "hProperties""#,
    );
}

/// "keeps the last valid document after a rejected link destination"
#[test]
fn keeps_the_last_valid_document_after_a_rejected_link_destination() {
    block_rejected(
        mutating("link", |node| {
            node.insert("url".into(), "javascript:alert(1)".into());
        }),
        "rejected by the navigation owner",
    );
}

/// "keeps the last valid document after a data: link destination"
#[test]
fn keeps_the_last_valid_document_after_a_data_link_destination() {
    block_rejected(
        mutating("link", |node| {
            node.insert("url".into(), "data:image/png;base64,iVBORw0KGgo=".into());
        }),
        "rejected by the navigation owner",
    );
}

/// "keeps the last valid document after a data: image source"
#[test]
fn keeps_the_last_valid_document_after_a_data_image_source() {
    assert_rejected(
        Some("![alt](/logo.png)"),
        Adapter::default(),
        mutating("image", |node| {
            node.insert("url".into(), "data:image/png;base64,iVBORw0KGgo=".into());
        }),
        "rejected by the resource owner",
    );
}

/// "keeps the last valid document after an image without its text alternative"
#[test]
fn keeps_the_last_valid_document_after_an_image_without_its_text_alternative() {
    assert_rejected(
        Some("![alt](/logo.png)"),
        Adapter::default(),
        mutating("image", |node| {
            node.insert("alt".into(), Value::Null);
        }),
        "requires its text alternative",
    );
}

/// "keeps the last valid document after invalid built-in structure"
#[test]
fn keeps_the_last_valid_document_after_invalid_built_in_structure() {
    block_rejected(
        mutating("paragraph", |node| {
            node["children"]
                .as_array_mut()
                .unwrap()
                .push(json!({"type": "thematicBreak"}));
        }),
        r#"cannot be a child of a "paragraph" node"#,
    );
}

/// "keeps the last valid document after a nested link"
#[test]
fn keeps_the_last_valid_document_after_a_nested_link() {
    block_rejected(
        mutating("link", |node| {
            node.insert(
                "children".into(),
                json!([{"type": "link", "url": "/inner", "title": null, "children": []}]),
            );
        }),
        "nested inside another link",
    );
}

/// "keeps the last valid document after a loose list item"
#[test]
fn keeps_the_last_valid_document_after_a_loose_list_item() {
    assert_rejected(
        Some("- one\n- two"),
        Adapter::default(),
        mutating("listItem", |node| {
            node.insert("spread".into(), true.into());
        }),
        "loose list item",
    );
}

/// "keeps the last valid document after a ragged transformed table"
#[test]
fn keeps_the_last_valid_document_after_a_ragged_transformed_table() {
    assert_rejected(
        Some("| a | b |\n| - | - |\n| c | d |"),
        Adapter::default(),
        mutating("tableRow", |node| {
            node["children"].as_array_mut().unwrap().pop();
        }),
        "ragged rows",
    );
}

/// "keeps the last valid document after block output from an inline entry point"
#[test]
fn keeps_the_last_valid_document_after_block_output_from_an_inline_entry_point() {
    assert_rejected(
        Some("Inline Astryx prose"),
        Adapter {
            display: ExtensionDisplay::Inline,
            ..Adapter::default()
        },
        |mut tree, _| {
            children(&mut tree).push(
                json!({"type": "paragraph", "children": [{"type": "text", "value": "extra"}]}),
            );
            Ok(tree)
        },
        "one paragraph of phrasing content",
    );
}

/// "keeps the last valid document after a transformer that throws"
#[test]
fn keeps_the_last_valid_document_after_a_transformer_that_throws() {
    block_rejected(
        |_, _| Err("boom".to_string()),
        "^Remark adapter: the plugin threw an error$",
    );
}

/// "keeps the last valid document after a transformer that fails the file"
#[test]
fn keeps_the_last_valid_document_after_a_transformer_that_fails_the_file() {
    block_rejected(
        |_, file| Err(file.fail("unsupported document", None)),
        "^Remark adapter: the plugin reported the document as unsupported$",
    );
}

/// "keeps the last valid document after a transformer that returns a non-root node"
#[test]
fn keeps_the_last_valid_document_after_a_transformer_that_returns_a_non_root_node() {
    block_rejected(
        |_, _| Ok(json!({"type": "paragraph", "children": []})),
        "cannot be a child of|must return the document root|unsupported",
    );
}

fn set_url_plugin(node_type: &'static str, url: &'static str) -> MarkdownPlugin {
    plugin_with_transform(
        &format!("remark-{node_type}-url"),
        mutating(node_type, move |node| {
            node.insert("url".into(), url.into());
        }),
    )
}

fn parse_with(source: &str, plugins: Vec<MarkdownPlugin>) -> Root {
    parse_markdown_ast(
        source,
        &ParseOptions {
            plugins,
            ..ParseOptions::default()
        },
    )
}

/// "stores a whitespace or padded URL from a Remark plugin in its normalized form"
/// (canonical side: upstream stores `""` for a blank URL, which renders no link or image,
/// and `/x` for a padded one).
#[test]
fn stores_a_whitespace_or_padded_url_from_a_remark_plugin_in_its_normalized_form() {
    let blank = parse_with("[go](/docs)", vec![set_url_plugin("link", " \t ")]);
    assert_eq!(
        json_of(&blank),
        json!({"type": "root", "children": [{
            "type": "paragraph",
            "children": [{"type": "link", "url": "", "children": [{"type": "text", "value": "go"}]}],
            "position": {"start": {"offset": 0}, "end": {"offset": 11}},
        }]})
    );
    let image = parse_with("![pic](/a.png)", vec![set_url_plugin("image", " ")]);
    assert_eq!(
        json_of(&image),
        json!({"type": "root", "children": [{
            "type": "image",
            "url": "",
            "alt": "pic",
            "position": {"start": {"offset": 0}, "end": {"offset": 14}},
        }]})
    );
    let padded = parse_with("[go](/docs)", vec![set_url_plugin("link", "  /x  ")]);
    assert_eq!(
        json_of(&padded),
        json!({"type": "root", "children": [{
            "type": "paragraph",
            "children": [{"type": "link", "url": "/x", "children": [{"type": "text", "value": "go"}]}],
            "position": {"start": {"offset": 0}, "end": {"offset": 11}},
        }]})
    );
}

/// The parser half of "renders readable source after a rejected transform": the document
/// stays as parsed and the failure is reported (upstream: `console.warn` is called).
#[test]
fn renders_readable_source_after_a_rejected_transform() {
    let broken = plugin_with_transform("broken-remark", |mut tree, _| {
        children(&mut tree).push(json!({"type": "html", "value": "<script>x</script>"}));
        Ok(tree)
    });
    let source = "# Still readable\n\nProse survives.";
    let diagnostics = Rc::new(RefCell::new(Vec::<PluginDiagnostic>::new()));
    let sink = diagnostics.clone();
    let root = parse_markdown_ast(
        source,
        &ParseOptions {
            plugins: vec![broken],
            on_plugin_diagnostic: Some(Rc::new(move |diagnostic: &PluginDiagnostic| {
                sink.borrow_mut().push(diagnostic.clone())
            })),
            ..ParseOptions::default()
        },
    );
    assert_eq!(root, parse_markdown_ast(source, &ParseOptions::default()));
    let diagnostics = diagnostics.borrow();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].plugin, "broken-remark");
    assert_eq!(diagnostics[0].phase, PluginPhase::Transform);
    assert_eq!(diagnostics[0].code, DiagnosticCode::TransformFailed);
}

/// "keeps later plugins running after one adapted plugin fails"
#[test]
fn keeps_later_plugins_running_after_one_adapted_plugin_fails() {
    let failing = plugin_with_transform("failing-remark", |_, _| Err("boom".to_string()));
    let root = parse_with("Astryx ships", vec![failing, strong_term_plugin()]);
    assert_eq!(
        json_of(&root),
        json!({"type": "root", "children": [{
            "type": "paragraph",
            "children": [
                {"type": "strong", "children": [{"type": "text", "value": "Astryx"}]},
                {"type": "text", "value": " ships"},
            ],
            "position": {"start": {"offset": 0}, "end": {"offset": 12}},
        }]})
    );
}

/// "rejects provenance smuggled in from another document"
#[test]
fn rejects_provenance_smuggled_in_from_another_document() {
    let stashed = Rc::new(RefCell::new(None::<Value>));
    let stash = stashed.clone();
    let transform = create_markdown_remark_transform(move |mut tree, _| {
        let mut stash = stash.borrow_mut();
        match stash.as_ref() {
            None => *stash = Some(tree["children"][0].clone()),
            Some(node) => children(&mut tree).push(node.clone()),
        }
        Ok(tree)
    });
    let run = |source: &str| {
        let root = parse_markdown_ast(source, &ParseOptions::default());
        transform.run(
            &root,
            &TransformContext::new(source, true, ExtensionDisplay::Block),
        )
    };

    assert_eq!(run("First document."), Ok(None));
    assert_eq!(
        run("Second document."),
        reason("a \"paragraph\" node carries forged provenance")
    );
}
