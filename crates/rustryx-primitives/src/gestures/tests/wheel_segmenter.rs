//! Upstream `__tests__/wheelSegmenter.test.ts` (spec:AST-094 FR43, DEC-24).

use crate::gestures::wheel_segmenter::{
    WheelBoundary, WheelPhase, WheelSegment, WheelSegmenter, is_bunched,
};

use WheelBoundary::*;

/// Pushes `deltas` 16 ms apart from `from`, with a 180 ms finger pause.
fn feed(segmenter: &mut WheelSegmenter, deltas: &[f64], from: f64) -> Vec<WheelSegment> {
    deltas
        .iter()
        .enumerate()
        .map(|(i, delta)| segmenter.push(from + i as f64 * 16., *delta, 180., false))
        .collect()
}

#[test]
fn starts_a_gesture_at_the_first_wheel_and_again_after_a_finger_pause_of_idle_ms() {
    let mut s = WheelSegmenter::new();
    assert_eq!(s.push(0., 10., 180., false).boundary, Some(First));
    assert_eq!(s.push(16., 10., 180., false).boundary, None);
    assert_eq!(s.push(195., 10., 180., false).boundary, None);
    let after = s.push(375., 10., 180., false);
    assert_eq!(after.boundary, Some(Idle));
    assert_eq!(after.gesture, 2);
}

#[test]
fn reads_the_lift_where_a_run_of_shrinking_deltas_has_lost_a_fifth_of_its_size() {
    let seen = feed(
        &mut WheelSegmenter::new(),
        &[10., 10., 10., 10., 9.5, 9., 8.5, 8., 7.5],
        0.,
    );
    assert_eq!(
        seen.iter().map(|s| s.lift_now).collect::<Vec<_>>(),
        [false, false, false, false, false, false, false, true, false]
    );
    assert_eq!(
        seen.iter().map(|s| s.decaying).collect::<Vec<_>>(),
        [false, false, false, false, true, true, true, false, false]
    );
    assert_eq!(seen[7].phase, WheelPhase::Momentum);
    assert_eq!(seen[8].boundary, None);
}

#[test]
fn keeps_a_slow_finger_whose_deltas_shrink_and_grow_again_in_its_finger_phase() {
    let seen = feed(
        &mut WheelSegmenter::new(),
        &[10., 10., 10., 9., 8., 9., 10., 9., 8., 9.],
        0.,
    );
    assert!(!seen.iter().any(|s| s.lift_now));
    assert!(seen.iter().all(|s| s.phase == WheelPhase::Finger));
}

#[test]
fn starts_a_new_gesture_in_momentum_at_a_reversal_a_gap_or_a_jump() {
    for (name, next, at) in [(Reversal, -5., 16.), (Gap, 7., 50.), (Jump, 20., 16.)] {
        let mut s = WheelSegmenter::new();
        let seen = feed(&mut s, &[10., 10., 10., 10., 9.5, 9., 8.5, 8.], 0.);
        assert!(seen[7].lift_now);
        let segment = s.push(7. * 16. + at, next, 180., false);
        assert_eq!(segment.boundary, Some(name));
        assert_eq!(segment.phase, WheelPhase::Finger);
        assert_eq!(segment.gesture, 2);
    }
}

#[test]
fn starts_a_gesture_at_the_next_wheel_once_the_last_one_ended_on_its_own() {
    let mut s = WheelSegmenter::new();
    feed(&mut s, &[10., 10.], 0.);
    s.end();
    assert_eq!(s.push(40., 10., 180., false).boundary, Some(Idle));
}

/// Wheel times at `hz` from 1000 ms, through `stamp`, each moved by `jitter`.
fn frames(hz: f64, count: usize, whole: bool, jitter: f64) -> Vec<f64> {
    (0..count)
        .map(|i| {
            let time = 1000. + i as f64 * 1000. / hz + if i % 2 == 0 { jitter } else { -jitter };
            if whole { time.round() } else { time }
        })
        .collect()
}

