//! ICU MessageFormat, the syntax of every catalog message: upstream formats
//! them with `intl-messageformat`. Supported, following it:
//!
//! - `{name}`: the value as is (a number without locale formatting).
//! - `{name, number}` and the skeletons `::group-off` and `:: .0` (fraction
//!   digits), with locale digits and separators.
//! - `{name, plural, [offset:n] =0 {..} one {..} other {..}}` and
//!   `selectordinal`, with `#` for the number less the offset.
//! - `{name, select, a {..} other {..}}`.
//! - Apostrophe quoting: `''` is an apostrophe, and `'{'` quotes syntax.

use std::borrow::Cow;
use std::fmt::Write as _;

/// A value a message refers to by name.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg<'a> {
    /// Text, for plain arguments and `select`.
    Str(Cow<'a, str>),
    /// A number, for `number`, `plural` and `selectordinal`.
    Num(f64),
}

impl<'a> From<&'a str> for Arg<'a> {
    fn from(value: &'a str) -> Self {
        Arg::Str(Cow::Borrowed(value))
    }
}

impl From<String> for Arg<'_> {
    fn from(value: String) -> Self {
        Arg::Str(Cow::Owned(value))
    }
}

macro_rules! num_arg {
    ($($ty:ty),*) => {$(
        impl From<$ty> for Arg<'_> {
            fn from(value: $ty) -> Self {
                Arg::Num(value as f64)
            }
        }
    )*};
}
num_arg!(f64, f32, i64, i32, u64, u32, usize, isize, u16, i16, u8, i8);

/// Number formatting options from a `number` argument's skeleton.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NumberStyle {
    /// `group-off`: no grouping separators.
    pub no_grouping: bool,
    /// `.00`: exactly this many fraction digits.
    pub fraction_digits: Option<u8>,
}

/// The CLDR plural categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    fn keyword(self) -> &'static str {
        match self {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        }
    }
}

/// The locale services a message needs.
pub trait LocaleFormat {
    /// Formats a number with the locale's digits and separators.
    fn number(&self, value: f64, style: NumberStyle) -> String;
    /// The plural category of a number: cardinal, or ordinal for
    /// `selectordinal`.
    fn plural(&self, value: f64, ordinal: bool) -> PluralCategory;
}

#[derive(Clone, Debug, PartialEq)]
enum Part {
    Text(String),
    Arg(String),
    Number(String, NumberStyle),
    Plural {
        name: String,
        offset: f64,
        ordinal: bool,
        cases: Vec<(Selector, Vec<Part>)>,
    },
    Select {
        name: String,
        cases: Vec<(String, Vec<Part>)>,
    },
    Pound,
}

#[derive(Clone, Debug, PartialEq)]
enum Selector {
    Exact(f64),
    Keyword(String),
}

