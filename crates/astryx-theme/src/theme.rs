//! The active theme: which theme draws, in which mode, and how a component
//! reads its tokens and override rules during render.
//!
//! The app sets one global theme and mode (`set_theme`, `set_mode`). A
//! subtree can draw with another theme or appearance through [`scope`], the
//! counterpart of a nested Astryx `<Theme>` or `<MediaTheme>`. Components
//! call [`current`] in their render.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use astryx_primitives::{Provider, provide, use_context};
use gpui::{
    AbsoluteLength, App, DefiniteLength, Fill, FontFallbacks, Global, IntoElement, SharedString,
    StyleRefinement, Window, WindowAppearance,
};

use crate::fonts;
use crate::generated::{default_theme, tokens::Tokens};
use crate::types::*;

/// A complete theme: tokens for both appearances plus its component
/// overrides, adaptations, icons and fonts.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: Cow<'static, str>,
    pub light: Tokens,
    pub dark: Tokens,
    pub components: Cow<'static, [ComponentRule]>,
    pub adaptations: Cow<'static, [Adaptation]>,
    pub on_dark: Option<SurfaceOverride>,
    pub on_light: Option<SurfaceOverride>,
    /// Semantic icon name to the asset path of the SVG it draws.
    pub icons: Cow<'static, [(Cow<'static, str>, Cow<'static, str>)]>,
    /// TTF/OTF files the theme's font families need.
    pub fonts: Cow<'static, [&'static [u8]]>,
}

impl Default for Theme {
    /// Core's defaults: system fonts and core's fallback icons.
    fn default() -> Self {
        Theme {
            name: Cow::Borrowed("default"),
            light: default_theme::LIGHT,
            dark: default_theme::DARK,
            components: Cow::Borrowed(&[]),
            adaptations: Cow::Borrowed(&[]),
            on_dark: None,
            on_light: None,
            icons: Cow::Borrowed(&[]),
            fonts: Cow::Borrowed(&[]),
        }
    }
}

impl Theme {
    pub fn tokens(&self, appearance: Appearance) -> &Tokens {
        match appearance {
            Appearance::Light => &self.light,
            Appearance::Dark => &self.dark,
        }
    }

    /// The asset path of the SVG this theme draws for a semantic icon name.
    pub fn icon(&self, name: &str) -> Option<&str> {
        self.icons
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, path)| path.as_ref())
    }

    /// Adds component override rules after the theme's own, so they win where
    /// both apply. This is how an app restyles a component everywhere, the
    /// way a theme's `components` map does.
    pub fn with_rules(mut self, rules: impl IntoIterator<Item = ComponentRule>) -> Self {
        self.components.to_mut().extend(rules);
        self
    }

    /// Re-defines tokens in one appearance, or both when `appearance` is
    /// `None`. Fails on an unknown token or a value of the wrong kind.
    pub fn with_token_values(
        mut self,
        appearance: Option<Appearance>,
        overrides: impl IntoIterator<Item = (&'static str, TokenValue)>,
    ) -> Result<Self, TokenError> {
        for (name, value) in overrides {
            if appearance != Some(Appearance::Dark) {
                self.light.set(name, value.clone())?;
            }
            if appearance != Some(Appearance::Light) {
                self.dark.set(name, value)?;
            }
        }
        Ok(self)
    }

    fn surface(&self, appearance: Appearance) -> Option<&SurfaceOverride> {
        match appearance {
            Appearance::Dark => self.on_dark.as_ref(),
            Appearance::Light => self.on_light.as_ref(),
        }
    }
}

/// Which appearance the app draws in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    /// Follow the operating system's light or dark setting.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn resolve(self, window: &Window) -> Appearance {
        match self {
            ThemeMode::Light => Appearance::Light,
            ThemeMode::Dark => Appearance::Dark,
            ThemeMode::System => match window.appearance() {
                WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
                WindowAppearance::Light | WindowAppearance::VibrantLight => Appearance::Light,
            },
        }
    }
}

/// A theme resolved for drawing: one appearance, with adaptations and any
/// scoped token overrides already applied.
#[derive(Clone, Debug)]
pub struct ResolvedTheme {
    pub theme: Arc<Theme>,
    pub appearance: Appearance,
    tokens: Arc<Tokens>,
}

