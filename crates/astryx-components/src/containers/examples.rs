//! Container examples: the gallery's Containers page and the doc example
//! of Card, Section and EmptyState.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::containers::{
    Card, CardVariant, Elevation, EmptyState, Section, SectionSide, SectionVariant,
};
use crate::icon::{Icon, IconColor, IconSize};
use crate::layout::{Divider, SpacingStep, Stack};
use crate::text::{Heading, HeadingLevel, Text, TextType};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let card_body = |title: &'static str| {
        Stack::vertical()
            .gap(SpacingStep::S1)
            .child(Heading::new(HeadingLevel::H4, title))
            .child(Text::new("Cards hold one discrete item.").text_type(TextType::Supporting))
    };
    Stack::vertical()
        .gap(SpacingStep::S6)
        .child(label("Cards: default, muted, a color variant, elevated"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(Card::new().width(px(200.)).child(card_body("Default")))
                .child(
                    Card::new()
                        .width(px(200.))
                        .variant(CardVariant::Muted)
                        .child(card_body("Muted")),
                )
                .child(
                    Card::new()
                        .width(px(200.))
                        .variant(CardVariant::Blue)
                        .child(card_body("Blue")),
                )
                .child(
                    Card::new()
                        .width(px(200.))
                        .elevation(Elevation::Med)
                        .child(card_body("Elevated")),
                ),
        )
        .child(label("A full-bleed divider reaches the card's edges"))
        .child(
            Card::new()
                .width(px(320.))
                .child(card_body("Settings"))
                .child(Divider::new().full_bleed(true).mt(px(12.)).mb(px(12.)))
                .child(Text::new("Below the divider")),
        )
        .child(label("Sections"))
        .child(
            Stack::vertical()
                .child(
                    Section::new()
                        .dividers([SectionSide::Bottom])
                        .child(Heading::new(HeadingLevel::H4, "Profile"))
                        .child(Text::new("A section groups a region of the page.")),
                )
                .child(
                    Section::new()
                        .variant(SectionVariant::Muted)
                        .child(Heading::new(HeadingLevel::H4, "Notifications"))
                        .child(Text::new("The muted variant sets a region apart.")),
                ),
        )
        .child(label("EmptyState, regular and compact"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(
                    Card::new().width(px(380.)).child(
                        EmptyState::new("No messages yet")
                            .description("Messages your team sends will appear here.")
                            .icon(
                                Icon::new("search")
                                    .size(IconSize::Lg)
                                    .color(IconColor::Secondary),
                            ),
                    ),
                )
                .child(
                    Card::new().width(px(260.)).child(
                        EmptyState::new("Nothing found")
                            .description("Try another search.")
                            .compact(true),
                    ),
                ),
        )
        .into_any_element()
}
