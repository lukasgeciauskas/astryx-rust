//! Reordering a List (AST-103): reorder mode's keys, hint and
//! announcements, the drag, `start`, and a list without the prop.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Context, InteractiveElement, IntoElement, Modifiers, MouseButton, ParentElement, Point, Render,
    ScrollHandle, StatefulInteractiveElement, Styled, TestAppContext, VisualTestContext, Window,
    div, point, px,
};
use rustryx_primitives::{Politeness, ScrollHost};

use super::{
    FeedbackBehavior, ReorderMove, ReorderableLabels, ReorderableOptions, create_reorderable,
};
use crate::list::{List, ListItem};

type Items = Rc<RefCell<Vec<&'static str>>>;
type Log = Rc<RefCell<Vec<ReorderMove>>>;

struct Host {
    items: Items,
    log: Log,
    /// Apply the moves (an app that ignores them keeps its order).
    applies: bool,
    reorderable: bool,
    pinned: Option<&'static str>,
    labels: ReorderableLabels,
    feedback: FeedbackBehavior,
    scroll: ScrollHandle,
    height: f32,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (items, log, applies) = (self.items.clone(), self.log.clone(), self.applies);
        let pinned = self.pinned;
        let mut options = ReorderableOptions::new(move |m, window, _| {
            log.borrow_mut().push(m.clone());
            if applies {
                let mut items = items.borrow_mut();
                let key = match &m.key {
                    super::ItemKey::Text(t) => t.to_string(),
                    super::ItemKey::Number(n) => n.to_string(),
                };
                let from = items.iter().position(|i| *i == key).unwrap();
                let item = items.remove(from);
                items.insert(m.to, item);
            }
            window.refresh();
        })
        .labels(self.labels.clone())
        .feedback_behavior(self.feedback);
        if let Some(pinned) = pinned {
            options = options.is_reorderable(move |k| *k != super::ItemKey::from(pinned));
        }
        let r = create_reorderable(options);
        let mut list = List::new("groceries").label("Groceries").w(px(300.));
        if self.reorderable {
            list = list.reorderable(r);
        }
        let list = list.children(self.items.borrow().iter().map(|name| {
            ListItem::new(*name, *name)
                .item_key(*name)
                .into_any_element()
        }));
        div().p(px(20.)).child(
            div()
                .id("scroller")
                .w(px(340.))
                .h(px(self.height))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .child(rustryx_primitives::provide(
                    ScrollHost(self.scroll.clone()),
                    list,
                )),
        )
    }
}

struct Setup {
    items: Items,
    log: Log,
    scroll: ScrollHandle,
}

fn host<'a>(
    cx: &'a mut TestAppContext,
    names: &[&'static str],
    configure: impl FnOnce(&mut Host),
) -> (&'a mut VisualTestContext, Setup) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let items: Items = Rc::new(RefCell::new(names.to_vec()));
    let log: Log = Rc::default();
    let scroll = ScrollHandle::new();
    let setup = Setup {
        items: items.clone(),
        log: log.clone(),
        scroll: scroll.clone(),
    };
    let (_, cx) = cx.add_window_view(move |_, _| {
        let mut h = Host {
            items,
            log,
            applies: true,
            reorderable: true,
            pinned: None,
            labels: ReorderableLabels::default(),
            feedback: FeedbackBehavior::InsertionLine,
            scroll,
            height: 600.,
        };
        configure(&mut h);
        h
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, setup)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn shown(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

fn handle(cx: &mut VisualTestContext, key: &str) -> gpui::Bounds<gpui::Pixels> {
    let selector: &'static str = Box::leak(format!("groceries-reorder-{key}").into_boxed_str());
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("no handle for {key}"))
}

fn click(cx: &mut VisualTestContext, at: Point<gpui::Pixels>) {
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn press(cx: &mut VisualTestContext, key: &str) {
    cx.simulate_keystrokes(key);
    settle(cx);
}

fn said(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        rustryx_primitives::announce::announcement(Politeness::Polite, window, cx)
    })
    .to_string()
}

/// The latest announcement ends what the live region holds.
#[track_caller]
fn assert_said(cx: &mut VisualTestContext, expected: &str) {
    let text = said(cx);
    assert!(
        text.ends_with(expected),
        "said {text:?}, expected it to end with {expected:?}"
    );
}

fn moves(log: &Log) -> Vec<(String, usize)> {
    log.borrow()
        .iter()
        .map(|m| (format!("{:?}", m.key), m.to))
        .collect()
}

const FOUR: [&str; 4] = ["Eggs", "Milk", "Bread", "Jam"];

#[gpui::test]
fn fr2_a_list_without_the_object_has_no_handles(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, &FOUR, |h| h.reorderable = false);
    assert!(!shown(cx, "groceries-reorder-Milk"));
}

