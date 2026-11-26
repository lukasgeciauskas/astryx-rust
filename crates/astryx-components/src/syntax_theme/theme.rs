//! SyntaxTheme and `useSyntaxTheme`, upstream `theme/syntax/SyntaxTheme.tsx`: syntax colors for
//! the code in a subtree. Without one, code takes the theme's own syntax tokens.

use std::rc::Rc;

use astryx_primitives::{provide, use_context};
use astryx_syntax::{SyntaxColor, SyntaxThemeDefinition, SyntaxToken};
use astryx_theme::{Appearance, current};
use gpui::{
    AnyElement, App, HighlightStyle, Hsla, IntoElement, Rgba, SharedString, StyledText, Window,
};

/// The syntax colors code draws with, resolved for the current mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyntaxColors {
    pub keyword: Hsla,
    pub string: Hsla,
    pub comment: Hsla,
    pub number: Hsla,
    pub function: Hsla,
    pub r#type: Hsla,
    pub variable: Hsla,
    pub operator: Hsla,
    pub constant: Hsla,
    pub tag: Hsla,
    pub attribute: Hsla,
    pub property: Hsla,
    pub punctuation: Hsla,
    pub background: Hsla,
    pub added: Hsla,
    pub removed: Hsla,
    pub diff_header: Hsla,
    pub added_marker: Hsla,
    pub removed_marker: Hsla,
}

impl SyntaxColors {
    /// Upstream `token(name)`: the color for a token type (`keyword`, `diff-header`, ...).
    pub fn token(&self, name: &str) -> Option<Hsla> {
        Some(match name {
            "keyword" => self.keyword,
            "string" => self.string,
            "comment" => self.comment,
            "number" => self.number,
            "function" => self.function,
            "type" => self.r#type,
            "variable" => self.variable,
            "operator" => self.operator,
            "constant" => self.constant,
            "tag" => self.tag,
            "attribute" => self.attribute,
            "property" => self.property,
            "punctuation" => self.punctuation,
            "background" => self.background,
            "added" => self.added,
            "removed" => self.removed,
            "diff-header" => self.diff_header,
            "added-marker" => self.added_marker,
            "removed-marker" => self.removed_marker,
            _ => return None,
        })
    }
}

/// The SyntaxTheme in scope.
#[derive(Clone)]
struct ActiveSyntaxTheme(Rc<SyntaxThemeDefinition>);

/// A CSS color from a syntax theme: hex, `rgb()`, or a `light-dark(a, b)` string.
fn parse(value: &str, dark: bool) -> Option<Hsla> {
    let value = value.trim();
    if let Some(inner) = value
        .strip_prefix("light-dark(")
        .and_then(|v| v.strip_suffix(')'))
        && let Some((light, d)) = inner.split_once(',')
    {
        return parse(if dark { d } else { light }, dark);
    }
    if value == "transparent" {
        return Some(gpui::transparent_black());
    }
    Rgba::try_from(value).ok().map(Hsla::from)
}