/// A message that does not parse; it is shown as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub reason: &'static str,
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn error(&self, reason: &'static str) -> ParseError {
        ParseError {
            offset: self.pos,
            reason,
        }
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn word(&mut self) -> &'a str {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| !c.is_whitespace() && !matches!(c, '{' | '}' | ','))
        {
            self.bump();
        }
        &self.src[start..self.pos]
    }

    /// A message up to an unmatched `}` or the end. `in_plural` makes `#` the
    /// number.
    fn message(&mut self, in_plural: bool) -> Result<Vec<Part>, ParseError> {
        let mut parts = Vec::new();
        let mut text = String::new();
        while let Some(c) = self.peek() {
            match c {
                '}' => break,
                '{' => {
                    if !text.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut text)));
                    }
                    self.bump();
                    parts.push(self.argument()?);
                }
                '#' if in_plural => {
                    if !text.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut text)));
                    }
                    self.bump();
                    parts.push(Part::Pound);
                }
                '\'' => {
                    self.bump();
                    match self.peek() {
                        Some('\'') => {
                            self.bump();
                            text.push('\'');
                        }
                        Some('{' | '}' | '|') => self.quoted(&mut text),
                        Some('#') if in_plural => self.quoted(&mut text),
                        _ => text.push('\''),
                    }
                }
                _ => {
                    self.bump();
                    text.push(c);
                }
            }
        }
        if !text.is_empty() {
            parts.push(Part::Text(text));
        }
        Ok(parts)
    }

    /// Quoted literal text after an opening apostrophe, through the closing
    /// one; `''` inside is an apostrophe.
    fn quoted(&mut self, text: &mut String) {
        while let Some(c) = self.bump() {
            if c == '\'' {
                if self.peek() == Some('\'') {
                    self.bump();
                    text.push('\'');
                } else {
                    return;
                }
            } else {
                text.push(c);
            }
        }
    }

    /// The inside of `{ .. }`, after the `{`, through the closing `}`.
    fn argument(&mut self) -> Result<Part, ParseError> {
        self.skip_ws();
        let name = self.word().to_string();
        if name.is_empty() {
            return Err(self.error("argument without a name"));
        }
        self.skip_ws();
        let part = match self.bump() {
            Some('}') => return Ok(Part::Arg(name)),
            Some(',') => {
                self.skip_ws();
                match self.word() {
                    "number" => self.number(name)?,
                    "plural" => self.plural(name, false)?,
                    "selectordinal" => self.plural(name, true)?,
                    "select" => self.select(name)?,
                    _ => return Err(self.error("unsupported argument type")),
                }
            }
            _ => return Err(self.error("expected `,` or `}`")),
        };
        self.skip_ws();
        match self.bump() {
            Some('}') => Ok(part),
            _ => Err(self.error("expected `}`")),
        }
    }

    fn number(&mut self, name: String) -> Result<Part, ParseError> {
        self.skip_ws();
        let mut style = NumberStyle::default();
        if self.peek() == Some(',') {
            self.bump();
            let start = self.pos;
            while self.peek().is_some_and(|c| c != '}') {
                self.bump();
            }
            let skeleton = self.src[start..self.pos].trim();
            let skeleton = skeleton
                .strip_prefix("::")
                .ok_or_else(|| self.error("only number skeletons are supported"))?;
            for token in skeleton.split_whitespace() {
                if token == "group-off" {
                    style.no_grouping = true;
                } else if let Some(digits) = token.strip_prefix('.') {
                    if !digits.chars().all(|c| c == '0') {
                        return Err(self.error("unsupported fraction skeleton"));
                    }
                    style.fraction_digits = Some(digits.len() as u8);
                } else {
                    return Err(self.error("unsupported number skeleton"));
                }
            }
        }
        Ok(Part::Number(name, style))
    }

    fn cases(&mut self, in_plural: bool) -> Result<Vec<(String, Vec<Part>)>, ParseError> {
        let mut cases = Vec::new();
        loop {
            self.skip_ws();
            if matches!(self.peek(), Some('}') | None) {
                break;
            }
            let key = self.word().to_string();
            if key.is_empty() {
                return Err(self.error("expected a case key"));
            }
            self.skip_ws();
            if self.bump() != Some('{') {
                return Err(self.error("expected `{` after a case key"));
            }
            let message = self.message(in_plural)?;
            if self.bump() != Some('}') {
                return Err(self.error("unclosed case"));
            }
            cases.push((key, message));
        }
        if !cases.iter().any(|(key, _)| key == "other") {
            return Err(self.error("missing the `other` case"));
        }
        Ok(cases)
    }

    fn plural(&mut self, name: String, ordinal: bool) -> Result<Part, ParseError> {
        self.skip_ws();
        if self.bump() != Some(',') {
            return Err(self.error("expected `,` before plural cases"));
        }
        self.skip_ws();
        let mut offset = 0.0;
        if self.src[self.pos..].starts_with("offset:") {
            self.pos += "offset:".len();
            self.skip_ws();
            offset = self
                .word()
                .parse()
                .map_err(|_| self.error("bad plural offset"))?;
        }
        let cases = self
            .cases(true)?
            .into_iter()
            .map(|(key, message)| {
                let selector = match key.strip_prefix('=') {
                    Some(exact) => exact
                        .parse()
                        .map(Selector::Exact)
                        .map_err(|_| self.error("bad exact plural case")),
                    None => Ok(Selector::Keyword(key)),
                }?;
                Ok((selector, message))
            })
            .collect::<Result<_, ParseError>>()?;
        Ok(Part::Plural {
            name,
            offset,
            ordinal,
            cases,
        })
    }

    fn select(&mut self, name: String) -> Result<Part, ParseError> {
        self.skip_ws();
        if self.bump() != Some(',') {
            return Err(self.error("expected `,` before select cases"));
        }
        Ok(Part::Select {
            name,
            cases: self.cases(false)?,
        })
    }
}

