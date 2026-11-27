//! ListItem, upstream `List/ListItem.tsx`: one row of a List, on Item.
//!
//! - Inside a List it takes the list's density, dividers (a rule under every
//!   row but the last, square corners), marker and edge treatment (FR2);
//!   outside one, balanced density, no dividers, no marker (FR3).
//! - Markers sit on the label's first line: a dot, a ring or a number in the
//!   list's shared number column; a task row shows a read-only checkbox
//!   (FR4).
//! - Under a finger an interactive row's hit target reaches 44px where its
//!   neighbors leave room (List FR10).
//! - A row menu (List FR13) opens on the row's context request; a row with
//!   no focusable part of its own shows a "More actions" button as its
//!   keyboard path to the menu.

use std::rc::Rc;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::use_context;
use astryx_theme::current;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, FocusHandle, IntoElement, ParentElement, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, prelude::*, px, relative,
};

use super::list::{ListContext, ListEdge, ListMarker, ListRow, ListStyle, counter_text, row_inset};
use crate::choice::CheckboxInput;
use crate::indicator::ChoiceSize;
use crate::item::{Item, ItemAlign, ItemDensity};
use crate::overlay::MenuEntry;
use crate::reorderable::ItemKey;
use crate::reorderable::session::{self as reorder, Registration};
use crate::swipe::{
    ItemSwipeActions, ItemSwipeBehavior, Swipe, SwipeHost, SwipeId, swipe_with_item_key,
};

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type ActivateFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// One row of a List, upstream `ListItem`.
///
/// ```ignore
/// ListItem::new("profile", "Profile").on_click(|_, _, _| open_profile())
/// ```
#[derive(IntoElement)]
pub struct ListItem {
    id: ElementId,
    label: AnyElement,
    label_text: Option<SharedString>,
    description: Option<AnyElement>,
    description_text: bool,
    /// The description's text, for the dragged row's place.
    description_string: Option<SharedString>,
    start_content: Option<AnyElement>,
    end_content: Option<AnyElement>,
    on_click: Option<ClickFn>,
    delegate: Option<(FocusHandle, ActivateFn)>,
    href: Option<SharedString>,
    target: Option<SharedString>,
    align: ItemAlign,
    disabled: bool,
    selected: bool,
    item_key: Option<ItemKey>,
    reorder_label: Option<SharedString>,
    reorder_tooltip: Option<SharedString>,
    menu: Option<Vec<MenuEntry>>,
    swipe: Option<Rc<SwipeHost>>,
    swipe_actions: Option<ItemSwipeActions>,
    swipe_behavior: Option<ItemSwipeBehavior>,
    style: StyleRefinement,
}

