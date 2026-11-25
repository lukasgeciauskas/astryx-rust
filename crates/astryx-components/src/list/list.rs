//! List, upstream `List/List.tsx` and `ListContext.tsx`: a vertical
//! collection of ListItem rows.
//!
//! - `density`, `dividers` and the marker style reach every row through a
//!   context; each row also learns its place (first, last, its number).
//! - `edge` places the rows against the host they sit in (FR1-FR8):
//!   `padded` keeps the rows' own inset, `flush` drops it, `outset` reaches
//!   into the host's padding by up to the inset, and `bleed` reaches the
//!   host's padding edge, its first and last rows following a rounded
//!   host's corners. Hosts publish their padding, corners and children's
//!   places (`ContainerPadding`, `ContainerCorners`, `ContainerChild`);
//!   with none, the outer treatments look like `padded` (FR6).
//! - The list clips its inline overflow (FR9).
//! - Numbered rows share one marker column as wide as the widest number,
//!   less the rows' inset it may hang into.

use astryx_i18n::{LogicalStyled, use_direction};
use astryx_primitives::{provide, use_context};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px,
};
use smallvec::SmallVec;

use crate::item::ItemDensity;
use crate::layout::{ContainerChild, ContainerCorners, ContainerPadding};

/// Upstream `edge`: where the rows' text, dividers and backgrounds sit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListEdge {
    /// Text inside each row's padding.
    #[default]
    Padded,
    /// No side padding: text and paint stop at the list's edges.
    Flush,
    /// Text on the host's content line, paint reaching into its padding.
    Outset,
    /// Paint reaching the host's padding edge, like Table rows in a Card.
    Bleed,
}

impl ListEdge {
    pub fn as_str(self) -> &'static str {
        match self {
            ListEdge::Padded => "padded",
            ListEdge::Flush => "flush",
            ListEdge::Outset => "outset",
            ListEdge::Bleed => "bleed",
        }
    }
}

/// Upstream `listStyle`: the rows' markers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListStyle {
    #[default]
    None,
    Disc,
    /// Numbers, from `start`.
    Decimal,
    Circle,
}

impl ListStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            ListStyle::None => "none",
            ListStyle::Disc => "disc",
            ListStyle::Decimal => "decimal",
            ListStyle::Circle => "circle",
        }
    }
}

/// A marker a scope can name for its rows, beyond the list styles
/// (upstream `ListMarker`, set by Markdown's nested lists).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ListMarker {
    Disc,
    Circle,
    Square,
    Decimal,
    LowerAlpha,
    LowerRoman,
}

/// A task row's state (upstream `ListTaskMarker`): a read-only,
/// labelled checkbox in the marker's place.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct ListTask {
    pub checked: bool,
    pub label: SharedString,
}

/// What a List tells its rows (upstream `ListContext`).
#[derive(Clone, Debug)]
pub(crate) struct ListContext {
    pub density: ItemDensity,
    pub dividers: bool,
    pub edge: ListEdge,
    pub list_style: ListStyle,
    pub host: ContainerPadding,
    /// The row gap: half of it bounds a row's touch band (FR10).
    pub gap: Pixels,
    /// The shared number column's width, for numbered rows.
    pub number_column: Option<Pixels>,
    pub marker: Option<ListMarker>,
    pub task: Option<ListTask>,
    /// A reorderable list's session (AST-103).
    pub reorder: Option<crate::reorderable::session::ReorderCx>,
}

/// Rows inside it draw this marker or task box (upstream
/// `ListMarkerScope`).
#[allow(dead_code)]
pub(crate) fn list_marker_scope(
    marker: ListMarker,
    task: Option<ListTask>,
    child: impl IntoElement,
    cx: &App,
) -> AnyElement {
    match use_context::<ListContext>(cx) {
        Some(mut ctx) => {
            ctx.marker = Some(marker);
            ctx.task = task;
            provide(ctx, child).into_any_element()
        }
        None => child.into_any_element(),
    }
}

/// A row's corners, for a bleeding list at a rounded host's corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct RowCorners {
    pub start_start: Pixels,
    pub start_end: Pixels,
    pub end_start: Pixels,
    pub end_end: Pixels,
}

/// A row's place in its list.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ListRow {
    pub first: bool,
    pub last: bool,
    pub number: i64,
    pub corners: RowCorners,
    /// Its place among the list's children.
    pub index: usize,
}

