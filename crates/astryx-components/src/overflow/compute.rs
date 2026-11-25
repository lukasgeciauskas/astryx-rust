//! The fit, upstream `hooks/computeOverflow.ts` and `inlineBoundsOf`: how many
//! items fit a width, on one line or packed into rows.

/// Upstream `collapseFrom`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CollapseFrom {
    Start,
    /// Keep the leading items.
    #[default]
    End,
}

/// What fits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fit {
    pub visible: usize,
    pub rows: usize,
}

/// What the fit reads.
pub(crate) struct FitInput<'a> {
    pub widths: &'a [f32],
    pub gap: f32,
    pub available: f32,
    pub indicator: f32,
    pub min_visible: usize,
    pub max_visible: Option<usize>,
    pub max_rows: Option<usize>,
    pub collapse_from: CollapseFrom,
}

fn single_line(
    widths: &[f32],
    gap: f32,
    available: f32,
    indicator: f32,
    floor: usize,
    ceiling: usize,
) -> usize {
    let all = widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    if widths.len() <= ceiling && all <= available {
        return widths.len();
    }
    let (mut total, mut count) = (0., 0usize);
    for (i, &width) in widths.iter().enumerate() {
        if count >= ceiling {
            break;
        }
        let candidate = total + width + if i > 0 { gap } else { 0. };
        let reserved = if i + 1 == widths.len() {
            0.
        } else {
            indicator + if count > 0 || indicator > 0. { gap } else { 0. }
        };
        if candidate + reserved > available && count >= floor {
            break;
        }
        total = candidate;
        count += 1;
    }
    count
}

fn pack_rows(
    widths: &[f32],
    gap: f32,
    available: f32,
    reserve: f32,
    max_rows: usize,
) -> (usize, usize) {
    let (mut placed, mut row, mut row_width) = (0usize, 1usize, 0f32);
    let mut i = 0;
    while i < widths.len() {
        let w = widths[i];
        let first = row_width == 0.;
        let candidate = if first { w } else { row_width + gap + w };
        let on_last = row == max_rows;
        let reserved = if on_last && reserve > 0. {
            reserve + gap
        } else {
            0.
        };
        if candidate + reserved <= available {
            row_width = candidate;
            placed += 1;
            i += 1;
            continue;
        }
        if first {
            if on_last && reserved > 0. {
                break;
            }
            row_width = candidate;
            placed += 1;
            i += 1;
            continue;
        }
        if row >= max_rows {
            break;
        }
        row += 1;
        row_width = 0.;
    }
    (placed, row)
}

/// Upstream `countRows`: the rows the widths wrap into.
pub(crate) fn count_rows(widths: &[f32], gap: f32, available: f32) -> usize {
    if widths.is_empty() {
        return 0;
    }
    let (mut rows, mut row_width) = (1usize, 0f32);
    for &w in widths {
        let first = row_width == 0.;
        let candidate = if first { w } else { row_width + gap + w };
        if candidate <= available || first {
            row_width = candidate;
        } else {
            rows += 1;
            row_width = w;
        }
    }
    rows
}

/// Upstream `computeOverflow`.
pub(crate) fn compute(input: &FitInput) -> Fit {
    let n = input.widths.len();
    if n == 0 {
        return Fit {
            visible: 0,
            rows: 0,
        };
    }
    let floor = input.min_visible.min(n);
    let ceiling = input.max_visible.unwrap_or(n).min(n);
    let ordered: Vec<f32> = match input.collapse_from {
        CollapseFrom::End => input.widths.to_vec(),
        CollapseFrom::Start => input.widths.iter().rev().copied().collect(),
    };
    // The floor wins over a smaller ceiling.
    let clamp = |v: usize| v.min(ceiling).max(floor);
    match input.max_rows.filter(|&r| r > 1) {
        None => {
            let fit = single_line(
                &ordered,
                input.gap,
                input.available,
                input.indicator,
                floor,
                ceiling,
            );
            let visible = clamp(fit);
            Fit {
                visible,
                rows: usize::from(visible > 0),
            }
        }
        Some(max_rows) => {
            let (all, all_rows) = pack_rows(&ordered, input.gap, input.available, 0., max_rows);
            let (count, rows) = if all == n {
                (n, all_rows)
            } else {
                let (placed, _) = pack_rows(
                    &ordered,
                    input.gap,
                    input.available,
                    input.indicator,
                    max_rows,
                );
                let rows = count_rows(&ordered[..placed], input.gap, input.available);
                (placed, rows.max(usize::from(placed > 0)))
            };
            let visible = clamp(count);
            let rows = if visible == count {
                rows
            } else {
                count_rows(&ordered[..visible], input.gap, input.available)
            };
            Fit {
                visible,
                rows: if visible > 0 { rows.max(1) } else { 0 },
            }
        }
    }
}

