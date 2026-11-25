//! Reordering a collection's items (upstream `createReorderable`,
//! `spec:AST-103`). One object carries the behavior; a collection takes it
//! through its `reorderable` prop (List's [`List::reorderable`]), and each
//! row's key comes from the data (ListItem's `item_key`).
//!
//! Every collection drags the same way: a drag from a row's handle, the
//! source dimmed in place, a preview following the pointer, an accent
//! insertion line at the candidate and edge autoscroll; one report at the
//! drop. The handle is also a toggle: reorder mode moves the row a place per
//! arrow key, each move reported and announced, with a hint of the moves.
//!
//! [`List::reorderable`]: crate::list::List::reorderable

pub mod examples;
pub(crate) mod model;
pub(crate) mod session;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Div, ElementId, IntoElement, ParentElement, Pixels, Point, RenderOnce,
    SharedString, Styled, WeakEntity, Window, div, prelude::*,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

pub use astryx_primitives::ListWindowKey as ItemKey;

/// A committed move: the item that moved and where it went.
#[derive(Clone, Debug, PartialEq)]
pub struct ReorderMove {
    /// The moved item's key: its ListItem's `item_key`.
    pub key: ItemKey,
    /// Its index after the move, in the full order of the collection's items.
    pub to: usize,
}

/// How a drag previews its landing place (FR9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FeedbackBehavior {
    /// The source dims in place, a preview follows the pointer and an accent
    /// line marks the candidate.
    #[default]
    InsertionLine,
    /// The neighbours slide aside to open the gap where the item will land;
    /// the dragged item itself rides the pointer.
    Shift,
}

/// Every string the object announces or shows (FR1 `labels`). An absent key
/// falls back to the catalog; a given one replaces it, taking the catalog
/// message's ICU arguments.
#[derive(Clone, Debug, Default)]
pub struct ReorderableLabels {
    /// `{position}`, `{total}`.
    pub picked_up: Option<SharedString>,
    /// `{position}`, `{total}`.
    pub dropped: Option<SharedString>,
    /// `{position}`, `{total}`.
    pub cancelled: Option<SharedString>,
    /// `{label}`, `{position}`, `{total}`, `{previous}`, `{next}`.
    pub place: Option<SharedString>,
    /// `{label}`, `{position}`, `{total}`, `{next}`.
    pub place_at_top: Option<SharedString>,
    /// `{label}`, `{position}`, `{total}`, `{previous}`.
    pub place_at_end: Option<SharedString>,
    /// `{label}`, `{row}`, `{column}`, `{position}`, `{total}`.
    pub place_in_grid: Option<SharedString>,
    /// `{label}`, `{parent}`, `{position}`, `{total}`.
    pub place_in_tree: Option<SharedString>,
    /// `{label}`.
    pub refused: Option<SharedString>,
    /// `{label}`.
    pub not_moved: Option<SharedString>,
    /// `{label}`, `{position}`, `{total}`.
    pub mode_done: Option<SharedString>,
    /// `{position}`.
    pub unnamed_item: Option<SharedString>,
    /// `{label}`.
    pub handle: Option<SharedString>,
    pub handle_verb: Option<SharedString>,
    /// `{position}`.
    pub unnamed_handle: Option<SharedString>,
    pub handle_hint: Option<SharedString>,
    pub move_up: Option<SharedString>,
    pub move_down: Option<SharedString>,
    pub move_left: Option<SharedString>,
    pub move_right: Option<SharedString>,
    pub done: Option<SharedString>,
}

type MoveFn = Rc<dyn Fn(&ReorderMove, &mut Window, &mut App)>;
type KeyFn = Rc<dyn Fn(&ItemKey) -> bool>;
type PositionsFn = Rc<dyn Fn() -> Vec<Point<Pixels>>>;

/// What [`create_reorderable`] takes.
#[derive(Clone)]
pub struct ReorderableOptions {
    pub(crate) on_reorder: MoveFn,
    pub(crate) is_reorderable: Option<KeyFn>,
    pub(crate) positions: Option<PositionsFn>,
    pub(crate) feedback: FeedbackBehavior,
    pub(crate) labels: ReorderableLabels,
}

impl ReorderableOptions {
    /// `on_reorder` is called once per committed move: a drop, or each move
    /// in reorder mode. Apply it by rendering the items in their new order;
    /// a cancelled move calls nothing.
    pub fn new(on_reorder: impl Fn(&ReorderMove, &mut Window, &mut App) + 'static) -> Self {
        Self {
            on_reorder: Rc::new(on_reorder),
            is_reorderable: None,
            positions: None,
            feedback: FeedbackBehavior::InsertionLine,
            labels: ReorderableLabels::default(),
        }
    }

    /// Whether the item with `key` can move. One it rejects shows no handle;
    /// one at either end bounds the others.
    pub fn is_reorderable(mut self, f: impl Fn(&ItemKey) -> bool + 'static) -> Self {
        self.is_reorderable = Some(Rc::new(f));
        self
    }

    /// Drop positions for a layout the measured one does not place (a fanned
    /// hand): one slot centre per item, in item order, from the top-left of
    /// the collection's box. An array of another length is ignored.
    pub fn positions(mut self, f: impl Fn() -> Vec<Point<Pixels>> + 'static) -> Self {
        self.positions = Some(Rc::new(f));
        self
    }

    /// How a drag previews its landing place.
    pub fn feedback_behavior(mut self, feedback: FeedbackBehavior) -> Self {
        self.feedback = feedback;
        self
    }

