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

// ---- FR17: the content column moves with the side nav's collapse and expand -------

fn frames(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..(ms / 16) {
        wait(cx, 16);
    }
}

fn collapsible_shell(cx: &mut TestAppContext) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        div()
            .size_full()
            .child(
                AppShell::new("shell")
                    .mobile_nav(wide())
                    .side_nav(|| {
                        SideNav::new("nav")
                            .collapsible(true)
                            .child(SideNavItem::new("home", "Home").icon("info"))
                    })
                    .child(div().debug_selector(|| "content".into()).child("Content")),
            )
            .into_any_element()
    });
    settle(cx);
    cx
}

#[gpui::test]
fn fr17_the_content_column_glides_beside_the_rails_edge(cx: &mut TestAppContext) {
    let cx = collapsible_shell(cx);
    let open = cx.debug_bounds("shell-main").unwrap().left();
    click(cx, "side-nav-collapse");
    let start = cx.debug_bounds("shell-main").unwrap().left();
    assert!(
        (start - open).abs() < px(2.),
        "it starts where it was: {start:?}"
    );
    frames(cx, 96);
    let main = cx.debug_bounds("shell-main").unwrap();
    let nav = cx.debug_bounds("nav").unwrap();
    assert!(main.left() < open - px(1.), "gliding: {:?}", main.left());
    assert!(
        (nav.right() - main.left()).abs() < px(2.),
        "the rail's edge rides the column: {:?} vs {:?}",
        nav.right(),
        main.left()
    );
    frames(cx, 700);
    let rest = cx.debug_bounds("shell-main").unwrap();
    assert_eq!(
        main.size.width, rest.size.width,
        "the column never animates its width: it has its final width at once"
    );
    assert_eq!(rest.left(), cx.debug_bounds("nav").unwrap().right());
    assert!(rest.left() < open - px(100.), "the rail collapsed");
}

#[gpui::test]
fn fr17_an_expand_covers_the_gap_at_the_columns_trailing_edge(cx: &mut TestAppContext) {
    let cx = collapsible_shell(cx);
    click(cx, "side-nav-collapse");
    frames(cx, 700);
    click(cx, "side-nav-collapse");
    frames(cx, 96);
    let cover = cx.debug_bounds("app-shell-content-cover").unwrap();
    assert!(
        cover.size.width > px(1.),
        "the surface reaches over the gap"
    );
    frames(cx, 700);
    let cover = cx.debug_bounds("app-shell-content-cover").unwrap();
    assert_eq!(cover.size.width, px(0.));
}

#[gpui::test]
fn fr17_under_reduce_motion_with_a_mouse_it_lands_at_once(cx: &mut TestAppContext) {
    cx.update(|cx| astryx_primitives::set_reduced_motion(true, cx));
    let cx = collapsible_shell(cx);
    let open = cx.debug_bounds("shell-main").unwrap().left();
    click(cx, "side-nav-collapse");
    let main = cx.debug_bounds("shell-main").unwrap().left();
    assert!(main < open - px(100.), "no glide: {main:?}");
    assert_eq!(main, cx.debug_bounds("nav").unwrap().right());
}

#[gpui::test]
fn fr17_the_content_swaps_its_old_wrap_for_a_snapshot_copy_that_fades_out(cx: &mut TestAppContext) {
    let cx = collapsible_shell(cx);
    let before = cx.debug_bounds("shell-main").unwrap();
    assert!(shown(cx, "content"));
    assert!(
        cx.debug_bounds("app-shell-content-copy").is_none(),
        "at rest, no copy"
    );
    click(cx, "side-nav-collapse");
    frames(cx, 32);
    let copy = cx
        .debug_bounds("app-shell-content-copy")
        .expect("AST-105 FR8: the old wrap, held as a snapshot of main's last frame");
    assert_eq!(copy.size, before.size, "at main's old size");
    frames(cx, 700);
    assert!(
        cx.debug_bounds("app-shell-content-copy").is_none(),
        "removed when the swap ends"
    );
}

#[gpui::test]
fn fr17_a_change_during_a_running_swap_starts_its_cross_fade_from_what_the_content_shows(
    cx: &mut TestAppContext,
) {
    super::app_shell::SWAP_STARTS.with(|s| s.borrow_mut().clear());
    let cx = collapsible_shell(cx);
    click(cx, "side-nav-collapse");
    frames(cx, 160);
    assert!(cx.debug_bounds("app-shell-content-copy").is_some());
    click(cx, "side-nav-collapse");
    frames(cx, 32);
    assert!(
        cx.debug_bounds("app-shell-content-copy").is_some(),
        "the new change holds the content shown now as its copy"
    );
    let starts = super::app_shell::SWAP_STARTS.with(|s| s.borrow().clone());
    assert_eq!(starts.len(), 2, "{starts:?}");
    assert_eq!(
        starts[0],
        (1., 0.),
        "from rest: the copy opaque, the content unseen"
    );
    let (held, entering) = starts[1];
    assert!(
        held > 0. && held < 1. && entering > 0. && entering < 1.,
        "mid-swap each starts at what it shows: {starts:?}"
    );
    assert!((held + entering - 1.).abs() < 1e-4, "{starts:?}");
    frames(cx, 800);
    assert!(cx.debug_bounds("app-shell-content-copy").is_none());
}
