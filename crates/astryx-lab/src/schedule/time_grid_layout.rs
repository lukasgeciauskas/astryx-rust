//! The time grid's side-by-side layout (upstream `timeGridLayout.ts`): vertical extent as a
//! share of the visible minutes, and a column start and span inside each overlap
//! cluster.
//!
//! Two events share a cluster when their minutes intersect directly or through a chain
//! of intersecting events. Inside a cluster every event takes the first column whose
//! last occupant has ended, then widens into the following columns that nothing
//! occupies during its own minutes. Ordering is by start, then longer first, then `id`,
//! so the same events produce the same layout in any input order and nothing depends on
//! paint order. Pure: no dates, no styling.

use std::cmp::Ordering;

use gpui::SharedString;

use super::month_layout::code_unit_order;

/// A timed event clipped to a day column's visible minutes (upstream
/// `TimedEventInterval`).
#[derive(Clone, Debug, PartialEq)]
pub struct TimedEventInterval<E> {
    pub event: E,
    pub id: SharedString,
    /// Minutes since the start of the day, after clipping to the visible window.
    pub visible_start: f64,
    pub visible_end: f64,
}

/// Where one block paints (upstream `TimedEventPlacement`).
#[derive(Clone, Debug, PartialEq)]
pub struct TimedEventPlacement<E> {
    pub event: E,
    /// Block-axis offset as a percentage of the visible minutes.
    pub top: f64,
    /// Block-axis size as a percentage of the visible minutes.
    pub height: f64,
    /// Zero-based column inside the event's overlap cluster.
    pub column_start: usize,
    /// Columns the block widens across, including its own.
    pub column_span: usize,
    /// Columns in the event's overlap cluster.
    pub column_count: usize,
}

fn js_cmp(a: f64, b: f64) -> Ordering {
    // `a - b` as a comparator: NaN is equal.
    let d = a - b;
    if d < 0. {
        Ordering::Less
    } else if d > 0. {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

fn compare_intervals<E>(a: &TimedEventInterval<E>, b: &TimedEventInterval<E>) -> Ordering {
    if a.visible_start != b.visible_start {
        return js_cmp(a.visible_start, b.visible_start);
    }
    if a.visible_end != b.visible_end {
        return js_cmp(b.visible_end, a.visible_end);
    }
    code_unit_order(&a.id, &b.id)
}

fn overlaps<E>(a: &TimedEventInterval<E>, b: &TimedEventInterval<E>) -> bool {
    a.visible_start < b.visible_end && b.visible_start < a.visible_end
}

/// Lays out one day column's timed events (upstream `layoutTimedEvents`).
pub fn layout_timed_events<E: Clone>(
    intervals: &[TimedEventInterval<E>],
    min_minute: f64,
    max_minute: f64,
) -> Vec<TimedEventPlacement<E>> {
    let total_minutes = (max_minute - min_minute).max(1.);
    let mut sorted: Vec<&TimedEventInterval<E>> = intervals.iter().collect();
    sorted.sort_by(|a, b| compare_intervals(a, b));

    let mut clusters: Vec<Vec<&TimedEventInterval<E>>> = Vec::new();
    let mut cluster_end = f64::NEG_INFINITY;
    for interval in sorted {
        match clusters.last_mut() {
            // `!(start >= end)`: a NaN start or end joins the current cluster, as in JavaScript.
            Some(current)
                if !matches!(
                    interval.visible_start.partial_cmp(&cluster_end),
                    Some(Ordering::Greater | Ordering::Equal)
                ) =>
            {
                current.push(interval);
                cluster_end = if interval.visible_end.is_nan() || cluster_end.is_nan() {
                    f64::NAN
                } else {
                    cluster_end.max(interval.visible_end)
                };
            }
            _ => {
                clusters.push(vec![interval]);
                cluster_end = interval.visible_end;
            }
        }
    }

    let mut placements = Vec::new();
    for cluster in &clusters {
        let mut columns: Vec<Vec<usize>> = Vec::new();
        let mut column_of = vec![0usize; cluster.len()];
        for (i, interval) in cluster.iter().enumerate() {
            let column = columns
                .iter()
                .position(|occupants| {
                    let last = cluster[*occupants.last().expect("a column has an occupant")];
                    last.visible_end <= interval.visible_start
                })
                .unwrap_or_else(|| {
                    columns.push(Vec::new());
                    columns.len() - 1
                });
            columns[column].push(i);
            column_of[i] = column;
        }
        for (i, interval) in cluster.iter().enumerate() {
            let column_start = column_of[i];
            let mut column_span = 1;
            for occupants in columns.iter().skip(column_start + 1) {
                if occupants.iter().any(|o| overlaps(cluster[*o], interval)) {
                    break;
                }
                column_span += 1;
            }
            placements.push(TimedEventPlacement {
                event: interval.event.clone(),
                top: ((interval.visible_start - min_minute) / total_minutes) * 100.,
                height: ((interval.visible_end - interval.visible_start) / total_minutes) * 100.,
                column_start,
                column_span,
                column_count: columns.len(),
            });
        }
    }
    placements
}
