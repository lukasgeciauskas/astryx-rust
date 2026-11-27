//! ChatVirtualizer: upstream `ChatVirtualizer.test.tsx`, `ChatVirtualizerRotation.test.tsx` and
//! `ChatVirtualizerViewportResize.test.tsx` on GPUI's layout. Upstream's jsdom tests play the
//! browser: a hand-set `scrollTop`, hand-delivered resize entries. Here the layout is real:
//! the test resizes the window (a rotation, a width drag, a toolbar), sets the rows'
//! heights, and moves the scroller with a wheel or, playing the platform, by writing its
//! position.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Context, FocusHandle, Modifiers, Render, ScrollDelta, ScrollEvent, ScrollHandle, ScrollSource,
    SharedString, TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext, Window, div,
    point, px, size,
};

use super::virtualizer::list_of;
use super::virtualizer_engine::{Child, Machine};
use super::virtualizer_model::Mode;
use super::*;

#[derive(Clone)]
struct Row {
    id: SharedString,
    text: SharedString,
}

fn rows(n: usize) -> Rc<Vec<Row>> {
    thread("", n)
}

fn thread(prefix: &str, n: usize) -> Rc<Vec<Row>> {
    Rc::new(
        (0..n)
            .map(|i| Row {
                id: format!("{prefix}{i}").into(),
                text: format!("{prefix}message {i}").into(),
            })
            .collect(),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    /// The list's own scroller, 400 px tall.
    Own,
    /// Attached to the page's scroller (one spanning the window).
    Page,
    /// Attached to the page's scroller under a 200 px header.
    Header,
    /// Attached to a scroller that is not the page's, as tall as `box_height`.
    Boxed,
    /// Attach mode waiting for its scroller.
    Pending,
}

struct Host {
    data: Rc<Vec<Row>>,
    shape: Shape,
    row_height: Rc<Cell<f32>>,
    box_height: Rc<Cell<f32>>,
    handle: ChatVirtualizerHandle,
    page: ScrollHandle,
    label: Option<SharedString>,
    focusable: bool,
    focus: Rc<RefCell<HashMap<SharedString, FocusHandle>>>,
    rendered: Rc<RefCell<HashSet<SharedString>>>,
    created: Rc<RefCell<Vec<SharedString>>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (row_height, focusable, focus, rendered, created) = (
            self.row_height.clone(),
            self.focusable,
            self.focus.clone(),
            self.rendered.clone(),
            self.created.clone(),
        );
        let mut list = ChatVirtualizer::new(
            "list",
            self.data.clone(),
            |m: &Row, _| m.id.clone(),
            move |m: &Row, _, window, cx| {
                rendered.borrow_mut().insert(m.id.clone());
                // A row's own state, created once per identity.
                let created = created.clone();
                let id = m.id.clone();
                window.use_keyed_state(
                    gpui::ElementId::Name(format!("probe-{}", m.id).into()),
                    cx,
                    move |_, _| {
                        created.borrow_mut().push(id);
                    },
                );
                let row = div().h(px(row_height.get())).child(m.text.clone());
                if focusable {
                    let handle = focus
                        .borrow_mut()
                        .entry(m.id.clone())
                        .or_insert_with(|| cx.focus_handle())
                        .clone();
                    row.id(gpui::ElementId::Name(format!("button-{}", m.id).into()))
                        .track_focus(&handle)
                        .into_any_element()
                } else {
                    row.into_any_element()
                }
            },
        )
        .estimated_item_size(ItemSize::Fixed(100.))
        .handle(&self.handle);
        if let Some(label) = &self.label {
            list = list.label(label.clone());
        }
        let page = self.page.clone();
        match self.shape {
            Shape::Own => div().size_full().child(list.h(px(400.))).into_any_element(),
            Shape::Pending => div()
                .size_full()
                .child(list.scroll_element(None))
                .into_any_element(),
            Shape::Page | Shape::Header => div()
                .id("page")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&page)
                .when(self.shape == Shape::Header, |d| {
                    d.child(div().h(px(200.)).flex_none())
                })
                .child(list.scroll_element(Some(&page)))
                .into_any_element(),
            Shape::Boxed => div()
                .size_full()
                .child(
                    div()
                        .id("box")
                        .w_full()
                        .h(px(self.box_height.get()))
                        .overflow_y_scroll()
                        .track_scroll(&page)
                        .child(list.scroll_element(Some(&page))),
                )
                .into_any_element(),
        }
    }
}

