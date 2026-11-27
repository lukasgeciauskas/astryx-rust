//! Layout examples: the gallery's Layout page and the `example` in each
//! layout component's doc.

use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, div, px};
use rustryx_theme::{ThemeStyled, current};

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

/// Layout and its regions: an inbox with a resizable navigation panel.
pub fn regions_examples(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::layout::{Layout, LayoutContent, LayoutFooter, LayoutHeader, LayoutPanel};
    use crate::resizable::{ResizeHandle, UseResizable, use_resizable};
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let nav = use_resizable(
        "layout-nav",
        UseResizable::new()
            .default_size(200.)
            .min_size(140.)
            .max_size(320.)
            .collapsible(true),
        window,
        cx,
    );
    let item = |text: String| {
        div()
            .py(t.spacing.s2)
            .border_b_1()
            .border_color(t.color.border)
            .text_style(&t.text.body)
            .text_color(t.color.text_primary)
            .child(text)
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            Text::new(
                "Layout: header, a resizable start panel, content, an end panel, footer. The panel slides shut and open; a pull past its limits bands the handle",
            )
                .text_type(TextType::Supporting),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .w(px(760.))
                .h(px(380.))
                .border_1()
                .border_color(t.color.border)
                .rounded(t.radius.container)
                .overflow_hidden()
                .child(
                    Layout::new()
                        .default_has_dividers(true)
                        .header(
                            LayoutHeader::new().child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(Text::new("Inbox").text_type(TextType::Label))
                                    .child({
                                        // The panel slides shut and open by its edge.
                                        let region = nav.clone();
                                        crate::button::Button::new(
                                            "layout-nav-toggle",
                                            if nav.is_collapsed {
                                                "Show folders"
                                            } else {
                                                "Hide folders"
                                            },
                                        )
                                        .on_click(move |_, window, cx| {
                                            if region.is_collapsed {
                                                region.expand(window, cx);
                                            } else {
                                                region.collapse(window, cx);
                                            }
                                        })
                                    }),
                            ),
                        )
                        .start(
                            div()
                                .flex()
                                .h_full()
                                .child(
                                    LayoutPanel::new("layout-nav-panel")
                                        .resizable(&nav)
                                        .label("Folders")
                                        .children(
                                            ["Inbox", "Drafts", "Sent", "Archive"]
                                                .into_iter()
                                                .map(|f| item(f.to_string())),
                                        ),
                                )
                                .child(
                                    ResizeHandle::new("layout-nav-handle")
                                        .resizable(&nav)
                                        .divider(true)
                                        .label("Resize folders"),
                                ),
                        )
                        .content(
                            LayoutContent::new("layout-messages")
                                .label("Messages")
                                .children((1..=30).map(|i| item(format!("Message {i}")))),
                        )
                        .end(
                            LayoutPanel::new("layout-details")
                                .width(px(200.))
                                .has_divider(true)
                                .child(Text::new("Details").text_type(TextType::Supporting)),
                        )
                        .footer(
                            LayoutFooter::new()
                                .child(Text::new("30 messages").text_type(TextType::Supporting)),
                        ),
                ),
        )
        .into_any_element()
}
