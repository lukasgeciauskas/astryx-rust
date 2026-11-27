//! ChatTokenizedText, upstream `Chat/ChatTokenizedText.tsx`: a sent message's text with
//! its serialized token values drawn as the chips the composer showed.

use gpui::Refineable as _;
use gpui::{
    AnyElement, App, IntoElement, ParentElement as _, RenderOnce, SharedString, StyleRefinement,
    Styled, TextStyleRefinement, Window, div,
};
use rustryx_theme::current;

use super::composer_token::{ChatComposerToken, ChatComposerTokenElement};
use crate::text::InlineSpan;

/// A piece of the text: kept as written, or the token at an index of the token list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Segment {
    Text(String),
    Token(usize),
}

/// Splits `text` at literal occurrences of the non-empty `values` (FR1, FR2), as
/// upstream's escaped, global alternation does: scanning left to right, the first value
/// in list order that matches at the earliest position wins, and the scan resumes after
/// it. A matched value names the last token listed with it (upstream's value map), so
/// of two tokens sharing a value the later one renders (OQ1).
pub(crate) fn split<S: AsRef<str>>(text: &str, values: &[S]) -> Vec<Segment> {
    let candidates: Vec<(usize, &str)> = values
        .iter()
        .map(AsRef::as_ref)
        .enumerate()
        .filter(|(_, v)| !v.is_empty())
        .collect();
    if text.is_empty() {
        return Vec::new();
    }
    if candidates.is_empty() {
        return vec![Segment::Text(text.to_string())];
    }
    let mut out = Vec::new();
    let (mut last, mut pos) = (0, 0);
    while pos < text.len() {
        let rest = &text[pos..];
        match candidates.iter().find(|(_, v)| rest.starts_with(v)) {
            Some((_, value)) => {
                if pos > last {
                    out.push(Segment::Text(text[last..pos].to_string()));
                }
                let index = candidates
                    .iter()
                    .rev()
                    .find(|(_, v)| v == value)
                    .map_or(0, |(i, _)| *i);
                out.push(Segment::Token(index));
                // Never zero-width: empty values were dropped (PR1).
                pos += value.len();
                last = pos;
            }
            None => pos += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    if last < text.len() {
        out.push(Segment::Text(text[last..].to_string()));
    }
    out
}

/// Upstream `ChatTokenizedText`: text whose token values render as inline tokens.
/// Render-only: it keeps no state (FR5).
///
/// It flows as a paragraph of its own, in the text style around it. Inside a Text
/// paragraph it is one inline box (the paragraph's line breaking does not reach into
/// it).
///
/// ```ignore
/// ChatTokenizedText::new(message.text.clone()).tokens(mention_tokens.clone())
/// ```
#[derive(IntoElement)]
pub struct ChatTokenizedText {
    text: SharedString,
    tokens: Vec<ChatComposerToken>,
    style: StyleRefinement,
}

impl ChatTokenizedText {
    /// Upstream `children`: the message text holding serialized token values.
    pub fn new(text: impl Into<SharedString>) -> Self {
        ChatTokenizedText {
            text: text.into(),
            tokens: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `tokens`: the same definitions a composer's triggers produce. Each
    /// non-empty value is matched literally; empty values are ignored.
    pub fn tokens(mut self, tokens: impl IntoIterator<Item = ChatComposerToken>) -> Self {
        self.tokens = tokens.into_iter().collect();
        self
    }

    /// The text's segments for its tokens.
    pub(crate) fn segments(&self) -> Vec<Segment> {
        let values: Vec<&str> = self.tokens.iter().map(|t| t.value().as_ref()).collect();
        split(&self.text, &values)
    }
}

impl Styled for ChatTokenizedText {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ChatTokenizedText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        // Upstream's root is a plain inline span: it inherits the surrounding text style.
        let mut root = div();
        theme.refine(root.style(), "chat-tokenized-text", &[], None);
        root.style().refine(&self.style);
        let parts: Vec<AnyElement> = self
            .segments()
            .into_iter()
            .map(|segment| match segment {
                Segment::Text(text) => SharedString::from(text).into_any_element(),
                // A custom token's render runs only for a match (FR3).
                Segment::Token(index) => {
                    ChatComposerTokenElement::new(self.tokens[index].clone()).into_any_element()
                }
            })
            .collect();
        if parts.is_empty() {
            return root;
        }
        root.child(InlineSpan::new(TextStyleRefinement::default()).children(parts))
    }
}