struct Case {
    host: gpui::Entity<Host>,
    row_height: Rc<Cell<f32>>,
    box_height: Rc<Cell<f32>>,
    handle: ChatVirtualizerHandle,
    page: ScrollHandle,
    focus: Rc<RefCell<HashMap<SharedString, FocusHandle>>>,
    rendered: Rc<RefCell<HashSet<SharedString>>>,
    created: Rc<RefCell<Vec<SharedString>>>,
}

fn mount(
    cx: &mut TestAppContext,
    data: Rc<Vec<Row>>,
    shape: Shape,
) -> (Case, &mut VisualTestContext) {
    mount_with(cx, data, shape, false)
}

fn mount_with(
    cx: &mut TestAppContext,
    data: Rc<Vec<Row>>,
    shape: Shape,
    focusable: bool,
) -> (Case, &mut VisualTestContext) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let row_height = Rc::new(Cell::new(100.));
    let box_height = Rc::new(Cell::new(400.));
    let handle = ChatVirtualizerHandle::new();
    let page = ScrollHandle::new();
    let focus = Rc::default();
    let rendered = Rc::default();
    let created = Rc::default();
    let (host, cx) = cx.add_window_view({
        let (row_height, box_height, handle, page, focus, rendered, created) = (
            row_height.clone(),
            box_height.clone(),
            handle.clone(),
            page.clone(),
            Rc::clone(&focus),
            Rc::clone(&rendered),
            Rc::clone(&created),
        );
        move |_, _| Host {
            data,
            shape,
            row_height,
            box_height,
            handle,
            page,
            label: None,
            focusable,
            focus,
            rendered,
            created,
        }
    });
    cx.simulate_resize(size(
        px(400.),
        if shape == Shape::Boxed {
            px(500.)
        } else {
            px(400.)
        },
    ));
    settled(cx);
    // The window's first size is no rotation: let its settle time pass.
    wait(cx, 600);
    frame(cx);
    (
        Case {
            host,
            row_height,
            box_height,
            handle,
            page,
            focus,
            rendered,
            created,
        },
        cx,
    )
}

/// One frame.
fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

/// The frames a mount landing and its release take (upstream's `settled`), with the clock
/// moving on.
fn settled(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(16));
        frame(cx);
    }
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn set_data(case: &Case, cx: &mut VisualTestContext, data: Rc<Vec<Row>>) {
    case.host.update(cx, |h, cx| {
        h.data = data;
        cx.notify();
    });
    cx.run_until_parked();
}

fn machine() -> Rc<RefCell<Machine>> {
    list_of("list").expect("the list drew").0
}

fn painted() -> Vec<(SharedString, f64, f64)> {
    list_of("list")
        .expect("the list drew")
        .1
        .borrow()
        .painted
        .clone()
}

fn mounted() -> Vec<SharedString> {
    painted().into_iter().map(|(k, _, _)| k).collect()
}

fn is_mounted(key: &str) -> bool {
    painted().iter().any(|(k, _, _)| k.as_ref() == key)
}

fn children() -> Vec<Child> {
    list_of("list")
        .expect("the list drew")
        .1
        .borrow()
        .dom_children()
}

/// The row painted at the top of the view.
fn top_row() -> Option<usize> {
    painted()
        .into_iter()
        .find(|(_, top, h)| *top <= 0. && top + h > 0.)
        .and_then(|(k, _, _)| k.parse().ok())
}

/// Whether the row's top edge sits at the top of the view.
fn is_at_top(key: &str) -> bool {
    painted()
        .iter()
        .any(|(k, top, _)| k.as_ref() == key && top.abs() < 2.)
}

fn scroll_top(case: &Case) -> f64 {
    -f64::from(case.page.offset().y)
}

/// The platform moves the page's position (a browser keeping the reader's place).
fn page_scroll(case: &Case, cx: &mut VisualTestContext, at: f64) {
    case.page.set_offset(point(px(0.), px(-(at as f32))));
    frame(cx);
}

/// The reader turns the wheel over the scroller by `dy` (positive: toward the end).
fn wheel(cx: &mut VisualTestContext, dy: f32) {
    cx.simulate_event(ScrollEvent {
        position: point(px(200.), px(200.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-dy))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    cx.run_until_parked();
}

fn touch(cx: &mut VisualTestContext, phase: TouchPhase) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: point(px(200.), px(200.)),
        predicted_position: None,
        force: None,
    });
    cx.run_until_parked();
}

