//! Upstream `__tests__/VelocityTracker.test.ts`, `VelocityTracker.streams.test.ts`
//! (on upstream's recorded pointer streams, copied into `fixtures/`) and the
//! boundary case of `VelocityTracker.harness.test.ts`.

use serde_json::Value;

use crate::gestures::{Point, VelocityTracker, VelocityTrackerOptions};
use crate::motion::uikit::gestures::velocity as VELOCITY;

fn close(a: f64, b: f64, digits: i32) {
    assert!(
        (a - b).abs() < 10f64.powi(-digits) / 2.,
        "{a} is not close to {b}"
    );
}

/// Feeds `position(t)` sampled every `step` ms over [0, end].
fn trace(tracker: &mut VelocityTracker, position: impl Fn(f64) -> f64, end: f64, step: f64) {
    let mut t = 0.;
    while t <= end {
        tracker.add(position(t), 0., t);
        t += step;
    }
}

const STEADY: [(f64, f64); 21] = [
    (0., 840.),
    (57.702, 831.6666666666665),
    (73.904, 815.),
    (91.286, 789.6666666666665),
    (107.677, 773.),
    (125.123, 756.3333333333333),
    (141.298, 739.6666666666665),
    (157.709, 722.6666666666665),
    (174.631, 706.),
    (191.78, 689.3333333333333),
    (209.313, 672.6666666666666),
    (226.956, 655.6666666666666),
    (243.419, 639.),
    (260.223, 622.3333333333333),
    (277.866, 605.6666666666666),
    (294.774, 589.),
    (312.902, 572.),
    (333.081, 547.),
    (350.016, 530.3333333333333),
    (368.475, 513.3333333333333),
    (396.62, 488.33333333333326),
];
const STEADY_VELOCITIES: [f64; 19] = [
    -321.3, -1114.4, -1369.3, -1004.5, -970.3, -1031.5, -1025.7, -982.3, -967.6, -953.2, -973.3,
    -1008.3, -982.4, -952.8, -976.1, -998.0, -1187.9, -971.5, -914.4,
];
const ACCELERATING: [(f64, f64); 8] = [
    (0., 840.),
    (70.032, 838.6666666666665),
    (93.493, 827.6666666666665),
    (110.376, 790.),
    (131.223, 701.),
    (147.373, 640.),
    (164.226, 567.6666666666666),
    (181.326, 484.33333333333326),
];
const ACCELERATING_VELOCITIES: [f64; 6] = [-109.0, -821.3, -2638.6, -4170.8, -3880.1, -4408.3];

#[test]
fn reproduces_every_velocity_uikit_reported_on_the_recorded_streams() {
    for (samples, velocities) in [
        (&STEADY[..], &STEADY_VELOCITIES[..]),
        (&ACCELERATING[..], &ACCELERATING_VELOCITIES[..]),
    ] {
        let mut tracker = VelocityTracker::default();
        let mut errors = Vec::new();
        for (index, (time, y)) in samples.iter().enumerate() {
            tracker.add(0., *y, *time);
            if index >= 2 {
                errors.push((tracker.velocity(None).y - velocities[index - 2]).abs());
            }
        }
        assert_eq!(errors.len(), velocities.len());
        assert!(errors.iter().cloned().fold(0., f64::max) < 0.1);
    }
}

#[test]
fn keeps_the_last_moving_value_through_a_rest_while_the_finger_is_down() {
    let mut tracker = VelocityTracker::default();
    trace(&mut tracker, |t| t * 2., 160., 8.);
    close(tracker.velocity(None).x, 2000., 6);
}

#[test]
fn releases_at_0_after_a_rest_longer_than_the_stillness_interval_ast_100_fr2() {
    let mut tracker = VelocityTracker::new(VelocityTrackerOptions { stillness: 40. });
    trace(&mut tracker, |t| t * 2., 160., 8.);
    close(tracker.velocity(Some(168.)).x, 2000., 6);
    close(tracker.velocity(Some(200.)).x, 2000., 6);
    assert_eq!(tracker.velocity(Some(201.)), Point::ZERO);
    assert_eq!(tracker.scroll_velocity(201.), Point::ZERO);
}

