//! The stack of bottom sheets, upstream `BottomSheet/bottomSheetStack.ts` (FR5-FR13,
//! AR3-AR5): every presented sheet of a window in open order, one stack of modal
//! sheets and one group of non-modal ones.
//!
//! - **Order (FR5).** A sheet joins at the top when it opens; sheets opening in one
//!   frame join in render order, which is tree order (a sheet before the sheets
//!   rendered in its content, siblings in source order). A sheet reopened while it
//!   exits goes back on top.
//! - **Recede and block (FR6, AR3).** A covered modal sheet recedes a step per open
//!   level above it (two at most) and is blocked: below the top sheet's layer, and
//!   inert when it hosts no open sheet. Exactly one backdrop dims the page: the
//!   bottom open sheet's, a change of owner landing with no transition.
//! - **Replace (FR10, FR12).** When one sheet closes and one opens in the same frame,
//!   of one modality, the closing one the top of its group, neither hosting the other,
//!   and the press or focus that did it inside the closing sheet (or nowhere), the
//!   opening sheet replaces the closing one: it enters above the retained sheet, a
//!   taller retained sheet aligns down to meet it, and the retained sheet fades only
//!   after both motions; it is inert from the first frame and takes no dismissal, and
//!   the entering sheet adopts its focus-return target. A BottomSheetSwitcher flow is
//!   one level, and each of its own step changes is such a handoff.
//! - **Focus (FR9, AR4).** A closing top sheet returns focus into the sheet it
//!   reveals: to its opener when that is inside the revealed sheet, else to the
//!   revealed sheet's surface. Sheets closing together return focus once, to the
//!   bottom one's opener, after the last has left.
//!
//! The pairing of a replace is read once the frame that made the changes has been
//! drawn: until then a closing sheet holds its open pose, so a replace never starts
//! an exit it would have to reverse.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui::{App, FocusHandle, Global, SharedString, Window, WindowId};
use web_time::Instant;

/// The most steps a covered sheet recedes.
pub(crate) const MAX_RECEDE_STEPS: usize = 2;

/// What a sheet tells the stack each frame it renders.
#[derive(Clone)]
pub(crate) struct Report {
    pub key: String,
    pub modal: bool,
    /// Its effective open state (its own, gated by its host's).
    pub open: bool,
    /// The BottomSheetSwitcher flow it is a step of.
    pub flow: Option<SharedString>,
    /// The sheet whose content renders it.
    pub host: Option<String>,
    pub surface: FocusHandle,
    /// Where focus returns when it leaves.
    pub opener: Option<FocusHandle>,
    /// A control the owner named to take focus when it leaves (upstream
    /// `finalFocusRef` set to a control the sheet cannot hold a handle to).
    pub final_named: Option<gpui::ElementId>,
    /// Whether focus was inside it when it began to close (a non-modal sheet returns
    /// focus only then).
    pub had_focus: bool,
    /// Where its panel was drawn last frame.
    pub bounds: Option<gpui::Bounds<gpui::Pixels>>,
    /// Its resting top edge, px from the window's top.
    pub top_edge: f32,
    /// The theme's `--duration-medium` and Reduce Motion, for the handoff.
    pub duration: Duration,
    pub reduced: bool,
    pub coarse: bool,
    /// Alive while the sheet renders.
    pub alive: std::rc::Weak<()>,
}

/// A retained sheet's handoff motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Retained {
    /// How far it has moved down to meet the entering sheet, px.
    pub alignment: f32,
    /// Its opacity: 1 until both motions end, then fading.
    pub opacity: f32,
    /// The fade ended: the sheet leaves.
    pub done: bool,
}

/// A sheet's place in the stack (upstream `BottomSheetStackView`).
#[derive(Clone, Debug, Default)]
pub(crate) struct View {
    /// Open levels above it (0 on top).
    pub depth: usize,
    pub is_top: bool,
    /// More than one modal sheet is presented.
    pub is_stacked: bool,
    /// Its backdrop is the one visible scrim.
    pub owns_scrim: bool,
    /// Covered with no open sheet in its content: inert and hidden (AR3).
    pub blocked: bool,
    /// Its place in the stack, for its layer's depth.
    pub rank: usize,
    /// Replaced, through its handoff.
    pub retained: Option<Retained>,
    /// The focus-return target it adopted from the sheet it replaced.
    pub adopted_opener: Option<FocusHandle>,
}

