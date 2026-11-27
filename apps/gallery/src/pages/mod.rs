//! Gallery pages. Each component milestone adds its pages here; a component
//! page renders the component's own `examples` module, the same code
//! `astryx component <Name>` prints.

mod effects;
mod tokens;

use astryx::theme::ResolvedTheme;
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window};

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
    Collections,
    Tables,
    Dates,
    Shell,
    Chat,
    Motion,
    Theme,
    Charts,
    Lab,
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
        Page::Collections,
        Page::Tables,
        Page::Dates,
        Page::Shell,
        Page::Chat,
        Page::Motion,
        Page::Theme,
        Page::Charts,
        Page::Lab,
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
            Page::Collections => "Collections",
            Page::Tables => "Tables",
            Page::Dates => "Dates",
            Page::Shell => "Shell",
            Page::Chat => "Chat",
            Page::Motion => "Motion",
            Page::Theme => "Theme",
            Page::Charts => "Charts",
            Page::Lab => "Lab",
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
            Page::Content => gpui::div()
                .flex()
                .flex_col()
                .gap(gpui::px(48.))
                .child(astryx::components::content::examples::examples(window, cx))
                .child(astryx::components::syntax_theme::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::code_block::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::markdown::examples::examples(window, cx))
                .into_any_element(),
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
            Page::Tables => astryx::components::table::examples::examples(window, cx),
            Page::Shell => gpui::div()
                .flex()
                .flex_col()
                .gap(gpui::px(48.))
                .child(astryx::components::scrollable_area::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::resizable::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::layout::examples::regions_examples(
                    window, cx,
                ))
                .child(astryx::components::nav::examples::examples(window, cx))
                .child(astryx::components::side_nav::examples::examples(window, cx))
                .child(astryx::components::top_nav::examples::examples(window, cx))
                .child(astryx::components::mobile_nav::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::app_shell::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::outline::examples::examples(window, cx))
                .child(astryx::components::carousel::examples::examples(window, cx))
                .into_any_element(),
            Page::Chat => astryx::components::chat::examples::examples(window, cx),
            Page::Motion => astryx::components::animator::examples::examples(window, cx),
            Page::Theme => astryx::components::theming::examples::examples(window, cx),
            Page::Charts => astryx::charts::examples::examples(window, cx),
            Page::Lab => astryx::lab::examples::examples(window, cx),
            Page::Dates => gpui::div()
                .flex()
                .flex_col()
                .gap(gpui::px(48.))
                .child(astryx::components::date_input::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::time_input::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::date_time_input::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::date_range_input::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::calendar::examples::examples(window, cx))
                .into_any_element(),
            Page::Collections => gpui::div()
                .flex()
                .flex_col()
                .gap(gpui::px(48.))
                .child(astryx::components::selector::examples::examples(window, cx))
                .child(astryx::components::list::examples::examples(window, cx))
                .child(astryx::components::container_reveal::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::reorderable::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::metadata::examples::examples(window, cx))
                .child(astryx::components::overflow::examples::examples(window, cx))
                .child(astryx::components::typeahead::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::tokenizer::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::multi_selector::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::complex_selector::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::tree_list::examples::examples(
                    window, cx,
                ))
                .child(astryx::components::power_search::examples::examples(
                    window, cx,
                ))
                .into_any_element(),
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
