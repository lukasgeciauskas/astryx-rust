//! `cargo xtask install-dx`: installs Dioxus CLI 0.7.10 with our fixes for
//! hot-patching on Windows (see docs/KNOWN_ISSUES.md). Stock dx 0.7.10 fails
//! on our workspace in five ways; the patch fixes each:
//!
//! 1. **No capture.** Cargo 1.100 hands rustc the arguments of workspace
//!    crates in an `@argfile` on Windows. dx's rustc wrapper looked for
//!    `--crate-name` among the literal arguments, captured nothing, and the
//!    fat link failed with "Missing rustc capture". The wrapper now expands
//!    argfiles before recording (rustc still runs with the short original).
//! 2. **Replays too long.** Recorded arguments are replayed through an
//!    argfile when they would overflow a Windows command line.
//! 3. **Stack overflow after the first patch.** Stubs that jump from the
//!    patch into the original binary were `movabs rax, addr; jmp rax`. RAX
//!    is `__chkstk`'s argument (the frame size to probe), so the first
//!    patched function with a large frame probed an address-sized frame and
//!    overflowed the stack. Stubs now use `jmp [rip+0]`, which clobbers no
//!    register (dx already did on other x86-64 platforms).
//! 4. **Edits in nested crates ignored.** dx watched only the app's direct
//!    path dependencies; it now watches them transitively.
//! 5. **"Missing rustc args for replay".** A change cascaded to every
//!    workspace crate depending on the edited one, including crates the app
//!    never builds. It now stops at crates that reach the app.
//!
//! The installer writes [`MARKER`] beside `dx.exe`; `cargo xtask dev
//! --hot-patch` checks it, so a stock dx is caught before it misbehaves.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::workspace_root;

const VERSION: &str = "0.7.10";

/// Written beside the installed `dx` with [`PATCH_LEVEL`]. Bump the level
/// whenever `patch` changes, so `cargo xtask dev --hot-patch` asks for a
/// reinstall.
pub const MARKER: &str = "dx.astryx-patch";
pub const PATCH_LEVEL: u32 = 5;

pub fn cargo_home() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("CARGO_HOME") {
        return Ok(PathBuf::from(home));
    }
    let user = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .context("no home directory")?;
    Ok(PathBuf::from(user).join(".cargo"))
}