#[derive(Clone)]
struct Entry {
    report: Report,
    /// The frame's open state as the stack last saw it.
    open: bool,
    retained_since: Option<(Instant, f32)>,
    adopted_opener: Option<FocusHandle>,
    /// Closed in a batch with these other sheets.
    closed_with: Vec<String>,
    /// Whether it was the top of its group when it closed.
    was_top: bool,
}

#[derive(Clone)]
struct Flip {
    key: String,
    open: bool,
}

/// Who acted on a flip (upstream `batchActor`): the control last pressed, or the
/// element focused.
#[derive(Clone)]
pub(crate) enum Actor {
    /// A pointer or finger pressed here.
    At(gpui::Point<gpui::Pixels>),
    /// A key went down, or nothing was pressed recently, with this focused.
    Focus(FocusHandle),
    /// Nothing was pressed recently and nothing is focused.
    Nobody,
}

/// A press is the actor of a flip only while it is fresh: the handler it runs flips
/// the sheets within the same task (upstream `PRESS_ACTOR_WINDOW_MS`).
const PRESS_ACTOR_WINDOW: Duration = Duration::from_millis(1000);

#[derive(Default)]
struct WindowSheets {
    /// The last press, key or click in the window while sheets are presented.
    last_press: Option<(Actor, Instant)>,
    /// Who acted on the batch of flips in progress.
    batch_actor: Option<Actor>,
    entries: Vec<Entry>,
    batch: Vec<Flip>,
    resolving: bool,
    /// Keys whose focus return is the stack's own, at their exit's end.
    together: Vec<Vec<String>>,
}

#[derive(Default)]
struct Stacks(HashMap<WindowId, Rc<RefCell<WindowSheets>>>);

impl Global for Stacks {}

fn sheets(window: &Window, cx: &mut App) -> Rc<RefCell<WindowSheets>> {
    cx.default_global::<Stacks>()
        .0
        .entry(window.window_handle().window_id())
        .or_default()
        .clone()
}

