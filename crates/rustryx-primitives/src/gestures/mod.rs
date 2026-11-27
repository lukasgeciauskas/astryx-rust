//! A gesture recognizer system with UIKit's semantics, upstream
//! `packages/core/src/gestures`: the touch presentations of the components
//! build on it.
//!
//! UIKit is the reference throughout: the same recognizer states, the same
//! conflict rules, the same thresholds ([`crate::motion::uikit::gestures`]),
//! and touch handling that behaves like `UIControl` and `UIScrollView`.
//!
//! - The pure core: [`GestureArena`] decides who gets a touch; the
//!   recognizers ([`TapGestureRecognizer`], [`LongPressGestureRecognizer`],
//!   [`PanGestureRecognizer`], [`SwipeGestureRecognizer`],
//!   [`PinchGestureRecognizer`], [`ScreenEdgePanGestureRecognizer`])
//!   interpret touches; [`PressTracker`] is a control's view-level
//!   tracking; [`VelocityTracker`] is the one velocity filter. They run on
//!   plain pointer records and an injectable [`GestureClock`].
//! - The GPUI binding: [`host::GestureHost`] feeds the arena GPUI's input
//!   for the [`host::GestureArea`]s a window paints.

pub mod arena;
pub mod context_request;
pub mod finger_scroll;
pub mod geometry;
pub mod gesture_sync;
pub mod haptics;
pub mod hit_area;
pub mod host;
pub mod host_registry;
pub mod input_hold;
pub mod long_press;
pub mod native_scroll;
pub mod page_hold;
pub mod pan;
pub mod pinch;
pub mod press_tracker;
pub mod recognizer;
pub mod screen_edge_pan;
pub mod selection_lock;
pub mod swipe;
pub mod tap;
pub mod touch_action;
pub mod types;
pub mod velocity_tracker;
pub mod wheel_segmenter;

pub use arena::{
    ArenaBeganInput, ArenaCallbacks, ArenaOptions, ClickDisposition, GestureArena, GestureClock,
    ManualClock, TimerHandle, WeakArena,
};
pub use geometry::TAP_WOBBLE;
pub use long_press::{LongPress, LongPressGestureRecognizer, LongPressOptions};
pub use pan::{Hysteresis, Pan, PanDirection, PanGestureRecognizer, PanOptions};
pub use pinch::{Pinch, PinchGestureRecognizer, PinchOptions};
pub use press_tracker::{
    ControlEvent, PRESS_PROFILES, PressBounds, PressHighlightDelay, PressProfile, PressTracker,
    PressTrackerOptions,
};
pub use recognizer::{
    ALL_NATIVE_TOUCH, AnyRecognizer, MANIPULATION, NativeTouchBehavior, PanVerdict,
    PendingRecognition, Recognizer, RecognizerBehavior, RecognizerFamily, RecognizerKind,
    RecognizerOptions, WeakRecognizer,
};
pub use screen_edge_pan::{
    ScreenEdge, ScreenEdgePan, ScreenEdgePanGestureRecognizer, ScreenEdgePanOptions,
};
pub use selection_lock::{SelectionLock, lock_selection};
pub use swipe::{Swipe, SwipeDirection, SwipeGestureRecognizer, SwipeOptions};
pub use tap::{Tap, TapGestureRecognizer, TapOptions};
pub use types::{
    DefaultEnvironment, GestureEnvironment, GestureEventContext, GestureState, GestureTouch,
    NativeScrollProbe, Point, PointerKind, ScrollProbe, ScrollType, TouchBeganInput, TouchConsumer,
    TouchInput, TouchPhase, TouchSample, ViewKey, ViewRect,
};
pub use velocity_tracker::{Velocity, VelocityTracker, VelocityTrackerOptions};

#[cfg(test)]
mod tests;
