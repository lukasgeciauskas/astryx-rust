//! A region's resize state, upstream `Resizable/createResizable.ts`'s controller: bounds
//! resolved against the live basis, a pixel selection that a percentage default makes
//! once, the collapse rules, the gesture (start, move, end with the lift's velocity,
//! cancel) and the persisted entry; and its motion (FR8): the slide of the pane a
//! LayoutPanel lends it each time the region collapses or opens, and the rubber band a
//! pointer pulls past a limit, which the handle draws and springs back on
//! `springs.settle`.

use astryx_primitives::motion::uikit::springs::role::SETTLE;
use astryx_primitives::motion::{
    FnCurve, SharedCurve, project, resolve_spring, rubber_band, rubber_band_inverse,
    rubber_band_slope, solve_spring, uikit::scroll,
};
use gpui::App;
use web_time::Instant;

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
    /// A pointer drives it (a key's step is a gesture too).
    by_pointer: bool,
}

/// How far a pull past a limit draws the handle at most (upstream
/// `RUBBER_BAND_TRAVEL_PX`, `spec:AST-094` FR5).
pub(crate) const RUBBER_BAND_TRAVEL_PX: f32 = 8.;

/// How the next collapse or open slides (upstream `slideNextOnRelease`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum NextSlide {
    /// On the theme's curve tokens: `collapse()`, `expand()`, Enter, a key's step, a
    /// controlled change.
    #[default]
    Theme,
    /// A lift's close: `springs.settle` from its velocity (px/s along the size).
    Release(f32),
    /// A pointer reopening the pane drives the edge: nothing plays.
    Still,
}

/// The pane's slide: the extent it showed when the change began, the extent it heads
/// for, and the extent its content stays laid out at (the open size) while it is clipped.
#[derive(Clone)]
pub(crate) struct PaneSlide {
    pub from: f32,
    pub to: f32,
    pub held: f32,
    pub start: Instant,
    /// A lift's close: the progress (0 to 1) of `springs.settle` from its velocity, and
    /// how long it takes. Otherwise the theme's move curve plays.
    pub release: Option<(
        std::time::Duration,
        crate::animated_layout::engine::GlideEase,
    )>,
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
    /// A LayoutPanel renders the region's pane (LayoutPanel FR7): it slides, and a pointer
    /// pulls a rubber band past a limit.
    pub pane_lent: bool,
    /// The running slide of the pane.
    pub slide: Option<PaneSlide>,
    next_slide: NextSlide,
    /// What the running slide shows now, as the last frame drew it.
    pub slide_shown: Option<f32>,
    /// The drawn handle's travel past the limit the size stops at, during a pull.
    pub overshoot: f32,
    /// The handle springing back to the limit after a pull: its offset over time.
    pub spring_back: Option<(SharedCurve, Instant)>,
    /// The band a press caught mid-spring-back, which the pointer moves 1:1 before the
    /// band resists again (upstream `caught`).
    caught: f32,
    /// A pointer holds a closing pane's slide below the minimum: the extent it shows
    /// under the pointer (upstream `held`).
    pub held: Option<f32>,
    /// After a catch reopened the pane between snap points, the edge follows the
    /// pointer until it passes one (upstream `isCarryingToSnap`).
    carrying: bool,
    /// How many times a pointer caught a running slide: the content beside lands where it
    /// is laid out each time.
    pub catches: u32,
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
            pane_lent: false,
            slide: None,
            next_slide: NextSlide::Theme,
            slide_shown: None,
            overshoot: 0.,
            spring_back: None,
            caught: 0.,
            held: None,
            carrying: false,
            catches: 0,
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
        // While a caught edge carries to the next snap point, it rests between them.
        let rests_on: &[f32] = if self.gesture.is_some() && self.carrying {
            &[]
        } else {
            &snaps
        };
        let size = clamp_size(chosen, min, max, rests_on);
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