impl WindowSheets {
    fn index(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.report.key == key)
    }

    /// The key of a level: its flow's for a switcher step.
    fn level(entry: &Entry) -> String {
        entry
            .report
            .flow
            .as_ref()
            .map_or_else(|| entry.report.key.clone(), |f| format!("flow:{f}"))
    }

    /// Whether `key`'s host chain includes `host`.
    fn hosted_by(&self, key: &str, host: &str) -> bool {
        let mut at = self
            .index(key)
            .and_then(|i| self.entries[i].report.host.clone());
        let mut guard = 0;
        while let Some(h) = at {
            if h == host {
                return true;
            }
            guard += 1;
            if guard > 32 {
                return false;
            }
            at = self
                .index(&h)
                .and_then(|i| self.entries[i].report.host.clone());
        }
        false
    }

    /// The top open entry of a modality, `except` one key.
    fn top_open(&self, modal: bool, except: Option<&str>) -> Option<&Entry> {
        self.entries
            .iter()
            .rev()
            .filter(|e| e.report.modal == modal && e.open && e.retained_since.is_none())
            .find(|e| Some(e.report.key.as_str()) != except)
    }

    /// The topmost presented sheet the actor's press landed in.
    fn owner_of(&self, actor: &Actor, window: &Window) -> Option<usize> {
        match actor {
            Actor::At(point) => self
                .entries
                .iter()
                .rposition(|e| e.report.bounds.is_some_and(|b| b.contains(point))),
            Actor::Focus(handle) => self.entries.iter().rposition(|e| {
                e.report.surface == *handle || e.report.surface.contains(handle, window)
            }),
            Actor::Nobody => None,
        }
    }

    /// Who acted on a flip of `flipping` (upstream `actorFor`): a fresh press that is
    /// not spent (it landed in a sheet already closed or replaced before this flip),
    /// else what has focus.
    fn actor_for(&self, flipping: &str, now: Instant, window: &Window, cx: &App) -> Actor {
        if let Some((press, at)) = &self.last_press
            && now.saturating_duration_since(*at) <= PRESS_ACTOR_WINDOW
        {
            let spent = self.owner_of(press, window).is_some_and(|i| {
                let owner = &self.entries[i];
                owner.report.key != flipping && (!owner.open || owner.retained_since.is_some())
            });
            if !spent {
                return press.clone();
            }
        }
        window.focused(cx).map_or(Actor::Nobody, Actor::Focus)
    }

    /// Whether the actor is inside the entry's sheet, or nobody acted.
    fn acted_inside(&self, actor: &Actor, entry: &Entry, window: &Window) -> bool {
        match actor {
            Actor::Nobody => true,
            Actor::At(point) => entry.report.bounds.is_some_and(|b| b.contains(point)),
            Actor::Focus(handle) => {
                entry.report.surface == *handle || entry.report.surface.contains(handle, window)
            }
        }
    }

    fn view(&self, key: &str, now: Instant) -> View {
        let Some(index) = self.index(key) else {
            return View::default();
        };
        let entry = &self.entries[index];
        let mut view = View {
            rank: index,
            adopted_opener: entry.adopted_opener.clone(),
            ..Default::default()
        };
        if !entry.report.modal && entry.retained_since.is_none() {
            view.is_top = entry.open;
            return view;
        }
        let modal: Vec<&Entry> = self.entries.iter().filter(|e| e.report.modal).collect();
        view.is_stacked = modal.len() > 1;
        // The bottom presented sheet that is open or retained owns the one scrim.
        view.owns_scrim = modal
            .iter()
            .find(|e| e.open || e.retained_since.is_some())
            .is_some_and(|e| e.report.key == key);
        if let Some((since, alignment)) = entry.retained_since {
            let duration = entry.report.duration.as_secs_f32();
            let elapsed = now.saturating_duration_since(since).as_secs_f32();
            let reduced = entry.report.reduced;
            let fade = if reduced {
                if entry.report.coarse { 0.2 } else { 0. }
            } else {
                duration
            };
            // The entering sheet's entrance and the alignment run together; the
            // fade waits for both.
            let first = if reduced { 0. } else { duration };
            let align = if reduced || first == 0. {
                0.
            } else {
                alignment * ease((elapsed / first).min(1.))
            };
            let fading = (elapsed - first).max(0.);
            let opacity = if fade == 0. {
                if elapsed >= first { 0. } else { 1. }
            } else {
                1. - (fading / fade).min(1.)
            };
            view.retained = Some(Retained {
                alignment: if reduced { 0. } else { align },
                opacity,
                done: elapsed >= first + fade,
            });
            return view;
        }
        if !entry.open {
            return view;
        }
        // Open levels above it, a flow counted once.
        let mut levels: Vec<String> = Vec::new();
        for e in self.entries.iter().skip(index + 1) {
            if e.report.modal && e.open && e.retained_since.is_none() {
                let level = Self::level(e);
                if level != Self::level(entry) && !levels.contains(&level) {
                    levels.push(level);
                }
            }
        }
        view.depth = levels.len();
        view.is_top = view.depth == 0;
        let hosts_open = self
            .entries
            .iter()
            .any(|e| e.open && e.report.key != key && self.hosted_by(&e.report.key, key));
        view.blocked = view.depth > 0 && !hosts_open;
        view
    }
}

/// Eases a handoff's move: the theme's standard curve.
fn ease(t: f32) -> f32 {
    // cubic-bezier(0.2, 0, 0, 1), sampled closed-form enough for a 16 ms frame.
    let inv = 1. - t;
    1. - inv * inv * inv
}

