//! Value types of the token surface and of theme override rules.
//!
//! Every type here can be built in a `const`, so generated themes are static
//! data, and can also be built at run time, so an app can define its own
//! theme.

use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

pub use gpui::{AbsoluteLength, BoxShadow, Edges, FontWeight, Hsla, Point, hsla, px, rems};

/// A `cubic-bezier(x1, y1, x2, y2)` easing curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier(pub f32, pub f32, pub f32, pub f32);

impl CubicBezier {
    /// The eased progress at linear progress `t` in `0..=1`.
    pub fn ease(&self, t: f32) -> f32 {
        let CubicBezier(x1, y1, x2, y2) = *self;
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        let bezier = |a: f32, b: f32, s: f32| {
            let u = 1.0 - s;
            3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
        };
        // Solve x(s) = t by bisection; x is monotonic for valid curves.
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        let mut s = t;
        for _ in 0..32 {
            let x = bezier(x1, x2, s);
            if (x - t).abs() < 1e-5 {
                break;
            }
            if x < t {
                low = s;
            } else {
                high = s;
            }
            s = (low + high) / 2.0;
        }
        bezier(y1, y2, s)
    }
}

/// A CSS font-family stack, first choice first.
#[derive(Clone, Debug, PartialEq)]
pub struct FontStack(pub Cow<'static, [Cow<'static, str>]>);

impl FontStack {
    /// Generic CSS families, which name no font file.
    const GENERIC: &'static [&'static str] = &[
        "-apple-system",
        "BlinkMacSystemFont",
        "system-ui",
        "ui-sans-serif",
        "ui-serif",
        "ui-monospace",
        "ui-rounded",
        "sans-serif",
        "serif",
        "monospace",
        "cursive",
        "fantasy",
    ];

    /// Every named family in order, without the generic CSS families.
    pub fn named(&self) -> impl Iterator<Item = &str> {
        self.0
            .iter()
            .map(|f| f.as_ref())
            .filter(|f| !Self::is_generic(f))
    }

    /// Whether `family` is a generic CSS family, which names no font file.
    pub fn is_generic(family: &str) -> bool {
        Self::GENERIC.contains(&family)
    }

    /// True when the stack asks for a monospace face.
    pub fn is_monospace(&self) -> bool {
        self.0
            .iter()
            .any(|f| matches!(f.as_ref(), "monospace" | "ui-monospace"))
    }
}

/// A `box-shadow`: zero or more layers, drawn in order.
#[derive(Clone, Debug)]
pub struct Shadow(pub Cow<'static, [BoxShadow]>);

impl Shadow {
    /// The layers as GPUI shadows.
    pub fn layers(&self) -> Vec<BoxShadow> {
        self.0.to_vec()
    }
}

/// A CSS keyword value, such as a border style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keyword(pub Cow<'static, str>);

/// One type style: the size, weight and line height that draw together.
#[derive(Clone, Copy, Debug)]
pub struct TextStyleTokens {
    pub size: AbsoluteLength,
    pub weight: FontWeight,
    /// Line height as a multiple of the font size.
    pub leading: f32,
}

/// Any token's value, for reading and writing tokens by name.
#[derive(Clone, Debug)]
pub enum TokenValue {
    Color(Hsla),
    Length(AbsoluteLength),
    Weight(FontWeight),
    Leading(f32),
    Number(f32),
    Duration(Duration),
    Ease(CubicBezier),
    Families(FontStack),
    Shadow(Shadow),
    Keyword(Keyword),
}

impl TokenValue {
    /// The variant's name, as `Tokens::set` reports it.
    pub fn kind(&self) -> &'static str {
        match self {
            TokenValue::Color(_) => "Color",
            TokenValue::Length(_) => "Length",
            TokenValue::Weight(_) => "Weight",
            TokenValue::Leading(_) => "Leading",
            TokenValue::Number(_) => "Number",
            TokenValue::Duration(_) => "Duration",
            TokenValue::Ease(_) => "Ease",
            TokenValue::Families(_) => "Families",
            TokenValue::Shadow(_) => "Shadow",
            TokenValue::Keyword(_) => "Keyword",
        }
    }
}

