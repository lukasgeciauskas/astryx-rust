//! A reorderable collection's session (upstream `List/listReorderEngine.tsx`):
//! its rows as the last frame laid them out, a drag, reorder mode and the
//! announcements, with the handle, the overlay that draws the preview and
//! the insertion line, and the reorder mode hint.
//!
//! A collection renders [`begin`] once a frame, each of its rows
//! [`register`]s as it renders and ends with the [`handle`], and the
//! collection's last child is the [`overlay`]. The collection reports its
//! children's bounds to [`finalize`] once they are laid out.
//!
//! The drag's look follows upstream: the rows stay still until the drop; the
//! source dims in place (here the row's text, the one copy GPUI can draw
//! twice), and the row itself, dimmed, follows the pointer as the preview;
//! an accent line marks the candidate; and the scroller the collection sits
//! in autoscrolls near its edges. With a mouse the rows jump to their new
//! places at the drop, as upstream's do.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{Arg, current as i18n, use_direction};
use astryx_primitives::{
    LayerAlign, LayerOptions, Placement, Politeness, ScrollHost, anchored_layer, announce,
    is_focus_visible, use_context,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, Context, CursorStyle, DispatchPhase, ElementId, Entity, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Point, RenderOnce, Role, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Task, Toggled, Window, canvas, div, point, px, relative,
};

use super::model::{
    AUTOSCROLL_TICK_MS, INSERTION_LINE_THICKNESS, MOUSE_DRAG_SLOP, MOVING_OPACITY, ReorderBounds,
    ReorderGeometry, ReorderRow, autoscroll_step, bounds_of, clamp_top, drop_position, line_center,
    mode_line_center,
};
use super::{ItemKey, ReorderMove, Reorderable, ReorderableLabels};
use crate::content::Kbd;

/// One row of the collection, as the last frame laid it out.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub key: Option<ItemKey>,
    pub name: Option<SharedString>,
    pub movable: bool,
    /// Its bounds in the window.
    pub bounds: Bounds<Pixels>,
}

/// A row telling the session about itself as it renders.
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
    /// Measured at pickup, in the collection's coordinates.
    pub geometry: ReorderGeometry,
    /// The pointer's distance below the row's top at pickup.
    grab: f32,
    pointer: Point<Pixels>,
    pub position: usize,
    /// The row's inline start and width, in the collection's coordinates.
    span: (f32, f32),
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
    /// The collection's bounds, and its scroller's offset, when last laid out.
    list: Option<Bounds<Pixels>>,
    host_at_probe: f32,
    armed: Option<Armed>,
    pub drag: Option<Drag>,
    pub mode: Option<ItemKey>,
    pub last_step: i8,
    pending: Option<Pending>,
    reorderable: Option<Reorderable>,
    host: Option<ScrollHandle>,
    /// The latest press dragged its row: its click does not toggle the mode.
    dragged_press: bool,
    scroll_into_view: Option<ItemKey>,
    handles: HashMap<ItemKey, FocusHandle>,
    autoscroll: Option<Task<()>>,
}

/// What a collection gives its rows: the session, the object and a slot for
/// the dragged row, which the overlay draws at the pointer.
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

fn key_text(key: &ItemKey) -> String {
    match key {
        ItemKey::Number(n) => n.to_string(),
        ItemKey::Text(t) => t.to_string(),
    }
}

/// Starts a frame of the collection `id` reordered by `reorderable`.
pub(crate) fn begin(
    id: &ElementId,
    reorderable: &Reorderable,
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
    session.update(cx, |s, _| {
        s.next.clear();
        s.reorderable = Some(reorderable.clone());
        s.host = host;
    });
    ReorderCx {
        session,
        reorderable: reorderable.clone(),
        id: id.clone(),
        preview: Rc::new(RefCell::new(None)),
    }
}

/// A row renders: it joins this frame's order.
pub(crate) fn register(rc: &ReorderCx, registration: Registration, cx: &mut App) {
    rc.session.update(cx, |s, _| s.next.push(registration));
}

