//! Outline, upstream `Outline/Outline.tsx` with `useScrollSpy`: a table of contents. A list of
//! links to the page's headings, indented by level (FR1, FR2), each in the Tab order
//! (FR3), over a 2px track with an indicator that slides to the current one (FR6).
//! Choosing a link scrolls its heading to the top of the scroll root, below `offset`
//! (FR4); uncontrolled, the current heading follows the reader's scroll (FR5).

use crate::press::TouchPress as _;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::anchor::anchor;
use astryx_primitives::{control_focus, is_focus_visible, prefers_reduced_motion};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    App, AriaCurrent, Bounds, ElementId, FontWeight, IntoElement, ParentElement, Pixels,
    RenderOnce, Role, ScrollHandle, SharedString, StyleRefinement, Styled, Window, canvas, div,
    point, prelude::*, px,
};
use web_time::Instant;

type IdFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// Upstream `OutlineItem`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineItem {
    /// The heading's id (its anchor).
    pub id: SharedString,
    pub label: SharedString,
    /// The heading level, 1 to 6.
    pub level: u8,
}

impl OutlineItem {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, level: u8) -> Self {
        OutlineItem {
            id: id.into(),
            label: label.into(),
            level,
        }
    }
}

/// Upstream `density`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutlineDensity {
    #[default]
    Default,
    Compact,
}

/// A navigation in progress (FR4).
struct Navigation {
    id: SharedString,
    handle: ScrollHandle,
    from: Pixels,
    to: Pixels,
    start: Instant,
    /// The offset this navigation set last frame; anything else is the reader's scroll.
    last: Pixels,
}

/// The indicator's motion (FR6).
#[derive(Clone, Copy)]
struct Slide {
    from: (Pixels, Pixels),
    to: (Pixels, Pixels),
    start: Instant,
}

#[derive(Default)]
struct OutlineState {
    active: Option<SharedString>,
    reported: Option<SharedString>,
    navigation: Option<Navigation>,
    links: Rc<RefCell<HashMap<SharedString, Bounds<Pixels>>>>,
    list: Rc<RefCell<Option<Bounds<Pixels>>>>,
    slide: Option<Slide>,
}

/// Upstream `Outline`.
#[derive(IntoElement)]
pub struct Outline {
    id: ElementId,
    items: Vec<OutlineItem>,
    active_id: Option<SharedString>,
    on_active_id_change: Option<IdFn>,
    label: Option<SharedString>,
    density: OutlineDensity,
    on_navigate_start: Option<IdFn>,
    on_navigate_end: Option<IdFn>,
    offset: Pixels,
    scroll_container: Option<ScrollHandle>,
    scroll_on_click: bool,
    style: StyleRefinement,
}