/// `#RRGGBB` or `#RRGGBBAA` for a color.
pub fn hex(color: Hsla) -> String {
    let c = color.to_rgb();
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if byte(c.a) == 255 {
        format!("#{:02X}{:02X}{:02X}", byte(c.r), byte(c.g), byte(c.b))
    } else {
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            byte(c.r),
            byte(c.g),
            byte(c.b),
            byte(c.a)
        )
    }
}

fn length(f: &mut fmt::Formatter<'_>, length: &AbsoluteLength) -> fmt::Result {
    match length {
        AbsoluteLength::Pixels(px) => write!(f, "{}px", f32::from(*px)),
        AbsoluteLength::Rems(rems) => write!(f, "{}rem", rems.0),
    }
}

/// CSS-like text: `#0064E0`, `16px`, `1rem`, `125ms`, `cubic-bezier(..)`.
impl fmt::Display for TokenValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenValue::Color(c) => f.write_str(&hex(*c)),
            TokenValue::Length(l) => length(f, l),
            TokenValue::Weight(w) => write!(f, "{}", w.0),
            TokenValue::Leading(v) | TokenValue::Number(v) => write!(f, "{v}"),
            TokenValue::Duration(d) => write!(f, "{}ms", d.as_secs_f64() * 1000.0),
            TokenValue::Ease(CubicBezier(a, b, c, d)) => {
                write!(f, "cubic-bezier({a}, {b}, {c}, {d})")
            }
            TokenValue::Families(stack) => {
                let names: Vec<&str> = stack.0.iter().map(|n| n.as_ref()).collect();
                f.write_str(&names.join(", "))
            }
            TokenValue::Shadow(shadow) => {
                if shadow.0.is_empty() {
                    return f.write_str("none");
                }
                for (i, layer) in shadow.0.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(
                        f,
                        "{}{}px {}px {}px {}px {}",
                        if layer.inset { "inset " } else { "" },
                        f32::from(layer.offset.x),
                        f32::from(layer.offset.y),
                        f32::from(layer.blur_radius),
                        f32::from(layer.spread_radius),
                        hex(layer.color)
                    )?;
                }
                Ok(())
            }
            TokenValue::Keyword(k) => f.write_str(&k.0),
        }
    }
}

/// Why a token could not be set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenError {
    /// No shared token has this name.
    Unknown(String),
    /// The token holds another kind of value.
    WrongKind {
        token: String,
        expected: &'static str,
        got: &'static str,
    },
}

impl TokenError {
    pub fn wrong_kind(token: &str, expected: &'static str, got: &TokenValue) -> Self {
        TokenError::WrongKind {
            token: token.to_string(),
            expected,
            got: got.kind(),
        }
    }
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenError::Unknown(name) => write!(f, "no token is named {name}"),
            TokenError::WrongKind {
                token,
                expected,
                got,
            } => write!(f, "{token} holds a {expected}, not a {got}"),
        }
    }
}

impl std::error::Error for TokenError {}

/// Light or dark: the appearance a theme's tokens are resolved in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

/// The interaction state a component override applies in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleState {
    Hover,
    Active,
    FocusVisible,
}

/// A style a theme sets on a component part, mirroring the CSS properties
/// Astryx themes write.
#[derive(Clone, Debug)]
pub enum StyleProp {
    Background(Hsla),
    TextColor(Hsla),
    BorderColor(Hsla),
    BorderWidth(AbsoluteLength),
    BorderStyle(Keyword),
    Radius(AbsoluteLength),
    Padding(Edges<AbsoluteLength>),
    /// Start and end padding on the inline axis.
    PaddingInline(AbsoluteLength, AbsoluteLength),
    /// Top and bottom padding.
    PaddingBlock(AbsoluteLength, AbsoluteLength),
    FontFamily(FontStack),
    FontSize(AbsoluteLength),
    FontWeight(FontWeight),
    /// Line height as a multiple of the font size.
    LineHeight(f32),
    Height(AbsoluteLength),
    Shadow(Shadow),
    Opacity(f32),
}

