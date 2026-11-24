# Astryx → Rust/GPUI: full-parity port plan

## Context
Astryx (`../Astryx`) is Meta's design system: React 19 + StyleX, MIT, synced at upstream commit
`40ccf520`, `@astryxdesign/core` 0.7.18. The user is a Rust developer who wants it as native
Cargo crates on GPUI, to use in their other GPUI apps without React.

There is no time pressure. The goal is **everything ported**: all of core plus the optional
areas. After that the user adopts it in their projects.

Scale measured in the upstream repo:
- core has ~211K lines of real TS code (320K with comments).
- 111 categorized components plus ~200 sub-components.
- 103 hooks and ~518 runtime exports.
- Shared infrastructure is 105K lines: hooks, utils, Layer, motion, gestures, theme, i18n.
- lab 58K, charts 6.5K, richtext 12.5K, themes 15.5K.
- 219 component behavior specs with numbered FR (behavior) and AR (accessibility) items.

Core has one runtime dependency (`intl-messageformat`), so everything else is in-house and
gets ported or replaced. Estimated Rust size: 80–120K lines. This is a long roadmap, tracked
by a parity manifest.

## Decisions
| Topic | Decision |
|---|---|
| GPUI | Zed git main pinned at `2b248e607288f229695be89ca2e73face27e5c54` (2026-11-24), declared once in `[workspace.dependencies]`. Library crates depend on `gpui` only. Apps (gallery, user apps) add `gpui_platform`. Consumers must pin the same rev; the README documents a `[patch]` recipe. |
| Toolchain | `rust-toolchain.toml` uses channel `1.100.0`, matching Zed main, with target `wasm32-unknown-unknown`. The user has 1.91, and rustup auto-installs the pinned channel. |
| Foundation | Our own headless primitives on raw GPUI. No gpui-base or gpui-component. |
| Platforms | Windows (primary dev and test), Linux (CI), Web (`gpui_web`, single canvas window). macOS works but is untested. |
| Scope | All of core, the 7 themes with fonts, i18n (30 locales), Markdown/CodeBlock, Chat, the mobile/touch layer, charts, all 31 lab components, richtext, the 58 page templates, RTL, and high contrast. |
| Out | `vega` (wraps the Vega JS runtime; charts covers the use case), the CLI/build/docsite/storybook/sandbox tooling (replaced by gallery + xtask + rustdoc), React/StyleX/Tailwind/Next.js-specific APIs (swizzle is "copy the source"), and the Swift native package. |

## GPUI capabilities confirmed on Zed main
Checked via the GitHub API against `crates/gpui`:
- AccessKit accessibility (`src/_accessibility.rs`, `examples/a11y.rs`).
- `focus_visible` and tab stops (`examples/focus_visible.rs`, `tab_stop.rs`).
- `anchored` and `deferred` (`examples/anchor.rs`, `popover.rs`).
- `container_query` element, `spring.rs`, `gestures.rs`.
- `uniform_list` and `list`, `examples/input.rs` (IME via `EntityInputHandler`), `data_table.rs`, `tree.rs`.
- Web uses `web-time`.
- App bootstrap: `gpui_platform::application().run(..)`; on web, `gpui_platform::web_init()`.

## Workspace layout (`Astryx Rust/`, run `git init` first)
```
Cargo.toml  rust-toolchain.toml  LICENSE (MIT: Meta + ours)  THIRD_PARTY_NOTICES.md  UPSTREAM.md (synced commit, re-sync steps)
spec/                     committed JSON snapshots exported from ../Astryx (tokens, themes, overrides, locales, icons, manifest.json)
tools/export/             Node ESM scripts run against ../Astryx; only needed to re-sync
xtask/                    cargo xtask: codegen [--check] | parity [<Name>] | web | bump-gpui
crates/
  astryx/                 facade: prelude, astryx::init(cx), feature flags (theme-*, i18n-all, markdown, chat, touch, charts, lab, richtext)
  astryx-theme/           Tokens (generated), Theme, Appearance, registry, cx.theme(), scopes, color math, overrides, theme builder
  astryx-primitives/      headless behavior (list below)
  astryx-icons/           core fallback SVGs + Lucide subset, IconName registry, embedded AssetSource
  astryx-components/      every core component, one module per upstream directory
  astryx-themes/          7 generated themes + TTF fonts, one cargo feature per theme
  astryx-charts/  astryx-lab/  astryx-richtext/
apps/gallery/             Storybook equivalent: sidebar, per-component Default + Theme Sheet pages, Tokens page, Templates
                          section, switchers for theme/mode/size/locale/direction; native + wasm
examples/consumer/        tiny app using `astryx` via path, to keep the public API and rev alignment honest
```

