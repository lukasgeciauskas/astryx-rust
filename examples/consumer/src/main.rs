//! A minimal app that uses Astryx the way a downstream project does: through
//! the `astryx` facade only. It keeps the public API and the GPUI revision
//! alignment honest.

use astryx::prelude::*;
use gpui::{App, Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px, size};

struct Consumer;

impl Render for Consumer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(provide(
            ElementSize::Sm,
            div().child("Hello from an Astryx consumer"),
        ))
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(480.), px(320.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Consumer),
        )
        .expect("failed to open a window");
    });
}
