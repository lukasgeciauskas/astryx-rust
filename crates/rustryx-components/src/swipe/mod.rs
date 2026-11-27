//! Swipe actions, upstream `Swipe/` (spec:AST-057): `createSwipe` makes a row's
//! swipe actions, the verbs a sideways drag uncovers on each side, declared as data;
//! [`Item::swipe`](crate::item::Item::swipe) and
//! [`ListItem::swipe`](crate::list::ListItem::swipe) take the value.
//!
//! - [`create_swipe`] is a plain function, pure and cheap: define a swipe once and
//!   hand it to every row. Each action's `on_activate` receives the action's id and
//!   the row's `item_key`, so one definition serves every row (FR1, FR11). The ids
//!   are the swipe's type parameter: an enum of ids makes an unknown id a compile
//!   error, as upstream's literal ids do.
//! - A finger, a pen or a trackpad's two-finger swipe drags the row aside and
//!   uncovers the side's panel; a mouse drag and a mouse wheel's notches never do (FR4,
//!   FR6, AR2). The row's root translates by the travel and the panels
//!   counter-translate, so the row gains no element of its own (FR2).
//! - `reveal` (the default) rests the row open with every entry a real button; a
//!   fast release goes the way it moves, a slow one opens past half the panel, and a
//!   long drag past the confirm point fires the outermost entry, with the row running
//!   ahead and that entry covering its place. `commit` fires the one entry of a side
//!   on a full swipe and rests nowhere (FR3, FR5, DEC-9).
//! - An open row closes on a pointer outside it, a touch tap on its own content
//!   (which activates nothing), a scroll that moves it while focus is outside its
//!   open panel, Escape, a fired entry and unmount (FR7). Directions are logical
//!   (FR8). The rows' container clips in the inline axis: List does (FR9).
//!
//! The gesture is upstream `useSwipeAction` over the shared snap-point drag
//! (`drag_to_snap_points::snap_drag`, upstream `createSnapDrag`), with UIKit's
//! measured thresholds and springs (`motion::uikit::swipe_actions`). A disabled
//! entry is drawn at half strength as one group, upstream's `filter: opacity(0.5)`,
//! through an offscreen layer (fork patch R3), and a Reduce Motion cross-fade fades the
//! row as one group the same way.

mod layer;
mod release;
mod row;

pub mod examples;

#[cfg(test)]
mod tests;

use std::fmt::Debug;
use std::marker::PhantomData;
use std::rc::Rc;

use gpui::{AnyElement, App, SharedString, Window};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::drag_to_snap_points::snap_drag::dev_warn;
use crate::reorderable::ItemKey;

pub(crate) use layer::decorate;
pub(crate) use row::use_swipe_row;

/// Upstream `ItemSwipeActionVariant`: how an action looks. Ordinary, the one you
/// mean, the dangerous one; not a status set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ItemSwipeActionVariant {
    Neutral,
    #[default]
    Accent,
    Destructive,
}

impl ItemSwipeActionVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Accent => "accent",
            Self::Destructive => "destructive",
        }
    }
}

/// Upstream `ItemSwipeBehavior`: what a swipe does, on both sides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ItemSwipeBehavior {
    /// A release opens or closes the row by its speed, then by half the panel; the
    /// open row rests with every entry a real button. A long drag fires the outermost.
    #[default]
    Reveal,
    /// A release past the confirm point slides the row out and fires the one entry of
    /// a side; nothing rests, and the panel is presentational.
    Commit,
}

/// The ids a swipe's actions carry (upstream's literal `Id`): any value that can be
/// compared and printed, such as an enum of the actions or a `&'static str`.
pub trait SwipeId: Clone + PartialEq + Debug + 'static {}

impl<T: Clone + PartialEq + Debug + 'static> SwipeId for T {}

/// What an action's `on_activate` receives: the action's id and the row's
/// `item_key` (a `ListItem`'s; `None` on a plain `Item`).
#[derive(Clone, Debug, PartialEq)]
pub struct SwipeActivation<Id> {
    pub id: Id,
    pub item_key: Option<ItemKey>,
}

/// An action's icon, drawn above its label.
pub type IconFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
/// A deprecated action's handler, which takes nothing.
pub type DeprecatedFn = dyn Fn(&mut Window, &mut App);
type ActionFn<Id> = Rc<dyn Fn(&SwipeActivation<Id>, &mut Window, &mut App)>;
/// An entry's handler as a row runs it: with the row's key.
pub(crate) type EntryFn = Rc<dyn Fn(Option<&ItemKey>, &mut Window, &mut App)>;

