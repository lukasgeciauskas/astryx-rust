//! Popover, upstream `Popover/`: interactive content in a layer anchored to
//! the button that opens it (the button + dialog pattern).
//!
//! - The trigger is a Button (or anything built on it): it reports the
//!   popover's state (`aria-expanded`, `aria-haspopup`) and toggles it.
//! - The surface is a focus region (fork patch R12): Tab and Shift-Tab cycle
//!   inside it, modal or not (DEC-9). On opening, focus enters the first
//!   control in it, else the labeled surface (FR2); on closing, focus that
//!   would be lost returns to where it was (AR4).
//! - A press outside and Escape close it, each unless turned off; a press on
//!   the trigger toggles it instead (same-gesture reopen protection).
//! - The surface is at least as wide as the trigger, at most the viewport
//!   less its gutters, and scrolls when its content is taller (FR4).
//! - A hidden close button ends the focus region and shows when Tab reaches
//!   it; it is never where focus enters.

use std::rc::Rc;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{
    EscapeBehavior, LayerAlign, LayerOptions, Placement, anchored_layer, control_focus, provide,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, Entity, FocusHandle, HasPopup, IntoElement, Length, ParentElement,
    RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, div, prelude::*,
};
use web_time::Instant;

use super::disclosure::Disclosure;
use super::motion::{clearance, entrance, gutter, viewport_cap};
use crate::button::{Button, ButtonVariant};
use crate::layout::SpacingStep;

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type ContentFn = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;

/// Upstream `role`: what the surface is to assistive technology.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopoverRole {
    /// A dialog, named by `label`.
    #[default]
    Dialog,
    /// No role of its own: the content brings one (a menu, a listbox).
    None,
}

/// Upstream `presentation`: the surface that holds the content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopoverPresentation {
    /// Anchored to the trigger.
    #[default]
    Popover,
    /// A modal BottomSheet named by the popover's label.
    BottomSheet,
    /// A BottomSheet on a compact touch screen, else anchored.
    Adaptive,
}

/// A compact touch screen: where the adaptive presentations use a sheet,
/// and menus drill into submenus.
pub(crate) fn compact_touch(window: &Window) -> bool {
    window.viewport_size().width <= gpui::px(640.) && window.last_input_was_touch()
}

/// Whether `presentation` shows as a BottomSheet now.
pub(crate) fn presents_as_sheet(presentation: PopoverPresentation, window: &Window) -> bool {
    match presentation {
        PopoverPresentation::Popover => false,
        PopoverPresentation::BottomSheet => true,
        PopoverPresentation::Adaptive => compact_touch(window),
    }
}

/// A popover's state between frames.
pub(crate) struct PopoverState {
    pub(crate) open: bool,
    /// What had focus when it opened, to return focus to.
    return_to: Option<FocusHandle>,
    /// The surface: the focus region, and the fallback for initial focus.
    pub(crate) surface: FocusHandle,
    shown_at: Option<Instant>,
    side: Placement,
    controlled: bool,
    auto_focus: bool,
    /// Focus still has to enter: once the surface has been drawn.
    pending_focus: bool,
    on_open_change: Option<OpenFn>,
}

impl PopoverState {
    fn new(cx: &mut App) -> Self {
        PopoverState {
            open: false,
            return_to: None,
            surface: cx.focus_handle().tab_stop(false),
            shown_at: None,
            side: Placement::Below,
            controlled: false,
            auto_focus: true,
            pending_focus: false,
            on_open_change: None,
        }
    }
}

/// Asks to open or close: an uncontrolled popover does, then reports it; a
/// controlled one only reports it.
pub(crate) fn request(state: &Entity<PopoverState>, open: bool, window: &mut Window, cx: &mut App) {
    if state.read(cx).open == open {
        return;
    }
    let (controlled, report) = {
        let s = state.read(cx);
        (s.controlled, s.on_open_change.clone())
    };
    if !controlled {
        transition(state, open, window, cx);
    }
    if let Some(report) = report {
        report(open, window, cx);
    }
}

