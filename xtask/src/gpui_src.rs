//! `cargo xtask gpui-src`: prints where Cargo checked out the pinned GPUI, so
//! an agent can grep the real source (`crates/gpui/src`, `crates/gpui/examples`)
//! instead of guessing at an API that changes often.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::workspace_root;

pub fn run(_args: &[String]) -> Result<()> {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(workspace_root())
        .args(["metadata", "--format-version", "1"])
        .output()
        .context("failed to run cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let manifest = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"] == "gpui")
        .and_then(|p| p["manifest_path"].as_str())
        .context("gpui is not in the dependency graph")?;
    let crate_dir = Path::new(manifest)
        .parent()
        .context("manifest has no parent")?;
    let zed_root = crate_dir
        .parent()
        .and_then(Path::parent)
        .context("gpui is not inside a Zed checkout")?;
    println!("gpui crate:     {}", crate_dir.display());
    println!("gpui examples:  {}", crate_dir.join("examples").display());
    println!("zed checkout:   {}", zed_root.display());
    Ok(())
}
