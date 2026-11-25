//! A pan's velocity, upstream `gestures/VelocityTracker.ts`: UIKit's
//! filter over the newest moved samples, and a scrolling surface's release
//! blend. Times are milliseconds, positions px, velocities px/s.

use crate::motion::uikit::velocity::{CURRENT_WEIGHT, SCROLL_RELEASE_WEIGHT, STILLNESS_MS};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Sample {
    x: f64,
    y: f64,
    time: f64,
}

/// A velocity, px/s.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity {
    pub x: f64,
    pub y: f64,
}

fn blend(weight: f64, current: Velocity, previous: Velocity) -> Velocity {
    Velocity {
        x: weight * current.x + (1. - weight) * previous.x,
        y: weight * current.y + (1. - weight) * previous.y,
    }
}

/// Upstream `VelocityTracker`.
#[derive(Clone, Debug)]
pub struct VelocityTracker {
    stillness: f64,
    newest: Option<Sample>,
    moved: Vec<Sample>,
    moves: usize,
    pan: Velocity,
    previous_pan: Velocity,
}

impl Default for VelocityTracker {
    fn default() -> Self {
        Self {
            stillness: STILLNESS_MS,
            newest: None,
            moved: Vec::new(),
            moves: 0,
            pan: Velocity::default(),
            previous_pan: Velocity::default(),
        }
    }
}

impl VelocityTracker {
    /// Forgets every sample (a new gesture).
    pub fn reset(&mut self) {
        *self = Self {
            stillness: self.stillness,
            ..Self::default()
        };
    }

    /// Records a position at `time` ms. An older sample is ignored; a sample
    /// at the newest moved position is not motion.
    pub fn add(&mut self, x: f64, y: f64, time: f64) {
        if self.newest.is_some_and(|n| time < n.time) {
            return;
        }
        let sample = Sample { x, y, time };
        self.newest = Some(sample);
        let last = self.moved.last().copied();
        if last.is_some_and(|l| l.x == x && l.y == y) {
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

    /// The time of the newest sample, if any.
    pub fn last_time(&self) -> Option<f64> {
        self.newest.map(|s| s.time)
    }

    /// The pan's velocity; with `release_time`, 0 after a rest longer than
    /// the stillness interval or with fewer than two moves.
    pub fn velocity(&self, release_time: Option<f64>) -> Velocity {
        match release_time {
            Some(time) if !self.releases(time) => Velocity::default(),
            _ => self.pan,
        }
    }

    /// A scrolling surface's release velocity: 0.25 times the newest pan
    /// value plus 0.75 times the previous one, under the same release rules.
    pub fn scroll_velocity(&self, release_time: f64) -> Velocity {
        if !self.releases(release_time) {
            return Velocity::default();
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

fn pan_velocity(moved: &[Sample]) -> Velocity {
    let n = moved.len();
    if n < 2 {
        return Velocity::default();
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

fn pair_velocity(start: Sample, end: Sample) -> Velocity {
    let dt = end.time - start.time;
    if dt <= 0. {
        return Velocity::default();
    }
    Velocity {
        x: (end.x - start.x) / dt * 1000.,
        y: (end.y - start.y) / dt * 1000.,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_steady_drag_releases_at_its_speed() {
        let mut tracker = VelocityTracker::default();
        for k in 0..6 {
            tracker.add(0., f64::from(k) * 10., f64::from(k) * 16.);
        }
        let v = tracker.scroll_velocity(80.);
        assert!((v.y - 625.).abs() < 1e-6, "{v:?}");
    }

    #[test]
    fn a_rest_before_the_lift_releases_at_zero() {
        let mut tracker = VelocityTracker::default();
        for k in 0..4 {
            tracker.add(0., f64::from(k) * 10., f64::from(k) * 16.);
        }
        assert_eq!(tracker.scroll_velocity(48. + 41.), Velocity::default());
        assert!(tracker.scroll_velocity(48. + 40.).y > 0.);
    }

    #[test]
    fn one_move_releases_at_zero() {
        let mut tracker = VelocityTracker::default();
        tracker.add(0., 0., 0.);
        tracker.add(0., 10., 16.);
        assert_eq!(tracker.scroll_velocity(16.), Velocity::default());
    }
}
