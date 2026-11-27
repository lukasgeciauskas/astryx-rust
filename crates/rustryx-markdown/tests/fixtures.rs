//! Upstream's own parser output (`spec/markdown/parser-fixtures.json`, written by
//! `tools/export/export-markdown.mjs`): every input, under every option variant, must
//! parse to exactly upstream's JSON, and every heading text must slug as upstream's does.

use std::collections::BTreeSet;

use rustryx_markdown::{ParseOptions, parse_inline_ast, parse_markdown_ast, slugify};
use serde_json::Value;

const FIXTURES: &str = include_str!("../../../spec/markdown/parser-fixtures.json");

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
        other => panic!("unknown fixture variant {other}"),
    }
}

/// Runs every case of `section` through `parse`, returning the count and the mismatches.
fn check(
    fixtures: &Value,
    section: &str,
    parse: impl Fn(&str, &ParseOptions) -> Value,
) -> (usize, Vec<String>) {
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
    (total, failures)
}

fn report(what: &str, total: usize, failures: &[String]) {
    assert!(
        failures.is_empty(),
        "{} of {total} {what} fixtures differ from upstream; the first {}:\n\n{}",
        failures.len(),
        failures.len().min(SHOWN),
        failures[..failures.len().min(SHOWN)].join("\n\n")
    );
}

fn fixtures() -> Value {
    serde_json::from_str(FIXTURES).expect("the fixtures are JSON")
}

#[test]
fn block_parses_match_upstream() {
    let (total, failures) = check(&fixtures(), "block", |input, options| {
        serde_json::to_value(parse_markdown_ast(input, options)).expect("a tree serializes")
    });
    report("block", total, &failures);
}

#[test]
fn inline_parses_match_upstream() {
    let (total, failures) = check(&fixtures(), "inline", |input, options| {
        serde_json::to_value(parse_inline_ast(input, options)).expect("nodes serialize")
    });
    report("inline", total, &failures);
}

#[test]
fn slugs_match_upstream() {
    let fixtures = fixtures();
    let slugs = fixtures["slugs"].as_array().expect("slugs are an array");
    let failures: Vec<String> = slugs
        .iter()
        .filter_map(|pair| {
            let (text, expected) = (pair[0].as_str()?, pair[1].as_str()?);
            let actual = slugify(text);
            (actual != expected)
                .then(|| format!("slug of {text:?}: expected {expected:?}, actual {actual:?}"))
        })
        .collect();
    report("slug", slugs.len(), &failures);
}
