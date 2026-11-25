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
    Feedback,
    Media,
    Time,
    Buttons,
    Links,
    Choices,
    Inputs,
    Overlays,
    Navigation,
    Wayfinding,
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
        Page::Feedback,
        Page::Media,
        Page::Time,
        Page::Buttons,
        Page::Links,
        Page::Choices,
        Page::Inputs,
        Page::Overlays,
        Page::Navigation,
        Page::Wayfinding,
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
            Page::Feedback => "Feedback",
            Page::Media => "Media",
            Page::Time => "Time",
            Page::Buttons => "Buttons",
            Page::Links => "Links",
            Page::Choices => "Choices",
            Page::Inputs => "Inputs",
            Page::Overlays => "Overlays",
            Page::Navigation => "Navigation",
            Page::Wayfinding => "Wayfinding",
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
            Page::Feedback => astryx::components::feedback::examples::examples(window, cx),
            Page::Media => astryx::components::media::examples::examples(window, cx),
            Page::Time => astryx::components::time::examples::examples(window, cx),
            Page::Buttons => astryx::components::button::examples::examples(window, cx),
            Page::Links => astryx::components::link::examples::examples(window, cx),
            Page::Choices => astryx::components::choice::examples::examples(window, cx),
            Page::Inputs => astryx::components::input::examples::examples(window, cx),
            Page::Overlays => astryx::components::overlay::examples::examples(window, cx),
            Page::Navigation => astryx::components::segmented::examples::examples(window, cx),
            Page::Wayfinding => astryx::components::pagination::examples::examples(window, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Context, IntoElement, ParentElement, Render, TestAppContext, Window, div};

    use super::Page;

    struct PageView(Page);

    impl Render for PageView {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = astryx::theme::current(window, cx);
            div().child(self.0.render(&theme, window, cx))
        }
    }

    /// Every page renders without panicking: a smoke test over every
    /// component example (debug assertions included).
    #[gpui::test]
    fn every_page_renders(cx: &mut TestAppContext) {
        cx.update(astryx::init);
        for &page in Page::ALL {
            let (_, cx) = cx.add_window_view(move |_, _| PageView(page));
            cx.run_until_parked();
        }
    }
}