/// Trackpad streams: frame rates, whole-millisecond stamps and jitter.
const STREAMS: [(&str, f64, bool, f64); 11] = [
    ("60 Hz", 60., false, 0.),
    ("120 Hz", 120., false, 0.),
    ("240 Hz", 240., false, 0.),
    ("240 Hz, 0.3 ms jitter", 240., false, 0.3),
    ("240 Hz, whole ms", 240., true, 0.),
    ("300 Hz", 300., false, 0.),
    ("300 Hz, whole ms", 300., true, 0.),
    ("360 Hz", 360., false, 0.),
    ("360 Hz, whole ms", 360., true, 0.),
    ("480 Hz, whole ms", 480., true, 0.),
    ("500 Hz, whole ms", 500., true, 0.),
];

#[test]
fn takes_no_frame_of_a_steady_stream_as_a_merge() {
    for (name, hz, whole, jitter) in STREAMS {
        let mut s = WheelSegmenter::new();
        let seen: Vec<_> = frames(hz, 40, whole, jitter)
            .into_iter()
            .map(|time| s.push(time, 10., 180., false))
            .collect();
        assert!(!seen.iter().any(|s| s.bunched), "{name}");
    }
}

#[test]
fn takes_a_wheel_1_ms_after_a_stalls_last_one_as_a_merge() {
    for (name, hz, whole, jitter) in STREAMS {
        let mut s = WheelSegmenter::new();
        let times = frames(hz, 20, whole, jitter);
        for time in &times {
            s.push(*time, 10., 180., false);
        }
        let resumed = times[times.len() - 1] + 50.;
        assert!(!s.push(resumed, 10., 180., false).bunched, "{name}");
        let merged = s.push(resumed + 1., 20., 180., false);
        assert_eq!(merged.boundary, None, "{name}");
        assert!(merged.bunched, "{name}");
    }
}

#[test]
fn takes_each_of_a_stalls_held_back_wheels_as_a_merge_when_the_page_receives_them_one_by_one() {
    let mut s = WheelSegmenter::new();
    let times = frames(120., 20, false, 0.);
    for time in &times {
        s.push(*time, 10., 180., false);
    }
    let resumed = times[times.len() - 1] + 50.;
    s.push(resumed, 10., 180., false);
    let held: Vec<bool> = [1., 2., 3., 4., 5.]
        .iter()
        .map(|ms| s.push(resumed + ms, 10., 180., false).bunched)
        .collect();
    assert_eq!(held, [true, true, true, true, true]);
}

#[test]
fn judges_a_wheel_by_its_own_interval_alone_a_frame_after_slow_wheels_is_a_frame() {
    let mut s = WheelSegmenter::new();
    for time in [0., 80., 160., 240., 320., 400.] {
        s.push(time, 2., 180., false);
    }
    assert!(!s.push(416., 10., 180., false).bunched);
    assert!(!s.push(418., 10., 180., false).bunched);
}

#[test]
fn takes_a_wheel_within_1_5_ms_of_the_one_before_as_a_merge_and_no_frame_up_to_500_hz() {
    assert!(is_bunched(0.));
    assert!(is_bunched(1.));
    assert!(is_bunched(1.5));
    assert!(!is_bunched(2.));
    assert!(!is_bunched(1000. / 480.));
    assert!(!is_bunched(1000. / 60.));
}

/// A fling: a trackpad's deltas growing as the fingers speed up.
const FLING: [f64; 5] = [8., 16., 24., 30., 32.];

/// The fling's momentum: `count` steps shrinking 5 % a frame from 30.
fn momentum(count: usize) -> Vec<f64> {
    (0..count)
        .map(|frame| 30. * 0.95f64.powi(frame as i32))
        .collect()
}

