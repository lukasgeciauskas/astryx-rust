//! Every token of the active theme: colors, type scale, radius, shadow and
//! spacing. The visual check for the theme pipeline.

use astryx::prelude::*;
use astryx::theme::{ResolvedTheme, TextStyleTokens, TokenValue, Tokens};
use gpui::{AbsoluteLength, AnyElement, App, Hsla, Window, div, prelude::*, px};

use crate::gallery::section;

pub fn render(theme: &ResolvedTheme, window: &mut Window, _cx: &mut App) -> AnyElement {
    let t = theme.tokens();
    let rem = window.rem_size();
    let to_px = |length: AbsoluteLength| f32::from(length.to_pixels(rem));

    let colors = Tokens::NAMES
        .iter()
        .filter_map(|name| match t.get(name) {
            Some(TokenValue::Color(color)) => Some((name.trim_start_matches("--color-"), color)),
            _ => None,
        })
        .map(|(name, color)| swatch(name, color, t));

    let styles: [(&str, &TextStyleTokens, bool); 14] = [
        ("display-1", &t.text.display_1, true),
        ("display-2", &t.text.display_2, true),
        ("display-3", &t.text.display_3, true),
        ("heading-1", &t.text.heading_1, true),
        ("heading-2", &t.text.heading_2, true),
        ("heading-3", &t.text.heading_3, true),
        ("heading-4", &t.text.heading_4, true),
        ("heading-5", &t.text.heading_5, true),
        ("heading-6", &t.text.heading_6, true),
        ("large", &t.text.large, false),
        ("body", &t.text.body, false),
        ("label", &t.text.label, false),
        ("supporting", &t.text.supporting, false),
        ("code", &t.text.code, false),
    ];
    let type_scale = styles.into_iter().map(|(name, style, heading)| {
        let family = if name == "code" {
            &t.font.family.code
        } else if heading {
            &t.font.family.heading
        } else {
            &t.font.family.body
        };
        div()
            .flex()
            .items_center()
            .gap(t.spacing.s4)
            .child(
                div()
                    .w(px(220.))
                    .flex_none()
                    .text_style(&t.text.supporting)
                    .text_color(t.color.text_secondary)
                    .child(format!(
                        "{name} · {:.0}px / {:.0} / {}",
                        to_px(style.size),
                        style.weight.0,
                        style.leading
                    )),
            )
            .child(
                div()
                    .font_stack(family)
                    .text_style(style)
                    .child("The quick brown fox jumps over the lazy dog"),
            )
    });

    let named = |prefix: &'static str| {
        Tokens::NAMES
            .iter()
            .filter(move |n| n.starts_with(prefix))
            .copied()
    };

    let radii = named("--radius-").filter_map(|name| match t.get(name) {
        Some(TokenValue::Length(radius)) => Some(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(t.spacing.s2)
                .child(
                    div()
                        .size(px(72.))
                        .bg(t.color.background_surface)
                        .border_1()
                        .border_color(t.color.border_emphasized)
                        .rounded(radius),
                )
                .child(caption(name.trim_start_matches("--radius-"), t)),
        ),
        _ => None,
    });

    let shadows = named("--shadow-").filter_map(|name| match t.get(name) {
        Some(TokenValue::Shadow(shadow)) => Some(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(t.spacing.s2)
                .child(
                    div()
                        .w(px(120.))
                        .h(px(80.))
                        .bg(t.color.background_card)
                        .rounded(t.radius.container)
                        .shadow_token(&shadow),
                )
                .child(caption(name.trim_start_matches("--shadow-"), t)),
        ),
        _ => None,
    });

    let spacing = named("--spacing-").filter_map(|name| match t.get(name) {
        Some(TokenValue::Length(space)) => Some(
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s3)
                .child(
                    div()
                        .w(px(110.))
                        .child(caption(name.trim_start_matches("--"), t)),
                )
                .child(
                    div()
                        .h(px(12.))
                        .w(space)
                        .bg(t.color.accent)
                        .rounded(t.radius.inner),
                )
                .child(caption(&format!("{:.0}px", to_px(space)), t)),
        ),
        _ => None,
    });

    div()
        .flex()
        .flex_col()
        .gap(t.spacing.s12)
        .child(
            div()
                .font_stack(&t.font.family.heading)
                .text_style(&t.text.display_2)
                .child(format!("Tokens · {}", theme.theme.name)),
        )
        .child(section(
            "Color",
            theme,
            div().flex().flex_wrap().gap(t.spacing.s3).children(colors),
        ))
        .child(section(
            "Type scale",
            theme,
            div()
                .flex()
                .flex_col()
                .gap(t.spacing.s3)
                .children(type_scale),
        ))
        .child(section(
            "Radius",
            theme,
            div().flex().flex_wrap().gap(t.spacing.s6).children(radii),
        ))
        .child(section(
            "Shadow",
            theme,
            div()
                .flex()
                .flex_wrap()
                .gap(t.spacing.s8)
                .p(t.spacing.s4)
                .children(shadows),
        ))
        .child(section(
            "Spacing",
            theme,
            div().flex().flex_col().gap(t.spacing.s2).children(spacing),
        ))
        .into_any_element()
}

fn caption(text: &str, t: &Tokens) -> impl IntoElement {
    div()
        .text_style(&t.text.supporting)
        .text_color(t.color.text_secondary)
        .child(text.to_string())
}

fn hex(color: Hsla) -> String {
    let c = color.to_rgb();
    let byte = |v: f32| (v * 255.0).round() as u8;
    if c.a >= 1.0 {
        format!("#{:02X}{:02X}{:02X}", byte(c.r), byte(c.g), byte(c.b))
    } else {
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            byte(c.r),
            byte(c.g),
            byte(c.b),
            byte(c.a)
        )
    }
}

fn swatch(name: &str, color: Hsla, t: &Tokens) -> impl IntoElement {
    div()
        .w(px(150.))
        .flex()
        .flex_col()
        .gap(t.spacing.s1)
        .child(
            div()
                .h(px(44.))
                .rounded(t.radius.element)
                .border_1()
                .border_color(t.color.border)
                .bg(color),
        )
        .child(
            div()
                .text_style(&t.text.supporting)
                .overflow_hidden()
                .child(name.to_string()),
        )
        .child(caption(&hex(color), t))
}
