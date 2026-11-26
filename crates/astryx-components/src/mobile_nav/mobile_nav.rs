//! MobileNav, upstream `MobileNav/MobileNav.tsx`: the phone's navigation drawer. A modal
//! layer whose drawer slides in from an inline edge over a dimmed backdrop (FR1, FR7,
//! FR13), with a header and a close button above a scrolling navigation landmark (FR14).
//! It is controlled, or reads the enclosing AppShell's drawer state (FR2). Escape, a
//! press on the backdrop and the close button close it (FR4); the dialog closes within
//! 250 ms while the drawer slides out, inert (FR5), and focus returns to the opener
//! (FR6).

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{prefers_reduced_motion, use_context};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, IntoElement, MouseButton, ParentElement, Pixels, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Window, anchored, deferred, div, point, prelude::*, px,
};

use crate::app_shell::{AppShellMobile, OpenFn};
use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconColor};
use crate::overlay::DialogPurpose;
use crate::overlay::modal::{
    CloseFn, Phase, backdrop, faded, modal_layer, modal_state, phase, probes, sync_open,
};
use crate::text::{Heading, HeadingLevel};

/// FR5: the dialog closes no later than this after a close request.
const CLOSE_BY: Duration = Duration::from_millis(250);

/// Upstream `side`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MobileNavSide {
    /// The inline start edge (left in left-to-right text).
    Start,
    /// The inline end edge.
    End,
    /// The side of the window its opener is in (FR8).
    #[default]
    Auto,
}

