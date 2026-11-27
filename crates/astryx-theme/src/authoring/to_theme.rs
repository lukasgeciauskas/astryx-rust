//! A defined theme as this crate's typed [`crate::Theme`]: its tokens per mode,
//! component rules and surfaces read the way the theme export reads upstream's
//! (`tools/export/export-themes.mjs`), with the values `crate::spec` would
//! read from the JSON the export writes.
//!
//! The shipped themes are generated from that JSON; [`to_theme`] goes straight
//! from a [`DefinedTheme`] to the same values, with no JSON in between:
//!
//! - Tokens: [`resolve_theme_tokens`] in each mode, each `--*` token's value
//!   with its `var()`s substituted and its `light-dark()`s split for the mode,
//!   typed by its name ([`token_kind`]). Line heights go after sizes, so a
//!   length line height divides by its style's size. A value that does not
//!   convert is left out, and a token the surface does not have keeps core's
//!   default.
//! - Components: one rule per style key (`variant:blue+size:sm` is the
//!   conditions `variant = blue` and `size = sm`, a bare `selected` is
//!   `selected = true`, `base` has none), and one per nested selector block,
//!   written before the rule it sits in. `:hover`, `:active` and `:focus-visible`
//!   are states and a table row's `:nth-child(even …)` or `(odd …)` its
//!   `parity`; a rule under any other selector has no GPUI form and is left out.
//!   Each property is typed by [`property_kind`] in both modes, and one either
//!   mode cannot convert, or one GPUI has no style for, is left out.
//! - `onDark` and `onLight`: the surface's `color-scheme` and its tokens, read
//!   against the theme's tokens in the surface's mode.
//!
//! Declarations and surface tokens are in key order, as the export's sorted
//! JSON lists them; rules are in the theme's order. Icons, fonts, width points
//! and adaptations are [`Theme::default`]'s.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;

use super::css_values::{
    EdgeLengths, Typed, TypedContext, ValueKind, property_kind, substitute_vars, to_points,
    token_kind, typed, value_for_mode,
};
use super::define::{CssMap, DefinedTheme};
use super::js::js_trim;
use super::merge_components::{StyleMap, StyleValue};
use super::on_media::ResolvedOnMedia;
use super::theme_tokens::resolve_theme_tokens;
use super::token_values::ResolvedThemeMode;
use crate::Theme;
use crate::types::{
    Appearance, ComponentRule, Declaration, RuleState, StyleProp, SurfaceOverride, TokenValue,
};

/// The modes, light then dark, as the export lists them.
const MODES: [ResolvedThemeMode; 2] = [ResolvedThemeMode::Light, ResolvedThemeMode::Dark];

