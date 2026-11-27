//! PowerSearch examples: the gallery's Collections page and the doc example.

use std::rc::Rc;

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::layout::{SpacingStep, Stack};
use crate::power_search::{
    EnumItem, FilterValue, OperatorValue, PowerSearch, PowerSearchConfig, PowerSearchField,
    PowerSearchFilter, PowerSearchOperator,
};
use crate::search::{SearchItem, static_source};
use crate::text::{Text, TextType};

fn books() -> Rc<PowerSearchConfig> {
    let genres = [
        "Fiction",
        "Science fiction",
        "History",
        "Poetry",
        "Biography",
    ]
    .map(|g| EnumItem::new(g.to_lowercase().replace(' ', "_"), g));
    let people = static_source(
        [
            "Ada Lovelace",
            "Grace Hopper",
            "Alan Turing",
            "Katherine Johnson",
        ]
        .map(|n| SearchItem::new(n.to_lowercase().replace(' ', "-"), n)),
    );
    let contains = || {
        PowerSearchOperator::keyed(
            "contains",
            "@astryx.powersearch.operator.contains",
            OperatorValue::string(),
        )
    };
    Rc::new(
        PowerSearchConfig::new("books")
            .content_search_field("text")
            .field(PowerSearchField::new("text", "Text").operator(contains()))
            .field(
                PowerSearchField::new("title", "Title")
                    .operator(contains().value_only_match(true))
                    .operator(PowerSearchOperator::keyed(
                        "is",
                        "@astryx.powersearch.operator.is",
                        OperatorValue::string(),
                    )),
            )
            .field(
                PowerSearchField::new("genre", "Genre")
                    .group("Catalog")
                    .operator(
                        PowerSearchOperator::keyed(
                            "is",
                            "@astryx.powersearch.operator.is",
                            OperatorValue::Enum {
                                values: genres.to_vec(),
                            },
                        )
                        .value_only_match(true),
                    )
                    .operator(PowerSearchOperator::keyed(
                        "is_any_of",
                        "@astryx.powersearch.operator.isAnyOf",
                        OperatorValue::EnumList {
                            values: genres.to_vec(),
                        },
                    )),
            )
            .field(
                PowerSearchField::new("pages", "Pages")
                    .group("Catalog")
                    .operator(PowerSearchOperator::keyed(
                        "greater_than",
                        "@astryx.powersearch.operator.greaterThan",
                        OperatorValue::Integer {
                            min: Some(1),
                            max: None,
                            units: Some("pages".into()),
                        },
                    )),
            )
            .field(
                PowerSearchField::new("author", "Author").operator(
                    PowerSearchOperator::keyed(
                        "is",
                        "@astryx.powersearch.operator.is",
                        OperatorValue::EntityList {
                            search_source: Some(Rc::new(people)),
                            arbitrary_allowed: false,
                            tokenization: None,
                            render_item: None,
                        },
                    )
                    .value_only_match(true),
                ),
            )
            .field(PowerSearchField::new("starred", "Starred").operator(
                PowerSearchOperator::keyed(
                    "is_true",
                    "@astryx.powersearch.operator.isTrue",
                    OperatorValue::Empty,
                ),
            )),
    )
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let filters = window.use_keyed_state("power-search-filters", cx, |_, _| {
        vec![
            PowerSearchFilter::new("genre", "is", FilterValue::Enum("fiction".into()))
                .read_only(true),
            PowerSearchFilter::new("title", "contains", FilterValue::String("dune".into())),
        ]
    });
    let store = filters.clone();
    let count = 12 + filters.read(cx).len() * 7;
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label(
            "PowerSearch: a read-only filter, value-only matches, a result count",
        ))
        .child(
            PowerSearch::new(
                "books",
                books(),
                filters.read(cx).clone(),
                move |next, _, _, window, cx| {
                    store.update(cx, |f, _| *f = next);
                    window.refresh();
                },
            )
            .result_count(count)
            .w(px(720.)),
        )
        .into_any_element()
}
