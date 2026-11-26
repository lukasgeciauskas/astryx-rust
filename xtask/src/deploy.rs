//! `cargo xtask deploy --scope <team> [--preview]`: builds the browser gallery in release
//! and publishes it to Vercel (project `astryx-gallery`) with the Vercel CLI, which must be
//! installed and logged in. The site is `apps/gallery/web/deploy/` (a loading page and the
//! headers) around the wasm-bindgen output, assembled in `target/site/astryx-gallery`.

use std::fs;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::workspace_root;

pub fn run(args: &[String]) -> Result<()> {
    let scope = args
        .iter()
        .position(|a| a == "--scope")
        .and_then(|i| args.get(i + 1))
        .context("--scope <team> is required: the Vercel team that hosts the gallery")?;
    let production = !args.iter().any(|a| a == "--preview");

    crate::web::run(&["--release".to_string(), "--no-serve".to_string()])?;

    let root = workspace_root();
    let dist = root.join("apps/gallery/dist");
    // The folder's name is the Vercel project's.
    let site = root.join("target/site/astryx-gallery");
    if site.exists() {
        fs::remove_dir_all(&site)?;
    }
    fs::create_dir_all(&site)?;
    for name in ["astryx-gallery.js", "astryx-gallery_bg.wasm"] {
        fs::copy(dist.join(name), site.join(name)).with_context(|| format!("copying {name}"))?;
    }
    let template = root.join("apps/gallery/web/deploy");
    let wasm_bytes = fs::metadata(site.join("astryx-gallery_bg.wasm"))?.len();
    let index = fs::read_to_string(template.join("index.html"))?
        .replace("__WASM_BYTES__", &wasm_bytes.to_string());
    fs::write(site.join("index.html"), index)?;
    fs::copy(template.join("vercel.json"), site.join("vercel.json"))?;
    println!(
        "assembled {} ({:.1} MB of wasm)",
        site.display(),
        wasm_bytes as f64 / 1_048_576.
    );

    // `vercel` is a .cmd shim on Windows.
    let mut deploy = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "vercel"]);
        c
    } else {
        Command::new("vercel")
    };
    deploy
        .current_dir(&site)
        .args(["deploy", "--yes", "--scope", scope]);
    if production {
        deploy.arg("--prod");
    }
    let status = deploy
        .status()
        .context("failed to start the Vercel CLI (install: npm i -g vercel, then vercel login)")?;
    if !status.success() {
        bail!("vercel deploy exited with {status}");
    }
    Ok(())
}
