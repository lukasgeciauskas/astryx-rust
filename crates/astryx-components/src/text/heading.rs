//! Heading: h1–h6 with theme type styles, in the heading font.
//! Upstream: `packages/core/src/Text/Text.tsx` (Heading).

use astryx_theme::{ResolvedTheme, ThemeStyled, current};
use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    Refineable, RenderOnce, Role, SharedString, StatefulInteractiveElement as _, StyleRefinement,
    Styled, Window, div,
};

use super::text::{Common, TruncateTooltip, WordBreak};
use super::typography::*;

/// Upstream `level`: the heading level, 1 to 6.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadingLevel {
    H1 = 1,
    H2 = 2,
    H3 = 3,
    H4 = 4,
    H5 = 5,
    H6 = 6,
}

impl HeadingLevel {
    fn number(self) -> usize {
        self as usize
    }

    fn as_str(self) -> &'static str {
        ["1", "2", "3", "4", "5", "6"][self.number() - 1]
    }
}

/// Upstream `type` on Heading: a display style instead of the level's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayType {
    Display1,
    Display2,
    Display3,
}

/// A heading. Its level sets the type style and the accessibility level.
///
/// ```ignore
/// Heading::new(HeadingLevel::H2, "Billing").id("billing-heading")
/// ```
#[derive(IntoElement)]
pub struct Heading {
    common: Common,
    level: HeadingLevel,
    display_type: Option<DisplayType>,
    accessibility_level: Option<HeadingLevel>,
}

impl Heading {
    /// Upstream `level` and `children`.
    pub fn new(level: HeadingLevel, text: impl Into<SharedString>) -> Self {
        Heading {
            common: Common::new(text.into()),
            level,
            display_type: None,
            accessibility_level: None,
        }
    }

    astryx_primitives::setters! {
        /// Upstream `type`: draw in a display style instead of the level's.
        ?display_type: DisplayType,
        /// Upstream `accessibilityLevel`: the level screen readers announce, when it
        /// differs from the visual level.
        ?accessibility_level: HeadingLevel,
    }

    /// Upstream `color` (default primary).
    pub fn color(mut self, color: TextColor) -> Self {
        self.common.color = Some(color);
        self
    }

    /// Upstream `weight`: wins over the level's or display type's weight.
    pub fn weight(mut self, weight: TextWeight) -> Self {
        self.common.weight = Some(weight);
        self
    }

    /// Upstream `maxLines`.
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

    /// Upstream `id`. A heading with an id is exposed to screen readers as a
    /// heading of its level; GPUI's accessibility tree needs the id.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.common.id = Some(id.into());
        self
    }
}

impl Styled for Heading {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.common.style
    }
}

impl ParentElement for Heading {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.common.children.extend(elements);
    }
}

impl Heading {
    /// The styled root, before content; see `Text::root`.
    pub(crate) fn root(&self, theme: &ResolvedTheme) -> Div {
        let t = theme.tokens();
        let style = match self.display_type {
            Some(DisplayType::Display1) => &t.text.display_1,
            Some(DisplayType::Display2) => &t.text.display_2,
            Some(DisplayType::Display3) => &t.text.display_3,
            None => match self.level {
                HeadingLevel::H1 => &t.text.heading_1,
                HeadingLevel::H2 => &t.text.heading_2,
                HeadingLevel::H3 => &t.text.heading_3,
                HeadingLevel::H4 => &t.text.heading_4,
                HeadingLevel::H5 => &t.text.heading_5,
                HeadingLevel::H6 => &t.text.heading_6,
            },
        };
        let mut el = div().font_stack(&t.font.family.heading).text_style(style);
        el = self.common.apply(el, TextColor::Primary, theme);

        let color = self.common.color.unwrap_or(TextColor::Primary);
        let mut props = vec![("level", self.level.as_str()), ("color", color.as_str())];
        if let Some(display) = self.display_type {
            props.push((
                "type",
                match display {
                    DisplayType::Display1 => "display-1",
                    DisplayType::Display2 => "display-2",
                    DisplayType::Display3 => "display-3",
                },
            ));
        }
        theme.refine(el.style(), "heading", &props, None);
        el.style().refine(&self.common.style);
        el
    }
}

impl RenderOnce for Heading {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let el = self.root(&theme);
        let id = self.common.id.clone();
        let label = self.common.text.clone();
        let level = self.accessibility_level.unwrap_or(self.level).number();
        let el = self.common.finish(el);
        match id {
            Some(id) => el
                .id(id)
                .role(Role::Heading)
                .aria_level(level)
                .aria_label(label)
                .into_any_element(),
            None => el.into_any_element(),
        }
    }
}
