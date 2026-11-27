# GPUI cheat sheet (fork rev 01da1ad, Zed base 2b248e6)

Every API here is used by code in this repository and compiles at the pinned
revision of our fork (`docs/gpui-fork.md`). For anything else, read the real source, not memory:
`cargo xtask gpui-src` prints the local checkout; grep `crates/gpui/src` and
`crates/gpui/examples` (Apache-2.0). GPUI changes often, so an API you remember
from another version may not exist here.

## Views and components

- **View (stateful):** `struct V { .. }` + `impl Render for V { fn render(&mut self,
  window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement }`. Create with
  `cx.new(|cx| V { .. })` (an `Entity<V>`). Change it with
  `entity.update(cx, |v, cx| { ..; cx.notify(); })`; read with `entity.read(cx)`.
- **Component (stateless):** `#[derive(IntoElement)] struct C { .. }` +
  `impl RenderOnce for C { fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement }`.
  `render` runs during the parent's `request_layout`, which is why
  `rustryx_primitives::use_context` works inside it.
- **Custom element:** `impl Element for E`. Do not also `impl IntoElement`; GPUI has
  a blanket impl for every `Element`.
- **Uncontrolled state in a component:** `window.use_keyed_state(id, cx, |window, cx| init)`
  returns an `Entity<S>` that lives as long as an element with that id is drawn.
  - The key is resolved in the scope where `render` runs. A component that makes keyed
    state meant for another component it renders reads a different entity: hand the
    entity over instead (`Popover::controller(..)` takes its owner's
    `PopoverController`).
  - Reading keyed state outside an element phase panics ("can only be called during
    request_layout, prepaint, or paint"); in tests, observe through callbacks or debug
    selectors instead.
- **`role(..)` and the `aria_*` setters** are on stateful elements: give the div an
  `.id(..)` first. `.when(..)`, `.object_fit(..)` and the interactive methods come from
  `gpui::prelude::*`.

## Events, actions, focus

- Mouse handlers need an id: `div().id("x").on_click(|event, window, cx| ..)`. In a view,
  `cx.listener(|this, event, window, cx| ..)` gives `&mut Self`.
- Keyboard: `actions!(namespace, [Increment]);`, then
  `cx.bind_keys([KeyBinding::new("up", Increment, Some("MyContext"))])`, and on the element
  `.key_context("MyContext").track_focus(&focus).on_action(cx.listener(|this, _: &Increment, window, cx| ..))`.
- Focus: `let focus = cx.focus_handle();`, `window.focus(&focus, cx)`,
  `focus.is_focused(window)`.
- A key that matches a binding dispatches its action first, and the action stops the
  event: element `on_key_down` and `capture_key_down` listeners never see Backspace,
  Enter or arrows in a text field. To see every key first, use
  `cx.intercept_keystrokes(..)` (it runs before actions; `cx.stop_propagation()` there
  cancels the action). The editing engine does this for `on_key_down`.
- Enter and Space on a focused element with `on_click` arrive as a keyboard click
  (`ClickEvent::Keyboard`); do not also handle them in `on_key_down`. The click fires on
  key up, and only when the key went down on the same element, so the key that opens a
  menu never activates the row that takes focus.
- An element's `on_action` handler stops the action unless it calls `cx.propagate()`;
  the app-level `cx.on_action` handlers (Astryx's Tab traversal) run only if every
  element handler propagates.
- `cx.observe_keystrokes` runs after dispatch, whatever the elements did: the layer
  stack's Escape works this way, so an element cannot stop it. Block the layer's Escape
  (`EscapeBehavior::Block`) while a surface handles Escape itself.
- Held keys repeat as `KeyDownEvent { is_held: true, .. }`.
- Window-wide listeners: `window.on_mouse_event(|e: &MouseMoveEvent, phase, window, cx| ..)`
  registered while painting (from a `canvas` paint closure) hear every event for one
  frame. Bubble-phase listeners run in reverse paint order: later-painted (deferred)
  layers first. `on_mouse_move`/`on_mouse_up` on an element fire only while it is
  hovered.
- `MouseDownEvent::click_count` is 2 on a double click. Scrolling is `ScrollEvent` and
  `.on_scroll(..)` (`ScrollWheelEvent`/`on_scroll_wheel` are deprecated); its
  `delta.pixel_delta(line_height).y` is positive when scrolling up.
- `cx.on_focus` and `cx.on_blur` (and every focus listener) fire only in the active
  window. Test windows start inactive: call `window.activate_window()` first.

## Layout and style

- Flexbox on `div()`: `.flex().flex_col().gap(..).p(..).items_center()`. Token values
  (`AbsoluteLength`, `Hsla`) go straight into style methods: `.p(t.spacing.s4)`,
  `.bg(t.color.background_surface)`, `.rounded(t.radius.element)`.
- States: `.hover(|s| s.bg(..))`, `.active(..)`, `group("g")` + `.group_hover("g", ..)`.
- Text: use `ThemeStyled` (`.text_style(&t.text.body)`, `.font_stack(&t.font.family.body)`).
  Never call `.font_family("X")` with a family that is not registered: GPUI panics when
  no font in a stack exists.
- Shadows: `ThemeStyled::shadow_token(&t.shadow.low)`; `BoxShadow` has `inset`.
- Gradients: `linear_gradient(angle, from, to)`, two stops only.
- Transforms (our fork, patch R1): `.scale(0.96)`, `.scale_xy(-1., 1.)` (mirror),
  `.translate(x, y)`, `.translate_x(relative(1.))` (fractions of the element's own size)
  and `.transform_origin(0., 0.)`. They move the element and its subtree when painted;
  layout is unchanged and hit testing follows. They work in `.hover(..)`/`.active(..)`
  and `with_animation`. No element rotation: rotate SVGs with
  `svg().with_transformation(Transformation::rotate(..))`.
- Inline layout (our fork, patch R5): `.inline_flex()`, `.inline_block()` and
  `.inline_grid()` make inline-level boxes. Inside a block container (a plain `div()`,
  whose default display is block) consecutive ones flow in line boxes: side by side at
  their shrink-to-fit width, aligned on text baselines, wrapping at the container's width,
  placed by its `.text_center()`/`.text_right()`. In flex and grid containers they are
  ordinary items. Size keywords: `.w_fit()`, `.w_min()`, `.w_max()` (and `h_*`), or
  `fit_content()`, `min_content()`, `max_content()` wherever a `Length` goes. A custom
  measured element can report its first baseline with `window.set_layout_baseline(id, y)`.
- Overlays: `deferred(anchored().position(point).child(..))` draws above later siblings.
  Astryx wraps this in `rustryx_primitives::layer` and `anchored_layer` (see
  conventions).
- Active descendants: GPUI's `aria_active_descendant()` works only under the focused
  element, and a deferred layer paints at the accessibility root. Our fork's
  `aria_active_descendant_of(&focus_handle)` (patch R13) names the focused element by
  its handle instead, so a popup's option can be a combobox's active descendant.
- Focus listeners in a `RenderOnce`: `window.on_focus_in(&handle, cx, ..)` returns a
  `Subscription` to keep in keyed state (`cx.on_focus` needs a view's `Context`). It
  also fires for focus entering a descendant, and not when focus moves between the
  handle and its descendants.
- Images: `img(source)` with `.object_fit(ObjectFit::Contain)`. The natural size of a
  loaded image: `window.use_asset::<ImgResourceLoader>(&resource, cx)` (or the
  `RenderImage` of `ImageSource::Render`), then `image.size(0)`. A `RenderImage` built
  from an `image::RgbaImage` takes its bytes as BGRA.
- Async: `cx.background_executor().spawn(..)` needs `Send`; work that holds an `Rc`
  runs on the foreground with `cx.spawn(async move |cx| ..)` or
  `window.spawn(cx, async move |cx| ..)`. Timers:
  `cx.background_executor().timer(duration)`, and `background_executor().now()` is the
  clock tests advance.
- Virtual lists: `uniform_list(id, count, |range, window, cx| items)` and `list(..)`.
  They need a view: drawing one through the test `cx.draw` helper panics in
  `Window::current_view`.

## Running and testing

- App: `gpui_platform::application().with_assets(rustryx::Assets::new()).run(|cx| ..)`;
  on the web, call `gpui_platform::web_init()` first inside `#[wasm_bindgen(start)]`.
- Tests: `#[gpui::test] fn t(cx: &mut TestAppContext)`, with
  `gpui = { workspace = true, features = ["test-support"] }` in dev-dependencies.
  - Views: `let (view, cx) = cx.add_window_view(|window, cx| V { .. });`, then
    `cx.run_until_parked()`, `cx.simulate_keystrokes("up up")`,
    `cx.simulate_click(point(px(10.), px(10.)), Modifiers::none())`,
    `view.read_with(cx, |v, _| ..)`.
  - Elements: `cx.add_empty_window().draw(origin, space, |window, cx| element)`.
  - Working examples: `crates/rustryx-primitives/src/tests.rs`,
    `crates/rustryx-theme/src/tests.rs`.
  - Layout: give an element `.debug_selector(|| "name".into())` (a no-op in release, so
    components may carry them) and read `cx.debug_bounds("name")`. Astryx's text
    controls carry `<id>-control`, menu rows `<list id>-<row key>` (a DropdownMenu
    `m` has list `m-list`), CommandPalette rows `command-palette-item-<id>`.
  - Frames: `window.on_next_frame` never runs in tests. To act after the frame that first
    drew something, paint a `canvas` probe whose paint calls `window.defer(cx, ..)`.
  - Time: `cx.executor().advance_clock(duration)` moves timers and
    `background_executor().now()`; follow it with a few `refresh` +
    `run_until_parked` rounds.
  - Pointer: `simulate_mouse_down`, `simulate_mouse_move(point, button, mods)` and
    `simulate_mouse_up` drive drags and press-and-release gestures; a moved pointer
    updates hover, a down or up alone does not.
  - Announcements: render `rustryx_primitives::announcer()` in the host, then read
    `rustryx_primitives::announce::announcement(Politeness::Polite, window, cx)`.
  - Text input: `cx.simulate_input("abc")`; file dialogs:
    `cx.simulate_path_prompt_response(|_| Some(paths))`; OS file drops:
    `cx.simulate_event(FileDropEvent::Entered { position, paths })`, then `Pending` and
    `Submit`.
