//! A reorderable collection's session (upstream `List/listReorderEngine.tsx`):
//! its items as the last frame laid them out, a drag, reorder mode and the
//! announcements, with the handle, the overlay that draws the dragged item
//! and the insertion line, and reorder mode's hint.
//!
//! A collection calls [`begin`] once a frame; each of its items
//! [`register`]s as it renders and holds a handle; the collection's last
//! child is the [`overlay`], and it reports its children's bounds to
//! [`finalize`] once they are laid out. List does this for its rows, and
//! [`ReorderAttachment`](super::ReorderAttachment) for an app's own
//! elements.
//!
//! The drag's look follows upstream: the other items stay still until the
//! drop (or, with shift feedback, slide aside on `springs.settle` to open the
//! gap, [`super::slide`]); the source dims in place, and the item itself,
//! dimmed, follows the pointer as the preview (GPUI draws an element once a
//! frame, so the place keeps a copy of the row's text); an accent line marks
//! the candidate; and the scroller the collection sits in autoscrolls near its
//! edges. Under shift feedback or a finger, a drop, a cancel and a reorder-mode
//! move glide the rows into their new places; a mouse's drop behind the
//! insertion line lands at once, as upstream's does.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{Arg, current as i18n, use_direction};
use astryx_primitives::{
    LayerAlign, LayerOptions, Placement, Politeness, ScrollHost, anchored_layer, announce,
    is_focus_visible, prefers_reduced_motion, use_context,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, Context, CursorStyle, DispatchPhase, ElementId, Entity, FocusHandle,
    Hsla, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Point, RenderOnce, Role, ScrollHandle, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Task, Toggled, Window, canvas, div, point, px, relative,
};

use super::model::{
    AUTOSCROLL_TICK_MS, Arrow, INSERTION_LINE_THICKNESS, MOUSE_DRAG_SLOP, MOVING_OPACITY, Rect,
    ReorderBounds, ReorderGeometry, ReorderLayout, ReorderRow, arrow_target, autoscroll_step,
    bounds_of, clamp_top, drop_position, layout_of, line_center, mode_line_center, movable_range,
    nearest_slot, row_and_column, shift_candidate, shift_offset, slot_centers,
};
use super::slide::{SlideMotion, Slides};
use super::{FeedbackBehavior, ItemKey, ReorderMove, Reorderable, ReorderableLabels};
use crate::content::Kbd;

/// One item of the collection, as the last frame laid it out.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub key: Option<ItemKey>,
    pub name: Option<SharedString>,
    pub movable: bool,
    /// Its bounds in the window.
    pub bounds: Bounds<Pixels>,
}

/// An item telling the session about itself as it renders.
#[derive(Clone, Debug)]
pub(crate) struct Registration {
    /// Its place among the collection's children.
    pub index: usize,
    pub key: ItemKey,
    pub name: Option<SharedString>,
    pub movable: bool,
}

struct Armed {
    key: ItemKey,
    at: Point<Pixels>,
}

pub(crate) struct Drag {
    pub key: ItemKey,
    pub dragged: usize,
    /// Every item's box at pickup, in the collection's coordinates.
    rects: Vec<Rect>,
    layout: ReorderLayout,
    /// The items along the layout's axis, for upstream's drop rule.
    geometry: ReorderGeometry,
    /// Each slot's centre: the app's `positions`, else the measured one.
    centers: Vec<(f32, f32)>,
    given: bool,
    /// Shift feedback (FR9) rather than the insertion line.
    shift: bool,
    /// Under Reduce Motion with a coarse pointer the shift cross-fades in place.
    fade: bool,
    /// The pointer's speed, for the drop's glide from the release velocity.
    tracker: astryx_primitives::gestures::VelocityTracker,
    /// When the drag began, the tracker's time origin.
    began: web_time::Instant,
    /// The dragged item's size plus the gap, along the axis.
    pitch: f32,
    /// The pointer's offset into the item at pickup.
    grab: (f32, f32),
    pointer: Point<Pixels>,
    pub position: usize,
    range: (usize, usize),
}

enum Landed {
    Dropped,
    Place,
}

/// A reported move, waiting for the collection to render its new order.
struct Pending {
    key: ItemKey,
    to: usize,
    landed: Landed,
    /// Announced when the order stays.
    kept: SharedString,
}

#[derive(Default)]
pub(crate) struct Session {
    pub rows: Vec<Row>,
    next: Vec<Registration>,
    /// The collection's bounds, and its scroller's offset, when laid out.
    list: Option<Bounds<Pixels>>,
    host_at_probe: f32,
    armed: Option<Armed>,
    pub drag: Option<Drag>,
    pub mode: Option<ItemKey>,
    pub last_step: i8,
    pending: Option<Pending>,
    reorderable: Option<Reorderable>,
    host: Option<ScrollHandle>,
    rtl: bool,
    /// An app's own elements (`attach`), not a collection component.
    attached: bool,
    /// The latest press dragged its item: its click does not toggle the mode.
    dragged_press: bool,
    scroll_into_view: Option<ItemKey>,
    handles: HashMap<ItemKey, FocusHandle>,
    autoscroll: Option<Task<()>>,
    /// The rows' slides (FR9).
    pub(crate) slides: Slides,
}

/// What a collection gives its items: the session, the object, and a slot
/// for the dragged item, which the overlay draws at the pointer.
#[derive(Clone)]
pub(crate) struct ReorderCx {
    pub session: Entity<Session>,
    pub reorderable: Reorderable,
    pub id: ElementId,
    pub preview: Rc<RefCell<Option<AnyElement>>>,
}

impl std::fmt::Debug for ReorderCx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReorderCx").field("id", &self.id).finish()
    }
}

pub(crate) fn key_text(key: &ItemKey) -> String {
    match key {
        ItemKey::Number(n) => n.to_string(),
        ItemKey::Text(t) => t.to_string(),
    }
}

