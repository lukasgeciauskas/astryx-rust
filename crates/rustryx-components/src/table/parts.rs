//! Children mode's parts (upstream `TableHeader`, `TableBody`,
//! `TableFooter`, `TableRow`, `TableHeaderCell` and `TableCell`): the
//! caller composes them inside [`Table::composed`](super::Table::composed).
//!
//! The table is one grid; sections and rows lay their cells out as its own
//! (`display: contents`). Each cell takes the next column its row has free,
//! and paints its row's fill (stripe, hover), since a row has no box of its
//! own. Outside a table each part renders plain.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, Entity, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    Role, SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Window, div,
    prelude::*,
};
use rustryx_primitives::{provide, use_context};
use rustryx_theme::current;
use smallvec::SmallVec;

use super::cells::{CellPlace, RowState, TableStyle, cell, row_background, row_theme_props};
use super::model::TableColumnAlign;
use super::plugin::TableContextActions;

/// What a composed table tells its parts.
#[derive(Clone)]
pub(crate) struct TableParts {
    pub style: TableStyle,
    /// The table's id, for its rows' state.
    pub table: SharedString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Header,
    Body,
    Footer,
}

impl Section {
    fn as_str(self) -> &'static str {
        match self {
            Section::Header => "header",
            Section::Body => "body",
            Section::Footer => "footer",
        }
    }
}

/// A row's place in its section.
#[derive(Clone, Copy)]
struct RowPlace {
    section: Section,
    index: usize,
    last: bool,
}

/// What a row tells its cells: the next free column, its place and paint.
#[derive(Clone)]
struct RowCells {
    next_column: Rc<Cell<u16>>,
    place: RowPlace,
    hovered: Option<Entity<bool>>,
}

/// A cell's place among its row's children.
#[derive(Clone, Copy)]
struct CellOrdinal {
    index: usize,
    count: usize,
}

macro_rules! section {
    ($name:ident, $section:expr, $target:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(IntoElement, Default)]
        pub struct $name {
            children: SmallVec<[AnyElement; 8]>,
            style: StyleRefinement,
        }

        impl $name {
            pub fn new() -> Self {
                Self::default()
            }
        }

        impl ParentElement for $name {
            fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
                self.children.extend(elements);
            }
        }

        impl Styled for $name {
            fn style(&mut self) -> &mut StyleRefinement {
                &mut self.style
            }
        }

        impl RenderOnce for $name {
            fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
                let theme = current(window, cx);
                let parts = use_context::<TableParts>(cx);
                let count = self.children.len();
                let table = parts
                    .as_ref()
                    .map_or_else(|| SharedString::from("table"), |p| p.table.clone());
                let mut el = div()
                    .id(ElementId::Name(
                        format!("{}-{}", table, $section.as_str()).into(),
                    ))
                    .role(Role::RowGroup);
                if parts.is_some() {
                    el = el.display_contents();
                }
                theme.refine(el.style(), $target, &[], None);
                el.style().refine(&self.style);
                el.children(self.children.into_iter().enumerate().map(|(index, row)| {
                    provide(
                        RowPlace {
                            section: $section,
                            index,
                            last: index + 1 == count,
                        },
                        row,
                    )
                }))
            }
        }
    };
}

section!(
    TableHeader,
    Section::Header,
    "table-header",
    "The header rows of a composed table (upstream `TableHeader`)."
);
section!(
    TableBody,
    Section::Body,
    "table-body",
    "The body rows of a composed table (upstream `TableBody`)."
);
section!(
    TableFooter,
    Section::Footer,
    "table-footer",
    "The footer rows of a composed table (upstream `TableFooter`)."
);

/// A row of a composed table (upstream `TableRow`): its cells, in order.
#[derive(IntoElement, Default)]
pub struct TableRow {
    children: SmallVec<[AnyElement; 8]>,
    style: StyleRefinement,
}

impl TableRow {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for TableRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for TableRow {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TableRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let parts = use_context::<TableParts>(cx);
        let place = use_context::<RowPlace>(cx).unwrap_or(RowPlace {
            section: Section::Body,
            index: 0,
            last: true,
        });
        let body = place.section == Section::Body;
        let table = parts
            .as_ref()
            .map_or_else(|| SharedString::from("table"), |p| p.table.clone());
        let mut el = div()
            .id(ElementId::Name(
                format!("{}-{}-row-{}", table, place.section.as_str(), place.index).into(),
            ))
            .role(Role::Row);
        let hovered = match &parts {
            Some(parts) => {
                el = el.display_contents();
                (parts.style.hover && body).then(|| {
                    window.use_keyed_state(
                        ElementId::Name(
                            format!(
                                "{}-{}-row-{}-hover",
                                parts.table,
                                place.section.as_str(),
                                place.index
                            )
                            .into(),
                        ),
                        cx,
                        |_, _| false,
                    )
                })
            }
            None => None,
        };
        let theme_props = match &parts {
            Some(p) if place.section != Section::Header => {
                row_theme_props(&p.style, body.then_some(place.index))
            }
            _ => Vec::new(),
        };
        theme.refine(el.style(), "table-row", &theme_props, None);
        el.style().refine(&self.style);
        let count = self.children.len();
        let cells = RowCells {
            next_column: Rc::new(Cell::new(1)),
            place,
            hovered,
        };
        el.children(self.children.into_iter().enumerate().map(|(index, child)| {
            provide(cells.clone(), provide(CellOrdinal { index, count }, child))
        }))
    }
}

