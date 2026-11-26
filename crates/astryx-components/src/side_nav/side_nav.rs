//! SideNav, upstream `SideNav/SideNav.tsx`: the app's sidebar navigation. A sticky
//! top (header, top content), a scrolling list, and a sticky bottom (footer, then the
//! collapse button and footer icons at the small size). It collapses to an icon rail
//! and resizes on `use_resizable`. Upstream's rail motion (FR6, the shared edge
//! reveal of AST-105) waits for the animated-layout system (M11); the rail switches at
//! once.

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::{ElementSize, provide, use_context};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Role, StyleRefinement,
    Styled, Window, div, prelude::*, px,
};

use super::{SideNavCollapse, SideNavCollapseButton, SideNavRenderMode};
use crate::mobile_nav::MobileNav;
use crate::resizable::{
    ResizableRegion, ResizeHandle, ResizeHandlePosition, UseResizable, use_resizable,
};

/// Upstream's collapse under this width on a lift.
const COLLAPSE_THRESHOLD: f32 = 160.;

type BoolFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type SizeFn = Rc<dyn Fn(f32, &mut Window, &mut App)>;

/// Upstream `collapsible`'s object form.
#[derive(Clone)]
pub struct SideNavCollapseConfig {
    pub default_is_collapsed: Option<bool>,
    /// Controlled.
    pub is_collapsed: Option<bool>,
    pub on_collapsed_change: Option<BoolFn>,
    /// Whether the built-in collapse button shows. Default true.
    pub has_button: bool,
}

impl Default for SideNavCollapseConfig {
    fn default() -> Self {
        SideNavCollapseConfig {
            default_is_collapsed: None,
            is_collapsed: None,
            on_collapsed_change: None,
            has_button: true,
        }
    }
}

/// Upstream `ResizableConfig`: `resizable`'s object form.
#[derive(Clone)]
pub struct SideNavResizableConfig {
    pub default_width: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub auto_save_id: Option<gpui::SharedString>,
    pub on_width_change: Option<SizeFn>,
    pub default_is_collapsed: Option<bool>,
    pub is_collapsed: Option<bool>,
    pub on_collapse_change: Option<BoolFn>,
}

impl Default for SideNavResizableConfig {
    fn default() -> Self {
        SideNavResizableConfig {
            default_width: 260.,
            min_width: 180.,
            max_width: 480.,
            auto_save_id: None,
            on_width_change: None,
            default_is_collapsed: None,
            is_collapsed: None,
            on_collapse_change: None,
        }
    }
}

#[derive(Clone)]
enum Resizable {
    Off,
    Config(SideNavResizableConfig),
    Region(ResizableRegion),
}

/// Upstream `SideNav`.
#[derive(IntoElement)]
pub struct SideNav {
    id: ElementId,
    header: Option<AnyElement>,
    top_content: Option<AnyElement>,
    children: Vec<AnyElement>,
    footer: Option<AnyElement>,
    footer_icons: Option<AnyElement>,
    collapsible: Option<SideNavCollapseConfig>,
    resizable: Resizable,
    style: StyleRefinement,
}

impl SideNav {
    pub fn new(id: impl Into<ElementId>) -> Self {
        SideNav {
            id: id.into(),
            header: None,
            top_content: None,
            children: Vec::new(),
            footer: None,
            footer_icons: None,
            collapsible: None,
            resizable: Resizable::Off,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `header`: typically a SideNavHeading.
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// Upstream `topContent`: pinned below the header.
    pub fn top_content(mut self, content: impl IntoElement) -> Self {
        self.top_content = Some(content.into_any_element());
        self
    }

    /// Upstream `footer`: above the icon bar.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Upstream `footerIcons`: the icon bar, at the small size.
    pub fn footer_icons(mut self, icons: impl IntoElement) -> Self {
        self.footer_icons = Some(icons.into_any_element());
        self
    }

    /// Upstream `collapsible={true}`: uncontrolled, with the built-in button.
    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.collapsible = collapsible.then(SideNavCollapseConfig::default);
        self
    }

    /// Upstream `collapsible`'s object form.
    pub fn collapsible_config(mut self, config: SideNavCollapseConfig) -> Self {
        self.collapsible = Some(config);
        self
    }

    /// Upstream `resizable={true}`: 260 px, from 180 to 480.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = if resizable {
            Resizable::Config(SideNavResizableConfig::default())
        } else {
            Resizable::Off
        };
        self
    }

    /// Upstream `resizable`'s object form.
    pub fn resizable_config(mut self, config: SideNavResizableConfig) -> Self {
        self.resizable = Resizable::Config(config);
        self
    }

    /// Upstream `resizable` given `useResizable`'s output: the region owns the width
    /// and the collapse state (FR7).
    pub fn resizable_region(mut self, region: ResizableRegion) -> Self {
        self.resizable = Resizable::Region(region);
        self
    }
}

