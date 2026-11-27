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
//! - Adaptations: one per resolved rule ([`DefinedTheme::adaptation_rules`]),
//!   in order: its pointer, its width edges at the theme's width points (at
//!   least `from`, below `below`), and the tokens it writes typed in each mode,
//!   `var()`s read against the theme's tokens with the rule's own on top. The
//!   typed [`Adaptation`] holds no contrast or reduced-motion condition, so a
//!   rule with one is left out rather than applied everywhere.
//! - Width points: the theme's effective ones
//!   ([`DefinedTheme::adaptations`]), the defaults for none.
//!
//! Declarations and surface and adaptation tokens are in key order, as the
//! export's sorted JSON lists them; rules are in the theme's order. Fonts are
//! [`Theme::default`]'s. Icons go where the Icon component looks them up: each
//! entry's artwork under its name, and a by-variant entry's under `name:variant`
//! for each variant it can receive. The motion tempo the theme writes to
//! `--_astryx-motion-tempo` goes first among the adaptations, as one that always
//! applies, since [`Theme`] has no field for it.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;

use super::adaptations::{ResolvedThemeAdaptationRule, WidthBreakpoints};
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
    Adaptation, Appearance, ComponentRule, Declaration, Pointer, RuleState, StyleProp,
    SurfaceOverride, TokenValue,
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

/// A resolved rule as the typed [`Adaptation`] the export's JSON reads into
/// (the export writes `when`, `query` and the rule's tokens converted in each
/// mode against `{...raw[mode], ...rule.tokens}`), or `None` for a rule with a
/// contrast or reduced-motion condition, which an [`Adaptation`] cannot hold.
/// A width edge is the theme's point of that name, `points`.
fn adaptation(
    rule: &ResolvedThemeAdaptationRule,
    raw: &[CssMap; 2],
    points: Option<&WidthBreakpoints>,
) -> Option<Adaptation> {
    let when = &rule.when;
    if when.contrast.is_some() || when.motion.is_some() {
        return None;
    }
    let width = |name: Option<&String>| name.and_then(|name| points?.get(name)).map(|px| px as f32);
    let range = when.width.as_ref();
    let tokens = |index: usize, mode: ResolvedThemeMode| {
        let scope: Vec<(&str, &str)> = raw[index]
            .iter()
            .chain(rule.tokens.iter())
            .map(|(name, value)| (name, value.as_str()))
            .collect();
        let converted = convert_tokens(&rule.tokens, &scope, mode);
        // A rule's tempo stays: `ResolvedTheme` reads it where the rule holds.
        let tempo = converted.iter().find_map(|(name, value)| {
            (name == super::theme_motion::MOTION_TEMPO_PROPERTY)
                .then(|| match value {
                    Typed::Token(TokenValue::Number(n)) => Some(*n),
                    Typed::Token(TokenValue::Keyword(k)) => k.0.parse().ok(),
                    _ => None,
                })
                .flatten()
        });
        let mut list = token_list(converted);
        if let Some(tempo) = tempo {
            list.push((
                Cow::Borrowed(super::theme_motion::MOTION_TEMPO_PROPERTY),
                TokenValue::Number(tempo),
            ));
        }
        Cow::Owned(list)
    };
    Some(Adaptation {
        pointer: match when.pointer.as_deref() {
            Some("coarse") => Some(Pointer::Coarse),
            Some("fine") => Some(Pointer::Fine),
            _ => None,
        },
        width_below: width(range.and_then(|range| range.below.as_ref())),
        width_at_least: width(range.and_then(|range| range.from.as_ref())),
        light: tokens(0, ResolvedThemeMode::Light),
        dark: tokens(1, ResolvedThemeMode::Dark),
    })
}

/// A theme's width points as the typed theme holds them.
fn width_breakpoints(points: &WidthBreakpoints) -> crate::types::WidthBreakpoints {
    crate::types::WidthBreakpoints {
        sm: points.sm as f32,
        md: points.md as f32,
        lg: points.lg as f32,
        xl: points.xl as f32,
        xl2: points.xl2 as f32,
    }
}

