use std::rc::Rc;

use gpui::{Context, Render, TestAppContext, VisualTestContext};

use super::*;

const LONG: &str = "Astryx is an open source design system that grew inside Meta over the last eight years, where it became the most-used and largest design system in the company.";

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).child((self.0)())
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
    cx.run_until_parked();
    // The probe's measure takes effect on the next frame.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx
}

fn tab(cx: &mut VisualTestContext) {
    let keystroke = gpui::Keystroke::parse("tab").unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    cx.run_until_parked();
}

fn height(cx: &mut VisualTestContext, selector: &'static str) -> f32 {
    let bounds = cx.debug_bounds(selector).expect("the clamp is drawn");
    f32::from(bounds.size.height)
}

#[gpui::test]
fn it_clamps_to_its_lines(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .child(LineClamp::new("one", 1).child(LONG))
            .child(LineClamp::new("two", 2).child(LONG))
            .child(LineClamp::new("free", 0).child(LONG))
            .into_any_element()
    });
    let (one, two, free) = (
        height(cx, "one-line-clamp"),
        height(cx, "two-line-clamp"),
        height(cx, "free-line-clamp"),
    );
    assert!(one > 0., "it renders its children");
    assert!((two - 2. * one).abs() < 1., "two lines: {two} vs {one}");
    assert!(free > two, "max_lines 0 clamps nothing");
}

#[gpui::test]
fn short_content_is_not_clamped_and_takes_no_tab_stop(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        LineClamp::new("short", 3).child("Short").into_any_element()
    });
    tab(cx);
    assert!(cx.debug_bounds("short-tooltip-surface").is_none());
}

#[gpui::test]
fn a_clamped_root_is_a_tab_stop_whose_focus_opens_the_full_text(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        LineClamp::new("long", 2).child(LONG).into_any_element()
    });
    assert!(
        cx.debug_bounds("long-tooltip-surface").is_none(),
        "closed before focus"
    );
    tab(cx);
    assert!(
        cx.debug_bounds("long-tooltip-surface").is_some(),
        "keyboard focus opens it"
    );
}

#[gpui::test]
fn without_the_tooltip_a_clamped_root_takes_no_tab_stop(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        LineClamp::new("quiet", 2)
            .truncate_tooltip(false)
            .child(LONG)
            .into_any_element()
    });
    tab(cx);
    assert!(cx.debug_bounds("quiet-tooltip-surface").is_none());
}

#[gpui::test]
fn mixed_children_share_the_clamped_lines(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .child(LineClamp::new("plain", 1).child("Some"))
            .child(
                LineClamp::new("mixed", 2)
                    .child("Some ")
                    .child(rustryx_components::text::Text::new("mixed"))
                    .child(" inline content that runs on and on past the second line of the clamp"),
            )
            .into_any_element()
    });
    let (line, mixed) = (
        height(cx, "plain-line-clamp"),
        height(cx, "mixed-line-clamp"),
    );
    assert!((mixed - 2. * line).abs() < 1., "{mixed} vs {line}");
}

#[gpui::test]
fn element_children_are_measured_too_and_their_text_fills_the_tooltip(cx: &mut TestAppContext) {
    // Upstream measures the element's text content, whatever wraps it.
    let cx = host(cx, || {
        LineClamp::new("rich", 1)
            .child(rustryx_components::text::Text::new(
                "Astryx is an open source design system ",
            ))
            .child(rustryx_components::text::Text::new("that grew inside Meta").italic())
            .into_any_element()
    });
    tab(cx);
    assert!(
        cx.debug_bounds("rich-tooltip-surface").is_some(),
        "a clamp of Text children is a tab stop whose focus opens the tooltip"
    );
}
