//! What a view's recognizers leave the window, upstream
//! `gestures/touchAction.ts`: the intersection of every recognizer's native
//! touch behavior, as a CSS `touch-action` value. GPUI has no
//! `touch-action`; the host reads the combined behavior to decide whether a
//! finger's pan on a view stays the window's (a native scroll) or is held for
//! the view's recognizers, and the value is kept for parity and tests.

use super::recognizer::{AnyRecognizer, NativeTouchBehavior};

/// The intersection of what each claiming touch recognizer leaves the
/// window. A watcher leaves everything (FR20, DEC-7).
pub fn combined_native_behavior(recognizers: &[AnyRecognizer]) -> NativeTouchBehavior {
    let mut combined = NativeTouchBehavior {
        pan_x: true,
        pan_y: true,
        pinch_zoom: true,
        double_tap_zoom: true,
    };
    for recognizer in recognizers {
        if !recognizer.accepts_touch_input() || !recognizer.claims_touches() {
            continue;
        }
        let behavior = recognizer.native_touch_behavior();
        combined.pan_x &= behavior.pan_x;
        combined.pan_y &= behavior.pan_y;
        combined.pinch_zoom &= behavior.pinch_zoom;
        combined.double_tap_zoom &= behavior.double_tap_zoom;
    }
    combined
}

/// The `touch-action` value for a set of recognizers, or `None` when none
/// is needed.
pub fn derive_touch_action(recognizers: &[AnyRecognizer]) -> Option<String> {
    touch_action_of(combined_native_behavior(recognizers))
}

/// The `touch-action` value of a behavior.
pub fn touch_action_of(behavior: NativeTouchBehavior) -> Option<String> {
    if behavior.pan_x && behavior.pan_y && behavior.pinch_zoom && behavior.double_tap_zoom {
        return None;
    }
    if behavior.pan_x && behavior.pan_y && behavior.pinch_zoom {
        return Some("manipulation".into());
    }
    let mut tokens = Vec::new();
    if behavior.pan_x {
        tokens.push("pan-x");
    }
    if behavior.pan_y {
        tokens.push("pan-y");
    }
    if behavior.pinch_zoom {
        tokens.push("pinch-zoom");
    }
    Some(if tokens.is_empty() {
        "none".into()
    } else {
        tokens.join(" ")
    })
}

const ALL_PANS: [&str; 5] = ["pan-left", "pan-right", "pan-up", "pan-down", "pinch-zoom"];

fn touch_action_tokens(value: &str) -> Vec<&'static str> {
    let mut parts: Vec<&str> = value.split_whitespace().collect();
    if parts.is_empty() {
        parts.push("auto");
    }
    let mut tokens: Vec<&'static str> = Vec::new();
    let mut add = |token: &'static str| {
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    };
    for part in parts {
        match part {
            "auto" => {
                ALL_PANS.iter().for_each(|t| add(t));
                add("double-tap-zoom");
            }
            "manipulation" => ALL_PANS.iter().for_each(|t| add(t)),
            "pan-x" => {
                add("pan-left");
                add("pan-right");
            }
            "pan-y" => {
                add("pan-up");
                add("pan-down");
            }
            other => {
                if let Some(token) = ALL_PANS.iter().find(|t| **t == other) {
                    add(token);
                }
            }
        }
    }
    tokens
}

/// The `touch-action` that allows only what both values allow: the
/// derived value never overrides a caller's narrower one.
pub fn intersect_touch_action(derived: &str, caller: &str) -> String {
    let own = touch_action_tokens(caller);
    let both: Vec<&str> = touch_action_tokens(derived)
        .into_iter()
        .filter(|token| own.contains(token))
        .collect();
    if ALL_PANS.iter().all(|token| both.contains(token)) {
        return if both.contains(&"double-tap-zoom") {
            "auto".into()
        } else {
            "manipulation".into()
        };
    }
    let mut parts = Vec::new();
    for (axis, start, end) in [
        ("pan-x", "pan-left", "pan-right"),
        ("pan-y", "pan-up", "pan-down"),
    ] {
        match (both.contains(&start), both.contains(&end)) {
            (true, true) => parts.push(axis),
            (true, false) => parts.push(start),
            (false, true) => parts.push(end),
            (false, false) => {}
        }
    }
    if both.contains(&"pinch-zoom") {
        parts.push("pinch-zoom");
    }
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join(" ")
    }
}

/// Which pans a `touch-action` value lets the window make (upstream
/// `parseTouchAction`).
pub fn parse_touch_action(value: &str) -> (bool, bool) {
    let tokens = touch_action_tokens(value);
    (
        tokens.contains(&"pan-left") || tokens.contains(&"pan-right"),
        tokens.contains(&"pan-up") || tokens.contains(&"pan-down"),
    )
}
