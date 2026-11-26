//! The touch-action table of upstream's README ("What the browser is told")
//! and `touchAction.ts`'s intersection, which upstream's host tests check
//! through the DOM; here on the derived values directly.

use std::cell::Cell;
use std::rc::Rc;

use super::harness::TestEnv;
use crate::gestures::touch_action::{
    derive_touch_action, intersect_touch_action, parse_touch_action,
};
use crate::gestures::*;

fn pan(direction: PanDirection) -> AnyRecognizer {
    PanGestureRecognizer::new(PanOptions {
        direction,
        ..PanOptions::default()
    })
    .any()
    .clone()
}

fn action(recognizers: &[AnyRecognizer]) -> Option<String> {
    derive_touch_action(recognizers)
}

#[test]
fn derives_the_readmes_touch_action_table() {
    let tap = TapGestureRecognizer::new(TapOptions::default())
        .any()
        .clone();
    let press = LongPressGestureRecognizer::new(LongPressOptions::default())
        .any()
        .clone();
    assert_eq!(
        action(std::slice::from_ref(&tap)).as_deref(),
        Some("manipulation")
    );
    assert_eq!(action(&[press]).as_deref(), Some("manipulation"));
    assert_eq!(
        action(&[pan(PanDirection::Horizontal)]).as_deref(),
        Some("pan-y pinch-zoom")
    );
    let swipe = SwipeGestureRecognizer::new(SwipeOptions::default())
        .any()
        .clone();
    assert_eq!(action(&[swipe]).as_deref(), Some("pan-y pinch-zoom"));
    assert_eq!(
        action(&[pan(PanDirection::Vertical)]).as_deref(),
        Some("pan-x pinch-zoom")
    );
    assert_eq!(action(&[pan(PanDirection::Down)]), None);
    let edge = PanGestureRecognizer::new(PanOptions {
        scroll_edge: true,
        ..PanOptions::default()
    })
    .any()
    .clone();
    assert_eq!(action(&[edge]), None);
    assert_eq!(
        action(&[pan(PanDirection::Any)]).as_deref(),
        Some("pinch-zoom")
    );
    let pinch = PinchGestureRecognizer::new(PinchOptions::default())
        .any()
        .clone();
    assert_eq!(
        action(std::slice::from_ref(&pinch)).as_deref(),
        Some("pan-x pan-y")
    );
    assert_eq!(
        action(&[pinch, pan(PanDirection::Any)]).as_deref(),
        Some("none")
    );
    let mut watching = PanOptions {
        direction: PanDirection::Horizontal,
        ..PanOptions::default()
    };
    watching.base.claims_touches = false;
    let watcher = PanGestureRecognizer::new(watching).any().clone();
    assert_eq!(action(&[watcher]), None);
    // A horizontal edge pan on a strip within its band takes pan-x; on a
    // wider view it leaves everything.
    let strip = ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions::default());
    strip.set_environment(Some(Rc::new(TestEnv {
        width: Cell::new(390.),
        height: Cell::new(844.),
        rtl: Cell::new(false),
        view: Cell::new(Some(ViewRect {
            left: 0.,
            top: 0.,
            width: 20.,
            height: 844.,
        })),
    })));
    assert_eq!(
        action(&[strip.any().clone()]).as_deref(),
        Some("pan-y pinch-zoom")
    );
    let wide = ScreenEdgePanGestureRecognizer::new(ScreenEdgePanOptions::default());
    assert_eq!(action(&[wide.any().clone()]), None);
    let _ = tap;
}

#[test]
fn never_widens_a_callers_narrower_touch_action() {
    assert_eq!(
        intersect_touch_action("pan-y pinch-zoom", "auto"),
        "pan-y pinch-zoom"
    );
    assert_eq!(intersect_touch_action("manipulation", "pan-y"), "pan-y");
    assert_eq!(intersect_touch_action("pan-y pinch-zoom", "pan-x"), "none");
    assert_eq!(
        intersect_touch_action("manipulation", "auto"),
        "manipulation"
    );
    assert_eq!(intersect_touch_action("auto", "auto"), "auto");
    assert_eq!(
        intersect_touch_action("pan-y", "pan-up pinch-zoom"),
        "pan-up"
    );
    assert_eq!(parse_touch_action("pan-y pinch-zoom"), (false, true));
    assert_eq!(parse_touch_action("none"), (false, false));
}
