//! ChatVirtualizer's machinery (upstream `ChatVirtualizer`'s refs and passes): the scroll
//! position's reference frame (the end mode or a row anchor), the reader-or-machine ledger
//! that tells a scroll the reader made from one the list or the platform made, the
//! correction pass, the touch write-gate, the viewport-move restore, and the rendering
//! decisions (the arrived tail run, the pinned focused row, the busy mark).
//!
//! It runs against [`Dom`], the frame's content block as the browser would lay it out:
//! the spacers the pass writes, the mounted rows at their laid-out heights, and the scroll
//! position, clamped to the extent they describe as a browser clamps `scrollTop`. The
//! element (`virtualizer.rs`) builds one each frame from GPUI's layout and paints what it
//! ends with.

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::SharedString;

use super::virtualizer_model::*;

/// A scroll soon after a window resize, with no finger, is the platform's (upstream
/// `VIEWPORT_SETTLE_MS`: Safari's last rotation scroll lands inside this time). Reader input
/// ends it.
pub const VIEWPORT_SETTLE_MS: f64 = 500.;

/// The scroller's position and size, read together (upstream `ViewportRead`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportRead {
    /// The position (`scrollTop`).
    pub scroll_top: f64,
    /// The extent (`scrollHeight`).
    pub scroll_height: f64,
    /// The viewport's height (`clientHeight`).
    pub client_height: f64,
}

/// Whether a finger scroll ends in an elastic overscroll (upstream `isBouncing`), where a
/// write would snap the bounce onto the clamp.
pub fn is_bouncing(vp: ViewportRead) -> bool {
    vp.scroll_top < 0. || vp.scroll_top > (vp.scroll_height - vp.client_height).max(0.) + 1.
}

/// A child of the content block, in order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Child {
    /// The top spacer: everything above the first mounted row.
    Top,
    /// Between a pinned row above the window and the window.
    GapTop,
    /// Below the window, before a pinned row or the arrived tail run.
    GapBottom,
    /// Between a pinned row below the window and the arrived tail run.
    GapTail,
    /// The bottom spacer.
    Bottom,
    /// A mounted row, by its key.
    Row(SharedString),
}

/// The heights the pass writes (upstream's spacer refs and the content block's
/// `min-height` floor), px.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spacers {
    /// The top spacer.
    pub top: f64,
    /// The gap below a pinned row above the window.
    pub gap_top: f64,
    /// The gap below the window.
    pub gap_bottom: f64,
    /// The gap between a pinned row below the window and the arrived tail run.
    pub gap_tail: f64,
    /// The bottom spacer.
    pub bottom: f64,
    /// The extent's floor: the block is never shorter, so removing rows before the pass
    /// re-derives the geometry cannot collapse the extent and clamp the position.
    pub floor: f64,
}

/// The frame's content block and its scroller, as a layout engine would see them.
#[derive(Clone, Debug, Default)]
pub struct Dom {
    /// The block's children, in order.
    pub children: Vec<Child>,
    /// Each mounted row's laid-out height (its `offsetHeight`).
    pub heights: HashMap<SharedString, f64>,
    /// The spacers' heights as written.
    pub spacers: Spacers,
    /// The scroller's position (its `scrollTop`).
    pub scroll_top: f64,
    /// The scroller's viewport height (its `clientHeight`).
    pub client_height: f64,
    /// Where the block starts in the scroller's content.
    pub origin: f64,
    /// The scroller's content below the block.
    pub below: f64,
    /// Attach mode: the scroller is the caller's.
    pub attach: bool,
    /// Whether there is a scroller at all (attach mode can be pending).
    pub scroller: bool,
}

impl Dom {
    /// A child's height.
    pub fn child_height(&self, child: &Child) -> f64 {
        let s = &self.spacers;
        match child {
            Child::Top => s.top,
            Child::GapTop => s.gap_top,
            Child::GapBottom => s.gap_bottom,
            Child::GapTail => s.gap_tail,
            Child::Bottom => s.bottom,
            Child::Row(key) => self.heights.get(key).copied().unwrap_or(0.),
        }
    }

    /// The block's height: its children, never under the floor.
    pub fn block_height(&self) -> f64 {
        let sum: f64 = self.children.iter().map(|c| self.child_height(c)).sum();
        sum.max(self.spacers.floor)
    }

    /// The scroller's `scrollHeight`, never under its viewport.
    pub fn scroll_height(&self) -> f64 {
        (self.origin + self.block_height() + self.below).max(self.client_height)
    }

    /// Clamps the position into the scrollable range, as layout does.
    fn clamp(&mut self) {
        let max = (self.scroll_height() - self.client_height).max(0.);
        if !self.scroll_top.is_finite() {
            self.scroll_top = 0.;
        }
        self.scroll_top = self.scroll_top.clamp(0., max);
    }

