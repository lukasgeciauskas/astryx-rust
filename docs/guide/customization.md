# Customizing components

Astryx lets you customize at any level without forking. The Rust port keeps
the same four levels.

1. **One element: `Styled`.** Every component forwards `Styled` to its root, the
   counterpart of `className`/`xstyle`:
   `Button::new("save", "Save").w_full().mt(t.spacing.s4)`.
2. **A subtree: token re-definitions.** The counterpart of setting CSS custom
   properties on a container:
   ```rust
   rustryx_theme::with_tokens(
       [("--color-accent", TokenValue::Color(brand))],
       settings_panel,
       window,
       cx,
   )
   ```
   For a whole other theme in a subtree, use
   `rustryx_theme::scope(ResolvedTheme::new(..), child)`.
3. **The whole app: component rules and token values.** These work the way a theme's
   `components` map does:
   ```rust
   let theme = rustryx_themes::neutral::theme()
       .with_token_values(None, [("--radius-element", TokenValue::Length(px(4.).into()))])?
       .with_rules([ComponentRule { target: "button".into(), when: .., state: None, light: .., dark: .. }]);
   rustryx_theme::set_theme(theme, cx);
   ```
   App rules apply after the theme's rules, so they win.
4. **The component itself: swizzle.** Copy the component's module from
   `crates/rustryx-components/src/<name>/` into your app and edit it (MIT).
