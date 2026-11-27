//! The standard library's clock (`std::time::Instant::now`, `SystemTime::now`) panics
//! in the browser (wasm32-unknown-unknown), which leaves GPUI's App borrowed and breaks
//! the whole page. Code that can run in the browser reads the clock through `web_time`,
//! which is the standard clock natively and the browser's on the web. This test fails
//! when a source file names the standard clock outside the few native-only places below.

use std::path::{Path, PathBuf};

/// Files that may name the standard clock, and why.
const ALLOWED: &[(&str, &str)] = &[
    // Media Foundation's smoke tests run on Windows only.
    (
        "crates/rustryx-primitives/src/media/windows.rs",
        "Windows-only tests",
    ),
    // A test's time budget, never built for the browser.
    (
        "crates/rustryx-theme/src/authoring/status_roles.rs",
        "a test",
    ),
    // File modification times for `cargo xtask dev`, native only.
    ("apps/gallery/src/live_themes.rs", "native file watching"),
    // This file.
    ("crates/rustryx-primitives/tests/web_clock.rs", "this check"),
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !path.ends_with("target") && !path.ends_with("generated") {
                sources(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn browser_code_reads_the_clock_through_web_time() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for dir in ["crates", "apps", "examples"] {
        sources(&root.join(dir), &mut files);
    }
    let mut offenders = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOWED.iter().any(|(f, _)| rel == *f) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let names_std_clock = code.contains("std::time::Instant")
                || code.contains("std::time::SystemTime")
                || (code.contains("use std::time::{")
                    && (code.contains("Instant") || code.contains("SystemTime")));
            if names_std_clock {
                offenders.push(format!("{rel}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "use web_time::{{Instant, SystemTime}} instead of the standard clock:\n{}",
        offenders.join("\n")
    );
}
