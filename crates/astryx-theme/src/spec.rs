//! Themes from JSON at run time: the typed format `tools/export` writes to
//! `spec/themes/*.json` (see `tools/export/lib/values.mjs`). The compiled
//! themes are generated from the same files, so a theme loaded here draws the
//! same as its compiled twin, and editing the JSON lets a theme be tuned live
//! without a rebuild.

use std::borrow::Cow;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use gpui::{AbsoluteLength, BoxShadow, Edges, FontWeight, Hsla, Point, Rgba, px, rems};
use serde_json::{Map, Value};

use crate::types::*;
use crate::{Theme, Tokens};

fn number(value: &Value) -> Result<f32> {
    value
        .as_f64()
        .map(|n| n as f32)
        .context("expected a number")
}

fn color(hex: &str) -> Result<Hsla> {
    let hex = hex.strip_prefix('#').context("color is not #RRGGBBAA")?;
    if hex.len() != 8 {
        bail!("color #{hex} is not #RRGGBBAA");
    }
    let channel =
        |i: usize| -> Result<f32> { Ok(u8::from_str_radix(&hex[i..i + 2], 16)? as f32 / 255.0) };
    Ok(Rgba {
        r: channel(0)?,
        g: channel(2)?,
        b: channel(4)?,
        a: channel(6)?,
    }
    .into())
}

fn length(value: &Value) -> Result<AbsoluteLength> {
    let px_value = number(&value["length"])?;
    Ok(if value["rem"].as_bool().unwrap_or(false) {
        AbsoluteLength::Rems(rems(px_value / 16.0))
    } else {
        AbsoluteLength::Pixels(px(px_value))
    })
}

