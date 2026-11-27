//! A theme's `onDark` and `onLight` surfaces: upstream `theme/onMediaTokens.ts`.
//!
//! Content on an inverted surface (an inverted toast, a dark tooltip, a light
//! popover on a dark page) is drawn under the surface's color scheme, which
//! flips every `light-dark()` token. A few tokens need a different value on an
//! inverted surface than on a page of that mode: primary text and icons use the
//! surface's content color rather than the mode's grey, and the accent
//! collapses to it. These are the defaults; a theme's `onDark` and `onLight`
//! add token and component overrides of their own.

use super::define::{CssMap, TokenInputs};
use super::merge_components::{ComponentStyleMap, deep_merge_components};
use super::resolve_values::resolve_token_value;
use super::token_values::ResolvedThemeMode;

/// A theme's overrides for one kind of surface (upstream `OnMediaOverrides`):
/// the same shape as the theme's own tokens and components.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OnMediaOverrides {
    /// Token overrides on the surface.
    pub tokens: Option<TokenInputs>,
    /// Component style overrides on the surface.
    pub components: Option<ComponentStyleMap>,
}

/// A surface's resolved overrides, as a defined theme stores them (upstream
/// `ResolvedOnMedia`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedOnMedia {
    /// The token CSS values, `color-scheme` first.
    pub tokens: CssMap,
    /// Component style overrides.
    pub components: Option<ComponentStyleMap>,
}

/// The default overrides for content on a dark surface (upstream
/// `defaultOnDarkTokens`): the dark scheme, primary text and icons in
/// `--color-on-dark`, and the accent collapsed to it.
pub const DEFAULT_ON_DARK_TOKENS: [(&str, &str); 4] = [
    ("color-scheme", "dark"),
    ("--color-text-primary", "var(--color-on-dark)"),
    ("--color-icon-primary", "var(--color-on-dark)"),
    ("--color-accent", "var(--color-on-dark)"),
];

/// The default overrides for content on a light surface (upstream
/// `defaultOnLightTokens`), in `--color-on-light`.
pub const DEFAULT_ON_LIGHT_TOKENS: [(&str, &str); 4] = [
    ("color-scheme", "light"),
    ("--color-text-primary", "var(--color-on-light)"),
    ("--color-icon-primary", "var(--color-on-light)"),
    ("--color-accent", "var(--color-on-light)"),
];

