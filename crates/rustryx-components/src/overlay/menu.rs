//! What every menu shares, upstream `DropdownMenu/`'s parts: the entries a
//! menu holds, and the list that draws and drives them. DropdownMenu,
//! MoreMenu and ContextMenu open it.
//!
//! - Entries: actions (an icon, a description, end content, a link, or a
//!   submenu), checkbox rows, radio groups, titled sections and dividers.
//! - Keys: arrows move through the rows, wrapping, with Home and End;
//!   PageDown and PageUp go to the last and first fully visible row, then a
//!   viewport further; typing jumps to a row by its label. Disabled rows are
//!   reached and stay inert (FR17). Tab leaves the whole menu (FR23).
//! - The pointer: the row under it takes focus. A submenu opens on hover
//!   after 150 ms and stays open while the pointer travels the safe triangle
//!   toward it (FR13); leaving closes it after 200 ms.
//! - The press model (FR5, upstream `useMenuPress`, [`crate::gestures::menu_press`]): the
//!   row under the release acts and the highlight follows a held pointer; the opening
//!   press picks once it settled, a finger's once it travelled UIMenu's handoff; a mouse
//!   released outside closes the menu, a finger leaves it open; a held finger lights the
//!   row under it; a resting pointer near the edge of a scrolling menu scrolls it.
//! - Drill-in (FR11) and the touch sheet: a submenu replaces the rows, behind
//!   a Back row (or the sheet's Back button).

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Bounds, ClickEvent, DispatchPhase, ElementId, Entity, FocusHandle, HasPopup,
    IntoElement, KeyDownEvent, Length, MouseButton, MouseMoveEvent, ParentElement, Pixels, Point,
    RenderOnce, Role, ScrollHandle, SharedString, Styled, Task, Window, div, point, prelude::*, px,
    relative,
};
use rustryx_i18n::{Arg, LogicalStyled, current as i18n, use_direction};
use rustryx_primitives::{
    ElementSize, FocusNext, FocusPrevious, LayerAlign, LayerOptions, ListFocus, ListKeys,
    Placement, Typeahead, anchored_layer, is_focus_visible, use_link_handler,
};
use rustryx_theme::{ThemeStyled, current};
use web_time::Instant;

use crate::button::{ButtonVariant, IconButton};
use crate::focus::focus_ring_inset;
use crate::gestures::menu_press::{MenuPress, MenuPressRow, MenuRelease};
use crate::icon::{Icon, IconColor, IconSize, IconSource};
use crate::indicator::{CheckboxIndicator, ChoiceSize, RadioIndicator};
use crate::item::Item;
use crate::text::{Heading, HeadingLevel};

/// Upstream `MENU_BLOCK_CAP`: a menu level's greatest height.
pub(crate) fn menu_block_cap() -> Pixels {
    px(300.)
}
/// Upstream `useMenuHover`'s `showDelay`.
const SUBMENU_SHOW_DELAY: Duration = Duration::from_millis(150);
/// Upstream `useMenuHover`'s `hideDelay`.
const SUBMENU_HIDE_DELAY: Duration = Duration::from_millis(200);
/// Upstream `DEFAULT_CLICK_GUARD_MS`: a click this soon after a hover opened
/// a submenu keeps it open.
const SUBMENU_CLICK_GUARD: Duration = Duration::from_millis(500);
/// Upstream `SAFE_TRIANGLE_PADDING_PX`.
const SAFE_TRIANGLE_PADDING: f32 = 8.;

/// What a menu row does.
pub type MenuAction = Rc<dyn Fn(&mut Window, &mut App)>;
type CheckFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type ValueFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type BuildFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
/// Activates a row; the flag is a modified click.
type ActivateFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

// ----------------------------------------------------------------------------
// Entries

/// An action row, upstream `DropdownMenuItem` (a data item `{label,
/// onClick, ...}`), or with entries a submenu (`DropdownMenuSubMenu`).
#[derive(Clone)]
pub struct MenuItem {
    pub(crate) id: SharedString,
    pub(crate) label: SharedString,
    icon: Option<IconSource>,
    description: Option<SharedString>,
    on_click: Option<MenuAction>,
    href: Option<SharedString>,
    target: Option<SharedString>,
    disabled: bool,
    destructive: bool,
    close_on_select: bool,
    end_content: Option<BuildFn>,
    submenu: Vec<MenuEntry>,
    submenu_width: Option<Length>,
    submenu_presentation: SubmenuPresentation,
    spinner: bool,
    show_delay: Duration,
    hide_delay: Duration,
    on_submenu_open_change: Option<CheckFn>,
    submenu_header: Option<BuildFn>,
    submenu_footer: Option<BuildFn>,
}

/// Upstream `DropdownMenuSubMenu`'s `presentation`: where its rows show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubmenuPresentation {
    /// Beside the row.
    Flyout,
    /// In place of the menu's rows, behind a Back row.
    DrillIn,
    /// Drills in on a compact touch screen, else flies out.
    #[default]
    Adaptive,
}

impl MenuItem {
    /// Upstream `id` and `label`.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        MenuItem {
            id: id.into(),
            label: label.into(),
            icon: None,
            description: None,
            on_click: None,
            href: None,
            target: None,
            disabled: false,
            destructive: false,
            close_on_select: true,
            end_content: None,
            submenu: Vec::new(),
            submenu_width: None,
            submenu_presentation: SubmenuPresentation::Adaptive,
            spinner: false,
            show_delay: SUBMENU_SHOW_DELAY,
            hide_delay: SUBMENU_HIDE_DELAY,
            on_submenu_open_change: None,
            submenu_header: None,
            submenu_footer: None,
        }
    }

    /// Upstream `icon`: a semantic icon name or an asset.
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `onClick`.
    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `href`: the row opens it through the link handler (FR10).
    pub fn href(mut self, href: impl Into<SharedString>) -> Self {
        self.href = Some(href.into());
        self
    }

    /// Upstream `target`, with `href`.
    pub fn target(mut self, target: impl Into<SharedString>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// Upstream `isDisabled`: reached by the keyboard, never acts.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `variant: 'destructive'`.
    pub fn destructive(mut self, destructive: bool) -> Self {
        self.destructive = destructive;
        self
    }

    /// Upstream `hasCloseOnSelect`: on by default.
    pub fn close_on_select(mut self, close: bool) -> Self {
        self.close_on_select = close;
        self
    }

    /// Upstream `endContent`: a shortcut hint or a badge.
    pub fn end_content(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.end_content = Some(Rc::new(f));
        self
    }

    /// Upstream `items` (`DropdownMenuSubMenu`): the row opens a submenu.
    pub fn submenu(mut self, entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        self.submenu = entries.into_iter().collect();
        self
    }

    /// Upstream submenu `menuWidth`: the flyout's least width (else its
    /// content's, at least 160px).
    pub fn submenu_width(mut self, width: impl Into<Length>) -> Self {
        self.submenu_width = Some(width.into());
        self
    }

    /// Upstream submenu `presentation`.
    pub fn submenu_presentation(mut self, presentation: SubmenuPresentation) -> Self {
        self.submenu_presentation = presentation;
        self
    }

    /// Upstream submenu `hasSpinner`: a spinner instead of the chevron, for
    /// rows still loading.
    pub fn spinner(mut self, spinner: bool) -> Self {
        self.spinner = spinner;
        self
    }

    /// Upstream submenu `delay`: the hover wait before the flyout opens
    /// (150 ms).
    pub fn submenu_delay(mut self, delay: Duration) -> Self {
        self.show_delay = delay;
        self
    }

    /// Upstream submenu `hideDelay`: how long the flyout stays after the
    /// pointer leaves (200 ms).
    pub fn submenu_hide_delay(mut self, delay: Duration) -> Self {
        self.hide_delay = delay;
        self
    }

    /// Upstream submenu `onOpenChange`.
    pub fn on_submenu_open_change(
        mut self,
        f: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_submenu_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream submenu `header`: above its rows; the flyout becomes a modal
    /// dialog named by the row (FR20). A ContextMenu shows the rows alone.
    pub fn submenu_header(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.submenu_header = Some(Rc::new(f));
        self
    }

    /// Upstream submenu `footer`: below its rows, as `submenu_header`.
    pub fn submenu_footer(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.submenu_footer = Some(Rc::new(f));
        self
    }
}

/// A checkbox row, upstream `DropdownMenuCheckboxItem`.
#[derive(Clone)]
pub struct MenuCheckboxItem {
    id: SharedString,
    label: SharedString,
    icon: Option<IconSource>,
    description: Option<SharedString>,
    checked: bool,
    on_change: Option<CheckFn>,
    disabled: bool,
    close_on_select: bool,
    end_content: Option<BuildFn>,
}

impl MenuCheckboxItem {
    /// Upstream `label` and `value`.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, checked: bool) -> Self {
        MenuCheckboxItem {
            id: id.into(),
            label: label.into(),
            icon: None,
            description: None,
            checked,
            on_change: None,
            disabled: false,
            close_on_select: false,
            end_content: None,
        }
    }

    /// Upstream `icon`.
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `onChange`.
    pub fn on_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `isDisabled`.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `hasCloseOnSelect`: off by default.
    pub fn close_on_select(mut self, close: bool) -> Self {
        self.close_on_select = close;
        self
    }

    /// Upstream `endContent`.
    pub fn end_content(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.end_content = Some(Rc::new(f));
        self
    }
}

