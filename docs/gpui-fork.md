# Our GPUI fork

Astryx needs a few CSS effects that GPUI's renderer cannot draw, so it uses
a fork of Zed with renderer patches:

- Repository: https://github.com/lukasgeciauskas/zed (public, so the patches
  can also go upstream as pull requests).
- Branch: `astryx`. It is Zed main plus our patches, one commit per feature.
- Base: Zed `2b248e6`, 2026-11-24.

Every app that uses Astryx pins the same fork revision as this workspace
(see the README). Effects that need no renderer change (multi-stop and
radial gradients, fades, slides, icon rotation, shapes, image blur) live in
Astryx itself; see `docs/guide/effects.md`.

## Working on a patch

1. **Edit and build locally.** Edit `../zed-astryx`, a clone of the `astryx` branch.
   `cargo xtask gpui-local on` builds this workspace against the clone (`gpui`,
   `gpui_platform` and `gpui_web`).
2. **Prove it:**
   - add an example to the fork (`crates/gpui/examples/astryx_<feature>.rs`);
   - add a gallery page here;
   - capture it natively on Windows (`tools/capture-window.ps1`) and in the browser
     (`cargo xtask web`);
   - Linux CI builds it.
3. **Pin it:**
   - commit and push the fork branch;
   - run `cargo xtask bump-gpui <new rev>` and `cargo xtask gpui-local off`;
   - run the full suite;
   - commit here.

To follow Zed: rebase the `astryx` branch onto a newer Zed main, re-run the
examples, push, then `bump-gpui`.

## Patch roadmap

Each patch lands before the components that need it.

