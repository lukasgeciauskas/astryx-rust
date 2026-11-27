//! Upstream `Tour.test.tsx` and `TourCardTarget.interaction.browser.spec.ts`
//! (component:Tour FR1, AR1).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use astryx_components::button::Button;
use gpui::{
    Bounds, Context, Modifiers, MouseButton, Pixels, Render, TestAppContext, VisualTestContext,
    Window, div, point, px,
};

use super::*;

type Log = Rc<RefCell<Vec<String>>>;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).expect("a keystroke");
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(selector)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let b = bounds(cx, selector).unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(b.center(), Modifiers::none());
    settle(cx);
}

fn shown(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    bounds(cx, selector).is_some()
}

/// Upstream's `TwoStepTour` harness: two button targets and a two-step tour whose
/// `active` the dismissal turns off (except a skip).
struct TwoStep {
    active: Rc<Cell<bool>>,
    backdrop: Rc<Cell<bool>>,
    count: bool,
    log: Log,
    probe: Option<Log>,
}

impl Render for TwoStep {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let a = TourTarget::new("a");
        let b = TourTarget::new("b");
        let (active, log) = (self.active.clone(), self.log.clone());
        let probe = self.probe.clone();
        div()
            .p(px(40.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .child(
                        a.wrap(
                            div()
                                .debug_selector(|| "target-a".into())
                                .child(Button::new("target-a", "Target A")),
                        )
                        .control("target-a"),
                    )
                    .child(
                        b.wrap(
                            div()
                                .debug_selector(|| "target-b".into())
                                .child(Button::new("target-b", "Target B")),
                        )
                        .control("target-b"),
                    ),
            )
            .child(
                Tour::new("tour", self.active.get(), move |source, window, _| {
                    log.borrow_mut().push(source.as_str().to_string());
                    if source != TourDismissSource::Skip {
                        active.set(false);
                    }
                    window.refresh();
                })
                .backdrop(self.backdrop.get())
                .step_count_shown(self.count)
                .children(probe.map(|log| StepProbe(log).into_any_element()))
                .child(TourStep::new("first", &a, "First").child("First body"))
                .child(TourStep::new("second", &b, "Second").child("Second body")),
            )
    }
}

/// Records what `use_tour` reports on every render.
#[derive(IntoElement)]
struct StepProbe(Log);

impl RenderOnce for StepProbe {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let line = match use_tour(cx) {
            None => "none".to_string(),
            Some(t) => format!(
                "{} {} {} {}",
                t.active_step_index, t.step_count, t.is_first_step, t.is_last_step
            ),
        };
        self.0.borrow_mut().push(line);
        div()
    }
}

struct Harness {
    active: Rc<Cell<bool>>,
    backdrop: Rc<Cell<bool>>,
    log: Log,
}

fn two_step(
    cx: &mut TestAppContext,
    backdrop: bool,
    count: bool,
) -> (&mut VisualTestContext, Harness) {
    two_step_probed(cx, backdrop, count, None)
}

fn two_step_probed(
    cx: &mut TestAppContext,
    backdrop: bool,
    count: bool,
    probe: Option<Log>,
) -> (&mut VisualTestContext, Harness) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let harness = Harness {
        active: Rc::new(Cell::new(true)),
        backdrop: Rc::new(Cell::new(backdrop)),
        log: Log::default(),
    };
    let (active, backdrop, log) = (
        harness.active.clone(),
        harness.backdrop.clone(),
        harness.log.clone(),
    );
    let (_, cx) = cx.add_window_view(move |_, _| TwoStep {
        active,
        backdrop,
        count,
        log,
        probe,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, harness)
}

#[test]
fn the_step_count_reads_index_of_count() {
    assert_eq!(step_count_text(0, 2), "1 of 2");
    assert_eq!(step_count_text(1, 5), "2 of 5");
}

#[gpui::test]
fn shows_only_the_first_step_initially(cx: &mut TestAppContext) {
    let (cx, _) = two_step(cx, false, false);
    assert!(shown(cx, "tour-step-first"));
    assert!(!shown(cx, "tour-step-second"));
}

#[gpui::test]
fn advances_to_the_next_step_on_next(cx: &mut TestAppContext) {
    let (cx, _) = two_step(cx, false, false);
    click(cx, "tour-next");
    assert!(!shown(cx, "tour-step-first"));
    assert!(shown(cx, "tour-step-second"));
}

#[gpui::test]
fn goes_back_on_back(cx: &mut TestAppContext) {
    let (cx, _) = two_step(cx, false, false);
    click(cx, "tour-next");
    click(cx, "tour-back");
    assert!(shown(cx, "tour-step-first"));
}

