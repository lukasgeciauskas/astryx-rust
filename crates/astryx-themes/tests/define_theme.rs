//! `define_theme` against the shipped themes: each theme's definition (the
//! JSON-able part of upstream's `defineTheme` input, `spec/theme-definitions`)
//! defined at run time and made a typed theme must give exactly the tokens,
//! component rules and surfaces of its compiled twin, which is generated from
//! upstream's own resolved output.
//!
//! Adaptations are not compared: `define_theme` stores them as written.
//!
//! Rules are compared by what selects them (target, conditions, state) and in
//! order, since a later rule wins: the definition files keep upstream's key
//! order, and the JSON reader keeps it.

use std::collections::BTreeMap;

use astryx_theme::authoring::define::{DefineThemeInput, define_theme};
use astryx_theme::authoring::to_theme::to_theme;
use astryx_theme::{
    AbsoluteLength, Appearance, ComponentRule, Declaration, StyleProp, SurfaceOverride, Theme,
    Tokens, hex,
};

fn length(length: &AbsoluteLength) -> String {
    match length {
        AbsoluteLength::Pixels(px) => format!("{}px", f32::from(*px)),
        AbsoluteLength::Rems(rems) => format!("{}rem", rems.0),
    }
}

/// A style as text, every value in it.
fn style(prop: &StyleProp) -> String {
    match prop {
        StyleProp::Background(c) => format!("background {}", hex(*c)),
        StyleProp::TextColor(c) => format!("color {}", hex(*c)),
        StyleProp::BorderColor(c) => format!("border-color {}", hex(*c)),
        StyleProp::BorderWidth(l) => format!("border-width {}", length(l)),
        StyleProp::BorderStyle(k) => format!("border-style {}", k.0),
        StyleProp::Radius(l) => format!("radius {}", length(l)),
        StyleProp::Padding(e) => format!(
            "padding {} {} {} {}",
            length(&e.top),
            length(&e.right),
            length(&e.bottom),
            length(&e.left)
        ),
        StyleProp::PaddingInline(a, b) => format!("padding-inline {} {}", length(a), length(b)),
        StyleProp::PaddingBlock(a, b) => format!("padding-block {} {}", length(a), length(b)),
        StyleProp::FontFamily(f) => format!("font-family {:?}", f.0),
        StyleProp::FontSize(l) => format!("font-size {}", length(l)),
        StyleProp::FontWeight(w) => format!("font-weight {}", w.0),
        StyleProp::LineHeight(l) => format!("line-height {l}"),
        StyleProp::Height(l) => format!("height {}", length(l)),
        StyleProp::Shadow(s) => format!("shadow {}", astryx_theme::TokenValue::Shadow(s.clone())),
        StyleProp::Opacity(o) => format!("opacity {o}"),
    }
}

fn declaration(declaration: &Declaration) -> String {
    match declaration {
        Declaration::Style(prop) => style(prop),
        Declaration::Token(name, value) => format!("{name}: {value}"),
    }
}

/// What selects a rule: its target, conditions and state.
fn selects(rule: &ComponentRule) -> String {
    let when: Vec<String> = rule.when.iter().map(|(p, v)| format!("{p}={v}")).collect();
    format!("{}[{}]{:?}", rule.target, when.join("+"), rule.state)
}

/// A theme's rules by what selects them, each with its light and dark
/// declarations; a selection two rules share is a difference of its own.
fn rules(theme: &Theme, differences: &mut Vec<String>) -> BTreeMap<String, [Vec<String>; 2]> {
    let mut out = BTreeMap::new();
    for rule in theme.components.iter() {
        let declarations = [
            rule.light.iter().map(declaration).collect(),
            rule.dark.iter().map(declaration).collect(),
        ];
        if out.insert(selects(rule), declarations).is_some() {
            differences.push(format!(
                "{}: two rules select {}",
                theme.name,
                selects(rule)
            ));
        }
    }
    out
}

fn surface(surface: &Option<SurfaceOverride>) -> Option<(Appearance, Vec<String>)> {
    surface.as_ref().map(|s| {
        (
            s.appearance,
            s.tokens.iter().map(|(n, v)| format!("{n}: {v}")).collect(),
        )
    })
}

