//! A region's resize state, upstream `Resizable/createResizable.ts`'s controller
//! without its motion (the pane slide and the rubber-band drawing wait for the
//! animated-layout system, M11): bounds resolved against the live basis, a pixel
//! selection that a percentage default makes once, the collapse rules, the gesture
//! (start, move, end with the lift's velocity, cancel) and the persisted entry.

use astryx_primitives::motion::{project, uikit::scroll};
use gpui::App;

use super::model::{
    DEFAULT_COLLAPSED_SIZE, DEFAULT_MIN, DEFAULT_SIZE_FALLBACK, Persisted, ResizableSize,
    SERVER_BASIS, clamp_size, load_persisted, persist, resolve_bound,
};

/// The options a region renders with.
#[derive(Clone, Debug, Default)]
pub(crate) struct Options {
    pub default_size: Option<ResizableSize>,
    pub min_size: Option<ResizableSize>,
    pub max_size: Option<ResizableSize>,
    pub snaps: Vec<f32>,
    pub collapsible: bool,
    pub collapsed_size: Option<f32>,
    pub auto_save_id: Option<String>,
    pub default_is_collapsed: Option<bool>,
    /// Controlled collapse.
    pub is_collapsed: Option<bool>,
    /// A caller-owned container is the percentage basis.
    pub has_container: bool,
}

/// The measurements a resolution reads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Basis {
    /// The container's content box on the axis, once measured.
    pub container: Option<f32>,
    /// The window's extent on the axis.
    pub viewport: f32,
}

/// What an interaction reports, in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Report {
    Size(f32),
    Collapsed(bool),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Derived {
    /// The expanded size.
    pub size: f32,
    pub is_collapsed: bool,
    pub min: f32,
    pub max: f32,
    /// The maximum the separator reports: a configured one or a measured extent.
    pub effective_max: Option<f32>,
    pub is_basis_measured: bool,
}

#[derive(Clone, Copy, Debug)]
struct Gesture {
    basis: f32,
    start: (f32, bool),
    drag_start: f32,
    pointer: f32,
    size_before: Option<f32>,
}

/// One region.
pub(crate) struct Controller {
    opts: Options,
    persisted: Option<Persisted>,
    /// `None`: not chosen yet; `Some(None)`: waits for a measured basis.
    chosen: Option<Option<f32>>,
    uncontrolled_collapsed: bool,
    initial_default: Option<(f32, bool)>,
    committed_bounds: Option<(f32, f32)>,
    pub derived: Derived,
    gesture: Option<Gesture>,
    has_been_resized: bool,
    must_write: bool,
    written: Option<(String, f32, bool)>,
    /// The ResizeHandle that drives the region, where focus goes when its
    /// LayoutPanel collapses around it (LayoutPanel AR2).
    pub handle_focus: Option<gpui::FocusHandle>,
}

impl Controller {
    pub fn new(opts: Options, basis: &Basis, cx: &mut App) -> Self {
        let persisted = opts
            .auto_save_id
            .as_deref()
            .filter(|key| !key.is_empty())
            .and_then(|key| load_persisted(key, cx));
        let uncontrolled_collapsed = persisted
            .and_then(|p| p.is_collapsed)
            .or(opts.default_is_collapsed)
            .unwrap_or(false);
        let mut controller = Controller {
            opts,
            persisted,
            chosen: None,
            uncontrolled_collapsed,
            initial_default: None,
            committed_bounds: None,
            derived: Derived::default(),
            gesture: None,
            has_been_resized: false,
            must_write: false,
            written: None,
            handle_focus: None,
        };
        controller.compute(basis);
        controller
    }

    /// A new render's options: the region keeps its size and collapse state.
    pub fn update(&mut self, opts: Options, basis: &Basis, cx: &mut App) {
        self.opts = opts;
        self.commit(basis, cx);
    }

    pub fn collapsed_size(&self) -> f32 {
        self.opts.collapsed_size.unwrap_or(DEFAULT_COLLAPSED_SIZE)
    }

    pub fn collapsible(&self) -> bool {
        self.opts.collapsible
    }

    pub fn snaps(&self) -> &[f32] {
        &self.opts.snaps
    }

    fn controlled(&self) -> bool {
        self.opts.is_collapsed.is_some()
    }

    fn set_collapsed(&mut self, collapsed: bool) {
        if !self.controlled() {
            self.uncontrolled_collapsed = collapsed;
        }
    }

