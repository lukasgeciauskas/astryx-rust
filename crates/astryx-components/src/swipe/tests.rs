//! Swipe tests, after upstream `Swipe/createSwipe.test.ts`, `Item/Item.swipeLayer.test.tsx`,
//! `Item/Item.test.tsx`'s `swipe` cases, `List/ListItem.swipeKey.test.tsx` and spec:AST-057.
//! The release rules (`swipeRelease.test.ts`) are tested in `release.rs`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, IntoElement, Modifiers, ParentElement, Render, Styled,
    TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext, Window, div, point, px,
};

use super::layer::{DRAWN, Drawn};
use super::row::SwipePhase;
use super::*;
use crate::drag_to_snap_points::snap_drag::WARNINGS;
use crate::item::Item;
use crate::list::{List, ListItem};

type Build = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Host(Build);

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child((self.0)(window, cx))
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn frames(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..ms.div_ceil(16) {
        cx.executor().advance_clock(Duration::from_millis(16));
        settle(cx);
    }
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn touch(cx: &mut VisualTestContext, phase: TouchPhase, x: f32, y: f32) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: point(px(x), px(y)),
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

/// A finger from `from` by `dx` in `steps` moves of `ms` each, without lifting.
fn drag_to(cx: &mut VisualTestContext, from: f32, dx: f32, steps: u32, ms: u64) {
    touch(cx, TouchPhase::Started, from, ROW_Y);
    for step in 1..=steps {
        wait(cx, ms);
        touch(
            cx,
            TouchPhase::Moved,
            from + dx * step as f32 / steps as f32,
            ROW_Y,
        );
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

/// Lifts the finger at `x`, after resting `rest` ms (a long rest: no velocity).
fn lift(cx: &mut VisualTestContext, x: f32, rest: u64) {
    wait(cx, rest);
    touch(cx, TouchPhase::Ended, x, ROW_Y);
    settle(cx);
}

/// A slow drag by `dx` from `from`, lifted at rest.
fn slow_swipe(cx: &mut VisualTestContext, from: f32, dx: f32) {
    drag_to(cx, from, dx, 10, 30);
    lift(cx, from + dx, 200);
}

/// A quick flick by `dx` from `from`.
fn flick(cx: &mut VisualTestContext, from: f32, dx: f32) {
    drag_to(cx, from, dx, 3, 8);
    lift(cx, from + dx, 4);
}

fn tap(cx: &mut VisualTestContext, x: f32) {
    touch(cx, TouchPhase::Started, x, ROW_Y);
    wait(cx, 40);
    touch(cx, TouchPhase::Ended, x, ROW_Y);
    settle(cx);
}

fn click(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_click(point(px(x), px(y)), Modifiers::default());
    settle(cx);
}

fn drawn(id: &str) -> Drawn {
    DRAWN
        .with(|d| d.borrow().get(id).cloned())
        .expect("the row drew its swipe")
}

fn take_warnings() -> Vec<String> {
    WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

const ROW_Y: f32 = 15.;
const ROW_WIDTH: f32 = 300.;

type Log = Rc<RefCell<Vec<String>>>;

fn logger(
    log: &Log,
    name: &'static str,
) -> impl Fn(&SwipeActivation<&'static str>, &mut Window, &mut App) + 'static {
    let log = log.clone();
    move |a, _, _| log.borrow_mut().push(format!("{name}:{:?}", a.item_key))
}

/// Two trailing entries (Flag, then Delete outermost) and one leading (Unread).
fn mail(log: &Log) -> Swipe<&'static str> {
    create_swipe(
        CreateSwipeOptions::new()
            .leading([SwipeAction::new("unread", "Unread", logger(log, "unread"))])
            .trailing([
                SwipeAction::new("flag", "Flag", logger(log, "flag")),
                SwipeAction::new("delete", "Delete", logger(log, "delete"))
                    .variant(ItemSwipeActionVariant::Destructive),
            ]),
    )
}

fn one_trailing(log: &Log, behavior: ItemSwipeBehavior, removal: bool) -> Swipe<&'static str> {
    create_swipe(
        CreateSwipeOptions::new()
            .trailing([
                SwipeAction::new("archive", "Archive", logger(log, "archive")).has_removal(removal),
            ])
            .swipe_behavior(behavior),
    )
}

/// A 300 px wide clipping host with one interactive row given `swipe`.
fn row_page<'a>(
    cx: &'a mut TestAppContext,
    swipe: Rc<RefCell<Swipe<&'static str>>>,
    clicks: Rc<RefCell<usize>>,
) -> &'a mut VisualTestContext {
    host(cx, move |_, _| {
        let clicks = clicks.clone();
        div()
            .w(px(ROW_WIDTH))
            .overflow_x_hidden()
            .child(
                Item::new("row", "Inbox")
                    .on_click(move |_, _, _| *clicks.borrow_mut() += 1)
                    .swipe(swipe.borrow().clone()),
            )
            .into_any_element()
    })
}

type Setup<'a> = (
    &'a mut VisualTestContext,
    Rc<RefCell<usize>>,
    Rc<RefCell<Swipe<&'static str>>>,
);

