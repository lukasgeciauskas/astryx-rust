//! The reorder's geometry, after upstream `List/reorderModel.ts`: where a
//! dragged row lands, the drop range's bounds, the edge autoscroll's step and
//! where the dragged row may go.

/// UIKit's edge autoscroll (upstream `motion/uikit/reorder.ts`): a timer of
/// whole-point steps.
pub(crate) const AUTOSCROLL_TICK_MS: u64 = 20;
/// How far the dragged row's edge is past the scroll edge at each sample
/// (negative inside), and the points per tick there.
const AUTOSCROLL_DEPTHS: [f32; 8] = [-3., 6., 8., 10., 12., 16., 20., 22.];
const AUTOSCROLL_STEPS: [f32; 8] = [2., 2., 3., 3., 4., 6., 8., 10.];
/// How far inside the edge autoscroll starts, at the first step.
const AUTOSCROLL_START_INSET: f32 = 8.;
/// The travel that turns a mouse press on a handle into a drag.
pub(crate) const MOUSE_DRAG_SLOP: f32 = 4.;
/// The dragged row's and its preview's reduced emphasis.
pub(crate) const MOVING_OPACITY: f32 = 0.6;
/// The insertion line's thickness.
pub(crate) const INSERTION_LINE_THICKNESS: f32 = 2.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReorderRow {
    pub top: f32,
    pub height: f32,
    /// A row with no handle in a run at either end: it bounds a drag.
    pub is_fixed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReorderGeometry {
    pub rows: Vec<ReorderRow>,
    /// The space between two consecutive rows.
    pub gap: f32,
}

/// Which rows bound the drop range: each run of rows that cannot move at
/// either end. One in the middle is passed like any other.
pub(crate) fn bounds_of(movable: &[bool]) -> Vec<bool> {
    let mut bounds = vec![false; movable.len()];
    for (i, m) in movable.iter().enumerate() {
        if *m {
            break;
        }
        bounds[i] = true;
    }
    for (i, m) in movable.iter().enumerate().rev() {
        if *m {
            break;
        }
        bounds[i] = true;
    }
    bounds
}

/// The positions `dragged` can take: all but those of the fixed rows that
/// end the list.
pub(crate) fn movable_range(rows: &[ReorderRow], dragged: usize) -> (usize, usize) {
    let mut start = 0;
    while start < dragged && rows[start].is_fixed {
        start += 1;
    }
    let mut end = rows.len().saturating_sub(1);
    while end > dragged && rows[end].is_fixed {
        end -= 1;
    }
    (start, end)
}

/// The position the dragged row proposes for its centre, in list
/// coordinates. A row below changes places once the centre passes that row's
/// far edge less the dragged row's height (half a row for equal rows); a row
/// above mirrors it. Boundaries sit in the middle of a gap.
pub(crate) fn drop_position(geometry: &ReorderGeometry, dragged: usize, center: f32) -> usize {
    let height = geometry.rows[dragged].height;
    let mut position = 0;
    for (index, row) in geometry.rows.iter().enumerate() {
        if index == dragged {
            continue;
        }
        let boundary = if index < dragged {
            (row.top + height).min(row.top + row.height) + geometry.gap / 2.
        } else {
            (row.top + row.height - height).max(row.top) - geometry.gap / 2.
        };
        if boundary < center {
            position += 1;
        }
    }
    let (start, end) = movable_range(&geometry.rows, dragged);
    position.clamp(start, end)
}

/// Points per autoscroll tick with the dragged row's edge `depth` points
/// past the scroll edge (negative inside).
pub(crate) fn autoscroll_step(depth: f32) -> f32 {
    if depth < -AUTOSCROLL_START_INSET {
        return 0.;
    }
    if depth <= AUTOSCROLL_DEPTHS[0] {
        return AUTOSCROLL_STEPS[0];
    }
    let last = AUTOSCROLL_DEPTHS.len() - 1;
    if depth >= AUTOSCROLL_DEPTHS[last] {
        return AUTOSCROLL_STEPS[last];
    }
    let mut index = 1;
    while AUTOSCROLL_DEPTHS[index] < depth {
        index += 1;
    }
    let (d0, d1) = (AUTOSCROLL_DEPTHS[index - 1], AUTOSCROLL_DEPTHS[index]);
    let (s0, s1) = (AUTOSCROLL_STEPS[index - 1], AUTOSCROLL_STEPS[index]);
    // JavaScript's Math.round: halves round up.
    (s0 + (s1 - s0) * (depth - d0) / (d1 - d0) + 0.5).floor()
}

/// Where a dragged row may go: the scroller's visible edges and the list's
/// ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReorderBounds {
    pub scroll_top: f32,
    pub scroll_bottom: f32,
    pub list_top: f32,
    pub list_bottom: f32,
}