| # | Patch | What it adds | Needed by |
|---|---|---|---|
| R1 | Element transforms (**landed**, `4d4a306`) | Translate, scale and mirror any subtree around an origin, including text. Mouse hit testing follows the transform. | Button press scale (M3); dialog and popover entrances (M5); Carousel, Swipe (M9, M11) |
| R2 | Backdrop blur | A blurred copy of what is already drawn behind an element, clipped to its rounded rect | Chat composer, Lightbox, SegmentedControl (M5, M10); lab TabBar, MediaPlayer |
| R3 | Offscreen layers | A subtree drawn to a texture, then composited with group opacity, filters (blur, brightness, grayscale) or an alpha mask | Dialog, Drawer and MobileNav backdrops; PowerSearch, Swipe, Table (M5+) |
| R4 | Blend modes | Multiply and screen in fixed-function blending; the rest through R3 layers | Not used by upstream today; lowest priority |
| R5 | Inline layout (**landed**, `7ce1c61`) | Inline-level boxes (`inline-flex`, `inline-block`, `inline-grid`) that flow in line boxes inside block containers; `min-content`, `max-content` and `fit-content` sizes; text baselines | Every `inline-flex` component (M2, M3); inline text flow (Link in Text, Markdown, Chat) |
| R6 | ARIA states (**landed**, `de7e405`) | `aria_disabled`, `aria_read_only`, `aria_required`, `aria_invalid`, `aria_busy`, `aria_modal` and `aria_multiselectable` on any stateful element, setting the matching AccessKit flags | Every form control (M4 onward): disabled, read-only, required and invalid fields; busy controls; modal dialogs and multi-select lists (M5) |
| R7 | Display contents and grid tracks (**landed**, `7633ed6`) | `display: contents` (an element whose children lay out as its parent's), explicit grid track lists (`auto`, `min-content`, `max-content`, lengths, `fr`, `minmax`, `fit-content`), `grid-auto-flow`, `grid-auto-columns` and `grid-auto-rows` | FormLayout's `horizontal-labels` grid (M4); any upstream layout built on `display: contents` or track lists |
| R8 | Browser files (**landed**, `cc84f94`) | On the web: `prompt_for_paths` opens the browser's file dialog, files dragged from the system drop as `ExternalPaths`, and `gpui_web::web_files` gives each browser file a virtual path with its name, size, type and bytes | FileInput on the web (M4); any app that opens or receives files in the browser |
| R9 | `aria-current` (**landed**, `69dc0ba`) | `aria_current(AriaCurrent)` on any stateful element, setting AccessKit's aria-current; `AriaCurrent` is re-exported next to `Role` | The current page of Breadcrumbs and Pagination and the current step of a Stepper (M4); nav items (M9) |
| R10 | The view inside a `ViewElement` (**landed**, `2e391af`) | `ViewElement::view_mut` and `take_view`: a parent reaches the component inside an `AnyElement` | The inline text flow (M4): a Text or Link inside a paragraph joins its lines |
| R11 | Input modes and spellcheck (**landed**, `d8be0c9`) | `TextInputConfiguration::input_mode` (the HTML `inputmode` set) and `spellcheck`; the web's IME mirror applies both | `inputMode`, `enterKeyHint`, `autoCapitalize`, `hasAutoCorrect` and `hasSpellCheck` on text fields (M4) |
| R12 | Focus traps and `aria-haspopup` (**landed**, `93e506e`) | `tab_trap()`: Tab and Shift-Tab from inside an element cycle among its tab stops; `Window::with_tab_scope`; `aria_has_popup(HasPopup)` | Every M5 layer: Popover's focus region, dialogs, drawers and menus; popup triggers |
| R13 | Active descendants across layers (**landed**, `01da1ad`) | `aria_active_descendant_of(&FocusHandle)`: an element is the active descendant of the element tracking that handle while it is focused, wherever it sits in the tree | Selector's listbox options (M6), and every combobox popup after it |
| R14 | `aria-sort` (**landed**, `d436e06`) | `aria_sort(SortDirection)` on any stateful element, setting AccessKit's sort direction; `SortDirection` is re-exported | A sortable Table column's header (M7) |
| R15 | Named nodes and `aria-controls` (**landed**, `52f7859`) | `aria_id(name)` names an element's node as an HTML `id` does; `aria_controls(names)` points at named nodes. Both are collected per frame and resolved when the tree is finished, so a relation may name a node that comes later; unknown names are skipped | A tree table's expanders and row expansion's toggles (M7) |
| R16 | Native pickers (**landed**, `b39b0e1`) | `Window::show_native_picker(NativePickerRequest)` opens the platform's date, time or date-and-time picker below an anchor and answers `Picked` (an ISO value), `Cleared` or `Dismissed` through a channel; `native_picker_supported(kind)` says whether one exists | The native surfaces of DateInput, TimeInput and DateTimeInput (M8) |

### How each patch works

- **R1 (transforms), as built:**
  - Upstream rotates only icons (SVGs, which already take any matrix). Elements need
    translate, scale and mirroring, which map rectangles to rectangles.
  - So the window maps each primitive's geometry on the CPU before it reaches the scene:
    quad bounds, radii and borders; shadow offsets and blur; underlines; image boxes;
    path curves.
  - Glyph and SVG sprites compose the transform into their existing matrix. A pure
    translation moves glyph origins instead, so text stays sharp.
  - No shader changes: DirectX, wgpu (Linux, web) and Metal draw it unchanged.
  - Styles: `scale`, `translate` and `transform_origin`, separate fields like CSS's
    individual transform properties, so states compose. `Styled::scale`, `scale_xy`,
    `translate`, `translate_x`, `translate_y`, `transform_origin`.
  - `Window::with_element_transform` keeps a stack in prepaint and paint. Content masks
    are stored in window coordinates; `content_mask()` maps back to element coordinates.
  - Hitboxes cover the transformed area; deferred draws are anchored where the transform
    put them; cached views key on the transform.
  - Known limits:
    - Images are not mirrored by a negative scale.
    - Mouse positions in event handlers stay in window coordinates, as in the browser.
    - `:hover`/`:active` transforms are not applied to the hitbox, which avoids hover
      flicker at the edges.
  - Tests: `element_transform` in the fork (unit, paint geometry, clipping, hit testing).
    Example: `cargo run -p gpui --example astryx_transforms`. Gallery: the Effects page.
- **R5 (inline layout), as built:**
  - Taffy (0.15) lays out boxes but leaves inline formatting contexts to its host, and
    GPUI only knew block, flex and grid, so an `inline-flex` root stretched across a
    block parent.
  - `Display::InlineBlock`, `InlineFlex` and `InlineGrid` lay their children out as
    block, flex and grid. When a block container's node is built, each run of
    consecutive in-flow inline-level children goes into an anonymous line box: a
    wrapping flex row aligned on baselines and justified by the container's `text_align`.
    Line boxes live in a scope derived from their container's, so Taffy's cache keeps
    them between frames like any node.
  - `Length::MinContent`, `MaxContent` and `FitContent` map to Taffy's sizing keywords
    (and to `auto` for margins and insets).
  - Text nodes report their first baseline through `Window::set_layout_baseline`, which
    reaches Taffy with the node's measurement; flex containers carry their first item's
    baseline up, so boxes of different sizes line their text up.
  - Still to come: text that flows around inline elements in the same lines (a Link or
    Code inside a wrapping paragraph). That is a GPUI element, not a Taffy change, and
    lands before Markdown.
  - Example: `cargo run -p gpui --example astryx_inline`.
- **R6 (ARIA states), as built:**
  - GPUI's elements could carry a role, label, value and the like, but no states, so a
    disabled, required or invalid control looked enabled and valid to a screen reader.
  - Seven setters on `StatefulInteractiveElement` set AccessKit flags in the element's
    `write_a11y_info`: disabled, read-only, required, invalid (`Invalid::True`), busy,
    modal and multiselectable. Unset states stay absent.
  - `aria_disabled` is only the flag: a focus-tracking element keeps its tab stop, matching
    upstream's `aria-disabled` controls.
- **R7 (display contents and grid tracks), as built:**
  - Taffy 0.15 has no `display: contents`. A contents node is acquired like any other
    but never attached: when its parent requests layout, each contents child is
    replaced by its children (already expanded, since children are requested first),
    and the contents node's bounds are the union of its children's.
  - GPUI's grids only took `repeat(n, minmax(_, 1fr))`. `grid_template_columns` and
    `grid_template_rows` now take a `GridTrack` list, and `grid_auto_flow`,
    `grid_auto_columns` and `grid_auto_rows` reach Taffy.
- **R8 (browser files), as built:**
  - Browsers expose files only as `File` objects. `gpui_web::web_files` keeps each one
    under a virtual path (`/web-files/<n>/<name>`) with `info` (name, size, MIME type)
    and an async `read`, so path-based APIs work unchanged.
  - `prompt_for_paths` opens the dialog through a hidden `<input type="file">`; browsers
    allow it only during a user gesture.
  - Dragging files over the canvas starts an `ExternalPaths` drag with no paths (the
    browser holds the files back), so `drag_over` styles apply; at the drop the files
    get virtual paths and replace the empty list before the drop lands.
- **R9 (`aria-current`), as built:** one more ARIA property next to R6's states, kept
  unset unless a component sets it. The R6 test covers it.
- **R10 (the view inside a `ViewElement`), as built:** `#[derive(IntoElement)]` wraps a
  component in a `ViewElement`, so `AnyElement::downcast_mut::<ViewElement<Text>>()`
  finds a Text among a paragraph's children and `take_view` hands it over. The element
  must not be laid out after.
- **R11 (input modes and spellcheck), as built:** two fields on
  `TextInputConfiguration`, which an input handler returns every frame. The web mirror
  remembers the mode, so the keyboard it brings up after an editable tap is the field's;
  native platforms ignore both (they have no touch keyboard, and Astryx checks spelling
  itself).
- **R12 (focus traps and `aria-haspopup`), as built:**
  - GPUI's tab groups only reorder stops, and a group is known only by its tab index, so
    a trap is recorded as a range of the tab-stop map's insertion history (the stops
    painted inside the element). `next` and `prev` from a stop inside the innermost
    trap holding it search only that range, wrapping; from outside, they ignore traps.
  - The trap markers are tab-stop operations, so a cached view replays them.
  - Focusing a trap's container (a focusable element that is not a tab stop) and then
    calling `focus_next` lands on its first stop: how a layer moves focus in.