    /// Upstream `commit`: resolve, slide the pane when the region collapsed or opened,
    /// then persist.
    fn commit(&mut self, basis: &Basis, cx: &mut App) {
        let before = (self.derived.is_collapsed, self.shown_size());
        self.compute(basis);
        let kind = std::mem::take(&mut self.next_slide);
        if before.0 != self.derived.is_collapsed {
            self.slide_pane(before.1, kind, cx);
        }
        self.persist(cx);
    }

    /// The region collapsed or opened: a lent pane slides from the extent it showed
    /// (a running slide's, or its old size) to its new one, unless a pointer drives the
    /// edge (Resizable FR8).
    fn slide_pane(&mut self, shown_before: f32, kind: NextSlide, cx: &mut App) {
        let from = self
            .slide_shown
            .take()
            .unwrap_or(shown_before + self.overshoot);
        self.slide = None;
        if !self.pane_lent || kind == NextSlide::Still {
            return;
        }
        let to = self.shown_size();
        self.start_slide(from, to, kind, cx);
    }

    /// The pane's edge slides from `from` to `to`, on the theme's curve or, after a lift,
    /// `springs.settle` from its velocity (upstream `slideNextOnRelease`).
    fn start_slide(&mut self, from: f32, to: f32, kind: NextSlide, cx: &mut App) {
        if !self.pane_lent || (from - to).abs() < 0.5 {
            return;
        }
        let release = match kind {
            NextSlide::Release(velocity) => {
                // The extent's speed over the distance left, in progress per second.
                let progress = -f64::from(velocity) / f64::from(from - to);
                Some(crate::animated_layout::engine::spring_glide(
                    SETTLE, progress,
                ))
            }
            _ => None,
        };
        self.overshoot = 0.;
        self.spring_back = None;
        self.slide = Some(PaneSlide {
            from,
            to,
            held: self.derived.size.max(from).max(to),
            start: cx.background_executor().now(),
            release,
        });
    }

    /// How far the drawn handle runs past the bound `raw` is beyond (upstream
    /// `overshootPast`): the shared rubber band over about 8 px; below the minimum it
    /// stops at 0.
    fn overshoot_past(&mut self, raw: f32) -> f32 {
        let (min, max) = (self.derived.min, self.derived.max);
        if raw >= min && raw <= max {
            self.caught = 0.;
            return 0.;
        }
        let bound = if raw > max { max } else { min };
        let excess = raw - bound;
        let held = if self.caught.signum() == excess.signum() {
            self.caught
        } else {
            0.
        };
        // The band follows the pointer 1:1 over what a catch held, then resists again
        // over the rest of its extent (UIScrollView's rubber band, `spec:AST-094` FR5).
        let shown = if excess.abs() <= held.abs() {
            excess
        } else {
            held + rubber_band(
                f64::from(excess - held),
                f64::from((RUBBER_BAND_TRAVEL_PX - held.abs()).max(0.)),
            ) as f32
        };
        (bound + shown).max(0.) - bound
    }

    /// The pane opens at once at `size`, under the pointer, with no slide (upstream
    /// `reopenAt`): between snap points the edge then carries to the next one.
    fn reopen_at(&mut self, size: f32, basis: &Basis, cx: &mut App) -> Vec<Report> {
        let was = self.derived.is_collapsed;
        self.set_collapsed(false);
        let clamped = clamp_size(size, self.derived.min, self.derived.max, &[]);
        self.carrying = !self.opts.snaps.is_empty() && !self.opts.snaps.contains(&clamped);
        self.chosen = Some(Some(clamped));
        self.caught = 0.;
        self.next_slide = NextSlide::Still;
        self.slide_shown = None;
        self.overshoot = if self.pane_lent {
            self.overshoot_past(size)
        } else {
            0.
        };
        if let Some(g) = self.gesture.as_mut() {
            g.size_before = Some(clamped);
        }
        self.commit(basis, cx);
        let mut out = Vec::new();
        if was {
            out.push(Report::Collapsed(false));
        }
        out.push(Report::Size(clamped));
        out
    }

