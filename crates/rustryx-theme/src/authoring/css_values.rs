//! CSS values read into typed tokens: upstream's native theme generator readers
//! (`packages/native/scripts/css-values.mjs`) and this repo's
//! `tools/export/lib/values.mjs`.
//!
//! Upstream's Swift theme generator reads one resolved CSS value for one mode
//! (core's resolver output) into a static value: a color as 8-bit channels, a
//! length in points, a font weight, a line-height multiplier or shadow layers.
//! The export that writes `spec/themes/*.json` reads values through the same
//! readers, then types them by token name ([`token_kind`]) as `values.mjs`
//! does. [`typed`] runs that last step and returns this crate's
//! [`TokenValue`], the value `crate::spec` builds from the JSON `typed()`
//! writes, so a theme defined at run time reads the same as a generated one.
//!
//! A conversion that loses something records a note; a value the readers can't
//! convert is a [`CssValueError`]. Messages are upstream's, word for word (some
//! name Swift, whose generator they come from), since callers and tests match
//! on them. Strings are indexed by byte, not by UTF-16 unit as upstream's are;
//! every character the readers look for is ASCII, so both read a value alike.

use std::borrow::Cow;
use std::f64::consts::{E, PI};
use std::fmt;
use std::time::Duration;

use gpui::{AbsoluteLength, BoxShadow, Edges, FontWeight, Hsla, Point, px, rems};

use super::js::{
    is_js_whitespace, js_hypot, js_max, js_min, js_number_to_string, js_parse_float, js_round,
    js_to_hex_string, js_trim,
};
use super::token_values::ResolvedThemeMode;
use crate::types::{CubicBezier, FontStack, Keyword, Shadow, TokenValue};

/// A value the readers can't convert. Upstream `CssValueError`; the plain
/// `Error`s `values.mjs` throws are this too, with `has_no_static_form` false.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CssValueError {
    /// Why, in plain words: upstream's message.
    pub reason: String,
    /// True when CSS itself gives the value no static form (it depends on the
    /// page or on where the token is used, such as `var()` of an unknown token
    /// or `currentColor`); false when it is a form the readers do not read.
    pub has_no_static_form: bool,
}

impl fmt::Display for CssValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for CssValueError {}

/// A reader's result.
pub type CssResult<T> = Result<T, CssValueError>;

/// A form the readers do not read. Upstream `fail`.
fn fail<T>(reason: impl Into<String>) -> CssResult<T> {
    Err(CssValueError {
        reason: reason.into(),
        has_no_static_form: false,
    })
}

/// A value CSS gives no static form. Upstream `noStaticForm`.
fn no_static_form<T>(reason: impl Into<String>) -> CssResult<T> {
    Err(CssValueError {
        reason: reason.into(),
        has_no_static_form: true,
    })
}

/// The root font size `rem` and `em` lengths are read against, in points.
/// Upstream `ROOT_FONT_SIZE`.
pub const ROOT_FONT_SIZE: f64 = 16.0;

/// The note for a color outside Display P3, clipped as browsers draw it.
/// Upstream `OUTSIDE_P3`.
pub const OUTSIDE_P3: &str = "outside Display P3; clipped to match the web";

const REC2020_ALPHA: f64 = 1.09929682680944;
const REC2020_BETA: f64 = 0.018053968510807;

// =============================================================================
// Mode: light-dark() anywhere in a value
// =============================================================================

/// The byte index of the `)` that closes the `(` at `open`. Upstream `closing`,
/// which answers -1 for `None`.
fn closing(text: &str, open: usize) -> Option<usize> {
    let mut depth: i64 = 0;
    for (i, byte) in text.bytes().enumerate().skip(open) {
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

/// `text` split at top-level commas (outside parentheses), each part trimmed
/// and empty parts kept. Upstream `splitTopLevel` (`css-values.mjs`).
fn css_split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i64 = 0;
    let mut start = 0;
    for (i, byte) in text.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(js_trim(&text[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(js_trim(&text[start..]));
    parts
}

/// The value in one mode: every `light-dark(a, b)`, at any depth and in any
/// letter case, becomes `a` in light and `b` in dark (upstream
/// `valueForMode`). `light-dark(#000, #fff)` is `#000` in light mode;
/// `color-mix(in srgb, light-dark(red, blue), white)` is
/// `color-mix(in srgb, blue, white)` in dark mode.
pub fn value_for_mode(value: &str, mode: ResolvedThemeMode) -> CssResult<String> {
    const OPEN: &str = "light-dark(";
    // ASCII lowercasing keeps byte offsets, and a non-Unicode regex's /i folds
    // only ASCII letters to ASCII.
    let folded = value.to_ascii_lowercase();
    let mut out = String::new();
    let mut from = 0;
    while let Some(found) = folded[from..].find(OPEN) {
        let at = from + found;
        let open = at + OPEN.len() - 1;
        let Some(close) = closing(value, open) else {
            return fail("it has an unclosed light-dark()");
        };
        let sides = css_split_top_level(&value[open + 1..close]);
        if sides.len() != 2 {
            return fail("light-dark() needs exactly two values");
        }
        let side = match mode {
            ResolvedThemeMode::Light => sides[0],
            ResolvedThemeMode::Dark => sides[1],
        };
        out.push_str(&value[from..at]);
        out.push_str(&value_for_mode(side, mode)?);
        from = close + 1;
    }
    out.push_str(&value[from..]);
    Ok(out)
}

// =============================================================================
// Tokens
// =============================================================================

/// One token of a CSS value. Upstream's `{t: 'num' | 'id' | 'hash' | 'fn' |
/// 'op'}` objects.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// A number and its unit, lowercased (`""` for none, `"%"`).
    Num { v: f64, u: String },
    /// An identifier, lowercased.
    Id(String),
    /// The characters after a `#`, as written.
    Hash(String),
    /// A function (lowercased name, `""` for a bare `( )` group) and its body.
    Fn { name: String, body: Body },
    /// `,` `/` `+` `-` or `*`; `spaced` when whitespace is on both sides.
    Op { v: char, spaced: bool },
}

/// A function's body: tokens, or for `var()`, `env()`, `attr()` and `url()`
/// the text as written, which is not a value to tokenize.
#[derive(Clone, Debug, PartialEq)]
enum Body {
    Tokens(Vec<Tok>),
    Opaque(String),
}

/// The text a token's `v` holds, for the tokens whose `v` is a string.
fn text_of(tok: &Tok) -> Option<&str> {
    match tok {
        Tok::Id(v) | Tok::Hash(v) => Some(v),
        _ => None,
    }
}

/// The length of the number at the start of `s`:
/// `/^[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?/i`.
fn number_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let digits_from = |mut at: usize| {
        while at < b.len() && b[at].is_ascii_digit() {
            at += 1;
        }
        at
    };
    let mut at = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let integer_end = digits_from(at);
    if integer_end > at {
        at = integer_end;
        if b.get(at) == Some(&b'.') {
            at = digits_from(at + 1);
        }
    } else if b.get(at) == Some(&b'.') && digits_from(at + 1) > at + 1 {
        at = digits_from(at + 1);
    } else {
        return None;
    }
    if matches!(b.get(at), Some(b'e' | b'E')) {
        let mut exponent = at + 1;
        if matches!(b.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        let exponent_end = digits_from(exponent);
        if exponent_end > exponent {
            at = exponent_end;
        }
    }
    Some(at)
}

/// The length of the identifier at the start of `s`: `/^-?-?[a-z_][\w-]*/i`.
fn ident_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let dashes = b.iter().take(2).take_while(|&&c| c == b'-').count();
    if !b
        .get(dashes)
        .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
    {
        return None;
    }
    let mut at = dashes + 1;
    while at < b.len() && (b[at].is_ascii_alphanumeric() || b[at] == b'_' || b[at] == b'-') {
        at += 1;
    }
    Some(at)
}

/// Whether the character before byte `at` is JavaScript whitespace, counting
/// the start of the text as whitespace (upstream's `text[i - 1] ?? ' '`).
fn space_before(text: &str, at: usize) -> bool {
    text[..at].chars().next_back().is_none_or(is_js_whitespace)
}

/// Whether the character at byte `at` is JavaScript whitespace, counting the
/// end of the text as whitespace (upstream's `text[i + 1] ?? ' '`).
fn space_at(text: &str, at: usize) -> bool {
    text.get(at..)
        .and_then(|rest| rest.chars().next())
        .is_none_or(is_js_whitespace)
}

/// A CSS value as tokens (upstream `tokenize`): numbers with their units,
/// identifiers, `#` colors, functions with their bodies tokenized (the bodies
/// of `var()`, `env()`, `attr()` and `url()` kept as text), bare `( )` groups,
/// and `,` `/` `+` `-` `*` as operators. A sign right after a value is an
/// operator, not part of a number, so `calc(1rem-2px)` has a `-` to reject.
fn tokenize(text: &str) -> CssResult<Vec<Tok>> {
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0;
    while let Some(c) = text[i..].chars().next() {
        let rest = &text[i..];
        if is_js_whitespace(c) {
            i += c.len_utf8();
            continue;
        }
        let sign_is_operator = matches!(c, '+' | '-')
            && !matches!(out.last(), Some(Tok::Op { .. }))
            && !space_before(text, i);
        if let Some(len) = number_len(rest).filter(|_| !sign_is_operator) {
            let v = js_parse_float(&rest[..len]);
            i += len;
            let after = &text[i..];
            let unit_len = if after.starts_with('%') {
                1
            } else {
                after.bytes().take_while(u8::is_ascii_alphabetic).count()
            };
            let u = after[..unit_len].to_ascii_lowercase();
            i += unit_len;
            out.push(Tok::Num { v, u });
        } else if let Some(len) = ident_len(rest) {
            let ident = &rest[..len];
            i += len;
            if text[i..].starts_with('(') {
                let Some(close) = closing(text, i) else {
                    return fail(format!("it has an unclosed {ident}()"));
                };
                let name = ident.to_ascii_lowercase();
                let inner = &text[i + 1..close];
                let body = if matches!(name.as_str(), "var" | "env" | "attr" | "url") {
                    Body::Opaque(inner.to_string())
                } else {
                    Body::Tokens(tokenize(inner)?)
                };
                out.push(Tok::Fn { name, body });
                i = close + 1;
            } else {
                out.push(Tok::Id(ident.to_ascii_lowercase()));
            }
        } else if c == '#' {
            let len = rest[1..]
                .bytes()
                .take_while(u8::is_ascii_alphanumeric)
                .count();
            if len == 0 {
                return fail(format!("\"{rest}\" is not CSS"));
            }
            out.push(Tok::Hash(rest[1..=len].to_string()));
            i += 1 + len;
        } else if c == '(' {
            let Some(close) = closing(text, i) else {
                return fail("it has an unclosed (");
            };
            out.push(Tok::Fn {
                name: String::new(),
                body: Body::Tokens(tokenize(&text[i + 1..close])?),
            });
            i = close + 1;
        } else if matches!(c, ',' | '/' | '+' | '-' | '*') {
            // calc() reads + and - as operators only with a space on each side.
            let spaced = space_before(text, i) && space_at(text, i + 1);
            out.push(Tok::Op { v: c, spaced });
            i += 1;
        } else {
            return fail(format!("\"{c}\" is not part of a value Swift can read"));
        }
    }
    Ok(out)
}

/// Tokens split at `,` operators. Upstream `commas`.
fn commas(tokens: &[Tok]) -> Vec<&[Tok]> {
    tokens
        .split(|tok| matches!(tok, Tok::Op { v: ',', .. }))
        .collect()
}

// =============================================================================
// Numbers and math functions
// =============================================================================

/// What a number measures. Upstream's `k`: `'number'`, `'length'` (in px),
/// `'angle'` (in degrees) or `'percent'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Number,
    Length,
    Angle,
    Percent,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Kind::Number => "number",
            Kind::Length => "length",
            Kind::Angle => "angle",
            Kind::Percent => "percent",
        })
    }
}

/// A number with its kind. Upstream's `{v, k}`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Quantity {
    v: f64,
    k: Kind,
}

/// Points per unit. Upstream `LENGTH_UNITS`.
fn length_unit(unit: &str) -> Option<f64> {
    Some(match unit {
        "px" => 1.0,
        "rem" | "em" => ROOT_FONT_SIZE,
        "pt" => 4.0 / 3.0,
        "pc" => 16.0,
        "in" => 96.0,
        "cm" => 96.0 / 2.54,
        "mm" => 96.0 / 25.4,
        "q" => 96.0 / 101.6,
        _ => return None,
    })
}

/// Degrees per unit. Upstream `ANGLE_UNITS`.
fn angle_unit(unit: &str) -> Option<f64> {
    Some(match unit {
        "deg" => 1.0,
        "grad" => 0.9,
        "rad" => 180.0 / PI,
        "turn" => 360.0,
        _ => return None,
    })
}

/// Why a unit has no static form, or `None` for a unit that is simply not
/// read. Upstream `CONTEXT_UNITS` and the patterns after it.
fn context_unit(unit: &str) -> Option<&'static str> {
    const PERCENT: &str = "a percentage depends on the box it is used in";
    const VIEWPORT: &str = "viewport units depend on the screen";
    const FONT: &str = "font-relative units depend on the font in use";
    match unit {
        "%" => return Some(PERCENT),
        "vh" | "vw" | "vmin" | "vmax" => return Some(VIEWPORT),
        "ex" | "ch" | "lh" => return Some(FONT),
        _ => {}
    }
    // /^(d|s|l)?v(h|w|i|b|min|max)$|^cq/
    let viewport = unit
        .strip_prefix(['d', 's', 'l'])
        .unwrap_or(unit)
        .strip_prefix('v')
        .is_some_and(|axis| matches!(axis, "h" | "w" | "i" | "b" | "min" | "max"));
    if viewport || unit.starts_with("cq") {
        return Some("viewport and container units depend on the screen");
    }
    // /^r?(ex|ch|ic|cap|lh)$/
    let font = |u: &str| matches!(u, "ex" | "ch" | "ic" | "cap" | "lh");
    if font(unit) || unit.strip_prefix('r').is_some_and(font) {
        return Some(FONT);
    }
    None
}

