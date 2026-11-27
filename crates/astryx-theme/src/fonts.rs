//! Font stacks that always resolve.
//!
//! GPUI panics when a font family and every fallback are missing, and CSS
//! stacks name fonts a platform may lack (`"SF Mono", Monaco, Consolas` has
//! no match in a browser). Before a theme draws, [`prepare_stacks`] keeps the
//! families the text system has and appends a bundled face of the stack's
//! kind, so text always draws.
//!
//! On macOS and iOS the generic system families a browser there resolves to
//! San Francisco (`-apple-system`, `BlinkMacSystemFont`, `system-ui`) become
//! GPUI's system UI font (`.SystemUIFont`), so a stack that starts with them
//! draws in the system font, as upstream's does in Safari or Chrome on a Mac.

use std::borrow::Cow;
use std::collections::HashSet;

use gpui::App;

use crate::types::{FontStack, StyleProp, TokenValue};
use crate::{Declaration, Theme, Tokens};

macro_rules! font {
    ($path:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../astryx-themes/fonts/",
            $path
        ))
    };
}

/// The bundled sans-serif fallback family.
pub const FALLBACK_SANS: &str = "Figtree";
/// The bundled monospace fallback family.
pub const FALLBACK_MONO: &str = "JetBrains Mono";

/// The fallback faces, registered by `astryx_theme::init`.
pub const FALLBACK_FONTS: &[&[u8]] = &[
    font!("figtree/Figtree-400.ttf"),
    font!("figtree/Figtree-500.ttf"),
    font!("figtree/Figtree-600.ttf"),
    font!("figtree/Figtree-700.ttf"),
    font!("jetbrains-mono/JetBrainsMono-400.ttf"),
    font!("jetbrains-mono/JetBrainsMono-500.ttf"),
    font!("jetbrains-mono/JetBrainsMono-600.ttf"),
    font!("jetbrains-mono/JetBrainsMono-700.ttf"),
];

/// The families the text system can draw, lowercased.
pub fn available_families(cx: &App) -> HashSet<String> {
    cx.text_system()
        .all_font_names()
        .into_iter()
        .map(|name| name.to_lowercase())
        .collect()
}

/// GPUI's name for the platform's UI font.
pub const SYSTEM_UI_FONT: &str = ".SystemUIFont";

/// The generic families Apple's browsers draw in the system UI font.
const APPLE_SYSTEM_FAMILIES: &[&str] = &["-apple-system", "BlinkMacSystemFont", "system-ui"];

/// Whether this build's text system is Apple's (Core Text), which draws
/// [`SYSTEM_UI_FONT`] as San Francisco.
const APPLE_TEXT_SYSTEM: bool = cfg!(any(target_os = "macos", target_os = "ios"));

/// `stack` with only the named families in `available`, then the bundled
/// fallback of the stack's kind. On Apple's text system the system families
/// become [`SYSTEM_UI_FONT`] (see the module docs).
pub fn resolve_stack(stack: &FontStack, available: &HashSet<String>) -> FontStack {
    resolve_stack_on(stack, available, APPLE_TEXT_SYSTEM)
}

/// [`resolve_stack`] on Apple's text system (`apple`) or another.
pub fn resolve_stack_on(stack: &FontStack, available: &HashSet<String>, apple: bool) -> FontStack {
    let fallback = if stack.is_monospace() {
        FALLBACK_MONO
    } else {
        FALLBACK_SANS
    };
    let mut families: Vec<Cow<'static, str>> = Vec::new();
    for family in stack.0.iter().map(|f| f.as_ref()) {
        let resolved = if apple && APPLE_SYSTEM_FAMILIES.contains(&family) {
            Some(SYSTEM_UI_FONT)
        } else if FontStack::is_generic(family) || !available.contains(&family.to_lowercase()) {
            None
        } else {
            Some(family)
        };
        if let Some(resolved) = resolved
            && !families.iter().any(|f| f.eq_ignore_ascii_case(resolved))
        {
            families.push(Cow::Owned(resolved.to_string()));
        }
    }
    if !families.iter().any(|f| f.eq_ignore_ascii_case(fallback)) {
        families.push(Cow::Borrowed(fallback));
    }
    FontStack(Cow::Owned(families))
}

