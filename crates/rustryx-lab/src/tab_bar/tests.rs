//! Upstream `tabBarModel.test.ts` and `uikitTabBar.test.ts` (the geometry against
//! UIKit's measurements) and `TabBar.test.tsx` (the tab list and its keys, choosing and
//! reselecting, the panel scroll, badges, the lens at rest and the More tab), with the
//! touch press of `TabBar.interaction.browser.spec.ts`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Context, Modifiers, Render, TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext,
    point,
};
use rustryx_i18n::I18n;

use super::*;

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

// ---------------------------------------------------------------------------
// tabBarModel.test.ts / uikitTabBar.test.ts

#[test]
fn the_capsule_is_no_wider_than_16_plus_86_per_tab() {
    assert_eq!(natural_width(2.), 188.);
    assert_eq!(natural_width(3.), 274.);
    assert_eq!(natural_width(4.), 360.);
}

/// UIKit's layouts as measured: the capsule's x and width, the items' width and their
/// x in screen points (upstream `UIKIT_LAYOUTS`).
const UIKIT_LAYOUTS: [(&str, f64, f64, f64, &[f64]); 6] = [
    ("2 tabs", 107., 188., 94., &[111., 197.]),
    ("3 tabs", 64., 274., 94., &[68., 154., 240.]),
    ("4 tabs", 21., 360., 98., &[25., 109.67, 194.33, 279.]),
    ("5 tabs", 21., 360., 77., &[25., 93.75, 162.5, 231.25, 300.]),
    (
        "3 tabs and search",
        21.,
        290.,
        100.67,
        &[25., 115.67, 206.33],
    ),
    (
        "4 tabs and search",
        21.,
        290.,
        76.,
        &[25., 93.67, 162.33, 231.],
    ),
];

#[test]
fn item_width_and_item_centers_match_uikit_within_a_point() {
    for (label, capsule_x, capsule, width, xs) in UIKIT_LAYOUTS {
        let n = xs.len();
        assert!(
            (item_width(capsule, n as f64) - width).abs() < 1.,
            "{label}"
        );
        for (center, x) in item_centers(capsule, n, false).iter().zip(xs) {
            let uikit = x - capsule_x + width / 2.;
            assert!((center - uikit).abs() < 1., "{label}: {center} vs {uikit}");
        }
    }
    // Never wider than the capsule's inside.
    assert_eq!(item_width(400., 1.), 392.);
}

#[test]
fn the_first_and_last_items_sit_4_from_the_edges_and_the_others_spread_evenly() {
    let size = item_width(360., 4.);
    let centers = item_centers(360., 4, false);
    assert!(close(centers[0], 4. + size / 2., 9));
    assert!(close(centers[3], 360. - 4. - size / 2., 9));
    assert!(close(centers[2] - centers[1], centers[1] - centers[0], 9));
    // Right to left, the first is on the right.
    let rtl = item_centers(360., 4, true);
    assert!(close(rtl[0], 360. - centers[0], 9));
    assert_eq!(item_centers(200., 1, false), [100.]);
    assert!(item_centers(200., 0, false).is_empty());
}

#[test]
fn the_nearest_item_takes_the_earlier_one_on_a_tie() {
    let centers = [10., 30., 50.];
    assert_eq!(nearest_index(19., &centers), 0);
    assert_eq!(nearest_index(20., &centers), 0);
    assert_eq!(nearest_index(21., &centers), 1);
    assert_eq!(nearest_index(400., &centers), 2);
}

#[test]
fn the_lens_follows_the_finger_and_rubber_bands_8_past_60_beyond_the_last_item() {
    let centers = item_centers(290., 4, false);
    assert_eq!(lens_target(150., &centers), 150.);
    // A finger 60 pt past the last item leaves the lens about 8 pt past it.
    assert!(close(
        lens_target(centers[3] + 60., &centers) - centers[3],
        7.98,
        1
    ));
    assert!(close(
        lens_target(centers[0] - 60., &centers) - centers[0],
        -7.98,
        1
    ));
    // Never past the rubber band's dimension however far the finger goes.
    assert!(lens_target(centers[3] + 10_000., &centers) - centers[3] < LENS_EDGE_DIMENSION);
    assert_eq!(lens_target(5., &[]), 5.);
}