/// The theme's icons as the Icon component looks them up (`name`, then
/// `name:variant` for a requested variant): each entry's artwork under its name, and
/// a by-variant entry's under `name:variant` for each variant it can receive, one
/// the theme lists and, for an inherited entry, the theme that defined it lists too
/// (upstream `selectIconEntry` and `readIconEntry`). A fallback entry writes
/// nothing, so the built-in icon draws.
fn theme_icons(theme: &DefinedTheme) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
    use super::define::ThemeIconEntry;
    let active = theme.icon_variants.as_ref();
    let mut out = Vec::new();
    for (name, entry) in theme.icons.iter().flat_map(|icons| icons.iter()) {
        match entry {
            ThemeIconEntry::Fallback => {}
            ThemeIconEntry::Icon(path) => {
                out.push((Cow::Owned(name.to_string()), Cow::Owned(path.clone())));
            }
            ThemeIconEntry::Variants { icon, variants } => {
                out.push((Cow::Owned(name.to_string()), Cow::Owned(icon.clone())));
                let defining = match theme
                    .icon_entry_variants
                    .as_ref()
                    .and_then(|record| record.get(name))
                {
                    Some(list) => list.as_ref(),
                    None => active,
                };
                let lists = [active, defining];
                for (variant, path) in variants.iter() {
                    let receivable = lists
                        .iter()
                        .all(|list| list.is_some_and(|list| list.iter().any(|v| v == variant)));
                    if receivable {
                        out.push((
                            Cow::Owned(format!("{name}:{variant}")),
                            Cow::Owned(path.clone()),
                        ));
                    }
                }
            }
        }
    }
    out
}

