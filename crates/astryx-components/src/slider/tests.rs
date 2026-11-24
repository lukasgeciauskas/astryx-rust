//! Slider tests.

use std::cell::RefCell;

use gpui::{Modifiers, TestAppContext, point};

use super::*;
use crate::segmented::tests::{host, press};

const SCALE: Scale = Scale {
    min: 0.,
    max: 100.,
    step: 5.,
};

#[test]
fn values_snap_to_steps_within_bounds() {
    assert_eq!(SCALE.snap(12.), 10.);
    assert_eq!(SCALE.snap(13.), 15.);
    assert_eq!(SCALE.snap(140.), 100.);
    assert_eq!(SCALE.at(0.5), 50.);
    assert_eq!(SCALE.fraction(25.), 0.25);
}

#[test]
fn keys_step_page_and_jump() {
    assert_eq!(key_value(SCALE, 50., "right", false), Some(55.));
    assert_eq!(
        key_value(SCALE, 50., "right", true),
        Some(45.),
        "mirrored in RTL"
    );
    assert_eq!(key_value(SCALE, 50., "down", false), Some(45.));
    assert_eq!(key_value(SCALE, 50., "pageup", false), Some(60.));
    assert_eq!(key_value(SCALE, 50., "home", false), Some(0.));
    assert_eq!(key_value(SCALE, 50., "end", false), Some(100.));
    assert_eq!(key_value(SCALE, 50., "a", false), None);
}

#[test]
fn range_thumbs_keep_their_order_and_gap() {
    assert_eq!(move_thumb(&[20., 60.], 0, 70., 10.), [50., 60.]);
    assert_eq!(move_thumb(&[20., 60.], 1, 10., 0.), [20., 20.]);
    assert_eq!(move_thumb(&[20.], 0, 70., 10.), [70.]);
}

#[test]
fn a_press_moves_the_nearer_thumb() {
    assert_eq!(nearest_thumb(&[50.], 90.), 0);
    assert_eq!(nearest_thumb(&[20., 60.], 30.), 0);
    assert_eq!(nearest_thumb(&[20., 60.], 50.), 1);
    assert_eq!(
        nearest_thumb(&[40., 40.], 30.),
        0,
        "a tie below goes to the start thumb"
    );
    assert_eq!(
        nearest_thumb(&[40., 40.], 50.),
        1,
        "a tie above goes to the end thumb"
    );
}

#[gpui::test]
fn the_focused_thumb_moves_with_the_keys(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let log = log.clone();
            div()
                .w(px(300.))
                .child(
                    Slider::new("s", "Volume", 40.)
                        .label_hidden(true)
                        .on_change(move |value, _, _| log.borrow_mut().push(value)),
                )
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "right");
    press(cx, "end");
    assert_eq!(
        *log.borrow(),
        [SliderValue::Single(41.), SliderValue::Single(100.)]
    );
}

#[gpui::test]
fn pressing_the_track_moves_the_thumb_there(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let log = log.clone();
            div()
                .w(px(220.))
                .child(
                    Slider::new("s", "Volume", 0.)
                        .label_hidden(true)
                        .on_change(move |value, _, _| log.borrow_mut().push(value)),
                )
                .into_any_element()
        }
    });
    // The track spans x 20..240; its travel box 30..230, so x 130 is the middle.
    cx.simulate_mouse_down(
        point(px(130.), px(30.)),
        gpui::MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(*log.borrow(), [SliderValue::Single(50.)]);
}
