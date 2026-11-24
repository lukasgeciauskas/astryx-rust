//! Shared timing for looping animations. Upstream pins spinners and
//! skeletons to the document timeline's origin, so every instance on screen
//! moves in phase; [`elapsed`] is that timeline here.

use std::time::{Duration, Instant};

use gpui::{App, Global};

struct Timeline(Instant);

impl Global for Timeline {}

/// Time since the app's animation timeline began.
pub fn elapsed(cx: &mut App) -> Duration {
    if !cx.has_global::<Timeline>() {
        cx.set_global(Timeline(Instant::now()));
    }
    cx.global::<Timeline>().0.elapsed()
}
