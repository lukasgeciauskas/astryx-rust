//! Tooltip. Upstream: `Tooltip/` (`Tooltip.tsx`, `useTooltip.tsx`).
//!
//! Short, noninteractive text on hover and keyboard focus, drawn as an
//! anchored layer (`astryx_primitives::anchored_layer`) above everything. A
//! pointer opens it after `delay` and leaving closes it after `hide_delay`
//! (with a short bridge so the pointer can reach the surface); keyboard
//! focus opens it at once; Escape, a press outside, and a press on the
//! trigger close it. A [`TooltipController`] (upstream `useTooltip`'s `show`
//! and `hide`) pins it open, as InfoTip does.

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::use_direction;
use astryx_primitives::{
    LayerAlign, LayerOptions, Placement, anchored_layer, provide, use_context,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, Context, ElementId, Entity, FocusHandle, IntoElement, MouseButton,
    ParentElement, RenderOnce, Role, SharedString, StyleRefinement, Styled, Task, Window, div,
    prelude::*, px,
};
use web_time::Instant;

/// Upstream `HOVER_BRIDGE_DELAY`: with no hide delay, the pointer still gets
/// this long to cross from the trigger onto the surface.
const HOVER_BRIDGE: Duration = Duration::from_millis(100);
/// Upstream's surface width cap.
const MAX_WIDTH: f32 = 300.;

/// Upstream `focusTrigger`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusTrigger {
    /// Keyboard focus opens it, as `:focus-visible` does; so does any focus
    /// on a text-entry trigger.
    #[default]
    Auto,
    /// Any focus opens it (a disabled control's reason).
    Always,
    /// Focus never opens it.
    Never,
}

/// Upstream `touchTrigger`: what a tap does where there is no hover.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TouchTrigger {
    /// A tap opens it, unless the trigger performs an action of its own.
    #[default]
    Auto,
    /// Every tap opens it.
    Tap,
    /// No tap does.
    None,
}

/// Tells pressables inside a tooltip's trigger what describes them, so they
/// expose it as their accessible description (upstream `aria-describedby`).
#[derive(Clone)]
pub(crate) struct TooltipDescribes(pub SharedString);

/// The description an enclosing tooltip gives its trigger.
pub(crate) fn tooltip_description(cx: &App) -> Option<SharedString> {
    use_context::<TooltipDescribes>(cx).map(|d| d.0)
}

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// A tooltip's state between frames, shared with a [`TooltipController`].
pub struct TooltipState {
    /// Visible, when uncontrolled.
    shown: bool,
    /// Held open by `show()` until `hide()`, Escape, a press outside, or
    /// focus leaving the trigger.
    pinned: bool,
    over_trigger: bool,
    over_surface: bool,
    /// Focus that opens it was in the trigger last frame.
    focused: bool,
    /// Any focus was in the trigger last frame.
    within: bool,
    /// A press on the trigger closed it; hover reopens it only after the
    /// pointer leaves.
    suppressed: bool,
    timer: Option<Task<()>>,
    /// When it last appeared, for its entrance.
    shown_at: Option<Instant>,
    side: Placement,
    /// Focus inside the trigger.
    group: FocusHandle,
    /// A text trigger's own tab stop.
    text: FocusHandle,
    /// A hover card's surface: focus inside it holds the card open.
    surface: FocusHandle,
    /// Whether the trigger is a text span (else an element), for focus return.
    text_trigger: bool,
    /// Visible last frame.
    was_visible: bool,
    // Refreshed every frame from the component.
    controlled: Option<bool>,
    enabled: bool,
    on_open_change: Option<OpenFn>,
}

impl TooltipState {
    fn new(default_open: bool, cx: &mut Context<Self>) -> Self {
        TooltipState {
            shown: default_open,
            pinned: false,
            over_trigger: false,
            over_surface: false,
            focused: false,
            within: false,
            suppressed: false,
            timer: None,
            shown_at: None,
            side: Placement::Above,
            group: cx.focus_handle().tab_stop(false),
            text: cx.focus_handle().tab_stop(true),
            surface: cx.focus_handle().tab_stop(false),
            text_trigger: false,
            was_visible: false,
            controlled: None,
            enabled: true,
            on_open_change: None,
        }
    }

    fn visible(&self) -> bool {
        self.enabled && self.controlled.unwrap_or(self.shown)
    }
}

