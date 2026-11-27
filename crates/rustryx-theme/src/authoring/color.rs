//! Color parsing and color spaces for theme authoring.
//!
//! Two upstream modules share this file:
//!
//! - `utils/color.ts`: CSS color strings (hex, `rgb()`/`rgba()`, the named
//!   colors token expressions use) to and from [`Rgba`].
//! - `palette/color.ts`: the tonal palette generator's color math on hex
//!   colors: sRGB, linear sRGB, OKLab and OKLCH, the sRGB gamut limit at an
//!   OKLCH lightness and hue, and WCAG luminance and contrast.
//!
//! Upstream runs in JavaScript, and its outputs are strings shaped by
//! JavaScript's number rules (`Math.round` rounds halves up, `toFixed` rounds
//! ties up, `parseFloat` reads a number's longest prefix). The `js_*` helpers
//! at the end of this file reproduce those rules, so every port here returns
//! the string upstream returns for the same input. The other authoring
//! modules use them too.

use std::f64::consts::PI;
use std::fmt;

use super::js::{
    is_js_whitespace, js_max, js_min, js_number_to_string, js_parse_float, js_parse_int_hex,
    js_round, js_to_fixed, js_to_hex_string, js_trim,
};

// =============================================================================
// utils/color.ts
// =============================================================================

/// A color as red, green and blue channels from 0 to 255 and an alpha from 0
/// to 1. Upstream `RGBA`.
///
/// Channels need not be whole: compositing and OKLCH conversion produce
/// fractional channels, as upstream's do, and formatting rounds them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    /// Red, 0 to 255.
    pub r: f64,
    /// Green, 0 to 255.
    pub g: f64,
    /// Blue, 0 to 255.
    pub b: f64,
    /// Alpha, 0 (transparent) to 1 (opaque).
    pub a: f64,
}

/// The named colors token expressions use. Upstream `NAMED_COLORS`.
const NAMED_COLORS: [(&str, Rgba); 3] = [
    (
        "transparent",
        Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
    ),
    (
        "black",
        Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
    ),
    (
        "white",
        Rgba {
            r: 255.0,
            g: 255.0,
            b: 255.0,
            a: 1.0,
        },
    ),
];

/// `Math.max(min, Math.min(max, value))`: a NaN stays NaN. Upstream `clamp`.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    js_max(min, js_min(max, value))
}

/// Parses a hex color: `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`, with or
/// without the `#` and with surrounding whitespace allowed. `None` for
/// anything else. Upstream `parseHex`.
pub fn parse_hex(hex: &str) -> Option<Rgba> {
    let body = js_trim(hex);
    let body = body.strip_prefix('#').unwrap_or(body);
    // Only ASCII hex digits pass, so a non-ASCII body fails whatever its
    // length counts as.
    if !body.is_ascii() {
        return None;
    }
    let normalized: String = if body.len() == 3 || body.len() == 4 {
        body.chars().flat_map(|c| [c, c]).collect()
    } else {
        body.to_string()
    };
    if (normalized.len() != 6 && normalized.len() != 8)
        || !normalized.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    let byte = |at: usize| f64::from(u8::from_str_radix(&normalized[at..at + 2], 16).unwrap_or(0));
    Some(Rgba {
        r: byte(0),
        g: byte(2),
        b: byte(4),
        a: if normalized.len() == 8 {
            byte(6) / 255.0
        } else {
            1.0
        },
    })
}

/// Parses an `rgb()` or `rgba()` color: comma- or space-separated channels,
/// an optional `/ alpha`, and percentage channels. Channels clamp to 0–255
/// and alpha to 0–1; `None` when fewer than three channels parse. Upstream
/// `parseRgb`.
pub fn parse_rgb(value: &str) -> Option<Rgba> {
    let open = value.find('(')?;
    if !js_trim(value).ends_with(')') {
        return None;
    }
    let close = value.rfind(')')?;
    let body = value.get(open + 1..close).unwrap_or("").replace('/', " ");
    let parts: Vec<&str> = body
        .split(|c: char| is_js_whitespace(c) || c == ',')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 3 {
        return None;
    }

    let channel = |part: &str| {
        let n = if part.ends_with('%') {
            (js_parse_float(part) / 100.0) * 255.0
        } else {
            js_parse_float(part)
        };
        clamp(n, 0.0, 255.0)
    };
    let r = channel(parts[0]);
    let g = channel(parts[1]);
    let b = channel(parts[2]);
    if [r, g, b].iter().any(|c| c.is_nan()) {
        return None;
    }

    let mut a = 1.0;
    if parts.len() >= 4 {
        let raw = parts[3];
        a = if raw.ends_with('%') {
            js_parse_float(raw) / 100.0
        } else {
            js_parse_float(raw)
        };
        if a.is_nan() {
            return None;
        }
        a = clamp(a, 0.0, 1.0);
    }
    Some(Rgba { r, g, b, a })
}

/// Parses a concrete CSS color: hex, `rgb()`/`rgba()`, or a named color
/// token expressions use (`transparent`, `black`, `white`, in any case).
/// `None` for anything it can't evaluate (`var()`, `oklch()`, other names),
/// so a caller can keep the original expression rather than guess. Upstream
/// `parseColor`.
pub fn parse_color(value: &str) -> Option<Rgba> {
    let trimmed = js_trim(value);
    let name = trimmed.to_lowercase();
    if let Some((_, color)) = NAMED_COLORS.iter().find(|(named, _)| *named == name) {
        return Some(*color);
    }
    if trimmed.starts_with('#') {
        return parse_hex(trimmed);
    }
    let bytes = trimmed.as_bytes();
    let starts_with = |prefix: &[u8]| {
        bytes.len() >= prefix.len() && bytes[..prefix.len()].eq_ignore_ascii_case(prefix)
    };
    if starts_with(b"rgb(") || starts_with(b"rgba(") {
        return parse_rgb(trimmed);
    }
    None
}

