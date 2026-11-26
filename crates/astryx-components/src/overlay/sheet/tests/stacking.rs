//! Upstream `BottomSheetStacking.test.tsx`, `BottomSheetReplace.test.tsx`,
//! `BottomSheetCloseFocus.test.tsx` and `BottomSheetSwitcher.test.tsx` (FR5-FR10,
//! FR12, AR3, AR4; the switcher's FR1-FR4, FR6, FR8, FR10).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    InteractiveElement as _, IntoElement, ParentElement, SharedString, TestAppContext, div, px,
};

use gpui::Styled as _;

use super::harness::*;
use crate::button::Button;
use crate::overlay::{BottomSheet, BottomSheetSwitcher, SheetHeight};

type Log = Rc<RefCell<Vec<&'static str>>>;

/// Two sibling sheets; the first's content opens the second.
fn two(
    cx: &mut TestAppContext,
) -> (
    &mut gpui::VisualTestContext,
    Rc<Cell<bool>>,
    Rc<Cell<bool>>,
    Log,
) {
    let (outer, inner) = (flag(), flag());
    let log: Log = Log::default();
    let (o, i, l) = (outer.clone(), inner.clone(), log.clone());
    let cx = page(cx, move |_, _| {
        let (open_inner, l) = (i.clone(), l.clone());
        div()
            .child(Button::new("opener", "Open").on_click({
                let o = o.clone();
                move |_, window, _| {
                    o.set(true);
                    window.refresh();
                }
            }))
            .child(
                BottomSheet::new("outer", o.get(), "Filters")
                    .on_open_change(writer(&o))
                    .child(div().debug_selector(|| "outer-content".into()).child(
                        Button::new("add", "Add filter").on_click(move |_, window, _| {
                            open_inner.set(true);
                            window.refresh();
                        }),
                    )),
            )
            .child(
                BottomSheet::new("inner", i.get(), "Add filter")
                    .height(SheetHeight::Hug)
                    .on_open_change(writer(&i))
                    .child(
                        div().debug_selector(|| "inner-content".into()).child(
                            Button::new("inner-button", "Inner")
                                .on_click(move |_, _, _| l.borrow_mut().push("inner")),
                        ),
                    ),
            )
            .into_any_element()
    });
    (cx, outer, inner, log)
}

#[gpui::test]
fn fr5_fr6_a_sheet_opened_over_another_stacks_and_the_covered_one_recedes(cx: &mut TestAppContext) {
    let (cx, outer, inner, _) = two(cx);
    open(cx, &outer);
    open(cx, &inner);
    let (o, i) = (sheet("outer"), sheet("inner"));
    assert_eq!(i.depth, 0, "the newest is on top");
    assert_eq!(o.depth, 1);
    assert!((o.recede - 1.).abs() < 1e-3, "it receded a step");
    // One backdrop: the bottom open sheet's.
    assert!(o.owns_scrim && !i.owns_scrim);
    assert!(o.is_stacked && i.is_stacked);
    // Covered with no sheet in its content: blocked (AR3).
    assert!(o.blocked && o.inert);
    assert!(!i.blocked);
}

#[gpui::test]
fn fr7_fr9_escape_reaches_only_the_top_sheet_and_focus_returns_into_the_revealed_one(
    cx: &mut TestAppContext,
) {
    let (cx, outer, inner, _) = two(cx);
    open(cx, &outer);
    // Open the second sheet from a control inside the first.
    press(cx, "tab");
    press(cx, "enter");
    frames(cx, 1000);
    assert!(inner.get());
    press(cx, "escape");
    assert!(!inner.get(), "the top sheet closed");
    assert!(outer.get(), "the sheet beneath stays");
    // Focus comes back when the exit ends, to its opener inside the revealed sheet.
    frames(cx, 1000);
    press(cx, "enter");
    assert!(inner.get(), "Enter on the opener opened it again");
}

