//! The one context request, upstream `gestures/contextRequest.ts`: a finger
//! or a pen held still for UIKit's long press, a pointer's context click (a
//! right click, a ctrl-click on macOS), or Shift+F10 and the context-menu
//! key. Each gesture makes one request, reporting its kind, its pointer and
//! its point (none from the keyboard). The hold is an arena participant, so
//! it loses to a claim another recognizer made on that finger first; a
//! control that keeps its press owns only its hold. The request answers on
//! its own area only, and only while enabled; inside nested requests only
//! the innermost answers.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use super::host::{ContextHandler, GestureAttachment, GestureHost, HostInner};
use super::long_press::{LongPressGestureRecognizer, LongPressOptions};
use super::recognizer::{AnyRecognizer, RecognizerOptions};
use super::selection_lock::lock_selection;
use super::types::{GestureState, GestureTouch, Point, PointerKind};

/// What made a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextRequestKind {
    Hold,
    Pointer,
    Keyboard,
}

/// What a context request reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextRequest {
    pub kind: ContextRequestKind,
    /// The pointer that made it; `None` from the keyboard.
    pub pointer_type: Option<PointerKind>,
    /// Where, in window px; `None` from the keyboard.
    pub point: Option<Point>,
    /// The pointer's id, when the host knows it.
    pub pointer_id: Option<u64>,
    /// When, on the host's clock.
    pub time: f64,
}

/// A hold's progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldPhase {
    Committed,
    Ended,
    Cancelled,
}

/// Upstream `ContextRequestOptions`.
#[derive(Clone)]
pub struct ContextRequestOptions {
    pub on_request: Rc<dyn Fn(&ContextRequest)>,
    /// Read at each input. @default always
    pub is_enabled: Option<Rc<dyn Fn() -> bool>>,
    /// Committed when the still finger claims its touch and the hold
    /// requests; then ended at its lift or cancelled.
    pub on_hold: Option<Rc<dyn Fn(HoldPhase)>>,
}

impl ContextRequestOptions {
    /// Options with a request callback.
    pub fn new(on_request: impl Fn(&ContextRequest) + 'static) -> Self {
        Self {
            on_request: Rc::new(on_request),
            is_enabled: None,
            on_hold: None,
        }
    }
}

/// How long after a request key's keydown the platform's own menu event is
/// that key's echo, ms.
pub const MENU_ECHO_MS: f64 = 500.;

/// The request's logic, apart from any window: tests drive it through an
/// arena and its pointer and key entries.
pub struct ContextRequestCore {
    options: ContextRequestOptions,
    recognizer: LongPressGestureRecognizer,
    held: Cell<Option<(u64, PointerKind)>>,
    is_hold_reported: Cell<bool>,
    last_request_key_at: Cell<f64>,
    last_pen_hold_end: Cell<f64>,
    host: Option<Weak<HostInner>>,
}

impl ContextRequestCore {
    /// The request; `host` answers whether a press-keeping control sits
    /// under a touch and the hold's clock.
    pub fn new(options: ContextRequestOptions, host: Option<&GestureHost>) -> Rc<Self> {
        Rc::new_cyclic(|this: &Weak<ContextRequestCore>| {
            let receive = this.clone();
            let mut base = RecognizerOptions::named("contextRequest");
            // A mouse held still selects text and drags: a hold is a finger's
            // or a pen's.
            base.allowed_pointer_types = Some(vec![PointerKind::Touch, PointerKind::Pen]);
            // A hold that opens a menu takes no selection or callout.
            base.selection_lock = Some(Some(lock_selection()));
            base.should_receive_touch =
                Some(Rc::new(move |r: &AnyRecognizer, touch: &GestureTouch| {
                    let Some(core) = receive.upgrade() else {
                        return false;
                    };
                    if !core.is_enabled() || core.keeps_press_under(r, touch) {
                        return false;
                    }
                    core.held.set(Some((touch.id(), touch.pointer_type())));
                    true
                }));
            let recognizer = LongPressGestureRecognizer::new(LongPressOptions {
                base,
                ..LongPressOptions::default()
            });
            let listen = this.clone();
            recognizer.add_listener(move |press| {
                if let Some(core) = listen.upgrade() {
                    core.on_press(press.any());
                }
            });
            ContextRequestCore {
                options,
                recognizer,
                held: Cell::new(None),
                is_hold_reported: Cell::new(false),
                last_request_key_at: Cell::new(f64::NEG_INFINITY),
                last_pen_hold_end: Cell::new(f64::NEG_INFINITY),
                host: host.map(GestureHost::downgrade),
            }
        })
    }

    /// The hold's recognizer, to attach (or to drive in tests).
    pub fn recognizer(&self) -> &LongPressGestureRecognizer {
        &self.recognizer
    }

    fn is_enabled(&self) -> bool {
        self.options
            .is_enabled
            .as_ref()
            .is_none_or(|enabled| enabled())
    }

    fn keeps_press_under(&self, r: &AnyRecognizer, touch: &GestureTouch) -> bool {
        let (Some(host), Some(view)) = (
            self.host.as_ref().and_then(GestureHost::from_weak),
            r.view(),
        ) else {
            return false;
        };
        host.keeps_press_between(touch.target(), view)
    }

