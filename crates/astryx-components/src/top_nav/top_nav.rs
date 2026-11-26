//! TopNav, upstream `TopNav/TopNav.tsx`: the app header's navigation bar. A heading and
//! start content at the inline start and end content at the inline end; with center
//! content, three regions that center it. The bar wraps when it runs out of room (FR3,
//! FR4). Inside AppShell it renders as the phone's bar (heading and end content, FR5) or
//! as the drawer's content (FR6).

use std::cell::Cell;
use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::{provide, use_context};
use astryx_theme::current;
use gpui::{
    AnyElement, App, Bounds, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Window, canvas, div, prelude::*, relative,
};

use super::hover::HoverGroup;
use crate::app_shell::use_app_shell_mobile;
use crate::layout::Divider;
use crate::mobile_nav::{MobileNav, MobileNavToggle};

/// The slot a TopNav part sits in (FR7): a menu's panel aligns to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TopNavSlot {
    #[default]
    Start,
    Center,
    End,
}

/// Upstream `TopNavRenderMode`: the bar, or (from AppShell) the phone's bar or its drawer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TopNavRenderMode {
    #[default]
    Default,
    MobileBar,
    Drawer,
}

/// Upstream `TopNavMobileContentContext`: the side nav content AppShell shows in the
/// drawer below the TopNav's items.
#[derive(Clone)]
pub(crate) struct TopNavMobileContent(pub BuildFn);

/// Builds an element when rendered.
pub(crate) type BuildFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// What a TopNav tells its parts: its box (a mega menu's panel hangs below it), the
/// slot, and the hover group of its menus.
#[derive(Clone)]
pub(crate) struct TopNavFrame {
    pub bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub slot: TopNavSlot,
    pub group: HoverGroup,
}

