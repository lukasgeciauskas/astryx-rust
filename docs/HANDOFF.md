# Handoff: where the port stands, and how to pick it up

Read this at the start of a new session, after `CLAUDE.md`. It records the state
of the work, the owner's standing instructions, the workflow that every
milestone has followed, and what comes next. Update it at the end of each
milestone.

_Last updated: 2026-11-27, after M13 (Charts & lab)._

## State

- **Milestones done:** M0, M1, M1.5, M2, M3, M4, M5, M6, M7, M8, M9, M10, M11, macOS
  support (a first-class platform), M12 and M13. Details and dates are in
  [`PLAN.md`](PLAN.md); each milestone's notes list what was pulled forward,
  invented or carried.
- **Parity:** core 110/110, charts 6/6, lab 47/47 (`cargo run -q -p rustryx-cli -- parity`,
  which counts each package).
- **Last commits on `main`:** M13's stages: `rustryx-charts` (the d3 subset, `Chart` and its
  chrome, every mark including the WebGL ones) and `rustryx-lab` (all 47 lab components,
  with the Core parity they exposed and a native media engine in
  `rustryx_primitives::media`). CI runs on
  ubuntu, windows, macos and wasm for each push; check it with `gh run list`.
- **GPUI:** the owner's public fork, https://github.com/lukasgeciauskas/zed,
  branch `astryx`, cloned at `../zed-astryx`. The workspace pins rev
  `769561f35b2eefa2fe86cd970fad7b4c1a9ea36f` (patches R1, R3 and R5 to R24, with R3's Metal path and
  R16's macOS picker; see
  [`gpui-fork.md`](gpui-fork.md)). The `[patch.crates-io]` block in
  `Cargo.toml` (async-task, calloop) is legitimate and stays.
- **Repository:** https://github.com/lukasgeciauskas/rustryx, public (made public for
  unlimited Actions minutes).
  Upstream Astryx (the specification) is at `../Astryx`.

### Carried into later milestones (recorded in each doc and in KNOWN_ISSUES)

| What | Waits for |
|---|---|
| Backdrop blur (Lightbox, scrims, ChatLayout's frosted dock) | Fork patch R2 |
| A media engine on Linux (MediaPlayer and Lightbox video report "can't be played" there) | The owner's approval of a GStreamer dependency |
| Theme adaptation rules conditioned on contrast or reduced motion (none ships) | M15's reading of the system's settings |
| Native date and time pickers on Linux (`Native` shows the Astryx surface there; GPUI has no toolkit on Linux) | Fork patch R16 built and checked on Linux (a GTK picker) |

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
- **Publish the gallery after each milestone.** Once a milestone is closed and its CI is
  green, run `cargo xtask deploy --scope lamit`: it builds the browser gallery in release
  and publishes it to https://rustryx-gallery.vercel.app (the `rustryx-gallery` project of
  the "lamit" Vercel team) through the logged-in Vercel CLI.
- **Commits.** Each stage: run the gate, then commit and push to `main` with the
  global git identity. Messages end with the attribution lines the session
  supplies (`Co-Authored-By: ...`). Never commit a
  `gpui-local` `[patch]` block.

## Workflow per stage

1. `cargo run -q -p rustryx-cli -- component <Name> --dense` for the props, then
   read `../Astryx/packages/core/src/<Name>/` and
   `../Astryx/docs/specs/components/<Name>/<Name>.spec.md`. Number the tests after
   the FR items.
2. Implement in `crates/rustryx-components/src/<module>/`, with every prop mapped
   in a `ComponentDoc` (`doc.rs` registry) and a gallery example in
   `examples.rs`.
3. Tests: `#[gpui::test]` with a host view; see the patterns in
   `overlay/menu_tests.rs`, `overlay/lightbox_tests.rs` and
   `overlay/palette_tests.rs`.
4. Look at it: temporarily default an example open (mark the line
   `// TEMP-SCREENSHOT`), `cargo build -p rustryx-gallery`, then
   `$env:GALLERY_PAGE='overlays'; $env:GALLERY_SCROLL='400'; powershell -File
   tools/capture-window.ps1 -Exe target/debug/rustryx-gallery.exe -Out <png>
   -WaitSeconds 20`, read the PNG, and revert (grep `TEMP-SCREENSHOT` must find
   nothing).
