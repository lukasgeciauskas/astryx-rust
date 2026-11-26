//! Collapsible and CollapsibleGroup. Upstream: `Collapsible/`.
//!
//! With an `animated_layout` handle (FR10, AST-105) opening and closing reveal or conceal
//! the content from its block-end edge, the content after the Collapsible glides inside
//! the handle's container, and the chevron turns on that motion. A closing content stays
//! drawn, inert, until its exit ends. Without one the chevron turns on the theme's own
//! `--duration-fast` and the content shows or hides at once. Focus inside content that
//! closes moves to the trigger (AR4).

#[cfg(test)]
mod tests;

use std::rc::Rc;

use astryx_i18n::use_direction;
use astryx_primitives::{control_focus, is_focus_visible};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px, relative,
};
use smallvec::SmallVec;

use crate::animated_layout::adopt::{indicator_turn, reveal_content};
use crate::animated_layout::{AnimatedLayout, use_edge_reveal};
use crate::button::{SelectionMode, toggled};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconColor, IconSize};
use crate::item::ItemDensity;

/// Upstream `chevronPosition`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChevronPosition {
    /// Leading, pointing into the row; turns down when open.
    Start,
    /// Trailing, pointing down; flips up when open.
    #[default]
    End,
}

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type GroupToggle = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// What a group decides for its items.
struct GroupPart {
    open: bool,
    toggle: GroupToggle,
    density: Option<ItemDensity>,
    divided: bool,
    first: bool,
    chevron: ChevronPosition,
}

#[derive(Default)]
struct OpenState(Option<bool>);

/// What the Collapsible showed last frame, for AR4.
struct Shown(bool);

