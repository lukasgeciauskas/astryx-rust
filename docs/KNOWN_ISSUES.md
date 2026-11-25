# Known issues

## GPUI

- **A font stack with no available family panics.** GPUI's text system panics when a
  font family and every fallback are missing (for example `"SF Mono", Monaco, Consolas`
  in a browser).
  - Astryx avoids it: every theme passes through `astryx_theme::prepare`, which keeps
    only families the text system has and appends a bundled face (Figtree or
    JetBrains Mono).
  - Code that sets `font_family` directly on GPUI elements must use a registered family.
  - On the web a panic aborts the wasm instance with the app's `RefCell` still borrowed,
    so every later event logs `RefCell already borrowed` and the page stops responding.
    Those log lines are a symptom of an earlier panic, not a separate bug; look for the
    first `panicked at` line.

## Components

- **A dragged row's place shows its text, not a copy of the row.** GPUI draws an element
  once a frame, so while a row is dragged the row itself (dimmed) rides the pointer as
  the preview, and its place shows its label, description, grip and divider, dimmed;
  start content and custom end content are not there. Upstream dims the row in place and
  draws a copy at the pointer. GPUI also reports no pointer type, so a finger's drag
  starts like a mouse's (4px of travel), without upstream's hold; and Escape cancels a
  drag, which upstream's mouse drag has no key for. Under shift feedback the passed items
  jump to their slots; their slide waits for the motion layer (M11).

