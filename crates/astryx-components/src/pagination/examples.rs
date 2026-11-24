//! Wayfinding examples: the gallery's Wayfinding page and the doc example
//! of Breadcrumbs and Pagination.

use astryx_primitives::ElementSize;
use gpui::{AnyElement, App, Entity, IntoElement, ParentElement, Window};

use crate::breadcrumbs::{BreadcrumbItem, Breadcrumbs, BreadcrumbsVariant};
use crate::layout::{SpacingStep, Stack};
use crate::pagination::{Pagination, PaginationVariant};
use crate::text::{Text, TextType};

fn store(entity: Entity<usize>) -> impl Fn(usize, &mut Window, &mut App) + 'static {
    move |page, window, cx| {
        entity.update(cx, |p, _| *p = page);
        window.refresh();
    }
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let pages = window.use_keyed_state("pages", cx, |_, _| 5usize);
    let count = window.use_keyed_state("count", cx, |_, _| 1usize);
    let dots = window.use_keyed_state("dots", cx, |_, _| 2usize);

    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("Breadcrumbs"))
        .child(
            Breadcrumbs::new("trail")
                .item(BreadcrumbItem::new("Home").href("/"))
                .item(BreadcrumbItem::new("Projects").href("/projects"))
                .item(BreadcrumbItem::new("Apollo")),
        )
        .child(
            Breadcrumbs::new("trail-small")
                .variant(BreadcrumbsVariant::Supporting)
                .item(BreadcrumbItem::new("Settings").on_click(|_, _, _| {}))
                .item(BreadcrumbItem::new("Notifications")),
        )
        .child(label("Pagination: pages, count, compact, dots"))
        .child(
            Pagination::new("pages", *pages.read(cx))
                .total_pages(20)
                .on_change(store(pages.clone())),
        )
        .child(
            Pagination::new("count", *count.read(cx))
                .variant(PaginationVariant::Count)
                .total_items(42)
                .on_change(store(count.clone())),
        )
        .child(
            Pagination::new("compact", *count.read(cx))
                .variant(PaginationVariant::Compact)
                .first_last(true)
                .size(ElementSize::Sm)
                .total_items(42)
                .on_change(store(count.clone())),
        )
        .child(
            Pagination::new("dots", *dots.read(cx))
                .variant(PaginationVariant::Dots)
                .total_pages(5)
                .on_change(store(dots.clone())),
        )
        .into_any_element()
}
