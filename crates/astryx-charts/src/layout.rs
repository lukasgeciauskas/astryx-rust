//! The chart's one layout pass (upstream `layout.ts`): the scales, stacking and bar
//! grouping every series shares, then each series' own `resolve`. It knows no mark
//! types (bar grouping reads the `"bar"` kind, as upstream's does).

use std::collections::HashMap;
use std::rc::Rc;

use gpui::SharedString;

use crate::Datum;
use crate::scale::{BandScale, LinearScale};
use crate::shape::stack;
use crate::types::{
    ChartScale, GroupInfo, ResolvedPositions, SeriesContext, SeriesDef, SeriesInfo, StackOffset,
    YBaseline,
};

/// The share of the data's range a continuous-only chart (no zero baseline) adds
/// above and below, so ends and curve overshoot keep off the plot's edges.
const CONTINUOUS_HEADROOM: f64 = 0.08;

/// What the layout pass reads (upstream `LayoutInput`).
pub struct LayoutInput<'a> {
    pub data: &'a Rc<[Datum]>,
    pub x_key: &'a SharedString,
    pub series: &'a [SeriesDef],
    pub width: f64,
    pub height: f64,
    /// How the y domain is derived without `y_domain`.
    pub y_baseline: Option<YBaseline>,
    /// An exact y domain: no baseline, headroom or rounding.
    pub y_domain: Option<[f64; 2]>,
    /// An exact x domain for numeric x, kept even with no data (a steady streaming
    /// window); a band scale ignores it.
    pub x_domain: Option<[f64; 2]>,
}

/// What the layout pass gives (upstream `LayoutResult`), with each series' info.
#[derive(Clone, Debug)]
pub struct LayoutResult {
    pub x_scale: ChartScale,
    pub y_scale: LinearScale,
    /// The categorical y scale when a series sets `y_band_key`.
    pub y_band_scale: Option<BandScale>,
    /// Each series' points by uid, in series order.
    pub resolved: ResolvedPositions,
    /// Each series' uid, stack position and group slot, in series order (its palette
    /// color is the chart root's to add).
    pub info: Vec<SeriesInfo>,
}

/// A JavaScript-truthy string option: set and not empty.
fn truthy(s: &Option<SharedString>) -> Option<&SharedString> {
    s.as_ref().filter(|s| !s.is_empty())
}

fn is_finite_domain(domain: [f64; 2]) -> bool {
    domain[0].is_finite() && domain[1].is_finite()
}

/// The finite `[min, max]` of numbers, `[0, 1]` when none is finite, widened when
/// all are equal so the scale never collapses onto one pixel.
fn finite_extent(nums: impl Iterator<Item = f64>) -> [f64; 2] {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in nums.filter(|v| v.is_finite()) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if !lo.is_finite() || !hi.is_finite() {
        return [0., 1.];
    }
    if lo == hi {
        let pad = or_one(lo.abs() * 0.5);
        return [lo - pad, hi + pad];
    }
    [lo, hi]
}

/// JavaScript's `x || 1` for a number.
fn or_one(x: f64) -> f64 {
    if x == 0. || x.is_nan() { 1. } else { x }
}