fn setup<'a>(cx: &'a mut TestAppContext, swipe: Swipe<&'static str>) -> Setup<'a> {
    let clicks = Rc::new(RefCell::new(0));
    let slot = Rc::new(RefCell::new(swipe));
    let cx = row_page(cx, slot.clone(), clicks.clone());
    (cx, clicks, slot)
}

// --- createSwipe.test.ts ------------------------------------------------------------

#[test]
fn create_swipe_carries_the_entries_per_side_and_reveal_by_default() {
    let log: Log = Rc::default();
    let swipe = mail(&log);
    assert_eq!(swipe.host.leading.len(), 1);
    assert_eq!(swipe.host.trailing.len(), 2);
    assert_eq!(swipe.host.behavior, ItemSwipeBehavior::Reveal);
    let commit = one_trailing(&log, ItemSwipeBehavior::Commit, false);
    assert_eq!(commit.host.behavior, ItemSwipeBehavior::Commit);
    assert!(
        create_swipe::<&str>(CreateSwipeOptions::new())
            .host
            .leading
            .is_empty()
    );
}

#[test]
fn create_swipe_serves_a_roleless_row_and_a_listitem_and_not_a_role_that_forbids_interactive_descendants()
 {
    let log: Log = Rc::default();
    let host = one_trailing(&log, ItemSwipeBehavior::Reveal, false).host;
    let row = SwipeRowFacts::default();
    assert!(host.serves(row));
    assert!(host.serves(SwipeRowFacts {
        role: Some("listitem"),
        ..row
    }));
    for role in ["option", "menuitem", "menuitemcheckbox", "menuitemradio"] {
        assert!(
            !host.serves(SwipeRowFacts {
                role: Some(role),
                ..row
            }),
            "{role}"
        );
    }
}

#[test]
fn create_swipe_serves_no_disabled_row_no_delegate_and_no_link_root() {
    let log: Log = Rc::default();
    let host = one_trailing(&log, ItemSwipeBehavior::Reveal, false).host;
    let row = SwipeRowFacts::default();
    assert!(!host.serves(SwipeRowFacts {
        disabled: true,
        ..row
    }));
    assert!(!host.serves(SwipeRowFacts {
        delegate: true,
        ..row
    }));
    assert!(!host.serves(SwipeRowFacts {
        link_root: true,
        ..row
    }));
    assert!(
        !create_swipe::<&str>(CreateSwipeOptions::new())
            .host
            .serves(row)
    );
}

#[test]
fn create_swipe_warns_in_development_when_a_committing_side_has_more_than_one_entry() {
    let log: Log = Rc::default();
    take_warnings();
    let _ = one_trailing(&log, ItemSwipeBehavior::Commit, false);
    let _ = mail(&log);
    assert!(take_warnings().is_empty());
    let _ = create_swipe(
        CreateSwipeOptions::new()
            .trailing([
                SwipeAction::new("a", "A", logger(&log, "a")),
                SwipeAction::new("b", "B", logger(&log, "b")),
            ])
            .swipe_behavior(ItemSwipeBehavior::Commit),
    );
    assert!(
        take_warnings()
            .iter()
            .any(|w| w.contains("swipeBehavior: 'commit'"))
    );
}

