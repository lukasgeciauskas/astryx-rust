//! Table examples: the gallery's Tables page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use crate::containers::Card;
use crate::layout::{SpacingStep, Stack};
use crate::people::{Avatar, AvatarSize};
use crate::status::{StatusDot, StatusDotVariant};
use crate::table::{
    Record, Table, TableBody, TableCell, TableColumn, TableColumnAlign, TableDensity,
    TableDividers, TableFooter, TableHeader, TableHeaderCell, TableRecord, TableRow,
    TableTextOverflow, TableValue, pixel, proportional,
};
use crate::text::{Text, TextType};

/// A person, read by key as upstream reads `item[key]`.
#[derive(Clone)]
pub struct Person {
    pub id: u32,
    pub name: &'static str,
    pub role: &'static str,
    pub active: bool,
    pub joined: i64,
    pub age: u32,
}

impl TableRecord for Person {
    fn value(&self, key: &str) -> TableValue {
        match key {
            "id" => self.id.into(),
            "name" => self.name.into(),
            "role" => self.role.into(),
            "active" => self.active.into(),
            "joined" => TableValue::Date(Some(self.joined)),
            "age" => self.age.into(),
            _ => TableValue::Null,
        }
    }
}

pub fn people() -> Vec<Person> {
    let day = 86_400_000;
    vec![
        Person {
            id: 1,
            name: "Ada Lovelace",
            role: "Analyst",
            active: true,
            joined: 1_700_000_000_000,
            age: 36,
        },
        Person {
            id: 2,
            name: "Grace Hopper",
            role: "Admiral",
            active: true,
            joined: 1_700_000_000_000 + 40 * day,
            age: 85,
        },
        Person {
            id: 3,
            name: "Alan Turing",
            role: "Mathematician",
            active: false,
            joined: 1_700_000_000_000 + 90 * day,
            age: 41,
        },
        Person {
            id: 4,
            name: "Katherine Johnson",
            role: "Mathematician",
            active: true,
            joined: 1_700_000_000_000 + 120 * day,
            age: 101,
        },
        Person {
            id: 5,
            name: "Edsger Dijkstra",
            role: "Programmer",
            active: false,
            joined: 1_700_000_000_000 + 200 * day,
            age: 72,
        },
    ]
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let columns = vec![
        TableColumn::new("name")
            .header("Name")
            .width(proportional(2.))
            .render_cell(|p: &Person, _, _| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        Avatar::new(("avatar", p.id as usize))
                            .name(p.name)
                            .size(AvatarSize::Xsm),
                    )
                    .child(p.name)
                    .into_any_element()
            }),
        TableColumn::new("role").header("Role"),
        TableColumn::new("active")
            .header("Status")
            .render_cell(|p: &Person, _, _| {
                let (variant, text) = if p.active {
                    (StatusDotVariant::Success, "Active")
                } else {
                    (StatusDotVariant::Neutral, "Away")
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(StatusDot::new(("status", p.id as usize), variant, text))
                    .child(text)
                    .into_any_element()
            }),
        TableColumn::new("age")
            .header("Age")
            .width(pixel(72.))
            .align(TableColumnAlign::End),
    ];
    let notes = vec![
        Record::new().with("event", "Deploy").with("note", "Rolled out the new billing pipeline to every region after the canary held for a day").with("owner", "Ada"),
        Record::new().with("event", "Incident").with("note", "Latency spike in eu-west traced to a cold cache after the failover drill").with("owner", "Grace"),
        Record::new().with("event", "Review").with("note", "Quarterly access review closed with two stale accounts removed").with("owner", "Alan"),
    ];
    Stack::vertical()
        .gap(SpacingStep::S6)
        .child(label("Table: rich cells, striped rows, hover"))
        .child(
            Table::new("people")
                .label("People")
                .data(people())
                .columns(columns)
                .id_key("id")
                .striped(true)
                .hover(true),
        )
        .child(label("Sortable: press a header; Shift+press adds a key"))
        .child(sortable(window, cx))
        .child(label(
            "Selection: checkboxes, select-all, the bulk-actions toolbar (Edsger is locked)",
        ))
        .child(selection(window, cx))
        .child(label("Row status and row numbers"))
        .child(status_and_index())
        .child(label(
            "Activation: press a row, or focus it and press Enter, to show it beside the table",
        ))
        .child(activation(window, cx))
        .child(label(
            "Compact, grid dividers; a date reads as its ISO time",
        ))
        .child(
            Table::new("generated")
                .data(people())
                .columns(vec![
                    TableColumn::new("id").header("Id").width(pixel(48.)),
                    TableColumn::new("name").header("Name"),
                    TableColumn::new("joined").header("Joined"),
                ])
                .density(TableDensity::Compact)
                .dividers(TableDividers::Grid),
        )
        .child(label("Truncated text, in a card it bleeds to the edges of"))
        .child(
            Card::new().w(px(520.)).child(
                Table::new("notes")
                    .data(notes)
                    .text_overflow(TableTextOverflow::Truncate)
                    .density(TableDensity::Spacious),
            ),
        )
        .child(label(
            "Composed: header, body and footer; columns fit their content",
        ))
        .child(
            Table::composed("composed")
                .child(
                    TableHeader::new().child(
                        TableRow::new()
                            .child(TableHeaderCell::new().child("Item"))
                            .child(TableHeaderCell::new().child("Qty"))
                            .child(
                                TableHeaderCell::new()
                                    .child("Price")
                                    .align(TableColumnAlign::End),
                            ),
                    ),
                )
                .child(
                    TableBody::new()
                        .child(
                            TableRow::new()
                                .child(TableCell::new().child("Mechanical keyboard"))
                                .child(TableCell::new().child("1"))
                                .child(
                                    TableCell::new()
                                        .child("$129.00")
                                        .align(TableColumnAlign::End),
                                ),
                        )
                        .child(
                            TableRow::new()
                                .child(TableCell::new().child("USB-C cable"))
                                .child(TableCell::new().child("3"))
                                .child(
                                    TableCell::new()
                                        .child("$27.00")
                                        .align(TableColumnAlign::End),
                                ),
                        ),
                )
                .child(
                    TableFooter::new().child(
                        TableRow::new()
                            .child(TableCell::new().col_span(2).child("Total"))
                            .child(
                                TableCell::new()
                                    .child("$156.00")
                                    .align(TableColumnAlign::End),
                            ),
                    ),
                ),
        )
        .child(label("Empty"))
        .child(Table::new("empty").data(Vec::<Person>::new()).columns(vec![
            TableColumn::new("name").header("Name"),
            TableColumn::new("role").header("Role"),
        ]))
        .into_any_element()
}