/// A finger on the content scrolls it by `dy`.
fn finger_scroll(cx: &mut VisualTestContext, dy: f32) {
    cx.simulate_event(ScrollEvent {
        position: point(px(200.), px(200.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-dy))),
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Touch,
    });
    cx.run_until_parked();
}

/// The finger lifts and its scroll, momentum included, ends.
fn finger_scroll_ends(cx: &mut VisualTestContext) {
    touch(cx, TouchPhase::Ended);
    wait(cx, 200);
    frame(cx);
}

fn at_bottom(case: &Case) -> bool {
    let max = f64::from(case.page.max_offset().y);
    (scroll_top(case) - max).abs() <= 1.
}

/// The list follows: the next growth lands on a larger bottom.
fn follows_after_growth(case: &Case, cx: &mut VisualTestContext, written: f64) -> bool {
    set_data(case, cx, rows(60));
    settled(cx);
    scroll_top(case) > written + 1.
}

// ---- Rendering --------------------------------------------------------------

#[gpui::test]
fn renders_the_windowed_rows_between_a_top_and_a_bottom_spacer(cx: &mut TestAppContext) {
    let (_case, _cx) = mount(cx, rows(5), Shape::Own);
    let c = children();
    assert_eq!(c.first(), Some(&Child::Top));
    assert_eq!(c.last(), Some(&Child::Bottom));
    assert_eq!(c.iter().filter(|c| matches!(c, Child::Row(_))).count(), 5);
}

#[gpui::test]
fn tags_every_rendered_row_with_its_identity(cx: &mut TestAppContext) {
    let (_case, cx) = mount(cx, rows(5), Shape::Own);
    for key in mounted() {
        let selector: &'static str = Box::leak(format!("list-row-{key}").into_boxed_str());
        assert!(
            cx.debug_bounds(selector).is_some(),
            "row {key} carries its key"
        );
    }
}

#[gpui::test]
fn renders_an_empty_list_as_just_the_spacers(cx: &mut TestAppContext) {
    let (_case, _cx) = mount(cx, rows(0), Shape::Own);
    assert_eq!(children(), [Child::Top, Child::Bottom]);
}

#[gpui::test]
fn renders_one_content_block_in_attach_mode(cx: &mut TestAppContext) {
    let (_case, cx) = mount(cx, rows(3), Shape::Page);
    assert!(
        cx.debug_bounds("list-scroller").is_none(),
        "no own scroller"
    );
    let c = children();
    assert_eq!(c.first(), Some(&Child::Top));
    assert_eq!(c.last(), Some(&Child::Bottom));
}

#[gpui::test]
fn renders_its_own_scroller_otherwise(cx: &mut TestAppContext) {
    let (_case, cx) = mount(cx, rows(3), Shape::Own);
    assert!(cx.debug_bounds("list-scroller").is_some());
}

#[gpui::test]
fn keeps_the_previous_extent_as_the_floor_until_the_pass_rederives_it(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(5), Shape::Own);
    assert_eq!(machine().borrow().spacers.floor, 500.);
    set_data(&case, cx, rows(2));
    frame(cx);
    assert_eq!(machine().borrow().spacers.floor, 200.);
}

#[gpui::test]
fn renders_nothing_but_spacers_while_the_attach_element_is_pending(cx: &mut TestAppContext) {
    let (_case, cx) = mount(cx, rows(3), Shape::Pending);
    assert!(
        cx.debug_bounds("list-scroller").is_none(),
        "no own scroller"
    );
    assert_eq!(children(), [Child::Top, Child::Bottom]);
}

#[gpui::test]
fn takes_declarations_through_the_handle_and_falls_back_from_a_missing_key(
    cx: &mut TestAppContext,
) {
    let (case, cx) = mount(cx, rows(50), Shape::Page);
    cx.update(|window, _| {
        case.handle.scroll_to_distance_from_bottom_px(0., window);
        case.handle.anchor_to_key("1", 0., window);
    });
    settled(cx);
    assert!(is_at_top("1"));
    // A key that is not in the data falls back to following the end.
    cx.update(|window, _| case.handle.anchor_to_key("missing-key", 0., window));
    settled(cx);
    assert_eq!(machine().borrow().mode, Mode::End { distance: 0. });
    assert!(at_bottom(&case));
}

