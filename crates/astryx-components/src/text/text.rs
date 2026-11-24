//! Text: semantic body text with theme-driven type styles.
//! Upstream: `packages/core/src/Text/Text.tsx`.

use std::sync::Arc;

use astryx_theme::{ResolvedTheme, ThemeStyled, current};
use gpui::{
    AnyElement, App, Div, ElementId, FontFeatures, InteractiveElement as _, IntoElement,
    ParentElement, Refineable, RenderOnce, SharedString, StyleRefinement, Styled, Window, div,
};
use smallvec::SmallVec;

use super::typography::*;

/// Upstream `as`: the element Text renders as. The heading elements draw in
/// the heading font, as upstream's theme styles `h1`–`h6`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextElement {
    #[default]
    Span,
    P,
    Div,
    Label,
    H1,
    H2,
    H3,
}

/// Upstream `hasTruncateTooltip`: whether, and where, truncated text shows
/// its full content in a tooltip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TruncateTooltip {
    #[default]
    Default,
    Off,
    Above,
    Below,
    Start,
    End,
}

/// Upstream `wordBreak`. GPUI breaks an over-long word only when it cannot
/// fit a line, which is `BreakWord`; `BreakAll` draws the same.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordBreak {
    BreakWord,
    BreakAll,
}

/// The settings Text and Heading share.
pub(crate) struct Common {
    pub text: SharedString,
    pub children: SmallVec<[AnyElement; 1]>,
    pub color: Option<TextColor>,
    pub weight: Option<TextWeight>,
    pub max_lines: usize,
    pub truncate_tooltip: TruncateTooltip,
    pub word_break: Option<WordBreak>,
    pub text_wrap: TextWrap,
    pub justify: Justify,
    pub strikethrough: bool,
    pub id: Option<ElementId>,
    pub style: StyleRefinement,
}

impl Common {
    pub fn new(text: SharedString) -> Self {
        Common {
            text,
            children: SmallVec::new(),
            color: None,
            weight: None,
            max_lines: 0,
            truncate_tooltip: TruncateTooltip::Default,
            word_break: None,
            text_wrap: TextWrap::Wrap,
            justify: Justify::Start,
            strikethrough: false,
            id: None,
            style: StyleRefinement::default(),
        }
    }

    /// Applies color, weight, wrapping, alignment and truncation to `el`.
    pub fn apply(&self, mut el: Div, default_color: TextColor, theme: &ResolvedTheme) -> Div {
        let t = theme.tokens();
        if let Some(color) = self.color.unwrap_or(default_color).token(t) {
            el = el.text_color(color);
        }
        if let Some(weight) = self.weight {
            el = el.font_weight(weight.token(t));
        }
        el = match self.justify {
            Justify::Start => el.text_left(),
            Justify::Center => el.text_center(),
            Justify::End => el.text_right(),
        };
        if self.text_wrap == TextWrap::Nowrap {
            el = el.whitespace_nowrap();
        }
        el = match self.max_lines {
            0 => el,
            1 => el.min_w_0().truncate(),
            lines => el
                .min_w_0()
                .overflow_hidden()
                .line_clamp(lines)
                .text_ellipsis(),
        };
        if self.strikethrough {
            el = el.line_through();
        }
        el
    }

    /// Adds the content.
    pub fn finish(self, el: Div) -> Div {
        el.child(self.text).children(self.children)
    }
}

/// Semantic text. Its size, weight and line height come from the theme's
/// type style; its color from the theme's text colors.
///
/// ```ignore
/// Text::new("Saved 3 minutes ago").text_type(TextType::Supporting)
/// ```
#[derive(IntoElement)]
pub struct Text {
    common: Common,
    text_type: TextType,
    size: Option<TextSize>,
    element: TextElement,
    tabular_numbers: bool,
}

impl Text {
    /// Upstream `children`: the text.
    pub fn new(text: impl Into<SharedString>) -> Self {
        Text {
            common: Common::new(text.into()),
            text_type: TextType::Body,
            size: None,
            element: TextElement::Span,
            tabular_numbers: false,
        }
    }

