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
- **Selector's empty texts are text.** Upstream's `emptyText` and `emptySearchText`
  take any node and announce the words it renders; here they are strings, which the
  live region speaks. A trigger whose search makes it a button is always described as
  required when it is, since its name is the bare label (upstream's name can carry the
  label's marker).
- **A Selector's `on_focus` misses focus coming back from its clear button.** GPUI's
  focus-in listener does not fire when focus moves between an element and its
  descendants, and the clear button sits inside the trigger's box. A press on the clear
  button also focuses the trigger first, where upstream leaves focus as it was.
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