- **Table: what GPUI's layout and AccessKit change.** A table's scroll region shows no
  edge fade where it can scroll; the shared scroll behavior lands with ScrollableArea
  (M9). An unknown row total (ARIA's `aria-rowcount=-1`, while more rows may load) has no
  AccessKit form, so no count is exposed then. A composed table's columns fit their
  content as a grid's `auto` tracks, which share leftover width equally rather than by
  content as HTML's automatic table layout does. A plugin transform cannot throw, so
  FR10's failure isolation has nothing to catch.

- **Table plugins: what Rust's ownership changes.** Upstream passes a row's object to
  cells and plugins, and compares rows by reference. Here a cell or plugin gets a
  `TableItem`, a handle into the table's data that a callback can keep, and a row-number
  column given `data` without a key function matches a row in that same data by position,
  else the last equal row (`PartialEq`); rows the caller copied, such as
  `use_table_sortable_state`'s sorted rows, need `data_keyed`. Activation's `data-active`
  attribute has no GPUI form (the row's `aria-current` and marker carry the state), and a
  table cell holds no selectable text, so FR6's text-selection gate never applies.
  Upstream's development warnings print once to stderr in debug builds.

- **Tree and grouped tables take their structure in Rust's terms.** A tree's
  `childrenKey` is a function returning a row's children (`TreeStateConfig::new`).
  Grouped rows cannot put header objects into the caller's data as upstream does: the
  hook returns the visible product rows as `data` and the plugin plans the headers among
  them, so other plugins' row and cell transforms never see a header. Row-click expansion
  (tree and row expansion) leaves a press to a nested control that stops its click
  (buttons, checkboxes, links); a table cell holds no selectable text to guard.

- **Sticky columns and column resize in GPUI.** A pinned cell follows the table region's
  sideways scroll and paints over the cells that scroll under it, as `position: sticky`
  does; each data cell is placed in its grid explicitly for that. A table's columns still
  run left to right in a right-to-left layout (M15's RTL work), so start-pinned columns pin
  to the left there too. A resize boundary is drawn deferred, over the body rows and inside
  the scroll region; touch input stands in for a coarse pointer, which takes the boundary
  out of pointer reach but leaves its keyboard resize. An unbounded column's announced
  maximum is the window's width (upstream's window extent).

- **Table virtualization in GPUI.** Windowed rows scroll in the table's own region, which
  takes its height from the table's parent (give the table a sized parent); a plugin that
  wraps the region (pagination's controls) leaves it unsized, so the rows then scroll with
  the page. Column windowing keeps a focused cell's column mounted by noting the column a
  key or a press last reached, as GPUI reports no focus moving into a cell; it forgets it
  when focus leaves the table.

- **Infinite scroll has no IntersectionObserver.** The sentinel checks, as it paints,
  whether it shows inside every clip above it, which is what the observer reports with
  its default root. `on_load_more` returns a `Task` of a `Result`, so FR4's non-Promise
  case cannot happen. The Load more button stays in view while the table scrolls sideways
  by following the region's scroll offset (upstream's sticky button).

- **The keyboard hint anchors to a focus ring.** GPUI tells no element's bounds by its
  focus, so the arrow-key hint (TabList, Toolbar, SegmentedControl) sits below the
  innermost focus ring drawn inside its composite, one frame after focus enters. A
  control that draws no Astryx focus ring puts the hint below the composite's start.

- **A windowed list needs its scroller to hold its anchor.** `WindowedList` windows from
  the clips above it wherever it sits. To keep its first visible row (or its end, or a
  given item) in place as rows change, it must move the scroller, so it needs a
  `ScrollHandle`: its `.scroll(..)`, or the nearest `ScrollHost` a page provides. Without
  one, rows still window but the scroll position does not follow the changes, and a scroll
  mounts its new rows one frame late. Rows the List does not hold directly get no
  dividers, so a windowed List draws none.

- **Butter's status fields show no status border, as upstream's do.** Butter's
  `text-input` base rule sets `borderColor` in the theme layer, which outranks the status
  border in upstream's CSS too; only the status icon and message show the status. The port
  matches upstream.

- **Inline text flow is close to CSS, not identical.** A Text with children lays them out
  with `InlineFlow` (`text/inline.rs`):
  - Whitespace is kept as written: runs of spaces do not collapse, and a newline breaks
    the line (GPUI text behaves the same way).
  - An inline box (anything other than text, an inline Text or a Link) sits at its
    max-content size, its bottom the paragraph's descent below the baseline; CSS would
    use the box's own last baseline.
  - Bidirectional text is laid out left to right, and text in a paragraph cannot be
    selected.
- **Spell checking depends on the platform's dictionaries.** Windows uses the system
  checker. Linux reads Hunspell dictionaries from `/usr/share/hunspell` and similar folders
  (packages such as `hunspell-en-us`); with none installed, nothing is marked. Browsers do
  not expose their checker, so on the web an app installs one with
  `astryx_primitives::spell::set_spell_checker(DictionaryChecker::new(aff, dic))`.
  `spellbook`, which reads the dictionaries, is MPL-2.0.

- **Dialogs pad their own content until Layout lands (M9).** Upstream's Dialog holds a
  Layout whose header, content and footer take the dialog's inset; here the dialog pads
  its children by that inset (`spacing-4`) and DialogHeader reaches to the edges for its
  divider. Long content scrolls the whole surface rather than a LayoutContent region.
- **Toasts need a ToastViewport in the window.** Upstream mounts a fallback viewport on
  the first toast when no LayerProvider exists; GPUI cannot add to a window's tree from
  outside it, so an app renders `ToastViewport::new()` once in its root view.
- **A popup trigger has no `aria-controls`.** AccessKit links nodes by id, and GPUI does
  not expose the node id of another element, so a trigger reports `aria-expanded` and
  `aria-haspopup` but not which surface it controls.
- **A text field's context menu takes focus from the field.** Browsers keep focus in a
  field while their native menu is open. Here the menu is an Astryx menu, so the field
  blurs as it opens (its `onBlur` runs) and focus comes back when it closes. Its strings
  ("Cut", "Add to dictionary"...) are the port's own, not upstream's, and ship in
  English: an app translates the `@astryx.textField.*` keys with its catalogs.
- **Lightbox settles on eased timing, not springs, until M11.** Its zoom, paging and
  drag-to-close settle over 280ms on the `move` easing; upstream uses UIKit's measured
  springs, which arrive with the motion layer (M11). Lightbox has no backdrop blur (GPUI
  has none; see `astryx docs effects`), and video items open in the system player until
  MediaPlayer's decoder (M13).
- **A CommandPalette source has no identity across renders.** React compares the
  `searchSource` object; a Rust value built in `render` is new every frame. A source kept
  in an `Rc` (`CommandPalette::from_rc`) or named with `source_key` re-runs the open
  palette's query when it changes (FR9); a plain `new(source)` never does.
- **Selector's and MultiSelector's empty texts are text.** Upstream's `emptyText` and
  `emptySearchText` take any node and announce the words it renders; here they are
  strings, which the live region speaks. A trigger whose search makes it a button is always described as
  required when it is, since its name is the bare label (upstream's name can carry the
  label's marker).
- **PowerSearch's date and time values wait for M8.** Their editors show the value
  (an ISO date in UTC) and cannot change it until M8 brings the date inputs, the
  calendar systems and `timezoneID`; tokens read dates as ISO dates meanwhile.
- **A ComplexSelector's trigger has no separate value.** `trigger_label` and
  `placeholder` are elements, as upstream's nodes are, so the combobox carries the shown
  value as the text inside it rather than as an accessible value.
- **Enter on a MultiSelector action cell presses Astryx buttons only.** Upstream clicks
  the first focusable control in the cell. GPUI cannot click an element from code, so
  the cell presses what registered with it: controls on the shared press path (Button,
  IconButton and what is built on them). A custom element with its own `on_click` in
  an action cell answers the pointer only.
- **A Selector's (or MultiSelector's) `on_focus` misses focus coming back from its clear button.** GPUI's
  focus-in listener does not fire when focus moves between an element and its
  descendants, and the clear button sits inside the trigger's box. A press on the clear
  button also focuses the trigger first, where upstream leaves focus as it was.
- **A TreeList row's name is its text `label`, and a header does not name the tree.**
  Upstream's `label` takes any node and names the row by its text; here `label` is the
  name (and typeahead text) and `label_content` the node shown in its place. Upstream
  points `aria-labelledby` at the header; a GPUI element cannot name another, so pass
  `aria_label`.
- **A List is named by `label`, not its header.** Upstream points `aria-labelledby` at
  the header; a GPUI element cannot name another, so a List takes its name as text.
- **A bleeding List's corners follow its host's direct child.** Hosts tell each direct
  child whether it is first or last; a List nested deeper (in a Stack inside a Card)
  reads its ancestor's place, where CSS's `:first-child` reads the List's own.
- **A delegate row cannot mark its control current.** Upstream writes `aria-current`
  on a selected row's `interactiveRef` control; GPUI cannot set another element's
  accessibility state, so the control reports it itself if it should.
- **The touch hit band knows only its List.** Between rows it reaches half the list's
  gap, and past the first and last rows as far as it needs; upstream also measures
  other neighbouring controls.
- **OverflowList cannot see a flex-row parent.** Upstream gives a single row its unfolded
  width in a flex row and lets it shrink first; GPUI cannot read the parent's display,
  so in a flex row give the list `flex_1()` or a width.
- **A Typeahead's source needs a name to count as replaced.** As with CommandPalette,
  a source built in `render` is new every frame; `from_rc` or `source_key` gives it the
  identity FR3's replaced-source rule needs.
- **The Menu key's name depends on the platform.** ContextMenu opens on Shift+F10 and on
  a key GPUI names `menu`, `contextmenu` or `apps`; a platform that names the Menu key
  otherwise opens it only with Shift+F10.

## Tooling

- **True hot-patching needs our patched dx.** `cargo xtask dev --hot-patch` patches
  edits into the running gallery in about 0.6 s, keeping all state. That covers the
  gallery and every workspace crate under it. Stock dx 0.7.10 fails on Windows: the
  capture fails, and once that is fixed the gallery overflows its stack right after
  the first patch.
  - `cargo xtask install-dx` builds dx 0.7.10 with five fixes, listed in
    `xtask/src/install_dx.rs`. The stack overflow was dx's Windows x64 jump stub
    (`movabs rax, addr; jmp rax`): it clobbered RAX, which `__chkstk` takes as the frame
    size to probe. The first patched function with a large stack frame then probed an
    address-sized frame.
  - Diagnosis tool: hot-reload builds of the gallery on Windows install
    `apps/gallery/src/overflow_probe.rs`, which writes the symbolized frames of a stack
    overflow to `%TEMP%/astryx-stack-overflow.txt`.
  - Subsecond limits still apply: changing a struct's layout, adding a dependency, or
    editing `main` needs a restart, as does a change to GPUI itself. The default
    `cargo xtask dev` (hot restart, about 4 s, plus live theme reload) handles those.
  - The `rust-lld` linker setting breaks dx's link interception, so the repo keeps the
    default linker.
  - Not yet tried on Linux.

- **Chrome screenshots of a background tab hang.** Chrome pauses
  `requestAnimationFrame` in a tab whose window is not in the foreground. GPUI's web
  backend draws from that loop, so a DevTools screenshot (`Page.captureScreenshot`,
  used by browser automation) of such a tab waits for a frame that never comes.
  - The page itself is fine.
  - Bring the window to the foreground first. With Claude in Chrome, a `resize_window`
    call does it. Screenshots then work, and theme switching was verified this way.
