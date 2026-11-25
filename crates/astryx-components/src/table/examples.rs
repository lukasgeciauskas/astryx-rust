//! Table examples: the gallery's Tables page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use crate::containers::Card;
use crate::layout::{SpacingStep, Stack};
use crate::people::{Avatar, AvatarSize};
use crate::power_search::{
    EnumItem, FieldDefinition, FieldKind, FilterRow, RowValue, create_power_search_config,
};
use crate::status::{StatusDot, StatusDotVariant};
use crate::table::TableFilterVariant;
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

impl FilterRow for Person {
    fn value(&self, field: &str) -> Option<RowValue> {
        Some(match field {
            "name" => RowValue::String(self.name.into()),
            "role" => RowValue::String(self.role.into()),
            "age" => RowValue::Number(self.age as f64),
            _ => return None,
        })
    }
}

/// A file or folder, for the tree.
#[derive(Clone)]
pub struct Node {
    pub id: &'static str,
    pub name: &'static str,
    pub size: &'static str,
    pub children: Vec<Node>,
}

impl TableRecord for Node {
    fn value(&self, key: &str) -> TableValue {
        match key {
            "id" => self.id.into(),
            "name" => self.name.into(),
            "size" => self.size.into(),
            _ => TableValue::Null,
        }
    }
}

fn leaf(id: &'static str, name: &'static str, size: &'static str) -> Node {
    Node {
        id,
        name,
        size,
        children: Vec::new(),
    }
}

fn folder(id: &'static str, name: &'static str, children: Vec<Node>) -> Node {
    Node {
        id,
        name,
        size: "",
        children,
    }
}

fn files() -> Vec<Node> {
    vec![
        folder(
            "src",
            "src",
            vec![
                folder(
                    "components",
                    "components",
                    vec![
                        leaf("button", "button.rs", "12 KB"),
                        leaf("table", "table.rs", "48 KB"),
                    ],
                ),
                leaf("lib", "lib.rs", "2 KB"),
            ],
        ),
        folder("docs", "docs", vec![leaf("plan", "PLAN.md", "30 KB")]),
        leaf("readme", "README.md", "4 KB"),
    ]
}