    /// After a pull past a limit, the drawn handle springs back to the bound on
    /// `springs.settle` from the release velocity, scaled by the band's slope (upstream
    /// `settleOvershoot`). The size never left the bound, so nothing is laid out.
    fn settle_overshoot(&mut self, velocity: f32, cx: &mut App) {
        let from = std::mem::take(&mut self.overshoot);
        if from == 0. || !self.pane_lent {
            return;
        }
        let dimension = f64::from(RUBBER_BAND_TRAVEL_PX);
        let slope = rubber_band_slope(rubber_band_inverse(f64::from(from), dimension), dimension);
        let curve = solve_spring(
            resolve_spring(SETTLE, 1.).to_spring(),
            f64::from(from),
            0.,
            f64::from(velocity) * slope,
        );
        let duration = curve.settle_time(0.05);
        let curve = FnCurve::new(
            duration,
            move |t| if t >= duration { 0. } else { curve.value(t) },
            move |t| if t >= duration { 0. } else { curve.velocity(t) },
        )
        .shared();
        self.spring_back = Some((curve, cx.background_executor().now()));
    }

    /// The drawn handle's offset past the size now: the pull's band, or its spring back.
    pub fn handle_offset(&mut self, now: Instant) -> (f32, bool) {
        if self.overshoot != 0. {
            return (self.overshoot, false);
        }
        match &self.spring_back {
            Some((curve, start)) => {
                let t = now.saturating_duration_since(*start).as_secs_f64();
                if t >= curve.duration() {
                    self.spring_back = None;
                    (0., false)
                } else {
                    (curve.at(t) as f32, true)
                }
            }
            None => (0., false),
        }
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
        self.next_slide = NextSlide::Theme;
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

    /// A gesture begins: the region holds its basis until it ends. A pointer that
    /// presses mid-slide takes the edge where it shows: a closing pane opens there at
    /// once, and a running band goes on from where it is drawn.
    pub fn start(&mut self, basis: &Basis, by_pointer: bool, cx: &mut App) {
        let live = if self.opts.has_container {
            basis.container.filter(|c| *c > 0.).unwrap_or(SERVER_BASIS)
        } else {
            basis.viewport
        };
        let caught = self
            .slide_shown
            .take()
            .filter(|_| self.slide.is_some() && by_pointer);
        let now = cx.background_executor().now();
        // A pointer catches a spring-back where it is drawn.
        let (band, _) = self.handle_offset(now);
        let band = if by_pointer { band } else { 0. };
        self.spring_back = None;
        self.overshoot = band;
        self.caught = band;
        self.carrying = false;
        self.held = None;
        let mut shown = self.shown_size() + band;
        if caught.is_some() || self.slide.is_some() {
            self.slide = None;
        }
        self.gesture = Some(Gesture {
            basis: live,
            start: (self.derived.size, self.derived.is_collapsed),
            drag_start: shown,
            pointer: shown,
            size_before: (!self.derived.is_collapsed).then_some(self.derived.size),
            by_pointer,
        });
        if let Some(at) = caught {
            // The edge stops where it shows; the content beside lands under it.
            self.catches = self.catches.wrapping_add(1);
            shown = at;
            if let Some(g) = self.gesture.as_mut() {
                g.drag_start = at;
                g.pointer = at;
            }
            if self.derived.is_collapsed && at < self.derived.min {
                // A closing pane caught below its minimum: the pointer holds the slide.
                self.held = Some(at.max(0.));
            } else {
                self.reopen_at(at, basis, cx);
            }
        }
        let _ = shown;
    }

    /// The gesture has moved `delta` px from where it began.
    pub fn move_by(&mut self, delta: f32, basis: &Basis, cx: &mut App) -> Vec<Report> {
        let Some(gesture) = self.gesture.as_mut() else {
            return Vec::new();
        };
        let previous = gesture.pointer;
        let raw = gesture.drag_start + delta;
        gesture.pointer = raw;
        let by_pointer = gesture.by_pointer;
        if self.held.is_some() {
            // The held slide follows the pointer until it passes the minimum, where the
            // pane opens under it.
            if raw < self.derived.min {
                self.held = Some(raw.max(0.));
                return Vec::new();
            }
            self.held = None;
            return self.reopen_at(raw, basis, cx);
        }
        if self.carrying
            && self
                .opts
                .snaps
                .iter()
                .any(|snap| (snap - previous) * (snap - raw) <= 0.)
        {
            self.carrying = false;
        }
        let collapsed_size = self.collapsed_size();
        if self.derived.is_collapsed && raw < collapsed_size {
            return Vec::new();
        }
        let reopens = self.derived.is_collapsed && raw >= collapsed_size;
        if reopens {
            self.set_collapsed(false);
            // Only a pointer reopens without a slide: it drives the edge. A key's step
            // plays one.
            self.next_slide = if by_pointer {
                NextSlide::Still
            } else {
                NextSlide::Theme
            };
        }
        let rests_on: &[f32] = if self.carrying { &[] } else { &self.opts.snaps };
        let clamped = clamp_size(raw, self.derived.min, self.derived.max, rests_on);
        self.chosen = Some(Some(clamped));
        // Only a pointer on a pane a LayoutPanel lends pulls the band; a key, and a
        // pane at the hook's size alone, stop at the limit.
        self.overshoot = if self.pane_lent && by_pointer {
            self.overshoot_past(raw)
        } else {
            0.
        };
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
        if let Some(shown) = self.held.take() {
            // A lift while the pointer holds a closing pane (upstream `letGoOfHeldSlide`):
            // under the threshold, or fast enough to rest under it, the slide plays on and
            // shuts; any other lift opens the pane to its minimum from where it shows.
            if (rests as f32) < self.collapsed_size() {
                self.start_slide(shown, 0., NextSlide::Release(velocity), cx);
            } else {
                self.slide_shown = Some(shown);
                self.next_slide = NextSlide::Release(velocity);
                self.set_collapsed(false);
                self.chosen = Some(Some(self.derived.min));
                self.commit(basis, cx);
                out.push(Report::Collapsed(false));
                out.push(Report::Size(self.derived.min));
            }
            self.end_gesture(basis, cx);
            return out;
        }
        if self.carrying {
            // A lift between snap points: the edge glides from where it shows to the snap
            // nearest it (upstream `settleOnNearestSnap`).
            self.carrying = false;
            let shown = self.derived.size + std::mem::take(&mut self.overshoot);
            let target = clamp_size(shown, self.derived.min, self.derived.max, &self.opts.snaps);
            self.chosen = Some(Some(target));
            self.commit(basis, cx);
            self.start_slide(shown, target, NextSlide::Release(velocity), cx);
            out.push(Report::Size(target));
            self.end_gesture(basis, cx);
            return out;
        }
        let closes = self.opts.collapsible
            && !self.derived.is_collapsed
            && (rests as f32) < self.collapsed_size();
        if closes {
            if let Some(before) = gesture.size_before {
                self.chosen = Some(Some(before));
            }
            // A lift's close settles from its release velocity; a key's on the theme.
            self.next_slide = if gesture.by_pointer {
                NextSlide::Release(velocity)
            } else {
                NextSlide::Theme
            };
            self.slide_shown = Some(self.shown_size() + self.overshoot);
            self.set_collapsed(true);
            self.commit(basis, cx);
            out.push(Report::Collapsed(true));
            out.push(Report::Size(0.));
        } else {
            self.settle_overshoot(velocity, cx);
        }
        self.end_gesture(basis, cx);
        out
    }

    /// The gesture ends without completing: a band springs back from rest, a held slide
    /// opens to the minimum and a carry lands on the nearest snap point.
    pub fn cancel(&mut self, basis: &Basis, cx: &mut App) {
        if let Some(shown) = self.held.take() {
            // Cancelled, a held slide plays on and shuts.
            self.start_slide(shown, 0., NextSlide::Theme, cx);
            self.end_gesture(basis, cx);
            return;
        }
        if self.carrying {
            self.end(0., basis, cx);
            return;
        }
        self.settle_overshoot(0., cx);
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
