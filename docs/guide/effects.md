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
| Multi-stop gradient | Rare | Adjacent 2-stop segments |
| `mask-image` edge fade | TabList, Carousel, Chat, DateInput scroll edges | Gradient overlay from the surface color to transparent |
| `backdrop-filter: blur` | Chat composer, Lightbox, SegmentedControl, lab TabBar/MediaPlayer | Fork patch R2, a real backdrop blur; until it lands, upstream's non-blur fallback fill |
| `filter` | Dialog/Drawer/MobileNav backdrops, PowerSearch, Swipe, Table | Fork patch R3 (offscreen layers with filters) |
| `transform: rotate` | Chevrons, disclosure icons | SVG `with_transformation(Transformation::rotate(..))` |
| `transform: translate` | Slides, sheets, toasts | Fork patch R1; GPUI's `with_element_offset` also moves a subtree at paint time |
| `transform: scale` (press, entrances) | Button press feedback, dialog and popover entrances | Fork patch R1 (transforms on any element, text included) |
| `clip-path` | Layer arrows, steppers, tokens | Rounded `overflow_hidden` or `canvas` paths; arbitrary alpha masks via fork patch R3 |
| Blend modes, radial/conic gradients | Not used upstream | Not needed |

Policy: fill a gap in this layer when GPUI's existing paint APIs can draw it.
Effects that need new shader code go in our GPUI fork as patches R1 to R4
(`docs/gpui-fork.md`).
