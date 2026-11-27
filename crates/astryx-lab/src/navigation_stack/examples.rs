//! The NavigationStack gallery (upstream's `NavigationStack` stories): a settings stack
//! with a large-titled root whose rows drill down, each screen with an end action, in a
//! phone-sized frame. Drag sideways in a pushed screen to swipe back.

use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::list::{List, ListItem};
use astryx_components::text::{Text, TextType};
use astryx_theme::current;
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, IntoElement, SharedString, Window, div, px};

use super::{NavigationScreen, NavigationStack, TitleDisplayMode, use_navigation_stack};

/// Rows that push their screen through `use_navigation_stack`.
#[derive(IntoElement)]
struct Rows {
    id: &'static str,
    entries: Vec<SharedString>,
}

impl RenderOnce for Rows {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let api = use_navigation_stack::<SharedString>(cx);
        let rows: Vec<AnyElement> = self
            .entries
            .into_iter()
            .map(|entry| {
                let api = api.clone();
                let label = entry.clone();
                ListItem::new(
                    ElementId::Name(format!("{}-{entry}", self.id).into()),
                    label,
                )
                .on_click(move |_, window, cx| api.push(entry.clone(), window, cx))
                .into_any_element()
            })
            .collect();
        List::new(self.id).dividers(true).children(rows)
    }
}

fn detail(entry: &SharedString, index: usize) -> NavigationScreen {
    let mut rows: Vec<SharedString> = Vec::new();
    if index < 2 {
        rows = (1..=3).map(|i| format!("{entry} {i}").into()).collect();
    }
    let lines: Vec<AnyElement> = (1..=30)
        .map(|i| {
            div()
                .p(px(12.))
                .child(format!("{entry} row {i}"))
                .into_any_element()
        })
        .collect();
    NavigationScreen::new(entry.clone())
        .end_content(Text::new("Edit"))
        .child(Rows {
            id: "ns-detail-rows",
            entries: rows,
        })
        .children(lines)
}

/// The NavigationStack examples.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let path = window.use_keyed_state("ns-path", cx, |_, _| Vec::<SharedString>::new());
    let theme = current(window, cx);
    let t = theme.tokens();
    let root = NavigationScreen::new("Settings")
        .title_display_mode(TitleDisplayMode::Large)
        .child(Rows {
            id: "ns-root-rows",
            entries: ["General", "Accessibility", "Privacy", "Battery"]
                .map(SharedString::from)
                .to_vec(),
        })
        .children((1..=20).map(|i| {
            div()
                .p(px(12.))
                .child(format!("Settings row {i}"))
                .into_any_element()
        }));
    let current_path = path.read(cx).clone();
    let stack = NavigationStack::new("ns-settings", root, current_path)
        .label("Settings")
        .get_key(|entry: &SharedString, _| entry.clone())
        .render_screen(|entry: &SharedString, index, _, _| detail(entry, index))
        .on_path_change(move |next, _, cx| {
            path.update(cx, |p, cx| {
                *p = next;
                cx.notify();
            })
        });
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new("Settings").text_type(TextType::Large))
        .child(
            div()
                .w(px(402.))
                .h(px(640.))
                .overflow_hidden()
                .rounded(px(24.))
                .border_1()
                .border_color(t.color.border)
                .child(stack),
        )
        .into_any_element()
}
