//! The HCT (hue, chroma, tone) color model for theme authoring: upstream
//! `theme/hct.ts`.
//!
//! Upstream's HCT is its own minimal take on Material's, not the CAM16 one in
//! material-color-utilities: hue and chroma are CIELAB's (the angle and
//! length of a*b*) and tone is CIE L* (0 black, 100 white), all under the D65
//! white point. [`hct_to_hex`] goes back to sRGB by bisecting chroma down
//! until the color fits.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use super::color::{format_hex, linear_to_srgb, parse_hex, srgb_to_linear};
use super::js::{js_max, js_min, js_round, js_to_hex_string};

/// A color as hue, chroma and tone. Upstream `HCT`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hct {
    /// CIELAB hue angle in degrees, 0 to 360.
    pub hue: f64,
    /// CIELAB chroma (C*ab), 0 to about 130 in sRGB.
    pub chroma: f64,
    /// CIE L* lightness, 0 (black) to 100 (white).
    pub tone: f64,
}

/// Linear sRGB as CIE XYZ under D65. Upstream `linearRgbToXyz`.
fn linear_rgb_to_xyz(r: f64, g: f64, b: f64) -> [f64; 3] {
    [
        0.4124564 * r + 0.3575761 * g + 0.1804375 * b,
        0.2126729 * r + 0.7151522 * g + 0.072175 * b,
        0.0193339 * r + 0.119192 * g + 0.9503041 * b,
    ]
}

/// CIE XYZ under D65 as linear sRGB, unclamped. Upstream `xyzToLinearRgb`.
fn xyz_to_linear_rgb(x: f64, y: f64, z: f64) -> [f64; 3] {
    [
        3.2404542 * x - 1.5371385 * y - 0.4985314 * z,
        -0.969266 * x + 1.8760108 * y + 0.041556 * z,
        0.0556434 * x - 0.2040259 * y + 1.0572252 * z,
    ]
}

/// The D65 reference white in XYZ. Upstream `D65_WHITE`.
const D65_WHITE: [f64; 3] = [0.95047, 1.0, 1.08883];

/// CIELAB's companding function. Upstream `labF`.
fn lab_f(t: f64) -> f64 {
    let delta = 6.0 / 29.0;
    if t > delta * delta * delta {
        t.cbrt()
    } else {
        t / (3.0 * delta * delta) + 4.0 / 29.0
    }
}

/// The inverse of [`lab_f`]. Upstream `labFInv`.
fn lab_f_inv(t: f64) -> f64 {
    let delta = 6.0 / 29.0;
    if t > delta {
        t * t * t
    } else {
        3.0 * delta * delta * (t - 4.0 / 29.0)
    }
}

