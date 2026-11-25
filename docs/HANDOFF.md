# Handoff: where the port stands, and how to pick it up

Read this at the start of a new session, after `CLAUDE.md`. It records the state
of the work, the owner's standing instructions, the workflow that every
milestone has followed, and what comes next. Update it at the end of each
milestone.

_Last updated: 2026-11-25, during M6 (Selector, List and its relatives done)._

## State

- **Milestones done:** M0, M1, M1.5, M2, M3, M4 and M5; M6 is in progress (Selector,
  Pagination's page-size selector, List, MetadataList, OverflowList and Typeahead are
  done). Details and dates are in
  [`PLAN.md`](PLAN.md); each milestone's notes list what was pulled forward,
  invented or carried.
- **Parity:** 77 of 110 core components (`cargo run -q -p astryx-cli -- parity`;
  `component --list` marks ported ones with `*`).
- **Last commits on `main`:** the commit that added this file; `af61065` docs: M5 is done; `f8b5551` Lightbox,
  use_lightbox, CommandPalette and search sources; `9af8f12` menus. CI is green
  on all three (ubuntu, windows, wasm).
- **GPUI:** the owner's public fork, https://github.com/lukasgeciauskas/zed,
  branch `astryx`, cloned at `../zed-astryx`. The workspace pins rev
  `01da1adfa69364e9df02b39d897e24d001cf6fde` (patches R1 and R5 to R13; see
  [`gpui-fork.md`](gpui-fork.md)). The `[patch.crates-io]` block in
  `Cargo.toml` (async-task, calloop) is legitimate and stays.
- **Repository:** https://github.com/lukasgeciauskas/astryx-rust, private.
  Upstream Astryx (the specification) is at `../Astryx`.

### Not yet ported (33)

- **M6 Collections (in progress):** MultiSelector, Tokenizer, ComplexSelector,
  TreeList, PowerSearch. Item's swipe actions wait for M11.
- **M7:** Table. **M8:** Calendar, DateInput, DateRangeInput, DateTimeInput,
  TimeInput. **M9:** AppShell, Layout, Resizable, ScrollableArea, Carousel,
  MobileNav, NavHeadingMenu, NavIcon, Outline, SideNav, TopNav. **M10:** Chat,
  ChatDictationButton, ChatToolCalls, CodeBlock, Markdown.
- **Utilities with no ComponentDoc yet:** InternationalizationProvider, Layer,
  LinkProvider, MediaTheme, SyntaxTheme, Theme. The first five already exist as
  Rust APIs (`astryx_i18n::provider`, `astryx_primitives::layer`,
  `link_provider`, `ResolvedTheme::on_surface` with `scope`, `astryx_theme::scope`);
  registering their docs is a quick win. SyntaxTheme lands with CodeBlock (M10).

### Carried into later milestones (recorded in each doc and in KNOWN_ISSUES)

| What | Waits for |
|---|---|
| Item swipe actions, SegmentedControl's sliding lens, Collapsible's and List's animated layout | Motion layer (M11) |
| Lightbox springs (it eases over 280ms now) | Motion layer (M11) |
| Lightbox video playback and `hasAutoPlay` (a video opens in the system player now) | MediaPlayer's decoder (M13) |
| Dialog content inset done by the dialog itself | Layout (M9) |
| FormLayout's `defaultEntryErrorBehavior` readers | Date inputs (M8) |
| Backdrop blur (Lightbox, scrims) | Fork patch R2 |

## The owner's standing instructions

- **A 1:1 replica.** Every upstream prop, behavior and FR item is ported. Where
  GPUI lacks something, build it (in this layer, or as a fork patch) rather than
  defer it: the inline text flow, the focus traps, the press model and the text
  field context menu were all invented this way. Defer only to a milestone
  that builds the missing dependency, and record it as `RustApi::Pending` or in
  the table above.
- **Finish a whole section.** A milestone is done when everything in it works,
  including items earlier milestones carried into it. Don't stop halfway, and
  don't leave stale "lands in Mx" notes behind (grep for them before closing a
  milestone).
- **Permissions.** Ask before installing software or accepting any agreement.
  Don't capture other apps' windows; only the gallery.
