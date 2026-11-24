//! Applying tokens to GPUI elements.

use gpui::{DefiniteLength, Styled};

use crate::theme::apply_style;
use crate::types::{FontStack, Shadow, StyleProp, TextStyleTokens};

/// Token-aware styling for any GPUI element.
pub trait ThemeStyled: Styled + Sized {
    /// Draws text with a CSS font stack: its first named family, then the
    /// rest as fallbacks.
    fn font_stack(mut self, stack: &FontStack) -> Self {
        apply_style(self.style(), &StyleProp::FontFamily(stack.clone()));
        self
    }

    /// Draws text in a type style: size, weight and line height together.
    fn text_style(mut self, style: &TextStyleTokens) -> Self {
        let text = &mut self.style().text;
        text.font_size = Some(style.size);
        text.font_weight = Some(style.weight);
        text.line_height = Some(DefiniteLength::Fraction(style.leading));
        self
    }

    /// Draws a token shadow, every layer in order.
    fn shadow_token(mut self, shadow: &Shadow) -> Self {
        self.style().box_shadow = Some(shadow.layers());
        self
    }
}

impl<T: Styled> ThemeStyled for T {}