/// The channel keywords of a relative color (`rgb(from …  r g b)`): the
/// origin's coordinates in the function's terms, and its alpha.
#[derive(Clone, Copy, Debug)]
struct Vars {
    names: [&'static str; 3],
    coords: [f64; 3],
    alpha: f64,
}

impl Vars {
    fn get(&self, name: &str) -> Option<f64> {
        if name == "alpha" {
            return Some(self.alpha);
        }
        self.names
            .iter()
            .position(|n| *n == name)
            .map(|at| self.coords[at])
    }
}

/// A number with its kind (upstream `quantity`): a number with a unit,
/// `calc()`/`min()`/`max()`/`clamp()` or a bare `( )` group over them, `pi`,
/// `e`, or a relative color's channel keyword (`vars`; a missing channel counts
/// as 0). An `em` counts against `em` px when given, else against the root font
/// size, with a note.
fn quantity(
    tok: &Tok,
    vars: Option<&Vars>,
    notes: &mut Vec<String>,
    em: Option<f64>,
) -> CssResult<Quantity> {
    match tok {
        Tok::Num { v, u } => {
            let v = *v;
            match u.as_str() {
                "" => {
                    return Ok(Quantity { v, k: Kind::Number });
                }
                "%" => {
                    return Ok(Quantity {
                        v,
                        k: Kind::Percent,
                    });
                }
                _ => {}
            }
            if let Some(scale) = length_unit(u) {
                if u == "em" {
                    if let Some(em) = em {
                        return Ok(Quantity {
                            v: v * em,
                            k: Kind::Length,
                        });
                    }
                    if v != 0.0 {
                        notes.push(format!(
                            "em read against the {}px root font size",
                            js_number_to_string(ROOT_FONT_SIZE)
                        ));
                    }
                }
                return Ok(Quantity {
                    v: v * scale,
                    k: Kind::Length,
                });
            }
            if let Some(scale) = angle_unit(u) {
                return Ok(Quantity {
                    v: v * scale,
                    k: Kind::Angle,
                });
            }
            if let Some(context) = context_unit(u) {
                return no_static_form(context);
            }
            fail(format!("\"{u}\" is not a unit Swift can read"))
        }
        Tok::Id(name) => {
            if let Some(v) = vars.and_then(|vars| vars.get(name)) {
                return Ok(Quantity {
                    v: if v.is_nan() { 0.0 } else { v },
                    k: Kind::Number,
                });
            }
            match name.as_str() {
                "pi" => Ok(Quantity {
                    v: PI,
                    k: Kind::Number,
                }),
                "e" => Ok(Quantity {
                    v: E,
                    k: Kind::Number,
                }),
                _ => {
                    dynamic(tok)?;
                    fail(format!("\"{}\" is not a number", show(tok)))
                }
            }
        }
        Tok::Fn { name, body } => {
            dynamic(tok)?;
            math(name, body, vars, notes, em)
        }
        _ => {
            dynamic(tok)?;
            fail(format!("\"{}\" is not a number", show(tok)))
        }
    }
}

/// `calc()`, `min()`, `max()`, `clamp()` or a bare `( )` group, evaluated.
/// Upstream `math`.
fn math(
    name: &str,
    body: &Body,
    vars: Option<&Vars>,
    notes: &mut Vec<String>,
    em: Option<f64>,
) -> CssResult<Quantity> {
    let tokens = match body {
        Body::Tokens(tokens) => tokens,
        // Only url() gets here with a text body. Upstream walks the text's
        // characters as tokens, none of which is a number.
        Body::Opaque(text) if text.is_empty() => return fail("an expression is empty"),
        Body::Opaque(_) => return fail("\"undefined\" is not a number"),
    };
    let mut args = Vec::new();
    for part in commas(tokens) {
        args.push(sum(part, vars, notes, em)?);
    }
    let same = || -> CssResult<Kind> {
        let mut kinds: Vec<Kind> = Vec::new();
        for arg in &args {
            if !kinds.contains(&arg.k) {
                kinds.push(arg.k);
            }
        }
        if kinds.len() > 1 {
            let kinds: Vec<String> = kinds.iter().map(Kind::to_string).collect();
            return fail(format!("{name}() mixes {}", kinds.join(" and ")));
        }
        Ok(args[0].k)
    };
    match name {
        "" | "calc" => {
            if args.len() != 1 {
                let shown = if name.is_empty() { "( )" } else { name };
                return fail(format!("{shown} takes one expression"));
            }
            Ok(args[0])
        }
        "min" => Ok(Quantity {
            v: args.iter().fold(f64::INFINITY, |m, a| js_min(m, a.v)),
            k: same()?,
        }),
        "max" => Ok(Quantity {
            v: args.iter().fold(f64::NEG_INFINITY, |m, a| js_max(m, a.v)),
            k: same()?,
        }),
        "clamp" => {
            if args.len() != 3 {
                return fail("clamp() takes three values");
            }
            Ok(Quantity {
                v: js_max(args[0].v, js_min(args[1].v, args[2].v)),
                k: same()?,
            })
        }
        _ => fail(format!("{name}() is not a function Swift can read")),
    }
}

/// One factor or more joined by `*` and `/`, starting at `tokens[*at]`.
/// Upstream's `product` in `sum`.
fn product(
    tokens: &[Tok],
    at: &mut usize,
    vars: Option<&Vars>,
    notes: &mut Vec<String>,
    em: Option<f64>,
) -> CssResult<Quantity> {
    // Upstream reads past the end here (a TypeError); this says so instead.
    let Some(first) = tokens.get(*at) else {
        return fail("an expression ends early");
    };
    *at += 1;
    let mut left = quantity(first, vars, notes, em)?;
    while let Some(&Tok::Op {
        v: op @ ('*' | '/'),
        ..
    }) = tokens.get(*at)
    {
        *at += 1;
        let Some(next) = tokens.get(*at) else {
            return fail("an expression ends early");
        };
        *at += 1;
        let right = quantity(next, vars, notes, em)?;
        if op == '*' {
            if left.k != Kind::Number && right.k != Kind::Number {
                return fail("calc() multiplies two dimensions");
            }
            left = Quantity {
                v: left.v * right.v,
                k: if left.k == Kind::Number {
                    right.k
                } else {
                    left.k
                },
            };
        } else {
            if right.k != Kind::Number || right.v == 0.0 {
                return fail("calc() divides by a dimension or by zero");
            }
            left = Quantity {
                v: left.v / right.v,
                k: left.k,
            };
        }
    }
    Ok(left)
}

/// An expression as `calc()` reads it: `+ -` over `* /` over quantities, and a
/// `+` or `-` only with a space on each side. Upstream `sum`.
fn sum(
    tokens: &[Tok],
    vars: Option<&Vars>,
    notes: &mut Vec<String>,
    em: Option<f64>,
) -> CssResult<Quantity> {
    if tokens.is_empty() {
        return fail("an expression is empty");
    }
    let mut at = 0;
    let mut total = product(tokens, &mut at, vars, notes, em)?;
    while at < tokens.len() {
        let op = &tokens[at];
        at += 1;
        let &Tok::Op {
            v: op @ ('+' | '-'),
            spaced,
        } = op
        else {
            return fail("calc() is missing an operator");
        };
        if !spaced {
            return fail(format!("calc() needs a space on each side of {op}"));
        }
        let right = product(tokens, &mut at, vars, notes, em)?;
        if right.k != total.k {
            return no_static_form(format!("calc() adds {} and {}", total.k, right.k));
        }
        total = Quantity {
            v: if op == '+' {
                total.v + right.v
            } else {
                total.v - right.v
            },
            k: total.k,
        };
    }
    Ok(total)
}

/// Whether a value is a CSS-wide keyword:
/// `/^(inherit|initial|unset|revert|revert-layer)$/`.
fn is_css_wide_keyword(value: &str) -> bool {
    matches!(
        value,
        "inherit" | "initial" | "unset" | "revert" | "revert-layer"
    )
}

/// Whether the character before byte `at` leaves a word boundary before a
/// word character there (`\b`): the start of the text, or a character that is
/// not `[A-Za-z0-9_]`.
fn word_boundary_before(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'))
}

/// Fails when a `font-family` value has no static form (upstream
/// `checkFamilyIsStatic`): a CSS-wide keyword, or `var()`, `env()` or
/// `attr()` outside a quoted name. A CSS-wide keyword as one name in a list is
/// not valid CSS, so it fails too.
pub fn check_family_is_static(value: &str) -> CssResult<()> {
    // value.replace(/"[^"]*"|'[^']*'/g, '""')
    let mut blanked = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(c) = rest.chars().next() {
        if matches!(c, '"' | '\'')
            && let Some(end) = rest[1..].find(c)
        {
            blanked.push_str("\"\"");
            rest = &rest[end + 2..];
            continue;
        }
        blanked.push(c);
        rest = &rest[c.len_utf8()..];
    }
    let bare = js_trim(&blanked).to_lowercase();
    if is_css_wide_keyword(&bare) {
        return no_static_form(format!("{bare} depends on where the token is used"));
    }
    // /\b(env|attr)\(/
    for (at, _) in bare.char_indices() {
        if !word_boundary_before(&bare, at) {
            continue;
        }
        for (name, open) in [("env", "env("), ("attr", "attr(")] {
            if bare[at..].starts_with(open) {
                return no_static_form(format!("{name}() is known only on the page"));
            }
        }
    }
    // /\bvar\(([^)]*)\)/
    for (at, _) in bare.char_indices() {
        if word_boundary_before(&bare, at)
            && bare[at..].starts_with("var(")
            && let Some(end) = bare[at + 4..].find(')')
        {
            let inner = &bare[at + 4..at + 4 + end];
            return no_static_form(format!("var({inner}) is not a token core's resolver knows"));
        }
    }
    if let Some(listed) = bare
        .split(',')
        .map(js_trim)
        .find(|name| is_css_wide_keyword(name))
    {
        return fail(format!("{listed} cannot be one name in a font-family list"));
    }
    Ok(())
}

/// Fails for a token that can only be known where it is used (`var()`,
/// `env()`, `attr()`, a CSS-wide keyword). Upstream `dynamic`, which answers
/// `null` for `Ok`.
fn dynamic(tok: &Tok) -> CssResult<()> {
    match tok {
        Tok::Fn { name, body } if name == "var" => {
            let inner = match body {
                Body::Opaque(text) => text.as_str(),
                Body::Tokens(_) => "",
            };
            no_static_form(format!("var({inner}) is not a token core's resolver knows"))
        }
        Tok::Fn { name, .. } if name == "env" || name == "attr" => {
            no_static_form(format!("{name}() is known only on the page"))
        }
        Tok::Id(v) if is_css_wide_keyword(v) => {
            no_static_form(format!("{v} depends on where the token is used"))
        }
        _ => Ok(()),
    }
}

/// A token as an error message shows it. Upstream `show`.
fn show(tok: &Tok) -> String {
    match tok {
        Tok::Num { v, u } => format!("{}{u}", js_number_to_string(*v)),
        Tok::Hash(v) => format!("#{v}"),
        Tok::Fn { name, .. } => format!("{name}()"),
        Tok::Id(v) => v.clone(),
        Tok::Op { v, .. } => v.to_string(),
    }
}

// =============================================================================
// Lengths, weights, line heights
// =============================================================================

/// One quantity from a whole value. Upstream `single`.
fn single(value: &str, notes: &mut Vec<String>, em: Option<f64>) -> CssResult<Quantity> {
    let tokens = tokenize(value)?;
    let [tok] = tokens.as_slice() else {
        return fail(format!("\"{value}\" is not one value"));
    };
    quantity(tok, None, notes, em)
}

/// The border-width keywords' lengths. Upstream `BORDER_KEYWORDS`.
fn border_keyword(value: &str) -> Option<f64> {
    Some(match value {
        "thin" => 1.0,
        "medium" => 3.0,
        "thick" => 5.0,
        _ => return None,
    })
}

/// A length in points (upstream `toPoints`): px × 1, rem and em × 16 (an em
/// with a note), pt × 4/3, the other absolute units, and `calc()`, `min()`,
/// `max()` and `clamp()` over them; a bare number reads as points. `thin`,
/// `medium` and `thick` are lengths only for a border width.
///
/// `0.875rem` is 14 and `calc((1rem - 2px) * 2)` is 28; a percentage, a
/// viewport unit or a font-relative one has no static form.
pub fn to_points(value: &str, notes: &mut Vec<String>, border_width: bool) -> CssResult<f64> {
    let keyword = js_trim(value).to_lowercase();
    if let Some(points) = border_keyword(&keyword) {
        if border_width {
            return Ok(points);
        }
        return fail(format!(
            "{keyword} names a border width; here it has no length"
        ));
    }
    let q = single(value, notes, None)?;
    match q.k {
        Kind::Percent => no_static_form("a percentage depends on the box it is used in"),
        Kind::Length | Kind::Number => Ok(q.v),
        Kind::Angle => fail("a angle is not a length"),
    }
}

/// A font weight from 1 to 1000 (upstream `toWeight`): `normal` is 400 and
/// `bold` 700; `bolder` and `lighter` read against the root's 400, as 700 and
/// 100, with a note.
pub fn to_weight(value: &str, notes: &mut Vec<String>) -> CssResult<f64> {
    let keyword = js_trim(value).to_lowercase();
    match keyword.as_str() {
        "normal" => return Ok(400.0),
        "bold" => return Ok(700.0),
        "bolder" | "lighter" => {
            notes.push(format!(
                "{keyword} read against normal (400), the root's inherited weight"
            ));
            return Ok(if keyword == "bolder" { 700.0 } else { 100.0 });
        }
        _ => {}
    }
    let q = single(value, notes, None)?;
    // Upstream's comparisons, kept so a NaN weight passes as it does there.
    #[allow(clippy::manual_range_contains)]
    if q.k != Kind::Number || q.v < 1.0 || q.v > 1000.0 {
        return fail("a font weight is a number from 1 to 1000");
    }
    Ok(q.v)
}

/// `Number(x.toPrecision(digits))`: `x` to `digits` significant digits, an
/// exact tie going to the larger magnitude as JavaScript breaks it (Rust's
/// `{:e}` breaks it to even).
fn js_to_precision(x: f64, digits: usize) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    if !x.is_finite() {
        return x;
    }
    // The exact decimal expansion: a double's runs to at most 767 digits.
    let exact = format!("{:.1100e}", x.abs());
    let (mantissa, exponent) = exact.split_once('e').unwrap_or((&exact, "0"));
    let exponent: i64 = exponent.parse().unwrap_or(0);
    let all: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let mut kept = all[..digits].to_vec();
    if all[digits] >= b'5' {
        let mut at = digits;
        loop {
            if at == 0 {
                kept.insert(0, b'1');
                break;
            }
            at -= 1;
            if kept[at] == b'9' {
                kept[at] = b'0';
            } else {
                kept[at] += 1;
                break;
            }
        }
    }
    let text = format!(
        "{}e{}",
        String::from_utf8_lossy(&kept),
        exponent - (digits as i64 - 1)
    );
    let rounded: f64 = text.parse().unwrap_or(f64::NAN);
    if x < 0.0 { -rounded } else { rounded }
}

/// A line height as a multiple of the font size (upstream `toLeading`): a
/// number as is, a percentage ÷ 100, a length ÷ `font_size` (points, rounded to
/// 12 significant digits), so an `em` counts against the style's own size.
/// `normal` depends on the font, so it reads as 1.2 with a note.
///
/// With a 14-point size, `1.5`, `150%`, `21px` and `calc(1em + 7px)` are all
/// 1.5. A zero or NaN `font_size` counts as none, as JavaScript's `||` has it.
pub fn to_leading(value: &str, font_size: Option<f64>, notes: &mut Vec<String>) -> CssResult<f64> {
    if js_trim(value).to_lowercase() == "normal" {
        notes.push("normal depends on the font; read as 1.2".to_string());
        return Ok(1.2);
    }
    let font_size = font_size.filter(|size| *size != 0.0 && !size.is_nan());
    let q = single(value, notes, font_size)?;
    match q.k {
        Kind::Number => Ok(q.v),
        Kind::Percent => Ok(q.v / 100.0),
        Kind::Length => match font_size {
            Some(size) => Ok(js_to_precision(q.v / size, 12)),
            None => fail("a length line height needs its style's font size"),
        },
        Kind::Angle => fail("a angle is not a line height"),
    }
}

// =============================================================================
// Color spaces
// =============================================================================

type Matrix = [[f64; 3]; 3];

/// A matrix times a vector. Upstream `mul`.
fn mul(m: &Matrix, [a, b, c]: [f64; 3]) -> [f64; 3] {
    m.map(|r| r[0] * a + r[1] * b + r[2] * c)
}

const D50_WHITE: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
const D65_TO_D50: Matrix = [
    [
        1.0479297925449969,
        0.022946870601609652,
        -0.05019226628920524,
    ],
    [
        0.02962780877005599,
        0.9904344267538799,
        -0.017073799063418826,
    ],
    [
        -0.009243040646204504,
        0.015055191490298152,
        0.7518742814281371,
    ],
];
const D50_TO_D65: Matrix = [
    [0.955473421488075, -0.02309845494876471, 0.06325924320057072],
    [
        -0.0283697093338637,
        1.0099953980813041,
        0.021041441191917323,
    ],
    [
        0.012314014864481998,
        -0.020507649298898964,
        1.330365926242124,
    ],
];

/// `f` applied to the magnitude, keeping the sign. Upstream `sign`.
fn sign(c: f64, f: impl Fn(f64) -> f64) -> f64 {
    if c < 0.0 { -f(-c) } else { f(c) }
}

/// An RGB space's transfer function (CSS Color 4, section 18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Trc {
    Srgb,
    Linear,
    A98,
    Prophoto,
    /// The BT.2020 piecewise curve, as Chromium draws `rec2020`; CSS Color 4
    /// specifies a 2.4 gamma.
    Rec2020,
}