/// Upstream `SwipeAction`: one verb, drawn as a filled surface in its variant's color
/// with the icon above the label. Every field is known before the gesture begins.
pub struct SwipeAction<Id> {
    id: Id,
    label: SharedString,
    icon: Option<IconFn>,
    on_activate: ActionFn<Id>,
    disabled: bool,
    variant: ItemSwipeActionVariant,
    has_removal: bool,
}

impl<Id: SwipeId> SwipeAction<Id> {
    /// Upstream `id`, `label` and `onActivate`. The label names the state the verb
    /// reaches ("Unread") and is the button's accessible name while the row rests open.
    /// `on_activate` fires on a tap on a resting entry, on a full swipe, or on a
    /// `commit` release; when the row slides out, once its content has left its box.
    pub fn new(
        id: Id,
        label: impl Into<SharedString>,
        on_activate: impl Fn(&SwipeActivation<Id>, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            icon: None,
            on_activate: Rc::new(on_activate),
            disabled: false,
            variant: ItemSwipeActionVariant::Accent,
            has_removal: false,
        }
    }

    /// Upstream `icon`: drawn above the label.
    pub fn icon(mut self, icon: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.icon = Some(Rc::new(icon));
        self
    }

    rustryx_primitives::setters! {
        /// Upstream `isDisabled`: focusable while the row rests open, but inert; a
        /// disabled outermost entry never confirms a full swipe.
        !disabled,
        /// Upstream `variant` (`accent` by default).
        variant: ItemSwipeActionVariant,
        /// Upstream `hasRemoval`: after this action fires, the row holds out at the edge
        /// it travelled toward instead of springing back. Only that.
        !has_removal,
    }

    fn erase(self) -> SwipeEntry {
        let id = self.id;
        let on_activate = self.on_activate;
        SwipeEntry {
            key: format!("{id:?}").into(),
            label: self.label,
            icon: self.icon,
            on_activate: Rc::new(move |item_key, window, cx| {
                on_activate(
                    &SwipeActivation {
                        id: id.clone(),
                        item_key: item_key.cloned(),
                    },
                    window,
                    cx,
                )
            }),
            disabled: self.disabled,
            variant: self.variant,
            has_removal: self.has_removal,
        }
    }
}

/// Upstream `CreateSwipeOptions`: the verbs a sideways drag uncovers on each side,
/// outermost last, and what a swipe does.
pub struct CreateSwipeOptions<Id> {
    /// Uncovered by a drag toward the inline end (rightward in left-to-right text).
    pub leading: Vec<SwipeAction<Id>>,
    /// Uncovered by a drag toward the inline start (leftward in left-to-right text).
    pub trailing: Vec<SwipeAction<Id>>,
    /// `reveal` by default.
    pub swipe_behavior: ItemSwipeBehavior,
}

impl<Id> Default for CreateSwipeOptions<Id> {
    fn default() -> Self {
        Self {
            leading: Vec::new(),
            trailing: Vec::new(),
            swipe_behavior: ItemSwipeBehavior::Reveal,
        }
    }
}

impl<Id: SwipeId> CreateSwipeOptions<Id> {
    /// No actions yet, `reveal`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `leading`, outermost last.
    pub fn leading(mut self, actions: impl IntoIterator<Item = SwipeAction<Id>>) -> Self {
        self.leading = actions.into_iter().collect();
        self
    }

    /// Upstream `trailing`, outermost last.
    pub fn trailing(mut self, actions: impl IntoIterator<Item = SwipeAction<Id>>) -> Self {
        self.trailing = actions.into_iter().collect();
        self
    }

    /// Upstream `swipeBehavior`.
    pub fn swipe_behavior(mut self, behavior: ItemSwipeBehavior) -> Self {
        self.swipe_behavior = behavior;
        self
    }
}

/// Upstream `Swipe`: a row's configured swipe actions, made only by [`create_swipe`]
/// and read only by the rows that take it; nothing in it is public. Its type carries
/// its actions' ids. Cheap to clone.
pub struct Swipe<Id = SharedString> {
    pub(crate) host: Rc<SwipeHost>,
    ids: PhantomData<fn() -> Id>,
}

impl<Id> Clone for Swipe<Id> {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            ids: PhantomData,
        }
    }
}

