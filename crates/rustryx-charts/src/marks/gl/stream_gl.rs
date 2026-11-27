//! `stream_gl`: a streaming line (upstream `marks/streamGL.tsx`). Points are pushed
//! through a [`StreamGlHandle`], not read from the chart's data, into a ring buffer,
//! and drawn with the chart's current scales each frame.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, AnyWindowHandle, App, AsyncApp, Bounds, Hsla, IntoElement, Pixels, SharedString,
    Window, canvas,
};

use super::{canvas_dpr, gl_rgb, premultiplied};
use crate::scale::LinearScale;
use crate::shape::{Line, PathStyle};
use crate::types::{
    ChartScale, GroupInfo, ResolvedPoint, Series, SeriesContext, SeriesDef, SeriesInfo, StackOffset,
};

/// Upstream's default `bufferSize`.
const DEFAULT_BUFFER_SIZE: usize = 500;

/// The ring buffer (upstream's `ringRef`): x and y pairs as the 32-bit floats of its
/// `Float32Array`, the next write at `head`, the newest `count` valid.
#[derive(Debug, PartialEq)]
struct Ring {
    data: Vec<f32>,
    head: usize,
    count: usize,
    capacity: usize,
}

impl Ring {
    fn new(capacity: usize) -> Self {
        Ring {
            data: vec![0.; capacity * 2],
            head: 0,
            count: 0,
            capacity,
        }
    }
}

/// What a handle shares with the marks that draw it.
struct Shared {
    ring: RefCell<Ring>,
    /// The windows the stream was last drawn in, which a push redraws.
    windows: RefCell<Vec<AnyWindowHandle>>,
    /// The app, once the stream has been drawn, to schedule redraws from anywhere.
    app: RefCell<Option<AsyncApp>>,
    /// A redraw is scheduled (upstream's pending `rafRef`).
    scheduled: Cell<bool>,
    /// How many redraws pushes and clears scheduled.
    #[cfg(test)]
    redraws: Cell<usize>,
    /// How many times the stream was painted.
    #[cfg(test)]
    paints: Cell<usize>,
}

/// The push and clear handle of a streaming line (upstream `StreamGLHandle`, which
/// upstream hands out through `handleRef`). Create one, pass a clone in
/// [`StreamGlOptions`], and push to it from anywhere on the main thread: pushes
/// coalesce into one redraw per frame of each window the stream is drawn in.
#[derive(Clone)]
pub struct StreamGlHandle(Rc<Shared>);

impl Default for StreamGlHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for StreamGlHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ring = self.0.ring.borrow();
        f.debug_struct("StreamGlHandle")
            .field("count", &ring.count)
            .field("capacity", &ring.capacity)
            .finish()
    }
}

impl StreamGlHandle {
    /// An empty stream of upstream's default 500 points; the mark's `buffer_size`
    /// resizes it.
    pub fn new() -> Self {
        StreamGlHandle(Rc::new(Shared {
            ring: RefCell::new(Ring::new(DEFAULT_BUFFER_SIZE)),
            windows: RefCell::new(Vec::new()),
            app: RefCell::new(None),
            scheduled: Cell::new(false),
            #[cfg(test)]
            redraws: Cell::new(0),
            #[cfg(test)]
            paints: Cell::new(0),
        }))
    }

    /// Appends a point, dropping the oldest when the buffer is full (upstream
    /// `push(x, y)`). A coordinate that is not finite is ignored. The buffer keeps
    /// 32-bit floats, as upstream's `Float32Array` does, so large values (a timestamp
    /// in milliseconds) lose precision.
    pub fn push(&self, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        {
            let mut ring = self.0.ring.borrow_mut();
            if ring.capacity == 0 {
                return;
            }
            let idx = ring.head * 2;
            ring.data[idx] = x as f32;
            ring.data[idx + 1] = y as f32;
            ring.head = (ring.head + 1) % ring.capacity;
            ring.count = (ring.count + 1).min(ring.capacity);
        }
        self.schedule_redraw();
    }

    /// Empties the stream (upstream `clear()`); the line disappears on the next frame.
    pub fn clear(&self) {
        {
            let mut ring = self.0.ring.borrow_mut();
            ring.head = 0;
            ring.count = 0;
        }
        self.schedule_redraw();
    }

