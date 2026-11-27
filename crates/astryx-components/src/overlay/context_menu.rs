//! Upstream `ContextMenu`: a menu that opens where it is invoked on an
//! area.
//!
//! - A right press opens it at the pointer (and, held, picks the row it is
//!   released on, as the press model says); Shift+F10 or the Menu key opens
//!   it at the area's bottom-start corner on its first row; a long press
//!   (upstream `useLongPress`) opens it under the finger, which then picks the row it
//!   lifts on after the handoff travel.
//! - It is built on the headless layer, [`create_context_menu`] / [`use_context_menu`]
//!   (spec:AST-107 FR8), which gives any element a context menu.
//! - A press outside closes it at the press; Escape closes the top-most
//!   level only (FR9); Tab leaves the menu (FR11). Focus returns to what had
//!   it.
//! - The sheet presentations hold the rows in a BottomSheet as touch rows.
//!
//! Upstream builds on the browser's `contextmenu` event; here the area takes
//! the right press and the keys itself.

use std::rc::Rc;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{EscapeBehavior, layer};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, Div, ElementId, Entity, FocusHandle, IntoElement, KeyDownEvent,
    Length, MouseButton, MouseDownEvent, ParentElement, Pixels, Point, RenderOnce, SharedString,
    Stateful, StyleRefinement, Styled, Window, div, point, prelude::*, px,
};
use smallvec::SmallVec;

use super::menu::{
    Initial, MenuAction, MenuEntry, MenuList, MenuRoot, MenuSize, MenuTree, menu_block_cap,
    menu_root, surface_probe, sync_root,
};
use super::motion::viewport_cap;
use super::popover::{PopoverPresentation, presents_as_sheet};
use crate::gestures::long_press::{LongPressOptions, use_long_press};
use crate::gestures::menu_press::MenuPressPointerType;
use crate::text::{Heading, HeadingLevel};

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type EntriesFn = Rc<dyn Fn(&mut Window, &mut App) -> Vec<MenuEntry>>;
/// Custom menu content.
pub(crate) type ContentFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A context menu's state between frames.
pub(crate) struct ContextState {
    open: bool,
    /// Where it opened, in window coordinates.
    at: Point<Pixels>,
    return_to: Option<FocusHandle>,
    surface: FocusHandle,
    /// The area's bounds as of last frame.
    bounds: Option<Bounds<Pixels>>,
    on_open_change: Option<OpenFn>,
}

