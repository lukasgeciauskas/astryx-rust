//! SegmentedControl tests, plus the shared host the tab and toolbar tests use.

use std::cell::RefCell;

use gpui::{
    AnyElement, Context, Modifiers, Render, TestAppContext, VisualTestContext, Window, div, point,
    px,
};

use super::*;

pub(crate) struct Host(pub Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().items_start().p(px(20.)).child((self.0)())
    }
}

pub(crate) fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.run_until_parked();
    cx
}

pub(crate) fn press(cx: &mut VisualTestContext, key: &'static str) {
    // `secondary-` is Cmd on a Mac and Ctrl elsewhere.
    let os = cx.read(|cx| cx.platform_info().os);
    let Ok(keystroke) = gpui::Keystroke::parse_with_os(key, os) else {
        panic!("not a keystroke: {key}")
    };
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
}

fn segments(log: Rc<RefCell<Vec<SharedString>>>) -> AnyElement {
    SegmentedControl::new("s", "View", "a")
        .on_change(move |value, _, _| log.borrow_mut().push(value))
        .item(SegmentedControlItem::new("a", "Alpha"))
        .item(SegmentedControlItem::new("b", "Beta").disabled(true))
        .item(SegmentedControlItem::new("c", "Gamma"))
        .into_any_element()
}

#[gpui::test]
fn arrows_select_skipping_disabled_segments(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || segments(log.clone())
    });
    press(cx, "tab");
    press(cx, "right");
    assert_eq!(*log.borrow(), [SharedString::from("c")]);
    press(cx, "home");
    assert_eq!(log.borrow().last(), Some(&SharedString::from("a")));
}

#[gpui::test]
fn clicking_a_segment_selects_it(cx: &mut TestAppContext) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || segments(log.clone())
    });
    // The first segment is selected already; clicking it changes nothing.
    cx.simulate_click(point(px(30.), px(30.)), Modifiers::none());
    cx.run_until_parked();
    assert!(log.borrow().is_empty());
}

// ---- The lens (FR9, FR10) ----------------------------------------------------

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn frames(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..(ms / 16) {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        settle(cx);
    }
}

fn three(value: Rc<RefCell<SharedString>>) -> AnyElement {
    SegmentedControl::new("s", "View", value.borrow().clone())
        .item(SegmentedControlItem::new("a", "Alpha"))
        .item(SegmentedControlItem::new("b", "Beta"))
        .item(SegmentedControlItem::new("c", "Gamma wide"))
        .into_any_element()
}

fn lens_host(cx: &mut TestAppContext) -> (&mut VisualTestContext, Rc<RefCell<SharedString>>) {
    let value = Rc::new(RefCell::new(SharedString::from("a")));
    let cx = host(cx, {
        let value = value.clone();
        move || three(value.clone())
    });
    settle(cx);
    (cx, value)
}

fn left(cx: &mut VisualTestContext, selector: &'static str) -> Option<f32> {
    cx.debug_bounds(selector).map(|b| f32::from(b.left()))
}

fn width(cx: &mut VisualTestContext, selector: &'static str) -> f32 {
    f32::from(cx.debug_bounds(selector).unwrap().size.width)
}

fn touch(cx: &mut VisualTestContext) {
    use gpui::{TouchEvent, TouchId, TouchPhase};
    for phase in [TouchPhase::Started, TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: point(px(600.), px(400.)),
            ..Default::default()
        });
    }
    settle(cx);
}

#[gpui::test]
fn fr9_a_change_slides_one_surface_from_the_old_segment_to_the_new(cx: &mut TestAppContext) {
    let (cx, value) = lens_host(cx);
    let (a, c) = (left(cx, "s-a").unwrap(), left(cx, "s-c").unwrap());
    let (wa, wc) = (width(cx, "s-a"), width(cx, "s-c"));
    assert!(
        left(cx, "s-lens").is_none(),
        "at rest the segment paints its surface"
    );
    *value.borrow_mut() = "c".into();
    settle(cx);
    let start = left(cx, "s-lens").expect("the lens carries the surface");
    assert!(
        (start - a).abs() < 1.,
        "it starts on the old segment: {start} vs {a}"
    );
    assert!(
        (width(cx, "s-lens") - wa).abs() < 1.,
        "at the old segment's width"
    );
    frames(cx, 64);
    let mid = left(cx, "s-lens").unwrap();
    assert!(mid > a + 1. && mid < c - 1., "travelling: {mid}");
    let w = width(cx, "s-lens");
    assert!(
        w > wa.min(wc) - 0.5 && w < wa.max(wc) + 0.5,
        "its width follows: {w}"
    );
    frames(cx, 600);
    assert!(
        left(cx, "s-lens").is_none(),
        "settled: the new segment paints its surface"
    );
    assert!(cx.debug_bounds("s-leaving").is_none());
}

#[gpui::test]
fn fr9_a_change_during_a_slide_starts_from_where_the_lens_shows(cx: &mut TestAppContext) {
    let (cx, value) = lens_host(cx);
    *value.borrow_mut() = "c".into();
    settle(cx);
    frames(cx, 64);
    let shown = left(cx, "s-lens").unwrap();
    *value.borrow_mut() = "b".into();
    settle(cx);
    let from = left(cx, "s-lens").unwrap();
    assert!((from - shown).abs() < 3., "no jump: {from} vs {shown}");
}

#[gpui::test]
fn fr9_under_touch_a_change_settles_on_the_selection_spring(cx: &mut TestAppContext) {
    let (cx, value) = lens_host(cx);
    touch(cx);
    let c = left(cx, "s-c").unwrap();
    let wc = width(cx, "s-c");
    let target = c + wc / 2.;
    *value.borrow_mut() = "c".into();
    settle(cx);
    // The selection spring (response 0.4 s, damping 0.85) overshoots a little; the
    // theme's easing never does.
    let mut overshoot = 0f32;
    for _ in 0..40 {
        frames(cx, 16);
        let Some(l) = left(cx, "s-lens") else { break };
        overshoot = overshoot.max(l + width(cx, "s-lens") / 2. - target);
    }
    assert!(
        overshoot > 0.05,
        "the spring carries past its target: {overshoot}"
    );
    frames(cx, 800);
    assert!(left(cx, "s-lens").is_none());
}

#[gpui::test]
fn fr10_under_reduce_motion_a_finger_cross_fades_in_place(cx: &mut TestAppContext) {
    cx.update(|cx| rustryx_primitives::set_reduced_motion(true, cx));
    let (cx, value) = lens_host(cx);
    touch(cx);
    let (a, c) = (left(cx, "s-a").unwrap(), left(cx, "s-c").unwrap());
    *value.borrow_mut() = "c".into();
    settle(cx);
    frames(cx, 32);
    let lens = left(cx, "s-lens").expect("the new surface fades in");
    assert!(
        (lens - c).abs() < 0.5,
        "it fades in on the new segment, no travel: {lens}"
    );
    let leaving = left(cx, "s-leaving").expect("the old surface fades out");
    assert!((leaving - a).abs() < 0.5, "where it was: {leaving}");
    frames(cx, 600);
    assert!(left(cx, "s-lens").is_none() && left(cx, "s-leaving").is_none());
}

#[gpui::test]
fn fr10_under_reduce_motion_a_mouse_lands_at_once(cx: &mut TestAppContext) {
    cx.update(|cx| rustryx_primitives::set_reduced_motion(true, cx));
    let (cx, value) = lens_host(cx);
    *value.borrow_mut() = "c".into();
    settle(cx);
    assert!(left(cx, "s-lens").is_none(), "nothing animates");
    assert!(left(cx, "s-leaving").is_none());
}
