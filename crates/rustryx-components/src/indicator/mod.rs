//! Selection indicators: the checkbox box, the radio circle and the check
//! mark of a chosen option. Upstream: `Indicator/`.
//!
//! An indicator is decorative: its owner (CheckboxInput, RadioList, a menu
//! row) carries the role, the name and the focus ring. It reacts to the
//! hover of its owner's row through `hover_group`, upstream's
//! `indicatorScope` marker.

use gpui::{
    AnyElement, App, Hsla, IntoElement, ParentElement, Pixels, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px, svg,
};
use rustryx_theme::color::mix_srgb;
use rustryx_theme::current;
use smallvec::SmallVec;

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{Icon, IconColor, IconSize};

mod theme;

pub use theme::{
    CHECK, CHECKBOX, IndicatorRequest, RADIO, SPINNER, TREE_DISCLOSURE, ThemeIndicatorFn,
    clear_theme_indicator, set_theme_indicator, theme_indicator,
};

/// Upstream indicator `state`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IndicatorState {
    #[default]
    Unchecked,
    Checked,
    /// A checkbox standing for a partly checked group.
    Indeterminate,
}

impl IndicatorState {
    pub fn is_on(self) -> bool {
        self != IndicatorState::Unchecked
    }

    fn as_theme_value(self) -> &'static str {
        match self {
            IndicatorState::Unchecked => "",
            IndicatorState::Checked => "checked",
            IndicatorState::Indeterminate => "indeterminate",
        }
    }
}

impl From<bool> for IndicatorState {
    fn from(checked: bool) -> Self {
        if checked {
            IndicatorState::Checked
        } else {
            IndicatorState::Unchecked
        }
    }
}

/// The `sm | md` size of choice controls: checkboxes, radios, switches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChoiceSize {
    Sm,
    #[default]
    Md,
}

impl ChoiceSize {
    pub fn as_str(self) -> &'static str {
        match self {
            ChoiceSize::Sm => "sm",
            ChoiceSize::Md => "md",
        }
    }

    /// The box and circle size: 20 or 24 px.
    pub fn box_size(self) -> Pixels {
        match self {
            ChoiceSize::Sm => px(20.),
            ChoiceSize::Md => px(24.),
        }
    }
}

/// Upstream's check path, drawn in a 10-unit box.
const CHECK_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M8.5 2.5L4 7.5L1.5 5" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

/// The shared box of the checkbox and radio indicators: colors by state,
/// the owner-hover mix, and the disabled look.
struct Paint {
    border: Hsla,
    background: Hsla,
    border_hover: Hsla,
    background_hover: Hsla,
    foreground: Hsla,
}

fn paint(state: IndicatorState, disabled: bool, cx_tokens: &rustryx_theme::Tokens) -> Paint {
    let c = &cx_tokens.color;
    if state.is_on() {
        let hover = mix_srgb(c.accent, c.tint_hover, 0.15);
        Paint {
            border: if disabled { c.border } else { c.accent },
            background: c.accent,
            border_hover: if disabled { c.border } else { hover },
            background_hover: if disabled { c.accent } else { hover },
            foreground: c.on_accent,
        }
    } else if disabled {
        Paint {
            border: c.border,
            background: c.background_muted,
            border_hover: c.border,
            background_hover: c.background_muted,
            foreground: c.accent,
        }
    } else {
        Paint {
            border: c.border_emphasized,
            background: c.background_surface,
            border_hover: mix_srgb(c.border_emphasized, c.tint_hover, 0.2),
            background_hover: mix_srgb(c.background_surface, c.tint_hover, 0.05),
            foreground: c.accent,
        }
    }
}

/// A theme's picture for the choice indicator `name`, drawn in place of the built-in
/// (upstream `useIndicator`: the theme's override, else the default).
/// Without one, the request comes back, with the children the built-in draws.
fn themed(
    name: &str,
    request: IndicatorRequest,
    window: &mut Window,
    cx: &mut App,
) -> Result<AnyElement, IndicatorRequest> {
    match theme_indicator(name, cx) {
        Some(draw) => Ok(draw(request, window, cx)),
        None => Err(request),
    }
}

