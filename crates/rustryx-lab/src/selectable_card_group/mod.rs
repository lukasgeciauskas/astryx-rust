//! `SelectableCardGroup`: a set of selectable cards with the semantics of its selection
//! (upstream lab `SelectableCardGroup` and `SelectableCardGroupItem`).
//!
//! One choice is a radio group: Core's SelectableCards, made radios through Core's
//! SelectableCardInput, with one tab stop (the chosen card, else the first), arrow keys
//! that move and select with wrapping, Space or Enter on a focused card, and a click
//! that never deselects. Several choices are Core's SelectableCards as checkboxes that
//! toggle. The label and description name and
//! describe the group through Core's Field, which takes the root styling.

pub mod examples;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, IntoElement, Length, ParentElement, Role, SharedString,
    StyleRefinement, Window, div,
};
use rustryx_components::containers::{
    CardVariant, Elevation, SelectableCard, SelectableCardInput, SelectableCardInputType,
    provide_selectable_card_input,
};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_components::field::Field;
use rustryx_components::layout::SpacingStep;
use rustryx_i18n::use_direction;
use rustryx_primitives::{ListFocus, ListKeys};
use rustryx_theme::current;

/// How the cards lay out (upstream `orientation`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectableCardGroupOrientation {
    /// Cards wrap in rows (the default).
    #[default]
    Horizontal,
    /// Cards stack.
    Vertical,
}

impl SelectableCardGroupOrientation {
    /// The theme value.
    pub fn as_str(self) -> &'static str {
        match self {
            SelectableCardGroupOrientation::Horizontal => "horizontal",
            SelectableCardGroupOrientation::Vertical => "vertical",
        }
    }
}

type SingleFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type MultipleFn = Rc<dyn Fn(Vec<SharedString>, &mut Window, &mut App)>;

/// What the group selects (upstream `type` with its `value` and `onChange`).
#[derive(Clone)]
enum Selection {
    /// `single`: the chosen card's value, or none yet.
    Single {
        value: Option<SharedString>,
        on_change: Option<SingleFn>,
    },
    /// `multiple`: the chosen cards' values.
    Multiple {
        value: Vec<SharedString>,
        on_change: Option<MultipleFn>,
    },
}

/// The values after a card of a multiple-choice group is set to `selected` (upstream's
/// `change`): the newest choice is last, and a deselected value leaves.
pub fn toggle_value(
    values: &[SharedString],
    value: &SharedString,
    selected: bool,
) -> Vec<SharedString> {
    let mut next: Vec<SharedString> = values.iter().filter(|v| *v != value).cloned().collect();
    if selected {
        next.push(value.clone());
    }
    next
}

