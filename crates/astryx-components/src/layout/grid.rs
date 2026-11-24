//! Grid and GridSpan. Upstream: `Grid/`.
//!
//! GPUI's grid has `repeat(n, minmax(0, 1fr))` tracks only, so:
//! - responsive columns (`{minWidth, max, repeat}`, CSS `auto-fill` /
//!   `auto-fit`) are counted from the grid's measured width, kept between
//!   frames; after a resize the count settles on the next frame;
//! - `rowHeight` (CSS `grid-auto-rows`) sizes each cell, with spans added
//!   through [`Grid::span`] so their cells cover several rows.

use std::cell::Cell;
use std::panic::Location;
use std::rc::Rc;

use astryx_theme::{ResolvedTheme, current};
use gpui::{
    AbsoluteLength, AnyElement, App, Div, ElementId, IntoElement, ParentElement, Pixels,
    RenderOnce, StyleRefinement, Styled, Window, canvas, div, prelude::*, px,
};
use smallvec::SmallVec;

use super::spacing::{Sizing, SpacingStep, size_setters};

/// Upstream `columns`' `repeat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridRepeat {
    /// CSS `auto-fill`: empty tracks keep their width. The default.
    #[default]
    Fill,
    /// CSS `auto-fit`: empty tracks collapse so the items stretch.
    Fit,
}

/// Upstream `columns`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridColumns {
    /// Exactly this many equal tracks at every width.
    Count(u16),
    /// As many tracks of at least `min_width` as fit, at most `max`.
    Responsive {
        min_width: Pixels,
        max: Option<u16>,
        repeat: GridRepeat,
    },
}

impl GridColumns {
    /// Responsive columns of at least `min_width`.
    pub fn min_width(min_width: Pixels) -> Self {
        GridColumns::Responsive {
            min_width,
            max: None,
            repeat: GridRepeat::Fill,
        }
    }

    /// The track count in a grid `width` wide with `gap` between tracks,
    /// holding `items`.
    pub fn count(&self, width: Option<Pixels>, gap: Pixels, items: usize) -> u16 {
        match *self {
            GridColumns::Count(n) => n.max(1),
            GridColumns::Responsive {
                min_width,
                max,
                repeat,
            } => {
                let tracks = match width {
                    // `auto-fill`: as many as fit; at least one.
                    Some(width) => ((width + gap) / (min_width + gap)).floor().max(1.) as u16,
                    None => max.unwrap_or(1),
                };
                let tracks = max.map_or(tracks, |max| tracks.min(max.max(1)));
                match repeat {
                    GridRepeat::Fill => tracks,
                    GridRepeat::Fit => tracks.min(items.max(1) as u16),
                }
            }
        }
    }

    fn variant(&self) -> &'static str {
        match self {
            GridColumns::Count(_) => "fixed",
            GridColumns::Responsive { .. } => "responsive",
        }
    }
}

impl From<u16> for GridColumns {
    fn from(n: u16) -> Self {
        GridColumns::Count(n)
    }
}

/// Upstream `align` / `justify`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridAlign {
    Start,
    Center,
    End,
    Stretch,
}

impl GridAlign {
    fn as_str(self) -> &'static str {
        match self {
            GridAlign::Start => "start",
            GridAlign::Center => "center",
            GridAlign::End => "end",
            GridAlign::Stretch => "stretch",
        }
    }
}

/// Upstream `GridSpan` `columns`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanColumns {
    Span(u16),
    /// Every column, CSS `1 / -1`.
    Full,
}

/// A grid child spanning several tracks, upstream `GridSpan`.
#[derive(IntoElement)]
pub struct GridSpan {
    columns: Option<SpanColumns>,
    rows: Option<u16>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
}

