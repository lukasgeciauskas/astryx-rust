//! `ChartStreamGL` (upstream lab `ChartStreamGL.tsx`): a streaming line kept in a ring buffer,
//! pushed through a handle and drawn with the chart's current scales, above its other
//! children.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use astryx_charts::shape::{Line, PathStyle};
use gpui::prelude::*;
use gpui::{AnyWindowHandle, App, AsyncApp, Hsla, Window, div};

use super::context::use_chart;
use super::paint::paint_path;
use super::webgl::{canvas_dpr, hex_to_gl, premultiplied};

/// The ring buffer (upstream's `ring` ref): x and y pairs as 32-bit floats, the next write
/// at `head`, the newest `count` valid.
#[derive(Debug, PartialEq)]
pub struct StreamRing {
    data: Vec<f32>,
    head: usize,
    count: usize,
    capacity: usize,
}

impl StreamRing {
    /// An empty ring of `capacity` points.
    pub fn new(capacity: usize) -> Self {
        StreamRing {
            data: vec![0.; capacity * 2],
            head: 0,
            count: 0,
            capacity,
        }
    }

    /// Upstream `push`'s write: the point at the head, the head moving on.
    pub fn push(&mut self, x: f64, y: f64) {
        if self.capacity == 0 {
            return;
        }
        let idx = self.head * 2;
        self.data[idx] = x as f32;
        self.data[idx + 1] = y as f32;
        self.head = (self.head + 1) % self.capacity;
        self.count = (self.count + 1).min(self.capacity);
    }

    /// Upstream `clear`.
    pub fn clear(&mut self) {
        self.head = 0;
        self.count = 0;
    }

    /// The points, oldest first (upstream's `drawBuf` order).
    pub fn points(&self) -> Vec<(f64, f64)> {
        (0..self.count)
            .map(|i| {
                let idx = ((self.head + self.capacity - self.count + i) % self.capacity) * 2;
                (f64::from(self.data[idx]), f64::from(self.data[idx + 1]))
            })
            .collect()
    }

    /// How many points it holds.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

#[derive(Default)]
struct Shared {
    /// The ring, made by the first mark that draws the stream (upstream's handle exists
    /// only once the mark is mounted).
    ring: RefCell<Option<StreamRing>>,
    windows: RefCell<Vec<AnyWindowHandle>>,
    app: RefCell<Option<AsyncApp>>,
    scheduled: Cell<bool>,
}

/// The push and clear handle of a streaming line (upstream `ChartStreamGLHandle`, handed out
/// through `handleRef`). Pass a clone to [`ChartStreamGl::handle`] and push from anywhere on
/// the main thread; pushes before the line first draws are dropped, as upstream's ref is
/// empty until it mounts.
#[derive(Clone, Default)]
pub struct ChartStreamGlHandle(Rc<Shared>);

impl ChartStreamGlHandle {
    /// A handle for one stream.
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `push(x, y)`: adds a point and redraws.
    pub fn push(&self, x: f64, y: f64) {
        if let Some(ring) = self.0.ring.borrow_mut().as_mut() {
            ring.push(x, y);
        } else {
            return;
        }
        self.redraw();
    }

    /// Upstream `clear()`: drops every point and clears the line.
    pub fn clear(&self) {
        if let Some(ring) = self.0.ring.borrow_mut().as_mut() {
            ring.clear();
        }
        self.redraw();
    }

    /// How many points the stream holds.
    pub fn len(&self) -> usize {
        self.0.ring.borrow().as_ref().map_or(0, StreamRing::len)
    }

    /// Whether the stream holds no points.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The points, oldest first.
    pub fn points(&self) -> Vec<(f64, f64)> {
        self.0
            .ring
            .borrow()
            .as_ref()
            .map_or_else(Vec::new, StreamRing::points)
    }

    fn attach(&self, capacity: usize, window: &Window, cx: &App) {
        if self.0.ring.borrow().is_none() {
            *self.0.ring.borrow_mut() = Some(StreamRing::new(capacity));
        }
        let handle = window.window_handle();
        let mut windows = self.0.windows.borrow_mut();
        if !windows.contains(&handle) {
            windows.push(handle);
        }
        if self.0.app.borrow().is_none() {
            *self.0.app.borrow_mut() = Some(cx.to_async());
        }
    }

    /// One redraw of every window that drew the stream, coalescing pushes in a frame.
    fn redraw(&self) {
        if self.0.scheduled.replace(true) {
            return;
        }
        let Some(app) = self.0.app.borrow().clone() else {
            self.0.scheduled.set(false);
            return;
        };
        let shared = self.0.clone();
        let executor = app.foreground_executor().clone();
        executor
            .spawn(async move {
                shared.scheduled.set(false);
                let windows = shared.windows.borrow().clone();
                app.update(|cx| {
                    for w in windows {
                        let _ = w.update(cx, |_, window, _| window.refresh());
                    }
                });
            })
            .detach();
    }
}

/// A streaming line (upstream lab `ChartStreamGL`): the newest `buffer_size` points pushed to its
/// handle, joined in push order on the chart's scales.
#[derive(IntoElement)]
pub struct ChartStreamGl {
    handle: Option<ChartStreamGlHandle>,
    color: Hsla,
    buffer_size: usize,
    line_width: f64,
    opacity: f64,
}

impl ChartStreamGl {
    /// Upstream `color`.
    pub fn new(color: Hsla) -> Self {
        ChartStreamGl {
            handle: None,
            color,
            buffer_size: 500,
            line_width: 2.,
            opacity: 1.,
        }
    }

    /// Upstream `handleRef`: the handle that pushes points (without one the line has none).
    pub fn handle(mut self, handle: ChartStreamGlHandle) -> Self {
        self.handle = Some(handle);
        self
    }

    /// Upstream `bufferSize`: how many of the newest points it keeps (500).
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Upstream `lineWidth` (2), in pixels of upstream's supersampled canvas.
    pub fn line_width(mut self, width: f64) -> Self {
        self.line_width = width;
        self
    }

    /// Upstream `opacity` (1).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }
}

impl RenderOnce for ChartStreamGl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div();
        };
        if ctx.width <= 0. || ctx.height <= 0. {
            return div();
        }
        let Some(handle) = self.handle else {
            return div();
        };
        handle.attach(self.buffer_size, window, cx);
        let points = handle.points();
        if points.len() < 2 {
            return div();
        }
        let (x_scale, y_scale) = (ctx.x_scale.clone(), ctx.y_scale);
        let pixels: Vec<(f64, f64)> = points
            .iter()
            .map(|(x, y)| {
                let px = match &x_scale {
                    astryx_charts::ChartScale::Linear(l) => l.scale(*x),
                    astryx_charts::ChartScale::Band(_) => f64::NAN,
                };
                (f64::from(px as f32), f64::from(y_scale.scale(*y) as f32))
            })
            .collect();
        let path = Line::new()
            .x(|p: &(f64, f64), _| p.0)
            .y(|p: &(f64, f64), _| p.1)
            .generate(&pixels);
        let color = premultiplied(hex_to_gl(self.color), self.opacity);
        let line_width = self.line_width;
        ctx.paint_over(move |origin, window, _| {
            let Some(color) = color else {
                return;
            };
            let width = (line_width / canvas_dpr(window.scale_factor())) as f32;
            paint_path(window, origin, &path, PathStyle::Stroke { width }, color);
        });
        div()
    }
}
