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
    Icon,
    Status,
    Content,
    People,
    Containers,
}

impl Page {
    pub const ALL: &'static [Page] = &[
        Page::Tokens,
        Page::Effects,
        Page::Text,
        Page::Layout,
        Page::Icon,
        Page::Status,
        Page::Content,
        Page::People,
        Page::Containers,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::Tokens => "Tokens",
            Page::Effects => "Effects",
            Page::Text => "Text",
            Page::Layout => "Layout",
            Page::Icon => "Icon",
            Page::Status => "Status",
            Page::Content => "Content",
            Page::People => "People",
            Page::Containers => "Containers",
        }
    }

    pub fn render(self, theme: &ResolvedTheme, window: &mut Window, cx: &mut App) -> AnyElement {
        match self {
            Page::Tokens => tokens::render(theme, window, cx),
            Page::Effects => effects::render(theme, window, cx),
            Page::Text => astryx::components::text::examples::examples(),
            Page::Layout => astryx::components::layout::examples::examples(window, cx),
            Page::Icon => astryx::components::icon::examples::examples(window, cx),
            Page::Status => astryx::components::status::examples::examples(window, cx),
            Page::Content => astryx::components::content::examples::examples(window, cx),
            Page::People => astryx::components::people::examples::examples(window, cx),
            Page::Containers => astryx::components::containers::examples::examples(window, cx),
        }
    }
}