5. Update `PLAN.md` (the milestone's notes), `KNOWN_ISSUES.md` and this file.
6. The gate, in this order, with fmt's check last:
   - `cargo fmt --all`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `cargo xtask codegen --check`
   - `cargo build -p rustryx-gallery --target wasm32-unknown-unknown`
   - `cargo fmt --all -- --check`
7. Commit, push, and watch CI: `gh run list --limit 3`, `gh run watch <id>`.

## Lessons from M4 and M5

GPUI facts are in [`guide/gpui.md`](guide/gpui.md) and this layer's patterns in
[`guide/conventions.md`](guide/conventions.md) (both also print with
`rustryx docs gpui` / `rustryx docs conventions`). The ones that cost the most
time:

- **Keyed state is scoped to where `render` runs.** A component that makes
  keyed state for another component it renders does not see that component's
  state. Hand the entity over instead (`Popover::controller`).
- **`on_next_frame` never runs in tests.** To act after the frame that first
  draws something, paint a `canvas` probe whose paint calls `window.defer`.
- **Escape reaches layers through `observe_keystrokes`,** after dispatch, so an
  action handler cannot stop it. A surface that wants Escape (a drilled menu) blocks
  its layer's Escape (`EscapeBehavior::Block`) while it handles the key. A key-down
  handler that calls `stop_propagation` does stop it: GPUI runs no keystroke observer
  for that key (the calendar dropping a range in progress inside a popover).
- **Clippy:** `type_complexity` fires on `Rc<dyn Fn(...)>` fields and
  `too_many_arguments` past seven; add type aliases and small structs as you
  write, not after.
- **Shell:** the bash tool breaks on inline Python heredocs that contain quotes.
  Write edit scripts to the scratchpad with the Write tool and run them with
  `python <file> .`. Re-read code after `cargo fmt` before matching text in it.
- **Tests:** an element needs `.debug_selector(..)` for `cx.debug_bounds`; menu
  rows carry `<list id>-<row key>`, palette rows `command-palette-item-<id>`.
  Read announcements with `rustryx_primitives::announce::announcement(..)`.
  Background `spawn` needs `Send`; items holding an `Rc` use `cx.spawn`.

## Lessons from M6

- **Element paths hold state.** `use_keyed_state` keys by the element id path, so
  wrapping a subtree in an element with an id (an `AnchoredLayer`) only some frames
  gives its controls new focus handles and drops focus. Wrap always, and add the
  layer's surface only when it shows (`keyboard_hint::KeyboardHint::wrap`). Keep
  state that must outlive a move between parents (a dragged row's handle) in a
  session keyed by data, not by path.
- **Geometry comes from probes.** GPUI tells no element's bounds by focus or id:
  read them in prepaint with a `canvas` (or `on_children_prepainted` on a `Div`)
  and use them next frame. `window.content_mask()` in prepaint is the clip of every
  scroller above, which is how `WindowedList` windows in any scroller.
- **Focus after a frame's own moves:** a deferral that defers again
  (`request_focus`) runs after a closing layer's focus return; `on_next_frame`
  never runs under the test platform.
- **GPUI clicks on Enter's release:** tests send the key-up themselves
  (`simulate_event(KeyUpEvent { .. })`) after `simulate_keystrokes`.
- **The live region coalesces:** an announcement already waiting is not repeated;
  advance the clock past its clear delay before asserting the same text again.
- **Temporary injections for screenshots:** a drag or reorder mode can be shown by
  starting it from the example after a few frames (and reverting the code); a real
  app's window receives the OS's pointer events, so hold the injected state against
  them.

## Lessons from M7

- **A `RenderOnce` renders at layout, after its parent's render returned.** A cell's
  control can read what the table learned from rows built after it (the tree's
  `aria-controls`, a pinned cell's row fill): share an `Rc<Cell<..>>` and read it in
  `render`.
- **Grid auto-placement wraps out-of-order cells.** Cells moved later in a row (pinned
  cells, which must paint last) need explicit row and column lines, or they land on a
  new row.
- **Paint above later siblings with `deferred`.** It keeps the scroll region's clip and
  hit-tests last: the resize boundary runs through the body rows that way.
- **Time on wasm is `web_time::Instant`.** The executor's `now()` returns it; mixing in
  `std::time::Instant` builds natively and fails the wasm build.
- **Upstream gives each table cell its own context menu** (TableCell FR4), not one menu
  per row, whatever an older code comment says; read the spec when the two disagree.

## Lessons from M8

- **A BottomSheet slides in by a transform.** Debug bounds give its laid-out place, but its
  hit areas sit below the window until the entrance ends: tests advance the clock (1 s)
  after opening a sheet before they click or scroll inside it.
- **Touch arrives three ways.** A tap synthesizes mouse presses; a pan becomes
  `ScrollEvent`s with `ScrollSource::Touch`, followed by GPUI's own momentum; an element
  that wants the raw finger claims a `TouchDragEvent` by calling `prevent_default` at
  `Started` (the wheel does, from a canvas probe's `window.on_mouse_event`). Tests drive
  touches with `simulate_event(TouchEvent { .. })`.
- **Read the pointer with `coarse_primary_pointer`,** not `last_input_was_touch`: a key
  press is no pointer.
- **GPUI turns Enter and Space on a focused clickable element into a click**
  (`ClickEvent::Keyboard`). An element whose keys mean something else checks
  `event.is_keyboard()` in `on_click`.
- **Focus listeners run inside a draw,** where `window.refresh()` is lost: defer the work
  (`window.defer`). A window that loses activation also fires focus-out, since the focus
  event reads the window's activity (the native picker's popup does this on Windows).
- **`gpui-local` must patch every fork crate the workspace names.** `gpui_web` was
  missing, so local wasm builds linked two copies of it; the xtask now patches it too.
- **Browser captures without Claude in Chrome:** run `cargo xtask web`, then headless
  Chrome: `chrome --headless=new --use-angle=swiftshader --enable-unsafe-swiftshader
  --virtual-time-budget=30000 --window-size=1300,900 --screenshot=<png>
  "http://127.0.0.1:8080/?page=dates&backend=webgl"`, with a scratch
  `--user-data-dir`. The query picks the page, theme, mode and scroll.

## Lessons from macOS support

- **Check macOS from Windows first.** `rustup target add aarch64-apple-darwin` is installed;
  `cargo check --target aarch64-apple-darwin` type-checks the workspace and the fork's
  Apple crates once `gpui_apple`'s `OUT_DIR` has a placeholder `shaders.metallib` (its
  build script compiles Metal only on a Mac). The fork's examples need a C compiler for
  the Mac target, so check them natively instead. Metal shader errors show only on the Mac
  runner: read the job log (`gh run view --job=<id> --log`).
- **The macOS job is the Mac.** `tools/ci-macos.sh gallery|fork` runs each window,
  captures it alone with `screencapture -l` and uploads `macos-screenshots`; read them
  with `gh run download <id> -n macos-screenshots`. The runner is an M1 VM with a working
  Metal device, Xcode 26 with the Metal toolchain, and a 1024 by 768 screen, which clamps
  the gallery's window.
- **`gpui_platform` needs `font-kit` on macOS,** or GPUI uses its no-op text system and
  draws no text at all.
- **Platform rules read `cx.platform_info()`,** not `cfg!(target_os)`: the web build runs
  on every platform, and tests can report a Mac (`cx.set_platform_info`) to check the
  macOS bindings on Windows. Tests type `secondary-` for Cmd/Ctrl shortcuts.
- **AppKit from a test thread:** `NSSpellChecker` and the haptics run only on the main
  thread (`MainThreadMarker::new()`), so unit tests on macOS see neither.
- **Showing an AppKit popover from GPUI's update re-enters it** (the popover takes key
  status from the window): open it from the window's foreground executor, as `prompt`
  does.

## Lessons from M12

- **Port against upstream's own output.** The theme export writes each shipped theme's
  `defineTheme` input (`spec/theme-definitions`) and each palette's generator request
  (`spec/palettes`); `define_theme` and the generator are checked against the resolved
  themes and palettes exactly, and helpers compared each module with upstream's built
  `dist` under Node on thousands of inputs.
- **JavaScript's number rules live in `authoring::js`** (`Math.round`, `toFixed`,
  `String(n)`): Rust's formatting rounds ties and picks digits differently, and a token
  value must be upstream's string.
