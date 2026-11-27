//! The Tour gallery (upstream's `Lab/Tour` stories).

use std::sync::Arc;

use astryx_components::button::{Button, ButtonVariant};
use astryx_components::containers::Card;
use astryx_components::layout::{SpacingStep, Stack, StackAlign};
use astryx_components::text::{Heading, HeadingLevel, Text, TextColor, TextType};
use astryx_components::theming::theme_provider;
use astryx_primitives::{LayerAlign, Placement};
use astryx_theme::ThemeMode;
use astryx_theme::authoring::define::{DefineThemeInput, TokenInput, TokenInputs, define_theme};
use gpui::{
    AnyElement, App, ElementId, Entity, IntoElement, ParentElement, Styled, Window, div, px,
};

use super::{Tour, TourStep, TourTarget};

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(div().min_h(px(200.)).p(px(32.)).child(body))
        .into_any_element()
}

/// A story's on/off switch, kept between frames.
fn switch(name: &'static str, initial: bool, window: &mut Window, cx: &mut App) -> Entity<bool> {
    window.use_keyed_state(ElementId::Name(name.into()), cx, move |_, _| initial)
}

fn set(flag: &Entity<bool>, value: bool) -> impl Fn(&mut Window, &mut App) + 'static {
    let flag = flag.clone();
    move |window, cx| {
        flag.update(cx, |f, _| *f = value);
        window.refresh();
    }
}

/// A secondary Button wrapped as a step's target.
fn target_button(
    target: &TourTarget,
    id: &'static str,
    label: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let radius = theme.tokens().radius.element.to_pixels(window.rem_size());
    target
        .wrap(Button::new(id, label).variant(ButtonVariant::Secondary))
        .control(id)
        .radius(radius)
        .into_any_element()
}

/// Upstream's `Showcase`: three steps under a dimmed page, with the step count.
fn showcase(window: &mut Window, cx: &mut App) -> AnyElement {
    let active = switch("tour-showcase", false, window, cx);
    let is_active = *active.read(cx);
    let (save, share, settings) = (
        TourTarget::new("tour-save"),
        TourTarget::new("tour-share"),
        TourTarget::new("tour-settings"),
    );
    let start = set(&active, true);
    let stop = set(&active, false);
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .child(target_button(&save, "tour-save-button", "Save", window, cx))
                .child(target_button(
                    &share,
                    "tour-share-button",
                    "Share",
                    window,
                    cx,
                ))
                .child(target_button(
                    &settings,
                    "tour-settings-button",
                    "Settings",
                    window,
                    cx,
                )),
        )
        .child(
            Button::new("tour-showcase-start", "Start tour")
                .on_click(move |_, window, cx| start(window, cx)),
        )
        .child(
            Tour::new("tour-showcase", is_active, move |_, window, cx| {
                stop(window, cx)
            })
            .backdrop(true)
            .step_count_shown(true)
            .child(
                TourStep::new("showcase-save", &save, "Save your work")
                    .child("Changes save automatically to the cloud as you go."),
            )
            .child(
                TourStep::new("showcase-share", &share, "Share with your team")
                    .child("Invite teammates and manage access from here."),
            )
            .child(
                TourStep::new("showcase-settings", &settings, "Tune your setup")
                    .child("Adjust preferences and defaults in Settings."),
            ),
        )
        .into_any_element()
}

/// Upstream's `WithoutBackdrop`: a one-step coachmark.
fn without_backdrop(window: &mut Window, cx: &mut App) -> AnyElement {
    let active = switch("tour-coachmark", false, window, cx);
    let is_active = *active.read(cx);
    let target = TourTarget::new("tour-feature");
    let start = set(&active, true);
    let stop = set(&active, false);
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(Heading::new(HeadingLevel::H3, "Feature callout"))
        .child(
            Text::new(
                "A single-step tour with no dimmed background \u{2014} a lightweight coachmark.",
            )
            .text_type(TextType::Body),
        )
        .child(div().child(target_button(
            &target,
            "tour-feature-button",
            "New feature",
            window,
            cx,
        )))
        .child(
            div().child(
                Button::new("tour-coachmark-start", "Highlight it")
                    .on_click(move |_, window, cx| start(window, cx)),
            ),
        )
        .child(
            Tour::new("tour-coachmark", is_active, move |_, window, cx| {
                stop(window, cx)
            })
            .child(
                TourStep::new("coachmark", &target, "Try the new feature")
                    .placement(Placement::Below)
                    .child("We just shipped this \u{2014} click to explore."),
            ),
        )
        .into_any_element()
}

