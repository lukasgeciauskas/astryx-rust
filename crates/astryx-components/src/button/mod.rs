//! Buttons: Button, IconButton, ButtonGroup, ToggleButton and
//! ToggleButtonGroup. Upstream: `Button/`, `IconButton/`, `ButtonGroup/`,
//! `ToggleButton/`.

pub mod examples;
mod toggle;

#[cfg(test)]
mod tests;

pub(crate) use toggle::toggled;
pub use toggle::{SelectionMode, ToggleButton, ToggleButtonGroup};

use std::rc::Rc;
use std::time::Duration;

use web_time::Instant;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{
    ElementSize, Politeness, announce, control_focus, prefers_reduced_motion, provide,
    resolve_size, use_context, use_link_handler,
};
use astryx_theme::{ResolvedTheme, ThemeStyled, Tokens, current};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, Hsla, IntoElement, Length, ParentElement, Pixels,
    RenderOnce, Role, SharedString, StyleRefinement, Styled, Task, Window, div, prelude::*, px,
};
use smallvec::SmallVec;

use crate::containers::Elevation;
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{IconDefaultSize, IconSize};
use crate::layout::Orientation;
use crate::press::{ClickFn, Press, apply_press};
use crate::status::{Spinner, SpinnerShade, SpinnerSize};

/// Upstream `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// The single most important action.
    Primary,
    #[default]
    Secondary,
    Ghost,
    Destructive,
}

impl ButtonVariant {
    /// (fill, foreground). Ghost has no fill.
    pub fn colors(self, t: &Tokens) -> (Option<Hsla>, Hsla) {
        let c = &t.color;
        match self {
            ButtonVariant::Primary => (Some(c.accent), c.on_accent),
            ButtonVariant::Secondary => (Some(c.neutral), c.text_primary),
            ButtonVariant::Ghost => (None, c.text_primary),
            ButtonVariant::Destructive => (Some(c.error), c.on_error),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ButtonVariant::Primary => "primary",
            ButtonVariant::Secondary => "secondary",
            ButtonVariant::Ghost => "ghost",
            ButtonVariant::Destructive => "destructive",
        }
    }

    /// Whether the fill alone separates it from the page (upstream's solid
    /// variants, whose group dividers use the on-fill color).
    fn solid(self) -> bool {
        matches!(self, ButtonVariant::Primary | ButtonVariant::Destructive)
    }
}

/// The fill a solid Button gives its content, upstream `--_button-fill` and
/// `--_button-on-fill`: a neutral Badge inside inverts the pair (FR2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonFill {
    pub fill: Hsla,
    pub on_fill: Hsla,
}

/// A button's place in a [`ButtonGroup`], upstream `ButtonGroupContext`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GroupMember {
    pub orientation: Orientation,
    pub first: bool,
    pub last: bool,
    pub disabled: bool,
}

/// The element size token for a size.
pub(crate) fn element_height(size: ElementSize, t: &Tokens) -> gpui::AbsoluteLength {
    match size {
        ElementSize::Sm => t.size.element_sm,
        ElementSize::Md => t.size.element_md,
        ElementSize::Lg => t.size.element_lg,
    }
}

/// Upstream FR3: an unsized icon is `sm` in sm and md buttons, `md` in lg.
fn icon_size_for(size: ElementSize) -> IconSize {
    match size {
        ElementSize::Lg => IconSize::Md,
        _ => IconSize::Sm,
    }
}

/// Upstream `SPINNER_DELAY`: a pending action shows its spinner after
/// `--duration-medium-min`, so a fast action never flashes one.
fn spinner_delay(t: &Tokens) -> Duration {
    t.duration.medium_min
}

/// Whether the spinner shows (upstream FR8): at once while loading or,
/// under reduced motion, while a fire-once action is pending; else once a
/// pending action has run for `delay`, so a fast one never flashes it.
pub(crate) fn spinner_revealed(
    loading: bool,
    pending: bool,
    interruptible: bool,
    delay: Duration,
    elapsed: Option<Duration>,
) -> bool {
    loading
        || (pending && !interruptible && delay.is_zero())
        || (pending && elapsed.is_some_and(|elapsed| elapsed >= delay))
}

type ActionFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> Task<()>>;