/// The dragged row's top. At the scroller's visible edge it stops with its
/// centre on the edge; at the list's own end it stops with its edge there.
pub(crate) fn clamp_top(top: f32, height: f32, b: ReorderBounds) -> f32 {
    let min = (b.scroll_top - height / 2.).max(b.list_top);
    let max = (b.scroll_bottom - height / 2.).min(b.list_bottom - height);
    top.max(min).min(max)
}

/// Where the insertion line's centre sits for the dragged row at
/// `position`: in the gap before the row it would land above, or after the
/// one it would land below. `None` at its own place.
pub(crate) fn line_center(
    geometry: &ReorderGeometry,
    dragged: usize,
    position: usize,
) -> Option<f32> {
    if position == dragged {
        return None;
    }
    let target = geometry.rows[position];
    Some(if position < dragged {
        target.top - geometry.gap / 2.
    } else {
        target.top + target.height + geometry.gap / 2.
    })
}

/// Reorder mode's line: above the row after a move up (and before any
/// move), below it after a move down, in the middle of the gap.
pub(crate) fn mode_line_center(rows: &[(f32, f32)], index: usize, step: i8) -> f32 {
    let (top, height) = rows[index];
    let bottom = top + height;
    let neighbour = if step < 0 {
        index.checked_sub(1)
    } else {
        Some(index + 1)
    }
    .and_then(|i| rows.get(i));
    let gap = match neighbour {
        None => 0.,
        Some(&(n_top, n_height)) if step < 0 => (top - (n_top + n_height)).max(0.),
        Some(&(n_top, _)) => (n_top - bottom).max(0.),
    };
    if step < 0 {
        top - gap / 2.
    } else {
        bottom + gap / 2.
    }
}

/// An item's box, in the collection's coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2., self.y + self.h / 2.)
    }
}

/// How a collection lays its items out, read from their boxes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReorderLayout {
    /// One column: Up and Down move.
    Vertical,
    /// One row: Left and Right move.
    Horizontal,
    /// Rows of several items (a grid, a wrapping board): all four move.
    Grid,
}

pub(crate) fn layout_of(rects: &[Rect]) -> ReorderLayout {
    let same = |f: fn(&Rect) -> f32| rects.windows(2).all(|w| (f(&w[0]) - f(&w[1])).abs() <= 1.);
    if rects.len() < 2 || same(|r| r.x) {
        ReorderLayout::Vertical
    } else if same(|r| r.y) {
        ReorderLayout::Horizontal
    } else {
        ReorderLayout::Grid
    }
}

/// The items' rows, in reading order: a row ends where the next item sits
/// lower.
pub(crate) fn rows_of(rects: &[Rect]) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for (i, r) in rects.iter().enumerate() {
        match rows.last_mut() {
            Some(row) if (rects[row[0]].y - r.y).abs() <= 1. => row.push(i),
            _ => rows.push(vec![i]),
        }
    }
    rows
}

/// An arrow key in reorder mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arrow {
    Up,
    Down,
    Left,
    Right,
}