/// The dioxus-cli source Cargo unpacked when it was installed or fetched.
fn registry_source() -> Result<PathBuf> {
    let src = cargo_home()?.join("registry").join("src");
    for index in fs::read_dir(&src).with_context(|| format!("reading {}", src.display()))? {
        let candidate = index?.path().join(format!("dioxus-cli-{VERSION}"));
        if candidate.join("Cargo.toml").exists() {
            return Ok(candidate);
        }
    }
    bail!(
        "dioxus-cli {VERSION} source not found; run `cargo install dioxus-cli --version {VERSION} --locked` once to download it, then rerun"
    )
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn replace(path: &Path, old: &str, new: &str) -> Result<()> {
    let text = fs::read_to_string(path)?;
    if text.contains(new) {
        return Ok(());
    }
    if !text.contains(old) {
        bail!(
            "{}: expected text not found; is this dioxus-cli {VERSION}?",
            path.display()
        );
    }
    fs::write(path, text.replacen(old, new, 1))?;
    Ok(())
}

const HELPERS: &str = r#"/// Expands rustc `@argfile` arguments (one argument per line). Cargo passes
/// long command lines this way on Windows.
pub(crate) fn expand_argfiles(args: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    for arg in args {
        if let Some(path) = arg.strip_prefix('@').filter(|p| !p.starts_with("shell:")) {
            if let Ok(text) = std::fs::read_to_string(path) {
                out.extend(text.lines().map(str::to_string));
                continue;
            }
        }
        out.push(arg);
    }
    out
}

/// The arguments to pass for `args`: the args themselves, or `@file` holding
/// them when they are too long for a Windows command line.
pub(crate) fn replay_args(args: &[String], tag: &str) -> Vec<String> {
    let total: usize = args.iter().map(|a| a.len() + 3).sum();
    if total < 24_000 {
        return args.to_vec();
    }
    let file = std::env::temp_dir().join(format!("dx-replay-{tag}-{}.args", std::process::id()));
    match std::fs::write(&file, args.join("\n")) {
        Ok(()) => vec![format!("@{}", file.display())],
        Err(_) => args.to_vec(),
    }
}

pub fn run_rustc() -> ExitCode {"#;

fn patch(src: &Path) -> Result<()> {
    // The copy sits under this workspace's target/; make it its own workspace.
    let manifest = src.join("Cargo.toml");
    let text = fs::read_to_string(&manifest)?;
    if !text.contains("\n[workspace]") {
        fs::write(&manifest, format!("{text}\n[workspace]\n"))?;
    }
    let wrapper = src.join("src/rustcwrapper.rs");
    replace(&wrapper, "pub fn run_rustc() -> ExitCode {", HELPERS)?;
    replace(
        &wrapper,
        "    let captured_args = args().skip(1).collect::<Vec<_>>();\n",
        "    let original_args = args().skip(1).collect::<Vec<_>>();\n    let captured_args = expand_argfiles(original_args.clone());\n",
    )?;
    replace(
        &wrapper,
        "    cmd.args(captured_args.iter().skip(1));",
        "    cmd.args(original_args.iter().skip(1));",
    )?;
    replace(
        &src.join("src/build/link.rs"),
        "        cmd.args(&replay_args);",
        "        cmd.args(crate::rustcwrapper::replay_args(&replay_args, crate_name));",
    )?;
    replace(
        &src.join("src/build/request.rs"),
        "                cmd.args(rustc_args.args[1..].iter());",
        "                cmd.args(crate::rustcwrapper::replay_args(&rustc_args.args[1..], \"tip\"));",
    )?;
    // Windows x64 stubs for symbols the patch takes from the original binary
    // were `movabs rax, addr; jmp rax`. That clobbers RAX, which `__chkstk`
    // takes as its argument (the frame size to probe), so the first patched
    // function with a large frame probed ~0x7FF6.. bytes and overflowed the
    // stack. `jmp [rip+0]` with the address inline clobbers nothing; dx
    // already emits it on the other x86-64 platforms.
    replace(
        &src.join("src/build/patch.rs"),
        "                            let mut code = vec![\n                                0x48, 0xB8, // movabs RAX, imm64 (move 64-bit immediate to RAX)\n                            ];\n                            // Append the absolute 64-bit address\n                            code.extend_from_slice(&abs_addr.to_le_bytes());\n                            // jmp RAX (jump to the address in RAX)\n                            code.extend_from_slice(&[0xFF, 0xE0]);\n                            code",
        "                            // jmp [rip+0] with the address inline: clobbers no register\n                            // (RAX carries __chkstk's argument).\n                            let mut code = vec![0xFF, 0x25, 0x00, 0x00, 0x00, 0x00];\n                            code.extend_from_slice(&abs_addr.to_le_bytes());\n                            code",
    )?;
    // dx watched only the app's direct path dependencies, so an edit in a
    // crate further down (astryx-components under the `astryx` facade) was
    // never noticed. Walk path dependencies transitively instead.
    replace(
        &src.join("src/serve/runner.rs"),
        "        let mut paths = vec![];\n\n        for (dependency, _edge) in self.workspace.krates.get_deps(crate_package) {",
        "        let mut paths: Vec<PathBuf> = vec![];\n        let mut stack = vec![crate_package];\n        let mut visited = std::collections::HashSet::new();\n        while let Some(package) = stack.pop() {\n        if !visited.insert(package) {\n            continue;\n        }\n        for (dependency, _edge) in self.workspace.krates.get_deps(package) {",
    )?;
    replace(
        &src.join("src/serve/runner.rs"),
        "            paths.push(\n                krate\n                    .manifest_path\n                    .parent()\n                    .unwrap()\n                    .to_path_buf()\n                    .into_std_path_buf(),\n            );\n        }\n\n        paths\n    }",
        "            let dir = krate\n                .manifest_path\n                .parent()\n                .unwrap()\n                .to_path_buf()\n                .into_std_path_buf();\n            if !paths.contains(&dir) {\n                paths.push(dir);\n            }\n            if let Some(id) = self.workspace.krates.nid_for_kid(&krates::Kid::from(krate.id.clone())) {\n                stack.push(id);\n            }\n        }\n        }\n\n        paths\n    }",
    )?;
    // The change cascade took every workspace crate that depends on the
    // edited one, including crates the app never builds (astryx-cli depends
    // on astryx-components). dx has no rustc args for those and the patch
    // failed with "Missing rustc args for replay". Keep only dependents
    // that reach the app.
    replace(
        &src.join("src/build/request.rs"),
        "        krates\n            .direct_dependents(target_nid)\n            .into_iter()\n            .filter_map(|dep| {\n                let name = dep.krate.name.replace('-', \"_\");\n                if workspace_names.contains(&name) {",
        "        let mut tip_closure: HashSet<String> = HashSet::new();\n        let mut stack = vec![self.crate_package];\n        let mut seen = HashSet::new();\n        while let Some(nid) = stack.pop() {\n            if !seen.insert(nid) {\n                continue;\n            }\n            for (dependency, _edge) in krates.get_deps(nid) {\n                let krate = match dependency {\n                    krates::Node::Krate { krate, .. } => krate,\n                    krates::Node::Feature { krate_index, .. } => &krates[krate_index.index()],\n                };\n                tip_closure.insert(krate.name.replace('-', \"_\"));\n                if let Some(id) = krates.nid_for_kid(&krates::Kid::from(krate.id.clone())) {\n                    stack.push(id);\n                }\n            }\n        }\n\n        krates\n            .direct_dependents(target_nid)\n            .into_iter()\n            .filter_map(|dep| {\n                let name = dep.krate.name.replace('-', \"_\");\n                if workspace_names.contains(&name) && tip_closure.contains(&name) {",
    )?;
    Ok(())
}

pub fn run(_args: &[String]) -> Result<()> {
    let source = registry_source()?;
    let work = workspace_root().join("target").join("dx-src");
    if work.exists() {
        fs::remove_dir_all(&work)?;
    }
    copy_dir(&source, &work)?;
    patch(&work)?;
    println!("patched dioxus-cli {VERSION} in {}", work.display());
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["install", "--locked", "--force", "--path"])
        .arg(&work)
        .env("CARGO_PROFILE_RELEASE_LTO", "false")
        .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "16")
        .status()
        .context("failed to run cargo install")?;
    if !status.success() {
        bail!("cargo install failed with {status}");
    }
    fs::write(
        cargo_home()?.join("bin").join(MARKER),
        PATCH_LEVEL.to_string(),
    )?;
    println!("installed the patched dx {VERSION}; `cargo xtask dev --hot-patch` uses it");
    Ok(())
}
