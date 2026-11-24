//! Typed spec values (see tools/export/lib/values.mjs) as `const` Rust
//! expressions over `astryx_theme` and GPUI types.

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// The kind of a typed value: the single key of its JSON object.
pub fn kind(value: &Value) -> Result<&str> {
    let object = value.as_object().context("typed value is not an object")?;
    if object.len() == 1 || object.contains_key("length") {
        let key = if object.contains_key("length") {
            "length"
        } else {
            object.keys().next().unwrap()
        };
        return Ok(key);
    }
    bail!(
        "typed value has keys {:?}",
        object.keys().collect::<Vec<_>>()
    )
}

/// The Rust type a token of this kind has.
pub fn rust_type(kind: &str) -> Result<&'static str> {
    Ok(match kind {
        "color" => "Hsla",
        "length" => "AbsoluteLength",
        "weight" => "FontWeight",
        "leading" | "number" => "f32",
        "duration" => "Duration",
        "ease" => "CubicBezier",
        "families" => "FontStack",
        "shadow" => "Shadow",
        "keyword" => "Keyword",
        other => bail!("no Rust type for kind {other}"),
    })
}

/// The `TokenValue` variant for a kind.
pub fn variant(kind: &str) -> Result<&'static str> {
    Ok(match kind {
        "color" => "Color",
        "length" => "Length",
        "weight" => "Weight",
        "leading" => "Leading",
        "number" => "Number",
        "duration" => "Duration",
        "ease" => "Ease",
        "families" => "Families",
        "shadow" => "Shadow",
        "keyword" => "Keyword",
        other => bail!("no TokenValue variant for kind {other}"),
    })
}

/// A typed value as a `const` expression of its kind's Rust type.
pub fn expr(value: &Value) -> Result<String> {
    let kind = kind(value)?;
    let inner = &value[kind];
    Ok(match kind {
        "color" => color(inner.as_str().context("color is not a string")?)?,
        "length" => length(value)?,
        "weight" => format!("FontWeight({})", float(number(inner)?)),
        "leading" | "number" => float(number(inner)?),
        "duration" => format!(
            "Duration::from_micros({})",
            (number(inner)? * 1000.0).round() as u64
        ),
        "ease" => {
            let points = inner.as_array().context("ease is not an array")?;
            let points: Vec<String> = points
                .iter()
                .map(|p| number(p).map(float))
                .collect::<Result<_>>()?;
            format!("CubicBezier({})", points.join(", "))
        }
        "families" => {
            let names = inner.as_array().context("families is not an array")?;
            let names: Vec<String> = names
                .iter()
                .map(|n| format!("Cow::Borrowed({:?})", n.as_str().unwrap_or_default()))
                .collect();
            format!("FontStack(Cow::Borrowed(&[{}]))", names.join(", "))
        }
        "shadow" => {
            let layers = inner.as_array().context("shadow is not an array")?;
            let layers: Vec<String> = layers.iter().map(shadow_layer).collect::<Result<_>>()?;
            format!("Shadow(Cow::Borrowed(&[{}]))", layers.join(", "))
        }
        "keyword" => format!(
            "Keyword(Cow::Borrowed({:?}))",
            inner.as_str().unwrap_or_default()
        ),
        other => bail!("cannot write a {other} value"),
    })
}

/// A typed value wrapped in its `TokenValue` variant.
pub fn token_value(value: &Value) -> Result<String> {
    Ok(format!(
        "TokenValue::{}({})",
        variant(kind(value)?)?,
        expr(value)?
    ))
}

fn number(value: &Value) -> Result<f64> {
    value.as_f64().context("expected a number")
}

/// An `f32` literal: the shortest form that reads back to the same `f32`.
pub fn float(value: f64) -> String {
    let f = value as f32;
    let text = format!("{f:?}");
    if text.contains(['.', 'e', 'E']) || text.contains("inf") || text.contains("NaN") {
        text
    } else {
        format!("{text}.0")
    }
}

fn length(value: &Value) -> Result<String> {
    let px = number(&value["length"])?;
    if value["rem"].as_bool().unwrap_or(false) {
        Ok(format!("AbsoluteLength::Rems(rems({}))", float(px / 16.0)))
    } else {
        Ok(format!("AbsoluteLength::Pixels(px({}))", float(px)))
    }
}

/// `#RRGGBBAA` as `hsla(..)`, converted exactly as GPUI's `From<Rgba> for
/// Hsla` converts it, so a token reads back as the same `Rgba`.
pub fn color(hex: &str) -> Result<String> {
    let hex = hex.strip_prefix('#').context("color is not #RRGGBBAA")?;
    if hex.len() != 8 {
        bail!("color #{hex} is not #RRGGBBAA");
    }
    let channel =
        |i: usize| -> Result<f32> { Ok(u8::from_str_radix(&hex[i..i + 2], 16)? as f32 / 255.0) };
    let (r, g, b, a) = (channel(0)?, channel(2)?, channel(4)?, channel(6)?);
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let delta = max - min;
    let l = (max + min) / 2.0;
    let s = if l == 0.0 || l == 1.0 {
        0.0
    } else if l < 0.5 {
        delta / (2.0 * l)
    } else {
        delta / (2.0 - 2.0 * l)
    };
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    let f = |v: f32| float(v as f64);
    Ok(format!("hsla({}, {}, {}, {})", f(h), f(s), f(l), f(a)))
}

fn shadow_layer(layer: &Value) -> Result<String> {
    let n = |key: &str| number(&layer[key]).map(float);
    Ok(format!(
        "BoxShadow {{ color: {}, offset: Point {{ x: px({}), y: px({}) }}, blur_radius: px({}), spread_radius: px({}), inset: {} }}",
        color(layer["color"].as_str().context("shadow color")?)?,
        n("x")?,
        n("y")?,
        n("blur")?,
        n("spread")?,
        layer["inset"].as_bool().unwrap_or(false),
    ))
}