## Theme pipeline (machine-generated, never hand-copied)
1. `tools/export/export-themes.mjs` runs after `pnpm -F @astryxdesign/core build` in `../Astryx`.
   - It reuses `packages/native/scripts/swift-ios-theme.mjs` (`resolveTheme`, `generatedThemes`,
     `publishedThemePackages`) and `css-values.mjs` (`toColor`, `toPoints`, `toShadow`,
     `toLeading`, `toWeight`, `valueForMode`). These call core's `resolveThemeTokens(theme,{mode})`.
   - It extends the Swift G5 surface to **every** group: color 98, spacing 15, size 3, border,
     focus 4, radius 7, shadow 8, duration 10, ease 4, families 3, font sizes 12, weights 4,
     type scale 42, data viz 56, syntax 24 plus 12 presets.
   - Output: `spec/themes/<theme>.json`, with light and dark modes and typed values: sRGB RGBA,
     px plus source unit, ms, bezier, shadow layers, family stacks.
   - It also writes each theme's `components` override rules, icon mappings, and font list.
2. `export-assets.mjs` writes:
   - core fallback icon paths (`Icon/defaultIcons.tsx`) and the Lucide SVGs themes map to;
   - locale catalogs (`packages/core/locales/*.json`);
   - `spec/manifest.json` from all 272 `*.doc.mjs`: name, category, props, types, defaults,
     theming targets and vars, and examples;
   - fonts: prefer the original TTFs from the upstream Google Fonts projects, else
     woff2→ttf (fontTools). OFL texts go into `THIRD_PARTY_NOTICES.md`.
3. `cargo xtask codegen` generates Rust from the JSON:
   - `astryx-theme/src/generated/tokens.rs`: a typed `Tokens` struct grouped as
     `color/space/radius/size/border/focus/shadow/duration/ease/font/text/data/syntax`,
     named by the Swift G6 rule in snake_case;
   - `astryx-themes/src/generated/<theme>.rs`: light and dark `Tokens`, plus
     `ComponentOverrides`;
   - the locale tables.
   `--check` is the drift test. The Rust build never needs Node.
4. Component overrides become a data model, mirroring `.astryx-<part>[data-<prop>=<v>]`:
   - `target -> Vec<Rule{conditions:[(prop,value)], patch: StylePatch{style: StyleRefinement, tokens: scoped token overrides}}>`.
   - Components call `theme.apply("button", &[("variant", ..), ("size", ..)], el)` after their
     defaults.
   - Codegen converts supported CSS properties: background, color, border, radius, padding,
     sizes, gap, font, opacity, shadow, and token re-definitions.
   - Unsupported selectors (`:where(.x *)`, `:nth-child`) go to
     `spec/themes/<t>.unsupported.json` for hand-porting.

## Component model (one set of rules for all ~300 parts)
- **Presentational:** `#[derive(IntoElement)]` + `RenderOnce`, with builder methods named
  after Astryx props in snake_case.
  - The `is`/`has` prefixes are dropped: `isDisabled` → `.disabled(true)`,
    `hasCloseButton` → `.close_button(true)`.
  - The required `label` and the `ElementId` are constructor arguments.
  - `impl Styled` forwards to the root as the `xstyle`/`className` equivalent.
  - `impl ParentElement` is added where Astryx takes children.
