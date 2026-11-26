//! TopNav tests, numbered after its specs.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::{LinkHandler, provide, set_link_handler};
use gpui::{
    IntoElement, Modifiers, ParentElement, Styled, TestAppContext, VisualTestContext, div, point,
    px,
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

fn logging_links(cx: &mut TestAppContext, log: &Log) {
    let log = log.clone();
    cx.update(|cx| {
        set_link_handler(
            LinkHandler::new(move |navigation, _, _| {
                log.borrow_mut().push(format!("open {}", navigation.href))
            }),
            cx,
        )
    });
}

fn shown(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

fn hover(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_move(at, None, Modifiers::none());
    settle(cx);
}

/// Leaves `from` as a pointer does, across its edge, then moves far off (outside the
/// safe triangle toward the open surface).
fn away(cx: &mut VisualTestContext, from: &'static str) {
    let bounds = cx.debug_bounds(from).unwrap();
    let edge = point(bounds.left() - px(2.), bounds.center().y);
    cx.simulate_mouse_move(edge, None, Modifiers::none());
    settle(cx);
    cx.simulate_mouse_move(point(px(5.), px(590.)), None, Modifiers::none());
    settle(cx);
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn menu_items() -> Vec<TopNavMenuItemData> {
    vec![
        TopNavMenuItemData::new("Docs").description("Guides"),
        TopNavMenuItemData::new("Blog"),
    ]
}

fn page(build: impl Fn() -> gpui::AnyElement + 'static) -> impl Fn() -> gpui::AnyElement {
    move || {
        div()
            .size_full()
            .child(div().w(px(900.)).child(build()))
            .into_any_element()
    }
}

#[gpui::test]
fn fr2_start_content_wins_over_children(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        page(|| {
            TopNav::new("nav")
                .start_content(TopNavItem::new("start", "Start"))
                .child(TopNavItem::new("child", "Child"))
                .into_any_element()
        }),
    );
    settle(cx);
    assert!(shown(cx, "start"));
    assert!(
        !shown(cx, "child"),
        "children only render without start content"
    );
}

#[gpui::test]
fn fr5_the_phones_bar_keeps_the_heading_and_end_content(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        page(|| {
            provide(
                AppShellMobile::new(false, |_, _, _| {}),
                provide(
                    TopNavRenderMode::MobileBar,
                    TopNav::new("nav")
                        .heading(TopNavHeading::new("brand").heading("Astryx"))
                        .start_content(TopNavItem::new("start", "Start"))
                        .start_content(TopNavMenu::new("menu", "Menu", menu_items()))
                        .end_content(TopNavItem::new("end", "End")),
                ),
            )
            .into_any_element()
        }),
    );
    settle(cx);
    assert!(shown(cx, "brand"));
    assert!(shown(cx, "end"));
    assert!(
        shown(cx, "nav-toggle"),
        "the drawer has content: the toggle shows"
    );
    assert!(!shown(cx, "start"), "start content does not render");
    assert!(
        !shown(cx, "menu"),
        "TopNavMenu renders nothing in the bar (FR9)"
    );
}

#[gpui::test]
fn fr6_the_drawer_renders_nothing_without_items(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        page(|| {
            let open = AppShellMobile::new(true, |_, _, _| {});
            provide(
                open,
                div()
                    .child(provide(
                        TopNavRenderMode::Drawer,
                        TopNav::new("empty")
                            .heading(TopNavHeading::new("lonely").heading("Astryx")),
                    ))
                    .child(provide(
                        TopNavRenderMode::Drawer,
                        TopNav::new("full")
                            .heading(TopNavHeading::new("brand").heading("Astryx"))
                            .start_content(TopNavItem::new("item", "Item")),
                    ))
                    .into_any_element(),
            )
            .into_any_element()
        }),
    );
    settle(cx);
    assert!(
        !shown(cx, "lonely"),
        "no items and no side nav content: nothing"
    );
    assert!(shown(cx, "full-drawer-drawer"), "the drawer is a MobileNav");
    assert!(shown(cx, "brand"));
    assert!(shown(cx, "item"));
}

#[gpui::test]
fn item_fr1_fr3_an_item_runs_its_click_unless_disabled(cx: &mut TestAppContext) {
    let log = Log::default();
    logging_links(cx, &log);
    let cx = host(cx, {
        let log = log.clone();
        page(move || {
            let (a, b) = (log.clone(), log.clone());
            TopNav::new("nav")
                .start_content(
                    TopNavItem::new("go", "Go")
                        .on_click(move |_, _| a.borrow_mut().push("go".into())),
                )
                .start_content(TopNavItem::new("link", "Link").href("https://example.com/a"))
                .start_content(
                    TopNavItem::new("off", "Off")
                        .href("https://example.com/off")
                        .disabled(true)
                        .on_click(move |_, _| b.borrow_mut().push("off".into())),
                )
                .into_any_element()
        })
    });
    settle(cx);
    click(cx, "go");
    click(cx, "link");
    click(cx, "off");
    assert_eq!(
        *log.borrow(),
        vec!["go".to_string(), "open https://example.com/a".to_string()],
        "a disabled item neither navigates nor fires"
    );
}