#[derive(Default)]
struct ButtonState {
    pending: usize,
    pending_since: Option<Instant>,
    reveal_scheduled: bool,
}

/// An action trigger, upstream `Button`.
///
/// ```ignore
/// Button::new("save", "Save changes").variant(ButtonVariant::Primary).on_click(|_, _, _| {})
/// ```
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    size: Option<ElementSize>,
    elevation: Elevation,
    loading: bool,
    interruptible: bool,
    disabled: bool,
    icon: Option<AnyElement>,
    icon_only: bool,
    width: Option<Length>,
    end_content: Option<AnyElement>,
    tooltip: Option<SharedString>,
    on_click: Option<ClickFn>,
    click_action: Option<ActionFn>,
    href: Option<SharedString>,
    target: Option<SharedString>,
    pressed: Option<bool>,
    expanded: Option<bool>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
}

impl Button {
    /// Upstream `label`: the visible text, and the accessible name.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Button {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::Secondary,
            size: None,
            elevation: Elevation::None,
            loading: false,
            interruptible: false,
            disabled: false,
            icon: None,
            icon_only: false,
            width: None,
            end_content: None,
            tooltip: None,
            on_click: None,
            click_action: None,
            href: None,
            target: None,
            pressed: None,
            expanded: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `variant`.
        variant: ButtonVariant,
        /// Upstream `size`: else the nearest size provider, else `md`.
        ?size: ElementSize,
        /// Upstream `elevation`: a resting shadow, ignored in a group.
        elevation: Elevation,
        /// Upstream `isLoading`: a spinner; stays focusable but inert.
        !loading,
        /// Upstream `isInterruptible`: stays operable while a `click_action` runs.
        !interruptible,
        /// Upstream `isDisabled`: focusable, half opacity, never activates.
        !disabled,
        /// Upstream `isIconOnly`: a square button; the label becomes its name.
        !icon_only,
        /// Upstream `width`: else the content's width, capped at the container.
        ?width: Length,
        /// Upstream `tooltip`. The tooltip itself lands with Tooltip (M5).
        ?tooltip: SharedString,
        /// Upstream `href`: a link through the link handler.
        ?href: SharedString,
        /// Upstream `target`, with `href`.
        ?target: SharedString,
    }

    /// Upstream `icon`: before the label; an unsized Icon is `sm` (`md` in lg).
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `endContent`: a trailing Icon or Badge.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `onClick`: runs first. Calling `window.prevent_default()`
    /// in it skips `click_action`.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `clickAction`: async work; the button is busy until the
    /// returned task finishes.
    pub fn click_action(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.click_action = Some(Rc::new(f));
        self
    }

    /// Upstream `aria-expanded`: whether the region this button discloses is
    /// open, for disclosure triggers.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    /// A toggle button's pressed state: the pressed overlay and weight, and
    /// `aria-pressed`. Used by [`ToggleButton`].
    pub(crate) fn pressed(mut self, pressed: bool) -> Self {
        self.pressed = Some(pressed);
        self
    }

    /// The styled root before content and behavior, for tests.
    pub(crate) fn root(
        &self,
        theme: &ResolvedTheme,
        size: ElementSize,
        group: Option<GroupMember>,
        dir: astryx_i18n::Direction,
        rem: Pixels,
    ) -> gpui::Div {
        let t = theme.tokens();
        let (fill, fg) = self.variant.colors(t);
        let height = element_height(size, t);
        let mut el = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(t.spacing.s2)
            .h(height)
            .min_w_0()
            .rounded(t.radius.element)
            .text_color(fg)
            .text_style(&t.text.label)
            .font_weight(t.font.weight.medium)
            .whitespace_nowrap();
        // A label taller than the element size grows the button (FR4).
        let label_line = t.text.label.size.to_pixels(rem) * t.text.label.leading;
        el.style().min_size.height = Some(label_line.into());
        if let Some(fill) = fill {
            el = el.bg(fill);
        }
        if self.icon_only {
            // Square from its height, so a height override stays square.
            el = el.aspect_ratio(1.).flex_none();
        } else {
            el = el.px(t.spacing.s3).flex_shrink(1.).max_w_full();
        }
        if let Some(width) = self.width {
            el = el.w(width);
        }
        if self.pressed == Some(true) {
            el = el
                .bg(astryx_theme::color::composite(
                    fill.unwrap_or(Hsla::transparent_black()),
                    t.color.overlay_pressed,
                ))
                .font_weight(t.font.weight.semibold);
        }
        match group {
            None => el = self.elevation.apply(el, t),
            Some(member) => {
                let radius = t.radius.element;
                let divider = if self.variant.solid() {
                    t.color.on_accent
                } else {
                    t.color.border
                };
                el = el.rounded(px(0.));
                let (start, end) = (member.first, member.last);
                match member.orientation {
                    Orientation::Horizontal => {
                        if start {
                            el = el.rounded_start(dir, radius);
                        } else {
                            el = el.border_start(dir, t.border.width).border_color(divider);
                        }
                        if end {
                            el = el.rounded_end(dir, radius);
                        }
                    }
                    Orientation::Vertical => {
                        if start {
                            el = el.rounded_t(radius);
                        } else {
                            el = el.border_t(t.border.width).border_color(divider);
                        }
                        if end {
                            el = el.rounded_b(radius);
                        }
                    }
                }
            }
        }
        let elevation = if group.is_some() {
            Elevation::None
        } else {
            self.elevation
        };
        theme.refine(
            el.style(),
            "button",
            &[
                ("variant", self.variant.as_str()),
                ("size", size.as_str()),
                ("elevation", elevation.as_str()),
            ],
            None,
        );
        el.style().refine(&self.style);
        el
    }
}

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let rem = window.rem_size();
        let group = use_context::<GroupMember>(cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let (fill, fg) = self.variant.colors(t);
        let disabled = self.disabled || group.is_some_and(|g| g.disabled);

        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ButtonState::default());
        let (pending, since) = {
            let s = state.read(cx);
            (s.pending > 0, s.pending_since)
        };
        // Explicit loading shows at once; a pending action after a delay
        // (none under reduced motion).
        let delay = if prefers_reduced_motion(cx) {
            Duration::ZERO
        } else {
            spinner_delay(t)
        };
        // The executor's clock, which tests can advance.
        let now = cx.background_executor().now();
        let elapsed = since.map(|since| now.saturating_duration_since(since));
        let revealed = spinner_revealed(self.loading, pending, self.interruptible, delay, elapsed);
        if pending && !revealed && !state.read(cx).reveal_scheduled {
            state.update(cx, |s, _| s.reveal_scheduled = true);
            let wait = delay.saturating_sub(elapsed.unwrap_or_default());
            let state = state.clone();
            window
                .spawn(cx, async move |cx| {
                    cx.background_executor().timer(wait).await;
                    cx.update(|window, cx| {
                        state.update(cx, |s, _| s.reveal_scheduled = false);
                        window.refresh();
                    })
                    .ok();
                })
                .detach();
        }
        let busy = (self.loading || pending) && !self.interruptible;
        let focus = control_focus(&self.id, window, cx);

        let mut el = self.root(&theme, size, group, dir, rem);
        // The overlays composite over the resting fill as finally styled,
        // theme and caller overrides included (upstream layers them as a
        // background image over whatever the background is).
        let resting = match el.style().background.as_ref() {
            Some(gpui::Fill::Color(bg)) => bg.as_solid(),
            None => fill,
        };
        if revealed {
            // Content keeps its place but turns clear (FR8).
            el = el.text_color(Hsla::transparent_black());
        }
        let icon_size = icon_size_for(size);
        let label_text = (!self.icon_only).then(|| {
            if self.children.is_empty() {
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(self.label.clone())
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.s2)
                    .min_w_0()
                    .children(self.children)
                    .into_any_element()
            }
        });
        let content = div()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .min_w_0()
            .children(
                self.icon
                    .map(|icon| provide(IconDefaultSize(icon_size), icon)),
            )
            .children(label_text)
            .children(self.end_content.filter(|_| !self.icon_only));
        let content = match fill {
            Some(fill) if self.variant.solid() => {
                provide(ButtonFill { fill, on_fill: fg }, content).into_any_element()
            }
            _ => content.into_any_element(),
        };
        let spinner = revealed.then(|| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(fg)
                .child(
                    Spinner::new("spinner")
                        .size(SpinnerSize::Sm)
                        .shade(SpinnerShade::Inherit),
                )
        });

        // Activation: onClick, then (unless it prevented default) the
        // action, fire-once unless interruptible (FR6, FR7).
        let on_click = self.on_click.clone();
        let action = self.click_action.clone();
        let interruptible = self.interruptible;
        let link = self
            .href
            .clone()
            .filter(|_| !disabled)
            .map(|href| (href, self.target.clone(), use_link_handler(cx)));
        let loading_text = i18n(cx).t("@astryx.button.loading");
        let activation_state = state.clone();
        let activate: ClickFn = Rc::new(move |event, window, cx| {
            if let Some(f) = &on_click {
                f(event, window, cx);
                if window.default_prevented() {
                    return;
                }
            }
            if let Some((href, target, handler)) = &link {
                handler.open(href, target.clone(), window, cx);
            }
            let Some(action) = &action else { return };
            if activation_state.read(cx).pending > 0 && !interruptible {
                return;
            }
            let task = action(event, window, cx);
            let now = cx.background_executor().now();
            activation_state.update(cx, |s, _| {
                s.pending += 1;
                s.pending_since.get_or_insert(now);
            });
            announce(loading_text.clone(), Politeness::Polite, window, cx);
            let state = activation_state.clone();
            window
                .spawn(cx, async move |cx| {
                    task.await;
                    cx.update(|window, cx| {
                        state.update(cx, |s, _| {
                            s.pending = s.pending.saturating_sub(1);
                            if s.pending == 0 {
                                s.pending_since = None;
                            }
                        });
                        window.refresh();
                    })
                    .ok();
                })
                .detach();
        });

        let variant = self.variant;
        let props = [("variant", variant.as_str()), ("size", size.as_str())];
        let ring_color = (variant == ButtonVariant::Destructive).then_some(t.color.error);
        let radius = t.radius.element.to_pixels(rem);
        let el = el.id(self.id.clone()).child(content).children(spinner);
        let el = apply_press(
            el,
            &focus,
            Press {
                fill: resting,
                scale: group.is_none().then_some(0.98),
                disabled,
                busy,
                ring: Some((radius, ring_color)),
                theme_target: Some(("button", &props)),
            },
            Some(activate),
            &theme,
            window,
            cx,
        );
        let role = if self.href.is_some() && !disabled {
            Role::Link
        } else {
            Role::Button
        };
        let mut el = el.role(role).aria_label(self.label);
        if let Some(pressed) = self.pressed {
            el = el.aria_toggled(if pressed {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            });
        }
        if let Some(expanded) = self.expanded {
            el = el.aria_expanded(expanded);
        }
        el
    }
}