/// Rows numbered from `start`, for the paged and loaded tables.
fn items(start: usize, count: usize) -> Vec<Record> {
    (start..start + count)
        .map(|n| {
            Record::new()
                .with("id", n as i64)
                .with("item", format!("Item {n}"))
                .with("owner", ["Ada", "Grace", "Alan", "Katherine"][n % 4])
        })
        .collect()
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
        .child(label("Pagination: five rows a page"))
        .child(paginated(window, cx))
        .child(label(
            "Filtering in popovers: the funnel opens each column's filter",
        ))
        .child(filtered(TableFilterVariant::Popover, window, cx))
        .child(label("Filtering inline, below the headers"))
        .child(filtered(TableFilterVariant::Inline, window, cx))
        .child(label(
            "Infinite scroll: the next batch loads as the end comes into view",
        ))
        .child(infinite(window, cx))
        .child(label(
            "Tree data: nested rows with expanders and expand-all",
        ))
        .child(tree(window, cx))
        .child(label("Grouped rows: collapsible sections by role"))
        .child(grouped(window, cx))
        .child(label("Row expansion: a detail panel below its row"))
        .child(expansion(window, cx))
        .child(label("Column settings: pick the columns (Name always shows)"))
        .child(column_settings(window, cx))
        .child(label(
            "Sticky and resizable columns: Name and Status stay put as the table scrolls; drag a header's edge",
        ))
        .child(sticky_resizable(window, cx))
        .child(label(
            "Virtualization: 10,000 rows and 40 columns; only the cells near the viewport mount",
        ))
        .child(virtualized(window, cx))
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

/// Nested rows with their own expanded state.
fn tree(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::table::{TreeStateConfig, use_table_tree_state};
    let state = use_table_tree_state(
        "files-tree",
        TreeStateConfig::new(files(), "id", |n: &Node| &n.children[..])
            .default_expanded_ids(["src"]),
        window,
        cx,
    );
    Table::new("files")
        .label("Files")
        .data(state.visible_data.clone())
        .columns(vec![
            TableColumn::new("name")
                .header("Name")
                .width(proportional(2.)),
            TableColumn::new("size")
                .header("Size")
                .align(TableColumnAlign::End),
        ])
        .id_key("id")
        .plugin(
            "tree",
            state
                .plugin
                .expand_all_control(true)
                .row_label(|n: &Node| n.name.into()),
        )
        .into_any_element()
}

/// People in sections by role; a header click collapses its section.
fn grouped(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::table::{GroupedRowsConfig, use_table_grouped_rows};
    let collapsed = window.use_keyed_state("people-collapsed", cx, |_, _| {
        std::collections::HashSet::from([gpui::SharedString::from("Programmer")])
    });
    let current = collapsed.read(cx).clone();
    let grouped = use_table_grouped_rows(
        GroupedRowsConfig::new(
            people(),
            |p: &Person| p.role.into(),
            current,
            move |key, window, cx| {
                collapsed.update(cx, |set, _| {
                    if !set.remove(&key) {
                        set.insert(key);
                    }
                });
                window.refresh();
            },
        )
        .row_key(|p: &Person| p.id.to_string().into()),
    );
    Table::new("people-grouped")
        .label("People by role")
        .data(grouped.data.clone())
        .columns(vec![
            TableColumn::new("name")
                .header("Name")
                .width(proportional(2.)),
            TableColumn::new("age")
                .header("Age")
                .align(TableColumnAlign::End),
        ])
        .id_key_with(grouped.id_key)
        .plugin("grouped", grouped.plugin)
        .into_any_element()
}

/// A panel with the person's details below their row.
fn expansion(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::table::{PanelVariant, TableRowExpansion};
    let open = window.use_keyed_state("people-open", cx, |_, _| {
        std::collections::HashSet::from([gpui::SharedString::from("2")])
    });
    let current = open.read(cx).clone();
    Table::new("people-expandable")
        .label("People, expandable")
        .data(people())
        .columns(vec![
            TableColumn::new("name")
                .header("Name")
                .width(proportional(2.)),
            TableColumn::new("role").header("Role"),
        ])
        .id_key("id")
        .plugin(
            "expansion",
            TableRowExpansion::new(
                current,
                move |key, window, cx| {
                    open.update(cx, |set, _| {
                        if !set.remove(&key) {
                            set.insert(key);
                        }
                    });
                    window.refresh();
                },
                |p: &Person| p.id.to_string().into(),
                |p, _, _| {
                    Text::new(format!(
                        "{} is {} and {}.",
                        p.name,
                        p.age,
                        if p.active { "active" } else { "away" }
                    ))
                    .text_type(TextType::Supporting)
                    .into_any_element()
                },
            )
            .row_label(|p: &Person| p.name.into())
            .panel_variant(PanelVariant::Muted),
        )
        .into_any_element()
}

/// Checkboxes choose the columns the table shows.
fn column_settings(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::choice::CheckboxInput;
    use crate::table::{
        ColumnSettingsConfig, ColumnSettingsOption, use_table_column_settings,
        use_table_column_settings_state,
    };
    let active = window.use_keyed_state("people-columns", cx, |_, _| {
        vec![gpui::SharedString::from("name"), "role".into()]
    });
    let current = active.read(cx).clone();
    let state = use_table_column_settings_state(ColumnSettingsConfig::new(
        [
            ColumnSettingsOption::new("name", "Name").always_visible(true),
            ColumnSettingsOption::new("role", "Role"),
            ColumnSettingsOption::new("age", "Age"),
        ],
        current,
        move |keys, window, cx| {
            active.update(cx, |a, _| *a = keys);
            window.refresh();
        },
    ));
    let picker = state.config.columns.iter().fold(
        Stack::horizontal().gap(SpacingStep::S4),
        |row, option| {
            let (state, key) = (state.clone(), option.key.clone());
            row.child(
                CheckboxInput::new(
                    gpui::SharedString::from(format!("column-{}", option.key)),
                    option.label.clone(),
                    state.is_column_active(&option.key),
                )
                .disabled(!state.is_column_toggleable(&option.key))
                .on_change(move |_, window, cx| state.toggle_column(&key, window, cx)),
            )
        },
    );
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(picker)
        .child(
            Table::new("people-columns-table")
                .label("People, chosen columns")
                .data(people())
                .columns(vec![
                    TableColumn::new("name")
                        .header("Name")
                        .width(proportional(2.)),
                    TableColumn::new("role").header("Role"),
                    TableColumn::new("age")
                        .header("Age")
                        .align(TableColumnAlign::End),
                ])
                .id_key("id")
                .plugin("columnSettings", use_table_column_settings(&state.config)),
        )
        .into_any_element()
}

/// A narrow table: pinned start and end columns, and drag-resizable widths.
fn sticky_resizable(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::table::{ColumnResizeConfig, TableStickyColumns, use_table_column_resize};
    let widths = window.use_keyed_state("people-widths", cx, |_, _| {
        std::collections::HashMap::<gpui::SharedString, f32>::new()
    });
    let current = widths.read(cx).clone();
    let columns = || {
        vec![
            TableColumn::new("name").header("Name").width(pixel(180.)),
            TableColumn::new("role").header("Role").width(pixel(200.)),
            TableColumn::new("joined")
                .header("Joined")
                .width(pixel(260.)),
            TableColumn::new("age").header("Age").width(pixel(120.)),
            TableColumn::new("active")
                .header("Status")
                .width(pixel(110.)),
        ]
    };
    let resize = use_table_column_resize(
        "people-resize",
        ColumnResizeConfig::new()
            .column_widths(current)
            .columns(columns())
            .on_column_resize_end(move |updates, window, cx| {
                widths.update(cx, |w, _| w.extend(updates));
                window.refresh();
            }),
        window,
        cx,
    );
    div()
        .w(px(560.))
        .child(
            Table::new("people-sticky")
                .label("People, pinned columns")
                .data(people())
                .columns(columns())
                .id_key("id")
                .striped(true)
                .hover(true)
                .plugin(
                    "sticky",
                    TableStickyColumns::new()
                        .start_keys(["name"])
                        .end_keys(["active"]),
                )
                .plugin("columnResize", resize),
        )
        .into_any_element()
}

/// Rows and columns windowed in the table's own viewport.
fn virtualized(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::table::{
        TableRowIndex, TableStickyColumns, use_table_columns_virtualization,
        use_table_rows_virtualization,
    };
    // Built once, kept between frames.
    let rows = window.use_keyed_state("virtual-rows", cx, |_, _| {
        let rows: std::rc::Rc<[Record]> = (0..10_000)
            .map(|i| {
                (0..40).fold(Record::new().with("id", i as i64), |r, c| {
                    r.with(format!("c{c}"), format!("R{i} C{c}"))
                })
            })
            .collect();
        rows
    });
    let rows = rows.read(cx).clone();
    let columns: Vec<TableColumn<Record>> = (0..40)
        .map(|c| {
            TableColumn::new(format!("c{c}"))
                .header(format!("Column {c}"))
                .width(pixel(120.))
        })
        .collect();
    div()
        .h(px(320.))
        .child(
            Table::new("virtual")
                .label("Ten thousand rows")
                .data(rows)
                .columns(columns)
                .id_key("id")
                .density(TableDensity::Compact)
                .plugin("rowIndex", TableRowIndex::new())
                .plugin(
                    "sticky",
                    TableStickyColumns::new().start_keys(["__rowIndex"]),
                )
                .virtualization(
                    use_table_rows_virtualization(29., 320.)
                        .with(use_table_columns_virtualization(900.)),
                ),
        )
        .into_any_element()
}

/// The caller pages the rows; the plugin draws the controls.
fn paginated(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::table::{TablePagination, paginate_data};
    let page = window.use_keyed_state("items-page", cx, |_, _| 1usize);
    let current = *page.read(cx);
    let all = items(1, 23);
    Table::new("items-paged")
        .label("Items, paged")
        .data(paginate_data(&all, current, 5))
        .columns(vec![
            TableColumn::new("item").header("Item"),
            TableColumn::new("owner").header("Owner"),
        ])
        .id_key("id")
        .plugin(
            "pagination",
            TablePagination::new(current, move |next, window, cx| {
                page.update(cx, |p, _| *p = next);
                window.refresh();
            })
            .total_items(all.len())
            .page_size(5),
        )
        .into_any_element()
}

/// Filters from a PowerSearch config, applied with `apply_filters`.
fn filtered(variant: TableFilterVariant, window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::table::{TableFilterRef, TableFiltering, to_search_filters, use_table_filter_state};
    let search = create_power_search_config(
        [
            FieldDefinition::new("name", FieldKind::String).label("Name"),
            FieldDefinition::new("role", FieldKind::Enum)
                .label("Role")
                .enum_values(
                    ["Analyst", "Admiral", "Mathematician", "Programmer"]
                        .map(|r| EnumItem::new(r, r)),
                ),
            FieldDefinition::new("age", FieldKind::Number).label("Age"),
        ],
        None,
    );
    let id = match variant {
        TableFilterVariant::Popover => "people-filters-popover",
        _ => "people-filters-inline",
    };
    let state = use_table_filter_state(id, None, window, cx);
    let columns = vec![
        TableColumn::new("name")
            .header("Name")
            .width(proportional(2.))
            .filter(TableFilterRef::Field("name".into())),
        TableColumn::new("role")
            .header("Role")
            .filter(TableFilterRef::Field("role".into())),
        TableColumn::new("age")
            .header("Age")
            .align(TableColumnAlign::End)
            .filter(TableFilterRef::Field("age".into())),
    ];
    let filters = to_search_filters(&state.filters, &columns, &search.config);
    let rows = search.apply_filters(&filters, &people());
    Table::new(id)
        .label("People, filterable")
        .data(rows)
        .columns(columns)
        .id_key("id")
        .plugin(
            "filter",
            TableFiltering::new(
                state.filters.clone(),
                state.on_filter_change(),
                search.config.clone(),
            )
            .variant(variant),
        )
        .into_any_element()
}

/// Batches of five, up to twenty rows, each after a short wait.
fn infinite(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::table::use_table_infinite_scroll;
    let rows = window.use_keyed_state("items-loaded", cx, |_, _| items(1, 5));
    let loading = window.use_keyed_state("items-loading", cx, |_, _| false);
    let count = rows.read(cx).len();
    let (r, l) = (rows.clone(), loading.clone());
    let plugin = use_table_infinite_scroll(
        "items-infinite",
        count < 20,
        *loading.read(cx),
        move |window, cx| {
            let (rows, loading) = (r.clone(), l.clone());
            loading.update(cx, |v, _| *v = true);
            let wait = cx
                .background_executor()
                .timer(std::time::Duration::from_millis(600));
            window.spawn(cx, async move |cx| {
                wait.await;
                cx.update(|window, cx| {
                    rows.update(cx, |v, _| {
                        let next = items(v.len() + 1, 5);
                        v.extend(next);
                    });
                    loading.update(cx, |v, _| *v = false);
                    window.refresh();
                })
                .map_err(|_| ())
            })
        },
        window,
        cx,
    );
    Table::new("items-infinite-table")
        .label("Items, loaded in batches")
        .data(rows.read(cx).clone())
        .columns(vec![
            TableColumn::new("item").header("Item"),
            TableColumn::new("owner").header("Owner"),
        ])
        .id_key("id")
        .plugin("infiniteScroll", plugin)
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