#[test]
fn releases_at_0_with_fewer_than_two_moved_samples_ast_100() {
    let mut tracker = VelocityTracker::default();
    tracker.add(0., 0., 0.);
    tracker.add(10., 0., 8.);
    close(tracker.velocity(None).x, 1250., 6);
    assert_eq!(tracker.velocity(Some(10.)), Point::ZERO);
    assert_eq!(tracker.scroll_velocity(10.), Point::ZERO);
    tracker.add(20., 0., 16.);
    close(tracker.velocity(Some(18.)).x, 1250., 6);
}

#[test]
fn updates_only_on_moved_samples_inv10() {
    let mut tracker = VelocityTracker::default();
    tracker.add(0., 0., 0.);
    tracker.add(10., 0., 10.);
    tracker.add(20., 0., 20.);
    let moving = tracker.velocity(None);
    tracker.add(20., 0., 30.);
    assert_eq!(tracker.velocity(None), moving);
    tracker.add(30., 0., 40.);
    close(tracker.velocity(None).x, 0.2 * 500. + 0.8 * 1000., 6);
}

#[test]
fn blends_a_scrolling_surfaces_release_from_the_last_two_pan_values_inv10() {
    let mut tracker = VelocityTracker::default();
    tracker.add(0., 0., 0.);
    tracker.add(10., 0., 10.);
    tracker.add(20., 0., 20.);
    let previous = tracker.velocity(None).x;
    tracker.add(40., 0., 30.);
    let newest = tracker.velocity(None).x;
    close(
        tracker.scroll_velocity(32.).x,
        0.25 * newest + 0.75 * previous,
        6,
    );
}

#[test]
fn reproduces_uikits_scroll_release_on_recorded_uipickerview_swipes_inv10() {
    type Swipe = (&'static str, &'static [(f64, f64)], f64);
    let swipes: [Swipe; 4] = [
        (
            "200 pt in 50 ms",
            &[(0., 368.), (46.48, 334.67), (72.92, 201.33)],
            933.4,
        ),
        (
            "200 pt in 100 ms",
            &[
                (0., 368.),
                (49.2, 351.33),
                (65.27, 301.33),
                (88.47, 251.33),
                (104.68, 218.),
                (129.4, 168.),
            ],
            2114.,
        ),
        (
            "200 pt in 200 ms",
            &[
                (0., 368.),
                (46.01, 359.67),
                (63.5, 334.67),
                (81.36, 318.),
                (97.69, 301.33),
                (120.12, 276.33),
                (137.2, 259.67),
                (171.83, 226.33),
                (196.13, 201.33),
                (212.66, 184.67),
                (230.83, 168.),
            ],
            1016.2,
        ),
        (
            "a 90 pt flick",
            &[(0., 353.), (41.5, 335.), (62.02, 281.)],
            543.6,
        ),
    ];
    for (name, stream, release) in swipes {
        let mut tracker = VelocityTracker::default();
        for (time, y) in stream {
            tracker.add(0., *y, *time);
        }
        let lift = stream[stream.len() - 1].0;
        let offset = -tracker.scroll_velocity(lift).y;
        assert!(
            (offset - release).abs() / release < 0.001,
            "{name}: {offset}"
        );
    }
}

#[test]
fn reads_a_steady_1000_px_per_s_exactly() {
    let mut tracker = VelocityTracker::default();
    trace(&mut tracker, |t| t, 200., 8.);
    close(tracker.velocity(None).x, 1000., 6);
    close(tracker.scroll_velocity(200.).x, 1000., 6);
}

#[test]
fn lags_a_hard_acceleration_by_a_frame_as_uikit_does() {
    let mut tracker = VelocityTracker::default();
    trace(&mut tracker, |t| 0.02 * t * t, 200., 8.);
    close(tracker.velocity(None).x, 0.8 * 7520. + 0.2 * 7840., 3);
}