/// Opens or closes, with the focus moves that go with it.
fn transition(state: &Entity<PopoverState>, open: bool, window: &mut Window, cx: &mut App) {
    if open {
        let focused = window.focused(cx);
        state.update(cx, |s, _| {
            s.open = true;
            s.shown_at = None;
            s.return_to = focused;
            s.pending_focus = s.auto_focus;
        });
    } else {
        let (surface, return_to) = state.update(cx, |s, _| {
            s.open = false;
            s.pending_focus = false;
            (s.surface.clone(), s.return_to.take())
        });
        // Focus that would be lost goes back where it was (AR4).
        let lost = window.focused(cx).is_none() || surface.contains_focused(window, cx);
        if lost && let Some(target) = return_to {
            target.focus(window, cx);
        }
    }
    window.refresh();
}

/// Moves focus into the surface: its first control, else the surface itself
/// (FR2). Runs right after the frame that first drew the surface, when its
/// tab stops exist.
fn enter_focus(state: &Entity<PopoverState>, window: &mut Window, cx: &mut App) {
    let Some(surface) = state.update(cx, |s, _| {
        std::mem::take(&mut s.pending_focus).then(|| s.surface.clone())
    }) else {
        return;
    };
    surface.focus(window, cx);
    window.focus_next(cx);
    let fallback = window
        .focused(cx)
        .is_none_or(|f| f == surface || is_close_button(&f, cx));
    if fallback {
        surface.focus(window, cx);
    }
}

/// The hidden close buttons of open popovers, so initial focus skips them.
#[derive(Default)]
struct CloseButtons(Vec<gpui::WeakFocusHandle>);

impl gpui::Global for CloseButtons {}

fn is_close_button(focus: &FocusHandle, cx: &App) -> bool {
    cx.try_global::<CloseButtons>().is_some_and(|buttons| {
        buttons
            .0
            .iter()
            .any(|weak| weak.upgrade().is_some_and(|h| &h == focus))
    })
}

fn register_close_button(focus: &FocusHandle, cx: &mut App) {
    let buttons = cx.default_global::<CloseButtons>();
    buttons.0.retain(|weak| weak.upgrade().is_some());
    if !buttons
        .0
        .iter()
        .any(|weak| weak.upgrade().is_some_and(|h| &h == focus))
    {
        buttons.0.push(focus.downgrade());
    }
}

/// Opens and closes a [`Popover`] from code, upstream `usePopover`'s
/// `show`, `hide` and `toggle`.
#[derive(Clone)]
pub struct PopoverController {
    state: Entity<PopoverState>,
}

impl PopoverController {
    /// The controller of the popover with element id `id`.
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id: ElementId = id.into();
        PopoverController {
            state: popover_state(&id, window, cx),
        }
    }

    pub fn show(&self, window: &mut Window, cx: &mut App) {
        request(&self.state, true, window, cx);
    }

    pub fn hide(&self, window: &mut Window, cx: &mut App) {
        request(&self.state, false, window, cx);
    }

    pub fn toggle(&self, window: &mut Window, cx: &mut App) {
        let open = self.state.read(cx).open;
        request(&self.state, !open, window, cx);
    }

    pub fn is_open(&self, cx: &App) -> bool {
        self.state.read(cx).open
    }
}

fn popover_state(id: &ElementId, window: &mut Window, cx: &mut App) -> Entity<PopoverState> {
    window.use_keyed_state(
        ElementId::Name(format!("{id}-popover").into()),
        cx,
        |_, cx| PopoverState::new(cx),
    )
}

/// Content anchored to the button that opens it, upstream `Popover`.
///
/// ```ignore
/// Popover::new("filters", "Filters")
///     .content(Stack::vertical().child(Switch::new("archived", "Show archived", false)))
///     .child(Button::new("filters-button", "Filters"))
/// ```
#[derive(IntoElement)]
pub struct Popover {
    id: ElementId,
    label: Option<SharedString>,
    trigger: Option<AnyElement>,
    content: Option<ContentFn>,
    placement: Placement,
    alignment: LayerAlign,
    open: Option<bool>,
    on_open_change: Option<OpenFn>,
    enabled: bool,
    width: Option<Length>,
    padding: SpacingStep,
    role: PopoverRole,
    modal: bool,
    close_button: bool,
    close_button_label: Option<SharedString>,
    auto_focus: bool,
    light_dismiss: bool,
    escape_dismiss: bool,
    presentation: PopoverPresentation,
    style: StyleRefinement,
    /// The owner's handle on the popover's state (a menu drives it).
    controller: Option<PopoverController>,
}

