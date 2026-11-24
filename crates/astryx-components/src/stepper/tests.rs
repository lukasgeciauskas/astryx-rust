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
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*clicks.borrow(), [0, 2], "the disabled step is no tab stop");
}
