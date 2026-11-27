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

- **Scrollbars overlay the content (fork patch R17).** Where the platform keeps scrollbars
  (Windows with "Always show scrollbars", for example) a browser gives a scroll container a
  gutter; here the bar always draws over the content's end edge, so a layout never
  changes when content starts to overflow. The bar stays on the right in right-to-left
  text, since GPUI's elements have no direction.
- **A wheel gesture is not latched (fork patch R17).** Browsers keep one gesture on the
  container it started in; here each event chains on its own, so a trackpad fling that
  reaches an inner box's edge goes on to scroll the page.
- **A scroll container measures its direct children.** GPUI sizes the scrollable content
  from the children's boxes, not from their own overflow, so a child that should scroll
  sideways keeps its content width (`flex_none`, or a fixed width).

## Components

- **A reorder drag: Escape cancels it.** A mouse drag is cancelled by Escape, which
  upstream's mouse drag has no key for.

- **Table: what GPUI's layout and AccessKit change.** An unknown row total (ARIA's `aria-rowcount=-1`, while more rows may load) has no
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
- **Spell checking depends on the platform's dictionaries.** Windows and macOS use the
  system checker (`ISpellChecker`, `NSSpellChecker`, the one Safari and Chrome use on a
  Mac). Linux reads Hunspell dictionaries from `/usr/share/hunspell` and similar folders
  (packages such as `hunspell-en-us`); with none installed, nothing is marked. Browsers do
  not expose their checker, so on the web an app installs one with
  `astryx_primitives::spell::set_spell_checker(DictionaryChecker::new(aff, dic))`.
  `spellbook`, which reads the dictionaries, is MPL-2.0.

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
- **Lightbox has no backdrop blur, and plays no video.** GPUI has no backdrop blur (fork
  patch R2; see `astryx docs effects`), and video items open in the system player until
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
- **Dates: ICU4X stands in for `Intl` and `Temporal` (M8).** Every calendar always works
  (ICU4X compiles its data in), so upstream's Gregorian fallback for a runtime without
  `Temporal` (AST-111 FR5) and the lunisolar server/browser caveat (FR19) never apply.
  ICU4X's CLDR data differs from V8's in places, and the date code follows ICU4X's own
  output so a field always reads back what it writes (FR13):
  - Amharic writes Ethiopic dates month first (`2/23/2019 AM`), where V8 writes them day
    first; typed Amharic Ethiopic dates read month first.
  - ICU4X writes some locales' lunisolar days in Han numerals and Hebrew dates in Hebrew
    letters; the shared formatter writes them in the locale's digits, as `Intl` does.
    Numeric Hebrew months are their position in the year (FR9), not ICU4X's `06a`/`06b`.
  - Some locales' numeric formats order fields differently from format to format
    (Finnish Chinese dates are `1905 5 17` and `17.5.1905`; Arabic Coptic's weekday format
    is month first); the parser learns each numeric format's separators and order.
  - Calendar display names and the week-of-year field name come from Node's `Intl` at
    export time (`spec/cldr.json`); a locale outside its list uses English.
- **The Gregorian fallback parser approximates `new Date(text)` (M8).** `parse_date_input`'s
  last step reads the date forms V8's lenient parser accepts (English month and weekday
  names anywhere, M/D/Y and Y/M/D numbers, a time of day); stranger text V8 reads is not
  read.
- **TimeInput commits a typed prefix (M8).** As upstream (its FR3 implementation gap),
  typing "2.30pm" in English commits 02:00 as soon as "2" parses, and keeps it at blur.
- **No server render, so today is always known (M8).** Calendar FR12's hydration rules
  (an empty month until today is known) and DateRangeInput's year-while-today-is-unknown
  trigger text have nothing to apply to.
- **Haptics play where the platform has them (M8, M11).** Upstream vibrates only through
  `navigator.vibrate` (Android browsers), as the web build does. A native macOS build
  also plays them on a Force Touch trackpad (`NSHapticFeedbackManager`): the wheels'
  selection ticks as level changes and a sheet settling onto a detent as an alignment,
  felt while a finger is on the trackpad. Windows and Linux have no system haptics; an app
  installs a player there with `set_haptic_player`.
- **A trackpad without lift phases lifts on a pause (M8).** Under a finger the touch wheel
  follows a trackpad's two-finger scroll (DateInput FR15). Where the platform reports the
  lift (macOS), the wheel coasts from it; elsewhere a 50 ms pause counts as the lift, so
  momentum the platform keeps sending before that pause moves the wheel too.
- **The scrolling column's tip is projected flat (M8).** Upstream tips each row with
  `rotateX` under a perspective; here a row scales by the same angle's cosine vertically and
  by 0.86 at the edges, without the perspective's trapezoid.
- **The touch calendar's pager settles after its momentum (M8).** Upstream's month pager is
  a native scroll-snapping scroller, whose momentum picks a month as the finger lifts. Here a
  swipe coasts on GPUI's touch momentum (or the platform's, on a trackpad) and then glides
  onto the nearest month.