/// A radio row, upstream `DropdownMenuRadioItem`.
#[derive(Clone)]
pub struct MenuRadioItem {
    value: SharedString,
    label: SharedString,
    icon: Option<IconSource>,
    description: Option<SharedString>,
    disabled: bool,
    end_content: Option<BuildFn>,
}

impl MenuRadioItem {
    /// Upstream `value` and `label`.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        MenuRadioItem {
            value: value.into(),
            label: label.into(),
            icon: None,
            description: None,
            disabled: false,
            end_content: None,
        }
    }

    /// Upstream `icon`.
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `isDisabled`.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `endContent`.
    pub fn end_content(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.end_content = Some(Rc::new(f));
        self
    }
}

/// One choice among radio rows, upstream `DropdownMenuRadioGroup`.
#[derive(Clone)]
pub struct MenuRadioGroup {
    title: Option<SharedString>,
    value: SharedString,
    on_change: Option<ValueFn>,
    items: Vec<MenuRadioItem>,
    close_on_select: bool,
    check_marks: bool,
}

impl MenuRadioGroup {
    /// Upstream `value`: the selected row's value.
    pub fn new(value: impl Into<SharedString>) -> Self {
        MenuRadioGroup {
            title: None,
            value: value.into(),
            on_change: None,
            items: Vec::new(),
            close_on_select: true,
            check_marks: false,
        }
    }

    /// Upstream `label`: a heading over the rows, and the group's name.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Upstream `onChange`.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream compound `children`: one radio row.
    pub fn item(mut self, item: MenuRadioItem) -> Self {
        self.items.push(item);
        self
    }

    /// Upstream `hasCloseOnSelect`: on by default.
    pub fn close_on_select(mut self, close: bool) -> Self {
        self.close_on_select = close;
        self
    }

    /// TabMenu's look: a check mark at the end of the selected row instead
    /// of radio markers, roomier rows and a semibold heading.
    pub(crate) fn check_marks(mut self) -> Self {
        self.check_marks = true;
        self
    }
}

/// One entry of a menu, upstream `DropdownMenuOption`.
#[derive(Clone)]
pub enum MenuEntry {
    Item(MenuItem),
    Checkbox(MenuCheckboxItem),
    RadioGroup(MenuRadioGroup),
    /// Upstream `DropdownMenuDivider` (`{type: 'divider'}`).
    Divider,
    /// Upstream `DropdownMenuGroup` (`{type: 'section', title, items}`): a
    /// group named by its title; untitled, its rows have no heading.
    Section {
        title: Option<SharedString>,
        entries: Vec<MenuEntry>,
    },
}

impl MenuEntry {
    /// A titled section.
    pub fn section(
        title: impl Into<SharedString>,
        entries: impl IntoIterator<Item = MenuEntry>,
    ) -> Self {
        MenuEntry::Section {
            title: Some(title.into()),
            entries: entries.into_iter().collect(),
        }
    }
}

impl From<MenuItem> for MenuEntry {
    fn from(item: MenuItem) -> Self {
        MenuEntry::Item(item)
    }
}

impl From<MenuCheckboxItem> for MenuEntry {
    fn from(item: MenuCheckboxItem) -> Self {
        MenuEntry::Checkbox(item)
    }
}

impl From<MenuRadioGroup> for MenuEntry {
    fn from(group: MenuRadioGroup) -> Self {
        MenuEntry::RadioGroup(group)
    }
}

/// Upstream `size`: how dense the rows are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl MenuSize {
    pub fn as_str(self) -> &'static str {
        match self {
            MenuSize::Sm => "sm",
            MenuSize::Md => "md",
            MenuSize::Lg => "lg",
        }
    }
}

// ----------------------------------------------------------------------------
// Geometry

/// Whether `p` is inside the triangle `a`, `b`, `c` (edges included).
fn in_triangle(p: Point<Pixels>, a: Point<Pixels>, b: Point<Pixels>, c: Point<Pixels>) -> bool {
    let f = f32::from;
    let sign = |p1: Point<Pixels>, p2: Point<Pixels>, p3: Point<Pixels>| {
        (f(p1.x) - f(p3.x)) * (f(p2.y) - f(p3.y)) - (f(p2.x) - f(p3.x)) * (f(p1.y) - f(p3.y))
    };
    let d1 = sign(p, a, b);
    let d2 = sign(p, b, c);
    let d3 = sign(p, c, a);
    let negative = d1 < 0. || d2 < 0. || d3 < 0.;
    let positive = d1 > 0. || d2 > 0. || d3 > 0.;
    !(negative && positive)
}

/// Upstream `isPointInSafeTriangle`: whether `p` is inside the triangle
/// from `origin` (where the pointer left the submenu's row) to the flyout's
/// near edge, padded at its ends. A pointer inside is heading for the
/// flyout.
pub(crate) fn in_safe_triangle(
    p: Point<Pixels>,
    origin: Point<Pixels>,
    flyout: Bounds<Pixels>,
) -> bool {
    let pad = px(SAFE_TRIANGLE_PADDING);
    let (left, right) = (flyout.left(), flyout.right());
    let (top, bottom) = (flyout.top(), flyout.bottom());
    let (start, end) = if origin.x <= left {
        (point(left, top - pad), point(left, bottom + pad))
    } else if origin.x >= right {
        (point(right, top - pad), point(right, bottom + pad))
    } else if origin.y <= top {
        (point(left - pad, top), point(right + pad, top))
    } else if origin.y >= bottom {
        (point(left - pad, bottom), point(right + pad, bottom))
    } else {
        return false;
    };
    in_triangle(p, origin, start, end)
}

/// FR12's PageDown (`down`) and PageUp: the last (first) fully visible row,
/// and from there the row a viewport further, never wrapping. `rows` are
/// every row's bounds in order, `viewport` the scrolled area.
pub(crate) fn page_target(
    rows: &[Bounds<Pixels>],
    viewport: Bounds<Pixels>,
    current: Option<usize>,
    down: bool,
) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }
    let slack = px(0.5);
    let last = rows.len() - 1;
    let within = |b: &Bounds<Pixels>, top: Pixels, bottom: Pixels| {
        b.top() >= top - slack && b.bottom() <= bottom + slack
    };
    let (top, bottom) = (viewport.top(), viewport.bottom());
    let height = viewport.size.height;
    if down {
        let edge = rows
            .iter()
            .rposition(|b| within(b, top, bottom))
            .unwrap_or(0);
        match current {
            Some(c) if c >= edge => {
                let limit = bottom + height;
                let target = rows
                    .iter()
                    .rposition(|b| b.bottom() <= limit + slack)
                    .unwrap_or(c);
                Some(if target > c {
                    target
                } else {
                    (c + 1).min(last)
                })
            }
            _ => Some(edge),
        }
    } else {
        let edge = rows
            .iter()
            .position(|b| within(b, top, bottom))
            .unwrap_or(last);
        match current {
            Some(c) if c <= edge => {
                let limit = top - height;
                let target = rows
                    .iter()
                    .position(|b| b.top() >= limit - slack)
                    .unwrap_or(c);
                Some(if target < c {
                    target
                } else {
                    c.saturating_sub(1)
                })
            }
            _ => Some(edge),
        }
    }
}