impl ResolvedTheme {
    /// Resolves `theme` in `appearance`, applying the adaptations that hold
    /// for the window's width and pointer.
    pub fn new(theme: Arc<Theme>, appearance: Appearance, width: f32, pointer: Pointer) -> Self {
        let mut tokens = theme.tokens(appearance).clone();
        for adaptation in theme.adaptations.iter() {
            if adaptation.applies(width, pointer) {
                let overrides = match appearance {
                    Appearance::Light => &adaptation.light,
                    Appearance::Dark => &adaptation.dark,
                };
                apply_tokens(&mut tokens, overrides.iter().map(|(n, v)| (n.as_ref(), v)));
            }
        }
        ResolvedTheme {
            theme,
            appearance,
            tokens: Arc::new(tokens),
        }
    }

    pub fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    /// The same theme with `overrides` re-defining tokens, as a theme's
    /// token declarations do for a component and what it contains.
    pub fn with_tokens<'a>(
        &self,
        overrides: impl IntoIterator<Item = (&'a str, &'a TokenValue)>,
    ) -> Self {
        let mut tokens = (*self.tokens).clone();
        apply_tokens(&mut tokens, overrides);
        ResolvedTheme {
            theme: self.theme.clone(),
            appearance: self.appearance,
            tokens: Arc::new(tokens),
        }
    }

    /// The theme as content on an inverted surface (`MediaTheme`): the
    /// surface's appearance plus the theme's `onDark`/`onLight` overrides.
    pub fn on_surface(&self, appearance: Appearance, width: f32, pointer: Pointer) -> Self {
        let resolved = ResolvedTheme::new(self.theme.clone(), appearance, width, pointer);
        match self.theme.surface(appearance) {
            Some(surface) => {
                resolved.with_tokens(surface.tokens.iter().map(|(n, v)| (n.as_ref(), v)))
            }
            None => resolved,
        }
    }

    /// The declarations of every override rule for `target` whose conditions
    /// hold for `props`, in `state` (`None` is the resting state), in theme
    /// order.
    pub fn declarations<'a>(
        &'a self,
        target: &'a str,
        props: &'a [(&'a str, &'a str)],
        state: Option<RuleState>,
    ) -> impl Iterator<Item = &'a Declaration> + 'a {
        let appearance = self.appearance;
        self.theme
            .components
            .iter()
            .filter(move |rule| rule.state == state && rule.matches(target, props))
            .flat_map(move |rule| rule.declarations(appearance).iter())
    }

    /// The theme a component part's children draw with: this theme plus the
    /// token re-definitions its override rules make, if any.
    pub fn for_part(&self, target: &str, props: &[(&str, &str)]) -> Option<Self> {
        let overrides: Vec<(&str, &TokenValue)> = self
            .declarations(target, props, None)
            .filter_map(|d| match d {
                Declaration::Token(name, value) => Some((name.as_ref(), value)),
                Declaration::Style(_) => None,
            })
            .collect();
        (!overrides.is_empty()).then(|| self.with_tokens(overrides))
    }

    /// Applies the style declarations of every matching rule in `state` to a
    /// style refinement, after the component's own defaults.
    pub fn refine(
        &self,
        style: &mut StyleRefinement,
        target: &str,
        props: &[(&str, &str)],
        state: Option<RuleState>,
    ) {
        for declaration in self.declarations(target, props, state) {
            if let Declaration::Style(prop) = declaration {
                apply_style(style, prop);
            }
        }
    }

    /// True when some rule for `target` styles `state`, so the component
    /// knows whether to install a hover or active style.
    pub fn has_state(&self, target: &str, props: &[(&str, &str)], state: RuleState) -> bool {
        self.declarations(target, props, Some(state))
            .next()
            .is_some()
    }
}

fn apply_tokens<'a>(
    tokens: &mut Tokens,
    overrides: impl IntoIterator<Item = (&'a str, &'a TokenValue)>,
) {
    for (name, value) in overrides {
        // A theme may re-define a token this port does not type yet; skip it.
        let _ = tokens.set(name, value.clone());
    }
}