    /// The buffered points, oldest first.
    pub(crate) fn points(&self) -> Vec<[f32; 2]> {
        let ring = self.0.ring.borrow();
        let (head, count, capacity) = (ring.head, ring.count, ring.capacity);
        (0..count)
            .map(|i| {
                let idx = ((head + capacity - count + i) % capacity) * 2;
                [ring.data[idx], ring.data[idx + 1]]
            })
            .collect()
    }

    /// Sizes the buffer, emptying it when the size changes (upstream's `bufferSize`
    /// effect).
    pub(crate) fn set_capacity(&self, capacity: usize) {
        let mut ring = self.0.ring.borrow_mut();
        if ring.capacity != capacity {
            *ring = Ring::new(capacity);
        }
    }

    /// Notes the window drawing the stream, so pushes redraw it.
    fn attach(&self, window: AnyWindowHandle, app: AsyncApp) {
        let mut windows = self.0.windows.borrow_mut();
        if !windows.contains(&window) {
            windows.push(window);
        }
        *self.0.app.borrow_mut() = Some(app);
    }

    /// Schedules one redraw of the stream's windows, unless one is pending (upstream
    /// `scheduleDraw`, coalescing onto one animation frame). Before the stream is
    /// first drawn there is nothing to redraw: its first frame shows the buffer.
    fn schedule_redraw(&self) {
        if self.0.scheduled.get() {
            return;
        }
        let Some(app) = self.0.app.borrow().clone() else {
            return;
        };
        self.0.scheduled.set(true);
        #[cfg(test)]
        self.0.redraws.set(self.0.redraws.get() + 1);
        let shared = self.0.clone();
        app.spawn(async move |cx: &mut AsyncApp| {
            shared.scheduled.set(false);
            let windows = shared.windows.borrow().clone();
            let mut open = Vec::with_capacity(windows.len());
            for window in windows {
                if cx
                    .update_window(window, |_, window, _| window.refresh())
                    .is_ok()
                {
                    open.push(window);
                }
            }
            *shared.windows.borrow_mut() = open;
        })
        .detach();
    }

    #[cfg(test)]
    pub(crate) fn redraws(&self) -> usize {
        self.0.redraws.get()
    }

    #[cfg(test)]
    pub(crate) fn paints(&self) -> usize {
        self.0.paints.get()
    }
}

/// A streaming line's options (upstream `StreamGLOptions`).
#[derive(Clone, Debug)]
pub struct StreamGlOptions {
    /// The line's color (upstream `color`, which is required).
    pub color: Hsla,
    /// How many of the newest points the line keeps (upstream `bufferSize`), 500 by
    /// default.
    pub buffer_size: Option<usize>,
    /// The line's width (upstream `lineWidth`), 2 by default, in pixels of
    /// upstream's supersampled canvas: GL widths are framebuffer pixels, and that
    /// canvas has `2 * devicePixelRatio` of them per pixel. (Browsers clamp GL lines to
    /// one framebuffer pixel; the width is what upstream asks for.)
    pub line_width: Option<f64>,
    /// The line's opacity (upstream `opacity`), 1 by default.
    pub opacity: Option<f64>,
    /// The push and clear handle (upstream `handleRef`); without one the line has no
    /// points.
    pub handle: Option<StreamGlHandle>,
}

impl StreamGlOptions {
    /// Options with `color` (upstream `{color}`).
    pub fn new(color: Hsla) -> Self {
        Self {
            color,
            buffer_size: None,
            line_width: None,
            opacity: None,
            handle: None,
        }
    }

    /// Sets how many points the line keeps (upstream `bufferSize`).
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = Some(size);
        self
    }

    /// Sets the line's width (upstream `lineWidth`).
    pub fn line_width(mut self, width: f64) -> Self {
        self.line_width = Some(width);
        self
    }

    /// Sets the line's opacity (upstream `opacity`).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Sets the push and clear handle (upstream `handleRef`).
    pub fn handle(mut self, handle: StreamGlHandle) -> Self {
        self.handle = Some(handle);
        self
    }
}

