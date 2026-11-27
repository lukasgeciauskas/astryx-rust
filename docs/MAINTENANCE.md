# Keeping up with Zed and upstream Astryx

A monthly routine, so the fork never falls far behind Zed and the port never
drifts from upstream. Small, frequent updates stay cheap; a jump of several
months can mean real conflict work. An agent can run this whole page.

## 1. Rebase the GPUI fork

In `../zed-astryx` (branch `astryx`, one commit per patch, R1 and R5 to R12):

1. Once: `git remote add upstream https://github.com/zed-industries/zed.git`
   (the clone has only `origin`, the fork). Then each month:
   `git fetch upstream && git rebase upstream/main`. A conflict means Zed
   changed code a patch touches: fix that one patch and continue. Update the
   fork's base in [`gpui-fork.md`](gpui-fork.md).
2. Run the fork's own GPUI tests (`cargo test -p gpui`) and its examples for the
   patches (`crates/gpui/examples/astryx_*.rs`).
3. `git push --force-with-lease origin astryx`.

If one patch conflicts month after month, that is the time to offer it to Zed
as a pull request; otherwise keep carrying it.

## 2. Repin this workspace

1. `cargo xtask bump-gpui <new rev>` (rewrites every GPUI pin and the lockfile).
2. Make sure `cargo xtask gpui-local off` is in effect: no local `[patch]`
   block may be committed.

## 3. Re-export upstream Astryx

1. `git -C ../Astryx pull`, then in `../Astryx`: `pnpm install` and
   `pnpm -F @astryxdesign/core build`.
2. `node tools/export/export-all.mjs ../Astryx`, then `cargo xtask codegen`
   (see [`../UPSTREAM.md`](../UPSTREAM.md); record the synced commit there).
3. Review what changed:
   - `cargo run -q -p rustryx-cli -- parity` for new components;
   - `git diff spec/` for token, theme, locale and manifest changes;
   - `git -C ../Astryx log --stat <last synced>..HEAD -- docs/specs` for spec
     changes to existing components; port each changed FR item with its test.

## 4. The gate

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask codegen --check
cargo build -p rustryx-gallery --target wasm32-unknown-unknown
cargo fmt --all -- --check
```

Then look over the gallery (`cargo run -p rustryx-gallery`, and `cargo xtask
web` for the browser) in light and dark.

## 5. Release

Commit, push, wait for CI to pass, then tag (`git tag vX.Y && git push --tags`).

Apps depend on a tag, not a branch, and commit their `Cargo.lock` and a
`rust-toolchain.toml`. A pinned app keeps building as it is; move it to a newer
tag deliberately when it needs a fix or a component, and test it then.
