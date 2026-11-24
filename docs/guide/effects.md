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
| `backdrop-filter: blur` | Chat composer, Lightbox, SegmentedControl, lab TabBar/MediaPlayer | Upstream's non-blur fallback fill (a solid or translucent surface token) |
| `filter` | Dialog/Drawer/MobileNav backdrops, PowerSearch, Swipe, Table | Per use: usually an overlay color or opacity |
| `transform: rotate` | Chevrons, disclosure icons | SVG `with_transformation(Transformation::rotate(..))` |
| `transform: translate` | Slides, sheets, toasts | Animated position offsets (`with_animation`) |
| `transform: scale` (press) | Button press feedback | Decided with Button in M3: an animated inset of the visual layer, or omitted |
| `clip-path` | Layer arrows, steppers, tokens | Rounded `overflow_hidden`, or `canvas` paths |
| Blend modes, radial/conic gradients | Not used upstream | Not needed |

Policy:
- Never fork GPUI. A fork breaks the shared GPUI revision with the apps that use Astryx.
- Fill a gap in this layer, or upstream it to Zed and bump the revision.
