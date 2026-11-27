# Rustryx

## cold and L9

Rustryx is a Rust port of [Astryx](https://github.com/meta/astryx), Meta's open source
design system, built on [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui),
Zed's GPU-accelerated UI framework. It runs natively on Windows, macOS and Linux
and in the browser (WebAssembly), with no React or web view.

> **Status: in development.** Milestones M0 to M13 are done: the theme pipeline
> and tooling, foundations, controls, the editing engine and fields, overlays,
> collections, tables, dates, the shell and navigation, content (Markdown,
> CodeBlock, Chat), motion and touch, theme authoring, charts and the lab. All of
> upstream's 110 core components, 6 chart components and 47 lab components are
> ported; rich text (M14) is next. See
> [`docs/PLAN.md`](docs/PLAN.md) for the full-parity roadmap,
> [`docs/HANDOFF.md`](docs/HANDOFF.md) for the current state,
> [`UPSTREAM.md`](UPSTREAM.md) for the synced upstream commit, and
> [`docs/KNOWN_ISSUES.md`](docs/KNOWN_ISSUES.md).

## Crates

| Crate | Purpose |
|---|---|
| `rustryx` | Facade. Depend on this one. |
| `rustryx-theme` | Design tokens, themes, light/dark appearance |
| `rustryx-primitives` | Headless behavior: context, focus, keyboard, layers, editing |
| `rustryx-i18n` | Locales, message catalogs (ICU MessageFormat), text direction |
| `rustryx-effects` | CSS effects GPUI lacks: multi-stop gradients, edge fades, shapes |
| `rustryx-themes` | The seven published themes and their fonts, one cargo feature each |
| `rustryx-icons` | Built-in icons |
| `rustryx-components` | The components |
| `rustryx-syntax` | Syntax highlighting for CodeBlock and the code editors |
| `rustryx-markdown` | Markdown: upstream's parser and its mdast-aligned AST |
| `rustryx-charts` | Charts: scales, shapes, `Chart` and its marks |
| `rustryx-lab` | The lab components (upstream's experimental package) |
| `rustryx-cli` (`apps/cli`) | The `rustryx` CLI |

## Using Rustryx in your app

GPUI is not published on crates.io at a current version, so Rustryx depends on
Zed's git repository at one pinned revision. **Your app must use the same
revision**, or Cargo builds two copies of GPUI whose types do not mix.

```toml
[dependencies]
rustryx = { git = "<this repository>", branch = "main" } # or path = "../rustryx/crates/rustryx"
gpui = { git = "https://github.com/lukasgeciauskas/zed", rev = "769561f35b2eefa2fe86cd970fad7b4c1a9ea36f" }
gpui_platform = { git = "https://github.com/lukasgeciauskas/zed", rev = "769561f35b2eefa2fe86cd970fad7b4c1a9ea36f", default-features = false }

# Zed patches these crates for GPUI. Patches apply only at a workspace root,
# so your app must repeat them.
[patch.crates-io]
async-task = { git = "https://github.com/smol-rs/async-task", rev = "41fd3359c75e221bd8b187a8ee361aa309c1714c" }
calloop = { git = "https://github.com/zed-industries/calloop", rev = "3759371fee14c40066d64777e2a36b6ffcc22590" }
```

Then set up Rustryx when the app starts:

```rust
gpui_platform::application()
    .with_assets(rustryx::Assets::new()) // serves the icons; wrap your own source with `with_fallback`
    .run(|cx| {
        rustryx::init(cx);
        rustryx::theme::set_theme(rustryx::themes::neutral::theme(), cx);
        rustryx::theme::set_mode(rustryx::theme::ThemeMode::System, cx);
        // open windows; components read `rustryx::theme::current(window, cx)`
    });
```

The `rustryx` crate enables every theme by default. To ship fewer fonts, turn
off default features and enable `theme-neutral` (or any other `theme-*`).

Use the Rust toolchain in [`rust-toolchain.toml`](rust-toolchain.toml); it
matches the pinned Zed revision. On Linux, enable `gpui_platform`'s `wayland`
and `x11` features; on macOS, its `font-kit` feature (GPUI's Core Text text
system: without it macOS draws no text). `examples/consumer/Cargo.toml` shows
both.

## Development

```sh
cargo run -p rustryx-gallery          # the component gallery, native
cargo xtask dev                      # edit loop: relaunches the gallery in place on save; theme JSON reloads live
cargo xtask web                      # the gallery in the browser, at http://127.0.0.1:8080
cargo test --workspace
cargo run -q -p rustryx-cli -- help   # the rustryx CLI: component docs, tokens, guides, parity
cargo xtask gpui-src                 # where the pinned GPUI source is checked out
```

Coding agents start at [`CLAUDE.md`](CLAUDE.md). The `rustryx` CLI is the Rust
port's counterpart of upstream's agent-facing CLI: `rustryx component Button --dense`
shows upstream's props and guidance next to the Rust API, and `rustryx parity`
shows what is left to port. Guides are in [`docs/guide/`](docs/guide/).

`cargo xtask web` needs `wasm-bindgen-cli` at the version in `Cargo.lock`
(`cargo install wasm-bindgen-cli --version <version>`).

Windows release builds compile GPUI's shaders with `fxc.exe` from the Windows
10/11 SDK; debug builds compile them at run time.

macOS builds compile GPUI's Metal shaders with Xcode's `metal` compiler (in Xcode 26
the Metal Toolchain component: `xcodebuild -downloadComponent MetalToolchain`);
`gpui_platform`'s `runtime_shaders` feature compiles them at run time instead.

## License

MIT, like Astryx. See [`LICENSE`](LICENSE). Bundled fonts and icons keep their
own licenses; see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

"Astryx" is Meta's name for its design system. This port is not affiliated with
or endorsed by Meta.