/// Red, green and blue (0 to 255; rounded, then clamped) as an uppercase
/// `#RRGGBB`. Upstream `formatHex`.
pub fn format_hex(r: f64, g: f64, b: f64) -> String {
    let channel =
        |c: f64| format!("{:0>2}", js_to_hex_string(clamp(js_round(c), 0.0, 255.0))).to_uppercase();
    format!("#{}{}{}", channel(r), channel(g), channel(b))
}

/// A color as a compact CSS color: `#RRGGBB` when opaque, otherwise
/// `rgba(r, g, b, a)` with whole channels and alpha to at most four decimals
/// (`rgba(0, 100, 224, 0.2)`). Upstream `formatColor`.
pub fn format_color(color: Rgba) -> String {
    let Rgba { r, g, b, a } = color;
    if a >= 1.0 {
        return format_hex(r, g, b);
    }
    let round = |n: f64| js_number_to_string(clamp(js_round(n), 0.0, 255.0));
    format!(
        "rgba({}, {}, {}, {})",
        round(r),
        round(g),
        round(b),
        js_number_to_string(js_parse_float(&js_to_fixed(a, 4)))
    )
}

/// The mid-grey a GL path draws for a color it can't parse: visible, never
/// NaN. Upstream `GL_FALLBACK`.
const GL_FALLBACK: [f64; 3] = [0.5, 0.5, 0.5];

/// A parsed color as `[r, g, b]` from 0 to 1 for GPU uniforms, alpha
/// dropped. `None` or a non-finite channel gives a mid-grey, so the GPU never
/// receives NaN. Compose it with a parser: `to_gl_floats(parse_hex(color))`.
/// Upstream `toGLFloats`.
pub fn to_gl_floats(color: Option<Rgba>) -> [f64; 3] {
    let Some(color) = color else {
        return GL_FALLBACK;
    };
    if ![color.r, color.g, color.b].iter().all(|c| c.is_finite()) {
        return GL_FALLBACK;
    }
    let channel = |c: f64| clamp(c, 0.0, 255.0) / 255.0;
    [channel(color.r), channel(color.g), channel(color.b)]
}

// =============================================================================
// palette/color.ts
// =============================================================================

/// Three color coordinates: red, green and blue, or OKLab's L, a and b.
/// Upstream `ColorTriple`.
pub type ColorTriple = [f64; 3];

/// An sRGB channel (0 to 255) as linear light (0 to 1). Upstream
/// `srgbToLinear` (`palette/color.ts`; `theme/hct.ts`, `theme/contrast.ts`
/// and `theme/statusColor.ts` repeat it).
pub(crate) fn srgb_to_linear(channel: f64) -> f64 {
    let value = channel / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// A linear-light channel (0 to 1) as a whole sRGB channel, clamped to
/// 0–255. Upstream `linearToSrgb` (`palette/color.ts`; `theme/hct.ts`
/// repeats it).
pub(crate) fn linear_to_srgb(channel: f64) -> f64 {
    let value = if channel <= 0.0031308 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    };
    js_round(js_min(255.0, js_max(0.0, value * 255.0)))
}

/// A hex color's red, green and blue (0 to 255), read as upstream reads it:
/// the first `#` dropped, three digits doubled, otherwise the first six
/// characters read with `parseInt(…, 16)`. Unreadable input gives black.
/// Upstream `hexToRgb`.
fn hex_to_rgb(hex: &str) -> ColorTriple {
    let value: Vec<u16> = hex.replacen('#', "", 1).encode_utf16().collect();
    let full: Vec<u16> = if value.len() == 3 {
        vec![value[0], value[0], value[1], value[1], value[2], value[2]]
    } else {
        value[..value.len().min(6)].to_vec()
    };
    // `>>` and `&` work on ToInt32 of the number, which maps NaN to 0 as
    // `as i32` does. Six hex digits always fit.
    let numeric = js_parse_int_hex(&full) as i32;
    [
        f64::from((numeric >> 16) & 0xff),
        f64::from((numeric >> 8) & 0xff),
        f64::from(numeric & 0xff),
    ]
}

/// Red, green and blue (0 to 255; clamped, then rounded) as a lowercase
/// `#rrggbb`. Upstream `rgbToHex`.
fn rgb_to_hex(red: f64, green: f64, blue: f64) -> String {
    let channel = |c: f64| {
        format!(
            "{:0>2}",
            js_to_hex_string(js_round(js_max(0.0, js_min(255.0, c))))
        )
    };
    format!("#{}{}{}", channel(red), channel(green), channel(blue))
}

/// Linear-light red, green and blue (0 to 1) as OKLab `[L, a, b]`. Upstream
/// `linearRgbToOklab`.
pub fn linear_rgb_to_oklab(red: f64, green: f64, blue: f64) -> ColorTriple {
    let l_root = (0.4122214708 * red + 0.5363325363 * green + 0.0514459929 * blue).cbrt();
    let m_root = (0.2119034982 * red + 0.6806995451 * green + 0.1073969566 * blue).cbrt();
    let s_root = (0.0883024619 * red + 0.2817188376 * green + 0.6299787005 * blue).cbrt();
    [
        0.2104542553 * l_root + 0.793617785 * m_root - 0.0040720468 * s_root,
        1.9779984951 * l_root - 2.428592205 * m_root + 0.4505937099 * s_root,
        0.0259040371 * l_root + 0.7827717662 * m_root - 0.808675766 * s_root,
    ]
}

