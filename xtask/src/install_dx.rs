//! `cargo xtask install-dx`: installs Dioxus CLI 0.7.10 with our fix for
//! Windows hot-patching (see docs/KNOWN_ISSUES.md).
//!
//! On Windows, Cargo 1.100 hands rustc the arguments of workspace crates in
//! an `@argfile`. dx's rustc wrapper looked for `--crate-name` among the
//! literal arguments, found none, captured nothing, and the fat link failed
//! with "Missing rustc capture". The patch:
//!
//! 1. expands `@argfile`s before the wrapper records the arguments (rustc
//!    itself still runs with the original, short command line);
//! 2. replays recorded arguments through an argfile when they are too long
//!    for a Windows command line.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::workspace_root;

const VERSION: &str = "0.7.10";

fn cargo_home() -> Result<PathBuf> {
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
    println!("installed the patched dx {VERSION}; `cargo xtask dev --hot-patch` uses it");
    Ok(())
}