impl Trc {
    /// Upstream's `toLinear`: an encoded channel as linear light.
    fn to_linear(self, c: f64) -> f64 {
        match self {
            Trc::Srgb => sign(c, |x| {
                if x <= 0.04045 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                }
            }),
            Trc::Linear => c,
            Trc::A98 => sign(c, |x| x.powf(563.0 / 256.0)),
            Trc::Prophoto => sign(c, |x| {
                if x <= 16.0 / 512.0 {
                    x / 16.0
                } else {
                    x.powf(1.8)
                }
            }),
            Trc::Rec2020 => sign(c, |x| {
                if x < REC2020_BETA * 4.5 {
                    x / 4.5
                } else {
                    ((x + REC2020_ALPHA - 1.0) / REC2020_ALPHA).powf(1.0 / 0.45)
                }
            }),
        }
    }

    /// Upstream's `fromLinear`: linear light back to the space's encoding.
    #[allow(clippy::wrong_self_convention)]
    fn from_linear(self, c: f64) -> f64 {
        match self {
            Trc::Srgb => sign(c, |x| {
                if x <= 0.0031308 {
                    x * 12.92
                } else {
                    1.055 * x.powf(1.0 / 2.4) - 0.055
                }
            }),
            Trc::Linear => c,
            Trc::A98 => sign(c, |x| x.powf(256.0 / 563.0)),
            Trc::Prophoto => sign(c, |x| {
                if x >= 1.0 / 512.0 {
                    x.powf(1.0 / 1.8)
                } else {
                    16.0 * x
                }
            }),
            Trc::Rec2020 => sign(c, |x| {
                if x > REC2020_BETA {
                    REC2020_ALPHA * x.powf(0.45) - (REC2020_ALPHA - 1.0)
                } else {
                    x * 4.5
                }
            }),
        }
    }
}

/// An RGB space: its transfer function, linear → XYZ matrix and inverse, and
/// whether the matrix lands on the D50 white (else D65). Upstream
/// `RGB_SPACES`.
struct RgbSpace {
    trc: Trc,
    d50: bool,
    to_xyz: Matrix,
    from_xyz: Matrix,
}

const SRGB_TO_XYZ: Matrix = [
    [0.41239079926595934, 0.357584339383878, 0.1804807884018343],
    [0.21263900587151027, 0.715168678767756, 0.07219231536073371],
    [0.01933081871559182, 0.11919477979462598, 0.9505321522496607],
];
const XYZ_TO_SRGB: Matrix = [
    [3.2409699419045226, -1.537383177570094, -0.4986107602930034],
    [-0.9692436362808796, 1.8759675015077202, 0.04155505740717559],
    [
        0.05563007969699366,
        -0.20397695888897652,
        1.0569715142428786,
    ],
];

const SRGB: RgbSpace = RgbSpace {
    trc: Trc::Srgb,
    d50: false,
    to_xyz: SRGB_TO_XYZ,
    from_xyz: XYZ_TO_SRGB,
};
const SRGB_LINEAR: RgbSpace = RgbSpace {
    trc: Trc::Linear,
    d50: false,
    to_xyz: SRGB_TO_XYZ,
    from_xyz: XYZ_TO_SRGB,
};
const DISPLAY_P3: RgbSpace = RgbSpace {
    trc: Trc::Srgb,
    d50: false,
    to_xyz: [
        [0.4865709486482162, 0.26566769316909306, 0.1982172852343625],
        [0.2289745640697488, 0.6917385218365064, 0.079286914093745],
        [0.0, 0.04511338185890264, 1.043944368900976],
    ],
    from_xyz: [
        [2.493496911941425, -0.9313836179191239, -0.40271078445071684],
        [
            -0.8294889695615747,
            1.7626640603183463,
            0.023624685841943577,
        ],
        [
            0.03584583024378447,
            -0.07617238926804182,
            0.9568845240076872,
        ],
    ],
};
const A98_RGB: RgbSpace = RgbSpace {
    trc: Trc::A98,
    d50: false,
    to_xyz: [
        [0.5766690429101305, 0.1855582379065463, 0.1882286462349947],
        [0.29734497525053605, 0.6273635662554661, 0.0752914584939978],
        [0.02703136138641234, 0.07068885253582723, 0.9913375368376388],
    ],
    from_xyz: [
        [
            2.0415879038107465,
            -0.5650069742788596,
            -0.34473135077832956,
        ],
        [-0.9692436362808795, 1.8759675015077202, 0.04155505740717557],
        [
            0.013444280632031142,
            -0.11836239223101838,
            1.0151749943912054,
        ],
    ],
};
const PROPHOTO_RGB: RgbSpace = RgbSpace {
    trc: Trc::Prophoto,
    d50: true,
    to_xyz: [
        [0.7977666449006423, 0.13518129740053308, 0.0313477341283922],
        [0.2880748288194013, 0.711835234241873, 0.00008993693872564],
        [0.0, 0.0, 0.8251046025104602],
    ],
    from_xyz: [
        [
            1.3457868816471583,
            -0.25557208737979464,
            -0.05110186497554526,
        ],
        [-0.5446307051249019, 1.5082477428451468, 0.02052744743642139],
        [0.0, 0.0, 1.2119675456389452],
    ],
};
const REC2020: RgbSpace = RgbSpace {
    trc: Trc::Rec2020,
    d50: false,
    to_xyz: [
        [0.6369580483012914, 0.14461690358620832, 0.1688809751641721],
        [0.2627002120112671, 0.6779980715188708, 0.05930171646986196],
        [0.0, 0.028072693049087428, 1.060985057710791],
    ],
    from_xyz: [
        [1.716651187971268, -0.355670783776392, -0.25336628137366],
        [-0.666684351832489, 1.616481236634939, 0.0157685458139111],
        [0.017639857445311, -0.042770613257809, 0.942103121235474],
    ],
};

const XYZ_TO_LMS: Matrix = [
    [0.819022437996703, 0.3619062600528904, -0.1288737815209879],
    [0.0329836539323885, 0.9292868615863434, 0.0361446663506424],
    [0.0481771893596242, 0.2642395317527308, 0.6335478284694309],
];
const LMS_TO_OKLAB: Matrix = [
    [0.210454268309314, 0.7936177747023054, -0.0040720430116193],
    [1.9779985324311684, -2.42859224204858, 0.450593709617411],
    [0.0259040424655478, 0.7827717124575296, -0.8086757549230774],
];
const LMS_TO_XYZ: Matrix = [
    [1.2268798758459243, -0.5578149944602171, 0.2813910456659647],
    [-0.0405757452148008, 1.112286803280317, -0.0717110580655164],
    [-0.0763729366746601, -0.4214933324022432, 1.5869240198367816],
];
const OKLAB_TO_LMS: Matrix = [
    [1.0, 0.3963377773761749, 0.2158037573099136],
    [1.0, -0.1055613458156586, -0.0638541728258133],
    [1.0, -0.0894841775298119, -1.2914855480194092],
];
const KAPPA: f64 = 24389.0 / 27.0;
const EPSILON: f64 = 216.0 / 24389.0;

/// A color space the readers convert through XYZ (D65).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Space {
    Srgb,
    SrgbLinear,
    DisplayP3,
    A98Rgb,
    ProphotoRgb,
    Rec2020,
    /// `xyz`, which `color-mix()` also reads `xyz-d65` as.
    Xyz,
    XyzD65,
    XyzD50,
    Hsl,
    Hwb,
    Lab,
    Lch,
    Oklab,
    Oklch,
}

/// What each coordinate of a space is, for carrying a missing (`none`) value
/// across spaces in `color-mix()` (CSS Color 4, section 12.2): reds, greens and
/// blues include X, Y and Z; HSL lightness is a lightness; HWB's whiteness and
/// blackness have no analog. Upstream `ANALOGOUS` and `analog`.
type Analog = [Option<&'static str>; 3];

impl Space {
    /// The space a CSS name (lowercase) names.
    fn from_name(name: &str) -> Option<Space> {
        Some(match name {
            "srgb" => Space::Srgb,
            "srgb-linear" => Space::SrgbLinear,
            "display-p3" => Space::DisplayP3,
            "a98-rgb" => Space::A98Rgb,
            "prophoto-rgb" => Space::ProphotoRgb,
            "rec2020" => Space::Rec2020,
            "xyz" => Space::Xyz,
            "xyz-d65" => Space::XyzD65,
            "xyz-d50" => Space::XyzD50,
            "hsl" => Space::Hsl,
            "hwb" => Space::Hwb,
            "lab" => Space::Lab,
            "lch" => Space::Lch,
            "oklab" => Space::Oklab,
            "oklch" => Space::Oklch,
            _ => return None,
        })
    }

    fn rgb(self) -> Option<&'static RgbSpace> {
        Some(match self {
            Space::Srgb => &SRGB,
            Space::SrgbLinear => &SRGB_LINEAR,
            Space::DisplayP3 => &DISPLAY_P3,
            Space::A98Rgb => &A98_RGB,
            Space::ProphotoRgb => &PROPHOTO_RGB,
            Space::Rec2020 => &REC2020,
            _ => return None,
        })
    }

    fn analog(self) -> Analog {
        match self {
            Space::Hsl => [Some("hue"), Some("colorfulness"), Some("lightness")],
            Space::Hwb => [Some("hue"), None, None],
            Space::Lab | Space::Oklab => [Some("lightness"), Some("opponentA"), Some("opponentB")],
            Space::Lch | Space::Oklch => [Some("lightness"), Some("colorfulness"), Some("hue")],
            _ => [Some("reds"), Some("greens"), Some("blues")],
        }
    }
}

/// Polar coordinates (lightness, chroma, hue) from rectangular ones; a hue is
/// missing below the `powerless` chroma. Upstream `toPolar`.
fn to_polar([l, a, b]: [f64; 3], powerless: f64) -> [f64; 3] {
    let c = js_hypot(a, b);
    let h = ((b.atan2(a) * 180.0) / PI + 360.0) % 360.0;
    [l, c, if c < powerless { f64::NAN } else { h }]
}

/// Rectangular coordinates from polar ones; a missing hue counts as 0.
/// Upstream `fromPolar`.
fn from_polar([l, c, h]: [f64; 3]) -> [f64; 3] {
    let r = ((if h.is_nan() { 0.0 } else { h }) * PI) / 180.0;
    [l, c * r.cos(), c * r.sin()]
}

/// sRGB (0 to 1) from HSL. Upstream `hslToSrgb`.
fn hsl_to_srgb([h, s, l]: [f64; 3]) -> [f64; 3] {
    let hue = if h.is_nan() {
        0.0
    } else {
        ((h % 360.0) + 360.0) % 360.0
    };
    let s = s / 100.0;
    let l = l / 100.0;
    let f = |n: f64| {
        let k = (n + hue / 30.0) % 12.0;
        l - s * js_min(l, 1.0 - l) * js_max(-1.0, js_min(js_min(k - 3.0, 9.0 - k), 1.0))
    };
    [f(0.0), f(8.0), f(4.0)]
}

/// HSL from sRGB (0 to 1); an achromatic color's hue is missing. Upstream
/// `srgbToHsl`.
fn srgb_to_hsl([r, g, b]: [f64; 3]) -> [f64; 3] {
    let max = js_max(js_max(r, g), b);
    let min = js_min(js_min(r, g), b);
    let l = (max + min) / 2.0;
    let d = max - min;
    let mut h = f64::NAN;
    let mut s = 0.0;
    if d > 1e-7 {
        s = if l == 0.0 || l == 1.0 {
            0.0
        } else {
            (max - l) / js_min(l, 1.0 - l)
        };
        h = (if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        }) * 60.0;
    }
    [h, s * 100.0, l * 100.0]
}

/// sRGB (0 to 1) from HWB. Upstream `hwbToSrgb`.
fn hwb_to_srgb([h, w, b]: [f64; 3]) -> [f64; 3] {
    let w = w / 100.0;
    let b = b / 100.0;
    if w + b >= 1.0 {
        let gray = w / (w + b);
        return [gray, gray, gray];
    }
    hsl_to_srgb([h, 100.0, 50.0]).map(|c| c * (1.0 - w - b) + w)
}

/// HWB from sRGB (0 to 1); a gray's hue is missing. Upstream `srgbToHwb`.
fn srgb_to_hwb(rgb: [f64; 3]) -> [f64; 3] {
    let [h, _, _] = srgb_to_hsl(rgb);
    let w = js_min(js_min(rgb[0], rgb[1]), rgb[2]);
    let b = 1.0 - js_max(js_max(rgb[0], rgb[1]), rgb[2]);
    [
        if w + b >= 1.0 - 1e-7 { f64::NAN } else { h },
        w * 100.0,
        b * 100.0,
    ]
}

/// XYZ (D65) from coordinates in `space`; a missing coordinate counts as 0.
/// Upstream `toXYZ`.
fn to_xyz(space: Space, coords: [f64; 3]) -> [f64; 3] {
    let c = coords.map(|v| if v.is_nan() { 0.0 } else { v });
    if let Some(s) = space.rgb() {
        let xyz = mul(&s.to_xyz, c.map(|v| s.trc.to_linear(v)));
        return if s.d50 { mul(&D50_TO_D65, xyz) } else { xyz };
    }
    match space {
        Space::Xyz | Space::XyzD65 => c,
        Space::XyzD50 => mul(&D50_TO_D65, c),
        Space::Hsl => to_xyz(Space::Srgb, hsl_to_srgb(c)),
        Space::Hwb => to_xyz(Space::Srgb, hwb_to_srgb(c)),
        Space::Lch => to_xyz(Space::Lab, from_polar(c)),
        Space::Oklch => to_xyz(Space::Oklab, from_polar(c)),
        Space::Lab => {
            let [l, a, b] = c;
            let f1 = (l + 16.0) / 116.0;
            let f0 = a / 500.0 + f1;
            let f2 = f1 - b / 200.0;
            let xyz = [
                if f0.powf(3.0) > EPSILON {
                    f0.powf(3.0)
                } else {
                    (116.0 * f0 - 16.0) / KAPPA
                },
                if l > KAPPA * EPSILON {
                    f1.powf(3.0)
                } else {
                    l / KAPPA
                },
                if f2.powf(3.0) > EPSILON {
                    f2.powf(3.0)
                } else {
                    (116.0 * f2 - 16.0) / KAPPA
                },
            ];
            let xyz = [0, 1, 2].map(|i| xyz[i] * D50_WHITE[i]);
            mul(&D50_TO_D65, xyz)
        }
        Space::Oklab => mul(&LMS_TO_XYZ, mul(&OKLAB_TO_LMS, c).map(|v| v.powf(3.0))),
        // The RGB spaces returned above.
        _ => c,
    }
}

/// Coordinates in `space` from XYZ (D65). Upstream `fromXYZ`.
fn from_xyz(space: Space, xyz: [f64; 3]) -> [f64; 3] {
    if let Some(s) = space.rgb() {
        let v = if s.d50 { mul(&D65_TO_D50, xyz) } else { xyz };
        return mul(&s.from_xyz, v).map(|c| s.trc.from_linear(c));
    }
    match space {
        Space::Xyz | Space::XyzD65 => xyz,
        Space::XyzD50 => mul(&D65_TO_D50, xyz),
        Space::Hsl => srgb_to_hsl(from_xyz(Space::Srgb, xyz)),
        Space::Hwb => srgb_to_hwb(from_xyz(Space::Srgb, xyz)),
        Space::Lch => to_polar(from_xyz(Space::Lab, xyz), 0.0015),
        Space::Oklch => to_polar(from_xyz(Space::Oklab, xyz), 0.000004),
        Space::Lab => {
            let d50 = mul(&D65_TO_D50, xyz);
            let f = [0, 1, 2].map(|i| {
                let v = d50[i] / D50_WHITE[i];
                if v > EPSILON {
                    v.cbrt()
                } else {
                    (KAPPA * v + 16.0) / 116.0
                }
            });
            [
                116.0 * f[1] - 16.0,
                500.0 * (f[0] - f[1]),
                200.0 * (f[1] - f[2]),
            ]
        }
        Space::Oklab => mul(&LMS_TO_OKLAB, mul(&XYZ_TO_LMS, xyz).map(f64::cbrt)),
        // The RGB spaces returned above.
        _ => xyz,
    }
}

/// A color: its space, coordinates (NaN for a missing one) and alpha.
/// Upstream's `{space, coords, alpha}`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Color {
    space: Space,
    coords: [f64; 3],
    alpha: f64,
}

/// `color` in `space`, a missing coordinate carried to its analog. Upstream
/// `convert`.
fn convert(color: Color, space: Space) -> Color {
    if color.space == space {
        return color;
    }
    let mut coords = from_xyz(space, to_xyz(color.space, color.coords));
    let from = color.space.analog();
    let to = space.analog();
    for (i, v) in color.coords.iter().enumerate() {
        let at = from[i].and_then(|name| to.iter().position(|n| *n == Some(name)));
        if let (true, Some(at)) = (v.is_nan(), at) {
            coords[at] = f64::NAN;
        }
    }
    Color {
        space,
        coords,
        alpha: color.alpha,
    }
}

// =============================================================================
// Color syntax
// =============================================================================