fn context_state(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<ContextState> {
    window.use_keyed_state(
        ElementId::Name(format!("{id}-context").into()),
        cx,
        |_, cx| ContextState {
            open: false,
            at: point(px(0.), px(0.)),
            return_to: None,
            surface: cx.focus_handle().tab_stop(false),
            bounds: None,
            on_open_change: None,
        },
    )
}

/// What opened a context menu and is still held, for the press model.
#[derive(Clone, Copy)]
enum MenuOpener {
    Mouse(MouseButton),
    Finger,
}

/// Opens the menu at `at`: `keyboard` lands on the first row; `press` is the
/// pointer still held, for the press model.
fn open_at(
    state: &Entity<ContextState>,
    root: &Entity<MenuRoot>,
    at: Point<Pixels>,
    keyboard: bool,
    press: Option<MenuOpener>,
    window: &mut Window,
    cx: &mut App,
) {
    let focused = window.focused(cx);
    let (was_open, report) = state.update(cx, |s, _| {
        let was_open = s.open;
        s.open = true;
        s.at = at;
        if !was_open {
            s.return_to = focused;
        }
        (was_open, s.on_open_change.clone())
    });
    root.update(cx, |r, cx| {
        r.initial = Some(if keyboard {
            Initial::First
        } else {
            Initial::Container
        });
        // The pointer that opened it is still held: its press picks on release.
        match press {
            Some(MenuOpener::Mouse(button)) => {
                r.press
                    .opened_under(MenuPressPointerType::Mouse, button, cx)
            }
            Some(MenuOpener::Finger) => {
                r.press
                    .opened_under(MenuPressPointerType::Touch, MouseButton::Left, cx)
            }
            None => r.press.reset(),
        }
        // A new invocation starts from the root level.
        if was_open {
            r.generation += 1;
        }
    });
    if !was_open && let Some(report) = report {
        report(true, window, cx);
    }
    window.refresh();
}

fn close(state: &Entity<ContextState>, window: &mut Window, cx: &mut App) {
    if !state.read(cx).open {
        return;
    }
    let (surface, return_to, report) = state.update(cx, |s, _| {
        s.open = false;
        (
            s.surface.clone(),
            s.return_to.take(),
            s.on_open_change.clone(),
        )
    });
    let lost = window.focused(cx).is_none() || surface.contains_focused(window, cx);
    if lost && let Some(target) = return_to {
        target.focus(window, cx);
    }
    if let Some(report) = report {
        report(false, window, cx);
    }
    window.refresh();
}

/// A menu that opens where it is invoked on its area, upstream
/// `ContextMenu`.
///
/// ```ignore
/// ContextMenu::new("file")
///     .entry(MenuItem::new("rename", "Rename"))
///     .entry(MenuItem::new("delete", "Delete").destructive(true))
///     .child(FileRow::new(file))
/// ```
#[derive(IntoElement)]
pub struct ContextMenu {
    id: ElementId,
    children: SmallVec<[AnyElement; 2]>,
    entries: Vec<MenuEntry>,
    entries_fn: Option<EntriesFn>,
    menu_width: Option<Length>,
    size: MenuSize,
    label: Option<SharedString>,
    on_open_change: Option<OpenFn>,
    presentation: PopoverPresentation,
    disabled: bool,
    inline: bool,
    style: StyleRefinement,
}

impl ContextMenu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        ContextMenu {
            id: id.into(),
            children: SmallVec::new(),
            entries: Vec::new(),
            entries_fn: None,
            menu_width: None,
            size: MenuSize::Md,
            label: None,
            on_open_change: None,
            presentation: PopoverPresentation::Popover,
            disabled: false,
            inline: false,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `items`.
    pub fn items(mut self, entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        self.entries.extend(entries);
        self
    }

    /// Upstream compound `menuContent`: one entry.
    pub fn entry(mut self, entry: impl Into<MenuEntry>) -> Self {
        self.entries.push(entry.into());
        self
    }

    /// Entries built when the menu draws, from what the invocation found
    /// (a text field's word under the pointer).
    pub fn entries_fn(
        mut self,
        f: impl Fn(&mut Window, &mut App) -> Vec<MenuEntry> + 'static,
    ) -> Self {
        self.entries_fn = Some(Rc::new(f));
        self
    }

    astryx_primitives::setters! {
        /// Upstream `menuWidth`: else at least 160px.
        ?menu_width: Length,
        /// Upstream `size`: the rows' density.
        size: MenuSize,
        /// Upstream `label`: the menu's name ("Context menu").
        ?label: SharedString,
        /// Upstream `presentation`.
        presentation: PopoverPresentation,
        /// Upstream `isDisabled`: nothing opens it.
        !disabled,
        /// Upstream `triggerAs="span"`: the area flows inline.
        !inline,
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }
}

impl ParentElement for ContextMenu {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for ContextMenu {
    /// Upstream `triggerXstyle`: the area.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ContextMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut area = div().id(self.id.clone()).relative();
        if self.inline {
            area = area.flex_none().self_start();
        }
        area.style().refine(&self.style);
        // ContextMenu is built on the headless factory (spec:AST-107 FR8).
        let menu = create_context_menu(
            self.id,
            ContextMenuOptions {
                items: self.entries,
                render: None,
                label: self.label,
                disabled: self.disabled,
                on_open_change: self.on_open_change,
                presentation: self.presentation,
                extras: ContextExtras {
                    entries_fn: self.entries_fn,
                    menu_width: self.menu_width,
                    size: self.size,
                },
            },
            window,
            cx,
        );
        menu.attach(area, window, cx).children(self.children)
    }
}

/// Called as a context menu is invoked, before it opens: with the pointer's
/// position, or `None` from the keyboard.
pub(crate) type InvokeFn = Rc<dyn Fn(Option<Point<Pixels>>, &mut Window, &mut App)>;

/// What [`with_context_menu`] opens.
pub(crate) struct ContextSpec {
    pub id: ElementId,
    pub entries: Vec<MenuEntry>,
    /// Custom content in place of the rows (upstream `createContextMenu`'s `render`).
    pub render: Option<ContentFn>,
    pub entries_fn: Option<EntriesFn>,
    pub menu_width: Option<Length>,
    pub size: MenuSize,
    pub label: Option<SharedString>,
    pub on_open_change: Option<OpenFn>,
    pub on_invoke: Option<InvokeFn>,
    pub presentation: PopoverPresentation,
    pub disabled: bool,
}

/// Gives `area` a context menu: its invocations (the right press, Shift+F10
/// and the Menu key, a long press) and, while open, the menu. The area
/// should be `relative`.
pub(crate) fn with_context_menu(
    mut area: Stateful<Div>,
    spec: ContextSpec,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rtl = use_direction(cx).is_rtl();
    let state = context_state(&spec.id, window, cx);
    let root = menu_root(&spec.id, window, cx);
    state.update(cx, |s, _| s.on_open_change = spec.on_open_change.clone());
    if spec.disabled {
        close(&state, window, cx);
    }
    let open = state.read(cx).open;
    sync_root(&root, open, window, cx);
    let label = spec
        .label
        .clone()
        .unwrap_or_else(|| i18n(cx).t("@astryx.contextMenu.label"));
    let sheet = presents_as_sheet(spec.presentation, window, cx);

    // The area's bounds: the keyboard invocation opens at its corner.
    {
        let state = state.clone();
        area = area.child(
            gpui::canvas(
                move |bounds, _, cx| state.update(cx, |s, _| s.bounds = Some(bounds)),
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
    }
    if !spec.disabled {
        let invoke = spec.on_invoke.clone();
        let (s, r) = (state.clone(), root.clone());
        area = area.on_mouse_down(
            MouseButton::Right,
            move |event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                if let Some(invoke) = &invoke {
                    invoke(Some(event.position), window, cx);
                }
                open_at(
                    &s,
                    &r,
                    event.position,
                    false,
                    Some(MenuOpener::Mouse(MouseButton::Right)),
                    window,
                    cx,
                );
            },
        );
        // A long press under a finger (upstream `useLongPress`) opens it under the finger,
        // which then picks the row it lifts on after the handoff travel.
        let invoke = spec.on_invoke.clone();
        let (s, r) = (state.clone(), root.clone());
        let hold = use_long_press(
            ElementId::Name(format!("{}-hold", spec.id).into()),
            LongPressOptions::new(move |at, window, cx| {
                let at = point(px(at.x as f32), px(at.y as f32));
                if let Some(invoke) = &invoke {
                    invoke(Some(at), window, cx);
                }
                open_at(&s, &r, at, false, Some(MenuOpener::Finger), window, cx);
            }),
            window,
            cx,
        );
        area = area.child(hold.probe().absolute().top_0().left_0().size_full());
        let invoke = spec.on_invoke.clone();
        let (s, r) = (state.clone(), root.clone());
        area = area.on_key_down(move |event: &KeyDownEvent, window, cx| {
            let k = &event.keystroke;
            let invoked = (k.key == "f10" && k.modifiers.shift)
                || matches!(k.key.as_str(), "menu" | "contextmenu" | "apps");
            if !invoked {
                return;
            }
            cx.stop_propagation();
            let Some(bounds) = s.read(cx).bounds else {
                return;
            };
            if let Some(invoke) = &invoke {
                invoke(None, window, cx);
            }
            // The bottom edge, stepped back a pixel (upstream
            // `keepAnchorInside`).
            let x = if rtl { bounds.right() } else { bounds.left() };
            let at = point(x, bounds.bottom() - px(1.));
            open_at(&s, &r, at, true, None, window, cx);
        });
    }

    if !open {
        return area;
    }
    let close_state = state.clone();
    let close_menu: MenuAction = Rc::new(move |window, cx| close(&close_state, window, cx));
    let cap = menu_block_cap().min(viewport_cap(t, window).height);
    let tree = MenuTree {
        root: root.clone(),
        close: close_menu.clone(),
        size: spec.size,
        tab_leaves: !sheet,
        touch: sheet,
        drill: sheet || root.read(cx).compact_touch,
        max_height: if sheet { px(100_000.) } else { cap },
        regions: false,
    };
    let entries = match &spec.entries_fn {
        Some(f) => f(window, cx),
        None => spec.entries,
    };
    let list: AnyElement = match &spec.render {
        Some(render) => render(window, cx),
        None => MenuList::new(
            ElementId::Name(format!("{}-list", spec.id).into()),
            entries,
            label.clone(),
            tree,
        )
        .into_any_element(),
    };
    let drilled = root.read(cx).drilled > 0;
    if sheet {
        let s = state.clone();
        return area.child(
            super::sheet::BottomSheet::new(
                ElementId::Name(format!("{}-sheet", spec.id).into()),
                true,
                label.clone(),
            )
            .on_open_change(move |open, window, cx| {
                if !open {
                    close(&s, window, cx);
                }
            })
            .child(
                div()
                    .relative()
                    .w_full()
                    .px(t.spacing.s1)
                    .flex()
                    .flex_col()
                    .child(surface_probe(&root, 0, true))
                    .when(!drilled, |el| {
                        el.child(
                            div()
                                .ml(t.spacing.s3)
                                .mb(t.spacing.s2)
                                .child(Heading::new(HeadingLevel::H3, label.clone())),
                        )
                    })
                    .child(list),
            ),
        );
    }
    let (at, surface_focus) = {
        let s = state.read(cx);
        (s.at, s.surface.clone())
    };
    let mut surface = div()
        .id(ElementId::Name(format!("{}-surface", spec.id).into()))
        .track_focus(&surface_focus)
        .occlude()
        .relative()
        .bg(t.color.background_popover)
        .rounded(t.radius.container)
        .shadow_token(&t.shadow.low)
        .font_stack(&t.font.family.body)
        .text_style(&t.text.body)
        .text_color(t.color.text_primary)
        .child(surface_probe(&root, 0, true))
        .child(list);
    surface = match &spec.menu_width {
        Some(width) => surface.w(*width),
        None => surface.min_w(px(160.)),
    };
    let props = [("size", spec.size.as_str())];
    theme.refine(surface.style(), "context-menu", &props, None);
    let corner = if rtl {
        gpui::Anchor::TopRight
    } else {
        gpui::Anchor::TopLeft
    };
    area.child(
        layer(surface, move |window, cx| close_menu(window, cx))
            .anchor(at, corner)
            .escape(if drilled {
                EscapeBehavior::Block
            } else {
                EscapeBehavior::Close
            }),
    )
}

/// A row menu (List FR13; upstream `useContextMenu` on a row): the row
/// `area` opens `entries` on its context request.
pub(crate) fn with_row_menu(
    area: Stateful<Div>,
    id: ElementId,
    entries: Vec<MenuEntry>,
    label: Option<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    with_context_menu(
        area,
        ContextSpec {
            id,
            entries,
            render: None,
            entries_fn: None,
            menu_width: None,
            size: MenuSize::Md,
            label,
            on_open_change: None,
            on_invoke: None,
            presentation: PopoverPresentation::Popover,
            disabled: false,
        },
        window,
        cx,
    )
}

/// The visible "More actions" button of a row with a menu and no focusable
/// part of its own (List FR13): the keyboard's and a screen reader's path
/// to the row menu `menu`, opening it at the button. Render it in the same
/// render scope as the row's [`with_row_menu`].
pub(crate) fn context_menu_button(
    id: ElementId,
    menu: ElementId,
    label: SharedString,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let state = context_state(&menu, window, cx);
    let root = menu_root(&menu, window, cx);
    crate::button::IconButton::new(id, label.clone(), crate::icon::Icon::new("moreHorizontal"))
        .variant(crate::button::ButtonVariant::Ghost)
        .size(astryx_primitives::ElementSize::Sm)
        .tooltip(label)
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            open_at(
                &state,
                &root,
                event.position(),
                event.is_keyboard(),
                None,
                window,
                cx,
            );
        })
        .into_any_element()
}

