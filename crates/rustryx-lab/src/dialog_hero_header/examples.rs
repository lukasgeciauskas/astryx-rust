//! The DialogHeroHeader gallery (upstream's `Lab/DialogHeroHeader` stories).

use gpui::{
    AnyElement, App, ElementId, Hsla, IntoElement, ParentElement, Styled, Window, canvas, div,
    fill, linear_color_stop, linear_gradient, point, px, size,
};
use rustryx_components::button::{Button, ButtonVariant};
use rustryx_components::icon::{Icon, IconSize};
use rustryx_components::layout::{
    Layout, LayoutContent, LayoutFooter, SpacingStep, Stack, StackAlign, StackWrap,
};
use rustryx_components::overlay::Dialog;
use rustryx_components::text::{DisplayType, Heading, HeadingLevel, Text, TextType};
use rustryx_i18n::{Direction, I18n};

use super::{DialogHeroHeader, DialogHeroHeaderMediaMode};

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

fn hex(rgb: u32, alpha: f32) -> Hsla {
    let mut c = gpui::rgb(rgb);
    c.a = alpha;
    c.into()
}

/// Upstream's `HeroMedia`: a stand-in hero illustration, 160 px tall and full width (a
/// 400 × 160 artwork cropped to fill, centered), dark or light.
fn hero_media(mode: DialogHeroHeaderMediaMode) -> impl IntoElement {
    let (from, to, accent) = match mode {
        DialogHeroHeaderMediaMode::Dark => (0x1c2340, 0x3b2d5e, 0x8ba7ff),
        DialogHeroHeaderMediaMode::Light => (0xe8ecfb, 0xf6e9f2, 0x5b74d6),
    };
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window.paint_quad(fill(
                bounds,
                linear_gradient(
                    135.,
                    linear_color_stop(hex(from, 1.), 0.),
                    linear_color_stop(hex(to, 1.), 1.),
                ),
            ));
            // `preserveAspectRatio="xMidYMid slice"`: scale to cover, centered.
            let scale =
                (f32::from(bounds.size.width) / 400.).max(f32::from(bounds.size.height) / 160.);
            let dx = (f32::from(bounds.size.width) - 400. * scale) / 2.;
            let dy = (f32::from(bounds.size.height) - 160. * scale) / 2.;
            window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                for (cx, cy, r, opacity) in [
                    (330., 40., 52., 0.35),
                    (70., 140., 70., 0.25),
                    (200., 80., 34., 0.55),
                ] {
                    let r = r * scale;
                    let center = bounds.origin + point(px(dx + cx * scale), px(dy + cy * scale));
                    window.paint_quad(
                        fill(
                            gpui::Bounds::new(
                                center - point(px(r), px(r)),
                                size(px(2. * r), px(2. * r)),
                            ),
                            hex(accent, opacity),
                        )
                        .corner_radii(px(r)),
                    );
                }
            });
        },
    )
    .w_full()
    .h(px(160.))
}

fn body(text: &'static str) -> LayoutContent {
    LayoutContent::new(ElementId::Name(format!("hero-body-{text}").into()))
        .child(Text::new(text).text_type(TextType::Body))
}