    /// The strings it announces and shows.
    pub fn labels(mut self, labels: ReorderableLabels) -> Self {
        self.labels = labels;
        self
    }
}

/// A collection's reorder behavior: pass it as the collection's
/// `reorderable`. Made once per collection, or again on each render: the
/// collection uses the latest callbacks.
#[derive(Clone)]
pub struct Reorderable(pub(crate) Rc<ReorderableInner>);

pub(crate) struct ReorderableInner {
    pub options: ReorderableOptions,
    /// The collections it was rendered with, in the order they mounted.
    pub sessions: RefCell<Vec<WeakEntity<session::Session>>>,
}

/// Makes a collection's items reorderable (FR1).
///
/// ```ignore
/// let reorderable = create_reorderable(ReorderableOptions::new(move |m, window, _| {
///     tasks.borrow_mut().move_item(&m.key, m.to);
///     window.refresh();
/// }));
/// List::new("tasks").reorderable(reorderable).children(rows)
/// ```
pub fn create_reorderable(options: ReorderableOptions) -> Reorderable {
    Reorderable(Rc::new(ReorderableInner {
        options,
        sessions: RefCell::new(Vec::new()),
    }))
}

impl Reorderable {
    /// Enters reorder mode for the item with `key` from the app's own UI
    /// (FR4a): its handle takes the focus and scrolls into view, and the
    /// mode's keys, hint, announcements and exits follow. A key that cannot
    /// move or is not mounted does nothing.
    pub fn start(&self, key: impl Into<ItemKey>, window: &mut Window, cx: &mut App) {
        session::start(self, key.into(), window, cx);
    }

    /// FR4b: reorders an app's own elements, with no collection component.
    /// Call it once a frame while they render; stop calling it to detach,
    /// which ends a drag or reorder mode in progress and reports nothing.
    ///
    /// The container takes the items as its children, in order
    /// ([`ReorderAttachment::container`]); each item is wrapped with
    /// [`ReorderAttachment::item`], and its handle, an element of the app's,
    /// with [`ReorderAttachment::handle`]. A move the app does not apply is
    /// refused and announced ("Can't move King of hearts").
    pub fn attach(
        &self,
        id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> ReorderAttachment {
        ReorderAttachment {
            rc: session::begin(&id.into(), self, true, window, cx),
        }
    }

    pub(crate) fn can_move(&self, key: &ItemKey) -> bool {
        self.0
            .options
            .is_reorderable
            .as_ref()
            .is_none_or(|f| f(key))
    }
}

/// A frame of [`Reorderable::attach`]: wraps an app's container, items and
/// handles.
#[derive(Clone)]
pub struct ReorderAttachment {
    rc: session::ReorderCx,
}

impl ReorderAttachment {
    /// The container, laid out as the app likes (a column, a row, a
    /// wrapping grid): its children are the `count` items, in order.
    pub fn container(&self, container: Div, count: usize) -> AnyElement {
        let session = self.rc.session.clone();
        container
            .relative()
            .on_children_prepainted(move |bounds, window, cx| {
                session::finalize(&session, &bounds, count, window, cx)
            })
            .child(session::overlay(&self.rc))
            .into_any_element()
    }

    /// The item at `index` with `key`, named `name` in announcements and
    /// its handle's name. A rejected key moves nothing.
    pub fn item(
        &self,
        index: usize,
        key: impl Into<ItemKey>,
        name: impl Into<SharedString>,
        element: impl IntoElement,
        cx: &mut App,
    ) -> AnyElement {
        let key = key.into();
        let movable = self.rc.reorderable.can_move(&key);
        session::register(
            &self.rc,
            session::Registration {
                index,
                key: key.clone(),
                name: Some(name.into()),
                movable,
            },
            cx,
        );
        session::place_item(
            &self.rc,
            &key,
            index,
            element.into_any_element(),
            || None,
            cx,
        )
    }

    /// The item's handle: the app's `element`, given the handle's role,
    /// name ("Reorder" and the item's name), toggle state, Tab stop, drag
    /// and keys. A rejected item's handle stays as it was.
    pub fn handle(&self, key: impl Into<ItemKey>, element: impl IntoElement) -> AnyElement {
        AttachedHandle {
            rc: self.rc.clone(),
            key: key.into(),
            element: element.into_any_element(),
        }
        .into_any_element()
    }
}

/// An attached handle, named once its item has registered.
#[derive(IntoElement)]
struct AttachedHandle {
    rc: session::ReorderCx,
    key: ItemKey,
    element: AnyElement,
}

impl RenderOnce for AttachedHandle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if !self.rc.reorderable.can_move(&self.key) {
            return self.element;
        }
        let (name, position) = session::registered(&self.rc, &self.key, cx);
        let body = div()
            .id(ElementId::Name(
                format!("{:?}-reorder-{}", self.rc.id, session::key_text(&self.key)).into(),
            ))
            .child(self.element);
        session::handle_on(&self.rc, &self.key, name, position, false, body, window, cx)
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CREATE_REORDERABLE_DOC: ComponentDoc = ComponentDoc {
    name: "createReorderable",
    rust_type: "astryx::components::reorderable::create_reorderable",
    props: &[
        p(
            "onReorder",
            RustApi::Argument("ReorderableOptions::new(on_reorder)"),
        ),
        p(
            "isReorderable",
            RustApi::Method("ReorderableOptions::is_reorderable"),
        ),
        p("reorderable", RustApi::Method("List::reorderable")),
    ],
    example: include_str!("examples.rs"),
};
