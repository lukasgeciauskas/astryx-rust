# Astryx for GPUI

A Rust port of [Astryx](https://github.com/meta/astryx), Meta's open source
design system, built on [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui),
Zed's GPU-accelerated UI framework. It runs natively on Windows and Linux and
in the browser (WebAssembly), with no React or web view.

> **Status: in development.** Milestones M0 to M5 are done: the theme pipeline,
> the developer tooling, foundations, actions and controls, the editing engine
> and fields, and the layer runtime with every overlay and menu. 72 of
> upstream's 110 core components are ported; collections (M6) are next. See
> [`docs/PLAN.md`](docs/PLAN.md) for the full-parity roadmap,
> [`docs/HANDOFF.md`](docs/HANDOFF.md) for the current state,
> [`UPSTREAM.md`](UPSTREAM.md) for the synced upstream commit, and
> [`docs/KNOWN_ISSUES.md`](docs/KNOWN_ISSUES.md).

## Crates

| Crate | Purpose |
|---|---|
| `astryx` | Facade. Depend on this one. |
| `astryx-theme` | Design tokens, themes, light/dark appearance |
| `astryx-primitives` | Headless behavior: context, focus, keyboard, layers, editing |
| `astryx-i18n` | Locales, message catalogs (ICU MessageFormat), text direction |
| `astryx-effects` | CSS effects GPUI lacks: multi-stop gradients, edge fades, shapes |
| `astryx-themes` | The seven published themes and their fonts, one cargo feature each |
| `astryx-icons` | Built-in icons |
| `astryx-components` | The components |
| `astryx-cli` (`apps/cli`) | The `astryx` CLI |

## Using Astryx in your app

GPUI is not published on crates.io at a current version, so Astryx depends on
Zed's git repository at one pinned revision. **Your app must use the same
revision**, or Cargo builds two copies of GPUI whose types do not mix.

```toml
[dependencies]
astryx = { git = "<this repository>", branch = "main" } # or path = "../Astryx Rust/crates/astryx"
gpui = { git = "https://github.com/lukasgeciauskas/zed", rev = "93e506ee3260224cb25b47ab20aa2238f764a16d" }
gpui_platform = { git = "https://github.com/lukasgeciauskas/zed", rev = "93e506ee3260224cb25b47ab20aa2238f764a16d", default-features = false }

# Zed patches these crates for GPUI. Patches apply only at a workspace root,
# so your app must repeat them.
[patch.crates-io]
async-task = { git = "https://github.com/smol-rs/async-task", rev = "41fd3359c75e221bd8b187a8ee361aa309c1714c" }
calloop = { git = "https://github.com/zed-industries/calloop", rev = "3759371fee14c40066d64777e2a36b6ffcc22590" }
```

Then set up Astryx when the app starts:

```rust
gpui_platform::application()
    .with_assets(astryx::Assets::new()) // serves the icons; wrap your own source with `with_fallback`
    .run(|cx| {
        astryx::init(cx);
        astryx::theme::set_theme(astryx::themes::neutral::theme(), cx);
        astryx::theme::set_mode(astryx::theme::ThemeMode::System, cx);
        // open windows; components read `astryx::theme::current(window, cx)`
    });
```

The `astryx` crate enables every theme by default. To ship fewer fonts, turn
off default features and enable `theme-neutral` (or any other `theme-*`).

Use the Rust toolchain in [`rust-toolchain.toml`](rust-toolchain.toml); it
matches the pinned Zed revision. On Linux, enable `gpui_platform`'s `wayland`
and `x11` features.

## Development

```sh
cargo run -p astryx-gallery          # the component gallery, native
cargo xtask dev                      # edit loop: relaunches the gallery in place on save; theme JSON reloads live
cargo xtask web                      # the gallery in the browser, at http://127.0.0.1:8080
cargo test --workspace
cargo run -q -p astryx-cli -- help   # the astryx CLI: component docs, tokens, guides, parity
cargo xtask gpui-src                 # where the pinned GPUI source is checked out
```

Coding agents start at [`CLAUDE.md`](CLAUDE.md). The `astryx` CLI is the Rust
port's counterpart of upstream's agent-facing CLI: `astryx component Button --dense`
shows upstream's props and guidance next to the Rust API, and `astryx parity`
shows what is left to port. Guides are in [`docs/guide/`](docs/guide/).

`cargo xtask web` needs `wasm-bindgen-cli` at the version in `Cargo.lock`
(`cargo install wasm-bindgen-cli --version <version>`).

Windows release builds compile GPUI's shaders with `fxc.exe` from the Windows
10/11 SDK; debug builds compile them at run time.

## License

MIT, like Astryx. See [`LICENSE`](LICENSE). Bundled fonts and icons keep their
own licenses; see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

"Astryx" is Meta's name for its design system. This port is not affiliated with
or endorsed by Meta.