/// CIE XYZ as CIELAB `[L*, a*, b*]`. Upstream `xyzToLab`.
fn xyz_to_lab(x: f64, y: f64, z: f64) -> [f64; 3] {
    let fx = lab_f(x / D65_WHITE[0]);
    let fy = lab_f(y / D65_WHITE[1]);
    let fz = lab_f(z / D65_WHITE[2]);
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIELAB as CIE XYZ. Upstream `labToXyz`.
fn lab_to_xyz(l: f64, a: f64, b: f64) -> [f64; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;
    [
        lab_f_inv(fx) * D65_WHITE[0],
        lab_f_inv(fy) * D65_WHITE[1],
        lab_f_inv(fz) * D65_WHITE[2],
    ]
}

/// A tone's CIE luminance Y, 0 to 1. Upstream `toneToY`.
fn tone_to_y(tone: f64) -> f64 {
    lab_f_inv((tone + 16.0) / 116.0)
}

/// A hex color's red, green and blue; black when it doesn't parse. Upstream
/// `hexToRgb` (`theme/hct.ts`).
fn hex_to_rgb(hex: &str) -> [f64; 3] {
    match parse_hex(hex) {
        Some(parsed) => [parsed.r, parsed.g, parsed.b],
        None => [0.0, 0.0, 0.0],
    }
}

/// A hex color as HCT: hue 0 to 360, chroma 0 to about 130, tone 0 to 100.
/// A string that isn't a hex color reads as black. Upstream `hexToHct`.
pub fn hex_to_hct(hex: &str) -> Hct {
    let [r, g, b] = hex_to_rgb(hex);
    let lr = srgb_to_linear(r);
    let lg = srgb_to_linear(g);
    let lb = srgb_to_linear(b);
    let [x, y, z] = linear_rgb_to_xyz(lr, lg, lb);
    let [l, a, b_lab] = xyz_to_lab(x, y, z);

    let hue_rad = b_lab.atan2(a);
    let mut hue = (hue_rad * 180.0) / PI;
    if hue < 0.0 {
        hue += 360.0;
    }

    let chroma = (a * a + b_lab * b_lab).sqrt();
    let tone = l;

    Hct {
        hue,
        chroma,
        tone: js_max(0.0, js_min(100.0, tone)),
    }
}

/// An HCT color as an uppercase `#RRGGBB`. Tone 0 or below is black, 100 or
/// above white, and chroma under 0.5 a grey of the tone; otherwise 16 rounds
/// of bisection find the highest chroma, up to the one asked for, that lands
/// in sRGB at the hue and tone. Upstream `hctToHex`.
pub fn hct_to_hex(hct: Hct) -> String {
    let Hct { hue, chroma, tone } = hct;

    if tone <= 0.0 {
        return "#000000".to_string();
    }
    if tone >= 100.0 {
        return "#FFFFFF".to_string();
    }
    if chroma < 0.5 {
        let gray = tone_to_gray(tone);
        return format_hex(gray, gray, gray);
    }

    let mut lo = 0.0;
    let mut hi = chroma;
    let mut best_hex = "#000000".to_string();

    for _ in 0..16 {
        let mid = (lo + hi) / 2.0;
        match hct_component_to_hex(hue, mid, tone) {
            Some(candidate) => {
                best_hex = candidate;
                lo = mid;
            }
            None => hi = mid,
        }
    }

    best_hex
}

/// The sRGB channel of the grey at a tone. Upstream `toneToGray`.
fn tone_to_gray(tone: f64) -> f64 {
    let y = tone_to_y(tone);
    linear_to_srgb(y)
}

/// An HCT color as an uppercase `#RRGGBB` when it lands in sRGB: no linear
/// channel moves more than 0.02 when rounded to a byte. Upstream
/// `hctComponentToHex`.
fn hct_component_to_hex(hue: f64, chroma: f64, tone: f64) -> Option<String> {
    let hue_rad = (hue * PI) / 180.0;
    let a = hue_rad.cos() * chroma;
    let b = hue_rad.sin() * chroma;
    let l = tone;

    let [x, y, z] = lab_to_xyz(l, a, b);
    let [lr, lg, lb] = xyz_to_linear_rgb(x, y, z);

    let r = linear_to_srgb(lr);
    let g = linear_to_srgb(lg);
    let b_val = linear_to_srgb(lb);

    let r_lin = srgb_to_linear(r);
    let g_lin = srgb_to_linear(g);
    let b_lin = srgb_to_linear(b_val);

    let tolerance = 0.02;
    if (r_lin - lr).abs() > tolerance
        || (g_lin - lg).abs() > tolerance
        || (b_lin - lb).abs() > tolerance
    {
        return None;
    }

    // Upstream's comparisons, kept so a NaN channel passes as it does there
    // (`!range.contains` would reject it).
    #[allow(clippy::manual_range_contains)]
    if r < 0.0 || r > 255.0 || g < 0.0 || g > 255.0 || b_val < 0.0 || b_val > 255.0 {
        return None;
    }

    Some(format_hex(r, g, b_val))
}

/// The tones [`tonal_palette`] fills. Upstream `PALETTE_TONES`.
const PALETTE_TONES: [u32; 14] = [0, 5, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 99, 100];

/// A tonal palette: [`hct_to_hex`] of a hue and chroma at the standard tones
/// 0, 5, 10, 20, …, 90, 95, 99 and 100, keyed by tone. Upstream
/// `tonalPalette`.
pub fn tonal_palette(hue: f64, chroma: f64) -> BTreeMap<u32, String> {
    PALETTE_TONES
        .into_iter()
        .map(|tone| {
            let hex = hct_to_hex(Hct {
                hue,
                chroma,
                tone: f64::from(tone),
            });
            (tone, hex)
        })
        .collect()
}

/// A hex color with an alpha (0 to 1) appended as two uppercase hex digits
/// (`hex_with_alpha("#FF0000", 0.5)` is `"#FF000080"`). Upstream
/// `hexWithAlpha`.
pub fn hex_with_alpha(hex: &str, alpha: f64) -> String {
    let alpha_hex = format!("{:0>2}", js_to_hex_string(js_round(alpha * 255.0))).to_uppercase();
    format!("{hex}{alpha_hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_to_rgb(hex: &str) -> [i32; 3] {
        let h = hex.trim_start_matches('#');
        let byte = |at: usize| i32::from_str_radix(&h[at..at + 2], 16).unwrap();
        [byte(0), byte(2), byte(4)]
    }

    /// Agreement with a value upstream computed under Node 24, to 12
    /// significant digits (Rust and V8 use different `cbrt`, `pow` and
    /// `atan2` implementations, which can differ in the last bits).
    fn approx(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0)
    }

    const ROUNDTRIP: [&str; 7] = [
        "#FF0000", "#00FF00", "#0000FF", "#000000", "#FFFFFF", "#808080", "#0064E0",
    ];

    // theme/hct.test.ts: hexToHct / hctToHex roundtrip

    #[test]
    fn hex_to_hct_and_back_roundtrips_within_one_per_channel() {
        for hex in ROUNDTRIP {
            let result = hct_to_hex(hex_to_hct(hex));
            let [r1, g1, b1] = hex_to_rgb(hex);
            let [r2, g2, b2] = hex_to_rgb(&result);
            assert!((r1 - r2).abs() <= 1, "{hex} -> {result}");
            assert!((g1 - g2).abs() <= 1, "{hex} -> {result}");
            assert!((b1 - b2).abs() <= 1, "{hex} -> {result}");
        }
    }

    #[test]
    fn hex_to_hct_and_back_returns_the_same_hex() {
        // Upstream returns each input unchanged under Node 24.
        for hex in ROUNDTRIP {
            assert_eq!(hct_to_hex(hex_to_hct(hex)), hex);
        }
    }

    // theme/hct.test.ts: hexToHct ranges

    #[test]
    fn hex_to_hct_produces_valid_ranges() {
        for hex in ["#FF0000", "#00FF00", "#0000FF", "#808080", "#0064E0"] {
            let Hct { hue, chroma, tone } = hex_to_hct(hex);
            assert!((0.0..=360.0).contains(&hue), "{hex}");
            assert!(chroma >= 0.0, "{hex}");
            assert!((0.0..=100.0).contains(&tone), "{hex}");
        }
    }

    #[test]
    fn black_has_tone_near_0() {
        assert!(hex_to_hct("#000000").tone.abs() < 0.5);
    }

    #[test]
    fn white_has_tone_near_100() {
        assert!((hex_to_hct("#FFFFFF").tone - 100.0).abs() < 0.5);
    }

    #[test]
    fn hex_to_hct_matches_upstream() {
        let cases: [(&str, [f64; 3]); 4] = [
            (
                "#FF0000",
                [39.99901061253291, 104.55176567686988, 53.240794141307205],
            ),
            (
                "#00FF00",
                [136.01595303206318, 119.77587390168695, 87.73472235279792],
            ),
            (
                "#0000FF",
                [306.2849380699878, 133.80761485376166, 32.297010932850725],
            ),
            (
                "#0064E0",
                [289.44515257548323, 73.67426145904015, 44.9327760882435],
            ),
        ];
        for (hex, [hue, chroma, tone]) in cases {
            let hct = hex_to_hct(hex);
            assert!(approx(hct.hue, hue), "{hex}: {hct:?}");
            assert!(approx(hct.chroma, chroma), "{hex}: {hct:?}");
            assert!(approx(hct.tone, tone), "{hex}: {hct:?}");
        }
        assert_eq!(
            hex_to_hct("#000000"),
            Hct {
                hue: 0.0,
                chroma: 0.0,
                tone: 0.0
            }
        );
    }

    #[test]
    fn hex_to_hct_reads_unparseable_input_as_black() {
        assert_eq!(
            hex_to_hct("nope"),
            Hct {
                hue: 0.0,
                chroma: 0.0,
                tone: 0.0
            }
        );
    }

    #[test]
    fn hct_to_hex_matches_upstream() {
        // Upstream `hctToHex` under Node 24.
        let cases: [([f64; 3], &str); 7] = [
            ([220.0, 48.0, 40.0], "#006A7D"),
            ([30.0, 120.0, 60.0], "#FF5151"),
            ([0.0, 0.3, 50.0], "#777777"),
            ([300.0, 80.0, 95.0], "#F3EEFF"),
            ([10.0, 50.0, 0.0], "#000000"),
            ([10.0, 50.0, 100.0], "#FFFFFF"),
            ([10.0, 50.0, -4.0], "#000000"),
        ];
        for ([hue, chroma, tone], expected) in cases {
            assert_eq!(
                hct_to_hex(Hct { hue, chroma, tone }),
                expected,
                "{hue} {chroma} {tone}"
            );
        }
    }

    // theme/hct.test.ts: tonalPalette

    #[test]
    fn tonal_palette_produces_entries_for_all_standard_tones() {
        let palette = tonal_palette(220.0, 48.0);
        for tone in [0, 5, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 99, 100] {
            let hex = palette.get(&tone).expect("tone present");
            assert_eq!(hex.len(), 7, "{hex}");
            assert!(hex.starts_with('#'), "{hex}");
            assert!(
                hex[1..]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b)),
                "{hex}"
            );
        }
    }

    #[test]
    fn tonal_palette_matches_upstream() {
        // Upstream `tonalPalette(220, 48)` under Node 24.
        let expected = [
            (0, "#000000"),
            (5, "#001C29"),
            (10, "#002436"),
            (20, "#003949"),
            (30, "#005061"),
            (40, "#006A7D"),
            (50, "#00849A"),
            (60, "#00A0B9"),
            (70, "#00BCD9"),
            (80, "#00DAFB"),
            (90, "#9FEFFF"),
            (95, "#D0F7FF"),
            (99, "#F3FEFF"),
            (100, "#FFFFFF"),
        ];
        let palette = tonal_palette(220.0, 48.0);
        let actual: Vec<(u32, &str)> = palette.iter().map(|(k, v)| (*k, v.as_str())).collect();
        assert_eq!(actual, expected);
    }

    // theme/hct.test.ts: hexWithAlpha

    #[test]
    fn hex_with_alpha_appends_ff_for_1() {
        assert_eq!(hex_with_alpha("#FF0000", 1.0), "#FF0000FF");
    }

    #[test]
    fn hex_with_alpha_appends_80_for_0_5() {
        assert_eq!(hex_with_alpha("#FF0000", 0.5), "#FF000080");
    }

    #[test]
    fn hex_with_alpha_appends_00_for_0() {
        assert_eq!(hex_with_alpha("#FF0000", 0.0), "#FF000000");
    }

    #[test]
    fn hex_with_alpha_appends_33_for_0_2() {
        assert_eq!(hex_with_alpha("#ABCDEF", 0.2), "#ABCDEF33");
    }

    #[test]
    fn hex_with_alpha_does_not_clamp() {
        // Upstream under Node 24: no clamping, so out-of-range alpha prints
        // as JavaScript prints the number.
        assert_eq!(hex_with_alpha("#FF0000", 2.0), "#FF00001FE");
        assert_eq!(hex_with_alpha("#FF0000", -0.5), "#FF0000-7F");
    }
}