/// Every CSS named color (CSS Color 4, section 6.1), as 6-digit hex. Upstream
/// `NAMED`.
const NAMED: [(&str, &str); 148] = [
    ("aliceblue", "f0f8ff"),
    ("antiquewhite", "faebd7"),
    ("aqua", "00ffff"),
    ("aquamarine", "7fffd4"),
    ("azure", "f0ffff"),
    ("beige", "f5f5dc"),
    ("bisque", "ffe4c4"),
    ("black", "000000"),
    ("blanchedalmond", "ffebcd"),
    ("blue", "0000ff"),
    ("blueviolet", "8a2be2"),
    ("brown", "a52a2a"),
    ("burlywood", "deb887"),
    ("cadetblue", "5f9ea0"),
    ("chartreuse", "7fff00"),
    ("chocolate", "d2691e"),
    ("coral", "ff7f50"),
    ("cornflowerblue", "6495ed"),
    ("cornsilk", "fff8dc"),
    ("crimson", "dc143c"),
    ("cyan", "00ffff"),
    ("darkblue", "00008b"),
    ("darkcyan", "008b8b"),
    ("darkgoldenrod", "b8860b"),
    ("darkgray", "a9a9a9"),
    ("darkgreen", "006400"),
    ("darkgrey", "a9a9a9"),
    ("darkkhaki", "bdb76b"),
    ("darkmagenta", "8b008b"),
    ("darkolivegreen", "556b2f"),
    ("darkorange", "ff8c00"),
    ("darkorchid", "9932cc"),
    ("darkred", "8b0000"),
    ("darksalmon", "e9967a"),
    ("darkseagreen", "8fbc8f"),
    ("darkslateblue", "483d8b"),
    ("darkslategray", "2f4f4f"),
    ("darkslategrey", "2f4f4f"),
    ("darkturquoise", "00ced1"),
    ("darkviolet", "9400d3"),
    ("deeppink", "ff1493"),
    ("deepskyblue", "00bfff"),
    ("dimgray", "696969"),
    ("dimgrey", "696969"),
    ("dodgerblue", "1e90ff"),
    ("firebrick", "b22222"),
    ("floralwhite", "fffaf0"),
    ("forestgreen", "228b22"),
    ("fuchsia", "ff00ff"),
    ("gainsboro", "dcdcdc"),
    ("ghostwhite", "f8f8ff"),
    ("gold", "ffd700"),
    ("goldenrod", "daa520"),
    ("gray", "808080"),
    ("green", "008000"),
    ("greenyellow", "adff2f"),
    ("grey", "808080"),
    ("honeydew", "f0fff0"),
    ("hotpink", "ff69b4"),
    ("indianred", "cd5c5c"),
    ("indigo", "4b0082"),
    ("ivory", "fffff0"),
    ("khaki", "f0e68c"),
    ("lavender", "e6e6fa"),
    ("lavenderblush", "fff0f5"),
    ("lawngreen", "7cfc00"),
    ("lemonchiffon", "fffacd"),
    ("lightblue", "add8e6"),
    ("lightcoral", "f08080"),
    ("lightcyan", "e0ffff"),
    ("lightgoldenrodyellow", "fafad2"),
    ("lightgray", "d3d3d3"),
    ("lightgreen", "90ee90"),
    ("lightgrey", "d3d3d3"),
    ("lightpink", "ffb6c1"),
    ("lightsalmon", "ffa07a"),
    ("lightseagreen", "20b2aa"),
    ("lightskyblue", "87cefa"),
    ("lightslategray", "778899"),
    ("lightslategrey", "778899"),
    ("lightsteelblue", "b0c4de"),
    ("lightyellow", "ffffe0"),
    ("lime", "00ff00"),
    ("limegreen", "32cd32"),
    ("linen", "faf0e6"),
    ("magenta", "ff00ff"),
    ("maroon", "800000"),
    ("mediumaquamarine", "66cdaa"),
    ("mediumblue", "0000cd"),
    ("mediumorchid", "ba55d3"),
    ("mediumpurple", "9370db"),
    ("mediumseagreen", "3cb371"),
    ("mediumslateblue", "7b68ee"),
    ("mediumspringgreen", "00fa9a"),
    ("mediumturquoise", "48d1cc"),
    ("mediumvioletred", "c71585"),
    ("midnightblue", "191970"),
    ("mintcream", "f5fffa"),
    ("mistyrose", "ffe4e1"),
    ("moccasin", "ffe4b5"),
    ("navajowhite", "ffdead"),
    ("navy", "000080"),
    ("oldlace", "fdf5e6"),
    ("olive", "808000"),
    ("olivedrab", "6b8e23"),
    ("orange", "ffa500"),
    ("orangered", "ff4500"),
    ("orchid", "da70d6"),
    ("palegoldenrod", "eee8aa"),
    ("palegreen", "98fb98"),
    ("paleturquoise", "afeeee"),
    ("palevioletred", "db7093"),
    ("papayawhip", "ffefd5"),
    ("peachpuff", "ffdab9"),
    ("peru", "cd853f"),
    ("pink", "ffc0cb"),
    ("plum", "dda0dd"),
    ("powderblue", "b0e0e6"),
    ("purple", "800080"),
    ("rebeccapurple", "663399"),
    ("red", "ff0000"),
    ("rosybrown", "bc8f8f"),
    ("royalblue", "4169e1"),
    ("saddlebrown", "8b4513"),
    ("salmon", "fa8072"),
    ("sandybrown", "f4a460"),
    ("seagreen", "2e8b57"),
    ("seashell", "fff5ee"),
    ("sienna", "a0522d"),
    ("silver", "c0c0c0"),
    ("skyblue", "87ceeb"),
    ("slateblue", "6a5acd"),
    ("slategray", "708090"),
    ("slategrey", "708090"),
    ("snow", "fffafa"),
    ("springgreen", "00ff7f"),
    ("steelblue", "4682b4"),
    ("tan", "d2b48c"),
    ("teal", "008080"),
    ("thistle", "d8bfd8"),
    ("tomato", "ff6347"),
    ("turquoise", "40e0d0"),
    ("violet", "ee82ee"),
    ("wheat", "f5deb3"),
    ("white", "ffffff"),
    ("whitesmoke", "f5f5f5"),
    ("yellow", "ffff00"),
    ("yellowgreen", "9acd32"),
];

/// The CSS system colors, which the browser picks. Upstream `SYSTEM_COLORS`.
const SYSTEM_COLORS: [&str; 19] = [
    "accentcolor",
    "accentcolortext",
    "activetext",
    "buttonborder",
    "buttonface",
    "buttontext",
    "canvas",
    "canvastext",
    "field",
    "fieldtext",
    "graytext",
    "highlight",
    "highlighttext",
    "linktext",
    "mark",
    "marktext",
    "selecteditem",
    "selecteditemtext",
    "visitedtext",
];

/// sRGB from hex digits (3, 4, 6 or 8 of them). Upstream `fromHex`.
fn from_hex(digits: &str) -> CssResult<Color> {
    let mut h: String = if digits.len() == 3 || digits.len() == 4 {
        digits.chars().flat_map(|c| [c, c]).collect()
    } else {
        digits.to_string()
    };
    if h.len() == 6 {
        h.push_str("ff");
    }
    if h.len() != 8 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return fail(format!("#{digits} is not a hex color"));
    }
    let n = |at: usize| f64::from(u8::from_str_radix(&h[at..at + 2], 16).unwrap_or(0)) / 255.0;
    Ok(Color {
        space: Space::Srgb,
        coords: [n(0), n(2), n(4)],
        alpha: n(6),
    })
}

/// One channel of a color function: its keyword, its value at 100%, and
/// whether it is a hue (which reads angles).
#[derive(Clone, Copy, Debug)]
struct Channel {
    name: &'static str,
    full: f64,
    hue: bool,
}

const fn ch(name: &'static str, full: f64) -> Channel {
    Channel {
        name,
        full,
        hue: false,
    }
}

const fn hue(name: &'static str) -> Channel {
    Channel {
        name,
        full: 0.0,
        hue: true,
    }
}

/// A color function's channels: the space they are in, a scale they are
/// multiplied by, and the range they clamp to at parse time, as CSS clamps
/// them. Upstream `CHANNELS`.
#[derive(Clone, Copy, Debug)]
struct ChannelFn {
    space: Space,
    scale: Option<f64>,
    ch: [Channel; 3],
    clamp: Option<(f64, f64)>,
}

/// The channels of a color function by name. Upstream `CHANNELS`.
fn channels(name: &str) -> Option<ChannelFn> {
    let plain = |space, ch| ChannelFn {
        space,
        scale: None,
        ch,
        clamp: None,
    };
    Some(match name {
        "rgb" | "rgba" => ChannelFn {
            space: Space::Srgb,
            scale: Some(1.0 / 255.0),
            ch: [ch("r", 255.0), ch("g", 255.0), ch("b", 255.0)],
            clamp: Some((0.0, 255.0)),
        },
        "hsl" | "hsla" => plain(Space::Hsl, [hue("h"), ch("s", 100.0), ch("l", 100.0)]),
        "hwb" => plain(Space::Hwb, [hue("h"), ch("w", 100.0), ch("b", 100.0)]),
        "lab" => plain(Space::Lab, [ch("l", 100.0), ch("a", 125.0), ch("b", 125.0)]),
        "lch" => plain(Space::Lch, [ch("l", 100.0), ch("c", 150.0), hue("h")]),
        "oklab" => plain(Space::Oklab, [ch("l", 1.0), ch("a", 0.4), ch("b", 0.4)]),
        "oklch" => plain(Space::Oklch, [ch("l", 1.0), ch("c", 0.4), hue("h")]),
        _ => return None,
    })
}

/// The spaces `color()` takes. Upstream `COLOR_FN_SPACES`.
fn color_fn_space(name: &str) -> Option<Space> {
    match name {
        "srgb" | "display-p3" | "a98-rgb" | "prophoto-rgb" | "rec2020" | "srgb-linear" | "xyz"
        | "xyz-d50" | "xyz-d65" => Space::from_name(name),
        _ => None,
    }
}

/// One channel: a number, percentage, angle, `none`, keyword or math.
/// Upstream `channel`.
fn channel(
    tok: &Tok,
    ch: &Channel,
    vars: Option<&Vars>,
    notes: &mut Vec<String>,
) -> CssResult<f64> {
    if matches!(tok, Tok::Id(v) if v == "none") {
        return Ok(f64::NAN);
    }
    let q = quantity(tok, vars, notes, None)?;
    match q.k {
        Kind::Percent if ch.hue => fail("a hue is not a percentage"),
        Kind::Percent => Ok((q.v / 100.0) * ch.full),
        Kind::Angle if ch.hue => Ok(q.v),
        Kind::Angle => fail("only a hue takes an angle"),
        Kind::Number => Ok(q.v),
        Kind::Length => fail("a length is not a color channel"),
    }
}

/// Alpha from a token: a number or percentage, held in 0…1. Upstream
/// `alphaOf`.
fn alpha_of(tok: &Tok, vars: Option<&Vars>, notes: &mut Vec<String>) -> CssResult<f64> {
    if matches!(tok, Tok::Id(v) if v == "none") {
        return Ok(f64::NAN);
    }
    let v = channel(tok, &ch("alpha", 1.0), vars, notes)?;
    Ok(js_min(1.0, js_max(0.0, v)))
}

/// The channel keywords of a relative color's origin, in the function's
/// terms. Upstream `originVars`.
fn origin_vars(spec: &ChannelFn, origin: Color) -> Vars {
    let scale = spec.scale.unwrap_or(1.0);
    Vars {
        names: spec.ch.map(|c| c.name),
        coords: convert(origin, spec.space).coords.map(|v| v / scale),
        alpha: origin.alpha,
    }
}

/// A color function: `rgb()`, `hsl()`, `hwb()`, `lab()`, `lch()`, `oklab()`,
/// `oklch()` or `color()`, in modern or legacy comma syntax, absolute or
/// relative (`rgb(from <color> r g b)`). Upstream `colorFunction`.
fn color_function(name: &str, body: &[Tok], notes: &mut Vec<String>) -> CssResult<Color> {
    let mut body = body;
    let mut origin = None;
    if matches!(body.first(), Some(Tok::Id(v)) if v == "from") {
        let Some(origin_tok) = body.get(1) else {
            return fail("from needs a color");
        };
        origin = Some(parse_color(origin_tok, notes)?);
        body = &body[2..];
    }
    let spec = if name == "color" {
        let space_name = match body.first() {
            Some(Tok::Id(v)) => Some(v.as_str()),
            _ => None,
        };
        let Some((space_name, space)) =
            space_name.and_then(|n| color_fn_space(n).map(|space| (n, space)))
        else {
            return fail(format!(
                "color({}) is not a color space Swift can read",
                space_name.unwrap_or("")
            ));
        };
        let names = if space_name.starts_with("xyz") {
            ["x", "y", "z"]
        } else {
            ["r", "g", "b"]
        };
        body = &body[1..];
        ChannelFn {
            space,
            scale: None,
            ch: names.map(|n| ch(n, 1.0)),
            clamp: None,
        }
    } else {
        let Some(spec) = channels(name) else {
            return fail(format!("\"{name}()\" is not a color"));
        };
        spec
    };
    let vars = origin.map(|origin| origin_vars(&spec, origin));
    let mut parts: Vec<&Tok> = body.iter().collect();
    let mut alpha_tok: Option<&Tok> = None;
    let legacy = origin.is_none() && body.iter().any(|t| matches!(t, Tok::Op { v: ',', .. }));
    if legacy {
        parts.clear();
        for part in commas(body) {
            let [tok] = part else {
                return fail(format!("{name}() has an empty or doubled value"));
            };
            parts.push(tok);
        }
        check_comma_syntax(name, &parts)?;
        if parts.len() == 4 {
            alpha_tok = parts.pop();
        }
    } else if let Some(slash) = body
        .iter()
        .position(|t| matches!(t, Tok::Op { v: '/', .. }))
    {
        if slash + 2 != body.len() {
            return fail(format!("{name}() needs one alpha after /"));
        }
        alpha_tok = Some(&body[slash + 1]);
        parts.truncate(slash);
    }
    let [c0, c1, c2] = parts.as_slice() else {
        return fail(format!("{name}() needs three channels"));
    };
    let mut coords = [0.0; 3];
    for (i, tok) in [c0, c1, c2].into_iter().enumerate() {
        coords[i] = channel(tok, &spec.ch[i], vars.as_ref(), notes)?;
    }
    // A relative color keeps its origin's gamut, so only literal channels clamp.
    if let (Some((low, high)), None) = (spec.clamp, origin) {
        coords = coords.map(|v| {
            if v.is_nan() {
                v
            } else {
                js_min(high, js_max(low, v))
            }
        });
    }
    if let Some(scale) = spec.scale {
        coords = coords.map(|v| v * scale);
    }
    if origin.is_none() && matches!(spec.space, Space::Hsl | Space::Hwb) {
        // hsl() saturation and lightness clamp at 100%, as browsers read them.
        let capped = spec.space == Space::Hsl;
        for v in &mut coords[1..] {
            *v = js_max(0.0, if capped { js_min(100.0, *v) } else { *v });
        }
    }
    if origin.is_none()
        && matches!(
            spec.space,
            Space::Lab | Space::Lch | Space::Oklab | Space::Oklch
        )
    {
        coords[0] = js_min(spec.ch[0].full, js_max(0.0, coords[0]));
        if matches!(spec.space, Space::Lch | Space::Oklch) {
            coords[1] = js_max(0.0, coords[1]);
        }
    }
    let alpha = match alpha_tok {
        Some(tok) => alpha_of(tok, vars.as_ref(), notes)?,
        None => vars.map_or(1.0, |vars| vars.alpha),
    };
    Ok(Color {
        space: spec.space,
        coords,
        alpha,
    })
}

