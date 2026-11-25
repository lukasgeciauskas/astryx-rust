//! What every table cell and row shares (upstream `table.stylex.ts`,
//! `TableCell`, `TableHeaderCell` and `TableRow` styles): density
//! padding, the header's type, dividers, alignment, the container's edges
//! and a body row's paint.

use astryx_i18n::{Direction, LogicalStyled};
use astryx_theme::{ResolvedTheme, ThemeStyled};
use gpui::{Div, Pixels, Styled, div};

use super::model::TableColumnAlign;
use crate::layout::ContainerPadding;

/// Row density: the cells' padding (upstream `density`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableDensity {
    Compact,
    #[default]
    Balanced,
    Spacious,
}

impl TableDensity {
    pub fn as_str(self) -> &'static str {
        match self {
            TableDensity::Compact => "compact",
            TableDensity::Balanced => "balanced",
            TableDensity::Spacious => "spacious",
        }
    }
}

/// The rules between cells (upstream `dividers`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableDividers {
    #[default]
    Rows,
    Columns,
    Grid,
    None,
}

impl TableDividers {
    pub(crate) fn rows(self) -> bool {
        matches!(self, TableDividers::Rows | TableDividers::Grid)
    }

    pub(crate) fn columns(self) -> bool {
        matches!(self, TableDividers::Columns | TableDividers::Grid)
    }
}

/// Where a body cell's content sits in its row (upstream `verticalAlign`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableVerticalAlign {
    #[default]
    Middle,
    Top,
    Bottom,
}

/// What text longer than its column does (upstream `textOverflow`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableTextOverflow {
    /// It wraps and the row grows.
    #[default]
    Wrap,
    /// It is cut with an ellipsis; a default-rendered cell shows it in full
    /// in a tooltip.
    Truncate,
}

/// What a table tells its parts (upstream `TableContext`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct TableStyle {
    pub density: TableDensity,
    pub dividers: TableDividers,
    pub striped: bool,
    pub hover: bool,
    pub vertical_align: TableVerticalAlign,
    pub text_overflow: TableTextOverflow,
    /// The padding host the table bleeds into, if any.
    pub host: Option<ContainerPadding>,
    pub dir: Direction,
}

/// A cell's place, for its padding at the table's edges and its dividers.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CellPlace {
    pub header: bool,
    /// The first and last columns.
    pub first: bool,
    pub last: bool,
    /// The last body row: no row divider under it.
    pub last_row: bool,
    pub align: Option<TableColumnAlign>,
}

/// The density's block and inline padding.
pub(crate) fn density_padding(
    theme: &ResolvedTheme,
    density: TableDensity,
    rem: Pixels,
) -> (Pixels, Pixels) {
    let t = theme.tokens();
    let (block, inline) = match density {
        TableDensity::Compact => (t.spacing.s1, t.spacing.s2),
        TableDensity::Balanced => (t.spacing.s2, t.spacing.s3),
        TableDensity::Spacious => (t.spacing.s3, t.spacing.s4),
    };
    (block.to_pixels(rem), inline.to_pixels(rem))
}

/// A cell: density padding (the first and last columns reach the host's
/// edge inset, at least a step in), the body's or the header's type,
/// dividers, alignment and overflow.
pub(crate) fn cell(theme: &ResolvedTheme, s: &TableStyle, place: CellPlace, rem: Pixels) -> Div {
    let t = theme.tokens();
    let (block, inline) = density_padding(theme, s.density, rem);
    let step = t.spacing.s2.to_pixels(rem);
    let (start, end) = match s.host {
        Some(host) => (
            if place.first {
                host.inline_start.max(step)
            } else {
                inline
            },
            if place.last {
                host.inline_end.max(step)
            } else {
                inline
            },
        ),
        None => (inline, inline),
    };
    let mut el = div()
        .relative()
        .flex()
        .flex_col()
        .min_w_0()
        .py(block)
        .padding_start(s.dir, start)
        .padding_end(s.dir, end)
        .overflow_hidden();
    el = if place.header {
        el.text_style(&t.text.label)
            .font_weight(t.font.weight.semibold)
            .text_color(t.color.text_secondary)
            .justify_center()
    } else {
        let el = el.text_style(&t.text.body);
        match s.vertical_align {
            TableVerticalAlign::Middle => el.justify_center(),
            TableVerticalAlign::Top => el.justify_start(),
            TableVerticalAlign::Bottom => el.justify_end(),
        }
    };
    el = match place.align.unwrap_or_default() {
        TableColumnAlign::Start => {
            if s.dir.is_rtl() {
                el.items_end().text_right()
            } else {
                el.items_start().text_left()
            }
        }
        TableColumnAlign::Center => el.items_center().text_center(),
        TableColumnAlign::End => {
            if s.dir.is_rtl() {
                el.items_start().text_left()
            } else {
                el.items_end().text_right()
            }
        }
    };
    // The header's divider always separates it from the body; a body row's
    // with row dividers, except under the last row.
    let width = t.border.width.to_pixels(rem);
    if place.header || (s.dividers.rows() && !place.last_row) {
        el = el.border_b(width).border_color(t.color.border);
    }
    if s.dividers.columns() && !place.last {
        el = if s.dir.is_rtl() {
            el.border_l(width)
        } else {
            el.border_r(width)
        }
        .border_color(t.color.border);
    }
    el
}

/// A body row's paint, from its place and state (upstream `TableRow` and
/// `rowPaint.ts`): selection's wash wins, then the hover overlay, then the
/// even rows' stripe.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RowState {
    /// Its display index among the body rows.
    pub index: usize,
    pub hovered: bool,
    pub selected_wash: bool,
}

pub(crate) fn row_background(
    theme: &ResolvedTheme,
    s: &TableStyle,
    state: RowState,
) -> Option<gpui::Hsla> {
    let t = theme.tokens();
    if state.selected_wash {
        Some(t.color.accent_muted)
    } else if s.hover && state.hovered {
        Some(t.color.overlay_hover)
    } else if s.striped && state.index % 2 == 1 {
        Some(t.color.background_muted)
    } else {
        None
    }
}

/// The theme's `table-row` props: `striped`, and a body row's `parity`.
pub(crate) fn row_theme_props(
    s: &TableStyle,
    body_index: Option<usize>,
) -> Vec<(&'static str, &'static str)> {
    let mut props = Vec::new();
    if s.striped && body_index.is_some() {
        props.push(("striped", "true"));
    }
    if let Some(i) = body_index {
        props.push(("parity", if i % 2 == 1 { "even" } else { "odd" }));
    }
    props
}