/// The DialogHeroHeader examples: dark and light media, start content with truncation, a
/// hidden title and a custom heading, a modal, no close button, and right to left.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let open = window.use_keyed_state(
        ElementId::Name("dialog-hero-header-modal-open".into()),
        cx,
        |_, _| false,
    );
    let is_open = *open.read(cx);
    let set_open = {
        let open = open.clone();
        move |value: bool, window: &mut Window, cx: &mut App| {
            open.update(cx, |o, _| *o = value);
            window.refresh();
        }
    };
    let close_modal = set_open.clone();
    let header_close = set_open.clone();
    let start = set_open.clone();

    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example(
            "Basic",
            Dialog::new("hero-basic", true).inline(true).child(
                Layout::new()
                    .header(
                        DialogHeroHeader::new(
                            "Welcome aboard",
                            hero_media(DialogHeroHeaderMediaMode::Dark),
                        )
                        .media_mode(DialogHeroHeaderMediaMode::Dark)
                        .on_open_change(|_, _, _| {}),
                    )
                    .content(body(
                        "Set up your workspace in three quick steps. You can change any of this \
                         later in Settings.",
                    ))
                    .footer(
                        LayoutFooter::new().child(
                            Stack::horizontal()
                                .gap(SpacingStep::S2)
                                .h_align(StackAlign::End)
                                .child(Button::new("hero-skip", "Skip").variant(ButtonVariant::Secondary))
                                .child(
                                    Button::new("hero-start", "Get started")
                                        .variant(ButtonVariant::Primary),
                                ),
                        ),
                    ),
            ),
        ))
        .child(example(
            "Light media",
            Dialog::new("hero-light", true).inline(true).child(
                Layout::new()
                    .header(
                        DialogHeroHeader::new(
                            "New in this release",
                            hero_media(DialogHeroHeaderMediaMode::Light),
                        )
                        .media_mode(DialogHeroHeaderMediaMode::Light)
                        .on_open_change(|_, _, _| {}),
                    )
                    .content(body(
                        "Charts now support streaming data, and the command palette learned fuzzy \
                         matching.",
                    )),
            ),
        ))
        .child(example(
            "Start content and truncation",
            Dialog::new("hero-truncate", true).inline(true).child(
                Layout::new()
                    .header(
                        DialogHeroHeader::new(
                            "A launch announcement with a title long enough to need truncation in \
                             a narrow dialog",
                            hero_media(DialogHeroHeaderMediaMode::Dark),
                        )
                        .media_mode(DialogHeroHeaderMediaMode::Dark)
                        .start_content(Icon::new("info").size(IconSize::Sm))
                        .max_lines(1)
                        .on_open_change(|_, _, _| {}),
                    )
                    .content(body(
                        "The full title stays available to screen readers and in the truncation \
                         tooltip.",
                    )),
            ),
        ))
        .child(example(
            "Hidden title and custom heading",
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .wrap(StackWrap::Wrap)
                .child(
                    Dialog::new("hero-hidden", true).inline(true).width(px(320.)).child(
                        Layout::new()
                            .header(
                                DialogHeroHeader::new(
                                    "Spring theme refresh",
                                    hero_media(DialogHeroHeaderMediaMode::Light),
                                )
                                .media_mode(DialogHeroHeaderMediaMode::Light)
                                .title_hidden(true)
                                .on_open_change(|_, _, _| {}),
                            )
                            .content(body(
                                "The media speaks for itself; the hidden title still names the \
                                 dialog.",
                            )),
                    ),
                )
                .child(
                    Dialog::new("hero-custom", true).inline(true).width(px(320.)).child(
                        Layout::new()
                            .header(
                                DialogHeroHeader::new(
                                    "Big moment",
                                    hero_media(DialogHeroHeaderMediaMode::Dark),
                                )
                                .title_element(
                                    Heading::new(HeadingLevel::H2, "Big moment")
                                        .display_type(DisplayType::Display3),
                                )
                                .media_mode(DialogHeroHeaderMediaMode::Dark)
                                .on_open_change(|_, _, _| {}),
                            )
                            .content(body(
                                "A caller-provided Heading element renders as-is for custom \
                                 treatments.",
                            )),
                    ),
                ),
        ))
        .child(example(
            "Modal",
            div()
                .child(
                    Button::new("hero-open", "Open hero dialog")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| start(true, window, cx)),
                )
                .child(
                    Dialog::new("hero-modal", is_open)
                        .on_open_change(move |value, window, cx| set_open(value, window, cx))
                        .child(
                            Layout::new()
                                .header(
                                    DialogHeroHeader::new(
                                        "Welcome aboard",
                                        hero_media(DialogHeroHeaderMediaMode::Dark),
                                    )
                                    .media_mode(DialogHeroHeaderMediaMode::Dark)
                                    .on_open_change(move |value, window, cx| {
                                        header_close(value, window, cx)
                                    }),
                                )
                                .content(body("Set up your workspace in three quick steps."))
                                .footer(
                                    LayoutFooter::new().child(
                                        Stack::horizontal()
                                            .h_align(StackAlign::End)
                                            .child(
                                                Button::new("hero-modal-start", "Get started")
                                                    .variant(ButtonVariant::Primary)
                                                    .on_click(move |_, window, cx| {
                                                        close_modal(false, window, cx)
                                                    }),
                                            ),
                                    ),
                                ),
                        ),
                ),
        ))
        .child(example(
            "Without close button",
            Dialog::new("hero-plain", true).inline(true).child(
                Layout::new()
                    .default_has_dividers(true)
                    .header(DialogHeroHeader::new(
                        "A new view of your workspace",
                        hero_media(DialogHeroHeaderMediaMode::Dark),
                    ))
                    .content(body("Close controls are supplied only when needed.")),
            ),
        ))
        .child(example(
            "Right to left",
            rustryx_i18n::provider(
                I18n::new("ar").dir(Direction::Rtl),
                Dialog::new("hero-rtl", true)
                    .inline(true)
                    .padding(SpacingStep::S8)
                    .child(
                        Layout::new()
                            .header(
                                DialogHeroHeader::new(
                                    "\u{645}\u{631}\u{62d}\u{628}\u{64b}\u{627} \u{628}\u{643}",
                                    hero_media(DialogHeroHeaderMediaMode::Light),
                                )
                                .media_mode(DialogHeroHeaderMediaMode::Light)
                                .start_content(Icon::new("info").size(IconSize::Sm))
                                .on_open_change(|_, _, _| {}),
                            )
                            .content(body(
                                "\u{62c}\u{647}\u{651}\u{632} \u{645}\u{633}\u{627}\u{62d}\u{629} \u{639}\u{645}\u{644}\u{643} \u{641}\u{64a} \u{62b}\u{644}\u{627}\u{62b} \u{62e}\u{637}\u{648}\u{627}\u{62a} \u{628}\u{633}\u{64a}\u{637}\u{629}.",
                            )),
                    ),
            ),
        ))
        .into_any_element()
}