/// Scrolls `scroll` so `row` is fully in view.
fn reveal(scroll: &ScrollHandle, row: Bounds<Pixels>) {
    let viewport = scroll.bounds();
    let mut offset = scroll.offset();
    if row.top() < viewport.top() {
        offset.y += viewport.top() - row.top();
    } else if row.bottom() > viewport.bottom() {
        offset.y -= row.bottom() - viewport.bottom();
    } else {
        return;
    }
    scroll.set_offset(offset);
}

// ----------------------------------------------------------------------------
// The menu root: what one open menu's levels share

/// Where focus enters a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Initial {
    /// The first enabled row: a keyboard open.
    First,
    /// The last enabled row: ArrowUp on the trigger.
    Last,
    /// The list itself, no row lit: a pointer open.
    Container,
}

/// One menu's state across its levels.
pub(crate) struct MenuRoot {
    /// Where focus enters the root level on its next frame.
    pub initial: Option<Initial>,
    /// The menu press model (upstream `useMenuPress`), shared by every level.
    pub press: MenuPress,
    /// The key that opened the menu: its repeats do nothing (FR12).
    pub opening_key: Option<SharedString>,
    /// How many levels are drilled in: Escape steps back, not out.
    pub drilled: usize,
    /// Counts openings, so each opening starts from the root level.
    pub generation: u64,
    pub was_open: bool,
    /// A compact touch screen opened it (sampled as it opened): submenus
    /// drill in (FR11).
    pub compact_touch: bool,
    /// Each drawn level's depth and bounds, this frame.
    pub surfaces: Vec<(usize, Bounds<Pixels>)>,
    /// When the menu last opened: its entrance's start.
    pub opened_at: Option<web_time::Instant>,
}

