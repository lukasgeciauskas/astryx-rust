//! Status examples: the gallery's Status page and the doc example of
//! Spinner, Skeleton, Badge and StatusDot.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::icon::Icon;
use crate::layout::{SpacingStep, Stack, StackAlign, StackItem, StackItemSize};
use crate::status::{
    Badge, BadgeVariant, Skeleton, SkeletonRadius, Spinner, SpinnerShade, SpinnerSize, StatusDot,
    StatusDotVariant,
};
use crate::text::{Text, TextType};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let row = || {
        Stack::horizontal()
            .gap(SpacingStep::S4)
            .v_align(StackAlign::Center)
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("Spinner sizes"))
        .child(
            row().children(
                [
                    SpinnerSize::Sm,
                    SpinnerSize::Md,
                    SpinnerSize::Lg,
                    SpinnerSize::Xl,
                ]
                .into_iter()
                .enumerate()
                .map(|(i, size)| Spinner::new(("spinner", i)).size(size)),
            ),
        )
        .child(label("Spinner shades and a label"))
        .child(
            row()
                .child(Spinner::new("subtle").shade(SpinnerShade::Subtle))
                .child(
                    Spinner::new("labeled")
                        .size(SpinnerSize::Lg)
                        .label("Loading messages"),
                ),
        )
        .child(label("Skeleton: a card loading"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .width(px(360.))
                .child(
                    Skeleton::new()
                        .width(px(48.))
                        .height(px(48.))
                        .radius(SkeletonRadius::Rounded),
                )
                .child(
                    StackItem::new().size(StackItemSize::Fill).child(
                        Stack::vertical()
                            .gap(SpacingStep::S2)
                            .child(Skeleton::new().height(px(14.)).width(px(160.)).index(1))
                            .child(Skeleton::new().height(px(12.)).index(2))
                            .child(Skeleton::new().height(px(12.)).width(px(220.)).index(3)),
                    ),
                ),
        )
        .child(label("Badges"))
        .child(
            row().gap(SpacingStep::S2).children(
                [
                    BadgeVariant::Neutral,
                    BadgeVariant::Info,
                    BadgeVariant::Success,
                    BadgeVariant::Warning,
                    BadgeVariant::Error,
                ]
                .into_iter()
                .map(|variant| Badge::new(variant.as_str()).variant(variant)),
            ),
        )
        .child(
            row().gap(SpacingStep::S2).children(
                [
                    BadgeVariant::Blue,
                    BadgeVariant::Cyan,
                    BadgeVariant::Green,
                    BadgeVariant::Orange,
                    BadgeVariant::Pink,
                    BadgeVariant::Purple,
                    BadgeVariant::Red,
                    BadgeVariant::Teal,
                    BadgeVariant::Yellow,
                ]
                .into_iter()
                .map(|variant| Badge::new(variant.as_str()).variant(variant)),
            ),
        )
        .child(
            row()
                .gap(SpacingStep::S2)
                .child(
                    Badge::new("Verified")
                        .variant(BadgeVariant::Success)
                        .icon(Icon::new("check")),
                )
                .child(Badge::new("A long label that is cut with an ellipsis").max_w(px(160.))),
        )
        .child(label("Status dots"))
        .child(
            row()
                .child(StatusDot::new(
                    "online",
                    StatusDotVariant::Success,
                    "Online",
                ))
                .child(StatusDot::new("away", StatusDotVariant::Warning, "Away"))
                .child(StatusDot::new("down", StatusDotVariant::Error, "Down").pulsing(true))
                .child(StatusDot::new("new", StatusDotVariant::Accent, "New"))
                .child(StatusDot::new(
                    "done",
                    StatusDotVariant::Neutral,
                    "Resolved",
                )),
        )
        .into_any_element()
}