    /// Whether a percentage reads the container.
    fn needs_container(&self) -> bool {
        let depends = |s: &Option<ResizableSize>| s.is_some_and(|s| s.depends_on_basis());
        self.opts.has_container
            && (depends(&self.opts.min_size)
                || depends(&self.opts.max_size)
                || (self.persisted.and_then(|p| p.size).is_none()
                    && depends(&self.opts.default_size)))
    }

    /// Upstream `compute`.
    pub fn compute(&mut self, basis: &Basis) {
        let needs_container = self.needs_container();
        let measured = basis.container.filter(|c| *c > 0.);
        let live = if self.opts.has_container {
            measured.unwrap_or(SERVER_BASIS)
        } else {
            basis.viewport
        };
        let is_basis_measured = !needs_container || measured.is_some();
        let resolve_with = self.gesture.map_or(live, |g| g.basis);
        let source = "useResizable";
        let min = resolve_bound(
            self.opts.min_size,
            resolve_with,
            DEFAULT_MIN,
            "minSize",
            source,
        );
        let max = resolve_bound(
            self.opts.max_size,
            resolve_with,
            f32::INFINITY,
            "maxSize",
            source,
        );
        if self.initial_default.is_none_or(|(_, is_final)| !is_final) {
            self.initial_default = Some((
                resolve_bound(
                    self.opts.default_size,
                    resolve_with,
                    DEFAULT_SIZE_FALLBACK,
                    "defaultSize",
                    source,
                ),
                is_basis_measured,
            ));
        }
        let default_px = self
            .initial_default
            .map_or(DEFAULT_SIZE_FALLBACK, |(px, _)| px);
        let snaps = self.opts.snaps.clone();
        if self.chosen.is_none() {
            self.chosen = Some(
                self.persisted
                    .and_then(|p| p.size)
                    .or(is_basis_measured.then(|| clamp_size(default_px, min, max, &snaps))),
            );
        }
        if is_basis_measured && self.chosen == Some(None) {
            self.chosen = Some(Some(clamp_size(default_px, min, max, &snaps)));
        }
        if is_basis_measured && self.gesture.is_none() && self.committed_bounds != Some((min, max))
        {
            self.committed_bounds = Some((min, max));
            if let Some(Some(chosen)) = self.chosen {
                self.chosen = Some(Some(clamp_size(chosen, min, max, &snaps)));
            }
        }
        let chosen = self.chosen.flatten().unwrap_or(default_px);
        let size = clamp_size(chosen, min, max, &snaps);
        let is_collapsed = self.opts.collapsible
            && self
                .opts
                .is_collapsed
                .unwrap_or(self.uncontrolled_collapsed);
        if min > max {
            astryx_primitives::diagnostics::dev_warn_once("useResizable:inverted", || {
                format!(
                    "useResizable: the resolved minimum ({min}px) is above the resolved \
                     maximum ({max}px). The maximum wins."
                )
            });
        }
        let measured_for_max = if self.opts.has_container {
            measured.filter(|_| needs_container)
        } else {
            Some(basis.viewport)
        };
        let effective_max = if max.is_finite() {
            Some(max)
        } else {
            let value = if is_collapsed { min } else { size };
            measured_for_max.map(|extent| extent.max(value).max(min))
        };
        self.derived = Derived {
            size,
            is_collapsed,
            min,
            max,
            effective_max,
            is_basis_measured,
        };
    }

    /// Upstream `commit`: resolve, then persist.
    fn commit(&mut self, basis: &Basis, cx: &mut App) {
        self.compute(basis);
        self.persist(cx);
    }

    /// Upstream `persist`: the last settled size, once a person resized, never
    /// mid-gesture.
    fn persist(&mut self, cx: &mut App) {
        let Some(key) = self.opts.auto_save_id.clone().filter(|k| !k.is_empty()) else {
            return;
        };
        if !self.derived.is_basis_measured {
            return;
        }
        let restored_changed = self
            .persisted
            .and_then(|p| p.size)
            .is_some_and(|size| size != self.derived.size);
        if (!self.has_been_resized && !restored_changed) || self.gesture.is_some() {
            return;
        }
        let next = (key, self.derived.size, self.derived.is_collapsed);
        if self.must_write || self.written.as_ref() != Some(&next) {
            self.must_write = false;
            persist(&next.0, next.1, next.2, cx);
            self.written = Some(next);
        }
    }

    /// The size shown: 0 while collapsed.
    pub fn shown_size(&self) -> f32 {
        if self.derived.is_collapsed {
            0.
        } else {
            self.derived.size
        }
    }