    astryx_primitives::setters! {
        /// Upstream `type`: size, weight and line height from the theme.
        text_type: TextType,
        /// Upstream `size`: a font size from the scale, keeping the type's weight and line height.
        ?size: TextSize,
        /// Upstream `as`: the element rendered.
        element: TextElement,
        /// Upstream `hasTabularNumbers`: equal-width digits for aligned numbers.
        !tabular_numbers,
    }

    /// Upstream `color`. Defaults to secondary for the supporting type and
    /// primary otherwise.
    pub fn color(mut self, color: TextColor) -> Self {
        self.common.color = Some(color);
        self
    }

    /// Upstream `weight`: overrides the type's weight.
    pub fn weight(mut self, weight: TextWeight) -> Self {
        self.common.weight = Some(weight);
        self
    }

    /// Upstream `maxLines`: lines before the text is cut with an ellipsis; 0 never cuts.
    pub fn max_lines(mut self, lines: usize) -> Self {
        self.common.max_lines = lines;
        self
    }

    /// Upstream `hasTruncateTooltip`. The tooltip itself lands with Tooltip (M5).
    pub fn truncate_tooltip(mut self, tooltip: TruncateTooltip) -> Self {
        self.common.truncate_tooltip = tooltip;
        self
    }

    /// Upstream `wordBreak`.
    pub fn word_break(mut self, word_break: WordBreak) -> Self {
        self.common.word_break = Some(word_break);
        self
    }

    /// Upstream `textWrap`.
    pub fn text_wrap(mut self, wrap: TextWrap) -> Self {
        self.common.text_wrap = wrap;
        self
    }

    /// Upstream `justify`.
    pub fn justify(mut self, justify: Justify) -> Self {
        self.common.justify = justify;
        self
    }

    /// Upstream `hasStrikethrough`.
    pub fn strikethrough(mut self, strikethrough: bool) -> Self {
        self.common.strikethrough = strikethrough;
        self
    }

    /// Upstream `id`: the element id, for tests and accessibility.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.common.id = Some(id.into());
        self
    }
}

impl Styled for Text {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.common.style
    }
}

impl ParentElement for Text {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.common.children.extend(elements);
    }
}

impl Text {
    /// The styled root, before content: defaults, then theme rules, then the
    /// caller's style. Kept apart from `render` so tests can inspect it.
    pub(crate) fn root(&self, theme: &ResolvedTheme) -> Div {
        let t = theme.tokens();
        let mut el = div();
        if let Some(style) = self.text_type.tokens(t) {
            el = el.text_style(style);
        }
        if self.text_type == TextType::Code {
            el = el.font_stack(&t.font.family.code);
        } else if matches!(
            self.element,
            TextElement::H1 | TextElement::H2 | TextElement::H3
        ) {
            el = el.font_stack(&t.font.family.heading);
        }
        if let Some(size) = self.size {
            el.style().text.font_size = Some(size.token(t));
        }
        if self.tabular_numbers {
            el = el.font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)])));
        }
        let default_color = if self.text_type == TextType::Supporting {
            TextColor::Secondary
        } else {
            TextColor::Primary
        };
        el = self.common.apply(el, default_color, theme);

        let color = self.common.color.unwrap_or(default_color);
        let mut props = vec![("type", self.text_type.as_str()), ("color", color.as_str())];
        if let Some(size) = self.size {
            props.push(("size", size.as_str()));
        }
        theme.refine(el.style(), "text", &props, None);
        // Upstream's theme CSS emits the `[data-size]` rules after the type
        // rules, so an explicit size wins over a theme's type font size.
        if let Some(size) = self.size {
            el.style().text.font_size = Some(size.token(t));
        }
        el.style().refine(&self.common.style);
        el
    }
}

impl RenderOnce for Text {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let el = self.root(&theme);
        let id = self.common.id.clone();
        let el = self.common.finish(el);
        match id {
            Some(id) => el.id(id).into_any_element(),
            None => el.into_any_element(),
        }
    }
}