/// OKLab `L`, `a` and `b` as linear-light red, green and blue, unclamped
/// (outside 0–1 when the color is outside sRGB). Upstream `oklabToLinearRgb`.
pub fn oklab_to_linear_rgb(lightness: f64, a: f64, b: f64) -> ColorTriple {
    let l_root = lightness + 0.3963377774 * a + 0.2158037573 * b;
    let m_root = lightness - 0.1055613458 * a - 0.0638541728 * b;
    let s_root = lightness - 0.0894841775 * a - 1.291485548 * b;
    let l = l_root.powf(3.0);
    let m = m_root.powf(3.0);
    let s = s_root.powf(3.0);
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
    ]
}

/// The error [`normalize_color`] returns for anything but a 3- or 6-digit
/// hex color. It displays as upstream's message, `Invalid color: <input>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidColor(pub String);

impl fmt::Display for InvalidColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid color: {}", self.0)
    }
}

impl std::error::Error for InvalidColor {}

/// A 3- or 6-digit hex color, with or without `#`, as a lowercase 6-digit
/// `#rrggbb` (`"ABC"` is `"#aabbcc"`). Anything else, alpha digits included,
/// is an [`InvalidColor`]. Upstream `normalizeColor`.
pub fn normalize_color(input: &str) -> Result<String, InvalidColor> {
    let value = js_trim(input);
    let body = value.strip_prefix('#').unwrap_or(value);
    let is_hex =
        |digits: usize| body.len() == digits && body.bytes().all(|b| b.is_ascii_hexdigit());
    if is_hex(3) {
        let doubled: String = body.chars().flat_map(|c| [c, c]).collect();
        return Ok(format!("#{}", doubled.to_ascii_lowercase()));
    }
    if is_hex(6) {
        return Ok(format!("#{}", body.to_ascii_lowercase()));
    }
    Err(InvalidColor(input.to_string()))
}

/// OKLCH coordinates with the hue in degrees, as [`hex_to_oklch`] returns
/// them. Upstream `hexToOklch`'s `{L, C, H}` result. (Upstream
/// `statusColor.ts`'s `OKLCH` measures hue in radians; that one is
/// [`status_color::Oklch`](super::status_color::Oklch).)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OklchDegrees {
    /// Lightness, 0 to 1. Upstream `L`.
    pub l: f64,
    /// Chroma, 0 to about 0.4. Upstream `C`.
    pub c: f64,
    /// Hue in degrees, 0 to 360. Upstream `H`.
    pub h: f64,
}

/// A 3- or 6-digit hex color as OKLCH, hue in degrees from 0 to 360. Fails
/// as [`normalize_color`] does. Upstream `hexToOklch`.
pub fn hex_to_oklch(hex: &str) -> Result<OklchDegrees, InvalidColor> {
    let [red, green, blue] = hex_to_rgb(&normalize_color(hex)?);
    let [lightness, a, b] = linear_rgb_to_oklab(
        srgb_to_linear(red),
        srgb_to_linear(green),
        srgb_to_linear(blue),
    );
    let mut hue = (b.atan2(a) * 180.0) / PI;
    if hue < 0.0 {
        hue += 360.0;
    }
    Ok(OklchDegrees {
        l: lightness,
        c: (a * a + b * b).sqrt(),
        h: hue,
    })
}

/// Whether an OKLCH color (hue in degrees) is inside sRGB, with 0.001 of
/// slack on each linear channel. Upstream `oklchInGamut`.
fn oklch_in_gamut(lightness: f64, chroma: f64, hue: f64) -> bool {
    let radians = (hue * PI) / 180.0;
    let [red, green, blue] =
        oklab_to_linear_rgb(lightness, chroma * radians.cos(), chroma * radians.sin());
    (-0.001..=1.001).contains(&red)
        && (-0.001..=1.001).contains(&green)
        && (-0.001..=1.001).contains(&blue)
}

/// An OKLCH color (hue in degrees) as a lowercase `#rrggbb`, negative linear
/// channels cut to 0 and the rest clamped; lightness 0 or below is black and
/// 1 or above white. Upstream `oklchToHex`.
fn oklch_to_hex(lightness: f64, chroma: f64, hue: f64) -> String {
    if lightness <= 0.0 {
        return "#000000".to_string();
    }
    if lightness >= 1.0 {
        return "#ffffff".to_string();
    }
    let radians = (hue * PI) / 180.0;
    let [red, green, blue] =
        oklab_to_linear_rgb(lightness, chroma * radians.cos(), chroma * radians.sin());
    rgb_to_hex(
        linear_to_srgb(js_max(0.0, red)),
        linear_to_srgb(js_max(0.0, green)),
        linear_to_srgb(js_max(0.0, blue)),
    )
}

/// The highest chroma (up to 0.4) that stays inside sRGB at an OKLCH hue
/// (degrees) and lightness, found by 20 rounds of bisection. Upstream
/// `maxOklchChroma`.
pub fn max_oklch_chroma(hue: f64, lightness: f64) -> f64 {
    let mut low = 0.0;
    let mut high = 0.4;
    for _ in 0..20 {
        let middle = (low + high) / 2.0;
        if oklch_in_gamut(lightness, middle, hue) {
            low = middle;
        } else {
            high = middle;
        }
    }
    low
}

/// An OKLCH color (hue in degrees) as a lowercase `#rrggbb`; a color outside
/// sRGB drops to the [`max_oklch_chroma`] at its hue and lightness first.
/// Upstream `oklchClampedHex`.
pub fn oklch_clamped_hex(lightness: f64, chroma: f64, hue: f64) -> String {
    if oklch_in_gamut(lightness, chroma, hue) {
        return oklch_to_hex(lightness, chroma, hue);
    }
    oklch_to_hex(lightness, max_oklch_chroma(hue, lightness), hue)
}

