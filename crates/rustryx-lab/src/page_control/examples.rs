//! The PageControl gallery (upstream's `PageControl` stories): five pages, fifty
//! pages, and a single page that hides its control. Each page box shows the page the
//! control is on; the control below it pages it.

use gpui::{AnyElement, App, FontWeight, IntoElement, ParentElement, Styled, Window, div, px};
use rustryx_components::layout::{SpacingStep, Stack, StackAlign};
use rustryx_components::text::{Text, TextType};
use rustryx_theme::current;

use super::PageControl;

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

/// Upstream's `Paged`: a page box over a control that owns no state of its own.
fn paged(
    id: &'static str,
    total_pages: usize,
    start: usize,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let page = window.use_keyed_state(format!("{id}-page"), cx, move |_, _| start);
    let shown = *page.read(cx);
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let page_box = div()
        .w(px(280.))
        .h(px(160.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.element.to_pixels(rem))
        .bg(t.color.background_muted)
        .text_color(t.color.text_primary)
        .text_size(px(20.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(format!("Page {shown}"));
    Stack::vertical()
        .align(StackAlign::Center)
        .gap(SpacingStep::S3)
        .padding(SpacingStep::S6)
        .child(page_box)
        .child(
            PageControl::new(id, total_pages as f64, shown as f64).on_change(move |next, _, cx| {
                page.update(cx, |page, cx| {
                    *page = next;
                    cx.notify();
                })
            }),
        )
        .into_any_element()
}

/// The PageControl examples.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example("Default", paged("pc-default", 5, 3, window, cx)))
        .child(example(
            "Many pages (50)",
            paged("pc-many", 50, 26, window, cx),
        ))
        .child(example(
            "Single page (hides_for_single_page)",
            PageControl::new("pc-single", 1., 1.)
                .on_change(|_, _, _| {})
                .hides_for_single_page(true),
        ))
        .into_any_element()
}
