//! SideNav, upstream `SideNav/SideNav.tsx`: the app's sidebar navigation. A sticky
//! top (header, top content), a scrolling list, and a sticky bottom (footer, then the
//! collapse button and footer icons at the small size). It collapses to an icon rail
//! and resizes on `use_resizable`.
//!
//! Collapsing or expanding the rail plays one motion on one clock (FR6, upstream
//! `railMotion.ts`): the shared two-state inline edge reveal
//! ([`crate::animated_layout::inline`]) snaps the layout to the final width and moves the
//! rail's inline-end edge from the width it shows to the new one, riding the glide of
//! AppShell's content column (AppShell FR17); SideNav keeps only its labels' and icons'
//! fades. The rail's old state is one transient copy: a snapshot of the rail's old look
//! (fork patch R3's kept offscreen layers), inert and out of the accessibility tree, for
//! every change, the toggle's or a controlled `is_collapsed`'s. GPUI keeps no frame it was
//! not asked to, so the rail takes the copy as the change plays: a collapse keeps the
//! frames in which it still presents its old state, and an expand presents the old,
//! collapsed rail in its first frame and keeps it (upstream's copy covers the rail in full
//! then, so that frame looks the same). A
//! collapse presents the old, expanded rail with its labels fading over the first 40%;
//! over the last 30% the copy of it fades out while the real collapsed rail, raised
//! above it, fades in, icons only. An expand reveals the real rail with its labels
//! fading in over the last 60%, under the copy of the old, collapsed rail, which fades
//! over the first 60%. A change during a motion starts from what the rail shows. A
//! renderer without offscreen layers keeps no snapshot: there the rail presents
//! its old state until the copy would fade, then its new one.
//!
//! Outside AppShell there is no content column to ride, so the rail's own box follows its
//! edge and what sits beside it reflows.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Role, StyleRefinement,
    Styled, Window, div, prelude::*, px,
};
use rustryx_i18n::{LogicalStyled as _, current as i18n};
use rustryx_primitives::{ElementSize, provide, use_context};
use rustryx_theme::current;