#[gpui::test]
fn keeps_row_identity_across_data_replacement(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(3), Shape::Own);
    let before = case
        .created
        .borrow()
        .iter()
        .filter(|k| k.as_ref() == "1")
        .count();
    set_data(&case, cx, rows(4));
    settled(cx);
    let after = case
        .created
        .borrow()
        .iter()
        .filter(|k| k.as_ref() == "1")
        .count();
    assert_eq!((before, after), (1, 1), "row 1 kept its state");
}

#[gpui::test]
fn a_prepend_reindexes_rows_without_renumbering_their_keys(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(3), Shape::Own);
    let before = mounted();
    let mut data = vec![
        Row {
            id: "older-a".into(),
            text: "a".into(),
        },
        Row {
            id: "older-b".into(),
            text: "b".into(),
        },
    ];
    data.extend(rows(3).iter().cloned());
    set_data(&case, cx, Rc::new(data));
    settled(cx);
    let after = mounted();
    for key in before {
        assert!(after.contains(&key), "{key} still answers to its key");
    }
    assert!(after.iter().any(|k| k.as_ref() == "older-b"));
}

#[gpui::test]
fn drops_the_host_geometry_when_it_returns_to_its_own_scroller(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(50), Shape::Header);
    case.host.update(cx, |h, cx| {
        h.shape = Shape::Own;
        cx.notify();
    });
    settled(cx);
    cx.update(|window, _| case.handle.scroll_to_distance_from_bottom_px(0., window));
    settled(cx);
    // The own scroller's bottom is the modelled total less its viewport: 50 x 100 - 400.
    let p = painted();
    let last = p.last().expect("rows painted");
    assert_eq!(last.0.as_ref(), "49");
    assert!((last.1 + last.2 - 400.).abs() < 1., "{last:?}");
}

#[gpui::test]
fn names_its_own_scroller_messages_unless_given_a_label(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(3), Shape::Own);
    assert!(cx.debug_bounds("list-scroller").is_some());
    let label = cx.update(|_, cx| rustryx_i18n::current(cx).t("@astryx.chatVirtualizer.label"));
    assert_eq!(label.as_ref(), "Messages");
    case.host.update(cx, |h, cx| {
        h.label = Some("Support thread".into());
        cx.notify();
    });
    frame(cx);
    assert!(cx.debug_bounds("list-scroller").is_some());
}

// ---- Following and who moved the scroller ---------------------------------------

#[gpui::test]
fn follows_the_end_from_the_first_frame(cx: &mut TestAppContext) {
    let (_case, _cx) = mount(cx, rows(60), Shape::Own);
    let p = painted();
    let last = p.last().expect("rows painted");
    assert_eq!(last.0.as_ref(), "59");
    assert!((last.1 - 300.).abs() < 1., "{p:?}");
    assert!(p.len() < 60, "only the window mounts");
}

#[gpui::test]
fn a_wheel_by_itself_does_not_disengage_follow(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(50), Shape::Page);
    let written = scroll_top(&case);
    // A wheel toward the end at the end moves nothing here.
    wheel(cx, 120.);
    frame(cx);
    assert!(follows_after_growth(&case, cx, written));
}

#[gpui::test]
fn a_scroll_up_the_next_write_outruns_still_disengages(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(50), Shape::Page);
    let written = scroll_top(&case);
    wheel(cx, -100.);
    frame(cx);
    assert!(!follows_after_growth(&case, cx, written));
}

#[gpui::test]
fn a_clamp_onto_a_new_smaller_bottom_is_not_displacement(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(50), Shape::Page);
    // Rows shrink outside any data change; the scroller clamps onto the new bottom.
    case.row_height.set(94.);
    frame(cx);
    frame(cx);
    let written = scroll_top(&case);
    assert!(follows_after_growth(&case, cx, written));
}

#[gpui::test]
fn skips_the_correction_pass_for_a_frame_that_changes_nothing_it_reads(cx: &mut TestAppContext) {
    let (_case, cx) = mount(cx, rows(60), Shape::Page);
    settled(cx);
    let passes = machine().borrow().passes;
    frame(cx);
    frame(cx);
    assert_eq!(machine().borrow().passes, passes);
}

#[gpui::test]
fn mounts_no_more_than_a_window_of_rows_when_a_large_batch_is_appended_while_following(
    cx: &mut TestAppContext,
) {
    let (case, cx) = mount(cx, rows(40), Shape::Page);
    let window = case.rendered.borrow().len();
    case.rendered.borrow_mut().clear();
    set_data(&case, cx, rows(2040));
    settled(cx);
    assert!(case.rendered.borrow().len() <= 2 * window.max(60));
    assert!(is_mounted("2039"));
}

