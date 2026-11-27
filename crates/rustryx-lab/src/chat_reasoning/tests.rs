//! Upstream `ChatReasoning.test.tsx`, plus the keyboard and the transition.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, Context, Modifiers, Render, TestAppContext, VisualTestContext, Window, div, px,
};
use rustryx_components::button::Button;

use super::*;

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(600.)).child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

/// Lets the open and close transition (300 ms) finish.
fn finish_transition(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(400));
    settle(cx);
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

fn click_header(cx: &mut VisualTestContext) {
    let header = cx
        .debug_bounds("chat-reasoning-header")
        .expect("the header is drawn");
    cx.simulate_click(header.center(), Modifiers::none());
    settle(cx);
}

fn content_height(cx: &mut VisualTestContext) -> f32 {
    let content = cx
        .debug_bounds("chat-reasoning-content")
        .expect("the content is always drawn");
    f32::from(content.size.height)
}

type Changes = Rc<RefCell<Vec<bool>>>;

fn reasoning(
    changes: Changes,
    build: impl Fn(ChatReasoning) -> ChatReasoning + 'static,
) -> impl Fn() -> AnyElement {
    move || {
        let changes = changes.clone();
        build(
            ChatReasoning::new("r")
                .on_expanded_change(move |open, _, _| changes.borrow_mut().push(open)),
        )
        .into_any_element()
    }
}

#[test]
fn line_parts_follow_upstream_conditions() {
    // Duration: given and not streaming.
    assert!(line_parts(true, None, false, false).duration);
    assert!(!line_parts(true, None, true, false).duration);
    assert!(!line_parts(false, None, false, false).duration);
    // Preview: collapsed, string content (not empty), not streaming.
    assert!(line_parts(false, Some("text"), false, false).preview);
    assert!(!line_parts(false, Some("text"), false, true).preview);
    assert!(!line_parts(false, Some("text"), true, false).preview);
    assert!(!line_parts(false, Some(""), false, false).preview);
    assert!(!line_parts(false, None, false, false).preview);
}

#[test]
fn the_label_defaults_to_thinking_and_takes_a_custom_one() {
    // "renders collapsed by default" (the label) and "supports custom label".
    assert_eq!(ChatReasoning::new("a").label, "Thinking");
    assert_eq!(
        ChatReasoning::new("b").label("Processing").label,
        "Processing"
    );
}

#[test]
fn the_shimmer_band_sweeps_across_the_label() {
    let secondary = gpui::hsla(0., 0., 0.4, 1.);
    let disabled = gpui::hsla(0., 0., 0.8, 1.);
    let close = |a: Hsla, b: Hsla| (Rgba::from(a).r - Rgba::from(b).r).abs() < 1e-4;
    // At the start, the start edge sits on an image end (secondary) and the far edge on
    // its middle (disabled).
    assert!(close(
        shimmer_color(0., Duration::ZERO, secondary, disabled),
        secondary
    ));
    assert!(close(
        shimmer_color(1., Duration::ZERO, secondary, disabled),
        disabled
    ));
    // A quarter period later (1 s), the band has moved a full label width: the start
    // edge now holds the middle.
    assert!(close(
        shimmer_color(0., Duration::from_secs(1), secondary, disabled),
        disabled
    ));
    // Halfway between, the mix is even.
    let mid = Rgba::from(shimmer_color(0.5, Duration::ZERO, secondary, disabled)).r;
    let expected = (Rgba::from(secondary).r + Rgba::from(disabled).r) / 2.;
    assert!((mid - expected).abs() < 1e-3, "{mid} vs {expected}");
    // It repeats every 4 s.
    assert!(close(
        shimmer_color(0.3, Duration::from_secs(4), secondary, disabled),
        shimmer_color(0.3, Duration::ZERO, secondary, disabled)
    ));
}

#[gpui::test]
fn renders_collapsed_by_default(cx: &mut TestAppContext) {
    let changes = Changes::default();
    let cx = host(
        cx,
        reasoning(changes.clone(), |r| r.text("Some reasoning text")),
    );
    assert!(cx.debug_bounds("chat-reasoning-label").is_some());
    assert_eq!(content_height(cx), 0., "the content is clipped away");
    assert!(changes.borrow().is_empty());
}

#[gpui::test]
fn shows_preview_text_when_collapsed(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        reasoning(Changes::default(), |r| {
            r.text("Some reasoning text about constraints")
        }),
    );
    assert!(cx.debug_bounds("chat-reasoning-preview").is_some());
}