/// Fails comma-syntax channels CSS does not allow: commas only in `rgb()` and
/// `hsl()` (and their `a` forms), no `none`, `rgb()` channels all numbers or
/// all percentages, and `hsl()` saturation and lightness as percentages.
/// Upstream `checkCommaSyntax`.
fn check_comma_syntax(name: &str, parts: &[&Tok]) -> CssResult<()> {
    if !matches!(name, "rgb" | "rgba" | "hsl" | "hsla") {
        return fail(format!("{name}() has no comma syntax"));
    }
    if parts.iter().any(|p| matches!(p, Tok::Id(v) if v == "none")) {
        return fail(format!("{name}() with commas cannot hold none"));
    }
    let percent = |p: &Tok| matches!(p, Tok::Num { u, .. } if u == "%");
    let plain = |p: &Tok| matches!(p, Tok::Num { u, .. } if u.is_empty());
    let is_num = |p: &Tok| matches!(p, Tok::Num { .. });
    if name.starts_with("rgb") {
        let nums: Vec<&Tok> = parts
            .iter()
            .take(3)
            .copied()
            .filter(|p| is_num(p))
            .collect();
        if !(nums.iter().all(|p| percent(p)) || nums.iter().all(|p| plain(p))) {
            return fail(format!(
                "{name}() with commas takes all numbers or all percentages"
            ));
        }
    } else if parts
        .iter()
        .skip(1)
        .take(2)
        .any(|p| is_num(p) && !percent(p))
    {
        return fail(format!(
            "{name}() with commas takes its saturation and lightness as percentages"
        ));
    }
    Ok(())
}

/// How `color-mix()` interpolates hues. Upstream `HUE_METHODS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HueMethod {
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

impl HueMethod {
    fn from_name(name: &str) -> Option<HueMethod> {
        Some(match name {
            "shorter" => HueMethod::Shorter,
            "longer" => HueMethod::Longer,
            "increasing" => HueMethod::Increasing,
            "decreasing" => HueMethod::Decreasing,
            _ => return None,
        })
    }
}

/// Whether `color-mix()` interpolates in a space of this name: an RGB space,
/// a polar or Lab space, or any name starting `xyz` (an unknown one fails
/// when a color converts to it).
fn is_mix_space(name: &str) -> bool {
    Space::from_name(name).is_some() || name.starts_with("xyz")
}

/// `color-mix()`, CSS Color 5 section 3 (upstream `colorMix`): `oklab` unless
/// `in <space> [<method> hue]` says otherwise, percentages normalized, alpha
/// premultiplied, a missing channel taken from the other color, and the alpha
/// scaled down when the percentages sum to less than 100%.
fn color_mix(body: &[Tok], notes: &mut Vec<String>) -> CssResult<Color> {
    let mut parts = commas(body);
    let mut space_name = "oklab".to_string();
    let mut hue_method = HueMethod::Shorter;
    if matches!(parts[0].first(), Some(Tok::Id(v)) if v == "in") {
        let spec = parts.remove(0);
        space_name = match spec.get(1) {
            Some(Tok::Id(v)) => v.clone(),
            _ => return fail("color-mix() needs a space after in"),
        };
        if space_name == "xyz-d65" {
            space_name = "xyz".to_string();
        }
        if !is_mix_space(&space_name) {
            return fail(format!(
                "color-mix() cannot interpolate in \"{space_name}\""
            ));
        }
        let method = spec.get(2).and_then(text_of).and_then(HueMethod::from_name);
        match method {
            Some(method) if spec.len() == 4 && spec.get(3).and_then(text_of) == Some("hue") => {
                hue_method = method;
            }
            _ if spec.len() != 2 => {
                return fail("color-mix() has an unknown interpolation method");
            }
            _ => {}
        }
    }
    if parts.len() != 2 {
        return fail("color-mix() mixes two colors");
    }
    let space = Space::from_name(&space_name);
    let mut items: Vec<(Color, Option<f64>)> = Vec::with_capacity(2);
    for part in &parts {
        let pct = part.iter().enumerate().find_map(|(at, t)| match t {
            Tok::Num { v, u } if u == "%" => Some((at, *v)),
            _ => None,
        });
        let rest: Vec<&Tok> = part
            .iter()
            .enumerate()
            .filter(|(at, _)| pct.is_none_or(|(pct_at, _)| pct_at != *at))
            .map(|(_, t)| t)
            .collect();
        let [color] = rest.as_slice() else {
            return fail("a color-mix() color is one color and an optional percentage");
        };
        // Upstream's comparisons, kept as written.
        #[allow(clippy::manual_range_contains)]
        if let Some((_, p)) = pct
            && (p < 0.0 || p > 100.0)
        {
            return fail("a color-mix() percentage is from 0% to 100%");
        }
        let parsed = parse_color(color, notes)?;
        let Some(space) = space else {
            return fail(format!("\"{space_name}\" is not a color space"));
        };
        items.push((convert(parsed, space), pct.map(|(_, p)| p)));
    }
    let Some(space) = space else {
        return fail(format!("\"{space_name}\" is not a color space"));
    };
    let (a, b) = (items[0].0, items[1].0);
    let (p1, p2) = match (items[0].1, items[1].1) {
        (None, None) => (50.0, 50.0),
        (None, Some(p2)) => (100.0 - p2, p2),
        (Some(p1), None) => (p1, 100.0 - p1),
        (Some(p1), Some(p2)) => (p1, p2),
    };
    let total = p1 + p2;
    if total == 0.0 {
        return no_static_form("color-mix() percentages add up to 0%");
    }
    let w = p2 / total;
    let pick = |x: f64, y: f64| {
        if x.is_nan() {
            Some(y)
        } else if y.is_nan() {
            Some(x)
        } else {
            None
        }
    };
    // A missing alpha takes the other color's before premultiplying.
    let alpha_a = if a.alpha.is_nan() { b.alpha } else { a.alpha };
    let alpha_b = if b.alpha.is_nan() { a.alpha } else { b.alpha };
    let alpha = alpha_a * (1.0 - w) + alpha_b * w;
    let pa = if alpha_a.is_nan() { 1.0 } else { alpha_a };
    let pb = if alpha_b.is_nan() { 1.0 } else { alpha_b };
    let unpremultiply = if alpha.is_nan() { 1.0 } else { alpha };
    let hue_at = space.analog().iter().position(|n| *n == Some("hue"));
    let mut coords = [0.0; 3];
    for (i, coord) in coords.iter_mut().enumerate() {
        let (x, y) = (a.coords[i], b.coords[i]);
        *coord = if let Some(missing) = pick(x, y) {
            missing
        } else if Some(i) == hue_at {
            let mut h1 = ((x % 360.0) + 360.0) % 360.0;
            let mut h2 = ((y % 360.0) + 360.0) % 360.0;
            let d = h2 - h1;
            match hue_method {
                HueMethod::Shorter => {
                    if d > 180.0 {
                        h1 += 360.0;
                    } else if d < -180.0 {
                        h2 += 360.0;
                    }
                }
                HueMethod::Longer => {
                    if d > 0.0 && d < 180.0 {
                        h1 += 360.0;
                    } else if d > -180.0 && d <= 0.0 {
                        h2 += 360.0;
                    }
                }
                HueMethod::Increasing => {
                    if h2 < h1 {
                        h2 += 360.0;
                    }
                }
                HueMethod::Decreasing => {
                    if h1 < h2 {
                        h1 += 360.0;
                    }
                }
            }
            (h1 * (1.0 - w) + h2 * w) % 360.0
        } else {
            let premixed = x * pa * (1.0 - w) + y * pb * w;
            if unpremultiply == 0.0 {
                0.0
            } else {
                premixed / unpremultiply
            }
        };
    }
    Ok(Color {
        space,
        coords,
        alpha: alpha * if total < 100.0 { total / 100.0 } else { 1.0 },
    })
}

/// A color token as a color (upstream `parseColor`, `css-values.mjs`): hex, a
/// named color, `transparent`, a color function or `color-mix()`.
/// `currentColor`, system colors and `var()` have no static form.
fn parse_color(tok: &Tok, notes: &mut Vec<String>) -> CssResult<Color> {
    match tok {
        Tok::Hash(digits) => return from_hex(digits),
        Tok::Id(name) => {
            if name == "transparent" {
                return Ok(Color {
                    space: Space::Srgb,
                    coords: [0.0; 3],
                    alpha: 0.0,
                });
            }
            if let Some((_, hex)) = NAMED.iter().find(|(named, _)| *named == name.as_str()) {
                return from_hex(hex);
            }
            if name == "currentcolor" {
                return no_static_form("currentColor depends on where the token is used");
            }
            if SYSTEM_COLORS.contains(&name.as_str()) {
                return no_static_form(format!("{name} is a system color the browser picks"));
            }
        }
        Tok::Fn {
            name,
            body: Body::Tokens(body),
        } => {
            if channels(name).is_some() || name == "color" {
                return color_function(name, body, notes);
            }
            if name == "color-mix" {
                return color_mix(body, notes);
            }
        }
        _ => {}
    }
    dynamic(tok)?;
    fail(format!("\"{}\" is not a color", show(tok)))
}

// =============================================================================
// Output gamut
// =============================================================================

const GAMUT_EPSILON: f64 = 0.000075;

fn in_gamut(rgb: &[f64; 3]) -> bool {
    rgb.iter()
        .all(|c| (-GAMUT_EPSILON..=1.0 + GAMUT_EPSILON).contains(c))
}

fn clip(rgb: [f64; 3]) -> [f64; 3] {
    rgb.map(|c| js_min(1.0, js_max(0.0, c)))
}

/// A channel (0 to 1) as two uppercase hex digits: the nearest of 256 steps,
/// halves up, after rounding away float noise below 1e-6. Upstream `byte`.
fn byte(c: f64) -> String {
    format!(
        "{:0>2}",
        js_to_hex_string(js_round(js_round(c * 255.0 * 1e6) / 1e6))
    )
    .to_uppercase()
}

/// A CSS color as 8-bit channels (upstream `toColor`): `#RRGGBBAA` in sRGB
/// when the color is inside sRGB, else `display-p3 #RRGGBBAA` (clipped per
/// channel, with the [`OUTSIDE_P3`] note, when it is outside Display P3 too).
///
/// It reads hex, the named colors, `rgb()`/`rgba()`, `hsl()`/`hsla()`,
/// `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, `color()` in the sRGB,
/// linear sRGB, Display P3, A98 RGB, ProPhoto RGB, Rec. 2020 and XYZ spaces,
/// `color-mix()` in any of those, and relative colors (`rgb(from red r g b /
/// 50%)`). `rgba(5, 54, 89, 0.1)` is `#0536591A`.
pub fn to_color(value: &str, notes: &mut Vec<String>) -> CssResult<String> {
    let tokens = tokenize(value)?;
    let [tok] = tokens.as_slice() else {
        return fail(format!("\"{value}\" is not one color"));
    };
    Ok(output_color(parse_color(tok, notes)?, notes))
}

/// A color as `#RRGGBBAA` or `display-p3 #RRGGBBAA`. Upstream `outputColor`.
fn output_color(color: Color, notes: &mut Vec<String>) -> String {
    let alpha = if color.alpha.is_nan() {
        0.0
    } else {
        color.alpha
    };
    let xyz = to_xyz(color.space, color.coords);
    let srgb = from_xyz(Space::Srgb, xyz);
    let hex = |rgb: [f64; 3]| {
        let [r, g, b] = clip(rgb).map(byte);
        format!("#{r}{g}{b}{}", byte(alpha))
    };
    if in_gamut(&srgb) {
        return hex(srgb);
    }
    let p3 = from_xyz(Space::DisplayP3, xyz);
    // Outside Display P3, each channel clips, as Chromium draws the color.
    if !in_gamut(&p3) {
        notes.push(OUTSIDE_P3.to_string());
    }
    format!("display-p3 {}", hex(p3))
}

// =============================================================================
// Shadows
// =============================================================================

/// One `box-shadow` layer as [`to_shadow`] reads it: offsets, blur and spread
/// in points, and the color as [`to_color`] writes it. Upstream's
/// `{x, y, blur, spread, isInset, color}`.
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowLayer {
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    pub spread: f64,
    pub is_inset: bool,
    pub color: String,
}

/// A `box-shadow` list as layers, as SwiftUI can draw them (upstream
/// `toShadow`). A drop layer is `x y [blur [spread]] color`; SwiftUI has no
/// drop spread, so a spread is left out with a note. An inset layer is a ring,
/// `inset 0 0 0 <width> color`; an offset or blurred inset has no static form.
/// `none` is no layers.
///
/// [`typed`] reads shadows its own way (`values.mjs` keeps a drop spread,
/// which GPUI draws); this is the Swift generator's reader.
pub fn to_shadow(value: &str, notes: &mut Vec<String>) -> CssResult<Vec<ShadowLayer>> {
    if js_trim(value).to_lowercase() == "none" {
        return Ok(Vec::new());
    }
    let mut layers = Vec::new();
    for layer in css_split_top_level(value) {
        let mut lengths = Vec::new();
        let mut color_tok = None;
        let mut is_inset = false;
        let tokens = tokenize(layer)?;
        for tok in &tokens {
            // CSS-wide keywords, var(), env(), and attr() have no static form.
            dynamic(tok)?;
            let is_length = match tok {
                Tok::Num { .. } => true,
                Tok::Fn { name, .. } => {
                    matches!(name.as_str(), "calc" | "min" | "max" | "clamp" | "")
                }
                _ => false,
            };
            if matches!(tok, Tok::Id(v) if v == "inset") {
                is_inset = true;
            } else if is_length {
                let q = quantity(tok, None, notes, None)?;
                if q.k != Kind::Length && !(q.k == Kind::Number && q.v == 0.0) {
                    return fail(format!(
                        "shadow layer \"{layer}\" has a {} where a length goes",
                        q.k
                    ));
                }
                lengths.push(q.v);
            } else if color_tok.is_some() {
                return fail(format!("shadow layer \"{layer}\" has two colors"));
            } else {
                color_tok = Some(tok);
            }
        }
        if !(2..=4).contains(&lengths.len()) {
            return fail(format!(
                "shadow layer \"{layer}\" needs two to four lengths"
            ));
        }
        let Some(color_tok) = color_tok else {
            return no_static_form(format!(
                "shadow layer \"{layer}\" has no color, so it draws in currentColor"
            ));
        };
        let length = |at: usize| lengths.get(at).copied().unwrap_or(0.0);
        let (x, y, blur, spread) = (length(0), length(1), length(2), length(3));
        let color = output_color(parse_color(color_tok, notes)?, notes);
        if is_inset {
            if x != 0.0 || y != 0.0 || blur != 0.0 {
                return no_static_form(format!(
                    "inset layer \"{layer}\" is not a ring; SwiftUI draws only inset rings"
                ));
            }
            layers.push(ShadowLayer {
                x,
                y,
                blur,
                spread,
                is_inset,
                color,
            });
            continue;
        }
        if spread != 0.0 {
            notes.push(format!("drop layer \"{layer}\" drawn without its spread"));
        }
        layers.push(ShadowLayer {
            x,
            y,
            blur,
            spread: 0.0,
            is_inset,
            color,
        });
    }
    Ok(layers)
}

// =============================================================================
// values.mjs: token kinds and typed values
// =============================================================================

/// The kind of value a token or style property holds, which decides how
/// [`typed`] reads it. `values.mjs`'s kind names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueKind {
    /// `color`: a [`TokenValue::Color`].
    Color,
    /// `length`: a [`TokenValue::Length`], in rems when written in rems.
    Length,
    /// `weight`: a [`TokenValue::Weight`].
    Weight,
    /// `leading`: a [`TokenValue::Leading`], a multiple of the font size.
    Leading,
    /// `duration`: a [`TokenValue::Duration`].
    Duration,
    /// `ease`: a [`TokenValue::Ease`].
    Ease,
    /// `families`: a [`TokenValue::Families`].
    Families,
    /// `shadow`: a [`TokenValue::Shadow`].
    Shadow,
    /// `edges`: a padding or margin shorthand, [`Typed::Edges`].
    Edges,
    /// `keyword`: a [`TokenValue::Keyword`].
    Keyword,
    /// `number`: a [`TokenValue::Number`].
    Number,
}

/// Whether `name` is `--text-<anything>` ending in `suffix` with no line
/// break in between, as `/^--text-.*<suffix>$/` matches.
fn text_style_token(name: &str, suffix: &str) -> bool {
    name.strip_prefix("--text-").is_some_and(|rest| {
        rest.ends_with(suffix) && !rest.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
    })
}

/// The kind of value a token holds, from its name (`values.mjs`
/// `tokenKind`): `--shadow-*` shadows, any `duration` a duration, `--ease-*`
/// easings, `--font-family-*` families, `--font-weight-*` and
/// `--text-*-weight` weights, `--text-*-leading` line heights, `*-style`
/// keywords, `--color-*` and `*-color` colors, and everything else a length.
pub fn token_kind(name: &str) -> ValueKind {
    if name.starts_with("--shadow-") {
        ValueKind::Shadow
    } else if name.contains("duration") {
        ValueKind::Duration
    } else if name.starts_with("--ease-") {
        ValueKind::Ease
    } else if name.starts_with("--font-family-") {
        ValueKind::Families
    } else if name.starts_with("--font-weight-") || text_style_token(name, "-weight") {
        ValueKind::Weight
    } else if text_style_token(name, "-leading") {
        ValueKind::Leading
    } else if name.ends_with("-style") {
        ValueKind::Keyword
    } else if name.starts_with("--color-") || name.ends_with("-color") {
        ValueKind::Color
    } else {
        ValueKind::Length
    }
}