impl ParentElement for SideNav {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for SideNav {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SideNav {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let i18n = i18n(cx);
        let mode = use_context::<SideNavRenderMode>(cx).unwrap_or_default();
        let collapse = self.collapsible.clone();
        let is_collapsible = collapse.is_some();
        let config = match &self.resizable {
            Resizable::Config(c) => c.clone(),
            _ => SideNavResizableConfig::default(),
        };
        // The collapse state lives in a region either way (upstream drives both through
        // useResizable): its own, or the caller's (FR7).
        let own = {
            let c = collapse.clone().unwrap_or_default();
            let mut hook = UseResizable::new()
                .default_size(config.default_width)
                .min_size(config.min_width)
                .max_size(config.max_width)
                .collapsible(is_collapsible)
                .collapsed_size(COLLAPSE_THRESHOLD);
            if let Some(id) = config.auto_save_id.clone() {
                hook = hook.auto_save_id(id);
            }
            if let Some(d) = c.default_is_collapsed.or(config.default_is_collapsed) {
                hook = hook.default_is_collapsed(d);
            }
            if let Some(v) = c.is_collapsed.or(config.is_collapsed) {
                hook = hook.is_collapsed(v);
            }
            if let Some(f) = config.on_width_change.clone() {
                hook = hook.on_size_change(move |size, window, cx| f(size, window, cx));
            }
            let report = c
                .on_collapsed_change
                .clone()
                .or(config.on_collapse_change.clone());
            if let Some(f) = report {
                hook = hook.on_collapse_change(move |v, window, cx| f(v, window, cx));
            }
            use_resizable(
                ElementId::Name(format!("{:?}-rail", self.id).into()),
                hook,
                window,
                cx,
            )
        };
        let region = match &self.resizable {
            Resizable::Region(region) => region.clone(),
            _ => own,
        };
        let collapsible = match &self.resizable {
            Resizable::Region(region) => region.props.collapsible,
            _ => is_collapsible,
        };
        let collapsed = collapsible && region.is_collapsed;
        let toggle: super::Action = {
            let region = region.clone();
            Rc::new(move |window, cx| {
                if region.is_collapsed {
                    region.expand(window, cx);
                } else {
                    region.collapse(window, cx);
                }
            })
        };
        let context = SideNavCollapse {
            collapsed,
            collapsible,
            toggle,
        };
        let resizable = !matches!(self.resizable, Resizable::Off);
        let icons_row = |icons: Option<AnyElement>, button: bool| {
            let mut row = div().flex().items_center().gap(t.spacing.s1);
            if collapsed {
                row = row.flex_col_reverse();
            }
            provide(
                ElementSize::Sm,
                row.when(button, |r| {
                    r.child(
                        div()
                            .debug_selector(|| "side-nav-collapse".to_string())
                            .child(SideNavCollapseButton::new()),
                    )
                })
                .children(icons),
            )
        };