fn shadow(layers: &Value) -> Result<Shadow> {
    let layers = layers.as_array().context("shadow is not an array")?;
    let layers = layers
        .iter()
        .map(|layer| {
            Ok(BoxShadow {
                color: color(layer["color"].as_str().context("shadow color")?)?,
                offset: Point {
                    x: px(number(&layer["x"])?),
                    y: px(number(&layer["y"])?),
                },
                blur_radius: px(number(&layer["blur"])?),
                spread_radius: px(number(&layer["spread"])?),
                inset: layer["inset"].as_bool().unwrap_or(false),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Shadow(Cow::Owned(layers)))
}

/// One typed value (`{color}`, `{length, rem}`, `{weight}`, …) as a token value.
pub fn token_value(value: &Value) -> Result<TokenValue> {
    let object = value.as_object().context("typed value is not an object")?;
    if object.contains_key("length") {
        return Ok(TokenValue::Length(length(value)?));
    }
    let (kind, inner) = object.iter().next().context("empty typed value")?;
    Ok(match kind.as_str() {
        "color" => TokenValue::Color(color(inner.as_str().context("color")?)?),
        "weight" => TokenValue::Weight(FontWeight(number(inner)?)),
        "leading" => TokenValue::Leading(number(inner)?),
        "number" => TokenValue::Number(number(inner)?),
        "duration" => TokenValue::Duration(Duration::from_micros(
            (number(inner)? * 1000.0).round() as u64,
        )),
        "ease" => {
            let p = inner.as_array().context("ease")?;
            TokenValue::Ease(CubicBezier(
                number(&p[0])?,
                number(&p[1])?,
                number(&p[2])?,
                number(&p[3])?,
            ))
        }
        "families" => TokenValue::Families(FontStack(Cow::Owned(
            inner
                .as_array()
                .context("families")?
                .iter()
                .map(|f| Cow::Owned(f.as_str().unwrap_or_default().to_string()))
                .collect(),
        ))),
        "shadow" => TokenValue::Shadow(shadow(inner)?),
        "keyword" => TokenValue::Keyword(Keyword(Cow::Owned(
            inner.as_str().unwrap_or_default().to_string(),
        ))),
        other => bail!("unknown value kind {other}"),
    })
}

fn tokens(base: &Tokens, values: &Map<String, Value>) -> Result<Tokens> {
    let mut tokens = base.clone();
    for (name, value) in values {
        if name.starts_with("--_") {
            continue;
        }
        let value = token_value(value).with_context(|| format!("token {name}"))?;
        // A token the port does not type yet is skipped, as codegen skips it.
        let _ = tokens.set(name, value);
    }
    Ok(tokens)
}

type TokenList = Cow<'static, [(Cow<'static, str>, TokenValue)]>;

fn token_list(values: &Value) -> Result<TokenList> {
    let mut out = Vec::new();
    for (name, value) in values.as_object().into_iter().flatten() {
        if !name.starts_with("--_") {
            out.push((Cow::Owned(name.clone()), token_value(value)?));
        }
    }
    Ok(Cow::Owned(out))
}

fn style_prop(property: &str, value: &Value) -> Result<Option<StyleProp>> {
    let typed = || token_value(value);
    let as_color = || match typed()? {
        TokenValue::Color(c) => Ok(c),
        _ => bail!("{property} is not a color"),
    };
    let as_length = || match typed()? {
        TokenValue::Length(l) => Ok(l),
        _ => bail!("{property} is not a length"),
    };
    Ok(Some(match property {
        "backgroundColor" => StyleProp::Background(as_color()?),
        "color" => StyleProp::TextColor(as_color()?),
        "borderColor" => StyleProp::BorderColor(as_color()?),
        "borderWidth" => StyleProp::BorderWidth(as_length()?),
        "borderRadius" => StyleProp::Radius(as_length()?),
        "fontSize" => StyleProp::FontSize(as_length()?),
        "height" => StyleProp::Height(as_length()?),
        "borderStyle" => match typed()? {
            TokenValue::Keyword(k) => StyleProp::BorderStyle(k),
            _ => bail!("borderStyle is not a keyword"),
        },
        "fontFamily" => match typed()? {
            TokenValue::Families(f) => StyleProp::FontFamily(f),
            _ => bail!("fontFamily is not a family list"),
        },
        "fontWeight" => match typed()? {
            TokenValue::Weight(w) => StyleProp::FontWeight(w),
            _ => bail!("fontWeight is not a weight"),
        },
        "lineHeight" => match typed()? {
            TokenValue::Leading(l) => StyleProp::LineHeight(l),
            _ => bail!("lineHeight is not a line height"),
        },
        "boxShadow" => match typed()? {
            TokenValue::Shadow(s) => StyleProp::Shadow(s),
            _ => bail!("boxShadow is not a shadow"),
        },
        "opacity" => match typed()? {
            TokenValue::Number(n) => StyleProp::Opacity(n),
            _ => bail!("opacity is not a number"),
        },
        "padding" | "paddingInline" | "paddingBlock" => {
            let edges = &value["edges"];
            if let Some(all) = edges["all"].as_array() {
                StyleProp::Padding(Edges {
                    top: length(&all[0])?,
                    right: length(&all[1])?,
                    bottom: length(&all[2])?,
                    left: length(&all[3])?,
                })
            } else if let Some(pair) = edges["inline"].as_array() {
                StyleProp::PaddingInline(length(&pair[0])?, length(&pair[1])?)
            } else if let Some(pair) = edges["block"].as_array() {
                StyleProp::PaddingBlock(length(&pair[0])?, length(&pair[1])?)
            } else {
                bail!("padding without edges")
            }
        }
        _ => return Ok(None),
    }))
}

fn declarations(props: &Map<String, Value>, mode: &str) -> Result<Vec<Declaration>> {
    let mut out = Vec::new();
    for (property, by_mode) in props {
        let value = &by_mode[mode];
        if property.starts_with("--") {
            if !property.starts_with("--_") {
                out.push(Declaration::Token(
                    Cow::Owned(property.clone()),
                    token_value(value)?,
                ));
            }
        } else if let Some(prop) = style_prop(property, value)? {
            out.push(Declaration::Style(prop));
        }
    }
    Ok(out)
}

fn rules(components: &Value) -> Result<Vec<ComponentRule>> {
    let mut out = Vec::new();
    for rule in components.as_array().into_iter().flatten() {
        let state = match rule["state"].as_str() {
            Some("hover") => Some(RuleState::Hover),
            Some("active") => Some(RuleState::Active),
            Some("focus-visible") => Some(RuleState::FocusVisible),
            _ => None,
        };
        // A table row's even or odd stripe is its `parity`, as codegen maps
        // it; any other nested selector has no GPUI form and is skipped.
        let parity = rule["selector"].as_str().and_then(|s| {
            let s = s.trim();
            if s.starts_with(":nth-child(even") {
                Some("even")
            } else if s.starts_with(":nth-child(odd") {
                Some("odd")
            } else {
                None
            }
        });
        if rule["selector"].is_string() && state.is_none() && parity.is_none() {
            continue;
        }
        let mut when = rule["when"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|pair| {
                (
                    Cow::Owned(pair[0].as_str().unwrap_or_default().to_string()),
                    Cow::Owned(pair[1].as_str().unwrap_or_default().to_string()),
                )
            })
            .collect::<Vec<_>>();
        if let Some(parity) = parity {
            when.push((Cow::Borrowed("parity"), Cow::Borrowed(parity)));
        }
        let props = rule["props"].as_object().context("rule props")?;
        out.push(ComponentRule {
            target: Cow::Owned(rule["target"].as_str().unwrap_or_default().to_string()),
            when: Cow::Owned(when),
            state,
            light: Cow::Owned(declarations(props, "light")?),
            dark: Cow::Owned(declarations(props, "dark")?),
        });
    }
    Ok(out)
}

fn surface(value: &Value) -> Result<Option<SurfaceOverride>> {
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(SurfaceOverride {
        appearance: if value["colorScheme"] == "dark" {
            Appearance::Dark
        } else {
            Appearance::Light
        },
        tokens: token_list(&value["tokens"])?,
    }))
}

