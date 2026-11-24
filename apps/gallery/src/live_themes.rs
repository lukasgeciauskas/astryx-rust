//! Live theme reload: when `GALLERY_LIVE_THEMES` names a folder of theme
//! JSON files (`cargo xtask dev` points it at `spec/themes`), saving the
//! active theme's file re-applies it at once, with no rebuild.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use astryx::theme::{self, Theme};
use gpui::App;

fn scan(dir: &Path) -> HashMap<PathBuf, SystemTime> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .collect()
}

/// Watches `GALLERY_LIVE_THEMES`, if set. `compiled` supplies each theme's
/// fonts and icons, which the JSON names but does not carry.
pub fn watch(compiled: Vec<Arc<Theme>>, cx: &mut App) {
    let Some(dir) = std::env::var_os("GALLERY_LIVE_THEMES").map(PathBuf::from) else {
        return;
    };
    eprintln!("astryx dev: live theme reload from {}", dir.display());
    cx.spawn(async move |cx| {
        let mut seen = scan(&dir);
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let now = scan(&dir);
            let changed: Vec<PathBuf> = now
                .iter()
                .filter(|(path, time)| seen.get(*path) != Some(*time))
                .map(|(path, _)| path.clone())
                .collect();
            seen = now;
            for path in changed {
                let Ok(json) = std::fs::read_to_string(&path) else {
                    continue;
                };
                cx.update(|cx| {
                    let (active, _) = theme::active(cx);
                    let base = compiled
                        .iter()
                        .find(|t| t.name == active.name)
                        .map(|t| (**t).clone())
                        .unwrap_or_default();
                    match Theme::from_json(&json, &base) {
                        Ok(loaded) if loaded.name == active.name => {
                            eprintln!("astryx dev: reloaded theme {}", loaded.name);
                            theme::set_theme(loaded, cx);
                        }
                        Ok(_) => {}
                        Err(error) => eprintln!("astryx dev: {}: {error:#}", path.display()),
                    }
                });
            }
        }
    })
    .detach();
}
