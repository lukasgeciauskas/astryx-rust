//! Link. Upstream: `Link/`.

pub mod examples;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::{control_focus, is_focus_visible, use_link_handler};
use astryx_theme::color::mix_srgb;
use astryx_theme::{RuleState, current};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, IntoElement, KeyboardButton, ParentElement, RenderOnce,
    Role, SharedString, StyleRefinement, Styled, Window, div, prelude::*, px, relative,
};
use smallvec::SmallVec;

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconSize};
use crate::layout::VisuallyHidden;
use crate::text::{Text, TextColor, TextSize, TextType, TextWeight};

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The form a Link takes (upstream FR1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkForm {
    /// An enabled destination: role link, activated by Enter.
    Anchor,
    /// No destination but a click handler: role button, Enter or Space.
    Button,
    /// A disabled destination: role link, inert.
    DisabledAnchor,
}

/// A navigation link, upstream `Link`.
///
/// ```ignore
/// Link::new("docs", "Read the docs").href("https://example.com/docs")
/// ```
#[derive(IntoElement)]
pub struct Link {
    id: ElementId,
    text: SharedString,
    href: Option<SharedString>,
    label: Option<SharedString>,
    underline: bool,
    disabled: bool,
    external: bool,
    new_tab_label: Option<SharedString>,
    target: Option<SharedString>,
    on_click: Option<ClickFn>,
    tooltip: Option<SharedString>,
    standalone: bool,
    text_type: TextType,
    size: Option<TextSize>,
    weight: Option<TextWeight>,
    color: TextColor,
    max_lines: usize,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
}

impl Link {
    /// Upstream `children`: the link text.
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Link {
            id: id.into(),
            text: text.into(),
            href: None,
            label: None,
            underline: false,
            disabled: false,
            external: false,
            new_tab_label: None,
            target: None,
            on_click: None,
            tooltip: None,
            standalone: false,
            text_type: TextType::Body,
            size: None,
            weight: None,
            color: TextColor::Accent,
            max_lines: 0,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `href`: the destination, through the link handler.
        ?href: SharedString,
        /// Upstream `label`: replaces the text as the accessible name, for
        /// links whose content is not descriptive text.
        ?label: SharedString,
        /// Upstream `hasUnderline`: underlined at rest, not only on hover.
        !underline,
        /// Upstream `isDisabled`: focusable, half opacity, never navigates.
        !disabled,
        /// Upstream `isExternalLink`: opens in a new tab, with an icon.
        !external,
        /// Upstream `newTabLabel`: the hidden new-tab text of an external link.
        ?new_tab_label: SharedString,
        /// Upstream `target`. `external` sets `_blank`.
        ?target: SharedString,
        /// Upstream `tooltip`. The tooltip itself lands with Tooltip (M5).
        ?tooltip: SharedString,
        /// Upstream `isStandalone`: body size and leading, for a link outside
        /// running text.
        !standalone,
        /// Upstream `type`.
        text_type: TextType,
        /// Upstream `size`.
        ?size: TextSize,
        /// Upstream `weight`.
        ?weight: TextWeight,
        /// Upstream `color`: accent by default.
        color: TextColor,
        /// Upstream `maxLines`: 0 never truncates.
        max_lines: usize,
    }

    /// Upstream `onClick`: runs before navigation; `window.prevent_default()`
    /// skips it. Without `href`, the link is a button.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    pub(crate) fn form(&self) -> LinkForm {
        match (&self.href, self.disabled) {
            (Some(_), false) => LinkForm::Anchor,
            (Some(_), true) => LinkForm::DisabledAnchor,
            (None, _) => LinkForm::Button,
        }
    }
}

impl Styled for Link {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Link {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Link {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let form = self.form();
        let focus = control_focus(&self.id, window, cx);
        // The root paints the color; the label, underline and icon take it.
        let color = self.color.token(t);
        let hover_color = match self.color {
            TextColor::Primary | TextColor::Secondary | TextColor::Accent => {
                color.map(|c| mix_srgb(c, t.color.tint_hover, 0.15))
            }
            _ => None,
        };
        let external_icon = self.external && form != LinkForm::Button;

        let mut el = div()
            .relative()
            .inline_flex()
            .flex_row()
            .items_center()
            .gap(t.spacing.s0_5)
            .max_w_full()
            .min_w_0();
        if let Some(color) = color {
            el = el.text_color(color);
        }
        if self.standalone {
            el = el
                .text_size(t.text.body.size)
                .line_height(relative(t.text.body.leading));
        }
        if self.underline {
            el = el.underline();
        }
        let props = [("color", self.color.as_str())];
        theme.refine(el.style(), "link", &props, None);
        el.style().refine(&self.style);