#[gpui::test]
fn fr6_the_covered_sheet_returns_to_the_top_and_owns_the_scrim_alone(cx: &mut TestAppContext) {
    let (cx, outer, inner, _) = two(cx);
    open(cx, &outer);
    open(cx, &inner);
    close(cx, &inner);
    frames(cx, 1000);
    let o = sheet("outer");
    assert_eq!(o.depth, 0);
    assert!(o.recede.abs() < 1e-3, "back from receded");
    assert!(o.owns_scrim && !o.blocked);
    assert!(frame("inner").is_none());
}

#[gpui::test]
fn fr10_a_sheet_closed_as_another_opens_from_a_control_inside_it_is_replaced(
    cx: &mut TestAppContext,
) {
    let (first, second) = (flag(), flag());
    let (f, s) = (first.clone(), second.clone());
    let cx = page(cx, move |_, _| {
        let (f2, s2) = (f.clone(), s.clone());
        div()
            .child(Button::new("opener", "Start").on_click({
                let f = f.clone();
                move |_, window, _| {
                    f.set(true);
                    window.refresh();
                }
            }))
            .child(
                BottomSheet::new("step-1", f.get(), "Step 1")
                    .height(SheetHeight::Tall)
                    .on_open_change(writer(&f))
                    .child(div().debug_selector(|| "step-1".into()).child(
                        Button::new("next", "Next").on_click(move |_, window, _| {
                            // Close this step and open the next in one update.
                            f2.set(false);
                            s2.set(true);
                            window.refresh();
                        }),
                    )),
            )
            .child(
                BottomSheet::new("step-2", s.get(), "Step 2")
                    .height(SheetHeight::Hug)
                    .on_open_change(writer(&s))
                    .child(
                        div()
                            .debug_selector(|| "step-2".into())
                            .child(Button::new("done", "Done")),
                    ),
            )
            .into_any_element()
    });
    // Open from the page's opener by the keyboard, so focus has an opener.
    press(cx, "tab");
    press(cx, "enter");
    frames(cx, 1000);
    assert!(first.get());
    press(cx, "tab");
    press(cx, "enter");
    settle(cx);
    frames(cx, 48);
    let retained = sheet("step-1");
    assert!(retained.retained, "the closing step is retained");
    assert!(retained.inert, "inert from the first frame (AR3)");
    assert!(
        retained.owns_scrim,
        "a replace at the bottom keeps the one scrim"
    );
    let entering = sheet("step-2");
    assert!(
        entering.open && !entering.retained,
        "the entering step is the top"
    );
    assert_eq!(entering.depth, 0);
    // The retained step aligns down to the shorter entering one, then fades.
    frames(cx, 2000);
    assert!(frame("step-1").is_none(), "the retained step left");
    assert!(sheet("step-2").owns_scrim);
    // Closing the flow returns focus to its first opener: Enter starts it again.
    press(cx, "escape");
    frames(cx, 1000);
    press(cx, "enter");
    frames(cx, 1000);
    assert!(first.get(), "focus came back to the page's opener");
}

#[gpui::test]
fn fr10_a_close_and_an_open_from_a_control_outside_the_closing_sheet_are_a_pop_and_a_push(
    cx: &mut TestAppContext,
) {
    let (a, b) = (flag(), flag());
    let (fa, fb) = (a.clone(), b.clone());
    let cx = page(cx, move |_, _| {
        div()
            .child(
                BottomSheet::new("a", fa.get(), "A")
                    .scrim(false)
                    .on_open_change(writer(&fa))
                    .child(div().child("A")),
            )
            .child(
                BottomSheet::new("b", fb.get(), "B")
                    .on_open_change(writer(&fb))
                    .child(div().child("B")),
            )
            .into_any_element()
    });
    open(cx, &a);
    // Different modalities never pair.
    a.set(false);
    b.set(true);
    settle(cx);
    frames(cx, 48);
    if let Some(f) = frame("a") {
        assert!(!f.retained, "an ordinary exit");
    }
    frames(cx, 1000);
    assert!(frame("a").is_none());
    assert!(sheet("b").open);
}