#[gpui::test]
fn fr1_a_rejected_row_shows_no_handle(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, &FOUR, |h| h.pinned = Some("Eggs"));
    assert!(!shown(cx, "groceries-reorder-Eggs"));
    assert!(shown(cx, "groceries-reorder-Milk"));
}

#[gpui::test]
fn fr4_reorder_mode_moves_a_place_per_arrow_and_announces_it(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |_| {});
    let at = handle(cx, "Milk").center();
    click(cx, at);
    assert!(shown(cx, "groceries-hint-down"), "the mode's hint");
    press(cx, "down");
    assert_eq!(moves(&s.log), [("Text(\"Milk\")".to_string(), 2)]);
    assert_eq!(*s.items.borrow(), ["Eggs", "Bread", "Milk", "Jam"]);
    assert_said(cx, "Milk, position 3 of 4, after Bread, before Jam");
    press(cx, "down");
    assert_said(cx, "Milk, position 4 of 4, after Jam, at the end");
    // One past the end does nothing.
    press(cx, "down");
    assert_eq!(s.log.borrow().len(), 2);
    press(cx, "up");
    press(cx, "up");
    press(cx, "up");
    assert_said(cx, "Milk, position 1 of 4, at the top, before Eggs");
    // Escape ends the mode, keeping the moves.
    press(cx, "escape");
    assert_said(cx, "Done, Milk at position 1 of 4");
    assert!(!shown(cx, "groceries-hint-down"));
    assert_eq!(*s.items.borrow(), ["Milk", "Eggs", "Bread", "Jam"]);
}

#[gpui::test]
fn fr4_enter_toggles_the_mode_and_the_handle_keeps_focus(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |_| {});
    // Tab reaches the first handle (the rows are not interactive).
    press(cx, "tab");
    press(cx, "enter");
    assert!(shown(cx, "groceries-hint-down"));
    press(cx, "down");
    press(cx, "down");
    assert_eq!(*s.items.borrow(), ["Milk", "Bread", "Eggs", "Jam"]);
    press(cx, "enter");
    assert_said(cx, "Done, Eggs at position 3 of 4");
}

#[gpui::test]
fn fr4_a_move_past_a_row_that_stays_at_an_end_is_refused(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &["Pinned", "Milk", "Bread"], |h| {
        h.pinned = Some("Pinned")
    });
    let at = handle(cx, "Milk").center();
    click(cx, at);
    press(cx, "up");
    assert!(s.log.borrow().is_empty());
    assert_said(cx, "Can't move past Pinned");
}

#[gpui::test]
fn fr4_the_hint_buttons_do_what_the_keys_do(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |_| {});
    let at = handle(cx, "Eggs").center();
    click(cx, at);
    let down = cx.debug_bounds("groceries-hint-down").unwrap();
    click(cx, down.center());
    assert_eq!(*s.items.borrow(), ["Milk", "Eggs", "Bread", "Jam"]);
    assert_said(cx, "Eggs, position 2 of 4, after Milk, before Bread");
    // Focus stays on the handle: the arrows still move it.
    press(cx, "down");
    assert_eq!(*s.items.borrow(), ["Milk", "Bread", "Eggs", "Jam"]);
    let done = cx.debug_bounds("groceries-hint-done").unwrap();
    click(cx, done.center());
    assert_said(cx, "Done, Eggs at position 3 of 4");
    assert!(!shown(cx, "groceries-hint-done"));
}

#[gpui::test]
fn fr4_a_move_the_app_keeps_announces_the_place_it_stayed(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |h| h.applies = false);
    let at = handle(cx, "Milk").center();
    click(cx, at);
    press(cx, "down");
    assert_eq!(s.log.borrow().len(), 1);
    assert_said(cx, "Milk, position 2 of 4, after Eggs, before Bread");
}