#[gpui::test]
fn mounts_at_most_a_window_of_rows_when_a_large_batch_arrives_in_history(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(60), Shape::Page);
    page_scroll(&case, cx, 0.);
    settled(cx);
    let window = mounted().len();
    set_data(&case, cx, rows(2060));
    frame(cx);
    assert!(mounted().len() <= 2 * window);
    // The newest message is among the ones that mount.
    assert!(is_mounted("2059"));
    // A second batch and then single arrivals, still in history.
    set_data(&case, cx, rows(4060));
    frame(cx);
    assert!(mounted().len() <= 2 * window);
    for n in [4061, 4062, 4063] {
        set_data(&case, cx, rows(n));
        frame(cx);
    }
    assert!(mounted().len() <= 2 * window);
    assert!(is_mounted("4062"));
}

#[gpui::test]
fn mounts_no_extra_rows_when_another_thread_replaces_the_rows_in_history(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, thread("a", 3000), Shape::Page);
    page_scroll(&case, cx, 0.);
    settled(cx);
    assert!(!is_mounted("a2999"));
    assert!(is_mounted("a0"));
    let in_history = mounted().len();
    case.rendered.borrow_mut().clear();
    set_data(&case, cx, thread("b", 3000));
    settled(cx);
    assert!(case.rendered.borrow().len() <= 2 * in_history.max(60));
}

// ---- The touch write-gate ------------------------------------------------------

#[gpui::test]
fn suppresses_scroll_writes_while_a_finger_scrolls_momentum_included(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(50), Shape::Page);
    touch(cx, TouchPhase::Started);
    finger_scroll(cx, -300.);
    frame(cx);
    let held = scroll_top(&case);
    // A declaration while the finger owns the viewport writes nothing.
    cx.update(|window, _| case.handle.scroll_to_distance_from_bottom_px(0., window));
    frame(cx);
    assert_eq!(scroll_top(&case), held);
    // The lift starts the momentum: still the finger's.
    touch(cx, TouchPhase::Ended);
    cx.update(|window, _| case.handle.scroll_to_distance_from_bottom_px(0., window));
    frame(cx);
    assert_eq!(scroll_top(&case), held);
}

#[gpui::test]
fn hands_the_viewport_back_when_the_finger_scroll_ends(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(50), Shape::Page);
    touch(cx, TouchPhase::Started);
    finger_scroll(cx, -300.);
    frame(cx);
    let held = scroll_top(&case);
    finger_scroll_ends(cx);
    cx.update(|window, _| case.handle.scroll_to_distance_from_bottom_px(0., window));
    settled(cx);
    assert_ne!(scroll_top(&case), held);
}

#[gpui::test]
fn opens_no_gate_for_a_scroll_with_no_finger_down(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(50), Shape::Page);
    wheel(cx, -300.);
    frame(cx);
    let moved = scroll_top(&case);
    cx.update(|window, _| case.handle.scroll_to_distance_from_bottom_px(0., window));
    settled(cx);
    assert_ne!(scroll_top(&case), moved);
}

#[gpui::test]
fn survives_a_gesture_that_ends_with_nothing_rendered(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(5), Shape::Page);
    touch(cx, TouchPhase::Started);
    finger_scroll(cx, -100.);
    frame(cx);
    // The list empties under the finger.
    set_data(&case, cx, rows(0));
    frame(cx);
    finger_scroll_ends(cx);
    wait(cx, 500);
    frame(cx);
    assert_eq!(children(), [Child::Top, Child::Bottom]);
}

// ---- Keyboard and assistive technology ---------------------------------------

#[gpui::test]
fn keeps_a_focused_row_mounted_while_the_window_moves_away_from_it(cx: &mut TestAppContext) {
    let (case, cx) = mount_with(cx, rows(60), Shape::Page, true);
    let handle = case.focus.borrow()["58"].clone();
    cx.update(|window, cx| window.focus(&handle, cx));
    frame(cx);
    page_scroll(&case, cx, 0.);
    settled(cx);
    assert!(is_mounted("58"), "the focused row stays");
    assert!(cx.update(|window, _| handle.is_focused(window)));
    assert!(!is_mounted("50"));
    // Focus leaves the list: the row is no longer kept.
    cx.update(|window, cx| window.blur(cx));
    settled(cx);
    assert!(!is_mounted("58"));
    assert!(!mounted().is_empty());
}

