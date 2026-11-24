//! Gallery pages. Each component milestone adds its pages here.

mod tokens;

use astryx::theme::ResolvedTheme;
use gpui::{AnyElement, App, Window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Tokens,
}

impl Page {
    pub const ALL: &'static [Page] = &[Page::Tokens];

    pub fn title(self) -> &'static str {
        match self {
            Page::Tokens => "Tokens",
        }
    }

    pub fn render(self, theme: &ResolvedTheme, window: &mut Window, cx: &mut App) -> AnyElement {
        match self {
            Page::Tokens => tokens::render(theme, window, cx),
        }
    }
}