/// Content behind a trigger that shows and hides it, upstream `Collapsible`.
///
/// ```ignore
/// Collapsible::new("faq-1", "What is Astryx?").child(Text::new("A design system."))
/// ```
#[derive(IntoElement)]
pub struct Collapsible {
    id: ElementId,
    trigger: AnyElement,
    default_open: bool,
    open: Option<bool>,
    disabled: bool,
    on_open_change: Option<OpenFn>,
    chevron_position: Option<ChevronPosition>,
    value: Option<SharedString>,
    group: Option<GroupPart>,
    animated_layout: Option<AnimatedLayout>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl Collapsible {
    /// Upstream `trigger`: always visible; text, or any element.
    pub fn new(id: impl Into<ElementId>, trigger: impl IntoElement) -> Self {
        Collapsible {
            id: id.into(),
            trigger: trigger.into_any_element(),
            default_open: true,
            open: None,
            disabled: false,
            on_open_change: None,
            chevron_position: None,
            value: None,
            group: None,
            animated_layout: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `defaultIsOpen`: open at first, uncontrolled.
        default_open: bool,
        /// Upstream `isOpen`: controlled.
        ?open: bool,
        /// Upstream `isDisabled`: dimmed, out of the tab order.
        !disabled,
        /// Upstream `chevronPosition`.
        ?chevron_position: ChevronPosition,
        /// Upstream `value`: its key in a CollapsibleGroup.
        ?value: SharedString,
    }

    /// Upstream `onOpenChange`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `animatedLayout`: the handle from [`use_animated_layout`]. With it,
    /// opening and closing glide the content after this Collapsible, up to the handle's
    /// container, instead of jumping; the content reveals from its edge and the chevron
    /// turns on the same motion (FR10).
    ///
    /// [`use_animated_layout`]: crate::animated_layout::use_animated_layout
    pub fn animated_layout(mut self, layout: AnimatedLayout) -> Self {
        self.animated_layout = Some(layout);
        self
    }
}

impl Styled for Collapsible {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Collapsible {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Collapsible {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-open", self.id).into()),
            cx,
            |_, _| OpenState::default(),
        );
        let in_group = self.group.as_ref().zip(self.value.clone());
        let open = match (&in_group, self.open) {
            (Some((group, _)), _) => group.open,
            (None, Some(open)) => open,
            (None, None) => state.read(cx).0.unwrap_or(self.default_open),
        };
        let density = self.group.as_ref().and_then(|g| g.density);
        let position = self
            .chevron_position
            .or(self.group.as_ref().map(|g| g.chevron))
            .unwrap_or_default();

        let layout = self.animated_layout.clone();
        let chevron = {
            // A trailing chevron points down and turns half over; a leading one points
            // into the row (mirrored under RTL) and turns a quarter to point down.
            let turn = indicator_turn(
                format!("{:?}-chevron", self.id),
                open,
                layout.as_ref(),
                (t.duration.fast, t.ease.standard),
                window,
                cx,
            );
            use std::f32::consts::{FRAC_PI_2, PI};
            let (icon, rotation) = match position {
                ChevronPosition::End => ("chevronDown", PI * turn),
                ChevronPosition::Start if dir.is_rtl() => ("chevronLeft", -FRAC_PI_2 * turn),
                ChevronPosition::Start => ("chevronRight", FRAC_PI_2 * turn),
            };
            let icon = Icon::new(icon)
                .size(IconSize::Sm)
                .color(IconColor::Secondary)
                .rotation(rotation);
            let size = t.text.large.size;
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(size)
                .when(position == ChevronPosition::Start, |el| el.mr(t.spacing.s2))
                .child(icon)
        };
        let label = div().flex_grow(1.).min_w_0().child(self.trigger);

        let focus = control_focus(&self.id, window, cx).tab_stop(!self.disabled);
        let pad = |d: Option<ItemDensity>| match d {
            Some(ItemDensity::Compact) => Some(t.spacing.s1),
            Some(ItemDensity::Balanced) => Some(t.spacing.s2),
            Some(ItemDensity::Spacious) => Some(t.spacing.s3),
            None => None,
        };
        let mut trigger = div()
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .text_size(t.text.large.size)
            .line_height(relative(t.text.large.leading))
            .font_weight(t.font.weight.semibold)
            .text_color(t.color.text_primary);
        if let Some(p) = pad(density) {
            trigger = trigger.py(p);
        }
        trigger = match position {
            ChevronPosition::Start => trigger.child(chevron).child(label),
            ChevronPosition::End => trigger.child(label).child(chevron),
        };
        theme.refine(
            trigger.style(),
            "collapsible-trigger",
            &[
                ("density", density.map_or("", |d| d.as_str())),
                ("open", if open { "open" } else { "" }),
                ("disabled", if self.disabled { "disabled" } else { "" }),
            ],
            None,
        );
        let mut trigger = trigger
            .id(ElementId::Name(format!("{:?}-trigger", self.id).into()))
            .track_focus(&focus);
        if self.disabled {
            trigger = trigger.opacity(0.5);
        } else {
            let pressed = t.color.overlay_pressed;
            let controlled = self.open.is_some();
            let on_open_change = self.on_open_change.clone();
            let group_toggle = in_group.map(|(g, v)| (g.toggle.clone(), v));
            let state = state.clone();
            trigger = trigger
                .cursor_pointer()
                .active(move |s| s.bg(pressed))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some((toggle, value)) = &group_toggle {
                        toggle(value, window, cx);
                        return;
                    }
                    if !controlled {
                        state.update(cx, |s, _| s.0 = Some(!open));
                    }
                    if let Some(f) = &on_open_change {
                        f(!open, window, cx);
                    }
                    window.refresh();
                });
        }
        if is_focus_visible(&focus, window) {
            trigger = trigger.child(focus_ring(t, px(0.), rem));
        }
        let trigger = trigger
            .role(Role::Button)
            .aria_expanded(open)
            .aria_disabled(self.disabled);

        // FR10: the content reveals and conceals from its block-end edge on the handle's
        // motion, and a closing content is held, inert, until it is gone.
        let reveal = use_edge_reveal(
            ElementId::Name(format!("{:?}-content", self.id).into()),
            open,
            layout.as_ref(),
            window,
            cx,
        );
        let content_focus = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-content-focus", self.id).into()),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        // AR4: focus inside content that closes, or that its exit holds, goes to the
        // trigger, however it closed; focus anywhere else stays.
        let shown = window.use_keyed_state(
            ElementId::Name(format!("{:?}-shown", self.id).into()),
            cx,
            |_, _| Shown(open),
        );
        let was_open = shown.update(cx, |s, _| std::mem::replace(&mut s.0, open));
        if !open && (was_open || reveal.mounted) && content_focus.contains_focused(window, cx) {
            let trigger_focus = focus.clone();
            window.defer(cx, move |window, cx| trigger_focus.focus(window, cx));
        }
        let content = reveal.mounted.then(|| {
            let mut content = div()
                .track_focus(&content_focus)
                // A press on the content's own surface leaves focus where it is.
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.prevent_default()
                })
                .pt(t.spacing.s1)
                .text_size(t.text.body.size)
                .line_height(relative(t.text.body.leading))
                .font_weight(t.text.body.weight)
                .text_color(t.color.text_primary)
                .child(div().children(self.children));
            if let Some(p) = pad(density) {
                content = content.pb(p);
            }
            theme.refine(
                content.style(),
                "collapsible-content",
                &[
                    ("density", density.map_or("", |d| d.as_str())),
                    ("open", if open { "open" } else { "" }),
                ],
                None,
            );
            content
        });
        let content = content.and_then(|c| reveal_content(reveal, c, !open, window, cx));

        let divided = self.group.as_ref().is_some_and(|g| g.divided);
        let first = self.group.as_ref().is_none_or(|g| g.first);
        let mut el = div().w_full();
        if divided && !first {
            el = el.border_t(t.border.width).border_color(t.color.border);
        }
        theme.refine(
            el.style(),
            "collapsible",
            &[
                ("density", density.map_or("", |d| d.as_str())),
                ("divided", if divided { "divided" } else { "" }),
            ],
            None,
        );
        el.style().refine(&self.style);
        el.id(self.id).child(trigger).children(content)
    }
}

// ----------------------------------------------------------------------------
// CollapsibleGroup

type ChangeFn = Rc<dyn Fn(Vec<SharedString>, &mut Window, &mut App)>;

#[derive(Default)]
struct GroupState(Option<Vec<SharedString>>);