#[test]
fn fr1_the_ids_a_swipe_carries_are_its_type() {
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Mail {
        Archive,
        Delete,
    }
    let fired: Rc<RefCell<Vec<Mail>>> = Rc::default();
    let f = fired.clone();
    let swipe: Swipe<Mail> = create_swipe(CreateSwipeOptions::new().trailing([
        SwipeAction::new(Mail::Archive, "Archive", move |a, _, _| {
            f.borrow_mut().push(a.id)
        }),
        SwipeAction::new(Mail::Delete, "Delete", |_, _, _| {}),
    ]));
    assert_eq!(swipe.host.ids_key(), "\u{1}Archive\u{0}Delete");
}

// --- Item.swipeLayer.test.tsx -------------------------------------------------------

#[gpui::test]
fn renders_a_row_without_swipe_actions_with_no_swipe_layer(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        div()
            .w(px(ROW_WIDTH))
            .child(Item::new("plain", "Inbox").on_click(|_, _, _| {}))
            .into_any_element()
    });
    assert!(cx.debug_bounds("plain").is_some());
    assert!(cx.debug_bounds("plain-swipe-panel-trailing").is_none());
    assert!(cx.debug_bounds("plain-swipe-panel-leading").is_none());
}

#[gpui::test]
fn renders_the_layer_of_a_row_given_swipe_at_its_first_render(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, one_trailing(&log, ItemSwipeBehavior::Reveal, false));
    assert!(cx.debug_bounds("row-swipe-panel-trailing").is_some());
    // No leading entries: no leading panel.
    assert!(cx.debug_bounds("row-swipe-panel-leading").is_none());
}

// --- Item.test.tsx: swipe ---------------------------------------------------------------

#[gpui::test]
fn keeps_the_panel_out_of_reach_at_rest_with_no_pixel_of_it_painted(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, mail(&log));
    let d = drawn("row");
    assert_eq!(d.travel, 0.);
    assert_eq!(d.state.phase, SwipePhase::Idle);
    // The panel covers the row's resting box; its track sits beside it.
    let row = cx.debug_bounds("row").unwrap();
    let panel = cx.debug_bounds("row-swipe-panel-trailing").unwrap();
    assert_eq!(panel, row);
}

#[gpui::test]
fn claims_the_touch_at_8_px_and_holds_the_row_at_the_fingers_travel(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, mail(&log));
    touch(cx, TouchPhase::Started, 250., ROW_Y);
    wait(cx, 16);
    touch(cx, TouchPhase::Moved, 245., ROW_Y);
    settle(cx);
    assert_eq!(drawn("row").travel, 0., "short of the claim the row stays");
    for x in [235., 220., 190.] {
        wait(cx, 16);
        touch(cx, TouchPhase::Moved, x, ROW_Y);
        settle(cx);
    }
    let d = drawn("row");
    assert_eq!(d.state.phase, SwipePhase::Dragging);
    assert!(
        (d.travel + 60.).abs() < 0.01,
        "at the finger's travel since touch-down: {}",
        d.travel
    );
    // Back past the start takes the row home and no further.
    wait(cx, 16);
    touch(cx, TouchPhase::Moved, 280., ROW_Y);
    settle(cx);
    assert_eq!(drawn("row").travel, 0.);
    lift(cx, 280., 100);
}

#[gpui::test]
fn reveal_springs_back_when_released_short_of_half_the_panel(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, mail(&log));
    slow_swipe(cx, 250., -50.);
    frames(cx, 1500);
    let d = drawn("row");
    assert_eq!(d.state.phase, SwipePhase::Idle);
    assert!(d.travel.abs() < 0.5, "{}", d.travel);
}