fn id_text(id: &ElementId) -> String {
    match id {
        ElementId::Name(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

/// Starts a frame of the collection `id` reordered by `reorderable`.
pub(crate) fn begin(
    id: &ElementId,
    reorderable: &Reorderable,
    attached: bool,
    window: &mut Window,
    cx: &mut App,
) -> ReorderCx {
    let session = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-reorder").into()),
        cx,
        |_, _: &mut Context<Session>| Session {
            last_step: -1,
            ..Session::default()
        },
    );
    // The object finds this collection for `start`; the last mounted wins.
    {
        let mut sessions = reorderable.0.sessions.borrow_mut();
        sessions.retain(|s| s.upgrade().is_some());
        if !sessions
            .iter()
            .any(|s| s.entity_id() == session.entity_id())
        {
            sessions.push(session.downgrade());
        }
    }
    let host = use_context::<ScrollHost>(cx).map(|h| h.0);
    let rtl = use_direction(cx).is_rtl();
    session.update(cx, |s, _| {
        s.next.clear();
        s.reorderable = Some(reorderable.clone());
        s.host = host;
        s.rtl = rtl;
        s.attached = attached;
    });
    ReorderCx {
        session,
        reorderable: reorderable.clone(),
        id: id.clone(),
        preview: Rc::new(RefCell::new(None)),
    }
}

/// An item's name and one-based position, as it registered this frame.
pub(crate) fn registered(rc: &ReorderCx, key: &ItemKey, cx: &App) -> (Option<SharedString>, usize) {
    let s = rc.session.read(cx);
    s.next
        .iter()
        .find(|r| &r.key == key)
        .map_or((None, 1), |r| (r.name.clone(), r.index + 1))
}

/// An item renders: it joins this frame's order.
pub(crate) fn register(rc: &ReorderCx, registration: Registration, cx: &mut App) {
    rc.session.update(cx, |s, _| s.next.push(registration));
}

fn rects_of(rows: &[Row], list: Bounds<Pixels>) -> Vec<Rect> {
    rows.iter()
        .map(|r| Rect {
            x: f32::from(r.bounds.left() - list.left()),
            y: f32::from(r.bounds.top() - list.top()),
            w: f32::from(r.bounds.size.width),
            h: f32::from(r.bounds.size.height),
        })
        .collect()
}

/// The collection's children are laid out: the frame's items, then what
/// waited for them (a move's announcement, a scroll into view).
pub(crate) fn finalize(
    session: &Entity<Session>,
    children: &[Bounds<Pixels>],
    count: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let mut announcements: Vec<SharedString> = Vec::new();
    let mut scroll: Option<(ScrollHandle, Bounds<Pixels>)> = None;
    let labels = labels_of(session, cx);
    let m = Messages::new(&labels, cx);
    session.update(cx, |s, _| {
        let mut rows: Vec<Row> = children
            .iter()
            .take(count)
            .map(|b| Row {
                key: None,
                name: None,
                movable: false,
                bounds: *b,
            })
            .collect();
        for r in s.next.drain(..) {
            if let Some(row) = rows.get_mut(r.index) {
                row.key = Some(r.key);
                row.name = r.name;
                row.movable = r.movable;
            }
        }
        s.rows = rows;
        // A landing ends once its frame is laid out.
        s.slides.landed();
        let keys: Vec<ItemKey> = s.rows.iter().filter_map(|r| r.key.clone()).collect();
        if s.drag.is_none() {
            s.slides.retain(&keys);
        }
        if let Some(p) = s.pending.take() {
            match index_of(&s.rows, &p.key) {
                Some(at) if at == p.to => {
                    announcements.push(match p.landed {
                        Landed::Dropped => m.dropped(at + 1, s.rows.len()),
                        Landed::Place => m.place(&s.rows, s.list, at),
                    });
                    if matches!(p.landed, Landed::Place) || s.attached {
                        s.scroll_into_view = Some(p.key.clone());
                    }
                }
                _ => announcements.push(p.kept),
            }
        }
        // An item leaving ends its mode without a word, and its drag.
        if let Some(key) = s.mode.clone()
            && index_of(&s.rows, &key).is_none()
        {
            s.mode = None;
        }
        if let Some(key) = s.drag.as_ref().map(|d| d.key.clone())
            && index_of(&s.rows, &key).is_none()
        {
            s.drag = None;
            s.autoscroll = None;
        }
        if let Some(key) = s.scroll_into_view.take()
            && let (Some(at), Some(host)) = (index_of(&s.rows, &key), s.host.clone())
        {
            scroll = Some((host, s.rows[at].bounds));
        }
    });
    // The item scrolls into view, the nearest way.
    if let Some((host, row)) = scroll {
        let view = host.bounds();
        let offset = host.offset();
        let delta = if row.top() < view.top() {
            view.top() - row.top()
        } else if row.bottom() > view.bottom() {
            -(row.bottom() - view.bottom()).min(row.top() - view.top())
        } else {
            px(0.)
        };
        if delta != px(0.) {
            host.set_offset(point(offset.x, offset.y + delta));
            window.defer(cx, |window, _| window.refresh());
        }
    }
    for text in announcements {
        window.defer(cx, move |window, cx| {
            announce(text, Politeness::Polite, window, cx)
        });
    }
}

/// The strings, from the object's labels or the catalog.
struct Messages<'a> {
    labels: &'a ReorderableLabels,
    i18n: astryx_i18n::I18n,
}

impl<'a> Messages<'a> {
    fn new(labels: &'a ReorderableLabels, cx: &App) -> Self {
        Self {
            labels,
            i18n: i18n(cx),
        }
    }

    fn get(
        &self,
        label: &Option<SharedString>,
        key: &str,
        args: &[(&str, Arg<'_>)],
    ) -> SharedString {
        match label {
            Some(template) => self.i18n.format_message(template, args).into(),
            None => self.i18n.format(key, args),
        }
    }

    fn counted(
        &self,
        label: &Option<SharedString>,
        key: &str,
        position: usize,
        total: usize,
    ) -> SharedString {
        self.get(
            label,
            key,
            &[
                ("position", Arg::Num(position as f64)),
                ("total", Arg::Num(total as f64)),
            ],
        )
    }

    fn picked_up(&self, position: usize, total: usize) -> SharedString {
        self.counted(
            &self.labels.picked_up,
            "@astryx.list.reorderPickedUp",
            position,
            total,
        )
    }

    fn dropped(&self, position: usize, total: usize) -> SharedString {
        self.counted(
            &self.labels.dropped,
            "@astryx.list.reorderDropped",
            position,
            total,
        )
    }

    fn cancelled(&self, position: usize, total: usize) -> SharedString {
        self.counted(
            &self.labels.cancelled,
            "@astryx.list.reorderCancelled",
            position,
            total,
        )
    }

    fn unnamed(&self, position: usize) -> SharedString {
        self.get(
            &self.labels.unnamed_item,
            "@astryx.list.reorderItemAt",
            &[("position", Arg::Num(position as f64))],
        )
    }

    fn name_of(&self, rows: &[Row], at: usize) -> SharedString {
        rows[at]
            .name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.unnamed(at + 1))
    }

    /// Where an item is: "Milk, position 3 of 5, after Eggs, before Bread",
    /// at the top or the end; in a grid with its row and column.
    fn place(&self, rows: &[Row], list: Option<Bounds<Pixels>>, at: usize) -> SharedString {
        let label = self.name_of(rows, at);
        let total = rows.len();
        if let Some(list) = list {
            let rects = rects_of(rows, list);
            if layout_of(&rects) == ReorderLayout::Grid {
                let (row, column) = row_and_column(&rects, at);
                return self.get(
                    &self.labels.place_in_grid,
                    "@astryx.list.reorderPlaceInGrid",
                    &[
                        ("label", label.as_ref().into()),
                        ("row", Arg::Num(row as f64)),
                        ("column", Arg::Num(column as f64)),
                        ("position", Arg::Num((at + 1) as f64)),
                        ("total", Arg::Num(total as f64)),
                    ],
                );
            }
        }
        let previous = (at > 0).then(|| self.name_of(rows, at - 1));
        let next = (at + 1 < total).then(|| self.name_of(rows, at + 1));
        let mut args: Vec<(&str, Arg<'_>)> = vec![
            ("label", label.as_ref().into()),
            ("position", Arg::Num((at + 1) as f64)),
            ("total", Arg::Num(total as f64)),
        ];
        if let Some(p) = &previous {
            args.push(("previous", p.as_ref().into()));
        }
        if let Some(n) = &next {
            args.push(("next", n.as_ref().into()));
        }
        match (&previous, &next) {
            (None, _) => self.get(
                &self.labels.place_at_top,
                "@astryx.list.reorderPlaceTop",
                &args,
            ),
            (_, None) => self.get(
                &self.labels.place_at_end,
                "@astryx.list.reorderPlaceEnd",
                &args,
            ),
            _ => self.get(&self.labels.place, "@astryx.list.reorderPlace", &args),
        }
    }

    fn refused(&self, label: &str) -> SharedString {
        self.get(
            &self.labels.refused,
            "@astryx.list.reorderRefused",
            &[("label", label.into())],
        )
    }

    fn not_moved(&self, label: &str) -> SharedString {
        self.get(
            &self.labels.not_moved,
            "@astryx.list.reorderNotMoved",
            &[("label", label.into())],
        )
    }

    fn mode_done(&self, label: &str, position: usize, total: usize) -> SharedString {
        self.get(
            &self.labels.mode_done,
            "@astryx.list.reorderModeDone",
            &[
                ("label", label.into()),
                ("position", Arg::Num(position as f64)),
                ("total", Arg::Num(total as f64)),
            ],
        )
    }

    fn handle(&self, label: Option<&str>, position: usize) -> SharedString {
        match label.filter(|l| !l.is_empty()) {
            Some(label) => self.get(
                &self.labels.handle,
                "@astryx.list.reorderHandle",
                &[("label", label.into())],
            ),
            None => self.get(
                &self.labels.unnamed_handle,
                "@astryx.list.reorderHandleAt",
                &[("position", Arg::Num(position as f64))],
            ),
        }
    }

    fn hint(&self) -> SharedString {
        self.get(&self.labels.handle_hint, "@astryx.list.reorderHint", &[])
    }

    fn arrow(&self, arrow: Arrow) -> SharedString {
        match arrow {
            Arrow::Up => self.get(&self.labels.move_up, "@astryx.list.reorderMoveUp", &[]),
            Arrow::Down => self.get(&self.labels.move_down, "@astryx.list.reorderMoveDown", &[]),
            Arrow::Left => self.get(&self.labels.move_left, "@astryx.list.reorderMoveLeft", &[]),
            Arrow::Right => self.get(
                &self.labels.move_right,
                "@astryx.list.reorderMoveRight",
                &[],
            ),
        }
    }

    fn done(&self) -> SharedString {
        self.get(&self.labels.done, "@astryx.list.reorderDone", &[])
    }
}

fn labels_of(session: &Entity<Session>, cx: &App) -> ReorderableLabels {
    session
        .read(cx)
        .reorderable
        .as_ref()
        .map(|r| r.0.options.labels.clone())
        .unwrap_or_default()
}

fn say(text: SharedString, window: &mut Window, cx: &mut App) {
    window.defer(cx, move |window, cx| {
        announce(text, Politeness::Polite, window, cx)
    });
}

/// Reports a move through the object's latest callback.
fn report(session: &Entity<Session>, m: ReorderMove, window: &mut Window, cx: &mut App) {
    let Some(r) = session.read(cx).reorderable.clone() else {
        return;
    };
    (r.0.options.on_reorder)(&m, window, cx);
    window.refresh();
}

fn index_of(rows: &[Row], key: &ItemKey) -> Option<usize> {
    rows.iter().position(|r| r.key.as_ref() == Some(key))
}

/// What a move the app does not apply announces: a List's item stays at
/// its place; an attached one cannot move (FR4b).
fn kept_message(s: &Session, m: &Messages<'_>, at: usize, dropped: bool) -> SharedString {
    if s.attached {
        m.not_moved(&m.name_of(&s.rows, at))
    } else if dropped {
        m.cancelled(at + 1, s.rows.len())
    } else {
        m.place(&s.rows, s.list, at)
    }
}

/// The layout the items are in now.
fn layout_now(s: &Session) -> ReorderLayout {
    match s.list {
        Some(list) => layout_of(&rects_of(&s.rows, list)),
        None => ReorderLayout::Vertical,
    }
}

// ---- Reorder mode --------------------------------------------------------

/// The handle toggles its item's mode.
fn toggle(session: &Entity<Session>, key: &ItemKey, window: &mut Window, cx: &mut App) {
    if session.read(cx).drag.is_some() {
        return;
    }
    if session.read(cx).mode.as_ref() == Some(key) {
        end_mode(session, window, cx);
        return;
    }
    let focus = session.read(cx).handles.get(key).cloned();
    session.update(cx, |s, _| {
        s.mode = Some(key.clone());
        s.last_step = -1;
    });
    if let Some(focus) = focus {
        focus.focus(window, cx);
    }
    window.refresh();
}

/// Ends reorder mode, keeping its moves: "Done, Milk at position 3 of 5".
pub(crate) fn end_mode(session: &Entity<Session>, window: &mut Window, cx: &mut App) {
    let Some(key) = session.update(cx, |s, _| s.mode.take()) else {
        return;
    };
    let labels = labels_of(session, cx);
    let m = Messages::new(&labels, cx);
    let rows = session.read(cx).rows.clone();
    if let Some(at) = index_of(&rows, &key) {
        let name = m.name_of(&rows, at);
        say(m.mode_done(&name, at + 1, rows.len()), window, cx);
    }
    if let Some(focus) = session.read(cx).handles.get(&key).cloned()
        && !focus.is_focused(window)
        && window.focused(cx).is_none()
    {
        focus.focus(window, cx);
    }
    window.refresh();
}

/// Reorder mode's move along the layout (FR4): reported at once and
/// announced once it lands. A move into an item that stays at either end is
/// refused and announced; one past the end does nothing.
fn step(session: &Entity<Session>, arrow: Arrow, window: &mut Window, cx: &mut App) {
    let (key, rows, list, rtl, dragging) = {
        let s = session.read(cx);
        (
            s.mode.clone(),
            s.rows.clone(),
            s.list,
            s.rtl,
            s.drag.is_some(),
        )
    };
    let (Some(key), Some(list)) = (key, list) else {
        return;
    };
    if dragging {
        return;
    }
    let Some(index) = index_of(&rows, &key) else {
        return;
    };
    let Some(to) = arrow_target(&rects_of(&rows, list), index, arrow, rtl) else {
        return;
    };
    let labels = labels_of(session, cx);
    let m = Messages::new(&labels, cx);
    let movable: Vec<bool> = rows.iter().map(|r| r.movable).collect();
    if bounds_of(&movable)[to] {
        say(m.refused(&m.name_of(&rows, to)), window, cx);
        return;
    }
    let kept = kept_message(session.read(cx), &m, index, false);
    let glides = lands_gliding(session, window, cx);
    session.update(cx, |s, _| {
        // The moved row and the rows it passes slide into their new places.
        if glides {
            s.slides.land();
        }
        s.last_step = if to < index { -1 } else { 1 };
        s.pending = Some(Pending {
            key: key.clone(),
            to,
            landed: Landed::Place,
            kept,
        });
    });
    report(session, ReorderMove { key, to }, window, cx);
}

/// FR4a: reorder mode from the app's own UI.
pub(crate) fn start(reorderable: &Reorderable, key: ItemKey, window: &mut Window, cx: &mut App) {
    let session = {
        let mut sessions = reorderable.0.sessions.borrow_mut();
        sessions.retain(|s| s.upgrade().is_some());
        if sessions.len() > 1 && cfg!(debug_assertions) {
            eprintln!(
                "createReorderable: `start` was called on an object passed to more than one collection: it starts reorder mode in the one mounted last."
            );
        }
        sessions.last().and_then(|s| s.upgrade())
    };
    let found = session.as_ref().is_some_and(|session| {
        let s = session.read(cx);
        s.rows
            .iter()
            .any(|r| r.key.as_ref() == Some(&key) && r.movable)
            && reorderable.can_move(&key)
    });
    let Some(session) = session.filter(|_| found) else {
        if cfg!(debug_assertions) {
            eprintln!(
                "createReorderable: `start({})` found no mounted item with that key that can be reordered, so it did nothing.",
                key_text(&key)
            );
        }
        return;
    };
    if session.read(cx).drag.is_some() {
        return;
    }
    session.update(cx, |s, _| {
        s.mode = Some(key.clone());
        s.last_step = -1;
        s.scroll_into_view = Some(key.clone());
    });
    // The UI that started it may hand its focus back as it closes (a menu
    // restoring its trigger's): the handle claims it after.
    if let Some(focus) = session.read(cx).handles.get(&key).cloned() {
        window.defer(cx, move |window, cx| {
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        });
    }
    window.refresh();
}

// ---- The drag ------------------------------------------------------------

/// The scroller's visible band, in the window, and how far it can go.
fn scroller(host: &Option<ScrollHandle>, window: &Window) -> (f32, f32, f32, f32) {
    match host {
        Some(h) => {
            let b = h.bounds();
            (
                f32::from(b.top()),
                f32::from(b.bottom()),
                -f32::from(h.offset().y),
                f32::from(h.max_offset().y),
            )
        }
        None => (0., f32::from(window.viewport_size().height), 0., 0.),
    }
}

/// The collection's origin now: where it was laid out, moved by any scroll
/// since.
fn origin(s: &Session) -> (f32, f32) {
    let laid = s
        .list
        .map_or((0., 0.), |b| (f32::from(b.left()), f32::from(b.top())));
    match &s.host {
        Some(h) => (laid.0, laid.1 + (f32::from(h.offset().y) - s.host_at_probe)),
        None => laid,
    }
}

/// Where the dragged item sits now, in the collection's coordinates: under
/// the pointer, stopped at the scroller's edges and the collection's ends.
fn dragged_at(s: &Session, d: &Drag, window: &Window) -> (f32, f32) {
    let (ox, oy) = origin(s);
    let own = d.rects[d.dragged];
    let x = f32::from(d.pointer.x) - ox - d.grab.0;
    let y = f32::from(d.pointer.y) - oy - d.grab.1;
    let (scroll_top, scroll_bottom, _, _) = scroller(&s.host, window);
    let low = |f: fn(&Rect) -> f32| d.rects.iter().map(f).fold(f32::INFINITY, f32::min);
    let high = |f: fn(&Rect) -> f32| d.rects.iter().map(f).fold(f32::NEG_INFINITY, f32::max);
    let clamped_y = clamp_top(
        y + oy,
        own.h,
        ReorderBounds {
            scroll_top,
            scroll_bottom,
            list_top: oy + low(|r| r.y),
            list_bottom: oy + high(|r| r.y + r.h),
        },
    ) - oy;
    match d.layout {
        ReorderLayout::Vertical if !d.given => (own.x, clamped_y),
        ReorderLayout::Horizontal if !d.given => (
            x.clamp(
                low(|r| r.x),
                (high(|r| r.x + r.w) - own.w).max(low(|r| r.x)),
            ),
            own.y,
        ),
        _ => (
            x.clamp(
                low(|r| r.x),
                (high(|r| r.x + r.w) - own.w).max(low(|r| r.x)),
            ),
            clamped_y,
        ),
    }
}

/// The candidate for the dragged item at `at` (its box's origin).
fn candidate(d: &Drag, at: (f32, f32), rtl: bool, width: f32) -> usize {
    let own = d.rects[d.dragged];
    let center = (at.0 + own.w / 2., at.1 + own.h / 2.);
    let reading = |x: f32| if rtl { width - x } else { x };
    match (d.layout, d.given, d.shift) {
        (_, true, false) | (ReorderLayout::Grid, false, false) => {
            nearest_slot(&d.centers, center, d.range)
        }
        (_, true, true) | (ReorderLayout::Grid, false, true) => {
            shift_candidate(&d.centers, d.position, center, d.range)
        }
        (ReorderLayout::Vertical, false, false) => drop_position(&d.geometry, d.dragged, center.1),
        (ReorderLayout::Horizontal, false, false) => {
            drop_position(&d.geometry, d.dragged, reading(center.0))
        }
        (ReorderLayout::Vertical, false, true) | (ReorderLayout::Horizontal, false, true) => {
            let along = d.layout == ReorderLayout::Vertical;
            let starts: Vec<f32> = d.geometry.rows.iter().map(|r| r.top).collect();
            let sizes: Vec<f32> = d.geometry.rows.iter().map(|r| r.height).collect();
            let slots: Vec<(f32, f32)> = slot_centers(&starts, &sizes, d.dragged)
                .into_iter()
                .map(|c| (c, 0.))
                .collect();
            let at = if along { center.1 } else { reading(center.0) };
            shift_candidate(&slots, d.position, (at, 0.), d.range)
        }
    }
}

/// A press on a handle moved past the slop: the drag begins.
fn begin_drag(
    session: &Entity<Session>,
    key: ItemKey,
    pointer: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    if session.read(cx).mode.is_some() {
        end_mode(session, window, cx);
    }
    let labels = labels_of(session, cx);
    let m = Messages::new(&labels, cx);
    let options = session
        .read(cx)
        .reorderable
        .as_ref()
        .map(|r| r.0.options.clone());
    let given_positions = options
        .as_ref()
        .and_then(|o| o.positions.as_ref().map(|f| f()));
    // Under Reduce Motion the neighbours do not travel: with a coarse pointer the gap
    // opens as an in-place cross-fade, and otherwise the drag shows the insertion line.
    let reduced = prefers_reduced_motion(cx);
    let coarse = astryx_primitives::coarse_primary_pointer(window, cx);
    let shift = options
        .as_ref()
        .is_some_and(|o| o.feedback == FeedbackBehavior::Shift)
        && (!reduced || coarse);
    let fade = reduced;
    let began_at = cx.background_executor().now();
    let began = session.update(cx, |s, _| {
        s.armed = None;
        let list = s.list?;
        let dragged = index_of(&s.rows, &key)?;
        if !s.rows[dragged].movable {
            return None;
        }
        let rects = rects_of(&s.rows, list);
        let layout = layout_of(&rects);
        let movable: Vec<bool> = s.rows.iter().map(|r| r.movable).collect();
        let bounds = bounds_of(&movable);
        let width = f32::from(list.size.width);
        // The items along the axis, in reading order.
        let along: Vec<(f32, f32)> = rects
            .iter()
            .map(|r| match layout {
                ReorderLayout::Horizontal if s.rtl => (width - r.x - r.w, r.w),
                ReorderLayout::Horizontal => (r.x, r.w),
                _ => (r.y, r.h),
            })
            .collect();
        let rows: Vec<ReorderRow> = along
            .iter()
            .enumerate()
            .map(|(i, &(top, height))| ReorderRow {
                top,
                height,
                is_fixed: bounds[i],
            })
            .collect();
        let gap = if rows.len() > 1 {
            (rows[1].top - rows[0].top - rows[0].height).max(0.)
        } else {
            0.
        };
        let geometry = ReorderGeometry { rows, gap };
        let range = movable_range(&geometry.rows, dragged);
        let (centers, given) = match given_positions.filter(|p| p.len() == rects.len()) {
            Some(p) => (
                p.iter().map(|c| (f32::from(c.x), f32::from(c.y))).collect(),
                true,
            ),
            None => (rects.iter().map(Rect::center).collect(), false),
        };
        let own = rects[dragged];
        let pitch = along[dragged].1 + geometry.gap;
        s.drag = Some(Drag {
            key: key.clone(),
            dragged,
            grab: (
                f32::from(pointer.x - list.left()) - own.x,
                f32::from(pointer.y - list.top()) - own.y,
            ),
            rects,
            layout,
            geometry,
            centers,
            given,
            shift,
            fade,
            tracker: {
                let mut tracker = astryx_primitives::gestures::VelocityTracker::default();
                tracker.add(f64::from(pointer.x), f64::from(pointer.y), 0.);
                tracker
            },
            began: began_at,
            pitch,
            pointer,
            position: dragged,
            range,
        });
        s.dragged_press = true;
        Some((dragged, s.rows.len()))
    });
    if let Some((dragged, total)) = began {
        say(m.picked_up(dragged + 1, total), window, cx);
        update_autoscroll(session, window, cx);
    }
    window.refresh();
}

/// Whether a landing glides the rows it moves: under shift feedback, whose drop settles
/// into the open gap, or under the touch presentation (upstream's `uikit` presentation).
/// A mouse's drop behind the insertion line lands at once, as upstream's does.
fn lands_gliding(session: &Entity<Session>, window: &Window, cx: &mut App) -> bool {
    let shift = session
        .read(cx)
        .reorderable
        .as_ref()
        .is_some_and(|r| r.0.options.feedback == FeedbackBehavior::Shift);
    shift || astryx_primitives::coarse_primary_pointer(window, cx)
}

/// The drop: the move is reported once, and announced once it lands.
fn drop_drag(session: &Entity<Session>, window: &mut Window, cx: &mut App) {
    let Some(d) = session.update(cx, |s, _| {
        s.autoscroll = None;
        s.drag.take()
    }) else {
        return;
    };
    // The dragged row lands from where its preview was let go.
    let at = {
        let s = session.read(cx);
        let (x, y) = dragged_at(s, &d, window);
        let (ox, oy) = origin(s);
        point(px(x + ox), px(y + oy))
    };
    // It glides from the drag's release velocity (upstream's `pending.velocity`).
    let released = cx
        .background_executor()
        .now()
        .saturating_duration_since(d.began)
        .as_secs_f64()
        * 1000.;
    let v = d.tracker.velocity(Some(released));
    let glides = lands_gliding(session, window, cx);
    session.update(cx, |s, _| {
        s.slides.let_go_at(&d.key, at, (v.x, v.y));
        if glides {
            s.slides.land();
        }
    });
    let labels = labels_of(session, cx);
    let m = Messages::new(&labels, cx);
    let total = d.rects.len();
    if d.position == d.dragged {
        say(m.dropped(d.position + 1, total), window, cx);
        window.refresh();
        return;
    }
    let kept = kept_message(session.read(cx), &m, d.dragged, true);
    session.update(cx, |s, _| {
        s.pending = Some(Pending {
            key: d.key.clone(),
            to: d.position,
            landed: Landed::Dropped,
            kept,
        });
    });
    report(
        session,
        ReorderMove {
            key: d.key,
            to: d.position,
        },
        window,
        cx,
    );
}

/// The drag ends where it started; nothing is reported.
fn cancel_drag(session: &Entity<Session>, window: &mut Window, cx: &mut App) {
    let Some(d) = session.update(cx, |s, _| {
        s.autoscroll = None;
        s.drag.take()
    }) else {
        return;
    };
    // The rows go home: the dragged one from where it was shown.
    let at = {
        let s = session.read(cx);
        let (x, y) = dragged_at(s, &d, window);
        let (ox, oy) = origin(s);
        point(px(x + ox), px(y + oy))
    };
    let glides = lands_gliding(session, window, cx);
    session.update(cx, |s, _| {
        s.slides.let_go_at(&d.key, at, (0., 0.));
        if glides {
            s.slides.land();
        }
    });
    let labels = labels_of(session, cx);
    let m = Messages::new(&labels, cx);
    say(m.cancelled(d.dragged + 1, d.rects.len()), window, cx);
    window.refresh();
}

/// Points per tick toward the edge the dragged item is past: positive down.
fn autoscroll_points(s: &Session, window: &Window) -> f32 {
    let Some(d) = &s.drag else {
        return 0.;
    };
    let (scroll_top, scroll_bottom, position, max) = scroller(&s.host, window);
    let height = d.rects[d.dragged].h;
    let top = dragged_at(s, d, window).1 + origin(s).1;
    let mut step = 0.;
    if position < max {
        step = autoscroll_step(top + height - scroll_bottom);
    }
    if step == 0. && position > 0. {
        step = -autoscroll_step(scroll_top - top);
    }
    step
}

/// Starts the edge autoscroll's timer while the dragged item is near an
/// edge.
fn update_autoscroll(session: &Entity<Session>, window: &mut Window, cx: &mut App) {
    let s = session.read(cx);
    if s.host.is_none() || s.autoscroll.is_some() || autoscroll_points(s, window) == 0. {
        return;
    }
    let weak = session.downgrade();
    let task = window.spawn(cx, async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(AUTOSCROLL_TICK_MS))
                .await;
            let Some(session) = weak.upgrade() else {
                return;
            };
            let more = cx
                .update(|window, cx| {
                    let s = session.read(cx);
                    let step = autoscroll_points(s, window);
                    let Some(host) = s.host.clone().filter(|_| step != 0.) else {
                        session.update(cx, |s, _| s.autoscroll = None);
                        return false;
                    };
                    let offset = host.offset();
                    let max = host.max_offset().y;
                    let y = (offset.y - px(step)).max(-max).min(px(0.));
                    host.set_offset(point(offset.x, y));
                    window.refresh();
                    true
                })
                .unwrap_or(false);
            if !more {
                return;
            }
        }
    });
    session.update(cx, |s, _| s.autoscroll = Some(task));
}