#[test]
fn the_lifted_lens_grows_8_on_every_side() {
    let (x, y) = lift_scale(76.);
    assert!(close(x * 76., 92., 6));
    assert!(close(y * ITEM_HEIGHT, 70., 6));
    assert!(close(98. * lift_scale(98.).0, 114., 6));
    let (cx_, sx, sy) = copy_transform(20., 1.2, 1.25);
    assert!(close(cx_, -20. / 1.2, 12));
    assert!(close(sx, 1. / 1.2, 12) && close(sy, 0.8, 12));
}

#[test]
fn the_more_tab_holds_the_tabs_past_the_fourth_when_there_are_more_than_five() {
    let tabs = |n: usize| {
        (0..n)
            .map(|i| TabBarItem::new(format!("t{i}"), format!("T{i}"), "home"))
            .collect::<Vec<_>>()
    };
    assert!(overflow_items(&tabs(5)).is_empty());
    let six = tabs(6);
    let held: Vec<_> = overflow_items(&six)
        .iter()
        .map(|i| i.value().clone())
        .collect();
    assert_eq!(held, [SharedString::from("t4"), "t5".into()]);
}

// ---------------------------------------------------------------------------
// TabBar.test.tsx

type Log = Rc<RefCell<Vec<SharedString>>>;

#[derive(Clone)]
struct Setup {
    value: SharedString,
    items: Vec<TabBarItem>,
    label: Option<&'static str>,
    aria_label: Option<&'static str>,
    follows: bool,
    i18n: Option<I18n>,
}

struct Host {
    setup: Setup,
    changes: Log,
    reselects: Log,
    panels: Vec<(SharedString, Vec<ScrollHandle>)>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (changes, reselects, follows, view) = (
            self.changes.clone(),
            self.reselects.clone(),
            self.setup.follows,
            cx.entity(),
        );
        let mut bar = TabBar::new("bar", self.setup.value.clone())
            .items(self.setup.items.clone())
            .on_change(move |value, _, cx| {
                changes.borrow_mut().push(value.clone());
                if follows {
                    view.update(cx, |h, cx| {
                        h.setup.value = value;
                        cx.notify();
                    });
                }
            })
            .on_reselect(move |value, _, _| reselects.borrow_mut().push(value));
        if let Some(label) = self.setup.label {
            bar = bar.label(label);
        }
        if let Some(label) = self.setup.aria_label {
            bar = bar.aria_label(label);
        }
        let panels = self.panels.iter().map(|(id, handles)| {
            let mut panel = TabBarPanel::new(id.clone()).w(px(400.));
            for (i, handle) in handles.iter().enumerate() {
                panel = panel.scroll_area(handle).child(
                    div()
                        .id(ElementId::Name(format!("{id}-area-{i}").into()))
                        .h(px(100.))
                        .overflow_y_scroll()
                        .track_scroll(handle)
                        .child(div().h(px(if i == 0 && handles.len() > 1 {
                            100.5
                        } else {
                            2000.
                        }))),
                );
            }
            panel.into_any_element()
        });
        let el = div()
            .relative()
            .w(px(402.))
            .h(px(874.))
            .children(panels)
            .child(bar);
        match &self.setup.i18n {
            Some(i) => rustryx_i18n::provider(i.clone(), el).into_any_element(),
            None => el.into_any_element(),
        }
    }
}

fn four() -> Vec<TabBarItem> {
    vec![
        TabBarItem::new("home", "Home", "home"),
        TabBarItem::new("library", "Library", "book"),
        TabBarItem::new("browse", "Browse", "grid").badge_label("New items"),
        TabBarItem::new("profile", "Profile", "user"),
    ]
}

fn setup(value: &str) -> Setup {
    Setup {
        value: SharedString::from(value.to_string()),
        items: four(),
        label: Some("Sections"),
        aria_label: None,
        follows: true,
        i18n: None,
    }
}

fn mount(
    cx: &mut TestAppContext,
    setup: Setup,
) -> (Entity<Host>, &mut VisualTestContext, Log, Log) {
    mount_with(cx, setup, Vec::new())
}

fn mount_with(
    cx: &mut TestAppContext,
    setup: Setup,
    panels: Vec<(SharedString, Vec<ScrollHandle>)>,
) -> (Entity<Host>, &mut VisualTestContext, Log, Log) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (changes, reselects) = (Log::default(), Log::default());
    let (view, cx) = cx.add_window_view({
        let (changes, reselects) = (changes.clone(), reselects.clone());
        move |_, _| Host {
            setup,
            changes,
            reselects,
            panels,
        }
    });
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    (view, cx, changes, reselects)
}

fn drawn() -> Drawn {
    DRAWN
        .with(|d| d.borrow().get("bar").cloned())
        .expect("the bar drew")
}

