//! Carousel tests, numbered after its spec.

use std::time::Duration;

use gpui::{
    IntoElement, Modifiers, ParentElement, ScrollDelta, ScrollEvent, Styled, TestAppContext,
    TouchPhase, VisualTestContext, div, point, px,
};

use super::*;
use crate::segmented::tests::host;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..(ms / 16).max(1) {
        cx.executor().advance_clock(Duration::from_millis(16));
        settle(cx);
    }
}

/// Ten 100px items, no gap, in a 350px row: it scrolls to 650.
fn row(
    cx: &mut TestAppContext,
    build: impl Fn(Carousel) -> Carousel + 'static,
) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        div()
            .w(px(350.))
            .child(
                build(Carousel::new("c").gap(crate::layout::SpacingStep::S0))
                    .children((0..10).map(|i| div().w(px(100.)).h(px(60.)).child(format!("{i}")))),
            )
            .into_any_element()
    });
    settle(cx);
    cx
}

/// How far the row has scrolled.
fn at(cx: &mut VisualTestContext) -> f32 {
    let port = cx.debug_bounds("c-scroller").unwrap();
    let first = cx.debug_bounds("c-item-0").unwrap();
    f32::from(port.left() - first.left())
}

fn wheel(cx: &mut VisualTestContext, dx: f32, phase: TouchPhase, shift: bool) {
    let at = cx.debug_bounds("c-scroller").unwrap().center();
    cx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(dx), px(0.))),
        modifiers: Modifiers {
            shift,
            ..Default::default()
        },
        touch_phase: phase,
        ..Default::default()
    });
    settle(cx);
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

#[gpui::test]
fn fr1_fr4_a_run_out_button_hides_and_next_steps_about_a_view(cx: &mut TestAppContext) {
    let cx = row(cx, |c| c);
    assert!(
        cx.debug_bounds("c-prev").is_none(),
        "nothing before the start"
    );
    assert!(cx.debug_bounds("c-next").is_some());
    click(cx, "c-next");
    wait(cx, 400);
    // A view less half an item: 350 - 50.
    assert_eq!(at(cx), 300.);
    assert!(cx.debug_bounds("c-prev").is_some());
}

#[gpui::test]
fn fr6_native_snap_settles_a_wheel_on_a_stop(cx: &mut TestAppContext) {
    let cx = row(cx, |c| c.snap(true));
    wheel(cx, -130., TouchPhase::Moved, false);
    wait(cx, 600);
    assert_eq!(at(cx), 100., "the nearest item start");
}

#[gpui::test]
fn fr7_snap_paging_moves_one_item_per_flick(cx: &mut TestAppContext) {
    let cx = row(cx, |c| c.snap(use_snap_paging(SnapPagingPages::Item)));
    wheel(cx, 0., TouchPhase::Started, false);
    wheel(cx, -260., TouchPhase::Moved, false);
    wheel(cx, 0., TouchPhase::Ended, false);
    wait(cx, 600);
    assert_eq!(at(cx), 100., "at most one page from where the drag began");
}

#[gpui::test]
fn has_loop_wraps_next_at_the_end_to_the_start(cx: &mut TestAppContext) {
    let handle = CarouselHandle::new();
    let cx = row(cx, {
        let handle = handle.clone();
        move |c| c.looping(true).handle(handle.clone())
    });
    cx.update(|window, cx| handle.scroll_to(9, window, cx));
    wait(cx, 400);
    assert_eq!(at(cx), 650.);
    assert!(handle.can_scroll_next(), "looping keeps next available");
    click(cx, "c-next");
    wait(cx, 400);
    assert_eq!(at(cx), 0.);
}

#[gpui::test]
fn shift_and_the_wheel_scroll_sideways(cx: &mut TestAppContext) {
    let cx = row(cx, |c| c);
    let at_point = cx.debug_bounds("c-scroller").unwrap().center();
    cx.simulate_event(ScrollEvent {
        position: at_point,
        delta: ScrollDelta::Pixels(point(px(0.), px(-80.))),
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
    settle(cx);
    assert!(at(cx) > 0.);
}