#[gpui::test]
fn reveal_rests_open_past_half_the_panel_and_fires_a_tapped_entry(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, clicks, _) = setup(cx, mail(&log));
    slow_swipe(cx, 250., -120.);
    frames(cx, 1500);
    let d = drawn("row");
    assert_eq!(d.state.phase, SwipePhase::Resting);
    assert_eq!(d.state.side, super::release::SwipeSide::Trailing);
    let open = d.geometry[1].open;
    assert!(open >= 144., "two entries of at least 72 px: {open}");
    assert!((d.travel + open).abs() < 0.5, "{}", d.travel);
    // The inner entry, Flag, sits at the outer edge's inner side.
    let flag = (ROW_WIDTH as f64 - open + 10.) as f32;
    click(cx, flag, ROW_Y);
    assert_eq!(log.borrow().as_slice(), ["flag:None"]);
    assert_eq!(
        *clicks.borrow(),
        0,
        "the entry's click does not reach the row"
    );
    frames(cx, 1500);
    assert_eq!(
        drawn("row").state.phase,
        SwipePhase::Idle,
        "firing closes the row"
    );
}

#[gpui::test]
fn reveal_opens_on_a_flick_from_short_of_half_the_panel_and_fires_nothing(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, mail(&log));
    flick(cx, 250., -40.);
    frames(cx, 1500);
    assert_eq!(drawn("row").state.phase, SwipePhase::Resting);
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn reveal_closes_an_open_row_on_a_flick_back(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, mail(&log));
    slow_swipe(cx, 250., -120.);
    frames(cx, 1500);
    assert_eq!(drawn("row").state.phase, SwipePhase::Resting);
    flick(cx, 60., 40.);
    frames(cx, 1500);
    assert_eq!(drawn("row").state.phase, SwipePhase::Idle);
}

#[gpui::test]
fn fr7_closes_a_resting_row_on_a_pointer_outside_it(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, clicks, _) = setup(cx, mail(&log));
    slow_swipe(cx, 250., -120.);
    frames(cx, 1500);
    assert!(drawn("row").state.phase == SwipePhase::Resting);
    click(cx, 100., 200.);
    frames(cx, 1500);
    assert_eq!(drawn("row").state.phase, SwipePhase::Idle);
    assert_eq!(*clicks.borrow(), 0);
}

#[gpui::test]
fn fr7_closes_a_resting_row_on_escape_with_focus_inside(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, mail(&log));
    slow_swipe(cx, 250., -120.);
    frames(cx, 1500);
    crate::segmented::tests::press(cx, "tab");
    settle(cx);
    crate::segmented::tests::press(cx, "escape");
    settle(cx);
    frames(cx, 1500);
    assert_eq!(drawn("row").state.phase, SwipePhase::Idle);
}

#[gpui::test]
fn fr7_closes_a_resting_row_on_a_tap_on_its_own_content_and_the_tap_activates_nothing(
    cx: &mut TestAppContext,
) {
    let log: Log = Rc::default();
    let (cx, clicks, _) = setup(cx, mail(&log));
    slow_swipe(cx, 250., -120.);
    frames(cx, 1500);
    tap(cx, 60.);
    frames(cx, 1500);
    assert_eq!(drawn("row").state.phase, SwipePhase::Idle);
    assert_eq!(*clicks.borrow(), 0, "the tap activates nothing");
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn ar3_leaves_activation_without_a_touch_unchanged_while_open(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, clicks, _) = setup(cx, mail(&log));
    slow_swipe(cx, 250., -120.);
    frames(cx, 1500);
    // A mouse click on the content reaches the row, which stays open.
    click(cx, 60., ROW_Y);
    assert_eq!(*clicks.borrow(), 1);
    assert_eq!(drawn("row").state.phase, SwipePhase::Resting);
}

#[gpui::test]
fn ar2_never_swipes_under_a_mouse(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, mail(&log));
    cx.simulate_mouse_down(
        point(px(250.), px(ROW_Y)),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    for x in [240., 200., 150., 100.] {
        wait(cx, 16);
        cx.simulate_mouse_move(
            point(px(x), px(ROW_Y)),
            Some(gpui::MouseButton::Left),
            Modifiers::default(),
        );
        settle(cx);
    }
    assert_eq!(drawn("row").travel, 0.);
    cx.simulate_mouse_up(
        point(px(100.), px(ROW_Y)),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    settle(cx);
    assert_eq!(drawn("row").state.phase, SwipePhase::Idle);
}

#[gpui::test]
fn does_not_move_toward_a_side_without_entries(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, one_trailing(&log, ItemSwipeBehavior::Reveal, false));
    drag_to(cx, 50., 120., 8, 16);
    assert_eq!(
        drawn("row").travel,
        0.,
        "no leading entries: no drag toward the inline end"
    );
    lift(cx, 170., 100);
}