/// The OKLab lightness of a CIE L* tone (0 to 100): the tone's relative
/// luminance, cube-rooted. Tones at or past the ends give 0 and 1. Upstream
/// `toneToOklabLightness`.
pub fn tone_to_oklab_lightness(tone: f64) -> f64 {
    if tone <= 0.0 {
        return 0.0;
    }
    if tone >= 100.0 {
        return 1.0;
    }
    let relative = if tone > 8.0 {
        ((tone + 16.0) / 116.0).powf(3.0)
    } else {
        tone / 903.3
    };
    relative.cbrt()
}

/// A hex color's linear-light red, green and blue, each 0 to 1. Upstream
/// `hexToLinearRgb`.
pub fn hex_to_linear_rgb(hex: &str) -> ColorTriple {
    hex_to_rgb(hex).map(srgb_to_linear)
}

/// A hex color's WCAG relative luminance, 0 (black) to 1 (white). Upstream
/// `luminance`.
pub fn luminance(hex: &str) -> f64 {
    let [red, green, blue] = hex_to_rgb(hex).map(srgb_to_linear);
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

/// A hex color's red, green and blue, 0 to 255. Upstream `hexToRgbTriple`.
pub fn hex_to_rgb_triple(hex: &str) -> ColorTriple {
    hex_to_rgb(hex)
}

/// Red, green and blue (0 to 255, rounded) as a lowercase `#rrggbb`.
/// Upstream `rgbTripleToHex`.
pub fn rgb_triple_to_hex(rgb: ColorTriple) -> String {
    rgb_to_hex(js_round(rgb[0]), js_round(rgb[1]), js_round(rgb[2]))
}

/// A color drawn at `alpha` over `under`, as the opaque color the eye sees,
/// blended channel by channel in whatever space the triples are in.
/// Upstream `compositeOver` (`palette/color.ts`).
pub fn composite_over(over: ColorTriple, alpha: f64, under: ColorTriple) -> ColorTriple {
    [0, 1, 2].map(|n| over[n] * alpha + under[n] * (1.0 - alpha))
}

/// The WCAG contrast ratio of two opaque hex colors, 1 to 21. Upstream
/// `contrastRatio` (`palette/color.ts`).
pub fn contrast_ratio(a: &str, b: &str) -> f64 {
    let x = luminance(a);
    let y = luminance(b);
    (js_max(x, y) + 0.05) / (js_min(x, y) + 0.05)
}

/// A 3- or 6-digit hex color as OKLab `[L, a, b]`, by way of its OKLCH
/// coordinates. Fails as [`normalize_color`] does. Upstream
/// `oklabCoordinates`.
pub fn oklab_coordinates(hex: &str) -> Result<ColorTriple, InvalidColor> {
    let value = hex_to_oklch(hex)?;
    let radians = (value.h * PI) / 180.0;
    Ok([value.l, value.c * radians.cos(), value.c * radians.sin()])
}

/// OKLab `[L, a, b]` as a lowercase `#rrggbb`: lightness clamped to 0–1 and
/// chroma cut back into sRGB as [`oklch_clamped_hex`] does. Upstream
/// `oklabCoordinatesToHex`.
pub fn oklab_coordinates_to_hex(coordinates: ColorTriple) -> String {
    let [lightness, a, b] = coordinates;
    let chroma = (a * a + b * b).sqrt();
    let hue = ((b.atan2(a) * 180.0) / PI + 360.0) % 360.0;
    oklch_clamped_hex(js_min(1.0, js_max(0.0, lightness)), chroma, hue)
}

/// A contrast ratio to two decimals, cut down and never rounded up, so a
/// pair just under a bar never prints as meeting it (4.4999 is 4.49).
/// Upstream `ratioDown`.
pub fn ratio_down(ratio: f64) -> f64 {
    (ratio * 100.0 + 1e-9).floor() / 100.0
}

#[cfg(test)]
mod tests {
    use super::super::js::js_hypot;
    use super::*;

    fn rgba(r: f64, g: f64, b: f64, a: f64) -> Rgba {
        Rgba { r, g, b, a }
    }

    /// Agreement with a value upstream computed under Node, to 12
    /// significant digits: Rust and V8 call different `cbrt`, `pow` and
    /// `atan2` implementations, which can differ in the last bits.
    fn approx(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0)
    }

    fn assert_triple(actual: ColorTriple, expected: ColorTriple) {
        for (a, e) in actual.into_iter().zip(expected) {
            assert!(approx(a, e), "{actual:?} != {expected:?}");
        }
    }

    // utils/color.test.ts: parseHex

    #[test]
    fn parse_hex_reads_rrggbb() {
        assert_eq!(parse_hex("#0064E0"), Some(rgba(0.0, 100.0, 224.0, 1.0)));
    }

    #[test]
    fn parse_hex_reads_without_a_leading_hash() {
        assert_eq!(parse_hex("0064E0"), Some(rgba(0.0, 100.0, 224.0, 1.0)));
    }

    #[test]
    fn parse_hex_expands_rgb_shorthand() {
        assert_eq!(parse_hex("#abc"), Some(rgba(170.0, 187.0, 204.0, 1.0)));
    }

    #[test]
    fn parse_hex_reads_rrggbbaa_alpha() {
        assert_eq!(
            parse_hex("#00000080"),
            Some(rgba(0.0, 0.0, 0.0, 128.0 / 255.0))
        );
    }

    #[test]
    fn parse_hex_expands_rgba_shorthand_alpha() {
        assert_eq!(
            parse_hex("#f008"),
            Some(rgba(255.0, 0.0, 0.0, 136.0 / 255.0))
        );
    }

    #[test]
    fn parse_hex_rejects_non_hex_input() {
        // Upstream's `parseHex(null)` case has no Rust form: a `&str` is
        // never null.
        assert_eq!(parse_hex("rgb(0,0,0)"), None);
        assert_eq!(parse_hex("#12"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
    }

    // utils/color.test.ts: parseRgb

    #[test]
    fn parse_rgb_reads_comma_separated_channels() {
        assert_eq!(
            parse_rgb("rgb(0, 100, 224)"),
            Some(rgba(0.0, 100.0, 224.0, 1.0))
        );
    }

    #[test]
    fn parse_rgb_reads_rgba_alpha() {
        assert_eq!(
            parse_rgb("rgba(0, 0, 0, 0.5)"),
            Some(rgba(0.0, 0.0, 0.0, 0.5))
        );
    }

    #[test]
    fn parse_rgb_reads_space_separated_channels_with_slash_alpha() {
        assert_eq!(
            parse_rgb("rgb(0 100 224 / 0.25)"),
            Some(rgba(0.0, 100.0, 224.0, 0.25))
        );
    }

    #[test]
    fn parse_rgb_reads_percentage_channels() {
        assert_eq!(
            parse_rgb("rgb(100%, 0%, 0%)"),
            Some(rgba(255.0, 0.0, 0.0, 1.0))
        );
    }

    #[test]
    fn parse_rgb_clamps_alpha_and_channels_into_range() {
        assert_eq!(
            parse_rgb("rgba(300, -20, 10, 2)"),
            Some(rgba(255.0, 0.0, 10.0, 1.0))
        );
    }

    #[test]
    fn parse_rgb_rejects_what_it_cannot_parse() {
        assert_eq!(parse_rgb("rgb(0, 0)"), None);
        assert_eq!(parse_rgb("#000000"), None);
    }

    #[test]
    fn parse_rgb_reads_channels_as_parse_float_does() {
        // Expected values: upstream `parseRgb` run under Node 24.
        assert_eq!(
            parse_rgb("rgb(1e2, .5, +3)"),
            Some(rgba(100.0, 0.5, 3.0, 1.0))
        );
        assert_eq!(
            parse_rgb("rgb(12px, 0, 0)"),
            Some(rgba(12.0, 0.0, 0.0, 1.0))
        );
        assert_eq!(
            parse_rgb("rgb(0 0 0 / 50%)"),
            Some(rgba(0.0, 0.0, 0.0, 0.5))
        );
        assert_eq!(parse_rgb("rgb(a, 0, 0)"), None);
        assert_eq!(parse_rgb("rgb(0,0,0,x)"), None);
    }

    // utils/color.test.ts: parseColor

    #[test]
    fn parse_color_routes_hex_and_rgb_strings_to_the_right_parser() {
        assert_eq!(parse_color("#FFFFFF"), Some(rgba(255.0, 255.0, 255.0, 1.0)));
        assert_eq!(parse_color("rgb(1, 2, 3)"), Some(rgba(1.0, 2.0, 3.0, 1.0)));
    }

    #[test]
    fn parse_color_resolves_the_named_colors_token_expressions_use() {
        assert_eq!(parse_color("transparent"), Some(rgba(0.0, 0.0, 0.0, 0.0)));
        assert_eq!(parse_color("BLACK"), Some(rgba(0.0, 0.0, 0.0, 1.0)));
        assert_eq!(parse_color("white"), Some(rgba(255.0, 255.0, 255.0, 1.0)));
    }

    #[test]
    fn parse_color_returns_none_for_unsupported_colors() {
        assert_eq!(parse_color("var(--color-accent)"), None);
        assert_eq!(parse_color("oklch(0.5 0.1 200)"), None);
        assert_eq!(parse_color("rebeccapurple"), None);
    }

    #[test]
    fn parse_color_returns_none_for_names_the_prototype_chain_answers() {
        assert_eq!(parse_color("constructor"), None);
        assert_eq!(parse_color("__proto__"), None);
        assert_eq!(parse_color("CONSTRUCTOR"), None);
    }

    #[test]
    fn composed_parsers_stay_none_for_prototype_names() {
        let rgba = parse_color("constructor").or_else(|| parse_hex("constructor"));
        assert_eq!(rgba, None);
    }

    // utils/color.test.ts: formatHex

    #[test]
    fn format_hex_formats_channels_as_uppercase_rrggbb() {
        assert_eq!(format_hex(0.0, 100.0, 224.0), "#0064E0");
    }

    #[test]
    fn format_hex_rounds_and_clamps_out_of_range_channels() {
        assert_eq!(format_hex(-5.0, 300.0, 127.6), "#00FF80");
    }

    #[test]
    fn format_hex_prints_a_nan_channel_as_javascript_does() {
        // Upstream `formatHex(NaN, 0, 0)` under Node 24.
        assert_eq!(format_hex(f64::NAN, 0.0, 0.0), "#NAN0000");
    }

    // utils/color.test.ts: formatColor

    #[test]
    fn format_color_formats_opaque_colors_as_hex() {
        assert_eq!(format_color(rgba(0.0, 100.0, 224.0, 1.0)), "#0064E0");
    }

    #[test]
    fn format_color_formats_translucent_colors_as_rgba() {
        assert_eq!(
            format_color(rgba(0.0, 100.0, 224.0, 0.2)),
            "rgba(0, 100, 224, 0.2)"
        );
    }

    #[test]
    fn format_color_round_trips_through_parse_color() {
        let parsed = parse_color("rgba(10, 20, 30, 0.5)").expect("parses");
        assert_eq!(format_color(parsed), "rgba(10, 20, 30, 0.5)");
    }

    #[test]
    fn format_color_rounds_alpha_as_to_fixed_does() {
        // Expected strings: upstream `formatColor` run under Node 24.
        assert_eq!(
            format_color(rgba(1.0, 2.0, 3.0, 0.03125)),
            "rgba(1, 2, 3, 0.0313)"
        );
        assert_eq!(
            format_color(rgba(1.0, 2.0, 3.0, 0.12345)),
            "rgba(1, 2, 3, 0.1235)"
        );
        assert_eq!(
            format_color(rgba(1.0, 2.0, 3.0, -0.00001)),
            "rgba(1, 2, 3, 0)"
        );
        assert_eq!(
            format_color(rgba(1.0, 2.0, 3.0, 0.00005)),
            "rgba(1, 2, 3, 0.0001)"
        );
        assert_eq!(
            format_color(rgba(f64::NAN, 2.0, 3.0, 0.5)),
            "rgba(NaN, 2, 3, 0.5)"
        );
    }

    // utils/color.test.ts: toGLFloats

    #[test]
    fn to_gl_floats_converts_channels_to_unit_floats_dropping_alpha() {
        assert_eq!(
            to_gl_floats(Some(rgba(0.0, 100.0, 224.0, 0.5))),
            [0.0, 100.0 / 255.0, 224.0 / 255.0]
        );
    }

    #[test]
    fn to_gl_floats_returns_the_neutral_fallback_for_none() {
        assert_eq!(to_gl_floats(None), [0.5, 0.5, 0.5]);
    }

    #[test]
    fn to_gl_floats_clamps_hand_constructed_out_of_range_channels() {
        assert_eq!(
            to_gl_floats(Some(rgba(300.0, -5.0, 127.5, 1.0))),
            [1.0, 0.0, 127.5 / 255.0]
        );
    }

    #[test]
    fn to_gl_floats_returns_the_fallback_for_non_finite_channels() {
        assert_eq!(
            to_gl_floats(Some(rgba(f64::NAN, 0.0, 0.0, 1.0))),
            [0.5, 0.5, 0.5]
        );
        assert_eq!(
            to_gl_floats(Some(rgba(0.0, f64::INFINITY, 0.0, 1.0))),
            [0.5, 0.5, 0.5]
        );
    }

    #[test]
    fn to_gl_floats_hex_path_conformance_table() {
        let cases: [(&str, [f64; 3]); 11] = [
            ("#0064E0", [0.0, 100.0 / 255.0, 224.0 / 255.0]),
            ("0064E0", [0.0, 100.0 / 255.0, 224.0 / 255.0]),
            ("#f00", [1.0, 0.0, 0.0]),
            ("#f008", [1.0, 0.0, 0.0]),
            ("#00000080", [0.0, 0.0, 0.0]),
            ("  #ffffff  ", [1.0, 1.0, 1.0]),
            ("#12", [0.5, 0.5, 0.5]),
            ("not-a-color", [0.5, 0.5, 0.5]),
            ("var(--color-accent)", [0.5, 0.5, 0.5]),
            ("rgb(255, 0, 0)", [0.5, 0.5, 0.5]),
            ("", [0.5, 0.5, 0.5]),
        ];
        for (input, expected) in cases {
            assert_eq!(to_gl_floats(parse_hex(input)), expected, "{input:?}");
        }
    }

    #[test]
    fn to_gl_floats_composes_with_parse_color() {
        assert_eq!(to_gl_floats(parse_color("rgb(255, 0, 0)")), [1.0, 0.0, 0.0]);
        assert_eq!(to_gl_floats(parse_color("transparent")), [0.0, 0.0, 0.0]);
    }

    // utils/color.test.ts: alpha application (charts/lab `colors.alpha`)

    fn apply_alpha(color: &str, opacity: f64) -> String {
        match parse_color(color).or_else(|| parse_hex(color)) {
            Some(rgba) => format_color(Rgba { a: opacity, ..rgba }),
            None => color.to_string(),
        }
    }

    #[test]
    fn alpha_applies_opacity_to_an_opaque_hex_color() {
        assert_eq!(apply_alpha("#0064E0", 0.2), "rgba(0, 100, 224, 0.2)");
    }

    #[test]
    fn alpha_applies_opacity_to_bare_hex() {
        assert_eq!(apply_alpha("0064E0", 0.2), "rgba(0, 100, 224, 0.2)");
    }

    #[test]
    fn alpha_opacity_wins_over_an_existing_alpha_byte() {
        assert_eq!(apply_alpha("#0064E080", 0.2), "rgba(0, 100, 224, 0.2)");
    }

    #[test]
    fn alpha_opacity_wins_over_rgba_input_alpha() {
        assert_eq!(
            apply_alpha("rgba(0, 100, 224, 0.9)", 0.2),
            "rgba(0, 100, 224, 0.2)"
        );
    }

    #[test]
    fn alpha_full_opacity_serializes_back_to_hex() {
        assert_eq!(apply_alpha("#0064E0", 1.0), "#0064E0");
    }

    #[test]
    fn alpha_leaves_unparseable_input_unchanged() {
        assert_eq!(
            apply_alpha("var(--color-accent)", 0.2),
            "var(--color-accent)"
        );
    }

    #[test]
    fn alpha_preserves_names_the_prototype_chain_answers() {
        assert_eq!(apply_alpha("constructor", 0.2), "constructor");
        assert_eq!(apply_alpha("__proto__", 1.0), "__proto__");
    }

    // palette/color.ts has no upstream tests. Expected values come from
    // running upstream `palette/color.ts` under Node 24.

    #[test]
    fn normalize_color_expands_and_lowercases() {
        assert_eq!(normalize_color("#ABC").as_deref(), Ok("#aabbcc"));
        assert_eq!(normalize_color("abc").as_deref(), Ok("#aabbcc"));
        assert_eq!(normalize_color(" #0064E0 ").as_deref(), Ok("#0064e0"));
    }

    #[test]
    fn normalize_color_rejects_anything_but_three_or_six_digits() {
        let error = normalize_color("#12").unwrap_err();
        assert_eq!(error.to_string(), "Invalid color: #12");
        assert!(normalize_color("#0064E080").is_err());
        assert!(normalize_color("##abc").is_err());
    }

    #[test]
    fn hex_to_oklch_matches_upstream() {
        let blue = hex_to_oklch("#0064E0").unwrap();
        assert!(approx(blue.l, 0.5334360733851473), "{blue:?}");
        assert!(approx(blue.c, 0.20490477709448957), "{blue:?}");
        assert!(approx(blue.h, 258.81008101651366), "{blue:?}");
        let red = hex_to_oklch("#e3193b").unwrap();
        assert!(approx(red.l, 0.5858178908636312), "{red:?}");
        assert!(approx(red.c, 0.22584195934669912), "{red:?}");
        assert!(approx(red.h, 21.25810617959327), "{red:?}");
        assert_eq!(
            hex_to_oklch("#000"),
            Ok(OklchDegrees {
                l: 0.0,
                c: 0.0,
                h: 0.0
            })
        );
        assert!(hex_to_oklch("#0064E080").is_err());
    }

    #[test]
    fn oklab_conversions_match_upstream() {
        assert_triple(
            linear_rgb_to_oklab(1.0, 1.0, 1.0),
            [
                0.9999999934735462,
                8.095285553011422e-11,
                3.727390762708893e-8,
            ],
        );
        assert_triple(
            linear_rgb_to_oklab(0.2, 0.4, 0.6),
            [
                0.7162406167417781,
                -0.03416396201145022,
                -0.061006422612144395,
            ],
        );
        assert_triple(
            oklab_to_linear_rgb(0.5, 0.1, -0.1),
            [0.2187056162159623, 0.060342335611774614, 0.3210379262198801],
        );
        assert_triple(oklab_to_linear_rgb(1.0, 0.0, 0.0), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn max_oklch_chroma_bisects_to_the_gamut_edge() {
        assert_eq!(max_oklch_chroma(264.0, 0.5), 0.28123245239257816);
        assert_eq!(max_oklch_chroma(30.0, 0.7), 0.1917743682861328);
        assert_eq!(max_oklch_chroma(110.0, 0.95), 0.20773506164550784);
    }

    #[test]
    fn oklch_clamped_hex_cuts_chroma_into_srgb() {
        assert_eq!(oklch_clamped_hex(0.5, 0.3, 264.0), "#033fff");
        assert_eq!(oklch_clamped_hex(0.7, 0.05, 30.0), "#bc938c");
        assert_eq!(oklch_clamped_hex(0.62, 0.2, 145.0), "#00a22a");
        assert_eq!(oklch_clamped_hex(0.0, 0.1, 30.0), "#000000");
        assert_eq!(oklch_clamped_hex(1.0, 0.1, 30.0), "#ffffff");
    }

    #[test]
    fn tone_to_oklab_lightness_matches_upstream() {
        assert!(approx(tone_to_oklab_lightness(50.0), 0.5689655172413793));
        assert!(approx(tone_to_oklab_lightness(5.0), 0.17689382168242415));
        assert!(approx(tone_to_oklab_lightness(8.0), 0.20689626895186877));
        assert_eq!(tone_to_oklab_lightness(0.0), 0.0);
        assert_eq!(tone_to_oklab_lightness(-3.0), 0.0);
        assert_eq!(tone_to_oklab_lightness(100.0), 1.0);
    }

    #[test]
    fn hex_luminance_matches_upstream() {
        assert_triple(
            hex_to_linear_rgb("#808080"),
            [
                0.21586050011389926,
                0.21586050011389926,
                0.21586050011389926,
            ],
        );
        assert!(approx(luminance("#767676"), 0.18116424424986022));
        assert!(approx(luminance("#0064E0"), 0.144961612976391));
    }

    #[test]
    fn hex_to_rgb_triple_reads_hex_as_parse_int_does() {
        let cases: [(&str, ColorTriple); 9] = [
            ("#0064E0", [0.0, 100.0, 224.0]),
            ("#abc", [170.0, 187.0, 204.0]),
            ("#0064E080", [0.0, 100.0, 224.0]),
            ("zz", [0.0, 0.0, 0.0]),
            ("-ff", [0.0, 0.0, 0.0]),
            (" fff", [0.0, 15.0, 255.0]),
            ("12zz56", [0.0, 0.0, 18.0]),
            ("0x1234", [0.0, 18.0, 52.0]),
            // Three UTF-16 code units, so it doubles as shorthand.
            ("a\u{1F600}", [0.0, 0.0, 170.0]),
        ];
        for (input, expected) in cases {
            assert_eq!(hex_to_rgb_triple(input), expected, "{input:?}");
        }
    }

    #[test]
    fn rgb_triple_to_hex_rounds_and_clamps() {
        assert_eq!(rgb_triple_to_hex([0.4, 100.5, 224.0]), "#0065e0");
        assert_eq!(rgb_triple_to_hex([-3.0, 300.0, 15.49]), "#00ff0f");
    }

    #[test]
    fn composite_over_blends_triples() {
        assert_eq!(
            composite_over([0.0, 0.0, 0.0], 0.5, [255.0, 255.0, 255.0]),
            [127.5, 127.5, 127.5]
        );
    }

    #[test]
    fn hex_contrast_ratio_matches_upstream() {
        assert_eq!(contrast_ratio("#000000", "#ffffff"), 21.0);
        assert!(approx(
            contrast_ratio("#767676", "#ffffff"),
            4.542224959605253
        ));
        assert!(approx(
            contrast_ratio("#ffffff", "#0064e0"),
            5.385675590030896
        ));
    }

    #[test]
    fn oklab_coordinates_round_trip_through_hex() {
        let coordinates = oklab_coordinates("#0064e0").unwrap();
        assert_triple(
            coordinates,
            [
                0.5334360733851473,
                -0.03976418007018422,
                -0.20100939694324846,
            ],
        );
        assert_eq!(oklab_coordinates_to_hex(coordinates), "#0064e0");
        assert_eq!(oklab_coordinates_to_hex([0.6, 0.3, 0.2]), "#e53200");
        assert_eq!(oklab_coordinates_to_hex([1.2, 0.0, 0.0]), "#ffffff");
        assert!(oklab_coordinates("nope").is_err());
    }

    #[test]
    fn ratio_down_never_rounds_up() {
        assert_eq!(ratio_down(4.4999), 4.49);
        assert_eq!(ratio_down(4.5), 4.5);
        assert_eq!(ratio_down(4.499999999999), 4.5);
        assert_eq!(ratio_down(4.56789), 4.56);
        assert_eq!(ratio_down(21.0), 21.0);
    }

    // The JavaScript rules. Expected values come from Node 24.

    #[test]
    fn js_round_rounds_halves_up() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(-0.5), 0.0);
        assert_eq!(js_round(0.49999999999999994), 0.0);
    }

    #[test]
    fn js_min_and_max_propagate_nan() {
        assert!(js_max(0.0, f64::NAN).is_nan());
        assert!(js_min(f64::NAN, 1.0).is_nan());
        assert!(js_max(-0.0, 0.0).is_sign_positive());
        assert!(js_min(0.0, -0.0).is_sign_negative());
    }

    #[test]
    fn js_hypot_matches_v8() {
        // V8 normalizes by the larger argument and Kahan-sums the squares.
        assert_eq!(js_hypot(3.0, 4.0), 5.0);
        assert_eq!(js_hypot(0.0, -0.0), 0.0);
        assert!(js_hypot(f64::NAN, 1.0).is_nan());
        assert_eq!(js_hypot(f64::NAN, f64::INFINITY), f64::INFINITY);
    }

    #[test]
    fn js_parse_float_reads_the_longest_number_prefix() {
        let cases: [(&str, f64); 12] = [
            ("12px", 12.0),
            (".5", 0.5),
            ("-Infinity", f64::NEG_INFINITY),
            ("Infinityx", f64::INFINITY),
            ("1e", 1.0),
            ("1e+5x", 100000.0),
            (" \u{FEFF}3", 3.0),
            ("+.5e-1", 0.05),
            ("1.", 1.0),
            ("  -12.5e-1%", -1.25),
            ("0x10", 0.0),
            ("100%", 100.0),
        ];
        for (input, expected) in cases {
            assert_eq!(js_parse_float(input), expected, "{input:?}");
        }
        for input in ["px", ".", "-.e1", ""] {
            assert!(js_parse_float(input).is_nan(), "{input:?}");
        }
    }

    #[test]
    fn js_to_fixed_rounds_exact_ties_up() {
        let cases: [(f64, usize, &str); 12] = [
            (0.5, 0, "1"),
            (2.5, 0, "3"),
            (-2.5, 0, "-3"),
            (-1.5, 0, "-2"),
            (9.5, 0, "10"),
            (99.5, 0, "100"),
            (1.005, 2, "1.00"),
            (0.995, 2, "0.99"),
            (0.125, 2, "0.13"),
            (0.03125, 4, "0.0313"),
            (-0.0, 2, "0.00"),
            (1e21, 2, "1e+21"),
        ];
        for (x, digits, expected) in cases {
            assert_eq!(js_to_fixed(x, digits), expected, "{x}.toFixed({digits})");
        }
    }

    #[test]
    fn js_number_to_string_prints_as_javascript_does() {
        let cases: [(f64, &str); 11] = [
            (100.0, "100"),
            (123.456, "123.456"),
            (-0.0, "0"),
            (0.1 + 0.2, "0.30000000000000004"),
            (0.000001, "0.000001"),
            (1e-7, "1e-7"),
            (1.5e-7, "1.5e-7"),
            (1e21, "1e+21"),
            (123456789012345680000.0, "123456789012345680000"),
            // Exact ties between two shortest candidates go to the even one.
            (869631767272949.2, "869631767272949.2"),
            (-0.018739700317382812, "-0.018739700317382812"),
        ];
        for (x, expected) in cases {
            assert_eq!(js_number_to_string(x), expected);
        }
        assert_eq!(js_number_to_string(f64::NAN), "NaN");
        assert_eq!(js_number_to_string(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn js_to_hex_string_prints_as_javascript_does() {
        assert_eq!(js_to_hex_string(255.0), "ff");
        assert_eq!(js_to_hex_string(510.0), "1fe");
        assert_eq!(js_to_hex_string(-127.0), "-7f");
        assert_eq!(js_to_hex_string(2f64.powi(64)), "10000000000000000");
        assert_eq!(js_to_hex_string(1e30), "c9f2c9cd04675000000000000");
        assert_eq!(js_to_hex_string(f64::NAN), "NaN");
    }
}