    /// The scroller's position and size (upstream `readViewport`).
    pub fn read(&mut self) -> ViewportRead {
        self.clamp();
        ViewportRead {
            scroll_top: self.scroll_top,
            scroll_height: self.scroll_height(),
            client_height: self.client_height,
        }
    }

    /// The scroller's position after layout.
    pub fn scroll_top(&mut self) -> f64 {
        self.clamp();
        self.scroll_top
    }

    /// Writes the scroller's position; layout clamps it.
    pub fn set_scroll_top(&mut self, value: f64) {
        if value.is_finite() {
            self.scroll_top = value;
        }
        self.clamp();
    }

    /// Each mounted row with its painted top and bottom, relative to the viewport's top,
    /// in order.
    pub fn row_rects(&self) -> Vec<(SharedString, f64, f64)> {
        let mut y = self.origin - self.scroll_top;
        let mut rects = Vec::new();
        for child in &self.children {
            let h = self.child_height(child);
            if let Child::Row(key) = child {
                rects.push((key.clone(), y, y + h));
            }
            y += h;
        }
        rects
    }

    /// The mounted rows' keys, in order.
    pub fn rows(&self) -> impl Iterator<Item = &SharedString> {
        self.children.iter().filter_map(|c| match c {
            Child::Row(key) => Some(key),
            _ => None,
        })
    }
}

/// What the pass reads of the props this frame.
pub struct Props<'a> {
    /// The data's identity: upstream compares `data` by reference.
    pub data: &'a Rc<dyn Any>,
    /// Each row's key.
    pub keys: &'a [SharedString],
    /// Each row's item type, with `getItemType`.
    pub types: Option<&'a [SharedString]>,
    /// A static estimate; else `estimate_fn` prices each row.
    pub fixed: Option<f64>,
    /// An authoritative estimate per row.
    pub estimate_fn: Option<&'a dyn Fn(usize) -> f64>,
    /// Upstream `endThreshold`.
    pub end_threshold: f64,
    /// Upstream `overscanTop`.
    pub overscan_top: f64,
    /// Upstream `overscanBottom`.
    pub overscan_bottom: f64,
}

/// What the pass's skip check compares of the sizing props.
type Sizing = (u64, u64, Option<u64>, bool, bool);

impl Props<'_> {
    /// What the pass's skip check compares (upstream's `sizing`): the overscans and the
    /// estimate, and whether there is an item classifier (GPUI rebuilds closures each
    /// render, so presence stands for identity).
    fn sizing(&self) -> Sizing {
        (
            self.overscan_top.to_bits(),
            self.overscan_bottom.to_bits(),
            self.fixed.map(f64::to_bits),
            self.estimate_fn.is_some(),
            self.types.is_some(),
        )
    }
}

fn same_data(a: &Rc<dyn Any>, b: &Rc<dyn Any>) -> bool {
    std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b))
}

/// The tail messages the window has not reached (upstream's `arrival` state).
#[derive(Default)]
struct Arrival {
    data: Option<Rc<dyn Any>>,
    keys_of_data: Vec<SharedString>,
    win: Option<Win>,
    keys: HashSet<SharedString>,
}

/// The tasks a frame leaves for the next one (upstream's animation-frame callbacks).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameTasks {
    /// A clamped write's retry (`rafRetry`).
    pub retry: bool,
    /// The release of a landed declaration (`rafRelease`).
    pub release: bool,
    /// The restore after a platform's scroll for a viewport change (`restoreFrame`).
    pub restore: bool,
    /// The gesture's flush, waiting out an elastic overscroll (`flushRetry`).
    pub flush: bool,
}

impl FrameTasks {
    fn any(&self) -> bool {
        self.retry || self.release || self.restore || self.flush
    }
}

