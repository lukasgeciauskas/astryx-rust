#![cfg_attr(target_family = "wasm", no_main)]

mod dev_state;
mod gallery;
mod live_themes;
#[cfg(all(windows, feature = "hot-reload"))]
mod overflow_probe;
mod pages;

use gpui::{App, Bounds, WindowBounds, WindowOptions, point, prelude::*, px, size};

use dev_state::DevState;

fn run() {
    #[cfg(all(windows, feature = "hot-reload"))]
    overflow_probe::install();
    application()
        .with_assets(astryx::Assets::new())
        .run(|cx: &mut App| {
            astryx::init(cx);
            let themes = gallery::available_themes();
            // Every theme's fonts load up front so switching never reflows
            // through a fallback face.
            for theme in &themes {
                astryx::theme::register_fonts(theme, cx);
            }
            // Under `cargo xtask dev` the saved state reopens the gallery in
            // place. Otherwise GALLERY_THEME, GALLERY_MODE and GALLERY_PAGE pick
            // the start, for screenshots; the default is neutral in system mode.
            let state = DevState::load();
            let wanted = state
                .as_ref()
                .map(|s| s.theme.clone())
                .filter(|t| !t.is_empty())
                .or_else(|| env("GALLERY_THEME"))
                .unwrap_or_else(|| "neutral".into());
            let start = themes
                .iter()
                .find(|t| t.name == wanted.as_str())
                .cloned()
                .unwrap_or_else(|| themes[0].clone());
            astryx::theme::set_theme(start, cx);
            live_themes::watch(themes.clone(), cx);
            let mode = state
                .as_ref()
                .map(|s| s.mode.clone())
                .filter(|m| !m.is_empty())
                .or_else(|| env("GALLERY_MODE"));
            match mode.as_deref() {
                Some("light") => astryx::theme::set_mode(astryx::theme::ThemeMode::Light, cx),
                Some("dark") => astryx::theme::set_mode(astryx::theme::ThemeMode::Dark, cx),
                _ => {}
            }

            let bounds = match state.as_ref().and_then(|s| s.bounds) {
                Some([x, y, w, h]) => Bounds::new(point(px(x), px(y)), size(px(w), px(h))),
                None => Bounds::centered(None, size(px(1280.), px(860.)), cx),
            };
            let page = state
                .as_ref()
                .map(|s| s.page.clone())
                .filter(|p| !p.is_empty())
                .or_else(|| env("GALLERY_PAGE"));
            let scroll_y = state.as_ref().map_or(0.0, |s| s.scroll_y);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| cx.new(|_| gallery::Gallery::new(themes, page.as_deref(), scroll_y)),
            )
            .expect("failed to open the gallery window");
            cx.activate(true);
        });
}

fn env(name: &str) -> Option<String> {
    #[cfg(not(target_family = "wasm"))]
    {
        std::env::var(name).ok()
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = name;
        None
    }
}

#[cfg(not(target_family = "wasm"))]
fn application() -> gpui::Application {
    gpui_platform::application()
}

/// `?backend=webgl` or `?backend=webgpu` forces a browser graphics backend.
#[cfg(target_family = "wasm")]
fn application() -> gpui::Application {
    let search = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .unwrap_or_default();
    let has = |param: &str| {
        search
            .trim_start_matches('?')
            .split('&')
            .any(|p| p == param)
    };
    let backend = if has("backend=webgl") {
        gpui_platform::WebBackendPreference::WebGl
    } else if has("backend=webgpu") {
        gpui_platform::WebBackendPreference::WebGpu
    } else {
        gpui_platform::WebBackendPreference::Auto
    };
    gpui_platform::application_with_web_backend(backend)
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    run();
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    gpui_platform::web_init();
    run();
}