impl ListItem {
    /// Upstream `label` as text, which truncates to one line.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        let label: SharedString = label.into();
        let mut item = Self::new_element(id, label.clone());
        item.label_text = Some(label);
        item
    }

    /// Upstream `label` as any element, drawn as given.
    pub fn new_element(id: impl Into<ElementId>, label: impl IntoElement) -> Self {
        ListItem {
            id: id.into(),
            label: label.into_any_element(),
            label_text: None,
            description: None,
            description_text: false,
            description_string: None,
            start_content: None,
            end_content: None,
            on_click: None,
            delegate: None,
            href: None,
            target: None,
            align: ItemAlign::Center,
            disabled: false,
            selected: false,
            item_key: None,
            reorder_label: None,
            reorder_tooltip: None,
            menu: None,
            swipe: None,
            swipe_actions: None,
            swipe_behavior: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `swipe`: the row's swipe actions (see [`Item::swipe`]), handed to its
    /// Item with only the row's `item_key` added, which each action's `on_activate`
    /// receives. The same value and key hand the row the same value.
    pub fn swipe<Id: SwipeId>(mut self, swipe: Swipe<Id>) -> Self {
        self.swipe = Some(swipe.host);
        self
    }

    /// Deprecated upstream `swipeActions`, passed to the Item unchanged.
    pub fn swipe_actions(mut self, actions: ItemSwipeActions) -> Self {
        self.swipe_actions = Some(actions);
        self
    }

    /// Deprecated upstream `swipeBehavior`, passed to the Item unchanged.
    pub fn swipe_behavior(mut self, behavior: ItemSwipeBehavior) -> Self {
        self.swipe_behavior = Some(behavior);
        self
    }

    astryx_primitives::setters! {
        /// Upstream `align`: how start and end content sit against the text.
        align: ItemAlign,
        /// Upstream `isDisabled`: an interactive row stays reachable but
        /// never activates.
        !disabled,
        /// Upstream `isSelected`: the selected tint, and `aria-current` on
        /// an interactive row.
        !selected,
        /// Upstream `href`: the row is a link through the link handler.
        ?href: SharedString,
        /// Upstream `target`, with `href`.
        ?target: SharedString,
    }

    /// Upstream `itemKey`: the row's identity for a reorderable List, a
    /// string or a number from the data; `on_reorder` reports it as given.
    pub fn item_key(mut self, key: impl Into<ItemKey>) -> Self {
        self.item_key = Some(key.into());
        self
    }

    /// The row's name in its reorder handle's label and the reorder announcements, for
    /// a row whose label is an element (upstream reads the label's text).
    pub fn reorder_label(mut self, label: impl Into<SharedString>) -> Self {
        self.reorder_label = Some(label.into());
        self
    }

    /// A tooltip on the row's reorder handle: why the row does not move, for a disabled
    /// row (upstream TransferList's `DisabledHandleTooltip`).
    pub fn reorder_tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.reorder_tooltip = Some(tooltip.into());
        self
    }

    /// Upstream `description` as text: one line, ellipsized.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        let description: SharedString = description.into();
        self.description_string = Some(description.clone());
        self.description = Some(description.into_any_element());
        self.description_text = true;
        self
    }

    /// Upstream `description` as any element, for its full text.
    pub fn description_element(mut self, description: impl IntoElement) -> Self {
        self.description = Some(description.into_any_element());
        self.description_text = false;
        self
    }

    /// Upstream `startContent`: an icon or avatar before the label.
    pub fn start_content(mut self, content: impl IntoElement) -> Self {
        self.start_content = Some(content.into_any_element());
        self
    }

    /// Upstream `endContent`: a badge or chevron after the label.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `onClick`: the whole row is a button.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `interactiveRef`: a control inside the row owns its keys
    /// and action; a press on the row's surface focuses it and runs
    /// `activate` (see `Item::interactive`).
    pub fn interactive(
        mut self,
        focus: FocusHandle,
        activate: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.delegate = Some((focus, Rc::new(activate)));
        self
    }

    /// A row menu (List FR13; upstream passes the row to `useContextMenu`):
    /// a right press, a long press, Shift+F10 or the Menu key open it.
    pub fn context_menu(mut self, entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        self.menu = Some(entries.into_iter().collect());
        self
    }
}

/// What a List keeps of a keyed row to draw its exit once it is gone (List FR12), where
/// the renderer keeps no snapshot of the row's last frame (a renderer without fork
/// patch R3's offscreen layers): its text and selection; its start and end content are
/// not kept.
#[derive(Clone, Default)]
pub(super) struct RowGhost {
    label: Option<SharedString>,
    description: Option<SharedString>,
    align: ItemAlign,
    selected: bool,
    /// It was the list's last row: no divider under it.
    pub last: bool,
}

impl RowGhost {
    /// The row as its exit draws it: static, its text and selection.
    pub(super) fn row(&self, id: ElementId) -> ListItem {
        let mut item = ListItem::new(id, self.label.clone().unwrap_or_default())
            .align(self.align)
            .selected(self.selected);
        if let Some(d) = &self.description {
            item = item.description(d.clone());
        }
        item
    }
}

