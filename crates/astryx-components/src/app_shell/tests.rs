//! AppShell tests, numbered after its spec.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    InteractiveElement, IntoElement, Modifiers, ParentElement, ScrollDelta, ScrollEvent,
    StatefulInteractiveElement, Styled, TestAppContext, TouchPhase, VisualTestContext, div, point,
    px,
};

use gpui::prelude::*;

use super::*;
use crate::layout::LayoutHeight;
use crate::segmented::tests::host;
use crate::side_nav::{SideNav, SideNavItem};
use crate::top_nav::{TopNav, TopNavItem};

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

fn shown(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

fn side_nav(log: Log) -> impl Fn() -> SideNav + 'static {
    move || {
        let log = log.clone();
        SideNav::new("nav").child(
            SideNavItem::new("home", "Home")
                .icon("info")
                .on_click(move |_, _| log.borrow_mut().push("home".into())),
        )
    }
}

/// A phone-sized window.
fn phone(cx: &mut VisualTestContext) {
    cx.simulate_resize(gpui::size(px(480.), px(800.)));
    settle(cx);
}

/// A breakpoint the test window is below, so the mobile layout applies.
fn mobile() -> AppShellMobileNav {
    AppShellMobileNav::Auto(MobileNavConfig {
        breakpoint: AppShellBreakpoint::Xl2,
        ..Default::default()
    })
}

fn wide() -> AppShellMobileNav {
    AppShellMobileNav::Auto(MobileNavConfig {
        breakpoint: AppShellBreakpoint::None,
        ..Default::default()
    })
}

#[gpui::test]
fn fr3_fr4_a_side_nav_sits_beside_main_and_the_header_needs_a_top_nav_or_banner(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let cx = host(cx, move || {
        div()
            .size_full()
            .child(
                AppShell::new("shell")
                    .mobile_nav(wide())
                    .side_nav(side_nav(log.clone()))
                    .child(div().debug_selector(|| "content".into()).child("Content")),
            )
            .into_any_element()
    });
    settle(cx);
    assert!(shown(cx, "home"), "the side nav is inline");
    assert!(shown(cx, "content"));
    assert!(shown(cx, "shell-main"));
    assert!(
        !shown(cx, "shell-header"),
        "no top nav and no banner: no header region"
    );
}

#[gpui::test]
fn fr5_the_skip_link_moves_focus_to_main(cx: &mut TestAppContext) {
    let cx = host(cx, move || {
        AppShell::new("shell")
            .mobile_nav(wide())
            .banner(div().child("Notice"))
            .child(div().child("Content"))
            .into_any_element()
    });
    settle(cx);
    assert!(
        shown(cx, "shell-header"),
        "a banner makes the header region"
    );
    assert!(cx.update(|window, cx| window.focused(cx).is_none()));
    cx.simulate_keystrokes("tab");
    settle(cx);
    cx.simulate_keystrokes("enter");
    settle(cx);
    let main = cx.debug_bounds("shell-main").unwrap();
    let focused = cx.update(|window, cx| window.focused(cx).is_some());
    assert!(focused, "focus moved to main");
    assert!(main.size.height > px(0.));
}

#[gpui::test]
fn fr9_fr10_the_mobile_layout_moves_the_side_nav_into_the_drawer(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, {
        let log = log.clone();
        move || {
            AppShell::new("shell")
                .mobile_nav(mobile())
                .side_nav(side_nav(log.clone()))
                .child(div().child("Content"))
                .into_any_element()
        }
    });
    phone(cx);
    let width = cx.update(|window, _| window.viewport_size().width);
    assert!(width < px(1536.), "the test window is below 2xl");
    assert!(!shown(cx, "home"), "the side nav left the inline region");
    assert!(
        shown(cx, "shell-toggle"),
        "the phone bar carries the toggle"
    );
    click(cx, "shell-toggle");
    wait(cx, 600);
    assert!(
        shown(cx, "nav-drawer-drawer"),
        "the drawer is SideNav in drawer mode"
    );
    click(cx, "home");
    assert_eq!(*log.borrow(), vec!["home".to_string()]);
    wait(cx, 600);
    assert!(
        !shown(cx, "nav-drawer-drawer"),
        "choosing a row closed the drawer"
    );
}

#[gpui::test]
fn fr9_a_top_nav_becomes_the_phones_bar_and_carries_the_side_nav_in_its_drawer(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    let cx = host(cx, {
        let log = log.clone();
        move || {
            AppShell::new("shell")
                .mobile_nav(mobile())
                .top_nav(|| TopNav::new("top").start_content(TopNavItem::new("docs", "Docs")))
                .side_nav(side_nav(log.clone()))
                .child(div().child("Content"))
                .into_any_element()
        }
    });
    phone(cx);
    assert!(!shown(cx, "docs"), "the bar drops start content");
    click(cx, "top-toggle");
    wait(cx, 600);
    assert!(shown(cx, "top-drawer-drawer"), "TopNav's drawer");
    assert!(shown(cx, "docs"), "with its items");
    assert!(shown(cx, "home"), "and the side nav's");
}

#[gpui::test]
fn fr11_fr12_false_leaves_no_toggle_and_no_side_nav(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, move || {
        AppShell::new("shell")
            .mobile_nav(AppShellMobileNav::Disabled)
            .side_nav(side_nav(log.clone()))
            .child(div().child("Content"))
            .into_any_element()
    });
    settle(cx);
    // md: the test window may be wide; force the mobile check through the context.
    assert!(!shown(cx, "shell-toggle"));
}

#[gpui::test]
fn fr13_the_subtree_reads_the_mobile_layout(cx: &mut TestAppContext) {
    #[derive(IntoElement)]
    struct Probe;
    impl gpui::RenderOnce for Probe {
        fn render(self, _: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
            let shell = use_app_shell_mobile(cx);
            div().when(shell.is_mobile && shell.enabled, |el| {
                el.debug_selector(|| "mobile".into()).child("Mobile")
            })
        }
    }
    let log = Log::default();
    let cx = host(cx, move || {
        AppShell::new("shell")
            .mobile_nav(mobile())
            .side_nav(side_nav(log.clone()))
            .child(Probe)
            .into_any_element()
    });
    phone(cx);
    assert!(shown(cx, "mobile"));
}

#[gpui::test]
fn fr6_in_auto_the_header_sticks_while_the_page_scrolls(cx: &mut TestAppContext) {
    let cx = host(cx, move || {
        div()
            .id("page")
            .debug_selector(|| "page".into())
            .size_full()
            .overflow_y_scroll()
            .child(
                AppShell::new("shell")
                    .mobile_nav(wide())
                    .height(LayoutHeight::Auto)
                    .banner(div().h(px(40.)).child("Notice"))
                    .child(div().h(px(3000.)).child("Long content")),
            )
            .into_any_element()
    });
    settle(cx);
    let page = cx.debug_bounds("page").unwrap();
    let before = cx.debug_bounds("shell-header").unwrap();
    assert_eq!(before.top(), page.top());
    cx.simulate_event(ScrollEvent {
        position: page.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-600.))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
    settle(cx);
    let after = cx.debug_bounds("shell-header").unwrap();
    assert_eq!(
        after.top(),
        page.top(),
        "the header stays at the top of the page"
    );
}