fn busy() -> bool {
    machine().borrow().busy
}

#[gpui::test]
fn delivers_the_first_message_of_an_empty_transcript_unmarked(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(0), Shape::Page);
    set_data(&case, cx, rows(1));
    frame(cx);
    assert!(is_mounted("0"));
    assert!(!busy());
    set_data(&case, cx, rows(2));
    frame(cx);
    assert!(!busy());
}

#[gpui::test]
fn marks_a_thread_loaded_into_an_empty_list_or_replacing_the_rows_busy(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(0), Shape::Page);
    // The data change draws its own frame: the commit.
    set_data(&case, cx, thread("a", 60));
    assert!(busy(), "the commit that mounts history");
    frame(cx);
    assert!(busy(), "and the frame after it");
    frame(cx);
    assert!(!busy());
    set_data(&case, cx, thread("b", 60));
    assert!(busy());
    frame(cx);
    assert!(busy());
    frame(cx);
    assert!(!busy());
}

#[gpui::test]
fn marks_the_rows_the_window_moves_over_busy_and_new_tail_rows_not(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(60), Shape::Page);
    settled(cx);
    assert!(!busy());
    case.host.update(cx, |h, cx| {
        h.data = rows(61);
        cx.notify();
    });
    frame(cx);
    assert!(!busy(), "a tail append while following is news");
    case.page.set_offset(point(px(0.), px(0.)));
    frame(cx);
    assert!(busy(), "scrolling up mounts history");
    frame(cx);
    assert!(busy());
    frame(cx);
    assert!(!busy());
}

#[gpui::test]
fn mounts_a_message_that_arrives_in_history_unmarked(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(60), Shape::Page);
    page_scroll(&case, cx, 0.);
    settled(cx);
    assert!(!is_mounted("59"));
    assert!(!busy());
    case.host.update(cx, |h, cx| {
        h.data = rows(61);
        cx.notify();
    });
    frame(cx);
    assert!(is_mounted("60"));
    assert!(!busy());
    // The reply streams: it stays mounted, and the update is not busy either.
    let mut streamed: Vec<Row> = rows(61).iter().cloned().collect();
    streamed[60].text = "message 60, streamed".into();
    case.host.update(cx, |h, cx| {
        h.data = Rc::new(streamed);
        cx.notify();
    });
    frame(cx);
    assert!(is_mounted("60"));
    assert!(!busy());
    settled(cx);
    // The rows between the window and the reply stay unmounted.
    assert!(!is_mounted("59"));
}

#[gpui::test]
fn announces_a_reply_in_history_once_and_no_history_on_the_way_down(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(60), Shape::Page);
    page_scroll(&case, cx, 0.);
    settled(cx);
    // A polite log reads out each row added while its block is not busy.
    let read_out = |before: &[SharedString]| -> Vec<SharedString> {
        if busy() {
            return Vec::new();
        }
        mounted()
            .into_iter()
            .filter(|k| !before.contains(k))
            .collect()
    };
    let before = mounted();
    case.host.update(cx, |h, cx| {
        h.data = rows(61);
        cx.notify();
    });
    frame(cx);
    assert_eq!(read_out(&before), [SharedString::from("60")]);
    let mut heard = Vec::new();
    for at in [1500., 3000., 4500., 5700.] {
        let before = mounted();
        page_scroll(&case, cx, at);
        heard.extend(read_out(&before));
        settled(cx);
    }
    assert!(is_mounted("59") && is_mounted("60"));
    assert!(heard.is_empty(), "nothing read out on the way: {heard:?}");
}

// ---- Rotation on the page scroller ------------------------------------------------

const ROW: f32 = 100.;
const PORTRAIT_ROW: f32 = 150.;

/// The phone rotates: the window takes its new size, the rows their new height, and a
/// platform that keeps the reader's place writes the page's position.
fn rotate(
    case: &Case,
    cx: &mut VisualTestContext,
    view_height: f32,
    row_height: f32,
    platform_scroll_top: Option<f64>,
) {
    case.row_height.set(row_height);
    cx.simulate_resize(size(px(400.), px(view_height)));
    if let Some(at) = platform_scroll_top {
        case.page.set_offset(point(px(0.), px(-(at as f32))));
    }
    frame(cx);
    frame(cx);
    frame(cx);
}