/// Tells the stack where a rendering sheet is and returns its place. Call it each
/// frame the sheet is presented.
pub(crate) fn sync(report: Report, window: &mut Window, cx: &mut App) -> View {
    let stack = sheets(window, cx);
    let now = cx.background_executor().now();
    let mut flipped = false;
    {
        let mut s = stack.borrow_mut();
        // A sheet that stopped rendering (an unmount, or its host left) is gone.
        s.entries.retain(|e| e.report.alive.strong_count() > 0);
        // The first flip of a batch reads who acted (upstream `recordFlip`).
        let flips = match s.index(&report.key) {
            None => report.open,
            Some(i) => s.entries[i].open != report.open,
        };
        if flips && s.batch.is_empty() {
            let actor = s.actor_for(&report.key, now, window, cx);
            s.batch_actor = Some(actor);
        }
        match s.index(&report.key) {
            None => {
                if report.open {
                    s.entries.push(Entry {
                        open: true,
                        report: report.clone(),
                        retained_since: None,
                        adopted_opener: None,
                        closed_with: Vec::new(),
                        was_top: false,
                    });
                    s.batch.push(Flip {
                        key: report.key.clone(),
                        open: true,
                    });
                    flipped = true;
                }
            }
            Some(i) => {
                let was = s.entries[i].open;
                if report.open && !was {
                    // Reopened while it left: back on top.
                    let mut entry = s.entries.remove(i);
                    entry.open = true;
                    entry.retained_since = None;
                    entry.closed_with.clear();
                    entry.report = report.clone();
                    s.entries.push(entry);
                    s.batch.push(Flip {
                        key: report.key.clone(),
                        open: true,
                    });
                    flipped = true;
                } else if !report.open && was {
                    let top = s
                        .top_open(report.modal, None)
                        .is_some_and(|e| e.report.key == report.key);
                    let entry = &mut s.entries[i];
                    entry.open = false;
                    entry.was_top = top;
                    entry.report = report.clone();
                    s.batch.push(Flip {
                        key: report.key.clone(),
                        open: false,
                    });
                    flipped = true;
                } else {
                    s.entries[i].report = report.clone();
                }
            }
        }
    }
    if flipped && !stack.borrow().resolving {
        stack.borrow_mut().resolving = true;
        let weak = Rc::downgrade(&stack);
        window.defer(cx, move |window, cx| {
            if let Some(stack) = weak.upgrade() {
                resolve(&stack, window, cx);
            }
            window.refresh();
        });
    }
    let view = stack.borrow().view(&report.key, now);
    if view.retained.is_some() {
        window.request_animation_frame();
    }
    view
}

/// Pairs the frame's opens and closes: a replace, or sheets that closed together.
fn resolve(stack: &Rc<RefCell<WindowSheets>>, window: &Window, cx: &mut App) {
    let now = cx.background_executor().now();
    let mut s = stack.borrow_mut();
    s.resolving = false;
    let batch = std::mem::take(&mut s.batch);
    let actor = s.batch_actor.take().unwrap_or(Actor::Nobody);
    let opens: Vec<&Flip> = batch.iter().filter(|f| f.open).collect();
    let closes: Vec<&Flip> = batch.iter().filter(|f| !f.open).collect();
    if let ([open], [close]) = (opens.as_slice(), closes.as_slice()) {
        let (Some(oi), Some(ci)) = (s.index(&open.key), s.index(&close.key)) else {
            return;
        };
        let (opening, closing) = (s.entries[oi].clone(), s.entries[ci].clone());
        let same_flow = opening.report.flow.is_some() && opening.report.flow == closing.report.flow;
        let either_flow = opening.report.flow.is_some() || closing.report.flow.is_some();
        // The top of its group with the opening sheet set aside.
        let was_top = !s.entries.iter().skip(ci + 1).any(|e| {
            e.report.key != open.key
                && e.report.modal == closing.report.modal
                && e.open
                && e.retained_since.is_none()
        });
        let pairs = same_flow
            || (!either_flow
                && opening.report.modal == closing.report.modal
                && was_top
                && !s.hosted_by(&open.key, &close.key)
                && !s.hosted_by(&close.key, &open.key)
                && s.acted_inside(&actor, &closing, window));
        if pairs && !closing.open && opening.open {
            let alignment = (opening.report.top_edge - closing.report.top_edge).max(0.);
            let entry = &mut s.entries[ci];
            entry.retained_since = Some((now, alignment));
            let opener = entry.report.opener.clone();
            let entry = &mut s.entries[oi];
            entry.adopted_opener = opener;
            // The press is spent: a later flip reads focus.
            s.last_press = None;
            return;
        }
    }
    let closed: Vec<String> = closes.iter().map(|f| f.key.clone()).collect();
    if closed.len() > 1 {
        for key in &closed {
            if let Some(i) = s.index(key) {
                s.entries[i].closed_with = closed.clone();
            }
        }
        s.together.push(closed);
    }
}