#[test]
fn keeps_a_flings_momentum_one_gesture_and_reads_new_fingers_as_a_jump() {
    for (name, hz, whole, jitter) in STREAMS {
        let mut s = WheelSegmenter::new();
        let glide = momentum(60);
        let mut sizes: Vec<f64> = FLING.to_vec();
        sizes.extend(&glide);
        sizes.push(glide[59] * 3.);
        sizes.push(glide[59] * 3.);
        let times = frames(hz, sizes.len(), whole, jitter);
        let seen: Vec<_> = sizes
            .iter()
            .enumerate()
            .map(|(i, size)| s.push(times[i], *size, 180., false))
            .collect();
        let lift = seen
            .iter()
            .position(|s| s.lift_now)
            .unwrap_or_else(|| panic!("{name}: no lift"));
        let glided = &seen[lift..FLING.len() + glide.len()];
        assert!(glided.iter().all(|s| s.boundary.is_none()), "{name}");
        assert!(
            seen[seen.len() - 2..]
                .iter()
                .any(|s| s.boundary == Some(Jump)),
            "{name}"
        );
    }
}

#[test]
fn reads_a_momentum_wheel_1_ms_after_the_one_before_as_one_stalls_merge_of_two_steps() {
    for (name, hz, whole, jitter) in STREAMS {
        let mut s = WheelSegmenter::new();
        let mut sizes: Vec<f64> = FLING.to_vec();
        sizes.extend(momentum(10));
        let times = frames(hz, sizes.len() + 2, whole, jitter);
        for (i, size) in sizes.iter().enumerate() {
            s.push(times[i], *size, 180., false);
        }
        let step = 30. * 0.95f64.powi(10);
        let last = times[sizes.len() - 1];
        let merged = s.push(last + 1., step + step * 0.95, 180., false);
        let next = s.push(times[sizes.len() + 1], step * 0.9, 180., false);
        assert_eq!(
            (merged.boundary, merged.phase, next.boundary, next.phase),
            (None, WheelPhase::Momentum, None, WheelPhase::Momentum),
            "{name}"
        );
    }
}

#[test]
fn still_starts_a_new_gesture_at_a_jump_up_against_the_decay_right_after_a_merged_momentum_wheel() {
    let mut s = WheelSegmenter::new();
    feed(&mut s, &[10., 10., 10., 10., 9.5, 9., 8.5, 8., 7.5], 0.);
    s.push(144., 3., 180., false);
    s.push(145., 2.8 + 2.6, 180., false);
    let fingers = s.push(161., 5., 180., false);
    assert_eq!((fingers.boundary, fingers.gesture), (Some(Jump), 2));
}

#[test]
fn reads_no_lift_from_the_steps_after_a_merged_finger_wheel() {
    let mut s = WheelSegmenter::new();
    let times = [
        0., 16., 32., 48., 64., 80., 81., 97., 113., 129., 145., 161.,
    ];
    let sizes = [4., 4., 4., 4., 4., 3.9, 8., 4.2, 4.1, 4., 3.9, 3.8];
    let seen: Vec<_> = times
        .iter()
        .zip(sizes)
        .map(|(time, size)| s.push(*time, size, 180., false))
        .collect();
    assert_eq!(
        (seen.iter().any(|s| s.lift_now), seen[7].decaying),
        (false, false)
    );
}

#[test]
fn reads_a_slowing_drags_lift_at_the_same_frame_whether_or_not_a_stall_merged_two_of_its_steps() {
    let mut sizes = vec![10., 10., 10., 10.];
    for frame in 1..=30 {
        sizes.push(10. * 0.99f64.powi(frame));
    }
    let times: Vec<f64> = (0..sizes.len()).map(|i| i as f64 * 16.).collect();
    let lift_time = |merge: Option<usize>| {
        let mut s = WheelSegmenter::new();
        for index in 0..sizes.len() {
            let mut size = sizes[index];
            let mut time = times[index];
            if Some(index) == merge {
                size += sizes[index + 1];
                time = times[index - 1] + 1.;
            } else if merge.is_some_and(|m| index == m + 1) {
                continue;
            }
            if s.push(time, size, 180., false).lift_now {
                return Some(times[index]);
            }
        }
        None
    };
    let control = lift_time(None);
    assert!(control.is_some());
    assert_eq!(lift_time(Some(10)), control);
}

