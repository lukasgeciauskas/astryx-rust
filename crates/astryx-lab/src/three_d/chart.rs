//! `ThreeDChart` (upstream lab `ThreeDChart.tsx`): the root of a 3D chart. It measures its
//! width, works out the domains and the camera's projection, and gives them to its marks;
//! the camera turns by drag, arrow keys and an optional auto-rotation that respects reduced
//! motion and pauses while hovered or focused.

use std::panic::Location;
use std::rc::Rc;
use std::time::Duration;

use astryx_charts::{ChartValue, Datum};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, CursorStyle, ElementId, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Role, SharedString, StyleRefinement, Task, Window,
    canvas, div,
};
use web_time::Instant;

use super::context::ThreeDContext;
use super::projection::Projection;
use super::types::Camera;
use crate::lab_chart::MeasuredArea;

/// Degrees the camera turns per arrow key (upstream `KEYBOARD_STEP_DEGREES`).
pub const KEYBOARD_STEP_DEGREES: f64 = 10.;
/// How often auto-rotation steps (upstream's 50 ms, about 20 fps).
const ROTATION_INTERVAL: Duration = Duration::from_millis(50);

/// The root of a 3D chart (upstream lab `ThreeDChart`): its marks (`ThreeDScatter`, `ThreeDBar`,
/// `ThreeDSurface`, `ThreeDScatterGl`, `ThreeDGrid`, `ThreeDAxis`) read its context with
/// [`use_3d`](super::use_3d).
#[derive(IntoElement)]
pub struct ThreeDChart {
    id: ElementId,
    data: Rc<[Datum]>,
    x_key: SharedString,
    y_key: SharedString,
    z_key: SharedString,
    x_domain: Option<[f64; 2]>,
    y_domain: Option<[f64; 2]>,
    z_domain: Option<[f64; 2]>,
    height: f64,
    azimuth: f64,
    elevation: f64,
    interactive: bool,
    auto_rotate: f64,
    label: Option<SharedString>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
    source: &'static Location<'static>,
}

impl ThreeDChart {
    /// Upstream `data`, `xKey`, `yKey` (vertical) and `zKey` (depth). `id` keys its camera.
    #[track_caller]
    pub fn new(
        id: impl Into<ElementId>,
        data: impl Into<Rc<[Datum]>>,
        x_key: impl Into<SharedString>,
        y_key: impl Into<SharedString>,
        z_key: impl Into<SharedString>,
    ) -> Self {
        ThreeDChart {
            id: id.into(),
            data: data.into(),
            x_key: x_key.into(),
            y_key: y_key.into(),
            z_key: z_key.into(),
            x_domain: None,
            y_domain: None,
            z_domain: None,
            height: 400.,
            azimuth: 35.,
            elevation: 25.,
            interactive: false,
            auto_rotate: 0.,
            label: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
            source: Location::caller(),
        }
    }

    /// Upstream `xDomain` (the numbers' extent by default).
    pub fn x_domain(mut self, min: f64, max: f64) -> Self {
        self.x_domain = Some([min, max]);
        self
    }

    /// Upstream `yDomain`.
    pub fn y_domain(mut self, min: f64, max: f64) -> Self {
        self.y_domain = Some([min, max]);
        self
    }

    /// Upstream `zDomain`.
    pub fn z_domain(mut self, min: f64, max: f64) -> Self {
        self.z_domain = Some([min, max]);
        self
    }

    /// Upstream `height`, in pixels (400). The width fills the container.
    pub fn height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }

    /// Upstream `azimuth`: the camera's first turn around the vertical axis, in degrees (35).
    pub fn azimuth(mut self, degrees: f64) -> Self {
        self.azimuth = degrees;
        self
    }

    /// Upstream `elevation`: the camera's first height above the floor, in degrees (25).
    pub fn elevation(mut self, degrees: f64) -> Self {
        self.elevation = degrees;
        self
    }

    /// Upstream `interactive`: drag and the arrow keys turn the camera (off).
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// Upstream `autoRotate`: degrees per 60 fps frame (0, off). It pauses while hovered or
    /// focused and is off under reduced motion.
    pub fn auto_rotate(mut self, degrees: f64) -> Self {
        self.auto_rotate = degrees;
        self
    }

    /// Upstream `label`: the accessible name ("3D chart of {y} by {x} and {z}" by default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for ThreeDChart {
    /// Upstream `children`: the marks, drawn in order.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for ThreeDChart {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// A key's numbers' extent (upstream `computeDomain`): `[0, 1]` for a side with no number.
pub fn compute_domain(data: &[Datum], key: &str) -> [f64; 2] {
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for d in data {
        if let Some(ChartValue::Number(v)) = d.get(key) {
            if *v < min {
                min = *v;
            }
            if *v > max {
                max = *v;
            }
        }
    }
    [
        if min == f64::INFINITY { 0. } else { min },
        if max == f64::NEG_INFINITY { 1. } else { max },
    ]
}

