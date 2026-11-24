//! TabList tests.

use std::cell::RefCell;

use gpui::TestAppContext;

use super::*;
use crate::segmented::tests::{host, press};

#[gpui::test]
fn arrows_move_focus_and_enter_activates(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let log = log.clone();
            TabList::new("t", "a")
                .on_change(move |value, _, _| log.borrow_mut().push(value))
                .tab(Tab::new("a", "Alpha"))
                .tab(Tab::new("b", "Beta"))
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "right");
    assert!(log.borrow().is_empty(), "moving focus does not activate");
    press(cx, "enter");
    assert_eq!(*log.borrow(), [SharedString::from("b")]);
}

#[test]
fn arrows_page_the_strip_and_stop_at_its_ends() {
    // A 200 px strip with 300 px more to scroll pages by 160 px.
    assert_eq!(paged_offset(0., 200., 300., 1.), -160.);
    assert_eq!(paged_offset(-160., 200., 300., 1.), -300.);
    assert_eq!(paged_offset(-300., 200., 300., -1.), -140.);
    assert_eq!(paged_offset(-100., 200., 300., -1.), 0.);
}
