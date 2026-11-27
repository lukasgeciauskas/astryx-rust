//! `TabBarMoreList`: the More tab's screen, a list of the tabs past the fourth
//! (upstream lab `TabBarMoreList`, UIKit's `UIMoreListController`).
//!
//! It takes the same TabBarItems given to TabBar, so both agree on which tabs overflow.
//! Each row shows its tab's icon, label and badge and a disclosure chevron that points
//! to the inline end; the selected tab's row reads as selected, a disabled tab's row is
//! disabled, and choosing a row calls `on_change` with its value.

pub mod examples;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, IntoElement, SharedString, StyleRefinement, Window, div};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_components::icon::Icon;
use rustryx_components::layout::VisuallyHidden;
use rustryx_components::list::{List, ListItem};
use rustryx_components::status::{Badge, BadgeVariant};
use rustryx_i18n::{current as i18n, use_direction};

use crate::tab_bar::{TabBarItem, badged_name, has_badge, overflow_items};

type ValueFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// The More tab's screen (upstream lab `TabBarMoreList`).
///
/// ```ignore
/// TabBarMoreList::new("more", tabs.clone()).value(tab).on_change(|value, _, _| { /* show it */ })
/// ```
#[derive(IntoElement)]
pub struct TabBarMoreList {
    id: ElementId,
    items: Vec<TabBarItem>,
    value: Option<SharedString>,
    on_change: Option<ValueFn>,
    style: StyleRefinement,
}

impl TabBarMoreList {
    /// The list for `items`, the same TabBarItems given to TabBar (upstream
    /// `children`).
    pub fn new(id: impl Into<ElementId>, items: impl IntoIterator<Item = TabBarItem>) -> Self {
        TabBarMoreList {
            id: id.into(),
            items: items.into_iter().collect(),
            value: None,
            on_change: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `value`: the selected tab's value; its row reads as selected.
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Upstream `onChange`: a row's value when it is chosen.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl Styled for TabBarMoreList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TabBarMoreList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let label = i18n(cx).t("@astryx.tabBar.more");
        let rtl = use_direction(cx).is_rtl();
        let mut list = List::new(self.id.clone()).label(label).dividers(true);
        list.style().refine(&self.style);
        #[cfg(test)]
        DRAWN.with(|d| {
            *d.borrow_mut() = Some((
                label_for_test(cx),
                overflow_items(&self.items)
                    .iter()
                    .map(|item| DrawnRow {
                        value: item.value().clone(),
                        name: badged_name(item, cx).unwrap_or_else(|| item_label(item)),
                        selected: self.value.as_ref() == Some(item.value()),
                        disabled: item_disabled(item),
                    })
                    .collect(),
            ));
        });
        let rows: Vec<AnyElement> = overflow_items(&self.items)
            .iter()
            .map(|item| {
                let id = ElementId::Name(format!("{:?}-row-{}", self.id, item.value()).into());
                // The row's name is one message with both, as the tab's is.
                let mut row = match badged_name(item, cx) {
                    None => ListItem::new(id, item_label(item)),
                    Some(name) => ListItem::new_element(
                        id,
                        div()
                            .child(div().child(item_label(item)))
                            .child(VisuallyHidden::new().child(name)),
                    ),
                };
                let icon = item_icon(item, window, cx);
                // Right to left it points left, as Core's disclosure rows' do.
                let chevron = Icon::new(if rtl { "chevronLeft" } else { "chevronRight" });
                let mut end = div().flex().items_center().gap_2();
                if has_badge(item_badge(item)) {
                    end = end.child(
                        Badge::new(item_badge(item).cloned().unwrap_or_default())
                            .variant(BadgeVariant::Error),
                    );
                }
                row = row
                    .start_content(icon)
                    .end_content(end.child(chevron))
                    .selected(self.value.as_ref() == Some(item.value()))
                    .disabled(item_disabled(item));
                let (value, on_change) = (item.value().clone(), self.on_change.clone());
                row.on_click(move |_, window, cx| {
                    if let Some(f) = &on_change {
                        f(value.clone(), window, cx);
                    }
                })
                .into_any_element()
            })
            .collect();
        list.children(rows)
    }
}

#[cfg(test)]
fn label_for_test(cx: &App) -> SharedString {
    i18n(cx).t("@astryx.tabBar.more")
}

#[cfg(test)]
thread_local! {
    /// The list's name and rows as last drawn, for tests.
    pub(crate) static DRAWN: std::cell::RefCell<Option<(SharedString, Vec<DrawnRow>)>> = const { std::cell::RefCell::new(None) };
}

/// A row as drawn, for tests.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DrawnRow {
    pub value: SharedString,
    pub name: SharedString,
    pub selected: bool,
    pub disabled: bool,
}

fn item_label(item: &TabBarItem) -> SharedString {
    crate::tab_bar::item_label(item).clone()
}

fn item_badge(item: &TabBarItem) -> Option<&SharedString> {
    crate::tab_bar::item_badge(item)
}

fn item_disabled(item: &TabBarItem) -> bool {
    crate::tab_bar::item_disabled(item)
}

fn item_icon(item: &TabBarItem, window: &mut Window, cx: &mut App) -> AnyElement {
    crate::tab_bar::item_icon(item, window, cx)
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `TabBarMoreList`.
pub const TAB_BAR_MORE_LIST_DOC: ComponentDoc = ComponentDoc {
    name: "TabBarMoreList",
    rust_type: "rustryx::lab::tab_bar_more_list::TabBarMoreList",
    props: &[
        p("children", RustApi::Argument("items")),
        p("onChange", RustApi::Method("on_change")),
        p("value", RustApi::Method("value")),
    ],
    example: include_str!("examples.rs"),
};