/// Upstream `createSwipe`: a row's swipe actions. Pass the result to
/// [`Item::swipe`](crate::item::Item::swipe) or
/// [`ListItem::swipe`](crate::list::ListItem::swipe). A committing side with two or
/// more entries fires none of them and warns in development.
///
/// ```ignore
/// #[derive(Clone, Copy, Debug, PartialEq)]
/// enum Mail { Archive, Delete }
/// let mail = create_swipe(CreateSwipeOptions::new().trailing([
///     SwipeAction::new(Mail::Archive, "Archive", |a, _, _| archive(a.item_key.clone())),
///     SwipeAction::new(Mail::Delete, "Delete", |a, _, _| remove(a.item_key.clone()))
///         .variant(ItemSwipeActionVariant::Destructive)
///         .has_removal(true),
/// ]));
/// ListItem::new("inbox", "Inbox").item_key(message.id).swipe(mail.clone())
/// ```
pub fn create_swipe<Id: SwipeId>(options: CreateSwipeOptions<Id>) -> Swipe<Id> {
    let behavior = options.swipe_behavior;
    let leading: Vec<SwipeEntry> = options
        .leading
        .into_iter()
        .map(SwipeAction::erase)
        .collect();
    let trailing: Vec<SwipeEntry> = options
        .trailing
        .into_iter()
        .map(SwipeAction::erase)
        .collect();
    if behavior == ItemSwipeBehavior::Commit && (leading.len() > 1 || trailing.len() > 1) {
        // A committing side never rests open, so a full swipe confirms from no open row.
        dev_warn(
            "createSwipe",
            "`swipeBehavior: 'commit'` fires no entry on a side with two or more entries. \
             Give a committing side one entry, or use `swipeBehavior: 'reveal'`.",
        );
    }
    Swipe {
        host: Rc::new(SwipeHost {
            leading,
            trailing,
            behavior,
            item_key: None,
        }),
        ids: PhantomData,
    }
}

/// An action as a row runs it (upstream `SwipeEntry`): one `create_swipe` made, or an
/// entry of the deprecated `swipe_actions`.
#[derive(Clone)]
pub(crate) struct SwipeEntry {
    /// The id as text, what the row keys the entry and its panel on ("" for none).
    pub key: SharedString,
    pub label: SharedString,
    pub icon: Option<IconFn>,
    pub on_activate: EntryFn,
    pub disabled: bool,
    pub variant: ItemSwipeActionVariant,
    pub has_removal: bool,
}

/// What a row knows about itself that decides whether swipe actions serve it
/// (upstream `SwipeRowFacts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SwipeRowFacts {
    /// A parent role given to the row (a menu item, an option); `None` for none.
    pub role: Option<&'static str>,
    pub disabled: bool,
    /// The row is the enlarged target of a control inside it.
    pub delegate: bool,
    /// The root is itself a link.
    pub link_root: bool,
}

/// A `Swipe` value as a row reads it (upstream `SwipeHost`).
pub(crate) struct SwipeHost {
    pub leading: Vec<SwipeEntry>,
    pub trailing: Vec<SwipeEntry>,
    pub behavior: ItemSwipeBehavior,
    /// The `item_key` of the `ListItem` the row is.
    pub item_key: Option<ItemKey>,
}

/// Roles whose content model permits interactive descendants, so a swipe panel's
/// buttons may live inside the row; a row with no role qualifies too.
const SWIPE_ROLES: &[&str] = &["listitem"];

impl SwipeHost {
    /// Whether swipe actions serve a row with these facts: some side has an entry, and
    /// the row is not disabled, not a nested control's enlarged target, not a link
    /// root, and has a role that permits interactive descendants (DEC-3).
    pub fn serves(&self, facts: SwipeRowFacts) -> bool {
        (!self.leading.is_empty() || !self.trailing.is_empty())
            && !facts.disabled
            && !facts.delegate
            && !facts.link_root
            && facts.role.is_none_or(|role| SWIPE_ROLES.contains(&role))
    }

    /// The actions' ids side by side: what tells a change of actions (FR11).
    pub fn ids_key(&self) -> String {
        let ids = |entries: &[SwipeEntry]| {
            entries
                .iter()
                .map(|e| e.key.to_string())
                .collect::<Vec<_>>()
                .join("\u{0}")
        };
        format!("{}\u{1}{}", ids(&self.leading), ids(&self.trailing))
    }

    pub fn entries(&self, side: release::SwipeSide) -> &[SwipeEntry] {
        match side {
            release::SwipeSide::Leading => &self.leading,
            release::SwipeSide::Trailing => &self.trailing,
        }
    }
}

