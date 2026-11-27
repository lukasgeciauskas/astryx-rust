//! Syntax theme definitions, upstream `theme/syntax/defineSyntaxTheme.ts`.

use std::borrow::Cow;

/// Upstream `SyntaxTokenValue`: one color, or a `[light, dark]` pair (CSS `light-dark()`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyntaxColor {
    Single(Cow<'static, str>),
    LightDark(Cow<'static, str>, Cow<'static, str>),
}

impl SyntaxColor {
    /// The color for an appearance.
    pub fn resolve(&self, dark: bool) -> &str {
        match self {
            SyntaxColor::Single(c) => c,
            SyntaxColor::LightDark(light, d) => {
                if dark {
                    d
                } else {
                    light
                }
            }
        }
    }
}

impl From<&'static str> for SyntaxColor {
    fn from(color: &'static str) -> Self {
        SyntaxColor::Single(Cow::Borrowed(color))
    }
}

/// Upstream `SyntaxThemeTokenInput`: the 14 slots, and the five diff row slots, which a
/// theme may leave to their defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxThemeTokens {
    pub keyword: SyntaxColor,
    pub string: SyntaxColor,
    pub comment: SyntaxColor,
    pub number: SyntaxColor,
    pub function: SyntaxColor,
    pub r#type: SyntaxColor,
    pub variable: SyntaxColor,
    pub operator: SyntaxColor,
    pub constant: SyntaxColor,
    pub tag: SyntaxColor,
    pub attribute: SyntaxColor,
    pub property: SyntaxColor,
    pub punctuation: SyntaxColor,
    pub background: SyntaxColor,
    pub added: Option<SyntaxColor>,
    pub removed: Option<SyntaxColor>,
    pub diff_header: Option<SyntaxColor>,
    pub added_marker: Option<SyntaxColor>,
    pub removed_marker: Option<SyntaxColor>,
}

/// Upstream `SyntaxThemeDefinition`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxThemeDefinition {
    pub name: Cow<'static, str>,
    pub tokens: SyntaxThemeTokens,
}

/// Upstream `defineSyntaxTheme`.
pub fn define_syntax_theme(
    name: impl Into<Cow<'static, str>>,
    tokens: SyntaxThemeTokens,
) -> SyntaxThemeDefinition {
    SyntaxThemeDefinition {
        name: name.into(),
        tokens,
    }
}