#[gpui::test]
fn hides_back_on_the_first_step(cx: &mut TestAppContext) {
    let (cx, _) = two_step(cx, false, false);
    assert!(!shown(cx, "tour-back"));
}

#[gpui::test]
fn labels_the_final_step_done_and_earlier_ones_next(cx: &mut TestAppContext) {
    let (cx, _) = two_step(cx, false, false);
    assert!(shown(cx, "tour-next") && !shown(cx, "tour-done"));
    click(cx, "tour-next");
    assert!(shown(cx, "tour-done") && !shown(cx, "tour-next"));
}

#[gpui::test]
fn dismisses_with_complete_past_the_last_step(cx: &mut TestAppContext) {
    let (cx, h) = two_step(cx, false, false);
    click(cx, "tour-next");
    click(cx, "tour-done");
    assert_eq!(*h.log.borrow(), ["complete"]);
    assert!(!shown(cx, "tour-step-first") && !shown(cx, "tour-step-second"));
}

#[gpui::test]
fn shows_the_step_count_when_asked(cx: &mut TestAppContext) {
    let (cx, _) = two_step(cx, false, true);
    assert!(shown(cx, "tour-step-count"));
    click(cx, "tour-next");
    assert!(shown(cx, "tour-step-count"));
}

#[gpui::test]
fn shows_no_step_count_by_default(cx: &mut TestAppContext) {
    let (cx, _) = two_step(cx, false, false);
    assert!(!shown(cx, "tour-step-count"));
}

#[gpui::test]
fn draws_a_highlight_for_the_active_step_and_a_backdrop_only_when_set(cx: &mut TestAppContext) {
    let (cx, h) = two_step(cx, false, false);
    assert!(shown(cx, "tour-highlight"));
    assert!(
        !shown(cx, "tour-backdrop"),
        "a plain coachmark only rings the target"
    );
    h.backdrop.set(true);
    settle(cx);
    assert!(shown(cx, "tour-highlight"));
    assert!(shown(cx, "tour-backdrop"));
}

#[gpui::test]
fn dismisses_with_backdrop_when_the_backdrop_is_pressed(cx: &mut TestAppContext) {
    let (cx, h) = two_step(cx, true, false);
    // A press on the dimmed page (a corner, far from the callout).
    cx.simulate_click(point(px(4.), px(4.)), Modifiers::none());
    settle(cx);
    assert_eq!(*h.log.borrow(), ["backdrop"]);
}

#[gpui::test]
fn a_press_on_the_target_under_a_backdrop_ends_the_tour(cx: &mut TestAppContext) {
    let (cx, h) = two_step(cx, true, false);
    click(cx, "target-a");
    assert_eq!(*h.log.borrow(), ["backdrop"]);
}

#[gpui::test]
fn does_not_restyle_the_target(cx: &mut TestAppContext) {
    let (cx, h) = two_step(cx, true, false);
    let with = bounds(cx, "target-a").expect("drawn");
    h.active.set(false);
    settle(cx);
    let without = bounds(cx, "target-a").expect("drawn");
    assert_eq!(
        with, without,
        "the ring is an overlay; the target keeps its box"
    );
}

#[gpui::test]
fn moves_the_highlight_across_steps_and_clears_it_when_the_tour_ends(cx: &mut TestAppContext) {
    let (cx, _) = two_step(cx, true, false);
    let ring = |cx: &mut VisualTestContext| bounds(cx, "tour-highlight").expect("a highlight");
    let a = bounds(cx, "target-a").expect("drawn");
    assert_eq!(ring(cx), a.dilate(px(HIGHLIGHT_PADDING)));
    click(cx, "tour-next");
    let b = bounds(cx, "target-b").expect("drawn");
    assert_eq!(ring(cx), b.dilate(px(HIGHLIGHT_PADDING)));
    click(cx, "tour-done");
    assert!(!shown(cx, "tour-highlight"));
    assert!(!shown(cx, "tour-backdrop"));
}

#[gpui::test]
fn renders_nothing_while_inactive(cx: &mut TestAppContext) {
    let (cx, h) = two_step(cx, false, false);
    h.active.set(false);
    settle(cx);
    assert!(!shown(cx, "tour-step-first"));
    assert!(!shown(cx, "tour-highlight"));
}

/// A page that renders whatever the test builds.
type Build = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Page(Build);