- **State:** controlled-first like Astryx: `.value(v).on_change(|v, window, cx| ..)`.
  - Where Astryx allows uncontrolled use (Popover, `isDefaultOpen`), internal state lives in
    `window.use_keyed_state(id, ..)`.
  - Heavy editors (inputs, Table, Calendar, Typeahead, Tokenizer, CodeEditor, RichText) use an
    app-owned `Entity<XState>` + `X::new(&state)`, with `EventEmitter` events.
- **Async actions:** `clickAction` and `changeAction` become closures returning
  `Task<anyhow::Result<()>>`; pending/loading state is tracked automatically.
- **Slots and render props:** typed slot builders (`.start_icon(IconName)`,
  `.end_content(impl IntoElement)`) and `Fn(&Item, &mut Window, &mut App) -> AnyElement`.
- **Context** (size cascade, interactive role, icon default size, avatar size, theme
  scope/MediaTheme, locale, direction):
  - a `Provide<T>` element pushes the value while its children run `request_layout`, where
    `RenderOnce::render` executes, and pops it after;
  - resolution follows the documented order: explicit prop > nearest provider > component
    default.
- **Web feature mapping:**
  - `light-dark()` → per-Appearance tokens. `system` follows `window.appearance()`.
  - Hover/press overlay gradients → `Hsla` alpha compositing.
  - `color-mix` → Rust srgb/oklab mix.
  - `:hover`/`:active`/`:focus-visible` → GPUI states, plus our InteractionModality.
  - `:has()` and ancestor markers → state passed from parent to child at render.
  - Container queries → `container_query`. Media queries → platform settings and width
    `Adaptations`.
  - Transitions, `@starting-style` and WAAPI → `with_animation`, springs, and an
    `Animated<T>` keyed tween that honors reduced motion.
  - ARIA → AccessKit roles, labels and actions on element ids.
  - Intl/Temporal → icu4x + jiff. `intl-messageformat` → a small ICU MessageFormat
    interpreter over icu4x plurals and decimals.
  - Logical start/end helpers are used from day one, so RTL is a toggle later.

## Primitives (`astryx-primitives`), each mapped to its upstream source and spec
| Primitive | Upstream source | Specs |
|---|---|---|
| context + size cascade | `SizeContext/`, `InteractiveRoleContext/` | `architecture/component-size-cascade.md` |
| interaction modality, focus ring | `utils/focusOutline.stylex.ts` | `architecture/interaction-modality.md` |
| press feedback | `hooks/usePressFeedback` | `architecture/press-feedback.md` |
| list / grid / tree roving focus, typeahead | `hooks/useListFocus`, `useGridFocus`, `useTreeFocus`, `useTypeahead` | — |
| combobox active-descendant | `Selector/hooks.ts` `useCombobox` | — |
| hotkeys | `hooks/useHotkeys` | — |
| focus trap | `hooks/useFocusTrap` | — |
| layer runtime | `Layer/useLayer.tsx`, `layerStack.ts`, `toastHost.tsx` | `architecture/layer-runtime.md`, `families/overlay-dismissal.md`, AST-003/027/090/116 |

The layer runtime covers:
- the `AstryxRoot` host: modal outlet, toast host, drawers;
- the Escape stack;
- outside press;
- the positioner: side/align/offset/flip/shift, viewport gutter, max-size caps;
- focus return, scroll lock, enter/exit motion, hover intent, the submenu safe triangle.

The remaining primitives:
- **Text editing engine:** built on GPUI's `examples/input.rs`. It covers IME,
  graphemes/words via `unicode-segmentation`, selection, clipboard, undo/redo, multiline/soft
  wrap, and a segment/mask hook. Follows `IME_GUARD_DESIGN.md`, AST-109/117/118.
- **Virtualization:** `virtualization/`, `architecture/list-windowing.md`.
- **Reorderable:** AST-103.
- **Announcements:** AST-086.
- **i18n runtime:** `i18n/`, `architecture/internationalization.md`.
- **Motion and gestures:** `motion/`, `gestures/`, `dragToSnapPoints/`, `animator/`,
  `keyboard/`. Ported in M11.

