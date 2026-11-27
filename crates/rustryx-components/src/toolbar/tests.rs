//! Toolbar tests: one tab stop, arrows inside.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::TestAppContext;

use super::*;
use crate::button::Button;
use crate::segmented::tests::{host, press};

#[gpui::test]
fn a_toolbar_is_one_tab_stop_and_arrows_move_inside(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let button = |id: &'static str| {
                let log = log.clone();
                Button::new(id, id).on_click(move |_, _, _| log.borrow_mut().push(id))
            };
            div()
                .flex()
                .flex_col()
                .child(
                    Toolbar::new("t", "Tools")
                        .start_content(div().flex().child(button("a")).child(button("b")))
                        .end_content(button("c")),
                )
                .child(button("after"))
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["a"], "Tab enters at the first control");
    press(cx, "right");
    press(cx, "right");
    press(cx, "enter");
    assert_eq!(log.borrow().last(), Some(&"c"), "arrows move inside");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(
        log.borrow().last(),
        Some(&"after"),
        "Tab leaves the toolbar"
    );
}
