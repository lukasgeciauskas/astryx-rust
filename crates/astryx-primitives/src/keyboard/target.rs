//! Upstream `keyboard/keyboardTarget.ts`: a covered field's own path
//! (`keepFieldAboveKeyboard`), its reveal through the nearest scroller, and the scroll
//! back once the keyboard has gone (AST-099 FR9's scrolling content).
//!
//! Every Core field registers with the keyboard module as it draws (the text editor does),
//! so the path runs for each with nothing for the app to turn on. It runs once per window
//! under a coarse primary pointer ([`super::follows_keyboard`]); under a mouse or a
//! trackpad nothing attaches. A field inside a keyboard target (a sheet, a chat
//! composer's dock) moves nothing itself: the target keeps it clear.
//!
//! GPUI has no CSS `position: fixed`: a window's layout is its own, so there is no app
//! container docked at the layout viewport's bottom for the path to ride, and no
//! browser reveal to refuse (GPUI's web build focuses its hidden input with
//! `preventScroll`). A field's scrollers are the ones its content marks
//! ([`super::push_scroller`]), the page's `ScrollHost` being the document.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{App, FocusHandle, Global, ScrollHandle, Task, Window, WindowId, point, px};

use super::band::distance_into_band;
use super::height::{
    FocusTarget, KEYBOARD_EPSILON, KeyboardField, KeyboardHeightEvent, KeyboardSubscription,
    focused_field, get_keyboard_height, is_finger_on_screen, layout_viewport_height,
    subscribe_keyboard_height, touch_starts,
};
use super::pan::{PanWatch, pan_visual_viewport_to};
use super::{FIELD_OFFSET, follows_keyboard};
use crate::motion::cubic_bezier;

/// How long the browser's own smooth scroll takes here, ms, and its curve: the scroll back
/// rides it (upstream leaves both to the browser; this is CSS's `ease-in-out`).
pub const SMOOTH_SCROLL_MS: f64 = 300.;
const SMOOTH_SCROLL_CURVE: [f64; 4] = [0.42, 0., 0.58, 1.];

/// How far `handle` is scrolled down, px (GPUI's offsets are negative).
pub fn scroll_top_of(handle: &ScrollHandle) -> f64 {
    -f64::from(handle.offset().y)
}

/// How far `handle` can scroll down, px.
pub fn scroll_range_of(handle: &ScrollHandle) -> f64 {
    f64::from(handle.max_offset().y).max(0.)
}

/// Scrolls `handle` to `top` at once.
pub fn scroll_top_to(handle: &ScrollHandle, top: f64) {
    let offset = handle.offset();
    handle.set_offset(point(offset.x, px(-(top as f32))));
}

/// A scroll on the browser's smooth scroll: `handle` heads to `top` over
/// [`SMOOTH_SCROLL_MS`]. A scroll anyone else makes meanwhile stops it, so a person's
/// scroll stands. Dropping the task stops it where it is.
pub fn smooth_scroll(
    handle: &ScrollHandle,
    top: f64,
    window: &mut Window,
    cx: &mut App,
) -> Task<()> {
    let handle = handle.clone();
    let from = scroll_top_of(&handle);
    let start = cx.background_executor().now();
    window.spawn(cx, async move |cx| {
        let ease = cubic_bezier(SMOOTH_SCROLL_CURVE);
        let mut last = from;
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let Ok(done) = cx.update(|window, cx| {
                if (scroll_top_of(&handle) - last).abs() > KEYBOARD_EPSILON {
                    return true;
                }
                let elapsed = cx
                    .background_executor()
                    .now()
                    .saturating_duration_since(start)
                    .as_secs_f64()
                    * 1000.;
                let progress = (elapsed / SMOOTH_SCROLL_MS).min(1.);
                last = from + (top - from) * ease.ease(progress);
                scroll_top_to(&handle, last);
                window.refresh();
                progress >= 1.
            }) else {
                return;
            };
            if done {
                return;
            }
        }
    })
}

/// A scroll Core made for the keyboard (upstream `AddedScroll`).
struct AddedScroll {
    handle: ScrollHandle,
    /// Where the scroller was before Core's first scroll.
    from: f64,
    /// Where Core left it, or `None` once anyone else scrolled it.
    at: Option<f64>,
}

/// Who reveals a field its scroller cannot place (upstream `UnplacedField`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unplaced {
    /// The document, then a pan.
    Document,
    /// Nobody: the scroller holds every field it reveals.
    Keep,
}