/// A button showing only an icon, upstream `IconButton`.
#[derive(IntoElement)]
pub struct IconButton(Button);

impl IconButton {
    /// Upstream `label` (its accessible name) and `icon`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        icon: impl IntoElement,
    ) -> Self {
        IconButton(Button::new(id, label).icon(icon).icon_only(true))
    }

    /// Upstream `variant`.
    pub fn variant(self, variant: ButtonVariant) -> Self {
        Self(self.0.variant(variant))
    }
    /// Upstream `size`.
    pub fn size(self, size: ElementSize) -> Self {
        Self(self.0.size(size))
    }
    /// Upstream `elevation`.
    pub fn elevation(self, elevation: Elevation) -> Self {
        Self(self.0.elevation(elevation))
    }
    /// Upstream `isLoading`.
    pub fn loading(self, loading: bool) -> Self {
        Self(self.0.loading(loading))
    }
    /// Upstream `isDisabled`.
    pub fn disabled(self, disabled: bool) -> Self {
        Self(self.0.disabled(disabled))
    }
    /// Upstream `isInterruptible`.
    pub fn interruptible(self, interruptible: bool) -> Self {
        Self(self.0.interruptible(interruptible))
    }
    /// Upstream `tooltip`. The tooltip itself lands with Tooltip (M5).
    pub fn tooltip(self, tooltip: impl Into<SharedString>) -> Self {
        Self(self.0.tooltip(tooltip))
    }
    /// Upstream `width`: else square.
    pub fn width(self, width: impl Into<Length>) -> Self {
        Self(self.0.width(width))
    }
    /// Upstream `href`.
    pub fn href(self, href: impl Into<SharedString>) -> Self {
        Self(self.0.href(href))
    }
    /// Upstream `target`.
    pub fn target(self, target: impl Into<SharedString>) -> Self {
        Self(self.0.target(target))
    }
    /// Upstream `aria-expanded`, for disclosure triggers.
    pub fn expanded(self, expanded: bool) -> Self {
        Self(self.0.expanded(expanded))
    }
    /// Upstream `onClick`.
    pub fn on_click(self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        Self(self.0.on_click(f))
    }
    /// Upstream `clickAction`.
    pub fn click_action(
        self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        Self(self.0.click_action(f))
    }
}

