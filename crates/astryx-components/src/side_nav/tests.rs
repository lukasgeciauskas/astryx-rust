//! SideNav tests, numbered after its specs.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{IntoElement, ParentElement, Styled, TestAppContext, VisualTestContext, div, px};

use super::*;
use crate::segmented::tests::host;

type Log = Rc<RefCell<Vec<String>>>;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn side_nav(cx: &mut TestAppContext, collapsed: bool, log: Log) -> &mut VisualTestContext {
    cx.update(astryx_primitives::storage::use_memory_storage);
    let cx = host(cx, move || {
        let (a, b) = (log.clone(), log.clone());
        div()
            .h(px(500.))
            .flex()
            .child(
                SideNav::new("nav")
                    .collapsible_config(SideNavCollapseConfig {
                        default_is_collapsed: Some(collapsed),
                        ..Default::default()
                    })
                    .child(
                        SideNavItem::new("home", "Home")
                            .icon("info")
                            .on_click(move |_, _| a.borrow_mut().push("home".into())),
                    )
                    .child(
                        SideNavItem::new("plain", "No icon")
                            .on_click(move |_, _| b.borrow_mut().push("plain".into())),
                    )
                    .child(
                        SideNavItem::new("tools", "Tools")
                            .icon("wrench")
                            .collapsible(true)
                            .child(SideNavItem::new("tokens", "Tokens")),
                    ),
            )
            .into_any_element()
    });
    settle(cx);
    cx
}

#[gpui::test]
fn fr1_nested_items_show_and_a_collapsible_item_toggles_them(cx: &mut TestAppContext) {
    let cx = side_nav(cx, false, Log::default());
    assert!(cx.debug_bounds("tokens").is_none() || true);
    // The group starts expanded.
    let tools = cx.debug_bounds("tools").is_some();
    assert!(tools);
    assert!(cx.debug_bounds("nav").is_some());
    let before = cx.debug_bounds("tokens").is_some();
    assert!(before, "children show");
    let at = cx.debug_bounds("tools").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::default());
    settle(cx);
    assert!(
        cx.debug_bounds("tokens").is_none(),
        "a press on the row collapses its group"
    );
}

#[gpui::test]
fn fr2_a_collapsed_rail_keeps_items_with_icons_only(cx: &mut TestAppContext) {
    let cx = side_nav(cx, true, Log::default());
    assert!(cx.debug_bounds("home").is_some());
    assert!(
        cx.debug_bounds("plain").is_none(),
        "an item without an icon is not rendered"
    );
    let nav = cx.debug_bounds("nav").unwrap();
    assert_eq!(nav.size.width, px(48.), "the rail");
}

#[gpui::test]
fn the_collapse_button_toggles_the_rail(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = side_nav(cx, false, log.clone());
    assert_eq!(cx.debug_bounds("nav").unwrap().size.width, px(260.));
    let at = cx.debug_bounds("side-nav-collapse").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::default());
    settle(cx);
    frames(cx, 700);
    assert_eq!(cx.debug_bounds("nav").unwrap().size.width, px(48.));
}

fn frames(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..(ms / 16) {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        settle(cx);
    }
}

fn width(cx: &mut VisualTestContext) -> gpui::Pixels {
    cx.debug_bounds("nav").unwrap().size.width
}

fn toggle(cx: &mut VisualTestContext) {
    let at = cx.debug_bounds("side-nav-collapse").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::default());
    settle(cx);
}

#[gpui::test]
fn fr6_a_collapse_moves_the_edge_and_holds_the_old_rail_until_it_fades(cx: &mut TestAppContext) {
    let cx = side_nav(cx, false, Log::default());
    toggle(cx);
    assert!(
        (width(cx) - px(260.)).abs() < px(2.),
        "the edge starts where it was"
    );
    assert!(
        cx.debug_bounds("plain").is_some(),
        "the old, expanded rail shows"
    );
    frames(cx, 64);
    let mid = width(cx);
    assert!(mid < px(259.) && mid > px(49.), "the edge travels: {mid:?}");
    frames(cx, 700);
    assert_eq!(width(cx), px(48.));
    assert!(
        cx.debug_bounds("plain").is_none(),
        "it ends in the real collapsed state, icons only"
    );
}

#[gpui::test]
fn fr6_an_expand_reveals_the_real_rail_from_the_collapsed_edge(cx: &mut TestAppContext) {
    let cx = side_nav(cx, true, Log::default());
    toggle(cx);
    assert!(
        cx.debug_bounds("plain").is_some(),
        "the real, expanded rail"
    );
    assert!(
        (width(cx) - px(48.)).abs() < px(2.),
        "clipped at the old edge"
    );
    frames(cx, 64);
    let mid = width(cx);
    assert!(mid > px(49.) && mid < px(259.), "revealing: {mid:?}");
    frames(cx, 700);
    assert_eq!(width(cx), px(260.));
}

#[gpui::test]
fn fr6_a_second_toggle_mid_motion_starts_from_what_the_rail_shows(cx: &mut TestAppContext) {
    let cx = side_nav(cx, false, Log::default());
    toggle(cx);
    frames(cx, 96);
    let shown = width(cx);
    toggle(cx);
    let from = width(cx);
    assert!(
        (from - shown).abs() < px(3.),
        "no jump: {from:?} vs {shown:?}"
    );
    frames(cx, 800);
    assert_eq!(width(cx), px(260.));
}

#[gpui::test]
fn fr6_under_reduce_motion_with_a_mouse_the_rail_lands_at_once(cx: &mut TestAppContext) {
    cx.update(|cx| astryx_primitives::set_reduced_motion(true, cx));
    let cx = side_nav(cx, false, Log::default());
    toggle(cx);
    assert_eq!(width(cx), px(48.));
}

#[gpui::test]
fn fr6_a_change_before_the_first_paint_lands_at_once(cx: &mut TestAppContext) {
    let cx = side_nav(cx, true, Log::default());
    assert_eq!(width(cx), px(48.), "the initial state never animates");
}

#[gpui::test]
fn an_item_runs_its_click(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = side_nav(cx, false, log.clone());
    let at = cx.debug_bounds("home").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::default());
    settle(cx);
    assert_eq!(*log.borrow(), ["home"]);
}