#[gpui::test]
fn fr1_labels_replace_the_catalog(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, &FOUR, |h| {
        h.labels = ReorderableLabels {
            mode_done: Some("Finished: {label} is {position, number} of {total, number}".into()),
            ..ReorderableLabels::default()
        };
    });
    let at = handle(cx, "Jam").center();
    click(cx, at);
    press(cx, "escape");
    assert_said(cx, "Finished: Jam is 4 of 4");
}

/// Presses at `from`, passes the slop with a 5px step (where the drag
/// begins, so the item follows from there), then moves so the item has
/// travelled `to - from`.
fn drag(cx: &mut VisualTestContext, from: Point<gpui::Pixels>, to: Point<gpui::Pixels>) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    settle(cx);
    let (dx, dy) = (f32::from(to.x - from.x), f32::from(to.y - from.y));
    let length = dx.hypot(dy);
    let (ux, uy) = if length > 0. {
        (dx / length, dy / length)
    } else {
        (0., 1.)
    };
    let slop = point(px(ux * 5.), px(uy * 5.));
    cx.simulate_mouse_move(from + slop, Some(MouseButton::Left), Modifiers::none());
    settle(cx);
    let steps = 6;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let at = point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t) + slop;
        cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::none());
        settle(cx);
    }
}

#[gpui::test]
fn fr3_a_drag_reports_once_at_the_drop(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |_| {});
    let from = handle(cx, "Eggs").center();
    let row = handle(cx, "Milk").center().y - from.y;
    drag(cx, from, point(from.x, from.y + row * 2.));
    assert_said(cx, "Picked up, position 1 of 4.");
    assert!(s.log.borrow().is_empty(), "nothing reported while dragging");
    assert!(
        shown(cx, "groceries-reorder-Eggs"),
        "the row rides the pointer"
    );
    cx.simulate_mouse_up(
        point(from.x, from.y + row * 2.),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert_eq!(moves(&s.log), [("Text(\"Eggs\")".to_string(), 2)]);
    assert_eq!(*s.items.borrow(), ["Milk", "Bread", "Eggs", "Jam"]);
    assert_said(cx, "Dropped, position 3 of 4");
    // The press that dragged did not toggle reorder mode.
    assert!(!shown(cx, "groceries-hint-down"));
}

