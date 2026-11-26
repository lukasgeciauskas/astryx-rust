//! Resizable examples: the gallery's Shell page.

use astryx_theme::{ThemeStyled, current};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use crate::layout::{SpacingStep, Stack};
use crate::resizable::{
    ResizableSize, ResizeDirection, ResizeHandle, UseResizable, use_resizable, use_resizable_basis,
};
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let pane = |name: &'static str| {
        div()
            .size_full()
            .p(t.spacing.s3)
            .text_style(&t.text.body)
            .text_color(t.color.text_secondary)
            .child(name)
    };
    let sidebar = use_resizable(
        "example-sidebar",
        UseResizable::new()
            .default_size(220.)
            .min_size(160.)
            .max_size(400.)
            .snaps([220., 320.])
            .collapsible(true)
            .auto_save_id("gallery-sidebar"),
        window,
        cx,
    );
    let editor = use_resizable_basis("example-editor", window, cx);
    let terminal = use_resizable(
        "example-terminal",
        UseResizable::new()
            .direction(ResizeDirection::Vertical)
            .default_size(ResizableSize::percent(35.))
            .min_size(60.)
            .max_size(ResizableSize::percent(70.))
            .container(editor.clone()),
        window,
        cx,
    );
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            Text::new(
                "Resizable: drag a handle, or focus it and use the arrows, Home, End and Enter",
            )
            .text_type(TextType::Supporting),
        )
        .child(
            div()
                .flex()
                .w(px(640.))
                .h(px(320.))
                .border_1()
                .border_color(t.color.border)
                .rounded(t.radius.container)
                .overflow_hidden()
                .child(
                    div()
                        .w(px(sidebar.size))
                        .flex_none()
                        .overflow_hidden()
                        .child(pane("Sidebar")),
                )
                .child(
                    ResizeHandle::new("sidebar-handle")
                        .resizable(&sidebar)
                        .divider(true)
                        .label("Resize sidebar"),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(editor.probe())
                        .child(div().flex_1().min_h_0().child(pane("Editor")))
                        .child(
                            ResizeHandle::new("terminal-handle")
                                .direction(ResizeDirection::Vertical)
                                .resizable(&terminal)
                                .reversed(true)
                                .divider(true)
                                .label("Resize terminal"),
                        )
                        .child(
                            div()
                                .h(px(terminal.size))
                                .flex_none()
                                .overflow_hidden()
                                .child(pane("Terminal")),
                        ),
                ),
        )
        .into_any_element()
}
