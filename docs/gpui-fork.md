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
| R1 | Element transforms (**landed**, `4d4a306`) | Translate, scale and mirror any subtree around an origin, including text. Mouse hit testing follows the transform. | Button press scale (M3); dialog and popover entrances (M5); Carousel, Swipe (M9, M11) |
| R2 | Backdrop blur | A blurred copy of what is already drawn behind an element, clipped to its rounded rect | Chat composer, Lightbox, SegmentedControl (M5, M10); lab TabBar, MediaPlayer |
| R3 | Offscreen layers | A subtree drawn to a texture, then composited with group opacity, filters (blur, brightness, grayscale) or an alpha mask | Dialog, Drawer and MobileNav backdrops; PowerSearch, Swipe, Table (M5+) |
| R4 | Blend modes | Multiply and screen in fixed-function blending; the rest through R3 layers | Not used by upstream today; lowest priority |

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
- **R2 (backdrop blur):**
  - A new scene primitive.
  - When the renderer meets it, it ends the current pass, copies the region behind it from
    the target, blurs that copy with a dual-Kawase downsample and upsample, and draws it
    clipped to the element's corner radii.
  - Then it resumes with load-on-begin.
  - The surface texture needs copy-source usage on wgpu.
- **R3 (offscreen layers):** a paint-time layer stack renders a subtree to its own texture,
  then composites it through a filter shader.
