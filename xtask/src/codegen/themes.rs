//! One Rust module per published theme, plus the report of override rules
//! that have no GPUI form yet.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use super::literal;
use super::tokens::{self, Node};

/// A CSS property in a component override as a `StyleProp` constructor.
fn style_prop(property: &str, value: &Value) -> Result<Option<String>> {
    let expr = || literal::expr(value);
    Ok(Some(match property {
        "backgroundColor" => format!("StyleProp::Background({})", expr()?),
        "color" => format!("StyleProp::TextColor({})", expr()?),
        "borderColor" => format!("StyleProp::BorderColor({})", expr()?),
        "borderWidth" => format!("StyleProp::BorderWidth({})", expr()?),
        "borderStyle" => format!("StyleProp::BorderStyle({})", expr()?),
        "borderRadius" => format!("StyleProp::Radius({})", expr()?),
        "fontFamily" => format!("StyleProp::FontFamily({})", expr()?),
        "fontSize" => format!("StyleProp::FontSize({})", expr()?),
        "fontWeight" => format!("StyleProp::FontWeight({})", expr()?),
        "lineHeight" => format!("StyleProp::LineHeight({})", expr()?),
        "height" => format!("StyleProp::Height({})", expr()?),
        "boxShadow" => format!("StyleProp::Shadow({})", expr()?),
        "opacity" => format!("StyleProp::Opacity({})", expr()?),
        "padding" | "paddingInline" | "paddingBlock" => {
            let edges = &value["edges"];
            let len = |v: &Value| -> Result<String> {
                let mut typed = Map::new();
                typed.insert("length".into(), v["length"].clone());
                typed.insert("rem".into(), v["rem"].clone());
                literal::expr(&Value::Object(typed))
            };
            if let Some(all) = edges["all"].as_array() {
                format!(
                    "StyleProp::Padding(Edges {{ top: {}, right: {}, bottom: {}, left: {} }})",
                    len(&all[0])?,
                    len(&all[1])?,
                    len(&all[2])?,
                    len(&all[3])?
                )
            } else if let Some(pair) = edges["inline"].as_array() {
                format!(
                    "StyleProp::PaddingInline({}, {})",
                    len(&pair[0])?,
                    len(&pair[1])?
                )
            } else if let Some(pair) = edges["block"].as_array() {
                format!(
                    "StyleProp::PaddingBlock({}, {})",
                    len(&pair[0])?,
                    len(&pair[1])?
                )
            } else {
                bail!("padding without edges")
            }
        }
        _ => return Ok(None),
    }))
}

/// A declaration list for one mode, or the reasons it cannot be written.
fn declarations(props: &Map<String, Value>, mode: &str, todo: &mut Vec<String>) -> Result<String> {
    let mut out = Vec::new();
    for (property, by_mode) in props {
        let value = &by_mode[mode];
        if property.starts_with("--") {
            if tokens::token_path(property)?.is_none() {
                todo.push(format!("private token `{property}`"));
                continue;
            }
            out.push(format!(
                "Declaration::Token(Cow::Borrowed({property:?}), {})",
                literal::token_value(value)?
            ));
        } else if let Some(prop) = style_prop(property, value)? {
            out.push(format!("Declaration::Style({prop})"));
        } else if mode == "light" {
            todo.push(format!("CSS property `{property}`"));
        }
    }
    Ok(format!("Cow::Borrowed(&[{}])", out.join(", ")))
}

fn token_list(tokens: &Map<String, Value>) -> Result<String> {
    let mut out = Vec::new();
    for (token, value) in tokens {
        if tokens::token_path(token)?.is_none() {
            continue;
        }
        out.push(format!(
            "(Cow::Borrowed({token:?}), {})",
            literal::token_value(value)?
        ));
    }
    Ok(format!("Cow::Borrowed(&[{}])", out.join(", ")))
}

pub struct ThemeOutput {
    pub module: String,
    pub todo: Vec<String>,
}

