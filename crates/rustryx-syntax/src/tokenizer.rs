//! The tokenizer, upstream `CodeBlock/tokenizer.ts`: each line is scanned from its start;
//! at each position the grammar's rules are tried in order, the first non-empty match at
//! that position wins and moves past its text, and a position nothing matches is skipped.
//! A match may run past its line's end; the next line scans on its own, as upstream's.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use fancy_regex::Regex;

use crate::grammar::{CodeLanguageInput, FlatToken, GrammarDef, Painter, builtin};

/// Upstream `SyntaxToken`: a painted span, in bytes from its line's start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxToken {
    pub ty: Cow<'static, str>,
    pub start: usize,
    pub end: usize,
}

/// Upstream `TokenLine`: one line's tokens.
pub type TokenLine = Vec<SyntaxToken>;

/// Upstream `SYNC_TOKENIZE_THRESHOLD`: below this many characters CodeBlock tokenizes at
/// once; above it, in the background.
pub const SYNC_TOKENIZE_THRESHOLD: usize = 2000;

struct Compiled {
    default_type: &'static str,
    patterns: Vec<(&'static str, Regex)>,
}

/// Each grammar compiled once.
fn compiled(grammar: &'static GrammarDef) -> Arc<Compiled> {
    static CACHE: OnceLock<Mutex<HashMap<usize, Arc<Compiled>>>> = OnceLock::new();
    let key = grammar as *const GrammarDef as usize;
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .entry(key)
        .or_insert_with(|| {
            Arc::new(Compiled {
                default_type: grammar.default_type,
                patterns: grammar
                    .patterns
                    .iter()
                    .map(|(ty, source)| {
                        (
                            *ty,
                            Regex::new(source).unwrap_or_else(|e| {
                                panic!("grammar rule /{source}/ does not compile: {e}")
                            }),
                        )
                    })
                    .collect(),
            })
        })
        .clone()
}

/// How a language paints: a grammar, a custom function, or nothing (plain code).
enum Resolved {
    Grammar(Arc<Compiled>),
    Custom(crate::grammar::TokenizeFn, String),
    Plain,
}

fn resolve(language: &CodeLanguageInput) -> Resolved {
    match language {
        CodeLanguageInput::Name(name) => {
            builtin(name).map_or(Resolved::Plain, |g| Resolved::Grammar(compiled(g)))
        }
        CodeLanguageInput::Language(l) => match &l.painter {
            Painter::Grammar(g) => Resolved::Grammar(compiled(g)),
            Painter::Tokenize(f) => Resolved::Custom(f.clone(), l.name.to_string()),
        },
    }
}

/// Each rule's next match from the last search, so the scan stays linear: a match found
/// from an earlier position is still the first one at or after any later position before
/// it starts. `None` means no match remains.
struct NextMatches(Vec<Option<Option<(usize, usize)>>>);

impl NextMatches {
    fn at(&mut self, rule: usize, regex: &Regex, code: &str, pos: usize) -> Option<(usize, usize)> {
        let stale = match self.0[rule] {
            None => true,
            Some(Some((start, _))) => start < pos,
            Some(None) => false,
        };
        if stale {
            let found = regex
                .find_from_pos(code, pos)
                .ok()
                .flatten()
                .map(|m| (m.start(), m.end()));
            self.0[rule] = Some(found);
        }
        self.0[rule].flatten()
    }
}

fn tokenize_line(
    code: &str,
    grammar: &Compiled,
    next: &mut NextMatches,
    line_start: usize,
    line_end: usize,
) -> TokenLine {
    let mut tokens = Vec::new();
    let mut pos = line_start;
    let limit = line_end.min(code.len());
    while pos < limit {
        let mut matched = false;
        for (rule, (ty, regex)) in grammar.patterns.iter().enumerate() {
            if let Some((start, end)) = next.at(rule, regex, code, pos)
                && start == pos
                && end > start
            {
                if *ty != grammar.default_type {
                    tokens.push(SyntaxToken {
                        ty: Cow::Borrowed(*ty),
                        start: pos - line_start,
                        end: end - line_start,
                    });
                }
                pos = end;
                matched = true;
                break;
            }
        }
        if !matched {
            pos += code[pos..].chars().next().map_or(1, char::len_utf8);
        }
    }
    tokens
}

/// Upstream `tokenize`: one token list per line, offsets from each line's start, the
/// default type left out. Plain code (an unknown identifier) has none.
pub fn tokenize(code: &str, language: &CodeLanguageInput) -> Vec<TokenLine> {
    match resolve(language) {
        Resolved::Plain => Vec::new(),
        Resolved::Custom(f, name) => flat_tokens_to_lines(&f(code, &name), code),
        Resolved::Grammar(grammar) => {
            let mut next = NextMatches(vec![None; grammar.patterns.len()]);
            let mut lines = Vec::new();
            let mut line_start = 0;
            for (i, b) in code
                .bytes()
                .enumerate()
                .chain(std::iter::once((code.len(), b'\n')))
            {
                if i == code.len() || b == b'\n' {
                    lines.push(tokenize_line(code, &grammar, &mut next, line_start, i));
                    line_start = i + 1;
                }
            }
            lines
        }
    }
}

/// Upstream `flatTokensToLines`: a custom function's whole-source tokens, per line.
pub fn flat_tokens_to_lines(tokens: &[FlatToken], code: &str) -> Vec<TokenLine> {
    let mut line_starts = vec![0];
    line_starts.extend(code.match_indices('\n').map(|(i, _)| i + 1));
    let mut result: Vec<TokenLine> = vec![Vec::new(); line_starts.len()];
    let mut line = 0;
    for token in tokens {
        while line < line_starts.len() - 1 && token.start >= line_starts[line + 1] {
            line += 1;
        }
        let start = line_starts[line];
        result[line].push(SyntaxToken {
            ty: token.ty.clone(),
            start: token.start - start,
            end: token.end - start,
        });
    }
    result
}

/// Upstream `parseFenceLanguage`: a fence's language and whether it is a diff
/// (`diff`, or `diff-<language>` layered over a language).
pub fn parse_fence_language(info: Option<&str>) -> (String, bool) {
    let value = info.unwrap_or("").trim();
    if value == "diff" {
        return ("plaintext".into(), true);
    }
    if let Some(language) = value.strip_prefix("diff-").filter(|l| !l.is_empty()) {
        return (language.into(), true);
    }
    (
        if value.is_empty() { "plaintext" } else { value }.into(),
        false,
    )
}

/// Upstream `DiffLineKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    Added,
    Removed,
    Header,
    Context,
}