/// What a list keeps between frames (upstream's refs).
pub struct Machine {
    /// Each measured row's size, px.
    pub sizes: HashMap<SharedString, f64>,
    /// The rows' offsets as the last pass derived them.
    pub geo: Geo,
    /// What the position keeps.
    pub mode: Mode,
    buckets: HashMap<SharedString, Bucket>,
    type_by_key: HashMap<SharedString, SharedString>,
    /// The position the machinery last wrote: a scroll that lands there is its echo.
    prog_target: Option<f64>,
    /// Whether the reader moved the position last (the live-anchor refresh may then
    /// re-derive the anchor from the position).
    user_scrolled: bool,
    last_event_st: Option<f64>,
    last_user_event_t: f64,
    /// How far the reader has moved away from where the machinery placed them.
    user_away: f64,
    /// The geometry moved since the last correction.
    pub geo_changed: bool,
    /// A declared target is in flight: scroll events may not flip the mode.
    pub converging: bool,
    /// Prices hold still while a declared target is in flight.
    pub price_freeze: bool,
    // The page scroller's viewport changes.
    /// The scroller is the page's (it spans the window).
    pub is_page_viewport: bool,
    viewport_changed_at: f64,
    pass_viewport_height: Option<f64>,
    is_restore_armed: bool,
    // The touch write-gate.
    /// A finger's scroll owns the position.
    pub gesture_active: bool,
    gesture_moved: bool,
    /// What the top spacer absorbs while the gate holds, px.
    pub gesture_adj: f64,
    /// The rows rendered.
    pub win: Win,
    origin: f64,
    below: f64,
    /// The spacers as the last pass wrote them.
    pub spacers: Spacers,
    // Rendering.
    /// The row holding focus.
    pub focused_key: Option<SharedString>,
    pinned_key: Option<SharedString>,
    tail_key: Option<SharedString>,
    arrival: Arrival,
    rendered_keys: HashSet<SharedString>,
    news_keys: HashSet<SharedString>,
    pending_rendered: HashSet<SharedString>,
    pending_busy: bool,
    /// The content block is busy for the log (`aria-busy`).
    pub busy: bool,
    busy_clear: Option<u8>,
    passed: Option<(String, u64, Sizing)>,
    prev_data: Option<Rc<dyn Any>>,
    /// The tasks for the next frame.
    pub tasks: FrameTasks,
    /// A pass is owed this frame (a declaration, a flush, a size change).
    pub sync_requested: bool,
    /// The window changed: the rows must be rendered again.
    win_changed: bool,
    // For tests and readouts.
    /// The correction passes run.
    pub passes: usize,
    /// The position writes made.
    pub writes: usize,
}

impl Default for Machine {
    fn default() -> Self {
        Machine {
            sizes: HashMap::new(),
            geo: Geo::default(),
            mode: Mode::End { distance: 0. },
            buckets: HashMap::new(),
            type_by_key: HashMap::new(),
            prog_target: None,
            user_scrolled: false,
            last_event_st: None,
            last_user_event_t: 0.,
            user_away: 0.,
            geo_changed: false,
            converging: true,
            price_freeze: true,
            is_page_viewport: false,
            viewport_changed_at: f64::NEG_INFINITY,
            pass_viewport_height: None,
            is_restore_armed: false,
            gesture_active: false,
            gesture_moved: false,
            gesture_adj: 0.,
            win: Win::EMPTY,
            origin: 0.,
            below: 0.,
            spacers: Spacers::default(),
            focused_key: None,
            pinned_key: None,
            tail_key: None,
            arrival: Arrival::default(),
            rendered_keys: HashSet::new(),
            news_keys: HashSet::new(),
            pending_rendered: HashSet::new(),
            pending_busy: false,
            busy: false,
            busy_clear: None,
            passed: None,
            prev_data: None,
            tasks: FrameTasks::default(),
            sync_requested: false,
            win_changed: false,
            passes: 0,
            writes: 0,
        }
    }
}

impl Machine {
    fn bucket(&mut self, t: &SharedString) -> &mut Bucket {
        self.buckets.entry(t.clone()).or_default()
    }

    /// A row's measurement feeds the running averages (upstream `noteMeasured`).
    fn note_measured(&mut self, key: &SharedString, prev: Option<f64>, px: f64) {
        let ty = self.type_by_key.get(key).cloned();
        for t in [Some(SharedString::from(GLOBAL_BUCKET)), ty]
            .into_iter()
            .flatten()
        {
            let b = self.bucket(&t);
            match prev {
                None => {
                    b.sum += px;
                    b.count += 1.;
                }
                Some(prev) => b.sum += px - prev,
            }
        }
    }

    /// A row's size from a measurement after layout (upstream's resize-observer path):
    /// never 0, and only a change counts.
    pub fn observe_row(&mut self, key: &SharedString, px: f64) {
        let prev = self.sizes.get(key).copied();
        if px > 0. && prev != Some(px) {
            self.note_measured(key, prev, px);
            self.sizes.insert(key.clone(), px);
            self.geo_changed = true;
            self.sync_requested = true;
        }
    }

    /// The scroll container changed size (upstream's container entry): the end target and
    /// the window's coverage move with no row changing.
    pub fn observe_container(&mut self) {
        self.geo_changed = true;
        self.sync_requested = true;
    }

    /// Decides repricing once per pass, before the rebuild (upstream `repriceBuckets`).
    fn reprice_buckets(&mut self, p: &Props, now: f64) {
        let Some(base) = p.fixed else {
            // A function estimate is authoritative: nothing to reprice.
            return;
        };
        if self.converging || self.price_freeze {
            return;
        }
        // Sampled once per pass, and only while scrolling is quiet: a repricing moves every
        // unmeasured offset at once.
        let quiet = now - self.last_user_event_t > 250.;
        // Global first: it is the baseline every type bucket shrinks toward.
        let (global, changed) = settle(self.buckets.get_mut(GLOBAL_BUCKET), base, quiet);
        self.geo_changed |= changed;
        if p.types.is_some() {
            let names: Vec<SharedString> = self
                .buckets
                .keys()
                .filter(|k| k.as_ref() != GLOBAL_BUCKET)
                .cloned()
                .collect();
            for name in names {
                let (_, changed) = settle(self.buckets.get_mut(&name), global, quiet);
                self.geo_changed |= changed;
            }
        }
    }

