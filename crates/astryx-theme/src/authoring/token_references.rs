//! Token references (`var(--name)`) and their resolution: upstream
//! `theme/tokenReferences.ts`.
//!
//! [`resolve_token_references`] follows each `var(--name[, fallback])` in a flat
//! map of token names to CSS values, and the scanners it shares
//! ([`split_top_level_comma`], [`split_top_level_whitespace`],
//! [`find_matching_paren`]) read a CSS value the way upstream's do: a comma or
//! space inside parentheses or a quoted string is not top level, and in a quoted
//! string a backslash escapes the next character.
//!
//! Upstream indexes strings by UTF-16 unit; here indexes are byte offsets. Every
//! character the scanners look for is ASCII, so the two split a value at the
//! same places. Whitespace and trimming follow JavaScript's `\s` and
//! `String.prototype.trim`, not Rust's `char::is_whitespace` (which differ on
//! U+0085 and U+FEFF).

use std::collections::{HashMap, HashSet};

use super::js::{is_js_whitespace, js_trim};

/// Call `visit` with the byte index and character of each top-level character
/// of a CSS value, until it returns true (upstream `scanTopLevel`). A
/// top-level character is outside parentheses and outside quoted strings; in a
/// quoted string a backslash escapes the next character. An unmatched `)` does
/// not take the depth below zero.
fn scan_top_level(input: &str, mut visit: impl FnMut(usize, char) -> bool) {
    let mut depth: u32 = 0;
    let mut quote: Option<char> = None;
    let mut is_escaped = false;

    for (i, c) in input.char_indices() {
        if let Some(open) = quote {
            if is_escaped {
                is_escaped = false;
            } else if c == '\\' {
                is_escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }

        if c == '"' || c == '\'' {
            quote = Some(c);
            continue;
        }

        if c == '(' {
            depth += 1;
            continue;
        }

        if c == ')' {
            depth = depth.saturating_sub(1);
            continue;
        }

        if depth == 0 && visit(i, c) {
            return;
        }
    }
}

/// The arguments of a CSS function body split on the first top-level comma,
/// each trimmed, or `None` when there is none (upstream
/// `splitTopLevelComma`). A comma inside a nested function such as `rgba()`,
/// `color-mix()` or `var()`, or inside a quoted string, does not split.
pub fn split_top_level_comma(input: &str) -> Option<(&str, &str)> {
    let mut split = None;
    scan_top_level(input, |i, c| {
        if c != ',' {
            return false;
        }
        split = Some((js_trim(&input[..i]), js_trim(&input[i + 1..])));
        true
    });
    split
}

/// A CSS value split on the whitespace between its components (upstream
/// `splitTopLevelWhitespace`). Whitespace inside parentheses or quotes belongs
/// to one component, so `clamp(8px, 2vw, 16px)` and `var(--gap, 1px 2px)` each
/// stay whole. An empty value has no components.
pub fn split_top_level_whitespace(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    scan_top_level(input, |i, c| {
        if is_js_whitespace(c) {
            if i > start {
                parts.push(&input[start..i]);
            }
            start = i + c.len_utf8();
        }
        false
    });
    if input.len() > start {
        parts.push(&input[start..]);
    }
    parts
}

/// The byte index of the `)` matching the `(` at `open_index`, or `None` when
/// it is unbalanced (upstream `findMatchingParen`, which answers -1). Quotes
/// are not considered.
pub fn find_matching_paren(input: &str, open_index: usize) -> Option<usize> {
    let mut depth: i64 = 0;
    for (i, byte) in input.bytes().enumerate().skip(open_index) {
        if byte == b'(' {
            depth += 1;
        } else if byte == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// The state of one [`resolve_token_references_with`] call: the map it reads
/// and the values resolved so far, in the order they resolved.
struct Resolver<'a> {
    raw: HashMap<&'a str, &'a str>,
    cache: HashMap<String, String>,
    order: Vec<String>,
    evaluate: &'a dyn Fn(&str) -> String,
}

impl Resolver<'_> {
    /// `expr` with each `var()` replaced (upstream's `substitute`). `seen` holds
    /// the reference chain, so a cycle resolves to the literal reference
    /// instead of recursing forever.
    fn substitute(&mut self, expr: &str, seen: &mut HashSet<String>) -> String {
        let Some(start) = expr.find("var(") else {
            return expr.to_string();
        };
        let open_index = start + "var".len();
        let Some(close_index) = find_matching_paren(expr, open_index) else {
            return expr.to_string();
        };

        let inner = &expr[open_index + 1..close_index];
        let comma_split = split_top_level_comma(inner);
        let name = js_trim(comma_split.map_or(inner, |(name, _)| name));
        let fallback = comma_split.map(|(_, fallback)| fallback);

        let replacement = if seen.contains(name) {
            expr[start..=close_index].to_string()
        } else if self.raw.contains_key(name) {
            seen.insert(name.to_string());
            let resolved = self.reference(name, seen);
            seen.remove(name);
            resolved
        } else if let Some(fallback) = fallback {
            self.substitute(js_trim(fallback), seen)
        } else {
            expr[start..=close_index].to_string()
        };

        let rest = self.substitute(&expr[close_index + 1..], seen);
        format!("{}{replacement}{rest}", &expr[..start])
    }

    /// `name`'s resolved value, computed once (upstream's `reference`).
    fn reference(&mut self, name: &str, seen: &mut HashSet<String>) -> String {
        if let Some(cached) = self.cache.get(name) {
            return cached.clone();
        }
        let Some(value) = self.raw.get(name).copied() else {
            return String::new();
        };
        let substituted = if value.contains("var(") {
            self.substitute(value, seen)
        } else {
            value.to_string()
        };
        let resolved = (self.evaluate)(&substituted);
        // A cycle through `name` can cache it while it resolves; the later
        // write keeps its place, as a JavaScript object's would.
        if self
            .cache
            .insert(name.to_string(), resolved.clone())
            .is_none()
        {
            self.order.push(name.to_string());
        }
        resolved
    }
}

/// Every value in `raw` resolved to a concrete one (upstream
/// `resolveTokenReferences` with no `evaluate`): each `var(--name[, fallback])`
/// is replaced by `name`'s resolved value, or by its fallback when `raw` has no
/// `name`, and is kept as written when neither exists or the references form a
/// cycle. See [`resolve_token_references_with`].
pub fn resolve_token_references<K: AsRef<str>, V: AsRef<str>>(
    raw: &[(K, V)],
) -> Vec<(String, String)> {
    resolve_token_references_with(raw, |expr| expr.to_string())
}

/// [`resolve_token_references`] with `evaluate` run on each value once its
/// references are substituted (upstream `resolveTokenReferences(raw,
/// evaluate)`; upstream's tokens.ts evaluates CSS color functions there).
///
/// `raw` is upstream's object as name and value pairs: a name listed twice
/// takes its last value, as an object built from the pairs would. The result
/// holds every name of `raw` in the order its value resolved, which is
/// upstream's object key order: a name another one references resolves first.
pub fn resolve_token_references_with<K: AsRef<str>, V: AsRef<str>>(
    raw: &[(K, V)],
    evaluate: impl Fn(&str) -> String,
) -> Vec<(String, String)> {
    let mut names: Vec<&str> = Vec::with_capacity(raw.len());
    let mut map: HashMap<&str, &str> = HashMap::with_capacity(raw.len());
    for (name, value) in raw {
        let name = name.as_ref();
        if map.insert(name, value.as_ref()).is_none() {
            names.push(name);
        }
    }
    let mut resolver = Resolver {
        raw: map,
        cache: HashMap::new(),
        order: Vec::new(),
        evaluate: &evaluate,
    };
    for name in names {
        resolver.reference(name, &mut HashSet::new());
    }
    let Resolver {
        mut cache, order, ..
    } = resolver;
    order
        .into_iter()
        .map(|name| {
            let value = cache.remove(&name).unwrap_or_default();
            (name, value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::js::{js_number_to_string as js_number_string, js_round};
    use super::*;

    fn lookup<'a>(tokens: &'a [(String, String)], name: &str) -> Option<&'a str> {
        tokens
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    // Upstream tokenReferences.test.ts, `splitTopLevelWhitespace`.
    #[test]
    fn split_top_level_whitespace_keeps_parentheses_and_quotes_whole() {
        let cases: [(&str, &[&str]); 9] = [
            ("clamp(8px, 2vw, 16px)", &["clamp(8px, 2vw, 16px)"]),
            ("max(8px, 2vw)", &["max(8px, 2vw)"]),
            (
                "calc(1px + (2px * 3)) 4px",
                &["calc(1px + (2px * 3))", "4px"],
            ),
            ("var(--a, \"x y\") 1px", &["var(--a, \"x y\")", "1px"]),
            ("\"a\\\" b\" 1px", &["\"a\\\" b\"", "1px"]),
            ("'it\\'s here' 2px", &["'it\\'s here'", "2px"]),
            ("  8px  12px\t16px\n", &["8px", "12px", "16px"]),
            ("", &[]),
            ("   ", &[]),
        ];
        for (input, parts) in cases {
            assert_eq!(split_top_level_whitespace(input), parts, "{input:?}");
        }
    }

    // Upstream tokenReferences.test.ts, `splitTopLevelComma`.
    #[test]
    fn split_top_level_comma_splits_on_the_first_top_level_comma() {
        let cases = [
            ("--a, 1px 2px", ("--a", "1px 2px")),
            ("--a, rgba(0, 0, 0, 0.5)", ("--a", "rgba(0, 0, 0, 0.5)")),
            ("\"x, y\", z", ("\"x, y\"", "z")),
            ("\"a\\\", b\", c", ("\"a\\\", b\"", "c")),
        ];
        for (input, parts) in cases {
            assert_eq!(split_top_level_comma(input), Some(parts), "{input:?}");
        }
    }

    // Upstream tokenReferences.test.ts: "answers null when there is no
    // top-level comma".
    #[test]
    fn split_top_level_comma_answers_none_without_a_top_level_comma() {
        assert_eq!(split_top_level_comma("rgba(0, 0, 0)"), None);
        assert_eq!(split_top_level_comma("\"a, b\""), None);
    }

    // Expected values from reading `splitTopLevelWhitespace`: JavaScript's `\s`
    // matches U+00A0 and U+FEFF, and not U+0085.
    #[test]
    fn split_top_level_whitespace_uses_javascript_whitespace() {
        assert_eq!(split_top_level_whitespace("1px\u{A0}2px"), ["1px", "2px"]);
        assert_eq!(split_top_level_whitespace("1px\u{FEFF}2px"), ["1px", "2px"]);
        assert_eq!(split_top_level_whitespace("1px\u{85}2px"), ["1px\u{85}2px"]);
    }

    // Expected values from reading `findMatchingParen`.
    #[test]
    fn find_matching_paren_finds_the_balancing_paren() {
        assert_eq!(find_matching_paren("var(--a)", 3), Some(7));
        assert_eq!(find_matching_paren("a(b(c)d)e", 1), Some(7));
        assert_eq!(find_matching_paren("a(b(c)d)e", 3), Some(5));
        assert_eq!(find_matching_paren("var(--a", 3), None);
        assert_eq!(find_matching_paren("var()", 10), None);
    }

    // Expected values from reading `resolveTokenReferences`.
    #[test]
    fn resolve_token_references_follows_references_and_fallbacks() {
        let resolved = resolve_token_references(&[
            ("--a", "var(--b)"),
            ("--b", "4px"),
            ("--c", "var(--missing, var(--b))"),
            ("--d", "var(--missing)"),
            ("--e", "calc(var(--b) + var( --b ))"),
            ("--f", "var(--missing, 1px 2px)"),
        ]);
        assert_eq!(lookup(&resolved, "--a"), Some("4px"));
        assert_eq!(lookup(&resolved, "--c"), Some("4px"));
        assert_eq!(lookup(&resolved, "--d"), Some("var(--missing)"));
        assert_eq!(lookup(&resolved, "--e"), Some("calc(4px + 4px)"));
        assert_eq!(lookup(&resolved, "--f"), Some("1px 2px"));
    }

    // Upstream's cache is the result, so a name another one references comes
    // first: `--b` resolves while `--a` does.
    #[test]
    fn resolve_token_references_lists_names_in_resolution_order() {
        let resolved =
            resolve_token_references(&[("--a", "var(--b)"), ("--b", "1px"), ("--c", "2px")]);
        let names: Vec<&str> = resolved.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["--b", "--a", "--c"]);
    }

    // Expected values from tracing `resolveTokenReferences`: resolving `--a`
    // reaches `--b`, then `--a` again, whose reference to `--b` closes the
    // cycle and stays as written. That literal then fills `--b` and `--a` both,
    // and `--a`, cached first while the cycle resolved, keeps its place.
    #[test]
    fn resolve_token_references_keeps_a_cycle_as_written() {
        let resolved = resolve_token_references(&[("--a", "var(--b)"), ("--b", "var(--a)")]);
        assert_eq!(
            resolved,
            [
                ("--a".to_string(), "var(--b)".to_string()),
                ("--b".to_string(), "var(--b)".to_string())
            ]
        );
        let own = resolve_token_references(&[("--a", "var(--a)")]);
        assert_eq!(own, [("--a".to_string(), "var(--a)".to_string())]);
    }

    #[test]
    fn resolve_token_references_with_evaluates_each_substituted_value() {
        let resolved =
            resolve_token_references_with(&[("--a", "var(--b)"), ("--b", "x")], |expr| {
                format!("[{expr}]")
            });
        // `--b` evaluates once, and `--a` evaluates the substituted `[x]`.
        assert_eq!(lookup(&resolved, "--b"), Some("[x]"));
        assert_eq!(lookup(&resolved, "--a"), Some("[[x]]"));
    }

    #[test]
    fn resolve_token_references_takes_the_last_value_of_a_repeated_name() {
        let resolved = resolve_token_references(&[("--a", "1px"), ("--b", "2px"), ("--a", "3px")]);
        assert_eq!(
            resolved,
            [
                ("--a".to_string(), "3px".to_string()),
                ("--b".to_string(), "2px".to_string())
            ]
        );
    }

    // Expected values: what JavaScript's `Math.round` answers for each.
    #[test]
    fn js_round_rounds_halves_up() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(0.49999999999999994), 0.0);
        assert_eq!(js_round(131.25 / 5.0), 26.0);
        assert!(js_round(-0.2).is_sign_negative());
        assert!(js_round(f64::NAN).is_nan());
    }

    // Expected values: what `String(n)` answers in JavaScript.
    #[test]
    fn js_number_string_writes_numbers_as_javascript_does() {
        let cases = [
            (0.875, "0.875"),
            (1.0, "1"),
            (1.3333, "1.3333"),
            (1300.0, "1300"),
            (-0.0, "0"),
            (-1.5, "-1.5"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1e21, "1e+21"),
            (123456789012345680000.0, "123456789012345680000"),
            (0.000001, "0.000001"),
            (1e-7, "1e-7"),
            (1.5e-7, "1.5e-7"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
        ];
        for (value, text) in cases {
            assert_eq!(js_number_string(value), text, "{value:e}");
        }
    }
}