/// Every way `defined` differs from `compiled`: tokens in both modes, rules
/// and surfaces.
fn differences(defined: &Theme, compiled: &Theme) -> Vec<String> {
    let mut out = Vec::new();
    let name = &compiled.name;
    if defined.name != compiled.name {
        out.push(format!("name {} is not {name}", defined.name));
    }
    for token in Tokens::NAMES {
        for mode in [Appearance::Light, Appearance::Dark] {
            let a = defined.tokens(mode).get(token).map(|v| v.to_string());
            let b = compiled.tokens(mode).get(token).map(|v| v.to_string());
            if a != b {
                out.push(format!(
                    "{name} {mode:?} {token}: defined {a:?}, compiled {b:?}"
                ));
            }
        }
    }
    let ours = rules(defined, &mut out);
    let theirs = rules(compiled, &mut out);
    if defined.components.len() != compiled.components.len() {
        out.push(format!(
            "{name}: {} rules defined, {} compiled",
            defined.components.len(),
            compiled.components.len()
        ));
    }
    for (key, declarations) in &theirs {
        match ours.get(key) {
            None => out.push(format!("{name}: no defined rule selects {key}")),
            Some(defined) if defined != declarations => out.push(format!(
                "{name} {key}: defined {defined:?}, compiled {declarations:?}"
            )),
            Some(_) => {}
        }
    }
    for key in ours.keys().filter(|key| !theirs.contains_key(*key)) {
        out.push(format!("{name}: no compiled rule selects {key}"));
    }
    let order = |theme: &Theme| theme.components.iter().map(selects).collect::<Vec<_>>();
    if order(defined) != order(compiled) {
        out.push(format!(
            "{name}: rules in another order: defined {:?}, compiled {:?}",
            order(defined),
            order(compiled)
        ));
    }
    for (which, a, b) in [
        ("onDark", &defined.on_dark, &compiled.on_dark),
        ("onLight", &defined.on_light, &compiled.on_light),
    ] {
        if surface(a) != surface(b) {
            out.push(format!(
                "{name} {which}: defined {:?}, compiled {:?}",
                surface(a),
                surface(b)
            ));
        }
    }
    out
}

fn defined(json: &str) -> Theme {
    let json: serde_json::Value = serde_json::from_str(json).unwrap();
    let input = DefineThemeInput::from_json(&json).unwrap();
    define_theme(&input).unwrap().to_theme()
}

#[test]
fn every_shipped_theme_defined_from_its_definition_matches_its_compiled_twin() {
    let cases = [
        (
            include_str!("../../../spec/theme-definitions/butter.json"),
            astryx_themes::butter::theme(),
        ),
        (
            include_str!("../../../spec/theme-definitions/chocolate.json"),
            astryx_themes::chocolate::theme(),
        ),
        (
            include_str!("../../../spec/theme-definitions/gothic.json"),
            astryx_themes::gothic::theme(),
        ),
        (
            include_str!("../../../spec/theme-definitions/matcha.json"),
            astryx_themes::matcha::theme(),
        ),
        (
            include_str!("../../../spec/theme-definitions/neutral.json"),
            astryx_themes::neutral::theme(),
        ),
        (
            include_str!("../../../spec/theme-definitions/stone.json"),
            astryx_themes::stone::theme(),
        ),
        (
            include_str!("../../../spec/theme-definitions/y2k.json"),
            astryx_themes::y2k::theme(),
        ),
    ];
    let mut all = Vec::new();
    for (json, compiled) in cases {
        all.extend(differences(&defined(json), &compiled));
    }
    assert!(
        all.is_empty(),
        "{} differences:\n{}",
        all.len(),
        all.join("\n")
    );
}

#[test]
fn the_default_theme_defined_from_a_name_alone_matches_cores_defaults() {
    let theme = define_theme(&DefineThemeInput::new("default"))
        .unwrap()
        .to_theme();
    // Core's own theme has no surfaces (upstream exports it from no theme at
    // all); a defined theme always has the default ones.
    let core = Theme {
        on_dark: theme.on_dark.clone(),
        on_light: theme.on_light.clone(),
        ..Theme::default()
    };
    let found = differences(&theme, &core);
    assert!(found.is_empty(), "{}", found.join("\n"));
    let on_dark = surface(&theme.on_dark).unwrap();
    assert_eq!(on_dark.0, Appearance::Dark);
    let names: Vec<&str> = on_dark
        .1
        .iter()
        .map(|t| t.split(':').next().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "--color-accent",
            "--color-icon-primary",
            "--color-text-primary"
        ]
    );
    assert_eq!(surface(&theme.on_light).unwrap().0, Appearance::Light);

    // No theme at all is core's default theme, surfaces and all.
    let none = to_theme(None);
    let found = differences(&none, &Theme::default());
    assert!(found.is_empty(), "{}", found.join("\n"));
    assert!(none.on_dark.is_none() && none.on_light.is_none());
}

#[test]
fn refuses_a_local_tokens_field_that_holds_undefined_by_name() {
    // defineTheme.test.ts 'refuses a localTokens field that holds undefined,
    // by name'; JSON's null stands for the undefined.
    let json = serde_json::json!({"name": "no-local", "localTokens": null});
    assert_eq!(
        DefineThemeInput::from_json(&json).unwrap_err().to_string(),
        "defineTheme(\"no-local\"): localTokens is undefined: omit the field or pass an object."
    );
}