## Per-component porting recipe (repeat ~300×)
1. `cargo xtask parity <Name>` prints the manifest entry: props, defaults, theming targets,
   examples.
2. Read the upstream material:
   - `packages/core/src/<Name>/*.tsx`;
   - `docs/specs/components/<Name>/<Name>.spec.md` (FR/AR tables);
   - the family contract in `docs/families/`;
   - Playwright a11y and interaction specs in `__tests__/`.
3. Implement `astryx-components/src/<name>/`:
   - every prop, or a waiver with a reason in `parity.toml`;
   - token styles and theming targets;
   - keyboard behavior via actions, `key_context` and default keybindings;
   - AccessKit roles from the AR items.
4. Write `#[gpui::test]` tests per FR, with simulated keystrokes and clicks and state asserts.
5. Build the gallery page from `packages/cli/assets/templates` blocks: Default plus a Theme
   Sheet of variants × sizes × states.
6. Compare against Astryx Storybook (`pnpm storybook` in `../Astryx`) in light and dark,
   neutral plus one other theme.
7. Mark the component done in the parity tracker. Page templates are ported as soon as their
   components exist.

## Milestones (dependency-ordered; each ends green on Windows + Linux CI + wasm build)
- **M0 Bootstrap & spikes**
  - `git init`; workspace, toolchain, pinned GPUI; gallery opens on Windows, runs in the
    browser, builds on Linux CI (GitHub Actions).
  - Spikes:
    - (a) `Provide<T>` context reaches `RenderOnce` children;
    - (b) `use_keyed_state` uncontrolled state;
    - (c) a deferred+anchored popover with flip;
    - (d) an AccessKit role visible in Accessibility Insights / Narrator;
    - (e) TTF and variable-font weights on Windows and web;
    - (f) keystroke simulation in `#[gpui::test]`;
    - (g) `examples/consumer` with rev alignment and the `[patch]` recipe.
  - Fallbacks get decided if any spike fails.
- **M1 Theme pipeline:** export scripts, codegen and drift check; `astryx-theme` runtime
  (Appearance, registry, scopes, MediaTheme, color math, adaptations, overrides);
  `astryx::init`; fonts and icons registered.
  - Exit: the gallery Tokens page renders every theme in both modes and matches Storybook.
- **M2 Primitives I + foundations:** context, direction, modality and focus ring, press,
  overlays, hotkeys, a11y helpers, i18n runtime + catalogs, link handler.
  - Components: Text (truncation), Heading, Stack/HStack/VStack/Center/Grid/Divider/AspectRatio,
    VisuallyHidden, Icon, Spinner, Skeleton, Badge, StatusDot, Kbd, Code, Blockquote,
    Citation, Avatar(+Group), Card, Section, EmptyState, Thumbnail, Timestamp, Timer,
    ProgressBar, Banner.
- **M3 Actions & simple controls:** list focus and typeahead.
  - Components: Button (all variants, loading, icon-only, href, clickAction), IconButton,
    ToggleButton, ButtonGroup, Link, SegmentedControl, CheckboxInput/List, RadioList, Switch,
    Slider, Indicator, Token, ClickableCard, SelectableCard, Collapsible, Toolbar,
    Breadcrumbs, Pagination, Stepper/Step, TabList.
- **M4 Editing engine & fields:** TextInput, TextArea, NumberInput, InputGroup, Field,
  FieldStatus, FormLayout, FileInput (`cx.prompt_for_paths`).
- **M5 Layer runtime & overlays:** Tooltip, InfoTip, Popover, HoverCard, Overlay,
  Dialog(+Header, imperative), AlertDialog, Drawer, Toast(+viewport/queue), DropdownMenu
  (submenus, menu press), ContextMenu, MoreMenu, Lightbox, CommandPalette.
- **M6 Collections:** combobox, virtualization, tree/grid focus, reorderable.
  - Components: List, Item, Selector, MultiSelector, Typeahead, Tokenizer, ComplexSelector,
    TreeList, OverflowList, MetadataList, PowerSearch.
