//! Drawer and DrawerHeader, upstream `Drawer/`: a full-height side panel
//! that floats over the page, for inspectors and detail views.
//!
//! - It slides in from the `end` edge (right in LTR) or the `start` edge over
//!   `--duration-medium`, and back out when closed, keeping its content
//!   drawn through the exit.
//! - With a scrim it is modal, like a Dialog: a dim backdrop, a focus region
//!   with focus entry and return, Escape and scrim presses closing per
//!   `purpose`. Without one it is not: the page stays interactive and only
//!   Escape closes it, the last-opened drawer first.
//! - 400px wide by default; at 640px or narrower it leaves 56px of the page
//!   showing (or covers it all, with `full_width_on_mobile`). Its content
//!   scrolls, padded by `spacing-4` unless `padding` says otherwise.

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{LogicalStyled, use_direction};
use astryx_primitives::{prefers_reduced_motion, provide};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, IntoElement, Length, MouseButton, ParentElement, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Window, div, prelude::*, px,
};
use smallvec::SmallVec;

use super::dialog::{DialogHeader, DialogPurpose};
use super::modal::{Phase, backdrop, faded, modal_layer, modal_state, phase, probes, sync_open};
use crate::layout::SpacingStep;

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// Upstream `side`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DrawerSide {
    Start,
    /// Right in left-to-right text: the inspector convention.
    #[default]
    End,
}

/// A side panel over the page, upstream `Drawer`.
///
/// ```ignore
/// Drawer::new("inspector", open, "Order 1042")
///     .on_open_change(|open, window, cx| ...)
///     .child(
///         Layout::new()
///             .header(DrawerHeader::new("Order 1042").on_open_change(...))
///             .content(LayoutContent::new("body").child(...)),
///     )
///     .child(details)
/// ```
#[derive(IntoElement)]
pub struct Drawer {
    id: ElementId,
    open: bool,
    label: SharedString,
    on_open_change: Option<OpenFn>,
    children: SmallVec<[AnyElement; 2]>,
    side: DrawerSide,
    width: Option<Length>,
    full_width_on_mobile: bool,
    scrim: bool,
    purpose: DialogPurpose,
    padding: Option<SpacingStep>,
    style: StyleRefinement,
}

impl Drawer {
    /// Upstream `isOpen` and `label`, the drawer's accessible name.
    pub fn new(id: impl Into<ElementId>, open: bool, label: impl Into<SharedString>) -> Self {
        Drawer {
            id: id.into(),
            open,
            label: label.into(),
            on_open_change: None,
            children: SmallVec::new(),
            side: DrawerSide::End,
            width: None,
            full_width_on_mobile: false,
            scrim: true,
            purpose: DialogPurpose::Info,
            padding: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `side`.
        side: DrawerSide,
        /// Upstream `width`: 400px on wide screens.
        ?width: Length,
        /// Upstream `experimental.isFullWidthOnMobile`.
        !full_width_on_mobile,
        /// Upstream `hasScrim`: modal with a scrim, else the page stays
        /// interactive.
        scrim: bool,
        /// Upstream `purpose`.
        purpose: DialogPurpose,
        /// Upstream `padding`: `spacing-4` by default.
        ?padding: SpacingStep,
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }
}

impl ParentElement for Drawer {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Drawer {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Drawer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let state = modal_state(&self.id, window, cx);
        sync_open(&state, self.open, window, cx);
        let duration = if prefers_reduced_motion(cx) {
            Duration::from_millis(10)
        } else {
            t.duration.medium
        };
        let shown = match phase(&state, duration, duration, window, cx) {
            Phase::Hidden => {
                return div()
                    .id(self.id.clone())
                    .display_contents()
                    .into_any_element();
            }
            Phase::Open { progress } => t.ease.standard.ease(progress),
            // Its content stays drawn through the exit.
            Phase::Closing { progress } => 1. - t.ease.standard.ease(progress),
        };

        let viewport = window.viewport_size();
        let width = self.width.unwrap_or(px(400.).into());
        let mobile = viewport.width <= px(640.);
        let padding = self
            .padding
            .map_or(t.spacing.s4.to_pixels(rem), |s| s.token(t).to_pixels(rem));
        let report = self.on_open_change.clone();
        let close = move |window: &mut Window, cx: &mut App| {
            if let Some(f) = &report {
                f(false, window, cx);
            }
        };

