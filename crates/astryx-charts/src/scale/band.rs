//! d3-scale's `scaleBand`.

use std::collections::HashMap;

use astryx_theme::authoring::js::{js_max, js_min, js_round};
use gpui::SharedString;

/// A band scale (d3-scale `scaleBand`): splits a continuous range into one band per
/// domain value, with inner and outer padding, built up the way d3's chained setters
/// are: `BandScale::new().domain(categories).range(0., width).padding(0.2)`.
/// [`Clone`] is d3's `copy`.
#[derive(Clone, Debug, PartialEq)]
pub struct BandScale {
    domain: Vec<SharedString>,
    /// Each domain value's position (d3's `InternMap`).
    index: HashMap<SharedString, usize>,
    r0: f64,
    r1: f64,
    round: bool,
    padding_inner: f64,
    padding_outer: f64,
    align: f64,
    step: f64,
    bandwidth: f64,
    /// Each band's start, in domain order (the ordinal scale's range).
    values: Vec<f64>,
}

impl Default for BandScale {
    fn default() -> Self {
        Self::new()
    }
}

impl BandScale {
    /// A scale with an empty domain, the range [0, 1], no padding or rounding and the
    /// bands centred (d3-scale `scaleBand()`).
    pub fn new() -> Self {
        let mut scale = Self {
            domain: Vec::new(),
            index: HashMap::new(),
            r0: 0.,
            r1: 1.,
            round: false,
            padding_inner: 0.,
            padding_outer: 0.,
            align: 0.5,
            step: 0.,
            bandwidth: 0.,
            values: Vec::new(),
        };
        scale.rescale();
        scale
    }

    /// Sets the domain (d3-scale `band.domain(values)`). A repeated value keeps its
    /// first position, as d3's `InternMap` does.
    pub fn domain<S: Into<SharedString>>(mut self, values: impl IntoIterator<Item = S>) -> Self {
        self.domain.clear();
        self.index.clear();
        for value in values {
            let value = value.into();
            if self.index.contains_key(&value) {
                continue;
            }
            self.index.insert(value.clone(), self.domain.len());
            self.domain.push(value);
        }
        self.rescale();
        self
    }

    /// Sets the range (d3-scale `band.range([r0, r1])`). A descending range lays the
    /// bands out from its high end, in domain order.
    pub fn range(mut self, r0: f64, r1: f64) -> Self {
        self.r0 = r0;
        self.r1 = r1;
        self.rescale();
        self
    }

    /// Whether the step and the band starts and widths are whole numbers (d3-scale
    /// `band.round(round)`).
    pub fn round(mut self, round: bool) -> Self {
        self.round = round;
        self.rescale();
        self
    }

    /// Sets both paddings (d3-scale `band.padding(padding)`): the outer padding to
    /// `padding` and the inner to it at most 1.
    pub fn padding(mut self, padding: f64) -> Self {
        self.padding_outer = padding;
        self.padding_inner = js_min(1., padding);
        self.rescale();
        self
    }

    /// The share of each step left empty between bands, at most 1 (d3-scale
    /// `band.paddingInner(padding)`).
    pub fn padding_inner(mut self, padding: f64) -> Self {
        self.padding_inner = js_min(1., padding);
        self.rescale();
        self
    }

    /// The space before the first band and after the last, in steps (d3-scale
    /// `band.paddingOuter(padding)`).
    pub fn padding_outer(mut self, padding: f64) -> Self {
        self.padding_outer = padding;
        self.rescale();
        self
    }

    /// Where the bands sit in leftover space, from 0 (the start) to 1 (the end); 0.5
    /// centres them (d3-scale `band.align(align)`).
    pub fn align(mut self, align: f64) -> Self {
        self.align = js_max(0., js_min(1., align));
        self.rescale();
        self
    }

    /// The domain, without repeats (d3-scale `band.domain()`).
    pub fn get_domain(&self) -> &[SharedString] {
        &self.domain
    }

    /// The range (d3-scale `band.range()`).
    pub fn get_range(&self) -> [f64; 2] {
        [self.r0, self.r1]
    }

    /// Whether the scale rounds (d3-scale `band.round()`).
    pub fn is_round(&self) -> bool {
        self.round
    }

    /// The inner padding (d3-scale `band.paddingInner()`, also `band.padding()`).
    pub fn get_padding_inner(&self) -> f64 {
        self.padding_inner
    }

    /// The outer padding (d3-scale `band.paddingOuter()`).
    pub fn get_padding_outer(&self) -> f64 {
        self.padding_outer
    }

    /// The alignment (d3-scale `band.align()`).
    pub fn get_align(&self) -> f64 {
        self.align
    }

    /// The width of each band (d3-scale `band.bandwidth()`).
    pub fn bandwidth(&self) -> f64 {
        self.bandwidth
    }

    /// The distance between the starts of adjacent bands (d3-scale `band.step()`).
    pub fn step(&self) -> f64 {
        self.step
    }

    /// The start of the band for `value` (d3-scale `band(value)`), or `None` for a
    /// value not in the domain (d3's `undefined`).
    pub fn scale(&self, value: &str) -> Option<f64> {
        let i = *self.index.get(value)?;
        self.values.get(i).copied()
    }

    /// Recomputes the step, the bandwidth and the band starts (d3-scale's internal
    /// `rescale`).
    fn rescale(&mut self) {
        let n = self.domain.len() as f64;
        let reverse = self.r1 < self.r0;
        let (mut start, stop) = if reverse {
            (self.r1, self.r0)
        } else {
            (self.r0, self.r1)
        };
        let mut step =
            (stop - start) / js_max(1., n - self.padding_inner + self.padding_outer * 2.);
        if self.round {
            step = step.floor();
        }
        start += (stop - start - step * (n - self.padding_inner)) * self.align;
        let mut bandwidth = step * (1. - self.padding_inner);
        if self.round {
            start = js_round(start);
            bandwidth = js_round(bandwidth);
        }
        self.step = step;
        self.bandwidth = bandwidth;
        self.values = (0..self.domain.len())
            .map(|i| start + step * i as f64)
            .collect();
        if reverse {
            self.values.reverse();
        }
    }
}
