//! Upstream's plugin system over a corpus (`spec/markdown/plugin-fixtures.json`, written by
//! `tools/export/export-markdown.mjs`): every first-party plugin and the exporter's test
//! plugins, defined again here, must give upstream's canonical tree (block and inline),
//! Outline, heading projection and fence proposals, and report the plugin failures
//! upstream reported (by plugin and phase).

mod plugin_sets;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use astryx_markdown::ast::{Block, Root};
use astryx_markdown::plugins::{
    DiagnosticCode, MarkdownPlugin, PluginDiagnostic, PluginPhase, prepare_markdown_plugins,
};
use astryx_markdown::{
    ParseOptions, parse_inline_ast, parse_markdown_ast, parse_outline_from_markdown,
    project_markdown_headings,
};
use plugin_sets::plugin_sets;
use serde_json::{Value, json};

const FIXTURES: &str = include_str!("../../../spec/markdown/plugin-fixtures.json");

/// How many mismatches the failure message shows in full.
const SHOWN: usize = 6;

/// Options for `plugins` that record every diagnostic.
fn options(
    plugins: &[MarkdownPlugin],
    is_final: bool,
) -> (ParseOptions, Rc<RefCell<Vec<PluginDiagnostic>>>) {
    let diagnostics = Rc::new(RefCell::new(Vec::new()));
    let sink = diagnostics.clone();
    (
        ParseOptions {
            plugins: plugins.to_vec(),
            is_final,
            on_plugin_diagnostic: Some(Rc::new(move |diagnostic: &PluginDiagnostic| {
                sink.borrow_mut().push(diagnostic.clone())
            })),
            ..ParseOptions::default()
        },
        diagnostics,
    )
}

/// The failures as upstream records them: `[plugin, phase]` pairs, sorted, once each.
fn failures(diagnostics: &[PluginDiagnostic]) -> Value {
    let pairs: BTreeSet<(String, &str)> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code != DiagnosticCode::Suppressed)
        .map(|diagnostic| (diagnostic.plugin.clone(), diagnostic.phase.as_str()))
        .collect();
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.phase != PluginPhase::Configuration)
    );
    pairs
        .into_iter()
        .map(|(plugin, phase)| json!([plugin, phase]))
        .collect()
}

fn proposals(root: &Root) -> Vec<Value> {
    fn visit(blocks: &[Block], out: &mut Vec<Value>) {
        for block in blocks {
            match block {
                Block::Code(code) => out.push(code.fence_proposal().map_or(Value::Null, |node| {
                    serde_json::to_value(Block::Extension(node.clone())).unwrap()
                })),
                Block::Blockquote(quote) => visit(&quote.children, out),
                Block::List(list) => list
                    .children
                    .iter()
                    .for_each(|item| visit(&item.children, out)),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    visit(&root.children, &mut out);
    out
}

fn heading_count(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .map(|block| match block {
            Block::Heading(_) => 1,
            Block::Blockquote(quote) => heading_count(&quote.children),
            Block::List(list) => list
                .children
                .iter()
                .map(|item| heading_count(&item.children))
                .sum(),
            _ => 0,
        })
        .sum()
}

#[test]
fn plugin_parses_match_upstream() {
    let fixtures: Value = serde_json::from_str(FIXTURES).expect("the fixtures are JSON");
    let sets = plugin_sets();
    let mut total = 0;
    let mut mismatches = Vec::new();
    let mut check = |what: &str, case: &Value, expected: &Value, actual: Value| {
        total += 1;
        if *expected != actual {
            mismatches.push(format!(
                "{what} [{} final={}] input {:?}\n  expected: {expected}\n  actual:   {actual}",
                case["set"],
                case["isFinal"],
                case["input"].as_str().unwrap_or_default()
            ));
        }
    };
    for case in fixtures["cases"].as_array().expect("cases are an array") {
        let set = case["set"].as_str().expect("a set name");
        let plugins = sets
            .get(set)
            .unwrap_or_else(|| panic!("unknown plugin set {set}"));
        let input = case["input"].as_str().expect("an input");
        let is_final = case["isFinal"].as_bool().expect("finality");

        let (block_options, block_diagnostics) = options(plugins, is_final);
        let root = parse_markdown_ast(input, &block_options);
        check(
            "block",
            case,
            &case["block"],
            serde_json::to_value(&root).unwrap(),
        );
        check(
            "block failures",
            case,
            &case["failures"],
            failures(&block_diagnostics.borrow()),
        );
        let found = proposals(&root);
        match case.get("proposals") {
            Some(expected) => check("proposals", case, expected, Value::Array(found)),
            None => check(
                "proposals",
                case,
                &Value::Bool(true),
                Value::Bool(found.iter().all(Value::is_null)),
            ),
        }
        if let Some(expected) = case.get("inline") {
            let (inline_options, inline_diagnostics) = options(plugins, true);
            let nodes = parse_inline_ast(input, &inline_options);
            check(
                "inline",
                case,
                expected,
                serde_json::to_value(&nodes).unwrap(),
            );
            check(
                "inline failures",
                case,
                &case["inlineFailures"],
                failures(&inline_diagnostics.borrow()),
            );
        }
        if let Some(expected) = case.get("outline") {
            let outline = parse_outline_from_markdown(input, plugins, is_final);
            check(
                "outline",
                case,
                expected,
                serde_json::to_value(&outline).unwrap(),
            );
        }
        if let Some(expected) = case.get("headings") {
            let prepared = prepare_markdown_plugins(plugins).ok();
            let projection = project_markdown_headings(&root, prepared.as_ref());
            let headings: Vec<Value> = (0..heading_count(&root.children))
                .map(|index| {
                    projection.heading(index).map_or(Value::Null, |heading| {
                        json!({ "id": heading.id, "label": heading.label, "permalink": heading.permalink_url })
                    })
                })
                .collect();
            check("headings", case, expected, Value::Array(headings));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {total} plugin fixture checks differ from upstream; the first {}:\n\n{}",
        mismatches.len(),
        mismatches.len().min(SHOWN),
        mismatches[..mismatches.len().min(SHOWN)].join("\n\n")
    );
    eprintln!("{total} plugin fixture checks match upstream");
}