impl Popover {
    /// Upstream `label`: the dialog's accessible name.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        let label: SharedString = label.into();
        Popover {
            id: id.into(),
            label: (!label.is_empty()).then_some(label),
            trigger: None,
            content: None,
            placement: Placement::Below,
            alignment: LayerAlign::Start,
            open: None,
            on_open_change: None,
            enabled: true,
            width: None,
            padding: SpacingStep::S3,
            role: PopoverRole::Dialog,
            modal: true,
            close_button: true,
            close_button_label: None,
            auto_focus: true,
            light_dismiss: true,
            escape_dismiss: true,
            presentation: PopoverPresentation::Popover,
            style: StyleRefinement::default(),
            controller: None,
        }
    }

    astryx_primitives::setters! {
        /// Upstream `placement`: below by default; start and end follow the
        /// reading direction.
        placement: Placement,
        /// Upstream `alignment`: start by default.
        alignment: LayerAlign,
        /// Upstream `isEnabled`: off, the trigger does nothing.
        enabled: bool,
        /// Upstream `width`: else at least the trigger's width.
        ?width: Length,
        /// Upstream `padding`: a spacing step inside the surface; 3 by default.
        padding: SpacingStep,
        /// Upstream `role`.
        role: PopoverRole,
        /// Upstream `isModal`: a dialog popover sets `aria-modal`.
        modal: bool,
        /// Upstream `hasCloseButton`: a hidden close button that shows when
        /// Tab reaches it.
        close_button: bool,
        /// Upstream `closeButtonLabel`.
        ?close_button_label: SharedString,
        /// Upstream `hasAutoFocus`: focus enters the popover when it opens.
        auto_focus: bool,
        /// Upstream `hasLightDismiss`: a press outside closes it.
        light_dismiss: bool,
        /// Upstream `hasEscapeDismiss`: Escape closes it.
        escape_dismiss: bool,
        /// Upstream `presentation`.
        presentation: PopoverPresentation,
    }

    /// Shares `controller`'s state, so the owner that made it drives this
    /// popover (a component that renders a Popover reads a different scope).
    pub(crate) fn controller(mut self, controller: PopoverController) -> Self {
        self.controller = Some(controller);
        self
    }

    /// Upstream `children`: the trigger, a Button or anything built on it.
    pub fn child(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    /// Upstream `content`: shown while the popover is open.
    pub fn content(mut self, content: impl IntoElement + 'static) -> Self {
        self.content = Some(Box::new(move |_, _| content.into_any_element()));
        self
    }

    /// Upstream `content`, built by `f` only while the popover is open.
    pub fn content_fn(
        mut self,
        f: impl FnOnce(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.content = Some(Box::new(f));
        self
    }

    /// Upstream `isOpen`: a controlled popover shows while `open` is true.
    pub fn open(mut self, open: impl Into<Option<bool>>) -> Self {
        self.open = open.into();
        self
    }

    /// Upstream `onOpenChange`: asked to open (a trigger press) or to close
    /// (a press outside, Escape, the close button).
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }
}

impl Styled for Popover {
    /// Upstream `xstyle`: the painted surface.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Popover {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rtl = use_direction(cx).is_rtl();
        let state = match self.controller.take() {
            Some(controller) => controller.state,
            None => popover_state(&self.id, window, cx),
        };
        state.update(cx, |s, _| {
            s.controlled = self.open.is_some();
            s.auto_focus = self.auto_focus;
            s.on_open_change = self.on_open_change.clone();
        });
        if let Some(open) = self.open
            && state.read(cx).open != open
        {
            transition(&state, open, window, cx);
        }
        let open = state.read(cx).open;

