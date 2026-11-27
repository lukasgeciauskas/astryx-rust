//! The data the gesture system passes around, upstream `gestures/types.ts`:
//! recognizer states, pointer kinds, touches (`UITouch`), the per-event
//! context, the environment a host provides, and the view-level touch
//! consumer contract. Positions are window px, times milliseconds on the
//! event clock.

use std::cell::{Ref, RefCell};
use std::fmt;
use std::rc::Rc;

use super::recognizer::AnyRecognizer;
use super::selection_lock::SelectionLock;

/// An opaque key for what a recognizer is attached to, or what a touch
/// landed on: the host's gesture area id. The arena never reads it.
pub type ViewKey = u64;

/// `UIGestureRecognizer.State`. A discrete recognizer goes straight from
/// `Possible` to `Ended` (UIKit's `.recognized` is the same value).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GestureState {
    Possible,
    Began,
    Changed,
    Ended,
    Cancelled,
    Failed,
}

impl GestureState {
    /// Upstream's name for the state.
    pub fn name(self) -> &'static str {
        match self {
            Self::Possible => "possible",
            Self::Began => "began",
            Self::Changed => "changed",
            Self::Ended => "ended",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    /// `isTerminal`: ended, cancelled or failed.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Ended | Self::Cancelled | Self::Failed)
    }

    /// `isActive`: began or changed.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Began | Self::Changed)
    }

    /// `isTransitionAllowed`: the transitions UIKit allows.
    pub fn allows(self, to: GestureState) -> bool {
        use GestureState::*;
        match self {
            Possible => matches!(to, Began | Ended | Failed),
            Began | Changed => matches!(to, Changed | Ended | Cancelled),
            Ended | Cancelled | Failed => to == Possible,
        }
    }
}

