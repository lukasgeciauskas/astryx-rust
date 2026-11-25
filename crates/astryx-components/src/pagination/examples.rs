//! Wayfinding examples: the gallery's Wayfinding page and the doc example
//! of Breadcrumbs, Pagination and Stepper.

use astryx_primitives::ElementSize;
use gpui::{AnyElement, App, Entity, IntoElement, ParentElement, Styled, Window, div, px};

use crate::breadcrumbs::{BreadcrumbItem, Breadcrumbs, BreadcrumbsVariant};
use crate::layout::Orientation;
use crate::layout::{SpacingStep, Stack};
use crate::overlay::MenuItem;
use crate::pagination::{Pagination, PaginationVariant};
use crate::stepper::{
    CollapsedVariant, HorizontalOptions, IndicatorPosition, Step, StepStatus, Stepper,
};
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
        .child(
            Breadcrumbs::new("trail-menu")
                .item(BreadcrumbItem::new("Home").href("/"))
                .item(BreadcrumbItem::new("Projects").menu([
                    MenuItem::new("apollo", "Apollo").into(),
                    MenuItem::new("gemini", "Gemini").into(),
                    MenuItem::new("mercury", "Mercury").into(),
                ]))
                .item(BreadcrumbItem::new("Apollo")),
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
        .child(label("Stepper: indicators on the track"))
        .child(
            Stepper::new("checkout-track", *step.read(cx))
                .indicator_position(IndicatorPosition::OnTrack)
                .on_step_click(store(step.clone()))
                .step(Step::new("Cart").description("3 items"))
                .step(Step::new("Shipping").description("Address and speed"))
                .step(Step::new("Payment").optional(true))
                .step(Step::new("Review").status(StepStatus::Warning)),
        )
        .child(
            Stepper::new("setup-track", *step.read(cx))
                .orientation(Orientation::Vertical)
                .indicator_position(IndicatorPosition::OnTrack)
                .on_step_click(store(step.clone()))
                .step(Step::new("Cart").description("3 items"))
                .step(
                    Step::new("Shipping")
                        .description("Address and speed")
                        .child(Text::new("Free delivery on orders over $50.")),
                )
                .step(Step::new("Payment").optional(true))
                .step(Step::new("Review")),
        )
        .child(label("Stepper: compact under 112px a step"))
        .child(
            div().w(px(360.)).child(
                Stepper::new("checkout-compact", *step.read(cx))
                    .on_step_click(store(step.clone()))
                    .step(Step::new("Cart").description("3 items"))
                    .step(Step::new("Shipping").description("Address and speed"))
                    .step(Step::new("Payment").optional(true))
                    .step(Step::new("Review").status(StepStatus::Warning)),
            ),
        )
        .child(
            div().w(px(360.)).child(
                Stepper::new("checkout-compact-track", *step.read(cx))
                    .indicator_position(IndicatorPosition::OnTrack)
                    .horizontal_options(HorizontalOptions {
                        collapsed_variant: CollapsedVariant::WithLabel,
                        ..HorizontalOptions::default()
                    })
                    .step(Step::new("Cart"))
                    .step(Step::new("Shipping"))
                    .step(Step::new("Payment"))
                    .step(Step::new("Review")),
            ),
        )
        .into_any_element()
}