/// Where an arrow moves the item at `index` (FR4): Up and Down along a
/// column, Left and Right along a row (mirrored right to left); in a grid,
/// Left and Right go to the previous and next place in reading order, and
/// Up and Down to the same column of the row above or below, or that row's
/// last place when it is shorter. `None` when the key does not move on
/// this layout or the place is past an end.
pub(crate) fn arrow_target(rects: &[Rect], index: usize, arrow: Arrow, rtl: bool) -> Option<usize> {
    let back = |i: usize| i.checked_sub(1);
    let forward = |i: usize| (i + 1 < rects.len()).then_some(i + 1);
    let (left, right) = if rtl {
        (forward(index), back(index))
    } else {
        (back(index), forward(index))
    };
    match (layout_of(rects), arrow) {
        (ReorderLayout::Vertical, Arrow::Up) => back(index),
        (ReorderLayout::Vertical, Arrow::Down) => forward(index),
        (ReorderLayout::Vertical, _) => None,
        (_, Arrow::Left) => left,
        (_, Arrow::Right) => right,
        (ReorderLayout::Horizontal, _) => None,
        (ReorderLayout::Grid, vertical) => {
            let rows = rows_of(rects);
            let r = rows.iter().position(|row| row.contains(&index))?;
            let c = rows[r].iter().position(|&i| i == index)?;
            let target = if vertical == Arrow::Up {
                r.checked_sub(1)?
            } else {
                r + 1
            };
            let row = rows.get(target)?;
            Some(row[c.min(row.len() - 1)])
        }
    }
}

/// An item's row and column, one-based, for a grid's announcement.
pub(crate) fn row_and_column(rects: &[Rect], index: usize) -> (usize, usize) {
    let rows = rows_of(rects);
    for (r, row) in rows.iter().enumerate() {
        if let Some(c) = row.iter().position(|&i| i == index) {
            return (r + 1, c + 1);
        }
    }
    (1, index + 1)
}

/// Where the dragged item's centre sits when it lands in each slot, along a
/// one-axis list of varying sizes (`starts`, `sizes`): a slot before it
/// starts where that item starts, one after it ends where that item ends.
pub(crate) fn slot_centers(starts: &[f32], sizes: &[f32], dragged: usize) -> Vec<f32> {
    let own = sizes[dragged];
    (0..starts.len())
        .map(|i| match i.cmp(&dragged) {
            std::cmp::Ordering::Less => starts[i] + own / 2.,
            std::cmp::Ordering::Equal => starts[i] + own / 2.,
            std::cmp::Ordering::Greater => starts[i] + sizes[i] - own / 2.,
        })
        .collect()
}

/// FR1's insertion-line candidate for slots it cannot order by rows: the
/// slot whose centre is nearest the dragged item's centre, within the drop
/// range.
pub(crate) fn nearest_slot(centers: &[(f32, f32)], at: (f32, f32), range: (usize, usize)) -> usize {
    let d = |c: &(f32, f32)| (c.0 - at.0).powi(2) + (c.1 - at.1).powi(2);
    (range.0..=range.1)
        .min_by(|a, b| d(&centers[*a]).total_cmp(&d(&centers[*b])))
        .unwrap_or(range.0)
}

/// FR9's midpoint rule: the candidate moves from `current` to another slot
/// only once the dragged item's centre passes that slot's centre, measured
/// along the line between the two; when several qualify the nearest wins.
/// So a pointer resting near a boundary does not flicker.
pub(crate) fn shift_candidate(
    centers: &[(f32, f32)],
    current: usize,
    at: (f32, f32),
    range: (usize, usize),
) -> usize {
    let c = centers[current];
    let d = |p: &(f32, f32)| (p.0 - at.0).powi(2) + (p.1 - at.1).powi(2);
    (range.0..=range.1)
        .filter(|&s| s != current)
        .filter(|&s| {
            let line = (centers[s].0 - c.0, centers[s].1 - c.1);
            let length = line.0 * line.0 + line.1 * line.1;
            length > 0. && ((at.0 - c.0) * line.0 + (at.1 - c.1) * line.1) / length >= 1.
        })
        .min_by(|a, b| d(&centers[*a]).total_cmp(&d(&centers[*b])))
        .unwrap_or(current)
}