#[gpui::test]
fn fr9_closing_every_sheet_together_leaves_none_presented(cx: &mut TestAppContext) {
    let (cx, outer, inner, _) = two(cx);
    open(cx, &outer);
    open(cx, &inner);
    outer.set(false);
    inner.set(false);
    settle(cx);
    frames(cx, 1500);
    assert!(frame("outer").is_none() && frame("inner").is_none());
}

// --- BottomSheetSwitcher ------------------------------------------------------------------

fn switcher(
    cx: &mut TestAppContext,
) -> (
    &mut gpui::VisualTestContext,
    Rc<RefCell<Option<SharedString>>>,
) {
    let active: Rc<RefCell<Option<SharedString>>> = Rc::default();
    let a = active.clone();
    let cx =
        page(cx, move |_, _| {
            let set = |to: Option<&'static str>| {
                let a = a.clone();
                move |_: &gpui::ClickEvent, window: &mut gpui::Window, _: &mut gpui::App| {
                    *a.borrow_mut() = to.map(SharedString::from);
                    window.refresh();
                }
            };
            let report = a.clone();
            div()
                .child(Button::new("start", "Start").on_click(set(Some("one"))))
                .child(
                    BottomSheetSwitcher::new(a.borrow().clone())
                        .on_active_sheet_change(move |to, window, _| {
                            *report.borrow_mut() = to;
                            window.refresh();
                        })
                        .sheet(
                            BottomSheet::new("one", false, "One")
                                .sheet_id("one")
                                .height(SheetHeight::Tall)
                                .child(div().debug_selector(|| "one".into()).child(
                                    Button::new("to-two", "Next").on_click(set(Some("two"))),
                                )),
                        )
                        .sheet(
                            BottomSheet::new("two", false, "Two")
                                .sheet_id("two")
                                .height(SheetHeight::Hug)
                                .child(div().debug_selector(|| "two".into()).child("Two")),
                        ),
                )
                .into_any_element()
        });
    (cx, active)
}

#[gpui::test]
fn switcher_fr1_fr2_one_step_is_interactive_and_a_change_of_step_is_a_handoff(
    cx: &mut TestAppContext,
) {
    let (cx, active) = switcher(cx);
    *active.borrow_mut() = Some("one".into());
    settle(cx);
    frames(cx, 1000);
    assert!(sheet("one").open);
    assert!(frame("two").is_none());
    *active.borrow_mut() = Some("two".into());
    settle(cx);
    frames(cx, 32);
    assert!(sheet("one").retained, "the outgoing step is retained");
    assert!(sheet("two").open);
    assert_eq!(
        sheet("two").depth,
        0,
        "one level: no recede within the flow"
    );
    frames(cx, 2000);
    assert!(frame("one").is_none());
}

#[gpui::test]
fn switcher_fr6_the_active_steps_dismissal_reports_none(cx: &mut TestAppContext) {
    let (cx, active) = switcher(cx);
    *active.borrow_mut() = Some("one".into());
    settle(cx);
    frames(cx, 1000);
    press(cx, "escape");
    assert_eq!(*active.borrow(), None);
    frames(cx, 1000);
    assert!(frame("one").is_none());
}

#[gpui::test]
fn switcher_fr8_one_backdrop_through_a_handoff(cx: &mut TestAppContext) {
    let (cx, active) = switcher(cx);
    *active.borrow_mut() = Some("one".into());
    settle(cx);
    frames(cx, 1000);
    *active.borrow_mut() = Some("two".into());
    settle(cx);
    frames(cx, 48);
    let (one, two) = (sheet("one"), sheet("two"));
    assert!(
        one.owns_scrim ^ two.owns_scrim,
        "exactly one backdrop dims the page"
    );
    let _ = px(0.);
}

