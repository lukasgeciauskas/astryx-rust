//! Upstream `ContextMenu`: a menu that opens where it is invoked on an
//! area.
//!
//! - A right press opens it at the pointer (and, held, picks the row it is
//!   released on, as the press model says); Shift+F10 or the Menu key opens
//!   it at the area's bottom-start corner on its first row; a long press on
//!   a touch screen opens it under the finger.
//! - A press outside closes it at the press; Escape closes the top-most
//!   level only (FR9); Tab leaves the menu (FR11). Focus returns to what had
//!   it.
//! - The sheet presentations hold the rows in a BottomSheet as touch rows.
//!
//! Upstream builds on the browser's `contextmenu` event; here the area takes
//! the right press and the keys itself.

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{EscapeBehavior, layer};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, Div, ElementId, Entity, FocusHandle, IntoElement, KeyDownEvent,
    Length, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Point, RenderOnce,
    SharedString, Stateful, StyleRefinement, Styled, Task, Window, div, point, prelude::*, px,
};
use smallvec::SmallVec;

use super::menu::{
    Initial, MenuAction, MenuEntry, MenuList, MenuRoot, MenuSize, MenuTree, PressGesture,
    menu_block_cap, menu_root, surface_probe, sync_root,
};
use super::motion::viewport_cap;
use super::popover::{PopoverPresentation, presents_as_sheet};
use crate::text::{Heading, HeadingLevel};

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type EntriesFn = Rc<dyn Fn(&mut Window, &mut App) -> Vec<MenuEntry>>;

/// Upstream `LONG_PRESS_DURATION_MS`.
const LONG_PRESS: Duration = Duration::from_millis(500);
/// How far a finger may drift before a long press is a pan.
const LONG_PRESS_TOLERANCE: f32 = 10.;

/// A context menu's state between frames.
pub(crate) struct ContextState {
    open: bool,
    /// Where it opened, in window coordinates.
    at: Point<Pixels>,
    return_to: Option<FocusHandle>,
    surface: FocusHandle,
    /// The area's bounds as of last frame.
    bounds: Option<Bounds<Pixels>>,
    long_press: Option<(Task<()>, Point<Pixels>)>,
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
            long_press: None,
            on_open_change: None,
        },
    )
}

/// Opens the menu at `at`: `keyboard` lands on the first row; `press` is the
/// button still held, for the press model.
fn open_at(
    state: &Entity<ContextState>,
    root: &Entity<MenuRoot>,
    at: Point<Pixels>,
    keyboard: bool,
    press: Option<MouseButton>,
    window: &mut Window,
    cx: &mut App,
) {
    let focused = window.focused(cx);
    let now = cx.background_executor().now();
    let (was_open, report) = state.update(cx, |s, _| {
        let was_open = s.open;
        s.open = true;
        s.at = at;
        s.long_press = None;
        if !was_open {
            s.return_to = focused;
        }
        (was_open, s.on_open_change.clone())
    });
    root.update(cx, |r, _| {
        r.initial = Some(if keyboard {
            Initial::First
        } else {
            Initial::Container
        });
        r.press = press.map(|button| PressGesture {
            at: now,
            entered: false,
            button,
        });
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
        let spec = ContextSpec {
            id: self.id,
            entries: self.entries,
            entries_fn: self.entries_fn,
            menu_width: self.menu_width,
            size: self.size,
            label: self.label,
            on_open_change: self.on_open_change,
            on_invoke: None,
            presentation: self.presentation,
            disabled: self.disabled,
        };
        with_context_menu(area, spec, window, cx).children(self.children)
    }
}

/// Called as a context menu is invoked, before it opens: with the pointer's
/// position, or `None` from the keyboard.
pub(crate) type InvokeFn = Rc<dyn Fn(Option<Point<Pixels>>, &mut Window, &mut App)>;

/// What [`with_context_menu`] opens.
pub(crate) struct ContextSpec {
    pub id: ElementId,
    pub entries: Vec<MenuEntry>,
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
    let sheet = presents_as_sheet(spec.presentation, window);

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
                    Some(MouseButton::Right),
                    window,
                    cx,
                );
            },
        );
        // A long press under a finger.
        let invoke = spec.on_invoke.clone();
        let (s, r) = (state.clone(), root.clone());
        area = area.on_mouse_down(
            MouseButton::Left,
            move |event: &MouseDownEvent, window, cx| {
                if !window.last_input_was_touch() {
                    return;
                }
                let at = event.position;
                let timer = cx.background_executor().timer(LONG_PRESS);
                let (weak, root, invoke) = (s.downgrade(), r.clone(), invoke.clone());
                let task = window.spawn(cx, async move |cx| {
                    timer.await;
                    cx.update(|window, cx| {
                        if let Some(state) = weak.upgrade() {
                            if let Some(invoke) = &invoke {
                                invoke(Some(at), window, cx);
                            }
                            open_at(&state, &root, at, false, None, window, cx);
                        }
                    })
                    .ok();
                });
                s.update(cx, |s, _| s.long_press = Some((task, at)));
            },
        );
        let s = state.clone();
        area = area.on_mouse_move(move |event: &MouseMoveEvent, _, cx| {
            let drifted = s.read(cx).long_press.as_ref().is_some_and(|(_, from)| {
                let (dx, dy) = (
                    f32::from(event.position.x - from.x),
                    f32::from(event.position.y - from.y),
                );
                (dx * dx + dy * dy).sqrt() > LONG_PRESS_TOLERANCE
            });
            if drifted {
                s.update(cx, |s, _| s.long_press = None);
            }
        });
        let s = state.clone();
        area = area.on_mouse_up(MouseButton::Left, move |_, _, cx| {
            if s.read(cx).long_press.is_some() {
                s.update(cx, |s, _| s.long_press = None);
            }
        });
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
    let list = MenuList::new(
        ElementId::Name(format!("{}-list", spec.id).into()),
        entries,
        label.clone(),
        tree,
    );
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