        let mut text = Text::new(self.text.clone())
            .text_type(self.text_type)
            .color(TextColor::Inherit)
            .max_lines(self.max_lines);
        if let Some(size) = self.size {
            text = text.size(size);
        }
        if let Some(weight) = self.weight {
            text = text.weight(weight);
        }
        let new_tab = external_icon.then(|| {
            let label = self
                .new_tab_label
                .clone()
                .unwrap_or_else(|| i18n(cx).t("@astryx.link.newTab"));
            [
                Icon::new("externalLink")
                    .size(IconSize::Xsm)
                    .into_any_element(),
                VisuallyHidden::new().child(label).into_any_element(),
            ]
        });

        let mut el = el
            .id(self.id.clone())
            .track_focus(&focus)
            .child(div().min_w_0().child(text))
            .children(self.children)
            .children(new_tab.into_iter().flatten());

        if self.disabled {
            // Focusable but inert: no hover, no press, no activation (FR5).
            el = el.opacity(0.5).on_click(|_, _, cx| cx.stop_propagation());
        } else {
            let mut hover = StyleRefinement::default().underline();
            if let Some(hover_color) = hover_color {
                hover = hover.text_color(hover_color);
            }
            let mut active = StyleRefinement::default().bg(t.color.overlay_pressed);
            theme.refine(&mut hover, "link", &props, Some(RuleState::Hover));
            theme.refine(&mut active, "link", &props, Some(RuleState::Active));
            let on_click = self.on_click.clone();
            let navigation = self
                .href
                .clone()
                .filter(|_| form == LinkForm::Anchor)
                .map(|href| {
                    let target = if self.external {
                        Some(SharedString::from("_blank"))
                    } else {
                        self.target.clone()
                    };
                    (href, target, use_link_handler(cx))
                });
            el = el
                .cursor_pointer()
                .hover(move |_| hover)
                .active(move |_| active)
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    // A native anchor activates on Enter, not Space.
                    if form == LinkForm::Anchor
                        && let ClickEvent::Keyboard(key) = event
                        && key.button == KeyboardButton::Space
                    {
                        return;
                    }
                    if let Some(f) = &on_click {
                        f(event, window, cx);
                        if window.default_prevented() {
                            return;
                        }
                    }
                    if let Some((href, target, handler)) = &navigation {
                        handler.open(href, target.clone(), window, cx);
                    }
                });
        }
        if is_focus_visible(&focus, window) {
            el = el.child(focus_ring(t, px(0.), rem));
        }
        let role = match form {
            LinkForm::Anchor | LinkForm::DisabledAnchor => Role::Link,
            LinkForm::Button => Role::Button,
        };
        let name = self.label.filter(|l| !l.is_empty()).unwrap_or(self.text);
        let mut el = el.role(role).aria_label(name);
        if let Some(description) = crate::overlay::tooltip_description(cx) {
            el = el.aria_description(description);
        }
        crate::overlay::with_tooltip(&self.id, self.tooltip, true, el)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const LINK_DOC: ComponentDoc = ComponentDoc {
    name: "Link",
    rust_type: "astryx::components::link::Link",
    props: &[
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p(
            "as",
            RustApi::Waived("the link handler (link_provider) routes every href"),
        ),
        p("label", RustApi::Method("label")),
        p("href", RustApi::Method("href")),
        p("hasUnderline", RustApi::Method("underline")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isExternalLink", RustApi::Method("external")),
        p("newTabLabel", RustApi::Method("new_tab_label")),
        p("target", RustApi::Method("target")),
        p("rel", RustApi::Waived("GPUI opens URLs without a referrer")),
        p(
            "download",
            RustApi::Waived("downloads are the link handler's to decide"),
        ),
        p(
            "referrerPolicy",
            RustApi::Waived("GPUI opens URLs without a referrer"),
        ),
        p("onClick", RustApi::Method("on_click")),
        p("tooltip", RustApi::Method("tooltip")),
        p("isStandalone", RustApi::Method("standalone")),
        p("type", RustApi::Method("text_type")),
        p("size", RustApi::Method("size")),
        p("weight", RustApi::Method("weight")),
        p("color", RustApi::Method("color")),
        p(
            "display",
            RustApi::Waived("GPUI has no inline layout; a Link is always a flex row"),
        ),
        p("maxLines", RustApi::Method("max_lines")),
        p("children", RustApi::Argument("text")),
    ],
    example: include_str!("examples.rs"),
};
