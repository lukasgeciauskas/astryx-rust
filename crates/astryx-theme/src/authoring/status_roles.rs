//! The status roles a theme leaves unset, derived and scoped: upstream
//! `theme/statusRoleDerivation.ts` and `theme/statusRoleDefaults.ts`, with the
//! parts of `theme/declarationBoundary.ts` they read.
//!
//! Each status (`info`, `success`, `warning`, `error`) has one strong fill,
//! `--color-<status>`. A theme that sets a fill, the content colors, a pale fill
//! or a surface gets the roles it leaves unset computed against its own colors
//! ([`derive_status_role_tokens`], architecture:theme-compilation INV17):
//!
//! - `--color-on-<status>`: the theme's light or dark content color
//!   (`--color-on-dark` or `--color-on-light`), whichever contrasts more with
//!   the fill, or pure white or black when neither reaches 4.5:1;
//! - `--color-text-<status>`: the fill's hue at 4.5:1 on the surface and card;
//! - `--color-on-<status>-muted`: the fill's hue at 4.5:1 on the pale fill
//!   over the surface.
//!
//! Fills are never derived. The roles whose core default references another
//! token (the text roles follow the hue's text color, icon and border roles
//! follow the text role) are then repeated at the theme root as `var()`
//! aliases when the theme sets their source, unless an alias would close a
//! reference cycle ([`generated_scoped_status_role_defaults`], INV16). The
//! theme's resolved tokens and its stylesheet read the same list.
//!
//! The cascade and cycle checks read declarations as a browser would: a name
//! in its canonical spelling, a value's `var()` references and `!important`
//! found by the CSS tokenizer ([`analyze_declaration_value`]).

use std::collections::HashSet;
use std::sync::OnceLock;

use super::TOKEN_DEFAULTS;
use super::color::Rgba;
use super::define::{CssMap, ThemeAdaptationCondition};
use super::js::js_trim;
use super::status_color::{
    composite, contrast_ratio, is_chromatic, parse_concrete_color, solve_lightness, to_hex,
};
use super::token_values::ResolvedThemeMode;

// =============================================================================
// declarationBoundary.ts: the CSS tokenizer rules
// =============================================================================

/// A newline after preprocessing (CSS Syntax §4.2; CR and FF are newlines).
/// Upstream `isNewline`.
fn is_newline(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{c}')
}

/// CSS whitespace. Upstream `isWhitespace`.
fn is_whitespace(ch: char) -> bool {
    ch == ' ' || ch == '\t' || is_newline(ch)
}

/// Whether the character at `at` is a hex digit (none past the end).
fn is_hex_digit_at(input: &[char], at: usize) -> bool {
    input.get(at).is_some_and(char::is_ascii_hexdigit)
}

/// A name-start code point: a letter, an underscore or any non-ASCII code
/// point. Upstream `isNameStart`.
fn is_name_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || !ch.is_ascii()
}

/// A name code point. Upstream `isNameChar`.
fn is_name_char(ch: char) -> bool {
    is_name_start(ch) || ch.is_ascii_digit() || ch == '-'
}

/// CSS Syntax §3.3 preprocessing, for scanning only: CR LF, CR and FF become
/// LF and NUL becomes U+FFFD. Upstream `preprocess`, which also replaces lone
/// surrogates, which a Rust string cannot hold.
fn preprocess(input: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\u{c}' => out.push('\n'),
            '\0' => out.push('\u{fffd}'),
            other => out.push(other),
        }
    }
    out
}

/// A code point that makes an unquoted `url()` a bad-url token. Upstream
/// `NON_PRINTABLE`.
fn is_non_printable(ch: char) -> bool {
    matches!(ch, '\u{1}'..='\u{8}' | '\u{b}' | '\u{e}'..='\u{1f}' | '\u{7f}')
}

/// "Check if two code points are a valid escape" (§4.3.8). Upstream
/// `isValidEscape`.
fn is_valid_escape(input: &[char], at: usize) -> bool {
    input.get(at) == Some(&'\\') && input.get(at + 1).is_some_and(|&next| !is_newline(next))
}

/// "Check if three code points would start an ident sequence" (§4.3.9).
/// Upstream `startsIdent`.
fn starts_ident(input: &[char], at: usize) -> bool {
    match input.get(at) {
        Some('-') => input.get(at + 1).is_some_and(|&next| {
            next == '-' || is_name_start(next) || is_valid_escape(input, at + 1)
        }),
        Some('\\') => is_valid_escape(input, at),
        Some(&ch) => is_name_start(ch),
        None => false,
    }
}

/// "Consume an escaped code point" (§4.3.7), positioned after the backslash:
/// the position after the escape. A hex escape takes up to six hex digits and
/// one whitespace; anything else takes one code point. Upstream
/// `consumeEscape`.
fn consume_escape(input: &[char], at: usize) -> usize {
    if is_hex_digit_at(input, at) {
        let mut end = at;
        while end < input.len() && end - at < 6 && is_hex_digit_at(input, end) {
            end += 1;
        }
        if end < input.len() && is_whitespace(input[end]) {
            end += 1;
        }
        return end;
    }
    at + 1
}

/// "Consume an ident sequence" (§4.3.11) from `at`: the position after it and
/// the name with its escapes decoded (a hex escape as its code point, or
/// U+FFFD for zero, a surrogate or one past U+10FFFF). Upstream
/// `consumeIdent`.
fn consume_ident(input: &[char], at: usize) -> (usize, String) {
    let mut name = String::new();
    let mut i = at;
    while i < input.len() {
        let ch = input[i];
        if is_name_char(ch) {
            name.push(ch);
            i += 1;
        } else if is_valid_escape(input, i) {
            let end = consume_escape(input, i + 1);
            let escaped: String = input[i + 1..end.min(input.len())].iter().collect();
            if is_hex_digit_at(input, i + 1) {
                let code = u32::from_str_radix(js_trim(&escaped), 16).unwrap_or(0);
                name.push(
                    (code != 0)
                        .then(|| char::from_u32(code))
                        .flatten()
                        .unwrap_or('\u{fffd}'),
                );
            } else {
                name.push_str(&escaped);
            }
            i = end;
        } else {
            break;
        }
    }
    (i, name)
}

/// "Consume a string token" (§4.3.5) at its opening quote: the position after
/// the closing quote, or why the value cannot be one declaration. Upstream
/// `consumeString`.
fn consume_string(input: &[char], at: usize) -> Result<usize, String> {
    let quote = input[at];
    let mut i = at + 1;
    while i < input.len() {
        let ch = input[i];
        if ch == quote {
            return Ok(i + 1);
        }
        if is_newline(ch) {
            // A raw newline ends the string as a bad-string token.
            return Err("a newline inside a quoted string makes a bad string".to_string());
        }
        if ch == '\\' {
            if i + 1 >= input.len() {
                break;
            }
            // An escaped newline continues the line; any other escape takes
            // the escaped code point, so an escaped quote never closes.
            i = if is_newline(input[i + 1]) {
                i + 2
            } else {
                consume_escape(input, i + 1)
            };
            continue;
        }
        i += 1;
    }
    Err(format!(
        "an unclosed {quote} string would swallow the rest of the rule"
    ))
}

/// "Consume a url token" (§4.3.6) after `url(` and its leading whitespace,
/// with a code point other than a quote next: the position after the closing
/// `)`, or why the value cannot be one declaration. Upstream `consumeUrl`.
fn consume_url(input: &[char], at: usize) -> Result<usize, String> {
    let mut i = at;
    while i < input.len() {
        let ch = input[i];
        if ch == ')' {
            return Ok(i + 1);
        }
        if is_whitespace(ch) {
            while i < input.len() && is_whitespace(input[i]) {
                i += 1;
            }
            if i >= input.len() {
                break;
            }
            if input[i] == ')' {
                return Ok(i + 1);
            }
            return Err("whitespace inside an unquoted url() makes a bad url".to_string());
        }
        if is_non_printable(ch) {
            return Err("a non-printable character inside url() makes a bad url".to_string());
        }
        if ch == '"' || ch == '\'' || ch == '(' {
            return Err(format!(
                "\"{ch}\" inside an unquoted url() makes a bad url; quote the URL instead"
            ));
        }
        if ch == '\\' {
            if !is_valid_escape(input, i) {
                return Err("a stray backslash inside url() makes a bad url".to_string());
            }
            i = consume_escape(input, i + 1);
            continue;
        }
        i += 1;
    }
    Err("an unclosed url( would swallow the rest of the rule".to_string())
}

