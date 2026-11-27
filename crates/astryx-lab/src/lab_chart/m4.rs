//! `m4Reduce` (upstream lab `m4.ts`): pixel-aware reduction of a time series, the M4
//! algorithm of Jugel et al. (VLDB 2014). Each pixel column keeps its first, minimum,
//! maximum and last point, the fewest that draw the same line at that width.

/// A point of a time series (upstream `M4Point`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M4Point {
    pub x: f64,
    pub y: f64,
}

impl M4Point {
    /// A point at (`x`, `y`).
    pub fn new(x: f64, y: f64) -> Self {
        M4Point { x, y }
    }
}

/// Reduces `data` (sorted by x) to at most 4 points per pixel column of a `width`-pixel chart
/// (upstream `m4Reduce`). Data that already fits (`4 × width` points or fewer) comes back as it
/// is; `x_domain` sets the columns' span (the data's first and last x by default). An x span
/// that is not positive keeps the first point only. A width that is not a number reduces
/// to nothing (upstream throws reading a bucket that does not exist).
pub fn m4_reduce(data: &[M4Point], width: f64, x_domain: Option<[f64; 2]>) -> Vec<M4Point> {
    if data.is_empty() || width <= 0. {
        return Vec::new();
    }
    if data.len() as f64 <= width * 4. {
        return data.to_vec();
    }
    if width.is_nan() {
        return Vec::new();
    }
    let (x_min, x_max) = match x_domain {
        Some([a, b]) => (a, b),
        None => (data[0].x, data[data.len() - 1].x),
    };
    let x_range = x_max - x_min;
    if x_range.is_nan() || x_range <= 0. {
        return vec![data[0]];
    }
    let bucket_count = width.ceil() as usize;
    // Each bucket's first, last, minimum and maximum, by index into the data.
    let mut buckets: Vec<Option<[usize; 4]>> = vec![None; bucket_count];
    for (i, p) in data.iter().enumerate() {
        let raw = (((p.x - x_min) / x_range) * bucket_count as f64).floor();
        let idx = raw.min(bucket_count as f64 - 1.);
        // A point left of the domain lands on a bucket that does not exist upstream, which
        // throws; it is dropped here.
        if idx.is_nan() || idx < 0. {
            continue;
        }
        let bucket = &mut buckets[idx as usize];
        match bucket {
            None => *bucket = Some([i, i, i, i]),
            Some([_, last, min, max]) => {
                *last = i;
                if p.y < data[*min].y {
                    *min = i;
                }
                if p.y > data[*max].y {
                    *max = i;
                }
            }
        }
    }
    let mut result = Vec::new();
    for [first, last, min, max] in buckets.into_iter().flatten() {
        let mut unique: Vec<usize> = Vec::with_capacity(4);
        for i in [first, min, max, last] {
            if !unique.contains(&i) {
                unique.push(i);
            }
        }
        // Stable, by x (upstream's `sort((a, b) => a.x - b.x)`).
        unique.sort_by(|a, b| {
            let d = data[*a].x - data[*b].x;
            if d < 0. {
                std::cmp::Ordering::Less
            } else if d > 0. {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        result.extend(unique.into_iter().map(|i| data[i]));
    }
    result
}
