//! The typography vocabulary Text and Heading share, mapped onto tokens the
//! same way `packages/core/src/Text/text.stylex.ts` maps it onto CSS
//! variables.

use astryx_theme::{TextStyleTokens, Tokens};
use gpui::{AbsoluteLength, FontWeight, Hsla};

/// Upstream `type`: the semantic text type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextType {
    #[default]
    Body,
    Large,
    Label,
    Supporting,
    Code,
    Display1,
    Display2,
    Display3,
    /// Size, weight and line height all come from the parent.
    Inherit,
}

impl TextType {
    pub fn as_str(self) -> &'static str {
        match self {
            TextType::Body => "body",
            TextType::Large => "large",
            TextType::Label => "label",
            TextType::Supporting => "supporting",
            TextType::Code => "code",
            TextType::Display1 => "display-1",
            TextType::Display2 => "display-2",
            TextType::Display3 => "display-3",
            TextType::Inherit => "inherit",
        }
    }

    /// The type style's tokens, or `None` for `Inherit`.
    pub fn tokens(self, t: &Tokens) -> Option<&TextStyleTokens> {
        Some(match self {
            TextType::Body => &t.text.body,
            TextType::Large => &t.text.large,
            TextType::Label => &t.text.label,
            TextType::Supporting => &t.text.supporting,
            TextType::Code => &t.text.code,
            TextType::Display1 => &t.text.display_1,
            TextType::Display2 => &t.text.display_2,
            TextType::Display3 => &t.text.display_3,
            TextType::Inherit => return None,
        })
    }
}

/// Upstream `size`: an explicit font size from the size scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSize {
    Xs4,
    Xs3,
    Xs2,
    /// Upstream `xsm`, the `--font-size-xs` step.
    Xsm,
    Sm,
    Base,
    Lg,
    Xl,
    Xl2,
    Xl3,
    Xl4,
}

impl TextSize {
    pub fn as_str(self) -> &'static str {
        match self {
            TextSize::Xs4 => "4xs",
            TextSize::Xs3 => "3xs",
            TextSize::Xs2 => "2xs",
            TextSize::Xsm => "xsm",
            TextSize::Sm => "sm",
            TextSize::Base => "base",
            TextSize::Lg => "lg",
            TextSize::Xl => "xl",
            TextSize::Xl2 => "2xl",
            TextSize::Xl3 => "3xl",
            TextSize::Xl4 => "4xl",
        }
    }

    pub fn token(self, t: &Tokens) -> AbsoluteLength {
        let s = &t.font.size;
        match self {
            TextSize::Xs4 => s.s4xs,
            TextSize::Xs3 => s.s3xs,
            TextSize::Xs2 => s.s2xs,
            TextSize::Xsm => s.xs,
            TextSize::Sm => s.sm,
            TextSize::Base => s.base,
            TextSize::Lg => s.lg,
            TextSize::Xl => s.xl,
            TextSize::Xl2 => s.s2xl,
            TextSize::Xl3 => s.s3xl,
            TextSize::Xl4 => s.s4xl,
        }
    }
}

/// Upstream `color`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextColor {
    Primary,
    Secondary,
    Disabled,
    /// Upstream draws placeholder text in the secondary color.
    Placeholder,
    Accent,
    /// The parent's color.
    Inherit,
}

impl TextColor {
    pub fn as_str(self) -> &'static str {
        match self {
            TextColor::Primary => "primary",
            TextColor::Secondary => "secondary",
            TextColor::Disabled => "disabled",
            TextColor::Placeholder => "placeholder",
            TextColor::Accent => "accent",
            TextColor::Inherit => "inherit",
        }
    }

    /// The color token, or `None` to inherit.
    pub fn token(self, t: &Tokens) -> Option<Hsla> {
        Some(match self {
            TextColor::Primary => t.color.text_primary,
            TextColor::Secondary | TextColor::Placeholder => t.color.text_secondary,
            TextColor::Disabled => t.color.text_disabled,
            TextColor::Accent => t.color.text_accent,
            TextColor::Inherit => return None,
        })
    }
}

/// Upstream `weight`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextWeight {
    Normal,
    Medium,
    Semibold,
    Bold,
}

impl TextWeight {
    pub fn token(self, t: &Tokens) -> FontWeight {
        let w = &t.font.weight;
        match self {
            TextWeight::Normal => w.normal,
            TextWeight::Medium => w.medium,
            TextWeight::Semibold => w.semibold,
            TextWeight::Bold => w.bold,
        }
    }
}

/// Upstream `justify`: logical alignment (start/end follow the reading
/// direction once RTL lands in M15; until then start is left).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
}

/// Upstream `textWrap`. GPUI wraps greedily, so `Balance` and `Pretty` wrap
/// like `Wrap`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextWrap {
    #[default]
    Wrap,
    Nowrap,
    Balance,
    Pretty,
}