#[gpui::test]
fn fr3_a_drop_in_place_reports_nothing_and_escape_cancels(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |_| {});
    let from = handle(cx, "Milk").center();
    drag(cx, from, point(from.x, from.y + px(6.)));
    cx.simulate_mouse_up(
        point(from.x, from.y + px(6.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert!(s.log.borrow().is_empty());
    assert_said(cx, "Dropped, position 2 of 4");
    let row = handle(cx, "Bread").center().y - from.y;
    drag(cx, from, point(from.x, from.y + row * 2.));
    press(cx, "escape");
    cx.simulate_mouse_up(
        point(from.x, from.y + row * 2.),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert!(s.log.borrow().is_empty(), "a cancelled drag calls nothing");
    assert_said(cx, "Reorder cancelled, back to position 2 of 4");
}

#[gpui::test]
fn fr3_a_drop_the_app_keeps_says_it_went_back(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |h| h.applies = false);
    let from = handle(cx, "Eggs").center();
    let row = handle(cx, "Milk").center().y - from.y;
    drag(cx, from, point(from.x, from.y + row * 2.));
    cx.simulate_mouse_up(
        point(from.x, from.y + row * 2.),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert_eq!(s.log.borrow().len(), 1);
    assert_said(cx, "Reorder cancelled, back to position 1 of 4");
}

#[gpui::test]
fn fr3_a_drag_near_the_scroll_edge_autoscrolls(cx: &mut TestAppContext) {
    let names: Vec<&'static str> = (0..30)
        .map(|i| &*Box::leak(format!("Item {i}").into_boxed_str()))
        .collect();
    let (cx, s) = host(cx, &names, |h| h.height = 200.);
    let from = handle(cx, "Item 1").center();
    drag(cx, from, point(from.x, px(215.)));
    for _ in 0..10 {
        cx.executor().advance_clock(Duration::from_millis(20));
        settle(cx);
    }
    assert!(
        f32::from(s.scroll.offset().y) < -10.,
        "scrolled: {:?}",
        s.scroll.offset()
    );
    cx.simulate_mouse_up(
        point(from.x, px(215.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    let to = s.log.borrow()[0].to;
    assert!(to > 4, "dropped further down the scrolled list: {to}");
}

#[gpui::test]
fn fr4a_start_enters_reorder_mode_from_the_app(cx: &mut TestAppContext) {
    struct Started {
        items: Items,
        r: Rc<RefCell<Option<super::Reorderable>>>,
    }
    impl Render for Started {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let items = self.items.clone();
            let r = create_reorderable(ReorderableOptions::new(move |m, window, _| {
                let mut items = items.borrow_mut();
                let key = format!("{:?}", m.key);
                let from = items.iter().position(|i| key.contains(i)).unwrap();
                let item = items.remove(from);
                items.insert(m.to, item);
                window.refresh();
            }));
            *self.r.borrow_mut() = Some(r.clone());
            List::new("groceries").w(px(300.)).reorderable(r).children(
                self.items
                    .borrow()
                    .iter()
                    .map(|n| ListItem::new(*n, *n).item_key(*n)),
            )
        }
    }
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let items: Items = Rc::new(RefCell::new(FOUR.to_vec()));
    let r: Rc<RefCell<Option<super::Reorderable>>> = Rc::default();
    let (i2, r2) = (items.clone(), r.clone());
    let (_, cx) = cx.add_window_view(move |_, _| Started { items: i2, r: r2 });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    let object = r.borrow().clone().unwrap();
    cx.update(|window, cx| object.start("Bread", window, cx));
    settle(cx);
    assert!(shown(cx, "groceries-hint-down"));
    press(cx, "up");
    assert_eq!(*items.borrow(), ["Eggs", "Bread", "Milk", "Jam"]);
    // A key that is not there does nothing.
    press(cx, "escape");
    let object = r.borrow().clone().unwrap();
    cx.update(|window, cx| object.start("Nope", window, cx));
    settle(cx);
    assert!(!shown(cx, "groceries-hint-down"));
}

#[gpui::test]
fn fr3_a_release_outside_the_window_drops(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |_| {});
    let from = handle(cx, "Eggs").center();
    let row = handle(cx, "Milk").center().y - from.y;
    let to = point(from.x, from.y + row * 2.);
    drag(cx, from, to);
    // The next move comes without the button: it was released elsewhere.
    cx.simulate_mouse_move(to, None, Modifiers::none());
    settle(cx);
    assert_eq!(moves(&s.log), [("Text(\"Eggs\")".to_string(), 2)]);
}

#[gpui::test]
fn fr9_shift_feedback_slides_the_passed_rows_aside(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |h| h.feedback = FeedbackBehavior::Shift);
    let from = handle(cx, "Eggs").center();
    let milk = handle(cx, "Milk");
    let bread = handle(cx, "Bread");
    let pitch = milk.top() - handle(cx, "Eggs").top();
    let jam = handle(cx, "Jam");
    // Past Bread's slot centre: Milk and Bread slide up a pitch on springs.settle; Jam
    // stays.
    drag(cx, from, point(from.x, bread.center().y + px(1.)));
    wait(cx, 80);
    let sliding = handle(cx, "Milk").top();
    assert!(
        sliding < milk.top() - px(1.) && sliding > milk.top() - pitch + px(1.),
        "sliding: {sliding:?}"
    );
    wait(cx, 1000);
    assert_eq!(handle(cx, "Milk").top(), milk.top() - pitch);
    assert_eq!(handle(cx, "Bread").top(), bread.top() - pitch);
    assert_eq!(handle(cx, "Jam").top(), jam.top());
    cx.simulate_mouse_up(
        point(from.x, bread.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert_eq!(moves(&s.log), [("Text(\"Eggs\")".to_string(), 2)]);
    assert_eq!(
        handle(cx, "Milk").top(),
        milk.top() - pitch,
        "the new order, with no layout jump"
    );
    // The dropped row settles into the open gap from where it was let go.
    wait(cx, 1000);
    assert_eq!(handle(cx, "Eggs").top(), bread.top(), "in the gap, at rest");
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..(ms / 16) {
        cx.executor().advance_clock(Duration::from_millis(16));
        settle(cx);
    }
}

#[gpui::test]
fn fr9_a_reorder_mode_move_under_shift_slides_the_rows_into_place(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |h| h.feedback = FeedbackBehavior::Shift);
    let (eggs, milk) = (handle(cx, "Eggs"), handle(cx, "Milk"));
    click(cx, eggs.center());
    press(cx, "down");
    assert_eq!(*s.items.borrow(), ["Milk", "Eggs", "Bread", "Jam"]);
    let shown = handle(cx, "Eggs").top();
    assert!(
        (shown - eggs.top()).abs() < px(2.),
        "it starts where it was: {shown:?}"
    );
    wait(cx, 80);
    let mid = handle(cx, "Eggs").top();
    assert!(
        mid > eggs.top() + px(1.) && mid < milk.top() - px(1.),
        "sliding: {mid:?}"
    );
    wait(cx, 1000);
    assert_eq!(handle(cx, "Eggs").top(), milk.top());
    assert_eq!(handle(cx, "Milk").top(), eggs.top());
}

#[gpui::test]
fn fr9_a_mouse_drop_behind_the_insertion_line_lands_at_once(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |_| {});
    let from = handle(cx, "Eggs").center();
    let bread = handle(cx, "Bread");
    drag(cx, from, point(from.x, bread.center().y + px(1.)));
    cx.simulate_mouse_up(
        point(from.x, bread.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert_eq!(moves(&s.log), [("Text(\"Eggs\")".to_string(), 2)]);
    assert_eq!(handle(cx, "Eggs").top(), bread.top(), "no glide");
}

#[gpui::test]
fn fr9_under_reduced_motion_shift_shows_the_line(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |h| h.feedback = FeedbackBehavior::Shift);
    cx.update(|_, cx| rustryx_primitives::set_reduced_motion(true, cx));
    let from = handle(cx, "Eggs").center();
    let milk = handle(cx, "Milk");
    let bread = handle(cx, "Bread");
    drag(cx, from, point(from.x, bread.center().y + px(1.)));
    assert_eq!(handle(cx, "Milk").top(), milk.top(), "no row travels");
    cx.simulate_mouse_up(
        point(from.x, bread.center().y),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert_eq!(s.log.borrow().len(), 1);
}

/// An app's own cards in a wrapping grid, three to a row, each with its
/// own grip as the handle (FR4b).
struct Cards {
    items: Items,
    log: Log,
    applies: bool,
    feedback: FeedbackBehavior,
    positions: Option<Vec<Point<gpui::Pixels>>>,
}

impl Render for Cards {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (items, log, applies) = (self.items.clone(), self.log.clone(), self.applies);
        let mut options = ReorderableOptions::new(move |m, window, _| {
            log.borrow_mut().push(m.clone());
            if applies {
                let mut items = items.borrow_mut();
                let key = format!("{:?}", m.key);
                let from = items
                    .iter()
                    .position(|i| key == format!("Text({i:?})"))
                    .unwrap();
                let item = items.remove(from);
                items.insert(m.to, item);
            }
            window.refresh();
        })
        .feedback_behavior(self.feedback);
        if let Some(p) = self.positions.clone() {
            options = options.positions(move || p.clone());
        }
        let attached = create_reorderable(options).attach("cards", window, cx);
        let names = self.items.borrow().clone();
        let cards: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let grip = div().size(px(24.)).bg(gpui::black());
                let card = div()
                    .w(px(100.))
                    .h(px(60.))
                    .flex()
                    .items_start()
                    .justify_end()
                    .child(attached.handle(*name, grip));
                attached.item(i, *name, *name, card, cx)
            })
            .collect();
        div().p(px(20.)).child(
            attached.container(
                div()
                    .flex()
                    .flex_wrap()
                    .w(px(330.))
                    .gap(px(10.))
                    .children(cards),
                names.len(),
            ),
        )
    }
}

fn cards(
    cx: &mut TestAppContext,
    configure: impl FnOnce(&mut Cards),
) -> (&mut VisualTestContext, Items, Log) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let names: Vec<&'static str> = (1..=7)
        .map(|i| &*Box::leak(format!("Card {i}").into_boxed_str()))
        .collect();
    let items: Items = Rc::new(RefCell::new(names));
    let log: Log = Rc::default();
    let (i2, l2) = (items.clone(), log.clone());
    let (_, cx) = cx.add_window_view(move |_, _| {
        let mut c = Cards {
            items: i2,
            log: l2,
            applies: true,
            feedback: FeedbackBehavior::InsertionLine,
            positions: None,
        };
        configure(&mut c);
        c
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, items, log)
}

fn card(cx: &mut VisualTestContext, n: usize) -> gpui::Bounds<gpui::Pixels> {
    let selector: &'static str = Box::leak(format!("cards-reorder-Card {n}").into_boxed_str());
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("no handle for Card {n}"))
}

#[gpui::test]
fn fr4b_fr4_an_attached_grid_moves_by_all_four_arrows(cx: &mut TestAppContext) {
    let (cx, items, _) = cards(cx, |_| {});
    let at = card(cx, 2).center();
    click(cx, at);
    assert!(
        shown(cx, "cards-hint-left") && shown(cx, "cards-hint-up"),
        "the grid's hint has all four moves"
    );
    press(cx, "right");
    assert_eq!(items.borrow()[2], "Card 2");
    assert_said(cx, "Card 2, row 1, column 3, position 3 of 7");
    // Right across the row's end.
    press(cx, "right");
    assert_said(cx, "Card 2, row 2, column 1, position 4 of 7");
    // Down into the short last row: its last place.
    press(cx, "down");
    assert_eq!(items.borrow()[6], "Card 2");
    assert_said(cx, "Card 2, row 3, column 1, position 7 of 7");
    // The live region clears before the same place is said again.
    cx.executor().advance_clock(Duration::from_secs(10));
    settle(cx);
    press(cx, "up");
    assert_said(cx, "Card 2, row 2, column 1, position 4 of 7");
}

#[gpui::test]
fn fr4b_a_move_the_app_does_not_apply_is_refused(cx: &mut TestAppContext) {
    let (cx, items, log) = cards(cx, |c| c.applies = false);
    let at = card(cx, 1).center();
    click(cx, at);
    press(cx, "right");
    assert_eq!(log.borrow().len(), 1);
    assert_eq!(items.borrow()[0], "Card 1");
    assert_said(cx, "Can't move Card 1");
}

#[gpui::test]
fn fr3_fr4b_a_drag_in_a_grid_lands_in_the_nearest_slot(cx: &mut TestAppContext) {
    let (cx, items, log) = cards(cx, |_| {});
    let from = card(cx, 1).center();
    let target = card(cx, 5).center();
    drag(cx, from, target);
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
    settle(cx);
    assert_eq!(moves(&log), [("Text(\"Card 1\")".to_string(), 4)]);
    assert_eq!(items.borrow()[4], "Card 1");
}

#[gpui::test]
fn fr9_shift_in_a_grid_moves_items_slot_to_slot(cx: &mut TestAppContext) {
    let (cx, _, _) = cards(cx, |c| c.feedback = FeedbackBehavior::Shift);
    let (c2, c3, c4) = (card(cx, 2), card(cx, 3), card(cx, 4));
    let from = card(cx, 1).center();
    drag(cx, from, c4.center() + point(px(0.), px(1.)));
    wait(cx, 1000);
    // Cards 2-4 take the slots before them.
    assert_eq!(card(cx, 2).origin, card_slot(c2, -110., 0.));
    assert_eq!(card(cx, 3).origin, card_slot(c3, -110., 0.));
    assert_eq!(card(cx, 4).origin, card_slot(c4, 220., -70.));
}

fn card_slot(b: gpui::Bounds<gpui::Pixels>, dx: f32, dy: f32) -> Point<gpui::Pixels> {
    point(b.origin.x + px(dx), b.origin.y + px(dy))
}

#[gpui::test]
fn fr1_positions_place_the_slots(cx: &mut TestAppContext) {
    // A hand whose slots run right to left over the cards' boxes: slot 0
    // where the third card sits.
    let given: Vec<Point<gpui::Pixels>> = (0..7)
        .map(|i| {
            let i = 6 - i;
            point(
                px((i % 3) as f32 * 110. + 50.),
                px((i / 3) as f32 * 70. + 30.),
            )
        })
        .collect();
    let (cx, _, log) = cards(cx, move |c| c.positions = Some(given));
    let from = card(cx, 2).center();
    // Onto the box of card 7, where slot 0 is.
    let target = card(cx, 7).center();
    drag(cx, from, target);
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
    settle(cx);
    assert_eq!(moves(&log), [("Text(\"Card 2\")".to_string(), 0)]);
}

#[gpui::test]
fn fr1_positions_of_the_wrong_length_are_ignored(cx: &mut TestAppContext) {
    let (cx, _, log) = cards(cx, |c| c.positions = Some(vec![point(px(0.), px(0.))]));
    let from = card(cx, 1).center();
    let target = card(cx, 5).center();
    drag(cx, from, target);
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
    settle(cx);
    assert_eq!(moves(&log), [("Text(\"Card 1\")".to_string(), 4)]);
}

/// A drag down past Bread's slot at 16 ms a step, released at once or after a pause.
fn thrown_drop(cx: &mut VisualTestContext, pause: bool) -> (gpui::Pixels, gpui::Pixels) {
    let from = handle(cx, "Eggs").center();
    let bread = handle(cx, "Bread");
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    settle(cx);
    let to = point(from.x, bread.center().y + px(1.));
    for i in 1..=8 {
        cx.executor().advance_clock(Duration::from_millis(16));
        let t = i as f32 / 8.;
        let at = point(from.x, from.y + (to.y - from.y) * t);
        cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::none());
        settle(cx);
    }
    if pause {
        wait(cx, 200);
    }
    let let_go = handle(cx, "Eggs").top();
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    settle(cx);
    cx.executor().advance_clock(Duration::from_millis(48));
    settle(cx);
    (let_go, handle(cx, "Eggs").top())
}

