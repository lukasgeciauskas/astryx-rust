//! ComplexSelector behavior tests, numbered after the spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Context, InteractiveElement, IntoElement, Modifiers, ParentElement, Pixels, Point, Render,
    SharedString, Styled, TestAppContext, VisualTestContext, Window, div, px,
};

use super::*;
use crate::button::Button;
use crate::field::InputStatus;

type Log = Rc<RefCell<Vec<String>>>;
type Value = Rc<RefCell<SharedString>>;
type Adjust = Rc<
    dyn Fn(
        ComplexSelector<SharedString>,
        &mut Window,
        &mut gpui::App,
    ) -> ComplexSelector<SharedString>,
>;
type Held = Rc<RefCell<Option<ComplexSelectorController>>>;

struct Host {
    value: Value,
    log: Log,
    adjust: Adjust,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (store, log) = (self.value.clone(), self.log.clone());
        let seen = self.log.clone();
        let current = self.value.borrow().clone();
        let selector = ComplexSelector::new("c", "Fruit", current.clone())
            .trigger_label(current)
            .on_change(move |next: SharedString, window, _| {
                log.borrow_mut().push(format!("change {next}"));
                *store.borrow_mut() = next;
                window.refresh();
            })
            .content(move |value, commit, close, state, _, _| {
                seen.borrow_mut()
                    .push(format!("content {value} busy={}", state.busy));
                let (pick, shut) = (commit.clone(), close.clone());
                // Content-sized rows, so a press at a row's center lands on
                // its button.
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .child(
                        div().debug_selector(|| "pick".into()).child(
                            Button::new("pick", "Pick Cherry")
                                .on_click(move |_, window, cx| pick("Cherry".into(), window, cx)),
                        ),
                    )
                    .child(div().debug_selector(|| "done".into()).child(
                        Button::new("done", "Done").on_click(move |_, window, cx| shut(window, cx)),
                    ))
                    .into_any_element()
            });
        let selector = (self.adjust)(selector, window, cx);
        let after = self.log.clone();
        div()
            .flex()
            .flex_row()
            .gap(px(40.))
            .items_start()
            .pt(px(120.))
            .pl(px(120.))
            .child(rustryx_primitives::announcer())
            .child(div().w(px(240.)).child(selector))
            .child(
                div().debug_selector(|| "after".into()).child(
                    Button::new("after", "After")
                        .on_click(move |_, _, _| after.borrow_mut().push("after".into())),
                ),
            )
    }
}

fn host(
    cx: &mut TestAppContext,
    adjust: impl Fn(
        ComplexSelector<SharedString>,
        &mut Window,
        &mut gpui::App,
    ) -> ComplexSelector<SharedString>
    + 'static,
) -> (&mut VisualTestContext, Value, Log) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
    });
    let value: Value = Rc::new(RefCell::new("Apple".into()));
    let log = Log::default();
    let (v, l) = (value.clone(), log.clone());
    let adjust: Adjust = Rc::new(adjust);
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        value: v,
        log: l,
        adjust,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, value, log)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    cx.executor().advance_clock(Duration::from_millis(400));
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
    settle(cx);
}

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .is_some()
}