/// Upstream `inlineBoundsOf`: the fully folded width (the items the floor
/// keeps and the indicator) and the unfolded width (up to the cap).
pub(crate) fn inline_bounds(
    widths: &[f32],
    indicator: f32,
    gap: f32,
    min_visible: usize,
    collapse_from: CollapseFrom,
    max_visible: Option<usize>,
) -> (f32, f32) {
    let span = |set: &[f32]| set.iter().sum::<f32>() + gap * set.len().saturating_sub(1) as f32;
    let kept = min_visible.min(widths.len());
    let pick = |count: usize| match collapse_from {
        CollapseFrom::End => &widths[..count],
        CollapseFrom::Start => &widths[widths.len() - count..],
    };
    let kept_widths = pick(kept);
    let collapsed = if kept < widths.len() {
        span(kept_widths) + if kept_widths.is_empty() { 0. } else { gap } + indicator
    } else {
        span(widths)
    };
    let allowed = kept.max(max_visible.unwrap_or(widths.len()).min(widths.len()));
    let allowed_widths = pick(allowed);
    let capped = allowed < widths.len() && indicator > 0.;
    let rendered = span(allowed_widths)
        + if capped {
            (if allowed_widths.is_empty() { 0. } else { gap }) + indicator
        } else {
            0.
        };
    (collapsed.ceil(), rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(widths: &[f32], available: f32, indicator: f32) -> FitInput<'_> {
        FitInput {
            widths,
            gap: 8.,
            available,
            indicator,
            min_visible: 0,
            max_visible: None,
            max_rows: None,
            collapse_from: CollapseFrom::End,
        }
    }

    #[test]
    fn everything_fits_or_the_indicator_takes_room() {
        let widths = [50., 50., 50., 50.];
        assert_eq!(compute(&fit(&widths, 300., 30.)).visible, 4);
        // 50+8+50 = 108, plus 8+30 for the indicator = 146 <= 150.
        assert_eq!(compute(&fit(&widths, 150., 30.)).visible, 2);
        assert_eq!(compute(&fit(&widths, 100., 30.)).visible, 1);
    }

    #[test]
    fn the_floor_wins_over_the_cap() {
        let widths = [50., 50., 50., 50.];
        let mut input = fit(&widths, 10., 30.);
        input.min_visible = 2;
        assert_eq!(compute(&input).visible, 2, "at least the floor");
        input.max_visible = Some(1);
        assert_eq!(compute(&input).visible, 2, "the floor beats a smaller cap");
        let mut capped = fit(&widths, 1000., 30.);
        capped.max_visible = Some(3);
        assert_eq!(compute(&capped).visible, 3);
    }

    #[test]
    fn rows_pack_up_to_the_most() {
        let widths = [60., 60., 60., 60., 60.];
        let mut input = fit(&widths, 130., 20.);
        input.max_rows = Some(2);
        let Fit { visible, rows } = compute(&input);
        // Two per row; the last row keeps room for the indicator.
        assert_eq!((visible, rows), (3, 2));
        assert_eq!(count_rows(&widths, 8., 130.), 3);
    }

    #[test]
    fn inline_bounds_fold_and_unfold() {
        let widths = [40., 40., 40.];
        let (collapsed, rendered) = inline_bounds(&widths, 20., 8., 1, CollapseFrom::End, None);
        assert_eq!(collapsed, 68., "one item, a gap and the indicator");
        assert_eq!(rendered, 136., "everything");
        let (_, capped) = inline_bounds(&widths, 20., 8., 0, CollapseFrom::End, Some(2));
        assert_eq!(capped, 116., "two items and the indicator");
    }
}