/// The closer of a block opener. Upstream `CLOSER`.
fn closer(opener: char) -> char {
    match opener {
        '(' => ')',
        '[' => ']',
        _ => '}',
    }
}

/// The opener of a block closer.
fn opener(closer: char) -> char {
    match closer {
        ')' => '(',
        ']' => '[',
        _ => '{',
    }
}

/// The position of `*/` at or after `from`.
fn find_comment_end(input: &[char], from: usize) -> Option<usize> {
    (from..input.len().saturating_sub(1)).find(|&i| input[i] == '*' && input[i + 1] == '/')
}

/// Why `input` cannot be emitted as the value of one declaration, or `None`
/// when it can (upstream `checkDeclarationValue`). Strings, comments, an
/// unquoted `url()` body, escapes and nested `()`, `[]` and `{}` blocks are
/// read as the CSS tokenizer reads them, so a `;` or brace inside them is inert;
/// an unnested `;`, `{` or `}`, an unbalanced closer, anything left open at the
/// end, and a bad string or bad url are refused.
pub fn check_declaration_value(input: &str) -> Option<String> {
    let value = preprocess(input);
    let mut open: Vec<char> = Vec::new();
    let mut i = 0;
    while i < value.len() {
        let ch = value[i];

        if ch == '/' && value.get(i + 1) == Some(&'*') {
            let Some(close) = find_comment_end(&value, i + 2) else {
                return Some(
                    "an unclosed /* comment would swallow the rest of the rule".to_string(),
                );
            };
            i = close + 2;
            continue;
        }

        if ch == '"' || ch == '\'' {
            match consume_string(&value, i) {
                Ok(end) => i = end,
                Err(reason) => return Some(reason),
            }
            continue;
        }

        if starts_ident(&value, i) {
            let (end, name) = consume_ident(&value, i);
            i = end;
            if value.get(i) == Some(&'(') && name.to_lowercase() == "url" {
                let mut j = i + 1;
                while j < value.len() && is_whitespace(value[j]) {
                    j += 1;
                }
                if matches!(value.get(j), Some('"' | '\'')) {
                    // `url("...")` is an ordinary function token; the loop
                    // reads the string and the closing paren.
                    open.push(')');
                    i += 1;
                    continue;
                }
                match consume_url(&value, j) {
                    Ok(end) => i = end,
                    Err(reason) => return Some(reason),
                }
            }
            continue;
        }

        if ch == '\\' {
            if i + 1 >= value.len() {
                // `\` before the generator's own `;` would escape it.
                return Some(
                    "a trailing backslash would escape the declaration terminator".to_string(),
                );
            }
            // `\` and a newline is no escape: the backslash is a delim and the
            // newline whitespace. Anything else escapes the next code point.
            i = if is_newline(value[i + 1]) {
                i + 1
            } else {
                consume_escape(&value, i + 1)
            };
            continue;
        }

        if ch == '(' || ch == '[' || ch == '{' {
            if ch == '{' && open.is_empty() {
                return Some("an unquoted \"{\" would open a nested block".to_string());
            }
            open.push(closer(ch));
            i += 1;
            continue;
        }

        if ch == ')' || ch == ']' || ch == '}' {
            let Some(expected) = open.pop() else {
                return Some(if ch == '}' {
                    "an unquoted \"}\" would close the rule".to_string()
                } else {
                    format!("an unbalanced \"{ch}\" has nothing to close")
                });
            };
            if expected != ch {
                return Some(format!(
                    "\"{ch}\" does not close the open \"{}\"",
                    opener(expected)
                ));
            }
            i += 1;
            continue;
        }

        if ch == ';' && open.is_empty() {
            return Some("an unquoted \";\" would end the declaration".to_string());
        }

        i += 1;
    }

    open.last().map(|&closer| {
        format!(
            "an unclosed \"{}\" would swallow the rest of the rule",
            opener(closer)
        )
    })
}

/// What [`analyze_declaration_value`] finds in a value (upstream
/// `DeclarationValueAnalysis`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeclarationValueAnalysis {
    /// Why the value cannot be one declaration ([`check_declaration_value`]),
    /// or `None`.
    pub reason: Option<String>,
    /// The custom properties its `var()` functions reference, in order, their
    /// names in canonical spelling.
    pub references: Vec<String>,
    /// Whether it ends in `!important`.
    pub important: bool,
    /// The name of the one `var()` that is the whole value (`!important`
    /// aside), or `None`.
    pub single_var_reference: Option<String>,
}

/// The position after whitespace and comments from `start`. Upstream
/// `skipWhitespaceAndComments`.
fn skip_whitespace_and_comments(input: &[char], start: usize) -> usize {
    let mut index = start;
    loop {
        while index < input.len() && is_whitespace(input[index]) {
            index += 1;
        }
        if input.get(index) != Some(&'/') || input.get(index + 1) != Some(&'*') {
            return index;
        }
        match find_comment_end(input, index + 2) {
            Some(close) => index = close + 2,
            None => return input.len(),
        }
    }
}

/// The custom property a `var(` whose `(` is at `open_paren` references: an
/// ident starting `--` after whitespace and comments, followed by a comma or
/// `)`. Upstream `varReferenceAt`.
fn var_reference_at(input: &[char], open_paren: usize) -> Option<String> {
    let start = skip_whitespace_and_comments(input, open_paren + 1);
    if !starts_ident(input, start) {
        return None;
    }
    let (end, name) = consume_ident(input, start);
    if !name.starts_with("--") {
        return None;
    }
    let end = skip_whitespace_and_comments(input, end);
    matches!(input.get(end), Some(',' | ')')).then_some(name)
}

/// Whether the characters at `at` start a number. Upstream `startsNumber`.
fn starts_number(input: &[char], at: usize) -> bool {
    let digit = |i: usize| input.get(i).is_some_and(char::is_ascii_digit);
    match input.get(at) {
        Some('+' | '-') => digit(at + 1) || (input.get(at + 1) == Some(&'.') && digit(at + 2)),
        Some('.') => digit(at + 1),
        Some(_) => digit(at),
        None => false,
    }
}

/// The position after the number at `at`. Upstream `consumeNumber`.
fn consume_number(input: &[char], at: usize) -> usize {
    let digit = |i: usize| input.get(i).is_some_and(char::is_ascii_digit);
    let mut index = at;
    if matches!(input.get(index), Some('+' | '-')) {
        index += 1;
    }
    while digit(index) {
        index += 1;
    }
    if input.get(index) == Some(&'.') && digit(index + 1) {
        index += 2;
        while digit(index) {
            index += 1;
        }
    }
    if matches!(input.get(index), Some('e' | 'E'))
        && (digit(index + 1)
            || (matches!(input.get(index + 1), Some('+' | '-')) && digit(index + 2)))
    {
        index += 1;
        if matches!(input.get(index), Some('+' | '-')) {
            index += 1;
        }
        while digit(index) {
            index += 1;
        }
    }
    index
}

/// A top-level token of a value, as the analysis records it.
#[derive(Clone, Debug, PartialEq)]
enum TopLevel {
    /// A delim (only `!` is recorded as one).
    Delim,
    /// An ident that is not a function, with its name.
    Ident(String),
    /// A `var()` of one custom property.
    Var(String),
    /// Anything else.
    Other,
}

