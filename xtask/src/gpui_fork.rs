//! Our GPUI fork: github.com/lukasgeciauskas/zed, branch `astryx` (Zed main
//! plus our renderer patches; see docs/gpui-fork.md).
//!
//! - `gpui-local on|off`: build against the local checkout at `../zed-astryx`
//!   while writing renderer patches, through a marked `[patch]` block in the
//!   workspace Cargo.toml. Turn it off before committing.
//! - `bump-gpui <rev>`: pin the fork revision everywhere the workspace names
//!   it, after pushing the fork.

use std::fs;

use anyhow::{Context, Result, bail};

use crate::workspace_root;

pub const FORK_URL: &str = "https://github.com/lukasgeciauskas/zed";
const BEGIN: &str = "# BEGIN gpui-local (cargo xtask gpui-local; never commit)";
const END: &str = "# END gpui-local";

fn local_block() -> String {
    format!(
        "{BEGIN}\n[patch.\"{FORK_URL}\"]\ngpui = {{ path = \"../zed-astryx/crates/gpui\" }}\ngpui_platform = {{ path = \"../zed-astryx/crates/gpui_platform\" }}\n{END}\n"
    )
}

fn strip_block(text: &str) -> String {
    match (text.find(BEGIN), text.find(END)) {
        (Some(start), Some(end)) => {
            let end = end + END.len();
            let end = text[end..].find('\n').map_or(text.len(), |n| end + n + 1);
            format!("{}{}", &text[..start], &text[end..])
        }
        _ => text.to_string(),
    }
}

pub fn local(args: &[String]) -> Result<()> {
    let root = workspace_root();
    let manifest = root.join("Cargo.toml");
    let text = fs::read_to_string(&manifest)?;
    let stripped = strip_block(&text);
    match args.first().map(String::as_str) {
        Some("on") => {
            let checkout = root.join("../zed-astryx/crates/gpui");
            if !checkout.exists() {
                bail!(
                    "no local fork checkout at {}: git clone --branch astryx {FORK_URL} ../zed-astryx",
                    checkout.display()
                );
            }
            fs::write(
                &manifest,
                format!("{}\n{}", stripped.trim_end(), "\n") + &local_block(),
            )?;
            println!(
                "building against ../zed-astryx; run `cargo xtask gpui-local off` before committing"
            );
        }
        Some("off") => {
            fs::write(&manifest, format!("{}\n", stripped.trim_end()))?;
            println!("building against the pinned fork revision");
        }
        _ => bail!("usage: cargo xtask gpui-local on|off"),
    }
    Ok(())
}

pub fn bump(args: &[String]) -> Result<()> {
    let rev = args.first().context("usage: cargo xtask bump-gpui <rev>")?;
    if rev.len() != 40 || !rev.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("give the full 40-character commit hash");
    }
    let root = workspace_root();
    for file in ["Cargo.toml", "README.md", "UPSTREAM.md"] {
        let path = root.join(file);
        let text = fs::read_to_string(&path)?;
        let mut out = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            if line.contains(FORK_URL) && line.contains("rev = \"") {
                let start = line.find("rev = \"").unwrap() + 7;
                let end = start + line[start..].find('"').unwrap();
                out.push_str(&line[..start]);
                out.push_str(rev);
                out.push_str(&line[end..]);
            } else {
                out.push_str(line);
            }
        }
        fs::write(&path, out)?;
    }
    println!("pinned the GPUI fork at {rev}; run `cargo update -p gpui` and the full test suite");
    Ok(())
}
