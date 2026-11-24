//! Layout examples: the gallery's Layout page and the `example` in each
//! layout component's doc.

use astryx_theme::{ThemeStyled, current};
use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, div, px};

use crate::layout::{
    AspectRatio, AspectShape, Center, Divider, DividerVariant, Grid, GridColumns, GridSpan,
    Orientation, SpacingStep, Stack, StackAlign, StackItem, StackItemSize, VisuallyHidden,
};
use crate::text::{Text, TextType};

/// A tinted box standing in for content.
fn tile(label: impl Into<SharedString>, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    div()
        .p(t.spacing.s3)
        .bg(t.color.background_muted)
        .border_1()
        .border_color(t.color.border)
        .rounded(t.radius.element)
        .text_style(&t.text.label)
        .text_color(t.color.text_primary)
        .child(label.into())
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let tiles = |n: usize, window: &mut Window, cx: &mut App| -> Vec<AnyElement> {
        (1..=n)
            .map(|i| tile(format!("Item {i}"), window, cx).into_any_element())
            .collect()
    };

    Stack::vertical()
        .gap(SpacingStep::S6)
        .child(label("HStack, gap 2, centered vertically"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .v_align(StackAlign::Center)
                .children(tiles(3, window, cx)),
        )
        .child(label("HStack with a filling item and space between"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .child(tile("Start", window, cx))
                .child(
                    StackItem::new()
                        .size(StackItemSize::Fill)
                        .child(tile("Fills", window, cx)),
                )
                .child(tile("End", window, cx)),
        )
        .child(label("VStack, end-aligned, padded"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S1)
                .h_align(StackAlign::End)
                .padding(SpacingStep::S3)
                .children(tiles(2, window, cx)),
        )
        .child(label("Center, 80px tall"))
        .child(
            Center::new()
                .height(px(80.))
                .child(tile("Centered", window, cx)),
        )
        .child(label("Grid, 3 columns, with a two-column span"))
        .child(
            Grid::new(3u16)
                .gap(SpacingStep::S2)
                .children(tiles(3, window, cx))
                .span(
                    GridSpan::new()
                        .columns(2u16)
                        .child(tile("Spans two", window, cx)),
                )
                .child(tile("Item 5", window, cx)),
        )
        .child(label(
            "Responsive grid: columns of at least 160px, at most 4",
        ))
        .child(
            Grid::new(GridColumns::Responsive {
                min_width: px(160.),
                max: Some(4),
                repeat: crate::layout::GridRepeat::Fill,
            })
            .gap(SpacingStep::S2)
            .children(tiles(6, window, cx)),
        )
        .child(label("Dividers"))
        .child(Divider::new())
        .child(
            Divider::new()
                .variant(DividerVariant::Strong)
                .label("Strong, labeled"),
        )
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .height(px(40.))
                .child(tile("Left", window, cx))
                .child(Divider::new().orientation(Orientation::Vertical))
                .child(tile("Right", window, cx)),
        )
        .child(label("AspectRatio 16:9, and an ellipse at 1:1"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(
                    div().w(px(240.)).child(
                        AspectRatio::new(16. / 9.)
                            .bg(current(window, cx).tokens().color.background_muted)
                            .child(
                                Center::new()
                                    .width(gpui::relative(1.))
                                    .height(gpui::relative(1.))
                                    .child(tile("16:9", window, cx)),
                            ),
                    ),
                )
                .child(
                    div().w(px(96.)).child(
                        AspectRatio::new(1.)
                            .shape(AspectShape::Ellipse)
                            .bg(current(window, cx).tokens().color.background_blue),
                    ),
                ),
        )
        .child(label("VisuallyHidden: read by screen readers, not shown"))
        .child(
            div()
                .relative()
                .child(tile("Visible text", window, cx))
                .child(VisuallyHidden::new().child("Hidden description")),
        )
        .child(label(
            "Inline flow: inline components in a plain block wrap like words",
        ))
        .child(inline_flow(window, cx))
        .into_any_element()
}

/// Buttons, badges, tokens and a link in a narrow block container: they flow
/// side by side on one baseline and wrap (fork patch R5).
fn inline_flow(window: &mut Window, cx: &mut App) -> impl IntoElement {
    use crate::button::{Button, ButtonVariant};
    use crate::link::Link;
    use crate::status::Badge;
    use crate::token::{Token, TokenColor};
    let theme = current(window, cx);
    let t = theme.tokens();
    let gap = |el: AnyElement| {
        div()
            .inline_flex()
            .mr(t.spacing.s2)
            .mb(t.spacing.s2)
            .child(el)
    };
    div()
        .w(px(360.))
        .p(t.spacing.s3)
        .border_1()
        .border_color(t.color.border)
        .rounded(t.radius.element)
        .child(gap(Button::new("inline-save", "Save")
            .variant(ButtonVariant::Primary)
            .into_any_element()))
        .child(gap(Badge::new("New").into_any_element()))
        .child(gap(
            Button::new("inline-cancel", "Cancel").into_any_element()
        ))
        .child(gap(Token::new("inline-token", "Design")
            .color(TokenColor::Blue)
            .into_any_element()))
        .child(gap(Link::new("inline-link", "Learn more")
            .href("https://example.com")
            .into_any_element()))
        .child(gap(Badge::new("12").into_any_element()))
}