/// A declaration value read with the CSS tokenizer rules of
/// [`check_declaration_value`] (upstream `analyzeDeclarationValue`): why it
/// cannot be one declaration, or the custom properties its `var()`s
/// reference, whether it ends in `!important`, and the `var()` it is when it is
/// one. Comments, strings, hash and at tokens, numbers and an unquoted `url()`
/// body never hold a reference.
pub fn analyze_declaration_value(input: &str) -> DeclarationValueAnalysis {
    if let Some(reason) = check_declaration_value(input) {
        return DeclarationValueAnalysis {
            reason: Some(reason),
            ..DeclarationValueAnalysis::default()
        };
    }

    let value = preprocess(input);
    let mut references = Vec::new();
    let mut open: Vec<char> = Vec::new();
    let mut last_two: Vec<TopLevel> = Vec::new();
    let mut first: Option<TopLevel> = None;
    let mut count = 0usize;
    let mut record = |token: TopLevel, open: &[char]| {
        if open.is_empty() {
            count += 1;
            if first.is_none() {
                first = Some(token.clone());
            }
            last_two.push(token);
            if last_two.len() > 2 {
                last_two.remove(0);
            }
        }
    };

    let mut index = 0;
    while index < value.len() {
        let ch = value[index];
        if is_whitespace(ch) {
            index += 1;
            continue;
        }
        if ch == '/' && value.get(index + 1) == Some(&'*') {
            index = find_comment_end(&value, index + 2).map_or(value.len(), |close| close + 2);
            continue;
        }
        if ch == '"' || ch == '\'' {
            record(TopLevel::Other, &open);
            index = consume_string(&value, index).unwrap_or(value.len());
            continue;
        }
        if ch == '#' {
            // Upstream reads past the end as a TypeError for a value ending in
            // `#`; here nothing follows, so no name does.
            let name_start = index + 1;
            if value.get(name_start).is_some_and(|&c| is_name_char(c))
                || is_valid_escape(&value, name_start)
            {
                record(TopLevel::Other, &open);
                index = consume_ident(&value, name_start).0;
                continue;
            }
        }
        if ch == '@' && starts_ident(&value, index + 1) {
            record(TopLevel::Other, &open);
            index = consume_ident(&value, index + 1).0;
            continue;
        }
        if starts_number(&value, index) {
            record(TopLevel::Other, &open);
            index = consume_number(&value, index);
            if starts_ident(&value, index) {
                index = consume_ident(&value, index).0;
            } else if value.get(index) == Some(&'%') {
                index += 1;
            }
            continue;
        }
        if starts_ident(&value, index) {
            let (end, name) = consume_ident(&value, index);
            index = end;
            let is_function = value.get(index) == Some(&'(');
            let lower = name.to_lowercase();
            let reference = if is_function && lower == "var" {
                var_reference_at(&value, index)
            } else {
                None
            };
            record(
                match &reference {
                    Some(reference) => TopLevel::Var(reference.clone()),
                    None if is_function => TopLevel::Other,
                    None => TopLevel::Ident(name),
                },
                &open,
            );
            if is_function {
                if let Some(reference) = reference {
                    references.push(reference);
                }
                if lower == "url" {
                    let mut body = index + 1;
                    while body < value.len() && is_whitespace(value[body]) {
                        body += 1;
                    }
                    if !matches!(value.get(body), Some('"' | '\'')) {
                        index = consume_url(&value, body).unwrap_or(value.len());
                        continue;
                    }
                }
                open.push(')');
                index += 1;
            }
            continue;
        }
        if ch == '(' || ch == '[' || ch == '{' {
            record(TopLevel::Other, &open);
            open.push(closer(ch));
            index += 1;
            continue;
        }
        if ch == ')' || ch == ']' || ch == '}' {
            open.pop();
            index += 1;
            continue;
        }
        record(
            if ch == '!' {
                TopLevel::Delim
            } else {
                TopLevel::Other
            },
            &open,
        );
        index += 1;
    }

    let important = match last_two.as_slice() {
        [TopLevel::Delim, TopLevel::Ident(name)] => name.to_lowercase() == "important",
        _ => false,
    };
    let single_var_reference = match first {
        Some(TopLevel::Var(reference)) if count - if important { 2 } else { 0 } == 1 => {
            Some(reference)
        }
        _ => None,
    };
    DeclarationValueAnalysis {
        reason: None,
        references,
        important,
        single_var_reference,
    }
}

/// The browser-canonical spelling of one valid declaration name (its escapes
/// decoded), or `None` when it is not one CSS ident. Upstream
/// `canonicalizeDeclarationName`.
pub fn canonicalize_declaration_name(input: &str) -> Option<String> {
    let name = preprocess(input);
    if name.is_empty() || !starts_ident(&name, 0) {
        return None;
    }
    let (end, ident) = consume_ident(&name, 0);
    (end == name.len()).then_some(ident)
}

// =============================================================================
// statusRoleDerivation.ts
// =============================================================================

/// The statuses, in upstream's order (upstream `STATUS_NAMES`).
pub const STATUS_NAMES: [&str; 4] = ["info", "success", "warning", "error"];

/// The contrast the derived text roles reach. Upstream `TEXT_RATIO`.
const TEXT_RATIO: f64 = 4.5;

/// The contrast the derivation solves for: slightly above the floor so that
/// rounding to `#RRGGBB` never lands below it. Upstream `SOLVE_RATIO`.
const SOLVE_RATIO: f64 = TEXT_RATIO + 0.08;

/// The modes the roles are derived in. Upstream `MODES`.
const MODES: [ResolvedThemeMode; 2] = [ResolvedThemeMode::Light, ResolvedThemeMode::Dark];

/// The strong fill of a status, the source its other roles derive from:
/// `--color-<status>`. Upstream `statusSourceToken`.
pub fn status_source_token(status: &str) -> String {
    format!("--color-{status}")
}

/// Core's color token defaults (upstream `colorDefaults`): the
/// [`TOKEN_DEFAULTS`] before the first spacing token, since `tokenDefaults`
/// spreads `colorDefaults` first and `spacingDefaults` next.
pub fn color_defaults() -> &'static [(&'static str, &'static str)] {
    let end = TOKEN_DEFAULTS
        .iter()
        .position(|(name, _)| *name == "--spacing-0")
        .unwrap_or(TOKEN_DEFAULTS.len());
    &TOKEN_DEFAULTS[..end]
}

/// The core color default of `name`.
fn color_default(name: &str) -> Option<&'static str> {
    color_defaults()
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
}

/// `input` split at every top-level comma (outside parentheses), each part
/// trimmed. Upstream `splitTopLevel` (`statusRoleDerivation.ts`).
fn split_top_level(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i64 = 0;
    let mut start = 0;
    for (i, ch) in input.char_indices() {
        if ch == '(' {
            depth += 1;
        }
        if ch == ')' {
            depth -= 1;
        }
        if ch == ',' && depth == 0 {
            parts.push(js_trim(&input[start..i]));
            start = i + 1;
        }
    }
    parts.push(js_trim(&input[start..]));
    parts
}

/// One token in `mode`, following `light-dark()` and `var()` through the
/// theme's tokens and then core's color defaults (upstream `resolveToken`).
/// `None` for an unknown token or a reference cycle.
fn resolve_token(
    tokens: &CssMap,
    name: &str,
    mode: ResolvedThemeMode,
    seen: &HashSet<String>,
) -> Option<String> {
    if seen.contains(name) {
        return None;
    }
    let raw = tokens
        .get(name)
        .map(String::as_str)
        .or_else(|| color_default(name));
    let mut seen = seen.clone();
    seen.insert(name.to_string());
    resolve_value(tokens, raw, mode, &seen)
}

/// The parts of `/^var\(\s*(--[\w-]+)\s*(?:,(.*))?\)$/s` in `value`: the name
/// and the fallback, if any.
fn whole_var(value: &str) -> Option<(&str, Option<&str>)> {
    let rest = value.strip_prefix("var(")?;
    let rest = rest.trim_start_matches(super::js::is_js_whitespace);
    let name_len = rest
        .strip_prefix("--")?
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(rest.len() - 2);
    if name_len == 0 {
        return None;
    }
    let (name, after) = rest.split_at(2 + name_len);
    let after = after.trim_start_matches(super::js::is_js_whitespace);
    if after == ")" {
        return Some((name, None));
    }
    let fallback = after.strip_prefix(',')?.strip_suffix(')')?;
    Some((name, Some(fallback)))
}