#[test]
fn handles_one_and_two_samples_and_repeated_timestamps() {
    let mut tracker = VelocityTracker::default();
    assert_eq!(tracker.velocity(None), Point::ZERO);
    tracker.add(0., 0., 0.);
    assert_eq!(tracker.velocity(None), Point::ZERO);
    tracker.add(10., 20., 10.);
    assert_eq!(tracker.velocity(None), Point::new(1000., 2000.));
    tracker.add(20., 20., 10.);
    close(tracker.velocity(None).x, 2000., 6);
    tracker.add(500., 500., 5.);
    let last = tracker.last_sample().expect("a sample");
    assert_eq!((last.x, last.y, last.time), (20., 20., 10.));
}

#[test]
fn forgets_everything_on_reset() {
    let mut tracker = VelocityTracker::default();
    trace(&mut tracker, |t| t, 80., 8.);
    tracker.reset();
    assert!(tracker.last_sample().is_none());
    assert_eq!(tracker.velocity(None), Point::ZERO);
    assert_eq!(tracker.velocity(Some(80.)), Point::ZERO);
}

// --- VelocityTracker.harness.test.ts -------------------------------------------------

/// Three moves, then a lift `ms` after the last one.
fn lift_after(ms: f64) -> (Vec<(f64, f64, f64)>, f64) {
    (
        vec![(100., 50., 0.), (116., 50., 16.), (132., 50., 32.)],
        32. + ms,
    )
}

fn core_speed((samples, lift): &(Vec<(f64, f64, f64)>, f64)) -> f64 {
    let mut tracker = VelocityTracker::default();
    for (x, y, t) in samples {
        tracker.add(*x, *y, *t);
    }
    let velocity = tracker.velocity(Some(*lift));
    velocity.x.hypot(velocity.y)
}

#[test]
fn straddles_the_stillness_reset_with_its_two_boundary_lifts() {
    assert!(core_speed(&lift_after(VELOCITY::STILLNESS_MS)) > 0.);
    assert_eq!(core_speed(&lift_after(VELOCITY::STILLNESS_MS + 1.)), 0.);
}

// --- VelocityTracker.streams.test.ts ------------------------------------------------

const FIXTURES: [(&str, &str); 18] = [
    (
        "android-chromium-fling.json",
        include_str!("fixtures/pointer-streams/android-chromium-fling.json"),
    ),
    (
        "android-chromium-slow.json",
        include_str!("fixtures/pointer-streams/android-chromium-slow.json"),
    ),
    (
        "android-chromium-stop-release.json",
        include_str!("fixtures/pointer-streams/android-chromium-stop-release.json"),
    ),
    (
        "android-firefox-fling.json",
        include_str!("fixtures/pointer-streams/android-firefox-fling.json"),
    ),
    (
        "android-firefox-slow.json",
        include_str!("fixtures/pointer-streams/android-firefox-slow.json"),
    ),
    (
        "android-firefox-stop-release.json",
        include_str!("fixtures/pointer-streams/android-firefox-stop-release.json"),
    ),
    (
        "desktop-chromium-mouse-fling.json",
        include_str!("fixtures/pointer-streams/desktop-chromium-mouse-fling.json"),
    ),
    (
        "desktop-chromium-mouse-slow.json",
        include_str!("fixtures/pointer-streams/desktop-chromium-mouse-slow.json"),
    ),
    (
        "desktop-chromium-mouse-stop-release.json",
        include_str!("fixtures/pointer-streams/desktop-chromium-mouse-stop-release.json"),
    ),
    (
        "desktop-chromium-pen-fling.json",
        include_str!("fixtures/pointer-streams/desktop-chromium-pen-fling.json"),
    ),
    (
        "desktop-chromium-pen-slow.json",
        include_str!("fixtures/pointer-streams/desktop-chromium-pen-slow.json"),
    ),
    (
        "desktop-chromium-pen-stop-release.json",
        include_str!("fixtures/pointer-streams/desktop-chromium-pen-stop-release.json"),
    ),
    (
        "desktop-chromium-touch-fling.json",
        include_str!("fixtures/pointer-streams/desktop-chromium-touch-fling.json"),
    ),
    (
        "desktop-chromium-touch-slow.json",
        include_str!("fixtures/pointer-streams/desktop-chromium-touch-slow.json"),
    ),
    (
        "desktop-chromium-touch-stop-release.json",
        include_str!("fixtures/pointer-streams/desktop-chromium-touch-stop-release.json"),
    ),
    (
        "ios-safari-fling.json",
        include_str!("fixtures/pointer-streams/ios-safari-fling.json"),
    ),
    (
        "ios-safari-slow.json",
        include_str!("fixtures/pointer-streams/ios-safari-slow.json"),
    ),
    (
        "ios-safari-stop-release.json",
        include_str!("fixtures/pointer-streams/ios-safari-stop-release.json"),
    ),
];

