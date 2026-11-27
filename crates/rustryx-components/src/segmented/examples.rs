//! Navigation-strip examples: the gallery's Segments page and the doc
//! example of SegmentedControl, TabList and Toolbar.

use gpui::{AnyElement, App, Entity, IntoElement, ParentElement, SharedString, Styled, Window};

use crate::button::{Button, ButtonGroup, ButtonVariant, IconButton, ToggleButton};
use crate::containers::SectionSide;
use crate::icon::Icon;
use crate::layout::{SpacingStep, Stack};
use crate::segmented::{SegmentLayout, SegmentedControl, SegmentedControlItem};
use crate::status::Badge;
use crate::tabs::{Tab, TabList, TabMenu, TabMenuOption};
use crate::text::{Text, TextType};
use crate::toolbar::Toolbar;
use rustryx_primitives::ElementSize;

fn store(entity: Entity<SharedString>) -> impl Fn(SharedString, &mut Window, &mut App) + 'static {
    move |next, window, cx| {
        entity.update(cx, |v, _| *v = next);
        window.refresh();
    }
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let view = window.use_keyed_state("view", cx, |_, _| SharedString::from("list"));
    let range = window.use_keyed_state("range", cx, |_, _| SharedString::from("week"));
    let tab = window.use_keyed_state("tab", cx, |_, _| SharedString::from("overview"));
    let many = window.use_keyed_state("many", cx, |_, _| SharedString::from("t0"));

    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label(
            "SegmentedControl: the selected surface slides to the new segment (a finger settles it on the selection spring)",
        ))
        .child(
            SegmentedControl::new("view", "View", view.read(cx).clone())
                .on_change(store(view.clone()))
                .item(SegmentedControlItem::new("list", "List").icon(Icon::new("menu")))
                .item(SegmentedControlItem::new("grid", "Grid").icon(Icon::new("viewColumns")))
                .item(SegmentedControlItem::new("board", "Board").disabled(true)),
        )
        .child(
            SegmentedControl::new("range", "Range", range.read(cx).clone())
                .size(ElementSize::Sm)
                .layout(SegmentLayout::Fill)
                .on_change(store(range.clone()))
                .items(
                    [("day", "Day"), ("week", "Week"), ("month", "Month")]
                        .map(|(v, l)| SegmentedControlItem::new(v, l)),
                ),
        )
        .child(label("TabList (with a divider; a narrow strip scrolls)"))
        .child(
            TabList::new("tabs", tab.read(cx).clone())
                .divider(true)
                .on_change(store(tab.clone()))
                .tab(Tab::new("overview", "Overview"))
                .tab(Tab::new("activity", "Activity").end_content(Badge::new("3")))
                .tab(Tab::new("settings", "Settings").icon(Icon::new("wrench")))
                .menu(TabMenu::new(
                    "More",
                    [
                        TabMenuOption::new("members", "Members"),
                        TabMenuOption::new("billing", "Billing"),
                        TabMenuOption::new("audit", "Audit log").icon("clock"),
                    ],
                )),
        )
        .child(
            gpui::div().w(gpui::px(320.)).child(
                TabList::new("many", many.read(cx).clone())
                    .size(ElementSize::Sm)
                    .on_change(store(many.clone()))
                    .tabs((0..9).map(|i| Tab::new(format!("t{i}"), format!("Section {i}")))),
            ),
        )
        .child(label("Toolbar (one tab stop; arrows move inside)"))
        .child(
            Toolbar::new("toolbar", "Formatting")
                .dividers([SectionSide::Top, SectionSide::Bottom])
                .start_content(
                    ButtonGroup::new("history", "History")
                        .child(IconButton::new("undo", "Undo", Icon::new("chevronLeft")))
                        .child(IconButton::new("redo", "Redo", Icon::new("chevronRight"))),
                )
                .center_content(ToggleButton::new("bold", "Bold"))
                .end_content(Button::new("share", "Share").variant(ButtonVariant::Primary)),
        )
        .into_any_element()
}
