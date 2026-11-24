//! Feedback examples: the gallery's Feedback page and the doc example of
//! ProgressBar and Banner.

use gpui::{AnyElement, App, IntoElement, ParentElement, Window, px};

use crate::feedback::{
    Banner, BannerContainer, BannerStatus, Collapsible, ProgressBar, ProgressMark, ProgressVariant,
};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    Stack::vertical()
        .gap(SpacingStep::S5)
        .max_width(px(560.))
        .child(label("ProgressBar"))
        .child(
            ProgressBar::new("upload", "Uploading photos")
                .value(45.)
                .value_label(true),
        )
        .child(
            ProgressBar::new("quota", "Storage")
                .value(7.2)
                .max(10.)
                .value_label(true)
                .variant(ProgressVariant::Warning)
                .format_value_label(|value, max| format!("{value:.1} GB of {max:.0} GB")),
        )
        .child(
            ProgressBar::new("goal", "Fundraising goal")
                .value(62.)
                .variant(ProgressVariant::Success)
                .marks([
                    ProgressMark {
                        value: 50.,
                        label: "Halfway".into(),
                    },
                    ProgressMark {
                        value: 80.,
                        label: "Stretch".into(),
                    },
                ]),
        )
        .child(ProgressBar::new("loading", "Loading").indeterminate(true))
        .child(
            ProgressBar::new("hidden", "Syncing")
                .value(30.)
                .label_hidden(true),
        )
        .child(label("Banners"))
        .child(
            Banner::new("info", BannerStatus::Info, "A new version is available")
                .description("Restart to update.")
                .dismissable(true),
        )
        .child(
            Banner::new(
                "warning",
                BannerStatus::Warning,
                "Your trial ends in 3 days",
            )
            .collapsible(Collapsible::Uncontrolled { default_open: true })
            .child(Text::new("Add a payment method to keep your projects.")),
        )
        .child(
            Banner::new("error", BannerStatus::Error, "Payment failed")
                .description("Check your card details."),
        )
        .child(
            Banner::new("success", BannerStatus::Success, "Settings saved")
                .container(BannerContainer::Section),
        )
        .into_any_element()
}