impl Render for Page {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(px(40.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child((self.0)(window, cx))
    }
}

fn page(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Page(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

#[gpui::test]
fn a_tour_step_outside_a_tour_renders_nothing(cx: &mut TestAppContext) {
    let cx = page(cx, |_, _| {
        let t = TourTarget::new("t");
        div()
            .child(t.wrap(div().child("T")))
            .child(TourStep::new("orphan", &t, "Orphan").child("Body"))
            .into_any_element()
    });
    assert!(!shown(cx, "tour-step-orphan"));
    assert!(!shown(cx, "tour-highlight"));
}

#[gpui::test]
fn use_tour_returns_none_outside_a_tour(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = page(cx, {
        let log = log.clone();
        move |_, _| StepProbe(log.clone()).into_any_element()
    });
    let _ = cx;
    assert_eq!(log.borrow().last().map(String::as_str), Some("none"));
}

#[gpui::test]
fn use_tour_reports_the_step_position_inside_a_tour(cx: &mut TestAppContext) {
    let probe = Log::default();
    let (_cx, _) = two_step_probed(cx, false, false, Some(probe.clone()));
    // Index 0 of 2: first, not last.
    assert_eq!(
        probe.borrow().last().map(String::as_str),
        Some("0 2 true false")
    );
}

#[gpui::test]
fn resets_to_the_first_step_in_the_frame_that_deactivates(cx: &mut TestAppContext) {
    let probe = Log::default();
    let (cx, h) = two_step_probed(cx, false, false, Some(probe.clone()));
    click(cx, "tour-next");
    assert!(probe.borrow().last().is_some_and(|l| l.starts_with("1 ")));
    probe.borrow_mut().clear();
    h.active.set(false);
    settle(cx);
    assert!(!probe.borrow().is_empty());
    assert!(
        probe.borrow().iter().all(|l| l.starts_with("0 ")),
        "no frame carries the old step: {:?}",
        probe.borrow()
    );
}

/// Upstream's `CardTargetTour`: a card step, then a step on a card action, with page
/// controls around the tour.
struct CardTour {
    active: Rc<Cell<bool>>,
    unmount: bool,
    log: Log,
}

impl Render for CardTour {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let card = TourTarget::new("card");
        let action = TourTarget::new("action");
        let (active, log) = (self.active.clone(), self.log.clone());
        let (on_action, on_after, on_start) = (log.clone(), log.clone(), log.clone());
        let start_active = self.active.clone();
        let tour = (!self.unmount || self.active.get()).then(|| {
            Tour::new("tour", self.active.get(), move |source, window, _| {
                log.borrow_mut().push(source.as_str().to_string());
                active.set(false);
                window.refresh();
            })
            .child(TourStep::new("revenue", &card, "Your revenue").child("Totals for the quarter."))
            .child(TourStep::new("act", &action, "Act on it").child("Press the action to try it."))
        });
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(16.))
            .child(
                card.wrap(
                    div()
                        .debug_selector(|| "card".into())
                        .w(px(240.))
                        .h(px(80.))
                        .child("Revenue card"),
                ),
            )
            .child(
                action
                    .wrap(
                        div().debug_selector(|| "card-action".into()).child(
                            Button::new("card-action", "Card action").on_click(move |_, _, _| {
                                on_action.borrow_mut().push("action".into())
                            }),
                        ),
                    )
                    .control("card-action"),
            )
            .child(div().debug_selector(|| "start".into()).child(
                Button::new("start", "Start tour").on_click(move |_, window, _| {
                    on_start.borrow_mut().push("start".into());
                    start_active.set(true);
                    window.refresh();
                }),
            ))
            .children(tour)
            .child(
                // Clear of the callouts.
                div().mt(px(300.)).debug_selector(|| "after".into()).child(
                    Button::new("after", "After the tour")
                        .on_click(move |_, _, _| on_after.borrow_mut().push("after".into())),
                ),
            )
    }
}

fn card_tour(
    cx: &mut TestAppContext,
    active: bool,
    unmount: bool,
) -> (&mut VisualTestContext, Rc<Cell<bool>>, Log) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let flag = Rc::new(Cell::new(active));
    let log = Log::default();
    let (_, cx) = cx.add_window_view({
        let (flag, log) = (flag.clone(), log.clone());
        move |_, _| CardTour {
            active: flag,
            unmount,
            log,
        }
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, flag, log)
}

#[gpui::test]
fn the_callout_anchors_below_a_card_target(cx: &mut TestAppContext) {
    let (cx, _, _) = card_tour(cx, true, false);
    let ring = bounds(cx, "tour-highlight").expect("the highlight over the card");
    let card = bounds(cx, "card").expect("drawn");
    assert_eq!(ring, card.dilate(px(HIGHLIGHT_PADDING)));
    let callout = bounds(cx, "tour-step-revenue").expect("the callout");
    assert!(
        callout.top() >= ring.bottom() - px(1.),
        "below the highlight"
    );
    assert!(
        callout.left() < ring.right() && callout.right() > ring.left(),
        "overlapping it"
    );
}

#[gpui::test]
fn focus_moves_into_the_callout_as_a_step_shows(cx: &mut TestAppContext) {
    let (cx, _, log) = card_tour(cx, true, false);
    // The region holds focus: the next tab stop is its close control.
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["close"]);
}

