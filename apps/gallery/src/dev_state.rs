//! Where the gallery is: page, theme, mode, window bounds and scroll. When
//! `cargo xtask dev` relaunches the gallery after a rebuild, it passes a
//! state file in `GALLERY_STATE_FILE`, and the gallery reopens in place.

use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DevState {
    pub page: String,
    pub theme: String,
    pub mode: String,
    /// x, y, width, height in logical pixels.
    pub bounds: Option<[f32; 4]>,
    pub scroll_y: f32,
}

fn path() -> Option<PathBuf> {
    std::env::var_os("GALLERY_STATE_FILE").map(PathBuf::from)
}

impl DevState {
    /// The saved state, when the gallery runs under `cargo xtask dev`.
    pub fn load() -> Option<Self> {
        let text = std::fs::read_to_string(path()?).ok()?;
        let mut state = DevState::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "page" => state.page = value.to_string(),
                "theme" => state.theme = value.to_string(),
                "mode" => state.mode = value.to_string(),
                "scroll_y" => state.scroll_y = value.parse().unwrap_or(0.0),
                "bounds" => {
                    let parts: Vec<f32> = value.split(',').filter_map(|v| v.parse().ok()).collect();
                    if let [x, y, w, h] = parts[..] {
                        state.bounds = Some([x, y, w, h]);
                    }
                }
                _ => {}
            }
        }
        Some(state)
    }

    /// Writes the state, if the gallery runs under `cargo xtask dev`.
    pub fn save(&self) {
        let Some(path) = path() else { return };
        let bounds = self
            .bounds
            .map(|[x, y, w, h]| format!("{x},{y},{w},{h}"))
            .unwrap_or_default();
        let text = format!(
            "page={}\ntheme={}\nmode={}\nbounds={bounds}\nscroll_y={}\n",
            self.page, self.theme, self.mode, self.scroll_y
        );
        let _ = std::fs::write(path, text);
    }
}
