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

- **True hot-patching (`cargo xtask dev --hot-patch`) crashes on Windows after the
  first patch.** The default `cargo xtask dev` (hot restart, about 4 s, plus live theme
  reload) does not use it.
  - **Found and fixed: the capture failure.** On Windows, Cargo 1.100 hands rustc the
    arguments of workspace crates in an `@argfile`. dx 0.7.10's rustc wrapper looked for
    `--crate-name` in the literal arguments, captured nothing, and the fat link failed with
    `Missing rustc capture`.
  - `cargo xtask install-dx` installs dx 0.7.10 with our fix (`xtask/src/install_dx.rs`).
    The wrapper expands argfiles, and replays use an argfile when the command line would
    be too long. With it, the fat build succeeds, the gallery launches, and an edit is
    hot-patched in about 0.6 s.
  - **Still open: a stack overflow.** Right after the first patch the gallery's main
    thread overflows its stack (`0xc000041d`). It is recursion, not stack size: it happens
    with an 8 MB stack and with `/INCREMENTAL:NO`.
  - Suspected cause: subsecond's jump table and the patched `AnyView` render indirection
    on Windows.
  - Next steps:
    - get the recursive frames with a debugger (WinDbg/cdb);
    - try the same flow on Linux, subsecond's best-supported platform;
    - follow upstream subsecond fixes when GPUI moves its pin.
  - The `rust-lld` linker setting breaks dx's link interception, so the repo keeps the
    default linker.

- **Chrome screenshots of a background tab hang.** Chrome pauses
  `requestAnimationFrame` in a tab whose window is not in the foreground. GPUI's web
  backend draws from that loop, so a DevTools screenshot (`Page.captureScreenshot`,
  used by browser automation) of such a tab waits for a frame that never comes.
  - The page itself is fine.
  - Bring the window to the foreground first. With Claude in Chrome, a `resize_window`
    call does it. Screenshots then work, and theme switching was verified this way.