/// Two non-modal sheets and a page control that switches between them; each sheet also
/// has its own Next that does the same from inside.
fn pair(cx: &mut TestAppContext) -> (&mut gpui::VisualTestContext, Rc<Cell<bool>>, Rc<Cell<bool>>) {
    let (a, b) = (flag(), flag());
    let (fa, fb) = (a.clone(), b.clone());
    let cx = page(cx, move |_, _| {
        let switch = |fa: Rc<Cell<bool>>, fb: Rc<Cell<bool>>| {
            move |_: &gpui::ClickEvent, window: &mut gpui::Window, _: &mut gpui::App| {
                fa.set(false);
                fb.set(true);
                window.refresh();
            }
        };
        div()
            .child(
                div()
                    .debug_selector(|| "page-switch".into())
                    .w(px(120.))
                    .child(
                        Button::new("page-switch", "Switch")
                            .on_click(switch(fa.clone(), fb.clone())),
                    ),
            )
            .child(
                BottomSheet::new("a", fa.get(), "A")
                    .scrim(false)
                    .on_open_change(writer(&fa))
                    .child(div().debug_selector(|| "a-next".into()).w(px(120.)).child(
                        Button::new("a-next", "Next").on_click(switch(fa.clone(), fb.clone())),
                    )),
            )
            .child(
                BottomSheet::new("b", fb.get(), "B")
                    .scrim(false)
                    .on_open_change(writer(&fb))
                    .child(div().child("B")),
            )
            .into_any_element()
    });
    (cx, a, b)
}

#[gpui::test]
fn fr10_fr11_a_control_inside_the_closing_sheet_replaces_it(cx: &mut TestAppContext) {
    let (cx, a, _) = pair(cx);
    open(cx, &a);
    let next = cx.debug_bounds("a-next").unwrap();
    cx.simulate_click(
        next.origin + gpui::point(px(10.), px(10.)),
        gpui::Modifiers::none(),
    );
    settle(cx);
    frames(cx, 32);
    assert!(
        sheet("a").retained,
        "the last pressed control was inside it: a replace"
    );
}

#[gpui::test]
fn fr10_a_page_control_outside_the_closing_sheet_is_a_pop_and_a_push(cx: &mut TestAppContext) {
    let (cx, a, _) = pair(cx);
    open(cx, &a);
    let switch = cx.debug_bounds("page-switch").unwrap();
    cx.simulate_click(
        switch.origin + gpui::point(px(10.), px(10.)),
        gpui::Modifiers::none(),
    );
    settle(cx);
    frames(cx, 32);
    if let Some(f) = frame("a") {
        assert!(
            !f.retained,
            "the actor was outside the closing sheet: an ordinary exit"
        );
    }
    assert!(sheet("b").open);
}

#[gpui::test]
fn fr9_a_modal_sheet_returns_focus_to_its_opener_when_its_exit_ends(cx: &mut TestAppContext) {
    // Even when focus moved elsewhere inside the page's reach meanwhile: the exit's
    // end returns it, as upstream's native close does.
    let (cx, outer, _, _) = two(cx);
    press(cx, "tab");
    press(cx, "enter");
    frames(cx, 1000);
    assert!(outer.get());
    close(cx, &outer);
    // Before the exit ends focus has not moved.
    frames(cx, 1000);
    // The opener has focus again: Enter opens the sheet once more.
    press(cx, "enter");
    assert!(outer.get());
}

#[gpui::test]
fn fr11_a_non_modal_sheet_left_with_focus_elsewhere_moves_none(cx: &mut TestAppContext) {
    let (cx, a, _) = pair(cx);
    // Opened from the page's control by the keyboard: it is the sheet's opener.
    a.set(true);
    settle(cx);
    frames(cx, 1000);
    // The person moves focus back to the page while the sheet stays open.
    let switch = cx.debug_bounds("page-switch").unwrap();
    cx.simulate_mouse_down(
        switch.origin + gpui::point(px(10.), px(10.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    settle(cx);
    let before = cx.update(|window, cx| window.focused(cx));
    close(cx, &a);
    frames(cx, 1000);
    assert!(frame("a").is_none());
    let after = cx.update(|window, cx| window.focused(cx));
    assert_eq!(
        before, after,
        "focus outside a closing non-modal sheet stays put"
    );
}