/// Opens the tooltip now (or, when controlled, asks to).
fn show(state: &Entity<TooltipState>, pin: bool, window: &mut Window, cx: &mut App) {
    let report = state.update(cx, |s, _| {
        s.timer = None;
        if !s.enabled {
            return None;
        }
        if pin {
            s.pinned = true;
        }
        if s.controlled.is_some() {
            return s.on_open_change.clone();
        }
        if s.shown {
            return None;
        }
        s.shown = true;
        s.shown_at = None;
        s.on_open_change.clone()
    });
    if let Some(f) = report {
        f(true, window, cx);
    }
    window.refresh();
}

/// Closes the tooltip now (or, when controlled, asks to).
fn hide(state: &Entity<TooltipState>, window: &mut Window, cx: &mut App) {
    let report = state.update(cx, |s, _| {
        s.timer = None;
        s.pinned = false;
        if s.controlled.is_some() {
            return s.on_open_change.clone();
        }
        if !s.shown {
            return None;
        }
        s.shown = false;
        s.on_open_change.clone()
    });
    if let Some(f) = report {
        f(false, window, cx);
    }
    window.refresh();
}

/// Gives focus back to the trigger: a text trigger's span, or the first tab
/// stop inside an element trigger.
fn return_focus(state: &Entity<TooltipState>, window: &mut Window, cx: &mut App) {
    let (text_trigger, text, group) = {
        let s = state.read(cx);
        (s.text_trigger, s.text.clone(), s.group.clone())
    };
    if text_trigger {
        text.focus(window, cx);
    } else {
        group.focus(window, cx);
        window.focus_next(cx);
    }
}

/// Opens or closes after `delay`, replacing any pending change. A close
/// waits while the pointer is over the trigger or surface, focus holds it,
/// or it is pinned.
fn schedule(
    state: &Entity<TooltipState>,
    open: bool,
    delay: Duration,
    window: &mut Window,
    cx: &mut App,
) {
    let timer = cx.background_executor().timer(delay);
    let weak = state.downgrade();
    let task = window.spawn(cx, async move |cx| {
        timer.await;
        cx.update(|window, cx| {
            let Some(state) = weak.upgrade() else { return };
            if open {
                show(&state, false, window, cx);
            } else {
                let held = {
                    let s = state.read(cx);
                    s.over_trigger || s.over_surface || s.focused || s.pinned
                };
                if !held {
                    hide(&state, window, cx);
                }
            }
        })
        .ok();
    });
    state.update(cx, |s, _| s.timer = Some(task));
}

/// Shows and hides a tooltip from code, upstream `useTooltip`'s `show()` and
/// `hide()`. `show()` pins it: it stays open until `hide()`, Escape, a press
/// outside, or focus leaving its trigger.
#[derive(Clone)]
pub struct TooltipController(Entity<TooltipState>);

impl TooltipController {
    /// A controller kept between frames under `id`; pass it to the tooltip
    /// with [`Tooltip::controller`].
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let key = ElementId::Name(format!("{}-tooltip-state", id.into()).into());
        TooltipController(window.use_keyed_state(key, cx, |_, cx| TooltipState::new(false, cx)))
    }

    pub fn show(&self, window: &mut Window, cx: &mut App) {
        show(&self.0, true, window, cx);
    }

    pub fn hide(&self, window: &mut Window, cx: &mut App) {
        hide(&self.0, window, cx);
    }

    /// Whether it is showing.
    pub fn is_shown(&self, cx: &App) -> bool {
        self.0.read(cx).visible()
    }

    /// Whether `show()` holds it open.
    pub fn is_pinned(&self, cx: &App) -> bool {
        self.0.read(cx).pinned
    }
}

/// Builds rich tooltip content each time the tooltip shows.
pub type ContentFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// What a tooltip shows.
#[derive(Clone)]
pub enum TooltipContent {
    Text(SharedString),
    Element(ContentFn),
    /// An element for this frame: a hover card's content.
    #[doc(hidden)]
    Ready(Rc<std::cell::RefCell<Option<AnyElement>>>),
}

/// What surface the engine draws: a tooltip, or a hover card (upstream
/// HoverCard shares Tooltip's triggers and timing).
#[derive(Clone, Default)]
pub(crate) enum Surface {
    #[default]
    Tooltip,
    /// A card of interactive content, a dialog when labeled.
    Card { label: Option<SharedString> },
}

impl<T: Into<SharedString>> From<T> for TooltipContent {
    fn from(text: T) -> Self {
        TooltipContent::Text(text.into())
    }
}