fn in_history(cx: &mut TestAppContext) -> (Case, &mut VisualTestContext) {
    let (case, cx) = mount(cx, rows(200), Shape::Page);
    // The reader scrolls into history: row 50 is at the top.
    page_scroll(&case, cx, 50. * f64::from(ROW));
    frame(cx);
    assert_eq!(top_row(), Some(50));
    (case, cx)
}

#[gpui::test]
fn keeps_the_top_row_through_a_rotation_to_portrait_in_history(cx: &mut TestAppContext) {
    let (case, cx) = in_history(cx);
    // The platform keeps the place by scaling the position with the content.
    rotate(
        &case,
        cx,
        700.,
        PORTRAIT_ROW,
        Some(50. * f64::from(ROW) * f64::from(PORTRAIT_ROW / ROW)),
    );
    assert!(is_at_top("50"), "{:?}", painted().first());
}

#[gpui::test]
fn keeps_the_top_row_through_a_rotation_when_the_platform_keeps_the_position(
    cx: &mut TestAppContext,
) {
    let (case, cx) = in_history(cx);
    rotate(&case, cx, 700., PORTRAIT_ROW, None);
    assert!(is_at_top("50"));
}

#[gpui::test]
fn keeps_the_top_row_through_a_rotation_back_to_landscape(cx: &mut TestAppContext) {
    let (case, cx) = in_history(cx);
    rotate(
        &case,
        cx,
        700.,
        PORTRAIT_ROW,
        Some(50. * f64::from(PORTRAIT_ROW)),
    );
    assert!(is_at_top("50"));
    let back = (scroll_top(&case) * f64::from(ROW / PORTRAIT_ROW)).round();
    rotate(&case, cx, 400., ROW, Some(back));
    assert!(is_at_top("50"));
}

#[gpui::test]
fn keeps_the_top_row_when_the_platform_writes_its_position_after_the_pass_settles(
    cx: &mut TestAppContext,
) {
    let (case, cx) = in_history(cx);
    rotate(&case, cx, 700., PORTRAIT_ROW, None);
    assert!(is_at_top("50"));
    // The platform's last write of the rotation lands after the pass has settled.
    wait(cx, 40);
    let at = scroll_top(&case) + 125.;
    page_scroll(&case, cx, at);
    frame(cx);
    frame(cx);
    assert!(is_at_top("50"));
}

#[gpui::test]
fn keeps_the_top_row_when_the_platform_scrolls_before_the_resize(cx: &mut TestAppContext) {
    let (case, cx) = in_history(cx);
    // The viewport is already portrait when the platform's scroll arrives.
    case.row_height.set(PORTRAIT_ROW);
    cx.simulate_resize(size(px(400.), px(700.)));
    case.page
        .set_offset(point(px(0.), px(-(50. * PORTRAIT_ROW + 300.))));
    frame(cx);
    rotate(&case, cx, 700., PORTRAIT_ROW, None);
    assert!(is_at_top("50"));
}

#[gpui::test]
fn follows_a_wheel_scroll_right_after_a_rotation(cx: &mut TestAppContext) {
    let (case, cx) = in_history(cx);
    rotate(&case, cx, 700., PORTRAIT_ROW, None);
    let before = top_row().expect("a row at the top");
    wheel(cx, 10. * PORTRAIT_ROW);
    frame(cx);
    frame(cx);
    assert!(top_row().expect("a row at the top") >= before + 9);
}

#[gpui::test]
fn keeps_following_the_end_through_a_rotation_to_landscape(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(60), Shape::Page);
    assert!(at_bottom(&case));
    // Landscape: rows get shorter, and the platform scales the position down.
    let scaled = (scroll_top(&case) * 0.7).round();
    rotate(&case, cx, 300., 70., Some(scaled));
    set_data(&case, cx, rows(70));
    settled(cx);
    assert!(at_bottom(&case));
}

#[gpui::test]
fn follows_a_keyboard_scroll_right_after_a_rotation(cx: &mut TestAppContext) {
    let (case, cx) = in_history(cx);
    rotate(&case, cx, 700., PORTRAIT_ROW, None);
    let before = top_row().expect("a row at the top");
    // A key ends the platform's time, even one whose handler stops it.
    cx.simulate_keystrokes("pagedown");
    let at = scroll_top(&case) + 10. * f64::from(PORTRAIT_ROW);
    page_scroll(&case, cx, at);
    frame(cx);
    assert!(top_row().expect("a row at the top") >= before + 9);
}