fn pointer_moved(
    session: &Entity<Session>,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let armed = session
        .read(cx)
        .armed
        .as_ref()
        .map(|a| (a.key.clone(), a.at));
    if let Some((key, at)) = armed
        && session.read(cx).drag.is_none()
    {
        let moved = position - at;
        if f32::from(moved.x).hypot(f32::from(moved.y)) > MOUSE_DRAG_SLOP {
            begin_drag(session, key, position, window, cx);
        }
        return;
    }
    if session.read(cx).drag.is_some() {
        let now = cx.background_executor().now();
        session.update(cx, |s, _| {
            if let Some(d) = &mut s.drag {
                d.pointer = position;
                let ms = now.saturating_duration_since(d.began).as_secs_f64() * 1000.;
                d.tracker
                    .add(f64::from(position.x), f64::from(position.y), ms);
            }
        });
        update_autoscroll(session, window, cx);
        window.refresh();
    }
}

fn released(session: &Entity<Session>, window: &mut Window, cx: &mut App) {
    if session.read(cx).drag.is_some() {
        drop_drag(session, window, cx);
    }
    session.update(cx, |s, _| s.armed = None);
}

/// While the dragged item would land elsewhere under shift feedback, where
/// the item at `index` slides (FR9).
pub(crate) fn shift_of(rc: &ReorderCx, index: usize, cx: &App) -> Option<(f32, f32)> {
    let s = rc.session.read(cx);
    let d = s.drag.as_ref().filter(|d| d.shift)?;
    let offset = shift_offset(
        &d.centers, d.layout, d.given, d.pitch, d.dragged, d.position, index,
    );
    (offset != (0., 0.)).then_some(offset)
}

