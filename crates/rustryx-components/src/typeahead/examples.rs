//! Typeahead examples: the gallery's Collections page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::layout::{SpacingStep, Stack};
use crate::search::{SearchItem, Searchable, static_source};
use crate::text::{Text, TextType};
use crate::typeahead::{Typeahead, TypeaheadItem};

fn people() -> Vec<SearchItem> {
    vec![
        SearchItem::new("ada", "Ada Lovelace").group("Engineering"),
        SearchItem::new("alan", "Alan Turing").group("Research"),
        SearchItem::new("grace", "Grace Hopper").group("Engineering"),
        SearchItem::new("katherine", "Katherine Johnson").group("Research"),
        SearchItem::new("margaret", "Margaret Hamilton"),
        SearchItem::new("radia", "Radia Perlman"),
    ]
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let assignee = window.use_keyed_state("typeahead-assignee", cx, |_, _| {
        Some(SearchItem::new("grace", "Grace Hopper"))
    });
    let reviewer = window.use_keyed_state("typeahead-reviewer", cx, |_, _| None::<SearchItem>);
    let (a, r) = (assignee.clone(), reviewer.clone());
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("Typeahead"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(
                    Typeahead::new("assignee", "Assignee", static_source(people()))
                        .value(assignee.read(cx).clone())
                        .placeholder("Search people…")
                        .on_change(move |item, window, cx| {
                            a.update(cx, |v, _| *v = item);
                            window.refresh();
                        })
                        .w(px(280.)),
                )
                .child(
                    Typeahead::new("reviewer", "Reviewer", static_source(people()))
                        .value(reviewer.read(cx).clone())
                        .entries_on_focus(true)
                        .description("Shows everyone on focus")
                        .render_item(|item, _, _| {
                            TypeaheadItem::new(Searchable::label(item))
                                .description(Searchable::group(item).unwrap_or("Contractor".into()))
                                .into_any_element()
                        })
                        .on_change(move |item, window, cx| {
                            r.update(cx, |v, _| *v = item);
                            window.refresh();
                        })
                        .w(px(280.)),
                ),
        )
        .into_any_element()
}