/// One value in `mode` (upstream `resolveValue`): trimmed, a `light-dark()` by
/// its side, a whole `var()` by its token or else its fallback, anything else
/// as it is.
fn resolve_value(
    tokens: &CssMap,
    raw: Option<&str>,
    mode: ResolvedThemeMode,
    seen: &HashSet<String>,
) -> Option<String> {
    let value = js_trim(raw?);
    let is_light_dark = value
        .get(..11)
        .is_some_and(|head| head.eq_ignore_ascii_case("light-dark("));
    if is_light_dark && value.ends_with(')') {
        let open = value.find('(').unwrap_or(0);
        let parts = split_top_level(&value[open + 1..value.len() - 1]);
        let side = match mode {
            ResolvedThemeMode::Light => parts.first(),
            ResolvedThemeMode::Dark => parts.get(1),
        };
        return resolve_value(tokens, side.copied(), mode, seen);
    }
    if let Some((name, fallback)) = whole_var(value) {
        return resolve_token(tokens, name, mode, seen).or_else(|| {
            fallback
                .filter(|fallback| !fallback.is_empty())
                .and_then(|fallback| resolve_value(tokens, Some(fallback), mode, seen))
        });
    }
    Some(value.to_string())
}

/// Every token of `tokens` and of core's color defaults resolved in each mode,
/// light then dark (upstream `resolveModes`). A token that does not resolve is
/// left out.
pub fn resolve_modes(tokens: &CssMap) -> [CssMap; 2] {
    let mut names: Vec<&str> = color_defaults().iter().map(|(name, _)| *name).collect();
    for name in tokens.keys() {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    MODES.map(|mode| {
        let mut out = CssMap::new();
        for name in &names {
            if let Some(value) = resolve_token(tokens, name, mode, &HashSet::new()) {
                out.insert(*name, value);
            }
        }
        out
    })
}

/// Core's own colors in each mode, resolved once. Upstream `core`.
fn core() -> &'static [CssMap; 2] {
    static CORE: OnceLock<[CssMap; 2]> = OnceLock::new();
    CORE.get_or_init(|| resolve_modes(&CssMap::new()))
}

/// A mode's index in a light and dark pair.
fn mode_index(mode: ResolvedThemeMode) -> usize {
    match mode {
        ResolvedThemeMode::Light => 0,
        ResolvedThemeMode::Dark => 1,
    }
}

/// The opaque hue the text roles of `status` derive from: its fill, or core's
/// fill when the theme's is grey, so derived text stays chromatic. Upstream
/// `statusHue`.
fn status_hue(status: &str, resolved: &CssMap, mode: ResolvedThemeMode) -> Option<Rgba> {
    let source = status_source_token(status);
    if let Some(color) = parse_concrete_color(resolved.get(&source).map(String::as_str))
        && is_chromatic(color)
    {
        return Some(Rgba { a: 1.0, ..color });
    }
    let fallback = parse_concrete_color(core()[mode_index(mode)].get(&source).map(String::as_str))?;
    Some(Rgba { a: 1.0, ..fallback })
}

/// `color` made opaque over `under` when it is translucent. Upstream
/// `opaqueOver`.
fn opaque_over(color: Rgba, under: Rgba) -> Rgba {
    if color.a < 1.0 {
        composite(color, under)
    } else {
        color
    }
}

/// The roles of one status in one mode. Upstream `ModeRoles`.
#[derive(Clone, Copy, Debug, Default)]
struct ModeRoles {
    on: Option<Rgba>,
    text: Option<Rgba>,
    on_muted: Option<Rgba>,
}

const WHITE: Rgba = Rgba {
    r: 255.0,
    g: 255.0,
    b: 255.0,
    a: 1.0,
};

const BLACK: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

/// The roles of `status` in one mode, or `None` when a color they are measured
/// against is not concrete (an `oklch()` value, an unknown reference): the
/// status then keeps its scoped aliases rather than a guess. Upstream
/// `deriveForMode`.
fn derive_for_mode(status: &str, resolved: &CssMap, mode: ResolvedThemeMode) -> Option<ModeRoles> {
    let color = |name: &str| parse_concrete_color(resolved.get(name).map(String::as_str));
    let source = color(&status_source_token(status))?;
    let hue = status_hue(status, resolved, mode)?;
    let surface = color("--color-background-surface")?;
    let card_color = color("--color-background-card")?;
    let muted = color(&format!("--color-{status}-muted"))?;
    let on_dark = color("--color-on-dark")?;
    let on_light = color("--color-on-light")?;

    let opaque_surface = opaque_over(surface, WHITE);
    let card = opaque_over(card_color, opaque_surface);
    let fill = opaque_over(source, opaque_surface);

    let light_ink = opaque_over(on_dark, fill);
    let dark_ink = opaque_over(on_light, fill);
    let theme_ink = if contrast_ratio(light_ink, fill) >= contrast_ratio(dark_ink, fill) {
        light_ink
    } else {
        dark_ink
    };
    // A theme's off-white and off-black content can both miss the floor on a
    // mid-tone fill; pure white or black always reaches it.
    let on = if contrast_ratio(theme_ink, fill) >= TEXT_RATIO {
        theme_ink
    } else if contrast_ratio(WHITE, fill) >= contrast_ratio(BLACK, fill) {
        WHITE
    } else {
        BLACK
    };

    let text = solve_lightness(hue, &[opaque_surface, card], SOLVE_RATIO);

    let pale = composite(
        if muted.a < 1.0 {
            muted
        } else {
            Rgba { a: 1.0, ..muted }
        },
        opaque_surface,
    );
    let on_muted = solve_lightness(hue, &[pale], SOLVE_RATIO);
    Some(ModeRoles {
        on: Some(on),
        text,
        on_muted,
    })
}

/// A derived role, in upstream's `ROLE_TOKENS` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    On,
    Text,
    OnMuted,
}

impl Role {
    /// Every role, in upstream's `ROLE_TOKENS` order.
    const ALL: [Role; 3] = [Role::On, Role::Text, Role::OnMuted];

    /// The role's token for `status`. Upstream `ROLE_TOKENS[role]`.
    fn token(self, status: &str) -> String {
        match self {
            Role::On => format!("--color-on-{status}"),
            Role::Text => format!("--color-text-{status}"),
            Role::OnMuted => format!("--color-on-{status}-muted"),
        }
    }

    /// The role's color in a mode's roles.
    fn read(self, roles: &ModeRoles) -> Option<Rgba> {
        match self {
            Role::On => roles.on,
            Role::Text => roles.text,
            Role::OnMuted => roles.on_muted,
        }
    }
}

/// Concrete `light-dark()` values for every status role a theme leaves unset
/// while setting a color that role is measured against (upstream
/// `deriveStatusRoleTokens`). Authored roles are never overridden.
///
/// The on role follows the fill and the content colors, the text role the
/// fill and the surfaces, the on-muted role the fill, the pale fill and the
/// surfaces. A status none of whose colors the theme sets is skipped, and so
/// is one with a color that does not resolve to a concrete value in either
/// mode.
pub fn derive_status_role_tokens(tokens: &CssMap, local_tokens: &CssMap) -> Vec<(String, String)> {
    let authored = |name: &str| tokens.contains_key(name);
    let surface_set = authored("--color-background-surface") || authored("--color-background-card");
    let ink_set = authored("--color-on-dark") || authored("--color-on-light");
    let mut merged = local_tokens.clone();
    merged.assign(tokens);
    let resolved = resolve_modes(&merged);
    let mut entries = Vec::new();
    for status in STATUS_NAMES {
        let fill_set = authored(&status_source_token(status));
        let muted_set = authored(&format!("--color-{status}-muted"));
        // Which roles this theme's own colors affect.
        let affects = |role: Role| match role {
            Role::On => fill_set || ink_set,
            Role::Text => fill_set || surface_set,
            Role::OnMuted => fill_set || muted_set || surface_set,
        };
        if !Role::ALL.into_iter().any(affects) {
            continue;
        }
        let Some(per_mode) = MODES
            .iter()
            .map(|&mode| derive_for_mode(status, &resolved[mode_index(mode)], mode))
            .collect::<Option<Vec<ModeRoles>>>()
        else {
            continue;
        };
        for role in Role::ALL {
            let token = role.token(status);
            if !affects(role) || authored(&token) {
                continue;
            }
            let (Some(light), Some(dark)) = (role.read(&per_mode[0]), role.read(&per_mode[1]))
            else {
                continue;
            };
            entries.push((
                token,
                format!("light-dark({}, {})", to_hex(light), to_hex(dark)),
            ));
        }
    }
    entries
}