/// Row selection with its own state and the bulk-actions toolbar.
fn selection(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::button::Button;
    use crate::table::{SelectionStateConfig, TableSelectionToolbar, use_table_selection_state};
    // The caller owns the keys here; Edsger is locked while selected.
    let keys = window.use_keyed_state("people-selected", cx, |_, _| {
        std::collections::HashSet::from([gpui::SharedString::from("2"), "5".into()])
    });
    let current = keys.read(cx).clone();
    let selection = use_table_selection_state(
        "people-selection",
        SelectionStateConfig::new(people(), "id")
            .selected_keys(current, move |next, window, cx| {
                keys.update(cx, |k, _| *k = next);
                window.refresh();
            })
            .is_item_enabled(|p: &Person| p.id != 5),
        window,
        cx,
    );
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(
            TableSelectionToolbar::new("people-bulk", selection.state.clone())
                .start_content(Button::new("people-archive", "Archive")),
        )
        .child(
            Table::new("people-selectable")
                .label("People, selectable")
                .data(people())
                .columns(vec![
                    TableColumn::new("name")
                        .header("Name")
                        .width(proportional(2.)),
                    TableColumn::new("role").header("Role"),
                ])
                .id_key("id")
                .plugin(
                    "selection",
                    selection.plugin.row_label(|p: &Person| p.name.into()),
                ),
        )
        .into_any_element()
}

