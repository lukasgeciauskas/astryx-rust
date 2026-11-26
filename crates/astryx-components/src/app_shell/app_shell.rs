//! AppShell, upstream `AppShell/AppShell.tsx`: the app's outermost frame. A skip link, then a
//! Layout of the header region (banner above the top nav), the side nav beside the main
//! region, and in the mobile layout the drawer (FR1). Below the theme's width point the
//! side nav leaves the inline region for the drawer, and the top nav turns into the
//! phone's bar (FR8-FR12); AppShell tells its subtree about it through
//! `AppShellMobile` (FR13). At its root a Layout lays out as at the page root (FR18).
//!
//! While the inline side nav collapses or expands (FR17), the shell holds its own animated
//! layout handle: the rail's change arms it, so the content column beside the rail glides
//! from its old place to its new one on the rail's one clock, the theme's move curve over
//! `--duration-medium`, and never animates its width; on an expand the content surface
//! reaches over the gap the narrower column leaves at its trailing edge until the rail's
//! edge closes it. Upstream also holds the old content as one transient copy that fades out
//! as the new content fades in (AST-105 FR8's swap); that copy waits for a snapshot of
//! main's last frame, fork patch R3's offscreen layers in M11 stage 5, so the content
//! plays upstream's no-copy path for now. Under Reduce Motion it cross-fades with a coarse pointer and lands at
//! once with a fine one.

use std::cell::Cell;
use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::scroll_area::{
    ScrollAxis, ScrollKeyboardAccess, ScrollRole, UseScrollableAreaOptions, use_scrollable_area,
};
use astryx_primitives::{control_focus, is_focus_visible, provide};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Bounds, ElementId, FocusHandle, FontWeight, IntoElement, ParentElement,
    Pixels, RenderOnce, Role, StyleRefinement, Styled, Window, canvas, deferred, div, prelude::*,
    px,
};

use super::mobile::{AppShellMobile, OpenFn};
use crate::layout::{Layout, LayoutHeader, LayoutHeight, LayoutPanel, SpacingStep, layout_reset};
use crate::mobile_nav::MobileNavToggle;
use crate::side_nav::{SideNavRenderMode, SideNavShell};
use crate::top_nav::{TopNavMobileContent, TopNavRenderMode};

/// Builds a slot's element; AppShell may draw a nav more than once (the bar, the drawer).
pub type SlotFn = Rc<dyn Fn() -> AnyElement>;

/// Upstream `variant`: how the nav areas contrast with the content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AppShellVariant {
    /// The body background everywhere, no dividers.
    Wash,
    /// The surface everywhere, no dividers.
    Surface,
    /// Dividers between the nav areas and the content.
    Section,
    /// Wash nav areas around a raised surface content area.
    #[default]
    Elevated,
}

impl AppShellVariant {
    fn as_str(self) -> &'static str {
        match self {
            AppShellVariant::Wash => "wash",
            AppShellVariant::Surface => "surface",
            AppShellVariant::Section => "section",
            AppShellVariant::Elevated => "elevated",
        }
    }
}

/// Upstream `AppShellBreakpoint`: the theme's width point below which the mobile layout applies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AppShellBreakpoint {
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
    /// Upstream `2xl`.
    Xl2,
    /// Never the mobile layout.
    None,
}

impl AppShellBreakpoint {
    fn name(self) -> Option<&'static str> {
        match self {
            AppShellBreakpoint::Sm => Some("sm"),
            AppShellBreakpoint::Md => Some("md"),
            AppShellBreakpoint::Lg => Some("lg"),
            AppShellBreakpoint::Xl => Some("xl"),
            AppShellBreakpoint::Xl2 => Some("2xl"),
            AppShellBreakpoint::None => None,
        }
    }
}

/// Upstream `MobileNavConfig`.
#[derive(Clone)]
pub struct MobileNavConfig {
    /// Upstream `hasToggle`: the menu toggle places itself. Default true.
    pub has_toggle: bool,
    /// Upstream `isOpen`: given, AppShell does not own the drawer's state.
    pub is_open: Option<bool>,
    /// Upstream `onOpenChange`: every requested change.
    pub on_open_change: Option<OpenFn>,
    /// Upstream `content`: replaces the auto drawer (a MobileNav, typically).
    pub content: Option<SlotFn>,
    /// Upstream `breakpoint`. Default `md`.
    pub breakpoint: AppShellBreakpoint,
}

