# GPUI cheat sheet (Zed rev 2b248e6)

Every API here is used by code in this repository and compiles at the pinned
revision. For anything else, read the real source, not memory:
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
  `astryx_primitives::use_context` works inside it.
- **Custom element:** `impl Element for E`. Do not also `impl IntoElement`; GPUI has
  a blanket impl for every `Element`.
- **Uncontrolled state in a component:** `window.use_keyed_state(id, cx, |window, cx| init)`
  returns an `Entity<S>` that lives as long as an element with that id is drawn.

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
  (`ClickEvent::Keyboard`); do not also handle them in `on_key_down`.
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
- Virtual lists: `uniform_list(id, count, |range, window, cx| items)` and `list(..)`.
  They need a view: drawing one through the test `cx.draw` helper panics in
  `Window::current_view`.

## Running and testing

- App: `gpui_platform::application().with_assets(astryx::Assets::new()).run(|cx| ..)`;
  on the web, call `gpui_platform::web_init()` first inside `#[wasm_bindgen(start)]`.
- Tests: `#[gpui::test] fn t(cx: &mut TestAppContext)`, with
  `gpui = { workspace = true, features = ["test-support"] }` in dev-dependencies.
  - Views: `let (view, cx) = cx.add_window_view(|window, cx| V { .. });`, then
    `cx.run_until_parked()`, `cx.simulate_keystrokes("up up")`,
    `cx.simulate_click(point(px(10.), px(10.)), Modifiers::none())`,
    `view.read_with(cx, |v, _| ..)`.
  - Elements: `cx.add_empty_window().draw(origin, space, |window, cx| element)`.
  - Working examples: `crates/astryx-primitives/src/tests.rs`,
    `crates/astryx-theme/src/tests.rs`.
  - Layout: give an element `.debug_selector(|| "name".into())` (a no-op in release) and
    read `cx.debug_bounds("name")`. Astryx's text controls carry `<id>-control`.
  - Text input: `cx.simulate_input("abc")`; file dialogs:
    `cx.simulate_path_prompt_response(|_| Some(paths))`; OS file drops:
    `cx.simulate_event(FileDropEvent::Entered { position, paths })`, then `Pending` and
    `Submit`.
