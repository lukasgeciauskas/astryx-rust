# Rustryx — agent guide

A 1:1 Rust port of Meta's Astryx design system (`../Astryx`, React) onto Zed's
GPUI, through the owner's fork (`docs/gpui-fork.md`) pinned to one revision. The
roadmap is `docs/PLAN.md` (milestones M0–M15); known problems are in
`docs/KNOWN_ISSUES.md`.

**Starting a session:** read `docs/HANDOFF.md` first. It records where the work
stands, the owner's standing instructions (a 1:1 replica; build what GPUI lacks;
finish whole milestones), the per-stage workflow and what comes next.
`docs/MAINTENANCE.md` is the monthly Zed and upstream update.

## Before writing code, look things up

Don't guess. GPUI changes often, and upstream Astryx is the specification.

- `cargo run -q -p rustryx-cli -- docs`: guides.
  - `gpui`: a cheat sheet verified at the pinned rev, with the test patterns.
  - `conventions`, `customization`, `effects`, `porting`.
- `cargo run -q -p rustryx-cli -- component <Name> --dense`: upstream props and guidance,
  plus the Rust API once ported. `component --list` lists everything; `parity` shows what
  is left.
- `cargo run -q -p rustryx-cli -- tokens <filter> --theme neutral --mode dark`: token
  names, Rust fields and values.
- `cargo xtask gpui-src`: where the pinned GPUI source is. Grep `crates/gpui/src` and
  `crates/gpui/examples` for any API you are unsure of.
- Upstream behavior specs: `../Astryx/docs/specs/components/<Name>/<Name>.spec.md`
  (numbered FR/AR items become tests).

## Commands

| Task | Command |
|---|---|
| Inner loop | `cargo check -p <crate>` (~1–2 s incremental) |
| Gallery, native | `cargo run -p rustryx-gallery` (`GALLERY_THEME=butter GALLERY_MODE=light GALLERY_PAGE=buttons GALLERY_SCROLL=400` to pick a start state; `GALLERY_SECTION=Schedule` picks a Lab or 3D section) |
| Gallery edit loop | `cargo xtask dev`: a save rebuilds and relaunches the gallery in place (same page, theme, mode, window, scroll) in about 4 s; editing `spec/themes/*.json` re-applies the theme live with no rebuild |
| True hot-patching | `cargo xtask install-dx` once, then `cargo xtask dev --hot-patch`: edits patch into the running gallery in about 0.6 s; layout or dependency changes need the hot-restart loop (`docs/KNOWN_ISSUES.md`) |
| Gallery, browser | `cargo xtask web` (http://127.0.0.1:8080; `?page=buttons&theme=butter&mode=light&scroll=400` picks the start state; `&section=` too) |
| Publish the gallery | `cargo xtask deploy --scope lamit` (after each milestone; https://rustryx-gallery.vercel.app) |
| Screenshot (Windows) | `powershell -File tools/capture-window.ps1 -Exe target/debug/rustryx-gallery.exe -Out shot.png`; for a running window, `tools/capture-running.ps1 -Name rustryx-gallery -Out shot.png` |
| Screenshots (macOS) | CI's `macos` job runs `tools/ci-macos.sh gallery|fork`; `gh run download <id> -n macos-screenshots`, then read the PNGs |
| Type-check for macOS | `cargo check --target aarch64-apple-darwin` (see HANDOFF's macOS lessons for the Metal placeholder) |
| Tests | `cargo test --workspace` |
| Regenerate from upstream | `node tools/export/export-all.mjs ../Astryx`, then `cargo xtask codegen` (see `UPSTREAM.md`) |

Measured on the dev machine (Windows, debug): an edit in `rustryx-theme` or
`rustryx-primitives` rebuilds the gallery in about 3.7 s, and `cargo check` takes
1–2 s. The first build of GPUI takes minutes.

## Rules

- **Generated files:** never hand-edit `*/generated/*`, `spec/`,
  `crates/rustryx-icons/assets/` or `crates/rustryx-themes/fonts/`. Change
  `tools/export` or `xtask/src/codegen` and regenerate. CI runs
  `cargo xtask codegen --check`.
- **Tokens:** read them during render with `rustryx_theme::current(window, cx)`. Never
  hard-code a value a token provides.
- **Fonts:** never set `.font_family()` to an unregistered font; GPUI panics. Use
  `ThemeStyled::font_stack`.
- **GPUI:** it comes from our fork (`docs/gpui-fork.md`), pinned to one revision that every
  app shares. Renderer patches go to the fork via `cargo xtask gpui-local on`; everything
  else is filled in this layer (see `rustryx docs effects`).
- **Licensing:** code adapted from GPUI or gpui-component (Apache-2.0) keeps an
  attribution comment. Never copy from Zed's GPL crates (`editor`, `ui`, the Zed app).
- **New components:** follow `rustryx docs porting`. That includes registering a
  `ComponentDoc` in `crates/rustryx-components/src/doc.rs` and a gallery example.
- **Before you finish:**
  - `cargo fmt --all`;
  - `cargo clippy --workspace --all-targets -- -D warnings`;
  - `cargo test --workspace`;
  - `cargo xtask codegen --check`;
  - `cargo build -p rustryx-gallery --target wasm32-unknown-unknown`.
