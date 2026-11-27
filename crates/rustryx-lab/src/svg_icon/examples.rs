//! The SVGIcon gallery (upstream's `Lab/SVGIcon` stories for SVGIcon and
//! WeightedSVGIcon).

use gpui::prelude::*;
use gpui::{AnyElement, App, FontWeight, Rgba, Window, div, px, rgb};
use rustryx_components::layout::{SpacingStep, Stack};
use rustryx_components::text::{Heading, HeadingLevel, Text, TextColor, TextType};

use super::{
    SVGIcon, SVGIconColor, SVGIconSize, SVGIconVariation, WeightedIconSprite, WeightedSVGIcon,
    bell_icon, bell_weighted_icon, eye_icon, home_icon, home_weighted_icon, lock_icon, mail_icon,
    mail_weighted_icon, menu_icon, pin_icon_weight, search_icon, search_weighted_icon,
    settings_icon, settings_weighted_icon, star_weighted_icon, starter_icons,
    weighted_starter_icons,
};

const VARIATIONS: [SVGIconVariation; 5] = [
    SVGIconVariation::Linear,
    SVGIconVariation::Bold,
    SVGIconVariation::Twotone,
    SVGIconVariation::Bulk,
    SVGIconVariation::Broken,
];

fn story(title: &'static str, description: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(Heading::new(HeadingLevel::H3, title))
        .children((!description.is_empty()).then(|| {
            Text::new(description)
                .text_type(TextType::Supporting)
                .color(TextColor::Secondary)
        }))
        .child(body)
        .into_any_element()
}

fn label(text: impl Into<gpui::SharedString>) -> Text {
    Text::new(text)
        .text_type(TextType::Label)
        .color(TextColor::Secondary)
}

fn grid_row(
    name: impl Into<gpui::SharedString>,
    cells: impl IntoIterator<Item = AnyElement>,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .child(
            div()
                .w(px(120.))
                .child(Text::new(name).text_type(TextType::Label)),
        )
        .children(
            cells
                .into_iter()
                .map(|c| div().w(px(64.)).flex().justify_center().child(c)),
        )
        .into_any_element()
}

fn header(labels: impl IntoIterator<Item = String>) -> AnyElement {
    grid_row("", labels.into_iter().map(|l| label(l).into_any_element()))
}

fn variation_row(icon: super::SVGIconDef) -> AnyElement {
    Stack::horizontal()
        .gap(SpacingStep::S3)
        .children(VARIATIONS.map(|v| {
            Stack::vertical()
                .gap(SpacingStep::S0_5)
                .child(
                    SVGIcon::new(icon.clone())
                        .variation(v)
                        .size(SVGIconSize::Lg),
                )
                .child(label(v.as_str()))
        }))
        .into_any_element()
}

const WEIGHTS: [u16; 5] = [300, 400, 500, 600, 700];