#[gpui::test]
fn writes_the_position_at_most_once_for_a_run_of_scrolls_after_a_rotation(cx: &mut TestAppContext) {
    let (case, cx) = in_history(cx);
    rotate(&case, cx, 700., PORTRAIT_ROW, None);
    wait(cx, 40);
    frame(cx);
    let writes = machine().borrow().writes;
    let start = scroll_top(&case);
    for step in 1..=8 {
        page_scroll(&case, cx, start + f64::from(step) * 50.);
    }
    assert!(machine().borrow().writes - writes <= 1);
}

#[gpui::test]
fn follows_a_scroll_on_its_own_scroller_in_the_frame_its_height_changes(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(200), Shape::Boxed);
    page_scroll(&case, cx, 50. * f64::from(ROW));
    frame(cx);
    let before = top_row().expect("a row at the top");
    // A growing composer shrinks the scroller in the frame the reader scrolls.
    case.box_height.set(300.);
    let at = scroll_top(&case) - 10. * f64::from(ROW);
    case.host.update(cx, |_, cx| cx.notify());
    page_scroll(&case, cx, at);
    frame(cx);
    assert!(top_row().expect("a row at the top") <= before - 9);
}

#[gpui::test]
fn writes_no_position_during_a_rotation_while_a_finger_scrolls(cx: &mut TestAppContext) {
    let (case, cx) = mount(cx, rows(200), Shape::Page);
    touch(cx, TouchPhase::Started);
    finger_scroll(cx, -5000.);
    frame(cx);
    let held = scroll_top(&case);
    rotate(&case, cx, 700., PORTRAIT_ROW, None);
    assert_eq!(scroll_top(&case), held);
    finger_scroll_ends(cx);
    settled(cx);
    assert_ne!(scroll_top(&case), held);
}

// ---- Viewport changes on the page scroller -----------------------------------------

fn passes() -> usize {
    machine().borrow().passes
}

/// One frame of a viewport change: the window takes its new size.
fn viewport_frame(cx: &mut VisualTestContext, width: f32, height: f32) -> usize {
    let start = passes();
    cx.simulate_resize(size(px(width), px(height)));
    frame(cx);
    passes() - start
}

#[gpui::test]
fn treats_a_scroller_spanning_the_window_as_the_page(cx: &mut TestAppContext) {
    let (_case, _cx) = mount(cx, rows(60), Shape::Page);
    assert!(machine().borrow().is_page_viewport);
}

#[gpui::test]
fn runs_one_correction_pass_for_a_width_drag(cx: &mut TestAppContext) {
    let (_case, cx) = mount(cx, rows(60), Shape::Page);
    assert_eq!(viewport_frame(cx, 360., 400.), 1);
}

#[gpui::test]
fn runs_one_correction_pass_for_a_height_only_change(cx: &mut TestAppContext) {
    let (_case, cx) = mount(cx, rows(60), Shape::Page);
    assert_eq!(viewport_frame(cx, 400., 360.), 1);
}

#[gpui::test]
fn runs_one_correction_pass_for_a_height_change_in_the_frame_after_a_width_change(
    cx: &mut TestAppContext,
) {
    let (_case, cx) = mount(cx, rows(60), Shape::Page);
    let width = viewport_frame(cx, 360., 400.);
    let height = viewport_frame(cx, 360., 360.);
    assert_eq!([width, height], [1, 1]);
}

#[gpui::test]
fn runs_one_correction_pass_in_each_frame_of_a_height_width_height_sequence(
    cx: &mut TestAppContext,
) {
    let (_case, cx) = mount(cx, rows(60), Shape::Page);
    let counts = [
        viewport_frame(cx, 400., 360.),
        viewport_frame(cx, 360., 360.),
        viewport_frame(cx, 360., 400.),
    ];
    assert_eq!(counts, [1, 1, 1]);
}

#[gpui::test]
fn runs_one_correction_pass_a_frame_when_a_toolbar_change_resizes_several_times(
    cx: &mut TestAppContext,
) {
    // GPUI draws a frame for each platform resize, so several resizes are several
    // frames: one pass each, and none for the frame after them.
    let (_case, cx) = mount(cx, rows(60), Shape::Page);
    let start = passes();
    for h in [380., 370., 360.] {
        cx.simulate_resize(size(px(400.), px(h)));
    }
    assert_eq!(passes() - start, 3);
    frame(cx);
    assert_eq!(passes() - start, 3);
}