    /// An unmeasured row's price (upstream `estOf`).
    fn est_of(&self, p: &Props, i: usize) -> f64 {
        if let Some(f) = p.estimate_fn {
            return f(i);
        }
        let global = effective_avg(self.buckets.get(GLOBAL_BUCKET), p.fixed.unwrap_or(0.));
        match p.types {
            Some(types) => effective_avg(self.buckets.get(&types[i]), global),
            None => global,
        }
    }

    /// The rows' offsets from the data (upstream `recompute`).
    fn recompute(&mut self, p: &Props) {
        let geo = recompute(p.keys.to_vec(), &self.sizes, |i| self.est_of(p, i));
        self.geo = geo;
    }

    /// Where the model's zero sits on screen (upstream `paintOrigin`).
    fn paint_origin(&self) -> f64 {
        self.origin + self.gesture_adj
    }

    fn compute_window(&self, vp: ViewportRead, desired_top: Option<f64>, p: &Props) -> Win {
        compute_window(
            &self.geo,
            desired_top.unwrap_or(vp.scroll_top),
            vp.client_height,
            self.paint_origin(),
            p.overscan_top,
            p.overscan_bottom,
        )
    }

    fn set_window_if_changed(&mut self, next: Win) {
        if next != self.win {
            self.geo_changed = true;
            self.win = next;
            self.win_changed = true;
        }
    }

    /// Sizes both spacers and the gaps so the mounted rows sit at their true place in the
    /// scroll range, and moves the floor to the extent the geometry describes (upstream
    /// `writeSpacers`).
    fn write_spacers(&mut self, dom: &mut Dom, w: Win) {
        let g = &self.geo;
        let empty = w.end < w.start as isize || g.offsets.is_empty();
        let offset_after = |i: usize| {
            if i + 1 < g.offsets.len() {
                g.offsets[i + 1]
            } else {
                g.total
            }
        };
        let pinned = if empty {
            None
        } else {
            self.pinned_key
                .as_ref()
                .and_then(|k| g.index.get(k).copied())
        };
        let pinned_above = pinned.is_some_and(|p| p < w.start);
        let pinned_below = pinned.is_some_and(|p| p as isize > w.end);
        let tail_index = if empty {
            None
        } else {
            self.tail_key.as_ref().and_then(|k| g.index.get(k).copied())
        };
        let tail = tail_index.filter(|t| *t as isize > w.end);
        let s = &mut self.spacers;
        if empty {
            s.top = 0.;
            s.gap_top = 0.;
            s.gap_bottom = 0.;
            s.gap_tail = 0.;
            s.bottom = g.total.max(0.);
            s.floor = g.total;
            dom.spacers = *s;
            return;
        }
        let end = w.end as usize;
        let first = if pinned_above {
            pinned.unwrap_or(w.start)
        } else {
            w.start
        };
        let last = if tail.is_some() {
            g.keys.len() - 1
        } else if pinned_below {
            pinned.unwrap_or(end)
        } else {
            end
        };
        // The gesture adjustment shifts the whole block without touching the position;
        // clamped at the very top of the list, where there is no spacer left to absorb it.
        let top = (g.offsets[first] + self.gesture_adj).max(0.);
        let below_end = offset_after(last);
        s.top = top;
        s.gap_top = match pinned {
            Some(p) if pinned_above => (g.offsets[w.start] - offset_after(p)).max(0.),
            _ => 0.,
        };
        s.gap_bottom = match (pinned, tail) {
            (Some(p), _) if pinned_below => (g.offsets[p] - offset_after(end)).max(0.),
            (_, Some(t)) => (g.offsets[t] - offset_after(end)).max(0.),
            _ => 0.,
        };
        s.gap_tail = match (pinned, tail) {
            (Some(p), Some(t)) if pinned_below => (g.offsets[t] - offset_after(p)).max(0.),
            _ => 0.,
        };
        s.bottom = (g.total - below_end).max(0.);
        // The extent these spacers describe: the model total, plus what the top spacer
        // carries beyond its model offset.
        s.floor = g.total + top - g.offsets[first];
        dom.spacers = *s;
    }

    /// The anchor for what is at a scroll offset (upstream `anchorAt`).
    fn anchor_at(&self, scroll_top: f64) -> Mode {
        anchor_at(&self.geo, scroll_top, self.paint_origin())
    }

