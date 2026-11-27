# CSS effects in GPUI

GPUI draws rectangles, borders, shadows, text, SVG, images, paths and 2-stop
linear gradients. It has no CSS engine. This table is how each CSS effect
Astryx uses is drawn. Checked at Zed rev `2b248e6` against upstream usage.

| CSS effect | Upstream use | GPUI approach |
|---|---|---|
| Multiple and inset `box-shadow` | Elevation, rings | Native: `BoxShadow` has `inset`, any number of layers |
| Hover/press overlay (`linear-gradient` of an overlay token) | Most interactive fills | `astryx_theme::color::composite(fill, t.color.overlay_hover)` |
| `color-mix(in srgb, ..)` | Indicators, Link, Switch, Slider | `astryx_theme::color::mix_srgb` |
| 2-stop `linear-gradient` | Fades, table sticky edges | `linear_gradient(angle, from, to)` |
| Multi-stop gradient, stops at lengths (`black 36px`, `calc(100% - 8px)`) | Edge masks, fades | `astryx_effects::gradient(direction, stops)`: adjacent 2-stop bands, premultiplied like CSS, rounded with `.rounded*()` |
| `mask-image` edge fade | TabList, Carousel, Chat, DateInput scroll edges, Table and Tokenizer (AST-025 FR22), the touch wheel | Fork patch R3: `.mask_linear(astryx_effects::edge_mask(axis, start, end))` on the masked element; without offscreen layers (`GPUI_DISABLE_OFFSCREEN_LAYERS`) `astryx_effects::edge_fade(edge, length, surface)`, an overlay from the surface color to clear |
| `backdrop-filter: blur` | Chat composer, Lightbox, SegmentedControl, lab TabBar/MediaPlayer | Fork patch R2, a real backdrop blur; until it lands, upstream's non-blur fallback fill |
| `filter` | Swipe's disabled entry (`opacity(0.5)`); upstream's Dialog, Drawer and MobileNav backdrops deliberately use none | Fork patch R3: `.layer_opacity`, `.filter_blur`, `.filter_brightness`, `.filter_grayscale` |
| Group `opacity` (an element and its children faded as one) | Entrances, exits, cross-fades, the animator's `opacity` | Fork patch R3: `.layer_opacity(..)`; `.opacity(..)` fades each primitive and needs no layer |
| A transient copy of an element's last look | SideNav's rail, AppShell's content swap, List exits, a dragged row's preview | Fork patch R3: `.retain_layer(id)` keeps an element's frame, `gpui::layer_snapshot(id)` draws it again after the element changed or left |
| `transform: rotate` / CSS `rotate` | Chevrons, disclosure icons, Spinner; the animator's `rotate` channel | SVG `with_transformation(Transformation::rotate(..))` for icons; any element with `.rotate_degrees(..)` (fork patch R3), hit testing included |
| `transform: translate` | Slides, sheets, toasts | `.translate(x, y)`, fork patch R1; percentages are `relative(..)` of the element's own size |
| `transform: scale` (press, entrances) | Button press feedback, dialog and popover entrances | `.scale(..)` and `.transform_origin(..)`, fork patch R1; text, icons and shadows follow |
| `scaleX(-1)` RTL mirroring | Disclosure chevrons in tables, Collapsible | `.scale_xy(-1., 1.)`, fork patch R1 |
| `rotateX(..)` (3D) | DateInput wheel | Approximated with `.scale_xy(1., cos(angle))`: no perspective |
| `clip-path` | Layer arrows, steppers, tokens | `astryx_effects::polygon` / `triangle` (GPUI paths); rounded `overflow_hidden`; arbitrary alpha masks via fork patch R3 (`.mask(..)`, `Window::with_layer_mask`) |
| Blend modes, radial/conic gradients | Not used upstream | Not needed |

Policy: fill a gap in this layer when GPUI's existing paint APIs can draw it
(`crates/astryx-effects`; the gallery's Effects page shows each one).
Effects that need new shader code go in our GPUI fork as patches R1 to R4
(`docs/gpui-fork.md`).
