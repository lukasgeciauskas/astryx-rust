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
  - Text fields (TextInput, TextArea, NumberInput) stay controlled by `value`, as
    upstream. Their editing state (caret, selection, undo, IME) lives in a keyed
    `astryx_primitives::editing::TextEditState` that follows `value` every render.
  - Heavy editors (Table, Calendar, CodeEditor) use an app-owned `Entity<XState>` plus
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
- **Joined groups** (InputGroup, ButtonGroup): CSS's `:first-child` and `:last-child`
  become a context the group provides around each direct child (its place), which the
  member reads; a member clears it for its own insides. A joined member's focus ring is
  painted `deferred` so the next member never covers it.
- **Accessibility states:** set `aria_disabled`, `aria_read_only`, `aria_required`,
  `aria_invalid` and `aria_busy` (fork patch R6) and `aria_current` (R9) on the element
  that carries the role.
- **Anchored layers** (tooltips, popovers, menu flyouts): wrap the trigger in
  `astryx_primitives::anchored_layer(id, trigger)` and give it `.surface(content,
  LayerOptions { placement, align, offset, margin, rtl, .. }, on_dismiss)`. The layer is
  layout-transparent, measures the surface, places it with `position_layer` (flip to the
  other side, then slide into the viewport, mirrored in RTL), paints it deferred and joins
  the layer stack, so Escape and outside presses reach the topmost layer first.
- **Popup triggers:** a popup provides an `overlay::Disclosure` around its trigger
  (expanded, popup kind, toggle); Button reads it for `aria-expanded` and
  `aria-haspopup` (fork patch R12) and toggles after its own click handler. A layer
  that takes focus makes its surface a `tab_trap()` with a non-tab-stop focus handle,
  moves focus in right after the frame that first draws it (a deferred call from a
  probe painted in the surface), and returns focus that would be lost on close.
  - A menu's Disclosure also carries `on_press`, `on_hover`, `on_key` and `open_tint`:
    Button calls them on an enabled trigger (press-down opens a menu, ArrowDown/ArrowUp
    open it on the first or last row, the trigger keeps the pressed tint while open).
    A trigger that is not a Button (a breadcrumb's link) is `overlay::MenuTrigger`.
- **Modal surfaces** (Dialog, Drawer, BottomSheet, Lightbox) share `overlay::modal`:
  `modal_state`, `sync_open`, `phase` (enter and exit progress), `backdrop`, `probes`
  (focus entry) and `modal_layer`.
- **Menus** all draw `overlay::menu::MenuList` over a `MenuTree` (the root state, the
  close action, size, and whether Tab leaves, rows are touch rows or submenus drill in).
  - Entries are data: `MenuEntry` (`MenuItem`, `MenuCheckboxItem`, `MenuRadioGroup`,
    `Divider`, `Section`); a `MenuItem` with `submenu(..)` is upstream's
    DropdownMenuSubMenu.
  - The owner (DropdownMenu, ContextMenu) keeps a `MenuRoot` (`menu_root`,
    `sync_root`), records its surfaces with `surface_probe`, and passes `regions`
    (whether submenus may hold a header and footer).
  - Any element can carry a context menu: `overlay::with_context_menu(area, ContextSpec
    { .. }, window, cx)`; text fields do it through `input::field_menu`.
- **Search sources** (`crate::search`): `Searchable` items, a `SearchSource` that
  answers `Ready` or `Pending(Task)` and may show partial lists through its
  `SearchContext`, and `static_source(items)`. CommandPalette uses them; Typeahead
  and Tokenizer should too. Selector filters its own options, as upstream does. A
  source built in `render` has no identity: keep it in an `Rc` or name it with a key
  when a change must re-run a query.
- **Hosts** (Card, Section, Dialog, BottomSheet) wrap their children with
  `layout::host_children(children, padding, corners)`: `ContainerPadding`,
  `ContainerCorners` and each child's `ContainerChild` place, which full-bleed
  content (a Divider, a TabList, a bleeding List) reads.
- **Comboboxes** (Selector, and MultiSelector after it): focus stays on the trigger or
  the search field, and the highlighted option claims it with
  `aria_active_descendant_of(&focus)` (fork patch R13), since the list sits in
  another layer. The keys are `selector::combobox::Combobox::key` (pure; apply its
  `KeyOutcome`). Options take the pointer by press: rows highlight on mouse move
  and pick on the release of a press that began in the list (a window
  `MouseUpEvent` listener registered while the list is drawn), never on click.
  `AnchoredLayer::position_with` places a layer by any rule (Selector's selected
  option over its trigger).
- **Tooltip props:** a component's `tooltip` goes through `overlay::with_tooltip(id,
  text, acts, el)`; `labelTooltip` is an InfoTip in the label row (`FieldLabel`);
  `disabledMessage` is `overlay::disabled_reason`, which shows on any focus; a tooltip
  status is `input::control::status_indicator`, a focusable glyph. A pressable inside a
  Tooltip reads `tooltip_description(cx)` for its `aria_description`.
- **Inline content:** a Text with children is a paragraph (`text::inline`). A component
  that can join a paragraph's lines gets a `flows_inline` check and an `inline(self,
  flow, window, cx)` method, and `InlineBuilder::child` recognizes it through
  `downcast_mut::<ViewElement<T>>()` (fork patch R10). Its text joins in its own style
  (`flow.styled`), an interactive span is a `flow.grouped` group with a stand-in element
  for focus, role and keyboard activation, and anything else stays an inline box.
- **Measuring yourself:** a component that depends on its own size (Grid's responsive
  tracks, Stepper's compact mode, Text truncation tooltips) paints an absolute
  `canvas` probe over its box, keeps the size in keyed state and requests another frame
  when it changes.
- **Pressables** (Button and everything built on it) share `press::apply_press`, which
  provides:
  - a tab stop (`control_focus`);
  - the hover and pressed overlays composited over the fill;
  - the press scale, skipped under reduced motion;
  - the focus ring;
  - the disabled and busy guards: focusable, but inert.

- **The port's own strings** (text for what upstream leaves to the browser, such as a
  text field's context menu) live in `astryx-i18n/src/local.rs`, English only, as the
  catalog's last fallback. Use the `@astryx.<area>.<name>` key style; a test fails if
  upstream ever ships the same key.

## Code

- Generated files (`*/generated/*`, `spec/`, `crates/astryx-icons/assets`,
  `crates/astryx-themes/fonts`) are never edited by hand. Change the exporter or the
  codegen and regenerate.
- Comments explain why, not what. Public items get a one-line doc comment naming the
  upstream prop or concept they map to.
- Licensing: GPUI's own crate and examples, and Longbridge's gpui-component, are
  Apache-2.0. Code adapted from them keeps an attribution comment. Never copy from Zed's
  GPL-licensed crates (`editor`, `ui`, and the rest of the Zed app).
