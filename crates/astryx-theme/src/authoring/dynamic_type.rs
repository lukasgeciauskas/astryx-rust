//! The reader's text size categories and the font-size scales they give: upstream
//! `theme/dynamicType.ts`.
//!
//! On iOS the reader picks a text size in Settings, and each UIKit text style follows
//! it on its own curve ([`super::uikit_typography`]). Upstream reads the category from
//! a hidden `font: -apple-system-body` probe in Mobile Safari, and elsewhere gives every
//! scale `env(preferred-text-scale)`. Each theme writes a font size as its own value
//! times the scale of the text style it plays, so only sizes move, and every scale is 1
//! at the default category, Large.
//!
//! The CSS half is ported as upstream writes it: [`scaled_font_size_entries`] and
//! [`reader_text_size_entries`] give the declarations a theme block emits, and
//! [`dynamic_type_scale_rule`] the root rule that sets a category's scales. GPUI has no
//! stylesheet, so [`reader_text_size_tokens`] computes the same sizes on a theme's
//! typed tokens, as a browser would for those declarations, and
//! [`crate::ResolvedTheme`] applies it at the app's category.
//!
//! The category is the app's to set ([`set_text_size_category`]); it starts at Large.
//! On Windows, until the app sets one, it follows the system's text size
//! (`UISettings::TextScaleFactor`), mapped to the category whose Body scale is
//! nearest ([`text_size_category_for_scale`]) and read again when a window gains focus
//! or on [`refresh_text_size_category`]. Upstream's DOM probe and adopted stylesheet
//! (`syncReaderTextSize`) have no counterpart here.

use gpui::App;

use super::TOKEN_DEFAULTS;
use super::define::CssMap;
use super::js::{is_js_whitespace, js_number_to_string, js_to_fixed, js_trim};
use super::uikit_typography::{PointSizes, TYPOGRAPHY};
use crate::generated::tokens::Tokens;
use crate::types::{AbsoluteLength, TokenValue, px, rems};

// =============================================================================
// Categories
// =============================================================================

/// One of the reader's text size categories (upstream `TextSizeCategory`), smallest
/// to largest, in the order of the UIKit typography table: the seven standard sizes,
/// then the five accessibility sizes Larger Accessibility Sizes unlocks. Large is the
/// default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextSizeCategory {
    /// xSmall.
    Xs,
    /// Small.
    S,
    /// Medium.
    M,
    /// Large, the default: every scale is 1.
    #[default]
    L,
    /// xLarge.
    Xl,
    /// xxLarge.
    Xxl,
    /// xxxLarge.
    Xxxl,
    /// Accessibility Medium.
    Ax1,
    /// Accessibility Large.
    Ax2,
    /// Accessibility xLarge.
    Ax3,
    /// Accessibility xxLarge.
    Ax4,
    /// Accessibility xxxLarge.
    Ax5,
}

/// The reader's text size categories, smallest to largest (upstream
/// `TEXT_SIZE_CATEGORIES`).
pub const TEXT_SIZE_CATEGORIES: [TextSizeCategory; 12] = [
    TextSizeCategory::Xs,
    TextSizeCategory::S,
    TextSizeCategory::M,
    TextSizeCategory::L,
    TextSizeCategory::Xl,
    TextSizeCategory::Xxl,
    TextSizeCategory::Xxxl,
    TextSizeCategory::Ax1,
    TextSizeCategory::Ax2,
    TextSizeCategory::Ax3,
    TextSizeCategory::Ax4,
    TextSizeCategory::Ax5,
];

impl TextSizeCategory {
    /// Upstream's name: `xs`, `s`, `m`, `l`, `xl`, `xxl`, `xxxl`, `ax1` to `ax5`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xs => "xs",
            Self::S => "s",
            Self::M => "m",
            Self::L => "l",
            Self::Xl => "xl",
            Self::Xxl => "xxl",
            Self::Xxxl => "xxxl",
            Self::Ax1 => "ax1",
            Self::Ax2 => "ax2",
            Self::Ax3 => "ax3",
            Self::Ax4 => "ax4",
            Self::Ax5 => "ax5",
        }
    }

    /// The category upstream names `name`, or `None` for a name it does not list.
    pub fn from_name(name: &str) -> Option<Self> {
        TEXT_SIZE_CATEGORIES
            .into_iter()
            .find(|category| category.as_str() == name)
    }

    /// The category's column in the UIKit table (upstream
    /// `TEXT_SIZE_CATEGORIES.indexOf(category)`).
    pub fn column(self) -> usize {
        self as usize
    }
}

/// Index of Large, the default category, in every table row (upstream `LARGE`).
const LARGE: usize = 3;

/// The category whose Body size this is, or `None` when no category has it (a size
/// Safari does not produce, so nothing can be inferred from it). Body has a distinct
/// size in every category, so it names the category alone. Upstream
/// `textSizeCategoryForBodySize`.
pub fn text_size_category_for_body_size(px: f64) -> Option<TextSizeCategory> {
    column_for_body_size(px).map(|column| TEXT_SIZE_CATEGORIES[column])
}

/// The table column whose Body size this is (upstream `columnForBodySize`, which
/// returns -1 for none).
fn column_for_body_size(px: f64) -> Option<usize> {
    TYPOGRAPHY
        .body
        .iter()
        .position(|size| (size - px).abs() < 0.5)
}

/// The category whose Body scale is nearest `scale`, the earlier one on a tie: how a
/// platform's single text scale (Windows' `TextScaleFactor`) names a category.
/// Upstream has no such reading; it gives every scale the engine's
/// `env(preferred-text-scale)` instead.
pub fn text_size_category_for_scale(scale: f64) -> TextSizeCategory {
    let body = &TYPOGRAPHY.body;
    let mut nearest = TextSizeCategory::L;
    let mut distance = f64::INFINITY;
    for category in TEXT_SIZE_CATEGORIES {
        let off = (body[category.column()] / body[LARGE] - scale).abs();
        if off < distance {
            nearest = category;
            distance = off;
        }
    }
    nearest
}

// =============================================================================
// Curves
// =============================================================================

/// A UIKit text style the theme's sizes can scale on (upstream `DynamicTypeStyle`, a
/// key of `typography`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DynamicTypeStyle {
    LargeTitle,
    Title1,
    Title2,
    Title3,
    Body,
    Footnote,
    Caption1,
    Caption2,
}

impl DynamicTypeStyle {
    /// Every style, in the order of the UIKit table (`Object.keys(typography)`).
    pub const ALL: [Self; 8] = [
        Self::LargeTitle,
        Self::Title1,
        Self::Title2,
        Self::Title3,
        Self::Body,
        Self::Footnote,
        Self::Caption1,
        Self::Caption2,
    ];

    /// Upstream's key: `largeTitle`, `title1`, …, `caption2`.
    pub fn key(self) -> &'static str {
        match self {
            Self::LargeTitle => "largeTitle",
            Self::Title1 => "title1",
            Self::Title2 => "title2",
            Self::Title3 => "title3",
            Self::Body => "body",
            Self::Footnote => "footnote",
            Self::Caption1 => "caption1",
            Self::Caption2 => "caption2",
        }
    }

    /// The style's point sizes per category (upstream `typography[style]`).
    pub fn row(self) -> &'static PointSizes {
        match self {
            Self::LargeTitle => &TYPOGRAPHY.large_title,
            Self::Title1 => &TYPOGRAPHY.title1,
            Self::Title2 => &TYPOGRAPHY.title2,
            Self::Title3 => &TYPOGRAPHY.title3,
            Self::Body => &TYPOGRAPHY.body,
            Self::Footnote => &TYPOGRAPHY.footnote,
            Self::Caption1 => &TYPOGRAPHY.caption1,
            Self::Caption2 => &TYPOGRAPHY.caption2,
        }
    }
}

