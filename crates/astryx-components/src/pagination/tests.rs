//! Pagination and Breadcrumbs tests.

use std::cell::RefCell;

use gpui::TestAppContext;

use super::PageSlot::{Ellipsis, Page};
use super::*;
use crate::breadcrumbs::{BreadcrumbItem, Breadcrumbs};
use crate::segmented::tests::{host, press};

#[test]
fn page_ranges_match_upstream() {
    assert_eq!(
        page_range(1, 5, 1),
        [Page(1), Page(2), Page(3), Page(4), Page(5)]
    );
    assert_eq!(
        page_range(2, 20, 1),
        [
            Page(1),
            Page(2),
            Page(3),
            Page(4),
            Page(5),
            Ellipsis,
            Page(20)
        ]
    );
    assert_eq!(
        page_range(19, 20, 1),
        [
            Page(1),
            Ellipsis,
            Page(16),
            Page(17),
            Page(18),
            Page(19),
            Page(20)
        ]
    );
    assert_eq!(
        page_range(10, 20, 1),
        [
            Page(1),
            Ellipsis,
            Page(9),
            Page(10),
            Page(11),
            Ellipsis,
            Page(20)
        ]
    );
}

#[gpui::test]
fn next_and_previous_move_by_the_step(cx: &mut TestAppContext) {
    let pages = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let pages = pages.clone();
        move || {
            let pages = pages.clone();
            Pagination::new("p", 5)
                .variant(PaginationVariant::None)
                .total_pages(6)
                .step(2)
                .on_change(move |page, _, _| pages.borrow_mut().push(page))
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(
        *pages.borrow(),
        [3, 6],
        "back two; forward two, clamped to the last"
    );
}

#[gpui::test]
fn breadcrumb_links_navigate_but_the_current_page_does_not(cx: &mut TestAppContext) {
    let opened = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let opened = opened.clone();
        move || {
            let opened = opened.clone();
            let handler = astryx_primitives::LinkHandler::new(move |navigation, _, _| {
                opened.borrow_mut().push(navigation.href.clone())
            });
            astryx_primitives::link_provider(
                handler,
                Breadcrumbs::new("b")
                    .item(BreadcrumbItem::new("Home").href("/"))
                    .item(BreadcrumbItem::new("Here").href("/here")),
            )
            .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "enter");
    // The current page is no tab stop: Tab wraps back to Home.
    press(cx, "tab");
    press(cx, "enter");
    let opened = opened.borrow();
    assert!(opened.iter().all(|href| href == "/"), "{opened:?}");
    assert!(!opened.is_empty());
}

#[gpui::test]
fn the_input_variant_goes_to_a_typed_page(cx: &mut TestAppContext) {
    cx.update(astryx_primitives::editing::init);
    let pages = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let pages = pages.clone();
        move || {
            let pages = pages.clone();
            Pagination::new("p", 3)
                .variant(PaginationVariant::Input)
                .total_pages(20)
                .on_change(move |page, _, _| pages.borrow_mut().push(page))
                .into_any_element()
        }
    });
    // Tabbing in selects the text: focus events need the active window.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    // First, previous, then the page box.
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "tab");
    cx.simulate_input("12");
    press(cx, "enter");
    cx.simulate_input("99");
    press(cx, "enter");
    assert_eq!(
        *pages.borrow(),
        [12, 20],
        "a page past the last clamps to it"
    );
}