// =============================================================================
// statusRoleDefaults.ts
// =============================================================================

/// Public tokens whose core default references another public token, as
/// `(role, source)`: the status roles, `--duration-exit` and `--radius-chat`
/// (upstream `SCOPED_REFERENCE_DEFAULTS`). A theme that sets a source repeats
/// the role it leaves unset at its root, so a nested theme region binds the
/// alias to its own source.
pub const SCOPED_REFERENCE_DEFAULTS: [(&str, &str); 18] = [
    ("--color-text-info", "--color-text-blue"),
    ("--color-text-success", "--color-text-green"),
    ("--color-text-warning", "--color-text-yellow"),
    ("--color-text-error", "--color-text-red"),
    ("--color-on-info-muted", "--color-text-blue"),
    ("--color-on-success-muted", "--color-text-green"),
    ("--color-on-warning-muted", "--color-text-yellow"),
    ("--color-on-error-muted", "--color-text-red"),
    // Icon and border roles follow the text role, set or derived.
    ("--color-icon-info", "--color-text-info"),
    ("--color-icon-success", "--color-text-success"),
    ("--color-icon-warning", "--color-text-warning"),
    ("--color-icon-error", "--color-text-error"),
    ("--color-border-info", "--color-text-info"),
    ("--color-border-success", "--color-text-success"),
    ("--color-border-warning", "--color-text-warning"),
    ("--color-border-error", "--color-text-error"),
    ("--duration-exit", "--duration-fast"),
    ("--radius-chat", "--radius-container"),
];

/// A role's place in [`SCOPED_REFERENCE_DEFAULTS`], 0 for any other token
/// (upstream `STATUS_ROLE_ORDER.get(role) ?? 0`).
fn status_role_order(role: &str) -> usize {
    SCOPED_REFERENCE_DEFAULTS
        .iter()
        .position(|(name, _)| *name == role)
        .unwrap_or(0)
}

/// The tokens a browser's cascade holds after a run of declarations, and the
/// ones set `!important` (upstream `TokenCascadeState`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenCascadeState {
    /// Each token's winning value, by canonical name.
    pub values: CssMap,
    /// The tokens whose winning value is `!important`.
    pub important: HashSet<String>,
}

/// Declares each token set on `state`, in order, as the browser's cascade would
/// (upstream `assignTokenCascade`): by its canonical name, skipping a
/// declaration that cannot be emitted (an invalid name or value), and not
/// letting a normal value replace an `!important` one.
pub fn assign_token_cascade(state: &mut TokenCascadeState, token_sets: &[Option<&CssMap>]) {
    for tokens in token_sets.iter().flatten() {
        for (raw_token, value) in tokens.iter() {
            let Some(token) = canonicalize_declaration_name(raw_token) else {
                continue;
            };
            let analysis = analyze_declaration_value(value);
            // As joinDeclarations: a declaration that cannot be emitted never
            // takes part in the cascade.
            if analysis.reason.is_some() {
                continue;
            }
            if state.important.contains(&token) && !analysis.important {
                continue;
            }
            state.values.insert(token.clone(), value.clone());
            if analysis.important {
                state.important.insert(token);
            } else {
                state.important.remove(&token);
            }
        }
    }
}

/// The cascade of `token_sets` from nothing (upstream `createTokenCascade`).
pub fn create_token_cascade(token_sets: &[Option<&CssMap>]) -> TokenCascadeState {
    let mut state = TokenCascadeState::default();
    assign_token_cascade(&mut state, token_sets);
    state
}

/// Whether `candidate_value`, set on `relevant_token`, would close a reference
/// cycle through it among `tokens` (upstream
/// `tokenValuesWouldHaveCycleWithValue`).
pub fn token_values_would_have_cycle_with_value(
    tokens: &CssMap,
    relevant_token: &str,
    candidate_value: &str,
) -> bool {
    let mut stack = analyze_declaration_value(candidate_value).references;
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(token) = stack.pop() {
        if token == relevant_token {
            return true;
        }
        let Some(value) = tokens.get(&token) else {
            continue;
        };
        if !visited.insert(token) {
            continue;
        }
        for reference in analyze_declaration_value(value).references {
            if reference == relevant_token || tokens.contains_key(&reference) {
                stack.push(reference);
            }
        }
    }
    false
}

/// The token writes of one resolved adaptation rule, as the status-role
/// cascade reads them (the `when`, `tokens` and `localTokens` of upstream's
/// `ResolvedThemeAdaptationRule`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AdaptationTokenWrites {
    /// When the rule applies.
    pub when: ThemeAdaptationCondition,
    /// Its concrete token writes.
    pub tokens: CssMap,
    /// Its concrete local-token writes.
    pub local_tokens: Option<CssMap>,
}

/// Each named width point's rank among the width cells. Upstream
/// `WIDTH_CELL_RANK`.
fn width_cell_rank(name: &str) -> Option<usize> {
    ["sm", "md", "lg", "xl", "2xl"]
        .iter()
        .position(|point| *point == name)
        .map(|at| at + 1)
}

/// Whether a rule's condition holds in one cell of the environment. Upstream
/// `adaptationConditionMatchesCell`.
fn adaptation_condition_matches_cell(
    condition: &ThemeAdaptationCondition,
    width_rank: usize,
    pointer: &str,
    contrast: &str,
    motion: &str,
) -> bool {
    let width = condition.width.as_ref();
    if let Some(rank) = width
        .and_then(|w| w.from.as_deref())
        .and_then(width_cell_rank)
        && width_rank < rank
    {
        return false;
    }
    if let Some(rank) = width
        .and_then(|w| w.below.as_deref())
        .and_then(width_cell_rank)
        && width_rank >= rank
    {
        return false;
    }
    condition.pointer.as_deref().is_none_or(|p| p == pointer)
        && condition.contrast.as_deref().is_none_or(|c| c == contrast)
        && condition.motion.as_deref().is_none_or(|m| m == motion)
}

/// Every set of rules that can match together, as rule indexes in order, the
/// root's empty set included (upstream `reachableAdaptationMatchSets`): one per
/// distinct set found over the width ranks, pointers, contrast preferences and
/// motion preferences, in the order first found.
pub fn reachable_adaptation_match_sets(rules: &[AdaptationTokenWrites]) -> Vec<Vec<usize>> {
    let mut sets: Vec<Vec<usize>> = Vec::new();
    for width_rank in 0..=5 {
        for pointer in ["coarse", "fine", "none"] {
            for contrast in ["more", "less", "no-preference", "custom"] {
                for motion in ["reduce", "no-preference"] {
                    let indexes: Vec<usize> = rules
                        .iter()
                        .enumerate()
                        .filter(|(_, rule)| {
                            adaptation_condition_matches_cell(
                                &rule.when, width_rank, pointer, contrast, motion,
                            )
                        })
                        .map(|(index, _)| index)
                        .collect();
                    if !sets.contains(&indexes) {
                        sets.push(indexes);
                    }
                }
            }
        }
    }
    sets
}

/// The token values in effect where the rules at `matching_rule_indexes`
/// match (upstream `effectiveAdaptationTokenValues`): the root's tokens, its
/// aliases and local tokens, then each rule's tokens, conditional aliases and
/// local tokens in order, then each rule's authored status roles again, as the
/// stylesheet replays them so an explicit role beats a later rule's alias.
pub fn effective_adaptation_token_values(
    root_tokens: &CssMap,
    root_local_tokens: Option<&CssMap>,
    root_aliases: &[(String, String)],
    rules: &[AdaptationTokenWrites],
    matching_rule_indexes: &[usize],
    conditional_aliases: &[Vec<(String, String)>],
) -> CssMap {
    let aliases: CssMap = root_aliases.iter().cloned().collect();
    let mut effective =
        create_token_cascade(&[Some(root_tokens), Some(&aliases), root_local_tokens]);
    for &index in matching_rule_indexes {
        let rule = &rules[index];
        let conditional: CssMap = conditional_aliases
            .get(index)
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        assign_token_cascade(
            &mut effective,
            &[
                Some(&rule.tokens),
                Some(&conditional),
                rule.local_tokens.as_ref(),
            ],
        );
    }
    for &index in matching_rule_indexes {
        let authored: CssMap = authored_status_role_entries(&rules[index].tokens)
            .into_iter()
            .collect();
        assign_token_cascade(&mut effective, &[Some(&authored)]);
    }
    effective.values
}