fn tab(cx: &mut VisualTestContext, value: &str) -> gpui::Point<Pixels> {
    let selector: &'static str = Box::leak(format!("bar-tab-{value}").into_boxed_str());
    cx.debug_bounds(selector)
        .expect("the tab is drawn")
        .center()
}

fn click(cx: &mut VisualTestContext, value: &str) {
    let at = tab(cx, value);
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    cx.run_until_parked();
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn finger(cx: &mut VisualTestContext, phase: TouchPhase, at: gpui::Point<Pixels>) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: at,
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

/// A tap far from the bar, so the window reads its primary pointer as coarse.
fn as_touch(cx: &mut VisualTestContext) {
    for phase in [TouchPhase::Started, TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(99),
            phase,
            position: point(px(200.), px(100.)),
            predicted_position: None,
            force: None,
        });
    }
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(drawn().touch);
}

#[gpui::test]
fn names_its_tab_list_tabs_when_it_is_given_no_name(cx: &mut TestAppContext) {
    let mut s = setup("home");
    s.label = None;
    let _ = mount(cx, s);
    assert_eq!(drawn().list_label.as_ref(), "Tabs");
}

#[gpui::test]
fn gives_aria_label_to_the_tab_list_over_label(cx: &mut TestAppContext) {
    let mut s = setup("home");
    s.aria_label = Some("Main sections");
    let _ = mount(cx, s);
    assert_eq!(drawn().list_label.as_ref(), "Main sections");
}

#[gpui::test]
fn wires_a_tab_to_the_panel_it_controls_through_panel_id(cx: &mut TestAppContext) {
    let mut s = setup("home");
    s.items[0] = TabBarItem::new("home", "Home", "home").panel_id("home-panel");
    let _ = mount(cx, s);
    let d = drawn();
    assert_eq!(d.tabs[0].panel.as_deref(), Some("home-panel"));
    assert_eq!(d.tabs[1].panel, None);
}

#[gpui::test]
fn is_a_named_tablist_of_tabs_with_the_selected_one_marked(cx: &mut TestAppContext) {
    let _ = mount(cx, setup("library"));
    let d = drawn();
    assert_eq!(d.list_label.as_ref(), "Sections");
    let names: Vec<_> = d.tabs.iter().map(|t| t.name.to_string()).collect();
    assert_eq!(names, ["Home", "Library", "Browse", "Profile"]);
    assert_eq!(
        d.tabs.iter().map(|t| t.selected).collect::<Vec<_>>(),
        [false, true, false, false]
    );
}

#[gpui::test]
fn chooses_another_tab_with_on_change_and_moves_the_selection(cx: &mut TestAppContext) {
    let (_, cx, changes, reselects) = mount(cx, setup("home"));
    click(cx, "browse");
    assert_eq!(*changes.borrow(), [SharedString::from("browse")]);
    assert!(reselects.borrow().is_empty());
    assert!(drawn().tabs[2].selected);
}

#[gpui::test]
fn reports_choosing_the_selected_tab_again_as_a_reselect_not_a_change(cx: &mut TestAppContext) {
    let (_, cx, changes, reselects) = mount(cx, setup("home"));
    click(cx, "home");
    assert_eq!(*reselects.borrow(), [SharedString::from("home")]);
    assert!(changes.borrow().is_empty());
}

fn panel_setup() -> Setup {
    let mut s = setup("home");
    s.items[0] = TabBarItem::new("home", "Home", "home").panel_id("panel-home");
    s.items[1] = TabBarItem::new("library", "Library", "book").panel_id("panel-library");
    s
}

fn scrolled(handle: &ScrollHandle, top: f32) {
    handle.set_offset(point(px(0.), px(-top)));
}

#[gpui::test]
fn scrolls_the_selected_tabs_panel_to_its_top_when_it_is_chosen_again_gliding_there(
    cx: &mut TestAppContext,
) {
    let area = ScrollHandle::new();
    let (_, cx, _, reselects) = mount_with(
        cx,
        panel_setup(),
        vec![("panel-home".into(), vec![area.clone()])],
    );
    scrolled(&area, 120.);
    click(cx, "home");
    assert_eq!(reselects.borrow().len(), 1);
    wait(cx, 100);
    let mid = rustryx_primitives::keyboard::scroll_top_of(&area);
    assert!(mid > 0. && mid < 120., "it glides: {mid}");
    wait(cx, 400);
    assert_eq!(rustryx_primitives::keyboard::scroll_top_of(&area), 0.);
}