        let surface_focus = state.read(cx).surface.clone();
        let mut surface = div()
            .id(ElementId::Name(format!("{}-surface", self.id).into()))
            .track_focus(&surface_focus)
            .occlude()
            .absolute()
            .top_0()
            .h(viewport.height)
            .flex()
            .flex_col()
            .bg(t.color.background_surface)
            .text_color(t.color.text_primary)
            .shadow_token(&t.shadow.high)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .role(Role::Dialog)
            .aria_label(self.label.clone())
            .aria_modal(self.scrim)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        surface = match (mobile, self.full_width_on_mobile) {
            (true, true) => surface.w(viewport.width),
            (true, false) => surface.w(width).max_w(viewport.width - px(56.)),
            (false, _) => surface.w(width),
        };
        if self.scrim {
            surface = surface.tab_trap();
        }
        // Slides from its edge; the page's edge border faces the page.
        let travel = 1. - shown;
        let at_end = self.side == DrawerSide::End;
        surface = if at_end {
            surface
                .inset_end(dir, px(0.))
                .border_start(dir, px(1.))
                .border_color(t.color.border)
        } else {
            surface
                .inset_start(dir, px(0.))
                .border_end(dir, px(1.))
                .border_color(t.color.border)
        };
        let toward_end = at_end != dir.is_rtl();
        let offset = gpui::relative(if toward_end { travel } else { -travel });
        surface = surface.translate(offset, px(0.));
        theme.refine(surface.style(), "drawer", &[], None);
        surface.style().refine(&self.style);
        let scroller_handle = crate::gestures::scroller::use_scroll_handle(
            ElementId::Name(format!("{}-content", self.id).into()),
            window,
            cx,
        );
        let content = div()
            .id(ElementId::Name(format!("{}-content", self.id).into()))
            .flex_grow(1.)
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .track_scroll(&scroller_handle)
            .p(padding)
            .child(provide(
                state.read(cx).focus.clone(),
                crate::layout::padding_container(
                    crate::layout::ContainerPadding {
                        inline_start: padding,
                        inline_end: padding,
                        block_start: padding,
                        block_end: padding,
                    },
                    None,
                    crate::layout::host_children(
                        self.children,
                        crate::layout::ContainerPadding {
                            inline_start: padding,
                            inline_end: padding,
                            block_start: padding,
                            block_end: padding,
                        },
                        crate::layout::ContainerCorners::default(),
                    ),
                    cx,
                ),
            ));
        let surface = surface
            .children(probes(&state, cx))
            .child(crate::gestures::scroller::scroll_begin(
                ElementId::Name(format!("{}-content", self.id).into()),
                Some(&scroller_handle),
                (false, true),
            ))
            .child(content)
            .child(crate::gestures::scroller::scroll_end());

        let info = self.purpose == DialogPurpose::Info;
        let press: super::modal::CloseFn = Rc::new(close.clone());
        let holder = if self.scrim {
            backdrop(
                &self.id,
                Some(faded(t.color.overlay, shown)),
                info.then_some(press),
                window,
            )
            .child(surface)
            .into_any_element()
        } else {
            // No scrim: a positioner that takes no presses of its own, so the
            // page under it stays interactive.
            let viewport = window.viewport_size();
            div()
                .absolute()
                .top_0()
                .left_0()
                .w(viewport.width)
                .h(viewport.height)
                .child(surface)
                .into_any_element()
        };
        modal_layer(&self.id, holder, self.scrim, self.purpose, close)
    }
}

/// A drawer's header, upstream `DrawerHeader`: DialogHeader's layout, with a
/// title that does not name the drawer (its `label` does) and no divider
/// unless asked.
#[derive(IntoElement)]
pub struct DrawerHeader(DialogHeader);

impl DrawerHeader {
    /// Upstream `title`.
    pub fn new(title: impl Into<SharedString>) -> Self {
        DrawerHeader(DialogHeader::new(title).plain())
    }

    /// Upstream `subtitle`.
    pub fn subtitle(self, subtitle: impl Into<SharedString>) -> Self {
        DrawerHeader(self.0.subtitle(subtitle))
    }

    /// Upstream `hasDivider`.
    pub fn divider(self, divider: bool) -> Self {
        DrawerHeader(self.0.divider(divider))
    }

    /// Upstream `onOpenChange`: a close button.
    pub fn on_open_change(self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        DrawerHeader(self.0.on_open_change(f))
    }

    /// Upstream `startContent`.
    pub fn start_content(self, content: impl IntoElement) -> Self {
        DrawerHeader(self.0.start_content(content))
    }

    /// Upstream `endContent`.
    pub fn end_content(self, content: impl IntoElement) -> Self {
        DrawerHeader(self.0.end_content(content))
    }
}

impl RenderOnce for DrawerHeader {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.0
    }
}