/// The dragged item's place: `Some((size, shift))` when `key` is being
/// dragged; with shift feedback its place is the gap.
pub(crate) fn drag_source(
    rc: &ReorderCx,
    key: &ItemKey,
    cx: &App,
) -> Option<(gpui::Size<Pixels>, bool)> {
    let s = rc.session.read(cx);
    let d = s.drag.as_ref().filter(|d| &d.key == key)?;
    let own = d.rects[d.dragged];
    Some((gpui::size(px(own.w), px(own.h)), d.shift))
}

// ---- The handle ----------------------------------------------------------

/// The handle's box and its three lines: 28px square, taking only the
/// lines' 14px in its row (the negative block margins), reaching 8px into
/// the row's end padding.
pub(crate) fn grip(color: Hsla) -> gpui::Div {
    let bar = || div().w(px(20.)).h(px(2.)).rounded(px(1.)).bg(color);
    div()
        .relative()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .flex_none()
        .w(px(28.))
        .h(px(28.))
        .my(px(-7.))
        .mr(px(-8.))
        .rounded(px(4.))
        .child(bar())
        .child(bar())
        .child(bar())
}

fn handle_id(rc: &ReorderCx, key: &ItemKey) -> String {
    format!("{}-reorder-{}", id_text(&rc.id), key_text(key))
}