impl DefinedTheme {
    /// The tempo the theme's motion plays at: what it writes to
    /// `--_astryx-motion-tempo` (upstream `themeTempo` reads it; a duration scale alone
    /// writes 1), else 1, and whether it sets `tempo` itself
    /// (`ResolvedMotionKnobs::tempo_is_set` of its effective motion).
    pub fn motion_tempo(&self) -> crate::types::MotionTempo {
        use super::theme_motion::{MOTION_TEMPO_PROPERTY, resolve_motion_knobs};
        let knobs = resolve_motion_knobs(self.axes.as_ref().and_then(|axes| axes.motion.as_ref()));
        crate::types::MotionTempo {
            tempo: self.tokens.get(MOTION_TEMPO_PROPERTY).map_or(1.0, |text| {
                rustryx_primitives::motion::theme_tempo::theme_tempo_from_text(text)
            }),
            is_set: knobs.tempo_is_set,
        }
    }
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
    let points = theme
        .adaptations
        .as_ref()
        .map(|adaptations| &adaptations.width_breakpoints);
    out.adaptations = Cow::Owned(
        theme
            .adaptation_rules
            .iter()
            .flatten()
            .filter_map(|rule| adaptation(rule, &raw, points))
            .collect(),
    );
    if let Some(points) = points {
        out.width_breakpoints = width_breakpoints(points);
    }
    out.icons = Cow::Owned(theme_icons(theme));
    out.motion_tempo = theme.motion_tempo().tempo;
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
    fn adaptations_carry_their_pointer_width_edges_and_tokens_typed_per_mode() {
        use super::super::adaptations::{
            ThemeAdaptationCondition, ThemeAdaptationRule, ThemeAdaptationValue,
            ThemeAdaptationWidthCondition, ThemeAdaptations,
        };
        let condition = |pointer: Option<&str>, from: Option<&str>, below: Option<&str>| {
            ThemeAdaptationCondition {
                pointer: pointer.map(str::to_string),
                width: (from.is_some() || below.is_some()).then(|| ThemeAdaptationWidthCondition {
                    from: from.map(str::to_string),
                    below: below.map(str::to_string),
                }),
                ..ThemeAdaptationCondition::default()
            }
        };
        let writes =
            |when: ThemeAdaptationCondition, tokens: &[(&str, &str)]| ThemeAdaptationRule {
                when,
                value: ThemeAdaptationValue {
                    tokens: Some(
                        tokens
                            .iter()
                            .map(|(name, value)| (*name, TokenInput::Css(value.to_string())))
                            .collect(),
                    ),
                    ..ThemeAdaptationValue::default()
                },
            };
        let input = DefineThemeInput {
            tokens: Some(
                [(
                    "--color-accent",
                    TokenInput::Pair("#112233".into(), "#445566".into()),
                )]
                .into(),
            ),
            adaptations: Some(ThemeAdaptations {
                width_breakpoints: Some([("md", 800.0)].into()),
                rules: Some(vec![
                    // A var() reads the theme's tokens with the rule's own on top.
                    writes(
                        condition(Some("coarse"), None, Some("lg")),
                        &[
                            ("--color-border", "var(--color-accent)"),
                            ("--_private", "1px"),
                        ],
                    ),
                    writes(
                        condition(None, Some("md"), None),
                        &[
                            ("--color-accent", "#abcdef"),
                            ("--color-border", "var(--color-accent)"),
                        ],
                    ),
                    // The typed adaptation has no contrast condition.
                    writes(
                        ThemeAdaptationCondition {
                            contrast: Some("more".into()),
                            ..ThemeAdaptationCondition::default()
                        },
                        &[("--color-border", "#000000")],
                    ),
                ]),
            }),
            ..DefineThemeInput::new("adaptive")
        };
        let theme = define_theme(&input).unwrap().to_theme();
        assert_eq!(theme.width_breakpoints.md, 800.0);
        assert_eq!(theme.width_breakpoints.lg, 1024.0);
        let described: Vec<_> = theme
            .adaptations
            .iter()
            .map(|adaptation| {
                let tokens = |tokens: &[(Cow<'static, str>, TokenValue)]| {
                    tokens
                        .iter()
                        .map(|(name, value)| format!("{name}: {value}"))
                        .collect::<Vec<_>>()
                };
                (
                    adaptation.pointer,
                    adaptation.width_below,
                    adaptation.width_at_least,
                    tokens(&adaptation.light),
                    tokens(&adaptation.dark),
                )
            })
            .collect();
        assert_eq!(
            described,
            [
                (
                    Some(Pointer::Coarse),
                    Some(1024.0),
                    None,
                    vec!["--color-border: #112233".to_string()],
                    vec!["--color-border: #445566".to_string()],
                ),
                (
                    None,
                    None,
                    Some(800.0),
                    vec![
                        "--color-accent: #ABCDEF".to_string(),
                        "--color-border: #ABCDEF".to_string()
                    ],
                    vec![
                        "--color-accent: #ABCDEF".to_string(),
                        "--color-border: #ABCDEF".to_string()
                    ],
                ),
            ]
        );
        // No theme has the default width points and no adaptations.
        let none = to_theme(None);
        assert!(none.adaptations.is_empty());
        assert_eq!(
            none.width_breakpoints,
            crate::types::WidthBreakpoints::DEFAULT
        );
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

#[cfg(test)]
mod icon_and_tempo_tests {
    use std::sync::Arc;

    use super::super::define::{DefineThemeInput, ThemeIconEntry, define_theme};
    use super::super::theme_motion::ThemeMotionConfig;
    use super::*;
    use crate::types::{MotionTempo, Pointer};

    fn define(input: DefineThemeInput) -> Arc<DefinedTheme> {
        Arc::new(define_theme(&input).unwrap())
    }

    fn listed(names: &[&str]) -> Option<Vec<String>> {
        Some(names.iter().map(|name| name.to_string()).collect())
    }

    fn icons_of(theme: &Theme) -> Vec<(&str, &str)> {
        theme
            .icons
            .iter()
            .map(|(name, path)| (name.as_ref(), path.as_ref()))
            .collect()
    }

    fn by_variant(icon: &str, variants: &[(&str, &str)]) -> ThemeIconEntry {
        ThemeIconEntry::Variants {
            icon: icon.to_string(),
            variants: variants
                .iter()
                .map(|(variant, path)| (*variant, path.to_string()))
                .collect(),
        }
    }

    // The icons go to `Theme::icons` as the Icon component reads them: a name's
    // artwork, a fallback entry nothing, and a by-variant entry's variants under
    // `name:variant` only for the variants the theme lists (upstream `readIconEntry`
    // passes no other variant).
    #[test]
    fn writes_the_icons_the_icon_component_looks_up() {
        let theme = define(DefineThemeInput {
            icon_variants: listed(&["outline", "filled"]),
            icons: Some(
                [
                    ("close", ThemeIconEntry::from("icons/lucide/x.svg")),
                    ("menu", ThemeIconEntry::Fallback),
                    (
                        "info",
                        by_variant(
                            "icons/lucide/info.svg",
                            &[
                                ("outline", "icons/lucide/circle-help.svg"),
                                ("bold", "icons/lucide/badge-info.svg"),
                            ],
                        ),
                    ),
                ]
                .into(),
            ),
            ..DefineThemeInput::new("icons")
        });
        let out = theme.to_theme();
        assert_eq!(
            icons_of(&out),
            [
                ("close", "icons/lucide/x.svg"),
                ("info", "icons/lucide/info.svg"),
                ("info:outline", "icons/lucide/circle-help.svg"),
            ]
        );
        assert_eq!(out.icon("close"), Some("icons/lucide/x.svg"));
        assert_eq!(out.icon("menu"), None);
        assert_eq!(out.icon("info:bold"), None);
        // No list, no variant.
        let unlisted = define(DefineThemeInput {
            icons: Some([("info", by_variant("i.svg", &[("outline", "o.svg")]))].into()),
            ..DefineThemeInput::new("unlisted")
        });
        assert_eq!(icons_of(&unlisted.to_theme()), [("info", "i.svg")]);
        assert!(to_theme(None).icons.is_empty());
    }

    // An inherited by-variant entry receives a variant only when both its defining
    // theme's list and the child's list contain it (upstream `selectIconEntry`'s
    // `variantLists`).
    #[test]
    fn an_inherited_entry_receives_only_variants_both_lists_hold() {
        let base = define(DefineThemeInput {
            icon_variants: listed(&["outline", "filled"]),
            icons: Some(
                [(
                    "info",
                    by_variant("i.svg", &[("outline", "o.svg"), ("filled", "f.svg")]),
                )]
                .into(),
            ),
            ..DefineThemeInput::new("base")
        });
        let child = define(DefineThemeInput {
            extends: Some(base.clone()),
            icon_variants: listed(&["outline", "bold"]),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(
            icons_of(&child.to_theme()),
            [("info", "i.svg"), ("info:outline", "o.svg")]
        );
        assert_eq!(
            icons_of(&base.to_theme()),
            [
                ("info", "i.svg"),
                ("info:outline", "o.svg"),
                ("info:filled", "f.svg")
            ]
        );
    }

    fn motion(tempo: Option<f64>, scale: bool) -> Option<ThemeMotionConfig> {
        Some(ThemeMotionConfig {
            tempo,
            fast: scale.then_some(125.0),
            medium: scale.then_some(300.0),
            ratio: scale.then_some(0.75),
            ..ThemeMotionConfig::default()
        })
    }

    /// The tempo a resolved theme plays at, as the animator reads it.
    fn played(theme: &DefinedTheme) -> f64 {
        crate::ResolvedTheme::new(
            Arc::new(theme.to_theme()),
            Appearance::Dark,
            1280.0,
            Pointer::Fine,
        )
        .motion_tempo()
    }

    // The motion tempo: the `--_astryx-motion-tempo` the theme writes, which a
    // duration scale alone writes as 1 (upstream `resolveThemeValues`), and whether
    // the theme sets `tempo` (FR5: springs follow the tempo only when it does).
    #[test]
    fn carries_the_motion_tempo_the_theme_writes() {
        assert_eq!(
            define(DefineThemeInput::new("plain")).motion_tempo(),
            MotionTempo::DEFAULT
        );
        let slow = define(DefineThemeInput {
            motion: motion(Some(1.5), false),
            ..DefineThemeInput::new("slow")
        });
        assert_eq!(
            slow.motion_tempo(),
            MotionTempo {
                tempo: 1.5,
                is_set: true
            }
        );
        assert_eq!(played(&slow), 1.5);
        // Clamped to 0.75–1.5.
        let clamped = define(DefineThemeInput {
            motion: motion(Some(3.0), false),
            ..DefineThemeInput::new("clamped")
        });
        assert_eq!(clamped.motion_tempo().tempo, 1.5);
        // A duration scale alone plays its springs at 1.
        let scaled = define(DefineThemeInput {
            motion: motion(None, true),
            ..DefineThemeInput::new("scaled")
        });
        assert_eq!(
            scaled
                .tokens
                .get("--_astryx-motion-tempo")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(scaled.motion_tempo(), MotionTempo::DEFAULT);
        assert!(scaled.to_theme().adaptations.is_empty());
        assert_eq!(slow.to_theme().motion_tempo, 1.5);
        assert_eq!(played(&scaled), 1.0);
        // A child inherits its base's tempo, and can set it back to 1.
        let child = define(DefineThemeInput {
            extends: Some(slow.clone()),
            ..DefineThemeInput::new("child")
        });
        assert_eq!(child.motion_tempo(), slow.motion_tempo());
        assert_eq!(played(&child), 1.5);
        let reset = define(DefineThemeInput {
            extends: Some(slow),
            motion: motion(Some(1.0), false),
            ..DefineThemeInput::new("reset")
        });
        assert_eq!(
            reset.motion_tempo(),
            MotionTempo {
                tempo: 1.0,
                is_set: true
            }
        );
        assert_eq!(played(&reset), 1.0);
        // Core's default theme plays at 1.
        assert_eq!(
            crate::ResolvedTheme::new(
                Arc::new(to_theme(None)),
                Appearance::Light,
                1280.0,
                Pointer::Fine
            )
            .motion_tempo(),
            1.0
        );
    }
}