        let toggle = self.enabled.then(|| {
            let state = state.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                let open = state.read(cx).open;
                request(&state, !open, window, cx);
            }) as Rc<dyn Fn(&mut Window, &mut App)>
        });
        let trigger = provide(
            Disclosure::new(
                open,
                match self.role {
                    PopoverRole::Dialog => HasPopup::Dialog,
                    PopoverRole::None => HasPopup::Menu,
                },
                toggle,
            ),
            self.trigger.unwrap_or_else(|| div().into_any_element()),
        );
        // The sheet presentations: the trigger stays where it is, and a
        // BottomSheet holds the content.
        let sheet = presents_as_sheet(self.presentation, window);
        if sheet {
            let content = self.content;
            let sheet_state = state.clone();
            let mut bottom = super::sheet::BottomSheet::new(
                ElementId::Name(format!("{}-sheet", self.id).into()),
                open,
                self.label.clone().unwrap_or_default(),
            )
            .on_open_change(move |open, window, cx| request(&sheet_state, open, window, cx));
            if let Some(build) = content {
                bottom = bottom.child(build(window, cx));
            }
            return div()
                .display_contents()
                .child(anchored_layer(
                    ElementId::Name(format!("{}-layer", self.id).into()),
                    trigger,
                ))
                .child(bottom)
                .into_any_element();
        }
        let mut layer = anchored_layer(
            ElementId::Name(format!("{}-layer", self.id).into()),
            trigger,
        )
        .light_dismiss(self.light_dismiss)
        .escape(if self.escape_dismiss {
            EscapeBehavior::Close
        } else {
            EscapeBehavior::Block
        });
        if !open {
            state.update(cx, |s, _| s.shown_at = None);
            return layer.into_any_element();
        }

        let now = cx.background_executor().now();
        let since = state.update(cx, |s, _| *s.shown_at.get_or_insert(now));
        let (surface_focus, side) = {
            let s = state.read(cx);
            (s.surface.clone(), s.side)
        };
        let cap = viewport_cap(t, window);
        let content = self
            .content
            .map(|build| build(window, cx))
            .unwrap_or_else(|| div().into_any_element());

        // The hidden close button: the end of the focus region, shown while
        // focused (upstream's fallback close control).
        let close = self.close_button.then(|| {
            let close_id = ElementId::Name(format!("{}-close", self.id).into());
            let close_focus = control_focus(&close_id, window, cx);
            register_close_button(&close_focus, cx);
            let focused = close_focus.is_focused(window);
            let label = self
                .close_button_label
                .clone()
                .unwrap_or_else(|| i18n(cx).t("@astryx.popover.close"));
            let state = state.clone();
            let button = Button::new(close_id, label)
                .variant(ButtonVariant::Secondary)
                .on_click(move |_, window, cx| request(&state, false, window, cx));
            let wrap = div()
                .absolute()
                .bottom_0()
                .left_0()
                .right_0()
                .flex()
                .justify_center();
            if focused {
                wrap.pt(t.spacing.s1).child(button)
            } else {
                wrap.h(gpui::px(1.))
                    .overflow_hidden()
                    .opacity(0.)
                    .child(button)
            }
        });

        let mut surface = div()
            .id(ElementId::Name(format!("{}-surface", self.id).into()))
            .track_focus(&surface_focus)
            .tab_trap()
            .occlude()
            .relative()
            .min_w_0()
            .max_w(cap.width)
            .max_h(cap.height)
            .overflow_y_scroll()
            .bg(t.color.background_popover)
            .rounded(t.radius.container)
            .shadow_token(&t.shadow.low)
            .p(self.padding.token(t))
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .text_color(t.color.text_primary)
            .child(content)
            .children(close)
            .when(state.read(cx).pending_focus, |el| {
                let state = state.clone();
                el.child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |_, _, window, cx| {
                            let state = state.clone();
                            window.defer(cx, move |window, cx| enter_focus(&state, window, cx));
                        },
                    )
                    .absolute()
                    .size_0(),
                )
            });
        if let Some(width) = self.width {
            surface = surface.w(width);
        }
        if self.role == PopoverRole::Dialog {
            surface = surface.role(Role::Dialog).aria_modal(self.modal);
            if let Some(label) = &self.label {
                surface = surface.aria_label(label.clone());
            }
        }
        theme.refine(surface.style(), "popover", &[], None);
        surface.style().refine(&self.style);
        let surface = entrance(surface, since, side, rtl, t, window, cx);

        let options = LayerOptions {
            placement: self.placement,
            align: self.alignment,
            offset: clearance(t, window),
            margin: gutter(t, window),
            rtl,
            priority: 1,
        };
        let (dismiss, placed) = (state.clone(), state.clone());
        layer = layer
            .match_anchor_width(self.width.is_none())
            .surface(surface, options, move |window, cx| {
                request(&dismiss, false, window, cx)
            })
            .on_placed(move |position, _, cx| {
                placed.update(cx, |s, _| s.side = position.side);
            });
        layer.into_any_element()
    }
}
