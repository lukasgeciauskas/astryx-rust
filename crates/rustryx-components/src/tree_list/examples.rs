//! TreeList examples: the gallery's Collections page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::icon::Icon;
use crate::layout::{SpacingStep, Stack};
use crate::status::Badge;
use crate::text::{Text, TextType};
use crate::tree_list::{TreeList, TreeListDensity, TreeListItemData, TreeListVariant};

fn files() -> Vec<TreeListItemData> {
    vec![
        TreeListItemData::new("src", "src")
            .expanded(true)
            .children([
                TreeListItemData::new("components", "components")
                    .expanded(true)
                    .children([
                        TreeListItemData::new("button", "button.rs")
                            .start_content(|_, _| Icon::new("copy").into_any_element()),
                        TreeListItemData::new("tree", "tree_list.rs")
                            .selected(true)
                            .description("Edited just now")
                            .end_content(|_, _| Badge::new("New").into_any_element()),
                    ]),
                TreeListItemData::new("lib", "lib.rs"),
            ]),
        TreeListItemData::new("docs", "docs").children([
            TreeListItemData::new("plan", "PLAN.md"),
            TreeListItemData::new("handoff", "HANDOFF.md"),
        ]),
        TreeListItemData::new("readme", "README.md").disabled(true),
    ]
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    // An animatedLayout handle: expanding a folder glides the note after the tree.
    let layout = crate::animated_layout::use_animated_layout("files-layout", window, cx);
    let animated = layout.container(
        gpui::div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .w(px(320.))
            .child(
                TreeList::new("files-animated", files())
                    .aria_label("Files, animated")
                    .animated_layout(layout.clone()),
            )
            .child(
                Text::new("This note glides as folders above it open and close.")
                    .text_type(TextType::Supporting),
            ),
        window,
        cx,
    );
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("TreeList: line guides, and compact without guides"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S6)
                .child(
                    TreeList::new("files", files())
                        .aria_label("Files")
                        .w(px(320.)),
                )
                .child(
                    TreeList::new("files-compact", files())
                        .aria_label("Files, compact")
                        .density(TreeListDensity::Compact)
                        .variant(TreeListVariant::NoGuides)
                        .w(px(280.)),
                ),
        )
        .child(label(
            "With an animatedLayout handle (and under a finger without one), folders reveal their rows and the rest glides",
        ))
        .child(animated)
        .into_any_element()
}