/// Who moves a field that gains focus (upstream `placementOf`).
#[derive(Clone)]
struct Placement {
    scroller: ScrollHandle,
    /// The scroller's box, or `None` for the document.
    bounds: Option<gpui::Bounds<gpui::Pixels>>,
    unplaced: Unplaced,
}

/// The placement of `field` when it gains focus: its nearest scroller reveals it, the
/// document a field its scroll area cannot place. `None` keeps the platform's own
/// reveal: inside a keyboard target, or on a page with nothing to scroll.
fn placement_of(field: &KeyboardField) -> Option<Placement> {
    if field.is_inside_target() || !field.keeps_itself {
        return None;
    }
    match (&field.scroller, &field.document) {
        (Some((handle, bounds)), document) => Some(Placement {
            scroller: handle.clone(),
            bounds: Some(*bounds),
            unplaced: if document.is_some() {
                Unplaced::Document
            } else {
                Unplaced::Keep
            },
        }),
        (None, Some(document)) if scroll_range_of(document) > 0. => Some(Placement {
            scroller: document.clone(),
            bounds: None,
            unplaced: Unplaced::Keep,
        }),
        _ => None,
    }
}

struct Session {
    focus: FocusHandle,
    placement: Placement,
    is_placed: bool,
    has_revealed: bool,
    starts_at_reveal: u64,
}

struct Keeper {
    session: Option<Session>,
    last_height: f64,
    scrolls: Vec<AddedScroll>,
    scroll_backs: Vec<Task<()>>,
    pans: Option<PanWatch>,
    subscription: Option<KeyboardSubscription>,
}

#[derive(Default)]
struct Keepers(std::collections::HashMap<WindowId, Rc<RefCell<Keeper>>>);

impl Global for Keepers {}

/// Upstream `keepFieldAboveKeyboard`, for every Core field of the window: attached while
/// the window follows the keyboard (a coarse primary pointer), detached under a mouse.
/// The keyboard module calls it each frame a field keeping itself draws.
pub(crate) fn sync_keeper(window: &mut Window, cx: &mut App) {
    let follows = follows_keyboard(window, cx);
    let id = window.window_handle().window_id();
    let existing = cx
        .try_global::<Keepers>()
        .and_then(|k| k.0.get(&id).cloned());
    match (follows, existing) {
        (true, Some(_)) | (false, None) => {}
        (false, Some(keeper)) => {
            let detached = {
                let mut k = keeper.borrow_mut();
                k.session = None;
                k.pans = None;
                k.subscription.take()
            };
            drop(detached);
            cx.default_global::<Keepers>().0.remove(&id);
        }
        (true, None) => {
            let height = get_keyboard_height(window, cx);
            let keeper = Rc::new(RefCell::new(Keeper {
                session: None,
                last_height: height,
                scrolls: Vec::new(),
                scroll_backs: Vec::new(),
                pans: None,
                subscription: None,
            }));
            let session_owns = Rc::downgrade(&keeper);
            keeper.borrow_mut().pans = Some(PanWatch::new(height, move |field| {
                session_owns.upgrade().is_some_and(|k| {
                    k.borrow()
                        .session
                        .as_ref()
                        .is_some_and(|s| s.focus == field.focus && s.is_placed)
                })
            }));
            let weak = Rc::downgrade(&keeper);
            let subscription =
                subscribe_keyboard_height(window, cx, true, move |event, window, cx| {
                    if let Some(keeper) = weak.upgrade() {
                        on_event(&keeper, event, window, cx);
                    }
                });
            keeper.borrow_mut().subscription = Some(subscription);
            cx.default_global::<Keepers>().0.insert(id, keeper);
            // A field that has focus before the path attaches gets no later focus event.
            if let Some(field) = focused_field(window, cx) {
                let keeper = cx.default_global::<Keepers>().0.get(&id).cloned();
                if let Some(keeper) = keeper {
                    begin(&keeper, &field, window, cx);
                }
            }
        }
    }
}