/// Upstream's `CardTarget`: a step on a card, then one on a control the user can press
/// while the tour stays open.
fn card_target(window: &mut Window, cx: &mut App) -> AnyElement {
    let active = switch("tour-card", false, window, cx);
    let is_active = *active.read(cx);
    let refreshed =
        window.use_keyed_state(ElementId::Name("tour-card-refreshed".into()), cx, |_, _| {
            0usize
        });
    let count = *refreshed.read(cx);
    let theme = astryx_theme::current(window, cx);
    let card_radius = theme.tokens().radius.container.to_pixels(window.rem_size());
    let (card, refresh) = (
        TourTarget::new("tour-card"),
        TourTarget::new("tour-refresh"),
    );
    let start = set(&active, true);
    let stop = set(&active, false);
    let radius = theme.tokens().radius.element.to_pixels(window.rem_size());
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            card.wrap(
                Card::new().padding(SpacingStep::S4).child(
                    Stack::vertical()
                        .gap(SpacingStep::S1)
                        .child(Heading::new(HeadingLevel::H4, "Revenue"))
                        .child(Text::new("$1.2M this quarter").text_type(TextType::Body)),
                ),
            )
            .radius(card_radius),
        )
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .v_align(StackAlign::Center)
                .child(
                    refresh
                        .wrap(
                            Button::new("tour-refresh-button", "Refresh")
                                .variant(ButtonVariant::Secondary)
                                .on_click(move |_, window, cx| {
                                    refreshed.update(cx, |n, _| *n += 1);
                                    window.refresh();
                                }),
                        )
                        .control("tour-refresh-button")
                        .radius(radius),
                )
                .child(
                    Text::new(format!("Refreshed {count} times"))
                        .text_type(TextType::Supporting)
                        .color(TextColor::Secondary),
                ),
        )
        .child(
            div().child(
                Button::new("tour-card-start", "Start tour")
                    .on_click(move |_, window, cx| start(window, cx)),
            ),
        )
        .child(
            Tour::new("tour-card", is_active, move |_, window, cx| {
                stop(window, cx)
            })
            .step_count_shown(true)
            .child(
                TourStep::new("card-revenue", &card, "Your revenue")
                    .child("Totals for the quarter, updated daily."),
            )
            .child(
                TourStep::new("card-refresh", &refresh, "Refresh it")
                    .child("Press Refresh to try it. The tour stays open."),
            ),
        )
        .into_any_element()
}

/// Upstream's `ScopedTheme`: a magenta accent in a scoped theme, which the ring takes.
fn scoped_theme(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = {
        thread_local! {
            static MAGENTA: Arc<astryx_theme::Theme> = {
                let input = DefineThemeInput {
                    tokens: Some(TokenInputs::from([(
                        "--color-accent",
                        TokenInput::Pair("#D6006E".into(), "#FF4FA3".into()),
                    )])),
                    ..DefineThemeInput::new("tour-magenta-demo")
                };
                Arc::new(define_theme(&input).expect("a token override defines a theme").to_theme())
            };
        }
        MAGENTA.with(Arc::clone)
    };
    let active = switch("tour-scoped", false, window, cx);
    let is_active = *active.read(cx);
    let target = TourTarget::new("tour-scoped-feature");
    let start = set(&active, true);
    let stop = set(&active, false);
    let target_el = target_button(&target, "tour-scoped-button", "New feature", window, cx);
    theme_provider(
        theme,
        Stack::vertical()
            .gap(SpacingStep::S4)
            .child(Heading::new(HeadingLevel::H3, "Scoped theme"))
            .child(
                Text::new(
                    "This subtree uses a scoped theme with a magenta accent. The highlight ring picks it up because \
                     the overlay is drawn inside the Theme.",
                )
                .text_type(TextType::Body),
            )
            .child(div().child(target_el))
            .child(div().child(Button::new("tour-scoped-start", "Highlight it").on_click(move |_, window, cx| start(window, cx))))
            .child(
                Tour::new("tour-scoped", is_active, move |_, window, cx| stop(window, cx))
                    .backdrop(true)
                    .child(TourStep::new("scoped", &target, "Themed highlight").child("The ring uses this theme's accent color.")),
            ),
    )
    .mode(ThemeMode::Light)
    .into_any_element()
}

