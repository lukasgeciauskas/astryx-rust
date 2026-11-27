//! Gallery pages. Each component milestone adds its pages here; a component
//! page renders the component's own `examples` module, the same code
//! `rustryx component <Name>` prints.

mod effects;
mod tokens;

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window};
use rustryx::lab::examples::Section;
use rustryx::theme::ResolvedTheme;

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
    ThreeD,
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
        Page::ThreeD,
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
            Page::ThreeD => "3D",
        }
    }

    /// The page's sections, when it shows one at a time: pages of many live examples, where
    /// one animating example would otherwise rebuild them all on every frame.
    pub fn sections(self) -> &'static [Section] {
        match self {
            Page::Lab => rustryx::lab::examples::SECTIONS,
            Page::ThreeD => rustryx::lab::three_d::examples::SECTIONS,
            _ => &[],
        }
    }

    pub fn render(self, theme: &ResolvedTheme, window: &mut Window, cx: &mut App) -> AnyElement {
        match self {
            Page::Tokens => tokens::render(theme, window, cx),
            Page::Effects => effects::render(theme, window, cx),
            Page::Text => rustryx::components::text::examples::examples(),
            Page::Layout => rustryx::components::layout::examples::examples(window, cx),
            Page::Icon => rustryx::components::icon::examples::examples(window, cx),
            Page::Status => rustryx::components::status::examples::examples(window, cx),
            Page::Content => gpui::div()
                .flex()
                .flex_col()
                .gap(gpui::px(48.))
                .child(rustryx::components::content::examples::examples(window, cx))
                .child(rustryx::components::syntax_theme::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::code_block::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::markdown::examples::examples(
                    window, cx,
                ))
                .into_any_element(),
            Page::People => rustryx::components::people::examples::examples(window, cx),
            Page::Containers => rustryx::components::containers::examples::examples(window, cx),
            Page::Feedback => rustryx::components::feedback::examples::examples(window, cx),
            Page::Media => rustryx::components::media::examples::examples(window, cx),
            Page::Time => rustryx::components::time::examples::examples(window, cx),
            Page::Buttons => rustryx::components::button::examples::examples(window, cx),
            Page::Links => rustryx::components::link::examples::examples(window, cx),
            Page::Choices => rustryx::components::choice::examples::examples(window, cx),
            Page::Inputs => rustryx::components::input::examples::examples(window, cx),
            Page::Overlays => rustryx::components::overlay::examples::examples(window, cx),
            Page::Navigation => rustryx::components::segmented::examples::examples(window, cx),
            Page::Wayfinding => rustryx::components::pagination::examples::examples(window, cx),
            Page::Tables => rustryx::components::table::examples::examples(window, cx),
            Page::Shell => gpui::div()
                .flex()
                .flex_col()
                .gap(gpui::px(48.))
                .child(rustryx::components::scrollable_area::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::resizable::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::layout::examples::regions_examples(
                    window, cx,
                ))
                .child(rustryx::components::nav::examples::examples(window, cx))
                .child(rustryx::components::side_nav::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::top_nav::examples::examples(window, cx))
                .child(rustryx::components::mobile_nav::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::app_shell::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::outline::examples::examples(window, cx))
                .child(rustryx::components::carousel::examples::examples(
                    window, cx,
                ))
                .into_any_element(),
            Page::Chat => rustryx::components::chat::examples::examples(window, cx),
            Page::Motion => rustryx::components::animator::examples::examples(window, cx),
            Page::Theme => rustryx::components::theming::examples::examples(window, cx),
            Page::Charts => rustryx::charts::examples::examples(window, cx),
            Page::Lab => rustryx::lab::examples::examples(window, cx),
            // The 3D charts turn by themselves and draw thousands of shapes: their own
            // page keeps the Lab page still.
            Page::ThreeD => rustryx::lab::three_d::examples::examples(window, cx),
            Page::Dates => gpui::div()
                .flex()
                .flex_col()
                .gap(gpui::px(48.))
                .child(rustryx::components::date_input::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::time_input::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::date_time_input::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::date_range_input::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::calendar::examples::examples(
                    window, cx,
                ))
                .into_any_element(),
            Page::Collections => gpui::div()
                .flex()
                .flex_col()
                .gap(gpui::px(48.))
                .child(rustryx::components::selector::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::list::examples::examples(window, cx))
                .child(rustryx::components::container_reveal::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::reorderable::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::metadata::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::overflow::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::typeahead::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::tokenizer::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::multi_selector::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::complex_selector::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::tree_list::examples::examples(
                    window, cx,
                ))
                .child(rustryx::components::power_search::examples::examples(
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
            let theme = rustryx::theme::current(window, cx);
            div().child(self.0.render(&theme, window, cx))
        }
    }

    /// Every page renders without panicking: a smoke test over every
    /// component example (debug assertions included).
    #[gpui::test]
    fn every_page_renders(cx: &mut TestAppContext) {
        cx.update(rustryx::init);
        for &page in Page::ALL {
            let (_, cx) = cx.add_window_view(move |_, _| PageView(page));
            cx.run_until_parked();
        }
    }
}
