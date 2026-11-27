//! InfoTip. Upstream: `InfoTip/`.
//!
//! An info button that previews its help on hover and keyboard focus like a
//! tooltip, and pins it open on press (a toggletip): a click, a tap, Enter
//! or Space shows it until the same press, Escape, a press outside, or
//! focus leaving closes it. Pinning announces the help once.

use gpui::{
    App, ElementId, IntoElement, ParentElement, RenderOnce, Role, SharedString, StyleRefinement,
    Styled, Window, div, prelude::*, px,
};
use rustryx_i18n::current as i18n;
use rustryx_primitives::{
    LayerAlign, Placement, Politeness, announce, control_focus, is_focus_visible,
};
use rustryx_theme::current;

use super::tooltip::{Tooltip, TooltipController};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconSize};

/// An info button that shows short help, upstream `InfoTip`.
#[derive(IntoElement)]
pub struct InfoTip {
    id: ElementId,
    content: SharedString,
    label: Option<SharedString>,
    size: IconSize,
    placement: Placement,
    alignment: LayerAlign,
    hidden_until_focus: bool,
    style: StyleRefinement,
}

impl InfoTip {
    /// Upstream `content`: the help it reveals.
    pub fn new(id: impl Into<ElementId>, content: impl Into<SharedString>) -> Self {
        InfoTip {
            id: id.into(),
            content: content.into(),
            label: None,
            size: IconSize::Sm,
            placement: Placement::Above,
            alignment: LayerAlign::Center,
            hidden_until_focus: false,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `label`: the button's name (default "More information").
        ?label: SharedString,
        /// Upstream `size`: the info icon's size.
        size: IconSize,
        /// Where the help appears.
        placement: Placement,
        alignment: LayerAlign,
    }
}

impl InfoTip {
    /// Takes no room and shows nothing until keyboard focus reaches it (the
    /// InfoTip of a hidden label).
    pub(crate) fn hidden_until_focus(mut self, hidden: bool) -> Self {
        self.hidden_until_focus = hidden;
        self
    }
}

impl Styled for InfoTip {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for InfoTip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let tooltip = TooltipController::new(self.id.clone(), window, cx);
        let focus = control_focus(&self.id, window, cx);
        let pinned = tooltip.is_pinned(cx);
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| i18n(cx).t("@astryx.infoTip.label"));

        let content = self.content.clone();
        let press = tooltip.clone();
        // Upstream InfoTip's `useTouchHitArea`: the small button's target grows toward 44 px.
        let area_activate: crate::gestures::touch_hit_area::ActivateFn = {
            let (press, content) = (tooltip.clone(), self.content.clone());
            std::rc::Rc::new(move |window, cx| {
                if press.is_pinned(cx) {
                    press.hide(window, cx);
                } else {
                    press.show(window, cx);
                    announce(content.clone(), Politeness::Polite, window, cx);
                }
            })
        };
        let area = crate::gestures::touch_hit_area::hit_area(
            &ElementId::Name(format!("{}-button", self.id).into()),
            true,
            Some(area_activate),
            None,
            window,
            cx,
        );
        let mut button = div()
            .id(ElementId::Name(format!("{}-button", self.id).into()))
            .track_focus(&focus)
            .relative()
            .inline_flex()
            .items_center()
            .justify_center()
            .p(t.spacing.s0_5)
            .rounded_full()
            .cursor_pointer()
            .text_color(t.color.icon_secondary)
            .hover(|s| s.text_color(t.color.icon_primary))
            .role(Role::Button)
            .aria_label(label)
            .aria_description(content.clone())
            .aria_expanded(pinned)
            // A press pins the help, or unpins it (FR9).
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                if press.is_pinned(cx) {
                    press.hide(window, cx);
                } else {
                    press.show(window, cx);
                    announce(content.clone(), Politeness::Polite, window, cx);
                }
            })
            .child(Icon::new("info").size(self.size))
            .children(area);
        let focus_visible = is_focus_visible(&focus, window);
        if focus_visible {
            button = button.child(focus_ring(t, px(999.), rem));
        }
        if self.hidden_until_focus && !focus_visible {
            button = button.absolute().size_0().overflow_hidden();
        }
        theme.refine(button.style(), "info-tip", &[], None);
        button.style().refine(&self.style);
        Tooltip::new(self.id, self.content)
            .controller(&tooltip)
            .placement(self.placement)
            .alignment(self.alignment)
            .trigger_acts(false)
            .press_closes(false)
            .child(button)
    }
}
