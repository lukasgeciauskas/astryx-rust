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

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px,
};
use rustryx_i18n::{LogicalStyled, use_direction};
use rustryx_primitives::{provide, use_context};
use rustryx_theme::current;
use smallvec::SmallVec;

use super::motion::{self, Slot, row_id};
use crate::animated_layout::AnimatedLayout;
use crate::animated_layout::adopt::reveal_content;
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
    animated_layout: Option<AnimatedLayout>,
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
            animated_layout: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `animatedLayout`: the handle from [`use_animated_layout`]. With it, each
    /// row inserted, removed or moved between renders animates (FR12): an inserted row
    /// enters, a removed row collapses in place, inert, until it is gone, a row crossing
    /// rows that move the other way rides above them, and the rows after a change glide
    /// to their new places, all on the handle's one motion. A row's identity is its
    /// ListItem's `item_key`; a row without one changes at once.
    ///
    /// [`use_animated_layout`]: crate::animated_layout::use_animated_layout
    pub fn animated_layout(mut self, layout: AnimatedLayout) -> Self {
        self.animated_layout = Some(layout);
        self
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

    rustryx_primitives::setters! {
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
    theme: &rustryx_theme::ResolvedTheme,
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
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
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
        // FR12: with a handle, this frame's inserts, removals and moves.
        let mut motion = self
            .animated_layout
            .as_ref()
            .map(|layout| motion::begin(&self.id, &mut self.children, layout, gap, window, cx));
        let mut rows: Vec<Option<AnyElement>> = std::mem::take(&mut self.children)
            .into_iter()
            .enumerate()
            .map(|(i, child)| {
                let (first, last) = (i == 0, i + 1 == count);
                let row_corners = bleed_corners(corners, host, first && at_first, last && at_last);
                Some(
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
                    .into_any_element(),
                )
            })
            .collect();
        let rows: Vec<AnyElement> = match motion.as_mut() {
            None => rows.into_iter().flatten().collect(),
            Some(m) => {
                let surface = t.color.background_surface;
                let mut out = Vec::with_capacity(m.slots.len());
                for slot in m.slots.clone() {
                    match slot {
                        Slot::Row(i) => {
                            let Some(row) = rows[i].take() else { continue };
                            let Some(key) = m.keys[i].clone() else {
                                out.push(row);
                                continue;
                            };
                            let Some(reveal) = m.reveals.remove(&key) else {
                                out.push(row);
                                continue;
                            };
                            let content = reveal_content(reveal, row, false, window, cx)
                                .unwrap_or_else(|| div().into_any_element());
                            // A row crossing rows that move the other way rides above
                            // them, the list's surface behind it (a selected row keeps
                            // its own fill).
                            let content = if m.riders.contains(&key) {
                                gpui::deferred(
                                    div()
                                        .when(!m.selected[i], |el| el.bg(surface))
                                        .child(content),
                                )
                                .into_any_element()
                            } else {
                                content
                            };
                            // Each keyed row keeps its last frame (fork patch R3), so its
                            // exit can draw a faithful copy once it is gone.
                            out.push(
                                div()
                                    .id(row_id(&self.id, &key))
                                    .retain_layer(motion::row_copy_id(&self.id, &key))
                                    .child(content)
                                    .into_any_element(),
                            );
                        }
                        Slot::Exit(j) => {
                            let Some((exit, reveal)) = m.exits[j].take() else {
                                continue;
                            };
                            // The removed row's exit draws its last frame, start and end
                            // content included; a renderer without offscreen layers keeps
                            // no frame, and draws what the List kept of the row.
                            let copy_id = motion::row_copy_id(&self.id, &exit.key);
                            let ghost = match window.layer_snapshot_bounds(&copy_id) {
                                Some(bounds) => div()
                                    .debug_selector({
                                        let s = format!("{}-leaving-copy-{:?}", self.id, exit.key);
                                        move || s
                                    })
                                    .w_full()
                                    .h(bounds.size.height)
                                    .child(gpui::layer_snapshot(copy_id).size_full())
                                    .into_any_element(),
                                None => provide(
                                    ListRow {
                                        first: false,
                                        last: exit.ghost.last,
                                        number: 0,
                                        corners: RowCorners::default(),
                                        index: usize::MAX,
                                    },
                                    exit.ghost.row(ElementId::Name(
                                        format!("{}-leaving-{:?}", self.id, exit.key).into(),
                                    )),
                                )
                                .into_any_element(),
                            };
                            let content = reveal_content(reveal, ghost, true, window, cx);
                            out.push(
                                div()
                                    .id(ElementId::Name(
                                        format!("{}-leaving-row-{:?}", self.id, exit.key).into(),
                                    ))
                                    .children(content)
                                    .into_any_element(),
                            );
                        }
                    }
                }
                out
            }
        };

        // AST-103: a reorderable list's session, its rows' bounds once laid
        // out, and its overlay (the line and the dragged row) last.
        let reorder = self
            .reorderable
            .as_ref()
            .map(|r| crate::reorderable::session::begin(&self.id, r, false, window, cx));
        let record = motion.as_ref().map(|m| m.record());
        let base = match (&reorder, record) {
            (Some(rc), record) => {
                let session = rc.session.clone();
                div()
                    .relative()
                    .on_children_prepainted(move |bounds, window, cx| {
                        // The rows alone: a removed row's exit is not one of them.
                        let bounds = match &record {
                            Some(record) => record(&bounds, window, cx),
                            None => bounds,
                        };
                        crate::reorderable::session::finalize(&session, &bounds, count, window, cx)
                    })
            }
            (None, Some(record)) => div().on_children_prepainted(move |bounds, window, cx| {
                record(&bounds, window, cx);
            }),
            (None, None) => div(),
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
