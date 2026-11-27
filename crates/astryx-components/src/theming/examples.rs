//! Theme and MediaTheme examples: the gallery's Theme page. A theme defined at run
//! time from one accent seed (`define_theme`), drawn beside the app's theme in both
//! modes, and content on an inverted surface with MediaTheme.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use astryx_theme::authoring::color_scale::{AccentSeed, ColorScaleConfig};
use astryx_theme::authoring::define::{DefineThemeInput, define_theme};
use astryx_theme::{Appearance, Theme, ThemeMode};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use super::{MediaThemeMode, media_theme, theme_provider};
use crate::button::{Button, ButtonVariant};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};

thread_local! {
    static SEEDED: RefCell<HashMap<String, Arc<Theme>>> = RefCell::default();
}

/// A theme defined from one accent seed, as upstream's
/// `defineTheme({name, color: {accent}})` makes it: the accent, its tints and the
/// neutrals derive from the seed through HCT. Defined once per seed.
pub fn seeded_theme(name: &str, accent: &str) -> Arc<Theme> {
    SEEDED.with(|seeded| {
        seeded
            .borrow_mut()
            .entry(format!("{name} {accent}"))
            .or_insert_with(|| {
                let input = DefineThemeInput {
                    color: Some(ColorScaleConfig {
                        accent: Some(AccentSeed::Single(accent.to_string())),
                        ..ColorScaleConfig::default()
                    }),
                    ..DefineThemeInput::new(name)
                };
                let defined = define_theme(&input).expect("an accent seed defines a theme");
                Arc::new(defined.to_theme())
            })
            .clone()
    })
}

/// A small card of themed content.
fn sample(id: &'static str) -> AnyElement {
    let t = |text: &'static str, kind: TextType| Text::new(text).text_type(kind);
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(t("Quarterly report", TextType::Large))
        .child(t("Drafted from three sources.", TextType::Supporting))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .child(Button::new(format!("{id}-share"), "Share").variant(ButtonVariant::Primary))
                .child(Button::new(format!("{id}-edit"), "Edit").variant(ButtonVariant::Secondary)),
        )
        .into_any_element()
}

/// A surface in the theme around it.
fn surface(content: impl IntoElement, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    div()
        .p(t.spacing.s4)
        .rounded(t.radius.container)
        .bg(t.color.background_surface)
        .border_1()
        .border_color(t.color.border)
        .child(content)
        .into_any_element()
}

#[derive(gpui::IntoElement)]
struct Surface(AnyElement);

impl gpui::RenderOnce for Surface {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        surface(self.0, window, cx)
    }
}

/// Content on a dark inverted surface: MediaTheme gives it the theme's `onDark`
/// tokens, so its text and buttons read on the surface.
#[derive(gpui::IntoElement)]
struct Inverted;

impl gpui::RenderOnce for Inverted {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = astryx_theme::current(window, cx);
        let fill = theme.tokens().color.background_inverted;
        div()
            .p(theme.tokens().spacing.s4)
            .rounded(theme.tokens().radius.container)
            .bg(fill)
            .child(media_theme(MediaThemeMode::Auto, sample("inverted")).surface(fill))
    }
}

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let violet = seeded_theme("violet", "#5B3CC4");
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("The app's theme"))
        .child(Surface(sample("app")))
        .child(label(
            "A theme defined from the accent #5B3CC4, light and dark",
        ))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(
                    div().w(px(320.)).child(
                        theme_provider(violet.clone(), Surface(sample("violet-light")))
                            .mode(ThemeMode::Light),
                    ),
                )
                .child(div().w(px(320.)).child(
                    theme_provider(violet, Surface(sample("violet-dark"))).mode(ThemeMode::Dark),
                )),
        )
        .child(label(
            "MediaTheme on the inverted surface (auto picks the side)",
        ))
        .child(div().w(px(320.)).child(Inverted))
        .child(label("MediaTheme forced light"))
        .child(
            div().w(px(320.)).child(
                media_theme(MediaThemeMode::Light, Surface(sample("light")))
                    .fallback(Appearance::Light),
            ),
        )
        .into_any_element()
}