/// Upstream `MobileNav`.
#[derive(IntoElement)]
pub struct MobileNav {
    id: ElementId,
    open: Option<bool>,
    on_open_change: Option<OpenFn>,
    header: Option<AnyElement>,
    header_text: Option<SharedString>,
    width: Pixels,
    side: MobileNavSide,
    label: Option<SharedString>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl MobileNav {
    pub fn new(id: impl Into<ElementId>) -> Self {
        MobileNav {
            id: id.into(),
            open: None,
            on_open_change: None,
            header: None,
            header_text: None,
            width: px(320.),
            side: MobileNavSide::Auto,
            label: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `isOpen`: given, it wins over AppShell's state.
    pub fn open(mut self, open: bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Upstream `onOpenChange`: false when it should close.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `header` as an element (a logo, a heading, a search field).
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// Upstream `header` as a string: a level-2 heading that also names the drawer.
    pub fn header_text(mut self, header: impl Into<SharedString>) -> Self {
        self.header_text = Some(header.into());
        self
    }

    /// Upstream `width`: default 320, never over 85% of the window (FR9).
    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    /// Upstream `side`. Default auto.
    pub fn side(mut self, side: MobileNavSide) -> Self {
        self.side = side;
        self
    }

    /// Upstream `label`: else the header text, else "Navigation" (FR10).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for MobileNav {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for MobileNav {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MobileNav {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let dir = use_direction(cx);
        let shell = use_context::<AppShellMobile>(cx);
        // FR2: the props win; else AppShell's drawer state.
        let open = self
            .open
            .or(shell.as_ref().map(|s| s.is_open))
            .unwrap_or(false);
        let report: OpenFn = self
            .on_open_change
            .clone()
            .or_else(|| shell.as_ref().map(|s| s.set_open.clone()))
            .unwrap_or_else(|| Rc::new(|_, _, _| {}));
        let state = modal_state(&self.id, window, cx);
        let resolved = window.use_keyed_state(
            ElementId::Name(format!("{}-side", self.id).into()),
            cx,
            |_, _| MobileNavSide::End,
        );
        // FR8: auto resolves on each open from the opener's half of the window: the
        // pointer that opened it, else the toggle; else it keeps its last side.
        if open && !state.read(cx).open && self.side == MobileNavSide::Auto {
            let opener = if window.last_input_was_keyboard() {
                shell
                    .as_ref()
                    .and_then(|s| s.opener.get())
                    .map(|b| b.center().x)
            } else {
                Some(window.mouse_position().x)
            };
            if let Some(x) = opener {
                let half = window.viewport_size().width / 2.;
                let start_half = if dir.is_rtl() { x > half } else { x < half };
                resolved.update(cx, |s, _| {
                    *s = if start_half {
                        MobileNavSide::Start
                    } else {
                        MobileNavSide::End
                    }
                });
            }
        }
        let side = match self.side {
            MobileNavSide::Auto => *resolved.read(cx),
            side => side,
        };
        sync_open(&state, open, window, cx);
        let duration = if prefers_reduced_motion(cx) {
            Duration::from_millis(10)
        } else {
            t.duration.medium
        };
        let (shown, inert) = match phase(&state, duration, duration, window, cx) {
            Phase::Hidden => {
                return div()
                    .id(self.id.clone())
                    .display_contents()
                    .into_any_element();
            }
            Phase::Open { progress } => (t.ease.standard.ease(progress), false),
            // FR5: the slide-out outlives the dialog, which closes by 250 ms.
            Phase::Closing { progress } => (
                1. - t.ease.standard.ease(progress),
                duration.mul_f32(progress) >= CLOSE_BY.min(duration),
            ),
        };

        let i18n = i18n(cx);
        let label = self
            .label
            .clone()
            .or_else(|| self.header_text.clone())
            .unwrap_or_else(|| i18n.t("@astryx.mobileNav.navigation"));
        let side_name = if side == MobileNavSide::Start {
            "start"
        } else {
            "end"
        };
        let viewport = window.viewport_size();
        let width = self.width.min(viewport.width * 0.85);
        let at_start = side == MobileNavSide::Start;
        let surface_focus = state.read(cx).surface.clone();
        let mut drawer = div()
            .id(ElementId::Name(format!("{}-drawer", self.id).into()))
            .debug_selector({
                let s = format!("{}-drawer", self.id);
                move || s
            })
            .track_focus(&surface_focus)
            .absolute()
            .top_0()
            .h(viewport.height)
            .w(width)
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(t.color.background_surface)
            .text_color(t.color.text_primary)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .role(Role::Dialog)
            .aria_label(label.clone())
            .aria_modal(true);
        if !inert {
            drawer = drawer
                .occlude()
                .tab_trap()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        }
        drawer = if at_start {
            drawer
                .inset_start(dir, px(0.))
                .border_end(dir, px(1.))
                .border_color(t.color.border)
        } else {
            drawer
                .inset_end(dir, px(0.))
                .border_start(dir, px(1.))
                .border_color(t.color.border)
        };
        // FR7: it enters from its edge, from off-screen.
        let travel = 1. - shown;
        let toward_end = !at_start != dir.is_rtl();
        drawer = drawer.translate(
            gpui::relative(if toward_end { travel } else { -travel }),
            px(0.),
        );
        theme.refine(
            drawer.style(),
            "mobile-nav-drawer",
            &[("side", side_name)],
            None,
        );

        let has_header = self.header.is_some() || self.header_text.is_some();
        let close_button = {
            let report = report.clone();
            div()
                .debug_selector({
                    let s = format!("{}-close", self.id);
                    move || s
                })
                .child(
                    IconButton::new(
                        ElementId::Name(format!("{}-close", self.id).into()),
                        i18n.t("@astryx.mobileNav.closeNavigation"),
                        Icon::new("close").color(IconColor::Inherit),
                    )
                    .variant(ButtonVariant::Ghost)
                    .on_click(move |_, window, cx| report(false, window, cx)),
                )
        };
        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .when(has_header, |el| el.justify_between())
            .when(!has_header, |el| el.justify_end())
            .min_h(t.spacing.s12)
            .px(t.spacing.s2)
            .border_b_1()
            .border_color(t.color.border)
            .children(self.header_text.clone().map(|h| {
                div()
                    .ml(t.spacing.s1)
                    .child(Heading::new(HeadingLevel::H2, h))
            }))
            .children(self.header)
            .child(close_button);
        // FR14: the content is the drawer's navigation landmark, named like the dialog.
        let content = div()
            .id(ElementId::Name(format!("{}-content", self.id).into()))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .overscroll_contain()
            .px(t.spacing.s2)
            .py(t.spacing.s2)
            .role(Role::Navigation)
            .aria_label(label)
            .children(self.children);
        let drawer = drawer
            .children(if inert {
                Vec::new()
            } else {
                probes(&state, cx)
            })
            .child(header)
            .child(content);
        // FR13: the theme's overlay dim, unblurred.
        let dim = faded(t.color.overlay, shown);
        if inert {
            // FR5: the dialog has closed; the rest of the slide-out takes no input.
            return div()
                .id(self.id.clone())
                .child(
                    deferred(
                        anchored().position(point(px(0.), px(0.))).child(
                            div()
                                .w(viewport.width)
                                .h(viewport.height)
                                .bg(dim)
                                .child(drawer),
                        ),
                    )
                    .with_priority(1),
                )
                .into_any_element();
        }
        let close: CloseFn = Rc::new(move |window, cx| report(false, window, cx));
        let mut holder = backdrop(&self.id, Some(dim), Some(close.clone()), window).child(drawer);
        theme.refine(holder.style(), "mobile-nav", &[("side", side_name)], None);
        holder.style().refine(&self.style);
        modal_layer(
            &self.id,
            holder,
            true,
            DialogPurpose::Info,
            move |window, cx| close(window, cx),
        )
    }
}