/// The kind of value a CSS property in a component override holds
/// (`values.mjs` `propertyKind`): a custom property by [`token_kind`], the
/// properties Astryx themes write by name, and any other a length.
pub fn property_kind(property: &str) -> ValueKind {
    if property.starts_with("--") {
        return token_kind(property);
    }
    match property {
        "backgroundColor" | "color" | "borderColor" | "outlineColor" => ValueKind::Color,
        "fontFamily" => ValueKind::Families,
        "fontWeight" => ValueKind::Weight,
        "lineHeight" => ValueKind::Leading,
        "boxShadow" => ValueKind::Shadow,
        "borderStyle" => ValueKind::Keyword,
        "padding" | "paddingInline" | "paddingBlock" | "margin" => ValueKind::Edges,
        "opacity" => ValueKind::Number,
        _ => ValueKind::Length,
    }
}

/// `text` split at top-level occurrences of `separator` (outside
/// parentheses; `' '` splits at any whitespace), each part trimmed and empty
/// parts dropped (`values.mjs` `splitTopLevel`). `1px 2px rgba(0, 0, 0, 0.1)`
/// split at `' '` is `["1px", "2px", "rgba(0, 0, 0, 0.1)"]`.
pub fn split_top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i64 = 0;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
        } else if depth == 0
            && (if separator == ' ' {
                is_js_whitespace(c)
            } else {
                c == separator
            })
        {
            parts.push(js_trim(&text[start..i]));
            start = i + c.len_utf8();
        }
    }
    parts.push(js_trim(&text[start..]));
    parts.retain(|part| !part.is_empty());
    parts
}

/// `value` with every `var(--x[, fallback])` replaced by the value of `--x` in
/// `tokens`, or by its fallback when `tokens` has none, recursively, so the
/// value can be read on its own (`values.mjs` `substituteVars`). A name listed
/// twice in `tokens` takes its last value, as an object built from the pairs
/// would. Fails on a `var()` with neither, or nesting deeper than 20.
pub fn substitute_vars<K: AsRef<str>, V: AsRef<str>>(
    value: &str,
    tokens: &[(K, V)],
) -> CssResult<String> {
    substitute_vars_at(value, tokens, 0)
}

fn substitute_vars_at<K: AsRef<str>, V: AsRef<str>>(
    value: &str,
    tokens: &[(K, V)],
    depth: u32,
) -> CssResult<String> {
    if depth > 20 {
        return fail(format!("var() nesting too deep in \"{value}\""));
    }
    let Some(start) = value.find("var(") else {
        return Ok(value.to_string());
    };
    let bytes = value.as_bytes();
    let mut close = start + 4;
    let mut open: i64 = 1;
    while close < bytes.len() && open > 0 {
        if bytes[close] == b'(' {
            open += 1;
        } else if bytes[close] == b')' {
            open -= 1;
        }
        close += 1;
    }
    // Upstream drops the character before `close`: the matching `)`, or the
    // last character of an unclosed `var(`.
    let inner_end = value[..close]
        .char_indices()
        .next_back()
        .map_or(close, |(at, _)| at)
        .max(start + 4);
    let inner = &value[start + 4..inner_end];
    let split = split_top_level(inner, ',');
    // Upstream reads `undefined.trim()` (a TypeError) for an empty `var()`.
    let Some((name, fallback)) = split.split_first() else {
        return fail("var() is not defined");
    };
    let replacement = match tokens.iter().rev().find(|(key, _)| key.as_ref() == *name) {
        Some((_, found)) => found.as_ref().to_string(),
        None if fallback.is_empty() => return fail(format!("var({name}) is not defined")),
        None => fallback.join(", "),
    };
    let expanded = substitute_vars_at(&replacement, tokens, depth + 1)?;
    let next = format!("{}{expanded}{}", &value[..start], &value[close..]);
    substitute_vars_at(&next, tokens, depth + 1)
}

/// `Number(s)`: the whole string, trimmed, as a JavaScript number literal
/// (decimal, `Infinity`, or `0x`/`0o`/`0b` digits); 0 when empty, NaN when it
/// is not one. Rust's parser would also read `inf` and `nan`, which JavaScript
/// does not.
fn js_number(s: &str) -> f64 {
    let t = js_trim(s);
    match t {
        "" => return 0.0,
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefixes, radix) in [(["0x", "0X"], 16), (["0o", "0O"], 8), (["0b", "0B"], 2)] {
        if let Some(digits) = prefixes.iter().find_map(|p| t.strip_prefix(p)) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |n, c| {
                n * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0))
            });
        }
    }
    if !t
        .bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return f64::NAN;
    }
    t.parse().unwrap_or(f64::NAN)
}

/// A number as the typed JSON carries it and `crate::spec` reads it back as
/// `f32`: JSON writes a NaN or infinity as `null`, which does not read as a
/// number, and drops the sign of -0.
fn json_number(n: f64) -> CssResult<f32> {
    if !n.is_finite() {
        return fail("expected a number");
    }
    Ok(if n == 0.0 { 0.0 } else { n as f32 })
}

/// A color in sRGB, as `values.mjs`'s `color` reads it: a color outside sRGB
/// fails, since GPUI cannot draw it.
fn srgb_color(value: &str, notes: &mut Vec<String>) -> CssResult<String> {
    let out = to_color(value, notes)?;
    if out.starts_with("display-p3") {
        return fail(format!(
            "\"{value}\" is outside sRGB, which GPUI cannot draw"
        ));
    }
    Ok(out)
}

/// `#RRGGBBAA` as a GPUI color, as `crate::spec` converts it: each channel a
/// byte over 255, through GPUI's `Rgba` into `Hsla`.
fn hex_color(hex: &str) -> CssResult<Hsla> {
    let Some(digits) = hex
        .strip_prefix('#')
        .filter(|d| d.len() == 8 && d.is_ascii())
    else {
        return fail(format!("color {hex} is not #RRGGBBAA"));
    };
    let channel = |at: usize| match u8::from_str_radix(&digits[at..at + 2], 16) {
        Ok(byte) => Ok(f32::from(byte) / 255.0),
        Err(_) => fail(format!("color {hex} is not #RRGGBBAA")),
    };
    Ok(gpui::Rgba {
        r: channel(0)?,
        g: channel(2)?,
        b: channel(4)?,
        a: channel(6)?,
    }
    .into())
}

/// A length in points and whether it was written in rems (`values.mjs`
/// `length`, the `{length, rem}` JSON).
fn points_and_rem(value: &str, notes: &mut Vec<String>) -> CssResult<(f64, bool)> {
    let trimmed = js_trim(value);
    // /^-?[\d.]+rem$/
    let rem = trimmed
        .strip_prefix('-')
        .unwrap_or(trimmed)
        .strip_suffix("rem")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit() || b == b'.'));
    Ok((to_points(trimmed, notes, false)?, rem))
}

/// A length in points as GPUI's, in rems (points ÷ 16) when written in rems,
/// as `crate::spec` reads `{length, rem}`.
fn absolute_length((points, rem): (f64, bool)) -> CssResult<AbsoluteLength> {
    let points = json_number(points)?;
    Ok(if rem {
        AbsoluteLength::Rems(rems(points / 16.0))
    } else {
        AbsoluteLength::Pixels(px(points))
    })
}

/// A duration in milliseconds: `/^(-?[\d.]+)(ms|s)$/`. `values.mjs`
/// `duration`.
fn duration_ms(value: &str) -> CssResult<f64> {
    let v = js_trim(value);
    let (number, scale) = if let Some(number) = v.strip_suffix("ms") {
        (number, 1.0)
    } else if let Some(number) = v.strip_suffix('s') {
        (number, 1000.0)
    } else {
        return fail(format!("\"{value}\" is not a duration"));
    };
    let digits = number.strip_prefix('-').unwrap_or(number);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return fail(format!("\"{value}\" is not a duration"));
    }
    Ok(js_number(number) * scale)
}

/// The CSS easing keywords as cubic Béziers. `values.mjs` `EASE_KEYWORDS`.
const EASE_KEYWORDS: [(&str, [f64; 4]); 5] = [
    ("linear", [0.0, 0.0, 1.0, 1.0]),
    ("ease", [0.25, 0.1, 0.25, 1.0]),
    ("ease-in", [0.42, 0.0, 1.0, 1.0]),
    ("ease-out", [0.0, 0.0, 0.58, 1.0]),
    ("ease-in-out", [0.42, 0.0, 0.58, 1.0]),
];

/// An easing keyword or `cubic-bezier(x1, y1, x2, y2)` as its four points.
/// `values.mjs` `ease`.
fn ease_points(value: &str) -> CssResult<[f64; 4]> {
    let v = js_trim(value);
    if let Some((_, points)) = EASE_KEYWORDS.iter().find(|(name, _)| *name == v) {
        return Ok(*points);
    }
    let Some(inner) = v
        .strip_prefix("cubic-bezier(")
        .and_then(|rest| rest.strip_suffix(')'))
        .filter(|inner| !inner.contains(')'))
    else {
        return fail(format!("\"{value}\" is not a cubic-bezier easing"));
    };
    let points: Vec<f64> = inner.split(',').map(js_number).collect();
    match points.as_slice() {
        &[x1, y1, x2, y2] if !points.iter().any(|p| p.is_nan()) => Ok([x1, y1, x2, y2]),
        _ => fail(format!("\"{value}\" needs four numbers")),
    }
}

/// A `font-family` list as names, one quote stripped from each end of each.
/// `values.mjs` `families`.
fn families(value: &str) -> Vec<String> {
    split_top_level(value, ',')
        .into_iter()
        .map(|family| {
            let family = family.strip_prefix(['"', '\'']).unwrap_or(family);
            let family = family.strip_suffix(['"', '\'']).unwrap_or(family);
            js_trim(family).to_string()
        })
        .collect()
}

/// A `box-shadow` list as GPUI shadow layers, keeping a drop spread, which GPUI
/// draws (`values.mjs` `shadow`). A part that starts like a number, or with
/// `calc(`, is a length; any other part but `inset` is the color.
fn shadow_layers(value: &str, notes: &mut Vec<String>) -> CssResult<Vec<BoxShadow>> {
    if js_trim(value) == "none" {
        return Ok(Vec::new());
    }
    let mut layers = Vec::new();
    for layer in split_top_level(value, ',') {
        let mut lengths = Vec::new();
        let mut layer_color: Option<String> = None;
        let mut inset = false;
        for part in split_top_level(layer, ' ') {
            // /^-?[\d.]/
            let is_number = part
                .strip_prefix('-')
                .unwrap_or(part)
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_digit() || b == b'.');
            if part == "inset" {
                inset = true;
            } else if is_number || part.starts_with("calc(") {
                lengths.push(to_points(part, notes, false)?);
            } else if layer_color.is_some() {
                return fail(format!("shadow layer \"{layer}\" has two colors"));
            } else {
                layer_color = Some(srgb_color(part, notes)?);
            }
        }
        if !(2..=4).contains(&lengths.len()) {
            return fail(format!(
                "shadow layer \"{layer}\" needs two to four lengths"
            ));
        }
        let Some(color) = layer_color else {
            return fail(format!("shadow layer \"{layer}\" has no color"));
        };
        let length = |at: usize| json_number(lengths.get(at).copied().unwrap_or(0.0));
        layers.push(BoxShadow {
            color: hex_color(&color)?,
            offset: Point {
                x: px(length(0)?),
                y: px(length(1)?),
            },
            blur_radius: px(length(2)?),
            spread_radius: px(length(3)?),
            inset,
        });
    }
    Ok(layers)
}

/// The lengths of a padding or margin shorthand, as [`typed`] reads
/// [`ValueKind::Edges`]: the `{edges: {all | inline | block}}` JSON that
/// `crate::spec` turns into a padding style.
#[derive(Clone, Debug, PartialEq)]
pub enum EdgeLengths {
    /// `padding` (or `margin`): top, right, bottom and left, the CSS shorthand
    /// expanded (`4px 8px` is 4, 8, 4, 8).
    All(Edges<AbsoluteLength>),
    /// `paddingInline`: start and end (one value is both).
    Inline(AbsoluteLength, AbsoluteLength),
    /// `paddingBlock`: top and bottom (one value is both).
    Block(AbsoluteLength, AbsoluteLength),
}

/// A CSS edge shorthand (`padding: 4px 8px`) as its lengths. `values.mjs`
/// `edges`.
fn edges(value: &str, notes: &mut Vec<String>, property: Option<&str>) -> CssResult<EdgeLengths> {
    let mut parts = Vec::new();
    for part in split_top_level(value, ' ') {
        parts.push(absolute_length(points_and_rem(part, notes)?)?);
    }
    // No parts writes `[null, …]`, which does not read as a length.
    let Some(&first) = parts.first() else {
        return fail("expected a number");
    };
    let at = |i: usize, or: AbsoluteLength| parts.get(i).copied().unwrap_or(or);
    Ok(match property {
        Some("paddingInline") => EdgeLengths::Inline(first, at(1, first)),
        Some("paddingBlock") => EdgeLengths::Block(first, at(1, first)),
        _ => {
            let right = at(1, first);
            EdgeLengths::All(Edges {
                top: first,
                right,
                bottom: at(2, first),
                left: at(3, right),
            })
        }
    })
}

/// What [`typed`] reads besides the value: `values.mjs`'s `context`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TypedContext<'a> {
    /// The font size, in points, that a length line height divides by: the
    /// size of the style a `--text-*-leading` token belongs to, as
    /// [`to_points`] reads its `--text-*-size` token.
    pub font_size: Option<f64>,
    /// The CSS property an edge shorthand is for (`padding`, `paddingInline`,
    /// `paddingBlock`), which decides its shape.
    pub property: Option<&'a str>,
}

/// A value read by [`typed`].
#[derive(Clone, Debug)]
pub enum Typed {
    /// A token value: every kind but [`ValueKind::Edges`].
    Token(TokenValue),
    /// An edge shorthand, [`ValueKind::Edges`].
    Edges(EdgeLengths),
}

impl Typed {
    /// The token value, or `None` for an edge shorthand.
    pub fn into_token(self) -> Option<TokenValue> {
        match self {
            Typed::Token(value) => Some(value),
            Typed::Edges(_) => None,
        }
    }
}