impl Styled for IconButton {
    fn style(&mut self) -> &mut StyleRefinement {
        self.0.style()
    }
}

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.0
    }
}

/// Connected buttons with shared dividers and outer-only corners,
/// upstream `ButtonGroup`.
#[derive(IntoElement)]
pub struct ButtonGroup {
    id: ElementId,
    label: SharedString,
    orientation: Orientation,
    size: Option<ElementSize>,
    elevation: Elevation,
    disabled: bool,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 4]>,
}

impl ButtonGroup {
    /// Upstream `label`: the group's accessible name.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        ButtonGroup {
            id: id.into(),
            label: label.into(),
            orientation: Orientation::Horizontal,
            size: None,
            elevation: Elevation::None,
            disabled: false,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `orientation`.
        orientation: Orientation,
        /// Upstream `size`: given to every button.
        ?size: ElementSize,
        /// Upstream `elevation`: the group's shadow.
        elevation: Elevation,
        /// Upstream `isDisabled`: disables every button.
        !disabled,
    }
}

impl Styled for ButtonGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ButtonGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ButtonGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let mut el = match self.orientation {
            Orientation::Horizontal => div().flex_row_logical(dir),
            Orientation::Vertical => div().flex().flex_col(),
        };
        el = el.items_stretch().flex_none();
        if self.elevation != Elevation::None {
            el = self.elevation.apply(el.rounded(t.radius.element), t);
        }
        theme.refine(
            el.style(),
            "button-group",
            &[
                ("size", size.as_str()),
                ("orientation", self.orientation.as_str()),
                ("elevation", self.elevation.as_str()),
            ],
            None,
        );
        el.style().refine(&self.style);
        let count = self.children.len();
        let (orientation, disabled) = (self.orientation, self.disabled);
        let children = self
            .children
            .into_iter()
            .enumerate()
            .map(move |(i, child)| {
                let member = GroupMember {
                    orientation,
                    first: i == 0,
                    last: i + 1 == count,
                    disabled,
                };
                provide(member, provide(size, child))
            });
        el.id(self.id)
            .role(Role::Group)
            .aria_label(self.label)
            .children(children)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const NO_FORMS: &str = "GPUI has no HTML forms; handle submission in on_click";
