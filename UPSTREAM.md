# Upstream sync

This port tracks [Astryx](https://github.com/meta/astryx). The checkout lives
next to this repository at `../Astryx`.

| What | Value |
|---|---|
| Upstream commit | `40ccf520df30933a70df9ee3068cddb7f8d2d0a7` (2026-11-24) |
| `@astryxdesign/core` | 0.7.18 |
| GPUI | the fork `lukasgeciauskas/zed` at `01da1adfa69364e9df02b39d897e24d001cf6fde`, based on Zed `2b248e607288f229695be89ca2e73face27e5c54` (2026-11-24); see `docs/gpui-fork.md` |
| Rust toolchain | 1.100.0 (matches the Zed revision) |

## Re-sync from upstream

Node is needed only for this step. A normal Rust build reads the committed
snapshots in `spec/` and never runs Node.

1. Update `../Astryx`, then run `pnpm install` and
   `pnpm -F @astryxdesign/core build` there.
2. Run `node tools/export/export-all.mjs ../Astryx` from this repository. It
   rewrites `spec/`. `spec/cldr.json` also takes calendar and week-field display names
   from Node's own `Intl` (Node 22 or later, which imports upstream's generated `.ts`
   tables directly), so use a current Node.
3. Run `cargo xtask codegen` to regenerate the Rust sources from `spec/`.
4. Run `cargo test --workspace`, and review the diff of `spec/` to see what
   changed upstream. `spec/code/fixtures.json` and the `spec/markdown/*-fixtures.json` files
   hold upstream's own tokenizer and Markdown parser output (core, plugins, streaming,
   legacy shapes); when they change, the
   fixture tests in `rustryx-syntax` and `rustryx-markdown` fail until the port follows.
   `spec/theme-definitions/*.json` (each shipped theme's `defineTheme` input) and
   `spec/palettes/*.json` (each generated palette's request and output) do the same for
   theme authoring (`rustryx_theme::authoring`).
5. Update the commit hash in the table above.