/// Short text on hover and keyboard focus, upstream `Tooltip`.
#[derive(IntoElement)]
pub struct Tooltip {
    id: ElementId,
    content: TooltipContent,
    trigger: Option<AnyElement>,
    text: Option<SharedString>,
    placement: Placement,
    alignment: LayerAlign,
    delay: Duration,
    hide_delay: Duration,
    focus_trigger: FocusTrigger,
    touch_trigger: TouchTrigger,
    enabled: bool,
    open: Option<bool>,
    default_open: bool,
    on_open_change: Option<OpenFn>,
    hover_indication: Option<bool>,
    text_entry: bool,
    trigger_acts: bool,
    press_closes: bool,
    controller: Option<TooltipController>,
    surface: Surface,
    style: StyleRefinement,
}

impl Tooltip {
    /// Upstream `content`: the tooltip's text.
    pub fn new(id: impl Into<ElementId>, content: impl Into<TooltipContent>) -> Self {
        Tooltip {
            id: id.into(),
            content: content.into(),
            trigger: None,
            text: None,
            placement: Placement::Above,
            alignment: LayerAlign::Center,
            delay: Duration::from_millis(200),
            hide_delay: Duration::ZERO,
            focus_trigger: FocusTrigger::Auto,
            touch_trigger: TouchTrigger::Auto,
            enabled: true,
            open: None,
            default_open: false,
            on_open_change: None,
            hover_indication: None,
            text_entry: false,
            trigger_acts: false,
            press_closes: true,
            controller: None,
            surface: Surface::Tooltip,
            style: StyleRefinement::default(),
        }
    }

    /// Replaces the content.
    pub(crate) fn content(mut self, content: TooltipContent) -> Self {
        self.content = content;
        self
    }

    /// Draws a hover card instead of a tooltip.
    pub(crate) fn card(mut self, label: Option<SharedString>) -> Self {
        self.surface = Surface::Card { label };
        self
    }

    /// Content built when shown: anything, not only text.
    pub fn content_element(
        id: impl Into<ElementId>,
        build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Tooltip::new(id, TooltipContent::Element(Rc::new(build)))
    }

    astryx_primitives::setters! {
        /// Upstream `placement`.
        placement: Placement,
        /// Upstream `alignment`.
        alignment: LayerAlign,
        /// Upstream `focusTrigger`.
        focus_trigger: FocusTrigger,
        /// Upstream `touchTrigger`.
        touch_trigger: TouchTrigger,
        /// Upstream `isEnabled`.
        !enabled,
        /// Upstream `isDefaultOpen`: shown when it first renders.
        !default_open,
        /// Upstream `hasHoverIndication`: a dashed underline on the trigger
        /// (by default, on text triggers only).
        ?hover_indication: bool,
    }

    /// Upstream `children`: the element the tooltip is for.
    pub fn child(mut self, trigger: impl IntoElement) -> Self {
        self.trigger = Some(trigger.into_any_element());
        self
    }

    /// Upstream text `children`: a focusable span of text as the trigger.
    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Upstream `delay`: hover time before it opens (default 200 ms).
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Upstream `hideDelay`: time before it closes after the pointer or
    /// focus leaves.
    pub fn hide_delay(mut self, delay: Duration) -> Self {
        self.hide_delay = delay;
        self
    }

    /// Upstream `isOpen`: holds it shown or hidden; changes are reported
    /// through `on_open_change`.
    pub fn open(mut self, open: impl Into<Option<bool>>) -> Self {
        self.open = open.into();
        self
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Shares state with a [`TooltipController`] (upstream `useTooltip`).
    pub fn controller(mut self, controller: &TooltipController) -> Self {
        self.controller = Some(controller.clone());
        self
    }

    /// The trigger is a text field: any focus in it opens the tooltip, as
    /// browsers mark text fields `:focus-visible` on click.
    pub(crate) fn text_entry(mut self, text_entry: bool) -> Self {
        self.text_entry = text_entry;
        self
    }

    /// Whether a press on the trigger closes a preview (FR6). A trigger that
    /// pins the tooltip on press (InfoTip) keeps it.
    pub(crate) fn press_closes(mut self, closes: bool) -> Self {
        self.press_closes = closes;
        self
    }

    /// The trigger performs an action of its own on a tap, so under
    /// `TouchTrigger::Auto` a tap leaves the tooltip to hover and focus.
    pub(crate) fn trigger_acts(mut self, acts: bool) -> Self {
        self.trigger_acts = acts;
        self
    }
}

impl Styled for Tooltip {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Tooltip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rtl = use_direction(cx).is_rtl();
        let state = match &self.controller {
            Some(controller) => controller.0.clone(),
            None => {
                let default_open = self.default_open;
                window.use_keyed_state(
                    ElementId::Name(format!("{}-tooltip-state", self.id).into()),
                    cx,
                    move |_, cx| TooltipState::new(default_open, cx),
                )
            }
        };
        let was_enabled = state.read(cx).enabled;
        state.update(cx, |s, _| {
            s.controlled = self.open;
            s.enabled = self.enabled;
            s.on_open_change = self.on_open_change.clone();
            // Disabling drops a pending open (FR7).
            if !self.enabled && was_enabled {
                s.timer = None;
                s.shown = false;
                s.pinned = false;
            }
        });