/// A surface's overrides resolved (upstream `resolveOnMedia`): the defaults
/// for `surface`, then `base` (the resolved surface of the theme being
/// extended, so a child keeps its base's customizations), then `input`'s own
/// tokens, a `[light, dark]` tuple written as `light-dark()`. Components merge
/// the base's and the input's by [`deep_merge_components`].
pub fn resolve_on_media(
    surface: ResolvedThemeMode,
    input: Option<&OnMediaOverrides>,
    base: Option<&ResolvedOnMedia>,
) -> ResolvedOnMedia {
    let defaults = match surface {
        ResolvedThemeMode::Dark => DEFAULT_ON_DARK_TOKENS,
        ResolvedThemeMode::Light => DEFAULT_ON_LIGHT_TOKENS,
    };
    let mut tokens: CssMap = defaults
        .iter()
        .map(|&(name, value)| (name, value.to_string()))
        .collect();
    if let Some(base) = base {
        tokens.assign(&base.tokens);
    }
    for (key, value) in input
        .and_then(|input| input.tokens.as_ref())
        .into_iter()
        .flat_map(TokenInputs::iter)
    {
        if let Some(value) = value.value() {
            tokens.insert(key, resolve_token_value(value));
        }
    }
    ResolvedOnMedia {
        tokens,
        components: deep_merge_components(
            base.and_then(|base| base.components.as_ref()),
            input.and_then(|input| input.components.as_ref()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::super::define::TokenInput;
    use super::super::merge_components::StyleValue;
    use super::*;

    fn defaults(surface: ResolvedThemeMode) -> CssMap {
        let defaults = match surface {
            ResolvedThemeMode::Dark => DEFAULT_ON_DARK_TOKENS,
            ResolvedThemeMode::Light => DEFAULT_ON_LIGHT_TOKENS,
        };
        defaults.iter().map(|&(k, v)| (k, v.to_string())).collect()
    }

    fn overrides<const N: usize>(tokens: [(&str, TokenInput); N]) -> OnMediaOverrides {
        OnMediaOverrides {
            tokens: Some(tokens.into()),
            components: None,
        }
    }

    #[test]
    fn the_dark_defaults_set_the_scheme_and_the_on_dark_content() {
        let dark = defaults(ResolvedThemeMode::Dark);
        assert_eq!(dark.get("color-scheme").map(String::as_str), Some("dark"));
        for token in [
            "--color-text-primary",
            "--color-icon-primary",
            "--color-accent",
        ] {
            assert_eq!(
                dark.get(token).map(String::as_str),
                Some("var(--color-on-dark)"),
                "{token}"
            );
        }
        let light = defaults(ResolvedThemeMode::Light);
        assert_eq!(light.get("color-scheme").map(String::as_str), Some("light"));
        assert_eq!(
            light.get("--color-text-primary").map(String::as_str),
            Some("var(--color-on-light)")
        );
    }

    #[test]
    fn with_no_overrides_a_surface_is_its_defaults() {
        let dark = resolve_on_media(ResolvedThemeMode::Dark, None, None);
        assert_eq!(dark.tokens, defaults(ResolvedThemeMode::Dark));
        assert_eq!(dark.components, None);
        let light = resolve_on_media(ResolvedThemeMode::Light, None, None);
        assert_eq!(light.tokens, defaults(ResolvedThemeMode::Light));
    }

    #[test]
    fn merges_token_overrides_over_the_defaults() {
        let resolved = resolve_on_media(
            ResolvedThemeMode::Dark,
            Some(&overrides([(
                "--color-accent",
                TokenInput::Css("#90CAF9".into()),
            )])),
            None,
        );
        assert_eq!(
            resolved.tokens.get("--color-accent").map(String::as_str),
            Some("#90CAF9")
        );
        assert_eq!(
            resolved
                .tokens
                .get("--color-text-primary")
                .map(String::as_str),
            Some("var(--color-on-dark)")
        );
        let tuple = resolve_on_media(
            ResolvedThemeMode::Dark,
            Some(&overrides([(
                "--color-accent",
                TokenInput::Pair("#AAA".into(), "#BBB".into()),
            )])),
            None,
        );
        assert_eq!(
            tuple.tokens.get("--color-accent").map(String::as_str),
            Some("light-dark(#AAA, #BBB)")
        );
    }

    #[test]
    fn passes_component_overrides_through() {
        let components: ComponentStyleMap = [(
            "button",
            StyleValue::Block(
                [(
                    "variant:ghost",
                    StyleValue::Block([("borderWidth", StyleValue::Css("1px".into()))].into()),
                )]
                .into(),
            ),
        )]
        .into();
        let resolved = resolve_on_media(
            ResolvedThemeMode::Dark,
            Some(&OnMediaOverrides {
                tokens: None,
                components: Some(components.clone()),
            }),
            None,
        );
        assert_eq!(resolved.components, Some(components));
    }

    #[test]
    fn the_base_surface_sits_between_the_defaults_and_the_own_overrides() {
        let base = resolve_on_media(
            ResolvedThemeMode::Dark,
            Some(&overrides([
                ("--color-border", TokenInput::Css("#ffffff".into())),
                ("--color-accent", TokenInput::Css("#90CAF9".into())),
            ])),
            None,
        );
        let child = resolve_on_media(
            ResolvedThemeMode::Dark,
            Some(&overrides([
                ("--color-border", TokenInput::Css("#cccccc".into())),
                ("--color-track", TokenInput::Unset),
            ])),
            Some(&base),
        );
        let token = |name: &str| child.tokens.get(name).map(String::as_str);
        assert_eq!(token("--color-border"), Some("#cccccc"));
        assert_eq!(token("--color-accent"), Some("#90CAF9"));
        assert_eq!(token("--color-track"), None);
        assert_eq!(child.tokens.keys().next(), Some("color-scheme"));
    }
}