- **Key order matters for rules.** A later component rule wins, so definitions are
  exported in upstream's key order and `rustryx-theme`'s JSON reader keeps it
  (`serde_json/preserve_order`).

## Lessons from M13

- **Two helpers in parallel, one verified snapshot per commit.** While helpers edit the
  working tree, a stage is committed from a chosen set of files (a crate root's module
  list written as a blob with `git update-index --cacheinfo` when unfinished modules
  must stay out), then checked in a git worktree (`../astryx-verify`) with a shared
  `CARGO_TARGET_DIR` before the push.
- **Check computations against upstream's own code under Node.** Helpers bundled
  upstream's TypeScript with the esbuild in `../Astryx` and compared hundreds to
  hundreds of thousands of cases bit for bit (d3, layouts, projections, formatters).
  JavaScript details matter: V8's fdlibm `Math.sin`/`cos` (`crates/rustryx-lab/src/trig.rs`),
  V8's sort with NaN comparisons, `-0`, `Math.min`/`max` with NaN.
- **Large data is generated, never copied:** the emoji set and the SVG icons come from
  upstream through `tools/export` and `cargo xtask codegen` like the tokens.
- **ICU4X differs from browsers' ICU** in places (CLDR versions, no interval patterns,
  data defects such as Portuguese month-year); KNOWN_ISSUES lists what remains.

## Next: M14 Rich text

M13 (Charts & lab) is done: `rustryx-charts` ports `@astryxdesign/charts` and
`rustryx-lab` all 47 lab components, with a native media engine (Media Foundation,
AVFoundation, the browser's media elements) for MediaPlayer and Lightbox video. M14 builds
the rich-text package (`../Astryx/packages/richtext`: RichTextEditor, its toolbar and
auto-link plugin, RichTextView) on the editing engine: a document model, the editor, the
toolbar, and a lossless Markdown round trip (`AST-062`, `AST-064`, `AST-106`). The CLI
already lists the richtext entries (`rustryx component RichTextEditor --package richtext`);
add a `richtext` package count to `rustryx parity` with the crate.