impl Default for MobileNavConfig {
    fn default() -> Self {
        MobileNavConfig {
            has_toggle: true,
            is_open: None,
            on_open_change: None,
            content: None,
            breakpoint: AppShellBreakpoint::Md,
        }
    }
}

/// Upstream `mobileNav`: a config, `false`, or a node.
#[derive(Clone)]
pub enum AppShellMobileNav {
    /// The automatic drawer and toggle, configured.
    Auto(MobileNavConfig),
    /// Upstream `false`: no drawer and no toggle (FR12).
    Disabled,
    /// A node rendered as given at every width; it replaces the auto drawer.
    Node(SlotFn),
}

impl Default for AppShellMobileNav {
    fn default() -> Self {
        AppShellMobileNav::Auto(MobileNavConfig::default())
    }
}

/// The shell's state between frames.
struct ShellState {
    open: bool,
    /// Main's focus in `auto` (in `fill` it is its scroll region's).
    main: FocusHandle,
    /// The skip link moved focus to main: no ring (FR4).
    skipped: bool,
    /// The header region's height, measured in `auto` (FR6).
    header_height: Rc<Cell<Pixels>>,
    opener: Rc<Cell<Option<Bounds<Pixels>>>>,
}

/// Upstream `AppShell`.
#[derive(IntoElement)]
pub struct AppShell {
    id: ElementId,
    variant: AppShellVariant,
    banner: Option<AnyElement>,
    children: Vec<AnyElement>,
    content_padding: SpacingStep,
    height: LayoutHeight,
    top_nav: Option<SlotFn>,
    side_nav: Option<SlotFn>,
    mobile_nav: AppShellMobileNav,
    style: StyleRefinement,
}

