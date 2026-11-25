//! Text: semantic body text with theme-driven type styles.
//! Upstream: `packages/core/src/Text/Text.tsx`.

use std::sync::Arc;

use astryx_theme::{ResolvedTheme, ThemeStyled, current};
use gpui::{
    AnyElement, App, Div, ElementId, FontFeatures, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, Refineable, RenderOnce, SharedString, StyleRefinement, Styled, Window,
    div, px,
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
/// Upstream `display`: an inline Text flows with the text around it inside a
/// paragraph (a Text with children); a block one takes a line of its own.
/// A line limit makes a Text a block, as upstream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDisplay {
    #[default]
    Inline,
    Block,
}

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
    pub display: TextDisplay,
    pub capsize: bool,
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
            display: TextDisplay::Inline,
            capsize: false,
            style: StyleRefinement::default(),
        }
    }

    /// Whether it joins an enclosing paragraph's lines: inline, with no line
    /// limit (upstream forces a limited Text to a block).
    pub fn flows_inline(&self) -> bool {
        self.display == TextDisplay::Inline && self.max_lines == 0 && !self.capsize
    }

    /// Upstream `hasCapsize` (`text-box: trim-both cap alphabetic`): how much
    /// of the first line's box above the cap height, and of the last line's
    /// below the baseline, `root`'s text trims.
    pub fn text_box_trim(&self, root: &mut Div, window: &Window) -> Option<(Pixels, Pixels)> {
        if !self.capsize {
            return None;
        }
        let mut style = window.text_style();
        style.refine(&root.style().text);
        let rem = window.rem_size();
        let font_size = style.font_size.to_pixels(rem);
        let line_height = style.line_height_in_pixels(rem);
        let text_system = window.text_system();
        let font = text_system.resolve_font(&style.font());
        let baseline = text_system.baseline_offset(font, font_size, line_height);
        let cap = text_system.cap_height(font, font_size);
        Some((
            (baseline - cap).max(px(0.)),
            (line_height - baseline).max(px(0.)),
        ))
    }

    /// Lays the text and children out in an enclosing paragraph, in `root`'s
    /// text style.
    pub fn inline(
        self,
        mut root: Div,
        flow: &mut super::inline::InlineBuilder,
        window: &mut Window,
        cx: &mut App,
    ) {
        let refinement = root.style().text.clone();
        flow.styled(&refinement, |flow| {
            flow.text(self.text);
            for child in self.children {
                flow.child(child, window, cx);
            }
        });
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
            // Upstream: a capsize block drops a multi-line clamp.
            _ if self.capsize => el,
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

    /// Upstream `hasTruncateTooltip`: for text with a line limit, a probe
    /// that measures each frame whether the text overflows the limit. Returns
    /// the element with the probe and what it measured last.
    pub fn measure_truncation(
        &self,
        el: Div,
        window: &mut Window,
        cx: &mut App,
    ) -> (Div, Option<(ElementId, gpui::Entity<bool>)>) {
        if self.max_lines == 0
            || self.truncate_tooltip == TruncateTooltip::Off
            || self.text.is_empty()
        {
            return (el, None);
        }
        let base = format!(
            "{}-{}",
            self.id
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_default(),
            self.text
        );
        let truncated = window.use_keyed_state(
            ElementId::Name(format!("{base}-truncated").into()),
            cx,
            |_, _| false,
        );
        let (text, lines, state) = (self.text.clone(), self.max_lines, truncated.clone());
        let probe = gpui::canvas(
            move |bounds, window, cx| {
                let style = window.text_style();
                let rem = window.rem_size();
                let font_size = style.font_size.to_pixels(rem);
                let line_height = style.line_height_in_pixels(rem);
                let runs = [style.to_run(text.len())];
                let wrap = (lines > 1).then_some(bounds.size.width);
                let shaped = window
                    .text_system()
                    .shape_text(text.clone(), font_size, &runs, wrap, None)
                    .unwrap_or_default();
                let over = if lines == 1 {
                    shaped.iter().any(|line| {
                        line.size(line_height).width > bounds.size.width + gpui::px(0.5)
                    })
                } else {
                    shaped
                        .iter()
                        .map(|line| line.wrap_boundaries.len() + 1)
                        .sum::<usize>()
                        > lines
                };
                if *state.read(cx) != over {
                    state.update(cx, |v, _| *v = over);
                    window.on_next_frame(|window, _| window.refresh());
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        (
            el.relative().child(probe),
            Some((
                ElementId::Name(format!("{base}-truncate-tooltip").into()),
                truncated,
            )),
        )
    }

    /// The tooltip with the full text while it is truncated.
    pub fn truncation_tooltip(
        text: SharedString,
        placement: TruncateTooltip,
        measured: Option<(ElementId, gpui::Entity<bool>)>,
        el: AnyElement,
        cx: &App,
    ) -> AnyElement {
        let Some((id, truncated)) = measured else {
            return el;
        };
        let placement = match placement {
            TruncateTooltip::Below => astryx_primitives::Placement::Below,
            TruncateTooltip::Start => astryx_primitives::Placement::Start,
            TruncateTooltip::End => astryx_primitives::Placement::End,
            _ => astryx_primitives::Placement::Above,
        };
        crate::overlay::Tooltip::new(id, text)
            .enabled(*truncated.read(cx))
            .placement(placement)
            .child(el)
            .into_any_element()
    }

    /// Adds the content: the text alone, or a paragraph whose children flow
    /// inline with it; trimmed to `trim` (see [`Self::text_box_trim`]).
    pub fn finish(self, el: Div, trim: Option<(Pixels, Pixels)>) -> Div {
        let content = if self.children.is_empty() {
            self.text.into_any_element()
        } else {
            let text = (!self.text.is_empty()).then(|| self.text.into_any_element());
            super::inline::InlineFlow::new(text.into_iter().chain(self.children)).into_any_element()
        };
        match trim {
            // The lines keep their height; the box around them loses the
            // trimmed edges.
            Some((top, bottom)) => el.child(div().mt(-top).mb(-bottom).child(content)),
            None => el.child(content),
        }
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

    /// Upstream `hasTruncateTooltip`: the full text in a tooltip while it is cut.
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

    /// Upstream `display`: inline (the default) or block.
    pub fn display(mut self, display: TextDisplay) -> Self {
        self.common.display = display;
        self
    }

    /// Upstream `hasCapsize`: trims the box to the text's cap height and
    /// baseline, for optical alignment. A capsize Text is a block.
    pub fn capsize(mut self, capsize: bool) -> Self {
        self.common.capsize = capsize;
        self
    }

    /// Whether it joins an enclosing paragraph's lines.
    pub(crate) fn flows_inline(&self) -> bool {
        self.common.flows_inline()
    }

    /// Lays itself out inside an enclosing paragraph.
    pub(crate) fn inline(
        self,
        flow: &mut super::inline::InlineBuilder,
        window: &mut Window,
        cx: &mut App,
    ) {
        let theme = current(window, cx);
        let root = self.root(&theme);
        self.common.inline(root, flow, window, cx);
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
        let mut el = self.root(&theme);
        let trim = self.common.text_box_trim(&mut el, window);
        let (el, measured) = self.common.measure_truncation(el, window, cx);
        let (text, placement) = (self.common.text.clone(), self.common.truncate_tooltip);
        let id = self.common.id.clone();
        let el = self.common.finish(el, trim);
        let el = match id {
            Some(id) => el.id(id).into_any_element(),
            None => el.into_any_element(),
        };
        Common::truncation_tooltip(text, placement, measured, el, cx)
    }
}
