//! Animator and entry animation tests.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::motion::AnimatorValues;
use gpui::{
    InteractiveElement as _, IntoElement, ParentElement, Styled, TestAppContext, VisualTestContext,
    div, px,
};

use super::*;
use crate::segmented::tests::host;
use gpui::StatefulInteractiveElement as _;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn frames(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..(ms / 16) {
        cx.executor().advance_clock(Duration::from_millis(16));
        settle(cx);
    }
}

type Slot = Rc<RefCell<Option<AnimatorHandle>>>;

fn page(cx: &mut TestAppContext, slot: Slot) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        let slot = slot.clone();
        div()
            .p(px(20.))
            .child(gpui::canvas(|_, _, _| {}, |_, _, _, _| {}).size_0())
            .child(Probe { slot })
            .into_any_element()
    });
    settle(cx);
    cx
}

#[derive(gpui::IntoElement)]
struct Probe {
    slot: Slot,
}

impl gpui::RenderOnce for Probe {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let animator = use_animator("box", window, cx);
        *self.slot.borrow_mut() = Some(animator.clone());
        animator.apply(
            div().debug_selector(|| "box".into()).size(px(40.)),
            window,
            cx,
        )
    }
}

#[gpui::test]
fn animate_springs_the_element_and_finishes(cx: &mut TestAppContext) {
    let slot: Slot = Rc::default();
    let cx = page(cx, slot.clone());
    let handle = slot.borrow().clone().unwrap();
    let call =
        cx.update(|_, cx| handle.animate(AnimatorValues::new().x(200.), Default::default(), cx));
    frames(cx, 160);
    let mid = cx.update(|_, cx| handle.get(cx).x);
    assert!(mid > 20. && mid < 200., "moving: {mid}");
    assert!(!call.is_done());
    frames(cx, 2000);
    let end = cx.update(|_, cx| handle.get(cx).x);
    assert!((end - 200.).abs() < 0.5, "arrived: {end}");
    assert!(call.is_done());
}

#[gpui::test]
fn set_writes_at_once_and_a_later_call_takes_over(cx: &mut TestAppContext) {
    let slot: Slot = Rc::default();
    let cx = page(cx, slot.clone());
    let handle = slot.borrow().clone().unwrap();
    let first =
        cx.update(|_, cx| handle.animate(AnimatorValues::new().x(300.), Default::default(), cx));
    frames(cx, 100);
    cx.update(|_, cx| handle.set(AnimatorValues::new().x(50.), cx));
    settle(cx);
    assert!(first.is_done(), "taken over");
    assert_eq!(cx.update(|_, cx| handle.get(cx).x), 50.);
}

#[gpui::test]
fn reduced_motion_lands_at_once_with_a_mouse(cx: &mut TestAppContext) {
    cx.update(|cx| astryx_primitives::set_reduced_motion(true, cx));
    let slot: Slot = Rc::default();
    let cx = page(cx, slot.clone());
    let handle = slot.borrow().clone().unwrap();
    cx.update(|_, cx| handle.animate(AnimatorValues::new().x(120.), Default::default(), cx));
    frames(cx, 32);
    assert_eq!(cx.update(|_, cx| handle.get(cx).x), 120.);
}

#[derive(gpui::IntoElement)]
struct Rows {
    count: Rc<RefCell<usize>>,
}

impl gpui::RenderOnce for Rows {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let mut list = div().flex().flex_col();
        for i in 0..*self.count.borrow() {
            let entry = use_entry_animation(
                gpui::ElementId::Name(format!("row-{i}").into()),
                EntryAnimationPreset::FadeIn,
                window,
                cx,
            );
            let animating = entry.is_animating();
            list = list.child(
                div()
                    .debug_selector(move || format!("row-{i}-{animating}"))
                    .h(px(20.)),
            );
        }
        list
    }
}

#[gpui::test]
fn only_rows_added_after_the_first_paint_enter(cx: &mut TestAppContext) {
    let count = Rc::new(RefCell::new(1));
    let rows = count.clone();
    let cx = host(cx, move || {
        Rows {
            count: rows.clone(),
        }
        .into_any_element()
    });
    settle(cx);
    assert!(
        cx.debug_bounds("row-0-false").is_some(),
        "drawn on the first paint: no entrance"
    );
    *count.borrow_mut() = 2;
    settle(cx);
    assert!(
        cx.debug_bounds("row-1-true").is_some(),
        "added later: it enters"
    );
    frames(cx, 400);
    assert!(
        cx.debug_bounds("row-1-false").is_some(),
        "and comes to rest"
    );
}