/// The UIKit text style each raw font-size step scales like (upstream
/// `FONT_SIZE_STEP_STYLES`), smallest step first. A step follows the style of the
/// roles built on it: the base step carries body, label, code and heading 4 (Body);
/// one step up, heading 3 and large text (Title 3); then heading 2 (Title 2), heading 1
/// and display 3 (Title 1), display 2 and 1 (Large Title); one step down, supporting
/// text and heading 5 (Footnote, supporting text being the common one); heading 6
/// (Caption 1); the sub-scale steps (Caption 2).
pub const FONT_SIZE_STEP_STYLES: [(&str, DynamicTypeStyle); 12] = [
    ("--font-size-4xs", DynamicTypeStyle::Caption2),
    ("--font-size-3xs", DynamicTypeStyle::Caption2),
    ("--font-size-2xs", DynamicTypeStyle::Caption2),
    ("--font-size-xs", DynamicTypeStyle::Caption1),
    ("--font-size-sm", DynamicTypeStyle::Footnote),
    ("--font-size-base", DynamicTypeStyle::Body),
    ("--font-size-lg", DynamicTypeStyle::Title3),
    ("--font-size-xl", DynamicTypeStyle::Title2),
    ("--font-size-2xl", DynamicTypeStyle::Title1),
    ("--font-size-3xl", DynamicTypeStyle::Title1),
    ("--font-size-4xl", DynamicTypeStyle::LargeTitle),
    ("--font-size-5xl", DynamicTypeStyle::LargeTitle),
];

/// The style the raw font-size step `step` scales like, or `None` for a name that is
/// no step (upstream `FONT_SIZE_STEP_STYLES[step]`).
pub fn font_size_step_style(step: &str) -> Option<DynamicTypeStyle> {
    FONT_SIZE_STEP_STYLES
        .iter()
        .find(|(name, _)| *name == step)
        .map(|(_, style)| *style)
}

/// How much `style` grows (or shrinks) at `category`, relative to Large. Upstream
/// `dynamicTypeScale`.
pub fn dynamic_type_scale(style: DynamicTypeStyle, category: TextSizeCategory) -> f64 {
    let row = style.row();
    row[category.column()] / row[LARGE]
}

/// The custom property that carries `style`'s scale (`--_dynamic-type-large-title`).
/// Upstream's root Theme sets it on `:root`. Upstream `dynamicTypeScaleProperty`.
pub fn dynamic_type_scale_property(style: DynamicTypeStyle) -> String {
    let mut name = String::from("--_dynamic-type-");
    for c in style.key().chars() {
        if c.is_ascii_uppercase() {
            name.push('-');
            name.push(c.to_ascii_lowercase());
        } else {
            name.push(c);
        }
    }
    name
}

/// The styles some step scales on, in table order (upstream `SCALED_STYLES`).
fn scaled_styles() -> impl Iterator<Item = DynamicTypeStyle> {
    DynamicTypeStyle::ALL.into_iter().filter(|style| {
        FONT_SIZE_STEP_STYLES
            .iter()
            .any(|(_, stepped)| stepped == style)
    })
}

// =============================================================================
// CSS
// =============================================================================

/// A scale written to four decimals, without trailing zeros (upstream `formatScale`:
/// `String(Number(scale.toFixed(4)))`).
fn format_scale(scale: f64) -> String {
    let fixed = js_to_fixed(scale, 4);
    js_number_to_string(fixed.parse::<f64>().unwrap_or(f64::NAN))
}

/// The rule that gives the page a category's scales, for upstream's root Theme
/// stylesheet: one custom property per scaled style on `:root`. Large, and a size that
/// names no category (`None`), need none: every scaled declaration falls back to 1.
/// Upstream `dynamicTypeScaleRule`.
pub fn dynamic_type_scale_rule(category: Option<TextSizeCategory>) -> String {
    scale_rule_for_column(category.map(TextSizeCategory::column))
}

/// The rule for one table column (upstream `scaleRuleForColumn`, `None` for -1).
/// Large and `None` get none. It also scales Core's default steps in the base layer,
/// below every theme, so a page or theme with no type scale follows too.
fn scale_rule_for_column(column: Option<usize>) -> String {
    match column {
        None | Some(LARGE) => String::new(),
        Some(column) => scale_rule(|style| {
            let row = style.row();
            format_scale(row[column] / row[LARGE])
        }),
    }
}

/// The value every scale takes where the engine reports one text scale (upstream
/// `PREFERRED_TEXT_SCALE`).
pub const PREFERRED_TEXT_SCALE: &str = "env(preferred-text-scale, 1)";

/// The rule that gives every scaled style the engine's one text scale (upstream
/// `preferredTextScaleRule`). The system font scale on Android grows all text by the
/// same factor.
pub fn preferred_text_scale_rule() -> String {
    scale_rule(|_| PREFERRED_TEXT_SCALE.to_string())
}

/// Core's raw font-size steps at their defaults (upstream `textSizeDefaults`).
fn text_size_defaults() -> CssMap {
    TOKEN_DEFAULTS
        .iter()
        .filter(|(name, _)| font_size_step_style(name).is_some())
        .map(|(name, value)| (*name, value.to_string()))
        .collect()
}