#[gpui::test]
fn fr9_the_dropped_row_settles_from_the_release_velocity(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, &FOUR, |h| h.feedback = FeedbackBehavior::Shift);
    let (let_go, thrown) = thrown_drop(cx, false);
    wait(cx, 1000);
    let rest = handle(cx, "Eggs").top();
    assert!(
        thrown > let_go.max(rest) + px(0.5),
        "a thrown row carries on past its place before it settles: {thrown:?} vs {rest:?}"
    );
}

#[gpui::test]
fn fr9_a_row_held_still_before_the_drop_settles_from_rest(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, &FOUR, |h| h.feedback = FeedbackBehavior::Shift);
    let (let_go, shown) = thrown_drop(cx, true);
    wait(cx, 1000);
    let rest = handle(cx, "Eggs").top();
    let lo = let_go.min(rest) - px(0.5);
    let hi = let_go.max(rest) + px(0.5);
    assert!(
        shown >= lo && shown <= hi,
        "no overshoot from rest: {shown:?} in {lo:?}..{hi:?}"
    );
}

#[gpui::test]
fn the_dragged_row_dims_in_place_and_a_copy_of_it_follows_the_pointer(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, &FOUR, |_| {});
    let place = cx.debug_bounds("Eggs").unwrap();
    let from = handle(cx, "Eggs").center();
    let row = handle(cx, "Milk").center().y - from.y;
    drag(cx, from, point(from.x, from.y + row * 2.));
    // The row itself stays where it was, dimmed (fork patch R3 keeps its frame).
    assert_eq!(cx.debug_bounds("Eggs").unwrap().origin, place.origin);
    assert!(
        !shown(cx, "Eggs-reorder-source"),
        "no text-only stand-in at its place"
    );
    let copy = cx
        .debug_bounds("reorder-preview-copy")
        .expect("a copy of the row, its last frame, at the pointer");
    assert_eq!(copy.size, place.size);
    assert!(
        copy.origin.y > place.origin.y + row,
        "it follows the pointer"
    );
    cx.simulate_mouse_up(
        point(from.x, from.y + row * 2.),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert!(cx.debug_bounds("reorder-preview-copy").is_none());
}

