# Astryx for GPUI

A Rust port of [Astryx](https://github.com/meta/astryx), Meta's open source
design system, built on [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui),
Zed's GPU-accelerated UI framework. It runs natively on Windows and Linux and
in the browser (WebAssembly), with no React or web view.

> **Status: early development.** The workspace, toolchain, and core GPUI
> assumptions are in place (milestone M0). See [`docs/PLAN.md`](docs/PLAN.md)
> for the full-parity roadmap and [`UPSTREAM.md`](UPSTREAM.md) for the synced
> upstream commit.

## Crates

| Crate | Purpose |
|---|---|
| `astryx` | Facade. Depend on this one. |
| `astryx-theme` | Design tokens, themes, light/dark appearance |
| `astryx-primitives` | Headless behavior: context, focus, keyboard, layers, editing |
| `astryx-icons` | Built-in icons |
| `astryx-components` | The components |

## Using Astryx in your app

GPUI is not published on crates.io at a current version, so Astryx depends on
Zed's git repository at one pinned revision. **Your app must use the same
revision**, or Cargo builds two copies of GPUI whose types do not mix.

```toml
[dependencies]
astryx = { git = "<this repository>", branch = "main" } # or path = "../Astryx Rust/crates/astryx"
gpui = { git = "https://github.com/zed-industries/zed", rev = "2b248e607288f229695be89ca2e73face27e5c54" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "2b248e607288f229695be89ca2e73face27e5c54", default-features = false }

# Zed patches these crates for GPUI. Patches apply only at a workspace root,
# so your app must repeat them.
[patch.crates-io]
async-task = { git = "https://github.com/smol-rs/async-task", rev = "41fd3359c75e221bd8b187a8ee361aa309c1714c" }
calloop = { git = "https://github.com/zed-industries/calloop", rev = "3759371fee14c40066d64777e2a36b6ffcc22590" }
```

Use the Rust toolchain in [`rust-toolchain.toml`](rust-toolchain.toml); it
matches the pinned Zed revision. On Linux, enable `gpui_platform`'s `wayland`
and `x11` features.

## Development

```sh
cargo run -p astryx-gallery          # the component gallery, native
cargo xtask web                      # the gallery in the browser, at http://127.0.0.1:8080
cargo test --workspace
```

`cargo xtask web` needs `wasm-bindgen-cli` at the version in `Cargo.lock`
(`cargo install wasm-bindgen-cli --version <version>`).

Windows release builds compile GPUI's shaders with `fxc.exe` from the Windows
10/11 SDK; debug builds compile them at run time.

## License

MIT, like Astryx. See [`LICENSE`](LICENSE). Bundled fonts and icons keep their
own licenses; see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

"Astryx" is Meta's name for its design system. This port is not affiliated with
or endorsed by Meta.
