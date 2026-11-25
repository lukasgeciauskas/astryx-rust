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