/// The module for one theme. `lucide` maps lucide component names to icon
/// file stems; `default` fills tokens a theme lacks.
pub fn render_theme(
    theme: &Value,
    surface: &Node,
    default: &Value,
    lucide: &Map<String, Value>,
) -> Result<ThemeOutput> {
    let name = theme["name"].as_str().context("theme without a name")?;
    let mut out = String::new();
    let mut todo = Vec::new();
    for mode in ["light", "dark"] {
        let tokens = theme["tokens"][mode].as_object().context("tokens")?;
        let fallback = default["tokens"][mode]
            .as_object()
            .context("default tokens")?;
        writeln!(
            out,
            "/// The {name} theme's tokens in {mode} mode.\npub const {}: Tokens = {};\n",
            mode.to_uppercase(),
            tokens::render_value(surface, tokens, fallback)?
        )?;
    }

    let mut rules = Vec::new();
    for rule in theme["components"].as_array().context("components")? {
        let target = rule["target"].as_str().unwrap_or_default();
        let when: Vec<String> = rule["when"]
            .as_array()
            .context("when")?
            .iter()
            .map(|pair| {
                format!(
                    "(Cow::Borrowed({:?}), Cow::Borrowed({:?}))",
                    pair[0].as_str().unwrap_or(""),
                    pair[1].as_str().unwrap_or("")
                )
            })
            .collect();
        let mut when = when;
        let mut selector = rule["selector"].as_str();
        // A table row's even stripe: upstream's rows carry `parity`, so the
        // selector becomes that condition.
        if let Some(parity) = selector.and_then(parity_selector) {
            when.push(format!(
                "(Cow::Borrowed(\"parity\"), Cow::Borrowed({parity:?}))"
            ));
            selector = None;
        }
        let state = match rule["state"].as_str() {
            Some("hover") => "Some(RuleState::Hover)",
            Some("active") => "Some(RuleState::Active)",
            Some("focus-visible") => "Some(RuleState::FocusVisible)",
            _ => "None",
        };
        let label = format!(
            "`{target}` [{}]{}",
            rule["when"]
                .as_array()
                .map(|w| w
                    .iter()
                    .map(|p| format!(
                        "{}:{}",
                        p[0].as_str().unwrap_or(""),
                        p[1].as_str().unwrap_or("")
                    ))
                    .collect::<Vec<_>>()
                    .join("+"))
                .unwrap_or_default(),
            selector
                .map(|s| format!(" `{}`", truncate(s, 90)))
                .unwrap_or_default()
        );
        if selector.is_some() && state == "None" {
            todo.push(format!("{label}: selector has no GPUI form"));
            continue;
        }
        for item in rule["unsupported"].as_array().into_iter().flatten() {
            todo.push(format!(
                "{label}: `{}: {}` ({})",
                item["property"].as_str().unwrap_or(""),
                item["value"].as_str().unwrap_or(""),
                item["reason"].as_str().unwrap_or("")
            ));
        }
        let props = rule["props"].as_object().context("props")?;
        let mut rule_todo = Vec::new();
        let light = declarations(props, "light", &mut rule_todo)?;
        let dark = declarations(props, "dark", &mut Vec::new())?;
        todo.extend(rule_todo.into_iter().map(|t| format!("{label}: {t}")));
        rules.push(format!(
            "ComponentRule {{ target: Cow::Borrowed({target:?}), when: Cow::Borrowed(&[{}]), state: {state}, light: {light}, dark: {dark} }}",
            when.join(", ")
        ));
    }
    writeln!(
        out,
        "/// The {name} theme's component overrides.\npub const COMPONENTS: &[ComponentRule] = &[{}];\n",
        rules.join(",\n")
    )?;

    let breakpoints = theme["widthBreakpoints"].as_object();
    let mut adaptations = Vec::new();
    for rule in theme["adaptations"].as_array().into_iter().flatten() {
        let when = &rule["when"];
        let pointer = match when["pointer"].as_str() {
            Some("coarse") => "Some(Pointer::Coarse)",
            Some("fine") => "Some(Pointer::Fine)",
            None => "None",
            Some(other) => bail!("{name}: adaptation pointer {other}"),
        };
        let width = |key: &str| -> Result<String> {
            match when["width"][key].as_str() {
                None => Ok("None".into()),
                Some(bp) => {
                    let px = breakpoints
                        .and_then(|b| b.get(bp))
                        .and_then(Value::as_f64)
                        .with_context(|| format!("{name}: no breakpoint {bp}"))?;
                    Ok(format!("Some({})", literal::float(px)))
                }
            }
        };
        adaptations.push(format!(
            "Adaptation {{ pointer: {pointer}, width_below: {}, width_at_least: {}, light: {}, dark: {} }}",
            width("below")?,
            width("above")?,
            token_list(rule["tokens"]["light"].as_object().context("adaptation tokens")?)?,
            token_list(rule["tokens"]["dark"].as_object().context("adaptation tokens")?)?,
        ));
    }
    writeln!(
        out,
        "/// The {name} theme's adaptations to width and pointer.\npub const ADAPTATIONS: &[Adaptation] = &[{}];\n",
        adaptations.join(",\n")
    )?;

    for (key, constant) in [("onDark", "ON_DARK"), ("onLight", "ON_LIGHT")] {
        let value = match theme[key].as_object() {
            None => "None".to_string(),
            Some(surface) => format!(
                "Some(SurfaceOverride {{ appearance: Appearance::{}, tokens: {} }})",
                if surface["colorScheme"].as_str() == Some("dark") {
                    "Dark"
                } else {
                    "Light"
                },
                token_list(surface["tokens"].as_object().context("surface tokens")?)?
            ),
        };
        writeln!(
            out,
            "/// Token overrides for content on a {} surface.\npub const {constant}: Option<SurfaceOverride> = {value};\n",
            if constant == "ON_DARK" {
                "dark"
            } else {
                "light"
            }
        )?;
    }

    let mut icons = Vec::new();
    for (icon, source) in theme["icons"].as_object().context("icons")? {
        let component = source["lucide"].as_str().context("icon without lucide")?;
        let stem = lucide
            .get(component)
            .and_then(Value::as_str)
            .with_context(|| format!("{name}: lucide {component} was not exported"))?;
        icons.push(format!(
            "(Cow::Borrowed({icon:?}), Cow::Borrowed(\"icons/lucide/{stem}.svg\"))"
        ));
    }
    writeln!(
        out,
        "/// The icon each semantic name draws in this theme.\npub const ICONS: &[(Cow<'static, str>, Cow<'static, str>)] = &[{}];\n",
        icons.join(", ")
    )?;

    let mut fonts = Vec::new();
    for face in theme["fonts"].as_array().context("fonts")? {
        let family = face["family"].as_str().unwrap_or_default();
        let weight = face["weight"].as_str().unwrap_or("400");
        let style = face["style"].as_str().unwrap_or("normal");
        let slug = family.to_lowercase().replace(' ', "-");
        let file = format!(
            "{}-{weight}{}.ttf",
            family.replace(' ', ""),
            if style == "italic" { "Italic" } else { "" }
        );
        fonts.push(format!(
            "include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/fonts/{slug}/{file}\"))"
        ));
    }
    writeln!(
        out,
        "/// The font files this theme draws with.\npub const FONTS: &[&[u8]] = &[{}];\n",
        fonts.join(",\n")
    )?;

    writeln!(
        out,
        "/// The {name} theme.\npub fn theme() -> Theme {{\n    Theme {{\n        name: Cow::Borrowed({name:?}),\n        light: LIGHT,\n        dark: DARK,\n        components: Cow::Borrowed(COMPONENTS),\n        adaptations: Cow::Borrowed(ADAPTATIONS),\n        on_dark: ON_DARK,\n        on_light: ON_LIGHT,\n        icons: Cow::Borrowed(ICONS),\n        fonts: Cow::Borrowed(FONTS),\n    }}\n}}"
    )?;
    Ok(ThemeOutput { module: out, todo })
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

/// The markdown report of overrides that still need a hand port.
pub fn render_todo(todo: &BTreeMap<String, Vec<String>>) -> String {
    let mut out = String::from(
        "<!-- @generated by `cargo xtask codegen`. Do not edit. -->\n\n# Theme overrides without a GPUI form\n\nThese theme override rules were not generated because GPUI has no direct\nequivalent of their CSS selector or property. Each needs a hand port in the\ncomponent it targets.\n",
    );
    for (theme, items) in todo {
        if items.is_empty() {
            continue;
        }
        let _ = write!(out, "\n## {theme}\n\n");
        for item in items {
            let _ = writeln!(out, "- {item}");
        }
    }
    out
}

/// `:nth-child(even ...)` or `:nth-child(odd ...)`: the parity a table row
/// publishes, the one structural selector with a GPUI form.
fn parity_selector(selector: &str) -> Option<&'static str> {
    let s = selector.trim();
    if s.starts_with(":nth-child(even") {
        Some("even")
    } else if s.starts_with(":nth-child(odd") {
        Some("odd")
    } else {
        None
    }
}
