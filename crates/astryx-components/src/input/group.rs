//! InputGroup and InputGroupText. Upstream: `InputGroup/`.
//!
//! CSS joins a group's members with `:first-child` and `:last-child`; here
//! the group provides each direct child its place ([`GroupSlot`]), which
//! TextInput, NumberInput, InputGroupText and Button addons read. A member
//! clears the slot for its own insides, so a TextInput's clear button
//! keeps its shape. The group's disabled state ([`GroupDisabled`]) and name
//! ([`GroupContext`]) reach every descendant.

use astryx_primitives::{ElementSize, provide, resolve_size, use_context};
use astryx_theme::{ResolvedTheme, ThemeStyled, current};
use gpui::{
    AnyElement, App, Div, ElementId, Hsla, IntoElement, ParentElement, Pixels, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Window, deferred, div, prelude::*, px,
};
use smallvec::SmallVec;

use astryx_i18n::{Direction, LogicalStyled, use_direction};

use super::control::row_height;
use crate::field::{Field, InputStatus, StatusPlacement};

/// A direct child's place in the group row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GroupPlace {
    pub first: bool,
    pub last: bool,
}

/// The place of the element reading it, if it is a direct group member;
/// `GroupSlot(None)` inside a member.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GroupSlot(pub Option<GroupPlace>);

/// An enclosing group's disabled state (upstream
/// `InputGroupInternalContext`). A group never re-enables what disables
/// itself.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GroupDisabled(pub bool);

/// The group's name and description, which its inputs join to their own
/// (FR2, upstream `InputGroupContext`).
#[derive(Clone, Debug, Default)]
pub(crate) struct GroupContext {
    pub label: SharedString,
    pub described: Vec<SharedString>,
}

/// The place this element holds in a group, if it is a direct member.
pub(crate) fn group_place(cx: &App) -> Option<GroupPlace> {
    use_context::<GroupSlot>(cx).and_then(|slot| slot.0)
}

/// Whether an enclosing group is disabled.
pub(crate) fn group_disabled(cx: &App) -> bool {
    use_context::<GroupDisabled>(cx).is_some_and(|d| d.0)
}

pub(crate) fn group_context(cx: &App) -> Option<GroupContext> {
    use_context::<GroupContext>(cx)
}

/// Clears the member slot for a member's insides.
pub(crate) fn inside_member(child: impl IntoElement) -> impl IntoElement {
    provide(GroupSlot(None), child)
}

/// Joins a bordered member to its neighbours (upstream `groupStyles.inGroup`
/// and InputGroupText's corners): it overlaps the previous member's border,
/// and rounds only the group's outer corners.
pub(crate) fn join<E: Styled>(
    mut el: E,
    place: GroupPlace,
    radius: Pixels,
    border: Pixels,
    dir: Direction,
) -> E {
    el.style().corner_radii = Default::default();
    if !place.first {
        el = el.margin_start(dir, -border);
    } else {
        el = el.rounded_start(dir, radius);
    }
    if place.last {
        el = el.rounded_end(dir, radius);
    }
    el
}

/// The focus ring of a joined member, painted after its neighbours so the
/// next member's border never covers it (upstream `zIndex: 2`).
pub(crate) fn raised_ring(
    color: Hsla,
    border: Pixels,
    place: GroupPlace,
    radius: Pixels,
    dir: Direction,
) -> impl IntoElement {
    let mut ring = div()
        .absolute()
        .top(-border)
        .bottom(-border)
        .left(-border)
        .right(-border)
        .border(border)
        .border_color(color);
    if place.first {
        ring = ring.rounded_start(dir, radius);
    }
    if place.last {
        ring = ring.rounded_end(dir, radius);
    }
    deferred(ring).with_priority(1)
}

/// A group of one input with its prefix and suffix addons, joined into one
/// row, upstream `InputGroup`.
#[derive(IntoElement)]
pub struct InputGroup {
    id: ElementId,
    label: SharedString,
    label_hidden: bool,
    description: Option<SharedString>,
    disabled: bool,
    optional: bool,
    required: bool,
    size: Option<ElementSize>,
    status: Option<InputStatus>,
    label_tooltip: Option<SharedString>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 3]>,
}