/// Where each item shifts while the dragged one would land at `candidate`
/// (FR9): the items it passes take the next slot toward its old place. In a
/// one-axis list, by the dragged item's pitch; otherwise from slot to slot.
pub(crate) fn shift_offset(
    centers: &[(f32, f32)],
    layout: ReorderLayout,
    given: bool,
    pitch: f32,
    dragged: usize,
    candidate: usize,
    index: usize,
) -> (f32, f32) {
    let to = if index > dragged && index <= candidate {
        index - 1
    } else if index < dragged && index >= candidate {
        index + 1
    } else {
        return (0., 0.);
    };
    let sign = if to < index { -1. } else { 1. };
    match layout {
        ReorderLayout::Vertical if !given => (0., sign * pitch),
        ReorderLayout::Horizontal if !given => (sign * pitch, 0.),
        _ => (
            centers[to].0 - centers[index].0,
            centers[to].1 - centers[index].1,
        ),
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    fn grid(columns: usize, count: usize) -> Vec<Rect> {
        (0..count)
            .map(|i| Rect {
                x: (i % columns) as f32 * 110.,
                y: (i / columns) as f32 * 70.,
                w: 100.,
                h: 60.,
            })
            .collect()
    }

    #[test]
    fn reads_the_layout_from_the_boxes() {
        assert_eq!(layout_of(&grid(1, 4)), ReorderLayout::Vertical);
        assert_eq!(layout_of(&grid(4, 4)), ReorderLayout::Horizontal);
        assert_eq!(layout_of(&grid(3, 7)), ReorderLayout::Grid);
        assert_eq!(
            rows_of(&grid(3, 7)),
            [vec![0, 1, 2], vec![3, 4, 5], vec![6]]
        );
    }

    #[test]
    fn fr4_arrows_in_a_grid() {
        let g = grid(3, 7);
        // Left and Right go along reading order, across a row's end.
        assert_eq!(arrow_target(&g, 2, Arrow::Right, false), Some(3));
        assert_eq!(arrow_target(&g, 3, Arrow::Left, false), Some(2));
        // Right to left mirrors them.
        assert_eq!(arrow_target(&g, 2, Arrow::Left, true), Some(3));
        // Up and Down keep the column; a shorter row takes its last place.
        assert_eq!(arrow_target(&g, 1, Arrow::Down, false), Some(4));
        assert_eq!(arrow_target(&g, 5, Arrow::Down, false), Some(6));
        assert_eq!(arrow_target(&g, 4, Arrow::Up, false), Some(1));
        assert_eq!(arrow_target(&g, 1, Arrow::Up, false), None);
        assert_eq!(arrow_target(&g, 6, Arrow::Right, false), None);
        assert_eq!(row_and_column(&g, 4), (2, 2));
        // A column ignores Left and Right; a row ignores Up and Down.
        assert_eq!(arrow_target(&grid(1, 3), 1, Arrow::Left, false), None);
        assert_eq!(arrow_target(&grid(3, 3), 1, Arrow::Down, false), None);
        assert_eq!(arrow_target(&grid(3, 3), 1, Arrow::Left, false), Some(0));
    }

    #[test]
    fn slot_centres_along_one_axis() {
        // Rows of 40, 80, 40: dragging the first, its slot after the tall
        // row ends where that row ends.
        let c = slot_centers(&[0., 40., 120.], &[40., 80., 40.], 0);
        assert_eq!(c, [20., 100., 140.]);
    }

    #[test]
    fn fr1_the_nearest_slot_wins() {
        let centers = [(50., 30.), (160., 30.), (270., 30.), (50., 100.)];
        assert_eq!(nearest_slot(&centers, (150., 40.), (0, 3)), 1);
        assert_eq!(nearest_slot(&centers, (60., 90.), (0, 3)), 3);
        assert_eq!(nearest_slot(&centers, (60., 90.), (0, 2)), 0);
    }

    #[test]
    fn fr9_the_candidate_moves_once_the_centre_passes_the_slot_centre() {
        let centers = [(0., 20.), (0., 60.), (0., 100.)];
        // Resting near the boundary between 0 and 1 keeps 0.
        assert_eq!(shift_candidate(&centers, 0, (0., 41.), (0, 2)), 0);
        assert_eq!(shift_candidate(&centers, 0, (0., 59.), (0, 2)), 0);
        assert_eq!(shift_candidate(&centers, 0, (0., 60.), (0, 2)), 1);
        // Back from 1: only once past slot 0's centre.
        assert_eq!(shift_candidate(&centers, 1, (0., 30.), (0, 2)), 1);
        assert_eq!(shift_candidate(&centers, 1, (0., 20.), (0, 2)), 0);
        // Past several, the nearest qualifying wins.
        assert_eq!(shift_candidate(&centers, 0, (0., 110.), (0, 2)), 2);
    }

    #[test]
    fn fr9_the_passed_items_take_the_next_slot() {
        let rows = grid(1, 4);
        let centers: Vec<_> = rows.iter().map(Rect::center).collect();
        // Dragging row 0 to 2: rows 1 and 2 move up a pitch.
        for (i, expected) in [(0, 0.), (1, -70.), (2, -70.), (3, 0.)] {
            assert_eq!(
                shift_offset(&centers, ReorderLayout::Vertical, false, 70., 0, 2, i),
                (0., expected)
            );
        }
        // In a grid, from slot to slot.
        let gc: Vec<_> = grid(3, 6).iter().map(Rect::center).collect();
        assert_eq!(
            shift_offset(&gc, ReorderLayout::Grid, false, 0., 1, 3, 3),
            (220., -70.)
        );
        assert_eq!(
            shift_offset(&gc, ReorderLayout::Grid, false, 0., 1, 3, 2),
            (-110., 0.)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(heights: &[f32], gap: f32, fixed: &[usize]) -> ReorderGeometry {
        let mut top = 0.;
        let rows = heights
            .iter()
            .enumerate()
            .map(|(i, &height)| {
                let row = ReorderRow {
                    top,
                    height,
                    is_fixed: fixed.contains(&i),
                };
                top += height + gap;
                row
            })
            .collect();
        ReorderGeometry { rows, gap }
    }

    fn measured() -> ReorderGeometry {
        let heights: Vec<f32> = (0..40)
            .map(|i| if i == 6 || i == 9 { 88. } else { 44. })
            .collect();
        table(&heights, 0., &[])
    }

    #[test]
    fn reproduces_the_measured_drag_over_the_tall_row_and_back() {
        let m = measured();
        for (center, position) in [
            (178., 4),
            (221., 5),
            (309., 6),
            (354., 7),
            (398., 8),
            (396., 7),
            (352., 6),
            (306.8, 5),
        ] {
            assert_eq!(drop_position(&m, 3, center), position, "centre {center}");
        }
    }

    #[test]
    fn swaps_a_neighbour_after_half_a_row_and_a_tall_row_after_its_height() {
        let m = measured();
        assert_eq!(drop_position(&m, 3, 154.), 3);
        assert_eq!(drop_position(&m, 3, 176.), 3);
        assert_eq!(drop_position(&m, 3, 176.5), 4);
        assert_eq!(drop_position(&m, 3, 132.), 2);
        assert_eq!(drop_position(&m, 3, 221.), 5);
        assert_eq!(drop_position(&m, 3, 307.), 5);
        assert_eq!(drop_position(&m, 3, 309.), 6);
    }

    #[test]
    fn a_tall_row_passes_a_short_one_once_its_centre_passes_the_near_edge() {
        let g = table(&[44., 88., 44., 44.], 0., &[]);
        assert_eq!(drop_position(&g, 1, 88.), 1);
        assert_eq!(drop_position(&g, 1, 132.), 1);
        assert_eq!(drop_position(&g, 1, 132.5), 2);
        assert_eq!(drop_position(&g, 1, 44.5), 1);
        assert_eq!(drop_position(&g, 1, 44.), 0);
    }

    #[test]
    fn never_proposes_a_new_position_at_rest() {
        for heights in [
            vec![44., 100., 44., 44.],
            vec![30., 120., 30., 30., 30.],
            vec![88., 44., 88., 44.],
            vec![44., 44., 200., 44.],
        ] {
            let g = table(&heights, 4., &[]);
            for (i, row) in g.rows.iter().enumerate() {
                assert_eq!(
                    drop_position(&g, i, row.top + row.height / 2.),
                    i,
                    "{heights:?} row {i}"
                );
            }
        }
    }

    #[test]
    fn a_short_row_moves_up_past_a_tall_row_as_it_moves_down_past_one() {
        let m = measured();
        assert_eq!(drop_position(&m, 7, 374.), 7);
        assert_eq!(drop_position(&m, 7, 309.), 7);
        assert_eq!(drop_position(&m, 7, 308.), 6);
    }

    #[test]
    fn boundaries_sit_in_the_middle_of_the_gap() {
        let g = table(&[40., 40., 40.], 4., &[]);
        assert_eq!(drop_position(&g, 0, 42.), 0);
        assert_eq!(drop_position(&g, 0, 42.5), 1);
    }

    #[test]
    fn stops_at_fixed_rows_that_end_the_list_and_passes_one_in_the_middle() {
        let ends = table(&[44.; 5], 0., &[0, 4]);
        assert_eq!(drop_position(&ends, 2, 1000.), 3);
        assert_eq!(drop_position(&ends, 2, -1000.), 1);
        let middle = table(&[44.; 5], 0., &[2]);
        assert_eq!(drop_position(&middle, 0, 1000.), 4);
        assert_eq!(drop_position(&middle, 4, -1000.), 0);
    }

    #[test]
    fn the_movable_range_is_bounded_only_by_fixed_rows_at_the_ends() {
        let rows = table(&[44.; 6], 0., &[0, 1, 5]).rows;
        assert_eq!(movable_range(&rows, 3), (2, 4));
        let middle = table(&[44.; 6], 0., &[1, 4]).rows;
        assert_eq!(movable_range(&middle, 2), (0, 5));
        assert_eq!(movable_range(&middle, 0), (0, 5));
        assert_eq!(movable_range(&middle, 5), (0, 5));
        assert_eq!(movable_range(&table(&[44.; 3], 0., &[]).rows, 1), (0, 2));
        assert_eq!(
            bounds_of(&[false, true, false, true, false]),
            [true, false, false, false, true]
        );
        assert_eq!(bounds_of(&[false, false]), [true, true]);
    }

    #[test]
    fn autoscroll_follows_the_measured_steps() {
        assert_eq!(autoscroll_step(-12.), 0.);
        assert_eq!(autoscroll_step(-AUTOSCROLL_START_INSET - 0.5), 0.);
        assert_eq!(autoscroll_step(-AUTOSCROLL_START_INSET), 2.);
        for (depth, step) in [
            (-3., 2.),
            (6., 2.),
            (8., 3.),
            (10., 3.),
            (12., 4.),
            (16., 6.),
            (20., 8.),
            (22., 10.),
            (40., 10.),
            (14., 5.),
            (21., 9.),
        ] {
            assert_eq!(autoscroll_step(depth), step, "depth {depth}");
        }
    }

    #[test]
    fn the_dragged_row_stops_with_its_centre_on_the_scroll_edge_or_its_edge_on_the_list_end() {
        let b = ReorderBounds {
            scroll_top: 100.,
            scroll_bottom: 500.,
            list_top: 0.,
            list_bottom: 1000.,
        };
        assert_eq!(clamp_top(0., 40., b), 80.);
        assert_eq!(clamp_top(600., 40., b), 480.);
        let short = ReorderBounds {
            list_top: 150.,
            list_bottom: 300.,
            ..b
        };
        assert_eq!(clamp_top(0., 40., short), 150.);
        assert_eq!(clamp_top(600., 40., short), 260.);
    }

    #[test]
    fn line_centres() {
        let g = table(&[40., 40., 40.], 4., &[]);
        assert_eq!(line_center(&g, 1, 1), None);
        assert_eq!(line_center(&g, 1, 0), Some(-2.));
        assert_eq!(line_center(&g, 1, 2), Some(130.));
        let rows = [(0., 40.), (44., 40.), (88., 40.)];
        assert_eq!(mode_line_center(&rows, 1, -1), 42.);
        assert_eq!(mode_line_center(&rows, 1, 1), 86.);
        assert_eq!(mode_line_center(&rows, 0, -1), 0.);
    }
}