#[derive(gpui::IntoElement)]
struct Turned {
    slot: Slot,
    clicks: Rc<RefCell<usize>>,
}

impl gpui::RenderOnce for Turned {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let animator = use_animator("turned", window, cx);
        *self.slot.borrow_mut() = Some(animator.clone());
        let clicks = self.clicks.clone();
        animator.apply(
            div()
                .id("turned")
                .debug_selector(|| "turned".into())
                .w(px(80.))
                .h(px(20.))
                .on_click(move |_, _, _| *clicks.borrow_mut() += 1),
            window,
            cx,
        )
    }
}

#[gpui::test]
fn rotate_turns_the_element_and_its_hit_area(cx: &mut TestAppContext) {
    let slot: Slot = Rc::default();
    let clicks = Rc::new(RefCell::new(0usize));
    let (s, c) = (slot.clone(), clicks.clone());
    let cx = host(cx, move || {
        div()
            .p(px(40.))
            .child(Turned {
                slot: s.clone(),
                clicks: c.clone(),
            })
            .into_any_element()
    });
    settle(cx);
    let handle = slot.borrow().clone().unwrap();
    cx.update(|_, cx| handle.set(AnimatorValues::new().rotate(90.), cx));
    settle(cx);
    assert_eq!(cx.update(|_, cx| handle.get(cx).rotate), 90.);
    // Laid out 80 by 20; a quarter turn about its center stands it 20 by 80.
    let laid = cx.debug_bounds("turned").expect("drawn");
    let center = laid.center();
    let click = |cx: &mut VisualTestContext, x: f32, y: f32| {
        cx.simulate_click(gpui::point(px(x), px(y)), gpui::Modifiers::default());
        settle(cx);
    };
    // Inside the turned box, outside the laid-out one.
    click(cx, f32::from(center.x), f32::from(center.y) + 30.);
    assert_eq!(
        *clicks.borrow(),
        1,
        "the press lands on the turned element (fork patch R3)"
    );
    // Inside the laid-out box, outside the turned one.
    click(cx, f32::from(center.x) + 34., f32::from(center.y));
    assert_eq!(*clicks.borrow(), 1, "where it was laid out, nothing is hit");
}

/// How long a call to `to` takes to arrive, in 16 ms frames.
fn arrival_ms(cx: &mut VisualTestContext, handle: &AnimatorHandle, to: f64) -> u64 {
    let call =
        cx.update(|_, cx| handle.animate(AnimatorValues::new().x(to), Default::default(), cx));
    let mut ms = 0;
    while !call.is_done() && ms < 10_000 {
        frames(cx, 16);
        ms += 16;
    }
    ms
}

// Upstream: the animator reads `--_astryx-motion-tempo` where the element is, and a
// theme that sets `tempo` writes it (spec:AST-108 FR5). At 1.5 the same spring keeps
// its shape and takes 1.5 times as long.
#[gpui::test]
fn a_themes_motion_tempo_of_one_and_a_half_makes_a_spring_take_half_as_long_again(
    cx: &mut TestAppContext,
) {
    use astryx_theme::authoring::define::{DefineThemeInput, define_theme};
    use astryx_theme::authoring::theme_motion::ThemeMotionConfig;

    let slot: Slot = Rc::default();
    let cx = page(cx, slot.clone());
    let handle = slot.borrow().clone().unwrap();
    let normal = arrival_ms(cx, &handle, 200.);

    let slow = define_theme(&DefineThemeInput {
        motion: Some(ThemeMotionConfig {
            tempo: Some(1.5),
            ..ThemeMotionConfig::default()
        }),
        ..DefineThemeInput::new("slow-motion")
    })
    .unwrap();
    cx.update(|_, cx| astryx_theme::set_theme(slow.to_theme(), cx));
    settle(cx);
    assert_eq!(
        cx.update(|window, cx| astryx_theme::current(window, cx).motion_tempo()),
        1.5
    );
    let handle = slot.borrow().clone().unwrap();
    let slower = arrival_ms(cx, &handle, 0.);
    let ratio = slower as f64 / normal as f64;
    assert!(
        (ratio - 1.5).abs() < 0.1,
        "{normal} ms at tempo 1, {slower} ms at 1.5"
    );
}