/// The chart's accessible name: `label`, else upstream's English default.
pub fn accessible_label(
    label: Option<&str>,
    x_key: &str,
    y_key: &str,
    z_key: &str,
) -> SharedString {
    match label {
        Some(l) => l.to_string().into(),
        None => format!("3D chart of {y_key} by {x_key} and {z_key}").into(),
    }
}

/// The elevation's range, the same for drag and keys (upstream's `Math.max(-89, Math.min(89, v))`).
pub fn clamp_elevation(v: f64) -> f64 {
    let m = if v.is_nan() { f64::NAN } else { v.min(89.) };
    if m.is_nan() { f64::NAN } else { m.max(-89.) }
}

/// The camera after an arrow key (upstream `handleKeyDown`): `None` for any other key.
pub fn camera_after_key(camera: Camera, key: &str) -> Option<Camera> {
    let (daz, del) = match key {
        "left" => (-KEYBOARD_STEP_DEGREES, 0.),
        "right" => (KEYBOARD_STEP_DEGREES, 0.),
        "up" => (0., KEYBOARD_STEP_DEGREES),
        "down" => (0., -KEYBOARD_STEP_DEGREES),
        _ => return None,
    };
    Some(Camera {
        azimuth: camera.azimuth + daz,
        elevation: clamp_elevation(camera.elevation + del),
        distance: camera.distance,
    })
}

/// The camera during a drag of (`dx`, `dy`) pixels from `start` (upstream `handleMove`): half a degree
/// per pixel.
pub fn camera_after_drag(start: Camera, dx: f64, dy: f64) -> Camera {
    Camera::new(
        start.azimuth + dx * 0.5,
        clamp_elevation(start.elevation - dy * 0.5),
    )
}

/// The degrees one auto-rotation step turns (upstream's `autoRotate * (elapsed / 16.67)`),
/// normalized to a 60 fps frame.
pub fn rotation_step(auto_rotate: f64, elapsed_ms: f64) -> f64 {
    auto_rotate * (elapsed_ms / 16.67)
}

/// The camera and what drives it across frames.
pub(crate) struct ChartState {
    pub(crate) camera: Camera,
    drag: Option<(gpui::Point<Pixels>, Camera)>,
    hovered: bool,
    last_update: Option<Instant>,
    /// The timer for the next rotation step.
    tick: Option<Task<()>>,
}

/// What a chart drew, for tests: its label, whether it is a tab stop, its size, whether it is
/// turning, and its focus handle.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct ChartDrawn {
    pub label: String,
    pub focusable: bool,
    pub size: Option<(f64, f64)>,
    pub rotating: bool,
    pub focus: Option<gpui::FocusHandle>,
    pub hovered: bool,
    pub focused: bool,
}

#[cfg(test)]
thread_local! {
    pub(crate) static CHARTS: std::cell::RefCell<std::collections::HashMap<String, ChartDrawn>> = Default::default();
}

