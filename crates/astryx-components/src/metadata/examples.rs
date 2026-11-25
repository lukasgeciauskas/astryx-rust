//! MetadataList examples: the gallery's Collections page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::icon::{Icon, IconSize};
use crate::layout::{SpacingStep, Stack};
use crate::metadata::{MetadataColumns, MetadataList, MetadataListItem, MetadataOrientation};
use crate::status::Badge;
use crate::text::{Heading, HeadingLevel, Text, TextType};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let item = |l: &'static str, v: &'static str| MetadataListItem::new(l, v);
    Stack::vertical()
        .gap(SpacingStep::S6)
        .child(label("MetadataList"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S8)
                .child(
                    MetadataList::new("order")
                        .title(Heading::new(HeadingLevel::H5, "Order"))
                        .w(px(280.))
                        .child(item("Number", "#10482"))
                        .child(MetadataListItem::new("Status", Badge::new("Shipped")))
                        .child(
                            MetadataListItem::new("Placed", "Nov 24, 2026")
                                .icon(Icon::new("calendar").size(IconSize::Sm)),
                        )
                        .child(item("Carrier", "Parcel Co."))
                        .child(item("Tracking", "PX-2209-118"))
                        .max_items(3),
                )
                .child(
                    MetadataList::new("specs")
                        .columns(MetadataColumns::Count(2))
                        .w(px(360.))
                        .child(item("Width", "120 cm"))
                        .child(item("Depth", "60 cm"))
                        .child(item("Height", "74 cm"))
                        .child(item("Weight", "18 kg")),
                ),
        )
        .child(
            MetadataList::new("summary")
                .orientation(MetadataOrientation::Horizontal)
                .child(item("Owner", "Ada Lovelace"))
                .child(item("Created", "Monday"))
                .child(item("Visibility", "Team"))
                .into_any_element(),
        )
        .into_any_element()
}