impl Theme {
    /// A theme from its typed JSON (`spec/themes/<name>.json`).
    ///
    /// Tokens the JSON lacks keep `base`'s values; fonts and icons come from
    /// `base` too, since the JSON names them but does not carry them.
    pub fn from_json(json: &str, base: &Theme) -> Result<Theme> {
        let spec: Value = serde_json::from_str(json).context("theme JSON does not parse")?;
        let breakpoints = spec["widthBreakpoints"].as_object();
        let mut adaptations = Vec::new();
        for rule in spec["adaptations"].as_array().into_iter().flatten() {
            let when = &rule["when"];
            let width = |key: &str| {
                when["width"][key]
                    .as_str()
                    .and_then(|bp| breakpoints.and_then(|b| b.get(bp)).and_then(Value::as_f64))
                    .map(|v| v as f32)
            };
            adaptations.push(Adaptation {
                pointer: match when["pointer"].as_str() {
                    Some("coarse") => Some(Pointer::Coarse),
                    Some("fine") => Some(Pointer::Fine),
                    _ => None,
                },
                width_below: width("below"),
                width_at_least: width("from"),
                light: token_list(&rule["tokens"]["light"])?,
                dark: token_list(&rule["tokens"]["dark"])?,
            });
        }
        Ok(Theme {
            name: Cow::Owned(spec["name"].as_str().unwrap_or(&base.name).to_string()),
            light: tokens(
                &base.light,
                spec["tokens"]["light"]
                    .as_object()
                    .context("light tokens")?,
            )?,
            dark: tokens(
                &base.dark,
                spec["tokens"]["dark"].as_object().context("dark tokens")?,
            )?,
            components: Cow::Owned(rules(&spec["components"])?),
            adaptations: Cow::Owned(adaptations),
            width_breakpoints: {
                let point = |name: &str, default: f32| {
                    breakpoints
                        .and_then(|b| b.get(name))
                        .and_then(Value::as_f64)
                        .map_or(default, |v| v as f32)
                };
                let d = WidthBreakpoints::DEFAULT;
                WidthBreakpoints {
                    sm: point("sm", d.sm),
                    md: point("md", d.md),
                    lg: point("lg", d.lg),
                    xl: point("xl", d.xl),
                    xl2: point("2xl", d.xl2),
                }
            },
            on_dark: surface(&spec["onDark"])?,
            on_light: surface(&spec["onLight"])?,
            icons: base.icons.clone(),
            fonts: base.fonts.clone(),
            motion_tempo: spec["motionTempo"].as_f64().unwrap_or(1.0),
        })
    }
}
