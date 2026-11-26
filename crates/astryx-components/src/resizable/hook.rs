//! Upstream `useResizable`: one region, or named regions sharing an axis, a
//! container and a storage key. Its output goes to LayoutPanel's, ResizeHandle's and
//! SideNav's `resizable` slots; any element can use its `size`.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    App, Bounds, ElementId, Entity, IntoElement, Pixels, SharedString, Styled, Window, canvas,
};

use super::controller::{Basis, Controller, Options, Report};
use super::model::{ResizableSize, ResizeDirection};
use crate::animated_layout::LayoutMotion;

type SizeFn = Rc<dyn Fn(f32, &mut Window, &mut App)>;
type CollapseFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type StepFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `containerRef`: the element a percentage is a share of. Measured by its
/// `probe`, placed inside that element.
#[derive(Clone, Default)]
pub struct ResizableBasis(Rc<Cell<Option<Bounds<Pixels>>>>);

impl ResizableBasis {
    /// The container's last measured box.
    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.0.get()
    }

    /// Measures its parent's box on each layout; place it inside the container.
    pub fn probe(&self) -> impl IntoElement {
        let cell = self.0.clone();
        canvas(
            move |bounds, window, cx| {
                if cell.get() != Some(bounds) {
                    cell.set(Some(bounds));
                    window.defer(cx, |window, _| window.refresh());
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

/// A basis that lasts between frames, keyed by `id`.
pub fn use_resizable_basis(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> ResizableBasis {
    let id = id.into();
    window
        .use_keyed_state(
            ElementId::Name(format!("{id:?}-basis").into()),
            cx,
            |_, _| ResizableBasis::default(),
        )
        .read(cx)
        .clone()
}

/// Upstream `UseResizableSingleConfig` (and a region of the multi form).
#[derive(Clone, Default)]
pub struct UseResizable {
    default_size: Option<ResizableSize>,
    min_size: Option<ResizableSize>,
    max_size: Option<ResizableSize>,
    snaps: Vec<f32>,
    collapsible: bool,
    collapsed_size: Option<f32>,
    shrink_order: Option<i32>,
    container: Option<ResizableBasis>,
    direction: ResizeDirection,
    auto_save_id: Option<SharedString>,
    default_is_collapsed: Option<bool>,
    is_collapsed: Option<bool>,
    on_size_change: Option<SizeFn>,
    on_collapse_change: Option<CollapseFn>,
}

impl UseResizable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `defaultSize`: a percentage resolves once into pixels. Default 250.
    pub fn default_size(mut self, size: impl Into<ResizableSize>) -> Self {
        self.default_size = Some(size.into());
        self
    }

    /// Upstream `minSize`. Default 50.
    pub fn min_size(mut self, size: impl Into<ResizableSize>) -> Self {
        self.min_size = Some(size.into());
        self
    }

    /// Upstream `maxSize`. Unbounded by default.
    pub fn max_size(mut self, size: impl Into<ResizableSize>) -> Self {
        self.max_size = Some(size.into());
        self
    }

    /// Upstream `snaps`: the pixel sizes the region rests on.
    pub fn snaps(mut self, snaps: impl IntoIterator<Item = f32>) -> Self {
        self.snaps = snaps.into_iter().collect();
        self
    }

    /// Upstream `collapsible`.
    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.collapsible = collapsible;
        self
    }

    /// Upstream `collapsedSize`: under it a lift or a key step collapses. Default 40.
    pub fn collapsed_size(mut self, size: f32) -> Self {
        self.collapsed_size = Some(size);
        self
    }

    /// Upstream `shrinkOrder`. Upstream reads it nowhere; kept for parity.
    pub fn shrink_order(mut self, order: i32) -> Self {
        self.shrink_order = Some(order);
        self
    }

    /// Upstream `containerRef`.
    pub fn container(mut self, basis: ResizableBasis) -> Self {
        self.container = Some(basis);
        self
    }

    /// Upstream `direction`.
    pub fn direction(mut self, direction: ResizeDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Upstream `autoSaveId`: the size and collapse state persist under it.
    pub fn auto_save_id(mut self, id: impl Into<SharedString>) -> Self {
        self.auto_save_id = Some(id.into());
        self
    }

    /// Upstream `defaultIsCollapsed`; a persisted entry wins over it.
    pub fn default_is_collapsed(mut self, collapsed: bool) -> Self {
        self.default_is_collapsed = Some(collapsed);
        self
    }

    /// Upstream `isCollapsed`: controlled; every change reports instead.
    pub fn is_collapsed(mut self, collapsed: bool) -> Self {
        self.is_collapsed = Some(collapsed);
        self
    }

    /// Upstream `onSizeChange`.
    pub fn on_size_change(mut self, f: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_size_change = Some(Rc::new(f));
        self
    }

    /// Upstream `onCollapseChange`: once per change of the collapse state.
    pub fn on_collapse_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_collapse_change = Some(Rc::new(f));
        self
    }

    fn options(&self) -> Options {
        Options {
            default_size: self.default_size,
            min_size: self.min_size,
            max_size: self.max_size,
            snaps: self.snaps.clone(),
            collapsible: self.collapsible,
            collapsed_size: self.collapsed_size,
            auto_save_id: self.auto_save_id.as_ref().map(|s| s.to_string()),
            default_is_collapsed: self.default_is_collapsed,
            is_collapsed: self.is_collapsed,
            has_container: self.container.is_some(),
        }
    }
}

/// How a handle drives a region.
pub(crate) struct HookDriver {
    controller: Entity<Controller>,
    container: Option<ResizableBasis>,
    direction: ResizeDirection,
    on_size_change: Option<SizeFn>,
    on_collapse_change: Option<CollapseFn>,
}

impl HookDriver {
    fn basis(&self, window: &Window) -> Basis {
        read_basis(self.direction, self.container.as_ref(), window)
    }

    fn dispatch(&self, reports: Vec<Report>, window: &mut Window, cx: &mut App) {
        for report in reports {
            match report {
                Report::Size(size) => {
                    if let Some(f) = &self.on_size_change {
                        f(size, window, cx);
                    }
                }
                Report::Collapsed(collapsed) => {
                    if let Some(f) = &self.on_collapse_change {
                        f(collapsed, window, cx);
                    }
                }
            }
        }
        window.refresh();
    }
}

fn read_basis(
    direction: ResizeDirection,
    container: Option<&ResizableBasis>,
    window: &Window,
) -> Basis {
    let along = |size: gpui::Size<Pixels>| -> f32 {
        f32::from(match direction {
            ResizeDirection::Horizontal => size.width,
            ResizeDirection::Vertical => size.height,
        })
    };
    Basis {
        container: container.and_then(|c| c.bounds()).map(|b| along(b.size)),
        viewport: along(window.viewport_size()),
    }
}

/// A hand-built region (upstream's hand-built `ResizableProps`): callbacks for each
/// gesture step.
#[derive(Default)]
pub(crate) struct CustomDriver {
    pub on_start: Option<StepFn>,
    pub on_move: Option<SizeFn>,
    pub on_end: Option<StepFn>,
    pub on_cancel: Option<StepFn>,
}

#[derive(Clone)]
pub(crate) enum Driver {
    Hook(Rc<HookDriver>),
    Custom(Rc<CustomDriver>),
}

/// Upstream `ResizableProps`: what a `resizable` slot takes.
#[derive(Clone)]
pub struct ResizableProps {
    pub size: f32,
    pub is_collapsed: bool,
    pub min_size_px: f32,
    pub max_size_px: f32,
    pub snaps: Rc<[f32]>,
    pub collapsed_size: f32,
    pub collapsible: bool,
    /// The hook's axis, so a handle can flag a mismatch.
    pub direction: Option<ResizeDirection>,
    pub(crate) effective_max: Option<f32>,
    pub(crate) driver: Driver,
}

impl ResizableProps {
    /// A hand-built region: the handle reports its gesture through the callbacks, and
    /// toggles collapse with a resize to the minimum.
    pub fn custom(size: f32, min_size_px: f32, max_size_px: f32) -> Self {
        ResizableProps {
            size,
            is_collapsed: false,
            min_size_px,
            max_size_px,
            snaps: Rc::from(Vec::new()),
            collapsed_size: 0.,
            collapsible: false,
            direction: None,
            effective_max: None,
            driver: Driver::Custom(Rc::new(CustomDriver::default())),
        }
    }

    /// For a hand-built region: a gesture begins.
    pub fn on_resize_start(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.custom_driver(|d| d.on_start = Some(Rc::new(f)));
        self
    }

    /// For a hand-built region: the gesture moved by a delta from its start.
    pub fn on_resize_move(mut self, f: impl Fn(f32, &mut Window, &mut App) + 'static) -> Self {
        self.custom_driver(|d| d.on_move = Some(Rc::new(f)));
        self
    }

    /// For a hand-built region: the gesture ended.
    pub fn on_resize_end(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.custom_driver(|d| d.on_end = Some(Rc::new(f)));
        self
    }

    /// For a hand-built region: the gesture ended without completing.
    pub fn on_resize_cancel(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.custom_driver(|d| d.on_cancel = Some(Rc::new(f)));
        self
    }

    fn custom_driver(&mut self, f: impl FnOnce(&mut CustomDriver)) {
        if let Driver::Custom(driver) = &mut self.driver {
            let mut next = CustomDriver {
                on_start: driver.on_start.clone(),
                on_move: driver.on_move.clone(),
                on_end: driver.on_end.clone(),
                on_cancel: driver.on_cancel.clone(),
            };
            f(&mut next);
            *driver = Rc::new(next);
        }
    }

    /// The maximum the separator announces.
    pub(crate) fn announced_max(&self) -> Option<f32> {
        self.effective_max
            .or_else(|| self.max_size_px.is_finite().then_some(self.max_size_px))
    }

    /// The handle registers its focus with the region (LayoutPanel AR2).
    pub(crate) fn register_handle(&self, focus: &gpui::FocusHandle, cx: &mut App) {
        if let Driver::Hook(d) = &self.driver {
            let known = d.controller.read(cx).handle_focus.as_ref() == Some(focus);
            if !known {
                let focus = focus.clone();
                d.controller.update(cx, |c, _| c.handle_focus = Some(focus));
            }
        }
    }

    /// The handle that drives the region.
    pub(crate) fn handle_focus(&self, cx: &App) -> Option<gpui::FocusHandle> {
        match &self.driver {
            Driver::Hook(d) => d.controller.read(cx).handle_focus.clone(),
            Driver::Custom(_) => None,
        }
    }

    pub(crate) fn has_collapse_controls(&self) -> bool {
        matches!(self.driver, Driver::Hook(_))
    }

    /// A LayoutPanel renders the region's pane (LayoutPanel FR7): it lends its element,
    /// so the pane slides and a pointer's pull draws the rubber band. A hand-built region
    /// has no controller to lend it to, and never slides.
    pub(crate) fn lend_pane(&self, cx: &mut App) {
        if let Driver::Hook(d) = &self.driver
            && !d.controller.read(cx).pane_lent
        {
            d.controller.update(cx, |c, _| c.pane_lent = true);
        }
    }

    /// The lent pane's slide this frame (Resizable FR8, AST-105 FR9): on the theme's move
    /// curve over `--duration-medium`, or a lift's `springs.settle`. Under Reduce Motion
    /// with a coarse pointer an opening fades in and a closing lands at once; with a fine
    /// pointer both land at once. Asks for frames while it plays.
    pub(crate) fn pane_frame(&self, window: &mut Window, cx: &mut App) -> Option<PaneFrame> {
        let Driver::Hook(d) = &self.driver else {
            return None;
        };
        let (slide, held, open_size) = {
            let c = d.controller.read(cx);
            (c.slide.clone(), c.held, c.derived.size)
        };
        // A pointer holds a closing pane's slide: the edge stays under it.
        if let Some(shown) = held {
            return Some(PaneFrame {
                shown,
                to: 0.,
                held: open_size.max(shown),
                opacity: None,
                by_pointer: true,
                start: None,
                curve: None,
            });
        }
        let slide = slide?;
        let motion = LayoutMotion::read(window, cx);
        let now = cx.background_executor().now();
        let finish = |cx: &mut App| {
            d.controller.update(cx, |c, _| {
                c.slide = None;
                c.slide_shown = None;
            });
        };
        if !motion.animates() || (motion.cross_fade.is_some() && slide.to < slide.from) {
            finish(cx);
            return None;
        }
        let (progress, done) = slide_progress(&slide, &motion, now);
        if done {
            finish(cx);
            return None;
        }
        window.request_animation_frame();
        if motion.cross_fade.is_some() {
            return Some(PaneFrame {
                shown: slide.to,
                to: slide.to,
                held: slide.held,
                opacity: Some(progress),
                by_pointer: false,
                start: Some(slide.start),
                curve: None,
            });
        }
        let shown = (slide.from + (slide.to - slide.from) * progress).max(0.);
        d.controller.update(cx, |c, _| c.slide_shown = Some(shown));
        let curve = match &slide.release {
            Some((duration, ease)) => SlideCurve::Release {
                duration: *duration,
                ease: ease.clone(),
            },
            None => SlideCurve::Theme {
                duration: motion.duration,
                ease: motion.ease,
            },
        };
        Some(PaneFrame {
            shown,
            to: slide.to,
            held: slide.held,
            opacity: None,
            by_pointer: false,
            start: Some(slide.start),
            curve: Some(curve),
        })
    }

    /// While the lent pane slides open, how far its edge has yet to travel (px): the gap
    /// the content beside it, laid out at its final width, would leave until the edge
    /// arrives (AST-105 FR9). The content's trailing edge reaches over it. Nothing while it
    /// closes (the pane covers the content), while a pointer drives it, or under Reduce
    /// Motion, where nothing travels.
    pub(crate) fn opening_gap(&self, window: &Window, cx: &mut App) -> f32 {
        let Driver::Hook(d) = &self.driver else {
            return 0.;
        };
        let (slide, held) = {
            let c = d.controller.read(cx);
            (c.slide.clone(), c.held)
        };
        let Some(slide) = slide.filter(|s| held.is_none() && s.to > s.from) else {
            return 0.;
        };
        let motion = LayoutMotion::read(window, cx);
        if !motion.animates() || motion.cross_fade.is_some() {
            return 0.;
        }
        let (progress, done) = slide_progress(&slide, &motion, cx.background_executor().now());
        if done {
            return 0.;
        }
        let shown = slide.from + (slide.to - slide.from) * progress;
        (slide.to - shown).max(0.)
    }

    /// How many times a pointer has caught a slide of the region.
    pub(crate) fn catches(&self, cx: &App) -> u32 {
        match &self.driver {
            Driver::Hook(d) => d.controller.read(cx).catches,
            Driver::Custom(_) => 0,
        }
    }

    /// The handle's drawn travel past the size: a pull's rubber band, or its spring back
    /// on `springs.settle` (frames are asked for while it springs).
    pub(crate) fn handle_offset(&self, window: &mut Window, cx: &mut App) -> f32 {
        let Driver::Hook(d) = &self.driver else {
            return 0.;
        };
        let now = cx.background_executor().now();
        let (offset, moving) = d.controller.update(cx, |c, _| c.handle_offset(now));
        if moving {
            window.request_animation_frame();
        }
        if crate::animated_layout::LayoutMotion::read(window, cx).reduced {
            return 0.;
        }
        offset
    }

    /// A pointer presses the handle: the gesture begins, taking the edge where it shows.
    pub(crate) fn start_pointer(&self, window: &mut Window, cx: &mut App) {
        match &self.driver {
            Driver::Hook(d) => {
                let basis = d.basis(window);
                d.controller.update(cx, |c, cx| c.start(&basis, true, cx));
                window.refresh();
            }
            Driver::Custom(d) => {
                if let Some(f) = &d.on_start {
                    f(window, cx);
                }
            }
        }
    }

    pub(crate) fn start(&self, window: &mut Window, cx: &mut App) {
        match &self.driver {
            Driver::Hook(d) => {
                let basis = d.basis(window);
                d.controller.update(cx, |c, cx| c.start(&basis, false, cx));
            }
            Driver::Custom(d) => {
                if let Some(f) = &d.on_start {
                    f(window, cx);
                }
            }
        }
    }

    pub(crate) fn move_by(&self, delta: f32, window: &mut Window, cx: &mut App) {
        match &self.driver {
            Driver::Hook(d) => {
                let basis = d.basis(window);
                let reports = d
                    .controller
                    .update(cx, |c, cx| c.move_by(delta, &basis, cx));
                d.dispatch(reports, window, cx);
            }
            Driver::Custom(d) => {
                if let Some(f) = &d.on_move {
                    f(delta, window, cx);
                }
            }
        }
    }

    pub(crate) fn end(&self, velocity: f32, window: &mut Window, cx: &mut App) {
        match &self.driver {
            Driver::Hook(d) => {
                let basis = d.basis(window);
                let reports = d.controller.update(cx, |c, cx| c.end(velocity, &basis, cx));
                d.dispatch(reports, window, cx);
            }
            Driver::Custom(d) => {
                if let Some(f) = &d.on_end {
                    f(window, cx);
                }
            }
        }
    }

    pub(crate) fn cancel(&self, window: &mut Window, cx: &mut App) {
        match &self.driver {
            Driver::Hook(d) => {
                let basis = d.basis(window);
                d.controller.update(cx, |c, cx| c.cancel(&basis, cx));
                window.refresh();
            }
            Driver::Custom(d) => {
                if let Some(f) = &d.on_cancel {
                    f(window, cx);
                }
            }
        }
    }

    /// The region's own collapse and expand (APG window splitter's Enter).
    pub(crate) fn toggle(&self, window: &mut Window, cx: &mut App) {
        if let Driver::Hook(d) = &self.driver {
            let basis = d.basis(window);
            let reports = d.controller.update(cx, |c, cx| {
                if c.derived.is_collapsed {
                    c.expand(&basis, cx)
                } else {
                    c.collapse(&basis, cx)
                }
            });
            d.dispatch(reports, window, cx);
        }
    }
}

impl From<ResizableRegion> for ResizableProps {
    fn from(region: ResizableRegion) -> Self {
        region.props
    }
}

impl From<&ResizableRegion> for ResizableProps {
    fn from(region: &ResizableRegion) -> Self {
        region.props.clone()
    }
}

/// What a lent pane shows this frame while it slides.
#[derive(Clone)]
pub(crate) struct PaneFrame {
    /// The extent it shows, px: its edge facing the handle moves with it.
    pub shown: f32,
    /// The extent it is laid out at when the slide ends.
    pub to: f32,
    /// The extent its content stays laid out at, clipped to `shown`.
    pub held: f32,
    /// Reduce Motion under a coarse pointer: an opening fades in, at its new size.
    pub opacity: Option<f32>,
    /// A pointer holds the edge (a caught closing slide): the layout follows it.
    pub by_pointer: bool,
    /// When the slide began, and the curve it plays: the content beside glides on the
    /// same one.
    pub start: Option<web_time::Instant>,
    pub curve: Option<SlideCurve>,
}

/// The curve a pane's slide plays, which the content beside it glides on too.
#[derive(Clone)]
pub(crate) enum SlideCurve {
    /// The theme's move curve over `--duration-medium`.
    Theme {
        duration: std::time::Duration,
        ease: astryx_theme::CubicBezier,
    },
    /// A lift's `springs.settle` from its release velocity.
    Release {
        duration: std::time::Duration,
        ease: crate::animated_layout::engine::GlideEase,
    },
}

impl SlideCurve {
    /// The progress `seconds` after the slide began, 0 to 1.
    pub fn at(&self, seconds: f32) -> f32 {
        let (SlideCurve::Theme { duration, .. } | SlideCurve::Release { duration, .. }) = self;
        let d = duration.as_secs_f32();
        let t = if d <= 0. { 1. } else { (seconds / d).min(1.) };
        match self {
            SlideCurve::Theme { ease, .. } => ease.ease(t),
            SlideCurve::Release { ease, .. } => ease(t),
        }
    }
}

/// Upstream `ResizableRegion`: one region's state and controls.
#[derive(Clone)]
pub struct ResizableRegion {
    /// The size in pixels, 0 while collapsed.
    pub size: f32,
    pub is_collapsed: bool,
    /// For a `resizable` slot.
    pub props: ResizableProps,
}

impl ResizableRegion {
    fn driver(&self) -> Option<&Rc<HookDriver>> {
        match &self.props.driver {
            Driver::Hook(d) => Some(d),
            Driver::Custom(_) => None,
        }
    }

    /// Collapses the region, if collapsible.
    pub fn collapse(&self, window: &mut Window, cx: &mut App) {
        if let Some(d) = self.driver() {
            let basis = d.basis(window);
            let reports = d.controller.update(cx, |c, cx| c.collapse(&basis, cx));
            d.dispatch(reports, window, cx);
        }
    }

    /// Expands it to the size it had before it collapsed.
    pub fn expand(&self, window: &mut Window, cx: &mut App) {
        if let Some(d) = self.driver() {
            let basis = d.basis(window);
            let reports = d.controller.update(cx, |c, cx| c.expand(&basis, cx));
            d.dispatch(reports, window, cx);
        }
    }

    /// Resizes it to `size` px, within its bounds.
    pub fn resize(&self, size: f32, window: &mut Window, cx: &mut App) {
        if let Some(d) = self.driver() {
            let basis = d.basis(window);
            let reports = d.controller.update(cx, |c, cx| c.resize(size, &basis, cx));
            d.dispatch(reports, window, cx);
        }
    }
}

/// Upstream `useResizable` with one region, keyed by `id`.
pub fn use_resizable(
    id: impl Into<ElementId>,
    config: UseResizable,
    window: &mut Window,
    cx: &mut App,
) -> ResizableRegion {
    let id = id.into();
    let basis = read_basis(config.direction, config.container.as_ref(), window);
    let options = config.options();
    let created = Cell::new(false);
    let controller = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-resizable").into()),
        cx,
        |_, cx| {
            created.set(true);
            Controller::new(options.clone(), &basis, cx)
        },
    );
    if !created.get() {
        controller.update(cx, |c, cx| {
            if !c.is_gesture_active() {
                c.update(options, &basis, cx);
            }
        });
    }
    let c = controller.read(cx);
    let derived = c.derived;
    let props = ResizableProps {
        size: c.shown_size(),
        is_collapsed: derived.is_collapsed,
        min_size_px: derived.min,
        max_size_px: derived.max,
        snaps: Rc::from(c.snaps().to_vec()),
        collapsed_size: c.collapsed_size(),
        collapsible: c.collapsible(),
        direction: Some(config.direction),
        effective_max: derived.effective_max,
        driver: Driver::Hook(Rc::new(HookDriver {
            controller: controller.clone(),
            container: config.container.clone(),
            direction: config.direction,
            on_size_change: config.on_size_change.clone(),
            on_collapse_change: config.on_collapse_change.clone(),
        })),
    };
    ResizableRegion {
        size: props.size,
        is_collapsed: props.is_collapsed,
        props,
    }
}

/// Upstream `UseResizableMultiConfig`: named regions with one axis, container and
/// storage key (each region persists under `{autoSaveId}:{name}`).
#[derive(Clone, Default)]
pub struct UseResizableRegions {
    direction: ResizeDirection,
    container: Option<ResizableBasis>,
    auto_save_id: Option<SharedString>,
    regions: Vec<(SharedString, UseResizable)>,
}

impl UseResizableRegions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn direction(mut self, direction: ResizeDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn container(mut self, basis: ResizableBasis) -> Self {
        self.container = Some(basis);
        self
    }

    pub fn auto_save_id(mut self, id: impl Into<SharedString>) -> Self {
        self.auto_save_id = Some(id.into());
        self
    }

    /// Upstream `regions`: a region's sizing; its axis, container and key come from here.
    pub fn region(mut self, name: impl Into<SharedString>, config: UseResizable) -> Self {
        self.regions.push((name.into(), config));
        self
    }
}

/// The named regions `use_resizable_regions` returns.
#[derive(Clone)]
pub struct ResizableRegions(Vec<(SharedString, ResizableRegion)>);

impl ResizableRegions {
    pub fn get(&self, name: &str) -> Option<&ResizableRegion> {
        self.0
            .iter()
            .find(|(n, _)| n.as_ref() == name)
            .map(|(_, r)| r)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SharedString, &ResizableRegion)> {
        self.0.iter().map(|(n, r)| (n, r))
    }
}

/// Upstream `useResizable` with named regions.
pub fn use_resizable_regions(
    id: impl Into<ElementId>,
    config: UseResizableRegions,
    window: &mut Window,
    cx: &mut App,
) -> ResizableRegions {
    let id = id.into();
    let regions = config
        .regions
        .into_iter()
        .map(|(name, mut region)| {
            region.direction = config.direction;
            region.container = config.container.clone();
            region.auto_save_id = config
                .auto_save_id
                .as_ref()
                .map(|key| SharedString::from(format!("{key}:{name}")));
            let region = use_resizable(
                ElementId::Name(format!("{id}-{name}").into()),
                region,
                window,
                cx,
            );
            (name, region)
        })
        .collect();
    ResizableRegions(regions)
}

/// A slide's progress at `now` (0 to 1) and whether it has ended: a lift's spring, or the
/// theme's move curve (opacity's linear time under Reduce Motion's cross-fade).
fn slide_progress(
    slide: &super::controller::PaneSlide,
    motion: &LayoutMotion,
    now: web_time::Instant,
) -> (f32, bool) {
    if motion.cross_fade.is_some() {
        let p = motion.progress(slide.start, now);
        return (p, p >= 1.);
    }
    match &slide.release {
        Some((duration, ease)) => {
            let t = now.saturating_duration_since(slide.start).as_secs_f32()
                / duration.as_secs_f32().max(1e-6);
            (ease(t.min(1.)), t >= 1.)
        }
        None => {
            let p = motion.progress(slide.start, now);
            (p, p >= 1.)
        }
    }
}
