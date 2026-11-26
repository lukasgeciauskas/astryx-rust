//! The shared behavior of every pressable control: focus (a tab stop), the
//! hover and pressed overlays (upstream `interactionOverlayStyles`),
//! press scale, the focus ring, disabled and busy guards, and theme hover
//! rules. Controls build their own look, then hand their root here.

use std::rc::Rc;

use astryx_primitives::{is_focus_visible, prefers_reduced_motion};
use astryx_theme::color::composite;
use astryx_theme::{ResolvedTheme, RuleState};
use gpui::{
    App, ClickEvent, Div, FocusHandle, Hsla, InteractiveElement as _, ParentElement as _, Pixels,
    Stateful, StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
};

use crate::focus::focus_ring;

pub(crate) type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Provided around a grid's action cell (MultiSelector FR10): the control
/// in it registers its activation here, so Enter on the cell presses it
/// (upstream clicks the cell's first control).
#[derive(Clone, Default)]
pub(crate) struct ActionCell(Rc<std::cell::RefCell<Option<ClickFn>>>);

impl ActionCell {
    /// Presses the registered control; false when there is none.
    pub(crate) fn activate(
        &self,
        bounds: gpui::Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let f = self.0.borrow().clone();
        match f {
            Some(f) => {
                let event = ClickEvent::Keyboard(gpui::KeyboardClickEvent {
                    button: gpui::KeyboardButton::Enter,
                    bounds,
                });
                f(&event, window, cx);
                true
            }
            None => false,
        }
    }
}

/// The touch press model on a surface that paints its own press (upstream
/// `usePressFeedback` and `interactionOverlayStyles`): under a fine primary pointer GPUI's
/// active style keeps the mouse's press; under a finger (and every press on a touch-first
/// device) the window's press controller paints `overlay` over `base` as UIKit tracks a
/// control, at once outside a scroller and after the content-touch delay inside one, and
/// fades it on the theme's exit clock. `id` names the surface.
pub(crate) fn touch_press(
    el: Stateful<Div>,
    id: impl Into<gpui::ElementId>,
    disabled: bool,
    base: Hsla,
    overlay: Hsla,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let feedback = crate::gestures::use_press_feedback(id, window, cx).disabled(disabled);
    touch_press_with(el, &feedback, base, overlay, window, cx)
}

/// [`touch_press`] in a builder chain.
pub(crate) trait TouchPress: Sized {
    fn touch_press(
        self,
        id: impl Into<gpui::ElementId>,
        base: Hsla,
        overlay: Hsla,
        window: &mut Window,
        cx: &mut App,
    ) -> Self;
}

impl TouchPress for Stateful<Div> {
    fn touch_press(
        self,
        id: impl Into<gpui::ElementId>,
        base: Hsla,
        overlay: Hsla,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        touch_press(self, id, false, base, overlay, window, cx)
    }
}

/// [`touch_press`] with a surface the caller made (a menu row's, a focusable control's).
pub(crate) fn touch_press_with(
    el: Stateful<Div>,
    feedback: &crate::gestures::PressFeedback,
    base: Hsla,
    overlay: Hsla,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let pressed = composite(base, overlay);
    let mut el = if feedback.paints_buttons(window, cx) {
        el
    } else {
        el.active(move |s| s.bg(pressed))
    };
    if let Some(strength) = feedback.overlay(overlay, window, cx) {
        el = el.bg(composite(base, strength));
    }
    el.child(feedback.probe().absolute().top_0().left_0().size_full())
}

/// How a control presses.
pub(crate) struct Press<'a> {
    /// The touch press model's surface (upstream `usePressFeedback`): under a finger it
    /// paints the pressed overlay as UIKit tracks a control, and on a touch-first device
    /// it paints the mouse's and the Space key's press in place of the active style. The
    /// caller wraps its element in [`crate::gestures::PressFeedback::surface`].
    pub feedback: Option<&'a crate::gestures::PressFeedback>,
    /// The resting fill the overlays composite over (`None` is clear).
    pub fill: Option<Hsla>,
    /// Pressed scale (0.98 for a standalone Button), or none.
    pub scale: Option<f32>,
    /// Upstream disabled: focusable, half opacity, no activation.
    pub disabled: bool,
    /// Upstream busy: focusable, full opacity, no activation.
    pub busy: bool,
    /// The focus ring's corner radius and color (`None` for the token's).
    pub ring: Option<(Pixels, Option<Hsla>)>,
    /// Paints the ring after the control's neighbours, so a joined
    /// neighbour never covers it (an InputGroup addon's `zIndex: 2`).
    pub raise_ring: bool,
    /// The theme target and props whose hover and active rules apply.
    pub theme_target: Option<(&'a str, &'a [(&'a str, &'a str)])>,
}

/// Applies the press behavior to `el` and wires `on_click`.
pub(crate) fn apply_press(
    el: Stateful<Div>,
    focus: &FocusHandle,
    press: Press<'_>,
    on_click: Option<ClickFn>,
    theme: &ResolvedTheme,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let t = theme.tokens();
    let rem = window.rem_size();
    let inert = press.disabled || press.busy;
    let mut el = el.track_focus(focus);
    if press.disabled {
        el = el.opacity(0.5);
    }
    if !inert {
        let base = press.fill.unwrap_or(Hsla::transparent_black());
        let mut hover = StyleRefinement::default().bg(composite(base, t.color.overlay_hover));
        let mut active = StyleRefinement::default().bg(composite(base, t.color.overlay_pressed));
        if let Some(scale) = press.scale.filter(|_| !prefers_reduced_motion(cx)) {
            active = active.scale(scale);
        }
        if let Some((target, props)) = press.theme_target {
            theme.refine(&mut hover, target, props, Some(RuleState::Hover));
            theme.refine(&mut active, target, props, Some(RuleState::Active));
        }
        el = el.cursor_pointer().hover(move |_| hover);
        match press.feedback {
            Some(feedback) => {
                if !feedback.paints_buttons(window, cx) {
                    el = el.active(move |_| active);
                }
                if let Some(overlay) = feedback.overlay(t.color.overlay_pressed, window, cx) {
                    el = el.bg(composite(base, overlay));
                    if let Some(scale) = press.scale.filter(|_| !prefers_reduced_motion(cx)) {
                        el = el.scale(scale);
                    }
                }
            }
            None => el = el.active(move |_| active),
        }
        if let Some(on_click) = on_click {
            if let Some(cell) = astryx_primitives::use_context::<ActionCell>(cx)
                && cell.0.borrow().is_none()
            {
                *cell.0.borrow_mut() = Some(on_click.clone());
            }
            el = el.on_click(move |event, window, cx| {
                // A control's click is its own; a ClickableCard around it
                // never sees it (upstream: the card fires on its surface only).
                cx.stop_propagation();
                on_click(event, window, cx)
            });
        }
    } else {
        // Inert controls keep their tab stop but swallow presses, so a
        // click never reaches an ancestor's handler (upstream FR9).
        el = el.on_click(|_, _, cx| cx.stop_propagation());
    }
    if let Some((radius, color)) = press.ring
        && is_focus_visible(focus, window)
    {
        let mut ring = focus_ring(t, radius, rem);
        if let Some(color) = color {
            ring = ring.border_color(color);
        }
        el = if press.raise_ring {
            el.child(gpui::deferred(ring).with_priority(1))
        } else {
            el.child(ring)
        };
    }
    el
}