/// Runs the layout pass (upstream `computeLayout`).
pub fn compute_layout(input: LayoutInput<'_>) -> LayoutResult {
    let LayoutInput {
        data,
        x_key,
        series,
        width,
        height,
        y_baseline,
        y_domain,
        x_domain,
    } = input;
    let layouts: Vec<_> = series.iter().map(|s| s.layout()).collect();
    let data_keys: Vec<Vec<SharedString>> = series.iter().map(|s| s.data_keys()).collect();

    // A categorical y axis (a heatmap) makes both axes categorical: the columns tile
    // edge to edge like the rows, so the x band scale drops its outer padding.
    let y_band_key = layouts.iter().find_map(|l| l.y_band_key.clone());
    let is_categorical_grid = y_band_key.is_some();

    // 1. The x scale.
    let x_values: Vec<_> = data.iter().map(|d| d.get(x_key)).collect();
    let is_numeric_x =
        !x_values.is_empty() && x_values.iter().all(|v| v.is_some_and(|v| v.is_number()));
    // Strings that are not numbers are categories; dates and numeric strings still
    // place on a linear scale.
    let is_categorical_x = x_values.iter().any(|v| {
        matches!(v, Some(crate::ChartValue::Text(s)) if crate::ChartValue::Text(s.clone()).to_number().is_nan())
    });
    // Marks are clipped to the plot, so a dot at the first or last x would lose its
    // outer half: inset the linear range by the widest overhang (the domain stays).
    let widest = layouts.iter().fold(0f64, |max, l| match l.x_overhang {
        Some(o) if o.is_finite() && o > max => o,
        _ => max,
    });
    let x_inset = (width / 2.).min(widest);
    let x_range = [x_inset, width - x_inset];
    let x_scale = match x_domain {
        Some(domain) if is_finite_domain(domain) && !is_categorical_x => ChartScale::Linear(
            LinearScale::new()
                .domain(domain[0], domain[1])
                .range(x_range[0], x_range[1]),
        ),
        _ if is_numeric_x => {
            let [lo, hi] = finite_extent(
                x_values
                    .iter()
                    .map(|v| v.map_or(f64::NAN, |v| v.to_number())),
            );
            ChartScale::Linear(
                LinearScale::new()
                    .domain(lo, hi)
                    .range(x_range[0], x_range[1])
                    .nice(),
            )
        }
        _ => ChartScale::Band(
            BandScale::new()
                .domain(x_values.iter().map(|v| crate::js_string(*v)))
                .range(0., width)
                .padding(if is_categorical_grid { 0. } else { 0.2 }),
        ),
    };

    // 2. The y domain from every series' data keys, the zero baseline and the stacks.
    let mut all_keys: Vec<SharedString> = Vec::new();
    let mut include_zero = false;
    let mut stack_groups: Vec<(SharedString, Vec<SharedString>)> = Vec::new();
    for (layout, keys) in layouts.iter().zip(&data_keys) {
        for k in keys {
            if !all_keys.contains(k) {
                all_keys.push(k.clone());
            }
        }
        include_zero |= layout.include_zero;
        if let (Some(stack), Some(primary)) = (truthy(&layout.stack), keys.first()) {
            match stack_groups.iter_mut().find(|(name, _)| name == stack) {
                Some((_, group)) => group.push(primary.clone()),
                None => stack_groups.push((stack.clone(), vec![primary.clone()])),
            }
        }
    }

    let (mut y_min, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY);
    // A stack spans its running total's whole extent, every layer's bound and its zero
    // baseline, as stackOffsetNone draws it; a non-finite layer adds nothing.
    let mut stacked_keys: Vec<&SharedString> = Vec::new();
    for (_, keys) in &stack_groups {
        stacked_keys.extend(keys.iter());
        for d in data.iter() {
            let mut acc = 0.;
            y_min = y_min.min(acc);
            y_max = y_max.max(acc);
            for k in keys {
                if let Some(v) = d.finite(k) {
                    acc += v;
                    y_min = y_min.min(acc);
                    y_max = y_max.max(acc);
                }
            }
        }
    }
    // Other series add their own finite values.
    for d in data.iter() {
        for k in &all_keys {
            if stacked_keys.contains(&k) {
                continue;
            }
            if let Some(n) = d.finite(k) {
                y_min = y_min.min(n);
                y_max = y_max.max(n);
            }
        }
    }

    let (y_domain_final, apply_nice) = match y_domain {
        // The caller's exact domain (zoom, streaming): rounding it would ratchet.
        Some(domain) if is_finite_domain(domain) => (domain, false),
        _ => {
            if !y_min.is_finite() || !y_max.is_finite() {
                y_min = 0.;
                y_max = 1.;
            }
            if y_min == y_max {
                let pad = or_one(y_min.abs() * 0.5);
                y_min -= pad;
                y_max += pad;
            }
            match y_baseline.unwrap_or_default() {
                YBaseline::Zero => {
                    let abs = y_min.abs().max(y_max.abs());
                    y_min = -abs;
                    y_max = abs;
                }
                YBaseline::Data => {}
                YBaseline::Auto => {
                    if include_zero {
                        if y_min > 0. {
                            y_min = 0.;
                        }
                        if y_max < 0. {
                            y_max = 0.;
                        }
                    } else {
                        let pad = or_one(y_max - y_min) * CONTINUOUS_HEADROOM;
                        y_min -= pad;
                        y_max += pad;
                    }
                }
            }
            ([y_min, y_max], true)
        }
    };
    let y_scale = LinearScale::new()
        .domain(y_domain_final[0], y_domain_final[1])
        .range(height, 0.);
    let y_scale = if apply_nice { y_scale.nice() } else { y_scale };

    // 2b. The categorical y scale: one band per unique value, tiling the plot's height.
    let y_band_scale = y_band_key.map(|key| {
        let mut cats: Vec<SharedString> = Vec::new();
        for d in data.iter() {
            let c = d.string(&key);
            if !cats.contains(&c) {
                cats.push(c);
            }
        }
        BandScale::new().domain(cats).range(0., height).padding(0.)
    });

    // 3. Stacking (d3 `stack` with no reordering and a zero offset).
    let mut stacked_data: HashMap<SharedString, Vec<StackOffset>> = HashMap::new();
    for (_, keys) in &stack_groups {
        let layers = stack(data.len(), keys.len(), |row, key| {
            crate::js_number(data[row].get(&keys[key]))
        });
        for (key, layer) in keys.iter().zip(layers) {
            stacked_data.insert(
                key.clone(),
                layer
                    .into_iter()
                    .map(|[y0, y1]| StackOffset { y0, y1 })
                    .collect(),
            );
        }
    }

    // 4. Bar grouping: bars with a group share the band; a grouped stack takes one
    // slot by its stack name.
    let mut bar_groups: Vec<(SharedString, Vec<SharedString>)> = Vec::new();
    for (s, layout) in series.iter().zip(&layouts) {
        if s.kind() == "bar"
            && let Some(group) = truthy(&layout.group)
        {
            let slot = layout.stack.clone().unwrap_or_else(|| s.key());
            match bar_groups.iter_mut().find(|(name, _)| name == group) {
                Some((_, slots)) => {
                    if !slots.contains(&slot) {
                        slots.push(slot);
                    }
                }
                None => bar_groups.push((group.clone(), vec![slot])),
            }
        }
    }

    // 5. Each series resolves itself.
    let ctx = SeriesContext {
        data: data.clone(),
        x_key: x_key.clone(),
        x_scale: x_scale.clone(),
        y_scale,
        y_band_scale: y_band_scale.clone(),
        width,
        height,
    };
    // The last key of each stack draws on top.
    let stack_tops: Vec<&SharedString> = stack_groups
        .iter()
        .filter_map(|(_, keys)| keys.last())
        .collect();
    let mut resolved = Vec::with_capacity(series.len());
    let mut info = Vec::with_capacity(series.len());
    for (index, ((s, layout), keys)) in series.iter().zip(&layouts).zip(&data_keys).enumerate() {
        // An identity from the series' position: two series with one data key stay two.
        let uid: SharedString = format!("{index}:{}", s.key()).into();
        let stack_offsets = keys.first().and_then(|k| stacked_data.get(k));
        let is_top_of_stack = match truthy(&layout.stack) {
            Some(_) => keys.first().is_some_and(|k| stack_tops.contains(&k)),
            None => true,
        };
        // Only bars register slots, and only bars read them: another mark with a group
        // finds no slot of its own (upstream's index -1) and gets none.
        let group = truthy(&layout.group).and_then(|group| {
            let (_, slots) = bar_groups.iter().find(|(name, _)| name == group)?;
            let slot = layout.stack.clone().unwrap_or_else(|| s.key());
            let index = slots.iter().position(|k| *k == slot)?;
            Some(GroupInfo {
                index,
                count: slots.len(),
            })
        });
        let points = s.resolve(&ctx, stack_offsets.map(Vec::as_slice), group);
        resolved.push((uid.clone(), points));
        info.push(SeriesInfo {
            uid,
            is_top_of_stack,
            resolved_color: None,
            group,
        });
    }

    LayoutResult {
        x_scale,
        y_scale,
        y_band_scale,
        resolved,
        info,
    }
}
