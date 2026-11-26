//! MobileNav tests, numbered after its spec.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::provide;
use gpui::{
    InteractiveElement, IntoElement, Modifiers, ParentElement, StatefulInteractiveElement, Styled,
    TestAppContext, VisualTestContext, div, point, px,
};

use super::*;
use crate::app_shell::AppShellMobile;
use crate::segmented::tests::host;

type Log = Rc<RefCell<Vec<String>>>;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    settle(cx);
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

/// A page with a target behind the drawer, top right, and a controlled MobileNav.
fn page(
    cx: &mut TestAppContext,
    open: Rc<Cell<bool>>,
    log: Log,
    build: impl Fn(MobileNav) -> MobileNav + 'static,
) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        let (o, l1, l2) = (open.clone(), log.clone(), log.clone());
        div()
            .size_full()
            .child(
                div()
                    .id("behind")
                    .debug_selector(|| "behind".into())
                    .absolute()
                    .top(px(10.))
                    .right(px(10.))
                    .size(px(40.))
                    .on_click(move |_, _, _| l1.borrow_mut().push("behind".into())),
            )
            .child(
                build(MobileNav::new("nav"))
                    .open(o.get())
                    .header_text("Menu")
                    .on_open_change(move |v, window, _| {
                        o.set(v);
                        l2.borrow_mut().push(format!("open {v}"));
                        window.refresh();
                    })
                    .child(
                        div()
                            .id("link")
                            .debug_selector(|| "link".into())
                            .h(px(24.))
                            .child("Home"),
                    ),
            )
            .into_any_element()
    });
    settle(cx);
    wait(cx, 600);
    cx
}

fn start(nav: MobileNav) -> MobileNav {
    nav.side(MobileNavSide::Start)
}

#[gpui::test]
fn fr1_fr4_the_drawer_shows_and_its_close_button_closes_it(cx: &mut TestAppContext) {
    let (open, log) = (Rc::new(Cell::new(true)), Log::default());
    let cx = page(cx, open.clone(), log.clone(), start);
    assert!(cx.debug_bounds("nav-drawer").is_some());
    click(cx, "nav-close");
    assert_eq!(*log.borrow(), vec!["open false".to_string()]);
    assert!(!open.get());
}

#[gpui::test]
fn fr4_a_press_on_the_backdrop_closes_it_and_one_inside_does_not(cx: &mut TestAppContext) {
    let (open, log) = (Rc::new(Cell::new(true)), Log::default());
    let cx = page(cx, open, log.clone(), start);
    click(cx, "link");
    assert!(
        log.borrow().is_empty(),
        "a press inside the drawer keeps it"
    );
    let viewport = cx.update(|window, _| window.viewport_size());
    cx.simulate_click(
        point(viewport.width - px(100.), px(300.)),
        Modifiers::none(),
    );
    settle(cx);
    assert_eq!(*log.borrow(), vec!["open false".to_string()]);
}

#[gpui::test]
fn fr4_escape_closes_it(cx: &mut TestAppContext) {
    let (open, log) = (Rc::new(Cell::new(true)), Log::default());
    let cx = page(cx, open, log.clone(), start);
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(*log.borrow(), vec!["open false".to_string()]);
}

#[gpui::test]
fn fr5_the_page_takes_input_within_250ms_while_the_drawer_slides_out(cx: &mut TestAppContext) {
    let (open, log) = (Rc::new(Cell::new(true)), Log::default());
    let cx = page(cx, open, log.clone(), start);
    click(cx, "nav-close");
    log.borrow_mut().clear();
    click(cx, "behind");
    assert!(
        !log.borrow().contains(&"behind".to_string()),
        "still modal right after the request: the backdrop takes the press"
    );
    log.borrow_mut().clear();
    wait(cx, 260);
    assert!(
        cx.debug_bounds("nav-drawer").is_some(),
        "the drawer still slides out"
    );
    click(cx, "behind");
    assert_eq!(
        *log.borrow(),
        vec!["behind".to_string()],
        "the dialog has closed"
    );
    wait(cx, 400);
    assert!(cx.debug_bounds("nav-drawer").is_none());
}

#[gpui::test]
fn fr9_the_drawer_is_never_wider_than_85_percent(cx: &mut TestAppContext) {
    let cx = page(cx, Rc::new(Cell::new(true)), Log::default(), |nav| {
        start(nav).width(px(5000.))
    });
    let viewport = cx.update(|window, _| window.viewport_size());
    let drawer = cx.debug_bounds("nav-drawer").unwrap();
    assert_eq!(drawer.size.width, viewport.width * 0.85);
}

#[gpui::test]
fn fr8_auto_follows_the_half_its_opener_is_in(cx: &mut TestAppContext) {
    let open = Rc::new(Cell::new(false));
    let cx = host(cx, {
        let open = open.clone();
        move || {
            let (a, b, o) = (open.clone(), open.clone(), open.clone());
            div()
                .size_full()
                .child(
                    div()
                        .id("left")
                        .debug_selector(|| "left".into())
                        .absolute()
                        .top(px(10.))
                        .left(px(10.))
                        .size(px(40.))
                        .on_click(move |_, window, _| {
                            a.set(true);
                            window.refresh();
                        }),
                )
                .child(
                    div()
                        .id("right")
                        .debug_selector(|| "right".into())
                        .absolute()
                        .top(px(10.))
                        .right(px(10.))
                        .size(px(40.))
                        .on_click(move |_, window, _| {
                            b.set(true);
                            window.refresh();
                        }),
                )
                .child(MobileNav::new("nav").open(o.get()).on_open_change({
                    let o = o.clone();
                    move |v, window, _| {
                        o.set(v);
                        window.refresh();
                    }
                }))
                .into_any_element()
        }
    });
    settle(cx);
    let viewport = cx.update(|window, _| window.viewport_size());
    click(cx, "right");
    wait(cx, 600);
    let drawer = cx.debug_bounds("nav-drawer").unwrap();
    assert_eq!(
        drawer.right(),
        viewport.width,
        "opened from the end half: the end edge"
    );
    cx.simulate_keystrokes("escape");
    wait(cx, 600);
    click(cx, "left");
    wait(cx, 600);
    let drawer = cx.debug_bounds("nav-drawer").unwrap();
    assert_eq!(
        drawer.left(),
        px(0.),
        "opened from the start half: the start edge"
    );
}

#[gpui::test]
fn fr11_the_toggle_shows_only_in_the_mobile_layout_and_toggles(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let log = log.clone();
            div()
                .child(provide(
                    AppShellMobile::default(),
                    MobileNavToggle::new("desktop"),
                ))
                .child(provide(
                    AppShellMobile::new(false, move |v, _, _| {
                        log.borrow_mut().push(format!("open {v}"))
                    }),
                    MobileNavToggle::new("phone"),
                ))
                .into_any_element()
        }
    });
    settle(cx);
    assert!(
        cx.debug_bounds("desktop").is_none(),
        "not in the mobile layout"
    );
    click(cx, "phone");
    assert_eq!(*log.borrow(), vec!["open true".to_string()]);
}
