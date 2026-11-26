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
  - Text fields (TextInput, TextArea, NumberInput) stay controlled by `value`; their editing
    state lives in a keyed `TextEditState` (decided in M4).
  - Heavy editors (Table, Calendar, Typeahead, Tokenizer, CodeEditor, RichText) use an
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
- **M2 Primitives I + foundations (done, 2026-11-24):** context, direction, modality and
  focus ring, press, overlays, hotkeys, a11y helpers, i18n runtime + catalogs, link handler.
  - Carried into later milestones, recorded as `RustApi::Pending` in each doc: tooltips and
    the Timestamp hover card (M5). Banner's and Thumbnail's controls became IconButtons in M3.
  - Components: Text (truncation), Heading, Stack/HStack/VStack/Center/Grid/Divider/AspectRatio,
    VisuallyHidden, Icon, Spinner, Skeleton, Badge, StatusDot, Kbd, Code, Blockquote,
    Citation, Avatar(+Group), Card, Section, EmptyState, Thumbnail, Timestamp, Timer,
    ProgressBar, Banner.
- **M3 Actions & simple controls (done, 2026-11-24):** list focus and typeahead.
  - Primitives: Tab/Shift+Tab traversal (`astryx::init`), `control_focus`, `ListKeys` and
    `ListFocus` (roving tab stop, `on_move`), `RovingScope` (one tab stop among arbitrary
    controls), `Typeahead` (ICU4X folding).
  - Components: Button (all variants, loading, icon-only, href, clickAction), IconButton,
    ToggleButton(+Group), ButtonGroup, Link, SegmentedControl, CheckboxInput/List, RadioList,
    Switch, Slider, Indicator, Token, ClickableCard, SelectableCard, Collapsible(+Group),
    Toolbar, Breadcrumbs, Pagination, Stepper/Step, TabList.
  - Pulled forward because M3 components need them: Field and FieldStatus (from M4), and
    Item's core (from M6; swipe actions wait for M11).
  - Carried into later milestones, recorded in each doc: TabMenu and BreadcrumbItem
    menus (finished in M5 with DropdownMenu), Pagination's page-size selector (finished
    in M6 with Selector); the SegmentedControl lens and Item swipe actions (M11). Tooltips, InfoTips, disabled messages and Stepper's on-track and compact
    modes were finished with M4.
  - A gallery test renders every page, so an example that panics fails `cargo test`.
- **M4 Editing engine & fields (done):**
  - GPUI fork patch R6 (ARIA states: disabled, read-only, required, invalid, busy, modal,
    multiselectable), adopted by every control.
  - Primitives: `editing` (EditBuffer: graphemes, words, selection, IME composition,
    undo; TextEditState: the IME client, editing actions and key bindings, mouse
    selection, masking, insert filters, paste/focus/blur/key callbacks; the editor
    element: wrapping, selection, caret blink, scrolling).
  - i18n: locale number parsing (`parse_number`, `editable_number`, `number_symbols`) and
    `conjunction` lists.
  - Components: TextInput, TextArea, NumberInput, InputGroup(+Text), FormLayout, FileInput,
    Field (attached, detached and tooltip status), Pagination's input box.
  - GPUI fork patches R7 (display contents, grid tracks: FormLayout's horizontal-labels
    grid), R8 (browser files: FileInput on the web) and R9 (`aria-current`).
  - Pulled forward from M5 to finish every M3 and M4 prop: the anchored-layer runtime
    (`position_layer`: flip, slide, RTL; `anchored_layer`), Tooltip (hover delay and
    bridge, focus and touch triggers, controlled and pinned modes) and InfoTip, wired into
    every `tooltip`, `labelTooltip`, `disabledMessage` and tooltip-status prop, Slider's
    value tooltips, ProgressBar marks, Thumbnail labels and Text/Heading truncation.
  - Also finished: Stepper's on-track indicators, compact horizontal mode and fill
    sweep; CheckboxList/RadioList container and control styles.
  - Inline text flow (fork patch R10): a Text's text, inline Texts and Links share
    its lines, breaking at UAX #14 opportunities; a Link wraps inside itself and hovers,
    presses and takes focus as one; any other element is an inline box on the baseline.
    Text `display`, Center `isInline`.
  - Text `hasCapsize`: the box trimmed to cap height and baseline from font metrics.
  - Spell checking (`astryx_primitives::spell`): the Windows system checker, Hunspell
    dictionaries elsewhere; misspellings get a wavy underline once a field has had focus.
    Suggestions and "Add to dictionary" came with ContextMenu (M5).
  - Touch keyboards (fork patch R11): `inputMode`, `enterKeyHint`, `autoCapitalize`,
    `hasAutoCorrect` and `hasSpellCheck` reach the web's IME mirror; NumberInput asks for
    the number pad.
  - Carried into M8 and done there: FormLayout's `defaultEntryErrorBehavior` readers.
