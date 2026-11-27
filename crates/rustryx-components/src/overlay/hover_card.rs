//! HoverCard, upstream `HoverCard/`: richer content than a tooltip, on hover
//! and focus of its trigger. It shares Tooltip's triggers and timing (the
//! hover bridge between trigger and card, focus and touch triggers,
//! controlled and default-open modes, text triggers with a dashed cue) and
//! draws a card instead:
//!
//! - one surface carrying the `hover-card` target, padded by `spacing-3`,
//!   in the body font (FR1); a dialog named by `label`, else a group (FR3);
//! - interactive content keeps its own presses, and a press inside never
//!   reaches the trigger (FR11);
//! - focus inside the card holds it open, and when the card closes with focus
//!   inside, focus returns to the trigger (FR6, FR7).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, ElementId, IntoElement, RenderOnce, SharedString, StyleRefinement, Styled,
    Window,
};
use rustryx_primitives::{LayerAlign, Placement};

use super::tooltip::{FocusTrigger, Tooltip, TooltipContent, TouchTrigger};

/// Content on hover and focus of a trigger, upstream `HoverCard`.
///
/// ```ignore
/// HoverCard::new("author")
///     .label("Ada Lovelace")
///     .content(Text::new("Wrote the first program."))
///     .text("Ada Lovelace")
/// ```
#[derive(IntoElement)]
pub struct HoverCard {
    tooltip: Tooltip,
    label: Option<SharedString>,
}

impl HoverCard {
    pub fn new(id: impl Into<ElementId>) -> Self {
        HoverCard {
            tooltip: Tooltip::new(id, TooltipContent::Text(SharedString::default()))
                .delay(Duration::from_millis(300))
                .hide_delay(Duration::from_millis(200)),
            label: None,
        }
    }

    fn map(mut self, f: impl FnOnce(Tooltip) -> Tooltip) -> Self {
        self.tooltip = f(self.tooltip);
        self
    }

    /// Upstream `content`: shown while the card is open.
    pub fn content(self, content: impl IntoElement) -> Self {
        let element = Rc::new(RefCell::new(Some(content.into_any_element())));
        self.map(|t| t.content(TooltipContent::Ready(element)))
    }

    /// Upstream `content`, built by `f` only while the card is open.
    pub fn content_fn(self, f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.map(|t| t.content(TooltipContent::Element(Rc::new(f))))
    }

    /// Upstream `label`: names the card, which is then a dialog.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        let label: SharedString = label.into();
        self.label = (!label.is_empty()).then_some(label);
        self
    }

    /// Upstream `children`: the element the card is for.
    pub fn child(self, trigger: impl IntoElement) -> Self {
        self.map(|t| t.child(trigger))
    }

    /// Upstream text `children`: a focusable span of text as the trigger,
    /// with a dashed underline as its hover cue.
    pub fn text(self, text: impl Into<SharedString>) -> Self {
        self.map(|t| t.text(text))
    }

    /// Upstream `placement`: above by default.
    pub fn placement(self, placement: Placement) -> Self {
        self.map(|t| t.placement(placement))
    }

    /// Upstream `alignment`: centered by default.
    pub fn alignment(self, alignment: LayerAlign) -> Self {
        self.map(|t| t.alignment(alignment))
    }

    /// Upstream `delay`: hover time before it opens (default 300 ms).
    pub fn delay(self, delay: Duration) -> Self {
        self.map(|t| t.delay(delay))
    }

    /// Upstream `hideDelay`: time before it closes once the pointer leaves
    /// the trigger and the card (default 200 ms).
    pub fn hide_delay(self, delay: Duration) -> Self {
        self.map(|t| t.hide_delay(delay))
    }

    /// Upstream `focusTrigger`.
    pub fn focus_trigger(self, trigger: FocusTrigger) -> Self {
        self.map(|t| t.focus_trigger(trigger))
    }

    /// Upstream `touchTrigger`.
    pub fn touch_trigger(self, trigger: TouchTrigger) -> Self {
        self.map(|t| t.touch_trigger(trigger))
    }

    /// Upstream `isEnabled`.
    pub fn enabled(self, enabled: bool) -> Self {
        self.map(|t| t.enabled(enabled))
    }

    /// Upstream `hasHoverIndication`: the dashed cue (by default, on text
    /// triggers only).
    pub fn hover_indication(self, indicate: bool) -> Self {
        self.map(|t| t.hover_indication(indicate))
    }

    /// Upstream `isDefaultOpen`: open when it first renders.
    pub fn default_open(self, open: bool) -> Self {
        self.map(|t| t.default_open(open))
    }

    /// Upstream `isOpen`: holds it open or closed; Escape asks through
    /// `on_open_change`.
    pub fn open(self, open: impl Into<Option<bool>>) -> Self {
        self.map(|t| t.open(open))
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.map(|t| t.on_open_change(f))
    }
}

impl Styled for HoverCard {
    /// Upstream `xstyle`: the card surface.
    fn style(&mut self) -> &mut StyleRefinement {
        self.tooltip.style()
    }
}

impl RenderOnce for HoverCard {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.tooltip.card(self.label)
    }
}