/// One card of a SelectableCardGroup (upstream `SelectableCardGroupItem`): a
/// SelectableCard whose role, name and selection come from the group. It draws only
/// through a group's `item` (upstream throws when one renders outside a group; here
/// it is not an element at all).
pub struct SelectableCardGroupItem {
    value: SharedString,
    label: SharedString,
    disabled: bool,
    padding: Option<SpacingStep>,
    variant: Option<CardVariant>,
    elevation: Option<Elevation>,
    width: Option<Length>,
    height: Option<Length>,
    max_width: Option<Length>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl SelectableCardGroupItem {
    /// The card that stands for `value` in the group's value (upstream `value`), named
    /// `label` (upstream `label`).
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        SelectableCardGroupItem {
            value: value.into(),
            label: label.into(),
            disabled: false,
            padding: None,
            variant: None,
            elevation: None,
            width: None,
            height: None,
            max_width: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    /// Upstream `isDisabled`: this card alone; it stays focusable.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `padding`.
    pub fn padding(mut self, step: SpacingStep) -> Self {
        self.padding = Some(step);
        self
    }

    /// Upstream `variant`.
    pub fn variant(mut self, variant: CardVariant) -> Self {
        self.variant = Some(variant);
        self
    }

    /// Upstream `elevation`.
    pub fn elevation(mut self, elevation: Elevation) -> Self {
        self.elevation = Some(elevation);
        self
    }

    /// Upstream `width`.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// Upstream `height`.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = Some(height.into());
        self
    }

    /// Upstream `maxWidth`.
    pub fn max_width(mut self, max_width: impl Into<Length>) -> Self {
        self.max_width = Some(max_width.into());
        self
    }

    /// The Core SelectableCard this item renders as, `selected` and `disabled` as the
    /// group decides.
    fn card(
        self,
        id: ElementId,
        selected: bool,
        disabled: bool,
        on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> SelectableCard {
        let mut card = SelectableCard::new(id, self.label, selected)
            .disabled(disabled)
            .on_change(on_change);
        if let Some(step) = self.padding {
            card = card.padding(step);
        }
        if let Some(variant) = self.variant {
            card = card.variant(variant);
        }
        if let Some(elevation) = self.elevation {
            card = card.elevation(elevation);
        }
        if let Some(w) = self.width {
            card = card.width(w);
        }
        if let Some(h) = self.height {
            card = card.height(h);
        }
        if let Some(w) = self.max_width {
            card = card.max_width(w);
        }
        card.style().refine(&self.style);
        card.children(self.children)
    }
}

impl Styled for SelectableCardGroupItem {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for SelectableCardGroupItem {
    /// Upstream `children`: the card's content.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// A set of selectable cards (upstream lab `SelectableCardGroup`).
///
/// ```ignore
/// SelectableCardGroup::new("plan", "Plan", Some("basic"))
///     .on_change(|plan, _, _| { /* store it */ })
///     .item(SelectableCardGroupItem::new("basic", "Basic").child("Basic"))
///     .item(SelectableCardGroupItem::new("pro", "Pro").child("Pro"))
/// ```
#[derive(IntoElement)]
pub struct SelectableCardGroup {
    id: ElementId,
    label: SharedString,
    label_hidden: bool,
    description: Option<SharedString>,
    disabled: bool,
    orientation: SelectableCardGroupOrientation,
    selection: Selection,
    items: Vec<SelectableCardGroupItem>,
    style: StyleRefinement,
}

impl SelectableCardGroup {
    /// A single-choice group (upstream `type: 'single'`, the default): a radio group
    /// named `label` (upstream `label`) with `value` chosen, or none yet (upstream
    /// `value`).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: Option<impl Into<SharedString>>,
    ) -> Self {
        Self::with(
            id.into(),
            label.into(),
            Selection::Single {
                value: value.map(Into::into),
                on_change: None,
            },
        )
    }

    /// A multiple-choice group (upstream `type: 'multiple'`): a group of checkboxes
    /// named `label` with `values` chosen.
    pub fn multiple(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        values: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        Self::with(
            id.into(),
            label.into(),
            Selection::Multiple {
                value: values.into_iter().map(Into::into).collect(),
                on_change: None,
            },
        )
    }

    fn with(id: ElementId, label: SharedString, selection: Selection) -> Self {
        SelectableCardGroup {
            id,
            label,
            label_hidden: false,
            description: None,
            disabled: false,
            orientation: SelectableCardGroupOrientation::Horizontal,
            selection,
            items: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onChange` of a single-choice group: the value of the card the user
    /// chose. Choosing the chosen card again calls nothing.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        if let Selection::Single { on_change, .. } = &mut self.selection {
            *on_change = Some(Rc::new(f));
        }
        self
    }

    /// Upstream `onChange` of a multiple-choice group: the new list of chosen values,
    /// the newest choice last.
    pub fn on_change_values(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        if let Selection::Multiple { on_change, .. } = &mut self.selection {
            *on_change = Some(Rc::new(f));
        }
        self
    }

    /// Upstream `isLabelHidden`: hides the label visually; it still names the group.
    pub fn label_hidden(mut self, hidden: bool) -> Self {
        self.label_hidden = hidden;
        self
    }

    /// Upstream `description`: helper text under the label, describing the group.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `isDisabled`: every card; they stay focusable and announce disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `orientation` (`Horizontal` by default).
    pub fn orientation(mut self, orientation: SelectableCardGroupOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// One of upstream's `children`: a card.
    pub fn item(mut self, item: SelectableCardGroupItem) -> Self {
        self.items.push(item);
        self
    }

    /// Upstream `children`: the cards.
    pub fn items(mut self, items: impl IntoIterator<Item = SelectableCardGroupItem>) -> Self {
        self.items.extend(items);
        self
    }
}

impl Styled for SelectableCardGroup {
    /// Upstream `containerXstyle`, `containerClassName` and `containerStyle`: the Field
    /// box, not the group.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SelectableCardGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let group_disabled = self.disabled;
        let is_multiple = matches!(self.selection, Selection::Multiple { .. });

        let cards: Vec<AnyElement> = match self.selection.clone() {
            Selection::Multiple { value, on_change } => self
                .items
                .into_iter()
                .map(|item| {
                    let selected = value.contains(&item.value);
                    let id = ElementId::Name(format!("{}-{}", self.id, item.value).into());
                    let disabled = group_disabled || item.disabled;
                    let (values, v, on_change) =
                        (value.clone(), item.value.clone(), on_change.clone());
                    item.card(id, selected, disabled, move |next, window, cx| {
                        if let Some(f) = &on_change {
                            f(toggle_value(&values, &v, next), window, cx);
                        }
                    })
                    .into_any_element()
                })
                .collect(),
            Selection::Single { value, on_change } => {
                let disabled: Vec<bool> = self
                    .items
                    .iter()
                    .map(|i| group_disabled || i.disabled)
                    .collect();
                let values: Rc<[SharedString]> =
                    self.items.iter().map(|i| i.value.clone()).collect();
                let selected = value
                    .as_ref()
                    .and_then(|v| values.iter().position(|x| x == v));
                // The cards are radios: arrows move and select, wrapping. A disabled card
                // keeps its focus, so the arrows reach it, but it is never chosen.
                let (move_values, move_disabled, move_change) =
                    (values.clone(), disabled.clone(), on_change.clone());
                let list = ListFocus::new(
                    &self.id,
                    self.items.len(),
                    ListKeys::both().home_end(false).reach_disabled(true),
                    window,
                    cx,
                )
                .rtl(dir.is_rtl())
                .on_move(move |index, window, cx| {
                    if !move_disabled[index]
                        && let Some(f) = &move_change
                    {
                        f(move_values[index].clone(), window, cx);
                    }
                });
                let tab_stop = list.tab_stop(selected, window);
                // The radios' shared name (upstream `htmlName`, else a generated id).
                let name: SharedString = format!("{}", self.id).into();
                let rows: Vec<AnyElement> = self
                    .items
                    .into_iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let id = ElementId::Name(format!("{}-{}", self.id, item.value).into());
                        // Each card is a radio of the group's set: the group's roving
                        // focus is its tab stop, as a native radio group's is.
                        let input = SelectableCardInput {
                            input_type: SelectableCardInputType::Radio,
                            name: name.clone(),
                            value: item.value.clone(),
                            focus: Some(list.item_handle(i, tab_stop)),
                        };
                        let (v, on_change) = (values[i].clone(), on_change.clone());
                        let card = item.card(
                            id,
                            selected == Some(i),
                            disabled[i],
                            move |_, window, cx| {
                                if let Some(f) = &on_change {
                                    f(v.clone(), window, cx);
                                }
                            },
                        );
                        provide_selectable_card_input(input, card).into_any_element()
                    })
                    .collect();
                let key_down = list.key_down_listener();
                vec![
                    div()
                        .id("keys")
                        .display_contents()
                        .on_key_down(key_down)
                        .children(rows)
                        .into_any_element(),
                ]
            }
        };

        let mut group = div().flex().flex_wrap().gap(t.spacing.s3);
        if self.orientation == SelectableCardGroupOrientation::Vertical {
            group = group.flex_col().flex_nowrap();
        }
        theme.refine(
            group.style(),
            "selectable-card-group",
            &[
                ("orientation", self.orientation.as_str()),
                ("type", if is_multiple { "multiple" } else { "single" }),
            ],
            None,
        );
        let selector = format!("{}-group", self.id);
        let mut group = group
            .id(ElementId::Name(format!("{}-group", self.id).into()))
            .debug_selector(move || selector)
            .role(if is_multiple {
                Role::Group
            } else {
                Role::RadioGroup
            })
            .aria_label(self.label.clone())
            .aria_disabled(group_disabled)
            .children(cards);
        if let Some(description) = &self.description {
            group = group.aria_description(description.clone());
        }
        let mut field = Field::new(self.id.clone(), self.label)
            .label_hidden(self.label_hidden)
            .disabled(group_disabled);
        if let Some(description) = self.description {
            field = field.description(description);
        }
        field.style().refine(&self.style);
        field.child(group)
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `SelectableCardGroup`.
pub const SELECTABLE_CARD_GROUP_DOC: ComponentDoc = ComponentDoc {
    name: "SelectableCardGroup",
    rust_type: "rustryx::lab::selectable_card_group::SelectableCardGroup",
    props: &[
        p(
            "type",
            RustApi::Argument("constructor: new (single) or multiple"),
        ),
        p("value", RustApi::Argument("value / values")),
        p(
            "onChange",
            RustApi::Method("on_change (single) / on_change_values (multiple)"),
        ),
        p("label", RustApi::Argument("label")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p(
            "htmlName",
            RustApi::Waived(
                "GPUI has no forms: the app reads the group's value, there is nothing to submit",
            ),
        ),
        p("isDisabled", RustApi::Method("disabled")),
        p("orientation", RustApi::Method("orientation")),
        p("children", RustApi::Method("item / items")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("containerXstyle", RustApi::Trait("Styled")),
        p("containerClassName", RustApi::Trait("Styled")),
        p("containerStyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

/// Upstream lab `SelectableCardGroupItem`.
pub const SELECTABLE_CARD_GROUP_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "SelectableCardGroupItem",
    rust_type: "rustryx::lab::selectable_card_group::SelectableCardGroupItem",
    props: &[
        p("value", RustApi::Argument("value")),
        p("label", RustApi::Argument("label")),
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