// ---- Under a finger: UIKit's reorder control (motion/uikit/reorder.ts) ---------

fn finger(cx: &mut VisualTestContext, id: u64, phase: gpui::TouchPhase, at: Point<gpui::Pixels>) {
    cx.simulate_event(gpui::TouchEvent {
        id: gpui::TouchId(id),
        phase,
        position: at,
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

/// A finger moved from `from` to `to` in steps of 16 ms.
fn finger_move(cx: &mut VisualTestContext, from: Point<gpui::Pixels>, to: Point<gpui::Pixels>) {
    for i in 1..=8 {
        cx.executor().advance_clock(Duration::from_millis(16));
        let t = i as f32 / 8.;
        let at = point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        finger(cx, 1, gpui::TouchPhase::Moved, at);
        settle(cx);
    }
}

#[gpui::test]
fn a_finger_held_on_the_handle_for_the_content_touch_delay_picks_the_row_up(
    cx: &mut TestAppContext,
) {
    let (cx, s) = host(cx, &FOUR, |_| {});
    let from = handle(cx, "Eggs").center();
    let row = handle(cx, "Milk").center().y - from.y;
    finger(cx, 1, gpui::TouchPhase::Started, from);
    wait(cx, 100);
    assert!(
        !shown(cx, "reorder-preview-copy") && !said(cx).contains("Picked up"),
        "before 150 ms nothing is claimed"
    );
    wait(cx, 60);
    settle(cx);
    let to = point(from.x, from.y + row * 2.);
    finger_move(cx, from, to);
    assert_said(cx, "Picked up, position 1 of 4.");
    assert!(
        shown(cx, "reorder-preview-copy"),
        "the row follows the finger"
    );
    assert!(s.log.borrow().is_empty(), "nothing reported while dragging");
    finger(cx, 1, gpui::TouchPhase::Ended, to);
    settle(cx);
    assert_eq!(moves(&s.log), [("Text(\"Eggs\")".to_string(), 2)]);
    assert_eq!(*s.items.borrow(), ["Milk", "Bread", "Eggs", "Jam"]);
}

#[gpui::test]
fn a_finger_that_travels_before_the_claim_leaves_the_touch_to_the_scroller(
    cx: &mut TestAppContext,
) {
    let names: Vec<&'static str> = (0..30)
        .map(|i| &*Box::leak(format!("Item {i}").into_boxed_str()))
        .collect();
    let (cx, s) = host(cx, &names, |h| h.height = 200.);
    let from = handle(cx, "Item 2").center();
    finger(cx, 1, gpui::TouchPhase::Started, from);
    // A flick: 60 px up within 50 ms, well before the 150 ms claim.
    let to = point(from.x, from.y - px(60.));
    for i in 1..=3 {
        cx.executor().advance_clock(Duration::from_millis(16));
        finger(
            cx,
            1,
            gpui::TouchPhase::Moved,
            point(from.x, from.y - px(20. * i as f32)),
        );
    }
    wait(cx, 200);
    finger(cx, 1, gpui::TouchPhase::Ended, to);
    settle(cx);
    assert!(!said(cx).contains("Picked up"), "the row never moved");
    assert!(s.log.borrow().is_empty());
    assert!(
        f32::from(s.scroll.offset().y) < -1.,
        "the list scrolled instead: {:?}",
        s.scroll.offset()
    );
}

#[gpui::test]
fn a_second_finger_before_the_claim_is_the_pages_pinch(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, &FOUR, |_| {});
    let from = handle(cx, "Eggs").center();
    finger(cx, 1, gpui::TouchPhase::Started, from);
    wait(cx, 50);
    finger(cx, 2, gpui::TouchPhase::Started, point(px(300.), px(300.)));
    wait(cx, 200);
    assert!(
        !said(cx).contains("Picked up"),
        "two fingers never pick a row up"
    );
    finger(cx, 2, gpui::TouchPhase::Ended, point(px(300.), px(300.)));
    finger(cx, 1, gpui::TouchPhase::Ended, from);
    settle(cx);
    assert!(s.log.borrow().is_empty());
}

#[gpui::test]
fn a_tap_on_the_handle_toggles_reorder_mode(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, &FOUR, |_| {});
    let from = handle(cx, "Milk").center();
    finger(cx, 1, gpui::TouchPhase::Started, from);
    wait(cx, 40);
    finger(cx, 1, gpui::TouchPhase::Ended, from);
    settle(cx);
    assert!(
        shown(cx, "groceries-hint-down"),
        "the tap is the handle's click: reorder mode, with its hint"
    );
}

#[gpui::test]
fn a_held_finger_near_the_scroll_edge_autoscrolls(cx: &mut TestAppContext) {
    let names: Vec<&'static str> = (0..30)
        .map(|i| &*Box::leak(format!("Item {i}").into_boxed_str()))
        .collect();
    let (cx, s) = host(cx, &names, |h| h.height = 200.);
    let from = handle(cx, "Item 1").center();
    finger(cx, 1, gpui::TouchPhase::Started, from);
    wait(cx, 160);
    let to = point(from.x, px(215.));
    finger_move(cx, from, to);
    for _ in 0..10 {
        cx.executor().advance_clock(Duration::from_millis(20));
        settle(cx);
    }
    assert!(
        f32::from(s.scroll.offset().y) < -10.,
        "scrolled on UIKit's 20 ms clock: {:?}",
        s.scroll.offset()
    );
    finger(cx, 1, gpui::TouchPhase::Ended, to);
    settle(cx);
    let to = s.log.borrow()[0].to;
    assert!(to > 4, "dropped further down the scrolled list: {to}");
}