/// Wheel times from the gaps between them, from 0 ms.
fn times_of(gaps: &[f64]) -> Vec<f64> {
    let mut times = vec![0.];
    for gap in gaps {
        times.push(times[times.len() - 1] + gap);
    }
    times
}

struct Flick {
    name: &'static str,
    deltas: &'static [f64],
    gaps: &'static [f64],
    late: usize,
}

const STALLED_FLICKS: [Flick; 2] = [
    Flick {
        name: "a late step, then a merge of the rest",
        deltas: &[
            12., 12., 12., 12., 12., 12., 12., 36., 12., 12., 11., 10., 17., 7., 11., 4., 6.,
        ],
        gaps: &[
            16., 27., 0., 12., 18., 94., 0., 10., 0., 58., 31., 0., 16., 0., 86., 0.,
        ],
        late: 15,
    },
    Flick {
        name: "a late merge of the next two steps",
        deltas: &[
            12., 12., 12., 12., 12., 12., 12., 12., 12., 12., 12., 12., 11., 10., 9., 8., 7., 6.,
            5., 7., 2., 1.,
        ],
        gaps: &[
            16., 25., 0., 28., 12., 17., 14., 17., 16., 16., 16., 16., 16., 16., 16., 17., 16.,
            52., 0., 12., 17.,
        ],
        late: 18,
    },
];

/// Pushes a recorded stream as trackpad wheels, or as a mouse wheel's.
fn replay(deltas: &[f64], gaps: &[f64], continuous: bool) -> Vec<WheelSegment> {
    let mut s = WheelSegmenter::new();
    let times = times_of(gaps);
    deltas
        .iter()
        .enumerate()
        .map(|(i, delta)| s.push(times[i], *delta, 180., continuous))
        .collect()
}

#[test]
fn keeps_a_stalled_flick_one_gesture() {
    for flick in &STALLED_FLICKS {
        let seen = replay(flick.deltas, flick.gaps, true);
        assert_eq!(
            seen.iter().filter(|s| s.lift_now).count(),
            1,
            "{}",
            flick.name
        );
        assert!(
            seen[1..].iter().all(|s| s.boundary.is_none()),
            "{}",
            flick.name
        );
        assert_eq!(
            seen[seen.len() - 1].phase,
            WheelPhase::Momentum,
            "{}",
            flick.name
        );
    }
}

#[test]
fn splits_the_same_stream_at_its_late_wheel_when_it_is_a_mouse_wheels() {
    for flick in &STALLED_FLICKS {
        let seen = replay(flick.deltas, flick.gaps, false);
        assert_eq!(seen[flick.late].boundary, Some(Gap), "{}", flick.name);
    }
}

/// A glide of 1 px less a frame from 10 px, the lift read at its 8 px wheel.
fn glide() -> WheelSegmenter {
    let mut s = WheelSegmenter::new();
    let seen = feed(&mut s, &[10., 10., 10., 10., 9.5, 9., 8.5, 8.], 0.);
    assert!(seen[7].lift_now);
    s
}

const LIFT_AT: f64 = 7. * 16.;

#[test]
fn keeps_a_trackpad_wheel_that_carries_the_next_two_steps_after_a_skipped_frame() {
    let segment = glide().push(LIFT_AT + 32., 15., 180., true);
    assert_eq!(
        (segment.boundary, segment.phase),
        (None, WheelPhase::Momentum)
    );
}

#[test]
fn still_starts_a_new_gesture_at_a_trackpad_wheel_one_frame_later_that_rises_against_the_decay() {
    assert_eq!(
        glide().push(LIFT_AT + 16., 15., 180., true).boundary,
        Some(Jump)
    );
}