/// One declaration of a component override: a style on the part, or a token
/// re-defined for the part and everything inside it.
#[derive(Clone, Debug)]
pub enum Declaration {
    Style(StyleProp),
    Token(Cow<'static, str>, TokenValue),
}

/// A theme override for one component part (Astryx `components` entry),
/// selected the way `.astryx-<target>[data-<prop>="<value>"]` selects it.
#[derive(Clone, Debug)]
pub struct ComponentRule {
    /// The theming target, such as `button` or `segmented-control-item`.
    pub target: Cow<'static, str>,
    /// Every `(prop, value)` that must hold; a boolean state is `("selected", "true")`.
    pub when: Cow<'static, [(Cow<'static, str>, Cow<'static, str>)]>,
    /// The interaction state the rule applies in, or every state.
    pub state: Option<RuleState>,
    pub light: Cow<'static, [Declaration]>,
    pub dark: Cow<'static, [Declaration]>,
}

impl ComponentRule {
    /// The declarations for one appearance.
    pub fn declarations(&self, appearance: Appearance) -> &[Declaration] {
        match appearance {
            Appearance::Light => &self.light,
            Appearance::Dark => &self.dark,
        }
    }

    /// True when every condition holds for a part with these `(prop, value)`s.
    pub fn matches(&self, target: &str, props: &[(&str, &str)]) -> bool {
        self.target == target
            && self
                .when
                .iter()
                .all(|(prop, value)| props.iter().any(|(p, v)| p == prop && v == value))
    }
}

/// The kind of pointer an adaptation applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    Fine,
    Coarse,
}

/// Upstream `widthBreakpoints`: the theme's named width points, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidthBreakpoints {
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    /// Upstream `2xl`.
    pub xl2: f32,
}

impl WidthBreakpoints {
    /// Upstream `DEFAULT_WIDTH_BREAKPOINTS`.
    pub const DEFAULT: WidthBreakpoints = WidthBreakpoints {
        sm: 640.,
        md: 768.,
        lg: 1024.,
        xl: 1280.,
        xl2: 1536.,
    };

    /// The point named `name` (`sm`, `md`, `lg`, `xl` or `2xl`).
    pub fn get(&self, name: &str) -> Option<f32> {
        match name {
            "sm" => Some(self.sm),
            "md" => Some(self.md),
            "lg" => Some(self.lg),
            "xl" => Some(self.xl),
            "2xl" => Some(self.xl2),
            _ => None,
        }
    }
}

impl Default for WidthBreakpoints {
    fn default() -> Self {
        WidthBreakpoints::DEFAULT
    }
}

/// Token overrides that apply at some window widths or pointer kinds.
#[derive(Clone, Debug)]
pub struct Adaptation {
    pub pointer: Option<Pointer>,
    /// Applies when the window is narrower than this many pixels.
    pub width_below: Option<f32>,
    /// Applies when the window is at least this many pixels wide.
    pub width_at_least: Option<f32>,
    pub light: Cow<'static, [(Cow<'static, str>, TokenValue)]>,
    pub dark: Cow<'static, [(Cow<'static, str>, TokenValue)]>,
}

impl Adaptation {
    pub fn applies(&self, width: f32, pointer: Pointer) -> bool {
        self.pointer.is_none_or(|p| p == pointer)
            && self.width_below.is_none_or(|w| width < w)
            && self.width_at_least.is_none_or(|w| width >= w)
    }
}

/// The tempo a theme's motion plays at: what it writes to upstream's private
/// `--_astryx-motion-tempo`, which the animator reads (upstream `themeTempo`), and
/// whether the theme sets `tempo` itself (`ResolvedMotionKnobs::tempo_is_set`).
///
/// A theme whose durations come from a duration scale alone writes a tempo of 1, so
/// its springs keep their own pace (`spec:AST-108` FR5); every shipped theme plays at
/// 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionTempo {
    /// The tempo springs and timed motion play at: 1 is Astryx's own, 0.75 the
    /// quickest and 1.5 the slowest.
    pub tempo: f64,
    /// Whether the theme sets `tempo` itself, rather than playing at 1 by default.
    pub is_set: bool,
}

impl MotionTempo {
    /// Tempo 1, not set: a theme that sets no tempo.
    pub const DEFAULT: MotionTempo = MotionTempo {
        tempo: 1.0,
        is_set: false,
    };
}

impl Default for MotionTempo {
    fn default() -> Self {
        MotionTempo::DEFAULT
    }
}

/// Token overrides for content on an inverted surface (`MediaTheme`).
#[derive(Clone, Debug)]
pub struct SurfaceOverride {
    /// The appearance the surface forces.
    pub appearance: Appearance,
    pub tokens: Cow<'static, [(Cow<'static, str>, TokenValue)]>,
}