impl InputGroup {
    /// Upstream `label`: the group's accessible name.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        InputGroup {
            id: id.into(),
            label: label.into(),
            label_hidden: false,
            description: None,
            disabled: false,
            optional: false,
            required: false,
            size: None,
            status: None,
            label_tooltip: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`: between the label and the row, and part
        /// of every grouped input's description.
        ?description: SharedString,
        /// Upstream `isDisabled`: disables every input and addon inside.
        !disabled,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `size`: the row's height and its inputs' size.
        ?size: ElementSize,
        /// Upstream `status`: its message shows below the row.
        ?status: InputStatus,
        /// Upstream `labelTooltip`. The InfoTip lands with Tooltip (M5).
        ?label_tooltip: SharedString,
    }
}

impl Styled for InputGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for InputGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for InputGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let disabled = self.disabled || group_disabled(cx);

        let mut described: Vec<SharedString> = self.description.iter().cloned().collect();
        described.extend(self.status.as_ref().and_then(|s| s.message.clone()));
        let context = GroupContext {
            label: self.label.clone(),
            described: described.clone(),
        };
        let count = self.children.len();
        let members = self.children.into_iter().enumerate().map(|(i, child)| {
            let place = GroupPlace {
                first: i == 0,
                last: i + 1 == count,
            };
            provide(GroupSlot(Some(place)), child)
        });

        let mut props = vec![("size", size.as_str())];
        if let Some(status) = &self.status {
            props.push(("status", status.status.as_str()));
        }
        let mut row = div()
            .id(ElementId::Name(format!("{:?}-row", self.id).into()))
            .inline_flex()
            .items_stretch()
            .h(row_height(size, t, rem))
            .min_w_0()
            .role(Role::Group)
            .aria_label(self.label.clone())
            .aria_disabled(disabled)
            .children(members);
        if !described.is_empty() {
            row = row.aria_description(SharedString::from(described.join(" ")));
        }
        if disabled {
            row = row.opacity(0.5);
        }
        theme.refine(row.style(), "input-group", &props, None);
        row.style().refine(&self.style);

        // The row's size, disabled state and name reach every member.
        let row = provide(
            size,
            provide(GroupDisabled(disabled), provide(context, row)),
        );
        let mut field = Field::new(self.id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(disabled)
            .optional(self.optional)
            .required(self.required)
            .status_placement(StatusPlacement::Detached);
        if let Some(status) = self.status {
            field = field.status(status);
        }
        if let Some(tooltip) = self.label_tooltip {
            field = field.label_tooltip(tooltip);
        }
        field.child(row)
    }
}

/// A prefix or suffix in an [`InputGroup`], such as "$" or "https://",
/// upstream `InputGroupText`.
#[derive(IntoElement)]
pub struct InputGroupText {
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
}

impl InputGroupText {
    /// Upstream `children`: short text or an icon.
    pub fn new(content: impl IntoElement) -> Self {
        InputGroupText {
            style: StyleRefinement::default(),
            children: SmallVec::from_buf([content.into_any_element()]),
        }
    }
}

impl Styled for InputGroupText {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for InputGroupText {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

pub(crate) fn text_cell(theme: &ResolvedTheme, dir: Direction, rem: Pixels, cx: &App) -> Div {
    let t = theme.tokens();
    let place = group_place(cx).unwrap_or(GroupPlace {
        first: true,
        last: true,
    });
    let border = t.border.width.to_pixels(rem);
    let cell = div()
        .flex()
        .flex_none()
        .items_center()
        .px(t.spacing.s2)
        .bg(t.color.background_muted)
        .font_stack(&t.font.family.body)
        .text_style(&t.text.body)
        .text_color(t.color.text_secondary)
        .whitespace_nowrap()
        .border(t.border.width)
        .border_color(t.color.border_emphasized);
    join(cell, place, t.radius.element.to_pixels(rem), border, dir)
}

impl RenderOnce for InputGroupText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let part = theme.for_part("input-group-text", &[]);
        let theme = part.clone().unwrap_or(theme);
        let dir = use_direction(cx);
        let mut cell = text_cell(&theme, dir, window.rem_size(), cx);
        theme.refine(cell.style(), "input-group-text", &[], None);
        cell.style().refine(&self.style);
        let cell = cell.children(self.children);
        match part {
            Some(part) => astryx_theme::scope(part, cell).into_any_element(),
            None => cell.into_any_element(),
        }
    }
}

/// Joins a Button addon to the row.
pub(crate) fn join_addon<E: Styled>(el: E, place: GroupPlace, radius: Pixels, dir: Direction) -> E {
    // A Button addon fills the row and drops the corners toward its
    // neighbours (upstream `groupStyles.buttonAddon`).
    let mut el = el.h_full();
    if !place.first {
        el = el.rounded_start(dir, px(0.));
    } else {
        el = el.rounded_start(dir, radius);
    }
    if !place.last {
        el = el.rounded_end(dir, px(0.));
    } else {
        el = el.rounded_end(dir, radius);
    }
    el
}
