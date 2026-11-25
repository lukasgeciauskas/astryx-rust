//! The gallery shell: a toolbar to switch theme and mode, a page list, and
//! the selected page.

use std::sync::Arc;

use astryx::prelude::*;
use astryx::theme::{self, ResolvedTheme, Theme};
use gpui::{
    AnyElement, Context, ElementId, Hsla, ScrollHandle, SharedString, Window, div, point,
    prelude::*, px,
};

use crate::dev_state::DevState;
use crate::pages::Page;

/// Core's defaults plus every theme compiled into the gallery.
pub fn available_themes() -> Vec<Arc<Theme>> {
    let mut themes = vec![Arc::new(Theme::default())];
    themes.extend(astryx::themes::all().into_iter().map(Arc::new));
    themes
}

pub struct Gallery {
    themes: Vec<Arc<Theme>>,
    page: Page,
    scroll: ScrollHandle,
    restore_scroll: Option<f32>,
    saved: Option<DevState>,
}

impl Gallery {
    /// `page` names the start page (case-insensitive); `scroll_y` restores
    /// the page's scroll after a `cargo xtask dev` relaunch.
    pub fn new(themes: Vec<Arc<Theme>>, page: Option<&str>, scroll_y: f32) -> Self {
        Gallery {
            themes,
            page: page
                .and_then(|name| {
                    Page::ALL
                        .iter()
                        .copied()
                        .find(|p| p.title().eq_ignore_ascii_case(name))
                })
                .unwrap_or(Page::Tokens),
            scroll: ScrollHandle::new(),
            restore_scroll: (scroll_y > 0.0).then_some(scroll_y),
            saved: None,
        }
    }

    /// Saves where the gallery is when it changed, for `cargo xtask dev`.
    fn save_state(&mut self, theme: &str, mode: ThemeMode, window: &Window) {
        let b = window.bounds();
        let state = DevState {
            page: self.page.title().to_string(),
            theme: theme.to_string(),
            mode: match mode {
                ThemeMode::System => "system",
                ThemeMode::Light => "light",
                ThemeMode::Dark => "dark",
            }
            .to_string(),
            bounds: Some([
                f32::from(b.origin.x),
                f32::from(b.origin.y),
                f32::from(b.size.width),
                f32::from(b.size.height),
            ]),
            scroll_y: -f32::from(self.scroll.offset().y),
        };
        if self.saved.as_ref() != Some(&state) {
            state.save();
            self.saved = Some(state);
        }
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let resolved = theme::current(window, cx);
        let t = resolved.tokens();
        let (active, mode) = theme::active(cx);
        if let Some(y) = self.restore_scroll.take() {
            self.scroll.set_offset(point(px(0.), px(-y)));
        }
        self.save_state(&active.name, mode, window);

        let theme_chips = self.themes.iter().map(|candidate| {
            let selected = candidate.name == active.name;
            let candidate = candidate.clone();
            chip(
                ElementId::Name(format!("theme-{}", candidate.name).into()),
                candidate.name.to_string().into(),
                selected,
                &resolved,
            )
            .on_click(move |_, _, cx| theme::set_theme(candidate.clone(), cx))
        });
        let mode_chips = [
            (ThemeMode::System, "System"),
            (ThemeMode::Light, "Light"),
            (ThemeMode::Dark, "Dark"),
        ]
        .map(|(value, label)| {
            chip(
                ElementId::Name(format!("mode-{label}").into()),
                label.into(),
                mode == value,
                &resolved,
            )
            .on_click(move |_, _, cx| theme::set_mode(value, cx))
        });

        let pages = Page::ALL.iter().map(|&page| {
            let selected = page == self.page;
            div()
                .id(ElementId::Name(format!("page-{}", page.title()).into()))
                .px(t.spacing.s3)
                .py(t.spacing.s2)
                .rounded(t.radius.element)
                .when(selected, |d| {
                    d.bg(t.color.background_muted)
                        .text_color(t.color.text_primary)
                })
                .when(!selected, |d| d.text_color(t.color.text_secondary))
                .hover(|d| d.bg(t.color.overlay_hover))
                .cursor_pointer()
                .child(page.title())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.page = page;
                    cx.notify();
                }))
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.color.background_body)
            .text_color(t.color.text_primary)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(t.spacing.s2)
                    .px(t.spacing.s4)
                    .py(t.spacing.s3)
                    .border_b_1()
                    .border_color(t.color.border)
                    .bg(t.color.background_surface)
                    .child(div().text_style(&t.text.label).child("Theme"))
                    .children(theme_chips)
                    .child(div().w(px(16.)))
                    .child(div().text_style(&t.text.label).child("Mode"))
                    .children(mode_chips),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .w(px(200.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(t.spacing.s1)
                            .p(t.spacing.s3)
                            .border_r_1()
                            .border_color(t.color.border)
                            .bg(t.color.background_surface)
                            .children(pages),
                    )
                    .child(
                        div()
                            .id("page")
                            .track_scroll(&self.scroll)
                            .flex_1()
                            .min_w_0()
                            .overflow_y_scroll()
                            .p(t.spacing.s8)
                            .child(self.page.render(&resolved, window, cx)),
                    ),
            )
            // Toasts shown from any page.
            .child(astryx::components::overlay::ToastViewport::new())
    }
}

fn chip(
    id: ElementId,
    label: SharedString,
    selected: bool,
    theme: &ResolvedTheme,
) -> gpui::Stateful<gpui::Div> {
    let t = theme.tokens();
    let (bg, fg): (Hsla, Hsla) = if selected {
        (t.color.accent, t.color.on_accent)
    } else {
        (t.color.background_muted, t.color.text_primary)
    };
    let hover_bg = astryx::theme::color::composite(bg, t.color.overlay_hover);
    div()
        .id(id)
        .px(t.spacing.s3)
        .h(t.size.element_sm)
        .flex()
        .items_center()
        .rounded(t.radius.full)
        .bg(bg)
        .text_color(fg)
        .text_style(&t.text.label)
        .cursor_pointer()
        .hover(move |d| d.bg(hover_bg))
        .child(label)
}

/// A titled section of a page.
pub fn section(title: &str, theme: &ResolvedTheme, body: impl IntoElement) -> AnyElement {
    let t = theme.tokens();
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.s4)
        .child(
            div()
                .font_stack(&t.font.family.heading)
                .text_style(&t.text.heading_3)
                .child(title.to_string()),
        )
        .child(body)
        .into_any_element()
}
