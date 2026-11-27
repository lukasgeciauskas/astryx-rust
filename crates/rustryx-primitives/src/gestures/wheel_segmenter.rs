//! A wheel stream's gestures and a trackpad's lift, upstream
//! `gestures/wheelSegmenter.ts` (DEC-24). The web reports no trackpad
//! contact or phase, and GPUI's platforms report them only in part, so the
//! host reads both from the deltas where the platform gives no phase.
//!
//! A trackpad gesture has a finger phase and, after a fling, a momentum
//! phase. The lift is the start of a run of shrinking deltas that has lost a
//! fifth of its size. In the finger phase a pause of `idle_ms` ends the
//! gesture; in momentum, a reversal, a jump against the decay or a 50 ms
//! pause starts a new one. A late or merged trackpad wheel that the glide's
//! decay explains stays in its flick.

/// Why an event starts a new gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelBoundary {
    First,
    Idle,
    Reversal,
    Jump,
    Gap,
}

/// The phase of a wheel gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelPhase {
    Finger,
    Momentum,
}

/// What one wheel is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelSegment {
    /// Why this event starts a new gesture, or `None` when it continues one.
    pub boundary: Option<WheelBoundary>,
    pub phase: WheelPhase,
    /// The gesture's number, from 1.
    pub gesture: u32,
    /// The fingers lifted here (momentum begins with it).
    pub lift_now: bool,
    /// This event shrank, so it may be momentum: hold it until known.
    pub decaying: bool,
    /// This event came so soon after the one before that it is a merge.
    pub bunched: bool,
    /// The signed travel of the late wheel before this one, handed over when
    /// this one grows into a new gesture; else 0.
    pub carried: f64,
}

const MOMENTUM_GAP_MS: f64 = 50.;
const BUNCHED_MS: f64 = 1.5;
const LIFT_RUN: u32 = 4;
const LIFT_DECAY: f64 = 0.8;
const FIT_PX: f64 = 1.;
const FIT_SHARE: f64 = 0.25;
const MAX_STEPS: u32 = 64;
const LATE_FIT_SHARE: f64 = 0.15;
const RISE_PX: f64 = 0.5;
const FRAME_MS: f64 = 16.;
const SLOWEST_FRAME_MS: f64 = 1000. / 60.;
const FIRST_PACES: usize = 3;

/// Whether a wheel `interval_ms` after the one before is one stall's merge.
pub fn is_bunched(interval_ms: f64) -> bool {
    interval_ms <= BUNCHED_MS
}

fn sign(value: f64) -> f64 {
    super::geometry::sign(value)
}

/// Upstream `WheelSegmenter`.
#[derive(Clone, Debug)]
pub struct WheelSegmenter {
    pub gesture: u32,
    pub phase: WheelPhase,
    last: f64,
    previous: f64,
    sign: f64,
    count: u32,
    run: u32,
    run_start: f64,
    ratio: f64,
    paces: Vec<f64>,
    slower: Vec<f64>,
    on_pace: f64,
    paced: f64,
    resumed: bool,
    joined: f64,
}

impl Default for WheelSegmenter {
    fn default() -> Self {
        Self {
            gesture: 0,
            phase: WheelPhase::Finger,
            last: f64::NEG_INFINITY,
            previous: 0.,
            sign: 0.,
            count: 0,
            run: 0,
            run_start: 0.,
            ratio: 1.,
            paces: Vec::new(),
            slower: Vec::new(),
            on_pace: f64::NEG_INFINITY,
            paced: 0.,
            resumed: false,
            joined: 0.,
        }
    }
}

