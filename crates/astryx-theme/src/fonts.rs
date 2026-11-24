//! Font stacks that always resolve.
//!
//! GPUI panics when a font family and every fallback are missing, and CSS
//! stacks name fonts a platform may lack (`"SF Mono", Monaco, Consolas` has
//! no match in a browser). Before a theme draws, [`prepare_stacks`] keeps the
//! families the text system has and appends a bundled face of the stack's
//! kind, so text always draws.

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

/// `stack` with only the named families in `available`, then the bundled
/// fallback of the stack's kind.
pub fn resolve_stack(stack: &FontStack, available: &HashSet<String>) -> FontStack {
    let fallback = if stack.is_monospace() {
        FALLBACK_MONO
    } else {
        FALLBACK_SANS
    };
    let mut families: Vec<Cow<'static, str>> = stack
        .named()
        .filter(|family| available.contains(&family.to_lowercase()))
        .map(|family| Cow::Owned(family.to_string()))
        .collect();
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
            names(&resolve_stack(&body, &available)),
            ["Segoe UI", "Figtree"]
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