const NO_REFS: &str = "GPUI elements have no refs";
const NO_LINK_COMPONENT: &str = "the link handler (link_provider) routes every href";
const NO_REL: &str = "GPUI opens URLs without a referrer";
const TOOLTIP: &str = "the tooltip lands with Tooltip in M5";
const TEST_IDS: &str = "GPUI tests find elements by id";

pub const BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "Button",
    rust_type: "astryx::components::button::Button",
    props: &[
        p("label", RustApi::Argument("label")),
        p("variant", RustApi::Method("variant")),
        p("size", RustApi::Method("size")),
        p("elevation", RustApi::Method("elevation")),
        p("type", RustApi::Waived(NO_FORMS)),
        p("name", RustApi::Waived(NO_FORMS)),
        p("value", RustApi::Waived(NO_FORMS)),
        p("form", RustApi::Waived(NO_FORMS)),
        p("isLoading", RustApi::Method("loading")),
        p("isInterruptible", RustApi::Method("interruptible")),
        p("isDisabled", RustApi::Method("disabled")),
        p("icon", RustApi::Method("icon")),
        p("isIconOnly", RustApi::Method("icon_only")),
        p("width", RustApi::Method("width")),
        p("children", RustApi::Trait("ParentElement")),
        p("endContent", RustApi::Method("end_content")),
        p("tooltip", RustApi::Pending("tooltip", TOOLTIP)),
        p("onClick", RustApi::Method("on_click")),
        p("clickAction", RustApi::Method("click_action")),
        p("href", RustApi::Method("href")),
        p("as", RustApi::Waived(NO_LINK_COMPONENT)),
        p("ref", RustApi::Waived(NO_REFS)),
        p("target", RustApi::Method("target")),
        p("rel", RustApi::Waived(NO_REL)),
    ],
    example: include_str!("examples.rs"),
};

