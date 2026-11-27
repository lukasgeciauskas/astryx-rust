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

#[gpui::test]
fn a_tab_menu_opens_its_options_and_selects_through_the_tab_list(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let log = log.clone();
            TabList::new("t", "a")
                .on_change(move |value, _, _| log.borrow_mut().push(value))
                .tab(Tab::new("a", "Alpha"))
                .menu(TabMenu::new(
                    "More",
                    [
                        TabMenuOption::new("c", "Gamma"),
                        TabMenuOption::new("d", "Delta"),
                    ],
                ))
                .into_any_element()
        }
    });
    // Alpha is the tab stop; ArrowRight reaches the menu without opening it.
    press(cx, "tab");
    press(cx, "right");
    assert!(log.borrow().is_empty());
    press(cx, "enter");
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(
        *log.borrow(),
        [SharedString::from("d")],
        "the second option"
    );
}

#[test]
fn fades_out_at_the_strips_own_edge_not_at_the_bleed_edge() {
    use gpui::{MaskStop, px};
    // Nothing hidden: no mask.
    assert!(super::strip_fade(false, false, px(4.), px(32.), false).is_none());
    // Both edges hide tabs: transparent at the bleed, opaque a fade's width further in.
    let both = super::strip_fade(true, true, px(4.), px(32.), false).unwrap();
    assert_eq!(both.angle, 90.);
    assert_eq!(
        both.stops,
        vec![
            MaskStop::px(px(4.), 0.),
            MaskStop::px(px(36.), 1.),
            MaskStop::from_end(px(36.), 1.),
            MaskStop::from_end(px(4.), 0.),
        ]
    );
    // Only the end hides tabs: the start shows in full.
    let end = super::strip_fade(false, true, px(4.), px(32.), false).unwrap();
    assert_eq!(end.stops[0], MaskStop::px(px(0.), 1.));
    // Right to left, the start is the right edge.
    let rtl = super::strip_fade(true, false, px(4.), px(32.), true).unwrap();
    assert_eq!(rtl.angle, 270.);
    assert_eq!(rtl.stops[0], MaskStop::px(px(4.), 0.));
}