/// The SVGIcon examples: the default icon, every icon in every variation, the path
/// roles, the size scale, the colors, the mask gaps, stroke widths and structural
/// diversity; then WeightedSVGIcon following the font weight, inside Astryx text, from a
/// sprite, and pinned.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let live = window.use_keyed_state("weighted-live-weight", cx, |_, _| 400u16);
    let live_weight = *live.read(cx);
    let row_icons = [
        bell_weighted_icon(),
        home_weighted_icon(),
        search_weighted_icon(),
        settings_weighted_icon(),
        mail_weighted_icon(),
        star_weighted_icon(),
    ];
    let starters = starter_icons();
    let weighted = weighted_starter_icons();
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(story(
            "Default",
            "",
            SVGIcon::new(bell_icon()).variation(SVGIconVariation::Linear).size(SVGIconSize::Lg).color(SVGIconColor::Primary),
        ))
        .child(story(
            "Variation matrix",
            "Same SVG paths, different visual treatments. Stroke-role elements (menu lines, calendar pegs, bell clapper) stay as strokes even in bold and bulk.",
            Stack::vertical()
                .gap(SpacingStep::S2)
                .child(header(VARIATIONS.map(|v| v.as_str().to_string())))
                .children(starters.iter().map(|icon| {
                    grid_row(
                        icon.name.clone(),
                        VARIATIONS.map(|v| SVGIcon::new(icon.clone()).variation(v).size(SVGIconSize::Lg).into_any_element()),
                    )
                })),
        ))
        .child(story(
            "Path roles: fill vs stroke",
            "Stroke-role elements always stay strokes. Fill-role elements switch between stroke (linear) and fill (bold).",
            Stack::vertical()
                .gap(SpacingStep::S2)
                .child(label("Menu \u{2014} all stroke-role (lines never become fills)"))
                .child(variation_row(menu_icon()))
                .child(label("Home \u{2014} fill-role body + door (mask gap in bold)"))
                .child(variation_row(home_icon()))
                .child(label("Settings \u{2014} fill-role gear + circle (mask gap in bold)"))
                .child(variation_row(settings_icon())),
        ))
        .child(story(
            "Size scale with optical compensation",
            "Stroke width adjusts at smaller sizes for legibility.",
            Stack::horizontal().gap(SpacingStep::S3).children(
                [SVGIconSize::Xsm, SVGIconSize::Sm, SVGIconSize::Md, SVGIconSize::Lg]
                    .map(|size| Stack::vertical().gap(SpacingStep::S1).child(SVGIcon::new(settings_icon()).size(size)).child(label(size.as_str()))),
            ),
        ))
        .child(story(
            "Semantic colors",
            "",
            Stack::horizontal().gap(SpacingStep::S3).children(
                [
                    SVGIconColor::Primary,
                    SVGIconColor::Secondary,
                    SVGIconColor::Disabled,
                    SVGIconColor::Accent,
                    SVGIconColor::Positive,
                    SVGIconColor::Negative,
                    SVGIconColor::Warning,
                ]
                .map(|c| Stack::vertical().gap(SpacingStep::S1).child(SVGIcon::new(bell_icon()).size(SVGIconSize::Lg).color(c)).child(label(c.as_str()))),
            ),
        ))
        .child(story(
            "Mask gaps on different backgrounds",
            "Bold mode knocks gaps out with a mask: the gap is transparent, so it works on any background.",
            Stack::vertical().gap(SpacingStep::S2).children(
                [("White", rgb(0xffffff), false), ("Surface", rgb(0xf5f5f5), false), ("Accent", rgb(0x0066ff), true)].map(
                    |(name, bg, white): (&str, Rgba, bool)| {
                        Stack::vertical().gap(SpacingStep::S1).child(label(name)).child(
                            div()
                                .bg(bg)
                                .p(px(16.))
                                .rounded(px(8.))
                                .flex()
                                .gap(px(16.))
                                .when(white, |d| d.text_color(rgb(0xffffff)))
                                .children([home_icon(), settings_icon(), eye_icon(), search_icon(), mail_icon(), lock_icon()].map(|icon| {
                                    SVGIcon::new(icon)
                                        .variation(SVGIconVariation::Bold)
                                        .size(SVGIconSize::Lg)
                                        .color(if white { SVGIconColor::Inherit } else { SVGIconColor::Primary })
                                })),
                        )
                    },
                ),
            ),
        ))
        .child(story(
            "Stroke width range",
            "Linear mode at stroke widths from 1 to 3.",
            Stack::vertical()
                .gap(SpacingStep::S2)
                .child(header([1., 1.5, 2., 2.5, 3.].map(|w: f32| w.to_string())))
                .children(starters.iter().take(8).map(|icon| {
                    grid_row(
                        icon.name.clone(),
                        [1., 1.5, 2., 2.5, 3.].map(|w| SVGIcon::new(icon.clone()).size(SVGIconSize::Lg).stroke_width(w).into_any_element()),
                    )
                })),
        ))
        .child(story(
            "Structural diversity",
            "Organic curves, complex single paths, nested overlapping fills, and mixed fill and stroke roles, across all five variations.",
            Stack::vertical()
                .gap(SpacingStep::S2)
                .child(header(VARIATIONS.map(|v| v.as_str().to_string())))
                .children(starters.iter().skip(7).map(|icon| {
                    grid_row(
                        icon.name.clone(),
                        VARIATIONS.map(|v| SVGIcon::new(icon.clone()).variation(v).size(SVGIconSize::Lg).into_any_element()),
                    )
                })),
        ))
        .child(story(
            "Weighted: follows the font weight",
            "Icons pick their drawing from the text around them: no weight prop anywhere.",
            Stack::vertical().gap(SpacingStep::S3).children(WEIGHTS.map(|w| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .text_size(px(20.))
                    .font_weight(FontWeight(f32::from(w)))
                    .child(div().w(px(48.)).child(w.to_string()))
                    .children(row_icons.clone().map(|icon| WeightedSVGIcon::new(icon).color(SVGIconColor::Inherit)))
                    .child("Notifications and settings")
            })),
        ))
        .child(story(
            "Weighted: live weight",
            "Pick a weight: the text weight changes and the icons follow it, stop by stop.",
            Stack::vertical()
                .gap(SpacingStep::S4)
                .child(Stack::horizontal().gap(SpacingStep::S2).children((300..=700).step_by(50).map(|w: u16| {
                    let live = live.clone();
                    rustryx_components::button::Button::new(gpui::ElementId::Name(format!("weighted-live-{w}").into()), w.to_string())
                        .size(rustryx_primitives::ElementSize::Sm)
                        .on_click(move |_, _, cx| {
                            live.update(cx, |v, cx| {
                                *v = w;
                                cx.notify();
                            })
                        })
                })))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(16.))
                        .text_size(px(32.))
                        .font_weight(FontWeight(f32::from(live_weight)))
                        .child(format!("font-weight {live_weight}"))
                        .children(row_icons.clone().map(|icon| WeightedSVGIcon::new(icon).size(SVGIconSize::Lg).color(SVGIconColor::Inherit)))
                        .child("Inbox"),
                ),
        ))
        .child(story(
            "Weighted: inside Astryx text",
            "Astryx components set the font weight from the theme; icons inside them follow.",
            Stack::vertical().gap(SpacingStep::S3).children(
                [
                    ("normal", FontWeight::NORMAL),
                    ("medium", FontWeight::MEDIUM),
                    ("semibold", FontWeight::SEMIBOLD),
                    ("bold", FontWeight::BOLD),
                ]
                .map(|(name, weight)| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .font_weight(weight)
                        .child(WeightedSVGIcon::new(mail_weighted_icon()).size(SVGIconSize::Sm).color(SVGIconColor::Inherit))
                        .child(format!("{name}: 3 unread messages"))
                }),
            ),
        ))
        .child(story(
            "Weighted: sprite",
            "One sprite holds each icon's drawings once; every instance draws its symbol.",
            Stack::vertical()
                .gap(SpacingStep::S3)
                .child(WeightedIconSprite::new(weighted.clone()))
                .child(div().flex().flex_wrap().w(px(20. * 32.)).gap(px(4.)).children((0..200).map(|i| {
                    div()
                        .font_weight(FontWeight(f32::from(WEIGHTS[i % 5])))
                        .child(WeightedSVGIcon::new(weighted[i % weighted.len()].clone()).sprite(true).color(SVGIconColor::Inherit))
                }))),
        ))
        .child(story(
            "Weighted: pinned weight",
            "`weight` pins one icon; `pin_icon_weight` pins a whole subtree.",
            Stack::vertical()
                .gap(SpacingStep::S3)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .font_weight(FontWeight::LIGHT)
                        .child("Light text, icon pinned to 700:")
                        .child(WeightedSVGIcon::new(star_weighted_icon()).weight(700).color(SVGIconColor::Inherit)),
                )
                .child(pin_icon_weight(
                    300.,
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .font_weight(FontWeight::BOLD)
                        .child("Bold text, subtree pinned to 300:")
                        .children(row_icons.map(|icon| WeightedSVGIcon::new(icon).color(SVGIconColor::Inherit))),
                )),
        ))
        .into_any_element()
}