/// Collapsibles that open together, upstream `CollapsibleGroup`.
///
/// ```ignore
/// CollapsibleGroup::new("faq", SelectionMode::Single)
///     .dividers(true)
///     .item(Collapsible::new("q1", "Question one").value("q1").child(..))
/// ```
#[derive(IntoElement)]
pub struct CollapsibleGroup {
    id: ElementId,
    mode: SelectionMode,
    default_value: Vec<SharedString>,
    value: Option<Vec<SharedString>>,
    on_change: Option<ChangeFn>,
    dividers: bool,
    density: Option<ItemDensity>,
    chevron_position: ChevronPosition,
    items: Vec<Collapsible>,
    style: StyleRefinement,
}

impl CollapsibleGroup {
    /// Upstream `type`: one item open at a time, or any.
    pub fn new(id: impl Into<ElementId>, mode: SelectionMode) -> Self {
        CollapsibleGroup {
            id: id.into(),
            mode,
            default_value: Vec::new(),
            value: None,
            on_change: None,
            dividers: false,
            density: None,
            chevron_position: ChevronPosition::End,
            items: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `hasDividers`: hairlines between items, balanced padding.
        !dividers,
        /// Upstream `density`: row padding; balanced with dividers.
        ?density: ItemDensity,
        /// Upstream `chevronPosition`: for every item.
        chevron_position: ChevronPosition,
    }

    /// Upstream `defaultValue`: the items open at first (uncontrolled).
    pub fn default_value(
        mut self,
        value: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        self.default_value = value.into_iter().map(Into::into).collect();
        self
    }

    /// Upstream `value`: the open items (controlled).
    pub fn value(mut self, value: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.value = Some(value.into_iter().map(Into::into).collect());
        self
    }

    /// Upstream `onChange`: the next open items.
    pub fn on_change(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `children`: one item; give it a `value`.
    pub fn item(mut self, item: Collapsible) -> Self {
        self.items.push(item);
        self
    }
}

impl Styled for CollapsibleGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for CollapsibleGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-group", self.id).into()),
            cx,
            |_, _| GroupState::default(),
        );
        let open: Rc<[SharedString]> = match &self.value {
            Some(value) => value.clone().into(),
            None => state
                .read(cx)
                .0
                .clone()
                .unwrap_or_else(|| self.default_value.clone())
                .into(),
        };
        let controlled = self.value.is_some();
        let mode = self.mode;
        let on_change = self.on_change.clone();
        let current = open.clone();
        let toggle: GroupToggle = Rc::new(move |value, window, cx| {
            let next = toggled(mode, &current, value);
            if !controlled {
                state.update(cx, |s, _| s.0 = Some(next.clone()));
            }
            if let Some(f) = &on_change {
                f(next, window, cx);
            }
            window.refresh();
        });
        let density = self
            .density
            .or(self.dividers.then_some(ItemDensity::Balanced));
        let items = self.items.into_iter().enumerate().map(|(i, mut item)| {
            item.group = Some(GroupPart {
                open: item.value.as_ref().is_some_and(|v| open.contains(v)),
                toggle: toggle.clone(),
                density,
                divided: self.dividers,
                first: i == 0,
                chevron: self.chevron_position,
            });
            item
        });
        let mut el = div().flex().flex_col().w_full();
        el.style().refine(&self.style);
        el.id(self.id).children(items)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const NO_REFS: &str = "GPUI elements have no refs";
const TEST_IDS: &str = "GPUI tests find elements by id";

pub const COLLAPSIBLE_DOC: ComponentDoc = ComponentDoc {
    name: "Collapsible",
    rust_type: "astryx::components::collapsible::Collapsible",
    props: &[
        p("trigger", RustApi::Argument("trigger")),
        p("children", RustApi::Trait("ParentElement")),
        p("defaultIsOpen", RustApi::Method("default_open")),
        p("isOpen", RustApi::Method("open")),
        p("isDisabled", RustApi::Method("disabled")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("chevronPosition", RustApi::Method("chevron_position")),
        p("value", RustApi::Method("value")),
        p("animatedLayout", RustApi::Method("animated_layout")),
        p("ref", RustApi::Waived(NO_REFS)),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
        p("data-testid", RustApi::Waived(TEST_IDS)),
    ],
    example: include_str!("../containers/examples.rs"),
};

pub const COLLAPSIBLE_GROUP_DOC: ComponentDoc = ComponentDoc {
    name: "CollapsibleGroup",
    rust_type: "astryx::components::collapsible::CollapsibleGroup",
    props: &[
        p("type", RustApi::Argument("mode")),
        p("defaultValue", RustApi::Method("default_value")),
        p("value", RustApi::Method("value")),
        p("onChange", RustApi::Method("on_change")),
        p("hasDividers", RustApi::Method("dividers")),
        p("density", RustApi::Method("density")),
        p("chevronPosition", RustApi::Method("chevron_position")),
        p("children", RustApi::Method("item")),
        p("ref", RustApi::Waived(NO_REFS)),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
        p("data-testid", RustApi::Waived(TEST_IDS)),
    ],
    example: include_str!("../containers/examples.rs"),
};