/// Where focus goes when the sheet `key` finishes leaving (not replaced): the
/// revealed sheet's surface or the opener (FR9). `None` moves nothing: a covered
/// sheet left from code under an open one, or one of several closing together that
/// is not the last to leave.
pub(crate) enum FocusReturn {
    /// Leave focus where it is.
    Keep,
    /// Focus this.
    To(FocusHandle),
    /// The control the owner named takes focus when it next renders.
    Named(gpui::ElementId),
}

/// The sheet `key` finished leaving: it leaves the stack, and says where focus goes.
pub(crate) fn leave(key: &str, window: &mut Window, cx: &mut App) -> FocusReturn {
    let stack = sheets(window, cx);
    let mut s = stack.borrow_mut();
    let Some(i) = s.index(key) else {
        return FocusReturn::Keep;
    };
    let entry = s.entries.remove(i);
    if entry.open || entry.retained_since.is_some() {
        // Removed while open (an unmount), or a replaced sheet: the stack's handoff
        // moved focus already.
        if entry.retained_since.is_some() {
            return FocusReturn::Keep;
        }
    }
    if !entry.report.modal {
        // A non-modal sheet leaves the page live: it returns focus to its opener only
        // if focus was still in it when it closed (upstream `hadFocusAtCloseRef`); a
        // named target always takes it.
        if let Some(named) = entry.report.final_named {
            return FocusReturn::Named(named);
        }
        return entry
            .report
            .opener
            .filter(|_| entry.report.had_focus)
            .map_or(FocusReturn::Keep, FocusReturn::To);
    }
    // A covered sheet closed from code under open sheets moves nothing.
    if !entry.was_top && s.entries.iter().skip(i).any(|e| e.report.modal && e.open) {
        return FocusReturn::Keep;
    }
    // Closing together: once, after the last, to the bottom one's opener.
    if !entry.closed_with.is_empty() {
        let group = entry.closed_with.clone();
        let still_leaving = group.iter().any(|k| k != key && s.index(k).is_some());
        let slot = s.together.iter().position(|g| *g == group);
        // The bottom of the group is the first that closed, its opener kept.
        let bottom = slot.map(|_| entry.clone());
        if still_leaving {
            // Hand the bottom's opener on to whoever leaves last.
            if let Some(next) = group.iter().find(|k| *k != key && s.index(k).is_some())
                && let Some(ni) = s.index(next)
                && i <= ni
            {
                s.entries[ni].report.opener = entry.report.opener.clone();
            }
            return FocusReturn::Keep;
        }
        if let Some(slot) = slot {
            s.together.remove(slot);
        }
        return bottom
            .and_then(|e| e.report.opener)
            .map_or(FocusReturn::Keep, FocusReturn::To);
    }
    let revealed = s
        .entries
        .iter()
        .rev()
        .find(|e| e.report.modal && e.open && e.retained_since.is_none())
        .map(|e| e.report.surface.clone());
    // A named control is the owner's choice of where focus goes; it lives in the
    // flow it names it from, the revealed sheet when there is one.
    if let Some(named) = entry.report.final_named {
        return FocusReturn::Named(named);
    }
    match (revealed, entry.report.opener) {
        (Some(revealed), Some(opener)) if revealed.contains(&opener, window) => {
            FocusReturn::To(opener)
        }
        (Some(revealed), _) => FocusReturn::To(revealed),
        (None, Some(opener)) => FocusReturn::To(opener),
        (None, None) => FocusReturn::Keep,
    }
}

/// Notes a press, click or key in the window (upstream `listenForPresses`): the sheets
/// read it as who acted when they flip. Each presented sheet listens.
pub(crate) fn record_press(actor: Actor, window: &Window, cx: &mut App) {
    let now = cx.background_executor().now();
    let stack = sheets(window, cx);
    stack.borrow_mut().last_press = Some((actor, now));
}
