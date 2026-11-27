//! d3-shape's stack generator, with the order and offset upstream uses.

/// Stacks `keys` series over `rows` rows (d3-shape `stack()` with `stackOrderNone`
/// and `stackOffsetNone`, its defaults and what upstream uses): one series per key,
/// in key order, each a `[y0, y1]` per row. `value(row, key)` is the row's value for
/// the key, as d3's `+value(d, key)` reads it. Each series starts where the one below
/// ends, or where that one starts when its value is NaN.
pub fn stack(rows: usize, keys: usize, value: impl Fn(usize, usize) -> f64) -> Vec<Vec<[f64; 2]>> {
    let mut series: Vec<Vec<[f64; 2]>> = (0..keys).map(|_| Vec::with_capacity(rows)).collect();
    for row in 0..rows {
        for (key, layer) in series.iter_mut().enumerate() {
            layer.push([0., value(row, key)]);
        }
    }
    let order = order_none(keys);
    offset_none(&mut series, &order);
    series
}

/// The series in their given order (d3-shape `stackOrderNone`).
pub fn order_none(series: usize) -> Vec<usize> {
    (0..series).collect()
}

/// Stacks each series on the one before it in `order`, from a zero baseline (d3-shape
/// `stackOffsetNone`). A NaN top passes the baseline below it on.
pub fn offset_none(series: &mut [Vec<[f64; 2]>], order: &[usize]) {
    let n = series.len();
    if n <= 1 {
        return;
    }
    let m = series[order[0]].len();
    for i in 1..n {
        let (below, above) = (order[i - 1], order[i]);
        for j in 0..m {
            let [y0, y1] = series[below][j];
            let base = if y1.is_nan() { y0 } else { y1 };
            series[above][j][0] = base;
            series[above][j][1] += base;
        }
    }
}