/// A List row's handle: the grip, ending the row.
pub(crate) fn handle(
    rc: &ReorderCx,
    key: &ItemKey,
    label: Option<SharedString>,
    position: usize,
    disabled: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let color = if disabled {
        t.color.icon_disabled
    } else {
        t.color.icon_secondary
    };
    let body = grip(color).id(ElementId::Name(handle_id(rc, key).into()));
    handle_on(rc, key, label, position, disabled, body, window, cx)
}

/// Makes `body` an item's handle (FR4): a drag target, and a toggle
/// (`aria-pressed`) for reorder mode named "Reorder" and the item's name,
/// in the Tab order, with the mode's hint while its item is in the mode.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_on(
    rc: &ReorderCx,
    key: &ItemKey,
    label: Option<SharedString>,
    position: usize,
    disabled: bool,
    body: Stateful<gpui::Div>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let session = rc.session.clone();
    let focus = match session.read(cx).handles.get(key).cloned() {
        Some(f) => f,
        None => {
            let f = cx.focus_handle().tab_stop(true);
            session.update(cx, |s, _| s.handles.insert(key.clone(), f.clone()));
            f
        }
    };
    let (in_mode, dragging, layout) = {
        let s = session.read(cx);
        (
            s.mode.as_ref() == Some(key),
            s.drag.is_some(),
            layout_now(s),
        )
    };
    let labels = rc.reorderable.0.options.labels.clone();
    let m = Messages::new(&labels, cx);
    let selector = handle_id(rc, key);
    let mut el = body
        .debug_selector(move || selector.clone())
        .track_focus(&focus)
        .relative()
        .role(Role::Button)
        .aria_label(m.handle(label.as_deref(), position))
        .aria_description(m.hint())
        .aria_toggled(if in_mode {
            Toggled::True
        } else {
            Toggled::False
        });
    if disabled {
        el = el.aria_disabled(true);
    } else {
        el = el.cursor(if dragging {
            CursorStyle::ClosedHand
        } else {
            CursorStyle::OpenHand
        });
    }
    if in_mode {
        el = el.bg(t.color.overlay_pressed);
    }
    if is_focus_visible(&focus, window) {
        el = el.child(crate::focus::focus_ring_inset(t, px(4.), rem));
    }
    if !disabled {
        let (s1, k1) = (session.clone(), key.clone());
        let (s2, k2) = (session.clone(), key.clone());
        let (s3, k3) = (session.clone(), key.clone());
        let f1 = focus.clone();
        el = el
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                cx.stop_propagation();
                f1.focus(window, cx);
                s1.update(cx, |s, _| {
                    s.armed = Some(Armed {
                        key: k1.clone(),
                        at: event.position,
                    });
                    s.dragged_press = false;
                });
                window.refresh();
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                if s2.update(cx, |s, _| std::mem::take(&mut s.dragged_press)) {
                    return;
                }
                toggle(&s2, &k2, window, cx);
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let k = &event.keystroke;
                if k.modifiers.alt || k.modifiers.control || k.modifiers.platform {
                    return;
                }
                let mode = s3.read(cx).mode.as_ref() == Some(&k3);
                let arrow = match k.key.as_str() {
                    "up" => Some(Arrow::Up),
                    "down" => Some(Arrow::Down),
                    "left" => Some(Arrow::Left),
                    "right" => Some(Arrow::Right),
                    _ => None,
                };
                match k.key.as_str() {
                    "escape" if s3.read(cx).drag.is_some() => {
                        cx.stop_propagation();
                        cancel_drag(&s3, window, cx);
                    }
                    "escape" if mode => {
                        cx.stop_propagation();
                        end_mode(&s3, window, cx);
                    }
                    "enter" | "space" => {
                        cx.stop_propagation();
                        toggle(&s3, &k3, window, cx);
                    }
                    _ if mode && arrow.is_some() => {
                        cx.stop_propagation();
                        step(&s3, arrow.unwrap_or(Arrow::Down), window, cx);
                    }
                    _ => {}
                }
            });
    }
    if !in_mode {
        return el.into_any_element();
    }
    // The mode's hint, anchored below the handle's start (Core's keyboard
    // hint's layer and roles); its buttons do what the keys do.
    let rtl = use_direction(cx).is_rtl();
    let hint = mode_hint(rc, &m, layout, window, cx);
    let dismiss = session.clone();
    anchored_layer(
        ElementId::Name(format!("{:?}-reorder-hint", rc.id).into()),
        el,
    )
    .surface(
        hint,
        LayerOptions {
            placement: Placement::Below,
            align: LayerAlign::Start,
            rtl,
            ..LayerOptions::default()
        },
        move |window, cx| end_mode(&dismiss, window, cx),
    )
    .light_dismiss(false)
    .into_any_element()
}

type PressFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Reorder mode's hint: the moves along the layout's axis and Done, as
/// buttons that never take the focus from the handle.
fn mode_hint(
    rc: &ReorderCx,
    m: &Messages<'_>,
    layout: ReorderLayout,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let name = id_text(&rc.id);
    let button = |id: &str, label: Option<SharedString>, child: AnyElement, on_press: PressFn| {
        let mut b = div()
            .id(ElementId::Name(format!("{name}-hint-{id}").into()))
            .debug_selector({
                let s = format!("{name}-hint-{id}");
                move || s.clone()
            })
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor_pointer()
            .role(Role::Button)
            .child(child)
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                on_press(window, cx);
            });
        if let Some(label) = label {
            b = b.aria_label(label);
        }
        b
    };
    let arrows: &[(Arrow, &str, &str)] = match layout {
        ReorderLayout::Vertical => &[(Arrow::Up, "up", "up"), (Arrow::Down, "down", "down")],
        ReorderLayout::Horizontal => &[
            (Arrow::Left, "left", "left"),
            (Arrow::Right, "right", "right"),
        ],
        ReorderLayout::Grid => &[
            (Arrow::Left, "left", "left"),
            (Arrow::Right, "right", "right"),
            (Arrow::Up, "up", "up"),
            (Arrow::Down, "down", "down"),
        ],
    };
    let mut hint = div()
        .flex()
        .items_center()
        .gap(t.spacing.s1)
        .bg(t.color.background_popover)
        .rounded(t.radius.element)
        .shadow_token(&t.shadow.low)
        .py(t.spacing.s1)
        .px(t.spacing.s2)
        .text_size(t.text.supporting.size.to_pixels(rem))
        .line_height(relative(t.text.supporting.leading))
        .text_color(t.color.text_secondary)
        .whitespace_nowrap();
    for &(arrow, id, keys) in arrows {
        let s = rc.session.clone();
        hint = hint.child(button(
            id,
            Some(m.arrow(arrow)),
            Kbd::new(keys).into_any_element(),
            Rc::new(move |window, cx| step(&s, arrow, window, cx)),
        ));
    }
    let s = rc.session.clone();
    hint.child(button(
        "done",
        None,
        div().px(t.spacing.s1).child(m.done()).into_any_element(),
        Rc::new(move |window, cx| end_mode(&s, window, cx)),
    ))
    .into_any_element()
}

