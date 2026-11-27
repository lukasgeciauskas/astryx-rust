//! Text and Heading examples. The gallery renders these, and
//! `rustryx component Text` prints this file.

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px};

use crate::text::*;

pub fn examples() -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        // Headings, levels 1 to 6.
        .child(Heading::new(HeadingLevel::H1, "Heading 1").id("example-h1"))
        .child(Heading::new(HeadingLevel::H3, "Heading 3"))
        .child(Heading::new(HeadingLevel::H6, "Heading 6"))
        .child(
            Heading::new(HeadingLevel::H2, "A display heading").display_type(DisplayType::Display2),
        )
        // Semantic text types.
        .child(Text::new("Body text: the default type for paragraphs."))
        .child(Text::new("Large text for emphasis.").text_type(TextType::Large))
        .child(Text::new("Label text for form fields.").text_type(TextType::Label))
        .child(Text::new("Supporting text: secondary by default.").text_type(TextType::Supporting))
        .child(Text::new("let code = \"monospace\";").text_type(TextType::Code))
        // Colors and weights.
        .child(
            Text::new("Accent and bold.")
                .color(TextColor::Accent)
                .weight(TextWeight::Bold),
        )
        .child(Text::new("Disabled text.").color(TextColor::Disabled))
        .child(Text::new("Struck through.").strikethrough(true))
        // Truncation: one line, then two.
        .child(
            div().w(px(280.)).child(
                Text::new(
                    "A long line that is cut with an ellipsis when it does not fit its container.",
                )
                .max_lines(1),
            ),
        )
        .child(
            div().w(px(280.)).child(
                Text::new(
                    "A longer paragraph that is clamped to two lines. Everything after the \
                     second line is cut, and an ellipsis shows that more text follows.",
                )
                .max_lines(2),
            ),
        )
        // Tabular numbers keep digits aligned in columns.
        .child(Text::new("1,111.11  9,999.99").tabular_numbers(true))
        // Inline content: spans, links and inline boxes share the lines.
        .child(
            div().w(px(320.)).child(
                Text::new("Your trial ends on ")
                    .child(Text::new("March 3").weight(TextWeight::Semibold))
                    .child(". Read the ")
                    .child(
                        crate::link::Link::new(
                            "inline-terms",
                            "subscription terms and the cancellation policy",
                        )
                        .href("https://example.com/terms"),
                    )
                    .child(" before you upgrade, or ")
                    .child(Text::new("keep the free plan").color(TextColor::Secondary))
                    .child(" ")
                    .child(crate::status::Badge::new("Free"))
                    .child(" with its limits."),
            ),
        )
        .into_any_element()
}