fn begin(keeper: &Rc<RefCell<Keeper>>, field: &KeyboardField, window: &mut Window, cx: &mut App) {
    if !field.kind.text_entry {
        return;
    }
    let Some(placement) = placement_of(field) else {
        return;
    };
    // A field that gains focus during a scroll back stops it where the page is
    // (upstream `stopKeyboardScrollBack`).
    keeper.borrow_mut().scroll_backs.clear();
    keeper.borrow_mut().session = Some(Session {
        focus: field.focus.clone(),
        placement,
        is_placed: true,
        has_revealed: false,
        starts_at_reveal: 0,
    });
    let height = get_keyboard_height(window, cx);
    reveal_now(keeper, height, window, cx);
}

fn on_event(
    keeper: &Rc<RefCell<Keeper>>,
    event: &KeyboardHeightEvent,
    window: &mut Window,
    cx: &mut App,
) {
    match event {
        KeyboardHeightEvent::FocusIn { target, .. } => {
            if let FocusTarget::Field(field) = target {
                begin(keeper, field, window, cx);
            }
        }
        KeyboardHeightEvent::FocusOut { target, .. } => {
            let ends = target.field().is_some_and(|field| {
                keeper
                    .borrow()
                    .session
                    .as_ref()
                    .is_some_and(|s| s.focus == field.focus)
            });
            if ends {
                keeper.borrow_mut().session = None;
            }
        }
        KeyboardHeightEvent::Viewport { height, .. } => {
            let height = *height;
            let changed = (height - keeper.borrow().last_height).abs() > KEYBOARD_EPSILON;
            if changed {
                keeper.borrow_mut().last_height = height;
                // After the show's first reveal, a report during a press that began after
                // that reveal moves nothing (a toolbar hiding in a drag).
                let later_press = keeper.borrow().session.as_ref().is_some_and(|s| {
                    s.has_revealed
                        && is_finger_on_screen(window, cx)
                        && touch_starts(window, cx) != s.starts_at_reveal
                });
                if !later_press {
                    reveal_now(keeper, height, window, cx);
                }
            }
            let pans = keeper.borrow_mut().pans.take();
            if let Some(mut pans) = pans {
                pans.report(height, window, cx);
                keeper.borrow_mut().pans = Some(pans);
            }
            if changed && height <= KEYBOARD_EPSILON {
                scroll_back(keeper, window, cx);
            }
        }
    }
}

/// The band a field must be in: inside its scroller's box and the viewport, `offset`
/// above the keyboard while one shows.
fn band_of(placement: &Placement, height: f64, offset: f64, layout_height: f64) -> (f64, f64) {
    let keyboard_top = layout_height - height;
    let mut top = 0.;
    let mut bottom = if height > KEYBOARD_EPSILON {
        keyboard_top - offset
    } else {
        keyboard_top
    };
    if let Some(bounds) = placement.bounds {
        top = f64::from(bounds.top()).max(top);
        bottom = f64::from(bounds.bottom()).min(bottom);
    }
    (top, bottom)
}

/// Scrolls the field into its band through `placement`'s scroller; returns the part of
/// the distance it could not scroll.
fn reveal_through(
    keeper: &Rc<RefCell<Keeper>>,
    field: &KeyboardField,
    placement: &Placement,
    height: f64,
    layout_height: f64,
) -> f64 {
    let (top, bottom) = band_of(placement, height, FIELD_OFFSET, layout_height);
    let distance = distance_into_band(
        f64::from(field.bounds.top()),
        f64::from(field.bounds.bottom()),
        top,
        bottom,
    );
    if distance.abs() <= KEYBOARD_EPSILON {
        return 0.;
    }
    let handle = &placement.scroller;
    let now = scroll_top_of(handle);
    let target = (now + distance).clamp(0., scroll_range_of(handle));
    scroll_top_to(handle, target);
    if height > KEYBOARD_EPSILON {
        note_keyboard_scroll(keeper, handle, now, target);
    }
    distance - (target - now)
}