// ---- The overlay ---------------------------------------------------------

/// The collection's last child: the insertion line, the dragged item at the
/// pointer, and the probe that reads where the collection sits and takes
/// the drag's pointer.
pub(crate) fn overlay(rc: &ReorderCx) -> AnyElement {
    Overlay { rc: rc.clone() }.into_any_element()
}

#[derive(IntoElement)]
struct Overlay {
    rc: ReorderCx,
}

/// The insertion line beside `rect` (across the inline axis), before or
/// after it in reading order, in the middle of the gap.
fn side_line(rect: Rect, after: bool, gap: f32, rtl: bool) -> (f32, f32, f32, f32) {
    let right = after != rtl;
    let x = if right {
        rect.x + rect.w + gap / 2.
    } else {
        rect.x - gap / 2.
    };
    (
        x - INSERTION_LINE_THICKNESS / 2.,
        rect.y,
        INSERTION_LINE_THICKNESS,
        rect.h,
    )
}

/// The inline gap after `index` in its row.
fn inline_gap(rects: &[Rect]) -> f32 {
    rects
        .windows(2)
        .find(|w| (w[0].y - w[1].y).abs() <= 1.)
        .map_or(0., |w| {
            (w[1].x - w[0].x - w[0].w)
                .abs()
                .min((w[0].x - w[1].x - w[1].w).abs())
                .max(0.)
        })
}