/// The root status roles a theme leaves unset, in one canonical order
/// (upstream `generatedScopedStatusRoleDefaults`): the derived values of
/// [`derive_status_role_tokens`], then a `var(<source>)` alias for each
/// [`SCOPED_REFERENCE_DEFAULTS`] role the theme leaves unset whose source it
/// sets (or derives, or aliases just before), unless the alias would close a
/// reference cycle where any set of `rules` matches. A role the theme sets
/// keeps its own value.
pub fn generated_scoped_status_role_defaults(
    tokens: &CssMap,
    local_tokens: &CssMap,
    rules: &[AdaptationTokenWrites],
) -> Vec<(String, String)> {
    // Roles the theme leaves unset come from its own surfaces first. The
    // aliases below count them as set, so icon and border roles follow a
    // derived text role.
    let mut entries = derive_status_role_tokens(tokens, local_tokens);
    let derived: CssMap = entries.iter().cloned().collect();
    let authored_root =
        create_token_cascade(&[Some(tokens), Some(&derived), Some(local_tokens)]).values;
    let mut states: Vec<CssMap> = reachable_adaptation_match_sets(rules)
        .iter()
        .map(|indexes| {
            effective_adaptation_token_values(
                tokens,
                Some(local_tokens),
                &entries,
                rules,
                indexes,
                &[],
            )
        })
        .collect();

    for (role, source) in SCOPED_REFERENCE_DEFAULTS {
        let alias = format!("var({source})");
        if authored_root.contains_key(role)
            || (!authored_root.contains_key(source)
                && !entries.iter().any(|(emitted, _)| emitted == source))
        {
            continue;
        }
        let is_safe = states.iter().all(|values| {
            values.contains_key(role)
                || !token_values_would_have_cycle_with_value(values, role, &alias)
        });
        if is_safe {
            entries.push((role.to_string(), alias.clone()));
            for values in &mut states {
                if !values.contains_key(role) {
                    values.insert(role, alias.clone());
                }
            }
        }
    }
    // One canonical order whether a role is derived or aliased.
    entries.sort_by_key(|(role, _)| status_role_order(role));
    entries
}