/// The state word of the `singleSelection` and `multiSelection` families.
fn family_state(state: IndicatorState) -> &'static str {
    match state {
        IndicatorState::Unchecked => "unchecked",
        IndicatorState::Checked => "checked",
        IndicatorState::Indeterminate => "indeterminate",
    }
}

macro_rules! indicator_common {
    ($name:ident) => {
        impl $name {
            rustryx_primitives::setters! {
                /// Upstream `size`.
                size: ChoiceSize,
                /// Upstream `isDisabled`: half opacity, no hover.
                !disabled,
                /// The owner row's GPUI group: hovering the row tints the
                /// indicator (upstream `indicatorScope`).
                ?hover_group: SharedString,
            }
        }

        impl $name {
            /// Draws a theme's `name` entry, if it has one; else keeps the children.
            fn themed(
                &mut self,
                name: &str,
                window: &mut Window,
                cx: &mut App,
            ) -> Option<AnyElement> {
                let request = IndicatorRequest {
                    state: family_state(self.state),
                    size: self.size.box_size(),
                    control_size: self.size,
                    disabled: self.disabled,
                    hover_group: self.hover_group.clone(),
                    children: std::mem::take(&mut self.children).into_vec(),
                };
                match themed(name, request, window, cx) {
                    Ok(themed) => Some(themed),
                    Err(request) => {
                        self.children = SmallVec::from_vec(request.children);
                        None
                    }
                }
            }
        }

        impl Styled for $name {
            fn style(&mut self) -> &mut StyleRefinement {
                &mut self.style
            }
        }

        impl ParentElement for $name {
            /// Upstream `children`: content replacing the mark, such as a
            /// busy spinner.
            fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
                self.children.extend(elements);
            }
        }
    };
}

/// The checkbox box, upstream `CheckboxIndicator`.
#[derive(IntoElement)]
pub struct CheckboxIndicator {
    state: IndicatorState,
    size: ChoiceSize,
    disabled: bool,
    hover_group: Option<SharedString>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
}

impl CheckboxIndicator {
    pub fn new(state: impl Into<IndicatorState>) -> Self {
        CheckboxIndicator {
            state: state.into(),
            size: ChoiceSize::Md,
            disabled: false,
            hover_group: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }
}

indicator_common!(CheckboxIndicator);

impl RenderOnce for CheckboxIndicator {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // A theme's `checkbox` entry draws instead, in every host.
        if let Some(themed) = self.themed(CHECKBOX, window, cx) {
            return themed;
        }
        let theme = current(window, cx);
        let t = theme.tokens();
        let p = paint(self.state, self.disabled, t);
        let size = self.size.box_size();
        let mut el = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(size)
            .border(t.border.width)
            .rounded(t.radius.inner)
            .border_color(p.border)
            .bg(p.background)
            .text_color(p.foreground);
        if self.disabled {
            el = el.opacity(0.5);
        } else if let Some(group) = self.hover_group.clone() {
            let (border, background) = (p.border_hover, p.background_hover);
            el = el.group_hover(group, move |s| s.border_color(border).bg(background));
        }
        let props = [
            ("size", self.size.as_str()),
            ("checked", self.state.as_theme_value()),
            ("disabled", if self.disabled { "disabled" } else { "" }),
        ];
        theme.refine(el.style(), "checkbox-indicator", &props, None);
        el.style().refine(&self.style);
        if !self.children.is_empty() {
            return el.children(self.children).into_any_element();
        }
        // Mark sizes are shares of the outer box (upstream: 12/20, 14/24
        // for the check; half for the dash).
        let (check, dash) = match self.size {
            ChoiceSize::Sm => (px(12.), px(10.)),
            ChoiceSize::Md => (px(14.), px(12.)),
        };
        match self.state {
            IndicatorState::Checked => el.child(
                svg()
                    .data(CHECK_SVG)
                    .size(check)
                    .flex_none()
                    .text_color(p.foreground),
            ),
            IndicatorState::Indeterminate => {
                el.child(div().w(dash).h(px(2.)).rounded_full().bg(t.color.on_accent))
            }
            IndicatorState::Unchecked => el,
        }
        .into_any_element()
    }
}