#[test]
fn still_starts_a_new_gesture_at_a_trackpad_wheel_after_a_pause_that_no_sum_of_the_glide_explains()
{
    assert_eq!(
        glide().push(LIFT_AT + 60., 40., 180., true).boundary,
        Some(Gap)
    );
}

#[test]
fn keeps_a_late_trackpad_wheel_at_or_below_the_next_step_and_starts_a_new_gesture_at_the_next_one_that_grows()
 {
    let mut s = glide();
    assert_eq!(s.push(LIFT_AT + 120., 2., 180., true).boundary, None);
    let next = s.push(LIFT_AT + 136., 3., 180., true);
    assert_eq!(next.boundary, Some(Jump));
    assert_eq!(next.carried, 2.);
}

#[test]
fn still_starts_a_new_gesture_at_a_trackpad_wheel_after_a_pause_that_reverses() {
    assert_eq!(
        glide().push(LIFT_AT + 60., -7., 180., true).boundary,
        Some(Reversal)
    );
}

#[test]
fn starts_a_new_gesture_at_a_trackpad_wheel_that_grows_after_a_late_one_joined_the_glide_with_its_travel()
 {
    let mut s = glide();
    assert_eq!(s.push(LIFT_AT + 60., 7., 180., true).boundary, None);
    let next = s.push(LIFT_AT + 76., 8., 180., true);
    assert_eq!(next.boundary, Some(Jump));
    assert_eq!(next.carried, 7.);
}

#[test]
fn hands_over_only_the_late_wheel_just_before_a_wheel_that_grows_two_wheels_after_it_carries_nothing()
 {
    let mut s = glide();
    assert_eq!(s.push(LIFT_AT + 60., 7., 180., true).boundary, None);
    assert_eq!(s.push(LIFT_AT + 76., 6., 180., true).boundary, None);
    let grows = s.push(LIFT_AT + 92., 9., 180., true);
    assert_eq!(grows.boundary, Some(Jump));
    assert_eq!(grows.carried, 0.);
}

#[test]
fn keeps_a_whole_pixel_glide_one_flick_when_it_repeats_a_step_after_a_late_wheel_joined_it() {
    let mut s = WheelSegmenter::new();
    let seen: Vec<_> = [12., 12., 12., 12., 11., 10., 9., 8., 8.]
        .iter()
        .enumerate()
        .map(|(i, delta)| s.push(i as f64 * 16., *delta, 180., true))
        .collect();
    assert_eq!(seen.iter().filter(|s| s.lift_now).count(), 1);
    let late = 8. * 16. + 70.;
    let boundaries: Vec<_> = [7., 7., 6., 6., 5.]
        .iter()
        .enumerate()
        .map(|(i, delta)| s.push(late + i as f64 * 16., *delta, 180., true).boundary)
        .collect();
    assert_eq!(boundaries, [None, None, None, None, None]);
}

/// A glide that ends at 7 px a frame, the lift read at its 8 px wheel.
fn glide_to_seven() -> WheelSegmenter {
    let mut s = WheelSegmenter::new();
    let seen: Vec<_> = [12., 12., 12., 12., 11., 10., 9., 8., 7.]
        .iter()
        .enumerate()
        .map(|(i, delta)| s.push(i as f64 * 16., *delta, 180., true))
        .collect();
    assert_eq!(seen.iter().position(|s| s.lift_now), Some(7));
    s
}

#[test]
fn hands_a_trackpad_wheel_after_a_glide_that_ends_at_7_px_to_the_next_wheel_which_grows_into_a_new_gesture()
 {
    for (size, after) in [(20., 100.), (25., 110.), (30., 120.)] {
        let mut s = glide_to_seven();
        let time = 8. * 16. + after;
        assert_eq!(s.push(time, size, 180., true).boundary, None, "{size}");
        let next = s.push(time + 16., size, 180., true);
        assert_eq!(next.boundary, Some(Jump), "{size}");
        assert_eq!(next.carried, size, "{size}");
    }
}