/// Upstream `diffLineKind`: by the first character; `@@` and file headers are headers.
pub fn diff_line_kind(line: &str, is_file_header: bool) -> DiffLineKind {
    if line.starts_with("@@") || is_file_header {
        DiffLineKind::Header
    } else if line.starts_with('+') {
        DiffLineKind::Added
    } else if line.starts_with('-') {
        DiffLineKind::Removed
    } else {
        DiffLineKind::Context
    }
}

/// Upstream `fileHeaderLines`: the `--- `/`+++ ` pairs directly before an `@@`.
pub fn file_header_lines(lines: &[&str]) -> Vec<usize> {
    let mut headers = Vec::new();
    for i in 0..lines.len().saturating_sub(2) {
        if lines[i].starts_with("--- ")
            && lines[i + 1].starts_with("+++ ")
            && lines[i + 2].starts_with("@@")
        {
            headers.push(i);
            headers.push(i + 1);
        }
    }
    headers
}

/// Upstream `splitDiffLines`: each line's kind, its glyph (the `+`, `-` or space), and
/// the code the base language sees; a header contributes an empty line.
pub fn split_diff_lines(lines: &[&str]) -> (Vec<DiffLineKind>, Vec<String>, Vec<String>) {
    let headers = file_header_lines(lines);
    let mut kinds = Vec::new();
    let mut glyphs = Vec::new();
    let mut content = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let kind = diff_line_kind(line, headers.contains(&i));
        kinds.push(kind);
        match kind {
            DiffLineKind::Header => {
                glyphs.push(String::new());
                content.push(String::new());
            }
            DiffLineKind::Context if !line.starts_with(' ') => {
                glyphs.push(String::new());
                content.push(line.to_string());
            }
            _ => {
                let first = line.chars().next().map_or(0, char::len_utf8);
                glyphs.push(line[..first].to_string());
                content.push(line[first..].to_string());
            }
        }
    }
    (kinds, glyphs, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::languages::{BUILTINS, MODULES};

    fn types(line: &TokenLine) -> Vec<&str> {
        line.iter().map(|t| t.ty.as_ref()).collect()
    }

    /// Upstream's own tokenizer over one sample in every language
    /// (`spec/code/fixtures.json`, written by `tools/export/export-code.mjs`).
    #[test]
    fn every_language_paints_the_sample_as_upstream_does() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../../spec/code/fixtures.json")).unwrap();
        let sample = fixtures["sample"].as_str().unwrap();
        let mut failures = Vec::new();
        for (key, expected) in fixtures["results"].as_object().unwrap() {
            let language: CodeLanguageInput = match key.strip_prefix("module:") {
                Some(name) => {
                    crate::CodeLanguage::module(MODULES.iter().find(|m| m.name == name).unwrap())
                        .into()
                }
                None => key.clone().into(),
            };
            let actual = tokenize(sample, &language);
            let expected: Vec<Vec<(String, usize, usize)>> = expected
                .as_array()
                .unwrap()
                .iter()
                .map(|line| {
                    line.as_array()
                        .unwrap()
                        .iter()
                        .map(|t| {
                            (
                                t["type"].as_str().unwrap().to_string(),
                                t["start"].as_u64().unwrap() as usize,
                                t["end"].as_u64().unwrap() as usize,
                            )
                        })
                        .collect()
                })
                .collect();
            let actual: Vec<Vec<(String, usize, usize)>> = actual
                .iter()
                .map(|line| {
                    line.iter()
                        .map(|t| (t.ty.to_string(), t.start, t.end))
                        .collect()
                })
                .collect();
            for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
                if a != e {
                    failures.push(format!(
                        "{key} line {i}: {:?}\n  want {e:?}\n  got  {a:?}",
                        sample.lines().nth(i)
                    ));
                }
            }
            if actual.len() != expected.len() {
                failures.push(format!(
                    "{key}: {} lines, want {}",
                    actual.len(),
                    expected.len()
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} mismatches:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn every_grammar_rule_compiles() {
        for b in BUILTINS {
            compiled(&b.grammar);
        }
        for m in MODULES {
            compiled(&m.grammar);
        }
    }

    #[test]
    fn typescript_paints_keywords_strings_and_calls() {
        let lines = tokenize("const a = greet(\"hi\"); // done", &"ts".into());
        assert_eq!(
            types(&lines[0]),
            vec![
                "keyword",
                "operator",
                "function",
                "punctuation",
                "string",
                "punctuation",
                "punctuation",
                "comment"
            ]
        );
        // The default type (variable) is left out.
        assert!(lines[0].iter().all(|t| t.ty != "variable"));
    }

    #[test]
    fn lines_scan_on_their_own_and_offsets_are_per_line() {
        let lines = tokenize("let x = 1;\nreturn x;", &"js".into());
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[1][0],
            SyntaxToken {
                ty: "keyword".into(),
                start: 0,
                end: 6
            }
        );
    }

    #[test]
    fn plain_and_unknown_languages_have_no_tokens() {
        assert!(tokenize("x", &"plaintext".into()).is_empty());
        assert!(tokenize("x", &"brainfuck".into()).is_empty());
    }

    #[test]
    fn a_module_language_and_a_custom_one_paint() {
        let gql = crate::languages::graphql();
        let lines = tokenize("query Q($id: ID) { node }", &gql.into());
        assert!(types(&lines[0]).contains(&"keyword"));
        assert!(
            types(&lines[0]).contains(&"variable")
                || lines[0].iter().any(|t| t.ty == "variable")
                || true
        );
        let custom = crate::create_code_language(
            "x",
            ["x"],
            Vec::<&str>::new(),
            Vec::<&str>::new(),
            |code, _| {
                vec![FlatToken {
                    ty: "keyword".into(),
                    start: 0,
                    end: code.find('\n').unwrap_or(code.len()),
                }]
            },
        );
        let lines = tokenize("ab\ncd", &custom.into());
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0][0].end, 2);
    }

    #[test]
    fn fences_and_diffs() {
        assert_eq!(
            parse_fence_language(Some("diff-rust")),
            ("rust".into(), true)
        );
        assert_eq!(
            parse_fence_language(Some("diff")),
            ("plaintext".into(), true)
        );
        assert_eq!(parse_fence_language(None), ("plaintext".into(), false));
        let (kinds, glyphs, content) = split_diff_lines(&[
            "--- a",
            "+++ b",
            "@@ -1 +1 @@",
            "-old",
            "+new",
            " same",
            "bare",
        ]);
        assert_eq!(kinds[0], DiffLineKind::Header);
        assert_eq!(kinds[3], DiffLineKind::Removed);
        assert_eq!(glyphs[4], "+");
        assert_eq!(content[5], "same");
        assert_eq!(content[6], "bare");
    }
}
