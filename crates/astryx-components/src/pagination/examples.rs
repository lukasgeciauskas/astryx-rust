//! Wayfinding examples: the gallery's Wayfinding page and the doc example
//! of Breadcrumbs, Pagination and Stepper.

use astryx_primitives::ElementSize;
use gpui::{AnyElement, App, Entity, IntoElement, ParentElement, Window};

use crate::breadcrumbs::{BreadcrumbItem, Breadcrumbs, BreadcrumbsVariant};
use crate::layout::Orientation;
use crate::layout::{SpacingStep, Stack};
use crate::pagination::{Pagination, PaginationVariant};
use crate::stepper::{Step, StepStatus, Stepper};
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
    let step = window.use_keyed_state("step", cx, |_, _| 1usize);

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
        .child(label("Pagination: pages, input, count, compact, dots"))
        .child(
            Pagination::new("pages", *pages.read(cx))
                .total_pages(20)
                .on_change(store(pages.clone())),
        )
        .child(
            Pagination::new("input", *pages.read(cx))
                .variant(PaginationVariant::Input)
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
        .child(label("Stepper: horizontal (clickable) and vertical"))
        .child(
            Stepper::new("checkout", *step.read(cx))
                .on_step_click(store(step.clone()))
                .step(Step::new("Cart").description("3 items"))
                .step(Step::new("Shipping").description("Address and speed"))
                .step(Step::new("Payment").optional(true))
                .step(Step::new("Review").status(StepStatus::Warning)),
        )
        .child(
            Stepper::new("setup", 1)
                .orientation(Orientation::Vertical)
                .step(Step::new("Create account").status(StepStatus::Success))
                .step(
                    Step::new("Verify email")
                        .description("We sent a link to your inbox.")
                        .child(Text::new("Didn't get it? Check your spam folder.")),
                )
                .step(Step::new("Invite your team")),
        )
        .into_any_element()
}