/// A status gutter and row numbers.
fn status_and_index() -> AnyElement {
    use crate::table::{TableRowIndex, TableRowStatus, TableRowStatusColor, use_table_row_status};
    Table::new("people-status")
        .label("People, with status")
        .data(people())
        .columns(vec![
            TableColumn::new("name")
                .header("Name")
                .width(proportional(2.)),
            TableColumn::new("role").header("Role"),
        ])
        .id_key("id")
        .plugin(
            "rowStatus",
            use_table_row_status(|p: &Person| match (p.active, p.age) {
                (_, a) if a > 100 => Some(TableRowStatus::warning("Over a hundred")),
                (true, _) => Some(TableRowStatus::success("Active")),
                (false, a) if a > 70 => {
                    Some(TableRowStatus::color(TableRowStatusColor::Blue, "Emeritus"))
                }
                (false, _) => Some(TableRowStatus::error("Inactive")),
            }),
        )
        .plugin("rowIndex", TableRowIndex::new())
        .into_any_element()
}

/// Master-detail: the active row drives the panel beside the table.
fn activation(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::table::{RowKey, TableActivation};
    let active = window.use_keyed_state("people-active", cx, |_, _| Some(RowKey::from("2")));
    let key = active.read(cx).clone();
    let current = people()
        .into_iter()
        .find(|p| key.as_ref() == Some(&RowKey::from(p.id.to_string())));
    let state = active.clone();
    let plugin = TableActivation::new(key, move |_: &Person, key, window, cx| {
        let key = key.clone();
        state.update(cx, |k, _| *k = Some(key));
        window.refresh();
    })
    .is_item_activatable(|p: &Person| p.active || p.id != 3);
    Stack::horizontal()
        .gap(SpacingStep::S4)
        .child(
            div().flex_1().min_w_0().child(
                Table::new("people-activation")
                    .label("People, activatable")
                    .data(people())
                    .columns(vec![
                        TableColumn::new("name")
                            .header("Name")
                            .width(proportional(2.)),
                        TableColumn::new("role").header("Role"),
                    ])
                    .id_key("id")
                    .hover(true)
                    .plugin("activation", plugin),
            ),
        )
        .child(
            Card::new().w(px(220.)).child(match current {
                Some(p) => Stack::vertical()
                    .gap(SpacingStep::S1)
                    .child(Text::new(p.name).text_type(TextType::Label))
                    .child(Text::new(p.role).text_type(TextType::Supporting))
                    .into_any_element(),
                None => Text::new("No row is active")
                    .text_type(TextType::Supporting)
                    .into_any_element(),
            }),
        )
        .into_any_element()
}

/// A table sorted locally by its own sort state (useTableSortableState).
fn sortable(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::table::{SortEntry, SortableStateConfig, use_table_sortable_state};
    let state = use_table_sortable_state(
        "people-sort",
        SortableStateConfig::new(people())
            .default_sort(vec![SortEntry::ascending("name")])
            .multi_sort(true),
        window,
        cx,
    );
    Table::new("people-sorted")
        .label("People, sortable")
        .data(state.sorted_data.clone())
        .columns(vec![
            TableColumn::new("name")
                .header("Name")
                .width(proportional(2.))
                .sortable(true),
            TableColumn::new("role").header("Role").sortable(true),
            TableColumn::new("age")
                .header("Age")
                .width(pixel(96.))
                .align(TableColumnAlign::End)
                .sortable(true),
        ])
        .id_key("id")
        .plugin("sort", state.plugin.clone())
        .into_any_element()
}
