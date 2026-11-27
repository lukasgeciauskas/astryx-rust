//! Long-list windowing: the one shared module that decides which rows of a
//! long list render (upstream `virtualization/listWindow.ts`,
//! `architecture:list-windowing`).
//!
//! A windowed list renders the rows that cover its visible scrollport, plus a
//! few past each edge, and sizes its scroll range for the full list. This
//! module computes which rows render and where the scroll range ends; each
//! consumer decides when it windows and what a row is. [`WindowedList`] is the
//! GPUI adapter, on either axis, inside a container scroller or a page.
//!
//! The engine meets the record's FR1-FR4: it anchors the window to the first
//! visible row, to the end (follow) or to a given item ([`ListAnchor`]); the
//! anchored row keeps its place through inserts and resizes; rows may have
//! variable sizes ([`calculate_variable_list_range`]); and the adapter works in
//! a container scroll area and in a page's scroller.
//!
//! Like upstream's module it is internal (INV2): Astryx components use it,
//! and it stays out of the documented API until upstream settles its OQ1.

mod element;

pub use element::{ListPitch, ScrollHost, WindowedList, WindowedListAxis};

use std::collections::HashMap;

use gpui::SharedString;

/// Rows mounted past each edge of the visible range (fixed pitch).
pub const LIST_WINDOW_OVERSCAN: usize = 4;

/// A row's identity: the anchor and the focus island follow keys. A number
/// and a string with the same digits are different keys.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ListWindowKey {
    Number(i64),
    Text(SharedString),
}

impl From<i64> for ListWindowKey {
    fn from(n: i64) -> Self {
        Self::Number(n)
    }
}

impl From<usize> for ListWindowKey {
    fn from(n: usize) -> Self {
        Self::Number(n as i64)
    }
}

impl From<i32> for ListWindowKey {
    fn from(n: i32) -> Self {
        Self::Number(n as i64)
    }
}

impl From<&'static str> for ListWindowKey {
    fn from(s: &'static str) -> Self {
        Self::Text(s.into())
    }
}

impl From<String> for ListWindowKey {
    fn from(s: String) -> Self {
        Self::Text(s.into())
    }
}

impl From<SharedString> for ListWindowKey {
    fn from(s: SharedString) -> Self {
        Self::Text(s)
    }
}

/// Mounted rows, or a gap standing in for unmounted ones.
#[derive(Clone, Debug, PartialEq)]
pub enum ListWindowSegment {
    Rows {
        start: usize,
        end: usize,
    },
    /// `size` is the gap's extent along the list's axis.
    Gap {
        start: usize,
        end: usize,
        size: f64,
    },
}

impl ListWindowSegment {
    pub fn start(&self) -> usize {
        match self {
            Self::Rows { start, .. } | Self::Gap { start, .. } => *start,
        }
    }

    pub fn end(&self) -> usize {
        match self {
            Self::Rows { end, .. } | Self::Gap { end, .. } => *end,
        }
    }
}

/// The inputs of a fixed-pitch window.
#[derive(Clone, Debug, PartialEq)]
pub struct ListWindowOptions {
    pub row_count: usize,
    pub row_height: f64,
    pub viewport_height: f64,
    pub scroll_top: f64,
    /// The measured full content extent; it does not change the row total.
    pub scroll_height: Option<f64>,
    /// The distance from the scroll origin to the first row, such as a header.
    pub start_offset: f64,
    pub is_measured: bool,
    pub focus_index: Option<usize>,
}