impl RenderOnce for ThreeDChart {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (azimuth, elevation) = (self.azimuth, self.elevation);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-three-d", self.id).into()),
            cx,
            move |_, _| ChartState {
                camera: Camera::new(azimuth, elevation),
                drag: None,
                hovered: false,
                last_update: None,
                tick: None,
            },
        );
        let focus = astryx_primitives::focus::control_focus(&self.id, window, cx);
        let focusable = self.interactive || self.auto_rotate != 0.;
        let focused = focusable && focus.is_focused(window);
        let reduced = astryx_primitives::prefers_reduced_motion(cx);
        let auto_rotate = self.auto_rotate;
        let active = auto_rotate != 0. && !reduced && !state.read(cx).hovered && !focused;
        // Auto-rotation steps about every 50 ms while active, normalized to 60 fps; the first
        // step after it starts counts one interval (upstream's `lastUpdate === 0`). Upstream
        // checks on every animation frame and re-renders only on a step; a timer for the next
        // step draws the same frames.
        if active {
            let now = cx.background_executor().now();
            let due = state.update(cx, |s, _| {
                let due = s
                    .last_update
                    .is_none_or(|last| now.saturating_duration_since(last) >= ROTATION_INTERVAL);
                if s.drag.is_none() && due {
                    let elapsed = s.last_update.map_or(50., |last| {
                        now.saturating_duration_since(last).as_secs_f64() * 1000.
                    });
                    s.last_update = Some(now);
                    s.camera.azimuth += rotation_step(auto_rotate, elapsed);
                }
                s.last_update.map(|last| last + ROTATION_INTERVAL)
            });
            if state.read(cx).tick.is_none() {
                let wait = due.map_or(Duration::ZERO, |due| due.saturating_duration_since(now));
                let wait = if wait.is_zero() {
                    ROTATION_INTERVAL
                } else {
                    wait
                };
                let timer = cx.background_executor().timer(wait);
                let owner = state.downgrade();
                let tick = cx.spawn(async move |cx| {
                    timer.await;
                    let _ = owner.update(cx, |s, cx| {
                        s.tick = None;
                        cx.notify();
                    });
                });
                state.update(cx, |s, _| s.tick = Some(tick));
            }
        } else if state.read(cx).last_update.is_some() || state.read(cx).tick.is_some() {
            state.update(cx, |s, _| {
                s.last_update = None;
                s.tick = None;
            });
        }
        let camera = state.read(cx).camera;
        let label = accessible_label(self.label.as_deref(), &self.x_key, &self.y_key, &self.z_key);
        #[cfg(test)]
        CHARTS.with(|c| {
            let mut c = c.borrow_mut();
            let e = c.entry(self.id.to_string()).or_default();
            e.label = label.to_string();
            e.focusable = focusable;
            e.rotating = active;
            e.focus = Some(focus.clone());
            e.hovered = state.read(cx).hovered;
            e.focused = focused;
        });
        let x_domain = self
            .x_domain
            .unwrap_or_else(|| compute_domain(&self.data, &self.x_key));
        let y_domain = self
            .y_domain
            .unwrap_or_else(|| compute_domain(&self.data, &self.y_key));
        let z_domain = self
            .z_domain
            .unwrap_or_else(|| compute_domain(&self.data, &self.z_key));
        let height = self.height.max(0.);
        let interactive = self.interactive;
        let id = self.id.clone();
        let (data, x_key, y_key, z_key, children) =
            (self.data, self.x_key, self.y_key, self.z_key, self.children);
        let build = move |bounds: Bounds<Pixels>| -> AnyElement {
            let width = f64::from(bounds.size.width);
            #[cfg(test)]
            CHARTS.with(|c| {
                c.borrow_mut().entry(id.to_string()).or_default().size = Some((width, height))
            });
            let ctx = ThreeDContext {
                id: id.clone(),
                width,
                height,
                data,
                x_key,
                y_key,
                z_key,
                x_domain,
                y_domain,
                z_domain,
                camera,
                projection: Projection::new(&camera, width, height),
                overlays: Rc::default(),
            };
            let overlays = ctx.overlays.clone();
            let (down, mv, up, hover, key) = (
                state.clone(),
                state.clone(),
                state.clone(),
                state.clone(),
                state.clone(),
            );
            let mut svg = div()
                .id(ElementId::Name(format!("{id}-three-d-plot").into()))
                .debug_selector(|| "three-d-plot".into())
                .relative()
                .size_full()
                .role(Role::Image)
                .aria_label(label)
                .when(interactive, |d| d.cursor(CursorStyle::OpenHand))
                .on_hover(move |hovered, _, cx| {
                    hover.update(cx, |s, cx| {
                        s.hovered = *hovered;
                        if !*hovered {
                            s.drag = None;
                        }
                        cx.notify();
                    })
                })
                .on_mouse_move(move |e: &MouseMoveEvent, _, cx| {
                    mv.update(cx, |s, cx| {
                        if let Some((start, from)) = s.drag {
                            let (dx, dy) = (
                                f64::from(e.position.x - start.x),
                                f64::from(e.position.y - start.y),
                            );
                            s.camera = camera_after_drag(from, dx, dy);
                            cx.notify();
                        }
                    })
                })
                .children(
                    children
                        .into_iter()
                        .map(|child| div().absolute().top_0().left_0().size_full().child(child)),
                )
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds: Bounds<Pixels>, _, window, cx| {
                            let paints: Vec<_> = overlays.borrow_mut().drain(..).collect();
                            for paint in paints {
                                paint(bounds.origin, window, cx);
                            }
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                );
            for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
                let down = down.clone();
                svg = svg.on_mouse_down(button, move |e: &MouseDownEvent, _, cx| {
                    if !interactive {
                        return;
                    }
                    down.update(cx, |s, _| s.drag = Some((e.position, s.camera)));
                });
                let up = up.clone();
                svg = svg.on_mouse_up(button, move |_: &MouseUpEvent, _, cx| {
                    up.update(cx, |s, _| s.drag = None)
                });
            }
            if focusable {
                svg = svg.track_focus(&focus.clone().tab_stop(true));
            }
            svg = svg.on_key_down(move |e: &KeyDownEvent, _, cx| {
                if !interactive {
                    return;
                }
                let next = key.read(cx).camera;
                if let Some(camera) = camera_after_key(next, e.keystroke.key.as_str()) {
                    cx.stop_propagation();
                    key.update(cx, |s, cx| {
                        s.camera = camera;
                        cx.notify();
                    });
                }
            });
            astryx_primitives::provide(ctx, svg).into_any_element()
        };
        let area = MeasuredArea {
            id: (self.id.clone(), "three-d-area").into(),
            height,
            build: Some(Box::new(build)),
            child: None,
            source: self.source,
        };
        let mut root = div().id(self.id).w_full();
        root.style().refine(&self.style);
        root.child(area)
    }
}