// --- The headless layer (spec:AST-107 FR8) -----------------------------------

/// Upstream `createContextMenu`'s options (spec:AST-107 FR8): the shared options mean what
/// ContextMenu's props of the same names mean.
#[derive(Clone, Default)]
pub struct ContextMenuOptions {
    /// The rows.
    pub items: Vec<MenuEntry>,
    /// Custom content in place of the rows.
    pub render: Option<ContentFn>,
    /// The menu's name ("Context menu").
    pub label: Option<SharedString>,
    /// Nothing opens it.
    pub disabled: bool,
    pub on_open_change: Option<OpenFn>,
    pub presentation: PopoverPresentation,
    /// ContextMenu's own props beyond the shared ones.
    pub(crate) extras: ContextExtras,
}

/// What ContextMenu adds on the factory: entries built at the invocation, the menu's
/// width and the rows' size.
#[derive(Clone, Default)]
pub(crate) struct ContextExtras {
    entries_fn: Option<EntriesFn>,
    menu_width: Option<Length>,
    size: MenuSize,
}

impl ContextMenuOptions {
    pub fn items(mut self, items: impl IntoIterator<Item = MenuEntry>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn render(mut self, f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.render = Some(Rc::new(f));
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    pub fn presentation(mut self, presentation: PopoverPresentation) -> Self {
        self.presentation = presentation;
        self
    }
}

/// Upstream `createContextMenu`'s result: `open(at?)`, `close()`, and the element it attaches
/// to ([`ContextMenuHandle::attach`]). Cheap to clone.
#[derive(Clone)]
pub struct ContextMenuHandle {
    id: ElementId,
    state: Entity<ContextState>,
    root: Entity<MenuRoot>,
    options: ContextMenuOptions,
    rtl: bool,
}

impl ContextMenuHandle {
    /// Gives `area` its context menu: it opens on the element's context request (a long
    /// press, a right press, or Shift+F10 and the Menu key), and its menu draws there. The
    /// element should be `relative`. Render it each frame in the scope the hook ran in.
    pub fn attach(&self, area: Stateful<Div>, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        let o = self.options.clone();
        with_context_menu(
            area,
            ContextSpec {
                id: self.id.clone(),
                entries: o.items,
                render: o.render,
                entries_fn: o.extras.entries_fn,
                menu_width: o.extras.menu_width,
                size: o.extras.size,
                label: o.label,
                on_open_change: o.on_open_change,
                on_invoke: None,
                presentation: o.presentation,
                disabled: o.disabled,
            },
            window,
            cx,
        )
    }

    /// Upstream `open(at?)`: opens the menu at `at`, or at the element's bottom-start corner
    /// on its first row.
    pub fn open(&self, at: Option<Point<Pixels>>, window: &mut Window, cx: &mut App) {
        if self.options.disabled {
            return;
        }
        let at = at.map(|p| (p, false)).or_else(|| {
            self.state.read(cx).bounds.map(|b| {
                let x = if self.rtl { b.right() } else { b.left() };
                (point(x, b.bottom() - px(1.)), true)
            })
        });
        if let Some((at, keyboard)) = at {
            open_at(&self.state, &self.root, at, keyboard, None, window, cx);
        }
    }

    /// Upstream `close()`.
    pub fn close(&self, window: &mut Window, cx: &mut App) {
        close(&self.state, window, cx);
    }

    /// Whether the menu is open.
    pub fn is_open(&self, cx: &App) -> bool {
        self.state.read(cx).open
    }
}

/// Upstream `createContextMenu` (spec:AST-107 FR8): a headless context menu for the element
/// `id` names, which ContextMenu is built on. GPUI gives no element handles, so the factory
/// and its hook are one call made where the element renders.
pub fn create_context_menu(
    id: impl Into<ElementId>,
    options: ContextMenuOptions,
    window: &mut Window,
    cx: &mut App,
) -> ContextMenuHandle {
    let id = id.into();
    ContextMenuHandle {
        state: context_state(&id, window, cx),
        root: menu_root(&id, window, cx),
        rtl: use_direction(cx).is_rtl(),
        id,
        options,
    }
}

/// Upstream `useContextMenu(ref, options)`: the thin hook over [`create_context_menu`].
pub fn use_context_menu(
    id: impl Into<ElementId>,
    options: ContextMenuOptions,
    window: &mut Window,
    cx: &mut App,
) -> ContextMenuHandle {
    create_context_menu(id, options, window, cx)
}

const fn doc_prop(prop: &'static str, rust: crate::doc::RustApi) -> crate::doc::PropMapping {
    crate::doc::PropMapping { prop, rust }
}

const CONTEXT_MENU_FACTORY_PROPS: &[crate::doc::PropMapping] = {
    use crate::doc::RustApi::{Argument, Method};
    &[
        doc_prop(
            "element",
            Argument("id (the element's id; ContextMenuHandle::attach gives it the menu)"),
        ),
        doc_prop("options.items", Method("ContextMenuOptions::items")),
        doc_prop("options.render", Method("ContextMenuOptions::render")),
        doc_prop("options.label", Method("ContextMenuOptions::label")),
        doc_prop("options.isDisabled", Method("ContextMenuOptions::disabled")),
        doc_prop(
            "options.onOpenChange",
            Method("ContextMenuOptions::on_open_change"),
        ),
        doc_prop(
            "options.presentation",
            Method("ContextMenuOptions::presentation"),
        ),
        doc_prop("open(at)", Method("ContextMenuHandle::open")),
        doc_prop("close()", Method("ContextMenuHandle::close")),
    ]
};

/// Spec-only (spec:AST-107 FR8; no upstream doc yet).
pub const CREATE_CONTEXT_MENU_DOC: crate::doc::ComponentDoc = crate::doc::ComponentDoc {
    name: "createContextMenu",
    rust_type: "astryx::components::overlay::create_context_menu -> ContextMenuHandle",
    props: CONTEXT_MENU_FACTORY_PROPS,
    example: include_str!("../gestures/examples.rs"),
};

/// Spec-only (spec:AST-107 FR8; no upstream doc yet).
pub const USE_CONTEXT_MENU_DOC: crate::doc::ComponentDoc = crate::doc::ComponentDoc {
    name: "useContextMenu",
    rust_type: "astryx::components::overlay::use_context_menu -> ContextMenuHandle",
    props: CONTEXT_MENU_FACTORY_PROPS,
    example: include_str!("../gestures/examples.rs"),
};