impl Default for ListWindowOptions {
    fn default() -> Self {
        Self {
            row_count: 0,
            row_height: 0.,
            viewport_height: 0.,
            scroll_top: 0.,
            scroll_height: None,
            start_offset: 0.,
            is_measured: false,
            focus_index: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListWindowPlan {
    pub segments: Vec<ListWindowSegment>,
    pub start: usize,
    pub end: usize,
    pub first_visible_index: usize,
    /// The distance into the first visible row; negative while the leading
    /// content shows (the distance to the first row).
    pub offset: f64,
    pub total_height: f64,
}

pub fn is_positive_finite(value: f64) -> bool {
    value.is_finite() && value > 0.
}

/// The fixed-pitch window, or `None` when the inputs cannot form a finite one.
pub fn calculate_list_window(o: &ListWindowOptions) -> Option<ListWindowPlan> {
    let total_height = o.row_count as f64 * o.row_height;
    let invalid = !is_positive_finite(o.row_height)
        || !is_positive_finite(o.viewport_height)
        || !total_height.is_finite()
        || !(o.start_offset + total_height).is_finite()
        || !(o.viewport_height / o.row_height).is_finite()
        || !o.scroll_top.is_finite()
        || o.scroll_height.is_some_and(|h| !h.is_finite() || h < 0.)
        || !o.start_offset.is_finite()
        || o.start_offset < 0.;
    if invalid {
        return None;
    }
    if o.row_count == 0 {
        return Some(ListWindowPlan {
            segments: Vec::new(),
            start: 0,
            end: 0,
            first_visible_index: 0,
            offset: 0.,
            total_height: 0.,
        });
    }
    // Clamp against the full body extent, not the mounted rows.
    let content_height = (o.start_offset + total_height).max(o.scroll_height.unwrap_or(0.));
    let maximum_scroll = (content_height - o.viewport_height).max(0.);
    let clamped = o.scroll_top.max(0.).min(maximum_scroll);
    let list_top = clamped - o.start_offset;
    let body_top = list_top.max(0.).min(total_height);
    let body_bottom = (clamped + o.viewport_height - o.start_offset)
        .max(0.)
        .min(total_height);
    let first_visible_index = if o.is_measured {
        ((body_top / o.row_height).floor() as usize).min(o.row_count - 1)
    } else {
        0
    };
    // Above the first row, the offset is the negative distance to it, so an
    // anchor round-trips a scroll position inside the leading content.
    let offset = if !o.is_measured {
        0.
    } else if list_top < 0. {
        list_top
    } else {
        body_top - first_visible_index as f64 * o.row_height
    };
    let (start, end) = if o.is_measured {
        let last = first_visible_index.max((body_bottom / o.row_height).ceil() as usize);
        (
            first_visible_index.saturating_sub(LIST_WINDOW_OVERSCAN),
            o.row_count.min(last + LIST_WINDOW_OVERSCAN),
        )
    } else {
        let seeded = (o.viewport_height / o.row_height).ceil() as usize;
        (0, o.row_count.min(seeded + LIST_WINDOW_OVERSCAN))
    };
    Some(ListWindowPlan {
        segments: create_list_window_segments(o.row_count, o.row_height, start, end, o.focus_index),
        start,
        end,
        first_visible_index,
        offset,
        total_height,
    })
}

/// The mounted rows and gaps for a range, plus one retained row outside it
/// (the focus island).
pub fn create_list_window_segments(
    row_count: usize,
    row_height: f64,
    start: usize,
    end: usize,
    focus_index: Option<usize>,
) -> Vec<ListWindowSegment> {
    segments_with(row_count, start, end, focus_index, |a, b| {
        (b - a) as f64 * row_height
    })
}

/// [`create_list_window_segments`] for items of varying size: a gap's size
/// is the extent of its items.
pub fn create_variable_segments(
    offsets: &[f64],
    start: usize,
    end: usize,
    focus_index: Option<usize>,
) -> Vec<ListWindowSegment> {
    let count = offsets.len().saturating_sub(1);
    segments_with(count, start, end, focus_index, |a, b| {
        offsets[b] - offsets[a]
    })
}

fn segments_with(
    row_count: usize,
    start: usize,
    end: usize,
    focus_index: Option<usize>,
    gap: impl Fn(usize, usize) -> f64,
) -> Vec<ListWindowSegment> {
    if row_count == 0 {
        return Vec::new();
    }
    let mut intervals = vec![(start, end)];
    if let Some(f) = focus_index
        && f < row_count
        && (f < start || f >= end)
    {
        intervals.push((f, f + 1));
        intervals.sort_by_key(|i| i.0);
    }
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in intervals {
        match merged.last_mut() {
            Some(previous) if s <= previous.1 => previous.1 = previous.1.max(e),
            _ if e > s => merged.push((s, e)),
            _ => {}
        }
    }
    let mut segments = Vec::new();
    let mut cursor = 0;
    for (s, e) in merged {
        if s > cursor {
            segments.push(ListWindowSegment::Gap {
                start: cursor,
                end: s,
                size: gap(cursor, s),
            });
        }
        segments.push(ListWindowSegment::Rows { start: s, end: e });
        cursor = e;
    }
    if cursor < row_count {
        segments.push(ListWindowSegment::Gap {
            start: cursor,
            end: row_count,
            size: gap(cursor, row_count),
        });
    }
    segments
}

/// The inputs of a window over items of varying size.
#[derive(Clone, Debug)]
pub struct VariableListRangeOptions<'a> {
    /// The start of each item, then the total extent: one more entry than
    /// items.
    pub offsets: &'a [f64],
    pub viewport_size: f64,
    pub scroll_offset: f64,
    pub is_measured: bool,
    pub overscan: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VariableListRange {
    pub start: usize,
    pub end: usize,
    pub first_visible_index: usize,
    /// The distance from the first visible item's start to the scroll offset.
    pub offset: f64,
}

/// The first index in `[0, limit)` whose offset is at least `value`, or above
/// it when `strict`.
fn lower_bound(offsets: &[f64], value: f64, limit: usize, strict: bool) -> usize {
    let (mut low, mut high) = (0, limit);
    while low < high {
        let middle = (low + high) / 2;
        let below = if strict {
            offsets[middle] <= value
        } else {
            offsets[middle] < value
        };
        if below {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

/// The half-open item range for items of varying size (FR3). Before
/// measurement it holds the items that start inside the viewport, plus
/// overscan after them.
pub fn calculate_variable_list_range(o: &VariableListRangeOptions) -> VariableListRange {
    let count = o.offsets.len().saturating_sub(1);
    if count == 0 {
        return VariableListRange {
            start: 0,
            end: 0,
            first_visible_index: 0,
            offset: 0.,
        };
    }
    if !o.is_measured {
        return VariableListRange {
            start: 0,
            end: count.min(lower_bound(o.offsets, o.viewport_size, count, false) + o.overscan),
            first_visible_index: 0,
            offset: 0.,
        };
    }
    let total = o.offsets[count];
    let visible = o.viewport_size.max(0.);
    let scroll = o.scroll_offset.max(0.).min((total - visible).max(0.));
    // The last item that starts at or before the scroll offset.
    let first = (lower_bound(o.offsets, scroll, count, true).saturating_sub(1)).min(count - 1);
    let last = (first + 1).max(lower_bound(o.offsets, scroll + visible, count, false));
    VariableListRange {
        start: first.saturating_sub(o.overscan),
        end: count.min(last + o.overscan),
        first_visible_index: first,
        offset: scroll - o.offsets[first],
    }
}

/// The inputs of first-visible-key anchoring.
#[derive(Clone, Debug)]
pub struct ListAnchorOptions<'a> {
    pub previous_keys: &'a [ListWindowKey],
    pub keys: &'a [ListWindowKey],
    pub first_visible_index: usize,
    pub offset: f64,
    pub row_height: f64,
    pub viewport_height: f64,
    pub start_offset: f64,
    /// Non-row content after the rows, kept across data changes.
    pub trailing_content_height: f64,
    /// A key-to-index map, built when the keys change, never on a scroll.
    pub key_indices: Option<&'a HashMap<ListWindowKey, usize>>,
    /// Item starts plus the total extent, for items of varying size. With
    /// them, `offset` is a fraction of the anchor item's size and
    /// `row_height` is not read.
    pub offsets: Option<&'a [f64]>,
}

impl Default for ListAnchorOptions<'_> {
    fn default() -> Self {
        Self {
            previous_keys: &[],
            keys: &[],
            first_visible_index: 0,
            offset: 0.,
            row_height: 0.,
            viewport_height: 0.,
            start_offset: 0.,
            trailing_content_height: 0.,
            key_indices: None,
            offsets: None,
        }
    }
}

/// The scroll position that keeps the first visible row's key, and how far
/// into it the viewport was, after the rows change (FR2). When that key is
/// gone, the nearest surviving successor, then predecessor, then the start.
pub fn reconcile_list_anchor(o: &ListAnchorOptions) -> f64 {
    let built;
    let indices = match o.key_indices {
        Some(map) => map,
        None => {
            built = key_indices(o.keys);
            &built
        }
    };
    let find = |i: usize| o.previous_keys.get(i).and_then(|k| indices.get(k)).copied();
    let mut index = find(o.first_visible_index);
    if index.is_none() {
        index = (o.first_visible_index + 1..o.previous_keys.len()).find_map(find);
    }
    if index.is_none() && o.first_visible_index > 0 {
        let from = (o.first_visible_index - 1).min(o.previous_keys.len().saturating_sub(1));
        index = (0..=from).rev().find_map(find);
    }
    let extent = match o.offsets {
        Some(offsets) => offsets.get(o.keys.len()).copied().unwrap_or(0.),
        None => o.keys.len() as f64 * o.row_height,
    };
    let maximum_scroll =
        (o.start_offset + extent + o.trailing_content_height - o.viewport_height).max(0.);
    if let Some(offsets) = o.offsets {
        let item_start = offsets.get(index.unwrap_or(0)).copied().unwrap_or(0.);
        let item_size = match index {
            Some(i) => offsets.get(i + 1).copied().unwrap_or(item_start) - item_start,
            None => 0.,
        };
        let fraction = o.offset.clamp(0., 1.);
        return (o.start_offset + item_start + fraction * item_size)
            .min(maximum_scroll)
            .max(0.);
    }
    // With no surviving key, a position inside the leading content stays
    // where it is, and a position inside the rows moves to the first row.
    let fraction = match index {
        None => o.offset.max(-o.start_offset).min(0.),
        Some(_) => o.offset.max(-o.start_offset).min(o.row_height),
    };
    maximum_scroll.min(o.start_offset + index.unwrap_or(0) as f64 * o.row_height + fraction)
}

/// The key-to-index map of a key list.
pub fn key_indices(keys: &[ListWindowKey]) -> HashMap<ListWindowKey, usize> {
    keys.iter()
        .cloned()
        .enumerate()
        .map(|(i, k)| (k, i))
        .collect()
}

/// What a windowed list holds in place when its rows change (FR1).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ListAnchor {
    /// The first visible row keeps its place (upstream's one anchor).
    #[default]
    FirstVisible,
    /// The end: a list scrolled to its end stays there as rows arrive, for a
    /// streaming log or chat; scrolled away from it, the first visible row
    /// holds.
    End,
    /// A given item: it opens in view, at the viewport's start, and keeps its
    /// distance from the viewport's start through changes above it; while it
    /// is absent, the first visible row holds.
    Item(ListWindowKey),
}

/// The scroll position after the rows change, for an anchor (FR1, FR2).
/// `previous_scroll` and `previous_maximum` are the scroll position and its
/// limit before the change; `item_starts` gives an item's start before and
/// after the change, from the scroll origin.
pub fn reconcile_anchor(
    anchor: &ListAnchor,
    first_visible: &ListAnchorOptions,
    previous_scroll: f64,
    previous_maximum: f64,
    item_starts: impl Fn(&ListWindowKey) -> Option<(f64, f64)>,
) -> f64 {
    let extent = match first_visible.offsets {
        Some(offsets) => offsets.get(first_visible.keys.len()).copied().unwrap_or(0.),
        None => first_visible.keys.len() as f64 * first_visible.row_height,
    };
    let maximum = (first_visible.start_offset + extent + first_visible.trailing_content_height
        - first_visible.viewport_height)
        .max(0.);
    match anchor {
        ListAnchor::End if previous_scroll >= previous_maximum - 0.5 => maximum,
        ListAnchor::Item(key) => match item_starts(key) {
            Some((before, after)) => (after - (before - previous_scroll)).clamp(0., maximum),
            None => reconcile_list_anchor(first_visible),
        },
        _ => reconcile_list_anchor(first_visible),
    }
}

/// The logical inline scroll distance, from the inline start, for a physical
/// scroll offset (GPUI's: zero or negative, from the left).
pub fn logical_inline_scroll(physical: f64, maximum: f64, rtl: bool) -> f64 {
    let from_left = -physical;
    if rtl { maximum - from_left } else { from_left }
}

/// The physical scroll offset for a logical inline distance.
pub fn physical_inline_scroll(logical: f64, maximum: f64, rtl: bool) -> f64 {
    if rtl { -(maximum - logical) } else { -logical }
}

#[cfg(test)]
mod tests;