    /// The anchor read off the painted rows rather than derived (upstream
    /// `anchorFromPaint`): the topmost row showing in the viewport.
    fn anchor_from_paint(&self, dom: &Dom, h: f64) -> Option<Mode> {
        let mut best: Option<(SharedString, f64)> = None;
        for (key, top, bottom) in dom.row_rects() {
            if !self.geo.index.contains_key(&key) || bottom <= 0. || top >= h {
                continue;
            }
            if best.as_ref().is_none_or(|(_, t)| top < *t) {
                best = Some((key, top));
            }
        }
        best.map(|(key, viewport_offset)| Mode::Anchor {
            key,
            viewport_offset,
        })
    }

    /// Where the block starts in the scroller and what follows it (upstream
    /// `measureOrigin`); in its own scroller, nothing either side.
    fn measure_origin(&mut self, dom: &Dom, vp: ViewportRead) {
        if !dom.attach {
            self.origin = 0.;
            self.below = 0.;
            return;
        }
        self.origin = dom.origin;
        self.below = (vp.scroll_height - self.origin - self.geo.total).max(0.);
    }

    /// The scroll position for a reference frame (upstream `targetFor`).
    fn target_for(&self, mode: &Mode, client_height: f64) -> Option<f64> {
        target_for(
            mode,
            &self.geo,
            client_height,
            self.paint_origin(),
            self.below,
        )
    }

    /// The correction pass (upstream `syncAndRestore`): spacers from the old geometry,
    /// measure, recompute, spacers from the new geometry, restore the position.
    pub fn sync_and_restore(&mut self, dom: &mut Dom, p: &Props, now: f64) {
        self.sync_requested = false;
        if !dom.scroller {
            return;
        }
        // Spacers first, from the pre-measure geometry.
        self.write_spacers(dom, self.win);
        let vp = dom.read();
        self.passes += 1;
        let entry = vp.scroll_top;
        if self.is_page_viewport
            && self
                .pass_viewport_height
                .is_some_and(|h| vp.client_height != h)
        {
            // A platform scroll write can still follow this viewport pass.
            self.is_restore_armed = true;
        }
        self.pass_viewport_height = Some(vp.client_height);
        self.measure_origin(dom, vp);
        // Refresh the anchor from the live position, only when the reader moved it last.
        if matches!(self.mode, Mode::Anchor { .. })
            && self.user_scrolled
            && !self.converging
            && !self.geo.keys.is_empty()
        {
            self.mode = self.anchor_at(entry);
        }
        // The first-mount backstop: only rows never seen are measured here; a row
        // measured while empty keeps its estimate, so it stays reachable.
        let unmeasured: Vec<(SharedString, f64)> = dom
            .rows()
            .filter(|k| !self.sizes.contains_key(*k))
            .map(|k| (k.clone(), dom.heights.get(k).copied().unwrap_or(0.)))
            .collect();
        for (key, px) in unmeasured {
            if px == 0. {
                continue;
            }
            self.note_measured(&key, None, px);
            self.sizes.insert(key, px);
            self.geo_changed = true;
        }
        // While the write-gate holds, the correction goes into the top spacer instead.
        let held_anchor =
            (self.gesture_active && !self.geo.keys.is_empty()).then(|| self.anchor_at(entry));
        self.reprice_buckets(p, now);
        self.recompute(p);
        if let Some(anchor) = held_anchor
            && let Some(t) = self.target_for(&anchor, vp.client_height)
        {
            self.gesture_adj -= t - entry;
        }
        self.write_spacers(dom, self.win);
        if self.converging {
            self.geo_changed = true;
        }
        let mut target = self.target_for(&self.mode, vp.client_height);
        // Dead-anchor fallback: the anchored key is not in the data; follow the end.
        if target.is_none() {
            self.converging = false;
            if !p.keys.is_empty() {
                self.mode = Mode::End { distance: 0. };
                self.user_away = 0.;
                self.geo_changed = true;
                target = self.target_for(&self.mode, vp.client_height);
            }
        }
        let mut wrote = None;
        if let Some(t) = target
            && self.geo_changed
            && !self.gesture_active
        {
            self.geo_changed = false;
            if (entry - t).abs() > 0.5 {
                dom.set_scroll_top(t);
                wrote = Some(t);
                self.writes += 1;
            }
        }
        // The one read after the writes: where the position actually is.
        let actual = dom.scroll_top();
        if let Some(w) = wrote {
            // The echo target is the actual landing value; a clamp retries next frame.
            self.prog_target = Some(actual);
            if (actual - w).abs() > 1. {
                self.geo_changed = true;
                self.tasks.retry = true;
            }
        }
        if self.converging
            && let Some(t) = target
            && !p.keys.is_empty()
            && (actual - t).abs() <= CONVERGE_EPSILON_PX
        {
            // A head change keeps measuring in: both halves of the protection lift on a
            // later pass that sees the settled numbers.
            if self.price_freeze {
                self.tasks.release = true;
            } else {
                self.converging = false;
            }
        }
        // The window covers the desired position; under the gate, the actual one.
        let desired = match target {
            Some(t) if !self.gesture_active => t,
            _ => actual,
        };
        self.set_window_if_changed(self.compute_window(vp, Some(desired), p));
        // A move during the pass is the machinery's: claim it.
        if actual != entry {
            self.prog_target = Some(actual);
        }
    }

