//! `cargo xtask dev`: the gallery's edit loop.
//!
//! By default this is a hot restart. It watches the workspace sources,
//! rebuilds the gallery when one changes, and relaunches it in place: same
//! page, theme, mode, window bounds and scroll (see
//! `apps/gallery/src/dev_state.rs`). A failed build leaves the running
//! gallery up and prints the errors.
//!
//! `--hot-patch` uses Dioxus's `dx serve --hot-patch` instead, which patches
//! changed code into the live process through GPUI's `hot-reload` feature.
//! GPUI pins `subsecond =0.7.10`, so `dx` must be exactly that version, built
//! with our Windows fixes by `cargo xtask install-dx`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};

use crate::{install_dx, workspace_root};

const DX_VERSION: &str = "0.7.10";
// Theme JSON in spec/themes reloads live (GALLERY_LIVE_THEMES); generated
// Rust from it lives in crates/, which is watched.
const WATCHED: &[&str] = &["crates", "apps/gallery"];
const EXTENSIONS: &[&str] = &["rs", "toml", "svg", "ttf"];

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "--hot-patch") {
        let rest: Vec<String> = args
            .iter()
            .filter(|a| *a != "--hot-patch")
            .cloned()
            .collect();
        return hot_patch(&rest);
    }
    hot_restart()
}

/// Every watched file and its modification time.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, SystemTime> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                if name != "target" && name != "dist" && name != "node_modules" {
                    walk(&path, out);
                }
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXTENSIONS.contains(&e))
                && let Ok(modified) = entry.metadata().and_then(|m| m.modified())
            {
                out.insert(path, modified);
            }
        }
    }
    let mut out = BTreeMap::new();
    for dir in WATCHED {
        walk(&root.join(dir), &mut out);
    }
    out
}

fn build(root: &Path) -> Result<bool> {
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(root)
        .args(["build", "-p", "rustryx-gallery"])
        .status()
        .context("failed to run cargo build")?;
    Ok(status.success())
}

/// Launches a copy of the built gallery. Windows locks a running
/// executable, so running the build output itself would make the next
/// build fail to link; each launch gets its own copy instead.
fn launch(root: &Path, state_file: &Path, generation: u32) -> Result<(Child, PathBuf)> {
    let built = root
        .join("target")
        .join("debug")
        .join(format!("rustryx-gallery{}", std::env::consts::EXE_SUFFIX));
    let run_dir = root.join("target").join("dev-run");
    std::fs::create_dir_all(&run_dir)?;
    let copy = run_dir.join(format!(
        "rustryx-gallery-{generation}{}",
        std::env::consts::EXE_SUFFIX
    ));
    std::fs::copy(&built, &copy).context("failed to copy the gallery build")?;
    let child = Command::new(&copy)
        .current_dir(root)
        .env("GALLERY_STATE_FILE", state_file)
        .env("GALLERY_LIVE_THEMES", root.join("spec").join("themes"))
        .spawn()
        .context("failed to launch the gallery")?;
    Ok((child, copy))
}

fn hot_restart() -> Result<()> {
    let root = workspace_root();
    let state_file = root.join("target").join("gallery-dev-state.txt");
    println!(
        "rustryx dev: watching {} — edit and save to relaunch the gallery in place",
        WATCHED.join(", ")
    );

    let mut generation = 0u32;
    let mut child: Option<(Child, PathBuf)> = None;
    if build(&root)? {
        child = Some(launch(&root, &state_file, generation)?);
    }
    let mut seen = snapshot(&root);
    loop {
        std::thread::sleep(Duration::from_millis(400));
        // Stop watching once the gallery is closed by hand.
        if let Some((running, _)) = child.as_mut()
            && running.try_wait()?.is_some()
        {
            println!("rustryx dev: the gallery closed; exiting");
            return Ok(());
        }
        let now = snapshot(&root);
        if now == seen {
            continue;
        }
        // Let a burst of saves settle before building.
        std::thread::sleep(Duration::from_millis(250));
        seen = snapshot(&root);
        let started = std::time::Instant::now();
        if build(&root)? {
            generation += 1;
            let next = launch(&root, &state_file, generation)?;
            if let Some((mut old, old_copy)) = child.replace(next) {
                let _ = old.kill();
                let _ = old.wait();
                let _ = std::fs::remove_file(old_copy);
            }
            println!(
                "rustryx dev: relaunched in {:.1}s",
                started.elapsed().as_secs_f32()
            );
        } else {
            println!("rustryx dev: build failed; the running gallery is unchanged");
        }
    }
}

fn hot_patch(args: &[String]) -> Result<()> {
    let version = Command::new("dx")
        .arg("--version")
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string());
    match version {
        Some(v) if v.contains(DX_VERSION) => {}
        Some(v) => bail!(
            "dx {DX_VERSION} is required (found `{}`): cargo xtask install-dx",
            v.trim()
        ),
        None => bail!("dx is not installed: cargo xtask install-dx"),
    }
    // Stock dx 0.7.10 cannot hot-patch this workspace (see install_dx.rs).
    let level = install_dx::cargo_home()
        .ok()
        .and_then(|home| std::fs::read_to_string(home.join("bin").join(install_dx::MARKER)).ok())
        .and_then(|text| text.trim().parse::<u32>().ok());
    if level != Some(install_dx::PATCH_LEVEL) {
        bail!(
            "the installed dx lacks our hot-patch fixes (level {level:?}, need {}): cargo xtask install-dx",
            install_dx::PATCH_LEVEL
        );
    }

    // dx records the gallery's rustc invocation during its build to patch
    // later; a new mtime makes Cargo rebuild the crate so dx sees it.
    let entry = workspace_root().join("apps/gallery/src/main.rs");
    std::fs::File::options()
        .append(true)
        .open(&entry)
        .and_then(|file| file.set_modified(SystemTime::now()))
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
            "rustryx-gallery",
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
