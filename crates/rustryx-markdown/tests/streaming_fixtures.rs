//! Upstream's incremental parser and streaming trims over a corpus
//! (`spec/markdown/streaming-fixtures.json`, written by `tools/export/export-markdown.mjs`):
//! each case streams an input through one state, a snapshot at every chunk boundary with
//! `is_final: false` and then the whole input with `is_final: true`, and every snapshot
//! must give upstream's JSON. Each streamed step is checked by the 64-bit FNV-1a hash of
//! its canonical JSON (keys sorted, compact), the last streamed step and the final result
//! whole. A trimmed case streams `trim_streaming_artifacts`' output, as Markdown does;
//! a legacy case streams the released shape with `source_ranges`. Every trim of every
//! boundary must match too.

mod plugin_sets;

use std::collections::{BTreeSet, HashMap};

use plugin_sets::plugin_sets;
use rustryx_markdown::plugins::MarkdownPlugin;
use rustryx_markdown::{
    IncrementalState, ParseOptions, create_incremental_state, parse_markdown_ast_incremental,
    parse_markdown_incremental, trim_streaming_artifacts,
};
use serde_json::Value;

const FIXTURES: &str = include_str!("../../../spec/markdown/streaming-fixtures.json");

/// How many mismatches the failure message shows in full.
const SHOWN: usize = 6;

/// The exporter's `canonicalJson`: keys sorted, compact.
fn canonical_json(value: &Value, out: &mut String) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                canonical_json(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap());
                out.push(':');
                canonical_json(&map[key], out);
            }
            out.push('}');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap()),
    }
}

/// The exporter's `fnv1a`: 64-bit FNV-1a of the UTF-8 bytes, as 16 hex digits.
fn fnv1a(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn hash_of(value: &Value) -> String {
    let mut text = String::new();
    canonical_json(value, &mut text);
    fnv1a(&text)
}

/// The options a case's variant names: the parser-fixture variants, or a plugin set.
fn variant(name: &str, sets: &HashMap<&'static str, Vec<MarkdownPlugin>>) -> ParseOptions {
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
        set => ParseOptions {
            plugins: sets
                .get(set)
                .unwrap_or_else(|| panic!("unknown variant {set}"))
                .clone(),
            ..ParseOptions::default()
        },
    }
}

/// One snapshot through the case's entry point, as JSON.
fn parse(text: &str, state: &mut IncrementalState, options: &ParseOptions, legacy: bool) -> Value {
    if legacy {
        serde_json::to_value(parse_markdown_incremental(text, state, options)).unwrap()
    } else {
        serde_json::to_value(parse_markdown_ast_incremental(text, state, options)).unwrap()
    }
}

fn report(what: &str, total: usize, failures: &[String]) {
    assert!(
        failures.is_empty(),
        "{} of {total} {what} differ from upstream; the first {}:\n\n{}",
        failures.len(),
        failures.len().min(SHOWN),
        failures[..failures.len().min(SHOWN)].join("\n\n")
    );
    eprintln!("{total} {what} match upstream");
}

fn fixtures() -> Value {
    serde_json::from_str(FIXTURES).expect("the fixtures are JSON")
}

#[test]
fn streamed_parses_match_upstream() {
    let fixtures = fixtures();
    let sets = plugin_sets();
    let mut total = 0;
    let mut hashed = 0;
    let mut failures = Vec::new();
    for case in fixtures["cases"].as_array().expect("cases are an array") {
        let input = case["input"].as_str().expect("an input");
        let name = case["variant"].as_str().expect("a variant");
        let legacy = case["legacy"].as_bool().unwrap_or(false);
        let trimmed = case["trimmed"].as_bool().unwrap_or(false);
        let mut options = variant(name, &sets);
        options.source_ranges = legacy;
        let prepare = |text: &str| {
            if trimmed {
                trim_streaming_artifacts(text, options.math)
            } else {
                text.to_string()
            }
        };
        let label = format!(
            "[{name}{}{}] input {input:?}",
            if legacy { " legacy" } else { "" },
            if trimmed { " trimmed" } else { "" }
        );
        let mut state = create_incremental_state();
        let ends = case["ends"].as_array().expect("ends");
        let hashes = case["hashes"].as_array().expect("hashes");
        options.is_final = false;
        let mut first_failure = None;
        for (step, (end, hash)) in ends.iter().zip(hashes).enumerate() {
            let end = usize::try_from(end.as_u64().expect("an end")).unwrap();
            let text = prepare(&input[..end]);
            let actual = parse(&text, &mut state, &options, legacy);
            total += 1;
            if hash.as_str() == Some(hash_of(&actual).as_str()) {
                hashed += 1;
            } else if first_failure.is_none() {
                first_failure = Some(format!(
                    "{label}, step {step} (end {end}, text {text:?}): hash differs\n  actual:   {actual}"
                ));
            }
            if step + 1 == ends.len() && actual != case["last"] {
                failures.push(format!(
                    "{label}, last streamed step (end {end}, text {text:?})\n  expected: {}\n  actual:   {actual}",
                    case["last"]
                ));
            }
        }
        failures.extend(first_failure);
        options.is_final = true;
        let actual = parse(&prepare(input), &mut state, &options, legacy);
        total += 1;
        if actual != case["final"] {
            failures.push(format!(
                "{label}, final\n  expected: {}\n  actual:   {actual}",
                case["final"]
            ));
        }
        if case["settledText"].as_str() != Some(state.settled_text()) {
            failures.push(format!(
                "{label}, settled text\n  expected: {}\n  actual:   {:?}",
                case["settledText"],
                state.settled_text()
            ));
        }
    }
    report("streamed parses", total, &failures);
    eprintln!("{hashed} streamed steps matched upstream's hash");
}

#[test]
fn streaming_trims_match_upstream() {
    let fixtures = fixtures();
    let mut total = 0;
    let mut failures = Vec::new();
    for case in fixtures["trims"].as_array().expect("trims are an array") {
        let input = case["input"].as_str().expect("an input");
        let ends = case["ends"].as_array().expect("ends");
        for (key, math) in [("plain", false), ("math", true)] {
            for (end, expected) in ends.iter().zip(case[key].as_array().expect("trims")) {
                let end = usize::try_from(end.as_u64().expect("an end")).unwrap();
                let prefix = &input[..end];
                let keep = usize::try_from(expected[0].as_u64().expect("a kept length")).unwrap();
                let expected = format!(
                    "{}{}",
                    &prefix[..keep],
                    expected[1].as_str().expect("a tail")
                );
                let actual = trim_streaming_artifacts(prefix, math);
                total += 1;
                if actual != expected {
                    failures.push(format!(
                        "trim (math {math}) of {prefix:?}\n  expected: {expected:?}\n  actual:   {actual:?}"
                    ));
                }
            }
        }
    }
    report("streaming trims", total, &failures);
}
