//! Effects that GPUI lacks upstream and our fork or `astryx-effects` adds.
//! The visual check for each: every kind of primitive (fills, borders,
//! corner radii, shadows, text, icons, clipping) under the effect.
//!
//! Transforms (fork patch R1): `scale`, `translate` and `transform_origin`
//! move an element and its subtree when painted, without changing layout.

use std::time::Duration;

use astryx::icons::core_icon_path;
use astryx::prelude::*;
use astryx::theme::{ResolvedTheme, Tokens};
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Div, Window, div, ease_in_out, prelude::*, px,
    relative, svg,
};

use crate::gallery::section;

pub fn render(theme: &ResolvedTheme, _window: &mut Window, _cx: &mut App) -> AnyElement {
    let t = theme.tokens();
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.s8)
        .child(section(
            "Scale",
            theme,
            row(t)
                .child(slot(t, "1", card(t, "Card")))
                .child(slot(t, "0.8", card(t, "Card").scale(0.8)))
                .child(slot(t, "1.2", card(t, "Card").scale(1.2)))
                .child(slot(t, "1.2 x 0.6", card(t, "Card").scale_xy(1.2, 0.6)))
                .child(slot(
                    t,
                    "0.6 from top-left",
                    card(t, "Card").scale(0.6).transform_origin(0., 0.),
                )),
        ))
        .child(section(
            "Translate",
            theme,
            row(t)
                .child(slot(t, "none", card(t, "Card")))
                .child(slot(
                    t,
                    "24px, -12px",
                    card(t, "Card").translate(px(24.), px(-12.)),
                ))
                .child(slot(
                    t,
                    "50% of own width",
                    card(t, "Card").translate_x(relative(0.5)),
                ))
                .child(slot(
                    t,
                    "clipped by parent",
                    div()
                        .size_full()
                        .overflow_hidden()
                        .border_1()
                        .border_color(t.color.border)
                        .rounded(t.radius.container)
                        .child(card(t, "Card").translate(px(70.), px(40.))),
                )),
        ))
        .child(section(
            "Mirror and nesting",
            theme,
            row(t)
                .child(slot(t, "scale x -1", card(t, "Mirrored").scale_xy(-1., 1.)))
                .child(slot(
                    t,
                    "0.75, inner moved 20px",
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(t.color.background_muted)
                        .rounded(t.radius.container)
                        .scale(0.75)
                        .child(card(t, "Inner").translate_y(px(20.))),
                )),
        ))
        .child(section(
            "Interaction",
            theme,
            row(t)
                .child(slot(
                    t,
                    "press: 0.96",
                    card(t, "Press me")
                        .id("press")
                        .cursor_pointer()
                        .hover(|style| style.bg(t.color.overlay_hover))
                        .active(|style| style.scale(0.96)),
                ))
                .child(slot(
                    t,
                    "hover: lift 4px",
                    card(t, "Hover me")
                        .id("lift")
                        .hover(|style| style.translate_y(px(-4.))),
                ))
                .child(slot(
                    t,
                    "animated",
                    card(t, "Pulse").with_animation(
                        "pulse",
                        Animation::new(Duration::from_millis(1600))
                            .repeat()
                            .with_easing(ease_in_out),
                        |card, delta| {
                            let wave = (delta * std::f32::consts::TAU).sin();
                            card.scale(1.0 + 0.08 * wave)
                        },
                    ),
                )),
        ))
        .into_any_element()
}

fn row(t: &Tokens) -> Div {
    div().flex().flex_wrap().gap(t.spacing.s8)
}

/// A fixed-size slot with a caption, so every transform starts from the
/// same layout box.
fn slot(t: &Tokens, caption: &str, content: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.s3)
        .child(div().w(px(160.)).h(px(100.)).child(content))
        .child(
            div()
                .text_style(&t.text.supporting)
                .text_color(t.color.text_secondary)
                .child(caption.to_string()),
        )
}

/// Every primitive kind: fill, border, corner radii, shadow, text and an
/// SVG icon.
fn card(t: &Tokens, label: &str) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(t.spacing.s2)
        .bg(t.color.background_card)
        .border_1()
        .border_color(t.color.border)
        .rounded(t.radius.container)
        .shadow_token(&t.shadow.med)
        .text_style(&t.text.label)
        .text_color(t.color.text_primary)
        .child(
            svg()
                .path(core_icon_path("chevronRight").unwrap_or_default())
                .size(px(16.))
                .text_color(t.color.text_primary),
        )
        .child(label.to_string())
}