fn center(cx: &mut VisualTestContext, selector: &str) -> Point<Pixels> {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
        .center()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let at = center(cx, selector);
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn is_open(cx: &mut VisualTestContext) -> bool {
    shown(cx, "c-content")
}

fn lines(log: &Log, prefix: &str) -> Vec<String> {
    log.borrow()
        .iter()
        .filter(|l| l.starts_with(prefix))
        .cloned()
        .collect()
}

#[gpui::test]
fn fr1_the_content_commits_through_on_change_then_the_action(cx: &mut TestAppContext) {
    let order = Log::default();
    let o = order.clone();
    let (cx, value, log) = host(cx, move |c, _, _| {
        let o = o.clone();
        c.change_action(move |next, _, cx| {
            o.borrow_mut().push(format!("action {next}"));
            cx.background_executor().timer(Duration::from_secs(5))
        })
    });
    click(cx, "c-control");
    assert!(is_open(cx));
    click(cx, "pick");
    assert_eq!(lines(&log, "change"), ["change Cherry"]);
    assert_eq!(
        *order.borrow(),
        ["action Cherry"],
        "the action follows on_change"
    );
    assert_eq!(*value.borrow(), "Cherry");
    // FR6: the content sees busy while the action runs.
    assert!(
        lines(&log, "content")
            .last()
            .unwrap()
            .ends_with("busy=true")
    );
    assert!(shown(cx, "c-busy"));
}

#[gpui::test]
fn fr2_fr8_click_toggles_arrow_down_opens_and_the_content_closes(cx: &mut TestAppContext) {
    let opens = Log::default();
    let o = opens.clone();
    let (cx, _, _) = host(cx, move |c, _, _| {
        let o = o.clone();
        c.on_open_change(move |open, _, _| o.borrow_mut().push(format!("{open}")))
    });
    assert!(!is_open(cx), "hidden while closed");
    click(cx, "c-control");
    assert!(is_open(cx));
    click(cx, "c-control");
    assert!(!is_open(cx), "a second press closes it");
    press(cx, "down");
    assert!(is_open(cx), "ArrowDown opens it");
    click(cx, "done");
    assert!(!is_open(cx), "the content's close helper");
    press(cx, "down");
    assert!(is_open(cx), "focus came back to the trigger");
    press(cx, "escape");
    assert!(!is_open(cx));
    assert_eq!(
        *opens.borrow(),
        ["true", "false", "true", "false", "true", "false"]
    );
}

#[gpui::test]
fn fr2_focus_enters_the_content_on_open(cx: &mut TestAppContext) {
    let (cx, _, log) = host(cx, |c, _, _| c);
    click(cx, "c-control");
    // The first control in the content has focus: Enter presses it.
    press(cx, "enter");
    assert_eq!(lines(&log, "change"), ["change Cherry"]);
}

#[gpui::test]
fn fr5_disabled_keeps_it_closed(cx: &mut TestAppContext) {
    let held: Held = Rc::default();
    let clicks = Log::default();
    let (h, c2) = (held.clone(), clicks.clone());
    let (cx, _, _) = host(cx, move |c, window, cx| {
        let controller = ComplexSelectorController::new("c", window, cx);
        *h.borrow_mut() = Some(controller.clone());
        let c2 = c2.clone();
        c.disabled(true)
            .controller(controller)
            .on_click(move |_, _, _| c2.borrow_mut().push("click".into()))
    });
    click(cx, "c-control");
    assert!(!is_open(cx));
    press(cx, "down");
    assert!(!is_open(cx));
    let controller = held.borrow().clone().unwrap();
    cx.update(|window, cx| controller.open(window, cx));
    assert!(!is_open(cx));
    cx.update(|window, cx| controller.toggle(window, cx));
    assert!(!is_open(cx));
    assert!(clicks.borrow().is_empty(), "no press reaches the caller");
}

#[gpui::test]
fn fr6_loading_is_busy(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, |c, _, _| c.loading(true));
    assert!(shown(cx, "c-busy"));
    let said = cx.update(|window, cx| {
        rustryx_primitives::announce::announcement(
            rustryx_primitives::Politeness::Polite,
            window,
            cx,
        )
        .to_string()
    });
    let loading = cx.update(|_, cx| {
        rustryx_i18n::current(cx)
            .t("@astryx.spinner.loading")
            .to_string()
    });
    assert_eq!(said, loading, "the busy start is announced");
}

#[gpui::test]
fn fr7_an_attached_status_replaces_the_chevron(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, |c, _, _| c);
    assert!(shown(cx, "c-indicator"));
    let mut app = gpui::TestAppContext::single();
    let (cx, _, _) = host(&mut app, |c, _, _| c.status(InputStatus::error("Pick one")));
    assert!(!shown(cx, "c-indicator"), "the status icon takes its place");
}

#[gpui::test]
fn fr8_a_press_elsewhere_leaves_focus_there(cx: &mut TestAppContext) {
    let (cx, _, log) = host(cx, |c, _, _| c);
    click(cx, "c-control");
    assert!(is_open(cx));
    click(cx, "after");
    assert!(!is_open(cx));
    assert_eq!(lines(&log, "after"), ["after"]);
}

#[gpui::test]
fn fr8_the_controller_opens_closes_and_toggles(cx: &mut TestAppContext) {
    let held: Held = Rc::default();
    let h = held.clone();
    let (cx, _, _) = host(cx, move |c, window, cx| {
        let controller = ComplexSelectorController::new("c", window, cx);
        *h.borrow_mut() = Some(controller.clone());
        c.controller(controller)
    });
    let controller = held.borrow().clone().unwrap();
    cx.update(|window, cx| controller.open(window, cx));
    assert!(is_open(cx));
    assert!(cx.update(|_, cx| controller.is_open(cx)));
    cx.update(|window, cx| controller.toggle(window, cx));
    assert!(!is_open(cx));
    cx.update(|window, cx| controller.toggle(window, cx));
    assert!(is_open(cx));
    cx.update(|window, cx| controller.close(window, cx));
    assert!(!is_open(cx));
}

#[gpui::test]
fn fr10_a_caller_control_opens_it_and_takes_focus_back(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, |c, _, _| {
        c.render_trigger(|t, _, _| {
            t.attach(
                div()
                    .id("chip")
                    .debug_selector(|| "chip".into())
                    .size(px(24.)),
            )
            .into_any_element()
        })
    });
    assert!(!shown(cx, "c-control"), "no field of its own");
    click(cx, "chip");
    assert!(is_open(cx));
    click(cx, "done");
    assert!(!is_open(cx));
    press(cx, "down");
    assert!(is_open(cx), "focus is back on the control");
}

#[gpui::test]
fn fr11_a_caller_click_can_keep_it_closed(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, |c, _, _| {
        c.on_click(|_, window, _| window.prevent_default())
    });
    click(cx, "c-control");
    assert!(!is_open(cx));
}

#[gpui::test]
fn fr9_placement_and_alignment_follow_the_trigger(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, |c, _, _| {
        c.placement(rustryx_primitives::Placement::Below)
            .alignment(rustryx_primitives::LayerAlign::Start)
    });
    click(cx, "c-control");
    let trigger = cx.debug_bounds("c-control").unwrap();
    let content = cx.debug_bounds("c-content").unwrap();
    assert!(content.top() >= trigger.bottom(), "below the trigger");
    assert!(
        (content.left() - trigger.left()).abs() < px(1.),
        "start-aligned"
    );
}