#[gpui::test]
fn tab_moves_past_the_step_into_the_page(cx: &mut TestAppContext) {
    let (cx, _, log) = card_tour(cx, true, false);
    press(cx, "tab"); // close
    press(cx, "tab"); // next
    press(cx, "tab"); // out of the callout, into the page
    press(cx, "enter");
    let log = log.borrow();
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(
        ["action", "start", "after"].contains(&log[0].as_str()),
        "a page control: {log:?}"
    );
    drop(log);
    assert!(shown(cx, "tour-step-revenue"), "the tour keeps running");
}

#[gpui::test]
fn keeps_the_tour_running_when_the_target_is_pressed(cx: &mut TestAppContext) {
    let (cx, _, log) = card_tour(cx, true, false);
    click(cx, "tour-next");
    click(cx, "card-action");
    assert_eq!(
        *log.borrow(),
        ["action"],
        "the target acted; the tour was not dismissed"
    );
    assert!(shown(cx, "tour-step-act"));
}

#[gpui::test]
fn keeps_the_tour_running_when_the_page_is_pressed(cx: &mut TestAppContext) {
    let (cx, _, log) = card_tour(cx, true, false);
    click(cx, "after");
    assert_eq!(*log.borrow(), ["after"]);
    assert!(shown(cx, "tour-step-revenue"));
}

#[gpui::test]
fn ends_the_tour_with_escape(cx: &mut TestAppContext) {
    let (cx, _, log) = card_tour(cx, true, false);
    press(cx, "escape");
    assert_eq!(*log.borrow(), ["escape"]);
    assert!(!shown(cx, "tour-step-revenue"));
}

#[gpui::test]
fn ends_the_tour_with_close_from_the_close_control(cx: &mut TestAppContext) {
    let (cx, _, log) = card_tour(cx, true, false);
    click(cx, "tour-close");
    assert_eq!(*log.borrow(), ["close"]);
}

#[gpui::test]
fn returns_focus_to_a_focusable_target_when_the_tour_ends(cx: &mut TestAppContext) {
    let (cx, _, log) = card_tour(cx, true, false);
    click(cx, "tour-next");
    click(cx, "tour-close");
    press(cx, "enter");
    assert_eq!(
        *log.borrow(),
        ["close", "action"],
        "focus went to the card action"
    );
}

#[gpui::test]
fn returns_focus_to_the_opener_on_a_target_that_cannot_take_focus(cx: &mut TestAppContext) {
    let (cx, _, log) = card_tour(cx, false, false);
    click(cx, "start");
    assert!(shown(cx, "tour-step-revenue"));
    click(cx, "tour-close");
    press(cx, "enter");
    assert_eq!(
        *log.borrow(),
        ["start", "close", "start"],
        "focus went back to Start tour"
    );
}

#[gpui::test]
fn returns_focus_to_the_opener_when_the_app_unmounts_the_tour(cx: &mut TestAppContext) {
    let (cx, _, log) = card_tour(cx, false, true);
    click(cx, "start");
    assert!(shown(cx, "tour-step-revenue"));
    click(cx, "tour-close");
    assert!(!shown(cx, "tour-step-revenue"));
    press(cx, "enter");
    assert_eq!(
        *log.borrow(),
        ["start", "close", "start"],
        "focus went back to Start tour"
    );
}

#[gpui::test]
fn a_held_press_on_a_target_leaves_the_callout_in_place(cx: &mut TestAppContext) {
    let (cx, _, _) = card_tour(cx, true, false);
    click(cx, "tour-next");
    let before = bounds(cx, "tour-step-act").expect("the callout");
    let target = bounds(cx, "card-action").expect("drawn");
    cx.simulate_mouse_down(target.center(), MouseButton::Left, Modifiers::none());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    settle(cx);
    let held = bounds(cx, "tour-step-act").expect("still shown");
    cx.simulate_mouse_up(target.center(), MouseButton::Left, Modifiers::none());
    settle(cx);
    assert!((f32::from(held.left() - before.left())).abs() <= 0.5);
    assert!((f32::from(held.top() - before.top())).abs() <= 0.5);
}
