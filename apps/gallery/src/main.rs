#![cfg_attr(target_family = "wasm", no_main)]

use std::borrow::Cow;

use gpui::{
    App, Bounds, Context, FontWeight, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    rgb, size,
};

const FONTS: &[&[u8]] = &[
    include_bytes!("../../../crates/astryx-themes/fonts/figtree/Figtree-400.ttf"),
    include_bytes!("../../../crates/astryx-themes/fonts/figtree/Figtree-500.ttf"),
    include_bytes!("../../../crates/astryx-themes/fonts/figtree/Figtree-600.ttf"),
    include_bytes!("../../../crates/astryx-themes/fonts/figtree/Figtree-700.ttf"),
];

struct Gallery;

impl Render for Gallery {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let weights = [400., 500., 600., 700.];
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_8()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x1c2b33))
            .font_family("Figtree")
            .text_size(px(20.))
            .child(
                div()
                    .text_size(px(32.))
                    .font_weight(FontWeight(700.))
                    .child("Astryx gallery"),
            )
            .children(weights.map(|weight| {
                div().font_weight(FontWeight(weight)).child(format!(
                    "Figtree {weight}: The quick brown fox jumps over the lazy dog"
                ))
            }))
    }
}

fn run() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.text_system()
            .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect())
            .expect("failed to load the gallery fonts");
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Gallery),
        )
        .expect("failed to open the gallery window");
        cx.activate(true);
    });
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