    fn on_press(&self, press: &AnyRecognizer) {
        match press.state() {
            GestureState::Began => {
                // A request disabled during the hold makes none, and its hold
                // reports no phase.
                if !self.is_enabled() {
                    return;
                }
                self.is_hold_reported.set(true);
                if let Some(on_hold) = &self.options.on_hold {
                    on_hold(HoldPhase::Committed);
                }
                let point = press.location();
                let held = self.held.get();
                (self.options.on_request)(&ContextRequest {
                    kind: ContextRequestKind::Hold,
                    pointer_type: Some(held.map_or(PointerKind::Touch, |(_, kind)| kind)),
                    point: Some(point),
                    pointer_id: held.map(|(id, _)| id),
                    time: press.now(),
                });
            }
            GestureState::Ended | GestureState::Cancelled => {
                if self
                    .held
                    .get()
                    .is_some_and(|(_, kind)| kind == PointerKind::Pen)
                {
                    // A pen's own menu event at its hold's lift is that hold's.
                    self.last_pen_hold_end.set(press.now());
                }
                if self.is_hold_reported.get()
                    && let Some(on_hold) = &self.options.on_hold
                {
                    on_hold(if press.state() == GestureState::Ended {
                        HoldPhase::Ended
                    } else {
                        HoldPhase::Cancelled
                    });
                }
                self.is_hold_reported.set(false);
                self.held.set(None);
            }
            _ => {}
        }
    }

    /// A pointer's context click at `point`: requests, unless disabled or a
    /// request key's echo. Returns whether it answered.
    pub fn pointer_request(&self, point: Point, pointer: PointerKind, time: f64) -> bool {
        if !self.is_enabled() {
            return false;
        }
        // A held finger's menu is the hold's to request; a key's menu event
        // is that key's echo; a pen's own menu is its hold's, during it or
        // right after its lift.
        let pen_down = self.recognizer.tracked_touches().iter().any(|t| {
            t.pointer_type() == PointerKind::Pen
                && !matches!(
                    t.phase(),
                    super::types::TouchPhase::Ended | super::types::TouchPhase::Cancelled
                )
        });
        let host_pen = self
            .host
            .as_ref()
            .and_then(GestureHost::from_weak)
            .zip(self.recognizer.view())
            .is_some_and(|(host, view)| {
                host.is_pen_down_in(view) || time - host.last_pen_lift_in(view) < MENU_ECHO_MS
            });
        if pointer == PointerKind::Touch
            || time - self.last_request_key_at.get() < MENU_ECHO_MS
            || (pointer == PointerKind::Pen
                && (pen_down || host_pen || time - self.last_pen_hold_end.get() < MENU_ECHO_MS))
        {
            return true;
        }
        (self.options.on_request)(&ContextRequest {
            kind: ContextRequestKind::Pointer,
            pointer_type: Some(pointer),
            point: Some(point),
            pointer_id: None,
            time,
        });
        true
    }

    /// Shift+F10 or the context-menu key. A held key's repeats keep the echo
    /// window open and make no request. Returns whether it answered.
    pub fn key_request(&self, repeat: bool, time: f64) -> bool {
        if !self.is_enabled() {
            return false;
        }
        self.last_request_key_at.set(time);
        if repeat {
            return true;
        }
        (self.options.on_request)(&ContextRequest {
            kind: ContextRequestKind::Keyboard,
            pointer_type: None,
            point: None,
            pointer_id: None,
            time,
        });
        true
    }
}

struct Handler(Rc<ContextRequestCore>);

impl ContextHandler for Handler {
    fn pointer(&self, point: Point, pointer: PointerKind, time: f64) -> bool {
        self.0.pointer_request(point, pointer, time)
    }
    fn key(&self, repeat: bool, time: f64) -> bool {
        self.0.key_request(repeat, time)
    }
    fn is_enabled(&self) -> bool {
        self.0.is_enabled()
    }
}

/// A context request attached to an area; detaches when dropped.
pub struct ContextRequestAttachment {
    attachment: GestureAttachment,
    core: Rc<ContextRequestCore>,
}

impl ContextRequestAttachment {
    /// The request's logic.
    pub fn core(&self) -> &Rc<ContextRequestCore> {
        &self.core
    }
}

impl Drop for ContextRequestAttachment {
    fn drop(&mut self) {
        self.attachment
            .remove_recognizer(self.core.recognizer.any());
        *self.attachment.inner().context.borrow_mut() = None;
    }
}

/// Reports context requests on `attachment`'s area (upstream
/// `attachContextRequest`): its hold joins the area's recognizers, and the
/// host routes the area's context clicks and, while focus is inside it
/// (`GestureAttachment::set_focus`), its context-menu keys.
pub fn attach_context_request(
    host: &GestureHost,
    attachment: &GestureAttachment,
    options: ContextRequestOptions,
) -> ContextRequestAttachment {
    let core = ContextRequestCore::new(options, Some(host));
    attachment.add_recognizer(core.recognizer.any());
    *attachment.inner().context.borrow_mut() = Some(Rc::new(Handler(core.clone())));
    ContextRequestAttachment {
        attachment: attachment.clone(),
        core,
    }
}
