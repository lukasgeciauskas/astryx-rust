//! ClickableCard and SelectableCard. Upstream: `ClickableCard/`,
//! `SelectableCard/`.
//!
//! Both are a Card made pressable: a hover and press overlay over the whole
//! surface, the emphasized border on hover, and the focus ring outside.
//! A control inside keeps its own clicks (each stops propagation).

use std::rc::Rc;

use astryx_primitives::{control_focus, is_focus_visible, use_link_handler};
use gpui::{
    App, ClickEvent, ElementId, Hsla, InteractiveElement, Length, Role, StatefulInteractiveElement,
    Toggled,
};

use super::*;
use crate::choice::row_toggles;
use crate::focus::focus_ring;

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The selection ring a SelectableCard draws for its variant: the accent,
/// or the variant's own border color.
fn ring_color(variant: CardVariant, t: &Tokens) -> Hsla {
    let c = &t.color;
    match variant {
        CardVariant::Default | CardVariant::Transparent | CardVariant::Muted => c.accent,
        CardVariant::Blue => c.border_blue,
        CardVariant::Cyan => c.border_cyan,
        CardVariant::Gray => c.border_gray,
        CardVariant::Green => c.border_green,
        CardVariant::Orange => c.border_orange,
        CardVariant::Pink => c.border_pink,
        CardVariant::Purple => c.border_purple,
        CardVariant::Red => c.border_red,
        CardVariant::Teal => c.border_teal,
        CardVariant::Yellow => c.border_yellow,
    }
}

/// What a pressable card shares.
struct PressableCard {
    id: ElementId,
    label: SharedString,
    card: Card,
    disabled: bool,
    /// The selection ring, for a selected SelectableCard.
    ring: Option<Hsla>,
    theme_target: &'static str,
}

impl PressableCard {
    fn render(
        self,
        role: Role,
        toggled: Option<Toggled>,
        activate: Option<ClickFn>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Stateful<Div> {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let focus = control_focus(&self.id, window, cx);
        let group: SharedString = format!("{:?}-card", self.id).into();
        let radius = t.radius.container.to_pixels(rem);
        let variant = self.card.variant;
        let sizing = self.card.size;

        // The card fills the pressable box, which carries its size.
        let (mut card, padding) = self.card.root(&theme, dir, rem);
        let corners = super::corners_of(&mut card, &theme, rem);
        let mut card = card.relative();
        card.style().size.width = Some(Length::from(gpui::relative(1.)));
        if sizing.height.is_some() {
            card = card.h_full();
        }
        let part = theme.for_part("card", &[("variant", variant.as_str())]);
        let border = t.border.width.to_pixels(rem);
        let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);
        let mut overlay = div()
            .id("overlay")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .rounded((radius - border).max(px(0.)));
        if let Some(ring) = self.ring {
            overlay = overlay.border_2().border_color(ring);
            card = card.border_color(ring);
        }
        if !self.disabled {
            overlay = overlay
                .group_hover(group.clone(), move |s| s.bg(hover))
                .group_active(group.clone(), move |s| s.bg(pressed));
            if self.ring.is_none() {
                let emphasized = t.color.border_emphasized;
                card = card.group_hover(group.clone(), move |s| s.border_color(emphasized));
            }
        }
        let card = card
            .child(container_children(
                self.card.children,
                padding,
                corners,
                part,
                Some(crate::layout::LayoutContainerKind::Card),
                cx,
            ))
            .child(overlay);

        let mut el = div().relative().flex().flex_col();
        el = sizing.apply(el);
        theme.refine(
            el.style(),
            self.theme_target,
            &[("variant", variant.as_str())],
            None,
        );
        let mut el = el.id(self.id).group(group).track_focus(&focus).child(card);
        if self.disabled {
            el = el.opacity(0.5).on_click(|_, _, cx| cx.stop_propagation());
        } else if let Some(activate) = activate {
            el = el.cursor_pointer().on_click(move |event, window, cx| {
                cx.stop_propagation();
                activate(event, window, cx);
            });
        }
        if is_focus_visible(&focus, window) {
            el = el.child(focus_ring(t, radius, rem));
        }
        let el = el
            .role(role)
            .aria_label(self.label)
            .aria_disabled(self.disabled);
        match toggled {
            Some(toggled) => el.aria_toggled(toggled),
            None => el,
        }
    }
}

macro_rules! card_setters {
    () => {
        /// Upstream `padding`.
        pub fn padding(mut self, step: SpacingStep) -> Self {
            self.card = self.card.padding(step);
            self
        }
        /// Upstream `variant`.
        pub fn variant(mut self, variant: CardVariant) -> Self {
            self.card = self.card.variant(variant);
            self
        }
        /// Upstream `elevation`.
        pub fn elevation(mut self, elevation: Elevation) -> Self {
            self.card = self.card.elevation(elevation);
            self
        }
        /// Upstream `width`.
        pub fn width(mut self, width: impl Into<Length>) -> Self {
            self.card = self.card.width(width);
            self
        }
        /// Upstream `height`.
        pub fn height(mut self, height: impl Into<Length>) -> Self {
            self.card = self.card.height(height);
            self
        }
        /// Upstream `maxWidth`.
        pub fn max_width(mut self, max_width: impl Into<Length>) -> Self {
            self.card = self.card.max_width(max_width);
            self
        }
        /// Upstream `isDisabled`: half opacity, keeps its tab stop.
        pub fn disabled(mut self, disabled: bool) -> Self {
            self.disabled = disabled;
            self
        }
    };
}