        // Focus in the trigger opens it per `focus_trigger`; focus leaving
        // closes it, pinned or not, unless the pointer holds it (FR4, FR12).
        let (group, card) = {
            let s = state.read(cx);
            (s.group.clone(), s.surface.clone())
        };
        let in_card = card.contains_focused(window, cx);
        let within = group.contains_focused(window, cx) || in_card;
        let opens = in_card
            || (within
                && match self.focus_trigger {
                    FocusTrigger::Never => false,
                    FocusTrigger::Always => true,
                    FocusTrigger::Auto => self.text_entry || window.last_input_was_keyboard(),
                });
        let was_focused = state.read(cx).focused;
        if opens != was_focused {
            state.update(cx, |s, _| s.focused = opens);
            let state = state.clone();
            window.defer(cx, move |window, cx| {
                if opens {
                    show(&state, false, window, cx);
                } else {
                    let (pinned, over) = {
                        let s = state.read(cx);
                        (s.pinned, s.over_trigger || s.over_surface)
                    };
                    if pinned || !over {
                        hide(&state, window, cx);
                    }
                }
            });
        }
        let was_within = state.update(cx, |s, _| std::mem::replace(&mut s.within, within));
        if was_within && !within && state.read(cx).pinned {
            // Focus left a pinned tooltip's trigger (FR12).
            let state = state.clone();
            window.defer(cx, move |window, cx| hide(&state, window, cx));
        }

        let visible = state.read(cx).visible();
        let text_trigger = self.text.is_some();
        let was_visible = state.update(cx, |s, _| {
            s.text_trigger = text_trigger;
            std::mem::replace(&mut s.was_visible, visible)
        });
        if was_visible && !visible && in_card {
            // The card closed with focus inside it: back to the trigger
            // (HoverCard FR6, FR7).
            let state = state.clone();
            window.defer(cx, move |window, cx| return_focus(&state, window, cx));
        }
        let (delay, hide_delay) = (self.delay, self.hide_delay);
        let leave_delay = if hide_delay.is_zero() {
            HOVER_BRIDGE
        } else {
            hide_delay
        };

        // The trigger: the element, or a focusable span of text.
        let is_card = matches!(self.surface, Surface::Card { .. });
        let describes = match &self.content {
            TooltipContent::Text(text) if !is_card => Some(text.clone()),
            _ => None,
        };
        let indicate = self.hover_indication.unwrap_or(self.text.is_some());
        let trigger: AnyElement = match (self.text, self.trigger) {
            (Some(text), _) => {
                let mut span = div()
                    .id(ElementId::Name(format!("{}-text", self.id).into()))
                    .inline_flex()
                    .track_focus(&state.read(cx).text.clone())
                    .child(text);
                if let Some(d) = &describes {
                    span = span.aria_description(d.clone());
                }
                if indicate {
                    span = span
                        .border_b_1()
                        .border_dashed()
                        .border_color(t.color.border_emphasized);
                }
                span.into_any_element()
            }
            (None, Some(trigger)) => match (&describes, &self.surface) {
                (Some(d), _) => provide(TooltipDescribes(d.clone()), trigger).into_any_element(),
                // A labeled card tells a button trigger it opens a dialog
                // (HoverCard FR3).
                (None, Surface::Card { label: Some(_) }) => provide(
                    super::disclosure::Disclosure::new(visible, gpui::HasPopup::Dialog, None),
                    trigger,
                )
                .into_any_element(),
                (None, _) => trigger,
            },
            (None, None) => div().into_any_element(),
        };
        let touch = self.touch_trigger;
        let acts = self.trigger_acts;
        let press_closes = self.press_closes;
        let wrapper = div()
            .id(ElementId::Name(format!("{}-trigger", self.id).into()))
            .display_contents()
            .track_focus(&group)
            .on_hover({
                let state = state.clone();
                move |hovered, window, cx| {
                    if *hovered {
                        let suppressed = state.update(cx, |s, _| {
                            s.over_trigger = true;
                            s.suppressed
                        });
                        if !suppressed && !state.read(cx).visible() {
                            schedule(&state, true, delay, window, cx);
                        } else {
                            state.update(cx, |s, _| s.timer = None);
                        }
                    } else {
                        state.update(cx, |s, _| {
                            s.over_trigger = false;
                            s.suppressed = false;
                        });
                        let pinned = state.read(cx).pinned;
                        if !pinned {
                            schedule(&state, false, leave_delay, window, cx);
                        }
                    }
                }
            })
            .on_mouse_down(MouseButton::Left, {
                let state = state.clone();
                move |_, window, cx| {
                    if window.last_input_was_touch() || !press_closes {
                        return;
                    }
                    // A press on the trigger closes a preview (FR6).
                    let (shown, pinned, controlled) = {
                        let s = state.read(cx);
                        (s.visible(), s.pinned, s.controlled.is_some())
                    };
                    state.update(cx, |s, _| s.suppressed = true);
                    if shown && !pinned && !controlled {
                        hide(&state, window, cx);
                    } else {
                        state.update(cx, |s, _| s.timer = None);
                    }
                }
            })
            .on_click({
                let state = state.clone();
                move |_, window, cx| {
                    // A tap where there is no hover (FR5).
                    if !window.last_input_was_touch() {
                        return;
                    }
                    let taps = match touch {
                        TouchTrigger::Tap => true,
                        TouchTrigger::Auto => !acts,
                        TouchTrigger::None => false,
                    };
                    if taps {
                        if state.read(cx).visible() {
                            hide(&state, window, cx);
                        } else {
                            show(&state, true, window, cx);
                        }
                    }
                }
            })
            .child(trigger);

