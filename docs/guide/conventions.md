# Conventions

## Component model

- **Presentational components** are `#[derive(IntoElement)]` + `RenderOnce`.
- **Builder methods** are named after the upstream props in snake_case.
  - Drop `is`/`has` prefixes: `isDisabled` becomes `.disabled(true)`, and
    `hasCloseButton` becomes `.close_button(true)`.
  - The required `label` and the `ElementId` are constructor arguments.
- **Escape hatches:** every component implements `Styled` on its root, the counterpart of
  `className`/`xstyle`. `ParentElement` is implemented where upstream takes `children`.
- **State is controlled-first,** as upstream: `.value(v).on_change(|v, window, cx| ..)`.
  - Uncontrolled state (upstream `isDefaultOpen`, an uncontrolled Popover) lives in
    `window.use_keyed_state`.
  - Heavy editors (inputs, Table, Calendar) use an app-owned `Entity<XState>` plus
    `X::new(&state)`.
- **Context:** `astryx_primitives::provide(value, child)` / `use_context::<T>(cx)`.
  - Read context during render only.
  - Size cascade: `resolve_size(explicit, default, cx)`.
- **Tokens:** `let theme = astryx_theme::current(window, cx); let t = theme.tokens();`.
  Never hard-code a color, length or font that a token provides.
- **Theme overrides:**
  - Name each part's theming target as upstream does (`button`, `segmented-control-item`).
  - Pass the part's `(prop, value)` pairs to `theme.refine(style, target, props, state)` after
    the component's own defaults.
  - Use `theme.for_part(target, props)` to scope token re-definitions to the part's children.
- **Prop setters:** many optional props → a field list in the setters macro (introduced
  with the first M2 component), not hand-written boilerplate.
- **Inline components:** where upstream's root is `display: inline-flex` (Button, Badge,
  Kbd, Token, Link, Icon, Avatar...), the root is `.inline_flex()` (fork patch R5).
  - In a block parent (a plain `div()`), consecutive inline boxes flow like words: side by
    side at their content width, on one text baseline, wrapping at the parent's width,
    placed by its `text_align`.
  - In a flex or grid parent they are ordinary items, as in CSS: a stretching flex column
    stretches them.
  - Upstream `width: fit-content` is `.w_fit()` (also `.w_min()`, `.w_max()`).
- **Roving composites:** data-driven lists (RadioList, SegmentedControl, TabList) use
  `ListFocus`; a Toolbar of arbitrary controls uses `RovingScope`, which every control
  built on `control_focus` joins.
- **Pressables** (Button and everything built on it) share `press::apply_press`, which
  provides:
  - a tab stop (`control_focus`);
  - the hover and pressed overlays composited over the fill;
  - the press scale, skipped under reduced motion;
  - the focus ring;
  - the disabled and busy guards: focusable, but inert.

## Code

- Generated files (`*/generated/*`, `spec/`, `crates/astryx-icons/assets`,
  `crates/astryx-themes/fonts`) are never edited by hand. Change the exporter or the
  codegen and regenerate.
- Comments explain why, not what. Public items get a one-line doc comment naming the
  upstream prop or concept they map to.
- Licensing: GPUI's own crate and examples, and Longbridge's gpui-component, are
  Apache-2.0. Code adapted from them keeps an attribution comment. Never copy from Zed's
  GPL-licensed crates (`editor`, `ui`, and the rest of the Zed app).
