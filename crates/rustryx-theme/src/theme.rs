//! The active theme: which theme draws, in which mode, and how a component
//! reads its tokens and override rules during render.
//!
//! The app sets one global theme and mode (`set_theme`, `set_mode`). A
//! subtree can draw with another theme or appearance through [`scope`], the
//! counterpart of a nested Astryx `<Theme>` or `<MediaTheme>`. Components
//! call [`current`] in their render.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui::{
    AbsoluteLength, App, DefiniteLength, Fill, FontFallbacks, Global, IntoElement, SharedString,
    StyleRefinement, Window, WindowAppearance, WindowId,
};
use rustryx_primitives::{Provider, provide, use_context};

use crate::authoring::dynamic_type::{
    TextSizeCategory, reader_text_size_tokens, system_text_size_category,
};
use crate::authoring::theme_motion::MOTION_TEMPO_PROPERTY;
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
    /// Upstream `widthBreakpoints`: the named width points AppShell's mobile layout uses.
    pub width_breakpoints: WidthBreakpoints,
    pub on_dark: Option<SurfaceOverride>,
    pub on_light: Option<SurfaceOverride>,
    /// Semantic icon name to the asset path of the SVG it draws.
    pub icons: Cow<'static, [(Cow<'static, str>, Cow<'static, str>)]>,
    /// TTF/OTF files the theme's font families need.
    pub fonts: Cow<'static, [&'static [u8]]>,
    /// The tempo the theme's motion plays at (upstream's `--_astryx-motion-tempo` at
    /// the theme's root, which the animator reads): 1 is Astryx's own, 0.75 the
    /// quickest and 1.5 the slowest. A theme that sets no `tempo`, or a duration
    /// scale alone, plays at 1.
    pub motion_tempo: f64,
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
            width_breakpoints: WidthBreakpoints::DEFAULT,
            on_dark: None,
            on_light: None,
            icons: Cow::Borrowed(&[]),
            fonts: Cow::Borrowed(&[]),
            motion_tempo: 1.0,
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
/// scoped token overrides already applied, its font sizes at the reader's text size.
#[derive(Clone, Debug)]
pub struct ResolvedTheme {
    pub theme: Arc<Theme>,
    pub appearance: Appearance,
    text_size: TextSizeCategory,
    motion_tempo: f64,
    tokens: Arc<Tokens>,
}

/// The tempo `theme`'s motion plays at in `appearance` for the window's width and
/// pointer: the last `--_astryx-motion-tempo` its applying adaptations write, else its
/// root [`Theme::motion_tempo`] (upstream `themeTempo`, which reads the property where
/// the element is), as upstream writes the property at the theme's root and in its
/// adaptation rules.
fn theme_motion_tempo(theme: &Theme, appearance: Appearance, width: f32, pointer: Pointer) -> f64 {
    let mut tempo = Some(theme.motion_tempo);
    for adaptation in theme.adaptations.iter() {
        if !adaptation.applies(width, pointer) {
            continue;
        }
        let overrides = match appearance {
            Appearance::Light => &adaptation.light,
            Appearance::Dark => &adaptation.dark,
        };
        for (name, value) in overrides.iter() {
            if name == MOTION_TEMPO_PROPERTY
                && let TokenValue::Number(value) = value
            {
                tempo = Some(f64::from(*value));
            }
        }
    }
    rustryx_primitives::motion::theme_tempo(tempo)
}

/// `theme`'s tokens in `appearance`: the adaptations that hold for the window's
/// width and pointer, then a surface's overrides, then the font sizes at the
/// reader's text size `category`, as upstream scales the sizes a theme's root, its
/// adaptation rules and its media surfaces write.
fn resolve_tokens(
    theme: &Theme,
    appearance: Appearance,
    width: f32,
    pointer: Pointer,
    surface: Option<&SurfaceOverride>,
    category: TextSizeCategory,
) -> Tokens {
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
    if let Some(surface) = surface {
        apply_tokens(
            &mut tokens,
            surface.tokens.iter().map(|(n, v)| (n.as_ref(), v)),
        );
    }
    reader_text_size_tokens(&mut tokens, category);
    tokens
}

impl ResolvedTheme {
    /// Resolves `theme` in `appearance`, applying the adaptations that hold
    /// for the window's width and pointer, at the default text size (Large: the
    /// theme's own font sizes).
    pub fn new(theme: Arc<Theme>, appearance: Appearance, width: f32, pointer: Pointer) -> Self {
        Self::at_text_size(theme, appearance, width, pointer, TextSizeCategory::L)
    }

    /// [`ResolvedTheme::new`] at the reader's text size `category`: every font size
    /// scaled on its UIKit curve
    /// ([`reader_text_size_tokens`](crate::authoring::dynamic_type::reader_text_size_tokens)).
    /// [`current`] resolves the app's theme at the app's category
    /// ([`text_size_category`](crate::authoring::dynamic_type::text_size_category)).
    pub fn at_text_size(
        theme: Arc<Theme>,
        appearance: Appearance,
        width: f32,
        pointer: Pointer,
        category: TextSizeCategory,
    ) -> Self {
        let tokens = resolve_tokens(&theme, appearance, width, pointer, None, category);
        ResolvedTheme {
            motion_tempo: theme_motion_tempo(&theme, appearance, width, pointer),
            theme,
            appearance,
            text_size: category,
            tokens: Arc::new(tokens),
        }
    }

    pub fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    /// The reader's text size category the font sizes are drawn at.
    pub fn text_size_category(&self) -> TextSizeCategory {
        self.text_size
    }

