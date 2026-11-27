//! Stepper tests.

use std::cell::RefCell;

use gpui::TestAppContext;

use super::*;
use crate::segmented::tests::{host, press};

#[test]
fn steps_before_the_active_one_are_completed() {
    assert_eq!(progress(0, 1), StepState::Completed);
    assert_eq!(progress(1, 1), StepState::Active);
    assert_eq!(progress(2, 1), StepState::NotStarted);
}

#[gpui::test]
fn clickable_steps_report_their_index_and_skip_disabled_ones(cx: &mut TestAppContext) {
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let clicks = clicks.clone();
        move || {
            let clicks = clicks.clone();
            div()
                .w(px(600.))
                .child(
                    Stepper::new("s", 1)
                        .on_step_click(move |index, _, _| clicks.borrow_mut().push(index))
                        .step(Step::new("Cart"))
                        .step(Step::new("Shipping").disabled(true))
                        .step(Step::new("Payment")),
                )
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "enter");
    // The disabled step keeps its tab stop (spec FR1) and never activates.
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*clicks.borrow(), [0, 2]);
}

#[test]
fn only_a_single_advance_sweeps_and_only_its_span() {
    // Advanced from step 1 to 2, halfway through the sweep.
    let sweep = Sweep {
        span: Some(1),
        elapsed: 0.5,
    };
    // Separated: step 2's bar draws span 1 whole.
    assert_eq!(sweep.fill(1, 0., 1., true), 0.5);
    // Every other segment lands at once; an empty one stays empty.
    assert_eq!(sweep.fill(0, 0., 1., true), 1.);
    assert_eq!(sweep.fill(2, 0., 1., false), 0.);
    // On-track, horizontal: step 1's rail leaves first, step 2's lead after.
    let early = Sweep {
        span: Some(1),
        elapsed: 0.25,
    };
    assert_eq!(early.fill(1, 0., 0.5, true), 0.5);
    assert_eq!(early.fill(1, 0.5, 0.5, true), 0.);
    let late = Sweep {
        span: Some(1),
        elapsed: 0.75,
    };
    assert_eq!(late.fill(1, 0., 0.5, true), 1.);
    assert_eq!(late.fill(1, 0.5, 0.5, true), 0.5);
    // No sweep: everything at its target.
    let still = Sweep {
        span: None,
        elapsed: 1.,
    };
    assert_eq!(still.fill(1, 0.5, 0.5, true), 1.);
}

#[test]
fn compact_controls_skip_disabled_steps() {
    let disabled = [false, false, true, false];
    assert_eq!(adjacent_enabled(&disabled, 1, -1), Some(0));
    assert_eq!(adjacent_enabled(&disabled, 1, 1), Some(3));
    assert_eq!(adjacent_enabled(&disabled, 0, -1), None);
    assert_eq!(adjacent_enabled(&disabled, 3, 1), None);
    assert_eq!(adjacent_enabled(&[false, true, true], 0, 1), None);
}

fn checkout(
    width: f32,
    options: HorizontalOptions,
    position: IndicatorPosition,
    clicks: Rc<RefCell<Vec<usize>>>,
) -> impl Fn() -> AnyElement + 'static {
    move || {
        let clicks = clicks.clone();
        div()
            .w(px(width))
            .child(
                Stepper::new("s", 1)
                    .indicator_position(position)
                    .horizontal_options(options)
                    .on_step_click(move |index, _, _| clicks.borrow_mut().push(index))
                    .step(Step::new("Cart"))
                    .step(Step::new("Shipping"))
                    .step(Step::new("Payment").disabled(true))
                    .step(Step::new("Review")),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn a_narrow_stepper_moves_through_its_controls(cx: &mut TestAppContext) {
    let clicks = Rc::new(RefCell::new(Vec::new()));
    // 300px over four steps is under 112px a step.
    let cx = host(
        cx,
        checkout(
            300.,
            HorizontalOptions::default(),
            IndicatorPosition::Separated,
            clicks.clone(),
        ),
    );
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    // The steps are no longer buttons; the previous and next controls are.
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*clicks.borrow(), [0, 3], "next skips the disabled step");
}

#[gpui::test]
fn a_wide_stepper_keeps_its_step_buttons(cx: &mut TestAppContext) {
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let cx = host(
        cx,
        checkout(
            800.,
            HorizontalOptions::default(),
            IndicatorPosition::OnTrack,
            clicks.clone(),
        ),
    );
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*clicks.borrow(), [0]);
}

#[gpui::test]
fn the_label_only_variant_has_no_controls(cx: &mut TestAppContext) {
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let cx = host(
        cx,
        checkout(
            300.,
            HorizontalOptions {
                collapsed_variant: CollapsedVariant::WithLabel,
                ..HorizontalOptions::default()
            },
            IndicatorPosition::OnTrack,
            clicks.clone(),
        ),
    );
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    press(cx, "tab");
    press(cx, "enter");
    assert!(clicks.borrow().is_empty());
}