        let mut anchored = anchored_layer(
            ElementId::Name(format!("{}-layer", self.id).into()),
            wrapper,
        );
        if visible {
            // The entrance from the anchor's side (upstream `layerAnimations`).
            let now = cx.background_executor().now();
            let since = state.update(cx, |s, _| *s.shown_at.get_or_insert(now));
            let side = state.read(cx).side;
            let content = match &self.content {
                TooltipContent::Text(text) => text.clone().into_any_element(),
                TooltipContent::Element(build) => build(window, cx),
                TooltipContent::Ready(element) => element
                    .borrow_mut()
                    .take()
                    .unwrap_or_else(|| div().into_any_element()),
            };
            let selector = format!("{}-surface", self.id);
            let mut surface = div()
                .id(ElementId::Name(format!("{}-surface", self.id).into()))
                .debug_selector(|| selector)
                .rounded(t.radius.container)
                .font_stack(&t.font.family.body)
                .text_style(&t.text.body);
            surface = match &self.surface {
                Surface::Tooltip => surface
                    .max_w(px(MAX_WIDTH))
                    .py(t.spacing.s1)
                    .px(t.spacing.s2)
                    .bg(t.color.background_inverted)
                    .text_color(t.color.background_surface)
                    .role(Role::Tooltip),
                Surface::Card { label } => {
                    let cap = super::motion::viewport_cap(t, window);
                    let card = surface
                        .track_focus(&card)
                        .occlude()
                        .max_w(cap.width)
                        .max_h(cap.height)
                        .overflow_y_scroll()
                        .p(t.spacing.s3)
                        .bg(t.color.background_surface)
                        .text_color(t.color.text_primary)
                        .shadow_token(&t.shadow.med);
                    match label {
                        Some(label) => card.role(Role::Dialog).aria_label(label.clone()),
                        None => card.role(Role::Group),
                    }
                }
            };
            surface = surface
                .on_hover({
                    let state = state.clone();
                    move |hovered, window, cx| {
                        state.update(cx, |s, _| s.over_surface = *hovered);
                        if *hovered {
                            state.update(cx, |s, _| s.timer = None);
                        } else if !state.read(cx).pinned {
                            schedule(&state, false, leave_delay, window, cx);
                        }
                    }
                })
                // A press on the surface acts only within it (FR11).
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(content);
            if let Some(d) = &describes {
                surface = surface.aria_label(d.clone());
            }
            let target = if is_card { "hover-card" } else { "tooltip" };
            theme.refine(surface.style(), target, &[], None);
            surface.style().refine(&self.style);
            let surface = super::motion::entrance(surface, since, side, rtl, t, window, cx);
            let options = LayerOptions {
                placement: self.placement,
                align: self.alignment,
                offset: super::motion::clearance(t, window),
                margin: super::motion::gutter(t, window),
                rtl,
                priority: 1,
            };
            let (dismiss_state, placed_state) = (state.clone(), state.clone());
            anchored = anchored
                .surface(surface, options, move |window, cx| {
                    hide(&dismiss_state, window, cx)
                })
                .on_placed(move |position, _, cx| {
                    placed_state.update(cx, |s, _| s.side = position.side);
                });
        } else {
            state.update(cx, |s, _| s.shown_at = None);
        }
        anchored
    }
}