use super::{SideNavCollapse, SideNavCollapseButton, SideNavRenderMode, SideNavShell};
use crate::animated_layout::inline::use_two_state_edge;
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
        let real_collapsed = collapsible && region.is_collapsed;
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
        let shell = use_context::<SideNavShell>(cx);
        // FR6's transient copy is a snapshot of the rail's old look (fork patch R3), and
        // which state it shows: kept across frames.
        let copy_shows = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-rail-copy-shows", self.id).into()),
                cx,
                |_, _| Rc::new(std::cell::Cell::new(None::<bool>)),
            )
            .read(cx)
            .clone();
        let copy_id = ElementId::Name(format!("{}-rail-copy", self.id).into());
        let snapshots = window.supports_offscreen_layers();
        let resizable = !matches!(self.resizable, Resizable::Off);
        // FR6: the rail's edge, its fades, and which state it presents.
        let rem = window.rem_size();
        let collapsed_width = t.spacing.s12.to_pixels(rem);
        let expanded_width = if resizable { px(region.size) } else { px(260.) };
        let rail_id = ElementId::Name(format!("{}-rail-edge", self.id).into());
        let edge = (mode == SideNavRenderMode::Rail).then(|| {
            use_two_state_edge(
                rail_id.clone(),
                real_collapsed,
                if real_collapsed {
                    collapsed_width
                } else {
                    expanded_width
                },
                shell.as_ref().map(|s| &s.layout),
                window,
                cx,
            )
        });
        let clamp = |v: f32| v.clamp(0., 1.);
        let (labels, icons, collapsed) = match &edge {
            Some(edge) if edge.moving => {
                let p = edge.progress;
                let [from_labels, from_icons] = edge.carried;
                let (labels, icons) = if edge.cross_fade {
                    (
                        if real_collapsed { 0. } else { p },
                        if real_collapsed { p } else { 0. },
                    )
                } else if edge.conceals {
                    (
                        from_labels * (1. - clamp(p / 0.4)),
                        from_icons + (1. - from_icons) * clamp((p - 0.7) / 0.3),
                    )
                } else {
                    (
                        from_labels + (1. - from_labels) * clamp((p - 0.4) / 0.6),
                        from_icons * (1. - clamp(p / 0.6)),
                    )
                };
                // A collapse presents the old, expanded rail until the copy would fade. An
                // expand's first frame presents the old, collapsed rail once more and keeps
                // it, when no copy of it is kept yet: at that moment upstream's copy covers
                // the real rail in full, so the frame looks the same.
                let capture_old = snapshots
                    && !edge.conceals
                    && !edge.cross_fade
                    && edge.old != edge.state
                    && copy_shows.get() != Some(edge.old);
                let presented = if (edge.conceals && edge.presents_old(0.7) && !edge.cross_fade)
                    || capture_old
                {
                    edge.old
                } else {
                    real_collapsed
                };
                edge.record([labels, icons], cx);
                (labels, icons, presented)
            }
            Some(edge) => {
                let rest = if real_collapsed { [0., 1.] } else { [1., 0.] };
                edge.record(rest, cx);
                (1., 1., real_collapsed)
            }
            None => (1., 1., real_collapsed),
        };
        // FR6's transient copy: a snapshot of the rail's last frame (fork patch R3). A
        // collapse draws it from 70%, under the real collapsed rail; an expand draws it
        // over the real rail until it has faded, at 60%. The rail keeps its frames while
        // no copy shows, so the copy is the last frame of the old state.
        // An expand captures the old rail in its first frame (above).
        let capturing = edge.as_ref().is_some_and(|e| {
            e.moving && !e.conceals && !e.cross_fade && e.old != e.state && snapshots
        }) && collapsed != real_collapsed;
        let copy = match &edge {
            Some(edge)
                if edge.moving
                    && !edge.cross_fade
                    && edge.old != edge.state
                    && snapshots
                    && !capturing =>
            {
                let p = edge.progress;
                if edge.conceals {
                    // Before 70% the rail presents its old state itself and keeps its
                    // frames, so the copy is the old rail with its labels faded; drawn
                    // unseen until then, which keeps it.
                    Some(RailCopy {
                        opacity: if p < 0.7 {
                            0.
                        } else {
                            1. - clamp((p - 0.7) / 0.3)
                        },
                        width: expanded_width,
                        above: false,
                        keeps: p < 0.7,
                    })
                } else {
                    (p < 0.6).then_some(RailCopy {
                        opacity: icons,
                        width: collapsed_width,
                        above: true,
                        keeps: false,
                    })
                }
            }
            _ => None,
        };
        // A collapse's real collapsed rail fades in under the copy's fade, icons only.
        let incoming = copy
            .as_ref()
            .filter(|c| !c.above && !c.keeps)
            .map(|_| icons);
        // AppShell's content swaps on the rail's clock (AppShell FR17, AST-105 FR8).
        if let Some(shell) = &shell {
            let swap = edge
                .as_ref()
                .filter(|e| e.moving && e.old != e.state)
                .map(|e| super::RailSwap {
                    progress: e.progress,
                    cross_fade: e.cross_fade,
                });
            shell.swap.set(swap);
        }
        let context = SideNavCollapse {
            collapsed,
            collapsible,
            toggle,
            labels,
        };
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
        let moving = edge.as_ref().is_some_and(|e| e.moving);
        let laid_out = if real_collapsed {
            collapsed_width
        } else {
            expanded_width
        };
        // The presented state's own width, which its content keeps under the clip.
        let content_width = if collapsed {
            collapsed_width
        } else {
            expanded_width
        };
        let width = match &edge {
            Some(edge) if moving && !edge.cross_fade => edge.extent,
            _ => laid_out,
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
        let rides = shell.as_ref().is_some_and(|s| s.rides);
        if let Some(shell) = &shell {
            let gap = match &edge {
                Some(e) if rides && moving && !e.cross_fade && !e.conceals => {
                    (laid_out - e.extent).max(px(0.))
                }
                _ => px(0.),
            };
            shell.cover.set(gap);
        }
        if let Some(edge) = edge.as_ref().filter(|_| moving) {
            if edge.cross_fade {
                // Reduce Motion under a coarse pointer: opacity only.
                nav = nav.opacity(edge.progress);
            } else if rides {
                // In AppShell the layout snaps to the final width: the edge's clip frees
                // (or takes) the difference by its inline-end margin, and the content
                // column rides the glide beside it.
                let free = laid_out - edge.extent;
                nav = nav.margin_end(rustryx_i18n::use_direction(cx), free);
            }
        }
        if rides {
            // The rail's own parts take their place at once; only the content beside it
            // glides.
            let ease = t.ease.r#move;
            nav = nav.layout_glide(gpui::LayoutGlide::new(
                ElementId::Name(format!("{}-rail-parts", self.id).into()),
                t.duration.medium,
                move |x| ease.ease(x),
            ));
        }
        // The presented state keeps its own width under the clip, anchored at the rail's
        // inline start, so the moving inline-end edge reveals or covers it.
        if rustryx_i18n::use_direction(cx).is_rtl() {
            nav = nav.items_end();
        }
        let mut parts = div()
            .flex()
            .flex_col()
            .h_full()
            .flex_none()
            .w(content_width);
        if has_top {
            parts = parts.child(
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
        let scroller_handle = crate::gestures::scroller::use_scroll_handle(
            ElementId::Name(format!("{:?}-list", self.id).into()),
            window,
            cx,
        );
        let list = div()
            .id(ElementId::Name(format!("{:?}-list", self.id).into()))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&scroller_handle)
            .px(t.spacing.s2)
            .pt(if has_top { t.spacing.s1 } else { t.spacing.s2 })
            .pb(if has_bottom {
                t.spacing.s1
            } else {
                t.spacing.s2
            })
            .when(collapsed, |d| d.items_center())
            .child(crate::gestures::scroller::scroll_begin(
                ElementId::Name(format!("{:?}-list", self.id).into()),
                Some(&scroller_handle),
                (false, true),
            ))
            .children(self.children)
            .child(crate::gestures::scroller::scroll_end());
        parts = parts.child(list);
        if has_bottom || show_button {
            parts = parts.child(
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
        if let Some(opacity) = incoming {
            parts = parts.layer_opacity(opacity);
        }
        let rtl = rustryx_i18n::use_direction(cx).is_rtl();
        let copy_element = copy.as_ref().map(|c| {
            let el = div()
                .debug_selector({
                    let s = format!("{}-rail-copy", self.id);
                    move || s
                })
                .absolute()
                .top_0()
                .h_full()
                .w(c.width)
                .child(
                    gpui::layer_snapshot(copy_id.clone())
                        .size_full()
                        .layer_opacity(c.opacity),
                );
            if rtl { el.right_0() } else { el.left_0() }
        });
        if capturing || copy.as_ref().is_some_and(|c| c.keeps) {
            // Kept for the copy: an expand's first frame, which presents the old,
            // collapsed rail, and a collapse's frames while the rail presents its old,
            // expanded state.
            parts = parts.retain_layer(copy_id.clone());
            copy_shows.set(Some(collapsed));
        }
        if !edge.as_ref().is_some_and(|e| e.moving) && copy_shows.take().is_some() {
            // At rest no copy is kept: the next change takes its own.
            window.release_layer_snapshot(&copy_id);
        }
        nav = nav.relative();
        let above = copy.as_ref().is_some_and(|c| c.above);
        let (below, over) = if above {
            (None, copy_element)
        } else {
            (copy_element, None)
        };
        nav = nav.children(below).child(parts).children(over);
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
            .when(!collapsed && !moving, |d| {
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

/// The rail's transient copy this frame (FR6).
struct RailCopy {
    opacity: f32,
    /// The old state's width, which the copy keeps under the clip.
    width: gpui::Pixels,
    /// Over the real rail (an expand) rather than under it (a collapse).
    above: bool,
    /// Unseen while the rail presents its old state and keeps its frames for it.
    keeps: bool,
}
