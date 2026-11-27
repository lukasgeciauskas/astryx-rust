//! ComplexSelector examples: the gallery's Collections page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, px};

use crate::button::{Button, ButtonVariant};
use crate::choice::{CheckboxList, CheckboxListItem, RadioList, RadioListItem};
use crate::complex_selector::ComplexSelector;
use crate::icon::Icon;
use crate::layout::{SpacingStep, Stack};
use crate::selector::SelectorVariant;
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let priority =
        window.use_keyed_state("complex-priority", cx, |_, _| SharedString::from("Medium"));
    let filters = window.use_keyed_state("complex-filters", cx, |_, _| {
        vec![SharedString::from("Open")]
    });
    let draft = window.use_keyed_state("complex-filters-draft", cx, |_, _| {
        Vec::<SharedString>::new()
    });
    let set_priority = priority.clone();
    let set_filters = filters.clone();
    let shown_filters = filters.read(cx).clone();
    let filter_label: SharedString = if shown_filters.is_empty() {
        "Any status".into()
    } else {
        shown_filters
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join(", ")
            .into()
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("ComplexSelector"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                // A simple choice: pick and close.
                .child(
                    ComplexSelector::new("priority", "Priority", priority.read(cx).clone())
                        .trigger_label(priority.read(cx).clone())
                        .on_change(move |next, window, cx| {
                            set_priority.update(cx, |v, _| *v = next);
                            window.refresh();
                        })
                        .content(|value, commit, close, _, _, _| {
                            RadioList::new("priority-list", "Priority", value.clone())
                                .label_hidden(true)
                                .items(["Low", "Medium", "High"].map(|v| RadioListItem::new(v, v)))
                                .on_change(move |next, window, cx| {
                                    commit(next, window, cx);
                                    close(window, cx);
                                })
                                .into_any_element()
                        })
                        .w(px(220.)),
                )
                // A staged editor: the draft commits on Apply only.
                .child(
                    ComplexSelector::new("status-filter", "Status", shown_filters.clone())
                        .variant(SelectorVariant::Ghost)
                        .start_icon(Icon::new("funnel"))
                        .label_hidden(true)
                        .trigger_label(filter_label)
                        .on_change(move |next, window, cx| {
                            set_filters.update(cx, |v, _| *v = next);
                            window.refresh();
                        })
                        .on_open_change({
                            let (draft, filters) = (draft.clone(), filters.clone());
                            move |open, _, cx| {
                                if open {
                                    let now = filters.read(cx).clone();
                                    draft.update(cx, |d, _| *d = now);
                                }
                            }
                        })
                        .content(move |_, commit, close, _, _, cx| {
                            let (pick, apply) = (draft.clone(), draft.clone());
                            Stack::vertical()
                                .gap(SpacingStep::S3)
                                .child(
                                    CheckboxList::new("status-options", "Status")
                                        .label_hidden(true)
                                        .value(draft.read(cx).clone())
                                        .on_change(move |next, window, cx| {
                                            pick.update(cx, |d, _| *d = next);
                                            window.refresh();
                                        })
                                        .item(CheckboxListItem::new("Open").value("Open"))
                                        .item(CheckboxListItem::new("In review").value("In review"))
                                        .item(CheckboxListItem::new("Closed").value("Closed")),
                                )
                                .child(
                                    Button::new("status-apply", "Apply")
                                        .variant(ButtonVariant::Primary)
                                        .on_click(move |_, window, cx| {
                                            let chosen = apply.read(cx).clone();
                                            commit(chosen, window, cx);
                                            close(window, cx);
                                        }),
                                )
                                .into_any_element()
                        }),
                ),
        )
        .into_any_element()
}