fn reveal_now(keeper: &Rc<RefCell<Keeper>>, height: f64, window: &mut Window, cx: &mut App) {
    let Some(placement) = keeper
        .borrow()
        .session
        .as_ref()
        .map(|s| s.placement.clone())
    else {
        return;
    };
    let Some(field) = focused_field(window, cx) else {
        return;
    };
    if keeper
        .borrow()
        .session
        .as_ref()
        .is_none_or(|s| s.focus != field.focus)
    {
        return;
    }
    let starts = touch_starts(window, cx);
    if let Some(s) = keeper.borrow_mut().session.as_mut() {
        s.has_revealed = height > KEYBOARD_EPSILON;
        s.starts_at_reveal = starts;
    }
    let layout_height = layout_viewport_height(window);
    let mut left = reveal_through(keeper, &field, &placement, height, layout_height);
    if left.abs() > KEYBOARD_EPSILON
        && placement.unplaced == Unplaced::Document
        && height > KEYBOARD_EPSILON
        && let Some(document) = field.document.clone()
    {
        // The document reveals a field its scroll area cannot place.
        // The field already moved by the part its own scroller made.
        let done = distance_done(left, &field, &placement, height, layout_height);
        let shifted = KeyboardField {
            bounds: gpui::Bounds {
                origin: point(
                    field.bounds.origin.x,
                    field.bounds.origin.y - px(done as f32),
                ),
                size: field.bounds.size,
            },
            ..field.clone()
        };
        let document = Placement {
            scroller: document,
            bounds: None,
            unplaced: Unplaced::Keep,
        };
        left = reveal_through(keeper, &shifted, &document, height, layout_height);
    }
    // Where the document cannot scroll far enough, the visual viewport pans for the rest
    // in the same frame (upstream `panIntoView`, the web's room under the keyboard).
    let placed = left <= KEYBOARD_EPSILON;
    if !placed && height > KEYBOARD_EPSILON {
        let visual = window.visual_viewport_bounds();
        let viewport_top = f64::from(visual.origin.y);
        let room = layout_height - viewport_top - f64::from(visual.size.height);
        let pan = left
            .min(room)
            .min(f64::from(field.bounds.top()) - viewport_top);
        if pan > KEYBOARD_EPSILON {
            let top = viewport_top + pan;
            if let Some(pans) = keeper.borrow_mut().pans.as_mut() {
                pans.expect_pan(top);
            }
            pan_visual_viewport_to(top);
        }
    }
    if let Some(s) = keeper.borrow_mut().session.as_mut() {
        s.is_placed = placed;
    }
    window.refresh();
}

/// The part of a field's reveal its own scroller made, px.
fn distance_done(
    left: f64,
    field: &KeyboardField,
    placement: &Placement,
    height: f64,
    layout_height: f64,
) -> f64 {
    let (top, bottom) = band_of(placement, height, FIELD_OFFSET, layout_height);
    distance_into_band(
        f64::from(field.bounds.top()),
        f64::from(field.bounds.bottom()),
        top,
        bottom,
    ) - left
}

/// Upstream `noteKeyboardScroll`: records that Core scrolled `handle` from `before` to
/// `after` to keep a field above the keyboard. A scroller anyone else scrolls meanwhile
/// keeps that scroll.
fn note_keyboard_scroll(
    keeper: &Rc<RefCell<Keeper>>,
    handle: &ScrollHandle,
    before: f64,
    after: f64,
) {
    let mut k = keeper.borrow_mut();
    // GPUI's scroll handles have no identity to compare; a scroller is where it shows.
    match k
        .scrolls
        .iter_mut()
        .find(|s| s.handle.bounds() == handle.bounds())
    {
        None => {
            if (after - before).abs() >= 0.5 {
                k.scrolls.push(AddedScroll {
                    handle: handle.clone(),
                    from: before,
                    at: Some(after),
                });
            }
        }
        Some(added) => {
            added.at = match added.at {
                Some(at) if (before - at).abs() <= KEYBOARD_EPSILON => Some(after),
                _ => None,
            };
        }
    }
}

/// Once the keyboard reports it has gone, each scroller Core left where it put it goes
/// back to where it was before Core's first scroll, on the smooth scroll; under Reduce
/// Motion at once. A scroller the person scrolled meanwhile keeps the person's scroll.
fn scroll_back(keeper: &Rc<RefCell<Keeper>>, window: &mut Window, cx: &mut App) {
    let scrolls = std::mem::take(&mut keeper.borrow_mut().scrolls);
    let reduced = crate::prefers_reduced_motion(cx);
    let mut tasks = Vec::new();
    for added in scrolls {
        let Some(at) = added.at else { continue };
        if (scroll_top_of(&added.handle) - at).abs() > KEYBOARD_EPSILON {
            continue;
        }
        if reduced {
            scroll_top_to(&added.handle, added.from);
        } else {
            tasks.push(smooth_scroll(&added.handle, added.from, window, cx));
        }
    }
    keeper.borrow_mut().scroll_backs = tasks;
    window.refresh();
}