/// The syntax colors in scope: a SyntaxTheme's, else the theme's syntax tokens.
pub fn syntax_colors(window: &Window, cx: &App) -> SyntaxColors {
    let theme = current(window, cx);
    let t = theme.tokens();
    let c = &t.color;
    let base = SyntaxColors {
        keyword: c.syntax_keyword,
        string: c.syntax_string,
        comment: c.syntax_comment,
        number: c.syntax_number,
        function: c.syntax_function,
        r#type: c.syntax_type,
        variable: c.syntax_variable,
        operator: c.syntax_operator,
        constant: c.syntax_constant,
        tag: c.syntax_tag,
        attribute: c.syntax_attribute,
        property: c.syntax_property,
        punctuation: c.syntax_punctuation,
        background: c.syntax_background,
        added: c.syntax_added,
        removed: c.syntax_removed,
        diff_header: c.syntax_diff_header,
        added_marker: c.syntax_added_marker,
        removed_marker: c.syntax_removed_marker,
    };
    let Some(active) = use_context::<ActiveSyntaxTheme>(cx) else {
        return base;
    };
    let dark = theme.appearance == Appearance::Dark;
    let tokens = &active.0.tokens;
    let get =
        |color: &SyntaxColor, fallback: Hsla| parse(color.resolve(dark), dark).unwrap_or(fallback);
    let punctuation = get(&tokens.punctuation, base.punctuation);
    // Upstream's defaults for unset diff slots: markers follow punctuation, rows tint the
    // muted success and error colors at 25%, and the header is clear.
    let tint = |muted: Hsla| Hsla {
        a: muted.a * 0.25,
        ..muted
    };
    let opt = |color: &Option<SyntaxColor>, fallback: Hsla| {
        color
            .as_ref()
            .and_then(|c| parse(c.resolve(dark), dark))
            .unwrap_or(fallback)
    };
    SyntaxColors {
        keyword: get(&tokens.keyword, base.keyword),
        string: get(&tokens.string, base.string),
        comment: get(&tokens.comment, base.comment),
        number: get(&tokens.number, base.number),
        function: get(&tokens.function, base.function),
        r#type: get(&tokens.r#type, base.r#type),
        variable: get(&tokens.variable, base.variable),
        operator: get(&tokens.operator, base.operator),
        constant: get(&tokens.constant, base.constant),
        tag: get(&tokens.tag, base.tag),
        attribute: get(&tokens.attribute, base.attribute),
        property: get(&tokens.property, base.property),
        punctuation,
        background: get(&tokens.background, base.background),
        added: opt(&tokens.added, tint(c.success_muted)),
        removed: opt(&tokens.removed, tint(c.error_muted)),
        diff_header: opt(&tokens.diff_header, gpui::transparent_black()),
        added_marker: opt(&tokens.added_marker, punctuation),
        removed_marker: opt(&tokens.removed_marker, punctuation),
    }
}

/// Upstream `useSyntaxTheme`'s result.
#[derive(Clone, Debug)]
pub struct UseSyntaxTheme {
    /// The nearest SyntaxTheme's name.
    pub name: SharedString,
    /// The mode the colors are resolved for.
    pub dark: bool,
    /// Every token, resolved.
    pub tokens: SyntaxColors,
}

/// Upstream `useSyntaxTheme`: the nearest SyntaxTheme's colors for the current mode, or
/// None outside one (a drawing can skip itself).
pub fn use_syntax_theme(window: &Window, cx: &App) -> Option<UseSyntaxTheme> {
    let active = use_context::<ActiveSyntaxTheme>(cx)?;
    let dark = current(window, cx).appearance == Appearance::Dark;
    Some(UseSyntaxTheme {
        name: active.0.name.to_string().into(),
        dark,
        tokens: syntax_colors(window, cx),
    })
}

/// Upstream `SyntaxTheme`: code inside draws with `theme`'s colors.
pub fn syntax_theme(theme: SyntaxThemeDefinition, child: impl IntoElement) -> AnyElement {
    provide(ActiveSyntaxTheme(Rc::new(theme)), child).into_any_element()
}

/// One line of code with its tokens painted (the span rendering CodeBlock and Markdown's
/// code share). Offsets are byte offsets into `line`; a token running past its end stops
/// there.
pub(crate) fn highlighted_line(
    line: impl Into<SharedString>,
    tokens: &[SyntaxToken],
    colors: &SyntaxColors,
) -> StyledText {
    let line: SharedString = line.into();
    let len = line.len();
    let highlights = tokens
        .iter()
        .filter_map(|t| {
            let start = t.start.min(len);
            let end = t.end.min(len);
            let color = colors.token(&t.ty)?;
            (start < end && line.is_char_boundary(start) && line.is_char_boundary(end)).then(|| {
                (
                    start..end,
                    HighlightStyle {
                        color: Some(color),
                        ..Default::default()
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    StyledText::new(line).with_highlights(highlights)
}