impl WheelSegmenter {
    /// A segmenter before any wheel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads one wheel. `continuous` is whether it carries a trackpad's
    /// signature; only those can join a glide late.
    pub fn push(&mut self, time: f64, delta: f64, idle_ms: f64, continuous: bool) -> WheelSegment {
        let size = delta.abs();
        let sign = sign(delta);
        let gap = time - self.last;
        let bunched = self.count > 0 && is_bunched(gap);
        let frame = self.frame_ms();
        let mut boundary: Option<WheelBoundary> = None;
        let mut carried_steps: Option<u32> = None;
        if self.count == 0 {
            boundary = Some(if self.gesture == 0 {
                WheelBoundary::First
            } else {
                WheelBoundary::Idle
            });
        } else if self.phase == WheelPhase::Finger && gap >= idle_ms {
            boundary = Some(WheelBoundary::Idle);
        } else if self.phase == WheelPhase::Momentum {
            // A late wheel carries the steps of its whole stall.
            let frames = ((time - self.on_pace) / frame).round();
            let late = gap >= MOMENTUM_GAP_MS;
            let rises = !bunched && size >= 2. && size > self.previous * 1.3 + 0.5;
            if sign != 0. && self.sign != 0. && sign != self.sign {
                boundary = Some(WheelBoundary::Reversal);
            } else if late || rises {
                if continuous && sign == self.sign && (late || gap >= 1.5 * frame) {
                    carried_steps = self.glide_steps(size, frames.max(1.) as u32, LATE_FIT_SHARE);
                }
                if carried_steps.is_none() {
                    boundary = Some(if late {
                        WheelBoundary::Gap
                    } else {
                        WheelBoundary::Jump
                    });
                }
            } else if continuous
                && self.resumed
                && !bunched
                && size > FIT_PX
                && size > self.paced + RISE_PX
            {
                // After a late wheel, one that grows is the fingers'.
                boundary = Some(WheelBoundary::Jump);
            }
        }
        if carried_steps.is_none() && boundary.is_none() && bunched && continuous {
            if self.phase == WheelPhase::Momentum {
                carried_steps = self.glide_steps(size, MAX_STEPS, FIT_SHARE);
            } else if self.run > 0 {
                // In a shrinking run, the decay reads a merge only where it
                // explains it better than steady steps do.
                if let Some(decay) = self.glide_steps(size, MAX_STEPS, FIT_SHARE)
                    && decay > 1
                    && (size - self.glide_sum(decay)).abs() < self.steady_off(size)
                {
                    carried_steps = Some(decay);
                }
            }
        }
        let steps =
            carried_steps.unwrap_or(if boundary.is_none() && bunched && self.previous > 0. {
                ((size / self.previous).round() as u32).max(1)
            } else {
                1
            });
        let on_decay = carried_steps.is_some_and(|s| s > 1);
        let step = if on_decay {
            self.previous * self.ratio.powi(steps as i32)
        } else {
            size / f64::from(steps)
        };
        if boundary.is_some() {
            self.gesture += 1;
            self.phase = WheelPhase::Finger;
            self.count = 0;
            self.run = 0;
            self.ratio = 1.;
            self.resumed = false;
            self.paces.clear();
            self.slower.clear();
        }
        let carried = if boundary == Some(WheelBoundary::Jump) {
            self.joined
        } else {
            0.
        };
        let joined_late = boundary.is_none() && carried_steps.is_some() && !bunched;
        if joined_late {
            self.resumed = true;
        }
        self.joined = if joined_late {
            delta
        } else if bunched && boundary.is_none() && self.joined != 0. {
            self.joined + delta
        } else {
            0.
        };
        if !bunched {
            self.paced = size;
        }
        let mut lift_now = false;
        let mut decaying = false;
        if self.phase == WheelPhase::Finger && self.count >= 3 && size > 0. {
            decaying = if on_decay {
                self.ratio <= 0.995
            } else {
                step <= self.previous * 0.995 && step >= self.previous * 0.5
            };
            if decaying {
                if self.run == 0 {
                    self.run_start = self.previous;
                }
                self.run += 1;
            } else {
                self.run = 0;
            }
            if self.run >= LIFT_RUN && step <= self.run_start * LIFT_DECAY {
                self.phase = WheelPhase::Momentum;
                lift_now = true;
            }
        }
        if steps == 1 && self.count > 0 && self.previous > 0. && step > 0. {
            self.ratio = (step / self.previous).clamp(0.5, 1.);
        }
        if !bunched && boundary.is_none() && gap < MOMENTUM_GAP_MS {
            self.note_pace(gap, frame);
        }
        if boundary.is_some() || (!bunched && gap < 2. * frame) {
            self.on_pace = time;
        }
        self.count += 1;
        self.previous = step;
        if sign != 0. {
            self.sign = sign;
        }
        self.last = time;
        WheelSegment {
            boundary,
            phase: self.phase,
            gesture: self.gesture,
            lift_now,
            decaying: decaying && !lift_now,
            bunched: boundary.is_none() && bunched,
            carried,
        }
    }

    fn note_pace(&mut self, gap: f64, frame: f64) {
        let in_window = gap >= 0.5 * frame && gap <= 1.5 * frame;
        if self.paces.len() < FIRST_PACES || in_window {
            let keep = self.paces.len().saturating_sub(7);
            self.paces.drain(..keep);
            self.paces.push(gap);
            self.slower.clear();
            return;
        }
        if gap > 1.5 * frame {
            self.slower.push(gap);
        } else {
            self.slower.clear();
        }
        if self.slower.len() >= FIRST_PACES {
            self.paces = std::mem::take(&mut self.slower);
        }
    }

    fn frame_ms(&self) -> f64 {
        if self.paces.is_empty() {
            return FRAME_MS;
        }
        let mut sorted = self.paces.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let middle = sorted.len() >> 1;
        let median = if sorted.len() % 2 == 1 {
            sorted[middle]
        } else {
            (sorted[middle - 1] + sorted[middle]) / 2.
        };
        median.min(SLOWEST_FRAME_MS)
    }

    fn glide_sum(&self, count: u32) -> f64 {
        let mut next = self.previous * self.ratio;
        let mut sum = 0.;
        for _ in 0..count {
            sum += next;
            next *= self.ratio;
        }
        sum
    }

    fn steady_off(&self, size: f64) -> f64 {
        [self.run_start, self.previous]
            .into_iter()
            .filter(|step| *step > 0.)
            .map(|step| (size - (size / step).round().max(1.) * step).abs())
            .fold(f64::INFINITY, f64::min)
    }

    fn glide_steps(&self, size: f64, most: u32, share: f64) -> Option<u32> {
        if self.previous <= 0. || size <= 0. {
            return None;
        }
        let mut next = self.previous * self.ratio;
        if size <= next + FIT_PX {
            return Some(1);
        }
        let mut sum = next;
        for count in 2..=most.min(MAX_STEPS) {
            next *= self.ratio;
            sum += next;
            if (size - sum).abs() <= FIT_PX.max(sum * share) {
                return Some(count);
            }
        }
        None
    }

    /// The gesture ended on its own (its timer): the next event starts one.
    pub fn end(&mut self) {
        self.count = 0;
    }
}