thread_local! {
    #[cfg(test)]
    pub(crate) static KEYED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The value a `ListItem` hands its `Item` (upstream `swipeWithItemKey`): the same
/// actions, attached with the row's key.
pub(crate) fn swipe_with_item_key(
    host: &Rc<SwipeHost>,
    item_key: Option<&ItemKey>,
) -> Rc<SwipeHost> {
    #[cfg(test)]
    KEYED.with(|k| k.set(k.get() + 1));
    match item_key {
        None => host.clone(),
        Some(key) => Rc::new(SwipeHost {
            leading: host.leading.clone(),
            trailing: host.trailing.clone(),
            behavior: host.behavior,
            item_key: Some(key.clone()),
        }),
    }
}

/// Upstream `ItemSwipeAction` (deprecated: use [`SwipeAction`], whose id is required and
/// whose `on_activate` receives the id and the row's key).
#[derive(Clone)]
pub struct ItemSwipeAction {
    pub id: Option<SharedString>,
    pub label: SharedString,
    pub icon: Option<IconFn>,
    pub on_activate: Rc<DeprecatedFn>,
    pub is_disabled: bool,
    pub variant: ItemSwipeActionVariant,
    pub has_removal: bool,
}

impl ItemSwipeAction {
    /// An action with a label and a handler, `accent`, enabled, no removal.
    pub fn new(
        label: impl Into<SharedString>,
        on_activate: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: None,
            label: label.into(),
            icon: None,
            on_activate: Rc::new(on_activate),
            is_disabled: false,
            variant: ItemSwipeActionVariant::Accent,
            has_removal: false,
        }
    }

    fn entry(&self) -> SwipeEntry {
        let on_activate = self.on_activate.clone();
        SwipeEntry {
            key: self.id.clone().unwrap_or_default(),
            label: self.label.clone(),
            icon: self.icon.clone(),
            on_activate: Rc::new(move |_, window, cx| on_activate(window, cx)),
            disabled: self.is_disabled,
            variant: self.variant,
            has_removal: self.has_removal,
        }
    }
}

/// Upstream `ItemSwipeActions` (deprecated: use [`CreateSwipeOptions`]).
#[derive(Clone, Default)]
pub struct ItemSwipeActions {
    pub leading: Vec<ItemSwipeAction>,
    pub trailing: Vec<ItemSwipeAction>,
}

/// The deprecated path's host (spec:AST-057 DEP-0017): the released props, with the
/// swipe's current behavior and the released `commit` warning.
pub(crate) fn deprecated_host(
    actions: &ItemSwipeActions,
    behavior: Option<ItemSwipeBehavior>,
) -> Rc<SwipeHost> {
    let behavior = behavior.unwrap_or_default();
    if behavior == ItemSwipeBehavior::Commit
        && (actions.leading.len() > 1 || actions.trailing.len() > 1)
    {
        dev_warn(
            "Item",
            "`swipeBehavior=\"commit\"` fires no entry on a side with two or more entries. \
             Give a committing side one entry, or use `swipeBehavior=\"reveal\"`.",
        );
    }
    Rc::new(SwipeHost {
        leading: actions.leading.iter().map(ItemSwipeAction::entry).collect(),
        trailing: actions
            .trailing
            .iter()
            .map(ItemSwipeAction::entry)
            .collect(),
        behavior,
        item_key: None,
    })
}

/// Which swipe a row runs: `swipe` wins over the deprecated props, with a development
/// note when both are given.
pub(crate) fn resolve_row_swipe(
    swipe: Option<Rc<SwipeHost>>,
    actions: Option<&ItemSwipeActions>,
    behavior: Option<ItemSwipeBehavior>,
) -> Option<Rc<SwipeHost>> {
    match swipe {
        Some(host) => {
            if actions.is_some() || behavior.is_some() {
                dev_warn(
                    "Item",
                    "`swipe` is set, so the deprecated `swipeActions` and `swipeBehavior` are ignored.",
                );
            }
            Some(host)
        }
        None => actions.map(|a| deprecated_host(a, behavior)),
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CREATE_SWIPE_DOC: ComponentDoc = ComponentDoc {
    name: "createSwipe",
    rust_type: "rustryx::components::swipe::create_swipe -> Swipe<Id>",
    props: &[
        p(
            "options.leading",
            RustApi::Method(
                "CreateSwipeOptions::leading (SwipeAction::new(id, label, on_activate))",
            ),
        ),
        p(
            "options.trailing",
            RustApi::Method("CreateSwipeOptions::trailing"),
        ),
        p(
            "options.swipeBehavior",
            RustApi::Method("CreateSwipeOptions::swipe_behavior (ItemSwipeBehavior)"),
        ),
    ],
    example: include_str!("examples.rs"),
};
