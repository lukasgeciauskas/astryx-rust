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
   `cargo xtask gpui-local on` builds this workspace against the clone.
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
| R1 | Element transforms | Translate, scale and rotate any subtree around an origin, including text. Mouse hit testing follows the transform. | Button press scale (M3); dialog and popover entrances (M5); Carousel, Swipe (M9, M11) |
| R2 | Backdrop blur | A blurred copy of what is already drawn behind an element, clipped to its rounded rect | Chat composer, Lightbox, SegmentedControl (M5, M10); lab TabBar, MediaPlayer |
| R3 | Offscreen layers | A subtree drawn to a texture, then composited with group opacity, filters (blur, brightness, grayscale) or an alpha mask | Dialog, Drawer and MobileNav backdrops; PowerSearch, Swipe, Table (M5+) |
| R4 | Blend modes | Multiply and screen in fixed-function blending; the rest through R3 layers | Not used by upstream today; lowest priority |

### How each patch works

- **R1 (transforms):**
  - GPUI already gives glyph and SVG sprites a 2×3 transformation matrix.
  - The patch adds one to quads, shadows, underlines, image sprites and paths.
  - A window transform stack applies during paint, exposed as `Styled::transform`
    with a transform origin.
  - Hitboxes store the inverse transform.
  - Shaders change in `gpui_windows/src/shaders.hlsl` (Windows) and
    `gpui_wgpu/src/shaders*.wgsl` (Linux and web). The macOS Metal shader changes too but
    is untested, since macOS is not a target.
- **R2 (backdrop blur):**
  - A new scene primitive.
  - When the renderer meets it, it ends the current pass, copies the region behind it from
    the target, blurs that copy with a dual-Kawase downsample and upsample, and draws it
    clipped to the element's corner radii.
  - Then it resumes with load-on-begin.
  - The surface texture needs copy-source usage on wgpu.
- **R3 (offscreen layers):** a paint-time layer stack renders a subtree to its own texture,
  then composites it through a filter shader.