/// A parsed message.
#[derive(Clone, Debug, PartialEq)]
pub struct Message(Vec<Part>);

impl Message {
    /// Parses ICU MessageFormat.
    pub fn parse(source: &str) -> Result<Self, ParseError> {
        let mut parser = Parser {
            src: source,
            pos: 0,
        };
        let parts = parser.message(false)?;
        if parser.pos != source.len() {
            return Err(parser.error("unmatched `}`"));
        }
        Ok(Self(parts))
    }

    /// Formats with `args` in a locale. A missing argument formats as empty.
    pub fn format(&self, args: &[(&str, Arg<'_>)], locale: &dyn LocaleFormat) -> String {
        let mut out = String::new();
        write_parts(&self.0, args, locale, None, &mut out);
        out
    }
}

fn lookup<'v>(args: &'v [(&str, Arg<'_>)], name: &str) -> Option<&'v Arg<'v>> {
    args.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, value)| value)
}

fn number_of(arg: Option<&Arg<'_>>) -> f64 {
    match arg {
        Some(Arg::Num(n)) => *n,
        Some(Arg::Str(s)) => s.trim().parse().unwrap_or(f64::NAN),
        None => f64::NAN,
    }
}

fn write_parts(
    parts: &[Part],
    args: &[(&str, Arg<'_>)],
    locale: &dyn LocaleFormat,
    pound: Option<f64>,
    out: &mut String,
) {
    for part in parts {
        match part {
            Part::Text(text) => out.push_str(text),
            Part::Arg(name) => match lookup(args, name) {
                Some(Arg::Str(s)) => out.push_str(s),
                Some(Arg::Num(n)) => {
                    let _ = write!(out, "{n}");
                }
                None => {}
            },
            Part::Number(name, style) => {
                let value = number_of(lookup(args, name));
                out.push_str(&locale.number(value, *style));
            }
            Part::Pound => {
                if let Some(value) = pound {
                    out.push_str(&locale.number(value, NumberStyle::default()));
                }
            }
            Part::Plural {
                name,
                offset,
                ordinal,
                cases,
            } => {
                let value = number_of(lookup(args, name));
                let exact = cases
                    .iter()
                    .find(|(selector, _)| matches!(selector, Selector::Exact(n) if *n == value));
                let chosen = exact.or_else(|| {
                    let keyword = locale.plural(value - offset, *ordinal).keyword();
                    cases.iter().find(
                        |(selector, _)| matches!(selector, Selector::Keyword(k) if k == keyword),
                    )
                });
                let chosen = chosen.or_else(|| {
                    cases.iter().find(
                        |(selector, _)| matches!(selector, Selector::Keyword(k) if k == "other"),
                    )
                });
                if let Some((_, message)) = chosen {
                    write_parts(message, args, locale, Some(value - offset), out);
                }
            }
            Part::Select { name, cases } => {
                let value = match lookup(args, name) {
                    Some(Arg::Str(s)) => s.to_string(),
                    Some(Arg::Num(n)) => n.to_string(),
                    None => String::new(),
                };
                let chosen = cases
                    .iter()
                    .find(|(key, _)| *key == value)
                    .or_else(|| cases.iter().find(|(key, _)| key == "other"));
                if let Some((_, message)) = chosen {
                    write_parts(message, args, locale, pound, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// English-like rules without ICU: `one` for exactly 1.
    struct Plain;

    impl LocaleFormat for Plain {
        fn number(&self, value: f64, style: NumberStyle) -> String {
            match style.fraction_digits {
                Some(digits) => format!("{value:.*}", digits as usize),
                None => format!("{value}"),
            }
        }
        fn plural(&self, value: f64, ordinal: bool) -> PluralCategory {
            match (ordinal, value) {
                (false, 1.0) => PluralCategory::One,
                (true, 2.0) => PluralCategory::Two,
                _ => PluralCategory::Other,
            }
        }
    }

    fn fmt(source: &str, args: &[(&str, Arg<'_>)]) -> String {
        Message::parse(source).unwrap().format(args, &Plain)
    }

    #[test]
    fn plain_arguments_and_text() {
        assert_eq!(
            fmt("Hello, {name}!", &[("name", "Ada".into())]),
            "Hello, Ada!"
        );
        assert_eq!(fmt("{count} chars", &[("count", 3.into())]), "3 chars");
        assert_eq!(fmt("No args", &[]), "No args");
    }

    #[test]
    fn plurals_with_exact_cases_and_pound() {
        let source = "{n, plural, =0 {none} one {# page} other {# pages}}";
        assert_eq!(fmt(source, &[("n", 0.into())]), "none");
        assert_eq!(fmt(source, &[("n", 1.into())]), "1 page");
        assert_eq!(fmt(source, &[("n", 5.into())]), "5 pages");
    }

    #[test]
    fn plural_offset_applies_to_category_and_pound_but_not_exact() {
        let source =
            "{n, plural, offset:1 =1 {just you} one {you and # other} other {you and # others}}";
        assert_eq!(fmt(source, &[("n", 1.into())]), "just you");
        assert_eq!(fmt(source, &[("n", 2.into())]), "you and 1 other");
        assert_eq!(fmt(source, &[("n", 4.into())]), "you and 3 others");
    }

    #[test]
    fn nested_plural_from_the_catalog() {
        // @astryx.chat.pastedText.label
        let source = "{lines, plural, =1 {{chars, plural, other {{chars} chars}}} other {{lines} lines, {chars, plural, other {{chars} chars}}}}";
        let args = |lines: i32| [("lines", Arg::from(lines)), ("chars", Arg::from(12))];
        assert_eq!(fmt(source, &args(1)), "12 chars");
        assert_eq!(fmt(source, &args(3)), "3 lines, 12 chars");
    }

    #[test]
    fn select_falls_back_to_other() {
        let source =
            "This {item} is already {boundary, select, first {first} last {last} other {}}.";
        assert_eq!(
            fmt(
                source,
                &[("item", "row".into()), ("boundary", "last".into())]
            ),
            "This row is already last."
        );
        assert_eq!(
            fmt(
                source,
                &[("item", "row".into()), ("boundary", "middle".into())]
            ),
            "This row is already ."
        );
    }

    #[test]
    fn selectordinal_uses_ordinal_rules() {
        let source = "{n, selectordinal, two {#nd} other {#th}}";
        assert_eq!(fmt(source, &[("n", 2.into())]), "2nd");
        assert_eq!(fmt(source, &[("n", 4.into())]), "4th");
    }

    #[test]
    fn number_skeletons() {
        assert_eq!(
            Message::parse("{size, number, :: .0 group-off} KB")
                .unwrap()
                .0[0],
            Part::Number(
                "size".into(),
                NumberStyle {
                    no_grouping: true,
                    fraction_digits: Some(1)
                }
            )
        );
        assert_eq!(
            fmt(
                "{size, number, :: .0 group-off} KB",
                &[("size", 1.25.into())]
            ),
            "1.2 KB"
        );
    }

    #[test]
    fn apostrophes_quote_syntax() {
        assert_eq!(fmt("It''s {x}", &[("x", "ok".into())]), "It's ok");
        assert_eq!(
            fmt("Use '{braces}' literally", &[]),
            "Use {braces} literally"
        );
        assert_eq!(fmt("don't", &[]), "don't");
        assert_eq!(
            fmt("{n, plural, other {'#' is # }}", &[("n", 2.into())]),
            "# is 2 "
        );
    }

    #[test]
    fn malformed_messages_are_errors() {
        assert!(Message::parse("{unclosed").is_err());
        assert!(Message::parse("stray }").is_err());
        assert!(Message::parse("{n, plural, one {x}}").is_err());
        assert!(Message::parse("{n, date}").is_err());
    }
}