    /// A declaration (upstream `declare`): the machine-driven regime, protected from
    /// mode flips until reached.
    pub fn declare(&mut self, mode: Mode) {
        self.mode = mode;
        self.price_freeze = true;
        self.user_scrolled = false;
        self.user_away = 0.;
        self.converging = true;
        self.geo_changed = true;
        self.sync_requested = true;
    }

    /// The finger's scroll is over: hand the position back (upstream `flushGesture`).
    pub fn flush_gesture(&mut self, dom: &mut Dom, p: &Props) {
        self.tasks.flush = false;
        if !dom.scroller || !self.gesture_active {
            return;
        }
        let vp = dom.read();
        if is_bouncing(vp) {
            self.tasks.flush = true;
            return;
        }
        self.gesture_active = false;
        let max = (vp.scroll_height - vp.client_height).max(0.);
        // Re-state the reference frame from the resting position: a real scroll re-judges
        // follow against the bottom edge; an echo under a resting finger keeps the mode.
        if !self.converging {
            let at_end = if self.gesture_moved {
                max - vp.scroll_top <= p.end_threshold
            } else {
                matches!(self.mode, Mode::End { .. })
            };
            self.mode = if at_end {
                Mode::End { distance: 0. }
            } else {
                self.anchor_from_paint(dom, vp.client_height)
                    .unwrap_or_else(|| self.anchor_at(vp.scroll_top))
            };
            self.user_away = 0.;
            // The fold is a machine repositioning.
            self.user_scrolled = false;
        }
        self.gesture_adj = 0.;
        self.geo_changed = true;
        self.sync_requested = true;
    }

    /// Reader input ends the time in which a scroll counts as the platform's (upstream
    /// `endViewportMove`).
    pub fn end_viewport_move(&mut self) {
        self.viewport_changed_at = f64::NEG_INFINITY;
        self.is_restore_armed = false;
    }

    /// A wheel: an intent signal that breaks convergence, though it does not unpin by
    /// itself (upstream `onWheel`).
    pub fn on_wheel(&mut self) {
        self.converging = false;
        self.price_freeze = false;
        self.end_viewport_move();
    }

    /// The window resized (upstream `onViewportResize`).
    pub fn on_viewport_resize(&mut self, now: f64) {
        self.viewport_changed_at = now;
        self.is_restore_armed = true;
    }

    /// A replaced scroller carries none of the old one's scroll state.
    pub fn on_scroller_replaced(&mut self) {
        self.last_event_st = None;
        self.user_away = 0.;
        self.prog_target = None;
        self.gesture_active = false;
        self.gesture_moved = false;
        self.gesture_adj = 0.;
        self.tasks.flush = false;
        self.viewport_changed_at = f64::NEG_INFINITY;
        self.pass_viewport_height = None;
        self.is_restore_armed = false;
        self.tasks.restore = false;
    }

    /// The scroller's position moved (upstream `onScroll`); `finger` is whether the
    /// scroll is a finger's.
    pub fn on_scroll(&mut self, dom: &mut Dom, p: &Props, now: f64, finger: bool) {
        if !dom.scroller {
            return;
        }
        let vp = dom.read();
        // Displacement is measured from where the machinery last wrote.
        let last_write = self.prog_target;
        let is_own_echo = last_write.is_some_and(|w| (vp.scroll_top - w).abs() <= 1.);
        let is_viewport_move = self.is_page_viewport
            && !finger
            && !is_own_echo
            && (self
                .pass_viewport_height
                .is_some_and(|h| vp.client_height != h)
                || (self.is_restore_armed && now - self.viewport_changed_at < VIEWPORT_SETTLE_MS));
        let is_echo = is_viewport_move || is_own_echo;
        if is_viewport_move {
            self.user_scrolled = false;
            if self.is_restore_armed && !self.tasks.restore {
                self.is_restore_armed = false;
                self.geo_changed = true;
                self.tasks.restore = true;
            }
        }
        if !is_echo {
            self.prog_target = None;
            self.user_scrolled = true;
            self.last_user_event_t = now;
        }
        let prev_event_st = self.last_event_st;
        self.last_event_st = Some(vp.scroll_top);
        if finger {
            if !self.gesture_active {
                // A finger's scroll begins: an intent signal, like a wheel.
                self.gesture_active = true;
                self.gesture_moved = false;
                self.converging = false;
                self.price_freeze = false;
            }
            // A new finger scroll during a held flush continues the gesture.
            self.tasks.flush = false;
        }
        if self.gesture_active && !is_echo {
            self.gesture_moved = true;
        }
        if !is_echo && !self.converging {
            let dist = vp.scroll_height - vp.client_height - vp.scroll_top;
            // The clamp onto a new, smaller bottom lands exactly on the bottom, where a
            // reader scrolling up never lands: it is not displacement.
            let landed_at_bottom = dist < 1.;
            if self.geo.keys.is_empty() {
                self.mode = Mode::End { distance: 0. };
                self.user_away = 0.;
            } else if matches!(self.mode, Mode::End { .. }) {
                // Disengage on the reader's own displacement, never on the absolute
                // distance: while following, the tail grows between corrections.
                let from = last_write.or(prev_event_st);
                if let Some(from) = from
                    && !landed_at_bottom
                {
                    self.user_away = (self.user_away + (from - vp.scroll_top)).max(0.);
                }
                if self.user_away > p.end_threshold {
                    self.mode = self.anchor_at(vp.scroll_top);
                    self.user_away = 0.;
                }
            } else if dist <= p.end_threshold {
                // Re-engage on the absolute distance: nothing pulls the viewport in the
                // anchor mode, so reaching the bottom is the reader's doing.
                self.mode = Mode::End { distance: 0. };
                self.user_away = 0.;
            } else {
                self.mode = self.anchor_at(vp.scroll_top);
                self.user_away = 0.;
            }
        }
        self.set_window_if_changed(self.compute_window(vp, None, p));
    }

