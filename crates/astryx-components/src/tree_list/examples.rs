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

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
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
        .into_any_element()
}