    /// The tempo the theme's springs and timed motion play at (upstream
    /// `themeTempo`, which reads `--_astryx-motion-tempo`): the one the theme or an
    /// applying adaptation writes, else 1. A motion engine takes it with
    /// `MotionEngine::set_tempo`; every shipped theme plays at 1.
    pub fn motion_tempo(&self) -> f64 {
        self.motion_tempo
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
            text_size: self.text_size,
            motion_tempo: self.motion_tempo,
            tokens: Arc::new(tokens),
        }
    }

    /// The theme as content on an inverted surface (`MediaTheme`): the
    /// surface's appearance plus the theme's `onDark`/`onLight` overrides, at
    /// this theme's text size (the surface's font sizes scale too).
    pub fn on_surface(&self, appearance: Appearance, width: f32, pointer: Pointer) -> Self {
        let surface = self.theme.surface(appearance);
        let tokens = resolve_tokens(
            &self.theme,
            appearance,
            width,
            pointer,
            surface,
            self.text_size,
        );
        ResolvedTheme {
            theme: self.theme.clone(),
            appearance,
            text_size: self.text_size,
            motion_tempo: theme_motion_tempo(&self.theme, appearance, width, pointer),
            tokens: Arc::new(tokens),
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
    /// The reader's text size category the app set, which wins over the system's.
    app_text_size: Option<TextSizeCategory>,
    /// The category the system's text size names, as last read; `None` where the
    /// platform reports none.
    system_text_size: Cell<Option<TextSizeCategory>>,
    /// Whether the system's text size is read: on a platform that reports one, and
    /// not under GPUI's test platform, whose windows would read the machine's.
    reads_system_text_size: bool,
    /// The window last seen active, so a window that gains focus reads the system's
    /// text size again.
    active_window: Cell<Option<WindowId>>,
}

impl ThemeState {
    /// The category every font size is drawn at: the app's, else the system's,
    /// else Large.
    fn text_size(&self) -> TextSizeCategory {
        self.app_text_size
            .or(self.system_text_size.get())
            .unwrap_or_default()
    }

    /// Reads the system's text size again, where it is read; true when the
    /// category changed (the cache is cleared then).
    fn read_system_text_size(&self) -> bool {
        if !self.reads_system_text_size {
            return false;
        }
        let read = system_text_size_category();
        if read == self.system_text_size.get() {
            return false;
        }
        self.system_text_size.set(read);
        self.cache.borrow_mut().clear();
        true
    }

    /// Reads the system's text size when `window` gains focus (GPUI redraws a window
    /// whose focus changes, so its next `current` sees it).
    fn follow_focus(&self, window: &Window) {
        if !self.reads_system_text_size {
            return;
        }
        let id = window.window_handle().window_id();
        if window.is_window_active() {
            if self.active_window.get() != Some(id) {
                self.active_window.set(Some(id));
                self.read_system_text_size();
            }
        } else if self.active_window.get() == Some(id) {
            self.active_window.set(None);
        }
    }
}

impl Global for ThemeState {}

/// A scoped theme, provided to a subtree by [`scope`].
#[derive(Clone, Debug)]
pub struct ThemeScope(pub ResolvedTheme);

/// Registers the bundled fallback fonts and installs core's default theme in
/// system mode. `rustryx::init` calls this.
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
    let reads_system_text_size = !cx.background_executor().is_test();
    cx.set_global(ThemeState {
        theme: Arc::new(theme),
        mode: ThemeMode::System,
        cache: RefCell::default(),
        registered_fonts: RefCell::default(),
        app_text_size: None,
        system_text_size: Cell::new(
            reads_system_text_size
                .then(system_text_size_category)
                .flatten(),
        ),
        reads_system_text_size,
        active_window: Cell::new(None),
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
        .expect("rustryx_theme::init(cx) must run before drawing Astryx components")
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

/// Sets (or, with `None`, drops) the app's text size category and redraws
/// (`authoring::dynamic_type::set_text_size_category` and
/// `follow_system_text_size`).
pub(crate) fn set_app_text_size(category: Option<TextSizeCategory>, cx: &mut App) {
    init(cx);
    let state = cx.global_mut::<ThemeState>();
    state.app_text_size = category;
    state.cache.get_mut().clear();
    cx.refresh_windows();
}

/// The text size category the app draws at (`authoring::dynamic_type::text_size_category`).
pub(crate) fn text_size(cx: &App) -> TextSizeCategory {
    cx.try_global::<ThemeState>()
        .map_or(TextSizeCategory::L, ThemeState::text_size)
}

/// Reads the system's text size again and redraws when it changed
/// (`authoring::dynamic_type::refresh_text_size_category`).
pub(crate) fn refresh_system_text_size(cx: &mut App) {
    init(cx);
    let state = cx.global::<ThemeState>();
    let before = state.text_size();
    state.read_system_text_size();
    if state.text_size() != before {
        cx.refresh_windows();
    }
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
    eprintln!("rustryx: failed to load the {theme} theme's fonts: {error:#}");
}

/// The theme a component draws with during render: the nearest [`scope`],
/// else the app's theme in its mode for this window.
pub fn current(window: &Window, cx: &App) -> ResolvedTheme {
    if let Some(ThemeScope(scoped)) = use_context::<ThemeScope>(cx) {
        return scoped;
    }
    let state = state(cx);
    state.follow_focus(window);
    let appearance = state.mode.resolve(window);
    let width = f32::from(window.viewport_size().width);
    // Upstream `pointer: 'coarse'` adaptations hold under a finger (a coarse primary
    // pointer), as the browser's `(pointer: coarse)` media query does.
    let pointer = if rustryx_primitives::modality::coarse_primary_pointer_now(window, cx) {
        Pointer::Coarse
    } else {
        Pointer::Fine
    };
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
    let resolved = ResolvedTheme::at_text_size(
        state.theme.clone(),
        appearance,
        width,
        pointer,
        state.text_size(),
    );
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