/// The root rule: Core's default steps scaled in the base layer, then each scaled
/// style's scale on `:root` (upstream `scaleRule`).
fn scale_rule(scale_of: impl Fn(DynamicTypeStyle) -> String) -> String {
    let declarations = |entries: Vec<(String, String)>| {
        entries
            .iter()
            .map(|(name, value)| format!("  {name}: {value};"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let steps = declarations(scaled_font_size_entries(&text_size_defaults()));
    let scales = declarations(
        scaled_styles()
            .map(|style| (dynamic_type_scale_property(style), scale_of(style)))
            .collect(),
    );
    format!("@layer astryx-base {{\n:root {{\n{steps}\n}}\n}}\n:root {{\n{scales}\n}}")
}

/// The private custom property holding a step's scaled size
/// (`--_scaled-font-size-base`). Only this module writes it, and only from a fixed
/// length, so the cap that reads the next step's can never read back a variable a
/// theme wrote: a theme that sets `--font-size-lg` to `var(--font-size-base)`, at its
/// root or in any adaptation rule, closes no reference cycle. Upstream
/// `scaledStepProperty`.
pub fn scaled_step_property(step: &str) -> String {
    format!("--_scaled-{}", step.get(2..).unwrap_or_default())
}

/// The raw font-size writes a theme scales (upstream `scaledFontSizeEntries`): for each
/// step among `tokens` set to a fixed length ([`is_fixed_size`]), its private scaled
/// size (the value times its curve's scale, 1 when no category is set) and the step
/// itself reading it, as `(token, value)` pairs. A step written as a variable or
/// relative to its parent is left as written: it follows what it reads, so it never
/// scales twice.
///
/// A step never outgrows the step above it: each is capped at the next step's scaled
/// size. When the theme's own step is no larger than its next one, as in every ramp
/// `typography.scale` generates, that cap is the theme's own size at Large and binds at
/// every other category; for a ramp that is out of order, or a next step this block
/// does not set (it keeps the scaled size it inherits), the cap never goes below the
/// step's own size, so the theme's sizes at Large stay exactly its own. A next step
/// the block writes as a variable caps nothing.
pub fn scaled_font_size_entries(tokens: &CssMap) -> Vec<(String, String)> {
    let fixed_value = |name: Option<&str>| {
        let raw = name
            .and_then(|name| tokens.get(name))
            .map_or("", String::as_str);
        let (value, important) = split_important(raw);
        is_fixed_size(value).then_some((value, important))
    };
    let mut out = Vec::new();
    for (index, (name, style)) in FONT_SIZE_STEP_STYLES.iter().enumerate() {
        let Some((value, important)) = fixed_value(Some(name)) else {
            continue;
        };
        let scaled = format!(
            "calc({value} * var({}, 1))",
            dynamic_type_scale_property(*style)
        );
        let next = FONT_SIZE_STEP_STYLES.get(index + 1).map(|(next, _)| *next);
        let next_own = fixed_value(next);
        let mut expression = scaled.clone();
        if let Some(next) = next
            && (tokens.get(next).is_none() || next_own.is_some())
        {
            let own_px = length_in_px(Some(value));
            let next_px = next_own.and_then(|(next_value, _)| length_in_px(Some(next_value)));
            let next_property = scaled_step_property(next);
            expression = match (own_px, next_px) {
                (Some(own), Some(next)) if own <= next => {
                    format!("min({scaled}, var({next_property}))")
                }
                _ => format!("min({scaled}, max(var({next_property}, {value}), {value}))"),
            };
        }
        let property = scaled_step_property(name);
        out.push((property.clone(), expression));
        out.push((name.to_string(), format!("var({property}){important}")));
    }
    out
}

/// The raw step each semantic font size reads in the core defaults
/// (`--text-supporting-size` reads `--font-size-sm`), in their order (upstream
/// `TEXT_SIZE_STEPS`, from `typeScaleDefaults`, which [`TOKEN_DEFAULTS`] lists).
pub fn text_size_steps() -> Vec<(&'static str, &'static str)> {
    TOKEN_DEFAULTS
        .iter()
        .filter_map(|(name, value)| {
            let step = value.strip_prefix("var(")?.strip_suffix(')')?;
            let is_step_name = step.strip_prefix("--font-size-").is_some_and(|rest| {
                !rest.is_empty()
                    && rest
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            });
            (name.ends_with("-size") && is_step_name && font_size_step_style(step).is_some())
                .then_some((*name, step))
        })
        .collect()
}

/// Whether a size stays where the theme put it (upstream `isFixedSize`). That is a
/// length, such as `12px`, `0.75rem` or a `clamp()` of them, that reads no variable.
/// Sizes in `em` or `%` follow the parent.
pub fn is_fixed_size(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    // `/\d[a-z]/i`: a digit followed by a letter.
    let has_length = bytes
        .windows(2)
        .any(|pair| pair[0].is_ascii_digit() && pair[1].is_ascii_lowercase());
    // `/var\(|%|\d(?:em|ex|ch|ic|cap|lh)\b/i`.
    let is_word = |byte: Option<&u8>| byte.is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
    let relative = bytes.iter().enumerate().any(|(at, byte)| {
        byte.is_ascii_digit()
            && ["em", "ex", "ch", "ic", "cap", "lh"].iter().any(|unit| {
                lower[at + 1..].starts_with(unit) && !is_word(bytes.get(at + 1 + unit.len()))
            })
    });
    has_length && !lower.contains("var(") && !lower.contains('%') && !relative
}

/// A block's token declarations, in order, with its font sizes following the reader's
/// text size (upstream `readerTextSizeEntries`): each raw font-size step it writes at a
/// fixed length becomes its private scaled size and the step reading it
/// ([`scaled_font_size_entries`]), and each semantic size (`--text-*-size`) it fixes at
/// a length becomes that length times the scale of the step that size reads by
/// default, so supporting text fixed at `12px` scales like Footnote. A size that reads
/// a variable follows what it reads, and one relative to its parent (`em`, `%`)
/// follows the parent, so neither scales twice; a fixed size is not capped. A value
/// written `!important` stays important. Every other declaration keeps its value and
/// its place.
pub fn reader_text_size_entries(entries: &[(String, String)]) -> Vec<(String, String)> {
    let tokens: CssMap = entries.iter().cloned().collect();
    let scaled: CssMap = scaled_font_size_entries(&tokens).into_iter().collect();
    let fixed_steps = text_size_steps();
    let mut out = Vec::new();
    for (name, raw) in entries {
        if let Some(step) = scaled.get(name) {
            let property = scaled_step_property(name);
            out.push((
                property.clone(),
                scaled.get(&property).cloned().unwrap_or_default(),
            ));
            out.push((name.clone(), step.clone()));
            continue;
        }
        let (value, important) = split_important(raw);
        let reads = fixed_steps
            .iter()
            .find(|(text, _)| text == name)
            .and_then(|(_, step)| font_size_step_style(step));
        let value = match reads {
            Some(style) if is_fixed_size(value) => format!(
                "calc({value} * var({}, 1)){important}",
                dynamic_type_scale_property(style)
            ),
            _ => raw.clone(),
        };
        out.push((name.clone(), value));
    }
    out
}

/// A value and its trailing `!important`, apart (upstream `splitImportant`).
/// `!important` inside `calc()` makes the value invalid, so the scale wraps only the
/// value, and `!important` goes after the expression.
fn split_important(value: &str) -> (&str, &'static str) {
    // `/\s*!\s*important\s*$/i`, read from the end.
    let rest = value.trim_end_matches(is_js_whitespace);
    let Some(rest) = rest
        .len()
        .checked_sub("important".len())
        .filter(|at| rest.is_char_boundary(*at) && rest[*at..].eq_ignore_ascii_case("important"))
        .map(|at| &rest[..at])
    else {
        return (value, "");
    };
    match rest.trim_end_matches(is_js_whitespace).strip_suffix('!') {
        Some(before) => (before.trim_end_matches(is_js_whitespace), " !important"),
        None => (value, ""),
    }
}

/// A `px` or `rem` length in px (16px to the rem); `None` for anything else.
/// Upstream `lengthInPx`.
pub fn length_in_px(value: Option<&str>) -> Option<f64> {
    let value = js_trim(value.unwrap_or_default());
    let (number, per_unit) = match value.strip_suffix("px") {
        Some(number) => (number, 1.0),
        None => (value.strip_suffix("rem")?, 16.0),
    };
    // `-?\d*\.?\d+`.
    let digits = number.strip_prefix('-').unwrap_or(number);
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => ("", digits),
    };
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if fraction.is_empty() || !all_digits(whole) || !all_digits(fraction) {
        return None;
    }
    Some(number.parse::<f64>().ok()? * per_unit)
}

// =============================================================================
// Typed tokens
// =============================================================================

/// A typed length in px, 16px to the rem as [`length_in_px`] reads one.
fn typed_px(length: AbsoluteLength) -> f64 {
    match length {
        AbsoluteLength::Pixels(pixels) => f64::from(f32::from(pixels)),
        AbsoluteLength::Rems(r) => f64::from(r.0) * 16.0,
    }
}

/// `length` times `factor`, in its own unit.
fn scaled_length(length: AbsoluteLength, factor: f64) -> AbsoluteLength {
    match length {
        AbsoluteLength::Pixels(pixels) => {
            AbsoluteLength::Pixels(px((f64::from(f32::from(pixels)) * factor) as f32))
        }
        AbsoluteLength::Rems(r) => AbsoluteLength::Rems(rems((f64::from(r.0) * factor) as f32)),
    }
}

fn length_token(tokens: &Tokens, name: &str) -> Option<AbsoluteLength> {
    match tokens.get(name)? {
        TokenValue::Length(length) => Some(length),
        _ => None,
    }
}

/// A theme's typed tokens at the reader's text size `category`: the sizes a browser
/// computes for the declarations [`reader_text_size_entries`] and the root rule
/// ([`dynamic_type_scale_rule`]) write, so only font sizes move and every size is the
/// theme's own at Large.
///
/// Every raw step is a fixed length here, since the typed tokens hold resolved
/// lengths: each becomes its value times its curve's scale, capped at the next step's
/// scaled size (and never below its own size when the ramp is out of order), as
/// [`scaled_font_size_entries`] writes it. A semantic size (`--text-*-size`) that
/// equals the step it reads by default follows that step, as `var()` does; one the
/// theme fixed at another length is that length times the step's scale, uncapped.
/// Resolved tokens no longer say whether a size was written as a variable, so a step
/// a theme aliased to another (`--font-size-lg: var(--font-size-base)`) scales on its
/// own curve here, and a semantic size that reads another step than its default, or
/// is fixed at exactly its default step's length, scales as described above.
pub fn reader_text_size_tokens(tokens: &mut Tokens, category: TextSizeCategory) {
    if category == TextSizeCategory::L {
        return;
    }
    let Some(own) = FONT_SIZE_STEP_STYLES
        .iter()
        .map(|(name, _)| length_token(tokens, name))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let own_px: Vec<f64> = own.iter().map(|length| typed_px(*length)).collect();
    let mut scaled_px = vec![0.0; own.len()];
    for index in (0..own.len()).rev() {
        let style = FONT_SIZE_STEP_STYLES[index].1;
        let scaled = own_px[index] * dynamic_type_scale(style, category);
        scaled_px[index] = match scaled_px.get(index + 1) {
            None => scaled,
            Some(&next) if own_px[index] <= own_px[index + 1] => scaled.min(next),
            Some(&next) => scaled.min(next.max(own_px[index])),
        };
    }
    let scaled: Vec<AbsoluteLength> = own
        .iter()
        .zip(own_px.iter().zip(&scaled_px))
        .map(|(length, (own, scaled))| {
            if *own == 0.0 {
                *length
            } else {
                scaled_length(*length, scaled / own)
            }
        })
        .collect();
    for (text, step) in text_size_steps() {
        let Some(index) = FONT_SIZE_STEP_STYLES
            .iter()
            .position(|(name, _)| *name == step)
        else {
            continue;
        };
        let Some(size) = length_token(tokens, text) else {
            continue;
        };
        let value = if (typed_px(size) - own_px[index]).abs() < 1e-3 {
            scaled[index]
        } else {
            scaled_length(
                size,
                dynamic_type_scale(FONT_SIZE_STEP_STYLES[index].1, category),
            )
        };
        let _ = tokens.set(text, TokenValue::Length(value));
    }
    for ((name, _), length) in FONT_SIZE_STEP_STYLES.iter().zip(scaled) {
        let _ = tokens.set(name, TokenValue::Length(length));
    }
}

// =============================================================================
// The app's category
// =============================================================================

/// Sets the reader's text size category the app draws at, and redraws. It wins over
/// the system's from then on (see [`follow_system_text_size`]). Every theme's font
/// sizes follow it; Large, the default, leaves them as the theme wrote them.
pub fn set_text_size_category(category: TextSizeCategory, cx: &mut App) {
    crate::theme::set_app_text_size(Some(category), cx);
}

/// Drops the category the app set: the app follows the system's text size again where
/// the platform reports one (Windows), else draws at Large. Redraws.
pub fn follow_system_text_size(cx: &mut App) {
    crate::theme::set_app_text_size(None, cx);
}

/// The reader's text size category the app draws at now: the one it set, else the
/// system's, else Large.
pub fn text_size_category(cx: &App) -> TextSizeCategory {
    crate::theme::text_size(cx)
}

/// Reads the system's text size again (Windows; elsewhere there is nothing to read)
/// and redraws when it changed. A window gaining focus reads it too.
pub fn refresh_text_size_category(cx: &mut App) {
    crate::theme::refresh_system_text_size(cx);
}

/// The category the system's text size names (Windows' `UISettings::TextScaleFactor`,
/// by [`text_size_category_for_scale`]), or `None` where the platform reports none.
pub fn system_text_size_category() -> Option<TextSizeCategory> {
    system_text_scale_factor()
        .filter(|factor| factor.is_finite() && *factor > 0.0)
        .map(text_size_category_for_scale)
}

/// Windows' text scale (Settings > Accessibility > Text size), 1 to 2.25.
#[cfg(windows)]
fn system_text_scale_factor() -> Option<f64> {
    let settings = windows::UI::ViewManagement::UISettings::new().ok()?;
    settings.TextScaleFactor().ok()
}

#[cfg(not(windows))]
fn system_text_scale_factor() -> Option<f64> {
    None
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    use gpui::TestAppContext;

    use super::*;
    use crate::authoring::define::{DefineThemeInput, TokenInput, define_theme};
    use crate::authoring::resolve_values::{TypographyConfig, TypographyScale};

    fn css_map<const N: usize>(entries: [(&str, &str); N]) -> CssMap {
        entries
            .into_iter()
            .map(|(name, value)| (name, value.to_string()))
            .collect()
    }

    fn pairs(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    /// The step names, smallest first.
    fn steps() -> Vec<&'static str> {
        FONT_SIZE_STEP_STYLES
            .iter()
            .map(|(name, _)| *name)
            .collect()
    }

    /// A theme from a type scale.
    fn ramp_theme(name: &str, base: f64, ratio: f64) -> crate::authoring::define::DefinedTheme {
        define_theme(&DefineThemeInput {
            typography: Some(TypographyConfig {
                scale: Some(TypographyScale { base, ratio }),
                ..TypographyConfig::default()
            }),
            ..DefineThemeInput::new(name)
        })
        .unwrap()
    }

    fn entries_of(tokens: &CssMap) -> Vec<(String, String)> {
        tokens
            .iter()
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect()
    }

    /// Upstream test's `computedSizes`: what the browser computes for emitted custom
    /// properties at one category, in px, aligned with `entries`: each expression
    /// evaluated (min, max, calc of a length times a category scale, variables with
    /// their fallbacks), resolving each variable by name. A reference cycle, which a
    /// browser would compute to nothing, is an error.
    fn computed_sizes(
        entries: &[(String, String)],
        category: TextSizeCategory,
    ) -> Result<Vec<f64>, String> {
        let evaluator = Evaluator {
            expressions: entries
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect(),
            scales: DynamicTypeStyle::ALL
                .into_iter()
                .map(|style| {
                    (
                        dynamic_type_scale_property(style),
                        dynamic_type_scale(style, category),
                    )
                })
                .collect(),
            memo: RefCell::default(),
            visiting: RefCell::default(),
        };
        entries
            .iter()
            .map(|(name, _)| evaluator.resolve(name, None))
            .collect()
    }

    /// The emitted entries' computed sizes at one category, by name (upstream
    /// `sizesByName`).
    fn sizes_by_name(
        entries: &[(String, String)],
        category: TextSizeCategory,
    ) -> HashMap<String, f64> {
        let sizes = computed_sizes(entries, category).unwrap();
        entries
            .iter()
            .map(|(name, _)| name.clone())
            .zip(sizes)
            .collect()
    }

    struct Evaluator<'a> {
        expressions: HashMap<&'a str, &'a str>,
        scales: HashMap<String, f64>,
        memo: RefCell<HashMap<String, f64>>,
        visiting: RefCell<HashSet<String>>,
    }

    impl Evaluator<'_> {
        fn resolve(&self, name: &str, fallback: Option<&str>) -> Result<f64, String> {
            if let Some(scale) = self.scales.get(name) {
                return Ok(*scale);
            }
            if let Some(known) = self.memo.borrow().get(name) {
                return Ok(*known);
            }
            let Some(expression) = self.expressions.get(name).copied() else {
                return match fallback {
                    Some(fallback) => self.evaluate(fallback),
                    None => Err(format!("unset {name}")),
                };
            };
            if self.visiting.borrow().contains(name) {
                return Err(format!("reference cycle through {name}"));
            }
            self.visiting.borrow_mut().insert(name.to_string());
            let source = expression
                .strip_suffix("!important")
                .map_or(expression, str::trim_end);
            let value = self.evaluate(source)?;
            self.visiting.borrow_mut().remove(name);
            self.memo.borrow_mut().insert(name.to_string(), value);
            Ok(value)
        }

        fn evaluate(&self, source: &str) -> Result<f64, String> {
            let mut at = 0;
            let value = self.term(source, &mut at)?;
            skip_spaces(source, &mut at);
            if at != source.len() {
                return Err(format!("trailing input at {at} in {source}"));
            }
            Ok(value)
        }

        fn term(&self, source: &str, at: &mut usize) -> Result<f64, String> {
            skip_spaces(source, at);
            let pickers: [(&str, Pick); 2] = [("min(", f64::min), ("max(", f64::max)];
            for (name, pick) in pickers {
                if source[*at..].starts_with(name) {
                    read(source, at, name)?;
                    let first = self.term(source, at)?;
                    read(source, at, ",")?;
                    let second = self.term(source, at)?;
                    skip_spaces(source, at);
                    read(source, at, ")")?;
                    return Ok(pick(first, second));
                }
            }
            if source[*at..].starts_with("calc(") {
                read(source, at, "calc(")?;
                let left = self.term(source, at)?;
                skip_spaces(source, at);
                read(source, at, "*")?;
                let right = self.term(source, at)?;
                skip_spaces(source, at);
                read(source, at, ")")?;
                return Ok(left * right);
            }
            if source[*at..].starts_with("var(") {
                read(source, at, "var(")?;
                let bytes = source.as_bytes();
                let mut depth = 1;
                let mut end = *at;
                while depth > 0 {
                    match bytes.get(end) {
                        Some(b'(') => depth += 1,
                        Some(b')') => depth -= 1,
                        None => return Err(format!("unclosed var() in {source}")),
                        _ => {}
                    }
                    end += 1;
                }
                let inner = &source[*at..end - 1];
                *at = end;
                return match inner.find(',') {
                    None => self.resolve(inner.trim(), None),
                    Some(comma) => {
                        self.resolve(inner[..comma].trim(), Some(inner[comma + 1..].trim()))
                    }
                };
            }
            // `/^-?[\d.]+(rem|px)?/`.
            let rest = &source[*at..];
            let sign = usize::from(rest.starts_with('-'));
            let digits = rest[sign..]
                .bytes()
                .take_while(|b| b.is_ascii_digit() || *b == b'.')
                .count();
            if digits == 0 {
                return Err(format!("unexpected input at {at} in {source}"));
            }
            let number = &rest[..sign + digits];
            let unit = ["rem", "px"]
                .into_iter()
                .find(|unit| rest[sign + digits..].starts_with(unit));
            *at += number.len() + unit.map_or(0, str::len);
            let value: f64 = number
                .parse()
                .map_err(|_| format!("not a number: {number}"))?;
            Ok(match unit {
                Some("rem") => value * 16.0,
                _ => value,
            })
        }
    }

    /// `Math.min` or `Math.max`.
    type Pick = fn(f64, f64) -> f64;

    fn skip_spaces(source: &str, at: &mut usize) {
        while source.as_bytes().get(*at) == Some(&b' ') {
            *at += 1;
        }
    }

    fn read(source: &str, at: &mut usize, text: &str) -> Result<(), String> {
        if !source[*at..].starts_with(text) {
            return Err(format!("expected \"{text}\" at {at} in {source}"));
        }
        *at += text.len();
        Ok(())
    }

    /// The value that wins for `name` in a block's declarations: the last one
    /// (upstream's `winning` over one block, where source order decides).
    fn winning<'a>(entries: &'a [(String, String)], name: &str) -> Option<&'a str> {
        entries
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The custom properties that win in a block, `!important` dropped (upstream
    /// `winningAll`).
    fn winning_all(entries: &[(String, String)]) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for (name, value) in entries {
            let value = value
                .strip_suffix("!important")
                .map_or(value.as_str(), str::trim_end)
                .to_string();
            match out.iter_mut().find(|(key, _)| key == name) {
                Some(entry) => entry.1 = value,
                None => out.push((name.clone(), value)),
            }
        }
        out
    }

    /// The first reference cycle among the winning custom properties, if any
    /// (upstream `cycleIn`).
    fn cycle_in(properties: &[(String, String)]) -> Option<Vec<String>> {
        fn visit(
            name: &str,
            path: &mut Vec<String>,
            properties: &HashMap<&str, &str>,
            done: &mut HashSet<String>,
        ) -> Option<Vec<String>> {
            if done.contains(name) || !properties.contains_key(name) {
                return None;
            }
            if let Some(start) = path.iter().position(|open| open == name) {
                let mut cycle = path[start..].to_vec();
                cycle.push(name.to_string());
                return Some(cycle);
            }
            path.push(name.to_string());
            let value = properties[name];
            let mut rest = value;
            while let Some(at) = rest.find("var(") {
                rest = &rest[at + 4..];
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                    .unwrap_or(rest.len());
                if let Some(found) = visit(&rest[..end], path, properties, done) {
                    return Some(found);
                }
            }
            path.pop();
            done.insert(name.to_string());
            None
        }
        let map: HashMap<&str, &str> = properties
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let mut done = HashSet::new();
        properties
            .iter()
            .find_map(|(name, _)| visit(name, &mut Vec::new(), &map, &mut done))
    }

    /// The font-size properties among `properties` (upstream `fontSizes`).
    fn font_sizes(properties: &[(String, String)]) -> Vec<(String, String)> {
        properties
            .iter()
            .filter(|(name, _)| {
                name.starts_with("--font-size-") || name.starts_with("--_scaled-font-size-")
            })
            .cloned()
            .collect()
    }

    fn close(a: f64, b: f64, digits: i32) -> bool {
        (a - b).abs() < 10f64.powi(-digits) / 2.0
    }

    // Upstream dynamicType.test.ts: "has one category per column of the UIKit table,
    // Large fourth".
    #[test]
    fn one_category_per_column_of_the_uikit_table_large_fourth() {
        for style in DynamicTypeStyle::ALL {
            assert_eq!(style.row().len(), TEXT_SIZE_CATEGORIES.len());
        }
        assert_eq!(TEXT_SIZE_CATEGORIES[3], TextSizeCategory::L);
        assert_eq!(TextSizeCategory::default(), TextSizeCategory::L);
        for (index, category) in TEXT_SIZE_CATEGORIES.into_iter().enumerate() {
            assert_eq!(category.column(), index);
            assert_eq!(
                TextSizeCategory::from_name(category.as_str()),
                Some(category)
            );
        }
        assert_eq!(TextSizeCategory::from_name("xxxxl"), None);
        // The styles keep the table's keys and rows.
        for (style, (key, row)) in DynamicTypeStyle::ALL.into_iter().zip(TYPOGRAPHY.rows()) {
            assert_eq!(style.key(), key);
            assert_eq!(style.row(), row);
        }
    }

    // Upstream: "is named by the size of Body alone".
    #[test]
    fn the_size_of_body_alone_names_the_category() {
        for (index, category) in TEXT_SIZE_CATEGORIES.into_iter().enumerate() {
            assert_eq!(
                text_size_category_for_body_size(TYPOGRAPHY.body[index]),
                Some(category)
            );
        }
        // Sub-pixel rounding of a computed size still names the category.
        assert_eq!(
            text_size_category_for_body_size(17.2),
            Some(TextSizeCategory::L)
        );
    }

    // Upstream: "names nothing for a size Safari does not produce".
    #[test]
    fn a_size_safari_does_not_produce_names_nothing() {
        assert_eq!(text_size_category_for_body_size(18.0), None);
        assert_eq!(text_size_category_for_body_size(16.5), None);
        assert_eq!(text_size_category_for_body_size(f64::NAN), None);
    }

    // Upstream: "maps every raw font-size step to a UIKit text style".
    #[test]
    fn every_raw_font_size_step_maps_to_a_uikit_style() {
        let mut ours = steps();
        ours.sort_unstable();
        let defaults = text_size_defaults();
        let mut core: Vec<&str> = defaults.keys().collect();
        core.sort_unstable();
        assert_eq!(ours, core);
        for (_, style) in FONT_SIZE_STEP_STYLES {
            assert!(DynamicTypeStyle::ALL.contains(&style));
        }
        assert_eq!(
            font_size_step_style("--font-size-sm"),
            Some(DynamicTypeStyle::Footnote)
        );
        assert_eq!(font_size_step_style("--spacing-4"), None);
    }

    // Upstream: "leaves every size alone at Large".
    #[test]
    fn every_scale_is_1_at_large() {
        for style in DynamicTypeStyle::ALL {
            assert_eq!(dynamic_type_scale(style, TextSizeCategory::L), 1.0);
        }
    }

    // Upstream: "turns a Large size into the UIKit size at every category".
    #[test]
    fn a_large_size_turns_into_the_uikit_size_at_every_category() {
        for style in DynamicTypeStyle::ALL {
            let large = style.row()[3];
            for (index, category) in TEXT_SIZE_CATEGORIES.into_iter().enumerate() {
                assert!(close(
                    large * dynamic_type_scale(style, category),
                    style.row()[index],
                    10
                ));
            }
        }
        assert!(close(
            dynamic_type_scale(DynamicTypeStyle::Body, TextSizeCategory::Ax5),
            53.0 / 17.0,
            10
        ));
        assert!(close(
            dynamic_type_scale(DynamicTypeStyle::LargeTitle, TextSizeCategory::Xs),
            31.0 / 34.0,
            10
        ));
    }

    // Upstream: "gives the page a category its scales on :root, Large and an unnamed
    // size none".
    #[test]
    fn the_root_rule_gives_a_category_its_scales_large_and_no_category_none() {
        assert_eq!(dynamic_type_scale_rule(Some(TextSizeCategory::L)), "");
        assert_eq!(dynamic_type_scale_rule(None), "");
        let ax5 = dynamic_type_scale_rule(Some(TextSizeCategory::Ax5));
        assert!(ax5.contains("\n:root {\n  --_dynamic-type-"));
        assert!(ax5.contains(&format!(
            "{}: 3.1176;",
            dynamic_type_scale_property(DynamicTypeStyle::Body)
        )));
        assert!(ax5.contains(&format!(
            "{}: 1.7647;",
            dynamic_type_scale_property(DynamicTypeStyle::LargeTitle)
        )));
        assert_eq!(
            dynamic_type_scale_property(DynamicTypeStyle::LargeTitle),
            "--_dynamic-type-large-title"
        );
    }

    // Upstream: "scales only the raw font-size steps it is given at a length, each
    // capped by the next".
    #[test]
    fn scales_only_the_steps_given_at_a_length_each_capped_by_the_next() {
        assert_eq!(
            scaled_font_size_entries(&css_map([
                ("--font-size-base", "1rem"),
                ("--font-size-5xl", "2.625rem"),
                ("--spacing-4", "1rem"),
            ])),
            pairs(&[
                // Each step's scaled size is private; the step reads it.
                (
                    "--_scaled-font-size-base",
                    "min(calc(1rem * var(--_dynamic-type-body, 1)), max(var(--_scaled-font-size-lg, 1rem), 1rem))",
                ),
                ("--font-size-base", "var(--_scaled-font-size-base)"),
                // The largest step has nothing above it.
                (
                    "--_scaled-font-size-5xl",
                    "calc(2.625rem * var(--_dynamic-type-large-title, 1))",
                ),
                ("--font-size-5xl", "var(--_scaled-font-size-5xl)"),
            ])
        );
        // A step written as a variable or relative to its parent follows what it
        // reads, and the step below it is not capped by it.
        assert_eq!(
            scaled_font_size_entries(&css_map([
                ("--font-size-base", "1rem"),
                ("--font-size-lg", "var(--font-size-base)"),
                ("--font-size-xl", "1.25em"),
            ])),
            pairs(&[
                (
                    "--_scaled-font-size-base",
                    "calc(1rem * var(--_dynamic-type-body, 1))"
                ),
                ("--font-size-base", "var(--_scaled-font-size-base)"),
            ])
        );
    }

    // Upstream: "keeps every ramp in order at every category, and its own sizes at
    // Large".
    #[test]
    fn every_ramp_stays_in_order_at_every_category_and_its_own_at_large() {
        // The ramps a theme gets from `typography.scale`: the neutral phone profile,
        // and the dense, default and airy starting points.
        let ramps = [(16.0, 1.1736), (12.0, 1.125), (14.0, 1.2), (16.0, 1.25)];
        let steps = steps();
        for (base, ratio) in ramps {
            let theme = ramp_theme(&format!("dt-ramp-{base}-{ratio}"), base, ratio);
            let entries = scaled_font_size_entries(&theme.tokens);
            let names: Vec<&str> = entries
                .iter()
                .map(|(name, _)| name.as_str())
                .filter(|name| !name.starts_with("--_"))
                .collect();
            assert_eq!(names, steps);
            let own: Vec<f64> = steps
                .iter()
                .map(|name| length_in_px(theme.tokens.get(name).map(String::as_str)).unwrap())
                .collect();
            for category in TEXT_SIZE_CATEGORIES {
                let by_name = sizes_by_name(&entries, category);
                let sizes: Vec<f64> = steps.iter().map(|name| by_name[*name]).collect();
                for index in 1..steps.len() {
                    assert!(
                        sizes[index] >= sizes[index - 1] - 1e-9,
                        "{}, {base}/{ratio} at {}",
                        steps[index],
                        category.as_str()
                    );
                }
                if category == TextSizeCategory::L {
                    for (size, own) in sizes.iter().zip(&own) {
                        assert!(close(*size, *own, 9));
                    }
                }
            }
        }
        // Uncapped, heading 2 (Title 2) would pass heading 1 (Title 1) at AX5.
        let own = |index: i32| 16.0 * 1.1736f64.powi(index - 5);
        assert!(
            own(7) * dynamic_type_scale(DynamicTypeStyle::Title2, TextSizeCategory::Ax5)
                > own(8) * dynamic_type_scale(DynamicTypeStyle::Title1, TextSizeCategory::Ax5)
        );
    }

    // Upstream: "leaves a theme that sets no type scale on the sizes it inherits,
    // already scaled": the root rule scales Core's default steps below every theme.
    #[test]
    fn the_root_rule_scales_cores_default_steps() {
        let theme = define_theme(&DefineThemeInput {
            tokens: Some([("--color-accent", TokenInput::from("red"))].into()),
            ..DefineThemeInput::new("dt-colors-only")
        })
        .unwrap();
        let block = reader_text_size_entries(&entries_of(&theme.tokens));
        for name in steps() {
            assert_eq!(winning(&block, name), None);
        }
        let rule = dynamic_type_scale_rule(Some(TextSizeCategory::Ax5));
        assert!(rule.starts_with("@layer astryx-base {\n:root {"));
        for (name, value) in text_size_defaults().iter() {
            let step = scaled_step_property(name);
            let opening = if name == "--font-size-5xl" {
                ""
            } else {
                "min("
            };
            let style = font_size_step_style(name).unwrap();
            assert!(rule.contains(&format!(
                "  {step}: {opening}calc({value} * var({}, 1))",
                dynamic_type_scale_property(style)
            )));
            assert!(rule.contains(&format!("  {name}: var({step});")));
        }
    }

    // Upstream: "keeps the iOS text-entry floor: the root font size never changes":
    // the page's rule declares nothing but custom properties.
    #[test]
    fn the_root_rule_declares_only_custom_properties() {
        let rule = dynamic_type_scale_rule(Some(TextSizeCategory::Xs));
        for line in rule.lines().filter(|line| line.contains(':')) {
            assert!(
                line.trim().starts_with("--") || line.starts_with(":root"),
                "{line}"
            );
        }
    }

    // Upstream: "scales a size the theme fixes on the curve of the step it reads".
    #[test]
    fn a_fixed_semantic_size_scales_on_the_curve_of_the_step_it_reads() {
        let block = reader_text_size_entries(&pairs(&[("--text-supporting-size", "12px")]));
        let value = winning(&block, "--text-supporting-size").unwrap();
        assert_eq!(value, "calc(12px * var(--_dynamic-type-footnote, 1))");
        for category in TEXT_SIZE_CATEGORIES {
            let size =
                computed_sizes(&pairs(&[("--text-supporting-size", value)]), category).unwrap()[0];
            assert!(close(
                size,
                12.0 * dynamic_type_scale(DynamicTypeStyle::Footnote, category),
                6
            ));
        }
    }

    // Upstream: "scales only fixed sizes: a size that reads a variable or its parent
    // is never scaled twice".
    #[test]
    fn only_fixed_sizes_scale() {
        let tokens = [
            ("--text-supporting-size", "0.75rem"),
            ("--text-heading-6-size", "clamp(10px, 1vw + 6px, 12px)"),
            ("--text-heading-5-size", "var(--font-size-xs)"),
            ("--text-label-size", "calc(var(--font-size-base) + 1px)"),
            ("--text-code-size", "0.9em"),
            ("--text-large-size", "110%"),
        ];
        let block = reader_text_size_entries(&pairs(&tokens));
        assert_eq!(
            winning(&block, "--text-supporting-size"),
            Some("calc(0.75rem * var(--_dynamic-type-footnote, 1))")
        );
        assert_eq!(
            winning(&block, "--text-heading-6-size"),
            Some("calc(clamp(10px, 1vw + 6px, 12px) * var(--_dynamic-type-caption1, 1))")
        );
        // The others keep what the theme wrote.
        for (name, value) in &tokens[2..] {
            assert_eq!(winning(&block, name), Some(*value));
        }
    }

    // Upstream: "keeps !important outside the scaling expression, on a fixed size and
    // on a step".
    #[test]
    fn important_stays_outside_the_scaling_expression() {
        let block = reader_text_size_entries(&pairs(&[
            ("--text-supporting-size", "12px !important"),
            ("--font-size-base", "1rem !important"),
        ]));
        assert_eq!(
            winning(&block, "--text-supporting-size"),
            Some("calc(12px * var(--_dynamic-type-footnote, 1)) !important")
        );
        let plain = scaled_font_size_entries(&css_map([("--font-size-base", "1rem")]));
        assert_eq!(
            winning(&block, "--_scaled-font-size-base"),
            Some(plain[0].1.as_str())
        );
        assert_eq!(
            winning(&block, "--font-size-base"),
            Some("var(--_scaled-font-size-base) !important")
        );
        // The importance does not hide a length from the order check.
        assert_eq!(
            scaled_font_size_entries(&css_map([
                ("--font-size-sm", "0.75rem"),
                ("--font-size-base", "0.875rem !important"),
            ])),
            pairs(&[
                (
                    "--_scaled-font-size-sm",
                    "min(calc(0.75rem * var(--_dynamic-type-footnote, 1)), var(--_scaled-font-size-base))",
                ),
                ("--font-size-sm", "var(--_scaled-font-size-sm)"),
                (
                    "--_scaled-font-size-base",
                    "min(calc(0.875rem * var(--_dynamic-type-body, 1)), max(var(--_scaled-font-size-lg, 0.875rem), 0.875rem))",
                ),
                (
                    "--font-size-base",
                    "var(--_scaled-font-size-base) !important"
                ),
            ])
        );
        for name in ["--text-supporting-size", "--_scaled-font-size-base"] {
            let value = winning(&block, name).unwrap();
            let value = value.strip_suffix(" !important").unwrap_or(value);
            assert!(!value.contains("!important"), "{name}");
        }
    }

    // Upstream: "follows a step a theme writes as a variable, with no reference cycle",
    // over the theme's root token block.
    #[test]
    fn a_step_written_as_a_variable_follows_what_it_reads_with_no_cycle() {
        let theme = define_theme(&DefineThemeInput {
            typography: Some(TypographyConfig {
                scale: Some(TypographyScale {
                    base: 14.0,
                    ratio: 1.2,
                }),
                ..TypographyConfig::default()
            }),
            tokens: Some([("--font-size-lg", TokenInput::from("var(--font-size-base)"))].into()),
            ..DefineThemeInput::new("dt-alias")
        })
        .unwrap();
        let block = reader_text_size_entries(&entries_of(&theme.tokens));
        assert_eq!(
            winning(&block, "--font-size-lg"),
            Some("var(--font-size-base)")
        );
        assert_eq!(winning(&block, "--_scaled-font-size-lg"), None);
        // Body text, whose next step is now a variable, scales uncapped.
        assert_eq!(
            winning(&block, "--_scaled-font-size-base"),
            Some("calc(0.875rem * var(--_dynamic-type-body, 1))")
        );
        let properties = winning_all(&block);
        assert_eq!(cycle_in(&properties), None);
        // Evaluated: the theme's own sizes at Large, its order at AX5.
        let large = sizes_by_name(&font_sizes(&properties), TextSizeCategory::L);
        assert!(close(large["--font-size-base"], 14.0, 9));
        assert!(close(large["--font-size-lg"], 14.0, 9));
        let own_sm = length_in_px(theme.tokens.get("--font-size-sm").map(String::as_str)).unwrap();
        assert!(close(large["--font-size-sm"], own_sm, 9));
        let ax5 = sizes_by_name(&font_sizes(&properties), TextSizeCategory::Ax5);
        assert!(close(ax5["--font-size-lg"], ax5["--font-size-base"], 9));
        assert!(ax5["--font-size-sm"] <= ax5["--font-size-base"] + 1e-9);
    }

    // Upstream: "emits no reference cycle with a step aliased at the root …", the root
    // blocks and the page's rule (the adaptation-rule half needs adaptations).
    #[test]
    fn no_reference_cycle_with_a_step_aliased_at_the_root() {
        let steps = steps();
        let mut themes = vec![
            define_theme(&DefineThemeInput::new("dt-acyclic-plain")).unwrap(),
            ramp_theme("dt-acyclic-scale", 15.0, 1.25),
        ];
        for at in 0..steps.len() - 1 {
            themes.push(
                define_theme(&DefineThemeInput {
                    typography: Some(TypographyConfig {
                        scale: Some(TypographyScale {
                            base: 14.0,
                            ratio: 1.2,
                        }),
                        ..TypographyConfig::default()
                    }),
                    tokens: Some(
                        [(
                            steps[at + 1],
                            TokenInput::from(format!("var({})", steps[at])),
                        )]
                        .into(),
                    ),
                    ..DefineThemeInput::new(format!("dt-acyclic-alias-{}", at + 1))
                })
                .unwrap(),
            );
        }
        for theme in &themes {
            let block = reader_text_size_entries(&entries_of(&theme.tokens));
            assert_eq!(cycle_in(&winning_all(&block)), None, "{}", theme.name);
        }
        let rule = dynamic_type_scale_rule(Some(TextSizeCategory::Ax5));
        let declarations: Vec<(String, String)> = rule
            .lines()
            .filter_map(|line| {
                let (name, value) = line.trim().split_once(": ")?;
                Some((name.to_string(), value.strip_suffix(';')?.to_string()))
            })
            .collect();
        assert_eq!(cycle_in(&winning_all(&declarations)), None);
    }

    // From the TS: `isFixedSize`, `splitImportant` and `lengthInPx` on the values the
    // tests above do not reach.
    #[test]
    fn fixed_sizes_important_and_lengths_read_as_upstream() {
        for fixed in ["12px", "0.75rem", "clamp(10px, 1vw + 6px, 12px)", "2REM"] {
            assert!(is_fixed_size(fixed), "{fixed}");
        }
        for relative in [
            "var(--font-size-base)",
            "VAR(--x) 1px",
            "110%",
            "0.9em",
            "2ex",
            "1ch",
            "3ic",
            "1cap",
            "1.5lh",
            "1.5EM",
            "medium",
            "",
        ] {
            assert!(!is_fixed_size(relative), "{relative}");
        }
        assert_eq!(split_important("12px !important"), ("12px", " !important"));
        assert_eq!(split_important("12px!IMPORTANT  "), ("12px", " !important"));
        assert_eq!(split_important("12px ! important"), ("12px", " !important"));
        assert_eq!(split_important("12px important"), ("12px important", ""));
        assert_eq!(split_important("12px"), ("12px", ""));
        assert_eq!(length_in_px(Some(" 1rem ")), Some(16.0));
        assert_eq!(length_in_px(Some(".5px")), Some(0.5));
        assert_eq!(length_in_px(Some("-2px")), Some(-2.0));
        assert_eq!(length_in_px(Some("1.rem")), None);
        assert_eq!(length_in_px(Some("1em")), None);
        assert_eq!(length_in_px(Some("calc(1px)")), None);
        assert_eq!(length_in_px(None), None);
    }

    // Read from the table: 1 is Large, and a scale names the category whose Body
    // scale is nearest (19/17 for xLarge, 21/17 for xxLarge, 40/17 for AX3).
    #[test]
    fn a_single_text_scale_names_the_nearest_body_category() {
        assert_eq!(text_size_category_for_scale(1.0), TextSizeCategory::L);
        assert_eq!(text_size_category_for_scale(1.1), TextSizeCategory::Xl);
        assert_eq!(text_size_category_for_scale(1.25), TextSizeCategory::Xxl);
        assert_eq!(text_size_category_for_scale(2.25), TextSizeCategory::Ax3);
        assert_eq!(text_size_category_for_scale(0.5), TextSizeCategory::Xs);
        assert_eq!(text_size_category_for_scale(9.0), TextSizeCategory::Ax5);
        for (index, category) in TEXT_SIZE_CATEGORIES.into_iter().enumerate() {
            let scale = TYPOGRAPHY.body[index] / TYPOGRAPHY.body[3];
            assert_eq!(text_size_category_for_scale(scale), category);
        }
    }

    /// `value` (`0.875rem`, `12px`) as a typed length.
    fn typed(value: &str) -> AbsoluteLength {
        match value.strip_suffix("rem") {
            Some(number) => AbsoluteLength::Rems(rems(number.parse().unwrap())),
            None => AbsoluteLength::Pixels(px(value.strip_suffix("px").unwrap().parse().unwrap())),
        }
    }

    /// Core's tokens with a defined theme's raw steps, and every semantic size at the
    /// step it reads by default, as a theme's resolved tokens hold them.
    fn typed_tokens(theme: &crate::authoring::define::DefinedTheme) -> Tokens {
        let mut tokens = crate::generated::default_theme::LIGHT;
        for name in steps() {
            let value = theme.tokens.get(name).unwrap();
            tokens.set(name, TokenValue::Length(typed(value))).unwrap();
        }
        for (text, step) in text_size_steps() {
            let value = tokens.get(step).unwrap();
            tokens.set(text, value).unwrap();
        }
        tokens
    }

    fn typed_px_of(tokens: &Tokens, name: &str) -> f64 {
        typed_px(length_token(tokens, name).unwrap())
    }

    // The typed tokens at a category are what the browser computes for the theme's
    // declarations (`scaled_font_size_entries` evaluated), for every ramp above.
    #[test]
    fn typed_tokens_at_a_category_are_what_the_declarations_compute() {
        for (base, ratio) in [(16.0, 1.1736), (12.0, 1.125), (14.0, 1.2), (16.0, 1.25)] {
            let theme = ramp_theme("dt-typed", base, ratio);
            let entries = scaled_font_size_entries(&theme.tokens);
            for category in TEXT_SIZE_CATEGORIES {
                let mut tokens = typed_tokens(&theme);
                reader_text_size_tokens(&mut tokens, category);
                let css = sizes_by_name(&entries, category);
                for name in steps() {
                    assert!(
                        (typed_px_of(&tokens, name) - css[name]).abs() < 1e-3,
                        "{name} {base}/{ratio} at {}",
                        category.as_str()
                    );
                }
                // A semantic size reading its step follows it, cap included.
                for (text, step) in text_size_steps() {
                    assert!(
                        (typed_px_of(&tokens, text) - css[step]).abs() < 1e-3,
                        "{text}"
                    );
                }
            }
        }
    }

    // A semantic size fixed at another length scales on its default step's curve,
    // uncapped, as `reader_text_size_entries` writes it; Large changes nothing.
    #[test]
    fn a_fixed_typed_semantic_size_scales_on_its_steps_curve() {
        let theme = ramp_theme("dt-typed-fixed", 14.0, 1.2);
        let mut tokens = typed_tokens(&theme);
        tokens
            .set("--text-supporting-size", TokenValue::Length(typed("12px")))
            .unwrap();
        let mut large = tokens.clone();
        reader_text_size_tokens(&mut large, TextSizeCategory::L);
        assert_eq!(format!("{large:?}"), format!("{tokens:?}"));
        reader_text_size_tokens(&mut tokens, TextSizeCategory::Ax2);
        let expected = 12.0 * dynamic_type_scale(DynamicTypeStyle::Footnote, TextSizeCategory::Ax2);
        assert!((typed_px_of(&tokens, "--text-supporting-size") - expected).abs() < 1e-4);
        // A rem length stays in rems.
        assert!(matches!(
            length_token(&tokens, "--font-size-base"),
            Some(AbsoluteLength::Rems(_))
        ));
    }

    // The app's category reaches rendering: `current` draws every font size at it,
    // and Large, the default, leaves the theme's own.
    #[gpui::test]
    fn the_apps_category_scales_the_font_sizes_components_read(cx: &mut TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| assert_eq!(text_size_category(cx), TextSizeCategory::L));
        let cx = cx.add_empty_window();
        let read = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| {
                let theme = crate::current(window, cx);
                let tokens = theme.tokens();
                (
                    theme.text_size_category(),
                    typed_px(tokens.font.size.base),
                    typed_px(tokens.text.body.size),
                    typed_px(tokens.text.supporting.size),
                )
            })
        };
        let (category, base, body, supporting) = read(cx);
        assert_eq!(category, TextSizeCategory::L);
        // Core's defaults: 0.875rem and 0.75rem.
        assert_eq!((base, body, supporting), (14.0, 14.0, 12.0));

        cx.update(|_, cx| set_text_size_category(TextSizeCategory::Ax5, cx));
        cx.update(|_, cx| assert_eq!(text_size_category(cx), TextSizeCategory::Ax5));
        let (category, base, body, supporting) = read(cx);
        assert_eq!(category, TextSizeCategory::Ax5);
        // The CSS core's default steps compute to at AX5.
        let css = sizes_by_name(
            &scaled_font_size_entries(&text_size_defaults()),
            TextSizeCategory::Ax5,
        );
        assert!((base - css["--font-size-base"]).abs() < 1e-3);
        assert!((body - css["--font-size-base"]).abs() < 1e-3);
        assert!((supporting - css["--font-size-sm"]).abs() < 1e-3);
        assert!(base > 14.0);

        // An explicit refresh keeps the app's category; following the system again
        // drops it (the test platform reads no system size).
        cx.update(|_, cx| refresh_text_size_category(cx));
        cx.update(|_, cx| assert_eq!(text_size_category(cx), TextSizeCategory::Ax5));
        cx.update(|_, cx| follow_system_text_size(cx));
        let (category, base, _, _) = read(cx);
        assert_eq!(category, TextSizeCategory::L);
        assert_eq!(base, 14.0);
    }

    // A surface's sizes scale too, as upstream's `readerTextSizeEntries` writes a
    // MediaTheme surface's tokens.
    #[test]
    fn a_surfaces_font_sizes_follow_the_category() {
        use std::sync::Arc;

        use crate::types::{Appearance, Pointer, SurfaceOverride};
        let theme = crate::Theme {
            on_dark: Some(SurfaceOverride {
                appearance: Appearance::Dark,
                tokens: std::borrow::Cow::Owned(vec![(
                    std::borrow::Cow::Borrowed("--text-supporting-size"),
                    TokenValue::Length(typed("12.5px")),
                )]),
            }),
            ..crate::Theme::default()
        };
        let resolved = crate::ResolvedTheme::at_text_size(
            Arc::new(theme),
            Appearance::Light,
            1280.0,
            Pointer::Fine,
            TextSizeCategory::Xxxl,
        );
        let surface = resolved.on_surface(Appearance::Dark, 1280.0, Pointer::Fine);
        assert_eq!(surface.text_size_category(), TextSizeCategory::Xxxl);
        let expected =
            12.5 * dynamic_type_scale(DynamicTypeStyle::Footnote, TextSizeCategory::Xxxl);
        assert!((typed_px(surface.tokens().text.supporting.size) - expected).abs() < 1e-4);
    }
}