- **Privacy.** The owner's email never goes to an outside service.
- **Commits.** Each stage: run the gate, then commit and push to `main` with the
  global git identity. Messages end with the two attribution lines the session
  supplies (`Co-Authored-By: ...` and `Claude-Session: ...`). Never commit a
  `gpui-local` `[patch]` block.

## Workflow per stage

1. `cargo run -q -p astryx-cli -- component <Name> --dense` for the props, then
   read `../Astryx/packages/core/src/<Name>/` and
   `../Astryx/docs/specs/components/<Name>/<Name>.spec.md`. Number the tests after
   the FR items.
2. Implement in `crates/astryx-components/src/<module>/`, with every prop mapped
   in a `ComponentDoc` (`doc.rs` registry) and a gallery example in
   `examples.rs`.
3. Tests: `#[gpui::test]` with a host view; see the patterns in
   `overlay/menu_tests.rs`, `overlay/lightbox_tests.rs` and
   `overlay/palette_tests.rs`.
4. Look at it: temporarily default an example open (mark the line
   `// TEMP-SCREENSHOT`), `cargo build -p astryx-gallery`, then
   `$env:GALLERY_PAGE='overlays'; $env:GALLERY_SCROLL='400'; powershell -File
   tools/capture-window.ps1 -Exe target/debug/astryx-gallery.exe -Out <png>
   -WaitSeconds 20`, read the PNG, and revert (grep `TEMP-SCREENSHOT` must find
   nothing).
5. Update `PLAN.md` (the milestone's notes), `KNOWN_ISSUES.md` and this file.
6. The gate, in this order, with fmt's check last:
   - `cargo fmt --all`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `cargo xtask codegen --check`
   - `cargo build -p astryx-gallery --target wasm32-unknown-unknown`
   - `cargo fmt --all -- --check`
7. Commit, push, and watch CI: `gh run list --limit 3`, `gh run watch <id>`.

## Lessons from M4 and M5

GPUI facts are in [`guide/gpui.md`](guide/gpui.md) and this layer's patterns in
[`guide/conventions.md`](guide/conventions.md) (both also print with
`astryx docs gpui` / `astryx docs conventions`). The ones that cost the most
time:

- **Keyed state is scoped to where `render` runs.** A component that makes
  keyed state for another component it renders does not see that component's
  state. Hand the entity over instead (`Popover::controller`).
- **`on_next_frame` never runs in tests.** To act after the frame that first
  draws something, paint a `canvas` probe whose paint calls `window.defer`.
- **Escape reaches layers through `observe_keystrokes`,** after dispatch, so an
  element cannot stop it. A surface that wants Escape (a drilled menu) blocks
  its layer's Escape (`EscapeBehavior::Block`) while it handles the key.
- **Clippy:** `type_complexity` fires on `Rc<dyn Fn(...)>` fields and
  `too_many_arguments` past seven; add type aliases and small structs as you
  write, not after.
- **Shell:** the bash tool breaks on inline Python heredocs that contain quotes.
  Write edit scripts to the scratchpad with the Write tool and run them with
  `python <file> .`. Re-read code after `cargo fmt` before matching text in it.
- **Tests:** an element needs `.debug_selector(..)` for `cx.debug_bounds`; menu
  rows carry `<list id>-<row key>`, palette rows `command-palette-item-<id>`.
  Read announcements with `astryx_primitives::announce::announcement(..)`.
  Background `spawn` needs `Send`; items holding an `Rc` use `cx.spawn`.

## Next: M6 Collections

Suggested order, each a commit:

1. ~~**Selector**, then Pagination's page-size selector~~ (done: `selector/`,
   fork patch R13; Selector filters its own options, as upstream does).
2. ~~**List** (and Item's remaining props), then **MetadataList**,
   **OverflowList**~~ (done; List's `reorderable` comes with step 4).
3. **Typeahead** and **Tokenizer** (share `search::`), **MultiSelector**,
   **ComplexSelector**, **PowerSearch**.
4. **TreeList** (tree focus), virtualization for long lists, reordering.

Before starting, read PLAN.md's M6 entry and the upstream `Selector/`,
`Typeahead/` and `List/` folders; `Selector/hooks.ts` holds the combobox
behavior that CommandPalette already follows.