#[gpui::test]
fn item_fr5_a_drawer_row_closes_the_drawer_but_not_after_a_modified_link_click(
    cx: &mut TestAppContext,
) {
    let log = Log::default();
    logging_links(cx, &log);
    let cx = host(cx, {
        let log = log.clone();
        page(move || {
            let (a, closed) = (log.clone(), log.clone());
            let close = AppShellMobile::new(true, move |open, _, _| {
                closed.borrow_mut().push(format!("open {open}"))
            });
            provide(
                close,
                provide(
                    TopNavRenderMode::Drawer,
                    TopNav::new("nav")
                        .start_content(
                            TopNavItem::new("go", "Go")
                                .on_click(move |_, _| a.borrow_mut().push("go".into())),
                        )
                        .start_content(
                            TopNavItem::new("link", "Link").href("https://example.com/a"),
                        ),
                ),
            )
            .into_any_element()
        })
    });
    settle(cx);
    // The drawer slides in first.
    wait(cx, 600);
    click(cx, "go");
    assert_eq!(
        *log.borrow(),
        vec!["go".to_string(), "open false".to_string()]
    );
    log.borrow_mut().clear();
    let at = cx.debug_bounds("link").unwrap().center();
    cx.simulate_click(at, Modifiers::control());
    settle(cx);
    assert_eq!(
        *log.borrow(),
        vec!["open https://example.com/a".to_string()],
        "a modified click on a link leaves the drawer open"
    );
}

fn menu_bar(cx: &mut TestAppContext, log: Log) -> &mut VisualTestContext {
    logging_links(cx, &log);
    host(
        cx,
        page(move || {
            let log = log.clone();
            TopNav::new("nav")
                .start_content(TopNavMenu::new(
                    "res",
                    "Resources",
                    vec![
                        TopNavMenuItemData::new("Docs")
                            .on_click(move |_, _| log.borrow_mut().push("docs".into())),
                        TopNavMenuItemData::new("Blog"),
                    ],
                ))
                .into_any_element()
        }),
    )
}

#[gpui::test]
fn menu_fr2_hover_opens_after_the_delay_and_leaving_closes_it(cx: &mut TestAppContext) {
    let cx = menu_bar(cx, Log::default());
    settle(cx);
    hover(cx, "res");
    wait(cx, 100);
    assert!(!shown(cx, "res-item-0"), "not before the delay");
    wait(cx, 60);
    assert!(shown(cx, "res-item-0"), "open after 150 ms");
    away(cx, "res");
    wait(cx, 100);
    assert!(shown(cx, "res-item-0"), "the hide delay holds it");
    wait(cx, 150);
    assert!(!shown(cx, "res-item-0"), "closed after the hide delay");
}

#[gpui::test]
fn menu_fr4_a_mouse_pick_leaves_it_open_and_a_keyboard_pick_closes_it(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = menu_bar(cx, log.clone());
    settle(cx);
    click(cx, "res");
    assert!(shown(cx, "res-item-0"), "a click opens it");
    click(cx, "res-item-0");
    assert_eq!(*log.borrow(), vec!["docs".to_string()]);
    assert!(shown(cx, "res-item-0"), "a mouse pick leaves it open");
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert_eq!(log.borrow().len(), 2);
    assert!(!shown(cx, "res-item-0"), "a keyboard pick closes it");
}

fn mega_bar(cx: &mut TestAppContext, log: Log) -> &mut VisualTestContext {
    logging_links(cx, &log);
    host(
        cx,
        page(move || {
            let (a, b) = (log.clone(), log.clone());
            TopNav::new("nav")
                .start_content(
                    TopNavMegaMenu::new("mega", "Products")
                        .on_open_change(move |open, _, _| {
                            a.borrow_mut().push(format!("open {open}"))
                        })
                        .item(TopNavMegaMenuItem::new("m-1", "One").href("https://example.com/1"))
                        .item(TopNavMegaMenuItem::new("m-2", "Two"))
                        .featured(TopNavMegaMenuFeaturedCard::new("card", "New")),
                )
                .start_content(
                    TopNavMegaMenu::new("other", "Other")
                        .on_open_change(move |open, _, _| {
                            b.borrow_mut().push(format!("other {open}"))
                        })
                        .item(TopNavMegaMenuItem::new("o-1", "Uno").href("https://example.com/u")),
                )
                .into_any_element()
        }),
    )
}