impl AppShell {
    pub fn new(id: impl Into<ElementId>) -> Self {
        AppShell {
            id: id.into(),
            variant: AppShellVariant::Elevated,
            banner: None,
            children: Vec::new(),
            content_padding: SpacingStep::S0,
            height: LayoutHeight::Fill,
            top_nav: None,
            side_nav: None,
            mobile_nav: AppShellMobileNav::default(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `variant`. Default elevated.
    pub fn variant(mut self, variant: AppShellVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Upstream `banner`: system-wide announcements above the top nav.
    pub fn banner(mut self, banner: impl IntoElement) -> Self {
        self.banner = Some(banner.into_any_element());
        self
    }

    /// Upstream `contentPadding`: the main region's padding. Default 0.
    pub fn content_padding(mut self, step: SpacingStep) -> Self {
        self.content_padding = step;
        self
    }

    /// Upstream `height`: fill (the default) or auto.
    pub fn height(mut self, height: LayoutHeight) -> Self {
        self.height = height;
        self
    }

    /// Upstream `topNav`: a TopNav, built each time AppShell draws it.
    pub fn top_nav<E: IntoElement>(mut self, build: impl Fn() -> E + 'static) -> Self {
        self.top_nav = Some(Rc::new(move || build().into_any_element()));
        self
    }

    /// Upstream `sideNav`: a SideNav, built each time AppShell draws it.
    pub fn side_nav<E: IntoElement>(mut self, build: impl Fn() -> E + 'static) -> Self {
        self.side_nav = Some(Rc::new(move || build().into_any_element()));
        self
    }

    /// Upstream `mobileNav`.
    pub fn mobile_nav(mut self, mobile_nav: AppShellMobileNav) -> Self {
        self.mobile_nav = mobile_nav;
        self
    }
}

impl ParentElement for AppShell {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for AppShell {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for AppShell {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let i18n = i18n(cx);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-shell", self.id).into()),
            cx,
            |_, cx| ShellState {
                open: false,
                main: cx.focus_handle().tab_stop(false),
                skipped: false,
                header_height: Rc::new(Cell::new(px(0.))),
                opener: Rc::default(),
            },
        );
        let config = match &self.mobile_nav {
            AppShellMobileNav::Auto(config) => Some(config.clone()),
            _ => None,
        };
        let disabled = matches!(self.mobile_nav, AppShellMobileNav::Disabled);
        let node = match &self.mobile_nav {
            AppShellMobileNav::Node(build) => Some(build.clone()),
            _ => None,
        };
        // FR8: below the theme's width point; equality is the wider layout.
        let breakpoint = config
            .as_ref()
            .map_or(AppShellBreakpoint::Md, |c| c.breakpoint);
        let is_mobile = breakpoint
            .name()
            .and_then(|name| theme.theme.width_breakpoints.get(name))
            .is_some_and(|w| window.viewport_size().width < px(w));
        let controlled = config.as_ref().and_then(|c| c.is_open);
        let is_open = controlled.unwrap_or(state.read(cx).open);
        let has_top = self.top_nav.is_some();
        let has_side = self.side_nav.is_some();
        let has_banner = self.banner.is_some();
        // FR11: open and toggle need a nav, and neither false nor a node.
        let enabled = !disabled && (has_top || has_side) && node.is_none();
        let has_toggle = config.as_ref().is_none_or(|c| c.has_toggle);
        let set_open: OpenFn = {
            let (state, report) = (
                state.clone(),
                config.as_ref().and_then(|c| c.on_open_change.clone()),
            );
            Rc::new(move |open, window, cx| {
                if open && !enabled {
                    return;
                }
                if controlled.is_none() {
                    state.update(cx, |s, _| s.open = open);
                }
                if let Some(report) = &report {
                    report(open, window, cx);
                }
                window.refresh();
            })
        };
        let mobile = AppShellMobile {
            is_mobile,
            is_open,
            enabled,
            has_auto_toggle: has_toggle,
            set_open,
            opener: state.read(cx).opener.clone(),
        };
        let is_auto = self.height == LayoutHeight::Auto;
        let variant = self.variant.as_str();
        let (body, surface) = (t.color.background_body, t.color.background_surface);
        let nav_bg = match self.variant {
            AppShellVariant::Wash | AppShellVariant::Elevated => Some(body),
            AppShellVariant::Surface => Some(surface),
            AppShellVariant::Section => None,
        };
        let header_bg =
            nav_bg.or((is_auto && self.variant == AppShellVariant::Section).then_some(surface));
        let dividers = self.variant == AppShellVariant::Section;
        let side_drawer_content = |side: &SlotFn| {
            let side = side.clone();
            TopNavMobileContent(Rc::new(move |_, _| {
                provide(SideNavRenderMode::DrawerContent, side()).into_any_element()
            }))
        };

        // FR3: the header region, with the banner above the top nav.
        let top_takes_bar = !disabled && node.is_none();
        let top_content = self.top_nav.as_ref().map(|top| {
            if is_mobile && top_takes_bar {
                let bar = provide(TopNavRenderMode::MobileBar, top());
                match self.side_nav.as_ref().filter(|_| has_toggle) {
                    Some(side) => provide(side_drawer_content(side), bar).into_any_element(),
                    None => bar.into_any_element(),
                }
            } else {
                top()
            }
        });
        let header_height = state.read(cx).header_height.clone();
        let header = (has_top || has_banner).then(|| {
            let probe = {
                let cell = header_height.clone();
                canvas(
                    move |bounds, _, _| cell.set(bounds.size.height),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
            };
            let mut el = div()
                .id(ElementId::Name(format!("{}-header", self.id).into()))
                .debug_selector({
                    let s = format!("{}-header", self.id);
                    move || s
                })
                .relative()
                .flex_none()
                .role(Role::Banner)
                .when_some(header_bg, |el, bg| el.bg(bg))
                .child(
                    LayoutHeader::new()
                        .padding(SpacingStep::S0)
                        .has_divider(dividers && has_top)
                        .children(self.banner.map(|b| {
                            div()
                                .flex_none()
                                .when_some(nav_bg, |el, bg| el.bg(bg))
                                .child(b)
                        }))
                        .children(top_content),
                )
                .child(probe);
            theme.refine(
                el.style(),
                "app-shell-header",
                &[("variant", variant)],
                None,
            );
            el
        });
        // FR9: the side-nav-only phone bar, beside its toggle.
        let side_bar = (is_mobile && has_side && !has_top && !disabled && has_toggle).then(|| {
            let side = self.side_nav.clone().unwrap();
            let mut el = div()
                .id(ElementId::Name(format!("{}-mobile-bar", self.id).into()))
                .flex()
                .flex_none()
                .items_center()
                .h(t.spacing.s12)
                .px(t.spacing.s2)
                .when_some(nav_bg.or(Some(surface)), |el, bg| el.bg(bg))
                .when(!has_top && !has_banner, |el| el.role(Role::Banner))
                .child(
                    div()
                        .id(ElementId::Name(
                            format!("{}-mobile-nav-region", self.id).into(),
                        ))
                        .flex_1()
                        .min_w_0()
                        .role(Role::Navigation)
                        .aria_label(i18n.t("@astryx.appShell.mobileNavigation"))
                        .child(provide(SideNavRenderMode::TopBar, side())),
                )
                .child(MobileNavToggle::new(ElementId::Name(
                    format!("{}-toggle", self.id).into(),
                )));
            theme.refine(
                el.style(),
                "app-shell-header",
                &[("variant", variant)],
                None,
            );
            el
        });

        // FR17: the shell's own animated layout, lent to its inline SideNav.
        let content_layout = crate::animated_layout::use_animated_layout(
            ElementId::Name(format!("{}-content-layout", self.id).into()),
            window,
            cx,
        );
        let side_shell = SideNavShell {
            layout: content_layout.clone(),
            cover: Rc::new(Cell::new(px(0.))),
            rides: !is_auto,
        };
        // FR6, FR7: the inline side region; sticky below the header in auto.
        let side_inline = (has_side && !is_mobile).then(|| {
            let side = self.side_nav.clone().unwrap();
            let mut panel = LayoutPanel::new(ElementId::Name(format!("{}-side", self.id).into()))
                .padding(SpacingStep::S0)
                .has_divider(dividers)
                .scrollable(!is_auto)
                .child(provide(side_shell.clone(), side()));
            if let Some(bg) = nav_bg.or(is_auto.then_some(surface)) {
                panel = panel.bg(bg);
            }
            theme.refine(
                panel.style(),
                "app-shell-sidenav",
                &[("variant", variant)],
                None,
            );
            if is_auto {
                let below = header_height.get();
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .overflow_hidden()
                    .sticky_top(below)
                    .h(window.viewport_size().height - below)
                    .child(panel.flex_1())
                    .into_any_element()
            } else {
                panel.into_any_element()
            }
        });

        // FR4: the main region, focusable, a scroll region in fill.
        let main_id = ElementId::Name(format!("{}-main", self.id).into());
        let padding = self.content_padding.token(&t).to_pixels(rem);
        let inset = crate::layout::ContainerPadding {
            inline_start: padding,
            inline_end: padding,
            block_start: padding,
            block_end: padding,
        };
        let children = crate::layout::padding_container(
            inset,
            None,
            div().flex().flex_col().children(self.children),
            cx,
        );
        let skipped = state.read(cx).skipped;
        let (main, main_focus) = if is_auto {
            let focus = state.read(cx).main.clone();
            let el = div()
                .id(main_id.clone())
                .track_focus(&focus)
                .relative()
                .flex_1()
                .p(padding)
                .child(children);
            (el, focus)
        } else {
            let area = use_scrollable_area(
                &main_id,
                UseScrollableAreaOptions::new(
                    ScrollAxis::Block,
                    ScrollKeyboardAccess::Viewport {
                        label: "".into(),
                        role: ScrollRole::Group,
                    },
                ),
                window,
                cx,
            );
            let focus = area.focus_handle().clone();
            let root = astryx_primitives::anchor::ScrollRoot(area.scroll_handle().clone());
            let el = area
                .viewport(div().relative().size_full().p(padding))
                .child(provide(root, children));
            (el, focus)
        };
        // A ring for Tab focus on a scroll region, never for the skip link's move.
        let ring = !skipped && is_focus_visible(&main_focus, window);
        if skipped && !main_focus.is_focused(window) {
            state.update(cx, |s, _| s.skipped = false);
        }
        let elevated =
            self.variant == AppShellVariant::Elevated && has_top && side_inline.is_some();
        let page_radius = t.radius.page;
        let main = main
            .debug_selector({
                let s = format!("{}", main_id);
                move || s
            })
            .role(Role::Main)
            .when(ring, |el| {
                el.child(crate::focus::focus_ring_inset(
                    &t,
                    page_radius.to_pixels(rem),
                    rem,
                ))
            });
        // FR17: on an expand, the content surface reaches over the gap at its trailing edge
        // until the rail's edge closes it.
        let cover_color = match self.variant {
            AppShellVariant::Wash => Some(body),
            AppShellVariant::Surface | AppShellVariant::Elevated => Some(surface),
            AppShellVariant::Section => None,
        };
        let cover = cover_color.map(|color| ContentCover {
            gap: side_shell.cover.clone(),
            color,
            rtl: astryx_i18n::use_direction(cx).is_rtl(),
        });
        // FR16: elevated, the content area is a raised surface with the page's corner.
        let content = if elevated {
            div()
                .relative()
                .flex()
                .flex_1()
                .min_h_0()
                .h_full()
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(surface)
                        .rounded_tl(page_radius),
                )
                .child(main)
                .children(cover)
                .into_any_element()
        } else {
            let bg = cover_color;
            div()
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .size_full()
                .child(main.when_some(bg, |el, bg| el.bg(bg)))
                .children(cover)
                .into_any_element()
        };

        // FR5: the skip link, hidden until focused, moves focus to main.
        let skip_id = ElementId::Name(format!("{}-skip", self.id).into());
        let skip_focus = control_focus(&skip_id, window, cx);
        let skip_shown = skip_focus.is_focused(window);
        let skip = {
            let (state, main_focus) = (state.clone(), main_focus.clone());
            let el = div()
                .id(skip_id.clone())
                .debug_selector({
                    let s = format!("{skip_id}");
                    move || s
                })
                .track_focus(&skip_focus)
                .role(Role::Link)
                .aria_label(i18n.t("@astryx.appShell.skipToContent"))
                .on_click(move |_, window, cx| {
                    state.update(cx, |s, _| s.skipped = true);
                    window.focus(&main_focus, cx);
                })
                .child(i18n.t("@astryx.appShell.skipToContent"));
            if skip_shown {
                deferred(
                    el.absolute()
                        .top(t.spacing.s2)
                        .left(t.spacing.s2)
                        .px(t.spacing.s4)
                        .py(t.spacing.s2)
                        .bg(surface)
                        .text_color(t.color.text_accent)
                        .font_stack(&t.font.family.body)
                        .text_style(&t.text.body)
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .with_priority(3)
                .into_any_element()
            } else {
                el.absolute()
                    .size(px(1.))
                    .overflow_hidden()
                    .opacity(0.)
                    .into_any_element()
            }
        };

        // FR10, FR12: the drawer, a node, or the config's content.
        let drawer = if let Some(node) = &node {
            Some(node())
        } else if enabled && is_mobile {
            match config.as_ref().and_then(|c| c.content.clone()) {
                Some(content) => Some(content()),
                None => match (&self.top_nav, &self.side_nav) {
                    (Some(top), side) => {
                        let drawer = provide(TopNavRenderMode::Drawer, top());
                        Some(match side {
                            Some(side) => {
                                provide(side_drawer_content(side), drawer).into_any_element()
                            }
                            None => drawer.into_any_element(),
                        })
                    }
                    (None, Some(side)) => {
                        Some(provide(SideNavRenderMode::Drawer, side()).into_any_element())
                    }
                    (None, None) => None,
                },
            }
        } else {
            None
        };

        let layout = Layout::new()
            .without_pane_glide()
            .height(self.height)
            .padding(SpacingStep::S0)
            // In auto the header slot sticks at the top of the page (FR6); it is the
            // Layout column's own child, so the whole shell is its containing block.
            .header(
                div()
                    .flex()
                    .flex_col()
                    .when(is_auto, |el| el.sticky_top(px(0.)))
                    .children(header)
                    .children(side_bar),
            )
            .content(content);
        let layout = match side_inline {
            Some(side) => layout.start(side),
            None => layout,
        };
        let mut root = div()
            .id(self.id.clone())
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .overflow_hidden()
            .bg(match self.variant {
                AppShellVariant::Wash | AppShellVariant::Elevated => body,
                AppShellVariant::Surface | AppShellVariant::Section => surface,
            })
            .when(!is_auto, |el| el.h_full())
            .when(is_auto, |el| el.min_h_full())
            .child(skip)
            .child(layout)
            .children(drawer);
        theme.refine(root.style(), "app-shell", &[("variant", variant)], None);
        root.style().refine(&self.style);
        let root = content_layout.container(root, window, cx);
        provide(mobile, layout_reset(root, cx))
    }
}

/// The content surface past the column's trailing edge while the rail expands (FR17):
/// rendered after the rail, which writes the gap its edge has yet to close.
#[derive(IntoElement)]
struct ContentCover {
    gap: Rc<Cell<Pixels>>,
    color: gpui::Hsla,
    rtl: bool,
}

impl RenderOnce for ContentCover {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let gap = self.gap.get();
        let el = div()
            .absolute()
            .top_0()
            .bottom_0()
            .w(gap)
            .bg(self.color)
            .debug_selector(|| "app-shell-content-cover".into());
        if self.rtl {
            el.left(-gap)
        } else {
            el.right(-gap)
        }
    }
}
