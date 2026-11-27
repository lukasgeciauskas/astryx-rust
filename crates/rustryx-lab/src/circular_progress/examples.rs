//! The CircularProgress gallery (upstream's `CircularProgress` stories).

use gpui::{AnyElement, App, IntoElement, ParentElement, Window};
use rustryx_components::layout::{SpacingStep, Stack};
use rustryx_components::text::{Text, TextType};

use super::{CircularProgress, CircularProgressSize, CircularProgressVariant};

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

fn row() -> Stack {
    Stack::horizontal().gap(SpacingStep::S6)
}

/// The CircularProgress examples.
pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let sizes = [
        (CircularProgressSize::Sm, "Small"),
        (CircularProgressSize::Md, "Medium"),
        (CircularProgressSize::Lg, "Large"),
    ];
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example(
            "Default",
            row()
                .child(CircularProgress::new("cp-default", "Progress").value(60.))
                .child(
                    CircularProgress::new("cp-center", "Progress")
                        .value(75.)
                        .child(Text::new("75%").text_type(TextType::Supporting)),
                ),
        ))
        .child(example(
            "Sizes",
            row()
                .children(sizes.map(|(size, label)| {
                    CircularProgress::new(label, label).value(60.).size(size)
                })),
        ))
        .child(example(
            "Variants",
            row()
                .child(CircularProgress::new("cp-accent", "Accent").value(60.))
                .child(
                    CircularProgress::new("cp-success", "Positive")
                        .value(80.)
                        .variant(CircularProgressVariant::Success),
                )
                .child(
                    CircularProgress::new("cp-warning", "Warning")
                        .value(50.)
                        .variant(CircularProgressVariant::Warning),
                )
                .child(
                    CircularProgress::new("cp-error", "Negative")
                        .value(92.)
                        .variant(CircularProgressVariant::Error),
                )
                .child(
                    CircularProgress::new("cp-neutral", "Neutral")
                        .value(35.)
                        .variant(CircularProgressVariant::Neutral),
                ),
        ))
        .child(example(
            "Value labels",
            row()
                .child(CircularProgress::new("cp-empty", "Not started").value(0.))
                .child(
                    CircularProgress::new("cp-full", "Complete")
                        .value(100.)
                        .variant(CircularProgressVariant::Success)
                        .size(CircularProgressSize::Lg)
                        .child(Text::new("100%")),
                )
                .child(
                    CircularProgress::new("cp-value", "Upload progress")
                        .value(75.)
                        .size(CircularProgressSize::Lg)
                        .value_label(true),
                )
                .child(
                    CircularProgress::new("cp-steps", "Steps completed")
                        .value(3.)
                        .max(5.)
                        .size(CircularProgressSize::Lg)
                        .value_label(true)
                        .format_value_label(|value, max| format!("{value}/{max}").into()),
                )
                .child(
                    CircularProgress::new("cp-visible", "Uploading")
                        .value(40.)
                        .label_hidden(false),
                ),
        ))
        .child(example(
            "Disabled",
            row()
                .child(
                    CircularProgress::new("cp-canceled", "Canceled")
                        .value(30.)
                        .disabled(true),
                )
                .child(
                    CircularProgress::new("cp-canceled-value", "Canceled with value")
                        .value(30.)
                        .size(CircularProgressSize::Lg)
                        .disabled(true)
                        .value_label(true),
                )
                .child(
                    CircularProgress::new("cp-canceled-loading", "Canceled loading")
                        .indeterminate(true)
                        .disabled(true),
                ),
        ))
        .child(example(
            "Indeterminate",
            row()
                .children(sizes.map(|(size, label)| {
                    CircularProgress::new(format!("cp-spin-{label}"), "Loading")
                        .indeterminate(true)
                        .size(size)
                }))
                .child(
                    CircularProgress::new("cp-spin-success", "Loading")
                        .indeterminate(true)
                        .variant(CircularProgressVariant::Success),
                ),
        ))
        .into_any_element()
}