#[gpui::test]
fn mega_fr3_fr7_hover_opens_and_leaving_closes_and_both_report(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = mega_bar(cx, log.clone());
    settle(cx);
    hover(cx, "mega");
    wait(cx, 160);
    assert!(shown(cx, "mega-panel"), "open after the delay");
    away(cx, "mega");
    wait(cx, 300);
    assert!(!shown(cx, "mega-panel"), "closed after the hide delay");
    assert_eq!(
        *log.borrow(),
        vec!["open true".to_string(), "open false".to_string()]
    );
}

#[gpui::test]
fn mega_fr4_escape_closes_it(cx: &mut TestAppContext) {
    let cx = mega_bar(cx, Log::default());
    settle(cx);
    click(cx, "mega");
    assert!(shown(cx, "mega-panel"));
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(!shown(cx, "mega-panel"));
}

#[gpui::test]
fn mega_fr2_opening_a_sibling_closes_it(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = mega_bar(cx, log.clone());
    settle(cx);
    hover(cx, "mega");
    wait(cx, 160);
    assert!(shown(cx, "mega-panel"));
    log.borrow_mut().clear();
    // INV11: from an open menu of the same nav, the next shows at once.
    hover(cx, "other");
    assert!(shown(cx, "other-panel"), "the sibling shows at once");
    assert!(!shown(cx, "mega-panel"), "and this one closes");
    assert!(log.borrow().contains(&"open false".to_string()));
}

#[gpui::test]
fn mega_item_fr1_a_mouse_pick_runs_and_leaves_the_panel_open(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = mega_bar(cx, log.clone());
    settle(cx);
    click(cx, "mega");
    log.borrow_mut().clear();
    click(cx, "m-1");
    assert_eq!(
        *log.borrow(),
        vec!["open https://example.com/1".to_string()]
    );
    assert!(shown(cx, "mega-panel"), "a mouse pick leaves it open");
}

#[gpui::test]
fn mega_fr9_the_drawer_toggles_its_group(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        page(|| {
            provide(
                AppShellMobile::new(true, |_, _, _| {}),
                provide(
                    TopNavRenderMode::Drawer,
                    TopNav::new("nav").start_content(
                        TopNavMegaMenu::new("mega", "Products").item(
                            TopNavMegaMenuItem::new("m-1", "One").href("https://example.com/1"),
                        ),
                    ),
                ),
            )
            .into_any_element()
        }),
    );
    settle(cx);
    wait(cx, 600);
    assert!(!shown(cx, "m-1"), "collapsed content is out of reach");
    click(cx, "mega");
    assert!(shown(cx, "m-1"));
    click(cx, "mega");
    assert!(!shown(cx, "m-1"));
}

#[gpui::test]
fn heading_fr2_an_href_alone_makes_the_block_one_link(cx: &mut TestAppContext) {
    let log = Log::default();
    logging_links(cx, &log);
    let cx = host(
        cx,
        page(|| {
            TopNav::new("nav")
                .heading(
                    TopNavHeading::new("brand")
                        .heading("Astryx")
                        .subheading("Workspace")
                        .heading_href("https://example.com/home"),
                )
                .into_any_element()
        }),
    );
    settle(cx);
    click(cx, "brand");
    assert_eq!(
        *log.borrow(),
        vec!["open https://example.com/home".to_string()]
    );
}

#[gpui::test]
fn heading_fr5_hover_opens_the_menu_after_the_delay(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        page(|| {
            TopNav::new("nav")
                .heading(TopNavHeading::new("brand").heading("Astryx").menu(|| {
                    crate::nav::NavHeadingMenu::new("switcher")
                        .child(crate::nav::NavHeadingMenuItem::new("p-1", "Product"))
                        .into_any_element()
                }))
                .into_any_element()
        }),
    );
    settle(cx);
    hover(cx, "brand");
    wait(cx, 100);
    assert!(!shown(cx, "p-1"));
    wait(cx, 60);
    assert!(shown(cx, "p-1"), "open after 150 ms");
}

#[gpui::test]
fn card_fr3_the_link_needs_a_label_and_an_href(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        page(|| {
            div()
                .child(TopNavMegaMenuFeaturedCard::new("a", "A").link_label("More"))
                .child(
                    TopNavMegaMenuFeaturedCard::new("b", "B")
                        .link_label("More")
                        .link_href("https://example.com/b"),
                )
                .into_any_element()
        }),
    );
    settle(cx);
    assert!(!shown(cx, "a-link"));
    assert!(shown(cx, "b-link"));
}