fn resolve_tokens(tokens: &mut Tokens, available: &HashSet<String>) {
    for name in Tokens::NAMES {
        if let Some(TokenValue::Families(stack)) = tokens.get(name) {
            let _ = tokens.set(name, TokenValue::Families(resolve_stack(&stack, available)));
        }
    }
}

fn resolve_value(value: &mut TokenValue, available: &HashSet<String>) {
    if let TokenValue::Families(stack) = value {
        *stack = resolve_stack(stack, available);
    }
}

/// Rewrites every font stack in `theme` to one that resolves.
pub fn prepare_stacks(theme: &mut Theme, available: &HashSet<String>) {
    resolve_tokens(&mut theme.light, available);
    resolve_tokens(&mut theme.dark, available);
    for rule in theme.components.to_mut() {
        for declarations in [rule.light.to_mut(), rule.dark.to_mut()] {
            for declaration in declarations {
                match declaration {
                    Declaration::Style(StyleProp::FontFamily(stack)) => {
                        *stack = resolve_stack(stack, available);
                    }
                    Declaration::Token(_, value) => resolve_value(value, available),
                    Declaration::Style(_) => {}
                }
            }
        }
    }
    for adaptation in theme.adaptations.to_mut() {
        for (_, value) in adaptation
            .light
            .to_mut()
            .iter_mut()
            .chain(adaptation.dark.to_mut())
        {
            resolve_value(value, available);
        }
    }
    for surface in [&mut theme.on_dark, &mut theme.on_light]
        .into_iter()
        .flatten()
    {
        for (_, value) in surface.tokens.to_mut() {
            resolve_value(value, available);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(names: &[&'static str]) -> FontStack {
        FontStack(Cow::Owned(
            names.iter().map(|n| Cow::Borrowed(*n)).collect(),
        ))
    }

    fn names(stack: &FontStack) -> Vec<&str> {
        stack.0.iter().map(|f| f.as_ref()).collect()
    }

    #[test]
    fn keeps_available_families_and_appends_fallback() {
        let available = HashSet::from(["segoe ui".to_string(), "figtree".to_string()]);
        let body = stack(&["-apple-system", "Segoe UI", "Roboto", "sans-serif"]);
        assert_eq!(
            names(&resolve_stack_on(&body, &available, false)),
            ["Segoe UI", "Figtree"]
        );
    }

    #[test]
    fn apple_text_systems_draw_the_system_families_in_the_ui_font() {
        let available = HashSet::from([
            "helvetica".to_string(),
            "arial".to_string(),
            "figtree".to_string(),
        ]);
        let body = stack(&[
            "-apple-system",
            "BlinkMacSystemFont",
            "Segoe UI",
            "Roboto",
            "Helvetica",
            "Arial",
            "sans-serif",
        ]);
        assert_eq!(
            names(&resolve_stack_on(&body, &available, true)),
            [SYSTEM_UI_FONT, "Helvetica", "Arial", "Figtree"]
        );
        assert_eq!(
            names(&resolve_stack_on(&body, &available, false)),
            ["Helvetica", "Arial", "Figtree"]
        );
        // A stack without them is unchanged.
        let serif = stack(&["Georgia", "serif"]);
        assert_eq!(
            names(&resolve_stack_on(&serif, &available, true)),
            ["Figtree"]
        );
    }

    #[test]
    fn monospace_stack_falls_back_to_bundled_mono() {
        let code = stack(&["ui-monospace", "SF Mono", "Monaco", "monospace"]);
        assert_eq!(
            names(&resolve_stack(&code, &HashSet::new())),
            ["JetBrains Mono"]
        );
    }
}