macro_rules! table_cell {
    ($name:ident, $header:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(IntoElement, Default)]
        pub struct $name {
            children: SmallVec<[AnyElement; 2]>,
            col_span: u16,
            align: Option<TableColumnAlign>,
            context_menu: Vec<TableContextActions>,
            style: StyleRefinement,
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    col_span: 1,
                    ..Self::default()
                }
            }

            /// Columns it spans (HTML `colSpan`).
            pub fn col_span(mut self, span: u16) -> Self {
                self.col_span = span.max(1);
                self
            }

            /// Its content's horizontal alignment.
            pub fn align(mut self, align: TableColumnAlign) -> Self {
                self.align = Some(align);
                self
            }

            /// Right-click actions for the cell (upstream `contextMenuActions`).
            pub fn context_menu(mut self, actions: TableContextActions) -> Self {
                self.context_menu.push(actions);
                self
            }
        }

        impl ParentElement for $name {
            fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
                self.children.extend(elements);
            }
        }

        impl Styled for $name {
            fn style(&mut self) -> &mut StyleRefinement {
                &mut self.style
            }
        }

        impl RenderOnce for $name {
            fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
                render_cell(
                    $header,
                    self.children,
                    self.col_span,
                    self.align,
                    self.context_menu,
                    self.style,
                    window,
                    cx,
                )
            }
        }
    };
}

table_cell!(
    TableHeaderCell,
    true,
    "A header cell (upstream `TableHeaderCell`, a `<th>`)."
);
table_cell!(TableCell, false, "A cell (upstream `TableCell`, a `<td>`).");

#[allow(clippy::too_many_arguments)]
fn render_cell(
    header: bool,
    children: SmallVec<[AnyElement; 2]>,
    span: u16,
    align: Option<TableColumnAlign>,
    actions: Vec<TableContextActions>,
    style: StyleRefinement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let rem = window.rem_size();
    let (Some(parts), Some(row)) = (use_context::<TableParts>(cx), use_context::<RowCells>(cx))
    else {
        let mut el = div().id("table-cell");
        el.style().refine(&style);
        return el
            .role(if header {
                Role::ColumnHeader
            } else {
                Role::Cell
            })
            .children(children)
            .into_any_element();
    };
    let ordinal = use_context::<CellOrdinal>(cx).unwrap_or(CellOrdinal { index: 0, count: 1 });
    // The next free column of its row (grid auto-placement keeps the row).
    let column = row.next_column.get();
    row.next_column.set(column + span);
    let body = row.place.section == Section::Body;
    let place = CellPlace {
        header: header || row.place.section == Section::Header,
        first: column == 1,
        last: ordinal.index + 1 == ordinal.count,
        // No divider under any section's last row (the header's always
        // separates it from the body).
        last_row: !header && row.place.section != Section::Header && row.place.last,
        align,
    };
    let s = parts.style;
    let id = ElementId::Name(
        format!(
            "{}-{}-{}-{}",
            parts.table,
            row.place.section.as_str(),
            row.place.index,
            ordinal.index
        )
        .into(),
    );
    let selector = format!(
        "{}-{}-{}-{}",
        parts.table,
        row.place.section.as_str(),
        row.place.index,
        ordinal.index
    );
    let mut el = cell(&theme, &s, place, rem)
        .id(id.clone())
        .debug_selector(move || selector.clone())
        .col_start(column as i16)
        .col_end((column + span) as i16)
        .role(if header {
            Role::ColumnHeader
        } else {
            Role::Cell
        });
    // The row's fill, painted by each of its cells.
    if body {
        let hovered = row.hovered.as_ref().is_some_and(|h| *h.read(cx));
        if let Some(bg) = row_background(
            &theme,
            &s,
            RowState {
                index: row.place.index,
                hovered,
                selected_wash: false,
                active: false,
            },
        ) {
            el = el.bg(bg);
        }
        if let Some(state) = row.hovered.clone() {
            el = el.on_hover(move |over, window, cx| {
                state.update(cx, |h, _| *h = *over);
                window.refresh();
            });
        }
    }
    let target = if header {
        "table-header-cell"
    } else {
        "table-cell"
    };
    theme.refine(el.style(), target, &[("density", s.density.as_str())], None);
    el.style().refine(&style);
    let el = el.children(children);
    super::menu::with_actions(
        el,
        ElementId::Name(format!("{id:?}-menu").into()),
        actions,
        None,
        window,
        cx,
    )
    .into_any_element()
}