fn drawer_group_eases(cx: &mut TestAppContext, mega: bool) {
    use gpui::InteractiveElement as _;
    let cx = host(
        cx,
        page(move || {
            let group: gpui::AnyElement = if mega {
                TopNavMegaMenu::new("group", "Products")
                    .item(TopNavMegaMenuItem::new("g-1", "One").href("https://example.com/1"))
                    .item(TopNavMegaMenuItem::new("g-2", "Two").href("https://example.com/2"))
                    .into_any_element()
            } else {
                TopNavMenu::new("group", "Docs", menu_items()).into_any_element()
            };
            provide(
                AppShellMobile::new(true, |_, _, _| {}),
                provide(
                    TopNavRenderMode::Drawer,
                    TopNav::new("nav").start_content(
                        div()
                            .child(group)
                            .child(div().debug_selector(|| "below".into()).h(px(10.))),
                    ),
                ),
            )
            .into_any_element()
        }),
    );
    settle(cx);
    wait(cx, 600);
    let base = cx.debug_bounds("below").unwrap().top();
    click(cx, "group");
    for _ in 0..6 {
        wait(cx, 16);
    }
    let mid = cx.debug_bounds("below").unwrap().top() - base;
    wait(cx, 600);
    let open = cx.debug_bounds("below").unwrap().top() - base;
    assert!(open > px(20.), "open: {open:?}");
    assert!(
        mid > px(0.5) && mid < open,
        "the group's height eases open: {mid:?}"
    );
    click(cx, "group");
    for _ in 0..6 {
        wait(cx, 16);
    }
    let closing = cx.debug_bounds("below").unwrap().top() - base;
    assert!(
        closing > px(0.5) && closing < open,
        "and closed: {closing:?}"
    );
    wait(cx, 600);
    assert_eq!(cx.debug_bounds("below").unwrap().top(), base);
}

#[gpui::test]
fn menu_fr7_the_drawer_group_s_height_eases(cx: &mut TestAppContext) {
    drawer_group_eases(cx, false);
}

#[gpui::test]
fn mega_fr9_the_drawer_group_s_height_eases(cx: &mut TestAppContext) {
    drawer_group_eases(cx, true);
}

#[gpui::test]
fn the_drawer_group_changes_at_once_under_reduced_motion(cx: &mut TestAppContext) {
    cx.update(|cx| astryx_primitives::set_reduced_motion(true, cx));
    let cx = host(
        cx,
        page(|| {
            provide(
                AppShellMobile::new(true, |_, _, _| {}),
                provide(
                    TopNavRenderMode::Drawer,
                    TopNav::new("nav").start_content(TopNavMenu::new(
                        "group",
                        "Docs",
                        menu_items(),
                    )),
                ),
            )
            .into_any_element()
        }),
    );
    settle(cx);
    wait(cx, 600);
    click(cx, "group");
    let rows = cx.debug_bounds("group").unwrap().bottom();
    click(cx, "group");
    assert!(cx.debug_bounds("group").unwrap().bottom() == rows);
}

fn drawer_group_is_inert_while_it_collapses(cx: &mut TestAppContext, mega: bool) {
    let log = Log::default();
    logging_links(cx, &log);
    let cx = host(
        cx,
        page(move || {
            let group: gpui::AnyElement = if mega {
                TopNavMegaMenu::new("group", "Products")
                    .item(TopNavMegaMenuItem::new("g-1", "One").href("https://example.com/1"))
                    .into_any_element()
            } else {
                TopNavMenu::new(
                    "group",
                    "Docs",
                    [TopNavMenuItemData::new("One").href("https://example.com/1")],
                )
                .into_any_element()
            };
            provide(
                AppShellMobile::new(true, |_, _, _| {}),
                provide(
                    TopNavRenderMode::Drawer,
                    TopNav::new("nav").start_content(group),
                ),
            )
            .into_any_element()
        }),
    );
    settle(cx);
    wait(cx, 600);
    let row: &'static str = if mega { "g-1" } else { "group-drawer-item-0" };
    click(cx, "group");
    wait(cx, 600);
    assert!(shown(cx, row));
    // Collapse from the header, which keeps focus.
    click(cx, "group");
    wait(cx, 48);
    assert!(shown(cx, row), "held while it collapses");
    // A press on its row reaches nothing.
    click(cx, row);
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
    // Tab skips its rows, so Enter opens none of them.
    crate::segmented::tests::press(cx, "tab");
    crate::segmented::tests::press(cx, "enter");
    settle(cx);
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
}

#[gpui::test]
fn menu_fr7_a_collapsing_drawer_group_is_inert(cx: &mut TestAppContext) {
    drawer_group_is_inert_while_it_collapses(cx, false);
}

#[gpui::test]
fn mega_fr9_a_collapsing_drawer_group_is_inert(cx: &mut TestAppContext) {
    drawer_group_is_inert_while_it_collapses(cx, true);
}
