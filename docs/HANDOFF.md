# Handoff: where the port stands, and how to pick it up

Read this at the start of a new session, after `CLAUDE.md`. It records the state
of the work, the owner's standing instructions, the workflow that every
milestone has followed, and what comes next. Update it at the end of each
milestone.

_Last updated: 2026-11-25, during M8 (Dates): stages 1 and 2 done._

## State

- **Milestones done:** M0, M1, M1.5, M2, M3, M4, M5, M6 and M7; M8 (Dates) is under way
  (stage 1, calendar systems and Calendar, and stage 2, DateInput's pointer field, are done). Details and dates are in
  [`PLAN.md`](PLAN.md); each milestone's notes list what was pulled forward,
  invented or carried.
- **Parity:** 85 of 110 core components (`cargo run -q -p astryx-cli -- parity`;
  `component --list` marks ported ones with `*`).
- **Last commits on `main`:** M7's seven stages, from `cceff1a` (Table and its parts)
  through sorting, selection and activation, pagination and filtering, tree and grouped
  rows, column settings and resize, to virtualization. CI runs on
  ubuntu, windows and wasm for each push; check it with `gh run list`.
- **GPUI:** the owner's public fork, https://github.com/lukasgeciauskas/zed,
  branch `astryx`, cloned at `../zed-astryx`. The workspace pins rev
  `52f785921072e000e31d957c7008cde0a01f7afd` (patches R1 and R5 to R15; see
  [`gpui-fork.md`](gpui-fork.md)). The `[patch.crates-io]` block in
  `Cargo.toml` (async-task, calloop) is legitimate and stays.
- **Repository:** https://github.com/lukasgeciauskas/astryx-rust, private.
  Upstream Astryx (the specification) is at `../Astryx`.

### Not yet ported (27)

- **M8:** DateRangeInput, DateTimeInput, TimeInput (DateInput is ported; its sheet and
  native surfaces come later in M8). **M9:** AppShell, Layout, Resizable, ScrollableArea, Carousel,
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
| FormLayout's `defaultEntryErrorBehavior` readers | Date inputs (M8) |
| Backdrop blur (Lightbox, scrims) | Fork patch R2 |
| PowerSearch's date and time values (`time`, `date_absolute`, `date_relative`, `date_range`, `timezoneID`, FR25, FR26) | Calendar, date inputs and calendar systems (M8) |
| Table filtering's `date_absolute` and `time` controls | DateInput and TimeInput (M8) |
| Table row expansion's `animatedLayout` glide (FR15) | Motion layer (M11) |
| TreeList's `animatedLayout` and expand glide (FR8, FR9) | Motion layer (M11) |

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

## Next: M8 Dates

M7 (Table) is done: `crates/astryx-components/src/table/` (Table, its parts, all 14 plugin
families, selection's toolbar and activation), fork patches R14 and R15, tests in
`table/tests.rs`, `row_tests.rs`, `data_tests.rs`, `structure_tests.rs`,
`column_tests.rs` and `window_tests.rs`. What it carried: Table filtering's date and
time controls (M8 below) and row expansion's glide (M11).

M8 builds the date and time inputs on calendar systems (jiff + icu4x, AST-111): Calendar,
DateInput (segmented), DateRangeInput, DateTimeInput and TimeInput. Stage 1 is done: the
date and time foundations in `astryx_i18n::date` (calendar arithmetic, formatting as parts,
the typed date and time parsers) and Calendar with its three public hooks
(`crates/astryx-components/src/calendar/`). The owner approved the calendar crates; ICU4X
and jiff were already dependencies. Stage 2 added DateInput's pointer field (`crates/astryx-components/src/date_input/`) and the
shared entry-error report (`field/entry_report.rs`). The remaining stages, in order:
TimeInput (typed field), DateTimeInput, DateRangeInput, the touch sheets (MonthScroller,
the month/year and time wheels), the native pickers (a fork patch: a hidden
`<input type="date">` and `showPicker()` on the web, the Common Controls month calendar and
date-time picker on Windows), then the carried items. Carried into M8:
- PowerSearch's date and time values (`time`, `date_absolute`, `date_relative`,
  `date_range`, `timezoneID`, FR25, FR26).
- FormLayout's `defaultEntryErrorBehavior` readers.
- Table filtering's `date_absolute` and `time` controls (`table/filtering.rs`'s
  `filter_control`).

Ask the owner before adding the jiff or icu4x calendar crates (installing software).