- **Only the shown month's day is a tab stop (M8).** Upstream's pager keeps one tab stop in
  every mounted month pane, so Tab can reach months scrolled out of view. Here only the shown
  pane has one; the arrow keys still cross months.
- **TimeInput's sheet opens from the keyboard (M8).** Upstream's FR7 notes an
  implementation gap: Enter, Space and ArrowDown do not open its sheet. Here they do, as
  DateInput's sheet field does, so the wheels are reachable by keyboard as FR7 requires.
- **DateRangeInput's applied preset in dark mode is 3.88:1 (M8).** As upstream (its spec's
  implementation gap): the label in `text-accent` on the `accent-muted` fill, on the
  popover surface, is under 4.5:1. The tokens are upstream's.
- **Native pickers exist on Windows, macOS and the web (M8).** Fork patch R16 opens the
  Common Controls month calendar or date-time picker on Windows, an `NSDatePicker` in a
  popover on macOS (the graphical calendar for a date, the text field with a stepper for a
  time or a date and time), and the browser's `<input type=date|time|datetime-local>`
  picker on the web. Linux reports `native_picker_supported` false (GPUI has no toolkit
  there), and `Native` (or `AdaptiveNative` under a finger) resolves to the pointer's
  Astryx surface: the sheet under a finger, otherwise the popover (DateInput,
  DateTimeInput) or the typed field (TimeInput). The macOS picker has no clear, as the
  Windows one has none.
- **The native field paints its own text (M8).** Upstream lays its text over the browser's
  control and lets a press reach it. Here the value is Astryx text in the field's box, and
  a press, Space, ArrowDown or F4 opens the platform's picker below the box (Enter
  commits, as in upstream's control). The Windows popup and the macOS popover are drawn
  by the system, in the system's style rather than the theme's.
- **The browser's picker popup is not in captures (M8).** It is drawn outside the page,
  so a headless capture shows the native field but not the popup. The Windows popups
  were captured from the gallery and the fork's `astryx_native_picker` example, the
  macOS popovers from that example in the macOS CI job.

- **Sticky containment has no effect (M9).** Fork patch R18 brings `position: sticky`, but
  a ScrollableArea's viewport always clips, so it is always its descendants' sticky
  boundary: `sticky_containment: when_scrollable` (a fitting viewport lets sticky
  descendants reach an outer scroller) behaves like `always`. A viewport that stopped
  clipping while it fits would, in GPUI's layout, grow to its content and never scroll
  again, so this needs a layout-level clip toggle in the fork. Table pins its sticky
  columns itself.
- **A named scroll viewport keeps its own tab stop (M9).** Upstream's
  `contentOrViewport` hands forward Tab entry to a lone leading native button or link
  inside; GPUI's focus handles carry no element kind, so the viewport stays a stop.
- **No safe-area insets, ids or `aria-controls` in AppShell (M9).** GPUI reports no
  display cutouts, so AppShell's `env(safe-area-inset-*)` padding is zero; and GPUI
  elements have no DOM ids, so the main region's id, the drawer id and the toggle's
  `aria-controls` have no form (the skip link moves focus to the main region directly).
- **CodeBlock's copy cannot fail, and lines keep the viewport's direction (M10).** GPUI's
  clipboard write reports no failure, so the failed-copy state (FR2) and `on_copy_error`
  never arise. Each line renders left to right; upstream gives every line `dir="auto"`, so
  a line that starts with a right-to-left letter reads right to left (FR16). The highlight
  modes look alike: GPUI paints every token as a span of styled text.
- **Markdown's inline code and images are atomic boxes (M10).** A paragraph lays text out
  around them, but a code span never breaks across lines (upstream breaks a long one
  outside table cells), and an image keeps its intrinsic size up to the column's width.
  Remote images need the app's HTTP client, as any GPUI image does.
- **Markdown's `content_width` takes a `Length` (M10).** Upstream accepts any CSS width
  (`60ch`, `min(...)`); here pixels, rems or a fraction, and `Length::Auto` or zero drops the
  cap as upstream's `0` does.
- **Markdown plugin renderers have no error boundary (M10).** A render closure that
  returns `Err` shows the node's readable text and reports `render-failed`, as upstream's
  boundary does; a panic is not caught, and there is no Suspense fallback (GPUI renders
  synchronously). Source lines reach supplied renderers only: GPUI elements have no
  `data-source-line` attributes.
- **Markdown's plugin protocol where upstream's code and spec part (M10).** FR17's content
  categories are not in upstream's code (extension nodes have no children), so not here
  either; a transform may not remove another plugin's nodes, as upstream's code and tests
  require though FR18 allows it. The diagnostic codes and `depends_on` exist only in the
  spec, so their names are the port's; diagnostics are deduplicated per parse rather than
  per process. The Remark adapter runs a Rust function over the mdast subset as
  `serde_json` values. Offsets, spans and decoration ranges are UTF-8 bytes.
- **Chat's log announces nothing itself (M10).** GPUI exposes no live-region property or
  `aria-labelledby`, so ChatMessageList is a `log` role that a screen reader reads as the
  platform reads logs, and a ChatMessage is named by its sender fallback, not its `name`.
  The frosted dock has no blur (fork patch R2), and a bubble's name row lowers the avatar
  a frame after it first renders.
