//! Upstream `motion/coast.test.ts`.

use std::cell::RefCell;
use std::collections::BTreeMap;

use super::coast::*;
use super::uikit::scroll;
use super::{MotionProperty, solve_decay, solve_spring};

use MotionProperty::{X, Y};

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

/// Fixed presentation values, recording the times they are read at.
struct Fixed {
    values: BTreeMap<MotionProperty, f64>,
    reads: RefCell<Vec<Option<f64>>>,
}

fn fixed(values: &[(MotionProperty, f64)]) -> Fixed {
    Fixed {
        values: values.iter().copied().collect(),
        reads: RefCell::new(Vec::new()),
    }
}

impl Fixed {
    fn coast(
        &self,
        velocity: CoastVelocity,
        options: CoastOptions,
    ) -> super::animator::DecayCurves {
        coast_from(
            |property, at| {
                self.reads.borrow_mut().push(at);
                self.values.get(&property).copied().unwrap_or(0.)
            },
            1.,
            velocity,
            options,
        )
    }
}

#[test]
fn follows_the_coast_from_the_presentation_value_and_ends_where_it_projects() {
    let curves = fixed(&[(Y, 40.)]).coast(
        CoastVelocity {
            x: None,
            y: Some(-2400.),
        },
        CoastOptions::default(),
    );
    let y = curves.y.unwrap();
    let glide = solve_decay(40., -2400., None);
    for t in [0., 0.016, 0.1, 0.4, 1.2] {
        assert!(close(y.at(t), glide.value(t), 9));
        assert!(close(y.velocity(t), glide.velocity(t), 9));
    }
    assert_eq!(y.at(y.duration()), glide.rest);
}

#[test]
fn hands_the_coasts_velocity_to_the_edge_spring_at_an_edge() {
    let options = CoastOptions {
        bounds_y: Some(CoastBounds {
            min: -2000.,
            max: 0.,
        }),
        ..Default::default()
    };
    let y = fixed(&[(Y, -100.)])
        .coast(
            CoastVelocity {
                x: None,
                y: Some(3000.),
            },
            options,
        )
        .y
        .unwrap();
    let glide = solve_decay(-100., 3000., None);
    let hit = glide.time_to_reach(0.).unwrap();
    assert!(close(y.at(hit), 0., 6));
    assert!(close(y.velocity(hit - 1e-9), y.velocity(hit + 1e-9), 2));
    assert!(y.at(hit + 0.06) > 0.);
    assert_eq!(y.at(y.duration()), 0.);
}

#[test]
fn springs_straight_back_with_the_release_velocity_when_released_past_an_edge() {
    let options = CoastOptions {
        bounds_x: Some(CoastBounds {
            min: -500.,
            max: 0.,
        }),
        ..Default::default()
    };
    let x = fixed(&[(X, 60.)])
        .coast(
            CoastVelocity {
                x: Some(400.),
                y: None,
            },
            options,
        )
        .x
        .unwrap();
    let back = solve_spring(scroll::BOUNCE.to_spring(), 60., 0., 400.);
    assert!(x.is_transition());
    assert!(close(x.velocity(0.), 400., 6));
    assert!(close(x.at(0.1), back.value(0.1), 9));
    assert_eq!(x.at(x.duration()), 0.);
}

#[test]
fn rests_where_it_is_at_zero_velocity_inside_its_bounds() {
    let options = CoastOptions {
        bounds_x: Some(CoastBounds {
            min: -100.,
            max: 0.,
        }),
        ..Default::default()
    };
    let x = fixed(&[(X, -30.)])
        .coast(
            CoastVelocity {
                x: Some(0.),
                y: None,
            },
            options,
        )
        .x
        .unwrap();
    assert_eq!(x.duration(), 0.);
    assert_eq!(x.at(0.), -30.);
}

#[test]
fn reads_the_presentation_value_at_the_release_time_on_the_translate_axes_only() {
    let reader = fixed(&[(X, 0.), (Y, 0.)]);
    let curves = reader.coast(
        CoastVelocity {
            x: Some(500.),
            y: Some(200.),
        },
        CoastOptions {
            at: Some(1234.),
            ..Default::default()
        },
    );
    assert!(close(curves.x.unwrap().velocity(0.), 500., 9));
    assert!(close(curves.y.unwrap().velocity(0.), 200., 9));
    assert_eq!(*reader.reads.borrow(), vec![Some(1234.), Some(1234.)]);
}

#[test]
fn coasts_at_the_given_rate() {
    let options = CoastOptions {
        rate: scroll::DECELERATION_FAST,
        ..Default::default()
    };
    let x = fixed(&[(X, 0.)])
        .coast(
            CoastVelocity {
                x: Some(1000.),
                y: None,
            },
            options,
        )
        .x
        .unwrap();
    let glide = solve_decay(0., 1000., Some(scroll::DECELERATION_FAST));
    assert_eq!(x.at(x.duration()), glide.rest);
}