- **R13 (active descendants across layers), as built:**
  - GPUI's `aria_active_descendant()` is honored only under the focused node, and a
    deferred layer (a popup) paints at the accessibility root, so a combobox's
    options could never be its active descendant.
  - `aria_active_descendant_of(&FocusHandle)` keeps the handle on the element; in
    prepaint, while that handle is focused, the element's node becomes the reported
    focus (`A11y::set_active_descendant_of_focused`), provided a focused node exists.
    Deferred layers prepaint after the main tree, so the focused trigger's node is
    already known.

- **R16 (native pickers), as built:**
  - `NativePickerRequest` carries the kind, the value, `min` and `max` as ISO text,
    whether seconds show, and the anchor in window coordinates. `PlatformWindow` gains
    `native_picker_supported` and `show_native_picker` (defaults: false and `None`).
  - Windows (`gpui_windows/src/native_picker.rs`): an owned `WS_POPUP` tool window
    below the anchor hosts `SysMonthCal32` for dates or `SysDateTimePick32` for times
    (`DTS_TIMEFORMAT`) and date-times (`DTS_UPDOWN`, the locale's short date and time
    formats). A month-calendar pick answers at once; Enter answers with the control's
    value; Escape or a deactivation without a change dismisses, and one after a change
    answers with it.
  - Web (`gpui_web/src/window.rs`): a transparent `<input>` of the kind (`step=1` with
    seconds) placed over the anchor, focused, then `showPicker()`; `change` answers,
    `cancel`, `blur` and Escape dismiss, and the IME mirror takes focus back.
  - The test platform is off by default; `set_native_pickers_supported`,
    `native_picker_request` and `simulate_native_picker` drive it.
  - Example: `crates/gpui/examples/astryx_native_picker.rs`.

- **R2 (backdrop blur):**
  - A new scene primitive.
  - When the renderer meets it, it ends the current pass, copies the region behind it from
    the target, blurs that copy with a dual-Kawase downsample and upsample, and draws it
    clipped to the element's corner radii.
  - Then it resumes with load-on-begin.
  - The surface texture needs copy-source usage on wgpu.
- **R3 (offscreen layers):** a paint-time layer stack renders a subtree to its own texture,
  then composites it through a filter shader.
