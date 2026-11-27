//! Each window's gesture host, upstream `gestures/hostRegistry.ts`: code that
//! only asks a host that already exists (a page hold) reads it here without
//! creating one.

use std::collections::HashMap;

use gpui::{App, Global, WindowId};

use super::host::GestureHost;

#[derive(Default)]
pub(crate) struct GestureHosts(pub HashMap<WindowId, GestureHost>);

impl Global for GestureHosts {}

/// The window's host, if something created it.
pub fn existing_gesture_host(window: WindowId, cx: &App) -> Option<GestureHost> {
    cx.try_global::<GestureHosts>()
        .and_then(|hosts| hosts.0.get(&window).cloned())
}
