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
    assert_eq!(cx.debug_bounds("nav").unwrap().size.width, px(48.));
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