/// The status roles `tokens` authors, in [`SCOPED_REFERENCE_DEFAULTS`] order,
/// read through the cascade (upstream `authoredStatusRoleEntries`).
pub fn authored_status_role_entries(tokens: &CssMap) -> Vec<(String, String)> {
    let canonical = create_token_cascade(&[Some(tokens)]).values;
    SCOPED_REFERENCE_DEFAULTS
        .iter()
        .filter_map(|(role, _)| {
            canonical
                .get(role)
                .map(|value| (role.to_string(), value.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::define::ThemeAdaptationWidthCondition;
    use super::*;

    fn css_map<const N: usize>(entries: [(&str, &str); N]) -> CssMap {
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // declarationBoundary.test.ts

    #[test]
    fn check_declaration_value_keeps_valid_css_inside_one_declaration() {
        for (label, value) in [
            ("plain color", "#FF00FF"),
            ("color function", "color-mix(in oklch, #FF00FF 80%, white)"),
            (
                "gradient",
                "linear-gradient(135deg, #FF00FF 0%, #00FFFF 100%)",
            ),
            ("calc", "calc(100% - 24px)"),
            ("light-dark", "light-dark(#fff, #111)"),
            ("font stack", "'Inter Var', ui-sans-serif, system-ui"),
            ("important flag", "red !important"),
            (
                "data URI, semicolon inside url()",
                "url(data:image/png;base64,iVBORw0KGgo=)",
            ),
            ("uppercase URL()", "URL(data:image/svg+xml;base64,PHN2Zz4=)"),
            (
                "uppercase URL with an inert brace",
                "URL(data:text/plain,a{b)",
            ),
            (
                "mixed-case Url() with spaces",
                "Url( data:image/png;base64,iVBORw0= )",
            ),
            (
                "quoted url with semicolon",
                "url(\"data:image/svg+xml;utf8,<svg/>\")",
            ),
            ("quoted url with a quoted brace", "url('x.png?a={1;2}')"),
            (
                "url nested in image-set",
                "image-set(url(a;b.png) 1x, url(c;d.png) 2x)",
            ),
            ("escaped space in identifier", r"Gill\ Sans, serif"),
            ("escaped semicolon in identifier", r"a\;b"),
            ("hex escape", r"\31 23"),
            ("escaped quote inside a string", r#""a\" ; b""#),
            ("semicolon inside a double-quoted string", "\"a;b\""),
            ("semicolon inside a single-quoted string", "'a;b'"),
            ("braces inside a string", "\"} body { color: red }\""),
            ("closed comment", "/* brand */ red"),
            ("closed comment with a semicolon inside", "red /* ; } */"),
            ("comment-like text inside a string", "\"/* not a comment\""),
            (
                "balanced brackets in a grid line name",
                "[full-start] 1fr [full-end]",
            ),
            ("semicolon nested in parentheses", "var(--x, a;b)"),
            ("brace nested in parentheses", "var(--x, {a})"),
            ("backslash before a newline is a delim", "a\\\nb"),
            ("whitespace including tab and newline", "a\t\nb"),
            ("CRLF string continuation", "\"a\\\r\nb\""),
            ("CRLF after a hex escape in a string", "\"\\61\r\nb\""),
            ("escaped url identifier", r"u\72l(data:image/png;base64,a)"),
            ("null is preprocessed by CSS", "red\u{0}"),
            ("control character inside a string", "\"a\u{1}b\""),
            (
                "control character inside a closed comment",
                "red /* \u{1} ; } */",
            ),
            ("empty value", ""),
        ] {
            assert_eq!(check_declaration_value(value), None, "keeps {label}");
        }
    }

    #[test]
    fn check_declaration_value_rejects_true_declaration_and_rule_breaks() {
        for (label, value, fragment) in [
            ("top-level semicolon", "red; background: url(x)", ";"),
            ("top-level closing brace", "red } body { color: blue", "}"),
            ("top-level opening brace", "red { color: blue }", "{"),
            (
                "escaped quote is not a string opener",
                r#"\"; background: red; ""#,
                ";",
            ),
            ("unbalanced closing paren", "red) ; body { color: blue", ")"),
            ("unbalanced closing bracket", "red] ; x", "]"),
            ("mismatched closer", "calc(1] ; x)", "]"),
            ("unclosed paren", "calc(100% - 24px", "("),
            ("unclosed bracket", "[full-start", "["),
            ("unclosed string", "\"open", "string"),
            ("unclosed comment", "red /* comment", "comment"),
            ("unclosed url", "url(x.png", "url("),
            ("trailing backslash", "red\\", "backslash"),
            (
                "newline inside a string (bad string)",
                "\"a\nb\"",
                "bad string",
            ),
            (
                "whitespace inside an unquoted url (bad url)",
                "url(a b)",
                "bad url",
            ),
            ("uppercase bad url", "URL(a b)", "bad url"),
            (
                "quote inside an unquoted url (bad url)",
                "url(a\"b)",
                "bad url",
            ),
            (
                "paren inside an unquoted url (bad url)",
                "url(a(b)",
                "bad url",
            ),
            (
                "stray backslash inside an unquoted url (bad url)",
                "url(a\\\nb)",
                "bad url",
            ),
            (
                "control character inside an unquoted url",
                "url(a\u{1}b)",
                "bad url",
            ),
        ] {
            let reason = check_declaration_value(value);
            assert!(
                reason.as_deref().is_some_and(|r| r.contains(fragment)),
                "rejects {label}: {reason:?}"
            );
        }
        // Upstream's exact wording.
        assert_eq!(
            check_declaration_value("calc(1] ; x)").as_deref(),
            Some("\"]\" does not close the open \"(\"")
        );
        assert_eq!(
            check_declaration_value("red; x").as_deref(),
            Some("an unquoted \";\" would end the declaration")
        );
    }

    #[test]
    fn analysis_finds_var_references_as_the_css_tokenizer_reads_them() {
        let references = |value: &str| analyze_declaration_value(value).references;
        let info = vec!["--color-text-info".to_string()];
        for value in [
            "VAR(--color-text-info)",
            "vAr(--color-text-info)",
            r"var(--\63 olor-text-info)",
            r"v\61r(--color-text-info)",
            r"var(\2d\2d color-text-info)",
            "var(/*ok*/ --color-text-info)",
            "url(/*) var(--color-text-info) /* */",
            "calc(var(--color-text-info, 1px) + 2px)",
        ] {
            assert_eq!(references(value), info, "{value}");
        }
        for value in [
            "var (--color-text-info)",
            "var/**/(--color-text-info)",
            "#var(--color-text-info)",
            "1var(--color-text-info)",
            "@var(--color-text-info)",
            "#123456 /* var(--color-text-info) */",
            "\"var(--color-text-info)\"",
        ] {
            assert_eq!(references(value), Vec::<String>::new(), "{value}");
        }
        // A value that cannot be one declaration references nothing.
        assert_eq!(
            analyze_declaration_value("var(--a); x").references,
            Vec::<String>::new()
        );
    }

    #[test]
    fn analysis_finds_important_and_a_whole_value_var() {
        let important = analyze_declaration_value("var(--a) !IMPORTANT");
        assert!(important.important);
        assert_eq!(important.single_var_reference.as_deref(), Some("--a"));
        let plain = analyze_declaration_value(" var( /*x*/ --a ) ");
        assert!(!plain.important);
        assert_eq!(plain.single_var_reference.as_deref(), Some("--a"));
        assert_eq!(
            analyze_declaration_value("var(--a) var(--b)").single_var_reference,
            None
        );
        assert_eq!(
            analyze_declaration_value("red !important").single_var_reference,
            None
        );
        assert!(analyze_declaration_value("red !important").important);
        assert!(!analyze_declaration_value("important").important);
    }

    #[test]
    fn declaration_names_canonicalize_their_escapes() {
        assert_eq!(
            canonicalize_declaration_name(r"--\62 rand").as_deref(),
            Some("--brand")
        );
        assert_eq!(
            canonicalize_declaration_name(r"--x\ y").as_deref(),
            Some("--x y")
        );
        assert_eq!(
            canonicalize_declaration_name(r"--x\\y").as_deref(),
            Some(r"--x\y")
        );
        assert_eq!(
            canonicalize_declaration_name(r"--\d800").as_deref(),
            Some("--\u{fffd}")
        );
        assert_eq!(
            canonicalize_declaration_name("--\0").as_deref(),
            Some("--\u{fffd}")
        );
        assert_eq!(
            canonicalize_declaration_name("color").as_deref(),
            Some("color")
        );
        for name in ["", "1px", "-1", "a b", "color;", "a\\"] {
            assert_eq!(canonicalize_declaration_name(name), None, "{name:?}");
        }
    }

    // defineTheme.test.ts: the generated status-role aliases

    fn aliases(tokens: CssMap, local: CssMap) -> Vec<(String, String)> {
        generated_scoped_status_role_defaults(&tokens, &local, &[])
    }

    fn has(entries: &[(String, String)], role: &str, value: &str) -> bool {
        entries.iter().any(|(r, v)| r == role && v == value)
    }

    const INFO_ALIAS: (&str, &str) = ("--color-text-info", "var(--color-text-blue)");

    #[test]
    fn does_not_generate_a_status_alias_that_would_create_a_cycle() {
        let entries = aliases(
            css_map([("--color-text-blue", "var(--color-text-info)")]),
            CssMap::new(),
        );
        assert!(!has(&entries, INFO_ALIAS.0, INFO_ALIAS.1));
    }

    #[test]
    fn checks_the_complete_generated_alias_graph_for_cycles() {
        let entries = aliases(
            css_map([
                ("--color-text-blue", "var(--color-text-success)"),
                ("--color-text-green", "var(--color-text-info)"),
            ]),
            CssMap::new(),
        );
        // The first alias is safe; the second would close a loop through it.
        assert!(has(&entries, INFO_ALIAS.0, INFO_ALIAS.1));
        assert!(!has(
            &entries,
            "--color-text-success",
            "var(--color-text-green)"
        ));
    }

    #[test]
    fn includes_theme_local_references_in_the_cycle_checks() {
        let entries = aliases(
            css_map([("--color-text-blue", "var(--local-info-loop)")]),
            css_map([("--local-info-loop", "var(--color-text-info)")]),
        );
        assert!(!has(&entries, INFO_ALIAS.0, INFO_ALIAS.1));
    }

    #[test]
    fn recognizes_every_spelling_of_a_reference_while_checking_cycles() {
        for (label, accent, local) in [
            ("uppercase VAR()", "VAR(--color-text-info)", None),
            ("mixed-case var()", "vAr(--color-text-info)", None),
            ("escaped identifier", r"var(--\63 olor-text-info)", None),
            ("escaped function name", r"v\61r(--color-text-info)", None),
            (
                "escaped leading hyphens",
                r"var(\2d\2d color-text-info)",
                None,
            ),
            (
                "comment-separated identifier",
                "var(/*ok*/ --color-text-info)",
                None,
            ),
            (
                "non-ASCII local token",
                "var(--品牌)",
                Some(("--品牌", "var(--color-text-info)")),
            ),
            (
                "canonicalized escaped key",
                r"var(--\62 rand)",
                Some((r"--\62 rand", "var(--color-text-info)")),
            ),
            (
                "escaped space key",
                r"var(--x\ y)",
                Some((r"--x\ y", "var(--color-text-info)")),
            ),
            (
                "escaped backslash key",
                r"var(--x\\y)",
                Some((r"--x\\y", "var(--color-text-info)")),
            ),
            (
                "CRLF-preprocessed escape",
                "v\\61\r\nr(--color-text-info)",
                None,
            ),
            (
                "surrogate escape",
                r"var(--\d800)",
                Some(("--\u{fffd}", "var(--color-text-info)")),
            ),
            (
                "NUL replacement",
                "var(--\0)",
                Some(("--\u{fffd}", "var(--color-text-info)")),
            ),
            (
                "unquoted URL contents",
                "url(/*) var(--color-text-info) /* */",
                None,
            ),
        ] {
            let local: CssMap = local.into_iter().map(|(k, v)| (k, v.to_string())).collect();
            let entries = aliases(css_map([("--color-text-blue", accent)]), local);
            assert!(!has(&entries, INFO_ALIAS.0, INFO_ALIAS.1), "{label}");
        }
    }

    #[test]
    fn text_that_is_no_reference_leaves_the_alias_safe() {
        for accent in [
            "var (--color-text-info)",
            "var/**/(--color-text-info)",
            "var(--color-text-red, #var(--color-text-info))",
            "1var(--color-text-info)",
            "@var(--color-text-info)",
            "#123456 /* var(--color-text-info) */",
        ] {
            let entries = aliases(
                css_map([
                    ("--color-text-blue", accent),
                    ("--color-text-red", "#123456"),
                ]),
                CssMap::new(),
            );
            assert!(has(&entries, INFO_ALIAS.0, INFO_ALIAS.1), "{accent}");
        }
    }

    #[test]
    fn ignores_unrelated_legacy_cycles() {
        let entries = aliases(
            css_map([
                ("--color-text-blue", "#123456"),
                ("--color-background-body", "var(--color-background-surface)"),
                ("--color-background-surface", "var(--color-background-body)"),
            ]),
            CssMap::new(),
        );
        assert!(has(&entries, INFO_ALIAS.0, INFO_ALIAS.1));
    }

    #[test]
    fn handles_long_comment_runs_wide_reference_lists_and_reconverging_graphs() {
        let comments = "/**/".repeat(10_000);
        aliases(
            css_map([("--color-text-blue", &format!("#123456 {comments}"))]),
            CssMap::new(),
        );
        let references = "var(--wide-target) ".repeat(200_000);
        aliases(
            css_map([("--color-text-blue", &references)]),
            css_map([("--wide-target", "#123456")]),
        );
        let mut local = CssMap::new();
        for layer in 0..26 {
            let value = if layer == 25 {
                "#ffffff".to_string()
            } else {
                format!("var(--layer-{0}-a, var(--layer-{0}-b))", layer + 1)
            };
            local.insert(format!("--layer-{layer}-a"), value.clone());
            local.insert(format!("--layer-{layer}-b"), value);
        }
        let started = std::time::Instant::now();
        aliases(css_map([("--color-text-blue", "var(--layer-0-a)")]), local);
        assert!(started.elapsed().as_secs() < 10);
    }

    #[test]
    fn emits_every_source_derived_status_role_in_canonical_order() {
        let entries = aliases(
            css_map([
                ("--color-text-blue", "#200001"),
                ("--color-text-green", "#200002"),
                ("--color-text-yellow", "#200003"),
                ("--color-text-red", "#200004"),
            ]),
            CssMap::new(),
        );
        let expected: Vec<(String, String)> = SCOPED_REFERENCE_DEFAULTS[..16]
            .iter()
            .map(|(role, source)| (role.to_string(), format!("var({source})")))
            .collect();
        assert_eq!(entries, expected);
    }

    #[test]
    fn derives_the_content_on_a_status_fill_the_theme_sets() {
        // INV17: a theme that sets only the error fill gets its on-fill,
        // text and on-muted roles as concrete light-dark() colors.
        let entries = aliases(css_map([("--color-error", "#FF0000")]), CssMap::new());
        let value = |role: &str| {
            entries
                .iter()
                .find(|(r, _)| r == role)
                .map(|(_, v)| v.as_str())
        };
        let is_pair = |value: Option<&str>| {
            value.is_some_and(|v| {
                let inner = v
                    .strip_prefix("light-dark(")
                    .and_then(|v| v.strip_suffix(')'));
                inner.is_some_and(|inner| {
                    inner.split(", ").all(|hex| {
                        hex.len() == 7
                            && hex.starts_with('#')
                            && hex[1..]
                                .chars()
                                .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
                    })
                })
            })
        };
        for role in [
            "--color-on-error",
            "--color-text-error",
            "--color-on-error-muted",
        ] {
            assert!(is_pair(value(role)), "{role}: {:?}", value(role));
        }
        // On pure red, core's light content (#FFFFFF) reaches 4.0:1 and its
        // dark content (#000000) 5.25:1 (WCAG luminance 0.2126 for red), so
        // the on role is the dark content in both modes.
        assert_eq!(
            value("--color-on-error"),
            Some("light-dark(#000000, #000000)")
        );
        // Icon and border follow the derived text role.
        assert_eq!(value("--color-icon-error"), Some("var(--color-text-error)"));
        assert_eq!(
            value("--color-border-error"),
            Some("var(--color-text-error)")
        );
        // The derived roles come first, the on role (not a scoped default) at
        // order 0 before the text role.
        assert_eq!(entries[0].0, "--color-on-error");
    }

    #[test]
    fn a_theme_that_sets_no_color_derives_nothing() {
        assert!(derive_status_role_tokens(&CssMap::new(), &CssMap::new()).is_empty());
        assert!(aliases(CssMap::new(), CssMap::new()).is_empty());
        // Authored roles are never derived or aliased.
        let entries = aliases(
            css_map([
                ("--color-success", "#3FB950"),
                ("--color-text-success", "#123456"),
            ]),
            CssMap::new(),
        );
        assert!(
            !entries
                .iter()
                .any(|(role, _)| role == "--color-text-success")
        );
        assert_eq!(
            entries
                .iter()
                .find(|(role, _)| role == "--color-icon-success")
                .map(|(_, v)| v.as_str()),
            Some("var(--color-text-success)")
        );
    }

    #[test]
    fn core_color_defaults_are_the_token_defaults_before_spacing() {
        let colors = color_defaults();
        assert_eq!(colors.len(), 98);
        assert_eq!(
            colors[0],
            ("--color-accent", "light-dark(#0064E0, #0171E3)")
        );
        assert!(colors.iter().all(|(name, _)| name.starts_with("--color-")));
    }

    #[test]
    fn resolve_modes_follows_light_dark_and_var_through_core_colors() {
        let [light, dark] = resolve_modes(&css_map([
            ("--x", "light-dark(var(--color-accent), #fff)"),
            ("--y", "var(--missing, #123456)"),
            ("--a", "var(--b)"),
            ("--b", "var(--a)"),
        ]));
        assert_eq!(light.get("--x").map(String::as_str), Some("#0064E0"));
        assert_eq!(dark.get("--x").map(String::as_str), Some("#fff"));
        assert_eq!(light.get("--y").map(String::as_str), Some("#123456"));
        assert_eq!(light.get("--a"), None);
        assert_eq!(
            dark.get("--color-accent").map(String::as_str),
            Some("#0171E3")
        );
    }

    #[test]
    fn the_cascade_keeps_important_values_and_skips_invalid_declarations() {
        let state = create_token_cascade(&[
            Some(&css_map([("--a", "red !important"), ("--b", "red")])),
            Some(&css_map([
                ("--a", "blue"),
                ("--b", "blue; x"),
                (r"--\63", "green"),
            ])),
        ]);
        assert_eq!(
            state.values.get("--a").map(String::as_str),
            Some("red !important")
        );
        assert_eq!(state.values.get("--b").map(String::as_str), Some("red"));
        assert_eq!(state.values.get("--c").map(String::as_str), Some("green"));
        assert!(state.important.contains("--a"));
    }

    #[test]
    fn a_candidate_value_closes_a_cycle_through_the_tokens_it_reaches() {
        let tokens = css_map([("--x", "var(--y)"), ("--y", "var(--role)")]);
        assert!(token_values_would_have_cycle_with_value(
            &tokens, "--role", "var(--x)"
        ));
        assert!(!token_values_would_have_cycle_with_value(
            &tokens, "--role", "var(--z)"
        ));
        assert!(token_values_would_have_cycle_with_value(
            &tokens,
            "--role",
            "var(--role)"
        ));
    }

    fn rule(when: ThemeAdaptationCondition, tokens: CssMap) -> AdaptationTokenWrites {
        AdaptationTokenWrites {
            when,
            tokens,
            local_tokens: None,
        }
    }

    #[test]
    fn match_sets_list_every_reachable_combination_of_rules_in_order() {
        assert_eq!(
            reachable_adaptation_match_sets(&[]),
            vec![Vec::<usize>::new()]
        );
        let coarse_narrow = ThemeAdaptationCondition {
            width: Some(ThemeAdaptationWidthCondition {
                from: None,
                below: Some("lg".into()),
            }),
            pointer: Some("coarse".into()),
            ..ThemeAdaptationCondition::default()
        };
        let more = ThemeAdaptationCondition {
            contrast: Some("more".into()),
            ..ThemeAdaptationCondition::default()
        };
        let rules = [
            rule(coarse_narrow, CssMap::new()),
            rule(more, CssMap::new()),
        ];
        // The first cell (narrowest, coarse, more contrast) matches both.
        assert_eq!(
            reachable_adaptation_match_sets(&rules),
            vec![vec![0, 1], vec![0], vec![1], vec![]]
        );
    }

    #[test]
    fn effective_values_replay_authored_roles_after_every_rule() {
        let rules = [
            rule(
                ThemeAdaptationCondition::default(),
                css_map([("--color-text-info", "#111111")]),
            ),
            rule(
                ThemeAdaptationCondition::default(),
                css_map([("--color-text-blue", "#222222")]),
            ),
        ];
        let conditional = vec![
            vec![],
            vec![(
                "--color-text-info".to_string(),
                "var(--color-text-blue)".to_string(),
            )],
        ];
        let values = effective_adaptation_token_values(
            &css_map([("--color-text-blue", "#000000")]),
            None,
            &[],
            &rules,
            &[0, 1],
            &conditional,
        );
        // The later rule's alias is replaced by the earlier rule's authored role.
        assert_eq!(
            values.get("--color-text-info").map(String::as_str),
            Some("#111111")
        );
        assert_eq!(
            values.get("--color-text-blue").map(String::as_str),
            Some("#222222")
        );
        assert_eq!(
            authored_status_role_entries(&css_map([
                ("--color-icon-info", "red"),
                ("--color-text-info", "blue")
            ])),
            vec![
                ("--color-text-info".to_string(), "blue".to_string()),
                ("--color-icon-info".to_string(), "red".to_string()),
            ]
        );
    }
}