#[test]
fn hands_nothing_to_a_new_gesture_that_starts_late_after_a_late_wheel() {
    let mut s = glide_to_seven();
    let time = 8. * 16. + 100.;
    assert_eq!(s.push(time, 30., 180., true).boundary, None);
    let next = s.push(time + 60., 30., 180., true);
    assert_eq!(next.boundary, Some(Gap));
    assert_eq!(next.carried, 0.);
}

#[test]
fn hands_a_resumed_swipes_first_two_wheels_to_the_new_gesture_when_they_arrive_merged() {
    let mut s = glide_to_seven();
    let time = 8. * 16. + 100.;
    assert_eq!(s.push(time, 30., 180., true).boundary, None);
    assert_eq!(s.push(time + 0.5, 30., 180., true).boundary, None);
    let next = s.push(time + 16., 30., 180., true);
    assert_eq!(next.boundary, Some(Jump));
    assert_eq!(next.carried, 60.);
}

#[test]
fn starts_a_new_gesture_at_a_swipe_on_pace_right_after_a_whole_pixel_glide_after_an_earlier_gesture_showed_a_10_ms_gap()
 {
    let mut s = WheelSegmenter::new();
    for time in [0., 16., 26., 42.] {
        s.push(time, 12., 180., true);
    }
    let start = 342.;
    let deltas = [12., 12., 12., 12., 11., 10., 9., 8., 8., 7., 7.];
    let seen: Vec<_> = deltas
        .iter()
        .enumerate()
        .map(|(i, delta)| s.push(start + i as f64 * 16., *delta, 180., true))
        .collect();
    assert_eq!(seen[0].boundary, Some(Idle));
    assert_eq!(seen.iter().filter(|s| s.lift_now).count(), 1);
    assert_eq!(
        s.push(start + deltas.len() as f64 * 16., 14., 180., true)
            .boundary,
        Some(Jump)
    );
}

#[test]
fn starts_a_new_gesture_at_a_swipe_16_ms_after_a_stall_delivered_the_glides_held_wheels_3_ms_apart()
{
    let mut s = glide_to_seven();
    let mut time = 8. * 16.;
    for delta in [6., 6., 5., 5., 4., 4.] {
        time += 3.;
        s.push(time, delta, 180., true);
    }
    assert_eq!(s.push(time + 16., 12., 180., true).boundary, Some(Jump));
}

#[test]
fn keeps_a_16_ms_frame_when_a_gesture_starts_with_squeezed_wheels() {
    let mut s = WheelSegmenter::new();
    let mut times = vec![0., 3., 6.];
    times.extend((0..9).map(|i| 22. + f64::from(i) * 16.));
    let deltas = [12., 12., 12., 12., 12., 12., 11., 10., 9., 8., 7., 7.];
    let seen: Vec<_> = deltas
        .iter()
        .enumerate()
        .map(|(i, delta)| s.push(times[i], *delta, 180., true))
        .collect();
    assert_eq!(seen.iter().filter(|s| s.lift_now).count(), 1);
    assert_eq!(
        s.push(times[times.len() - 1] + 16., 14., 180., true)
            .boundary,
        Some(Jump)
    );
}

#[test]
fn keeps_a_27_px_trackpad_wheel_100_ms_after_a_glide_that_ends_at_7_px_in_the_flick() {
    let mut s = glide_to_seven();
    assert_eq!(s.push(8. * 16. + 100., 27., 180., true).boundary, None);
    assert_eq!(s.push(8. * 16. + 116., 3., 180., true).boundary, None);
}