impl ListItem {
    /// The row's identity for a List's update motion (FR12): its `item_key`.
    pub(super) fn motion_key(&self) -> Option<ItemKey> {
        self.item_key.clone()
    }

    pub(super) fn is_selected(&self) -> bool {
        self.selected
    }

    /// What the List keeps of the row for its exit.
    pub(super) fn ghost(&self, last: bool) -> RowGhost {
        RowGhost {
            label: self.label_text.clone(),
            description: self.description_string.clone(),
            align: self.align,
            selected: self.selected,
            last,
        }
    }
}

impl Styled for ListItem {
    /// Upstream `xstyle`, `className` and `style`: after the row's own.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ListItem {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let ctx = use_context::<ListContext>(cx);
        let row = use_context::<ListRow>(cx).unwrap_or(ListRow {
            first: true,
            last: true,
            number: 1,
            corners: Default::default(),
            index: 0,
        });
        let in_list = ctx.is_some();
        let density = ctx.as_ref().map_or(ItemDensity::Balanced, |c| c.density);
        let dividers = ctx.as_ref().is_some_and(|c| c.dividers);
        let edge = ctx.as_ref().map_or(ListEdge::Padded, |c| c.edge);
        let list_style = ctx.as_ref().map_or(ListStyle::None, |c| c.list_style);
        let host = ctx.as_ref().map(|c| c.host).unwrap_or_default();
        let (inset, themed) = row_inset(&theme, density, rem);
        let interactive = self.on_click.is_some() || self.href.is_some() || self.delegate.is_some();
        let menu_button = self.menu.is_some() && !interactive;

        let mut item = Item::new(self.id.clone(), self.label)
            .density(density)
            .align(self.align)
            .disabled(self.disabled)
            .selected(self.selected);
        if self.label_text.is_some() {
            item = item.label_lines(1usize);
        }
        if let Some(description) = self.description {
            item = item.description(description);
            if self.description_text {
                item = item.description_lines(1usize);
            }
        }
        if let Some(start) = self.start_content {
            item = item.start_content(start);
        }
        // FR13: a row with no focusable part shows its menu's keyboard path.
        let more = menu_button.then(|| {
            let label = i18n(cx).format(
                "@astryx.listItem.moreActions",
                &[(
                    "label",
                    self.label_text.clone().unwrap_or_default().as_ref().into(),
                )],
            );
            crate::overlay::context_menu_button(
                ElementId::Name(format!("{}-more", self.id).into()),
                ElementId::Name(format!("{}-menu", self.id).into()),
                label,
                window,
                cx,
            )
        });
        // AST-103: a reorderable List's row joins its order, and one that can
        // move ends with the handle (FR5: the one element reordering adds).
        let reorder = ctx.as_ref().and_then(|c| c.reorder.clone());
        let reorder_key = reorder.as_ref().and(self.item_key.clone());
        let handle = match (&reorder, &reorder_key) {
            (Some(rc), Some(key)) => {
                let movable = rc.reorderable.can_move(key);
                let name = self
                    .reorder_label
                    .clone()
                    .or_else(|| self.label_text.clone());
                reorder::register(
                    rc,
                    Registration {
                        index: row.index,
                        key: key.clone(),
                        name: name.clone(),
                        movable,
                    },
                    cx,
                );
                movable.then(|| {
                    let handle =
                        reorder::handle(rc, key, name, row.index + 1, self.disabled, window, cx);
                    match self.reorder_tooltip.clone() {
                        Some(text) => crate::overlay::Tooltip::new(
                            ElementId::Name(format!("{}-reorder-tooltip", self.id).into()),
                            text,
                        )
                        .child(handle)
                        .into_any_element(),
                        None => handle,
                    }
                })
            }
            _ => None,
        };
        let ends: Vec<AnyElement> = [
            self.end_content,
            more.map(IntoElement::into_any_element),
            handle,
        ]
        .into_iter()
        .flatten()
        .collect();
        match ends.len() {
            0 => {}
            1 => item = item.end_content(ends.into_iter().next().unwrap()),
            _ => {
                item =
                    item.end_content(div().flex().items_center().gap(t.spacing.s2).children(ends))
            }
        }
        if let Some(f) = self.on_click {
            item = item.on_click(move |event, window, cx| f(event, window, cx));
        }
        if let Some(href) = self.href {
            item = item.href(href);
            if let Some(target) = self.target {
                item = item.target(target);
            }
        }
        if let Some((focus, activate)) = self.delegate {
            item = item.interactive(focus, move |window, cx| activate(window, cx));
        }
        // FR1: `swipe` with only the row's key added, keyed once per value and key.
        if let Some(host) = self.swipe.take() {
            let keyed = window.use_keyed_state(
                ElementId::Name(format!("{}-swipe-key", self.id).into()),
                cx,
                |_, _| None::<(Rc<SwipeHost>, Option<ItemKey>, Rc<SwipeHost>)>,
            );
            let reuse = keyed.read(cx).as_ref().and_then(|(from, key, value)| {
                (Rc::ptr_eq(from, &host) && *key == self.item_key).then(|| value.clone())
            });
            let value = match reuse {
                Some(value) => value,
                None => {
                    let value = swipe_with_item_key(&host, self.item_key.as_ref());
                    let stored = (host.clone(), self.item_key.clone(), value.clone());
                    keyed.update(cx, |k, _| *k = Some(stored));
                    value
                }
            };
            item = item.swipe_host(Some(value));
        }
        if let Some(actions) = self.swipe_actions.take() {
            item = item.swipe_actions(actions);
        }
        if let Some(behavior) = self.swipe_behavior {
            item = item.swipe_behavior(behavior);
        }