/// JavaScript's default sort order of two keys: by UTF-16 code unit, as the
/// export's `stableStringify` sorts them.
fn js_key_order(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Every `--*` entry of `overrides` typed in `mode`, `var()`s resolved against
/// `scope` (a name listed twice takes its last value). The export's
/// `convertTokens`: line heights after the other tokens, each typed with the
/// length of its style's `-size` token, and a value that does not convert left
/// out.
fn convert_tokens<K: AsRef<str>, V: AsRef<str>>(
    overrides: &CssMap,
    scope: &[(K, V)],
    mode: ResolvedThemeMode,
) -> Vec<(String, Typed)> {
    let mut entries: Vec<(&str, &str)> = overrides
        .iter()
        .filter(|(token, _)| token.starts_with("--"))
        .map(|(token, value)| (token, value.as_str()))
        .collect();
    // A length line height divides by its style's size, so sizes go first.
    entries.sort_by_key(|(token, _)| token.ends_with("-leading"));
    let mut out = Vec::new();
    // The export reads `out[size].length`: the points of a token typed as a
    // length.
    let mut lengths: HashMap<String, f64> = HashMap::new();
    for (token, value) in entries {
        let size_token = token
            .strip_suffix("-leading")
            .map_or_else(|| token.to_string(), |stem| format!("{stem}-size"));
        let font_size = lengths.get(&size_token).copied();
        let kind = token_kind(token);
        let converted = substitute_vars(value, scope)
            .and_then(|value| value_for_mode(&value, mode))
            .and_then(|resolved| {
                let mut notes = Vec::new();
                let context = TypedContext {
                    font_size,
                    property: None,
                };
                let value = typed(kind, &resolved, &mut notes, &context)?;
                if kind == ValueKind::Length {
                    lengths.insert(
                        token.to_string(),
                        to_points(js_trim(&resolved), &mut notes, false)?,
                    );
                }
                Ok(value)
            });
        if let Ok(value) = converted {
            out.push((token.to_string(), value));
        }
    }
    out
}

/// The typed token values of a converted list, in key order, without the
/// private `--_*` ones, as `crate::spec` reads a surface's tokens.
fn token_list(converted: Vec<(String, Typed)>) -> Vec<(Cow<'static, str>, TokenValue)> {
    let mut list: Vec<(Cow<'static, str>, TokenValue)> = converted
        .into_iter()
        .filter(|(name, _)| !name.starts_with("--_"))
        .filter_map(|(name, value)| Some((Cow::Owned(name), value.into_token()?)))
        .collect();
    list.sort_by(|(a, _), (b, _)| js_key_order(a, b));
    list
}

/// A surface's overrides (the export's `surface`): its `color-scheme`, `mode`'s
/// name when it sets none, and its tokens typed in `mode` against the theme's
/// resolved tokens with the surface's own on top.
fn surface(spec: &ResolvedOnMedia, scope: &CssMap, mode: ResolvedThemeMode) -> SurfaceOverride {
    let color_scheme = spec.tokens.get("color-scheme").map(String::as_str);
    let mut rest = spec.tokens.clone();
    rest.remove("color-scheme");
    let scope: Vec<(&str, &str)> = scope
        .iter()
        .chain(rest.iter())
        .map(|(name, value)| (name, value.as_str()))
        .collect();
    let is_dark = match color_scheme {
        Some(scheme) => scheme == "dark",
        None => mode == ResolvedThemeMode::Dark,
    };
    SurfaceOverride {
        appearance: if is_dark {
            Appearance::Dark
        } else {
            Appearance::Light
        },
        tokens: Cow::Owned(token_list(convert_tokens(&rest, &scope, mode))),
    }
}

/// A rule as the export collects it, before `crate::spec` reads it.
struct ExportedRule {
    target: String,
    when: Vec<(String, String)>,
    selector: Option<String>,
    /// Each property typed in light and dark, in key order.
    props: Vec<(String, [Typed; 2])>,
}

/// `String(value)` of a style value that is not a block.
fn js_string(value: &StyleValue) -> String {
    match value {
        StyleValue::Css(css) => css.clone(),
        StyleValue::Null => "null".to_string(),
        StyleValue::Unset => "undefined".to_string(),
        StyleValue::Block(_) => "[object Object]".to_string(),
    }
}

/// A style value as JavaScript reads `if (value)`.
fn is_truthy(value: Option<&StyleValue>) -> bool {
    match value {
        Some(StyleValue::Css(css)) => !css.is_empty(),
        Some(StyleValue::Block(_)) => true,
        _ => false,
    }
}

/// The export's `collectRule`: `decls`'s properties typed in both modes, its
/// nested selector blocks as rules of their own (pushed first), then the rule.
fn collect_rule(
    rules: &mut Vec<ExportedRule>,
    target: &str,
    when: &[(String, String)],
    selector: Option<&str>,
    decls: &StyleMap,
    raw: &[CssMap; 2],
) {
    let mut props = Vec::new();
    for (property, value) in decls.iter() {
        if let StyleValue::Block(nested) = value {
            let nested_selector = format!("{}{property}", selector.unwrap_or_default());
            collect_rule(rules, target, when, Some(&nested_selector), nested, raw);
            continue;
        }
        let kind = property_kind(property);
        let font_size_value = decls.get("fontSize").filter(|_| property == "lineHeight");
        let typed_in = |mode: ResolvedThemeMode| {
            let scope = &raw[usize::from(mode == ResolvedThemeMode::Dark)];
            let read = |value: &StyleValue| {
                substitute_vars(&js_string(value), scope.as_slice())
                    .and_then(|value| value_for_mode(&value, mode))
            };
            let font_size = match font_size_value {
                Some(size) if is_truthy(Some(size)) => {
                    let size = read(size)?;
                    typed(
                        ValueKind::Length,
                        &size,
                        &mut Vec::new(),
                        &TypedContext::default(),
                    )?;
                    Some(to_points(js_trim(&size), &mut Vec::new(), false)?)
                }
                _ => None,
            };
            let context = TypedContext {
                font_size,
                property: Some(property),
            };
            typed(kind, &read(value)?, &mut Vec::new(), &context)
        };
        // A property either mode cannot convert is unsupported and left out.
        if let (Ok(light), Ok(dark)) = (typed_in(MODES[0]), typed_in(MODES[1])) {
            props.push((property.to_string(), [light, dark]));
        }
    }
    props.sort_by(|(a, _), (b, _)| js_key_order(a, b));
    rules.push(ExportedRule {
        target: target.to_string(),
        when: when.to_vec(),
        selector: selector.map(str::to_string),
        props,
    });
}

/// The export's `exportComponents`: every style key of every component as a
/// rule, its key read as conditions.
fn export_components(theme: &DefinedTheme, raw: &[CssMap; 2]) -> Vec<ExportedRule> {
    let mut rules = Vec::new();
    for (target, by_key) in theme.components.iter().flat_map(|map| map.iter()) {
        for (key, decls) in by_key.block().into_iter().flat_map(|keys| keys.iter()) {
            let when: Vec<(String, String)> = if key == "base" {
                Vec::new()
            } else {
                key.split('+')
                    .map(|part| match part.split_once(':') {
                        Some((prop, value)) => (prop.to_string(), value.to_string()),
                        None => (part.to_string(), "true".to_string()),
                    })
                    .collect()
            };
            let empty = StyleMap::new();
            let decls = decls.block().unwrap_or(&empty);
            collect_rule(&mut rules, target, &when, None, decls, raw);
        }
    }
    rules
}

/// A typed CSS property as the GPUI style `crate::spec` reads it into, or
/// `None` for a property GPUI has no style for.
fn style_prop(property: &str, value: &Typed) -> Option<StyleProp> {
    Some(match (property, value) {
        ("backgroundColor", Typed::Token(TokenValue::Color(c))) => StyleProp::Background(*c),
        ("color", Typed::Token(TokenValue::Color(c))) => StyleProp::TextColor(*c),
        ("borderColor", Typed::Token(TokenValue::Color(c))) => StyleProp::BorderColor(*c),
        ("borderWidth", Typed::Token(TokenValue::Length(l))) => StyleProp::BorderWidth(*l),
        ("borderRadius", Typed::Token(TokenValue::Length(l))) => StyleProp::Radius(*l),
        ("fontSize", Typed::Token(TokenValue::Length(l))) => StyleProp::FontSize(*l),
        ("height", Typed::Token(TokenValue::Length(l))) => StyleProp::Height(*l),
        ("borderStyle", Typed::Token(TokenValue::Keyword(k))) => StyleProp::BorderStyle(k.clone()),
        ("fontFamily", Typed::Token(TokenValue::Families(f))) => StyleProp::FontFamily(f.clone()),
        ("fontWeight", Typed::Token(TokenValue::Weight(w))) => StyleProp::FontWeight(*w),
        ("lineHeight", Typed::Token(TokenValue::Leading(l))) => StyleProp::LineHeight(*l),
        ("boxShadow", Typed::Token(TokenValue::Shadow(s))) => StyleProp::Shadow(s.clone()),
        ("opacity", Typed::Token(TokenValue::Number(n))) => StyleProp::Opacity(*n),
        ("padding" | "paddingInline" | "paddingBlock", Typed::Edges(edges)) => match edges {
            EdgeLengths::All(all) => StyleProp::Padding(*all),
            EdgeLengths::Inline(start, end) => StyleProp::PaddingInline(*start, *end),
            EdgeLengths::Block(top, bottom) => StyleProp::PaddingBlock(*top, *bottom),
        },
        _ => return None,
    })
}

/// One mode's declarations of a rule, as `crate::spec` reads its `props`: a
/// token re-definition for each `--*` property but the private `--_*` ones, and
/// a style for each property GPUI has one for.
fn declarations(props: &[(String, [Typed; 2])], mode: usize) -> Vec<Declaration> {
    props
        .iter()
        .filter_map(|(property, by_mode)| {
            let value = &by_mode[mode];
            if property.starts_with("--") {
                if property.starts_with("--_") {
                    return None;
                }
                let token = value.clone().into_token()?;
                return Some(Declaration::Token(Cow::Owned(property.clone()), token));
            }
            style_prop(property, value).map(Declaration::Style)
        })
        .collect()
}

/// An exported rule as `crate::spec` reads it: its state or row parity from
/// its selector, or `None` for a selector with no GPUI form.
fn component_rule(rule: ExportedRule) -> Option<ComponentRule> {
    let state = match rule.selector.as_deref() {
        Some(":hover") => Some(RuleState::Hover),
        Some(":active") => Some(RuleState::Active),
        Some(":focus-visible") => Some(RuleState::FocusVisible),
        _ => None,
    };
    let parity = rule.selector.as_deref().and_then(|selector| {
        let selector = selector.trim();
        if selector.starts_with(":nth-child(even") {
            Some("even")
        } else if selector.starts_with(":nth-child(odd") {
            Some("odd")
        } else {
            None
        }
    });
    if rule.selector.is_some() && state.is_none() && parity.is_none() {
        return None;
    }
    let mut when: Vec<(Cow<'static, str>, Cow<'static, str>)> = rule
        .when
        .into_iter()
        .map(|(prop, value)| (Cow::Owned(prop), Cow::Owned(value)))
        .collect();
    if let Some(parity) = parity {
        when.push((Cow::Borrowed("parity"), Cow::Borrowed(parity)));
    }
    Some(ComponentRule {
        target: Cow::Owned(rule.target),
        when: Cow::Owned(when),
        state,
        light: Cow::Owned(declarations(&rule.props, 0)),
        dark: Cow::Owned(declarations(&rule.props, 1)),
    })
}

/// A defined theme as a typed [`Theme`], or core's default theme for none,
/// with the values the theme export and `crate::spec` give it (see the
/// module docs). The name loses an `astryx-` prefix, as the export's file names
/// do.
pub fn to_theme(theme: Option<&DefinedTheme>) -> Theme {
    let raw = MODES.map(|mode| resolve_theme_tokens(theme, mode));
    let mut out = Theme::default();
    for (index, mode) in MODES.into_iter().enumerate() {
        let tokens = match mode {
            ResolvedThemeMode::Light => &mut out.light,
            ResolvedThemeMode::Dark => &mut out.dark,
        };
        for (name, value) in convert_tokens(&raw[index], raw[index].as_slice(), mode) {
            if name.starts_with("--_") {
                continue;
            }
            if let Some(value) = value.into_token() {
                // A token the port does not type yet is skipped, as codegen
                // skips it.
                let _ = tokens.set(&name, value);
            }
        }
    }
    let Some(theme) = theme else {
        return out;
    };
    out.name = Cow::Owned(
        theme
            .name
            .strip_prefix("astryx-")
            .unwrap_or(&theme.name)
            .to_string(),
    );
    out.components = Cow::Owned(
        export_components(theme, &raw)
            .into_iter()
            .filter_map(component_rule)
            .collect(),
    );
    out.on_dark = theme
        .on_dark
        .as_ref()
        .map(|spec| surface(spec, &raw[1], ResolvedThemeMode::Dark));
    out.on_light = theme
        .on_light
        .as_ref()
        .map(|spec| surface(spec, &raw[0], ResolvedThemeMode::Light));
    out
}

impl DefinedTheme {
    /// The theme as this crate's typed [`Theme`]: [`to_theme`].
    pub fn to_theme(&self) -> Theme {
        to_theme(Some(self))
    }
}

#[cfg(test)]
mod tests {
    use super::super::define::{DefineThemeInput, TokenInput, define_theme};
    use super::super::on_media::OnMediaOverrides;
    use super::*;
    use crate::types::{AbsoluteLength, hex};

    fn css(value: &str) -> StyleValue {
        StyleValue::Css(value.to_string())
    }

    fn block<const N: usize>(entries: [(&str, StyleValue); N]) -> StyleValue {
        StyleValue::Block(entries.into())
    }

    fn sample() -> Theme {
        let input = DefineThemeInput {
            tokens: Some(
                [
                    (
                        "--color-accent",
                        TokenInput::Pair("#112233".into(), "#445566".into()),
                    ),
                    ("--text-body-leading", TokenInput::Css("24px".into())),
                    ("--text-body-size", TokenInput::Css("16px".into())),
                    ("--_private", TokenInput::Css("1px".into())),
                ]
                .into(),
            ),
            components: Some(
                [
                    (
                        "card",
                        block([
                            (
                                "base",
                                block([
                                    ("padding", css("4px 8px")),
                                    ("letterSpacing", css("-0.02em")),
                                    ("gap", css("4px")),
                                    (
                                        ":hover",
                                        block([("backgroundColor", css("var(--color-accent)"))]),
                                    ),
                                    (":where(.x *)", block([("color", css("red"))])),
                                ]),
                            ),
                            (
                                "variant:blue+size:sm",
                                block([
                                    ("lineHeight", css("24px")),
                                    ("fontSize", css("16px")),
                                    (
                                        "--color-text-secondary",
                                        css("light-dark(#111111, #eeeeee)"),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    (
                        "row",
                        block([(
                            "selected",
                            block([(
                                ":nth-child(even of :not([x]))",
                                block([("backgroundColor", css("#f0f0f0"))]),
                            )]),
                        )]),
                    ),
                ]
                .into(),
            ),
            on_dark: Some(OnMediaOverrides {
                tokens: Some([("--color-accent", TokenInput::Css("#90CAF9".into()))].into()),
                components: None,
            }),
            ..DefineThemeInput::new("astryx-sample")
        };
        define_theme(&input).unwrap().to_theme()
    }

    fn describe(declarations: &[Declaration]) -> Vec<String> {
        declarations
            .iter()
            .map(|declaration| match declaration {
                Declaration::Token(name, value) => format!("{name}: {value}"),
                Declaration::Style(StyleProp::Background(c)) => format!("background {}", hex(*c)),
                Declaration::Style(StyleProp::FontSize(AbsoluteLength::Pixels(px))) => {
                    format!("font-size {}", f32::from(*px))
                }
                Declaration::Style(StyleProp::LineHeight(l)) => format!("line-height {l}"),
                Declaration::Style(StyleProp::Padding(e)) => format!(
                    "padding {:?} {:?} {:?} {:?}",
                    e.top, e.right, e.bottom, e.left
                ),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn tokens_are_typed_per_mode_with_line_heights_after_sizes() {
        let theme = sample();
        assert_eq!(theme.name, "sample");
        assert_eq!(
            theme
                .light
                .get("--color-accent")
                .map(|v| v.to_string())
                .as_deref(),
            Some("#112233")
        );
        assert_eq!(
            theme
                .dark
                .get("--color-accent")
                .map(|v| v.to_string())
                .as_deref(),
            Some("#445566")
        );
        // 24px over the 16px body size.
        assert_eq!(
            theme
                .light
                .get("--text-body-leading")
                .map(|v| v.to_string())
                .as_deref(),
            Some("1.5")
        );
        // A token the theme leaves out keeps core's default.
        let core = Theme::default();
        assert_eq!(
            theme.light.get("--spacing-4").map(|v| v.to_string()),
            core.light.get("--spacing-4").map(|v| v.to_string())
        );
    }

    /// A rule as its target, conditions, state and declarations in each mode.
    type Described = (
        String,
        Vec<String>,
        Option<RuleState>,
        Vec<String>,
        Vec<String>,
    );

    #[test]
    fn components_become_rules_nested_selectors_first() {
        let theme = sample();
        let rules: Vec<Described> = theme
            .components
            .iter()
            .map(|rule| {
                (
                    rule.target.to_string(),
                    rule.when.iter().map(|(p, v)| format!("{p}={v}")).collect(),
                    rule.state,
                    describe(&rule.light),
                    describe(&rule.dark),
                )
            })
            .collect();
        let padding = "padding 4px 8px 4px 8px".to_string();
        assert_eq!(
            rules,
            vec![
                (
                    "card".to_string(),
                    vec![],
                    Some(RuleState::Hover),
                    vec!["background #112233".to_string()],
                    vec!["background #445566".to_string()],
                ),
                // The :where() block has no GPUI form; letterSpacing (a
                // font-relative length) and gap (no GPUI style) are left out.
                (
                    "card".to_string(),
                    vec![],
                    None,
                    vec![padding.clone()],
                    vec![padding]
                ),
                (
                    "card".to_string(),
                    vec!["variant=blue".to_string(), "size=sm".to_string()],
                    None,
                    vec![
                        "--color-text-secondary: #111111".to_string(),
                        "font-size 16".to_string(),
                        "line-height 1.5".to_string(),
                    ],
                    vec![
                        "--color-text-secondary: #EEEEEE".to_string(),
                        "font-size 16".to_string(),
                        "line-height 1.5".to_string(),
                    ],
                ),
                (
                    "row".to_string(),
                    vec!["selected=true".to_string(), "parity=even".to_string()],
                    None,
                    vec!["background #F0F0F0".to_string()],
                    vec!["background #F0F0F0".to_string()],
                ),
                (
                    "row".to_string(),
                    vec!["selected=true".to_string()],
                    None,
                    vec![],
                    vec![],
                ),
            ]
        );
    }

    #[test]
    fn surfaces_carry_their_scheme_and_tokens_in_key_order() {
        let theme = sample();
        let on_dark = theme.on_dark.unwrap();
        assert_eq!(on_dark.appearance, Appearance::Dark);
        let tokens: Vec<String> = on_dark
            .tokens
            .iter()
            .map(|(name, value)| format!("{name}: {value}"))
            .collect();
        // --color-on-dark is #FFFFFF in core's dark mode.
        assert_eq!(
            tokens,
            [
                "--color-accent: #90CAF9",
                "--color-icon-primary: #FFFFFF",
                "--color-text-primary: #FFFFFF",
            ]
        );
        assert_eq!(theme.on_light.unwrap().appearance, Appearance::Light);
    }

    #[test]
    fn no_theme_is_cores_default() {
        let theme = to_theme(None);
        assert_eq!(theme.name, "default");
        assert!(theme.components.is_empty());
        assert!(theme.on_dark.is_none() && theme.on_light.is_none());
        let core = Theme::default();
        for name in crate::Tokens::NAMES {
            assert_eq!(
                theme.light.get(name).map(|v| v.to_string()),
                core.light.get(name).map(|v| v.to_string()),
                "{name}"
            );
        }
    }

    #[test]
    fn keys_sort_by_utf16_code_unit() {
        // JavaScript's default sort, which the export's JSON keys follow:
        // U+FF5E sorts before U+1F600, whose first code unit is a surrogate.
        assert_eq!(js_key_order("\u{ff5e}", "\u{1f600}"), Ordering::Greater);
        assert_eq!(js_key_order("--a", "--b"), Ordering::Less);
        assert_eq!(js_key_order("Z", "a"), Ordering::Less);
    }
}
