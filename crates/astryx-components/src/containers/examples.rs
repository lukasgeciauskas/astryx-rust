//! Container examples: the gallery's Containers page and the doc example
//! of Card, Section, EmptyState, ClickableCard, SelectableCard,
//! Collapsible and Token.

use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, px};

use crate::button::SelectionMode;
use crate::collapsible::{ChevronPosition, Collapsible, CollapsibleGroup};
use crate::containers::{
    Card, CardVariant, ClickableCard, Elevation, EmptyState, Section, SectionSide, SectionVariant,
    SelectableCard,
};
use crate::icon::{Icon, IconColor, IconSize};
use crate::layout::{Divider, SpacingStep, Stack};
use crate::text::{Heading, HeadingLevel, Text, TextType};
use crate::token::{Token, TokenColor};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let plan = window.use_keyed_state("selected-plan", cx, |_, _| SharedString::from("pro"));
    let chosen = plan.read(cx).clone();
    let select = |value: &'static str| {
        let plan = plan.clone();
        move |_: bool, window: &mut Window, cx: &mut App| {
            plan.update(cx, |p, _| *p = value.into());
            window.refresh();
        }
    };
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    // The FAQ's handle: opening an answer glides what follows it (AST-105).
    let faq_layout = crate::animated_layout::use_animated_layout("faq-layout", window, cx);
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
        .child(label("ClickableCard and SelectableCard"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    ClickableCard::new("open-apollo", "Open Apollo")
                        .width(px(220.))
                        .elevation(Elevation::Low)
                        .on_click(|_, _, _| {})
                        .child(card_body("Apollo")),
                )
                .child(
                    SelectableCard::new("plan-free", "Free plan", chosen == "free")
                        .width(px(200.))
                        .on_change(select("free"))
                        .child(card_body("Free")),
                )
                .child(
                    SelectableCard::new("plan-pro", "Pro plan", chosen == "pro")
                        .width(px(200.))
                        .variant(CardVariant::Blue)
                        .on_change(select("pro"))
                        .child(card_body("Pro")),
                ),
        )
        .child(label("Collapsible and CollapsibleGroup"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S6)
                .child(
                    Stack::vertical()
                        .width(px(320.))
                        .child(
                            Collapsible::new("details", "Shipping details")
                                .child(Text::new("Ships in 2 to 3 business days.")),
                        )
                        .child(
                            Collapsible::new("returns", "Returns")
                                .default_open(false)
                                .chevron_position(ChevronPosition::Start)
                                .child(Text::new("Free returns within 30 days.")),
                        ),
                )
                .child(
                    faq_layout.container(
                        gpui::div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .w(px(320.))
                            .child(
                                CollapsibleGroup::new("faq", SelectionMode::Single)
                                    .dividers(true)
                                    .default_value(["q1"])
                                    .item(
                                        Collapsible::new("q1", "What is Astryx?")
                                            .value("q1")
                                            .animated_layout(faq_layout.clone())
                                            .child(Text::new("A design system, ported to GPUI.")),
                                    )
                                    .item(
                                        Collapsible::new("q2", "Does it work on the web?")
                                            .value("q2")
                                            .animated_layout(faq_layout.clone())
                                            .child(Text::new(
                                                "Yes, through GPUI's wasm platform.",
                                            )),
                                    )
                                    .item(
                                        Collapsible::new("q3", "Is it themeable?")
                                            .value("q3")
                                            .animated_layout(faq_layout.clone())
                                            .child(Text::new("Seven themes ship with it.")),
                                    ),
                            )
                            .child(
                                Text::new(
                                    "With an animatedLayout handle, this note glides as answers open and close.",
                                )
                                .text_type(TextType::Supporting),
                            ),
                        window,
                        cx,
                    ),
                ),
        )
        .child(label("Token"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .child(Token::new("tok-default", "Default"))
                .child(Token::new("tok-blue", "Design").color(TokenColor::Blue))
                .child(
                    Token::new("tok-green", "Approved")
                        .color(TokenColor::Green)
                        .icon(Icon::new("check")),
                )
                .child(
                    Token::new("tok-remove", "Removable")
                        .color(TokenColor::Purple)
                        .on_remove(|_, _, _| {}),
                )
                .child(Token::new("tok-click", "Clickable").on_click(|_, _, _| {}))
                .child(Token::new("tok-sm", "Small").size(astryx_primitives::ElementSize::Sm))
                .child(Token::new("tok-off", "Disabled").disabled(true)),
        )
        .into_any_element()
}