- **M7 Table:** BaseTable/Table parts plus 14 plugins as a trait-based pipeline: columnResize,
  columnSettings, filtering, groupedRows, infiniteScroll, pagination, rowExpansion, rowIndex,
  rowStatus, selection, sortable, stickyColumns, tree, virtualization.
- **M8 Dates:** calendar systems (jiff + icu4x, AST-111), Calendar, DateInput (segmented),
  DateRangeInput, DateTimeInput, TimeInput.
- **M9 Shell & navigation:** AppShell, Layout, SideNav, TopNav, NavMenu/NavHeadingMenu,
  NavItem, NavIcon, MobileNav, Outline, Resizable, ScrollableArea (AST-025), Carousel,
  SnapPaging.
- **M10 Content:**
  - Markdown: pulldown-cmark/GFM mapped to Astryx nodes; upstream plugins; streaming parse;
    URL safety.
  - CodeBlock: port of the in-house grammar tokenizer with `fancy-regex`, 30 languages, 12
    syntax presets.
  - Chat suite: layout, message list with stream scroll, composer with tokens and trigger
    menu, tool calls, dictation behind a pluggable speech provider.
- **M11 Motion & touch:** springs, decay, rubber-band, gesture recognizers on GPUI pointer and
  touch events, keyboard clearance, dragToSnapPoints.
  - Components: BottomSheet(+Switcher), Swipe, coarse-pointer variants (sheet-backed
    Popover/Selector).
- **M12 Theme authoring parity:** a Rust `defineTheme` builder with type/radius/motion scale
  expansion, OKLCH palette engine (`palette/`), HCT generator (`theme/hct.ts`), status-role
  derivation, contrast, `onDark`/`onLight`, adaptations, per-theme icons.
- **M13 Charts & lab:**
  - `astryx-charts`: d3-scale and d3-shape essentials; bar, line, area, dot, band,
    candlestick, errorBar, referenceLine, heatmap and stream marks.
  - `astryx-lab`: all 31 lab components. CodeEditor and DiffViewer are built on the engine
    plus the tokenizer; MediaPlayer and ThreeD carry risk.
- **M14 Rich text:** a document model + editor on the engine, toolbar, lossless Markdown round
  trip (AST-062/064/106).
- **M15 RTL, high contrast, polish:** RTL mirroring, Windows high contrast, reduced-motion
  audit, remaining page templates, theme-showcase, rustdoc on every public item, README.

## Verification
- On every milestone, on Windows:
  - `cargo fmt --check`;
  - `cargo clippy --workspace -- -D warnings`;
  - `cargo test --workspace` (gpui `test-support`);
  - `cargo xtask codegen --check`;
  - `cargo xtask parity`, which reports manifest coverage and fails on undocumented gaps.
- Web: `cargo xtask web` builds the gallery for `wasm32-unknown-unknown` and serves it. Check
  it in Chrome, screenshotting via Claude in Chrome.
- Linux: the GitHub Actions ubuntu job runs build + tests.
- Visual: gallery vs Astryx Storybook side by side per component, light and dark, two themes.
- Accessibility: Accessibility Insights for Windows / Narrator pass at the end of M3, M4, M5
  and M6.
- `examples/consumer` must build against the facade, so the public API stays ergonomic.

## Risks
- **GPUI main churn.** The rev is pinned and bumped only via `cargo xtask bump-gpui` plus a
  full test run. Consumers must match the rev.
- **The context-propagation assumption.** M0 spike (a). Fallback: explicit propagation.
- **Text engine size.** It is the largest primitive, so it gets its own milestone.
- **Fonts.** Variable fonts and woff2 in GPUI (spike e). Sarina and UnifrakturMaguntia carry
  OFL Reserved Font Names; fetch originals rather than modify them.
- **Web backend gaps.** Single window; input issues were reported in March 2026. Test the web
  build every milestone.
- **Theme override selectors with no GPUI analog.** Handled as hand-ports via the unsupported
  report.
- **Accessibility parity** is bounded by GPUI's AccessKit surface.
- **The name.** "Astryx" is Meta's name. MIT covers the code, not the name. Keep attribution;
  rename before any public publish.