#[gpui::test]
fn skips_a_row_whose_content_overflows_it_by_a_pixel_or_less(cx: &mut TestAppContext) {
    // The first area overflows its 100 px box by half a pixel: Core's scroll test says
    // it does not scroll, so the list below it scrolls.
    let (chips, list) = (ScrollHandle::new(), ScrollHandle::new());
    let (_, cx, _, _) = mount_with(
        cx,
        panel_setup(),
        vec![("panel-home".into(), vec![chips.clone(), list.clone()])],
    );
    scrolled(&list, 120.);
    click(cx, "home");
    wait(cx, 600);
    assert_eq!(rustryx_primitives::keyboard::scroll_top_of(&list), 0.);
}

#[gpui::test]
fn jumps_to_the_top_under_reduce_motion(cx: &mut TestAppContext) {
    let area = ScrollHandle::new();
    let (_, cx, _, _) = mount_with(
        cx,
        panel_setup(),
        vec![("panel-home".into(), vec![area.clone()])],
    );
    cx.update(|_, cx| rustryx_primitives::set_reduced_motion(true, cx));
    scrolled(&area, 40.);
    click(cx, "home");
    assert_eq!(rustryx_primitives::keyboard::scroll_top_of(&area), 0.);
}

#[gpui::test]
fn scrolls_nothing_when_another_tab_is_chosen(cx: &mut TestAppContext) {
    let (home, library) = (ScrollHandle::new(), ScrollHandle::new());
    let (_, cx, _, _) = mount_with(
        cx,
        panel_setup(),
        vec![
            ("panel-home".into(), vec![home.clone()]),
            ("panel-library".into(), vec![library.clone()]),
        ],
    );
    scrolled(&home, 120.);
    scrolled(&library, 60.);
    click(cx, "library");
    wait(cx, 600);
    assert_eq!(rustryx_primitives::keyboard::scroll_top_of(&home), 120.);
    assert_eq!(rustryx_primitives::keyboard::scroll_top_of(&library), 60.);
}

#[gpui::test]
fn a_tab_without_a_panel_scrolls_nothing_and_still_reports_the_reselect(cx: &mut TestAppContext) {
    let area = ScrollHandle::new();
    let (_, cx, _, reselects) = mount_with(
        cx,
        setup("home"),
        vec![("panel-home".into(), vec![area.clone()])],
    );
    scrolled(&area, 80.);
    click(cx, "home");
    wait(cx, 600);
    assert_eq!(rustryx_primitives::keyboard::scroll_top_of(&area), 80.);
    assert_eq!(reselects.borrow().len(), 1);
}

#[gpui::test]
fn has_one_tab_stop_on_the_selected_tab_and_arrow_keys_move_between_tabs(cx: &mut TestAppContext) {
    let (_, cx, changes, _) = mount(cx, setup("library"));
    assert_eq!(drawn().tab_stop, Some(1));
    press(cx, "tab");
    press(cx, "right");
    assert_eq!(drawn().tab_stop, Some(2));
    press(cx, "end");
    assert_eq!(drawn().tab_stop, Some(3));
    press(cx, "home");
    assert_eq!(drawn().tab_stop, Some(0));
    // Moving focus does not choose a tab: Enter or Space does.
    assert!(changes.borrow().is_empty());
    assert!(drawn().tabs[1].selected);
    press(cx, "enter");
    assert_eq!(*changes.borrow(), [SharedString::from("home")]);
}

#[gpui::test]
fn names_a_badged_tab_with_one_catalog_message_so_a_translation_can_reorder_it(
    cx: &mut TestAppContext,
) {
    let mut s = setup("home");
    s.items[1] = TabBarItem::new("library", "Library", "book").badge(3);
    s.i18n =
        Some(I18n::new("en").overrides("en", [("@astryx.tabBar.badgedItem", "{badge} · {label}")]));
    let _ = mount(cx, s);
    assert_eq!(drawn().tabs[1].name.as_ref(), "3 · Library");
}