impl fmt::Display for GestureState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What made the contact. A pen on a tablet is `Pen`, not `Touch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerKind {
    Touch,
    Mouse,
    Pen,
}

impl PointerKind {
    /// Upstream's name for the kind.
    pub fn name(self) -> &'static str {
        match self {
            Self::Touch => "touch",
            Self::Mouse => "mouse",
            Self::Pen => "pen",
        }
    }
}

/// Scroll input a recognizer can take, UIKit's `UIScrollTypeMask`: a
/// trackpad's two-finger scroll or pinch (`Continuous`), or a mouse wheel's
/// notches (`Discrete`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScrollType {
    Continuous,
    Discrete,
}

/// A point (or a vector) in window px.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// The origin.
    pub const ZERO: Point = Point { x: 0., y: 0. };

    /// A point.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// One position of a contact at one instant.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TouchSample {
    pub x: f64,
    pub y: f64,
    /// Milliseconds on the event clock.
    pub time: f64,
}

/// A contact's phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TouchPhase {
    Began,
    Moved,
    Ended,
    Cancelled,
}

/// The scrollers between a touch and the window, as the host sees them:
/// enough for a recognizer to ask whether a native scroller would move if
/// the finger moved one way, without the recognizer reading any layout.
pub trait NativeScrollProbe {
    /// Whether a finger moving by (dx, dy) would scroll something under the
    /// touch (a scroller with room left that way, whose touch behavior lets
    /// a finger pan that axis).
    fn can_scroll(&self, dx: f64, dy: f64) -> bool;
    /// Whether anything under the touch scrolls natively at all.
    fn is_inside_scroller(&self) -> bool;
    /// Whether the window may pan this touch at all.
    fn allows_native_pan(&self) -> bool {
        true
    }
}

/// A shared scroll probe.
pub type ScrollProbe = Rc<dyn NativeScrollProbe>;

struct TouchCell {
    id: u64,
    pointer_type: PointerKind,
    start_x: f64,
    start_y: f64,
    start_time: f64,
    target: Option<ViewKey>,
    scroll: Option<ScrollProbe>,
    scroll_type: Option<ScrollType>,
    live: RefCell<TouchLive>,
}

#[derive(Clone)]
pub(crate) struct TouchLive {
    pub phase: TouchPhase,
    pub x: f64,
    pub y: f64,
    pub time: f64,
    pub samples: Vec<TouchSample>,
    pub taken_by_browser: bool,
    pub force: Option<f64>,
}

/// One contact, `UITouch`. The arena owns it and updates it in place; a
/// recognizer keeps a handle to every touch it receives. Cloning is cheap and
/// shares the contact.
#[derive(Clone)]
pub struct GestureTouch(Rc<TouchCell>);

impl fmt::Debug for GestureTouch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let live = self.0.live.borrow();
        f.debug_struct("GestureTouch")
            .field("id", &self.0.id)
            .field("pointer_type", &self.0.pointer_type)
            .field("phase", &live.phase)
            .field("x", &live.x)
            .field("y", &live.y)
            .field("time", &live.time)
            .finish()
    }
}

impl PartialEq for GestureTouch {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// What a new touch is made of.
pub(crate) struct TouchSeed {
    pub id: u64,
    pub pointer_type: PointerKind,
    pub x: f64,
    pub y: f64,
    pub time: f64,
    pub samples: Vec<TouchSample>,
    pub target: Option<ViewKey>,
    pub scroll: Option<ScrollProbe>,
    pub scroll_type: Option<ScrollType>,
    pub force: Option<f64>,
}

impl GestureTouch {
    pub(crate) fn new(seed: TouchSeed) -> Self {
        Self(Rc::new(TouchCell {
            id: seed.id,
            pointer_type: seed.pointer_type,
            start_x: seed.x,
            start_y: seed.y,
            start_time: seed.time,
            target: seed.target,
            scroll: seed.scroll,
            scroll_type: seed.scroll_type,
            live: RefCell::new(TouchLive {
                phase: TouchPhase::Began,
                x: seed.x,
                y: seed.y,
                time: seed.time,
                samples: seed.samples,
                taken_by_browser: false,
                force: seed.force,
            }),
        }))
    }

    /// A free-standing touch, for tests and recognizers driven without an
    /// arena.
    pub fn detached(id: u64, pointer_type: PointerKind, x: f64, y: f64, time: f64) -> Self {
        Self::new(TouchSeed {
            id,
            pointer_type,
            x,
            y,
            time,
            samples: vec![TouchSample { x, y, time }],
            target: None,
            scroll: None,
            scroll_type: None,
            force: None,
        })
    }

    pub(crate) fn live_mut(&self) -> std::cell::RefMut<'_, TouchLive> {
        self.0.live.borrow_mut()
    }

    /// Unique among the contacts the arena tracks at once.
    pub fn id(&self) -> u64 {
        self.0.id
    }
    pub fn pointer_type(&self) -> PointerKind {
        self.0.pointer_type
    }
    pub fn phase(&self) -> TouchPhase {
        self.0.live.borrow().phase
    }
    /// The latest position.
    pub fn x(&self) -> f64 {
        self.0.live.borrow().x
    }
    pub fn y(&self) -> f64 {
        self.0.live.borrow().y
    }
    /// The latest position as a point.
    pub fn position(&self) -> Point {
        let live = self.0.live.borrow();
        Point::new(live.x, live.y)
    }
    /// The time of the latest position.
    pub fn time(&self) -> f64 {
        self.0.live.borrow().time
    }
    pub fn start_x(&self) -> f64 {
        self.0.start_x
    }
    pub fn start_y(&self) -> f64 {
        self.0.start_y
    }
    /// Where the contact landed.
    pub fn start(&self) -> Point {
        Point::new(self.0.start_x, self.0.start_y)
    }
    pub fn start_time(&self) -> f64 {
        self.0.start_time
    }
    /// Positions since the previous event, oldest first, ending with the
    /// current one.
    pub fn samples(&self) -> Ref<'_, [TouchSample]> {
        Ref::map(self.0.live.borrow(), |live| live.samples.as_slice())
    }
    /// What the contact began on, opaque to the arena.
    pub fn target(&self) -> Option<ViewKey> {
        self.0.target
    }
    /// The native scrollers under the contact, when the host knows them.
    pub fn scroll(&self) -> Option<&ScrollProbe> {
        self.0.scroll.as_ref()
    }
    /// Set for a wheel's or a trackpad's scroll input, which is no contact.
    pub fn scroll_type(&self) -> Option<ScrollType> {
        self.0.scroll_type
    }
    /// The window took this contact for its own pan or zoom.
    pub fn taken_by_browser(&self) -> bool {
        self.0.live.borrow().taken_by_browser
    }
    /// The contact's normalized force (0 to 1) at its latest position, when
    /// the hardware reports one.
    pub fn force(&self) -> Option<f64> {
        self.0.live.borrow().force
    }
}

/// The input a host hands the arena for one contact in one event.
#[derive(Clone, Debug, Default)]
pub struct TouchInput {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub time: f64,
    /// Extra positions since the previous event, oldest first (coalesced).
    pub history: Vec<TouchSample>,
    /// The normalized force, when the hardware reports one.
    pub force: Option<f64>,
}

impl TouchInput {
    /// A contact at (x, y) at `time`.
    pub fn new(id: u64, x: f64, y: f64, time: f64) -> Self {
        Self {
            id,
            x,
            y,
            time,
            history: Vec::new(),
            force: None,
        }
    }
}

/// A new contact: where it is, what made it, and what it landed on.
#[derive(Clone)]
pub struct TouchBeganInput {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub time: f64,
    pub history: Vec<TouchSample>,
    pub pointer_type: PointerKind,
    pub target: Option<ViewKey>,
    pub scroll: Option<ScrollProbe>,
    pub scroll_type: Option<ScrollType>,
    /// The normalized force, when the hardware reports one.
    pub force: Option<f64>,
}

impl TouchBeganInput {
    /// A contact of `pointer_type` landing at (x, y) at `time`.
    pub fn new(id: u64, pointer_type: PointerKind, x: f64, y: f64, time: f64) -> Self {
        Self {
            id,
            x,
            y,
            time,
            history: Vec::new(),
            pointer_type,
            target: None,
            scroll: None,
            scroll_type: None,
            force: None,
        }
    }
}

/// What a recognizer is told alongside the touches that changed.
#[derive(Clone, Debug)]
pub struct GestureEventContext {
    /// The time of the event being processed.
    pub time: f64,
    /// Every contact the arena tracks now, not only this recognizer's.
    pub all_touches: Vec<GestureTouch>,
}

/// A box in window px.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// Facts about the window a recognizer may need and must not read itself:
/// the viewport for screen-edge pans, and a view's writing direction for
/// leading and trailing edges.
pub trait GestureEnvironment {
    /// The viewport's width and height.
    fn viewport_size(&self) -> (f64, f64);
    /// Whether `view` lays out right to left.
    fn is_rtl(&self, view: Option<ViewKey>) -> bool;
    /// The box of a recognizer's view, or `None` when it has none to measure.
    fn view_rect(&self, _view: Option<ViewKey>) -> Option<ViewRect> {
        None
    }
}

/// `DEFAULT_ENVIRONMENT` with no window: an empty viewport, left to right,
/// no view boxes.
pub struct DefaultEnvironment;

impl GestureEnvironment for DefaultEnvironment {
    fn viewport_size(&self) -> (f64, f64) {
        (0., 0.)
    }
    fn is_rtl(&self, _view: Option<ViewKey>) -> bool {
        false
    }
}

/// A view-level touch consumer: what UIKit delivers `touchesBegan` and
/// `touchesEnded` to when no recognizer takes the touch (a `UIControl`).
/// The arena delivers through it, honoring `delaysTouchesBegan`,
/// `delaysTouchesEnded` and `cancelsTouchesInView`.
pub trait TouchConsumer {
    fn touch_began(&self, touch: &GestureTouch);
    fn touch_moved(&self, touch: &GestureTouch);
    fn touch_ended(&self, touch: &GestureTouch);
    fn touch_cancelled(&self, touch: &GestureTouch);
    /// `UIControl`'s iOS 6 rule: a control's own action wins over an
    /// overlapping single-tap recognizer on an ancestor. Return false to stop
    /// `recognizer` (attached above this consumer) from beginning for a touch
    /// on it.
    fn allows_ancestor_recognizer(&self, _recognizer: &AnyRecognizer) -> bool {
        true
    }
    /// Whether this consumer can keep a touch at all (`keepsTouch` present
    /// upstream): a continuous-tracking control.
    fn can_keep_touch(&self) -> bool {
        false
    }
    /// Whether the consumer holds this touch now (a control whose press was
    /// delivered): the host keeps the window's pan off that finger.
    fn keeps_touch(&self, _touch_id: u64) -> bool {
        false
    }
    /// The lock a control whose hold acts brings for its press (FR41).
    fn selection_lock(&self) -> Option<SelectionLock> {
        None
    }
    /// Whether this touch's press holds the context menu closed now.
    fn holds_context_menu(&self, _touch_id: u64) -> bool {
        false
    }
    /// Whether the consumer takes touches now (a disabled control does not).
    fn is_enabled(&self) -> bool {
        true
    }
}