        // FR4: the marker, on the label's first line.
        let marker_kind = match list_style {
            ListStyle::None => None,
            ListStyle::Disc => Some(ListMarker::Disc),
            ListStyle::Circle => Some(ListMarker::Circle),
            ListStyle::Decimal => Some(ListMarker::Decimal),
        }
        .map(|kind| ctx.as_ref().and_then(|c| c.marker).unwrap_or(kind));
        let first_line = t.text.body.size.to_pixels(rem) * t.text.body.leading;
        if let Some(kind) = marker_kind {
            let task = ctx.as_ref().and_then(|c| c.task.clone());
            let marker = match (task, kind) {
                (Some(task), _) => div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .self_start()
                    .h(first_line)
                    .child(
                        CheckboxInput::new(
                            ElementId::Name(format!("{}-task", self.id).into()),
                            task.label,
                            task.checked,
                        )
                        .size(ChoiceSize::Sm)
                        .label_hidden(true)
                        .read_only(true),
                    )
                    .into_any_element(),
                (None, ListMarker::Disc | ListMarker::Circle | ListMarker::Square) => {
                    let dot = div().size(px(6.));
                    let dot = match kind {
                        ListMarker::Disc => dot.rounded_full().bg(t.color.text_primary),
                        ListMarker::Circle => dot
                            .rounded_full()
                            .border_1()
                            .border_color(t.color.text_primary),
                        _ => dot.bg(t.color.text_primary),
                    };
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .self_start()
                        .w(t.spacing.s4)
                        .h(first_line)
                        .child(dot)
                        .into_any_element()
                }
                (None, kind) => {
                    let width = ctx
                        .as_ref()
                        .and_then(|c| c.number_column)
                        .unwrap_or_else(|| t.spacing.s4.to_pixels(rem));
                    div()
                        .flex()
                        .flex_none()
                        .justify_end()
                        .self_start()
                        .w(width)
                        .whitespace_nowrap()
                        .text_color(t.color.text_primary)
                        .text_size(t.text.body.size)
                        .line_height(relative(t.text.body.leading))
                        .child(counter_text(row.number, kind))
                        .into_any_element()
                }
            };
            item = item.marker(marker);
        }