pub const ICON_BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "IconButton",
    rust_type: "astryx::components::button::IconButton",
    props: &[
        p("label", RustApi::Argument("label")),
        p("icon", RustApi::Argument("icon")),
        p("variant", RustApi::Method("variant")),
        p("size", RustApi::Method("size")),
        p("elevation", RustApi::Method("elevation")),
        p("isLoading", RustApi::Method("loading")),
        p("isDisabled", RustApi::Method("disabled")),
        p("tooltip", RustApi::Pending("tooltip", TOOLTIP)),
        p("onClick", RustApi::Method("on_click")),
        p("clickAction", RustApi::Method("click_action")),
        p("isInterruptible", RustApi::Method("interruptible")),
        p("width", RustApi::Method("width")),
        p("type", RustApi::Waived(NO_FORMS)),
        p("name", RustApi::Waived(NO_FORMS)),
        p("value", RustApi::Waived(NO_FORMS)),
        p("form", RustApi::Waived(NO_FORMS)),
        p("href", RustApi::Method("href")),
        p("as", RustApi::Waived(NO_LINK_COMPONENT)),
        p("target", RustApi::Method("target")),
        p("rel", RustApi::Waived(NO_REL)),
    ],
    example: include_str!("examples.rs"),
};

pub const TOGGLE_BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "ToggleButton",
    rust_type: "astryx::components::button::ToggleButton",
    props: &[
        p("label", RustApi::Argument("label")),
        p("isPressed", RustApi::Method("pressed")),
        p("onPressedChange", RustApi::Method("on_pressed_change")),
        p(
            "pressedChangeAction",
            RustApi::Method("pressed_change_action"),
        ),
        p("size", RustApi::Method("size")),
        p("elevation", RustApi::Method("elevation")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isLoading", RustApi::Method("loading")),
        p("icon", RustApi::Method("icon")),
        p("isIconOnly", RustApi::Method("icon_only")),
        p("pressedIcon", RustApi::Method("pressed_icon")),
        p("children", RustApi::Trait("ParentElement")),
        p("tooltip", RustApi::Pending("tooltip", TOOLTIP)),
        p("value", RustApi::Method("value")),
        p("data-testid", RustApi::Waived(TEST_IDS)),
    ],
    example: include_str!("examples.rs"),
};

pub const TOGGLE_BUTTON_GROUP_DOC: ComponentDoc = ComponentDoc {
    name: "ToggleButtonGroup",
    rust_type: "astryx::components::button::ToggleButtonGroup",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("label", RustApi::Argument("label")),
        p("type", RustApi::Argument("mode")),
        p("value", RustApi::Method("value")),
        p("onChange", RustApi::Method("on_change")),
        p("orientation", RustApi::Method("orientation")),
        p("size", RustApi::Method("size")),
        p("isDisabled", RustApi::Method("disabled")),
        p("xstyle", RustApi::Trait("Styled")),
        p("data-testid", RustApi::Waived(TEST_IDS)),
    ],
    example: include_str!("examples.rs"),
};

pub const BUTTON_GROUP_DOC: ComponentDoc = ComponentDoc {
    name: "ButtonGroup",
    rust_type: "astryx::components::button::ButtonGroup",
    props: &[
        p("ref", RustApi::Waived(NO_REFS)),
        p("children", RustApi::Trait("ParentElement")),
        p("label", RustApi::Argument("label")),
        p("orientation", RustApi::Method("orientation")),
        p("size", RustApi::Method("size")),
        p("elevation", RustApi::Method("elevation")),
        p("isDisabled", RustApi::Method("disabled")),
    ],
    example: include_str!("examples.rs"),
};