/// Upstream's `Placement` story: every side and alignment of the callout around a
/// centered target (the story's controls are buttons here).
fn placement(window: &mut Window, cx: &mut App) -> AnyElement {
    let active = switch("tour-placement", true, window, cx);
    let is_active = *active.read(cx);
    let choice = window.use_keyed_state(
        ElementId::Name("tour-placement-choice".into()),
        cx,
        |_, _| (Placement::Below, LayerAlign::Center),
    );
    let (side, align) = *choice.read(cx);
    let target = TourTarget::new("tour-placement-target");
    let stop = set(&active, false);
    let show = set(&active, true);
    let pick =
        |label: &'static str, id: &'static str, value: (Option<Placement>, Option<LayerAlign>)| {
            let choice = choice.clone();
            let selected = value.0 == Some(side) || value.1 == Some(align);
            Button::new(id, label)
                .variant(if selected {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                })
                .on_click(move |_, window, cx| {
                    choice.update(cx, |(s, a), _| {
                        if let Some(p) = value.0 {
                            *s = p;
                        }
                        if let Some(v) = value.1 {
                            *a = v;
                        }
                    });
                    window.refresh();
                })
        };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .child(pick(
                    "above",
                    "tour-place-above",
                    (Some(Placement::Above), None),
                ))
                .child(pick(
                    "below",
                    "tour-place-below",
                    (Some(Placement::Below), None),
                ))
                .child(pick(
                    "start",
                    "tour-place-start",
                    (Some(Placement::Start), None),
                ))
                .child(pick("end", "tour-place-end", (Some(Placement::End), None)))
                .child(pick(
                    "align start",
                    "tour-align-start",
                    (None, Some(LayerAlign::Start)),
                ))
                .child(pick(
                    "align center",
                    "tour-align-center",
                    (None, Some(LayerAlign::Center)),
                ))
                .child(pick(
                    "align end",
                    "tour-align-end",
                    (None, Some(LayerAlign::End)),
                )),
        )
        .child(
            Stack::vertical()
                .gap(SpacingStep::S4)
                .h_align(StackAlign::Center)
                .v_align(StackAlign::Center)
                .min_h(px(460.))
                .child(target_button(
                    &target,
                    "tour-placement-button",
                    "Target",
                    window,
                    cx,
                ))
                .children((!is_active).then(|| {
                    Button::new("tour-placement-show", "Show step")
                        .on_click(move |_, window, cx| show(window, cx))
                }))
                .child(
                    Tour::new("tour-placement", is_active, move |_, window, cx| {
                        stop(window, cx)
                    })
                    .child(
                        TourStep::new("placement", &target, "Positioned callout")
                            .placement(side)
                            .alignment(align)
                            .child(format!(
                                "placement=\"{}\" \u{b7} alignment=\"{}\"",
                                side.as_str(),
                                align.as_str()
                            )),
                    ),
                ),
        )
        .into_any_element()
}

/// The Tour examples: a dimmed three-step showcase, a coachmark, a card target with a
/// control to press, a scoped theme, and every placement.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example("Showcase", showcase(window, cx)))
        .child(example("Without backdrop", without_backdrop(window, cx)))
        .child(example("Card target", card_target(window, cx)))
        .child(example(
            "Themed ring (scoped theme)",
            scoped_theme(window, cx),
        ))
        .child(example("Placement & alignment", placement(window, cx)))
        .into_any_element()
}