        // FR2: dividers, and square corners with them.
        if dividers {
            item = item.rounded(px(0.));
            if !row.last {
                item = item.border_b(t.border.width).border_color(t.color.border);
            }
        }
        // FR3-FR7: the edge treatment. A theme's inline padding on the row
        // wins over the treatment's padding.
        match edge {
            ListEdge::Padded => {}
            ListEdge::Flush => {
                if !themed {
                    item = item.px(px(0.));
                }
            }
            ListEdge::Outset => {
                item = item
                    .margin_start(dir, (host.inline_start - inset).max(px(0.)))
                    .margin_end(dir, (host.inline_end - inset).max(px(0.)));
            }
            ListEdge::Bleed => {
                if !themed {
                    item = item
                        .padding_start(dir, host.inline_start.max(inset))
                        .padding_end(dir, host.inline_end.max(inset));
                }
                let c = row.corners;
                let (left_top, right_top, left_bottom, right_bottom) = if dir.is_rtl() {
                    (c.start_end, c.start_start, c.end_end, c.end_start)
                } else {
                    (c.start_start, c.start_end, c.end_start, c.end_end)
                };
                item = item
                    .rounded_tl(left_top)
                    .rounded_tr(right_top)
                    .rounded_bl(left_bottom)
                    .rounded_br(right_bottom);
            }
        }
        // FR10: the touch band reaches past the list's outer edges freely,
        // and between rows by half their gap.
        if interactive && !self.disabled {
            let half = ctx.as_ref().map_or(px(0.), |c| c.gap / 2.);
            let up = (in_list && !row.first).then_some(half);
            let down = (in_list && !row.last).then_some(half);
            item = item.touch_band(up, down);
        }
        theme.refine(item.style(), "list-item", &[], None);
        item.style().refine(&self.style);

        // While the row is dragged it dims in place and a copy of it, its kept
        // frame (fork patch R3), follows the pointer; under shift feedback its
        // place opens as the gap. A renderer without offscreen layers draws the
        // row itself at the pointer, and its place keeps this dimmed copy of its
        // text, grip and divider. Rows a shifting drag passes slide aside.
        let copy = {
            let (label, description) = (self.label_text.clone(), self.description_string.clone());
            let (id, align, selected) = (self.id.clone(), self.align, self.selected);
            let (grip, divider) = (
                t.color.icon_secondary,
                (dividers && !row.last).then_some((t.border.width, t.color.border)),
            );
            move || {
                let mut copy = Item::new(
                    ElementId::Name(format!("{id}-reorder-source").into()),
                    label.unwrap_or_default(),
                )
                .density(density)
                .align(align)
                .selected(selected)
                .label_lines(1usize)
                .end_content(reorder::grip(grip));
                if let Some(d) = description {
                    copy = copy.description(d).description_lines(1usize);
                }
                if let Some((width, color)) = divider {
                    copy = copy.rounded(px(0.)).border_b(width).border_color(color);
                }
                Some(copy.into_any_element())
            }
        };
        let element = match self.menu {
            None => item.into_any_element(),
            // FR13: the row's menu, on the row itself.
            Some(entries) => crate::overlay::with_row_menu(
                div()
                    .id(ElementId::Name(format!("{}-row", self.id).into()))
                    .relative()
                    .flex()
                    .flex_col()
                    .child(item),
                ElementId::Name(format!("{}-menu", self.id).into()),
                entries,
                self.label_text,
                window,
                cx,
            )
            .into_any_element(),
        };
        match (&reorder, &reorder_key) {
            (Some(rc), Some(key)) => reorder::place_item(rc, key, row.index, element, copy, cx),
            _ => element,
        }
    }
}
