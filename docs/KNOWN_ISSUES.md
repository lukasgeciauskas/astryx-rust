# Known issues

## GPUI

- **A font stack with no available family panics.** GPUI's text system
  panics when a font family and every fallback are missing (for example
  `"SF Mono", Monaco, Consolas` in a browser). Astryx avoids it: every theme
  passes through `astryx_theme::prepare`, which keeps only families the text
  system has and appends a bundled face (Figtree or JetBrains Mono). Code
  that sets `font_family` directly on GPUI elements must use a family that is
  registered.
- **Web: Chrome screenshots freeze the page.** Capturing a screenshot of the
  web gallery through the DevTools protocol (`Page.captureScreenshot`, which
  Chrome automation tools use) wedges the renderer on any non-trivial page,
  with WebGPU and WebGL2 alike. The page runs and handles input normally
  otherwise. Before the freeze GPUI logs `RefCell already borrowed` while
  handling resize and visual-viewport events during a draw, which points at
  re-entrancy in `gpui_web`'s window event handling. For visual checks, use
  the native gallery and `tools/capture-window.ps1` on Windows.
