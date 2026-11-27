//! SyntaxTheme examples: the gallery's Content page.

use gpui::{AnyElement, App, IntoElement, ParentElement, RenderOnce, Styled, Window, div, px};
use rustryx_syntax::{CodeLanguageInput, tokenize};
use rustryx_theme::ThemeStyled;

use super::{highlighted_line, syntax_colors, syntax_theme};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};

const SAMPLE: &str = "// Greets someone by name.\nexport function greet(name: string): string {\n  const count = 42;\n  return `Hello, ${name}!`;\n}";

/// The sample, painted with the syntax colors in scope.
#[derive(IntoElement)]
struct Snippet;

impl RenderOnce for Snippet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens().clone();
        let colors = syntax_colors(window, cx);
        let lines = tokenize(SAMPLE, &CodeLanguageInput::from("ts"));
        div()
            .p(t.spacing.s3)
            .rounded(t.radius.container)
            .bg(colors.background)
            .text_color(colors.variable)
            .font_stack(&t.font.family.code)
            .text_style(&t.text.code)
            .children(SAMPLE.lines().enumerate().map(|(i, line)| {
                let tokens = lines.get(i).cloned().unwrap_or_default();
                div().child(highlighted_line(line.to_string(), &tokens, &colors))
            }))
    }
}

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let labelled = |label: &'static str, el: AnyElement| {
        Stack::vertical()
            .gap(SpacingStep::S1)
            .w(px(420.))
            .child(Text::new(label).text_type(TextType::Supporting))
            .child(el)
    };
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(
            Text::new("SyntaxTheme: the theme's syntax colors, and two presets")
                .text_type(TextType::Supporting),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(16.))
                .child(labelled("Theme tokens", Snippet.into_any_element()))
                .child(labelled(
                    "Dracula",
                    syntax_theme(rustryx_syntax::dracula(), Snippet),
                ))
                .child(labelled(
                    "GitHub Light",
                    syntax_theme(rustryx_syntax::github_light(), Snippet),
                )),
        )
        .into_any_element()
}