type Sample = (f64, f64, f64);

struct Fixture {
    file: &'static str,
    value: Value,
}

impl Fixture {
    fn events(&self, kind: &str) -> Vec<&Value> {
        self.value["events"]
            .as_array()
            .expect("events")
            .iter()
            .filter(|e| e[0] == kind)
            .collect()
    }
    fn gesture(&self) -> &str {
        self.value["gesture"].as_str().expect("a gesture")
    }
    fn shape_only(&self) -> bool {
        self.value["release"]["check"] == "shape"
    }
    fn at_last_move(&self) -> bool {
        self.value["release"]["at"] == "lastMove"
    }
    fn speed(&self) -> f64 {
        self.value["injected"]["speed"].as_f64().expect("a speed")
    }
}

fn sample(e: &Value) -> Sample {
    (
        e[1].as_f64().expect("a time"),
        e[2].as_f64().expect("an x"),
        e[3].as_f64().expect("a y"),
    )
}

fn fixtures() -> Vec<Fixture> {
    FIXTURES
        .iter()
        .map(|(file, text)| Fixture {
            file,
            value: serde_json::from_str(text).expect("a fixture"),
        })
        .collect()
}

/// What the pan feeds its tracker: the touch-down, then each pointermove.
fn pointer_samples(f: &Fixture) -> Vec<Sample> {
    f.events("pointerdown")
        .into_iter()
        .take(1)
        .chain(f.events("pointermove"))
        .map(sample)
        .collect()
}