    /// The frame's animation-frame callbacks (upstream's `requestAnimationFrame`s): the
    /// tasks the last frame left, run before this frame's commit.
    pub fn run_frame_tasks(&mut self, tasks: FrameTasks, dom: &mut Dom, p: &Props) {
        if let Some(n) = self.busy_clear {
            if n <= 1 {
                self.busy_clear = None;
                self.busy = false;
            } else {
                self.busy_clear = Some(n - 1);
            }
        }
        if tasks.restore || tasks.retry {
            self.sync_requested = true;
        }
        if tasks.release {
            self.converging = false;
            self.price_freeze = false;
            self.geo_changed = true;
            self.sync_requested = true;
        }
        if tasks.flush {
            self.flush_gesture(dom, p);
        }
    }

    /// Whether the next frame has work (a task, the busy mark's clearing, a held gesture).
    pub fn wants_frame(&self) -> bool {
        self.tasks.any() || self.busy_clear.is_some() || self.gesture_active
    }

    /// The render phase's data check (upstream's `prevData`): a new array invalidates the
    /// geometry, a tail append is news, and a head change is a convergence.
    pub fn note_data(&mut self, p: &Props) {
        let Some(prev) = &self.prev_data else {
            self.prev_data = Some(p.data.clone());
            return;
        };
        if same_data(prev, p.data) {
            return;
        }
        self.prev_data = Some(p.data.clone());
        self.geo_changed = true;
        // The rows past the last row the previous data had are this change's news, but
        // only for a tail append.
        let previous = &self.geo.index;
        let mut appended = HashSet::new();
        let mut is_tail_append = false;
        for key in p.keys.iter().rev() {
            if previous.contains_key(key) {
                is_tail_append = true;
                break;
            }
            appended.insert(key.clone());
        }
        // The first message of a new conversation is news too.
        let is_first_message = previous.is_empty() && appended.len() == 1;
        self.news_keys = if is_tail_append || is_first_message {
            appended
        } else {
            HashSet::new()
        };
        // A head change: rows arrived above everything placed, a convergence.
        if let Some(head) = p.keys.first()
            && let Some(old) = self.geo.keys.first()
            && old != head
        {
            self.converging = true;
            self.price_freeze = true;
        }
    }

