//! Upstream's released projection (`spec/markdown/legacy-fixtures.json`, written by
//! `tools/export/export-markdown.mjs`): `parse_markdown` and `parse_inline` must give
//! exactly upstream's `parseMarkdown` and `parseInline` JSON (the `BlockNode` and
//! `InlineNode` shapes) for every parser-fixture input, under every option variant, and
//! with `source_ranges`, upstream's `range` on every top-level block.

use std::collections::BTreeSet;

use rustryx_markdown::{ParseOptions, parse_inline, parse_markdown};
use serde_json::Value;

const FIXTURES: &str = include_str!("../../../spec/markdown/legacy-fixtures.json");

/// How many mismatches the failure message shows in full.
const SHOWN: usize = 8;

/// The option variants the fixtures were written under.
fn variant(name: &str) -> ParseOptions {
    match name {
        "default" => ParseOptions::default(),
        "math" => ParseOptions {
            math: true,
            ..ParseOptions::default()
        },
        "autolink" => ParseOptions {
            autolink_gfm: true,
            ..ParseOptions::default()
        },
        "citations" => ParseOptions {
            source_ids: Some(BTreeSet::from(["1", "2", "src", "a"].map(String::from))),
            ..ParseOptions::default()
        },
        "ranges" => ParseOptions {
            source_ranges: true,
            ..ParseOptions::default()
        },
        "math+ranges" => ParseOptions {
            math: true,
            source_ranges: true,
            ..ParseOptions::default()
        },
        other => panic!("unknown fixture variant {other}"),
    }
}

fn check(section: &str, parse: impl Fn(&str, &ParseOptions) -> Value) {
    let fixtures: Value = serde_json::from_str(FIXTURES).expect("the fixtures are JSON");
    let mut total = 0;
    let mut failures = Vec::new();
    for case in fixtures[section]
        .as_array()
        .expect("a fixture section is an array")
    {
        let input = case["input"].as_str().expect("a fixture input is a string");
        for (name, expected) in case["results"]
            .as_object()
            .expect("fixture results are an object")
        {
            total += 1;
            let actual = parse(input, &variant(name));
            if &actual != expected {
                failures.push(format!(
                    "{section} [{name}] input {input:?}\n  expected: {expected}\n  actual:   {actual}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {total} legacy {section} fixtures differ from upstream; the first {}:\n\n{}",
        failures.len(),
        failures.len().min(SHOWN),
        failures[..failures.len().min(SHOWN)].join("\n\n")
    );
    eprintln!("{total} legacy {section} fixtures match upstream");
}

#[test]
fn legacy_block_parses_match_upstream() {
    check("block", |input, options| {
        serde_json::to_value(parse_markdown(input, options)).expect("blocks serialize")
    });
}

#[test]
fn legacy_inline_parses_match_upstream() {
    check("inline", |input, options| {
        serde_json::to_value(parse_inline(input, options)).expect("nodes serialize")
    });
}