    pub fn collapse(&mut self, basis: &Basis, cx: &mut App) -> Vec<Report> {
        if !self.opts.collapsible || self.derived.is_collapsed {
            return Vec::new();
        }
        self.has_been_resized = true;
        self.set_collapsed(true);
        self.commit(basis, cx);
        vec![Report::Collapsed(true), Report::Size(0.)]
    }

    pub fn expand(&mut self, basis: &Basis, cx: &mut App) -> Vec<Report> {
        let was = self.derived.is_collapsed;
        if was {
            self.has_been_resized = true;
        }
        self.set_collapsed(false);
        self.commit(basis, cx);
        let mut out = Vec::new();
        if was {
            out.push(Report::Collapsed(false));
        }
        out.push(Report::Size(self.derived.size));
        out
    }

    pub fn resize(&mut self, next: f32, basis: &Basis, cx: &mut App) -> Vec<Report> {
        if !next.is_finite() || next < 0. {
            astryx_primitives::diagnostics::dev_warn_once("useResizable:resize", || {
                format!(
                    "useResizable: resize({next}) is not a pixel size. Keeping the current size."
                )
            });
            return Vec::new();
        }
        let d = self.derived;
        let clamped = clamp_size(next, d.min, d.max, &self.opts.snaps);
        let was = d.is_collapsed;
        if was || clamped != d.size {
            self.has_been_resized = true;
        }
        self.chosen = Some(Some(clamped));
        self.set_collapsed(false);
        self.commit(basis, cx);
        let mut out = Vec::new();
        if was {
            out.push(Report::Collapsed(false));
        }
        out.push(Report::Size(clamped));
        out
    }

    /// A gesture begins: the region holds its basis until it ends.
    pub fn start(&mut self, basis: &Basis) {
        let live = if self.opts.has_container {
            basis.container.filter(|c| *c > 0.).unwrap_or(SERVER_BASIS)
        } else {
            basis.viewport
        };
        let shown = self.shown_size();
        self.gesture = Some(Gesture {
            basis: live,
            start: (self.derived.size, self.derived.is_collapsed),
            drag_start: shown,
            pointer: shown,
            size_before: (!self.derived.is_collapsed).then_some(self.derived.size),
        });
    }

    /// The gesture has moved `delta` px from where it began.
    pub fn move_by(&mut self, delta: f32, basis: &Basis, cx: &mut App) -> Vec<Report> {
        let Some(gesture) = self.gesture.as_mut() else {
            return Vec::new();
        };
        let raw = gesture.drag_start + delta;
        gesture.pointer = raw;
        let collapsed_size = self.collapsed_size();
        if self.derived.is_collapsed && raw < collapsed_size {
            return Vec::new();
        }
        let reopens = self.derived.is_collapsed && raw >= collapsed_size;
        if reopens {
            self.set_collapsed(false);
        }
        let clamped = clamp_size(raw, self.derived.min, self.derived.max, &self.opts.snaps);
        self.chosen = Some(Some(clamped));
        self.commit(basis, cx);
        let mut out = Vec::new();
        if reopens {
            out.push(Report::Collapsed(false));
        }
        out.push(Report::Size(clamped));
        out
    }

    /// The gesture ends as completed; a lift passes its velocity in px/s along the
    /// resize axis. A lift that comes to rest under the threshold collapses.
    pub fn end(&mut self, velocity: f32, basis: &Basis, cx: &mut App) -> Vec<Report> {
        let Some(gesture) = self.gesture else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let pointer = gesture.pointer as f64;
        let rests = pointer.min(project(
            pointer,
            velocity as f64,
            scroll::DECELERATION_NORMAL,
        ));
        let closes = self.opts.collapsible
            && !self.derived.is_collapsed
            && (rests as f32) < self.collapsed_size();
        if closes {
            if let Some(before) = gesture.size_before {
                self.chosen = Some(Some(before));
            }
            self.set_collapsed(true);
            self.commit(basis, cx);
            out.push(Report::Collapsed(true));
            out.push(Report::Size(0.));
        }
        self.end_gesture(basis, cx);
        out
    }

    /// The gesture ends without completing.
    pub fn cancel(&mut self, basis: &Basis, cx: &mut App) {
        self.end_gesture(basis, cx);
    }

    fn end_gesture(&mut self, basis: &Basis, cx: &mut App) {
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        if gesture.start != (self.derived.size, self.derived.is_collapsed) {
            self.has_been_resized = true;
        }
        self.must_write = true;
        self.commit(basis, cx);
    }

    pub fn is_gesture_active(&self) -> bool {
        self.gesture.is_some()
    }
}
