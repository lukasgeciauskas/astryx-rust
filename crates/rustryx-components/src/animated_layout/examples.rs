//! Animated layout examples for the gallery.

use gpui::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement as _, Styled, Window, div, px,
};

use super::{AnimatedLayout, use_animated_layout, use_edge_reveal};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(
            Text::new(
                "useAnimatedLayout: open a section; it reveals from its edge and everything after it glides on the theme's move curve",
            )
            .text_type(TextType::Supporting),
        )
        .child(Sections { id: "animated-layout-demo".into() })
        .into_any_element()
}

#[derive(IntoElement)]
struct Sections {
    id: ElementId,
}

impl RenderOnce for Sections {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let layout = use_animated_layout(self.id.clone(), window, cx);
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let (surface, border) = (t.color.background_surface, t.color.border);
        let open = window.use_keyed_state(
            ElementId::Name(format!("{}-open", self.id).into()),
            cx,
            |_, _| [false; 3],
        );
        let flags = *open.read(cx);
        let mut area = div().flex().flex_col().gap(px(8.)).w(px(520.));
        for (i, is_open) in flags.into_iter().enumerate() {
            area = area.child(section(
                &layout,
                &self.id,
                i,
                is_open,
                open.clone(),
                surface,
                border,
                window,
                cx,
            ));
        }
        layout.container(
            area.child(Text::new(
                "A paragraph after the sections glides with them, without opting in.",
            )),
            window,
            cx,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn section(
    layout: &AnimatedLayout,
    id: &ElementId,
    i: usize,
    is_open: bool,
    open: gpui::Entity<[bool; 3]>,
    surface: gpui::Hsla,
    border: gpui::Hsla,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let reveal = use_edge_reveal(
        ElementId::Name(format!("{id}-section-{i}").into()),
        is_open,
        Some(layout),
        window,
        cx,
    );
    div()
        .flex()
        .flex_col()
        .border_1()
        .border_color(border)
        .child(
            div()
                .id(ElementId::Name(format!("{id}-toggle-{i}").into()))
                .p(px(8.))
                .cursor_pointer()
                .child(Text::new(format!(
                    "Section {} ({})",
                    i + 1,
                    if is_open { "close" } else { "open" }
                )))
                .on_click(move |_, _, cx| {
                    open.update(cx, |o, _| o[i] = !o[i]);
                }),
        )
        .children(
            reveal.wrap(
                div()
                    .p(px(8.))
                    .bg(surface)
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(Text::new(
                        "Its content slides in under a still clip at its box.",
                    ))
                    .child(Text::new("A second line, so the section has some height."))
                    .child(Text::new("And a third.")),
            ),
        )
}
