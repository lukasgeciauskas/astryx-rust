//! Theme and MediaTheme (upstream `theme/Theme.tsx`, `theme/MediaTheme.tsx` and the
//! side picker in `hooks/mediaSide.ts`), and the docs of the app-level providers
//! whose Rust APIs live in the crates under this one: InternationalizationProvider
//! (`astryx_i18n::provider`), Layer (`astryx_primitives::layer`) and LinkProvider
//! (`astryx_primitives::link_provider`).
//!
//! A theme made at run time comes from `astryx_theme::authoring` (`define_theme`, then
//! `to_theme`); a shipped one from `astryx_themes`. [`ThemeProvider`] draws a subtree
//! with either, and [`MediaTheme`] gives content on an inverted surface the theme's
//! `onDark` or `onLight` tokens.

pub mod examples;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use astryx_theme::authoring::color::Rgba;
use astryx_theme::authoring::contrast::contrast_ratio;
use astryx_theme::{Appearance, Pointer, ResolvedTheme, Theme, ThemeMode};
use gpui::{AnyElement, App, Global, Hsla, IntoElement, RenderOnce, Window};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::IconSource;
use crate::icon::{
    ComponentIconChoice, TABLE_ROW_EXPAND, TABLE_SORT_INDICATOR, set_component_icon,
};
use astryx_theme::authoring::define::{
    DefinedTheme, ThemeComponentIcon, ThemeComponentIconRequest,
};

/// The component icon slots this crate draws, by name.
const SLOTS: [&str; 2] = [TABLE_SORT_INDICATOR, TABLE_ROW_EXPAND];

/// What a theme's request draws: its icon, nothing, or the component's own.
fn request_choice(request: &ThemeComponentIconRequest) -> ComponentIconChoice {
    match &request.icon {
        Some(Some(icon)) => ComponentIconChoice::Icon(IconSource::Named(icon.clone().into())),
        Some(None) => ComponentIconChoice::Hidden,
        None => ComponentIconChoice::Fallback,
    }
}

/// Gives the component icon slots a defined theme's `componentIcons` (upstream
/// `defineTheme({componentIcons})`, which a Theme applies to its subtree). Here the
/// slots are the app's, so call it when the theme becomes the app's theme. A slot
/// this crate does not draw is skipped; a state a by-state entry does not list keeps
/// the component's own icon.
pub fn apply_component_icons(theme: &DefinedTheme, cx: &mut App) {
    for (slot, value) in theme.component_icons.iter().flat_map(|icons| icons.iter()) {
        let Some(slot) = SLOTS.into_iter().find(|known| *known == slot) else {
            continue;
        };
        match value.clone() {
            ThemeComponentIcon::Icon(icon) => {
                let choice = ComponentIconChoice::Icon(IconSource::Named(icon.into()));
                set_component_icon(slot, move |_| choice.clone(), cx);
            }
            ThemeComponentIcon::Hidden => {
                set_component_icon(slot, |_| ComponentIconChoice::Hidden, cx);
            }
            ThemeComponentIcon::Request(request) => {
                let choice = request_choice(&request);
                set_component_icon(slot, move |_| choice.clone(), cx);
            }
            ThemeComponentIcon::ByState(states) => {
                set_component_icon(
                    slot,
                    move |state| match states.get(state) {
                        Some(Some(request)) => request_choice(request),
                        Some(None) => ComponentIconChoice::Hidden,
                        None => ComponentIconChoice::Fallback,
                    },
                    cx,
                );
            }
        }
    }
}

/// Themes readied once (fonts registered, font stacks resolved), by the `Arc`
/// they came in, which is kept so its address is never reused.
#[derive(Default)]
struct Prepared(HashMap<usize, (Arc<Theme>, Arc<Theme>)>);

impl Global for Prepared {}

/// `theme` readied to draw, once per `Arc`.
fn prepared(theme: &Arc<Theme>, cx: &mut App) -> Arc<Theme> {
    let key = Arc::as_ptr(theme) as usize;
    if let Some((original, ready)) = cx.default_global::<Prepared>().0.get(&key)
        && Arc::ptr_eq(original, theme)
    {
        return ready.clone();
    }
    let ready = astryx_theme::prepare(theme.clone(), cx);
    cx.default_global::<Prepared>()
        .0
        .insert(key, (theme.clone(), ready.clone()));
    ready
}