/// Writes one theme style declaration into a GPUI style refinement.
pub fn apply_style(style: &mut StyleRefinement, prop: &StyleProp) {
    match prop {
        StyleProp::Background(color) => style.background = Some(Fill::from(*color)),
        StyleProp::TextColor(color) => style.text.color = Some(*color),
        StyleProp::BorderColor(color) => style.border_color = Some(*color),
        StyleProp::BorderWidth(width) => {
            let edges = &mut style.border_widths;
            edges.top = Some(*width);
            edges.right = Some(*width);
            edges.bottom = Some(*width);
            edges.left = Some(*width);
        }
        StyleProp::BorderStyle(keyword) => {
            style.border_style = Some(match keyword.0.as_ref() {
                "dashed" | "dotted" => gpui::BorderStyle::Dashed,
                _ => gpui::BorderStyle::Solid,
            });
        }
        StyleProp::Radius(radius) => {
            let corners = &mut style.corner_radii;
            corners.top_left = Some(*radius);
            corners.top_right = Some(*radius);
            corners.bottom_right = Some(*radius);
            corners.bottom_left = Some(*radius);
        }
        StyleProp::Padding(edges) => {
            style.padding.top = Some(definite(edges.top));
            style.padding.right = Some(definite(edges.right));
            style.padding.bottom = Some(definite(edges.bottom));
            style.padding.left = Some(definite(edges.left));
        }
        StyleProp::PaddingInline(start, end) => {
            style.padding.left = Some(definite(*start));
            style.padding.right = Some(definite(*end));
        }
        StyleProp::PaddingBlock(start, end) => {
            style.padding.top = Some(definite(*start));
            style.padding.bottom = Some(definite(*end));
        }
        StyleProp::FontFamily(stack) => {
            let mut named = stack.named().map(SharedString::new);
            if let Some(first) = named.next() {
                style.text.font_family = Some(first);
                let rest: Vec<String> = named.map(|n| n.to_string()).collect();
                if !rest.is_empty() {
                    style.text.font_fallbacks = Some(FontFallbacks::from_fonts(rest));
                }
            }
        }
        StyleProp::FontSize(size) => style.text.font_size = Some(*size),
        StyleProp::FontWeight(weight) => style.text.font_weight = Some(*weight),
        StyleProp::LineHeight(leading) => {
            style.text.line_height = Some(DefiniteLength::Fraction(*leading));
        }
        StyleProp::Height(height) => style.size.height = Some(definite(*height).into()),
        StyleProp::Shadow(shadow) => style.box_shadow = Some(shadow.layers()),
        StyleProp::Opacity(opacity) => style.opacity = Some(*opacity),
    }
}

fn definite(length: AbsoluteLength) -> DefiniteLength {
    DefiniteLength::Absolute(length)
}

// ---------------------------------------------------------------------------
// Global state and scopes
// ---------------------------------------------------------------------------

struct ThemeState {
    theme: Arc<Theme>,
    mode: ThemeMode,
    cache: RefCell<HashMap<(Appearance, u32, bool), ResolvedTheme>>,
    registered_fonts: RefCell<HashSet<String>>,
}

impl Global for ThemeState {}

/// A scoped theme, provided to a subtree by [`scope`].
#[derive(Clone, Debug)]
pub struct ThemeScope(pub ResolvedTheme);

/// Registers the bundled fallback fonts and installs core's default theme in
/// system mode. `astryx::init` calls this.
pub fn init(cx: &mut App) {
    if cx.has_global::<ThemeState>() {
        return;
    }
    let fallbacks = fonts::FALLBACK_FONTS
        .iter()
        .map(|f| Cow::Borrowed(*f))
        .collect();
    if let Err(error) = cx.text_system().add_fonts(fallbacks) {
        log_font_error("fallback", &error);
    }
    let mut theme = Theme::default();
    fonts::prepare_stacks(&mut theme, &fonts::available_families(cx));
    cx.set_global(ThemeState {
        theme: Arc::new(theme),
        mode: ThemeMode::System,
        cache: RefCell::default(),
        registered_fonts: RefCell::default(),
    });
}

