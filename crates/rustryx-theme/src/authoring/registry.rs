//! Defined themes by name: upstream `theme/themeRegistry.ts`.
//!
//! Upstream keeps one registry per copy of React, which `defineTheme` fills, so code
//! can resolve theme data by a stable name without React context. GPUI is
//! single-threaded and its state lives in the [`App`], so the registry here is an
//! [`App`] global: [`define_theme`](super::define::define_theme) needs no `App` and
//! registers nothing; [`define_theme_in`] defines and registers, as upstream's
//! `defineTheme` does, and [`register_theme`] registers a theme defined elsewhere
//! (a built one, say).

use std::sync::Arc;

use gpui::{App, Global};

use super::define::{DefineThemeError, DefineThemeInput, DefinedTheme, OrderedMap, define_theme};

/// The registered themes, by name, in the order they were first registered.
#[derive(Default)]
struct ThemeRegistry(OrderedMap<Arc<DefinedTheme>>);

impl Global for ThemeRegistry {}

/// Registers a defined theme under its `name` (upstream `registerTheme`).
///
/// Registration is idempotent and replaces any previous theme with the same name, in
/// its place. Call it from app initialization or a theme package to make the theme
/// available by name ([`get_registered_theme`]).
pub fn register_theme(theme: impl Into<Arc<DefinedTheme>>, cx: &mut App) {
    let theme = theme.into();
    cx.default_global::<ThemeRegistry>()
        .0
        .insert(theme.name.clone(), theme);
}

/// A previously registered theme by name (upstream `getRegisteredTheme`); `None` for
/// no name, an empty one, or one no theme is registered under.
pub fn get_registered_theme(name: Option<&str>, cx: &App) -> Option<Arc<DefinedTheme>> {
    let name = name.filter(|name| !name.is_empty())?;
    cx.try_global::<ThemeRegistry>()?.0.get(name).cloned()
}

/// Every registered theme, keyed by name, in registration order (upstream
/// `getRegisteredThemes`). The map is a snapshot: changing it changes nothing
/// registered.
pub fn get_registered_themes(cx: &App) -> OrderedMap<Arc<DefinedTheme>> {
    cx.try_global::<ThemeRegistry>()
        .map(|registry| registry.0.clone())
        .unwrap_or_default()
}

/// Empties the registry (upstream `resetThemes`, for tests).
pub fn reset_themes(cx: &mut App) {
    if cx.has_global::<ThemeRegistry>() {
        cx.global_mut::<ThemeRegistry>().0 = OrderedMap::new();
    }
}

/// A theme from a definition ([`define_theme`]), registered under its name: upstream
/// `defineTheme`, which registers every theme it returns.
pub fn define_theme_in(
    input: &DefineThemeInput,
    cx: &mut App,
) -> Result<Arc<DefinedTheme>, DefineThemeError> {
    let theme = Arc::new(define_theme(input)?);
    register_theme(theme.clone(), cx);
    Ok(theme)
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;
    use crate::authoring::define::TokenInput;

    fn brand(cx: &mut App, accent: Option<&str>) -> Arc<DefinedTheme> {
        let input = DefineThemeInput {
            tokens: accent.map(|accent| [("--color-accent", TokenInput::from(accent))].into()),
            ..DefineThemeInput::new("brand")
        };
        define_theme_in(&input, cx).unwrap()
    }

    // Upstream themeRegistry.test.ts: "registers and resolves themes by name".
    #[gpui::test]
    fn registers_and_resolves_themes_by_name(cx: &mut TestAppContext) {
        cx.update(|cx| {
            reset_themes(cx);
            let theme = brand(cx, Some("#123456"));
            let found = get_registered_theme(Some("brand"), cx).unwrap();
            assert!(Arc::ptr_eq(&found, &theme));
        });
    }

    // Upstream: "returns null for empty or unknown names".
    #[gpui::test]
    fn returns_none_for_empty_or_unknown_names(cx: &mut TestAppContext) {
        cx.update(|cx| {
            reset_themes(cx);
            // Before anything registers, and after.
            assert!(get_registered_theme(Some("missing"), cx).is_none());
            brand(cx, None);
            assert!(get_registered_theme(None, cx).is_none());
            assert!(get_registered_theme(Some(""), cx).is_none());
            assert!(get_registered_theme(Some("missing"), cx).is_none());
        });
    }

    // Upstream: "replaces an existing theme with the same name".
    #[gpui::test]
    fn replaces_an_existing_theme_with_the_same_name(cx: &mut TestAppContext) {
        cx.update(|cx| {
            reset_themes(cx);
            let first = brand(cx, None);
            let other = define_theme_in(&DefineThemeInput::new("other"), cx).unwrap();
            let second = brand(cx, Some("#654321"));
            let found = get_registered_theme(Some("brand"), cx).unwrap();
            assert!(!Arc::ptr_eq(&found, &first));
            assert!(Arc::ptr_eq(&found, &second));
            // Replaced in its place, as a JavaScript Map keeps a key's position.
            let names: Vec<String> = get_registered_themes(cx)
                .keys()
                .map(str::to_string)
                .collect();
            assert_eq!(names, ["brand", "other"]);
            assert!(Arc::ptr_eq(
                &get_registered_theme(Some("other"), cx).unwrap(),
                &other
            ));
        });
    }

    // Upstream: "returns a defensive snapshot of registered themes".
    #[gpui::test]
    fn returns_a_defensive_snapshot_of_registered_themes(cx: &mut TestAppContext) {
        cx.update(|cx| {
            reset_themes(cx);
            let theme = brand(cx, None);
            let mut snapshot = get_registered_themes(cx);
            assert!(Arc::ptr_eq(snapshot.get("brand").unwrap(), &theme));
            snapshot.remove("brand");
            assert!(get_registered_themes(cx).get("brand").is_some());
        });
    }

    // Upstream: "allows explicit registration of built theme objects".
    #[gpui::test]
    fn allows_explicit_registration_of_built_themes(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let theme = define_theme_in(&DefineThemeInput::new("built"), cx).unwrap();
            reset_themes(cx);
            assert!(get_registered_theme(Some("built"), cx).is_none());
            register_theme(theme.clone(), cx);
            assert!(Arc::ptr_eq(
                &get_registered_theme(Some("built"), cx).unwrap(),
                &theme
            ));
            // A built theme, one upstream's build compiled, registers the same way.
            let compiled = DefinedTheme {
                name: "compiled".into(),
                built: true,
                ..DefinedTheme::default()
            };
            register_theme(compiled, cx);
            assert!(get_registered_theme(Some("compiled"), cx).unwrap().built);
        });
    }
}