/// The window's width and pointer, which a theme's adaptations read.
fn environment(window: &Window, cx: &App) -> (f32, Pointer) {
    let width = f32::from(window.viewport_size().width);
    let pointer = if astryx_primitives::modality::coarse_primary_pointer_now(window, cx) {
        Pointer::Coarse
    } else {
        Pointer::Fine
    };
    (width, pointer)
}

/// Upstream `Theme`: draws `child` with `theme` instead of the surrounding one,
/// in `mode`. `System` (the default) keeps the appearance around it, as
/// upstream's `system` takes the page's scheme; `Light` and `Dark` force one.
/// The theme's fonts register, and its font stacks resolve, the first time it
/// draws.
#[derive(IntoElement)]
pub struct ThemeProvider {
    theme: Arc<Theme>,
    mode: ThemeMode,
    child: AnyElement,
}

/// A [`ThemeProvider`] drawing `child` with `theme`.
pub fn theme_provider(theme: impl Into<Arc<Theme>>, child: impl IntoElement) -> ThemeProvider {
    ThemeProvider {
        theme: theme.into(),
        mode: ThemeMode::System,
        child: child.into_any_element(),
    }
}

impl ThemeProvider {
    /// The color mode (upstream `mode`): `System` (the default), `Light` or
    /// `Dark`.
    pub fn mode(mut self, mode: ThemeMode) -> Self {
        self.mode = mode;
        self
    }
}

impl RenderOnce for ThemeProvider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let appearance = match self.mode {
            ThemeMode::Light => Appearance::Light,
            ThemeMode::Dark => Appearance::Dark,
            ThemeMode::System => astryx_theme::current(window, cx).appearance,
        };
        let theme = prepared(&self.theme, cx);
        let (width, pointer) = environment(window, cx);
        // At the reader's text size, as the app's own theme draws.
        let category = astryx_theme::authoring::dynamic_type::text_size_category(cx);
        astryx_theme::scope(
            ResolvedTheme::at_text_size(theme, appearance, width, pointer, category),
            self.child,
        )
    }
}

/// A surface's luminance context (upstream `MediaThemeMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaThemeMode {
    /// Content over a dark surface: the dark scheme and the theme's `onDark`.
    Dark,
    /// Content over a light surface: the light scheme and the theme's `onLight`.
    Light,
    /// The side whose content reads better on the surface, or none when the
    /// surface's own text already reads on it; the fallback when the surface is
    /// unknown.
    Auto,
    /// No media context: the content keeps the theme around it.
    Off,
}

/// Upstream `pickMediaMode` (`hooks/mediaSide.ts`): no media context (`None`) when
/// the surface's own text (`ambient`) reads on it at 3:1, else the side whose
/// on-color reads better, dark on a tie. Colors must be opaque backgrounds.
pub fn pick_media_mode(
    background: Hsla,
    ambient: Option<Hsla>,
    on_dark: Hsla,
    on_light: Hsla,
) -> Option<Appearance> {
    /// WCAG's large-text line: low on purpose, it decides whether a surface is
    /// effectively inverted, not whether its text meets a standard.
    const AMBIENT_READS_AT: f64 = 3.0;
    let background = rgba(opaque(background));
    let ratio = |color: Hsla| contrast_ratio(rgba(color), background).unwrap_or(1.0);
    if let Some(ambient) = ambient
        && ratio(ambient) >= AMBIENT_READS_AT
    {
        return None;
    }
    Some(if ratio(on_dark) >= ratio(on_light) {
        Appearance::Dark
    } else {
        Appearance::Light
    })
}

fn opaque(color: Hsla) -> Hsla {
    Hsla { a: 1.0, ..color }
}

/// A GPUI color as the authoring module's 0–255 channels.
fn rgba(color: Hsla) -> Rgba {
    let c = color.to_rgb();
    Rgba {
        r: f64::from(c.r) * 255.0,
        g: f64::from(c.g) * 255.0,
        b: f64::from(c.b) * 255.0,
        a: f64::from(c.a),
    }
}