        match mode {
            SideNavRenderMode::TopBar => {
                let mut bar = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(48.))
                    .w_full()
                    .overflow_hidden();
                theme.refine(bar.style(), "side-nav", &[("mode", "topbar")], None);
                bar.style().refine(&self.style);
                return bar
                    .children(self.header)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.spacing.s1)
                            .ml_auto()
                            .child(provide(
                                ElementSize::Sm,
                                div().flex().children(self.footer_icons),
                            )),
                    )
                    .into_any_element();
            }
            SideNavRenderMode::Drawer | SideNavRenderMode::DrawerContent => {
                let has_footer = self.footer.is_some() || self.footer_icons.is_some();
                let body = div()
                    .flex()
                    .flex_col()
                    .children(self.top_content)
                    .children(self.children)
                    .when(has_footer, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_col()
                                .mt_auto()
                                .gap(t.spacing.s2)
                                .pt(t.spacing.s2)
                                .children(self.footer)
                                .children(self.footer_icons.map(|icons| {
                                    provide(
                                        ElementSize::Sm,
                                        div().flex().items_center().gap(t.spacing.s1).child(icons),
                                    )
                                })),
                        )
                    });
                let body = provide(SideNavCollapse::expanded(), body);
                // The drawer: a MobileNav headed by the header (upstream MobileNavDrawer).
                if mode == SideNavRenderMode::Drawer {
                    let mut drawer =
                        MobileNav::new(ElementId::Name(format!("{}-drawer", self.id).into()));
                    if let Some(header) = self.header {
                        drawer = drawer.header(header);
                    }
                    drawer.style().refine(&self.style);
                    return drawer.child(body).into_any_element();
                }
                return body.into_any_element();
            }
            SideNavRenderMode::Rail => {}
        }

        let has_top = self.header.is_some() || self.top_content.is_some();
        let has_bottom = self.footer.is_some() || self.footer_icons.is_some();
        let show_button = collapsible && collapse.as_ref().is_none_or(|c| c.has_button);
        let width = if collapsed {
            t.spacing.s12.to_pixels(window.rem_size())
        } else if resizable {
            px(region.size)
        } else {
            px(260.)
        };
        let mut nav = div()
            .id(self.id.clone())
            .debug_selector({
                let s = format!("{}", self.id);
                move || s
            })
            .flex()
            .flex_col()
            .h_full()
            .w(width)
            .flex_none()
            .overflow_hidden()
            .role(Role::Navigation)
            .aria_label(i18n.t("@astryx.sideNav.label"));
        theme.refine(nav.style(), "side-nav", &[], None);
        nav.style().refine(&self.style);
        if has_top {
            nav = nav.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .pt(t.spacing.s2)
                    .pb(t.spacing.s2)
                    .px(t.spacing.s2)
                    .gap(t.spacing.s2)
                    .when(collapsed, |d| d.items_center())
                    .children(self.header)
                    .children(self.top_content.map(|c| div().child(c))),
            );
        }
        let list = div()
            .id(ElementId::Name(format!("{:?}-list", self.id).into()))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(t.spacing.s2)
            .pt(if has_top { t.spacing.s1 } else { t.spacing.s2 })
            .pb(if has_bottom {
                t.spacing.s1
            } else {
                t.spacing.s2
            })
            .when(collapsed, |d| d.items_center())
            .children(self.children);
        nav = nav.child(list);
        if has_bottom || show_button {
            nav = nav.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .mt_auto()
                    .gap(t.spacing.s2)
                    .px(t.spacing.s2)
                    .pt(if collapsed {
                        px(0.)
                    } else {
                        t.spacing.s1.to_pixels(window.rem_size())
                    })
                    .pb(t.spacing.s2)
                    .when(collapsed, |d| d.items_center())
                    .children(self.footer)
                    .when(show_button || self.footer_icons.is_some(), |d| {
                        d.child(icons_row(self.footer_icons, show_button))
                    }),
            );
        }
        let nav = provide(context, nav);
        if !resizable {
            return nav.into_any_element();
        }
        // The resize handle sits over the rail's inline-end edge, hidden while collapsed.
        div()
            .relative()
            .flex()
            .flex_none()
            .h_full()
            .child(nav)
            .when(!collapsed, |d| {
                d.child(
                    ResizeHandle::new(ElementId::Name(format!("{:?}-resize", self.id).into()))
                        .position(ResizeHandlePosition::Overlay)
                        .resizable(&region)
                        .label(i18n.t("@astryx.sideNav.resizeSidebar")),
                )
            })
            .into_any_element()
    }
}
