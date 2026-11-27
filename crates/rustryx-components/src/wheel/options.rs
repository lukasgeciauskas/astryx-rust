//! A wheel's rows and the rules both wheels share, upstream
//! `DateInput/wheelOptions.ts`: where a rest, a key or a spin lands.

use gpui::SharedString;

/// The row pitch, px (`dateInputTouchSizes.wheelItemSize`).
pub(crate) const ROW_HEIGHT: f64 = 28.;
/// A day row's height and touch target, px (`daySize`).
pub(crate) const DAY_SIZE: f64 = 44.;
/// Week rows per month pane (`WEEKS_PER_PANE`).
pub(crate) const WEEKS_PER_PANE: f64 = 6.;
/// One month pane, the scrollport and the wheel body, px.
pub(crate) const PANE_HEIGHT: f64 = WEEKS_PER_PANE * DAY_SIZE;
/// Rows a Page key moves.
pub(crate) const PAGE_ROWS: i64 = 10;

/// One row of a wheel, upstream `WheelOption`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WheelOption {
    /// The row's identity: a month, a year, an hour.
    pub value: i64,
    pub label: SharedString,
    /// Outside the bounds: shown, never committed.
    pub disabled: bool,
    /// Where a key steps the row; list order without it.
    pub order: Option<i64>,
}

impl WheelOption {
    pub fn new(value: i64, label: impl Into<SharedString>) -> Self {
        Self {
            value,
            label: label.into(),
            disabled: false,
            order: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn order(mut self, order: i64) -> Self {
        self.order = Some(order);
        self
    }
}

/// `a` modulo `count`, never negative (`mod` in `hooks/usePager.ts`).
pub(crate) fn modulo(value: i64, count: i64) -> i64 {
    value.rem_euclid(count)
}

/// The copy of row `index` nearest position `from` in a loop of `count`
/// rows (`nearestLoopCopy`): `index` itself on a tie it is part of.
pub(crate) fn nearest_loop_copy(index: i64, from: f64, count: i64) -> i64 {
    if (index as f64 - from).abs() * 2. <= count as f64 {
        return index;
    }
    let count_f = count as f64;
    let shift = ((modulo(index, count) as f64 - from.rem_euclid(count_f) + count_f + count_f / 2.)
        % count_f)
        - count_f / 2.;
    (from + shift).round() as i64
}

/// Whether a spin may turn onto `index` (`canSpinTo`).
pub(crate) fn can_spin_to(options: &[WheelOption], index: usize) -> bool {
    options.get(index).is_some_and(|o| !o.disabled)
}

/// The enabled row nearest `index`, a tie going toward `toward`
/// (`nearestEnabledIndex`); `toward` when none is enabled.
pub(crate) fn nearest_enabled_index(options: &[WheelOption], index: i64, toward: i64) -> i64 {
    let enabled = |i: i64| i >= 0 && (i as usize) < options.len() && !options[i as usize].disabled;
    if enabled(index) {
        return index;
    }
    for distance in 1..options.len() as i64 {
        let (below, above) = (index - distance, index + distance);
        match (enabled(below), enabled(above)) {
            (true, true) => return if toward < index { below } else { above },
            (true, false) => return below,
            (false, true) => return above,
            _ => {}
        }
    }
    toward
}

/// How many rows a looping wheel resting at `position` travels to the
/// nearest enabled row (`restingLoopStep`), within half a turn.
pub(crate) fn resting_loop_step(
    enabled_at: impl Fn(i64) -> bool,
    position: i64,
    toward: i64,
    count: i64,
) -> i64 {
    if enabled_at(position) {
        return 0;
    }
    let mut distance = 1;
    while distance * 2 <= count {
        match (
            enabled_at(position - distance),
            enabled_at(position + distance),
        ) {
            (true, true) => {
                return if toward < position {
                    -distance
                } else {
                    distance
                };
            }
            (true, false) => return -distance,
            (false, true) => return distance,
            _ => {}
        }
        distance += 1;
    }
    toward - position
}

fn in_key_order(options: &[WheelOption]) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..options.len()).collect();
    if options.iter().any(|o| o.order.is_some()) {
        indices.sort_by_key(|&i| options[i].order.unwrap_or(i as i64));
    }
    indices
}

/// The row a key moves a wheel to from `from` by `delta` rows, in key
/// order (`steppedIndex`); `None` when no enabled row lies that way.
pub(crate) fn stepped_index(options: &[WheelOption], from: usize, delta: i64) -> Option<usize> {
    let count = options.len() as i64;
    if count == 0 || delta == 0 {
        return None;
    }
    let rows = in_key_order(options);
    let start = rows.iter().position(|&r| r == from).unwrap_or(0) as i64;
    let enabled = |p: i64| !options[rows[p as usize]].disabled;
    let step = delta.signum();
    let target = (start + delta).clamp(0, count - 1);
    let mut position = target;
    while (0..count).contains(&position) {
        if enabled(position) {
            return (position != start).then(|| rows[position as usize]);
        }
        position += step;
    }
    let mut position = target;
    while position != start {
        if enabled(position) {
            return Some(rows[position as usize]);
        }
        position -= step;
    }
    None
}

/// How many rows a key moves a looping wheel of `count` rows from `from`
/// by `delta` (`loopedStep`): never a whole lap, at most half a turn.
pub(crate) fn looped_step(
    enabled_at: impl Fn(i64) -> bool,
    from: i64,
    delta: i64,
    count: i64,
) -> Option<i64> {
    let step = delta.signum();
    let rows = if count < 2 { 0 } else { delta.abs() % count };
    if rows == 0 {
        return None;
    }
    let target = from + step * rows;
    let reach = rows.max(count / 2);
    let mut position = target;
    while (position - from).abs() <= reach {
        if enabled_at(position) {
            return Some(position - from);
        }
        position += step;
    }
    let mut position = target - step;
    while position != from {
        if enabled_at(position) {
            return Some(position - from);
        }
        position -= step;
    }
    None
}

/// The first or last enabled row in key order (`endIndex`).
pub(crate) fn end_index(options: &[WheelOption], last: bool) -> Option<usize> {
    let mut enabled = in_key_order(options)
        .into_iter()
        .filter(|&i| !options[i].disabled);
    if last {
        enabled.next_back()
    } else {
        enabled.next()
    }
}
