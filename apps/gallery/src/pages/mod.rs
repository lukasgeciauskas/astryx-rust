//! Gallery pages. Each component milestone adds its pages here; a component
//! page renders the component's own `examples` module, the same code
//! `astryx component <Name>` prints.

mod effects;
mod tokens;

use astryx::theme::ResolvedTheme;
use gpui::{AnyElement, App, Window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Tokens,
    Effects,
    Text,
    Layout,
}

impl Page {
    pub const ALL: &'static [Page] = &[Page::Tokens, Page::Effects, Page::Text, Page::Layout];

    pub fn title(self) -> &'static str {
        match self {
            Page::Tokens => "Tokens",
            Page::Effects => "Effects",
            Page::Text => "Text",
            Page::Layout => "Layout",
        }
    }

    pub fn render(self, theme: &ResolvedTheme, window: &mut Window, cx: &mut App) -> AnyElement {
        match self {
            Page::Tokens => tokens::render(theme, window, cx),
            Page::Effects => effects::render(theme, window, cx),
            Page::Text => astryx::components::text::examples::examples(),
            Page::Layout => astryx::components::layout::examples::examples(window, cx),
        }
    }
}
