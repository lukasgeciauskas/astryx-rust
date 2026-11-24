# Known issues

## GPUI

- **A font stack with no available family panics.** GPUI's text system panics when a
  font family and every fallback are missing (for example `"SF Mono", Monaco, Consolas`
  in a browser).
  - Astryx avoids it: every theme passes through `astryx_theme::prepare`, which keeps
    only families the text system has and appends a bundled face (Figtree or
    JetBrains Mono).
  - Code that sets `font_family` directly on GPUI elements must use a registered family.
  - On the web a panic aborts the wasm instance with the app's `RefCell` still borrowed,
    so every later event logs `RefCell already borrowed` and the page stops responding.
    Those log lines are a symptom of an earlier panic, not a separate bug; look for the
    first `panicked at` line.

## Tooling

- **True hot-patching (`cargo xtask dev --hot-patch`) does not start on Windows yet.** GPUI's
  `hot-reload` feature compiles, and `dx serve --hot-patch` (Dioxus CLI 0.7.10, the
  version GPUI pins) builds the gallery. The fat link then fails with
  `Missing rustc capture`.
  - The gallery crate is recompiled under dx: fresh objects appear in
    `target/x86_64-pc-windows-msvc/desktop-dev/build/astryx-gallery/<hash>/out/`,
    Cargo 1.100's build-dir layout.
  - dx's `RUSTC_WORKSPACE_WRAPPER` capture files never appear in
    `target/dx/.captured-args/<scope>/`.
  - Running `cargo rustc` by hand with the same `RUSTC_WORKSPACE_WRAPPER=dx` and
    `DX_RUSTC=<dir>` does write them, including `astryx_gallery.bin.json`.
  - Suspected cause: dx 0.7.10 against Cargo 1.100's build directory layout. Retry when
    GPUI's pinned `subsecond` (and so `dx`) moves on.
  - The `rust-lld` linker setting also breaks dx's link interception, so the repo keeps
    the default linker.
  - The default `cargo xtask dev` does not depend on dx. It is a hot restart: it rebuilds
    and relaunches the gallery in place in about 4 s, and it reloads theme JSON live.

- **Chrome screenshots of a background tab hang.** Chrome pauses
  `requestAnimationFrame` in a tab whose window is not in the foreground. GPUI's web
  backend draws from that loop, so a DevTools screenshot (`Page.captureScreenshot`,
  used by browser automation) of such a tab waits for a frame that never comes.
  - The page itself is fine.
  - Bring the window to the foreground first. With Claude in Chrome, a `resize_window`
    call does it. Screenshots then work, and theme switching was verified this way.