/// Upstream `MediaTheme`: content on an inverted surface (a media overlay, a scrim,
/// a toast, a tooltip) takes the surface's color scheme plus the theme's `onDark`
/// or `onLight` tokens, so primary text, icons and the accent read on it. It adds
/// no background: paint the surface on the parent.
///
/// `Auto` picks the side from the surface's color, given with [`MediaTheme::surface`]
/// (usually the token the parent paints, `background_inverted` say). Upstream
/// measures the painted surface; GPUI cannot read a parent's paint, so the surface
/// is named instead, and without one `Auto` takes the fallback.
#[derive(IntoElement)]
pub struct MediaTheme {
    mode: MediaThemeMode,
    fallback: Appearance,
    surface: Option<Hsla>,
    child: AnyElement,
}

/// A [`MediaTheme`] in `mode` around `child`.
pub fn media_theme(mode: MediaThemeMode, child: impl IntoElement) -> MediaTheme {
    MediaTheme {
        mode,
        fallback: Appearance::Dark,
        surface: None,
        child: child.into_any_element(),
    }
}

impl MediaTheme {
    /// The side `Auto` takes when the surface is unknown (upstream `fallback`,
    /// dark by default).
    pub fn fallback(mut self, fallback: Appearance) -> Self {
        self.fallback = fallback;
        self
    }

    /// The color the surface under the content is painted, which `Auto` reads.
    pub fn surface(mut self, color: Hsla) -> Self {
        self.surface = Some(color);
        self
    }
}

impl RenderOnce for MediaTheme {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = astryx_theme::current(window, cx);
        let side = match self.mode {
            MediaThemeMode::Dark => Some(Appearance::Dark),
            MediaThemeMode::Light => Some(Appearance::Light),
            MediaThemeMode::Off => None,
            MediaThemeMode::Auto => match self.surface {
                Some(surface) => {
                    let t = theme.tokens();
                    pick_media_mode(
                        surface,
                        Some(t.color.text_primary),
                        t.color.on_dark,
                        t.color.on_light,
                    )
                }
                None => Some(self.fallback),
            },
        };
        match side {
            Some(appearance) => {
                let (width, pointer) = environment(window, cx);
                astryx_theme::scope(theme.on_surface(appearance, width, pointer), self.child)
                    .into_any_element()
            }
            None => self.child,
        }
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const THEME_DOC: ComponentDoc = ComponentDoc {
    name: "Theme",
    rust_type: "astryx::components::theming::ThemeProvider (theme_provider; themes from astryx::themes or astryx::theme::authoring::define::define_theme)",
    props: &[
        p(
            "theme",
            RustApi::Argument("theme (Arc<astryx::theme::Theme>)"),
        ),
        p("mode", RustApi::Method("mode")),
        p("children", RustApi::Argument("child")),
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_THEME_DOC: ComponentDoc = ComponentDoc {
    name: "MediaTheme",
    rust_type: "astryx::components::theming::MediaTheme (media_theme)",
    props: &[
        p("mode", RustApi::Argument("mode (MediaThemeMode)")),
        p("fallback", RustApi::Method("fallback")),
        p("children", RustApi::Argument("child")),
    ],
    example: include_str!("examples.rs"),
};

pub const INTERNATIONALIZATION_PROVIDER_DOC: ComponentDoc = ComponentDoc {
    name: "InternationalizationProvider",
    rust_type: "astryx::i18n::provider(I18n, child)",
    props: &[
        p("locale", RustApi::Argument("I18n::new(locale)")),
        p("messages", RustApi::Method("I18n::messages")),
        p("overrides", RustApi::Method("I18n::overrides")),
        p("dir", RustApi::Method("I18n::dir")),
        p("calendar", RustApi::Method("I18n::calendar")),
        p("children", RustApi::Argument("child")),
    ],
    example: include_str!("examples.rs"),
};

pub const LAYER_DOC: ComponentDoc = ComponentDoc {
    name: "Layer",
    rust_type: "astryx::primitives::layer (layer, anchored_layer); toasts: astryx::components::overlay::ToastViewport",
    props: &[],
    example: include_str!("examples.rs"),
};

pub const LINK_PROVIDER_DOC: ComponentDoc = ComponentDoc {
    name: "LinkProvider",
    rust_type: "astryx::primitives::link_provider(LinkHandler, child)",
    props: &[
        p(
            "component",
            RustApi::Argument(
                "handler (LinkHandler: navigates an href; there is no anchor element to replace)",
            ),
        ),
        p("children", RustApi::Argument("child")),
    ],
    example: include_str!("examples.rs"),
};