/// The collection's children are laid out: the frame's rows, then whatever
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
    let labels = session
        .read(cx)
        .reorderable
        .as_ref()
        .map(|r| r.0.options.labels.clone())
        .unwrap_or_default();
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
        let index_of =
            |s: &Session, key: &ItemKey| s.rows.iter().position(|r| r.key.as_ref() == Some(key));
        if let Some(p) = s.pending.take() {
            match index_of(s, &p.key) {
                Some(at) if at == p.to => {
                    announcements.push(match p.landed {
                        Landed::Dropped => m.dropped(at + 1, s.rows.len()),
                        Landed::Place => m.place_of(&s.rows, at),
                    });
                    if matches!(p.landed, Landed::Place) {
                        s.scroll_into_view = Some(p.key.clone());
                    }
                }
                _ => announcements.push(p.kept),
            }
        }
        // A row leaving ends its mode without a word, and its drag.
        if let Some(key) = s.mode.clone()
            && index_of(s, &key).is_none()
        {
            s.mode = None;
        }
        if let Some(key) = s.drag.as_ref().map(|d| d.key.clone())
            && index_of(s, &key).is_none()
        {
            s.drag = None;
            s.autoscroll = None;
        }
        if let Some(key) = s.scroll_into_view.take()
            && let (Some(at), Some(host)) = (index_of(s, &key), s.host.clone())
        {
            scroll = Some((host, s.rows[at].bounds));
        }
    });
    // The row scrolls into view, the nearest way.
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

    /// "Milk, position 3 of 5, after Eggs, before Bread", at the top or the
    /// end.
    fn place_of(&self, rows: &[Row], at: usize) -> SharedString {
        let label = self.name_of(rows, at);
        let total = rows.len();
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

    fn move_up(&self) -> SharedString {
        self.get(&self.labels.move_up, "@astryx.list.reorderMoveUp", &[])
    }

    fn move_down(&self) -> SharedString {
        self.get(&self.labels.move_down, "@astryx.list.reorderMoveDown", &[])
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

// ---- Reorder mode --------------------------------------------------------

/// The handle toggles its row's mode.
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
    // Focus a pointer left in the hint goes back to the handle.
    if let Some(focus) = session.read(cx).handles.get(&key).cloned()
        && !focus.is_focused(window)
        && window.focused(cx).is_none()
    {
        focus.focus(window, cx);
    }
    window.refresh();
}

/// Reorder mode's move: a place up (`-1`) or down (`1`), reported at once
/// and announced once it lands. A move into a row that stays at either end
/// is refused and announced; one past the end does nothing.
fn step(session: &Entity<Session>, delta: i8, window: &mut Window, cx: &mut App) {
    let (key, rows, dragging) = {
        let s = session.read(cx);
        (s.mode.clone(), s.rows.clone(), s.drag.is_some())
    };
    let Some(key) = key else {
        return;
    };
    if dragging {
        return;
    }
    let Some(index) = index_of(&rows, &key) else {
        return;
    };
    let to = index as isize + delta as isize;
    if to < 0 || to as usize >= rows.len() {
        return;
    }
    let to = to as usize;
    let labels = labels_of(session, cx);
    let m = Messages::new(&labels, cx);
    let movable: Vec<bool> = rows.iter().map(|r| r.movable).collect();
    if bounds_of(&movable)[to] {
        say(m.refused(&m.name_of(&rows, to)), window, cx);
        return;
    }
    let kept = m.place_of(&rows, index);
    session.update(cx, |s, _| {
        s.last_step = delta;
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
    let started = session.as_ref().is_some_and(|session| {
        let s = session.read(cx);
        if s.drag.is_some() {
            return true;
        }
        let movable = s
            .rows
            .iter()
            .any(|r| r.key.as_ref() == Some(&key) && r.movable);
        movable && reorderable.can_move(&key)
    });
    let Some(session) = session.filter(|_| started) else {
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

/// The collection's top now: where it was laid out, moved by any scroll
/// since.
fn list_top(s: &Session) -> f32 {
    let laid = s.list.map_or(0., |b| f32::from(b.top()));
    match &s.host {
        Some(h) => laid + (f32::from(h.offset().y) - s.host_at_probe),
        None => laid,
    }
}

fn dragged_top(s: &Session, d: &Drag, window: &Window) -> f32 {
    let row = d.geometry.rows[d.dragged];
    let first = d.geometry.rows[0];
    let last = d.geometry.rows[d.geometry.rows.len() - 1];
    let origin = list_top(s);
    let (scroll_top, scroll_bottom, _, _) = scroller(&s.host, window);
    clamp_top(
        f32::from(d.pointer.y) - d.grab,
        row.height,
        ReorderBounds {
            scroll_top,
            scroll_bottom,
            list_top: origin + first.top,
            list_bottom: origin + last.top + last.height,
        },
    )
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
    let began = session.update(cx, |s, _| {
        s.armed = None;
        let list = s.list?;
        let dragged = index_of(&s.rows, &key)?;
        if !s.rows[dragged].movable {
            return None;
        }
        let movable: Vec<bool> = s.rows.iter().map(|r| r.movable).collect();
        let bounds = bounds_of(&movable);
        let top = f32::from(list.top());
        let rows: Vec<ReorderRow> = s
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| ReorderRow {
                top: f32::from(r.bounds.top()) - top,
                height: f32::from(r.bounds.size.height),
                is_fixed: bounds[i],
            })
            .collect();
        let gap = if rows.len() > 1 {
            (rows[1].top - rows[0].top - rows[0].height).max(0.)
        } else {
            0.
        };
        let row = s.rows[dragged].bounds;
        s.drag = Some(Drag {
            key: key.clone(),
            dragged,
            geometry: ReorderGeometry { rows, gap },
            grab: f32::from(pointer.y - row.top()),
            pointer,
            position: dragged,
            span: (
                f32::from(row.left() - list.left()),
                f32::from(row.size.width),
            ),
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

/// The drop: the move is reported once, and announced once it lands.
fn drop_drag(session: &Entity<Session>, window: &mut Window, cx: &mut App) {
    let Some(d) = session.update(cx, |s, _| {
        s.autoscroll = None;
        s.drag.take()
    }) else {
        return;
    };
    let labels = labels_of(session, cx);
    let m = Messages::new(&labels, cx);
    let total = d.geometry.rows.len();
    if d.position == d.dragged {
        say(m.dropped(d.position + 1, total), window, cx);
        window.refresh();
        return;
    }
    let kept = m.cancelled(d.dragged + 1, total);
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
    let labels = labels_of(session, cx);
    let m = Messages::new(&labels, cx);
    say(
        m.cancelled(d.dragged + 1, d.geometry.rows.len()),
        window,
        cx,
    );
    window.refresh();
}

/// Points per tick toward the edge the dragged row is past: positive down.
fn autoscroll_points(s: &Session, window: &Window) -> f32 {
    let Some(d) = &s.drag else {
        return 0.;
    };
    let (scroll_top, scroll_bottom, position, max) = scroller(&s.host, window);
    let height = d.geometry.rows[d.dragged].height;
    let top = dragged_top(s, d, window);
    let mut step = 0.;
    if position < max {
        step = autoscroll_step(top + height - scroll_bottom);
    }
    if step == 0. && position > 0. {
        step = -autoscroll_step(scroll_top - top);
    }
    step
}

/// Starts the edge autoscroll's timer while the dragged row is near an edge.
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
        session.update(cx, |s, _| {
            if let Some(d) = &mut s.drag {
                d.pointer = position;
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

// ---- The handle ----------------------------------------------------------

/// The handle that ends a reorderable row: a drag target and a toggle
/// (`aria-pressed`) for reorder mode, named "Reorder" and the row's label.
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
    let (in_mode, dragging) = {
        let s = session.read(cx);
        (s.mode.as_ref() == Some(key), s.drag.is_some())
    };
    let labels = rc.reorderable.0.options.labels.clone();
    let m = Messages::new(&labels, cx);
    let name = m.handle(label.as_deref(), position);
    let color = if disabled {
        t.color.icon_disabled
    } else {
        t.color.icon_secondary
    };
    let id = ElementId::Name(format!("{:?}-reorder-{}", rc.id, key_text(key)).into());
    let mut el = grip(color)
        .id(id.clone())
        .debug_selector({
            let id = format!("{}-reorder-{}", rc_name(&rc.id), key_text(key));
            move || id.clone()
        })
        .track_focus(&focus)
        .role(Role::Button)
        .aria_label(name)
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
                    "up" if mode => {
                        cx.stop_propagation();
                        step(&s3, -1, window, cx);
                    }
                    "down" if mode => {
                        cx.stop_propagation();
                        step(&s3, 1, window, cx);
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
    let hint = mode_hint(rc, &m, window, cx);
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

/// The handle's box and its three lines: 28px square, taking only the
/// lines' 14px in its row (the negative block margins), reaching 8px into
/// the row's end padding.
pub(crate) fn grip(color: gpui::Hsla) -> gpui::Div {
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

fn rc_name(id: &ElementId) -> String {
    match id {
        ElementId::Name(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

type PressFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Reorder mode's hint: the moves and Done, as buttons that never take the
/// focus from the handle.
fn mode_hint(rc: &ReorderCx, m: &Messages<'_>, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let name = rc_name(&rc.id);
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
    let s1 = rc.session.clone();
    let s2 = rc.session.clone();
    let s3 = rc.session.clone();
    div()
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
        .whitespace_nowrap()
        .child(button(
            "up",
            Some(m.move_up()),
            Kbd::new("up").into_any_element(),
            Rc::new(move |window, cx| step(&s1, -1, window, cx)),
        ))
        .child(button(
            "down",
            Some(m.move_down()),
            Kbd::new("down").into_any_element(),
            Rc::new(move |window, cx| step(&s2, 1, window, cx)),
        ))
        .child(button(
            "done",
            None,
            div().px(t.spacing.s1).child(m.done()).into_any_element(),
            Rc::new(move |window, cx| end_mode(&s3, window, cx)),
        ))
        .into_any_element()
}

// ---- The overlay ---------------------------------------------------------

/// The collection's last child: the insertion line, the dragged row at the
/// pointer, and the probe that reads where the collection sits and takes
/// the drag's pointer.
pub(crate) fn overlay(rc: &ReorderCx) -> AnyElement {
    Overlay { rc: rc.clone() }.into_any_element()
}

#[derive(IntoElement)]
struct Overlay {
    rc: ReorderCx,
}

impl RenderOnce for Overlay {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let session = self.rc.session.clone();
        let mut layer = div().absolute().top_0().left_0().size_full();
        let line = |left: f32, width: f32, center: f32| {
            div()
                .absolute()
                .left(px(left))
                .top(px(center - INSERTION_LINE_THICKNESS / 2.))
                .w(px(width))
                .h(px(INSERTION_LINE_THICKNESS))
                .rounded(px(INSERTION_LINE_THICKNESS / 2.))
                .bg(t.color.accent)
        };
        let drag = {
            let s = session.read(cx);
            s.drag.as_ref().map(|d| {
                let top = dragged_top(s, d, window);
                let origin = list_top(s);
                let row = d.geometry.rows[d.dragged];
                let position =
                    drop_position(&d.geometry, d.dragged, top + row.height / 2. - origin);
                (
                    top - origin,
                    position,
                    d.span,
                    line_center(&d.geometry, d.dragged, position),
                )
            })
        };
        if let Some((top, position, span, center)) = drag {
            session.update(cx, |s, _| {
                if let Some(d) = &mut s.drag {
                    d.position = position;
                }
            });
            if let Some(center) = center {
                layer = layer.child(line(span.0, span.1, center));
            }
            if let Some(row) = self.rc.preview.borrow_mut().take() {
                layer = layer.child(
                    div()
                        .absolute()
                        .left(px(span.0))
                        .top(px(top))
                        .w(px(span.1))
                        .opacity(MOVING_OPACITY)
                        .bg(t.color.background_surface)
                        .child(row),
                );
            }
        } else {
            let s = session.read(cx);
            if let (Some(key), Some(list)) = (&s.mode, s.list)
                && let Some(at) = index_of(&s.rows, key)
            {
                let origin = f32::from(list.top());
                let rows: Vec<(f32, f32)> = s
                    .rows
                    .iter()
                    .map(|r| {
                        (
                            f32::from(r.bounds.top()) - origin,
                            f32::from(r.bounds.size.height),
                        )
                    })
                    .collect();
                let row = s.rows[at].bounds;
                let center = mode_line_center(&rows, at, s.last_step);
                layer = layer.child(line(
                    f32::from(row.left() - list.left()),
                    f32::from(row.size.width),
                    center,
                ));
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

/// The dragged row's slot: `Some(its height)` when `key` is being dragged.
pub(crate) fn drag_source(rc: &ReorderCx, key: &ItemKey, cx: &App) -> Option<f32> {
    let s = rc.session.read(cx);
    let d = s.drag.as_ref().filter(|d| &d.key == key)?;
    Some(d.geometry.rows[d.dragged].height)
}