#[test]
fn hands_a_20_px_trackpad_wheel_100_ms_after_a_whole_pixel_tail_to_the_next_wheel_which_grows() {
    let mut s = WheelSegmenter::new();
    let seen: Vec<_> = [12., 12., 12., 12., 11., 10., 9., 8., 8., 7., 7.]
        .iter()
        .enumerate()
        .map(|(i, delta)| s.push(i as f64 * 16., *delta, 180., true))
        .collect();
    assert_eq!(seen.iter().filter(|s| s.lift_now).count(), 1);
    assert_eq!(s.push(10. * 16. + 100., 20., 180., true).boundary, None);
    let next = s.push(10. * 16. + 116., 24., 180., true);
    assert_eq!(next.boundary, Some(Jump));
    assert_eq!(next.carried, 20.);
}

#[test]
fn keeps_a_trackpad_glide_that_goes_on_falling_after_a_late_wheel_joined_it() {
    let mut s = glide();
    let boundaries: Vec<_> = [60., 76., 92.]
        .iter()
        .enumerate()
        .map(|(i, time)| s.push(LIFT_AT + time, 7. - i as f64, 180., true).boundary)
        .collect();
    assert_eq!(boundaries, [None, None, None]);
}

struct MergedRun {
    name: &'static str,
    deltas: &'static [f64],
    gaps: &'static [f64],
    lift: Option<usize>,
    before: Option<usize>,
}

const MERGED_RUNS: [MergedRun; 2] = [
    MergedRun {
        name: "three shrinking steps merged, then a late step",
        deltas: &[
            12., 12., 12., 24., 12., 36., 12., 12., 12., 11., 27., 7., 6., 5., 4., 6.,
        ],
        gaps: &[
            25., 45., 0., 64., 0., 32., 0., 60., 31., 0., 14., 1., 65., 26., 0.,
        ],
        lift: Some(12),
        before: Some(14),
    },
    MergedRun {
        name: "late steps with the next ones merged after them",
        deltas: &[
            12., 12., 12., 12., 36., 12., 12., 12., 12., 50., 7., 11., 4., 5., 1.,
        ],
        gaps: &[
            32., 0., 85., 0., 12., 0., 17., 90., 0., 82., 0., 9., 0., 17.,
        ],
        lift: Some(13),
        before: None,
    },
];

#[test]
fn reads_a_stalled_flicks_lift_where_its_shrinking_run_shows_it() {
    for run in &MERGED_RUNS {
        let seen = replay(run.deltas, run.gaps, true);
        assert_eq!(
            seen.iter().position(|s| s.lift_now),
            run.lift,
            "{}",
            run.name
        );
        assert!(
            seen[1..].iter().all(|s| s.boundary.is_none()),
            "{}",
            run.name
        );
    }
}

#[test]
fn reads_the_same_streams_lift_as_before_when_it_is_a_mouse_wheels() {
    for run in &MERGED_RUNS {
        let seen = replay(run.deltas, run.gaps, false);
        assert_eq!(
            seen.iter().position(|s| s.lift_now),
            run.before,
            "{}",
            run.name
        );
    }
}

#[test]
fn keeps_a_mouse_wheels_rules_for_every_wheel_without_the_trackpad_signature() {
    assert_eq!(
        glide().push(LIFT_AT + 32., 15., 180., false).boundary,
        Some(Jump)
    );
    assert_eq!(
        glide().push(LIFT_AT + 60., 7., 180., false).boundary,
        Some(Gap)
    );
    assert_eq!(glide().push(LIFT_AT + 60., 7., 180., true).boundary, None);
    let mut resumed = glide();
    assert_eq!(resumed.push(LIFT_AT + 60., 7., 180., true).boundary, None);
    assert_eq!(resumed.push(LIFT_AT + 76., 7., 180., false).boundary, None);
}

#[test]
fn keeps_a_merge_that_a_stall_delivers_after_a_late_wheel_in_the_glide() {
    let mut s = glide();
    assert_eq!(s.push(LIFT_AT + 65., 7., 180., true).boundary, None);
    assert_eq!(s.push(LIFT_AT + 95., 15., 180., true).boundary, None);
}
