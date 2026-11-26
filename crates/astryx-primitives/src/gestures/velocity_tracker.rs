//! The one velocity tracker every continuous recognizer uses, upstream
//! `gestures/VelocityTracker.ts` (`architecture:interaction-modality`
//! INV10, `spec:AST-100` FR1, FR2): UIKit's filter over the newest moved
//! samples, and a scrolling surface's release blend. Times are
//! milliseconds, positions px, velocities px/s.
//!
//! One filter, iOS's measured blend, updated on each moved sample (a sample
//! at the newest position is not motion): a pan's velocity is 0.2 times the
//! newest interval's velocity plus 0.8 times the previous interval's; with
//! one interval so far it is that interval. A release reads 0 after a rest
//! longer than the stillness interval, and with fewer than two moved
//! samples. Feed it one sample per input event.

use super::types::{Point, TouchSample};
use crate::motion::uikit::gestures::velocity::{
    CURRENT_WEIGHT, SCROLL_RELEASE_WEIGHT, STILLNESS_MS,
};

/// A velocity, px/s.
pub type Velocity = Point;

fn blend(weight: f64, current: Velocity, previous: Velocity) -> Velocity {
    Velocity {
        x: weight * current.x + (1. - weight) * previous.x,
        y: weight * current.y + (1. - weight) * previous.y,
    }
}

/// Upstream `VelocityTrackerOptions`.
#[derive(Clone, Copy, Debug)]
pub struct VelocityTrackerOptions {
    /// A contact still for longer than this (ms) before its release releases
    /// at 0 (spec:AST-100 FR2).
    pub stillness: f64,
}

impl Default for VelocityTrackerOptions {
    fn default() -> Self {
        Self {
            stillness: STILLNESS_MS,
        }
    }
}

/// Upstream `VelocityTracker`.
#[derive(Clone, Debug)]
pub struct VelocityTracker {
    stillness: f64,
    newest: Option<TouchSample>,
    moved: Vec<TouchSample>,
    moves: usize,
    pan: Velocity,
    previous_pan: Velocity,
}

impl Default for VelocityTracker {
    fn default() -> Self {
        Self::new(VelocityTrackerOptions::default())
    }
}

impl VelocityTracker {
    /// A tracker with `options`.
    pub fn new(options: VelocityTrackerOptions) -> Self {
        Self {
            stillness: options.stillness,
            newest: None,
            moved: Vec::new(),
            moves: 0,
            pan: Velocity::ZERO,
            previous_pan: Velocity::ZERO,
        }
    }

    /// Forgets every sample (a new gesture).
    pub fn reset(&mut self) {
        *self = Self::new(VelocityTrackerOptions {
            stillness: self.stillness,
        });
    }

    /// Records a position at `time` ms. An older sample is ignored; a moved
    /// sample at the newest moved sample's time replaces it; a sample at the
    /// newest moved position is not motion.
    pub fn add(&mut self, x: f64, y: f64, time: f64) {
        self.add_sample(x, y, time, false);
    }

    /// As [`Self::add`], but the contact moved even where the value holds
    /// (a pinch's scale holding while both fingers move): the interval reads
    /// as zero velocity.
    pub fn add_motion(&mut self, x: f64, y: f64, time: f64) {
        self.add_sample(x, y, time, true);
    }

    fn add_sample(&mut self, x: f64, y: f64, time: f64, is_motion: bool) {
        if self.newest.is_some_and(|n| time < n.time) {
            return;
        }
        let sample = TouchSample { x, y, time };
        self.newest = Some(sample);
        let last = self.moved.last().copied();
        if last.is_some_and(|l| l.x == x && l.y == y) && !is_motion {
            return;
        }
        match last {
            Some(l) if l.time == time => {
                *self.moved.last_mut().expect("a moved sample") = sample;
            }
            _ => {
                self.moved.push(sample);
                if self.moved.len() > 3 {
                    self.moved.remove(0);
                }
                if last.is_some() {
                    self.moves += 1;
                    self.previous_pan = self.pan;
                }
            }
        }
        self.pan = pan_velocity(&self.moved);
    }

    /// Records every sample in order.
    pub fn add_samples(&mut self, samples: &[TouchSample]) {
        for sample in samples {
            self.add(sample.x, sample.y, sample.time);
        }
    }

    /// Re-bases the samples by an offset (a value set mid-gesture): the
    /// intervals, the moved-sample count and the velocity carry on.
    pub fn offset_by(&mut self, dx: f64, dy: f64) {
        let shift = |s: TouchSample| TouchSample {
            x: s.x + dx,
            y: s.y + dy,
            time: s.time,
        };
        self.newest = self.newest.map(shift);
        for sample in &mut self.moved {
            *sample = shift(*sample);
        }
    }

    /// Re-bases the samples by a factor (a pinch whose scale was set
    /// mid-gesture): positions and velocities scale with it.
    pub fn scale_by(&mut self, factor: f64) {
        let scale = |s: TouchSample| TouchSample {
            x: s.x * factor,
            y: s.y * factor,
            time: s.time,
        };
        self.newest = self.newest.map(scale);
        for sample in &mut self.moved {
            *sample = scale(*sample);
        }
        self.pan = Velocity::new(self.pan.x * factor, self.pan.y * factor);
        self.previous_pan =
            Velocity::new(self.previous_pan.x * factor, self.previous_pan.y * factor);
    }

    /// The newest sample, moved or not.
    pub fn last_sample(&self) -> Option<TouchSample> {
        self.newest
    }

    /// The time of the newest sample, if any.
    pub fn last_time(&self) -> Option<f64> {
        self.newest.map(|s| s.time)
    }

    /// The pan's velocity; with `release_time`, 0 after a rest longer than
    /// the stillness interval or with fewer than two moves.
    pub fn velocity(&self, release_time: Option<f64>) -> Velocity {
        match release_time {
            Some(time) if !self.releases(time) => Velocity::ZERO,
            _ => self.pan,
        }
    }

    /// A scrolling surface's release velocity: 0.25 times the newest pan
    /// value plus 0.75 times the previous one, under the same release rules.
    pub fn scroll_velocity(&self, release_time: f64) -> Velocity {
        if !self.releases(release_time) {
            return Velocity::ZERO;
        }
        blend(SCROLL_RELEASE_WEIGHT, self.pan, self.previous_pan)
    }

    fn releases(&self, release_time: f64) -> bool {
        self.moves >= 2
            && self
                .moved
                .last()
                .is_some_and(|last| release_time - last.time <= self.stillness)
    }
}

fn pan_velocity(moved: &[TouchSample]) -> Velocity {
    let n = moved.len();
    if n < 2 {
        return Velocity::ZERO;
    }
    let current = pair_velocity(moved[n - 2], moved[n - 1]);
    if n == 2 {
        return current;
    }
    blend(
        CURRENT_WEIGHT,
        current,
        pair_velocity(moved[n - 3], moved[n - 2]),
    )
}

fn pair_velocity(start: TouchSample, end: TouchSample) -> Velocity {
    let dt = end.time - start.time;
    if dt <= 0. {
        return Velocity::ZERO;
    }
    Velocity {
        x: (end.x - start.x) / dt * 1000.,
        y: (end.y - start.y) / dt * 1000.,
    }
}
