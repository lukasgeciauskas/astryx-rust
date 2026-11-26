# Handoff: where the port stands, and how to pick it up

Read this at the start of a new session, after `CLAUDE.md`. It records the state
of the work, the owner's standing instructions, the workflow that every
milestone has followed, and what comes next. Update it at the end of each
milestone.

_Last updated: 2026-11-26, during M9 (Shell & navigation): stage 1 (fork patch R17) done._

## State

- **Milestones done:** M0, M1, M1.5, M2, M3, M4, M5, M6, M7 and M8; M9 is under way (stage 1:
  scroll chaining and scrollbars, fork patch R17). Details and dates are in
  [`PLAN.md`](PLAN.md); each milestone's notes list what was pulled forward,
  invented or carried.
- **Parity:** 88 of 110 core components (`cargo run -q -p astryx-cli -- parity`;
  `component --list` marks ported ones with `*`).
- **Last commits on `main`:** M8's stages, from `90581cb` (calendar systems and
  Calendar) through the pointer fields, DateRangeInput, the wheels and the three bottom
  sheets and the carried items, to the native pickers. CI runs on
  ubuntu, windows and wasm for each push; check it with `gh run list`.
- **GPUI:** the owner's public fork, https://github.com/lukasgeciauskas/zed,
  branch `astryx`, cloned at `../zed-astryx`. The workspace pins rev
  `1aea36f6c5d49b4e82dba459a7e6dd7c5150e6c1` (patches R1 and R5 to R17; see
  [`gpui-fork.md`](gpui-fork.md)). The `[patch.crates-io]` block in
  `Cargo.toml` (async-task, calloop) is legitimate and stays.
- **Repository:** https://github.com/lukasgeciauskas/astryx-rust, private.
  Upstream Astryx (the specification) is at `../Astryx`.

### Not yet ported (26)

- **M9:** AppShell, Layout, Resizable, ScrollableArea, Carousel,
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
| Item swipe actions, SegmentedControl's sliding lens, Collapsible's and List's animated layout, reordering's shift slide (AST-103 FR9) | Motion layer (M11) |
| Lightbox springs (it eases over 280ms now) | Motion layer (M11) |
| Lightbox video playback and `hasAutoPlay` (a video opens in the system player now) | MediaPlayer's decoder (M13) |
| Dialog content inset done by the dialog itself | Layout (M9) |
| Backdrop blur (Lightbox, scrims) | Fork patch R2 |
| Table row expansion's `animatedLayout` glide (FR15) | Motion layer (M11) |
| TreeList's `animatedLayout` and expand glide (FR8, FR9) | Motion layer (M11) |
| Native date and time pickers on macOS and Linux (`Native` shows the Astryx surface there) | Fork patch R16 built and checked on those platforms |

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
  Read announcements with `astryx_primitives::announce::announcement(..)`.
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

## Next: M9 Shell & navigation

M8 (Dates) is done: calendar systems in `astryx_i18n::date` (ICU4X), Calendar,
DateInput, TimeInput, DateTimeInput and DateRangeInput, each with its pointer field, its
bottom sheet (`wheel/`, the month pager, the month and year wheels) and its native
surface (fork patch R16: the Common Controls pickers on Windows, the browser's pickers on
the web; `date_input/native.rs`). The motion math was pulled forward from M11 into
`astryx_primitives::motion` and `gestures`. The items carried into M8 are done:
PowerSearch's date and time values with `timezoneID`, Table filtering's date and time
controls, and FormLayout's `defaultEntryErrorBehavior` readers.

M9 ports AppShell, Layout, SideNav, TopNav, NavMenu and NavHeadingMenu, NavItem, NavIcon,
MobileNav, Outline, Resizable, ScrollableArea (AST-025), Carousel and SnapPaging (see
`PLAN.md`). It carries Dialog's content inset, which Layout provides. Start with
`cargo run -q -p astryx-cli -- component <Name> --dense` for each and the upstream specs
in `../Astryx/docs/specs/components/`.