#[gpui::test]
fn the_full_swipe_opens_a_two_entry_side_from_closed_and_fires_nothing(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, mail(&log));
    slow_swipe(cx, 290., -280.);
    frames(cx, 1500);
    assert!(
        log.borrow().is_empty(),
        "DEC-9: only a drag from the open row fires"
    );
    assert_eq!(drawn("row").state.phase, SwipePhase::Resting);
}

#[gpui::test]
fn the_full_swipe_from_the_open_row_fires_the_outermost_entry_and_springs_back(
    cx: &mut TestAppContext,
) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, mail(&log));
    slow_swipe(cx, 250., -120.);
    frames(cx, 1500);
    assert_eq!(drawn("row").state.phase, SwipePhase::Resting);
    // From the open row, a long drag confirms: the row runs ahead and the outermost
    // entry covers its place.
    drag_to(cx, 140., -135., 10, 30);
    frames(cx, 1000);
    let d = drawn("row");
    assert!(d.armed && d.state.armed, "confirmed");
    assert_eq!(
        d.state.cover,
        Some(1),
        "the outermost entry covers the panel"
    );
    assert!(d.confirm > 0.99, "{}", d.confirm);
    lift(cx, 5., 200);
    frames(cx, 2000);
    assert_eq!(
        log.borrow().as_slice(),
        ["delete:None"],
        "the outermost, and no other"
    );
    let d = drawn("row");
    assert_eq!(
        d.state.phase,
        SwipePhase::Idle,
        "no removal: it springs back"
    );
    assert!(d.travel.abs() < 0.5);
}

#[gpui::test]
fn fr5_holds_the_row_out_after_a_full_swipe_with_has_removal_firing_once_its_content_has_left(
    cx: &mut TestAppContext,
) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, one_trailing(&log, ItemSwipeBehavior::Reveal, true));
    drag_to(cx, 290., -280., 12, 30);
    frames(cx, 1000);
    assert!(drawn("row").state.armed);
    lift(cx, 10., 200);
    let mut fired_at = None;
    for _ in 0..120 {
        frames(cx, 16);
        if !log.borrow().is_empty() && fired_at.is_none() {
            fired_at = Some(drawn("row").travel);
        }
    }
    let at = fired_at.expect("fired");
    assert!(
        at <= -(ROW_WIDTH as f64) + 0.5,
        "fired once past the row's edge: {at}"
    );
    assert_eq!(log.borrow().as_slice(), ["archive:None"]);
    assert_eq!(drawn("row").state.phase, SwipePhase::Held, "it holds out");
}

#[gpui::test]
fn the_full_swipe_never_confirms_toward_a_disabled_outermost_entry(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let swipe = create_swipe(CreateSwipeOptions::new().trailing([
        SwipeAction::new("archive", "Archive", logger(&log, "archive")).disabled(true),
    ]));
    let (cx, _, _) = setup(cx, swipe);
    drag_to(cx, 290., -280., 12, 30);
    frames(cx, 300);
    assert!(!drawn("row").state.armed);
    lift(cx, 10., 200);
    frames(cx, 1500);
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn commit_slides_out_and_fires_on_a_confirmed_full_swipe_and_nothing_rests(
    cx: &mut TestAppContext,
) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, one_trailing(&log, ItemSwipeBehavior::Commit, false));
    drag_to(cx, 290., -280., 12, 30);
    frames(cx, 1000);
    lift(cx, 10., 200);
    frames(cx, 2500);
    assert_eq!(log.borrow().as_slice(), ["archive:None"]);
    assert_eq!(drawn("row").state.phase, SwipePhase::Idle);
}