- **The composer's edges (M10).** Lines holding tokens wrap with the port's own greedy
  wrapper, which assumes left-to-right glyph order, so right-to-left text beside a token may
  wrap wrongly. A caller cancels a built-in key with `window.prevent_default()` and must
  also `cx.stop_propagation()` to keep the key from the field. Only clipboard file paths
  reach `on_files` (a pasted bitmap has no file). The trigger menu is placed where the
  trigger was a frame earlier. ChatTokenizedText inside a Text paragraph is one inline box.
  The drawer's custom summary still takes clicks, its focus ring draws inside the toggle
  row, and hover colors apply on every pointer. The `chat-composer` theme target sits on the
  painted body (the spec's FR7 fix; upstream as shipped puts it on the frame, OQ1).
- **Dictation needs a recognizer on native platforms (M10).** The web build uses the
  browser's speech recognition and Web Audio, as upstream does; GPUI offers neither
  natively, so a native app installs a `SpeechRecognitionProvider` (and plays the
  start and stop sounds through its `play_samples`). Without one, the dictation button
  hides.
- **Grammar word boundaries are Unicode (M10).** CodeBlock's grammars use JS's ASCII `\b`;
  `fancy-regex` has no ASCII boundary, so a keyword directly beside a non-ASCII letter
  (`éfor`) can paint differently. `\w` and `\d` keep JS's ASCII meaning.
- **Carousel snapping settles after the gesture (M9).** GPUI has no CSS scroll snap or
  platform momentum to rest on a stop, so the row scrolls freely and glides onto the
  nearest stop when a gesture ends (a trackpad's release, or 150 ms after the last wheel
  event); a release projects its velocity a quarter second ahead to pick the stop or the
  page. Upstream's evidence gap applies too: the one-page-per-flick feel is not measured
  against UIKit.
- **Outline navigation has no history or focus move (M9).** A native window has no URL,
  so choosing a link pushes no `#id`; GPUI has no sequential focus starting point, so the
  next Tab does not continue from the heading (WCAG 2.4.3 in the browser); and the
  heading scrolls within its scroll root only, where upstream's `offset: 0` path also
  scrolls every scrolling ancestor.
- **MobileNav's `side: auto` reads the pointer, not the focused element (M9).** Upstream
  resolves the side from the focused opener's half of the viewport. GPUI cannot report an
  element's bounds from its focus handle, so a drawer opened by a pointer uses the
  pointer's half, one opened from the keyboard uses AppShell's toggle, and otherwise it
  keeps its last side, as upstream does when nothing is focused.
- **Kept layers draw text without subpixel antialiasing.** A snapshot (fork patch R3) keeps
  an element's look as a texture, so its text is grayscale-antialiased. A List given an
  animated layout keeps each keyed row's last look for its exit copy, so its rows draw
  that way. Outside AppShell, or in an `auto`-height AppShell, the SideNav rail's own
  width follows its edge and its neighbours reflow.
- **App-owned docked bars don't ride the on-screen keyboard (AST-099 FR9).** GPUI has no
  `position: fixed`, so an app's own bottom bar cannot be told apart from other layout;
  Astryx's own docks (ChatLayout, BottomSheet) ride it.
- **Animated layout keys elements without ids by where they were built (fork patch R19).**
  Upstream's glide follows DOM nodes. GPUI elements do not outlive a frame, so a `div`
  without an id is known from frame to frame by the source place that built it and how
  many siblings that place built before it. Content inserted among siblings built by the
  same place (rows from one loop) therefore jumps unless the rows have element ids, as
  Astryx's own List and TreeList rows do. Elements inside a cached view are not followed.
- **The edge reveal measures the content it reveals.** Upstream's block-end reveal slides
  the content under a clip with transforms alone; here the clip's height follows the
  shared motion (the content keeps its own height and slides under it), so the content
  after it follows by layout, frame by frame, rather than by a transform.
- **Gestures: what GPUI's input reports.** GPUI has no pen pointer type, so a pen
  drives gestures as a mouse does (the recognizers' pen rules are tested but never
  reached). GPUI delivers input in order, so upstream's windows for late presses are not
  needed and not ported. It has no `scrollend`, so a finger's scroll counts as ended after 150 ms
  without movement. A finger that no gesture takes scrolls the window on UIKit's
  deceleration, since GPUI's own scroll physics is private. Scrollers inside a cached
  view are not followed.
- **Remembered sizes live in a file natively (M9).** `autoSaveId` writes to
  `localStorage` in a browser, as upstream does; a native app keeps the entries in
  `<data dir>/astryx/<executable>.json` (`astryx_primitives::storage`, which an app can
  replace with `set_storage`).

- **A one-panel Layout with a content width keeps its whole middle to that width (M9).**
  Upstream keeps the panel aligned to the content width while LayoutContent extends
  through the open side, so its scrollbar sits at the window's edge; here the middle row
  (panel and content) is centered at the content width. Without panels the content keeps
  its own box and aligns its children through its padding, as upstream does.

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
