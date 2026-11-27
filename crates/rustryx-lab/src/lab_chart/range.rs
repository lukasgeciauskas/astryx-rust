//! `useChartRange` (upstream lab `useChartRange.ts`): tracks a stream's x and y ranges and gives
//! the chart its `x_domain` and `y_domain`, updated with every push.

use gpui::{App, ElementId, Entity, Window};

use super::stream_gl::ChartStreamGlHandle;

/// What the range tracks (upstream `UseChartRangeOptions`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UseChartRangeOptions {
    /// Upstream `xWindow`: how much x the window shows once it slides.
    pub x_window: f64,
    /// Upstream `yDomain`: a fixed y domain, which turns y tracking off.
    pub y_domain: Option<[f64; 2]>,
    /// Upstream `yPadding`: the share of the y range added above and below (0.1).
    pub y_padding: f64,
    /// Upstream `yCenter`: keeps the y domain symmetric around zero (off).
    pub y_center: bool,
}

impl UseChartRangeOptions {
    /// A window of `x_window` with y tracked, padded 10%.
    pub fn new(x_window: f64) -> Self {
        UseChartRangeOptions {
            x_window,
            y_domain: None,
            y_padding: 0.1,
            y_center: false,
        }
    }
}

/// The ranges' state (upstream's `xDomain` and `yDomain` states and `rangeRef`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartRangeState {
    options: UseChartRangeOptions,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    y_min: f64,
    y_max: f64,
}

impl ChartRangeState {
    /// The ranges before any push: x `[0, 0]`, y the fixed domain or `[0, 1]`.
    pub fn new(options: UseChartRangeOptions) -> Self {
        let (y_min, y_max) = options
            .y_domain
            .map_or((f64::INFINITY, f64::NEG_INFINITY), |[a, b]| (a, b));
        ChartRangeState {
            options,
            x_domain: [0., 0.],
            y_domain: options.y_domain.unwrap_or([0., 1.]),
            y_min,
            y_max,
        }
    }

    /// The x domain to give the chart (upstream `xDomain`).
    pub fn x_domain(&self) -> [f64; 2] {
        self.x_domain
    }

    /// The y domain to give the chart (upstream `yDomain`: the fixed one when given).
    pub fn y_domain(&self) -> [f64; 2] {
        self.options.y_domain.unwrap_or(self.y_domain)
    }

    /// Upstream `push`'s tracking: x grows from `[0, x]` until it is a window wide, then
    /// slides; y widens to the newest extreme, padded (and centered on zero).
    pub fn push(&mut self, x: f64, y: f64) {
        let w = self.options.x_window;
        self.x_domain = if x >= w { [x - w, x] } else { [0., x.max(1.)] };
        if self.options.y_domain.is_some() {
            return;
        }
        let mut changed = false;
        if y < self.y_min {
            self.y_min = y;
            changed = true;
        }
        if y > self.y_max {
            self.y_max = y;
            changed = true;
        }
        if changed {
            let span = self.y_max - self.y_min;
            let range = if span == 0. || span.is_nan() {
                1.
            } else {
                span
            };
            let pad = range * self.options.y_padding;
            let (mut min, mut max) = (self.y_min - pad, self.y_max + pad);
            if self.options.y_center {
                let abs = min.abs().max(max.abs());
                min = -abs;
                max = abs;
            }
            self.y_domain = [min, max];
        }
    }

    /// Upstream `reset`.
    pub fn reset(&mut self) {
        *self = ChartRangeState::new(self.options);
    }
}

/// The range a component keeps across frames (upstream `useChartRange`'s `push` and `reset`).
#[derive(Clone)]
pub struct ChartRange(Entity<ChartRangeState>);

impl ChartRange {
    /// Upstream `push(x, y, streamRef)`: forwards the point to the stream, then updates the
    /// domains and redraws what reads them.
    pub fn push(&self, x: f64, y: f64, stream: Option<&ChartStreamGlHandle>, cx: &mut App) {
        if let Some(stream) = stream {
            stream.push(x, y);
        }
        self.0.update(cx, |s, cx| {
            s.push(x, y);
            cx.notify();
        });
    }

    /// Upstream `reset`.
    pub fn reset(&self, cx: &mut App) {
        self.0.update(cx, |s, cx| {
            s.reset();
            cx.notify();
        });
    }

    /// The current ranges.
    pub fn state(&self, cx: &App) -> ChartRangeState {
        *self.0.read(cx)
    }
}

/// What [`use_chart_range`] returns (upstream `UseChartRangeReturn`).
#[derive(Clone)]
pub struct UseChartRangeReturn {
    /// Upstream `xDomain`.
    pub x_domain: [f64; 2],
    /// Upstream `yDomain`.
    pub y_domain: [f64; 2],
    /// Upstream `push` and `reset`.
    pub range: ChartRange,
}

/// Tracks a stream's ranges for the chart (upstream `useChartRange`), kept under `id`. The
/// options apply from the next push; the tracked y extremes start from the first fixed
/// domain until a reset, as upstream's ref keeps its first value.
pub fn use_chart_range(
    id: impl Into<ElementId>,
    options: UseChartRangeOptions,
    window: &mut Window,
    cx: &mut App,
) -> UseChartRangeReturn {
    let state = window.use_keyed_state(id, cx, move |_, _| ChartRangeState::new(options));
    state.update(cx, |s, _| s.options = options);
    let s = *state.read(cx);
    UseChartRangeReturn {
        x_domain: s.x_domain(),
        y_domain: s.y_domain(),
        range: ChartRange(state),
    }
}