/// Upstream `TopNav`.
#[derive(IntoElement)]
pub struct TopNav {
    id: ElementId,
    heading: Option<AnyElement>,
    start: Vec<AnyElement>,
    children: Vec<AnyElement>,
    center: Option<AnyElement>,
    end: Option<AnyElement>,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl TopNav {
    pub fn new(id: impl Into<ElementId>) -> Self {
        TopNav {
            id: id.into(),
            heading: None,
            start: Vec::new(),
            children: Vec::new(),
            center: None,
            end: None,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `heading`: at the bar's inline start (a TopNavHeading).
    pub fn heading(mut self, heading: impl IntoElement) -> Self {
        self.heading = Some(heading.into_any_element());
        self
    }

    /// Upstream `startContent`: after the heading. It wins over children (FR2).
    pub fn start_content(mut self, content: impl IntoElement) -> Self {
        self.start.push(content.into_any_element());
        self
    }

    /// Upstream `centerContent`: centered between the start and end regions.
    pub fn center_content(mut self, content: impl IntoElement) -> Self {
        self.center = Some(content.into_any_element());
        self
    }

    /// Upstream `endContent`: at the inline end.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end = Some(content.into_any_element());
        self
    }

    /// Upstream `label`: the landmark's name. Default "Top navigation".
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for TopNav {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for TopNav {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TopNav {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let mode = use_context::<TopNavRenderMode>(cx).unwrap_or_default();
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| i18n(cx).t("@astryx.topNav.landmarkLabel"));
        // FR2: start content wins over children.
        let start = if self.start.is_empty() {
            self.children
        } else {
            self.start
        };
        let landmark = |mode_prop: &str| {
            let mut nav = div()
                .id(self.id.clone())
                .debug_selector({
                    let s = format!("{}", self.id);
                    move || s
                })
                .flex()
                .items_center()
                .w_full()
                .p(t.spacing.s2)
                .role(Role::Navigation)
                .aria_label(label.clone());
            theme.refine(nav.style(), "top-nav", &[("mode", mode_prop)], None);
            nav.style().refine(&self.style);
            nav
        };

        match mode {
            // FR5: the heading yields width first; start and center content do not render.
            TopNavRenderMode::MobileBar => {
                // FR5: the toggle shows when the drawer has content and toggles are automatic.
                let has_drawer_content = !start.is_empty()
                    || self.center.is_some()
                    || use_context::<TopNavMobileContent>(cx).is_some();
                let toggle =
                    (has_drawer_content && use_app_shell_mobile(cx).has_auto_toggle).then(|| {
                        MobileNavToggle::new(ElementId::Name(format!("{}-toggle", self.id).into()))
                    });
                landmark("mobile-bar")
                    .children(self.heading.map(|heading| {
                        div()
                            .flex()
                            .items_center()
                            .flex_grow_0()
                            .flex_shrink(1.)
                            .min_w_0()
                            .truncate()
                            .child(heading)
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.spacing.s1)
                            .flex_none()
                            .ml_auto()
                            .max_w_full()
                            .children(self.end)
                            .children(toggle),
                    )
                    .into_any_element()
            }
            // FR6: the drawer's content: start then center content in a list, a divider,
            // then any side nav content AppShell passes. With neither, nothing.
            TopNavRenderMode::Drawer => {
                let has_items = !start.is_empty() || self.center.is_some();
                let mobile = use_context::<TopNavMobileContent>(cx);
                if !has_items && mobile.is_none() {
                    return div().into_any_element();
                }
                let divider = has_items && mobile.is_some();
                let mut drawer =
                    MobileNav::new(ElementId::Name(format!("{}-drawer", self.id).into()));
                if let Some(heading) = self.heading {
                    drawer = drawer.header(heading);
                }
                drawer
                    .when(has_items, |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(t.spacing.s0_5)
                                .children(start)
                                .children(self.center),
                        )
                    })
                    .when(divider, |el| {
                        el.child(div().my(t.spacing.s2).child(Divider::new()))
                    })
                    .children(mobile.map(|m| (m.0)(window, cx)))
                    .into_any_element()
            }
            TopNavRenderMode::Default => {
                let shared = window
                    .use_keyed_state(
                        ElementId::Name(format!("{:?}-frame", self.id).into()),
                        cx,
                        |_, _| (Rc::new(Cell::new(None)), HoverGroup::default()),
                    )
                    .read(cx)
                    .clone();
                let frame = |slot| TopNavFrame {
                    bounds: shared.0.clone(),
                    slot,
                    group: shared.1.clone(),
                };
                let probe = {
                    let cell = shared.0.clone();
                    canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {})
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                };
                let has_start = !start.is_empty();
                let left = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(t.spacing.s4)
                    .flex_1()
                    .flex_basis(relative(0.))
                    .children(self.heading.map(|h| {
                        div()
                            .flex()
                            .items_center()
                            .flex_shrink(1.)
                            .min_w_0()
                            .child(h)
                    }))
                    .when(has_start, |el| {
                        el.child(provide(
                            frame(TopNavSlot::Start),
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(t.spacing.s1)
                                .children(start),
                        ))
                    });
                let nav = landmark("").flex_wrap().relative();
                let nav = match self.center {
                    None => nav
                        .child(left.flex_none().flex_basis(gpui::auto()))
                        .children(self.end.map(|e| {
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .justify_end()
                                .gap(t.spacing.s1)
                                .flex_none()
                                .ml_auto()
                                .max_w_full()
                                .child(provide(frame(TopNavSlot::End), e))
                        })),
                    Some(center) => nav
                        .child(left)
                        .child(provide(
                            frame(TopNavSlot::Center),
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap(t.spacing.s1)
                                .flex_grow(1000.)
                                .flex_shrink(1.)
                                .min_w_0()
                                .child(center),
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .justify_end()
                                .gap(t.spacing.s1)
                                .flex_1()
                                .flex_basis(relative(0.))
                                .children(self.end.map(|e| provide(frame(TopNavSlot::End), e))),
                        ),
                };
                nav.child(probe).into_any_element()
            }
        }
    }
}