    /// The rows this render mounts, in order (upstream's render): the window, a focused row
    /// kept outside it, and the arrived tail run.
    pub fn render_children(&mut self, p: &Props) -> Vec<Child> {
        let win = self.win;
        let keys = p.keys;
        let n = keys.len();
        let has_win = win.end >= win.start as isize;
        let index_of: HashMap<&SharedString, usize> =
            keys.iter().enumerate().map(|(i, k)| (k, i)).collect();
        if self.arrival.data.is_none() {
            self.arrival = Arrival {
                data: Some(p.data.clone()),
                keys_of_data: keys.to_vec(),
                win: Some(win),
                keys: HashSet::new(),
            };
        }
        let data_changed = !self
            .arrival
            .data
            .as_ref()
            .is_some_and(|d| same_data(d, p.data));
        if data_changed || self.arrival.win != Some(win) {
            let reached = |key: &SharedString| {
                has_win && index_of.get(key).map_or(-1, |i| *i as isize) <= win.end
            };
            let mut next: HashSet<SharedString> = self
                .arrival
                .keys
                .iter()
                .filter(|k| index_of.contains_key(k) && !reached(k))
                .cloned()
                .collect();
            if data_changed {
                let previous: HashSet<&SharedString> = self.arrival.keys_of_data.iter().collect();
                let mut run = Vec::new();
                let mut is_tail_append = false;
                for key in keys.iter().rev() {
                    if previous.contains(key) {
                        is_tail_append = true;
                        break;
                    }
                    run.push(key.clone());
                }
                // Only while the reader is away from the tail.
                let was_at_tail =
                    has_win && win.end >= self.arrival.keys_of_data.len() as isize - 1;
                if is_tail_append && !was_at_tail {
                    for key in run {
                        if !reached(&key) {
                            next.insert(key);
                        }
                    }
                }
            }
            // At most one window's worth of the newest arrivals stays mounted.
            let room = (win.end - win.start as isize + 1).max(1) as usize;
            if next.len() > room {
                let mut newest: Vec<SharedString> = next.into_iter().collect();
                newest.sort_by_key(|k| {
                    std::cmp::Reverse(index_of.get(k).map_or(-1, |i| *i as isize))
                });
                newest.truncate(room);
                next = newest.into_iter().collect();
            }
            self.arrival = Arrival {
                data: Some(p.data.clone()),
                keys_of_data: keys.to_vec(),
                win: Some(win),
                keys: next,
            };
        }
        let arrived = &self.arrival.keys;
        // A focused row outside the window stays mounted beside it.
        let mut pinned: isize = -1;
        if let Some(focused) = &self.focused_key
            && has_win
        {
            pinned = index_of.get(focused).map_or(-1, |i| *i as isize);
            if pinned >= win.start as isize && pinned <= win.end {
                pinned = -1;
            }
        }
        // The arrived tail run starts at the first arrived row past the window.
        let mut tail: isize = -1;
        if !arrived.is_empty() && has_win {
            let mut i = n as isize - 1;
            while i > win.end {
                if arrived.contains(&keys[i as usize]) {
                    tail = i;
                }
                i -= 1;
            }
        }
        self.tail_key = (tail >= 0).then(|| keys[tail as usize].clone());
        // A focused row inside the tail run is drawn by the run.
        if tail >= 0 && pinned >= tail {
            pinned = -1;
        }
        self.pinned_key = if pinned < 0 {
            None
        } else {
            self.focused_key.clone()
        };
        let mut children = vec![Child::Top];
        let row = |i: usize| Child::Row(keys[i].clone());
        if pinned >= 0 && pinned < win.start as isize {
            children.push(row(pinned as usize));
            children.push(Child::GapTop);
        }
        if has_win {
            let last = (win.end as usize).min(n.saturating_sub(1));
            for i in win.start..=last {
                if i < n {
                    children.push(row(i));
                }
            }
        }
        if pinned > win.end {
            children.push(Child::GapBottom);
            children.push(row(pinned as usize));
        }
        if tail >= 0 {
            children.push(if pinned > win.end {
                Child::GapTail
            } else {
                Child::GapBottom
            });
            for i in tail as usize..n {
                children.push(row(i));
            }
        }
        children.push(Child::Bottom);
        if let Some(types) = p.types {
            for child in &children {
                if let Child::Row(key) = child
                    && let Some(i) = index_of.get(key)
                {
                    self.type_by_key.insert(key.clone(), types[*i].clone());
                }
            }
        }
        // Busy when this render mounts a row that is not news.
        let rendered: HashSet<SharedString> = children
            .iter()
            .filter_map(|c| match c {
                Child::Row(k) => Some(k.clone()),
                _ => None,
            })
            .collect();
        self.pending_busy = rendered
            .iter()
            .any(|k| !self.rendered_keys.contains(k) && !self.news_keys.contains(k));
        self.pending_rendered = rendered;
        self.win_changed = false;
        children
    }

    /// The commit's layout effects (upstream's correction slot and
    /// `markHistoryMountsBusy`): the pass, unless the commit changed nothing it reads,
    /// then the busy mark. Returns whether the window changed, so the rows render again.
    pub fn commit(&mut self, dom: &mut Dom, p: &Props, now: f64, scroller_id: u64) -> bool {
        let keys: Vec<&str> = dom.rows().map(|k| k.as_ref()).collect();
        let passed = (keys.join("\n"), scroller_id, p.sizing());
        if self.geo_changed || self.sync_requested || self.passed.as_ref() != Some(&passed) {
            self.passed = Some(passed);
            self.sync_and_restore(dom, p, now);
        }
        // News is news once: a message that has mounted is history after.
        for key in &self.pending_rendered {
            self.news_keys.remove(key);
        }
        self.rendered_keys = std::mem::take(&mut self.pending_rendered);
        if self.pending_busy {
            self.busy = true;
            self.busy_clear = Some(2);
        }
        self.pending_busy = false;
        self.win_changed
    }
}
