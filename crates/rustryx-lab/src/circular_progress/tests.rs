use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{Context, Render, TestAppContext};

use super::*;

#[test]
fn the_value_is_held_within_zero_and_the_maximum() {
    // CircularProgress.test.tsx "clamps value to [0, max]".
    assert_eq!(ProgressValues::new(150., 100.).value, 100.);
    assert_eq!(ProgressValues::new(-10., 100.).value, 0.);
    let three = ProgressValues::new(3., 10.);
    assert_eq!((three.value, three.max, three.percentage), (3., 10., 0.3));
    // A NaN value or maximum is empty progress.
    let nan = ProgressValues::new(f64::NAN, 100.);
    assert_eq!((nan.value, nan.percentage), (0., 0.));
    let no_max = ProgressValues::new(50., f64::INFINITY);
    assert_eq!((no_max.value, no_max.max, no_max.percentage), (0., 0., 0.));
}

#[test]
fn the_default_label_is_a_whole_percent() {
    // "sets aria-valuetext from formatValueLabel".
    assert_eq!(default_format_value_label(50., 100.), "50%");
    assert_eq!(default_format_value_label(1., 3.), "33%");
    // Math.round rounds a half up.
    assert_eq!(default_format_value_label(0.5, 100.), "1%");
    assert_eq!(default_format_value_label(5., 0.), "0%");
}

#[test]
fn the_halves_and_cap_turn_with_the_value() {
    // "turns the fill's halves and leading cap from the value".
    let at = |p: f64| {
        let f = fill_turns(p);
        (f.right, f.right_shown, f.left, f.left_shown, f.cap, f.caps)
    };
    assert_eq!(at(0.), (0., false, 180., false, 0., false));
    assert_eq!(at(0.25), (90., true, 180., false, 90., true));
    assert_eq!(at(0.75), (180., true, 270., true, 270., true));
    assert_eq!(at(1.), (180., true, 360., true, 360., true));
}

#[test]
fn sizes_set_the_ring_geometry() {
    // "sets ring geometry from size": radius = (diameter - stroke) / 2.
    let (d, s) = CircularProgressSize::Md.geometry();
    assert_eq!((d, s, (d - s) / 2.), (48., 4., 22.));
    assert_eq!(CircularProgressSize::Sm.geometry(), (32., 3.));
    assert_eq!(CircularProgressSize::Lg.geometry(), (64., 5.));
}

type Seen = Rc<RefCell<Vec<(f64, f64)>>>;

struct Host(Rc<RefCell<f64>>, Seen);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let seen = self.1.clone();
        div().child(
            CircularProgress::new("p", "Progress")
                .value(*self.0.borrow())
                .value_label(true)
                .format_value_label(move |value, max| {
                    seen.borrow_mut().push((value, max));
                    format!("{value} of {max}").into()
                }),
        )
    }
}

#[gpui::test]
fn the_formatter_gets_the_clamped_value(cx: &mut TestAppContext) {
    // "uses custom formatValueLabel".
    cx.update(rustryx_theme::init);
    let (value, seen) = (Rc::new(RefCell::new(150.)), Seen::default());
    let (_, cx) = cx.add_window_view({
        let (value, seen) = (value.clone(), seen.clone());
        move |_, _| Host(value, seen)
    });
    cx.run_until_parked();
    assert_eq!(seen.borrow().last(), Some(&(100., 100.)));
}

#[test]
fn a_value_change_sweeps_from_where_the_arc_is_drawn() {
    // "animates a value change from where the arc is drawn": the first value draws at
    // rest; a change sweeps on the easing; an interruption starts from the point the
    // running sweep's own curve has reached.
    let linear = rustryx_theme::CubicBezier(0., 0., 1., 1.);
    let duration = Duration::from_millis(400);
    let t0 = web_time::Instant::now();
    let rest = next_sweep(None, Some(0.2), t0, duration, linear).unwrap();
    assert_eq!(rest.at(t0, duration, linear), (0.2, false));
    let sweep = next_sweep(Some(rest), Some(0.8), t0, duration, linear).unwrap();
    let (half, moving) = sweep.at(t0 + duration / 2, duration, linear);
    assert!(moving && (half - 0.5).abs() < 1e-3, "{half}");
    assert_eq!(sweep.at(t0 + duration, duration, linear), (0.8, false));
    let back = next_sweep(Some(sweep), Some(0.4), t0 + duration / 2, duration, linear).unwrap();
    assert!((back.from - 0.5).abs() < 1e-3 && back.to == 0.4);
    // The same value again keeps the running sweep.
    assert_eq!(
        next_sweep(Some(back), Some(0.4), t0 + duration, duration, linear),
        Some(back)
    );
}

#[test]
fn after_indeterminate_the_next_value_draws_at_once() {
    let linear = rustryx_theme::CubicBezier(0., 0., 1., 1.);
    let duration = Duration::from_millis(400);
    let t0 = web_time::Instant::now();
    let rest = next_sweep(None, Some(0.3), t0, duration, linear);
    assert_eq!(next_sweep(rest, None, t0, duration, linear), None);
    let after = next_sweep(None, Some(0.9), t0, duration, linear).unwrap();
    assert_eq!(after.at(t0, duration, linear), (0.9, false));
}
