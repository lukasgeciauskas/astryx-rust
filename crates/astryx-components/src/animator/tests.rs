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