- **M5 Layer runtime & overlays (done, 2026-11-25):** Popover (done: focus region through fork patch R12,
  focus entry and return, light and Escape dismissal, the disclosure contract with its
  trigger; its sheet presentations wait for BottomSheet), HoverCard (done: Tooltip's
  engine with a card surface; Timestamp's copyable hover card), Overlay (done), Dialog
  (+Header, imperative; done: modal layers drawn above earlier ones, purpose-based
  dismissal, focus entry and return), AlertDialog (done), BottomSheet (+Switcher; done:
  snap points, drag and swipe-to-dismiss from the grab handle), Drawer (+Header; done),
  Popover's sheet presentations (done), Toast (+viewport/queue; done).
  - Lightbox and `use_lightbox` (done): galleries that page by button, key or drag (40px
    apart); drag down to close; zoom by key, double click, Ctrl+wheel or trackpad pinch
    (with UIKit's overshoot and release), pan with rubber-band edges; the caption band
    with "more"; touch taps that hide the controls, and the touch exit. The zoom math
    (`zoom_stage.rs`) ports upstream's with its tests. Video waits for MediaPlayer's
    decoder (M13); until then a video item opens in the system player.
  - CommandPalette and its parts (done) on `search::SearchSource`, which M6's Typeahead
    and Tokenizer will share: async answers with stale ones dropped, partial lists, a
    failed answer as an empty one, groups, the result keys kept in the query field, picker
    mode, announcements.
  - Menus (done): DropdownMenu with every part (items, checkbox and radio rows, sections,
    dividers, submenus), MoreMenu, ContextMenu, TabList's TabMenu and BreadcrumbItem
    menus, all on one menu list.
    - Keys: arrows wrap, Home/End, PageUp/PageDown by the visible rows, typeahead;
      disabled rows are reached and stay inert; Tab leaves the menu.
    - Pointer: rows take focus under the pointer; submenus open on hover with the safe
      triangle and the 150/200 ms delays.
    - The press model: the trigger opens on press-down and the release picks the row
      under it once settled; a right press does the same for a ContextMenu.
    - Drill-in submenus behind a Back row; the sheet presentations as touch rows.
    - Header and footer regions make the menu, or a submenu's flyout, a modal dialog.
  - Text fields gain the browser's context menu, which GPUI lacks: spelling suggestions
    for the word under the pointer, "Add to dictionary", Cut, Copy, Paste and Select all.
- **M6 Collections (done):** combobox, virtualization, tree/grid focus, reorderable.
  - Components: List, Item, Selector, MultiSelector, Typeahead, Tokenizer, ComplexSelector,
    TreeList, OverflowList, MetadataList, PowerSearch.
  - Selector and SelectorOption (done): the input, secondary and ghost triggers; options,
    sections and dividers; the combobox keys (`selector::combobox`, FR8 opening
    highlight); closed-trigger typeahead that picks and announces; search with counted
    announcements (FR9); the highlight that follows its option (FR10); the selected
    option over the trigger, clamped to the window (FR2, through
    `AnchoredLayer::position_with`); the press model (FR7); read-only (FR6), loading
    (FR5), clear, changeAction, the bottom-sheet presentations (FR4), focus return
    (AR2) and the keyboard ring (AR5).
  - GPUI fork patch R13 (active descendants across layers): a popup's option is its
    combobox's active descendant.
  - Pagination's page-size selector (done), which closes the M3 item; `page_href` now
    receives the page size, as upstream's `getPageHref(page, pageSize)`.
  - List and ListItem (done): density, dividers, markers (disc, circle, numbers in one
    shared column, task boxes for Markdown), the four edge treatments against the host's
    padding and corners (FR1-FR8), the inline clip (FR9), the touch hit band (FR10) and
    press fade (FR11), and row menus with a "More actions" path (FR13). Hosts (Card,
    Section, Dialog, BottomSheet) now publish their corners and each child's place
    (`ContainerCorners`, `ContainerChild`) besides their padding.
  - Item's remaining props: `interactiveRef` (`Item::interactive`, a delegate control),
    `aria-current` on a selected interactive row; swipe actions wait for M11.
  - MetadataList and MetadataListItem (done) on R7 grid tracks; OverflowList (done) with
    upstream's `computeOverflow` fit over a hidden measuring copy.
  - Carried: List's and TreeList's `animatedLayout` (with TreeList's expand glide, FR8-FR9) to the motion layer (M11), like Collapsible's.
  - Typeahead, BaseTypeahead and TypeaheadItem (done) on `search::SearchSource`: the
    threshold and debounce (FR2), stale answers dropped and answers kept through a
    dismissal (FR3), partial lists that keep their rows and highlight (FR10), grouped
    results and the announced empty message (FR4), wrapping keys (FR5), the listbox and
    dialog menus with `TypeaheadController` (FR13-FR16), the selected Token with edit
    mode and Backspace (Typeahead FR8, FR12), focusable-disabled fields (FR9).
  - Tokenizer, inline presentation (done): tokens with remove and Backspace, chosen items
    never offered (FR13), the focus list kept open across picks (FR14), the create row,
    `max_entries` with the field kept mounted (FR1, FR15), Clear all, the folded
    `UnfocusedInline` and `UnfocusedLayer` overflow (FR3), announcements, and
    `TokenizerController` (focus, blur, open, close).
  - Tokenizer, bottom-sheet presentation (done): the trigger field with display-only
    tokens (FR17), one tall sheet with its bordered search, checkbox rows over a draft
    in stable order, the Create row, 50-row batches, the pinned footer, Clear all and
    discard confirmations, and Save reporting the last edit that holds (FR18-FR27).
  - MultiSelector (done): count, labels and badge triggers (input, secondary, ghost),
    a caller's trigger (`render_trigger`) and `MultiSelectorController`, checkbox rows
    with select-all, sections and dividers, search with the Create row, the selection
    order fixed at open, the grid popup for options with actions (arrow keys reach an
    action cell; Enter presses its control), announcements, busy and read-only states,
    and the shared sheet presentation (FR1-FR18). Selector and MultiSelector share the
    panel's search row (`selector::search_row`).
  - ComplexSelector (done): the combobox trigger (input, secondary, ghost) or a caller's
    control, and a dialog popover (the ported Popover) around the caller's content, which
    gets the value, a commit helper (`on_change`, then `change_action` with the value shown
    while it runs), a close helper and the shell's state; ArrowDown opens, the content or
    `ComplexSelectorController` closes, every route reports through `on_open_change`, and
    focus returns to the trigger only when it would be lost (FR1-FR13).
  - PowerSearch, stage A (done): the model (`power_search::model`), the config builder
    with `apply_filters` (upstream `usePowerSearchConfig`), value and token text, and the
    main field's rows: fields, field-and-operator and field-prefixed value rows, the
    content search, and FR27's value-only rows with FR29's order, per-field cap and
    "More" rows. Stage B (done): the value editors (the date and time ones since
    M8), `PowerSearchFilterEditor`
    (field, operator and value; Delete, Cancel, Apply; Enter and Escape; focus starting in
    the value; nested filters on a TreeList), and the field: filter tokens named by their
    text with their editor (FR24), the editor popover 400-720 px wide at the nearer edge of
    its opening control when capped (FR1-FR9), read-only fields and filters (FR22), Clear
    all keeping read-only filters (FR23), the result count (FR19), FR28's held source rows,
    `components` overrides and `PowerSearchController`. Next: the sheet flow (stage C), which
    starts with BottomSheet's stack (done: covered sheets recede a step and leave the
    stack's one dim to the top sheet, BottomSheet FR5-FR6; Escape reaches the top sheet
    and focus returns into the revealed one, FR7, FR9). Stage C so far: the adaptive
    resolution and its hold while the flow is open (FR10), the trigger field (FR11), the
    Filters sheet over a draft with rows, remove actions, the empty state, Clear all and
    discard confirmations and Done's replay (FR12, FR15b, FR15c), Add filter's grouped
    and searchable fields (FR13) and the value step with its Operator group and choice
    lists (FR14, FR17), Add filter's content search with its inline suggestions and
    FR30's value-only rows and "More" rows (FR13, FR30), nested groups in the value step
    (FR14), numbers staged as they are typed (FR17), FR16's focus rules (through the new
    `astryx_primitives::request_focus`), and the result count's announcement in both
    presentations (FR19), and "More" rows carrying the typed text into the editor they
    open (a text field's value, or a search editor's opening query: BaseTypeahead FR18,
    Tokenizer FR31, built here though upstream lists FR31 as an implementation gap),
    with a people field's "More" row opening its Tokenizer sheet directly (FR29, FR30).
    PowerSearch is done; its date and time values came with M8 (stage 8).
  - TreeList (done): data-driven rows with chevrons, guides, start and end content,
    density and variants; row actions (FR7); the WAI-ARIA tree keys as the
    `astryx_primitives::tree_focus` primitive (upstream `useTreeFocus`) with one tab stop
    and typeahead (AR1); focus kept when a branch collapses around it (AR2); and the
    theme's `tree-disclosure` mark (FR10). Theme indicators (`indicator::set_theme_indicator`,
    Indicator FR7) also give Spinner a theme's `spinner` picture.
  - Long-list windowing (done): `astryx_primitives::list_window`, upstream's one private
    windowing module (`virtualization/`, `architecture:list-windowing`). The fixed-pitch
    window with four rows of overscan, the range over items of varying size, first-visible-key
    anchoring and the focus island are ported with upstream's tests. The record's FR1-FR4,
    where upstream has gaps, are built too: the end (follow) and given-item anchors,
    measured rows of varying size, and container and page scroll (`WindowedList` reads its
    place under every clip above it and moves the scroller it sits in, its own or the
    page's `ScrollHost`, which the gallery provides). Both axes, with the inline insets and
    right-to-left. Table's row and column virtualization (M7) are its consumers; Selector
    renders every option, as upstream's does. INV5's byte budget measures a JavaScript
    bundle and has no Rust counterpart.
  - Reordering (`reorderable::create_reorderable`, AST-103), List its first adopter: the
    handle (a toggle named "Reorder" and the row's label) ends each keyed row the object
    accepts (FR1, FR2, FR5); a drag from it with the source dimmed in place, the row
    riding the pointer, upstream's drop-position rule and accent line, edge autoscroll on
    UIKit's measured steps, one report at the drop, and its announcements (FR3); reorder
    mode by click, Enter or Space, the arrows moving a place with each move reported and
    announced with its neighbours, refusals past rows that stay at the ends, the mode's
    line and its hint of buttons, Escape or Done (FR4); `start(key)` (FR4a); and
    `labels` (FR1, an upstream gap). Also built though upstream has not: `feedbackBehavior`'s
    shift (FR9: the midpoint rule, the passed items taking the next slots, the insertion
    line again under Reduce Motion; the slide itself waits for the motion layer, M11),
    `positions` (FR1: the nearest given slot, or FR9's rule over them; another length is
    ignored) and `attach` (FR4b: `ReorderAttachment` wraps an app's own container, items
    and handles, with the layout read from the items' boxes, so a row moves by Left and
    Right and a wrapping grid by all four arrows with its row and column announced, and a
    move the app does not apply is refused). A tree's `{parent, index}` moves wait for a
    tree collection that adopts the object; none does upstream, TreeList included.
  - `useKeyboardHint` (`keyboard_hint::use_keyboard_hint`), which no earlier milestone had
    ported: the arrow-key hint on keyboard entry into TabList, Toolbar and SegmentedControl,
    gone at the first arrow, Escape or focus leaving, not shown again, in the anchored hint
    layer reorder mode shares. GPUI tells no element's bounds by its focus, so it anchors
    to the focused control's focus ring, which records where it draws. Carried from People:
    AvatarGroup's one tab stop across its interactive members with wrapping arrows, Home
    and End (FR4) and its spoken hint (FR5), interactive Avatars taking focus, and
    AvatarGroupOverflow's `onClick` and `children`.
- **M7 Table (done, 2026-11-25):** BaseTable/Table parts plus 14 plugins as a trait-based pipeline:
  columnResize, columnSettings, filtering, groupedRows, infiniteScroll, pagination, rowExpansion,
  rowIndex, rowStatus, selection, sortable, stickyColumns, tree, virtualization.
  - Table and its parts (done): `table::Table<T>` over any `TableRecord` (a typed row, or the
    `Record` map), columns with `proportional`/`pixel` widths and their floors (FR13), row
    identity (FR16), the default cell text with missing values empty (FR25) and generated
    columns; the `TablePlugin<T>` trait with every transform, the header cell's slots and
    FR9's ordering, row and render contexts (FR17), a loading source (FR22) and context-menu
    actions gathered into one menu per header cell and per body cell; density, dividers, stripes,
    hover, vertical alignment, wrapping or truncating text with the default cell's tooltip,
    the named scroll region that becomes a tab stop and scrolls by keys (FR1), container
    bleed, row indexes and the empty state (FR8). A data-driven table lays out as one fixed
    grid template per row; children mode (`Table::composed` with TableHeader, TableBody,
    TableFooter, TableRow, TableHeaderCell and TableCell) as one shared grid whose columns
    fit their content (FR24). Themes' even-row stripe (`:nth-child(even)`) now maps to the
    row's `parity`, in codegen and at run time.
  - Sorting (done): `TableSortable` (upstream `useTableSortable`, FR5) wraps each sortable
    column's label in a sort control with its indicator, the multi-sort priority (its
    description too) and the header's sort actions; a press cycles ascending, descending,
    unsorted, and Shift adds a key. The header's `aria-sort` needed fork patch R14.
    `use_table_sortable_state` owns the state (or follows the caller's) and sorts rows
    stably, missing values last, numbers by value, the rest by the locale's numeric
    collation (`I18n::compare`, on `icu_collator`). A theme's `componentIcons` slots now have
    a registry (`icon::set_component_icon`; `table-sort-indicator` its first slot). Under
    truncation a cut heading shows in full while keyboard focus is in its header.
  - Selection, activation, row status and row numbers (done): `TableSelection`
    (`useTableSelection`) adds the 36 px checkbox column with select-all and publishes each
    row's selected state; `use_table_selection_state` keeps the keys (its own, or the
    caller's), with select-all keeping frozen rows; `TableSelectionToolbar` shows the count
    and clears the whole selection. One resolver now paints a row from every plugin's state
    (FR21): selection's wash, then the active fill, hover and the stripe.
    `TableActivation` (`useTableActivation`, built from its accepted contract, FR1-FR12)
    makes flat product rows with a canonical `id_key` tab stops whose primary click or
    non-repeated Enter or Space asks for activation; the active row carries
    `aria-current`, a fill and an inline-start marker, and a second whole-row owner, an
    invalid key or a press on a nested control leaves it inert. `use_table_row_status` and
    `TableRowIndex` (`useTableRowIndex`) add the status gutter and row numbers. Cells and
    body transforms receive rows as `TableItem` handles a callback can keep.
  - Pagination, filtering and infinite scroll (done): `TablePagination`
    (`useTablePagination`, with `paginate_data`) places the Pagination controls above,
    below or around the table and hides them for a single page. `TableFiltering`
    (`useTableFiltering`) resolves each column's `filter` against a PowerSearch config and
    renders the operator's control, in a popover behind a funnel button (Apply and Reset
    over a draft) or inline below the label; `use_table_filter_state` keeps the filters and
    `to_search_filters` turns them into PowerSearch filters for `apply_filters`.
    `use_table_infinite_scroll` adds the sentinel and Load more button after the table: a
    request latch, completion after the caller's loading flag clears, rearming while the
    sentinel stays in view, a failure pausing automatic loading, and focus handed to the
    table's region when the last batch removes a focused button. A table counts a plugin's
    loading only for flat product rows (FR22).
  - Tree, grouped rows and row expansion (done): `use_table_tree_state` flattens nested
    rows to the visible ones (cycles skipped, siblings sortable, its own expanded set or
    the caller's); `TableTreeData` puts the indent and expander on the tree column, an
    expand-all toggle in its header, and names each expanded row's children with
    `aria-controls` (fork patch R15, named nodes). `use_table_grouped_rows` heads each
    group with a synthetic row through a new row plan (`TablePlugin::row_plan`): product
    rows by index with a plugin's synthetic rows among them. `TableRowExpansion` adds the
    chevron column, the detail panel that takes its row's divider (FR5) and the toggle in
    every cell's menu; each body cell now has its own right-click menu, as upstream's
    TableCell does (FR4). The glide (FR15) waits for M11.
  - Column settings, sticky columns and column resize (done): `TableColumnSettings` and
    `use_table_column_settings_state` choose and order the columns (always-visible ones
    stay). `TableStickyColumns` pins contiguous runs to either edge: with no `position:
    sticky` in GPUI, a pinned cell follows the region's sideways scroll, paints over the
    cells under it with an opaque base and its row's fill, and casts a shadow while
    content is under it. `use_table_column_resize` puts a boundary at each resizable
    column's edge, drawn deferred through the body rows (FR2): hinted while the pointer is
    in the header row, accent under the pointer, on focus or in a drag, quiet while the
    region scrolls (FR10); a drag or the arrow keys resize, a proportional column trading
    width with its neighbor; it announces a splitter with the rendered width (AR7, AR8).
  - Virtualization (done): `use_table_rows_virtualization` windows flat rows with valid
    keys and default or fixed-height cells (selection, row numbers from 1) on
    `list_window`'s `WindowedList`, inside the table's own block viewport sized by its
    parent: overscan, a focused row kept as an island, the first visible row anchored,
    truthful row indices and count (FR2-FR8, FR10); taller equal rows become the pitch
    (FR17), and anything else (wrapping, custom cells, invalid keys, non-flat rows)
    renders every row (FR14, FR6). `use_table_columns_virtualization` mounts only the
    columns near the inline viewport, plus pinned columns and a focused cell's; an
    unmounted column keeps its grid track. Product rows are built by one `RowBuilder` that
    the full and windowed bodies share.
- **M8 Dates (done, 2026-11-25):** calendar systems (jiff + icu4x, AST-111), Calendar, DateInput (segmented),
  DateRangeInput, DateTimeInput, TimeInput.
  - Stage 1 (done): calendar systems and Calendar. `astryx_i18n::date` ports
    `plainDate`, `calendarSystem`, `calendarDateParser`, `dateParser`, `timeParser` and
    `timeFormat`: ICU4X's `icu_calendar` does Temporal's arithmetic (month codes,
    constrain and reject) and `icu_datetime` writes Intl's text, read as parts. The typed
    date parser learns each locale and calendar from that output and passes upstream's
    round-trip matrix (every calendar, native locales, English and Swedish, 1900-2100,
    FR9-FR14). A locale's `-u-ca-` keyword never picks a calendar, Timestamp included
    (FR2). `tools/export/export-cldr.mjs` exports upstream's weekday-header and
    month-capital tables and Intl's calendar and week-field names into `spec/cldr.json`.
    Calendar: one or two APG month grids, a roving tab stop per month, the keyboard model
    resolving its target day by date (arrows skip disabled days and cross months, Page
    keys step calendar months, Shift+Page calendar years), single and range selection
    with spans and announcements, week numbers, bounds, constraints, the handle, theme
    targets (`calendar`, `calendar-nav`, `calendar-day`, `calendar-range`,
    `calendar-range-preview`), and the public `use_calendar_days`,
    `use_calendar_navigation` and `use_calendar_constraints`.
  - Stage 2 (done): DateInput's pointer field. Typed entry in the field's calendar
    (`date_input/typed.rs`, upstream `useTypedDate`): a complete date reports once per
    typing session, two-digit and omitted years wait for Enter or blur, Gregorian text only
    `parseDateInput` reads commits at Enter or blur, rejected text stays with its own message
    (unreadable, nonexistent, out of range, unavailable) and `on_entry_error_change`
    (AST-111 FR9-FR14, AST-117, AST-118). `field/entry_report.rs` is upstream's
    `useEntryErrorReport` and `useValueReplacesEntry`, shared with TimeInput and
    DateTimeInput, and FormLayout's `defaultEntryErrorBehavior` now has its reader. The
    calendar popover opens from the toggle, a press in the input, ArrowDown or Alt+ArrowDown
    (keeping the caret), closes on a pick, Escape, an outside press, or focus leaving the
    field and calendar (FR8, FR9). Clear, `changeAction` with the busy state, disabled with
    its reason, the format hint and calendar description (AR9), InputGroup membership, and
    AST-043's presentation vocabulary (`date_input/presentation.rs`); the sheet and native
    surfaces are stages 6b and 7.
  - Stage 3 (done): TimeInput's typed field (`time_input/`): a draft that commits as soon
    as it parses to an in-range time, in the locale's digits, separators and day periods
    (FR3), ArrowUp and ArrowDown stepping by `increment` from the value or now with a
    polite announcement (FR4), the locale's hour cycle or `hour_format` with the format
    hint (FR5), Enter and blur commits that keep a rejected draft (`time_input/typed.rs`,
    upstream `useTypedTime`, FR13-FR15), clear, `changeAction`, disabled with its reason,
    and InputGroup membership with its status announced through the shared channel (FR10).
  - Stage 4 (done): DateTimeInput's pointer field (`date_time_input/`): a date segment on
    DateInput's typing session (now `date_input/segment.rs`, shared) and a time segment on
    TimeInput's, one combined value (a time before any date emits nothing, a new date keeps
    the time clamped into a bound on that day, FR2, FR6), the calendar popover, the optional
    preset-time listbox limited to the day's bounds with its keyboard (FR7), the time arrows,
    the date segment's message first with the time rejection through its own alert (FR16),
    clear, `changeAction`, disabled and the wrapping two-segment row (FR5).
  - Stage 5 (done): DateRangeInput (`date_range_input/`): the trigger button named by its
    label and range, with each end written in the calendar and the year only when needed
    (FR6), the range Calendar popover, presets as a labeled group of buttons whose applied
    preset is current and whose out-of-bounds or out-of-span preset is disabled but focusable
    (FR2, FR3, AR1-AR3, ORD1, ORD2), the presets beside the calendar or stacked above it in a
    wrapping row whose divider shows only between the parts (FR5), clear, `changeAction`
    and disabled. Escape drops a range in progress and leaves the popover open.
  - Stage 6a (done): the picker wheels and TimeInput's bottom sheet.
    - Pulled forward from M11 into `astryx_primitives`: `motion` (closed-form springs,
      UIScrollView's decay, projection, rubber band and the UIKit tables the wheels read)
      and `gestures::VelocityTracker`.
    - `wheel/`: the shared rules (`wheelOptions`), the release physics (`wheelPhysics`) and
      one `Wheel` element: UIPickerView's column under a finger (a claimed touch drag or a
      mouse pan, UIKit's coast, snap and rubber band, spring turns, loops, a notch per row,
      a trackpad's pan) and the snapping scroll column under a fine pointer (wheel and
      trackpad scrolling, mouse drags, the quiet-period settle, the tipped rows). Rests on
      a disabled row travel on (DateInput FR13), outside changes turn (FR14), keys step
      in time order and across the loop's seam (FR17), no row is marked while the touch
      column moves (FR18).
    - TimeInput's sheet (`time_input/sheet.rs`, `wheels.rs`, `wheel_rules.rs`): the
      read-only combobox, the BottomSheet of wheels with Save, UIDatePicker's TimeWheels
      under a finger (the AM/PM wheel follows the hour across 11 and 12, the minutes loop
      and keep the hour) and one-column-at-a-time wheels under a mouse (FR7).
    - `coarse_primary_pointer` (`astryx_primitives::modality`): upstream's
      `(pointer: coarse)` remembers the last pointer through key presses, so typing never
      flips a field's surface.
  - Stage 6b (done): DateInput's bottom sheet (`date_input/sheet.rs`): the read-only
    combobox (opened by ArrowDown, Enter or Space, FR8), the month model of any calendar
    (`month_model.rs`, FR21), the month pager (`month_scroller.rs`: six-row month grids drawn
    around the shown month, swipes and trackpads settling onto a month, the day grid's keys
    moving by date across months), the month and year wheels (`month_year_wheels.rs`, leap
    months by name, era labels by the kept day), the header (the month-and-year toggle, arrows
    that hide at the range's ends and hand their focus on, FR22, Reset) and the weekday row
    (FR19). A tapped day commits and the sheet stays open (FR9); under a finger the wheels move
    a chosen date, keeping its day (FR23).
  - Stage 6c (done): DateTimeInput's bottom sheet (`date_time_input/sheet.rs`): two read-only
    segments opening one sheet with a Date and a Time panel (a SegmentedControl switches
    them; "Save date" moves on to the time and its segment takes focus). The Date panel is
    DateInput's touch calendar, the Time panel TimeInput's wheels. A day keeps the time,
    clamped into a bound on that day; a time chosen before any date waits as a draft (FR2,
    FR6). Only disabled and loading lock the sheet; a pending Action still edits (FR9).
    Under a finger the month wheel moves a chosen date, keeping its day and time (FR14).
    SegmentedControl's segments now honour `request_focus` and carry debug selectors.
  - Stage 8 (done, ahead of stage 7): the carried items.
    - PowerSearch's date and time values: TimeInput for `time`; a calendar day as its day
      in `timezoneID`, else UTC, stored as that day's midnight (FR25), or an instant's
      wall-clock date and time in `timezoneID`, else the viewer's zone (FR26); a date range
      as a DateRangeInput, or an inline range Calendar in the sheet (FR17), each part a
      day. Tokens, rows and labels write the value in the provider's calendar and zone.
      `timezone_id` reaches the editor, the sheet and its trigger.
    - Table filtering's `date_absolute` and `time` controls: DateInput and TimeInput
      keeping ISO values.
    - FormLayout's `defaultEntryErrorBehavior`: read by DateInput, TimeInput and
      DateTimeInput since stages 2 to 4.
    - `astryx_i18n::datetime`: `day_of_instant`, `midnight_in_zone`,
      `wall_clock_in_zone`, `instant_in_zone`.
  - Stage 7 (done): the platform's pickers, through fork patch R16 (`b39b0e1`).
    - `Window::show_native_picker` and `native_picker_supported`: the Common Controls
      month calendar or date-time picker in a popup below the field on Windows, the
      browser's `<input type=date|time|datetime-local>` picker on the web, and none
      elsewhere, where `Native` resolves to the pointer's Astryx surface (KNOWN_ISSUES).
    - `date_input/native.rs`: the shared native segment (a press, Space, ArrowDown or F4
      opens the picker over the box; Enter and blur commit) and `NativeDateRejection`
      (upstream `useNativeDateRejection`): a refused date is held until Enter or blur, a
      new value ends it, and a caller's error message repeats once through the alert.
      DateInput's native field commits through `min`, `max` and `dateConstraints`,
      clears without taking focus back, and keeps its picker through a pending Action
      while refusing its commits (FR5, FR11, FR12).
    - TimeInput's native field keeps the field's own status: it steps whole minutes
      (upstream's `step=60`), and a pick outside the bounds is a surface rejection, shown
      at once under `Complete` and otherwise at Enter or blur. The adaptive and
      deprecated policies keep the typed field with seconds or an increment (FR2).
    - DateTimeInput's native segments: a date picker and a time picker. The adaptive
      policies keep the Astryx time with seconds, an increment or preset times, and a
      time picked before any date waits for the date (FR2).
    - The web gallery reads its start state from the query (`?page=dates`), and
      `cargo xtask gpui-local` now patches `gpui_web` too.
- **M9 Shell & navigation:** AppShell, Layout, SideNav, TopNav, NavMenu/NavHeadingMenu,
  NavItem, NavIcon, MobileNav, Outline, Resizable, ScrollableArea (AST-025), Carousel,
  SnapPaging.
  - Stage 1 (done): fork patch R17 (`1aea36f`), the scrolling AST-025 needs from the
    platform. A scroll moves the innermost element that can take it and chains to its
    ancestor at the edge (FR14); `overscroll_contain` keeps it there; every scrolling
    `div` draws scrollbars with a draggable thumb and a paging track, auto-hiding where
    the platform does (FR15), styled by `scrollbar_thin`, `scrollbar_none` and
    `scrollbar_color` (CSS `scrollbar-width` and `scrollbar-color`). Every scroll region
    already ported gains them; TabList's strip hides its bar, as upstream's does.
  - Stage 2 (done): ScrollableArea and `useScrollableArea` (AST-025).
    `astryx_primitives::scroll_area` is the hook: the requested logical axes, the
    effective-axis and edge state measured after each layout and published at most once
    a frame when it changes (FR4-FR9, PR1), a named tab stop only while a requested axis
    scrolls that keeps focus when the overflow goes (FR12-FR13, AR1-AR2), the scrolling
    keys (arrows, Page, Home, End, and Space from the viewport) passing on at an edge
    (FR14), and `overscroll: contain` on the effective axes. ScrollableArea composes it
    with a real content box (FR2), padding that the box publishes as container inset and
    `full_bleed` for the viewport (FR5), the neutral thumb under a fine pointer (FR3) and
    the `scrollable-area` theme target.
    - Carried in: Table's scroll region gets the shared edge fade under a coarse pointer,
      off on the edges its pinned columns mark (FR22, FR19), and contains sideways
      scrolling. Tokenizer in an InputGroup now scrolls its tokens in one track, with a
      hidden scrollbar, faded edges and keyboard focus revealing its token, as upstream's
      `TokenTrack`.
    - The gallery has a Shell page for M9.
  - Stage 3 (done): Resizable (`resizable/`). `use_resizable` (and
    `use_resizable_regions` for named regions) ports `createResizable`'s model: pixel,
    exact-percentage and one-bound percentage sizes resolved against a container
    (`ResizableBasis` and its probe) or the window (FR5, AST-010), the maximum winning
    a conflict (FR6), a default that resolves once, snaps, collapse below the threshold on
    a lift (with the release velocity's projection) or a key step, controlled and
    uncontrolled collapse, `collapse`, `expand` and `resize`, and `autoSaveId`
    persistence written once a person resizes and once per gesture
    (`astryx_primitives::storage`: localStorage in a browser, a JSON file natively).
    `ResizeHandle` is the APG window splitter: the separator's value, bounds and
    "Collapsed" text, the grab zone, the grip pill and its placement, inline or overlay,
    the divider, a pointer drag inverted under right-to-left (FR3), the arrow keys with
    snap steps, Home, End, Enter and a double-click collapse, disabled with its tab stop,
    a lost release cancelling the drag, and the `resize-handle` and `resize-handle-pill`
    targets (FR1-FR4, FR7, FR9, AR1).
    - Carried to M11: the pane slide and the rubber-band overshoot (FR8, AST-105 FR9).
  - Stage 4 (done): Layout, LayoutHeader, LayoutContent, LayoutFooter and LayoutPanel
    (`layout/regions.rs`). The Layout escapes its parent's published padding, publishes
    none inside, and arranges the slots; each region insets per edge by the Layout-edge
    inset (`padding`, else the nearest padding container's, else step 4) or the inner
    inset, collapses beside a header or footer without a divider, and publishes what it
    applies. The divider default stops at a layer, as do the container kind and the
    padding container (`astryx_primitives::provide_in_layer` and `use_layer_context`,
    upstream's layer-scoped contexts). Content and panels scroll on `scroll_area`,
    named only while they scroll; a panel takes a `resizable` region's size, draws its
    divider on its content-facing edge or overlaps the content by the inner inset, and
    sends focus to its handle when it collapses around it (AR2). Header, content and
    footer reflect the container (`card`, `dialog`, `bottom-sheet`).
    - Carried in: Dialog, Drawer and BottomSheet are padding containers, and
      DialogHeader is a LayoutHeader, so a dialog composes a Layout as upstream's does.
  - Stage 5 (done): NavIcon and NavHeadingMenu (`nav/`). NavIcon is the accent circle at
    a control's size, with no role, name or stop. NavHeadingMenu is one `menu` (none
    inside a heading's own menu) whose NavHeadingMenuItem rows register with it each
    frame: the arrows wrap through every row, disabled ones included, Home, End and
    typeahead move, Enter and Space choose, Escape closes the heading's popover; an
    enabled choice runs `on_click`, follows its `href` through the link provider, then
    closes; a press that lands on a row slides across the rows and acts on the one it
    lifts on, and a mouse released elsewhere dismisses (FR1-FR8). The heading contexts
    (close, dismiss, host) wait for SideNavHeading and TopNavHeading.
  - Stage 6 (done): SideNav (`side_nav/`). The nav landmark with a sticky top (header,
    top content), a scrolling list, and a sticky bottom (footer, then the collapse button
    and footer icons at the small size); `collapsible` (plain or controlled) and
    `resizable` (plain, configured, or a `use_resizable` region that owns the width and
    the collapse, FR7) both run on one region, with the overlay ResizeHandle at its end
    edge; render modes for AppShell's top bar and drawer. SideNavItem: icon and selected
    icon, `aria-current="page"`, end content and actions, nested items indented one
    `spacing-6` per level inside the row (FR3, FR4), collapsible groups with a chevron or
    an independent toggle; in the rail only items with icons show, a leaf with a tooltip
    and a parent opening a flyout of its children (FR2). SideNavSection, SideNavHeading
    (links, the menu popover from the heading or its own trigger, NavHeadingMenu's close
    or dismiss by state) and SideNavCollapseButton (in the nav or controlled outside).
    - Carried to M11: the rail's reveal motion (FR6, AST-105).
  - Stage 7 (done): TopNav (`top_nav/`). The bar: heading and start content, end content
    at the inline end, or three regions around center content, wrapping when it runs out
    of room (FR3, FR4); start content wins over children (FR2); the phone's bar (heading
    and end content, FR5) and the drawer's content (FR6) for AppShell. One hover
    recognizer (`top_nav/hover.rs`, upstream `useMenuHover` with INV11's intent) drives
    every menu: an entry delay, an exit guard with the safe triangle, a 500 ms click
    guard, a reopen suppression, and the nav as a group, so moving from an open menu
    opens the next at once and opening one closes the others. TopNavItem (selected,
    disabled with its tab stop, icon-only, the drawer row that closes the drawer except
    after a modified link click); TopNavMenu (NavHeadingMenu rows with the 40 px icon box,
    per-item `close_on_select`, a mouse pick leaving it open); TopNavMegaMenu (a panel
    below the nav, aligned to the trigger's slot, two item columns and the featured
    content, light dismiss, the hidden close button, focus to its first link on a
    click-open, its fade and 4 px slide); TopNavMegaMenuItem and FeaturedCard (the
    decorative image, the "label →" link mirrored in RTL); TopNavHeading (its six
    configurations, the chevron named with the heading, the menu overlapping the block,
    headed by a button that closes it). NavHeadingMenu now keeps one roving tab stop.
    - Carried to M11: the drawer groups' height animation (TopNavMenu FR7, TopNavMegaMenu
      FR9).
  - Stage 8 (done): MobileNav and MobileNavToggle (`mobile_nav/`), with
    `app_shell::AppShellMobile` (upstream `AppShellMobileContext`, `useAppShellMobile`) that
    AppShell will provide. A modal layer on the shared modal pieces: the drawer slides in
    from its inline edge over the theme's overlay dim (FR7, FR13), at most 85% of the
    window wide (FR9), with a header (a string as a level-2 heading that names it, FR10)
    and close button above a scrolling navigation landmark (FR14). Controlled, or AppShell's
    state (FR2); Escape, the backdrop and the close button close it (FR4); the dialog closes
    by 250 ms while the drawer slides out inert (FR5); `side: auto` follows the opener's
    half of the window (FR8). The toggle shows only in AppShell's mobile layout (FR11).
    TopNav's drawer is now a MobileNav headed by its heading, and its phone bar carries the
    toggle (TopNav FR5, FR6); every drawer row closes the drawer through the context.
  - Stage 9 (done): AppShell and `useAppShellMobile` (`app_shell/`), with fork patch R18
    (`position: sticky`). The skip link, then a Layout of the header region (banner above
    the top nav, `role="banner"`), the side region and the main region (`role="main"`,
    `content_padding`, a scroll region in `fill` whose tab stop exists only while it
    overflows), then the drawer (FR1-FR5). `fill` fills its parent (the window, for the
    outermost frame); `auto` grows and its header sticks at the top of the page with the
    side region sticky below it (FR6). Variant backgrounds and the elevated content's page
    corner (FR7, FR16). Below the theme's width point (`widthBreakpoints`, now on
    `Theme`): the side nav leaves the inline region, the top nav becomes the phone's bar
    carrying the side nav into its drawer, or a side-nav-only bar with its toggle; the
    auto drawer, a node, or the config's content; `false` (FR8-FR12). It provides
    `AppShellMobile` (FR13) and resets the Layout contexts at its root (FR18). Slots are
    builders, since a nav draws more than once. SideNav's drawer is now a MobileNav, and
    its rows close the drawer. `defaultIsMobile` (an SSR hint) has no native form: the
    first frame reads the window.
    - Carried to M11: the content area's swap as the side nav collapses (FR17).
  - Stage 10 (done): Outline and `useOutlineFromDOM` (`outline/`), on in-page anchors
    (`astryx_primitives::anchor`: a Heading with an id records its box and the scroll root
    it sits in, which AppShell's main region and ScrollableArea provide). Links indented by
    level, each in the Tab order, `aria-current="location"` on the current one, over a 2px
    track whose indicator slides to it (FR1-FR3, FR6, FR7). A link scrolls its heading
    smoothly to the scroll root's top below `offset` (at once under reduced motion);
    `on_navigate_start` and `on_navigate_end`, once, when it settles or the reader's scroll
    interrupts it; a second navigation ends the first; `scroll_on_click(false)` (FR4).
    Uncontrolled, the current heading follows the scroll on the activation line (FR5).
    `use_outline_from_container` lists the headings with ids inside a container.
    - Carried to M10: `useOutlineFromMarkdown`, which shares Markdown's parser and slugs.
  - Stage 11 (done): Carousel and `useSnapPaging` (`carousel/`). A row that scrolls sideways
    (Shift with the wheel too), with edge fades and previous and next buttons that hide,
    leaving no tab stop, when their direction runs out, a keyboard-pressed one handing
    focus to the row (FR1-FR4); wrap-around; the handle's `scroll_next`, `scroll_prev`,
    `scroll_to` and `can_scroll_*`. GPUI has no CSS scroll snap: the stops are computed
    as upstream's (`carousel/stops.rs`: item starts less the padding, clamped, merged, the
    end-page rule), and the row glides onto the nearest one once a gesture ends (a
    trackpad's release, or a wheel at rest) (FR5, FR6). Snap paging moves at most one page
    per flick (a view or an item) from the page the drag began on (FR7); an edge button
    steps about a view onto a stop.
- **M9 done.** Every M9 component is ported; its carried items are in HANDOFF's table.
- **M10 Content:**
  - Markdown: pulldown-cmark/GFM mapped to Astryx nodes; upstream plugins; streaming parse;
    URL safety.
  - CodeBlock: port of the in-house grammar tokenizer with `fancy-regex`, 30 languages, 12
    syntax presets.
  - Chat suite: layout, message list with stream scroll, composer with tokens and trigger
    menu, tool calls, dictation behind a pluggable speech provider.
  - Stages, each closed with tests, a gallery check, docs, the gate, a commit and CI:
    1. The syntax engine: the grammar tokenizer on `fancy-regex`, the built-in languages
       and the nine importable ones, the syntax theme tokens, `defineSyntaxTheme`, the
       presets and SyntaxTheme (with `useSyntaxTheme`).
    2. CodeBlock: header, language label, line numbers and highlighted lines, diff, copy,
       collapsing, wrapping, height, size, width, container, highlight mode.
    3. Markdown's core: CommonMark and GFM onto upstream's AST, block and inline
       rendering with the content components, density, heading levels, content width,
       URL safety and `onLinkClick`, autolinks.
    4. Markdown's extensions: plugins (syntax, AST transforms, renderers), inline plugins,
       citations and sources, math through `components`, code languages, streaming with
       its fade-in, heading links, and `useOutlineFromMarkdown` (carried from M9).
    5. Chat's layout and messages: ChatLayout and its scroll button, ChatMessageList with
       stream scroll and new messages, ChatMessage, its bubble and metadata,
       ChatSystemMessage, the typing indicator, the unread divider, the virtualizer.
    6. The composer: ChatComposer, its input (tokens, trigger menus, history, paste as
       token), drawer, token element, ChatTokenizedText, ChatSendButton.
    7. ChatToolCalls, dictation (`useSpeechRecognition` behind a pluggable provider,
       `useChatDictation`, ChatDictationButton), the emoji picker and the reaction bar.
  - Stage 1 (done): the syntax engine, the new `astryx-syntax` crate. `tools/export/export-code.mjs`
    exports upstream's grammars (17 built-in grammars under 42 identifiers, and the 9
    language modules), the 12 syntax presets, and fixtures of upstream's own tokenizer over
    one sample in every language; codegen translates each JS regex into `fancy-regex`
    (flags inline, JS's ASCII `\w` and `\d`). The tokenizer scans as upstream's (rules in
    order at each position, the first non-empty match wins, lines on their own, the default
    type left out) and matches the fixtures token for token. Custom languages
    (`create_code_language`), the fence and diff helpers, `define_syntax_theme` and the
    presets; SyntaxTheme and `use_syntax_theme` in `syntax_theme/`, with upstream's
    defaults for unset diff slots.
- **M11 Motion & touch:** springs, decay, rubber-band, gesture recognizers on GPUI pointer and
  touch events, keyboard clearance, dragToSnapPoints.
  - Components: BottomSheet(+Switcher), Swipe, coarse-pointer variants (sheet-backed
    Popover/Selector).
  - Table row expansion's `animatedLayout`: opening or closing a panel glides the rows
    after it, with the chevron on the glide's spring (Table FR15; Pending in
    `USE_TABLE_ROW_EXPANSION_DOC`).
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

## Decision log

- **Own primitives on Zed-git GPUI, not Longbridge's gpui-component (2026-11-24).**
  gpui-kit, gpui-base and gpui-component depend on crates.io `gpui-pre =0.3.14`. That is a
  different crate from the Zed-git `gpui` the user's apps use, so its types cannot mix with
  theirs, and adopting it would move every app off Zed git.
  gpui-component (Apache-2.0) stays useful as reading material for the hardest pieces: the
  text editing engine and the virtual table. Adapted code keeps attribution.
- **Our own GPUI fork for renderer-level effects (2026-11-24, replaces "no fork").**
  - Backdrop blur, element transforms, filters, masks and blend modes need new shader code
    inside GPUI.
  - They live on the `astryx` branch of github.com/lukasgeciauskas/zed, and all the user's
    apps pin that fork. See `docs/gpui-fork.md`.
  - Everything that needs no renderer change stays in this layer.
  - R1 (element transforms) landed without shader changes: elements only need
    rectangle-preserving transforms, which the window applies to primitive geometry.
- **ICU4X for everything upstream does with `Intl` (M2, 2026-11-24).** Plural rules,
  numbers, percentages, unit and list formats, dates and times in every CLDR calendar, and
  relative times come from ICU4X's compiled CLDR data, the data behind `Intl`. Its
  experimental crate (percent, units, relative time) is pinned with the stable 2.x crates
  GPUI already uses. Time zones (IANA rules, the system zone) come from jiff, with the tz
  database bundled on Windows and wasm. Translations ship as generated static tables
  (`cargo xtask codegen`), 30 locales behind the facade's default `locales` feature.
- **Agent tooling is part of the port (M1.5).**
  - `CLAUDE.md`;
  - the `astryx` CLI (`apps/cli`), with upstream docs, the Rust API, tokens and parity;
  - `cargo xtask gpui-src`;
  - hot reload through `cargo xtask dev`.
  The goal is fast, accurate agentic iteration.
