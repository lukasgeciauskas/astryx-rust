//! Repository tasks: `cargo xtask <task>`.
//!
//! - `codegen [--check]`: generate Rust sources from `spec/`.
//! - `dev`: run the gallery with hot-patching (needs `dx` 0.7.10).
//! - `gpui-src`: print where the pinned GPUI source is checked out.
//! - `web [--release] [--no-serve] [--port N]`: build the gallery for the
//!   browser into `apps/gallery/dist` and serve it on localhost.

mod codegen;
mod dev;
mod gpui_src;
mod web;

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((task, rest)) = args.split_first() else {
        bail!("usage: cargo xtask <codegen|dev|gpui-src|web> [options]");
    };
    match task.as_str() {
        "codegen" => codegen::run(rest),
        "dev" => dev::run(rest),
        "gpui-src" => gpui_src::run(rest),
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