/// One value, already in one mode (see [`value_for_mode`]) and with every
/// `var()` substituted (see [`substitute_vars`]), read as a `kind` value
/// (`values.mjs` `typed`). The result is the value `crate::spec` reads from
/// the JSON `typed()` writes: `#0064E0` is `TokenValue::Color` of `#0064E0FF`,
/// `0.875rem` is `TokenValue::Length(Rems(0.875))`, `175ms` is a 175 ms
/// `TokenValue::Duration`, and so on.
///
/// Colors outside sRGB fail, since GPUI cannot draw them. A conversion that
/// loses something pushes a note on `notes`; a value with no static form
/// fails.
pub fn typed(
    kind: ValueKind,
    value: &str,
    notes: &mut Vec<String>,
    context: &TypedContext<'_>,
) -> CssResult<Typed> {
    let v = js_trim(value);
    let token = match kind {
        ValueKind::Color => TokenValue::Color(hex_color(&srgb_color(v, notes)?)?),
        ValueKind::Length => TokenValue::Length(absolute_length(points_and_rem(v, notes)?)?),
        ValueKind::Weight => TokenValue::Weight(FontWeight(json_number(to_weight(v, notes)?)?)),
        ValueKind::Leading => {
            TokenValue::Leading(json_number(to_leading(v, context.font_size, notes)?)?)
        }
        ValueKind::Duration => {
            let ms = json_number(duration_ms(v)?)?;
            TokenValue::Duration(Duration::from_micros((ms * 1000.0).round() as u64))
        }
        ValueKind::Ease => {
            let [x1, y1, x2, y2] = ease_points(v)?;
            TokenValue::Ease(CubicBezier(
                json_number(x1)?,
                json_number(y1)?,
                json_number(x2)?,
                json_number(y2)?,
            ))
        }
        ValueKind::Families => TokenValue::Families(FontStack(Cow::Owned(
            families(v).into_iter().map(Cow::Owned).collect(),
        ))),
        ValueKind::Shadow => TokenValue::Shadow(Shadow(Cow::Owned(shadow_layers(v, notes)?))),
        ValueKind::Edges => return Ok(Typed::Edges(edges(v, notes, context.property)?)),
        ValueKind::Keyword => TokenValue::Keyword(Keyword(Cow::Owned(v.to_string()))),
        ValueKind::Number => {
            let n = js_number(v);
            if n.is_nan() {
                return fail(format!("\"{v}\" is not a number"));
            }
            TokenValue::Number(json_number(n)?)
        }
    };
    Ok(Typed::Token(token))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIGHT: ResolvedThemeMode = ResolvedThemeMode::Light;
    const DARK: ResolvedThemeMode = ResolvedThemeMode::Dark;

    fn color(css: &str) -> String {
        to_color(css, &mut Vec::new()).unwrap_or_else(|e| panic!("{css}: {e}"))
    }

    fn points(css: &str) -> f64 {
        to_points(css, &mut Vec::new(), false).unwrap_or_else(|e| panic!("{css}: {e}"))
    }

    fn reason<T: fmt::Debug>(result: CssResult<T>) -> String {
        result.expect_err("fails").reason
    }

    // Upstream swiftIOSTheme.test.ts, "swift theme generator: values (G7)":
    // "reads the color %s as %s", every row with its note.
    #[test]
    fn reads_every_color_syntax() {
        let rows: [(&str, &str, bool); 59] = [
            ("#0064E0", "#0064E0FF", false),
            ("#abc8", "#AABBCC88", false),
            ("rebeccapurple", "#663399FF", false),
            ("transparent", "#00000000", false),
            ("rgba(5, 54, 89, 0.1)", "#0536591A", false),
            ("rgb(10% 20% 30% / 50%)", "#1A334D80", false),
            ("rgb(none 0 0)", "#000000FF", false),
            ("rgb(calc(255 / 2) 0 0)", "#800000FF", false),
            ("hsl(210 100% 50%)", "#0080FFFF", false),
            ("hsla(120, 100%, 25%, 0.5)", "#00800080", false),
            ("hsl(0.5turn 50% 50%)", "#40BFBFFF", false),
            ("hsl(200grad 50% 50%)", "#40BFBFFF", false),
            ("hsl(3.14159rad 50% 50%)", "#40BFBFFF", false),
            ("hwb(194 0% 0% / .5)", "#00C4FF80", false),
            ("hwb(0 60% 60%)", "#808080FF", false),
            ("lab(29.2345% 39.3825 20.0664)", "#7D2329FF", false),
            ("lch(52.2345% 72.2 56.2)", "#C65D06FF", false),
            ("oklab(40.101% 0.1147 0.0453)", "#7D2329FF", false),
            ("oklch(40.101% 0.12332 21.555)", "#7D2329FF", false),
            ("oklch(0 0 0 / 5%)", "#0000000D", false),
            ("oklch(50% 0 0)", "#636363FF", false),
            ("color(srgb 0.5 0.2 0.1)", "#80331AFF", false),
            ("color(srgb-linear 0.5 0.2 0.1)", "#BC7C59FF", false),
            ("color(display-p3 0.4 0.6 0.2)", "#559B17FF", false),
            ("color(a98-rgb 0.4 0.6 0.2)", "#439A22FF", false),
            ("color(prophoto-rgb 0.4 0.6 0.2)", "#52B204FF", false),
            ("color(rec2020 0.4 0.6 0.2)", "#4AA829FF", false),
            ("color(rec2020 0.5 0.5 0.5)", "#8B8B8BFF", false),
            ("color(xyz-d65 0.2 0.3 0.4)", "display-p3 #44A4A3FF", false),
            ("color(xyz-d50 0.2 0.3 0.4)", "display-p3 #2CA5BAFF", false),
            ("color(display-p3 1 0 0)", "display-p3 #FF0000FF", false),
            ("color-mix(in srgb, red, blue)", "#800080FF", false),
            ("color-mix(in srgb, red 30%, blue)", "#4D00B3FF", false),
            ("color-mix(in srgb, red 20%, blue 20%)", "#80008066", false),
            (
                "color-mix(in srgb, rgb(255 0 0 / .5), blue)",
                "#5500AABF",
                false,
            ),
            (
                "color-mix(in srgb, rgb(none 50 0), rgb(10 200 0))",
                "#0A7D00FF",
                false,
            ),
            ("color-mix(in srgb-linear, red, blue)", "#BC00BCFF", false),
            ("color-mix(in xyz, red, blue)", "#BC00BCFF", false),
            ("color-mix(in display-p3, red, blue)", "#800A91FF", false),
            ("color-mix(in oklab, red, blue)", "#8C53A2FF", false),
            ("color-mix(red, blue)", "#8C53A2FF", false),
            (
                "color-mix(in lab, red, blue)",
                "display-p3 #B01185FF",
                false,
            ),
            ("color-mix(in hsl, red, blue)", "#FF00FFFF", false),
            (
                "color-mix(in oklch, red, blue)",
                "display-p3 #AA00BBFF",
                true,
            ),
            (
                "color-mix(in oklch longer hue, red, blue)",
                "display-p3 #009000FF",
                true,
            ),
            ("rgb(from #123456 calc(r + 10) g b)", "#1C3456FF", false),
            ("rgb(from red r g b / 50%)", "#FF000080", false),
            ("hsl(from blue calc(h + 180) s l)", "#FFFF00FF", false),
            ("oklch(0.7 0.4 150)", "display-p3 #00D100FF", true),
            ("oklch(0.9 0.4 300)", "display-p3 #FB7CFFFF", true),
            (
                "rgb(from color(display-p3 1 0 0) r g b / 50%)",
                "display-p3 #FF000080",
                false,
            ),
            (
                "hwb(from color(display-p3 1 0 0) h w b)",
                "display-p3 #FF0000FF",
                false,
            ),
            (
                "color-mix(in srgb, rgb(255 0 0 / none), rgb(0 0 255 / 0.5))",
                "#80008080",
                false,
            ),
            (
                "color-mix(in srgb, rgb(255 0 0 / none), rgb(0 0 255 / none))",
                "#80008000",
                false,
            ),
            (
                "color-mix(in lab, rgb(0 0 none), lab(50 20 30))",
                "#4F3525FF",
                false,
            ),
            (
                "color-mix(in lab, hsl(0 50% none), lab(50 20 30))",
                "#8E705EFF",
                false,
            ),
            ("hsl(120, 200%, 40%)", "#00CC00FF", false),
            ("hsl(120 200% 40%)", "#00CC00FF", false),
            (
                "color-mix(in oklch, oklch(0.6 0.1 400), oklch(0.6 0.1 20))",
                "#B4685CFF",
                false,
            ),
        ];
        for (css, expected, clipped) in rows {
            let mut notes = Vec::new();
            assert_eq!(to_color(css, &mut notes).as_deref(), Ok(expected), "{css}");
            let expected_notes: Vec<String> = if clipped {
                vec![OUTSIDE_P3.to_string()]
            } else {
                Vec::new()
            };
            assert_eq!(notes, expected_notes, "{css}");
        }
    }

    // Upstream: "reads lengths, weights, and line heights".
    #[test]
    fn reads_lengths_weights_and_line_heights() {
        let lengths = [
            ("0.875rem", 14.0),
            ("32px", 32.0),
            ("12pt", 16.0),
            ("1in", 96.0),
            ("0", 0.0),
            ("calc(1rem + 2px)", 18.0),
            ("calc((1rem - 2px) * 2)", 28.0),
            ("max(4px, 0.5rem)", 8.0),
            ("clamp(2px, 1rem, 12px)", 12.0),
        ];
        for (css, expected) in lengths {
            assert_eq!(points(css), expected, "{css}");
        }
        // thin, medium, and thick are lengths only for a border width.
        assert_eq!(to_points("thin", &mut Vec::new(), true), Ok(1.0));
        assert_eq!(to_points("medium", &mut Vec::new(), true), Ok(3.0));
        assert!(
            reason(to_points("medium", &mut Vec::new(), false)).contains("names a border width")
        );
        let mut notes = Vec::new();
        assert_eq!(to_points("1.5em", &mut notes, false), Ok(24.0));
        assert_eq!(notes, ["em read against the 16px root font size"]);
        assert_eq!(to_weight("bold", &mut Vec::new()), Ok(700.0));
        assert_eq!(to_weight("normal", &mut Vec::new()), Ok(400.0));
        assert_eq!(to_weight("650", &mut Vec::new()), Ok(650.0));
        assert_eq!(to_weight("bolder", &mut notes), Ok(700.0));
        let leading = |css: &str| to_leading(css, Some(14.0), &mut Vec::new());
        assert_eq!(leading("1.5"), Ok(1.5));
        assert_eq!(leading("150%"), Ok(1.5));
        assert_eq!(leading("21px"), Ok(1.5));
        // An em counts against the style's own size, not the 16-point root.
        assert_eq!(leading("1.5em"), Ok(1.5));
        assert_eq!(leading("calc(1em + 7px)"), Ok(1.5));
        assert_eq!(to_leading("normal", Some(14.0), &mut notes), Ok(1.2));
        assert_eq!(
            notes[1..],
            [
                "bolder read against normal (400), the root's inherited weight",
                "normal depends on the font; read as 1.2",
            ]
        );
    }

    // Upstream: "fails on a value with no static form, saying why".
    #[test]
    fn fails_on_a_value_with_no_static_form_saying_why() {
        let none = &mut Vec::new();
        let cases: [(CssResult<String>, &str); 11] = [
            (
                to_color("currentColor", none),
                "currentColor depends on where",
            ),
            (to_color("Canvas", none), "system color"),
            (
                to_color("var(--brand)", none),
                "var(--brand) is not a token",
            ),
            (to_color("color(--custom 1 0 0)", none), "not a color space"),
            (
                to_points("50%", none, false).map(|p| p.to_string()),
                "percentage depends on the box",
            ),
            (
                to_points("10vh", none, false).map(|p| p.to_string()),
                "viewport units",
            ),
            (
                to_points("2ch", none, false).map(|p| p.to_string()),
                "font-relative units",
            ),
            (
                to_points("calc(1px + 50%)", none, false).map(|p| p.to_string()),
                "adds length and percent",
            ),
            (
                to_points("env(safe-area-inset-top)", none, false).map(|p| p.to_string()),
                "env() is known only",
            ),
            (
                to_weight("1001", none).map(|w| w.to_string()),
                "from 1 to 1000",
            ),
            (
                to_shadow("inset 0 1px 2px #000", none).map(|s| format!("{s:?}")),
                "SwiftUI draws only inset rings",
            ),
        ];
        for (result, message) in cases {
            let reason = reason(result);
            assert!(reason.contains(message), "{reason} does not say {message}");
        }
    }

    // Upstream: "reads box-shadow lists as drop layers and inset rings".
    #[test]
    fn reads_shadows_as_drop_layers_and_inset_rings() {
        let neutral_low = "0 2px 4px light-dark(oklch(0 0 0 / 5%), oklch(0 0 0 / 25%)), 0 4px 8px light-dark(oklch(0 0 0 / 10%), oklch(0 0 0 / 40%)), inset 0 0 0 1px light-dark(transparent, oklch(1 0 0 / 8%))";
        let layer = |x, y, blur, spread, is_inset, color: &str| ShadowLayer {
            x,
            y,
            blur,
            spread,
            is_inset,
            color: color.to_string(),
        };
        let light = value_for_mode(neutral_low, LIGHT).unwrap();
        assert_eq!(
            to_shadow(&light, &mut Vec::new()),
            Ok(vec![
                layer(0.0, 2.0, 4.0, 0.0, false, "#0000000D"),
                layer(0.0, 4.0, 8.0, 0.0, false, "#0000001A"),
                layer(0.0, 0.0, 0.0, 1.0, true, "#00000000"),
            ])
        );
        let dark = value_for_mode(neutral_low, DARK).unwrap();
        assert_eq!(
            to_shadow(&dark, &mut Vec::new()).unwrap()[2].color,
            "#FFFFFF14"
        );
        assert_eq!(to_shadow("none", &mut Vec::new()), Ok(Vec::new()));
        let mut notes = Vec::new();
        assert_eq!(
            to_shadow("#000 0 calc(1px + 1px) 0.25rem 1px", &mut notes),
            Ok(vec![layer(0.0, 2.0, 4.0, 0.0, false, "#000000FF")])
        );
        assert_eq!(
            notes,
            ["drop layer \"#000 0 calc(1px + 1px) 0.25rem 1px\" drawn without its spread"]
        );
    }

    // Upstream: "takes light-dark() per mode at any depth (G1)".
    #[test]
    fn takes_light_dark_per_mode_at_any_depth() {
        assert_eq!(
            value_for_mode("light-dark(#000, #fff)", LIGHT).as_deref(),
            Ok("#000")
        );
        assert_eq!(
            value_for_mode("light-dark(#000, #fff)", DARK).as_deref(),
            Ok("#fff")
        );
        let mixed = "color-mix(in srgb, light-dark(red, blue), white)";
        assert_eq!(color(&value_for_mode(mixed, LIGHT).unwrap()), "#FF8080FF");
        assert_eq!(color(&value_for_mode(mixed, DARK).unwrap()), "#8080FFFF");
        assert_eq!(value_for_mode("#123", DARK).as_deref(), Ok("#123"));
    }

    // Upstream: "generates a [light, dark] tuple and a light-dark() value per
    // mode", on the values its theme holds: the accent's halves, an oklch() and
    // a nested mix, and 4px and 0.75rem.
    #[test]
    fn reads_each_mode_of_a_pair() {
        let accent = "light-dark(#000, #fff)";
        assert_eq!(color(&value_for_mode(accent, LIGHT).unwrap()), "#000000FF");
        assert_eq!(color(&value_for_mode(accent, DARK).unwrap()), "#FFFFFFFF");
        // 0.4901…, 0.1372…, 0.1607… are 125, 35 and 41 over 255.
        assert_eq!(color("oklch(40.101% 0.12332 21.555)"), "#7D2329FF");
        let border = "color-mix(in srgb, light-dark(red, blue) 50%, white)";
        assert_eq!(color(&value_for_mode(border, DARK).unwrap()), "#8080FFFF");
        assert_eq!(points("4px"), 4.0);
        assert_eq!(points("0.75rem"), 12.0);
    }

    // Upstream: "writes a color outside sRGB as SwiftUI's Display P3 color" and
    // "keeps a relative color in its origin's gamut through core's var()".
    #[test]
    fn a_color_outside_srgb_stays_display_p3() {
        assert_eq!(
            color("color(display-p3 1 0 0 / 50%)"),
            "display-p3 #FF000080"
        );
        assert_eq!(
            color("rgb(from color(display-p3 1 0 0) r g b / 50%)"),
            "display-p3 #FF000080"
        );
    }

    // Upstream: "turns a length line height into a multiplier of its style size".
    #[test]
    fn a_length_line_height_is_a_multiple_of_its_size() {
        assert_eq!(to_leading("20px", Some(16.0), &mut Vec::new()), Ok(1.25));
        assert!(
            reason(to_leading("20px", None, &mut Vec::new()))
                .contains("needs its style's font size")
        );
        assert!(
            reason(to_leading("20px", Some(0.0), &mut Vec::new()))
                .contains("needs its style's font size")
        );
    }

    // Expected values from running upstream's toLeading under Node 24: the
    // quotient is 2^-18 = 3.814697265625e-6, an exact tie at 12 digits, which
    // toPrecision breaks up; 20 / 14 rounds to 12 digits.
    #[test]
    fn a_length_line_height_rounds_to_12_digits_as_javascript_does() {
        assert_eq!(
            to_leading("0.000011444091796875px", Some(3.0), &mut Vec::new()),
            Ok(0.00000381469726563)
        );
        assert_eq!(
            to_leading("20px", Some(14.0), &mut Vec::new()),
            Ok(1.42857142857)
        );
        assert_eq!(
            to_leading("10px", Some(3.0), &mut Vec::new()),
            Ok(3.33333333333)
        );
    }

    // Upstream: "reads + and - in calc() only with a space on each side, as CSS
    // does".
    #[test]
    fn calc_needs_a_space_around_plus_and_minus() {
        assert_eq!(points("calc(1rem - 2px)"), 14.0);
        assert!(
            reason(to_points("calc(4px+2px)", &mut Vec::new(), false))
                .contains("space on each side of +")
        );
        assert!(
            reason(to_points("calc(1rem-2px)", &mut Vec::new(), false))
                .contains("space on each side of -")
        );
    }

    // Upstream: "fails the comma syntax CSS does not allow".
    #[test]
    fn fails_the_comma_syntax_css_does_not_allow() {
        let cases = [
            ("hwb(0, 0%, 0%)", "no comma syntax"),
            ("rgb(255, none, 0)", "cannot hold none"),
            ("rgba(255, 0, 0, none)", "cannot hold none"),
            ("rgb(255, 50%, 0)", "all numbers or all percentages"),
            (
                "hsl(120, 50, 40)",
                "saturation and lightness as percentages",
            ),
        ];
        for (css, message) in cases {
            let error = to_color(css, &mut Vec::new()).expect_err(css);
            assert!(error.reason.contains(message), "{css}: {}", error.reason);
            assert!(!error.has_no_static_form, "{css}");
        }
    }

    // Upstream: "falls back from --font-family-body: inherit" and
    // '"Figtree", env(x)' (no static form), and 'fails "Figtree", inherit as a
    // form it does not read'.
    #[test]
    fn a_family_list_is_static_unless_it_depends_on_the_page() {
        let static_reason = |value: &str| {
            let error = check_family_is_static(value).expect_err(value);
            assert!(error.has_no_static_form, "{value}");
            error.reason
        };
        assert_eq!(
            static_reason("inherit"),
            "inherit depends on where the token is used"
        );
        assert_eq!(
            static_reason("\"Figtree\", env(x)"),
            "env() is known only on the page"
        );
        assert_eq!(
            static_reason("var(--x)"),
            "var(--x) is not a token core's resolver knows"
        );
        let listed = check_family_is_static("\"Figtree\", inherit").expect_err("listed keyword");
        assert_eq!(
            listed.reason,
            "inherit cannot be one name in a font-family list"
        );
        assert!(!listed.has_no_static_form);
        // Expected from reading checkFamilyIsStatic: quoted names are blanked
        // first, and env() needs a word boundary before it.
        assert_eq!(check_family_is_static("\"var(--x)\", serif"), Ok(()));
        assert_eq!(check_family_is_static("myenv(x), serif"), Ok(()));
        assert_eq!(
            check_family_is_static("-apple-system, \"Segoe UI\", sans-serif"),
            Ok(())
        );
    }

    // Expected values from reading the tokenizer and math: a sign right after
    // a value is an operator, and calc() fails on what it cannot add.
    #[test]
    fn calc_reads_math_as_css_does() {
        assert_eq!(points("calc(-1px - -2px)"), 1.0);
        assert_eq!(points("calc(1px - (2px + 3px))"), -4.0);
        assert_eq!(points("calc(10px / 4)"), 2.5);
        assert_eq!(points("min(4px, 0.5rem, 3px)"), 3.0);
        assert!(
            reason(to_points("calc(2px * 3px)", &mut Vec::new(), false))
                .contains("multiplies two dimensions")
        );
        assert!(reason(to_points("calc(4px / 0)", &mut Vec::new(), false)).contains("by zero"));
        assert!(
            reason(to_points("min(1px, 2deg)", &mut Vec::new(), false))
                .contains("min() mixes length and angle")
        );
        assert_eq!(
            reason(to_points("calc(1px, 2px)", &mut Vec::new(), false)),
            "calc takes one expression"
        );
        assert_eq!(
            reason(to_points("1px 2px", &mut Vec::new(), false)),
            "\"1px 2px\" is not one value"
        );
        // Upstream crashes with a TypeError here; the port says why.
        assert_eq!(
            reason(to_points("calc(1px +)", &mut Vec::new(), false)),
            "an expression ends early"
        );
    }

    // Expected values from running tools/export/lib/values.mjs under Node 24.
    #[test]
    fn token_kinds_follow_the_token_name() {
        let cases = [
            ("--shadow-low", ValueKind::Shadow),
            ("--duration-fast", ValueKind::Duration),
            ("--x-duration", ValueKind::Duration),
            ("--ease-exit", ValueKind::Ease),
            ("--font-family-body", ValueKind::Families),
            ("--font-weight-bold", ValueKind::Weight),
            ("--text-body-weight", ValueKind::Weight),
            ("--text--weight", ValueKind::Weight),
            ("--text-weight", ValueKind::Length),
            ("--text-body-leading", ValueKind::Leading),
            ("--focus-outline-style", ValueKind::Keyword),
            ("--color-accent", ValueKind::Color),
            ("--focus-outline-color", ValueKind::Color),
            ("--colorx", ValueKind::Length),
            ("--spacing-1", ValueKind::Length),
            ("--text-a\nb-weight", ValueKind::Length),
        ];
        for (name, kind) in cases {
            assert_eq!(token_kind(name), kind, "{name:?}");
        }
    }

    // Expected values from running values.mjs under Node 24.
    #[test]
    fn property_kinds_follow_the_property() {
        let cases = [
            ("backgroundColor", ValueKind::Color),
            ("outlineColor", ValueKind::Color),
            ("fontFamily", ValueKind::Families),
            ("fontWeight", ValueKind::Weight),
            ("lineHeight", ValueKind::Leading),
            ("boxShadow", ValueKind::Shadow),
            ("borderStyle", ValueKind::Keyword),
            ("paddingInline", ValueKind::Edges),
            ("margin", ValueKind::Edges),
            ("opacity", ValueKind::Number),
            ("width", ValueKind::Length),
            ("--color-accent", ValueKind::Color),
        ];
        for (property, kind) in cases {
            assert_eq!(property_kind(property), kind, "{property}");
        }
    }

    // Expected values from running values.mjs under Node 24.
    #[test]
    fn split_top_level_splits_outside_parentheses_and_drops_empty_parts() {
        assert_eq!(
            split_top_level("1px 2px rgba(0, 0, 0, 0.1)", ' '),
            ["1px", "2px", "rgba(0, 0, 0, 0.1)"]
        );
        assert_eq!(split_top_level("a, b(c, d), e", ','), ["a", "b(c, d)", "e"]);
        assert_eq!(split_top_level("a,,b", ','), ["a", "b"]);
        assert_eq!(split_top_level("a\tb\nc", ' '), ["a", "b", "c"]);
        assert_eq!(split_top_level("(a,b),c)", ','), ["(a,b)", "c)"]);
        assert!(split_top_level("  ", ' ').is_empty());
    }

    // Expected values from running values.mjs under Node 24.
    #[test]
    fn substitute_vars_follows_references_and_fallbacks() {
        let tokens = [
            ("--a", "4px"),
            ("--b", "var(--a)"),
            ("--loop", "var(--loop)"),
            ("--a", "5px"),
        ];
        let sub = |value: &str| substitute_vars(value, &tokens);
        assert_eq!(sub("x var(--a) y").as_deref(), Ok("x 5px y"));
        assert_eq!(sub("var(--b)").as_deref(), Ok("5px"));
        assert_eq!(sub("var(--missing, 1px)").as_deref(), Ok("1px"));
        assert_eq!(sub("var(--missing, a, b)").as_deref(), Ok("a, b"));
        assert_eq!(
            sub("calc(var(--a) + var(--b))").as_deref(),
            Ok("calc(5px + 5px)")
        );
        assert_eq!(sub("var( --a )").as_deref(), Ok("5px"));
        assert_eq!(
            reason(sub("var(--missing)")),
            "var(--missing) is not defined"
        );
        assert_eq!(
            reason(sub("var(--loop)")),
            "var() nesting too deep in \"var(--loop)\""
        );
        // An unclosed var( loses its last character, as upstream's slice does.
        assert_eq!(reason(sub("var(--a")), "var(--) is not defined");
    }

    fn typed_token(kind: ValueKind, value: &str) -> CssResult<TokenValue> {
        typed(kind, value, &mut Vec::new(), &TypedContext::default())
            .map(|typed| typed.into_token().expect("a token value"))
    }

    // Expected values from running values.mjs's typed() under Node 24, read
    // the way crate::spec reads its JSON.
    #[test]
    fn typed_reads_each_kind_as_the_spec_reader_does() {
        let TokenValue::Color(accent) = typed_token(ValueKind::Color, "#0064E0").unwrap() else {
            panic!("a color");
        };
        let expected: Hsla = gpui::Rgba {
            r: 0.0,
            g: 100.0 / 255.0,
            b: 224.0 / 255.0,
            a: 1.0,
        }
        .into();
        assert_eq!(accent, expected);
        assert!(
            reason(typed_token(ValueKind::Color, "color(display-p3 1 0 0)"))
                .contains("outside sRGB")
        );
        let length = |value| match typed_token(ValueKind::Length, value) {
            Ok(TokenValue::Length(length)) => length,
            other => panic!("{value}: {other:?}"),
        };
        assert!(length("0.875rem") == AbsoluteLength::Rems(rems(0.875)));
        assert!(length("-0.5rem") == AbsoluteLength::Rems(rems(-0.5)));
        assert!(length("1.5em") == AbsoluteLength::Pixels(px(24.0)));
        assert!(length("1e1rem") == AbsoluteLength::Pixels(px(160.0)));
        assert!(length(".5rem") == AbsoluteLength::Rems(rems(0.5)));
        let duration = |value| match typed_token(ValueKind::Duration, value) {
            Ok(TokenValue::Duration(duration)) => duration,
            other => panic!("{value}: {other:?}"),
        };
        assert_eq!(duration("175ms"), Duration::from_millis(175));
        assert_eq!(duration("0.3s"), Duration::from_millis(300));
        assert_eq!(duration("5.s"), Duration::from_secs(5));
        assert_eq!(duration("-5ms"), Duration::ZERO);
        assert_eq!(
            reason(typed_token(ValueKind::Duration, "5")),
            "\"5\" is not a duration"
        );
        // Upstream writes {duration: null}, which the spec reader rejects.
        assert_eq!(
            reason(typed_token(ValueKind::Duration, "1.2.3s")),
            "expected a number"
        );
        let ease = |value| match typed_token(ValueKind::Ease, value) {
            Ok(TokenValue::Ease(ease)) => ease,
            other => panic!("{value}: {other:?}"),
        };
        assert_eq!(ease("ease-in-out"), CubicBezier(0.42, 0.0, 0.58, 1.0));
        assert_eq!(
            ease("cubic-bezier(0.24, 1, 0.4, 1)"),
            CubicBezier(0.24, 1.0, 0.4, 1.0)
        );
        assert_eq!(ease("cubic-bezier(,,,)"), CubicBezier(0.0, 0.0, 0.0, 0.0));
        assert_eq!(
            reason(typed_token(ValueKind::Ease, "cubic-bezier(1, 2, 3)")),
            "\"cubic-bezier(1, 2, 3)\" needs four numbers"
        );
        assert_eq!(
            reason(typed_token(ValueKind::Ease, "steps(4)")),
            "\"steps(4)\" is not a cubic-bezier easing"
        );
        let families = |value| match typed_token(ValueKind::Families, value) {
            Ok(TokenValue::Families(FontStack(names))) => {
                names.iter().map(|n| n.to_string()).collect::<Vec<_>>()
            }
            other => panic!("{value}: {other:?}"),
        };
        assert_eq!(
            families("\"SF Mono\", Monaco, Consolas, monospace"),
            ["SF Mono", "Monaco", "Consolas", "monospace"]
        );
        assert_eq!(families("'quoted'x"), ["quoted'x"]);
        assert_eq!(families("\""), [""]);
        assert!(matches!(
            typed_token(ValueKind::Keyword, " dashed "),
            Ok(TokenValue::Keyword(Keyword(k))) if k == "dashed"
        ));
        let number = |value| match typed_token(ValueKind::Number, value) {
            Ok(TokenValue::Number(n)) => n,
            other => panic!("{value}: {other:?}"),
        };
        assert_eq!(number("0.5"), 0.5);
        assert_eq!(number(""), 0.0);
        assert_eq!(number("0x10"), 16.0);
        assert_eq!(number("+3"), 3.0);
        assert_eq!(
            reason(typed_token(ValueKind::Number, "inf")),
            "\"inf\" is not a number"
        );
        assert_eq!(
            reason(typed_token(ValueKind::Number, "1_000")),
            "\"1_000\" is not a number"
        );
        let weight = typed(
            ValueKind::Weight,
            "bolder",
            &mut Vec::new(),
            &TypedContext::default(),
        );
        assert!(
            matches!(weight, Ok(Typed::Token(TokenValue::Weight(FontWeight(w)))) if w == 700.0)
        );
        let leading = TypedContext {
            font_size: Some(14.0),
            property: None,
        };
        let read = typed(ValueKind::Leading, "21px", &mut Vec::new(), &leading);
        assert!(matches!(read, Ok(Typed::Token(TokenValue::Leading(l))) if l == 1.5));
    }

    // Expected values from running values.mjs's typed() under Node 24: a drop
    // spread is kept, an inset ring is an inset layer, and a layer needs two to
    // four lengths and one color.
    #[test]
    fn typed_shadows_keep_the_drop_spread() {
        let TokenValue::Shadow(Shadow(layers)) = typed_token(
            ValueKind::Shadow,
            "0 2px 4px 1px red, inset 0px 0px 0px 2px rgba(1, 113, 227, 0.5)",
        )
        .unwrap() else {
            panic!("a shadow");
        };
        let red: Hsla = gpui::Rgba {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }
        .into();
        let selected: Hsla = gpui::Rgba {
            r: 1.0 / 255.0,
            g: 113.0 / 255.0,
            b: 227.0 / 255.0,
            a: 128.0 / 255.0,
        }
        .into();
        assert_eq!(
            layers.to_vec(),
            [
                BoxShadow {
                    color: red,
                    offset: Point {
                        x: px(0.0),
                        y: px(2.0)
                    },
                    blur_radius: px(4.0),
                    spread_radius: px(1.0),
                    inset: false,
                },
                BoxShadow {
                    color: selected,
                    offset: Point {
                        x: px(0.0),
                        y: px(0.0)
                    },
                    blur_radius: px(0.0),
                    spread_radius: px(2.0),
                    inset: true,
                },
            ]
        );
        assert!(
            matches!(typed_token(ValueKind::Shadow, "none"), Ok(TokenValue::Shadow(Shadow(l))) if l.is_empty())
        );
        assert_eq!(
            reason(typed_token(ValueKind::Shadow, "NONE")),
            "\"none\" is not a color"
        );
        assert_eq!(
            reason(typed_token(ValueKind::Shadow, "1px red")),
            "shadow layer \"1px red\" needs two to four lengths"
        );
        assert_eq!(
            reason(typed_token(ValueKind::Shadow, "0 1px 2px")),
            "shadow layer \"0 1px 2px\" has no color"
        );
        assert_eq!(
            reason(typed_token(ValueKind::Shadow, "0 1px red blue")),
            "shadow layer \"0 1px red blue\" has two colors"
        );
    }

    // Expected values from running values.mjs's typed() under Node 24.
    #[test]
    fn typed_edges_expand_the_shorthand() {
        let edges = |value: &str, property: &str| {
            let context = TypedContext {
                font_size: None,
                property: Some(property),
            };
            match typed(ValueKind::Edges, value, &mut Vec::new(), &context) {
                Ok(Typed::Edges(edges)) => edges,
                other => panic!("{value}: {other:?}"),
            }
        };
        let p = |v: f32| AbsoluteLength::Pixels(px(v));
        assert_eq!(
            edges("4px 8px", "padding"),
            EdgeLengths::All(Edges {
                top: p(4.0),
                right: p(8.0),
                bottom: p(4.0),
                left: p(8.0)
            })
        );
        assert_eq!(
            edges("1px 2px 3px", "padding"),
            EdgeLengths::All(Edges {
                top: p(1.0),
                right: p(2.0),
                bottom: p(3.0),
                left: p(2.0)
            })
        );
        assert_eq!(
            edges("1rem 2px", "margin"),
            EdgeLengths::All(Edges {
                top: AbsoluteLength::Rems(rems(1.0)),
                right: p(2.0),
                bottom: AbsoluteLength::Rems(rems(1.0)),
                left: p(2.0),
            })
        );
        assert_eq!(
            edges("1px", "paddingInline"),
            EdgeLengths::Inline(p(1.0), p(1.0))
        );
        assert_eq!(
            edges("4px 8px", "paddingBlock"),
            EdgeLengths::Block(p(4.0), p(8.0))
        );
    }
}
