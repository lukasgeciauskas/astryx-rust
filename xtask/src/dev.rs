//! `cargo xtask dev`: runs the gallery with hot-patching, so an edit shows in
//! the running window without a restart.
//!
//! GPUI's `hot-reload` feature routes every view render through Dioxus's
//! `subsecond` jump table in debug builds; `dx serve --hot-patch` rebuilds
//! only the changed code and patches it into the live process. GPUI pins
//! `subsecond =0.7.10`, so `dx` must be exactly that version.

use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::workspace_root;

const DX_VERSION: &str = "0.7.10";

pub fn run(args: &[String]) -> Result<()> {
    let version = Command::new("dx")
        .arg("--version")
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string());
    match version {
        Some(v) if v.contains(DX_VERSION) => {}
        Some(v) => bail!(
            "dx {DX_VERSION} is required (found `{}`): cargo install dioxus-cli --version {DX_VERSION} --locked",
            v.trim()
        ),
        None => {
            bail!("dx is not installed: cargo install dioxus-cli --version {DX_VERSION} --locked")
        }
    }

    // dx records the gallery's rustc invocation during its build to patch
    // later; if Cargo finds the crate fresh it never runs rustc and dx fails
    // with "Missing rustc capture". A new mtime makes Cargo rebuild it.
    let entry = workspace_root().join("apps/gallery/src/main.rs");
    std::fs::File::options()
        .append(true)
        .open(&entry)
        .and_then(|file| file.set_modified(std::time::SystemTime::now()))
        .context("failed to mark the gallery as changed")?;

    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let status = Command::new("dx")
        .current_dir(workspace_root())
        .args([
            "serve",
            "--hot-patch",
            "--platform",
            platform,
            "--package",
            "astryx-gallery",
            "--features",
            "hot-reload",
        ])
        .args(args)
        .status()
        .context("failed to start dx")?;
    if !status.success() {
        bail!("dx serve exited with {status}");
    }
    Ok(())
}
