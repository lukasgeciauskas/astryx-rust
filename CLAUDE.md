# Astryx for GPUI — agent guide

A 1:1 Rust port of Meta's Astryx design system (`../Astryx`, React) onto Zed's
GPUI, pinned to Zed git rev `2b248e6`. The roadmap is `docs/PLAN.md` (milestones
M0–M15); known problems are in `docs/KNOWN_ISSUES.md`.

## Before writing code, look things up

Don't guess. GPUI changes often, and upstream Astryx is the specification.

- `cargo run -q -p astryx-cli -- docs`: guides.
  - `gpui`: a cheat sheet verified at the pinned rev.
  - `conventions`, `customization`, `effects`, `porting`.
- `cargo run -q -p astryx-cli -- component <Name> --dense`: upstream props and guidance,
  plus the Rust API once ported. `component --list` lists everything; `parity` shows what
  is left.
- `cargo run -q -p astryx-cli -- tokens <filter> --theme neutral --mode dark`: token
  names, Rust fields and values.
- `cargo xtask gpui-src`: where the pinned GPUI source is. Grep `crates/gpui/src` and
  `crates/gpui/examples` for any API you are unsure of.
- Upstream behavior specs: `../Astryx/docs/specs/components/<Name>/<Name>.spec.md`
  (numbered FR/AR items become tests).

## Commands

| Task | Command |
|---|---|
| Inner loop | `cargo check -p <crate>` (~1–2 s incremental) |
| Gallery, native | `cargo run -p astryx-gallery` (`GALLERY_THEME=butter GALLERY_MODE=light` to pick a start state) |
| Gallery with hot reload | `cargo xtask dev` (needs `dx` 0.7.10; does not start on Windows yet, see `docs/KNOWN_ISSUES.md`) |
| Gallery, browser | `cargo xtask web` (http://127.0.0.1:8080) |
| Screenshot (Windows) | `powershell -File tools/capture-window.ps1 -Exe target/debug/astryx-gallery.exe -Out shot.png`; for a running window, `tools/capture-running.ps1 -Name astryx-gallery -Out shot.png` |
| Tests | `cargo test --workspace` |
| Regenerate from upstream | `node tools/export/export-all.mjs ../Astryx`, then `cargo xtask codegen` (see `UPSTREAM.md`) |

Measured on the dev machine (Windows, debug): an edit in `astryx-theme` or
`astryx-primitives` rebuilds the gallery in about 3.7 s, and `cargo check` takes
1–2 s. The first build of GPUI takes minutes.

## Rules

- **Generated files:** never hand-edit `*/generated/*`, `spec/`,
  `crates/astryx-icons/assets/` or `crates/astryx-themes/fonts/`. Change
  `tools/export` or `xtask/src/codegen` and regenerate. CI runs
  `cargo xtask codegen --check`.
- **Tokens:** read them during render with `astryx_theme::current(window, cx)`. Never
  hard-code a value a token provides.
- **Fonts:** never set `.font_family()` to an unregistered font; GPUI panics. Use
  `ThemeStyled::font_stack`.
- **GPUI:** never fork it. Every app that uses Astryx must share one GPUI revision.
  Fill gaps in this layer (see `astryx docs effects`).
- **Licensing:** code adapted from GPUI or gpui-component (Apache-2.0) keeps an
  attribution comment. Never copy from Zed's GPL crates (`editor`, `ui`, the Zed app).
- **New components:** follow `astryx docs porting`. That includes registering a
  `ComponentDoc` in `crates/astryx-components/src/doc.rs` and a gallery example.
- **Before you finish:**
  - `cargo fmt --all`;
  - `cargo clippy --workspace --all-targets -- -D warnings`;
  - `cargo test --workspace`;
  - `cargo xtask codegen --check`;
  - `cargo build -p astryx-gallery --target wasm32-unknown-unknown`.