struct StreamGl {
    color: Hsla,
    buffer_size: usize,
    line_width: f64,
    opacity: f64,
    handle: StreamGlHandle,
}

/// A streaming line (upstream `streamGL`): the points pushed to its handle, newest
/// `buffer_size` kept, joined in push order on the chart's scales, drawn above the
/// chart's axes inside the plot. Give the chart an `x_domain` so its x scale is
/// linear (with no data a chart's x is categorical, which places no pushed point).
pub fn stream_gl(options: StreamGlOptions) -> SeriesDef {
    let buffer_size = options.buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
    let handle = options.handle.unwrap_or_default();
    handle.set_capacity(buffer_size);
    Rc::new(StreamGl {
        color: options.color,
        buffer_size,
        line_width: options.line_width.unwrap_or(2.),
        opacity: options.opacity.unwrap_or(1.),
        handle,
    })
}

/// The line's vertices (upstream `draw`'s `drawBuf`): each buffered point through the
/// chart's scales, as 32-bit floats. A band x scale places no number (d3's band scale
/// of a number is `undefined`).
pub(crate) fn stream_vertices(
    points: &[[f32; 2]],
    x_scale: &ChartScale,
    y_scale: &LinearScale,
) -> Vec<[f32; 2]> {
    points
        .iter()
        .map(|[x, y]| {
            let px = match x_scale {
                ChartScale::Linear(linear) => linear.scale(f64::from(*x)),
                ChartScale::Band(_) => f64::NAN,
            };
            [px as f32, y_scale.scale(f64::from(*y)) as f32]
        })
        .collect()
}

/// The stroke width in pixels for a GL line `width` on a display of `scale_factor`:
/// a width GL rejects (not positive) leaves its default of 1.
pub(crate) fn stream_stroke_width(width: f64, scale_factor: f32) -> f64 {
    let width = if width > 0. { width } else { 1. };
    width / canvas_dpr(scale_factor)
}

impl Series for StreamGl {
    fn kind(&self) -> &'static str {
        "streamGL"
    }

    fn key(&self) -> SharedString {
        "stream".into()
    }

    fn data_keys(&self) -> Vec<SharedString> {
        Vec::new()
    }

    fn draws_above(&self) -> bool {
        true
    }

    // Streaming data is pushed, not in the static dataset.
    fn resolve(
        &self,
        _: &SeriesContext,
        _: Option<&[StackOffset]>,
        _: Option<GroupInfo>,
    ) -> Vec<ResolvedPoint> {
        Vec::new()
    }

    fn render(
        &self,
        _: &[ResolvedPoint],
        ctx: &SeriesContext,
        _: &SeriesInfo,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.handle.set_capacity(self.buffer_size);
        self.handle.attach(window.window_handle(), cx.to_async());
        if ctx.width <= 0. || ctx.height <= 0. {
            return None;
        }
        let handle = self.handle.clone();
        let (x_scale, y_scale) = (ctx.x_scale.clone(), ctx.y_scale);
        let color = premultiplied(gl_rgb(self.color), self.opacity);
        let line_width = self.line_width;
        Some(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window, _| {
                    #[cfg(test)]
                    handle.0.paints.set(handle.0.paints.get() + 1);
                    // Read the buffer as this frame paints it, on this frame's scales.
                    let points = handle.points();
                    let Some(color) = color else {
                        return;
                    };
                    if points.len() < 2 {
                        return;
                    }
                    let vertices = stream_vertices(&points, &x_scale, &y_scale);
                    let path = Line::new()
                        .defined(|p: &[f32; 2], _| p[0].is_finite() && p[1].is_finite())
                        .x(|p: &[f32; 2], _| f64::from(p[0]))
                        .y(|p: &[f32; 2], _| f64::from(p[1]))
                        .generate(&vertices);
                    let width = stream_stroke_width(line_width, window.scale_factor()) as f32;
                    if let Some(stroke) = path.build(bounds.origin, PathStyle::Stroke { width }) {
                        window.paint_path(stroke, color);
                    }
                },
            )
            .size_full()
            .into_any_element(),
        )
    }
}