#[gpui::test]
fn commit_springs_back_from_any_release_short_of_the_commit_point(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, one_trailing(&log, ItemSwipeBehavior::Commit, false));
    slow_swipe(cx, 250., -150.);
    frames(cx, 1500);
    assert_eq!(drawn("row").state.phase, SwipePhase::Idle);
    flick(cx, 250., -60.);
    frames(cx, 1500);
    assert_eq!(
        drawn("row").state.phase,
        SwipePhase::Idle,
        "a flick rests nothing"
    );
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn fr11_keeps_an_open_row_open_when_a_new_value_keeps_its_ids_and_closes_it_for_other_ids(
    cx: &mut TestAppContext,
) {
    let log: Log = Rc::default();
    let (cx, _, slot) = setup(cx, mail(&log));
    slow_swipe(cx, 250., -120.);
    frames(cx, 1500);
    *slot.borrow_mut() = mail(&log);
    settle(cx);
    frames(cx, 200);
    assert_eq!(
        drawn("row").state.phase,
        SwipePhase::Resting,
        "same ids: still open"
    );
    *slot.borrow_mut() = one_trailing(&log, ItemSwipeBehavior::Reveal, false);
    settle(cx);
    frames(cx, 1500);
    assert_eq!(
        drawn("row").state.phase,
        SwipePhase::Idle,
        "other ids: it closes"
    );
}

#[gpui::test]
fn reduced_motion_lands_a_release_without_travel_through_a_cross_fade(cx: &mut TestAppContext) {
    cx.update(|cx| astryx_primitives::set_reduced_motion(true, cx));
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, mail(&log));
    slow_swipe(cx, 250., -120.);
    let mut faded = false;
    for _ in 0..30 {
        frames(cx, 16);
        let d = drawn("row");
        faded |= d.opacity < 0.99;
        // The row never sits between where it was let go and its rest.
        assert!(
            d.travel <= -119. || (d.travel + d.geometry[1].open).abs() < 0.5,
            "{}",
            d.travel
        );
    }
    assert!(
        faded,
        "a finger's release cross-fades (design:reduced-motion DR2)"
    );
    let d = drawn("row");
    assert_eq!(d.state.phase, SwipePhase::Resting);
    assert!((d.travel + d.geometry[1].open).abs() < 0.5);
}

#[gpui::test]
fn the_deprecated_swipe_actions_still_rest_a_row_open_and_fire_a_tapped_entry(
    cx: &mut TestAppContext,
) {
    let fired = Rc::new(RefCell::new(0));
    let f = fired.clone();
    let cx = host(cx, move |_, _| {
        let f = f.clone();
        div()
            .w(px(ROW_WIDTH))
            .overflow_x_hidden()
            .child(
                Item::new("row", "Inbox")
                    .on_click(|_, _, _| {})
                    .swipe_actions(ItemSwipeActions {
                        leading: vec![],
                        trailing: vec![ItemSwipeAction::new("Archive", move |_, _| {
                            *f.borrow_mut() += 1
                        })],
                    }),
            )
            .into_any_element()
    });
    slow_swipe(cx, 250., -60.);
    frames(cx, 1500);
    assert_eq!(drawn("row").state.phase, SwipePhase::Resting);
    click(cx, ROW_WIDTH - 20., ROW_Y);
    assert_eq!(*fired.borrow(), 1);
}

#[test]
fn the_deprecated_props_give_way_to_swipe_with_a_development_note() {
    let log: Log = Rc::default();
    take_warnings();
    let host = resolve_row_swipe(
        Some(one_trailing(&log, ItemSwipeBehavior::Reveal, false).host),
        Some(&ItemSwipeActions::default()),
        None,
    );
    assert!(host.is_some_and(|h| h.trailing.len() == 1));
    assert!(take_warnings().iter().any(|w| w.contains("deprecated")));
}

// --- ListItem.swipeKey.test.tsx and spec:AST-057 FR1 ---------------------------------------