impl GridSpan {
    pub fn new() -> Self {
        GridSpan {
            columns: None,
            rows: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    /// Upstream `columns`: a column count, or `SpanColumns::Full`.
    pub fn columns(mut self, columns: impl Into<SpanColumns>) -> Self {
        self.columns = Some(columns.into());
        self
    }

    /// Upstream `rows`.
    pub fn rows(mut self, rows: u16) -> Self {
        self.rows = Some(rows);
        self
    }

    fn cell(self, total_columns: u16, height: Option<Pixels>) -> Div {
        let span = match self.columns {
            Some(SpanColumns::Full) => total_columns,
            Some(SpanColumns::Span(n)) => n.min(total_columns),
            None => 1,
        };
        let mut el = div().min_w_0().col_span(span).grid().grid_cols(1).h_full();
        if let Some(rows) = self.rows {
            el = el.row_span(rows);
        }
        if let Some(height) = height {
            el = el.h(height);
        }
        el.style().refine(&self.style);
        el.children(self.children)
    }
}

impl From<u16> for SpanColumns {
    fn from(n: u16) -> Self {
        SpanColumns::Span(n)
    }
}

impl Default for GridSpan {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for GridSpan {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for GridSpan {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for GridSpan {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let mut el = self.cell(u16::MAX, None);
        theme.refine(el.style(), "grid-span", &[], None);
        el
    }
}

enum GridChild {
    Plain(AnyElement),
    Span(Box<GridSpan>),
}

/// A grid with fixed or responsive columns, upstream `Grid`.
#[derive(IntoElement)]
pub struct Grid {
    columns: GridColumns,
    size: Sizing,
    gap: Option<SpacingStep>,
    row_gap: Option<SpacingStep>,
    column_gap: Option<SpacingStep>,
    row_height: Option<Pixels>,
    align: Option<GridAlign>,
    justify: Option<GridAlign>,
    id: Option<ElementId>,
    source: &'static Location<'static>,
    style: StyleRefinement,
    children: Vec<GridChild>,
}

impl Grid {
    /// Upstream `columns`: a count (`3`) or `GridColumns::min_width(px(240.))`.
    #[track_caller]
    pub fn new(columns: impl Into<GridColumns>) -> Self {
        Grid {
            columns: columns.into(),
            size: Sizing::default(),
            gap: None,
            row_gap: None,
            column_gap: None,
            row_height: None,
            align: None,
            justify: None,
            id: None,
            source: Location::caller(),
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `gap`.
        ?gap: SpacingStep,
        /// Upstream `rowGap`: overrides `gap` between rows.
        ?row_gap: SpacingStep,
        /// Upstream `columnGap`: overrides `gap` between columns.
        ?column_gap: SpacingStep,
        /// Upstream `rowHeight`: the height of each row.
        ?row_height: Pixels,
        /// Upstream `align`: vertical alignment of items in their cells.
        ?align: GridAlign,
        /// Upstream `justify`: horizontal alignment of items in their cells.
        ?justify: GridAlign,
    }

    size_setters!();

    /// The element id, which keeps the measured width when responsive.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Adds a child that spans several tracks.
    pub fn span(mut self, span: GridSpan) -> Self {
        self.children.push(GridChild::Span(Box::new(span)));
        self
    }

    fn gaps(&self, theme: &ResolvedTheme) -> (Option<AbsoluteLength>, Option<AbsoluteLength>) {
        let t = theme.tokens();
        let row = self.row_gap.or(self.gap).map(|s| s.token(t));
        let column = self.column_gap.or(self.gap).map(|s| s.token(t));
        (row, column)
    }
}

impl Styled for Grid {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Grid {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children
            .extend(elements.into_iter().map(GridChild::Plain));
    }
}

impl RenderOnce for Grid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let rem = window.rem_size();
        let (row_gap, column_gap) = self.gaps(&theme);
        let to_px = |l: Option<AbsoluteLength>| l.map_or(px(0.), |l| l.to_pixels(rem));

        let id = self
            .id
            .clone()
            .unwrap_or(ElementId::CodeLocation(*self.source));
        let measured = window.use_keyed_state(id.clone(), cx, |_, _| Rc::new(Cell::new(None)));
        let measured: Rc<Cell<Option<Pixels>>> = measured.read(cx).clone();
        let columns = self
            .columns
            .count(measured.get(), to_px(column_gap), self.children.len());

        let mut el = div().relative().grid().grid_cols(columns);
        if let Some(gap) = row_gap {
            el.style().gap.height = Some(gap.into());
        }
        if let Some(gap) = column_gap {
            el.style().gap.width = Some(gap.into());
        }
        el = match self.align {
            Some(GridAlign::Start) => el.items_start(),
            Some(GridAlign::Center) => el.items_center(),
            Some(GridAlign::End) => el.items_end(),
            Some(GridAlign::Stretch) => el.items_stretch(),
            None => el,
        };
        el = self.size.apply(el);
        let mut props = vec![("columns", self.columns.variant())];
        if let Some(gap) = self.gap {
            props.push(("gap", gap.as_str()));
        }
        if let Some(align) = self.align {
            props.push(("align", align.as_str()));
        }
        if let Some(justify) = self.justify {
            props.push(("justify", justify.as_str()));
        }
        theme.refine(el.style(), "grid", &props, None);
        el.style().refine(&self.style);

        let row_height = self.row_height;
        let row_gap_px = to_px(row_gap);
        // GPUI's grid has no `justify-items`: a cell box aligns the item.
        let justify = self.justify;
        let place = move |child: AnyElement| -> AnyElement {
            let cell = div().flex().w_full().min_w_0();
            match justify {
                Some(GridAlign::Start) => cell.justify_start().child(child).into_any_element(),
                Some(GridAlign::Center) => cell.justify_center().child(child).into_any_element(),
                Some(GridAlign::End) => cell.justify_end().child(child).into_any_element(),
                Some(GridAlign::Stretch) | None => child,
            }
        };
        let children = self.children.into_iter().map(|child| match child {
            GridChild::Plain(child) => match row_height {
                Some(height) => div()
                    .min_w_0()
                    .h(height)
                    .child(place(child))
                    .into_any_element(),
                None => place(child),
            },
            GridChild::Span(span) => {
                let rows = span.rows.unwrap_or(1);
                let height = row_height.map(|h| h * rows as f32 + row_gap_px * (rows - 1) as f32);
                span.cell(columns, height).into_any_element()
            }
        });

        let responsive = matches!(self.columns, GridColumns::Responsive { .. });
        el.children(children).when(responsive, |el| {
            // Measures the grid's width for the next frame's track count.
            el.child(
                canvas(
                    move |bounds, window, _| {
                        if measured.get() != Some(bounds.size.width) {
                            measured.set(Some(bounds.size.width));
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responsive_track_counts_follow_css_auto_fill() {
        let columns = GridColumns::Responsive {
            min_width: px(240.),
            max: Some(4),
            repeat: GridRepeat::Fill,
        };
        // 880px with 16px gaps holds three 240px tracks.
        assert_eq!(columns.count(Some(px(880.)), px(16.), 2), 3);
        // Never more than `max`.
        assert_eq!(columns.count(Some(px(2000.)), px(16.), 2), 4);
        // Narrower than one track: one column.
        assert_eq!(columns.count(Some(px(100.)), px(16.), 2), 1);
        let fit = GridColumns::Responsive {
            min_width: px(240.),
            max: None,
            repeat: GridRepeat::Fit,
        };
        // `auto-fit` collapses tracks beyond the items.
        assert_eq!(fit.count(Some(px(880.)), px(16.), 2), 2);
        assert_eq!(GridColumns::Count(4).count(Some(px(100.)), px(16.), 1), 4);
    }
}