/// Readies a theme to draw: registers its fonts and rewrites its font stacks
/// to families the text system has (see [`fonts`]). `set_theme` does this;
/// call it for a theme drawn through a nested [`scope`].
pub fn prepare(theme: impl Into<Arc<Theme>>, cx: &mut App) -> Arc<Theme> {
    init(cx);
    let mut theme = Arc::unwrap_or_clone(theme.into());
    register_fonts(&theme, cx);
    fonts::prepare_stacks(&mut theme, &fonts::available_families(cx));
    Arc::new(theme)
}

fn state(cx: &App) -> &ThemeState {
    cx.try_global::<ThemeState>()
        .expect("astryx_theme::init(cx) must run before drawing Astryx components")
}

/// Makes `theme` the app's theme, registers its fonts and redraws.
pub fn set_theme(theme: impl Into<Arc<Theme>>, cx: &mut App) {
    let theme = prepare(theme, cx);
    let state = cx.global_mut::<ThemeState>();
    state.theme = theme;
    state.cache.get_mut().clear();
    cx.refresh_windows();
}

/// Sets light, dark or system mode and redraws.
pub fn set_mode(mode: ThemeMode, cx: &mut App) {
    init(cx);
    let state = cx.global_mut::<ThemeState>();
    state.mode = mode;
    state.cache.get_mut().clear();
    cx.refresh_windows();
}

/// The app's theme and mode.
pub fn active(cx: &App) -> (Arc<Theme>, ThemeMode) {
    let state = state(cx);
    (state.theme.clone(), state.mode)
}

/// Registers a theme's font files with the text system, once per theme.
pub fn register_fonts(theme: &Theme, cx: &mut App) {
    if theme.fonts.is_empty() {
        return;
    }
    init(cx);
    let first = cx
        .global::<ThemeState>()
        .registered_fonts
        .borrow_mut()
        .insert(theme.name.to_string());
    if first {
        let fonts = theme.fonts.iter().map(|f| Cow::Borrowed(*f)).collect();
        if let Err(error) = cx.text_system().add_fonts(fonts) {
            log_font_error(&theme.name, &error);
        }
    }
}

fn log_font_error(theme: &str, error: &anyhow::Error) {
    eprintln!("astryx: failed to load the {theme} theme's fonts: {error:#}");
}

/// The theme a component draws with during render: the nearest [`scope`],
/// else the app's theme in its mode for this window.
pub fn current(window: &Window, cx: &App) -> ResolvedTheme {
    if let Some(ThemeScope(scoped)) = use_context::<ThemeScope>(cx) {
        return scoped;
    }
    let state = state(cx);
    let appearance = state.mode.resolve(window);
    let width = f32::from(window.viewport_size().width);
    let pointer = Pointer::Fine;
    // Adaptations only change at their breakpoints; key the cache on which
    // ones hold, not on the exact width.
    let mut mask = 0u32;
    for (i, adaptation) in state.theme.adaptations.iter().enumerate().take(32) {
        if adaptation.applies(width, pointer) {
            mask |= 1 << i;
        }
    }
    let key = (appearance, mask, pointer == Pointer::Coarse);
    if let Some(hit) = state.cache.borrow().get(&key) {
        return hit.clone();
    }
    let resolved = ResolvedTheme::new(state.theme.clone(), appearance, width, pointer);
    state.cache.borrow_mut().insert(key, resolved.clone());
    resolved
}

/// Draws `child` with `theme` instead of the surrounding one.
pub fn scope(theme: ResolvedTheme, child: impl IntoElement) -> Provider<ThemeScope> {
    provide(ThemeScope(theme), child)
}

/// Draws `child` with some tokens re-defined, the counterpart of setting CSS
/// custom properties on a subtree. Unknown tokens are ignored.
///
/// ```ignore
/// with_tokens([("--color-accent", TokenValue::Color(brand))], card, window, cx)
/// ```
pub fn with_tokens<'a>(
    overrides: impl IntoIterator<Item = (&'a str, TokenValue)>,
    child: impl IntoElement,
    window: &Window,
    cx: &App,
) -> Provider<ThemeScope> {
    let overrides: Vec<(&str, TokenValue)> = overrides.into_iter().collect();
    let theme = current(window, cx).with_tokens(overrides.iter().map(|(n, v)| (*n, v)));
    scope(theme, child)
}