impl RenderOnce for Overlay {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let accent = t.color.accent;
        let surface = t.color.background_surface;
        let session = self.rc.session.clone();
        let mut layer = div().absolute().top_0().left_0().size_full();
        let line = |(x, y, w, h): (f32, f32, f32, f32)| {
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
                .rounded(px(INSERTION_LINE_THICKNESS / 2.))
                .bg(accent)
        };
        let across = |rect: Rect, center: f32| {
            (
                rect.x,
                center - INSERTION_LINE_THICKNESS / 2.,
                rect.w,
                INSERTION_LINE_THICKNESS,
            )
        };
        let (rtl, width) = {
            let s = session.read(cx);
            (s.rtl, s.list.map_or(0., |l| f32::from(l.size.width)))
        };
        let drag = {
            let s = session.read(cx);
            s.drag.as_ref().map(|d| {
                let at = dragged_at(s, d, window);
                let position = candidate(d, at, rtl, width);
                let own = d.rects[d.dragged];
                let line = if d.shift {
                    None
                } else {
                    match d.layout {
                        ReorderLayout::Vertical if !d.given => {
                            line_center(&d.geometry, d.dragged, position).map(|c| across(own, c))
                        }
                        _ => (position != d.dragged).then(|| {
                            side_line(
                                d.rects[position],
                                position > d.dragged,
                                inline_gap(&d.rects),
                                rtl,
                            )
                        }),
                    }
                };
                (at, own, position, line)
            })
        };
        if let Some((at, own, position, line_at)) = drag {
            session.update(cx, |s, _| {
                if let Some(d) = &mut s.drag {
                    d.position = position;
                }
            });
            if let Some(l) = line_at {
                layer = layer.child(line(l));
            }
            if let Some(item) = self.rc.preview.borrow_mut().take() {
                layer = layer.child(
                    div()
                        .absolute()
                        .left(px(at.0))
                        .top(px(at.1))
                        .w(px(own.w))
                        .opacity(MOVING_OPACITY)
                        .bg(surface)
                        .child(item),
                );
            }
        } else {
            let s = session.read(cx);
            if let (Some(key), Some(list)) = (&s.mode, s.list)
                && let Some(at) = index_of(&s.rows, key)
            {
                let rects = rects_of(&s.rows, list);
                let l = match layout_of(&rects) {
                    ReorderLayout::Vertical => {
                        let rows: Vec<(f32, f32)> = rects.iter().map(|r| (r.y, r.h)).collect();
                        across(rects[at], mode_line_center(&rows, at, s.last_step))
                    }
                    _ => side_line(rects[at], s.last_step > 0, inline_gap(&rects), s.rtl),
                };
                layer = layer.child(line(l));
            }
        }
        let probe_session = session.clone();
        let listen_session = session.clone();
        let probe = canvas(
            move |bounds, _, cx| {
                probe_session.update(cx, |s, _| {
                    s.list = Some(bounds);
                    s.host_at_probe = s.host.as_ref().map_or(0., |h| f32::from(h.offset().y));
                });
            },
            move |_, _, window, cx| {
                let s = listen_session.read(cx);
                if s.armed.is_none() && s.drag.is_none() {
                    return;
                }
                let moved = listen_session.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    // A release outside the window sends no mouse-up: the
                    // next move without the button is the release.
                    if event.pressed_button != Some(MouseButton::Left) {
                        released(&moved, window, cx);
                    } else {
                        pointer_moved(&moved, event.position, window, cx);
                    }
                });
                let up = listen_session.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                        released(&up, window, cx);
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        layer.child(probe)
    }
}

/// An item's place while it is dragged: its dimmed copy (the List's text
/// copy, or what an app gives), or under shift feedback the gap; the item
/// itself goes to the overlay. Items the drag passes slide under shift
/// feedback.
pub(crate) fn place_item(
    rc: &ReorderCx,
    key: &ItemKey,
    index: usize,
    element: AnyElement,
    copy: impl FnOnce() -> Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    if let Some((size, shift)) = drag_source(rc, key, cx) {
        *rc.preview.borrow_mut() = Some(element);
        let place = div().w(size.width).h(size.height).flex_none();
        return match (shift, copy()) {
            (false, Some(copy)) => place
                .overflow_hidden()
                .opacity(MOVING_OPACITY)
                .child(copy)
                .into_any_element(),
            _ => place.into_any_element(),
        };
    }
    let target = shift_of(rc, index, cx).unwrap_or((0., 0.));
    let s = rc.session.read(cx);
    let motion = match &s.drag {
        Some(d) if d.fade => SlideMotion::Fade,
        Some(_) => SlideMotion::Glide,
        None if prefers_reduced_motion(cx) => SlideMotion::Jump,
        None => SlideMotion::Glide,
    };
    s.slides.clone().place(key, target, motion, element, cx)
}
