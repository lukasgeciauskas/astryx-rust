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

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
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