impl Outline {
    pub fn new(id: impl Into<ElementId>, items: impl IntoIterator<Item = OutlineItem>) -> Self {
        Outline {
            id: id.into(),
            items: items.into_iter().collect(),
            active_id: None,
            on_active_id_change: None,
            label: None,
            density: OutlineDensity::Default,
            on_navigate_start: None,
            on_navigate_end: None,
            offset: px(0.),
            scroll_container: None,
            scroll_on_click: true,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `activeId`: given, the Outline does not track the scroll.
    pub fn active_id(mut self, id: impl Into<SharedString>) -> Self {
        self.active_id = Some(id.into());
        self
    }

    /// Upstream `onActiveIdChange`.
    pub fn on_active_id_change(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_active_id_change = Some(Rc::new(f));
        self
    }

    /// Upstream `label`: default "Table of contents".
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `density`.
    pub fn density(mut self, density: OutlineDensity) -> Self {
        self.density = density;
        self
    }

    /// Upstream `onNavigateStart`.
    pub fn on_navigate_start(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_navigate_start = Some(Rc::new(f));
        self
    }

    /// Upstream `onNavigateEnd`: once per start, when the scroll settles or the reader
    /// interrupts it.
    pub fn on_navigate_end(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_navigate_end = Some(Rc::new(f));
        self
    }

    /// Upstream `offset`: the height of a header overlaying the scroll root's top.
    pub fn offset(mut self, offset: Pixels) -> Self {
        self.offset = offset;
        self
    }

    /// Upstream `scrollContainerRef`: the scroll root, else the headings' own.
    pub fn scroll_container(mut self, handle: ScrollHandle) -> Self {
        self.scroll_container = Some(handle);
        self
    }

    /// Upstream `hasScrollOnClick`: false owns the scrolling yourself. Default true.
    pub fn scroll_on_click(mut self, scroll: bool) -> Self {
        self.scroll_on_click = scroll;
        self
    }
}

impl Styled for Outline {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// FR2: levels 1 and 2 share the first indent; 3, 4 and 5-6 take the next three.
fn indent(level: u8, t: &astryx_theme::Tokens) -> gpui::AbsoluteLength {
    match level.saturating_sub(1).clamp(1, 4) {
        1 => t.spacing.s3,
        2 => t.spacing.s7,
        3 => t.spacing.s11,
        _ => t.spacing.s12,
    }
}

/// Ends a navigation: the heading becomes current and `on_navigate_end` fires.
fn end_navigation(
    state: &gpui::Entity<OutlineState>,
    end: &Option<IdFn>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(nav) = state.update(cx, |s, _| s.navigation.take()) else {
        return;
    };
    state.update(cx, |s, _| s.active = Some(nav.id.clone()));
    if let Some(end) = end.clone() {
        let id = nav.id;
        window.defer(cx, move |window, cx| end(id, window, cx));
    }
    window.refresh();
}

impl RenderOnce for Outline {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let reduced = prefers_reduced_motion(cx);
        let now = cx.background_executor().now();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-outline", self.id).into()),
            cx,
            |_, _| OutlineState::default(),
        );
        // The scroll root: the caller's, else the first heading's.
        let root = self.scroll_container.clone().or_else(|| {
            self.items
                .iter()
                .find_map(|item| anchor(&item.id, window, cx).and_then(|a| a.scroll_root))
        });

        // FR4: a navigation scrolls smoothly; the reader's own scroll ends it.
        let duration = t.duration.medium;
        if let Some((handle, from, to, start, last)) = state
            .read(cx)
            .navigation
            .as_ref()
            .map(|n| (n.handle.clone(), n.from, n.to, n.start, n.last))
        {
            if (handle.offset().y - last).abs() > px(0.5) {
                end_navigation(&state, &self.on_navigate_end, window, cx);
            } else {
                let progress = (now.saturating_duration_since(start).as_secs_f32()
                    / duration.as_secs_f32())
                .min(1.);
                let y = from + (to - from) * t.ease.standard.ease(progress);
                handle.set_offset(point(handle.offset().x, y));
                state.update(cx, |s, _| {
                    if let Some(n) = s.navigation.as_mut() {
                        n.last = y;
                    }
                });
                if progress < 1. {
                    window.request_animation_frame();
                } else {
                    end_navigation(&state, &self.on_navigate_end, window, cx);
                }
            }
        }

        // FR5: uncontrolled, the last heading past the activation line is current; the
        // first before any, the last at the scroll root's end.
        let controlled = self.active_id.is_some();
        if !controlled && state.read(cx).navigation.is_none() && !self.items.is_empty() {
            let spied = root.as_ref().and_then(|handle| {
                let port = handle.bounds();
                let line = port.top() + self.offset + px(1.);
                let at_end = handle.max_offset().y > px(0.)
                    && handle.offset().y <= -handle.max_offset().y + px(1.);
                if at_end {
                    return self.items.last().map(|i| i.id.clone());
                }
                let mut current = None;
                for item in &self.items {
                    if let Some(a) = anchor(&item.id, window, cx)
                        && a.bounds.top() <= line
                    {
                        current = Some(item.id.clone());
                    }
                }
                Some(current.unwrap_or_else(|| self.items[0].id.clone()))
            });
            let current = spied
                .or_else(|| state.read(cx).active.clone())
                .unwrap_or_else(|| self.items[0].id.clone());
            state.update(cx, |s, _| s.active = Some(current));
        }
        let active = self
            .active_id
            .clone()
            .or_else(|| state.read(cx).active.clone());
        // Uncontrolled, report each change.
        if !controlled
            && let Some(active) = active.clone()
            && state.read(cx).reported.as_ref() != Some(&active)
        {
            let first = state.read(cx).reported.is_none();
            state.update(cx, |s, _| s.reported = Some(active.clone()));
            if !first && let Some(f) = self.on_active_id_change.clone() {
                window.defer(cx, move |window, cx| f(active, window, cx));
            }
        }

        // FR6: the indicator slides to the current link's box.
        let target = active.as_ref().and_then(|id| {
            let list = (*state.read(cx).list.borrow())?;
            let link = state.read(cx).links.borrow().get(id).copied()?;
            Some((link.top() - list.top(), link.size.height))
        });
        let indicator = target.map(|to| {
            let slide = state.read(cx).slide;
            let current = |s: Slide| {
                let p = (now.saturating_duration_since(s.start).as_secs_f32()
                    / t.duration.fast_min.as_secs_f32().max(0.001))
                .min(1.);
                let e = t.ease.standard.ease(p);
                (
                    (
                        s.from.0 + (s.to.0 - s.from.0) * e,
                        s.from.1 + (s.to.1 - s.from.1) * e,
                    ),
                    p < 1.,
                )
            };
            match slide {
                Some(s) if s.to == to => {
                    let (at, moving) = current(s);
                    if moving && !reduced {
                        window.request_animation_frame();
                        at
                    } else {
                        to
                    }
                }
                Some(s) if !reduced => {
                    let (at, _) = current(s);
                    state.update(cx, |st, _| {
                        st.slide = Some(Slide {
                            from: at,
                            to,
                            start: now,
                        })
                    });
                    window.request_animation_frame();
                    at
                }
                _ => {
                    state.update(cx, |st, _| {
                        st.slide = Some(Slide {
                            from: to,
                            to,
                            start: now,
                        })
                    });
                    to
                }
            }
        });

        // A link's activation (FR4): the shared navigation path.
        let navigate: IdFn = {
            let (state, root, items) = (state.clone(), root.clone(), self.items.clone());
            let (start, end, report) = (
                self.on_navigate_start.clone(),
                self.on_navigate_end.clone(),
                self.on_active_id_change.clone(),
            );
            let (offset, scroll, controlled) = (self.offset, self.scroll_on_click, controlled);
            Rc::new(move |id, window, cx| {
                let Some(target) = anchor(&id, window, cx) else {
                    return;
                };
                let _ = &items;
                // A second navigation ends the first at once.
                end_navigation(&state, &end, window, cx);
                if let Some(f) = &start {
                    f(id.clone(), window, cx);
                }
                if controlled && let Some(f) = &report {
                    f(id.clone(), window, cx);
                }
                let handle = target.scroll_root.clone().or_else(|| root.clone());
                let Some(handle) = handle.filter(|_| scroll) else {
                    state.update(cx, |s, _| s.active = Some(id.clone()));
                    if let Some(f) = &end {
                        f(id, window, cx);
                    }
                    window.refresh();
                    return;
                };
                let port = handle.bounds();
                let from = handle.offset().y;
                let to = (from - (target.bounds.top() - (port.top() + offset)))
                    .min(px(0.))
                    .max(-handle.max_offset().y);
                if prefers_reduced_motion(cx) {
                    // DR3: no travel, it lands at once.
                    handle.set_offset(point(handle.offset().x, to));
                    state.update(cx, |s, _| s.active = Some(id.clone()));
                    if let Some(f) = &end {
                        f(id, window, cx);
                    }
                    window.refresh();
                    return;
                }
                let now = cx.background_executor().now();
                state.update(cx, |s, _| {
                    s.navigation = Some(Navigation {
                        id,
                        handle,
                        from,
                        to,
                        start: now,
                        last: from,
                    })
                });
                window.refresh();
            })
        };

        let links = state.read(cx).links.clone();
        let list_cell = state.read(cx).list.clone();
        let pad_y = match self.density {
            OutlineDensity::Default => t.spacing.s2,
            OutlineDensity::Compact => t.spacing.s1,
        };
        let list = div()
            .id(ElementId::Name(format!("{}-list", self.id).into()))
            .flex()
            .flex_col()
            .gap(t.spacing.s0_5)
            .flex_1()
            .min_w_0()
            .role(Role::List)
            .child(
                canvas(
                    move |bounds, _, _| *list_cell.borrow_mut() = Some(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .children(self.items.iter().map(|item| {
                let link_id = ElementId::Name(format!("{}-{}", self.id, item.id).into());
                let focus = control_focus(&link_id, window, cx);
                let is_active = active.as_ref() == Some(&item.id);
                let (navigate, id) = (navigate.clone(), item.id.clone());
                let probe = {
                    let (links, id) = (links.clone(), item.id.clone());
                    canvas(
                        move |bounds, _, _| {
                            links.borrow_mut().insert(id, bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                };
                let primary = t.color.text_primary;
                let mut link = div()
                    .id(link_id.clone())
                    .debug_selector({
                        let s = format!("{link_id}");
                        move || s
                    })
                    .track_focus(&focus)
                    .relative()
                    .flex()
                    .items_center()
                    .w_full()
                    .py(pad_y)
                    .pr(t.spacing.s2)
                    .pl(indent(item.level, &t))
                    .rounded(t.radius.element)
                    .font_stack(&t.font.family.body)
                    .text_style(&t.text.body)
                    .font_weight(FontWeight::NORMAL)
                    .text_color(t.color.text_secondary)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(primary))
                    // A finger's press paints as UIKit tracks a cell (upstream
                    // `usePressFeedback`).
                    .touch_press(
                        ElementId::Name(format!("{link_id}-press").into()),
                        gpui::transparent_black(),
                        t.color.overlay_pressed,
                        window,
                        cx,
                    )
                    .role(Role::Link)
                    .aria_label(item.label.clone())
                    .on_click(move |event, window, cx| {
                        // A modifier chord is the platform's.
                        let m = event.modifiers();
                        if m.control || m.platform || m.shift || m.alt {
                            return;
                        }
                        navigate(id.clone(), window, cx)
                    })
                    .child(div().min_w_0().truncate().child(item.label.clone()))
                    .child(probe);
                if is_active {
                    link = link
                        .text_color(t.color.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .aria_current(AriaCurrent::Location);
                }
                theme.refine(
                    link.style(),
                    "outline-item",
                    &[
                        ("level", level_name(item.level)),
                        ("active", if is_active { "active" } else { "" }),
                    ],
                    None,
                );
                let link = link.children(is_focus_visible(&focus, window).then(|| {
                    crate::focus::focus_ring_inset(&t, t.radius.element.to_pixels(rem), rem)
                }));
                div()
                    .id(ElementId::Name(format!("{link_id}-item").into()))
                    .role(Role::ListItem)
                    .child(link)
            }));
        // The track and the indicator, hidden from assistive technology.
        let track = div()
            .relative()
            .w(px(2.))
            .flex_none()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(2.))
                    .rounded_full()
                    .bg(t.color.border),
            )
            .children(indicator.map(|(top, height)| {
                div()
                    .absolute()
                    .left_0()
                    .top(top)
                    .h(height)
                    .w(px(2.))
                    .rounded_full()
                    .bg(t.color.icon_primary)
            }));
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| i18n(cx).t("@astryx.outline.label"));
        let mut nav = div()
            .id(self.id.clone())
            .debug_selector({
                let s = format!("{}", self.id);
                move || s
            })
            .relative()
            .flex()
            .flex_row()
            .gap(t.spacing.s0_5)
            .w_full()
            .role(Role::Navigation)
            .aria_label(label)
            .child(track)
            .child(list);
        theme.refine(
            nav.style(),
            "outline",
            &[(
                "density",
                match self.density {
                    OutlineDensity::Default => "default",
                    OutlineDensity::Compact => "compact",
                },
            )],
            None,
        );
        nav.style().refine(&self.style);
        nav
    }
}

fn level_name(level: u8) -> &'static str {
    match level {
        1 => "1",
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        _ => "6",
    }
}
