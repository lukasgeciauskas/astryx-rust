//! Upstream `BottomSheet.test.tsx`, `BottomSheetPanel.test.tsx`,
//! `BottomSheetKeyboard.test.tsx` and `BottomSheetDragExit.test.tsx`: the sheet in a
//! window, named after the spec's FR and AR items.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    InteractiveElement as _, IntoElement, Modifiers, ParentElement, Styled, TestAppContext,
    VisualTestContext, div, point, px,
};

use super::harness::*;
use crate::button::Button;
use crate::overlay::{BottomSheet, DialogPurpose, SheetHeight, SnapPoint};

fn basic(
    open: Rc<Cell<bool>>,
    configure: fn(BottomSheet) -> BottomSheet,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyElement {
    move |_, _| {
        div()
            .child(
                div()
                    .debug_selector(|| "page".into())
                    .h(px(40.))
                    .w(px(200.))
                    .child(Button::new("opener", "Open").on_click({
                        let open = open.clone();
                        move |_, window, _| {
                            open.set(true);
                            window.refresh();
                        }
                    })),
            )
            .child(configure(
                BottomSheet::new("s", open.get(), "Filters")
                    .on_open_change(writer(&open))
                    .child(
                        div()
                            .debug_selector(|| "content".into())
                            .child(Button::new("first", "First"))
                            .child(Button::new("last", "Last")),
                    ),
            ))
            .into_any_element()
    }
}

fn opened(
    cx: &mut TestAppContext,
    configure: fn(BottomSheet) -> BottomSheet,
) -> (&mut VisualTestContext, Rc<Cell<bool>>) {
    let flag = flag();
    let cx = page(cx, basic(flag.clone(), configure));
    open(cx, &flag);
    (cx, flag)
}

#[gpui::test]
fn fr1_a_presented_sheet_has_a_panel_a_scrolling_body_a_handle_and_a_scrim(
    cx: &mut TestAppContext,
) {
    let (cx, _) = opened(cx, |s| s);
    assert!(cx.debug_bounds("content").is_some());
    let s = sheet("s");
    assert!(s.open);
    assert_eq!(s.scrim, 1., "scrim-backed");
    // A press on the scrim closes an Info sheet.
    cx.simulate_click(point(px(5.), px(5.)), Modifiers::none());
    frames(cx, 1000);
    assert!(frame("s").is_none(), "pressed on the scrim, it left");
}

#[gpui::test]
fn fr4_the_content_pads_by_the_inset_and_starts_below_the_grab_bar(cx: &mut TestAppContext) {
    let (cx, _) = opened(cx, |s| s);
    let content = cx.debug_bounds("content").unwrap();
    let viewport = cx.update(|window, _| window.viewport_size());
    let s = sheet("s");
    // The panel's top edge is the window's height less its visible height.
    let top = viewport.height - px(s.full - s.reserve);
    // max(spacing-4, spacing-6): the block start clears the 24 px band.
    assert!(
        content.top() - top >= px(24.) - px(0.5),
        "{:?} {:?}",
        content.top(),
        top
    );
    // The inline inset is spacing-4.
    let panel_left = (viewport.width - content.size.width - px(32.)) / 2.;
    assert!((content.left() - panel_left - px(16.)).abs() < px(1.));
}

#[gpui::test]
fn fr4_padding_zero_still_clears_the_grab_bar(cx: &mut TestAppContext) {
    let (cx, _) = opened(cx, |s| s.padding(crate::layout::SpacingStep::S0));
    let content = cx.debug_bounds("content").unwrap();
    let viewport = cx.update(|window, _| window.viewport_size());
    let s = sheet("s");
    let top = viewport.height - px(s.full - s.reserve);
    assert!(content.top() - top >= px(23.5));
}

#[gpui::test]
fn fr7_escape_and_the_scrim_follow_purpose(cx: &mut TestAppContext) {
    // Info: Escape closes.
    let (cx, flag) = opened(cx, |s| s);
    press(cx, "escape");
    assert!(!flag.get());
}

#[gpui::test]
fn fr7_a_form_sheet_closes_on_escape_but_not_on_the_scrim(cx: &mut TestAppContext) {
    let (cx, flag) = opened(cx, |s| s.purpose(DialogPurpose::Form));
    cx.simulate_click(point(px(5.), px(5.)), Modifiers::none());
    settle(cx);
    assert!(flag.get(), "the scrim leaves a form open");
    press(cx, "escape");
    assert!(!flag.get());
}

#[gpui::test]
fn fr7_a_required_sheet_blocks_escape_the_scrim_and_the_swipe(cx: &mut TestAppContext) {
    let (cx, flag) = opened(cx, |s| s.purpose(DialogPurpose::Required));
    press(cx, "escape");
    cx.simulate_click(point(px(5.), px(5.)), Modifiers::none());
    settle(cx);
    let handle = handle_of(cx, "content");
    mouse_drag(cx, handle, 600., 4, 16);
    frames(cx, 1000);
    assert!(flag.get());
    // It settles back on its shortest detent instead.
    assert!(sheet("s").offset.abs() < 1.);
}

#[gpui::test]
fn fr16_tab_stays_inside_a_modal_sheet(cx: &mut TestAppContext) {
    let (cx, _) = opened(cx, |s| s);
    for _ in 0..6 {
        press(cx, "tab");
    }
    let inside = cx.update(|window, cx| {
        let focused = window.focused(cx);
        focused.is_some()
    });
    assert!(inside);
    assert!(frame("s").is_some());
}

#[gpui::test]
fn fr19_a_mouse_pull_on_the_handle_closes_an_info_sheet(cx: &mut TestAppContext) {
    let (cx, flag) = opened(cx, |s| s);
    let handle = handle_of(cx, "content");
    mouse_drag(cx, handle, 600., 4, 16);
    assert!(!flag.get(), "pulled away, it closed");
    // A mouse's close plays the stylesheet's exit, not the close spring.
    assert!(sheet("s").motion.starts_with("Exiting") || sheet("s").motion == "Open");
    frames(cx, 1000);
    assert!(frame("s").is_none());
}

#[gpui::test]
fn fr19_a_finger_flick_on_the_handle_leaves_on_the_close_spring_from_the_release(
    cx: &mut TestAppContext,
) {
    let (cx, flag) = opened(cx, |s| s);
    let handle = handle_of(cx, "content");
    finger(cx, handle, 80., 4, 8, true);
    assert!(!flag.get(), "the flick closed it");
    frames(cx, 32);
    let s = sheet("s");
    assert!(s.motion.starts_with("DragExit"), "{}", s.motion);
    assert!(
        s.offset > 80.,
        "it carries on from the release: {}",
        s.offset
    );
    // The scrim fades with the travel.
    assert!(s.scrim < 1.);
    frames(cx, 2000);
    assert!(frame("s").is_none());
}

#[gpui::test]
fn fr19_a_slow_handle_drag_settles_on_the_nearest_detent(cx: &mut TestAppContext) {
    let (cx, flag) = opened(cx, |s| {
        s.height(SheetHeight::Length(px(600.)))
            .snap_points([SnapPoint::Length(px(300.))])
    });
    let handle = handle_of(cx, "content");
    finger(cx, handle, 280., 20, 50, true);
    frames(cx, 1500);
    assert!(flag.get());
    let s = sheet("s");
    assert!(
        (s.offset - 300.).abs() < 1.,
        "rests at the 300 px detent: {}",
        s.offset
    );
    // A working-height detent lays its content out at its height.
    assert!((s.layout - 300.).abs() < 1.);
    assert!(s.height.is_some());
}

#[gpui::test]
fn fr18_a_finger_pull_up_past_the_top_stretches_and_springs_back(cx: &mut TestAppContext) {
    let (cx, flag) = opened(cx, |s| s);
    let handle = handle_of(cx, "content");
    finger(cx, handle, -150., 6, 40, false);
    let stretched = sheet("s").offset;
    assert!(sheet("s").dragging, "the finger holds the sheet");
    assert!(stretched < -10., "stretched: {stretched}");
    assert!(stretched > -150., "on the rubber band: {stretched}");
    wait(cx, 300);
    touch(
        cx,
        1,
        gpui::TouchPhase::Ended,
        point(handle.x, handle.y - px(150.)),
    );
    frames(cx, 1500);
    assert!(flag.get());
    assert!(sheet("s").offset.abs() < 0.5);
}

#[gpui::test]
fn fr15_under_reduce_motion_the_entrance_has_no_travel(cx: &mut TestAppContext) {
    cx.update(|cx| rustryx_primitives::set_reduced_motion(true, cx));
    let flag = flag();
    let cx = page(cx, basic(flag.clone(), |s| s));
    flag.set(true);
    settle(cx);
    let s = sheet("s");
    assert!(s.translation.abs() < 0.5, "no transform: {}", s.translation);
}

#[gpui::test]
fn fr15_under_reduce_motion_a_touch_swipe_dismissal_fades_out_in_place(cx: &mut TestAppContext) {
    cx.update(|cx| rustryx_primitives::set_reduced_motion(true, cx));
    let flag = flag();
    let cx = page(cx, basic(flag.clone(), |s| s));
    open(cx, &flag);
    let handle = handle_of(cx, "content");
    finger(cx, handle, 80., 4, 8, true);
    assert!(!flag.get());
    frames(cx, 48);
    if let Some(s) = frame("s") {
        assert!(s.opacity < 1., "a cross-fade: {}", s.opacity);
        assert!(!s.motion.starts_with("DragExit"), "no close spring");
    }
    frames(cx, 400);
    assert!(frame("s").is_none(), "within 200 ms");
}

#[gpui::test]
fn fr8_a_sheet_in_another_sheets_content_is_presented_only_while_its_host_is_open(
    cx: &mut TestAppContext,
) {
    let outer = flag();
    let inner = Rc::new(Cell::new(true));
    let (o, i) = (outer.clone(), inner.clone());
    let cx = page(cx, move |_, _| {
        BottomSheet::new("outer", o.get(), "Outer")
            .on_open_change(writer(&o))
            .child(
                BottomSheet::new("inner", i.get(), "Inner")
                    .on_open_change(writer(&i))
                    .child(div().debug_selector(|| "inner".into()).child("Inner")),
            )
            .into_any_element()
    });
    settle(cx);
    assert!(frame("inner").is_none(), "its host is closed");
    open(cx, &outer);
    assert!(frame("inner").is_some(), "presented with its host, on top");
    assert_eq!(sheet("inner").depth, 0);
    // The host closes from code: the inner sheet leaves with it, its state untouched.
    close(cx, &outer);
    frames(cx, 1000);
    assert!(frame("inner").is_none());
    assert!(inner.get(), "its own open state is never written");
    open(cx, &outer);
    assert!(frame("inner").is_some(), "back on top with its host");
}

#[gpui::test]
fn fr11_a_sheet_without_a_scrim_leaves_the_page_interactive(cx: &mut TestAppContext) {
    let clicks = Rc::new(Cell::new(0));
    let flag = flag();
    let (f, c) = (flag.clone(), clicks.clone());
    let cx = page(cx, move |_, _| {
        div()
            .child(div().debug_selector(|| "page-button".into()).child(
                Button::new("page-button", "Page").on_click({
                    let c = c.clone();
                    move |_, _, _| c.set(c.get() + 1)
                }),
            ))
            .child(
                BottomSheet::new("s", f.get(), "Filters")
                    .scrim(false)
                    .on_open_change(writer(&f))
                    .child(div().debug_selector(|| "content".into()).child("Filters")),
            )
            .into_any_element()
    });
    open(cx, &flag);
    let page_button = cx.debug_bounds("page-button").unwrap();
    // The button sits at the start of its line.
    cx.simulate_click(
        page_button.origin + point(px(12.), px(12.)),
        Modifiers::none(),
    );
    settle(cx);
    assert_eq!(clicks.get(), 1, "the page took the press");
    assert!(flag.get(), "and the sheet stayed open");
}

#[gpui::test]
fn fr25_fr26_the_panel_reads_the_surface_and_names_itself_to_a_layout(cx: &mut TestAppContext) {
    let flag = flag();
    let f = flag.clone();
    let cx = page(cx, move |_, _| {
        BottomSheet::new("s", f.get(), "Filters")
            .on_open_change(writer(&f))
            .child(
                crate::layout::Layout::new().content(
                    crate::layout::LayoutContent::new("body")
                        .child(div().debug_selector(|| "content".into()).child("x")),
                ),
            )
            .into_any_element()
    });
    open(cx, &flag);
    assert!(cx.debug_bounds("content").is_some());
}
