//! Repository tasks: `cargo xtask <task>`.
//!
//! - `web [--release] [--no-serve] [--port N]`: build the gallery for the
//!   browser into `apps/gallery/dist` and serve it on localhost.

mod web;

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((task, rest)) = args.split_first() else {
        bail!("usage: cargo xtask <web> [options]");
    };
    match task.as_str() {
        "web" => web::run(rest),
        other => bail!("unknown task `{other}`"),
    }
}

/// The workspace root, one level above this crate.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace")
        .to_path_buf()
}