/// The input the browser received: each pointermove's coalesced samples.
fn input_samples(f: &Fixture) -> Vec<Sample> {
    let mut samples = vec![sample(f.events("pointerdown")[0])];
    for event in f.events("pointermove") {
        let coalesced: Vec<Sample> = event
            .get(4)
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|s| {
                        (
                            s[0].as_f64().unwrap(),
                            s[1].as_f64().unwrap(),
                            s[2].as_f64().unwrap(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        if coalesced.is_empty() {
            samples.push(sample(event));
        } else {
            samples.extend(coalesced);
        }
    }
    samples
}

/// One pointermove per `every` input samples, at the latest of them.
fn busy_samples(f: &Fixture, every: usize) -> Vec<Sample> {
    let input = input_samples(f);
    let mut samples = vec![input[0]];
    let mut i = 1;
    while i < input.len() {
        samples.push(input[(i + every - 1).min(input.len() - 1)]);
        i += every;
    }
    samples
}

fn touch_samples(f: &Fixture) -> Vec<Sample> {
    f.events("touchstart")
        .into_iter()
        .chain(f.events("touchmove"))
        .map(sample)
        .collect()
}

fn release_time(f: &Fixture, lift: &str, moves: &[Sample]) -> f64 {
    if f.at_last_move() {
        moves[moves.len() - 1].0
    } else {
        f.events(lift)[0][1].as_f64().expect("a lift")
    }
}

fn replay(samples: &[Sample], release: f64, shift: f64) -> (Point, Point) {
    let mut tracker = VelocityTracker::default();
    for (time, x, y) in samples {
        tracker.add(*x, *y, time + shift);
    }
    (
        tracker.velocity(Some(release + shift)),
        tracker.scroll_velocity(release + shift),
    )
}

#[test]
fn has_a_slow_drag_a_fling_and_a_stop_then_release_for_every_platform_browser_and_pointer_type() {
    let sources = [
        "android-chromium",
        "android-firefox",
        "ios-safari",
        "desktop-chromium-mouse",
        "desktop-chromium-pen",
        "desktop-chromium-touch",
    ];
    let mut expected: Vec<String> = sources
        .iter()
        .flat_map(|s| ["fling", "slow", "stop-release"].map(|g| format!("{s}-{g}.json")))
        .collect();
    expected.sort();
    let mut files: Vec<String> = fixtures().iter().map(|f| f.file.to_string()).collect();
    files.sort();
    assert_eq!(files, expected);
    for f in fixtures() {
        assert_eq!(f.value["schema"], 1);
        assert!(
            f.value["caveat"]
                .as_str()
                .is_some_and(|c| c.contains("not a finger"))
        );
    }
}

#[test]
fn releases_each_stream_as_its_gesture_says() {
    for f in fixtures() {
        let samples = pointer_samples(&f);
        let release = release_time(&f, "pointerup", &samples);
        let (pan, scroll) = replay(&samples, release, 0.);
        if f.gesture() == "stop-release" {
            assert_eq!(pan, Point::ZERO, "{}", f.file);
            assert_eq!(scroll, Point::ZERO, "{}", f.file);
        } else if f.shape_only() {
            assert!(pan.y > 0., "{}", f.file);
            assert!(pan.x.abs() <= 0.1 * pan.y, "{}", f.file);
        } else {
            let speed = f.speed();
            assert!(
                (pan.y - speed).abs() <= 0.1 * speed,
                "{}: {}",
                f.file,
                pan.y
            );
            assert!(
                (scroll.y - speed).abs() <= 0.1 * speed,
                "{}: {}",
                f.file,
                scroll.y
            );
            assert!(pan.x.abs() <= 0.1 * speed, "{}", f.file);
        }
    }
}

#[test]
fn reads_the_same_velocity_from_the_touch_stream_within_10_percent() {
    for f in fixtures() {
        let touches = touch_samples(&f);
        if touches.len() <= 1 {
            continue;
        }
        let samples = pointer_samples(&f);
        let (pointer, _) = replay(&samples, release_time(&f, "pointerup", &samples), 0.);
        let (touch, _) = replay(&touches, release_time(&f, "touchend", &touches), 0.);
        let scale = pointer.y.abs().max(1.);
        assert!((touch.y - pointer.y).abs() <= 0.1 * scale, "{}", f.file);
    }
}

#[test]
fn reads_the_same_under_a_busy_main_thread_one_event_per_four_input_samples() {
    for f in fixtures() {
        if f.shape_only() {
            continue;
        }
        let samples = pointer_samples(&f);
        let release = release_time(&f, "pointerup", &samples);
        let (busy, _) = replay(&busy_samples(&f, 4), release, 0.);
        if f.gesture() == "stop-release" {
            assert_eq!(busy, Point::ZERO, "{}", f.file);
        } else {
            let speed = f.speed();
            assert!(
                (busy.y - speed).abs() <= 0.1 * speed,
                "{}: {}",
                f.file,
                busy.y
            );
        }
    }
}

#[test]
fn reads_each_events_own_time_a_shifted_clock_changes_nothing() {
    for f in fixtures() {
        let samples = pointer_samples(&f);
        let release = release_time(&f, "pointerup", &samples);
        let (pointer, _) = replay(&samples, release, 0.);
        let (shifted, _) = replay(&samples, release, 123_456.789);
        close(shifted.x, pointer.x, 6);
        close(shifted.y, pointer.y, 6);
    }
}