// ----------------------------------------------------------------------------
// ClickableCard

/// A card that navigates or acts, upstream `ClickableCard`.
///
/// ```ignore
/// ClickableCard::new("project", "Open Apollo").href("/projects/apollo").child(..)
/// ```
#[derive(IntoElement)]
pub struct ClickableCard {
    id: ElementId,
    label: SharedString,
    card: Card,
    disabled: bool,
    on_click: Option<ClickFn>,
    href: Option<SharedString>,
    target: Option<SharedString>,
}

impl ClickableCard {
    /// Upstream `label`: the card's accessible name.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        ClickableCard {
            id: id.into(),
            label: label.into(),
            card: Card::new(),
            disabled: false,
            on_click: None,
            href: None,
            target: None,
        }
    }

    card_setters!();

    /// Upstream `onClick`: the card's surface was pressed.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `href`: through the link handler.
    pub fn href(mut self, href: impl Into<SharedString>) -> Self {
        self.href = Some(href.into());
        self
    }

    /// Upstream `target`.
    pub fn target(mut self, target: impl Into<SharedString>) -> Self {
        self.target = Some(target.into());
        self
    }
}

impl Styled for ClickableCard {
    fn style(&mut self) -> &mut StyleRefinement {
        self.card.style()
    }
}

impl ParentElement for ClickableCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.card.extend(elements);
    }
}

impl RenderOnce for ClickableCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let on_click = self.on_click.clone();
        let navigation = self
            .href
            .clone()
            .filter(|_| !self.disabled)
            .map(|href| (href, self.target.clone(), use_link_handler(cx)));
        let role = if self.href.is_some() {
            Role::Link
        } else {
            Role::Button
        };
        let activate: ClickFn = Rc::new(move |event, window, cx| {
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
        PressableCard {
            id: self.id,
            label: self.label,
            card: self.card,
            disabled: self.disabled,
            ring: None,
            theme_target: "clickable-card",
        }
        .render(role, None, Some(activate), window, cx)
    }
}

// ----------------------------------------------------------------------------
// SelectableCard

type SelectFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// A card that toggles selected, upstream `SelectableCard`.
///
/// ```ignore
/// SelectableCard::new("plan-pro", "Pro plan", selected)
///     .on_change(|selected, _, _| { /* store it */ })
///     .child(..)
/// ```
#[derive(IntoElement)]
pub struct SelectableCard {
    id: ElementId,
    label: SharedString,
    selected: bool,
    card: Card,
    disabled: bool,
    on_change: Option<SelectFn>,
}

impl SelectableCard {
    /// Upstream `label` and `isSelected`.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, selected: bool) -> Self {
        SelectableCard {
            id: id.into(),
            label: label.into(),
            selected,
            card: Card::new(),
            disabled: false,
            on_change: None,
        }
    }

    card_setters!();

    /// Upstream `onChange`: the next selected state.
    pub fn on_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl Styled for SelectableCard {
    fn style(&mut self) -> &mut StyleRefinement {
        self.card.style()
    }
}

impl ParentElement for SelectableCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.card.extend(elements);
    }
}

impl RenderOnce for SelectableCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let ring = self
            .selected
            .then(|| ring_color(self.card.variant, theme.tokens()));
        let selected = self.selected;
        let on_change = self.on_change.clone();
        // Like a checkbox: Space toggles, Enter does not.
        let activate: ClickFn = Rc::new(move |event, window, cx| {
            if row_toggles(event)
                && let Some(f) = &on_change
            {
                f(!selected, window, cx);
            }
        });
        PressableCard {
            id: self.id,
            label: self.label,
            card: self.card,
            disabled: self.disabled,
            ring,
            theme_target: "selectable-card",
        }
        .render(
            Role::CheckBox,
            Some(if selected {
                Toggled::True
            } else {
                Toggled::False
            }),
            Some(activate),
            window,
            cx,
        )
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CLICKABLE_CARD_DOC: ComponentDoc = ComponentDoc {
    name: "ClickableCard",
    rust_type: "astryx::components::containers::ClickableCard",
    props: &[
        p("label", RustApi::Argument("label")),
        p("onClick", RustApi::Method("on_click")),
        p("href", RustApi::Method("href")),
        p("target", RustApi::Method("target")),
        p("isDisabled", RustApi::Method("disabled")),
        p("children", RustApi::Trait("ParentElement")),
        p("padding", RustApi::Method("padding")),
        p("variant", RustApi::Method("variant")),
        p("elevation", RustApi::Method("elevation")),
        p("width", RustApi::Method("width")),
        p("height", RustApi::Method("height")),
        p("maxWidth", RustApi::Method("max_width")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const SELECTABLE_CARD_DOC: ComponentDoc = ComponentDoc {
    name: "SelectableCard",
    rust_type: "astryx::components::containers::SelectableCard",
    props: &[
        p("label", RustApi::Argument("label")),
        p("isSelected", RustApi::Argument("selected")),
        p("onChange", RustApi::Method("on_change")),
        p("isDisabled", RustApi::Method("disabled")),
        p("children", RustApi::Trait("ParentElement")),
        p("padding", RustApi::Method("padding")),
        p("variant", RustApi::Method("variant")),
        p("elevation", RustApi::Method("elevation")),
        p("width", RustApi::Method("width")),
        p("height", RustApi::Method("height")),
        p("maxWidth", RustApi::Method("max_width")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