/// The radio circle, upstream `RadioIndicator`.
#[derive(IntoElement)]
pub struct RadioIndicator {
    state: IndicatorState,
    size: ChoiceSize,
    disabled: bool,
    hover_group: Option<SharedString>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
}

impl RadioIndicator {
    pub fn new(checked: bool) -> Self {
        RadioIndicator {
            state: checked.into(),
            size: ChoiceSize::Md,
            disabled: false,
            hover_group: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }
}

indicator_common!(RadioIndicator);

impl RenderOnce for RadioIndicator {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // A theme's `radio` entry draws instead, in every host.
        if let Some(themed) = self.themed(RADIO, window, cx) {
            return themed;
        }
        let theme = current(window, cx);
        let t = theme.tokens();
        let p = paint(self.state, self.disabled, t);
        let mut el = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(self.size.box_size())
            .border(t.border.width)
            .rounded_full()
            .border_color(p.border)
            .bg(p.background)
            .text_color(p.foreground);
        if self.disabled {
            el = el.opacity(0.5);
        } else if let Some(group) = self.hover_group.clone() {
            let (border, background) = (p.border_hover, p.background_hover);
            el = el.group_hover(group, move |s| s.border_color(border).bg(background));
        }
        let props = [
            ("size", self.size.as_str()),
            ("checked", if self.state.is_on() { "checked" } else { "" }),
            ("disabled", if self.disabled { "disabled" } else { "" }),
        ];
        theme.refine(el.style(), "radio-indicator", &props, None);
        el.style().refine(&self.style);
        if !self.children.is_empty() {
            return el.children(self.children).into_any_element();
        }
        // The dot: 8/20 and 10/24 of the circle.
        let dot = match self.size {
            ChoiceSize::Sm => px(8.),
            ChoiceSize::Md => px(10.),
        };
        el.when(self.state.is_on(), |el| {
            el.child(div().size(dot).rounded_full().bg(t.color.on_accent))
        })
        .into_any_element()
    }
}

/// The check mark of a chosen option, upstream `CheckIndicator` (menu and
/// selector rows). Nothing shows while unchecked.
#[derive(IntoElement)]
pub struct CheckIndicator {
    state: IndicatorState,
    size: ChoiceSize,
    disabled: bool,
    hover_group: Option<SharedString>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
}

impl CheckIndicator {
    pub fn new(checked: bool) -> Self {
        CheckIndicator {
            state: checked.into(),
            size: ChoiceSize::Md,
            disabled: false,
            hover_group: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }
}

indicator_common!(CheckIndicator);

impl RenderOnce for CheckIndicator {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // A theme's `check` entry draws instead, in every host.
        if let Some(themed) = self.themed(CHECK, window, cx) {
            return themed;
        }
        let theme = current(window, cx);
        let t = theme.tokens();
        if !self.children.is_empty() {
            let color = if self.disabled {
                t.color.icon_disabled
            } else {
                t.color.accent
            };
            let mut el = div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(gpui::rems(1.))
                .text_color(color);
            el.style().refine(&self.style);
            return el.children(self.children).into_any_element();
        }
        if !self.state.is_on() {
            return div().into_any_element();
        }
        let mut icon = Icon::new("check")
            .size(IconSize::Sm)
            .color(if self.disabled {
                IconColor::Disabled
            } else {
                IconColor::Accent
            });
        icon.style().refine(&self.style);
        icon.into_any_element()
    }
}

// ----------------------------------------------------------------------------
// Docs

pub const INDICATOR_DOC: ComponentDoc = ComponentDoc {
    name: "Indicator",
    rust_type: "rustryx::components::indicator::{CheckboxIndicator, RadioIndicator, CheckIndicator}",
    props: &[
        PropMapping {
            prop: "state",
            rust: RustApi::Argument("state"),
        },
        PropMapping {
            prop: "size",
            rust: RustApi::Method("size"),
        },
        PropMapping {
            prop: "isDisabled",
            rust: RustApi::Method("disabled"),
        },
        PropMapping {
            prop: "children",
            rust: RustApi::Trait("ParentElement"),
        },
    ],
    example: include_str!("../choice/examples.rs"),
};
