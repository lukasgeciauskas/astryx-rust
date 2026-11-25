//! Reordering a List (AST-103): reorder mode's keys, hint and
//! announcements, the drag, `start`, and a list without the prop.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::{Politeness, ScrollHost};
use gpui::{
    Context, InteractiveElement, IntoElement, Modifiers, MouseButton, ParentElement, Point, Render,
    ScrollHandle, StatefulInteractiveElement, Styled, TestAppContext, VisualTestContext, Window,
    div, point, px,
};

use super::{ReorderMove, ReorderableLabels, ReorderableOptions, create_reorderable};
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
        .labels(self.labels.clone());
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
                .child(astryx_primitives::provide(
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
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
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
        astryx_primitives::announce::announcement(Politeness::Polite, window, cx)
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

fn drag(cx: &mut VisualTestContext, from: Point<gpui::Pixels>, to: Point<gpui::Pixels>) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    settle(cx);
    let steps = 8;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let at = point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
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
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
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