/// A vertical list of ListItem rows, upstream `List`.
///
/// ```ignore
/// List::new("settings")
///     .dividers(true)
///     .child(ListItem::new("notifications", "Notifications").description("Manage your alerts"))
///     .child(ListItem::new("privacy", "Privacy"))
/// ```
#[derive(IntoElement)]
pub struct List {
    id: ElementId,
    children: SmallVec<[AnyElement; 8]>,
    density: ItemDensity,
    dividers: bool,
    edge: Option<ListEdge>,
    edge_compensation: bool,
    header: Option<AnyElement>,
    label: Option<SharedString>,
    list_style: ListStyle,
    start: i64,
    reorderable: Option<crate::reorderable::Reorderable>,
    style: StyleRefinement,
}

impl List {
    pub fn new(id: impl Into<ElementId>) -> Self {
        List {
            id: id.into(),
            children: SmallVec::new(),
            density: ItemDensity::Balanced,
            dividers: false,
            edge: None,
            edge_compensation: false,
            header: None,
            label: None,
            list_style: ListStyle::None,
            start: 1,
            reorderable: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `reorderable`: the object [`create_reorderable`] made. Its
    /// rows move by their handles' drags and reorder mode; each ListItem's
    /// `item_key` names it in the reports. Without it the list renders no
    /// handle and listens for no reorder (AST-103 FR2).
    ///
    /// [`create_reorderable`]: crate::reorderable::create_reorderable
    pub fn reorderable(mut self, reorderable: crate::reorderable::Reorderable) -> Self {
        self.reorderable = Some(reorderable);
        self
    }

    astryx_primitives::setters! {
        /// Upstream `density`: the rows' block padding.
        density: ItemDensity,
        /// Upstream `hasDividers`: a rule between rows.
        !dividers,
        /// Upstream `listStyle`.
        list_style: ListStyle,
        /// Upstream `start`: the first number of a decimal list.
        start: i64,
    }

    /// Upstream `edge`: `padded` by default.
    pub fn edge(mut self, edge: ListEdge) -> Self {
        self.edge = Some(edge);
        self
    }

    /// Upstream `edgeCompensation="inline"`, deprecated: `edge(Outset)`.
    /// An `edge` wins over it.
    #[deprecated(note = "use .edge(ListEdge::Outset)")]
    pub fn edge_compensation(mut self, inline: bool) -> Self {
        self.edge_compensation = inline;
        self
    }

    /// Upstream `header`: content above the rows. Name the list with
    /// [`label`](Self::label) (upstream names it by the header's text).
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// The list's accessible name: the header's text.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for List {
    /// Upstream `children`: ListItem rows.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for List {
    /// Upstream `xstyle`: the list element.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The inline inset an Item row applies at `density` (upstream
/// `--_item-inset-inline`), and whether a theme set it.
pub(crate) fn row_inset(
    theme: &astryx_theme::ResolvedTheme,
    density: ItemDensity,
    rem: Pixels,
) -> (Pixels, bool) {
    let t = theme.tokens();
    let mut probe = StyleRefinement::default();
    theme.refine(&mut probe, "item", &[("density", density.as_str())], None);
    let themed = probe.padding.left.or(probe.padding.right);
    match themed {
        Some(length) => (
            length.to_pixels(gpui::AbsoluteLength::Pixels(px(0.)), rem),
            true,
        ),
        None => (
            match density {
                ItemDensity::Spacious => t.spacing.s3.to_pixels(rem),
                _ => t.spacing.s2.to_pixels(rem),
            },
            false,
        ),
    }
}

/// A bleeding row's corners (FR5): a rounded host's corner, less the block
/// padding the host keeps, on a first or last row at that corner.
pub(crate) fn bleed_corners(
    corners: ContainerCorners,
    host: ContainerPadding,
    at_start: bool,
    at_end: bool,
) -> RowCorners {
    let mut row = RowCorners::default();
    if at_start {
        row.start_start = (corners.start_start - host.block_start).max(px(0.));
        row.start_end = (corners.start_end - host.block_start).max(px(0.));
    }
    if at_end {
        row.end_start = (corners.end_start - host.block_end).max(px(0.));
        row.end_end = (corners.end_end - host.block_end).max(px(0.));
    }
    row
}

/// A number as a counter style writes it, with decimal outside its range.
pub(crate) fn counter_text(number: i64, marker: ListMarker) -> String {
    let alpha = |mut n: i64| {
        let mut out = Vec::new();
        while n > 0 {
            n -= 1;
            out.push((b'a' + (n % 26) as u8) as char);
            n /= 26;
        }
        out.iter().rev().collect::<String>()
    };
    let roman = |mut n: i64| {
        const DIGITS: [(i64, &str); 13] = [
            (1000, "m"),
            (900, "cm"),
            (500, "d"),
            (400, "cd"),
            (100, "c"),
            (90, "xc"),
            (50, "l"),
            (40, "xl"),
            (10, "x"),
            (9, "ix"),
            (5, "v"),
            (4, "iv"),
            (1, "i"),
        ];
        let mut out = String::new();
        for (value, digits) in DIGITS {
            while n >= value {
                out.push_str(digits);
                n -= value;
            }
        }
        out
    };
    let body = match marker {
        ListMarker::LowerAlpha if number > 0 => alpha(number),
        ListMarker::LowerRoman if (1..=3999).contains(&number) => roman(number),
        _ => number.to_string(),
    };
    format!("{body}.")
}

impl RenderOnce for List {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        #[allow(deprecated)]
        let edge = self.edge.unwrap_or(if self.edge_compensation {
            ListEdge::Outset
        } else {
            ListEdge::Padded
        });
        // FR6: no host, no reach.
        let host = use_context::<ContainerPadding>(cx).unwrap_or_default();
        let corners = use_context::<ContainerCorners>(cx).unwrap_or_default();
        let place = use_context::<ContainerChild>(cx);
        let count = self.children.len();
        let gap = if self.dividers {
            px(0.)
        } else {
            t.spacing.s0_5.to_pixels(rem)
        };

        // Numbered rows share a column as wide as the widest number, less
        // the inset a number may hang into.
        let number_column = (self.list_style == ListStyle::Decimal).then(|| {
            let (inset, _) = row_inset(&theme, self.density, rem);
            let size = t.text.body.size.to_pixels(rem);
            let mut style = window.text_style();
            style.font_size = t.text.body.size;
            let widest = (0..count as i64)
                .map(|i| {
                    let text = counter_text(self.start + i, ListMarker::Decimal);
                    let run = style.to_run(text.len());
                    window
                        .text_system()
                        .shape_line(text.into(), size, &[run], None)
                        .width
                })
                .fold(px(0.), Pixels::max);
            t.spacing.s4.to_pixels(rem).max(widest - inset)
        });

        let bleed = edge == ListEdge::Bleed;
        let has_header = self.header.is_some();
        let at_first = bleed && !has_header && place.is_some_and(|p| p.first);
        let at_last = bleed && place.is_some_and(|p| p.last);
        let rows = self.children.into_iter().enumerate().map(|(i, child)| {
            let (first, last) = (i == 0, i + 1 == count);
            let row_corners = bleed_corners(corners, host, first && at_first, last && at_last);
            provide(
                ListRow {
                    first,
                    last,
                    number: self.start + i as i64,
                    corners: row_corners,
                    index: i,
                },
                child,
            )
            .into_any_element()
        });

        // AST-103: a reorderable list's session, its rows' bounds once laid
        // out, and its overlay (the line and the dragged row) last.
        let reorder = self
            .reorderable
            .as_ref()
            .map(|r| crate::reorderable::session::begin(&self.id, r, window, cx));
        let base = match &reorder {
            Some(rc) => {
                let session = rc.session.clone();
                div()
                    .relative()
                    .on_children_prepainted(move |bounds, window, cx| {
                        crate::reorderable::session::finalize(&session, &bounds, count, window, cx)
                    })
            }
            None => div(),
        };
        let overlay = reorder.as_ref().map(crate::reorderable::session::overlay);
        let mut list = base
            .id(self.id.clone())
            .flex()
            .flex_col()
            .gap(gap)
            .overflow_x_hidden()
            .role(Role::List);
        if let Some(label) = self.label.clone() {
            list = list.aria_label(label);
        }
        // Outset and bleed rows reach into the host's padding, so the list
        // (and its clip) reaches its padding edge first.
        if matches!(edge, ListEdge::Outset | ListEdge::Bleed) {
            list = list
                .margin_start(dir, -host.inline_start)
                .margin_end(dir, -host.inline_end);
        }
        theme.refine(
            list.style(),
            "list",
            &[
                ("density", self.density.as_str()),
                ("listStyle", self.list_style.as_str()),
            ],
            None,
        );
        list.style().refine(&self.style);
        let context = ListContext {
            density: self.density,
            dividers: self.dividers,
            edge,
            list_style: self.list_style,
            host,
            gap,
            number_column,
            marker: None,
            task: None,
            reorder,
        };
        let list = provide(context, list.children(rows).children(overlay)).into_any_element();
        match self.header {
            None => list,
            Some(header) => div()
                .flex()
                .flex_col()
                .child(div().mb(t.spacing.s2).child(header))
                .child(list)
                .into_any_element(),
        }
    }
}
