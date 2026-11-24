//! The generated themes against the upstream values they were exported from.

use std::sync::Arc;

use astryx_theme::{
    Appearance, Declaration, Pointer, ResolvedTheme, StyleProp, Theme, TokenError, TokenValue,
    Tokens,
};
use gpui::Hsla;

fn hex(color: Hsla) -> String {
    let c = color.to_rgb();
    let byte = |v: f32| (v * 255.0).round() as u8;
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        byte(c.r),
        byte(c.g),
        byte(c.b),
        byte(c.a)
    )
}

#[test]
fn every_theme_builds_with_fonts_and_distinct_modes() {
    let themes = astryx_themes::all();
    assert_eq!(themes.len(), 7);
    for theme in &themes {
        assert!(!theme.fonts.is_empty(), "{} ships fonts", theme.name);
        // Gothic is a dark-only theme: both modes draw its dark palette.
        if theme.name == "gothic" {
            continue;
        }
        assert_ne!(
            hex(theme.light.color.background_body),
            hex(theme.dark.color.background_body),
            "{} light and dark bodies differ",
            theme.name
        );
    }
}

#[test]
fn core_defaults_match_upstream() {
    // Upstream: '--color-accent': 'light-dark(#0064E0, #0171E3)'.
    let theme = Theme::default();
    assert_eq!(hex(theme.light.color.accent), "#0064E0FF");
    assert_eq!(hex(theme.dark.color.accent), "#0171E3FF");
    // Upstream: '--size-element-md': '32px'.
    let md = theme.light.size.element_md.to_pixels(gpui::px(16.));
    assert_eq!(f32::from(md), 32.0);
}

#[test]
fn token_set_and_get_round_trip() {
    let mut tokens: Tokens = Theme::default().light;
    let red: Hsla = gpui::rgba(0xff0000ff).into();
    tokens
        .set("--color-accent", TokenValue::Color(red))
        .unwrap();
    assert!(matches!(tokens.get("--color-accent"), Some(TokenValue::Color(c)) if c == red));
    assert_eq!(
        tokens.set("--spacing-4", TokenValue::Color(red)),
        Err(TokenError::WrongKind {
            token: "--spacing-4".into(),
            expected: "Length",
            got: "Color"
        })
    );
    assert!(matches!(
        tokens.set("--color-nope", TokenValue::Color(red)),
        Err(TokenError::Unknown(_))
    ));
}

#[test]
fn neutral_destructive_button_override_matches() {
    // Upstream neutral: button 'variant:destructive' sets backgroundColor to
    // var(--color-error-muted).
    let theme = Arc::new(astryx_themes::neutral::theme());
    let resolved = ResolvedTheme::new(theme.clone(), Appearance::Light, 1280.0, Pointer::Fine);
    let backgrounds: Vec<Hsla> = resolved
        .declarations(
            "button",
            &[("variant", "destructive"), ("size", "md")],
            None,
        )
        .filter_map(|d| match d {
            Declaration::Style(StyleProp::Background(c)) => Some(*c),
            _ => None,
        })
        .collect();
    assert_eq!(backgrounds.len(), 1);
    assert_eq!(hex(backgrounds[0]), hex(theme.light.color.error_muted));

    let primary = resolved
        .declarations("button", &[("variant", "primary")], None)
        .count();
    assert_eq!(primary, 0, "rules only match their own conditions");
}

#[test]
fn selected_item_scopes_token_overrides() {
    let theme = Arc::new(astryx_themes::neutral::theme());
    let resolved = ResolvedTheme::new(theme, Appearance::Dark, 1280.0, Pointer::Fine);
    let scoped = resolved
        .for_part("item", &[("selected", "true")])
        .expect("neutral re-defines a token for selected items");
    assert_ne!(
        hex(scoped.tokens().color.text_secondary),
        hex(resolved.tokens().color.text_secondary)
    );
    assert!(resolved.for_part("item", &[]).is_none());
}

#[test]
fn coarse_narrow_adaptation_rescales_type() {
    // Upstream neutral: (width < 1024px) and (pointer: coarse) sets the type
    // scale base to 16px.
    let theme = Arc::new(astryx_themes::neutral::theme());
    let desktop = ResolvedTheme::new(theme.clone(), Appearance::Light, 800.0, Pointer::Fine);
    let touch = ResolvedTheme::new(theme, Appearance::Light, 800.0, Pointer::Coarse);
    let rem = gpui::px(16.);
    let size = |r: &ResolvedTheme| f32::from(r.tokens().font.size.base.to_pixels(rem));
    assert_eq!(size(&desktop), 14.0);
    assert_eq!(size(&touch), 16.0);
}

#[test]
fn json_themes_match_their_compiled_twins() {
    // The compiled themes are generated from these files, so loading the JSON
    // at run time (live theme reload) must give the same tokens and rules.
    let cases = [
        (
            include_str!("../../../spec/themes/neutral.json"),
            astryx_themes::neutral::theme(),
        ),
        (
            include_str!("../../../spec/themes/butter.json"),
            astryx_themes::butter::theme(),
        ),
        (
            include_str!("../../../spec/themes/gothic.json"),
            astryx_themes::gothic::theme(),
        ),
    ];
    for (json, compiled) in cases {
        let loaded = Theme::from_json(json, &Theme::default()).unwrap();
        assert_eq!(loaded.name, compiled.name);
        for name in Tokens::NAMES {
            for mode in [Appearance::Light, Appearance::Dark] {
                let a = loaded.tokens(mode).get(name).map(|v| v.to_string());
                let b = compiled.tokens(mode).get(name).map(|v| v.to_string());
                assert_eq!(a, b, "{} {mode:?} {name}", compiled.name);
            }
        }
        assert_eq!(
            loaded.components.len(),
            compiled.components.len(),
            "{} rules",
            compiled.name
        );
        assert_eq!(loaded.adaptations.len(), compiled.adaptations.len());
    }
}