pub(crate) fn menu_root(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<MenuRoot> {
    window.use_keyed_state(
        ElementId::Name(format!("{id}-menu-root").into()),
        cx,
        |window, cx| MenuRoot {
            initial: None,
            press: {
                let press = MenuPress::new(window, cx);
                press.watch_activation(window, cx);
                press
            },
            opening_key: None,
            drilled: 0,
            generation: 0,
            was_open: false,
            compact_touch: false,
            surfaces: Vec::new(),
            opened_at: None,
        },
    )
}

/// Follows the menu opening and closing: a new opening starts fresh.
pub(crate) fn sync_root(root: &Entity<MenuRoot>, open: bool, window: &Window, cx: &mut App) {
    let compact = super::presentation::compact_touch(window, cx);
    let now = cx.background_executor().now();
    root.update(cx, |r, _| {
        if open && !r.was_open {
            r.opened_at = Some(now);
            r.generation += 1;
            r.drilled = 0;
            r.compact_touch = compact;
            // Opened from code: focus enters the list itself.
            r.initial.get_or_insert(Initial::Container);
        }
        // Only the close resets the press: a closed menu's trigger may hold a pending
        // press (the pointer model's hold, the content-touch delay in a scroller).
        if !open && r.was_open {
            r.press.reset();
        }
        if !open {
            r.opening_key = None;
            r.drilled = 0;
            r.surfaces.clear();
        }
        r.was_open = open;
    });
}

/// A canvas that records its parent's bounds as a menu surface (the parent
/// must be `relative`). `clear` starts the frame's list.
pub(crate) fn surface_probe(root: &Entity<MenuRoot>, depth: usize, clear: bool) -> AnyElement {
    let root = root.clone();
    gpui::canvas(
        move |bounds, _, cx| {
            root.update(cx, |r, _| {
                if clear {
                    r.surfaces.clear();
                }
                r.surfaces.push((depth, bounds));
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

/// What every level of one menu shares.
#[derive(Clone)]
pub(crate) struct MenuTree {
    pub root: Entity<MenuRoot>,
    /// Closes the whole menu.
    pub close: MenuAction,
    pub size: MenuSize,
    /// Tab leaves the menu: no header or footer (FR23).
    pub tab_leaves: bool,
    /// The touch sheet's rows.
    pub touch: bool,
    /// Submenus drill in rather than fly out.
    pub drill: bool,
    /// Each level's greatest height.
    pub max_height: Pixels,
    /// Submenus may hold a header and footer (DropdownMenu; a ContextMenu
    /// shows their rows alone).
    pub regions: bool,
}

// ----------------------------------------------------------------------------
// Rows

/// A drawn row, or a group's bounds.
enum Row {
    Back(SharedString),
    Action(MenuItem),
    Check(MenuCheckboxItem),
    Radio {
        group: Rc<MenuRadioGroup>,
        item: MenuRadioItem,
    },
    /// A group's start: its title, and whether the heading is semibold
    /// (TabMenu's).
    GroupStart(Option<SharedString>, bool),
    GroupEnd,
    Divider,
}

fn flatten(entries: &[MenuEntry], rows: &mut Vec<Row>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => rows.push(Row::Action(item.clone())),
            MenuEntry::Checkbox(item) => rows.push(Row::Check(item.clone())),
            MenuEntry::RadioGroup(group) => {
                rows.push(Row::GroupStart(group.title.clone(), group.check_marks));
                let shared = Rc::new(group.clone());
                for item in &group.items {
                    rows.push(Row::Radio {
                        group: shared.clone(),
                        item: item.clone(),
                    });
                }
                rows.push(Row::GroupEnd);
            }
            MenuEntry::Divider => rows.push(Row::Divider),
            MenuEntry::Section { title, entries } => {
                rows.push(Row::GroupStart(title.clone(), false));
                flatten(entries, rows);
                rows.push(Row::GroupEnd);
            }
        }
    }
}

impl Row {
    /// A stable key for the row's element id.
    fn key(&self) -> SharedString {
        match self {
            Row::Back(_) => "back".into(),
            Row::Action(item) => item.id.clone(),
            Row::Check(item) => item.id.clone(),
            Row::Radio { item, .. } => item.value.clone(),
            _ => SharedString::default(),
        }
    }

    /// The label and whether it is disabled, for a row that takes focus.
    fn focusable(&self) -> Option<(SharedString, bool)> {
        match self {
            Row::Back(label) => Some((label.clone(), false)),
            Row::Action(item) => Some((item.label.clone(), item.disabled)),
            Row::Check(item) => Some((item.label.clone(), item.disabled)),
            Row::Radio { item, .. } => Some((item.label.clone(), item.disabled)),
            _ => None,
        }
    }

    fn has_submenu(&self) -> bool {
        matches!(self, Row::Action(item) if !item.submenu.is_empty())
    }

    fn opens_submenu(&self) -> bool {
        matches!(self, Row::Action(item) if !item.submenu.is_empty() && !item.disabled)
    }

    /// Whether the row's submenu drills in, in a menu whose default is
    /// `drill`.
    fn drills(&self, drill: bool) -> bool {
        match self {
            Row::Action(item) => match item.submenu_presentation {
                SubmenuPresentation::Flyout => false,
                SubmenuPresentation::DrillIn => true,
                SubmenuPresentation::Adaptive => drill,
            },
            _ => false,
        }
    }

    /// The hover waits before the row's flyout opens and after the pointer
    /// leaves it.
    fn delays(&self) -> (Duration, Duration) {
        match self {
            Row::Action(item) => (item.show_delay, item.hide_delay),
            _ => (SUBMENU_SHOW_DELAY, SUBMENU_HIDE_DELAY),
        }
    }
}

/// A level's state between frames.
struct MenuState {
    generation: u64,
    /// The row whose flyout is open.
    submenu: Option<usize>,
    /// The row drilled into.
    drilled: Option<usize>,
    /// Where focus enters the next nested level.
    focus_nested: Option<Initial>,
    /// A row (focus index) to focus next frame: after stepping back.
    refocus: Option<usize>,
    hover_opened_at: Option<Instant>,
    /// A scheduled flyout change: the row to open, or none to close.
    pending: Option<Option<usize>>,
    hover_task: Option<Task<()>>,
    /// The last point on the open flyout's row: the safe triangle's apex.
    exit: Option<Point<Pixels>>,
    flyout: Option<Bounds<Pixels>>,
    flyout_focus: FocusHandle,
    container: FocusHandle,
    scroll: ScrollHandle,
    /// Each row's bounds, by row index, as of last frame.
    rows: Vec<Option<Bounds<Pixels>>>,
    typeahead: Typeahead,
    /// The submenu row last reported open (`onOpenChange`).
    reported: Option<usize>,
}

impl MenuState {
    fn new(cx: &mut App) -> Self {
        MenuState {
            generation: 0,
            submenu: None,
            drilled: None,
            focus_nested: None,
            refocus: None,
            hover_opened_at: None,
            pending: None,
            hover_task: None,
            exit: None,
            flyout: None,
            flyout_focus: cx.focus_handle().tab_stop(false),
            container: cx.focus_handle().tab_stop(false),
            scroll: ScrollHandle::new(),
            rows: Vec::new(),
            typeahead: Typeahead::new(),
            reported: None,
        }
    }

    fn reset(&mut self, generation: u64) {
        self.generation = generation;
        self.submenu = None;
        self.drilled = None;
        self.focus_nested = None;
        self.refocus = None;
        self.pending = None;
        self.hover_task = None;
        self.exit = None;
        self.flyout = None;
        self.typeahead.reset();
        self.scroll.set_offset(point(px(0.), px(0.)));
    }
}

/// Changes the open flyout after `delay`, unless something cancels it.
fn schedule(
    state: &Entity<MenuState>,
    target: Option<usize>,
    delay: Duration,
    window: &mut Window,
    cx: &mut App,
) {
    if state.read(cx).pending == Some(target) {
        return;
    }
    let timer = cx.background_executor().timer(delay);
    let weak = state.downgrade();
    let task = window.spawn(cx, async move |cx| {
        timer.await;
        cx.update(|window, cx| {
            let Some(state) = weak.upgrade() else { return };
            let now = cx.background_executor().now();
            state.update(cx, |s, _| {
                s.pending = None;
                s.hover_task = None;
                s.submenu = target;
                s.hover_opened_at = target.map(|_| now);
            });
            window.refresh();
        })
        .ok();
    });
    state.update(cx, |s, _| {
        s.pending = Some(target);
        s.hover_task = Some(task);
    });
}

fn cancel(state: &Entity<MenuState>, cx: &mut App) {
    if state.read(cx).pending.is_some() {
        state.update(cx, |s, _| {
            s.pending = None;
            s.hover_task = None;
        });
    }
}

/// Upstream `useMenuHover`, for one level: the pointer at `p` opens, keeps
/// or closes its flyout.
fn track_pointer(
    state: &Entity<MenuState>,
    root: &Entity<MenuRoot>,
    depth: usize,
    // Per row: its hover delays (open, close) when hovering opens it.
    hover: &[Option<(Duration, Duration)>],
    p: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let (open, exit, flyout, row, flyout_focused) = {
        let s = state.read(cx);
        let row = s
            .rows
            .iter()
            .position(|b| b.is_some_and(|b| b.contains(&p)));
        (
            s.submenu,
            s.exit,
            s.flyout,
            row,
            s.flyout_focus.contains_focused(window, cx),
        )
    };
    // Over a deeper level: the flyout chain stays.
    let deeper = root
        .read(cx)
        .surfaces
        .iter()
        .any(|(d, b)| *d > depth && b.contains(&p));
    if deeper {
        cancel(state, cx);
        return;
    }
    let target = row.filter(|&r| hover.get(r).copied().flatten().is_some());
    // The last point on a submenu's row: where the triangle starts when the
    // pointer leaves it.
    if target.is_some() {
        state.update(cx, |s, _| s.exit = Some(p));
    }
    match open {
        Some(open_row) => {
            if row == Some(open_row) {
                cancel(state, cx);
                return;
            }
            if let (Some(origin), Some(flyout)) = (exit, flyout)
                && in_safe_triangle(p, origin, flyout)
            {
                cancel(state, cx);
                return;
            }
            // Outside the menu, with focus in the flyout: it stays.
            if row.is_none() && flyout_focused {
                return;
            }
            let hide = hover
                .get(open_row)
                .copied()
                .flatten()
                .map_or(SUBMENU_HIDE_DELAY, |d| d.1);
            schedule(state, target, hide, window, cx);
        }
        None => match target {
            Some(r) => {
                let show = hover
                    .get(r)
                    .copied()
                    .flatten()
                    .map_or(SUBMENU_SHOW_DELAY, |d| d.0);
                schedule(state, Some(r), show, window, cx)
            }
            None => cancel(state, cx),
        },
    }
}

/// The rows of one menu level, upstream's `role="menu"` list.
#[derive(IntoElement)]
pub(crate) struct MenuList {
    id: ElementId,
    entries: Vec<MenuEntry>,
    label: SharedString,
    tree: MenuTree,
    depth: usize,
    /// A nested level's way back to its row: Left, and Escape when drilled.
    back: Option<MenuAction>,
    /// A drilled level's Back row label.
    back_row: Option<SharedString>,
    /// Where focus enters on this frame (nested levels).
    focus: Option<Initial>,
}

impl MenuList {
    pub(crate) fn new(
        id: impl Into<ElementId>,
        entries: Vec<MenuEntry>,
        label: SharedString,
        tree: MenuTree,
    ) -> Self {
        MenuList {
            id: id.into(),
            entries,
            label,
            tree,
            depth: 0,
            back: None,
            back_row: None,
            focus: None,
        }
    }
}

impl RenderOnce for MenuList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let rtl = dir.is_rtl();
        let tree = self.tree.clone();
        let root = tree.root.clone();
        let is_root = self.depth == 0 && self.back.is_none();
        // The level drawn first each frame (the root, or what drilled in for it).
        let first_level = self.depth == 0;
        let press = root.read(cx).press.clone();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-menu", self.id).into()),
            cx,
            |_, cx| MenuState::new(cx),
        );
        let generation = root.read(cx).generation;
        if state.read(cx).generation != generation {
            state.update(cx, |s, _| s.reset(generation));
        }
        let focus = if is_root {
            root.update(cx, |r, _| r.initial.take())
        } else {
            self.focus
        };

        let mut rows = Vec::new();
        if let Some(label) = &self.back_row {
            rows.push(Row::Back(label.clone()));
        }
        flatten(&self.entries, &mut rows);

        // A drilled level replaces this one's rows.
        if let Some(at) = state.read(cx).drilled
            && let Some(Row::Action(item)) = rows.get(at)
        {
            return self.drilled(&state, at, item, &rows, window, cx);
        }

        let focusable: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.focusable().map(|_| i))
            .collect();
        let labels: Rc<Vec<SharedString>> = Rc::new(
            focusable
                .iter()
                .filter_map(|&i| rows[i].focusable().map(|(l, _)| l))
                .collect(),
        );
        let disabled: Rc<Vec<bool>> = Rc::new(
            focusable
                .iter()
                .filter_map(|&i| rows[i].focusable().map(|(_, d)| d))
                .collect(),
        );
        // Rows whose submenu opens: by key (all of them), by hover (the
        // flyouts), and which drill in.
        let opens: Rc<Vec<bool>> = Rc::new(rows.iter().map(Row::opens_submenu).collect());
        let drills: Rc<Vec<bool>> = Rc::new(rows.iter().map(|r| r.drills(tree.drill)).collect());
        let hover_opens: Rc<Vec<bool>> = Rc::new(
            opens
                .iter()
                .zip(drills.iter())
                .map(|(o, d)| *o && !d)
                .collect(),
        );
        let delays: Rc<Vec<(Duration, Duration)>> = Rc::new(rows.iter().map(Row::delays).collect());
        // Report a submenu opening or closing.
        {
            let open_now = {
                let s = state.read(cx);
                s.submenu.or(s.drilled)
            };
            let reported = state.read(cx).reported;
            if open_now != reported {
                state.update(cx, |s, _| s.reported = open_now);
                let report = |at: Option<usize>, open: bool| {
                    at.and_then(|i| match rows.get(i) {
                        Some(Row::Action(item)) => item.on_submenu_open_change.clone(),
                        _ => None,
                    })
                    .map(|f| (f, open))
                };
                for (f, open) in [report(reported, false), report(open_now, true)]
                    .into_iter()
                    .flatten()
                {
                    window.defer(cx, move |window, cx| f(open, window, cx));
                }
            }
        }
        let row_of: Rc<Vec<usize>> = Rc::new(focusable.clone());
        let list = ListFocus::new(
            &self.id,
            focusable.len(),
            ListKeys::vertical().reach_disabled(true),
            window,
            cx,
        )
        .disabled(disabled.iter().copied());
        let skip_back = usize::from(self.back_row.is_some());
        let first_enabled = (skip_back..disabled.len())
            .find(|&i| !disabled[i])
            .or_else(|| (0..disabled.len()).find(|&i| !disabled[i]));
        let last_enabled = (0..disabled.len()).rev().find(|&i| !disabled[i]);
        let preferred = match focus {
            Some(Initial::Last) => last_enabled,
            _ => first_enabled,
        };
        let tab_stop = list.tab_stop(preferred, window);
        state.update(cx, |s, _| s.rows.resize(rows.len(), None));
        let (open_sub, refocus, container_focus, scroll) = {
            let s = state.read(cx);
            (s.submenu, s.refocus, s.container.clone(), s.scroll.clone())
        };

        // Focus enters (or comes back) once the rows are drawn.
        let entry = match (refocus, focus) {
            (Some(i), _) => Some(Some(i)),
            (None, Some(Initial::Container)) => Some(None),
            (None, Some(Initial::First)) => Some(first_enabled),
            (None, Some(Initial::Last)) => Some(last_enabled),
            (None, None) => None,
        };
        let entry_probe = entry.map(|target| {
            let list = list.clone();
            let state = state.clone();
            let container = container_focus.clone();
            gpui::canvas(
                |_, _, _| {},
                move |_, _, window, cx| {
                    let list = list.clone();
                    let container = container.clone();
                    state.update(cx, |s, _| s.refocus = None);
                    window.defer(cx, move |window, cx| match target {
                        Some(i) => list.focus(i, window, cx),
                        None => container.focus(window, cx),
                    });
                },
            )
            .absolute()
            .size_0()
        });

        let (pad_y, pad_x) = if tree.touch {
            (t.spacing.s3, t.spacing.s3)
        } else {
            match tree.size {
                MenuSize::Sm => (t.spacing.s1, t.spacing.s2),
                MenuSize::Md => (t.spacing.s1_5, t.spacing.s2),
                MenuSize::Lg => (t.spacing.s2, t.spacing.s2),
            }
        };
        let menu_padding = if tree.touch {
            px(0.)
        } else {
            t.spacing.s1.to_pixels(rem)
        };
        // Upstream `menuRowCornerStyles`: the menu's radius less its padding.
        let row_radius = (t.radius.container.to_pixels(rem) - menu_padding).max(px(0.));
        let marker_size = match tree.size {
            MenuSize::Sm => ChoiceSize::Sm,
            _ => ChoiceSize::Md,
        };
        let marker_at_end = tree.touch || rustryx_primitives::coarse_primary_pointer(window, cx);
        let link_handler = use_link_handler(cx);
        let gap = if tree.touch {
            t.spacing.s1
        } else {
            t.spacing.s0_5
        };

        // Groups nest: a stack of children lists.
        let mut stack: Vec<(Option<SharedString>, Vec<AnyElement>)> = vec![(None, Vec::new())];
        let mut index = 0usize;
        for (row_at, row) in rows.into_iter().enumerate() {
            match row {
                Row::Divider => {
                    let mut divider = div()
                        .id(ElementId::NamedInteger(
                            "menu-divider".into(),
                            row_at as u64,
                        ))
                        .my(t.spacing.s1)
                        .h(px(1.))
                        .flex_none()
                        .bg(t.color.border)
                        .role(Role::Splitter);
                    theme.refine(divider.style(), "dropdown-menu-divider", &[], None);
                    stack
                        .last_mut()
                        .expect("root")
                        .1
                        .push(divider.into_any_element());
                    continue;
                }
                Row::GroupStart(title, strong) => {
                    let mut children = Vec::new();
                    if let Some(title) = &title {
                        let mut heading = if tree.touch {
                            div()
                                .px(t.spacing.s3)
                                .child(Heading::new(HeadingLevel::H4, title.clone()))
                        } else {
                            div()
                                .py(t.spacing.s1)
                                .px(if strong { t.spacing.s3 } else { t.spacing.s2 })
                                .text_size(t.text.supporting.size)
                                .line_height(relative(t.text.supporting.leading))
                                .text_color(t.color.text_secondary)
                                .when(strong, |h| h.font_weight(t.font.weight.semibold))
                                .child(title.clone())
                        };
                        theme.refine(heading.style(), "dropdown-menu-section-heading", &[], None);
                        children.push(heading.into_any_element());
                    }
                    stack.push((title, children));
                    continue;
                }
                Row::GroupEnd => {
                    let (title, children) = stack.pop().expect("a group was open");
                    let mut group = div()
                        .id(ElementId::NamedInteger("menu-group".into(), row_at as u64))
                        .flex()
                        .flex_col()
                        .gap(gap)
                        .role(Role::Group);
                    if let Some(title) = title {
                        group = group.aria_label(title);
                    }
                    stack
                        .last_mut()
                        .expect("root")
                        .1
                        .push(group.children(children).into_any_element());
                    continue;
                }
                _ => {}
            }
            let focus_index = index;
            index += 1;
            let handle = list.item_handle(focus_index, tab_stop);
            let focused = handle.is_focused(window);
            let row_id = ElementId::Name(format!("{}-{}", self.id, row.key()).into());
            let (label, row_disabled) = row.focusable().expect("a focusable row");
            let has_sub = row.has_submenu();
            let sub_open = has_sub && open_sub == Some(row_at);

            // What activating the row does.
            let close = tree.close.clone();
            let activate: ActivateFn = match &row {
                Row::Back(_) => {
                    let back = self.back.clone();
                    Rc::new(move |_, window, cx| {
                        if let Some(back) = &back {
                            back(window, cx);
                        }
                    })
                }
                Row::Action(_) if has_sub => {
                    let state = state.clone();
                    let root = root.clone();
                    let drill = drills[row_at];
                    Rc::new(move |_, window, cx| {
                        if row_disabled {
                            return;
                        }
                        let by_key = window.last_input_was_keyboard();
                        let now = cx.background_executor().now();
                        if drill {
                            state.update(cx, |s, _| {
                                s.drilled = Some(row_at);
                                s.focus_nested = Some(if by_key {
                                    Initial::First
                                } else {
                                    Initial::Container
                                });
                            });
                            root.update(cx, |r, _| r.drilled += 1);
                        } else {
                            state.update(cx, |s, _| {
                                let guarded = s.submenu == Some(row_at)
                                    && s.hover_opened_at.is_some_and(|at| {
                                        now.saturating_duration_since(at) < SUBMENU_CLICK_GUARD
                                    });
                                s.pending = None;
                                s.hover_task = None;
                                if by_key || s.submenu != Some(row_at) {
                                    s.submenu = Some(row_at);
                                    s.focus_nested = by_key.then_some(Initial::First);
                                } else if !guarded {
                                    s.submenu = None;
                                }
                                s.hover_opened_at = None;
                            });
                        }
                        window.refresh();
                    })
                }
                Row::Action(item) => {
                    let item = item.clone();
                    let handler = link_handler.clone();
                    Rc::new(move |modified, window, cx| {
                        if row_disabled {
                            return;
                        }
                        // A modified click leaves the action to the link.
                        if !modified && let Some(f) = &item.on_click {
                            f(window, cx);
                        }
                        if item.close_on_select || modified {
                            close(window, cx);
                        }
                        if let Some(href) = &item.href {
                            let target = if modified {
                                Some(SharedString::from("_blank"))
                            } else {
                                item.target.clone()
                            };
                            handler.open(href, target, window, cx);
                        }
                    })
                }
                Row::Check(item) => {
                    let item = item.clone();
                    Rc::new(move |_, window, cx| {
                        if row_disabled {
                            return;
                        }
                        if let Some(f) = &item.on_change {
                            f(!item.checked, window, cx);
                        }
                        if item.close_on_select {
                            close(window, cx);
                        }
                    })
                }
                Row::Radio { group, item } => {
                    let group = group.clone();
                    let value = item.value.clone();
                    Rc::new(move |_, window, cx| {
                        if row_disabled {
                            return;
                        }
                        if let Some(f) = &group.on_change {
                            f(value.clone(), window, cx);
                        }
                        if group.close_on_select {
                            close(window, cx);
                        }
                    })
                }
                _ => unreachable!("only focusable rows reach here"),
            };

            let destructive = matches!(&row, Row::Action(item) if item.destructive);
            let label_color = if destructive {
                t.color.text_error
            } else {
                t.color.text_primary
            };
            let icon_for = |icon: &IconSource| {
                Icon::new(icon.clone())
                    .size(IconSize::Sm)
                    .color(if destructive {
                        IconColor::Error
                    } else {
                        IconColor::Secondary
                    })
            };
            let description_for = |d: &SharedString| {
                div()
                    .text_color(if destructive {
                        t.color.text_error
                    } else {
                        t.color.text_secondary
                    })
                    .child(d.clone())
            };
            let label_el = div()
                .text_size(t.text.label.size)
                .line_height(relative(t.text.label.leading))
                .text_color(label_color)
                .child(label.clone());
            let mut content = Item::new(row_id.clone(), label_el).bare(true);
            let (role, toggled) = match &row {
                Row::Back(_) => {
                    content = content.start_content(
                        Icon::new("chevronLeft")
                            .size(IconSize::Sm)
                            .color(IconColor::Secondary)
                            .mirror_in_rtl(dir),
                    );
                    (Role::MenuItem, None)
                }
                Row::Action(item) => {
                    if let Some(icon) = &item.icon {
                        content = content.start_content(icon_for(icon));
                    }
                    if let Some(d) = &item.description {
                        content = content.description(description_for(d));
                    }
                    if has_sub && item.spinner {
                        content = content.end_content(
                            crate::status::Spinner::new("spinner")
                                .size(crate::status::SpinnerSize::Sm)
                                .decorative(),
                        );
                    } else if has_sub {
                        let mut chevron = div().child(
                            Icon::new("chevronRight")
                                .size(IconSize::Sm)
                                .color(IconColor::Secondary)
                                .mirror_in_rtl(dir),
                        );
                        theme.refine(chevron.style(), "dropdown-menu-indicator-icon", &[], None);
                        content = content.end_content(chevron);
                    } else if let Some(end) = &item.end_content {
                        content = content.end_content(end(window, cx));
                    }
                    (Role::MenuItem, None)
                }
                Row::Check(item) => {
                    let marker = CheckboxIndicator::new(item.checked)
                        .size(marker_size)
                        .disabled(item.disabled);
                    let end = item.end_content.as_ref().map(|f| f(window, cx));
                    // A coarse primary pointer puts the marker at the row's end, where
                    // touch selection toggles sit (upstream CSS `order` under
                    // `(pointer: coarse)`), in the anchored menu as in the sheet.
                    content = if marker_at_end {
                        content.end_content(
                            div()
                                .flex()
                                .items_center()
                                .gap(t.spacing.s2)
                                .children(end)
                                .child(marker),
                        )
                    } else {
                        content.marker(marker).end_content_opt(end)
                    };
                    if let Some(icon) = &item.icon {
                        content = content.start_content(icon_for(icon));
                    }
                    if let Some(d) = &item.description {
                        content = content.description(description_for(d));
                    }
                    (Role::MenuItemCheckBox, Some(item.checked))
                }
                Row::Radio { group, item } if group.check_marks => {
                    let selected = group.value == item.value;
                    if let Some(icon) = &item.icon {
                        content = content.start_content(icon_for(icon));
                    }
                    content = content.end_content(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.spacing.s2)
                            .children(item.end_content.as_ref().map(|f| f(window, cx)))
                            .when(selected, |el| {
                                el.child(
                                    Icon::new("check")
                                        .size(IconSize::Sm)
                                        .color(IconColor::Accent),
                                )
                            }),
                    );
                    (Role::MenuItemRadio, Some(selected))
                }
                Row::Radio { group, item } => {
                    let selected = group.value == item.value;
                    let marker = RadioIndicator::new(selected)
                        .size(marker_size)
                        .disabled(item.disabled);
                    let end = item.end_content.as_ref().map(|f| f(window, cx));
                    content = if marker_at_end {
                        content.end_content(
                            div()
                                .flex()
                                .items_center()
                                .gap(t.spacing.s2)
                                .children(end)
                                .child(marker),
                        )
                    } else {
                        content.marker(marker).end_content_opt(end)
                    };
                    if let Some(icon) = &item.icon {
                        content = content.start_content(icon_for(icon));
                    }
                    if let Some(d) = &item.description {
                        content = content.description(description_for(d));
                    }
                    (Role::MenuItemRadio, Some(selected))
                }
                _ => unreachable!("only focusable rows reach here"),
            };
            let href = match &row {
                Row::Action(item) if !has_sub => item.href.clone(),
                _ => None,
            };

            let row_selector = format!("{}-{}", self.id, row.key());
            let row_key = SharedString::from(row_selector.clone());
            // TabMenu's rows are roomier, the selected one medium weight.
            let tab_menu_row = matches!(&row, Row::Radio { group, .. } if group.check_marks);
            let (row_pad_y, row_pad_x) = if tab_menu_row && !tree.touch {
                (t.spacing.s2, t.spacing.s3)
            } else {
                (pad_y, pad_x)
            };
            let mut el = div()
                .id(row_id.clone())
                .debug_selector(|| row_selector)
                .track_focus(&handle)
                .relative()
                .w_full()
                .flex_none()
                .rounded(row_radius)
                .py(row_pad_y)
                .px(row_pad_x)
                .text_size(t.text.label.size)
                .line_height(relative(t.text.label.leading))
                .text_color(label_color)
                .role(if href.is_some() { Role::Link } else { role })
                .aria_label(label.clone())
                .aria_disabled(row_disabled)
                .child(content);
            if tab_menu_row && toggled == Some(true) {
                el = el.font_weight(t.font.weight.medium);
            }
            if let Some(on) = toggled {
                el = el.aria_toggled(if on {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                });
            }
            if has_sub && !drills[row_at] {
                el = el.aria_expanded(sub_open).aria_has_popup(HasPopup::Menu);
            }
            if focused {
                el = el.bg(t.color.overlay_hover);
            }
            if row_disabled {
                el = el.opacity(0.5);
            } else {
                let pressed = t.color.overlay_pressed;
                el = el.cursor_pointer().active(move |s| s.bg(pressed));
            }
            if is_focus_visible(&handle, window) {
                el = el.child(focus_ring_inset(t, row_radius, rem));
            }
            // The row's bounds, for the pointer and PageUp/PageDown.
            {
                let state = state.clone();
                el = el.child(
                    gpui::canvas(
                        move |bounds, _, cx| {
                            state.update(cx, |s, _| {
                                if let Some(slot) = s.rows.get_mut(row_at) {
                                    *slot = Some(bounds);
                                }
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                );
            }
            // The pointer focuses the row under it (an enabled one).
            {
                let handle = handle.clone();
                el = el.on_mouse_move(move |_, window, cx| {
                    if !row_disabled && !handle.is_focused(window) {
                        handle.focus(window, cx);
                        window.refresh();
                    }
                });
            }
            // A held finger lights the row it is over: its pressed paint.
            if press.lit_row().as_deref() == Some(row_key.as_ref()) {
                el = el.bg(t.color.overlay_pressed);
            }
            if href.is_some() {
                let activate = activate.clone();
                el = el.on_mouse_up(MouseButton::Middle, move |_, window, cx| {
                    activate(true, window, cx)
                });
            }
            let mut el = el.on_click({
                let (press, activate) = (press.clone(), activate.clone());
                move |event: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    // The press model already answered this release.
                    if !event.is_keyboard() && press.take_stray_click(cx) {
                        return;
                    }
                    let m = event.modifiers();
                    activate(m.platform || m.control || m.shift, window, cx);
                }
            });
            let props = [("size", tree.size.as_str())];
            theme.refine(
                el.style(),
                if role == Role::MenuItemRadio {
                    "dropdown-menu-radio"
                } else {
                    "dropdown-menu-item"
                },
                &props,
                None,
            );

            let el = if sub_open && !drills[row_at] {
                self.flyout(&state, &row, &handle, el, window, cx)
            } else {
                el.into_any_element()
            };
            // An enabled row is one the press model picks (upstream's item selector).
            let el: AnyElement = if row_disabled {
                el
            } else {
                let activate = activate.clone();
                press
                    .row(
                        MenuPressRow::new(row_key.clone())
                            .focus(&handle)
                            .on_activate(move |release: &MenuRelease, window, cx| {
                                let m = release.modifiers;
                                activate(m.platform || m.control || m.shift, window, cx)
                            }),
                        el,
                    )
                    .into_any_element()
            };
            stack.last_mut().expect("root").1.push(el);
        }
        let (_, children) = stack.pop().expect("root");

        // Keys.
        let arrows = list.key_down_listener();
        let keys_list = list.clone();
        let keys_state = state.clone();
        let keys_root = root.clone();
        let back = self.back.clone();
        let escape_steps_back = self.back_row.is_some() || (tree.touch && self.back.is_some());
        let keys_opens = opens.clone();
        let keys_rows = row_of.clone();
        let keys_drills = drills.clone();
        let list_selector = self.id.to_string();
        let mut container = div()
            .id(self.id.clone())
            .debug_selector(|| list_selector)
            .track_focus(&container_focus)
            .key_context("menu")
            .relative()
            .flex()
            .flex_col()
            .gap(gap)
            .p(menu_padding)
            .max_h(tree.max_height)
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .role(Role::Menu)
            .aria_label(self.label.clone())
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                // The key that opened the menu repeats into nothing (FR12).
                let opening = keys_root.read(cx).opening_key.clone();
                if event.is_held && opening.as_deref() == Some(key) {
                    cx.stop_propagation();
                    return;
                }
                let current = keys_list.focused(window);
                let (into, out) = if rtl {
                    ("left", "right")
                } else {
                    ("right", "left")
                };
                if let Some(back) = &back
                    && (key == out || (key == "escape" && escape_steps_back))
                {
                    cx.stop_propagation();
                    back(window, cx);
                    return;
                }
                if key == into {
                    if let Some(row) = current
                        .and_then(|i| keys_rows.get(i).copied())
                        .filter(|&r| keys_opens.get(r).copied().unwrap_or(false))
                    {
                        cx.stop_propagation();
                        let drill = keys_drills.get(row).copied().unwrap_or(false);
                        keys_state.update(cx, |s, _| {
                            if drill {
                                s.drilled = Some(row);
                            } else {
                                s.submenu = Some(row);
                            }
                            s.pending = None;
                            s.hover_task = None;
                            s.focus_nested = Some(Initial::First);
                        });
                        if drill {
                            keys_root.update(cx, |r, _| r.drilled += 1);
                        }
                        window.refresh();
                    }
                    return;
                }
                if key == "pagedown" || key == "pageup" {
                    cx.stop_propagation();
                    let (rows, viewport) = {
                        let s = keys_state.read(cx);
                        let rows: Option<Vec<Bounds<Pixels>>> = keys_rows
                            .iter()
                            .map(|&r| s.rows.get(r).copied().flatten())
                            .collect();
                        (rows, s.scroll.bounds())
                    };
                    if let Some(rows) = rows
                        && let Some(target) =
                            page_target(&rows, viewport, current, key == "pagedown")
                    {
                        keys_list.focus(target, window, cx);
                        reveal(&keys_state.read(cx).scroll, rows[target]);
                    }
                    return;
                }
                let matched = keys_state.update(cx, |s, _| {
                    s.typeahead.key(
                        &event.keystroke,
                        labels.len(),
                        |i| labels.get(i).map(|l| l.as_ref()),
                        current,
                        |_| false,
                    )
                });
                match matched {
                    Some(Some(target)) => {
                        cx.stop_propagation();
                        keys_list.focus(target, window, cx);
                    }
                    Some(None) => cx.stop_propagation(),
                    None => arrows(event, window, cx),
                }
                // Keep the focused row in view.
                if let Some(i) = keys_list.focused(window) {
                    let s = keys_state.read(cx);
                    if let Some(bounds) = keys_rows
                        .get(i)
                        .and_then(|&r| s.rows.get(r).copied().flatten())
                    {
                        reveal(&s.scroll, bounds);
                    }
                }
            })
            .on_key_up({
                let root = root.clone();
                move |_, _, cx| {
                    root.update(cx, |r, _| r.opening_key = None);
                }
            });
        if tree.tab_leaves {
            let (next, prev) = (tree.close.clone(), tree.close.clone());
            container = container
                .on_action(move |_: &FocusNext, window, cx| {
                    next(window, cx);
                    cx.propagate();
                })
                .on_action(move |_: &FocusPrevious, window, cx| {
                    prev(window, cx);
                    cx.propagate();
                });
        }
        // The level's bounds, and its pointer tracking.
        container = container.child(surface_probe(&root, self.depth, false));
        if !tree.touch && hover_opens.iter().any(|o| *o) {
            let state = state.clone();
            let root = root.clone();
            let depth = self.depth;
            let hover: Rc<Vec<Option<(Duration, Duration)>>> = Rc::new(
                hover_opens
                    .iter()
                    .zip(delays.iter())
                    .map(|(opens, d)| opens.then_some(*d))
                    .collect(),
            );
            container = container.child(
                gpui::canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let (state, root, hover) = (state.clone(), root.clone(), hover.clone());
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble {
                                track_pointer(
                                    &state,
                                    &root,
                                    depth,
                                    &hover,
                                    event.position,
                                    window,
                                    cx,
                                );
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }
        // The press model: a mouse released outside every level (and the trigger)
        // closes the menu; the root level's rows scroll under a resting pointer.
        if first_level {
            let close = tree.close.clone();
            // The rows overflow the level (as last laid out): it scrolls.
            let overflows = {
                let s = state.read(cx);
                let rows: Vec<Bounds<Pixels>> = s.rows.iter().flatten().copied().collect();
                let viewport = scroll.bounds();
                match (rows.first(), rows.last()) {
                    (Some(first), Some(last)) => {
                        last.bottom() - first.top() > viewport.size.height + px(0.5)
                    }
                    _ => false,
                }
            };
            press.set_menu(
                move |window, cx| close(window, cx),
                overflows.then(|| scroll.clone()),
                Some(container_focus.clone()),
            );
        }
        let mut container = container
            .child(crate::gestures::scroller::scroll_begin(
                self.id.clone(),
                Some(&scroll),
                (false, true),
            ))
            .children(children)
            .child(crate::gestures::scroller::scroll_end())
            .children(entry_probe);
        let props = [("size", tree.size.as_str())];
        theme.refine(container.style(), "dropdown-menu", &props, None);
        press.menu(container, first_level).into_any_element()
    }
}

impl MenuList {
    /// The level drilled into from row `at`: its rows behind a Back row, or
    /// in the sheet behind a Back button beside its heading.
    fn drilled(
        &self,
        state: &Entity<MenuState>,
        at: usize,
        item: &MenuItem,
        rows: &[Row],
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let tree = self.tree.clone();
        let focus_index = rows[..at]
            .iter()
            .filter(|r| r.focusable().is_some())
            .count();
        let back_state = state.clone();
        let back_root = tree.root.clone();
        let back: MenuAction = Rc::new(move |window, cx| {
            back_state.update(cx, |s, _| {
                s.drilled = None;
                s.refocus = Some(focus_index);
            });
            back_root.update(cx, |r, _| r.drilled = r.drilled.saturating_sub(1));
            window.refresh();
        });
        let back_label = i18n(cx).format(
            "@astryx.dropdownMenu.backTo",
            &[("parent", Arg::from(self.label.as_ref()))],
        );
        let nested_focus = state.update(cx, |s, _| s.focus_nested.take());
        let nested = MenuList {
            id: ElementId::Name(format!("{}-{}-drill", self.id, item.id).into()),
            entries: item.submenu.clone(),
            label: item.label.clone(),
            tree: tree.clone(),
            depth: self.depth,
            back: Some(back.clone()),
            back_row: (!tree.touch).then_some(back_label),
            focus: nested_focus,
        };
        let regions = tree.regions;
        let region = |f: &Option<BuildFn>, window: &mut Window, cx: &mut App| {
            f.as_ref()
                .filter(|_| regions)
                .map(|f| div().flex_none().p(t.spacing.s1).child(f(window, cx)))
        };
        let (top, bottom) = (
            region(&item.submenu_header, window, cx),
            region(&item.submenu_footer, window, cx),
        );
        if !tree.touch {
            if top.is_none() && bottom.is_none() {
                return nested.into_any_element();
            }
            return div()
                .flex()
                .flex_col()
                .children(top)
                .child(nested)
                .children(bottom)
                .into_any_element();
        }
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(t.spacing.s1)
            .mb(t.spacing.s2)
            .child(
                IconButton::new(
                    ElementId::Name(format!("{}-{}-back", self.id, item.id).into()),
                    i18n(cx).t("@astryx.dropdownMenu.back"),
                    Icon::new("chevronLeft")
                        .size(IconSize::Sm)
                        .mirror_in_rtl(dir),
                )
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
                .on_click(move |_, window, cx| back(window, cx)),
            )
            .child(Heading::new(HeadingLevel::H3, item.label.clone()));
        div()
            .flex()
            .flex_col()
            .child(header)
            .children(top)
            .child(nested)
            .children(bottom)
            .into_any_element()
    }

    /// The open submenu: a layer on `row`'s end side holding the next level.
    fn flyout(
        &self,
        state: &Entity<MenuState>,
        row: &Row,
        handle: &FocusHandle,
        el: gpui::Stateful<gpui::Div>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let Row::Action(item) = row else {
            return el.into_any_element();
        };
        let theme = current(window, cx);
        let t = theme.tokens();
        let rtl = use_direction(cx).is_rtl();
        let tree = self.tree.clone();
        let row_id = ElementId::Name(format!("{}-{}-anchor", self.id, item.id).into());
        let back_state = state.clone();
        let back_handle = handle.clone();
        let back: MenuAction = Rc::new(move |window, cx| {
            back_state.update(cx, |s, _| {
                s.submenu = None;
                s.pending = None;
                s.hover_task = None;
            });
            back_handle.focus(window, cx);
            window.refresh();
        });
        let nested_focus = state.update(cx, |s, _| s.focus_nested.take());
        // With a header or footer the flyout is a modal dialog named by the
        // row, its Tab kept inside (FR20); a ContextMenu shows the rows alone.
        let regions =
            tree.regions && (item.submenu_header.is_some() || item.submenu_footer.is_some());
        let mut nested_tree = tree.clone();
        if regions {
            nested_tree.tab_leaves = false;
        }
        let nested = MenuList {
            id: ElementId::Name(format!("{}-{}-sub", self.id, item.id).into()),
            entries: item.submenu.clone(),
            label: item.label.clone(),
            tree: nested_tree,
            depth: self.depth + 1,
            back: Some(back.clone()),
            back_row: None,
            focus: nested_focus,
        };
        let region = |f: &Option<BuildFn>, window: &mut Window, cx: &mut App| {
            f.as_ref()
                .filter(|_| regions)
                .map(|f| div().flex_none().p(t.spacing.s1).child(f(window, cx)))
        };
        let (top, bottom) = (
            region(&item.submenu_header, window, cx),
            region(&item.submenu_footer, window, cx),
        );
        let flyout_focus = state.read(cx).flyout_focus.clone();
        let record = state.clone();
        let hover = state.clone();
        let mut surface = div()
            .id(ElementId::Name(
                format!("{}-{}-surface", self.id, item.id).into(),
            ))
            .track_focus(&flyout_focus)
            .occlude()
            .relative()
            .flex()
            .flex_col()
            .bg(t.color.background_popover)
            .rounded(t.radius.container)
            .shadow_token(&t.shadow.low)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .text_color(t.color.text_primary)
            .on_mouse_move(move |_, _, cx| cancel(&hover, cx))
            .children(top)
            .child(nested)
            .children(bottom)
            .child(
                gpui::canvas(
                    move |bounds, _, cx| record.update(cx, |s, _| s.flyout = Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        surface = match &item.submenu_width {
            Some(width) => surface.min_w(*width),
            None => surface.min_w(px(160.)),
        };
        if regions {
            // Tab cycles inside; it never reaches the menu, which would close.
            surface = surface
                .tab_trap()
                .role(Role::Dialog)
                .aria_modal(true)
                .aria_label(item.label.clone())
                .on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
                .on_action(|_: &FocusPrevious, window, cx| window.focus_prev(cx));
        }
        // Escape steps back to the row; a press on another level closes only
        // this flyout, one outside every level closes the menu.
        let root = tree.root.clone();
        let close = tree.close.clone();
        let close_flyout = state.clone();
        let dismiss = move |window: &mut Window, cx: &mut App| {
            if window.last_input_was_keyboard() {
                back(window, cx);
                return;
            }
            let p = window.mouse_position();
            let inside = root.read(cx).surfaces.iter().any(|(_, b)| b.contains(&p));
            if inside {
                close_flyout.update(cx, |s, _| s.submenu = None);
                window.refresh();
            } else {
                close(window, cx);
            }
        };
        anchored_layer(row_id, el)
            .surface(
                surface,
                LayerOptions {
                    placement: Placement::End,
                    align: LayerAlign::Start,
                    offset: px(0.),
                    margin: super::motion::gutter(t, window),
                    rtl,
                    priority: 1,
                },
                dismiss,
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;
    use gpui::size;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    #[test]
    fn the_safe_triangle_points_at_the_flyouts_near_edge() {
        let flyout = rect(200., 0., 100., 200.);
        let origin = point(px(150.), px(100.));
        assert!(in_safe_triangle(point(px(190.), px(30.)), origin, flyout));
        assert!(in_safe_triangle(point(px(190.), px(180.)), origin, flyout));
        assert!(!in_safe_triangle(point(px(160.), px(10.)), origin, flyout));
        assert!(
            !in_safe_triangle(point(px(100.), px(100.)), origin, flyout),
            "behind the apex"
        );
        // The edge is padded at its ends.
        assert!(in_safe_triangle(point(px(199.), px(-5.)), origin, flyout));
        // A flyout on the start side (right to left) mirrors.
        let start = rect(0., 0., 100., 200.);
        assert!(in_safe_triangle(point(px(110.), px(30.)), origin, start));
        // An origin over the flyout itself has nothing to travel toward.
        assert!(!in_safe_triangle(
            point(px(250.), px(60.)),
            point(px(250.), px(50.)),
            flyout
        ));
    }

    #[test]
    fn page_keys_go_to_the_visible_edge_then_a_viewport_further() {
        // Ten 30px rows in a 100px viewport scrolled to the top.
        let rows: Vec<_> = (0..10)
            .map(|i| rect(0., i as f32 * 30., 100., 30.))
            .collect();
        let viewport = rect(0., 0., 100., 100.);
        assert_eq!(
            page_target(&rows, viewport, Some(0), true),
            Some(2),
            "last fully visible"
        );
        assert_eq!(
            page_target(&rows, viewport, Some(2), true),
            Some(5),
            "a viewport further"
        );
        assert_eq!(
            page_target(&rows, viewport, Some(9), true),
            Some(9),
            "no wrap"
        );
        assert_eq!(page_target(&rows, viewport, Some(2), false), Some(0));
        assert_eq!(page_target(&rows, viewport, Some(0), false), Some(0));
        assert_eq!(page_target(&rows, viewport, None, true), Some(2));
    }
}