#[gpui::test]
fn puts_a_badge_in_its_tabs_name_and_draws_it_at_once(cx: &mut TestAppContext) {
    let mut s = setup("home");
    s.items[1] = TabBarItem::new("library", "Library", "book").badge(3);
    let (view, cx, _, _) = mount(cx, s);
    let d = drawn();
    assert_eq!(d.tabs[1].name.as_ref(), "Library, 3");
    assert_eq!(
        d.badges,
        [(SharedString::from("library"), SharedString::from("3"))]
    );
    assert!(cx.debug_bounds("bar-badge-library").is_some());
    for (badge, name) in [("12", "Library, 12"), ("", "Library")] {
        view.update(cx, |h, cx| {
            h.setup.items[1] = TabBarItem::new("library", "Library", "book").badge(badge);
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(drawn().tabs[1].name.as_ref(), name);
    }
    assert!(drawn().badges.is_empty());
    assert!(cx.debug_bounds("bar-badge-library").is_none());
}

#[gpui::test]
fn reads_a_badges_label_instead_of_the_badge_when_given_one(cx: &mut TestAppContext) {
    let mut s = setup("home");
    s.items[2] = TabBarItem::new("browse", "Browse", "grid")
        .badge("!")
        .badge_label("New items");
    let _ = mount(cx, s);
    assert_eq!(drawn().tabs[2].name.as_ref(), "Browse, New items");
}

#[gpui::test]
fn draws_the_lens_only_while_a_tab_is_selected_and_the_first_tab_keeps_the_tab_stop(
    cx: &mut TestAppContext,
) {
    let (view, cx, _, _) = mount(cx, setup("none"));
    assert!(!drawn().lens);
    assert!(cx.debug_bounds("tab-bar-lens").is_none());
    assert_eq!(drawn().tab_stop, Some(0));
    view.update(cx, |h, cx| {
        h.setup.value = "browse".into();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(drawn().lens);
    assert!(cx.debug_bounds("tab-bar-lens").is_some());
}

#[gpui::test]
fn draws_the_selected_icon_on_the_selected_tab_itself_and_each_item_once_at_rest(
    cx: &mut TestAppContext,
) {
    let drawn_icons: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let icon = |name: &'static str| {
        let log = drawn_icons.clone();
        move |_: &mut Window, _: &mut App| {
            log.borrow_mut().push(name);
            div().size(px(20.)).into_any_element()
        }
    };
    let mut s = setup("a");
    s.items = vec![
        TabBarItem::new("a", "A", "home")
            .icon_element(icon("outline"))
            .selected_icon_element(icon("filled")),
        TabBarItem::new("b", "B", "home").icon_element(icon("b")),
    ];
    let (_, cx, _, _) = mount(cx, s);
    drawn_icons.borrow_mut().clear();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(
        *drawn_icons.borrow(),
        ["filled", "b"],
        "each item once, no copy at rest"
    );
}

#[gpui::test]
fn under_a_mouse_never_mounts_a_copy(cx: &mut TestAppContext) {
    let (_, cx, _, _) = mount(cx, setup("home"));
    click(cx, "browse");
    assert!(drawn().tabs[2].selected);
    assert!(!drawn().copy_shown);
    wait(cx, 100);
    assert!(!drawn().copy_shown);
}

#[gpui::test]
fn under_a_finger_a_slide_mounts_the_copy_only_until_the_lens_rests(cx: &mut TestAppContext) {
    let (view, cx, _, _) = mount(cx, setup("home"));
    as_touch(cx);
    assert!(!drawn().copy_shown);
    // A key or the app moves the selection: the lens slides on lensTravel.
    view.update(cx, |h, cx| {
        h.setup.value = "browse".into();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        drawn().copy_shown,
        "the slide draws the selected look inside the lens"
    );
    wait(cx, 2000);
    assert!(!drawn().copy_shown, "once it rests the tab draws it again");
    assert!(close(drawn().lens_x, 0., 6));
}

#[gpui::test]
fn under_a_finger_a_tap_chooses_the_tab_once_and_the_lens_lifts_and_settles(
    cx: &mut TestAppContext,
) {
    let (_, cx, changes, reselects) = mount(cx, setup("home"));
    as_touch(cx);
    let at = tab(cx, "browse");
    finger(cx, TouchPhase::Started, at);
    wait(cx, 50);
    assert!(drawn().copy_shown, "the lifted lens carries the copy");
    assert!(drawn().flex > 1., "the capsule flexes");
    finger(cx, TouchPhase::Ended, at);
    wait(cx, 16);
    assert_eq!(*changes.borrow(), [SharedString::from("browse")]);
    assert!(reselects.borrow().is_empty());
    wait(cx, 2000);
    assert!(!drawn().copy_shown);
    assert!(close(drawn().flex, 1., 3));
}

#[gpui::test]
fn under_a_finger_a_drag_across_the_bar_chooses_the_tab_under_the_lift(cx: &mut TestAppContext) {
    let (_, cx, changes, _) = mount(cx, setup("home"));
    as_touch(cx);
    let (from, to) = (tab(cx, "home"), tab(cx, "profile"));
    finger(cx, TouchPhase::Started, from);
    for i in 1..=10 {
        wait(cx, 16);
        let s = i as f32 / 10.;
        finger(
            cx,
            TouchPhase::Moved,
            point(from.x + (to.x - from.x) * s, from.y),
        );
    }
    finger(cx, TouchPhase::Ended, to);
    wait(cx, 16);
    assert_eq!(*changes.borrow(), [SharedString::from("profile")]);
}

fn disabled_setup() -> Setup {
    let mut s = setup("home");
    s.items = vec![
        TabBarItem::new("home", "Home", "home"),
        TabBarItem::new("browse", "Browse", "grid").disabled(true),
        TabBarItem::new("profile", "Profile", "user"),
    ];
    s
}

#[gpui::test]
fn a_disabled_tab_is_marked_disabled_and_ignores_a_click(cx: &mut TestAppContext) {
    let (_, cx, changes, _) = mount(cx, disabled_setup());
    assert!(drawn().tabs[1].disabled);
    click(cx, "browse");
    assert!(changes.borrow().is_empty());
    assert!(!drawn().tabs[1].selected);
    click(cx, "profile");
    assert_eq!(*changes.borrow(), [SharedString::from("profile")]);
}

#[gpui::test]
fn a_disabled_tab_is_skipped_by_the_arrow_keys(cx: &mut TestAppContext) {
    let (_, cx, _, _) = mount(cx, disabled_setup());
    press(cx, "tab");
    press(cx, "right");
    assert_eq!(drawn().tab_stop, Some(2));
}

#[gpui::test]
fn under_a_finger_a_tap_on_a_disabled_tab_selects_nothing(cx: &mut TestAppContext) {
    let (_, cx, changes, _) = mount(cx, disabled_setup());
    as_touch(cx);
    let at = tab(cx, "browse");
    finger(cx, TouchPhase::Started, at);
    wait(cx, 50);
    assert!(
        !drawn().copy_shown,
        "a finger that lands on a disabled tab moves nothing"
    );
    finger(cx, TouchPhase::Ended, at);
    wait(cx, 50);
    assert!(changes.borrow().is_empty());
}

fn six() -> Vec<TabBarItem> {
    ["home", "library", "browse", "profile", "settings", "help"]
        .iter()
        .map(|v| {
            let label = format!("{}{}", v[..1].to_uppercase(), &v[1..]);
            TabBarItem::new(*v, label, "home")
        })
        .collect()
}

#[gpui::test]
fn shows_the_first_four_tabs_and_more_when_there_are_more_than_five(cx: &mut TestAppContext) {
    let mut s = setup("home");
    s.items = six();
    let _ = mount(cx, s);
    let names: Vec<_> = drawn().tabs.iter().map(|t| t.name.to_string()).collect();
    assert_eq!(names, ["Home", "Library", "Browse", "Profile", "More"]);
    assert_eq!(
        drawn().tabs[4].value.as_ref(),
        "more",
        "More takes moreValue"
    );
}

#[gpui::test]
fn names_the_more_tab_with_one_catalog_message_so_a_translation_can_change_it(
    cx: &mut TestAppContext,
) {
    let mut s = setup("more");
    s.items = six();
    s.i18n = Some(I18n::new("en").overrides("en", [("@astryx.tabBar.more", "Others")]));
    let _ = mount(cx, s);
    assert_eq!(drawn().tabs[4].name.as_ref(), "Others");
}

#[gpui::test]
fn selects_more_for_a_tab_it_holds_and_chooses_more_with_its_value(cx: &mut TestAppContext) {
    let mut s = setup("settings");
    s.items = six();
    s.follows = false;
    let (view, cx, changes, _) = mount(cx, s);
    assert!(drawn().tabs[4].selected);
    view.update(cx, |h, cx| {
        h.setup.value = "home".into();
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "more");
    assert_eq!(*changes.borrow(), [SharedString::from("more")]);
}

#[gpui::test]
fn more_again_while_it_holds_a_tab_is_a_reselect_back_to_its_list(cx: &mut TestAppContext) {
    let mut s = setup("help");
    s.items = six();
    let (_, cx, changes, reselects) = mount(cx, s);
    click(cx, "more");
    assert_eq!(*reselects.borrow(), [SharedString::from("more")]);
    assert!(changes.borrow().is_empty());
}