#[gpui::test]
fn a_list_item_keys_its_swipe_value_once_per_value_and_key(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let swipe = mail(&log);
    let state = Rc::new(RefCell::new(("Inbox", 1i64)));
    let s = state.clone();
    KEYED.with(|k| k.set(0));
    let cx = host(cx, move |_, _| {
        let (label, key) = *s.borrow();
        List::new("mail")
            .w(px(ROW_WIDTH))
            .child(ListItem::new("m", label).item_key(key).swipe(swipe.clone()))
            .into_any_element()
    });
    let first = KEYED.with(|k| k.get());
    assert_eq!(first, 1);
    *state.borrow_mut() = ("Inbox, read", 1);
    settle(cx);
    assert_eq!(
        KEYED.with(|k| k.get()),
        first,
        "the same value and key: no new value"
    );
    *state.borrow_mut() = ("Inbox, read", 2);
    settle(cx);
    assert_eq!(
        KEYED.with(|k| k.get()),
        first + 1,
        "another key: keyed again"
    );
}

#[gpui::test]
fn fr1_hands_each_rows_on_activate_the_actions_id_and_that_rows_item_key(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let swipe = one_trailing(&log, ItemSwipeBehavior::Reveal, false);
    let cx = host(cx, move |_, _| {
        div()
            .w(px(ROW_WIDTH))
            .overflow_x_hidden()
            .child(
                List::new("mail").edge(crate::list::ListEdge::Flush).child(
                    ListItem::new("m", "Inbox")
                        .item_key(7i64)
                        .on_click(|_, _, _| {})
                        .swipe(swipe.clone()),
                ),
            )
            .into_any_element()
    });
    let row = cx.debug_bounds("m").expect("the row");
    let y = f32::from(row.center().y);
    let right = f32::from(row.right());
    touch(cx, TouchPhase::Started, right - 40., y);
    for step in 1..=8 {
        wait(cx, 30);
        touch(cx, TouchPhase::Moved, right - 40. - 8. * step as f32, y);
    }
    wait(cx, 200);
    touch(cx, TouchPhase::Ended, right - 104., y);
    frames(cx, 1500);
    click(cx, right - 20., y);
    assert_eq!(log.borrow().as_slice(), ["archive:Some(Number(7))"]);
}

#[gpui::test]
fn fr8_directions_are_logical_under_right_to_left(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let swipe = mail(&log);
    let cx = host(cx, move |_, _| {
        astryx_i18n::provider(
            astryx_i18n::I18n::new("ar"),
            div().w(px(ROW_WIDTH)).overflow_x_hidden().child(
                Item::new("row", "Inbox")
                    .on_click(|_, _, _| {})
                    .swipe(swipe.clone()),
            ),
        )
        .into_any_element()
    });
    // Toward the inline start (rightward) uncovers the trailing side, which sits at
    // the inline end, on the left.
    slow_swipe(cx, 50., 120.);
    frames(cx, 1500);
    let d = drawn("row");
    assert_eq!(d.state.phase, SwipePhase::Resting);
    assert_eq!(d.state.side, super::release::SwipeSide::Trailing);
    assert!((d.travel - d.geometry[1].open).abs() < 0.5, "{}", d.travel);
    assert_eq!(d.geometry[1].sign, 1., "the panel is on the physical left");
}

#[gpui::test]
fn reveal_holds_the_row_out_after_a_tapped_entry_with_has_removal_firing_once_its_content_has_left(
    cx: &mut TestAppContext,
) {
    let log: Log = Rc::default();
    let (cx, _, _) = setup(cx, one_trailing(&log, ItemSwipeBehavior::Reveal, true));
    slow_swipe(cx, 250., -60.);
    frames(cx, 1500);
    assert_eq!(drawn("row").state.phase, SwipePhase::Resting);
    click(cx, ROW_WIDTH - 20., ROW_Y);
    assert!(
        log.borrow().is_empty(),
        "not at the tap: once the row has left its box"
    );
    assert_eq!(
        drawn("row").state.cover,
        Some(0),
        "the tapped entry covers the panel"
    );
    frames(cx, 2000);
    assert_eq!(log.borrow().as_slice(), ["archive:None"]);
    let d = drawn("row");
    assert_eq!(d.state.phase, SwipePhase::Held, "it holds out");
    assert!(d.travel <= -(ROW_WIDTH as f64), "{}", d.travel);
}