#[gpui::test]
fn element_children_show_no_preview(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        reasoning(Changes::default(), |r| r.child("Rich content")),
    );
    assert!(cx.debug_bounds("chat-reasoning-preview").is_none());
}

#[gpui::test]
fn expands_on_click(cx: &mut TestAppContext) {
    let changes = Changes::default();
    let cx = host(
        cx,
        reasoning(changes.clone(), |r| r.text("Reasoning content here")),
    );
    click_header(cx);
    assert_eq!(*changes.borrow(), [true]);
    finish_transition(cx);
    assert!(content_height(cx) > 0., "the content opens");
    assert!(
        cx.debug_bounds("chat-reasoning-preview").is_none(),
        "an expanded line drops the preview"
    );
    click_header(cx);
    assert_eq!(*changes.borrow(), [true, false]);
    finish_transition(cx);
    assert_eq!(content_height(cx), 0.);
}

#[gpui::test]
fn the_content_eases_open(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        reasoning(Changes::default(), |r| r.text("Reasoning content here")),
    );
    click_header(cx);
    cx.executor().advance_clock(Duration::from_millis(150));
    settle(cx);
    let midway = content_height(cx);
    finish_transition(cx);
    let open = content_height(cx);
    assert!(
        midway > 0. && midway < open,
        "{midway} on the way to {open}"
    );
}

#[gpui::test]
fn shows_duration_when_not_streaming(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        reasoning(Changes::default(), |r| r.duration("12s").text("Text")),
    );
    assert!(cx.debug_bounds("chat-reasoning-duration").is_some());
}

#[gpui::test]
fn hides_duration_when_streaming(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        reasoning(Changes::default(), |r| {
            r.duration("12s").streaming(true).text("Text")
        }),
    );
    assert!(cx.debug_bounds("chat-reasoning-duration").is_none());
    assert!(cx.debug_bounds("chat-reasoning-preview").is_none());
    assert!(cx.debug_bounds("chat-reasoning-label").is_some());
}

#[gpui::test]
fn supports_default_expanded(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        reasoning(Changes::default(), |r| {
            r.default_expanded(true).text("Content")
        }),
    );
    assert!(
        content_height(cx) > 0.,
        "open from the start, with no transition"
    );
}

#[gpui::test]
fn a_controlled_state_only_reports(cx: &mut TestAppContext) {
    let changes = Changes::default();
    let cx = host(
        cx,
        reasoning(changes.clone(), |r| r.expanded(false).text("Content")),
    );
    click_header(cx);
    assert_eq!(*changes.borrow(), [true]);
    finish_transition(cx);
    assert_eq!(content_height(cx), 0., "the caller did not open it");
}

#[gpui::test]
fn enter_and_space_toggle_once_each(cx: &mut TestAppContext) {
    let changes = Changes::default();
    let cx = host(cx, reasoning(changes.clone(), |r| r.text("Content")));
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*changes.borrow(), [true], "Enter toggles on key down, once");
    press(cx, "space");
    assert_eq!(*changes.borrow(), [true, false]);
}

#[gpui::test]
fn the_collapsed_content_is_inert_and_lifts_when_expanded(cx: &mut TestAppContext) {
    // Upstream marks collapsed content `inert`: out of the tab order (and the
    // accessibility tree). A button inside it takes no Tab while collapsed; Enter then
    // presses whatever did take focus.
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let clicks = clicks.clone();
        move || {
            let (inside, after) = (clicks.clone(), clicks.clone());
            div()
                .child(
                    ChatReasoning::new("r").child(
                        Button::new("inside", "Inside")
                            .on_click(move |_, _, _| inside.borrow_mut().push("inside")),
                    ),
                )
                .child(
                    Button::new("after", "After")
                        .on_click(move |_, _, _| after.borrow_mut().push("after")),
                )
                .into_any_element()
        }
    });
    press(cx, "tab"); // the header
    press(cx, "tab"); // past the inert content
    press(cx, "enter");
    assert_eq!(
        *clicks.borrow(),
        ["after"],
        "the collapsed content takes no tab stop"
    );
    // Open it (a click also focuses the header), and the button inside is next.
    click_header(cx);
    finish_transition(cx);
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(
        *clicks.borrow(),
        ["after", "inside"],
        "the expanded content's button takes Tab"
    );
}
