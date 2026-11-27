//! Layout region tests, numbered after `Layout.spec.md` and its regions' specs.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Pixels,
    Render, StatefulInteractiveElement as _, Styled, TestAppContext, VisualTestContext, Window,
    div, px,
};

use super::*;
use crate::segmented::tests::press;

type Build = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Host(Build);

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(800.))
            .h(px(600.))
            .flex()
            .flex_col()
            .child((self.0)(window, cx))
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::storage::use_memory_storage(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
    cx
}

fn mark(name: &'static str) -> gpui::Div {
    div()
        .debug_selector(move || name.to_string())
        .h(px(10.))
        .w_full()
}

fn at(cx: &mut VisualTestContext, name: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(name).expect(name)
}

fn full(header_divider: Option<bool>) -> impl Fn(&mut Window, &mut App) -> AnyElement {
    move |_, _| {
        let mut header = LayoutHeader::new().child(mark("h"));
        if let Some(d) = header_divider {
            header = header.has_divider(d);
        }
        Layout::new()
            .header(header)
            .start(LayoutPanel::new("start").width(px(150.)).child(mark("s")))
            .content(LayoutContent::new("main").child(mark("c")))
            .end(
                LayoutPanel::new("end")
                    .width(px(150.))
                    .has_divider(true)
                    .child(mark("e")),
            )
            .footer(LayoutFooter::new().has_divider(true).child(mark("f")))
            .into_any_element()
    }
}

#[gpui::test]
fn layout_fr2_regions_inset_by_the_edge_and_inner_insets(cx: &mut TestAppContext) {
    let cx = host(cx, full(Some(true)));
    let h = at(cx, "h");
    assert_eq!(
        (h.left(), h.top()),
        (px(16.), px(16.)),
        "the header's outer edges"
    );
    let s = at(cx, "s");
    assert_eq!(s.left(), px(16.), "the start panel's outer edge");
    let c = at(cx, "c");
    // The start panel has no divider: it overlaps the content by the inner inset,
    // so the two share one 16 px gap.
    assert_eq!(c.left(), px(150.), "{c:?}");
    assert_eq!(
        c.top(),
        h.bottom() + px(16.) + px(1.) + px(16.),
        "below the divider"
    );
    let e = at(cx, "e");
    assert_eq!(e.right(), px(800.) - px(16.));
}

#[gpui::test]
fn header_fr3_a_header_without_a_divider_collapses_the_spacing_below(cx: &mut TestAppContext) {
    let cx = host(cx, full(Some(false)));
    let h = at(cx, "h");
    let c = at(cx, "c");
    assert_eq!(
        c.top(),
        h.bottom() + px(16.),
        "only the header's own block-end inset"
    );
}

#[gpui::test]
fn layout_fr3_padding_zero_is_full_bleed(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Layout::new()
            .padding(SpacingStep::S0)
            .header(LayoutHeader::new().child(mark("h")))
            .content(LayoutContent::new("main").child(mark("c")))
            .into_any_element()
    });
    assert_eq!(at(cx, "h").left(), px(0.));
    assert_eq!(at(cx, "c").left(), px(0.));
}

#[gpui::test]
fn layout_fr5_the_divider_default_applies_and_an_explicit_value_wins(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Layout::new()
            .default_has_dividers(true)
            .header(LayoutHeader::new().child(mark("h")))
            .content(LayoutContent::new("main").child(mark("c")))
            .footer(LayoutFooter::new().has_divider(false).child(mark("f")))
            .into_any_element()
    });
    let (header, c) = (at(cx, "layout-header"), at(cx, "c"));
    assert_eq!(
        c.top(),
        header.bottom() + px(16.),
        "the default divider keeps the inner inset"
    );
    assert_eq!(
        at(cx, "h").bottom() + px(16.) + px(1.),
        header.bottom(),
        "and draws the line"
    );
    let (footer, f) = (at(cx, "layout-footer"), at(cx, "f"));
    assert_eq!(
        f.top(),
        footer.top() + px(16.),
        "an explicit false draws no line"
    );
}

#[gpui::test]
fn layout_fr3_a_nested_layout_takes_the_card_padding_not_its_parents(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        crate::containers::Card::new()
            .padding(SpacingStep::S6)
            .h(px(300.))
            .child(
                Layout::new().padding(SpacingStep::S2).content(
                    LayoutContent::new("outer").child(
                        Layout::new()
                            .height(LayoutHeight::Auto)
                            .content(LayoutContent::new("inner").child(mark("n"))),
                    ),
                ),
            )
            .into_any_element()
    });
    let n = at(cx, "n");
    // The outer Layout escapes the card's 24 px and insets by its own 8 px; the
    // nested one escapes that and insets by the card's 24 px.
    assert_eq!(n.left(), px(24.), "{n:?}");
}

#[gpui::test]
fn panel_fr1_a_resizable_size_wins_over_width(cx: &mut TestAppContext) {
    let cx = host(cx, |window, cx| {
        let region = crate::resizable::use_resizable(
            "nav",
            crate::resizable::UseResizable::new().default_size(240.),
            window,
            cx,
        );
        Layout::new()
            .padding(SpacingStep::S0)
            .start(
                LayoutPanel::new("start")
                    .width(px(100.))
                    .resizable(&region)
                    .has_divider(true)
                    .child(mark("s")),
            )
            .content(LayoutContent::new("main").child(mark("c")))
            .into_any_element()
    });
    // 240 less the inner inset on its content-facing edge and the divider.
    assert_eq!(at(cx, "s").size.width, px(240.) - px(16.) - px(1.));
}

#[gpui::test]
fn panel_ar2_focus_inside_a_collapsing_panel_moves_to_its_handle(cx: &mut TestAppContext) {
    let cx = host(cx, |window, cx| {
        let region = crate::resizable::use_resizable(
            "nav",
            crate::resizable::UseResizable::new()
                .default_size(200.)
                .collapsible(true),
            window,
            cx,
        );
        let collapse = region.clone();
        Layout::new()
            .start(
                div()
                    .flex()
                    .h_full()
                    .child(
                        LayoutPanel::new("start").resizable(&region).child(
                            crate::button::Button::new("inside", "Collapse")
                                .on_click(move |_, window, cx| collapse.collapse(window, cx)),
                        ),
                    )
                    .child(crate::resizable::ResizeHandle::new("nav-handle").resizable(&region)),
            )
            .content(LayoutContent::new("main").child(mark("c")))
            .into_any_element()
    });
    press(cx, "tab");
    press(cx, "enter");
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    // Enter on the handle now expands the panel again: focus is on the handle.
    press(cx, "enter");
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    assert!(cx.debug_bounds("nav-handle").is_some());
    assert!(
        at(cx, "c").left() > px(100.),
        "the panel opened again from its handle"
    );
}

#[gpui::test]
fn content_ar3_a_scrolling_region_is_a_tab_stop_only_while_it_scrolls(cx: &mut TestAppContext) {
    let cx = host(cx, |_, _| {
        Layout::new()
            .content(LayoutContent::new("main").children(
                (0..80).map(|i| div().h(px(20.)).debug_selector(move || format!("row-{i}"))),
            ))
            .into_any_element()
    });
    let top = at(cx, "row-0").top();
    press(cx, "tab");
    press(cx, "pagedown");
    assert!(at(cx, "row-0").top() < top, "the focused region scrolled");
}

// ---- The pane slide (LayoutPanel FR7, AR2; Resizable FR8) -------------------------

fn frames(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..(ms / 16) {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        for _ in 0..2 {
            cx.run_until_parked();
            cx.update(|window, _| window.refresh());
        }
    }
    cx.run_until_parked();
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
}

/// A Layout with a resizable start (or end) panel and its handle beside it.
fn sliding(end: bool, max: f32) -> impl Fn(&mut Window, &mut App) -> AnyElement {
    move |window, cx| {
        let region = crate::resizable::use_resizable(
            "nav",
            crate::resizable::UseResizable::new()
                .default_size(200.)
                .min_size(100.)
                .max_size(max)
                .collapsible(true),
            window,
            cx,
        );
        let panel = LayoutPanel::new("panel")
            .resizable(&region)
            .has_divider(true)
            .child(mark("s"));
        let handle = crate::resizable::ResizeHandle::new("nav-handle")
            .resizable(&region)
            .reversed(end);
        let side = if end {
            div().flex().h_full().child(handle).child(panel)
        } else {
            div().flex().h_full().child(panel).child(handle)
        };
        let layout = Layout::new()
            .padding(SpacingStep::S0)
            .content(LayoutContent::new("main").child(mark("c")));
        let layout = if end {
            layout.end(side)
        } else {
            layout.start(side)
        };
        layout.into_any_element()
    }
}

#[gpui::test]
fn panel_fr7_a_toggle_slides_the_panel_shut_by_its_content_facing_edge(cx: &mut TestAppContext) {
    let cx = host(cx, sliding(false, 400.));
    let (c0, s0) = (at(cx, "c").left(), at(cx, "s").left());
    press(cx, "tab");
    press(cx, "enter");
    settle(cx);
    let start = at(cx, "c").left();
    assert!(
        (start - c0).abs() < px(2.),
        "it starts where it was: {start:?}"
    );
    frames(cx, 96);
    let mid = at(cx, "c").left();
    assert!(
        mid < c0 - px(1.) && mid > px(20.),
        "the content follows the edge: {mid:?}"
    );
    assert_eq!(
        at(cx, "s").left(),
        s0,
        "the panel's content stays put under the clip"
    );
    frames(cx, 600);
    let shut = at(cx, "c").left();
    assert!(shut < px(40.), "shut: {shut:?}");
}

#[gpui::test]
fn panel_fr7_enter_slides_the_panel_open_again(cx: &mut TestAppContext) {
    let cx = host(cx, sliding(false, 400.));
    let c0 = at(cx, "c").left();
    press(cx, "tab");
    press(cx, "enter");
    frames(cx, 700);
    let shut = at(cx, "c").left();
    press(cx, "enter");
    settle(cx);
    let start = at(cx, "c").left();
    assert!(
        (start - shut).abs() < px(2.),
        "it starts shut: {start:?} vs {shut:?}"
    );
    frames(cx, 96);
    let mid = at(cx, "c").left();
    assert!(mid > shut + px(1.) && mid < c0 - px(1.), "opening: {mid:?}");
    frames(cx, 600);
    assert_eq!(at(cx, "c").left(), c0);
}

#[gpui::test]
fn panel_fr7_an_end_panel_slides_by_its_start_edge(cx: &mut TestAppContext) {
    let cx = host(cx, sliding(true, 400.));
    let e0 = at(cx, "s").left();
    press(cx, "tab");
    press(cx, "enter");
    frames(cx, 96);
    // The panel's content stays at its outer (end) edge while its start edge moves.
    assert_eq!(at(cx, "s").left(), e0, "its content stays put");
    let handle = at(cx, "nav-handle").left();
    assert!(
        handle > e0,
        "the edge facing the handle moves toward the end: {handle:?}"
    );
    frames(cx, 700);
    assert!(
        cx.debug_bounds("s")
            .is_none_or(|s| s.size.width == px(0.) || s.left() >= px(780.))
    );
}

#[gpui::test]
fn panel_fr7_a_controlled_change_slides(cx: &mut TestAppContext) {
    let collapsed = Rc::new(std::cell::Cell::new(false));
    let flag = collapsed.clone();
    let cx = host(cx, move |window, cx| {
        let region = crate::resizable::use_resizable(
            "nav",
            crate::resizable::UseResizable::new()
                .default_size(200.)
                .collapsible(true)
                .is_collapsed(flag.get()),
            window,
            cx,
        );
        Layout::new()
            .padding(SpacingStep::S0)
            .start(
                LayoutPanel::new("panel")
                    .resizable(&region)
                    .child(mark("s")),
            )
            .content(LayoutContent::new("main").child(mark("c")))
            .into_any_element()
    });
    let c0 = at(cx, "c").left();
    collapsed.set(true);
    settle(cx);
    frames(cx, 96);
    let mid = at(cx, "c").left();
    assert!(mid < c0 - px(1.) && mid > px(1.), "sliding: {mid:?}");
    frames(cx, 600);
    assert_eq!(at(cx, "c").left(), px(16.), "shut: the content's own inset");
}

#[gpui::test]
fn panel_fr7_a_lift_past_the_threshold_slides_shut_from_the_lift(cx: &mut TestAppContext) {
    let cx = host(cx, sliding(false, 400.));
    let handle = at(cx, "nav-handle").center();
    let y = handle.y;
    cx.simulate_mouse_down(handle, gpui::MouseButton::Left, gpui::Modifiers::none());
    settle(cx);
    let to = gpui::point(px(30.), y);
    cx.simulate_mouse_move(to, Some(gpui::MouseButton::Left), gpui::Modifiers::none());
    settle(cx);
    // The drag never collapses mid-way: the pane stops at its minimum.
    assert!(at(cx, "c").left() >= px(99.), "held at the minimum");
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    settle(cx);
    let lifted = at(cx, "c").left();
    assert!(
        lifted > px(50.),
        "the close starts from where the edge showed: {lifted:?}"
    );
    frames(cx, 1200);
    let shut = at(cx, "c").left();
    assert!(shut < px(40.), "shut: {shut:?}");
}

#[gpui::test]
fn panel_fr7_a_drag_that_reopens_plays_nothing(cx: &mut TestAppContext) {
    let cx = host(cx, sliding(false, 400.));
    press(cx, "tab");
    press(cx, "enter");
    frames(cx, 700);
    let handle = at(cx, "nav-handle").center();
    cx.simulate_mouse_down(handle, gpui::MouseButton::Left, gpui::Modifiers::none());
    settle(cx);
    let to = gpui::point(handle.x + px(150.), handle.y);
    cx.simulate_mouse_move(to, Some(gpui::MouseButton::Left), gpui::Modifiers::none());
    settle(cx);
    let c = at(cx, "c").left();
    assert!(c > px(140.), "the pointer drives the edge at once: {c:?}");
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    settle(cx);
}

#[gpui::test]
fn panel_fr7_under_reduce_motion_with_a_mouse_it_lands_at_once(cx: &mut TestAppContext) {
    let cx = host(cx, sliding(false, 400.));
    cx.update(|_, cx| rustryx_primitives::set_reduced_motion(true, cx));
    press(cx, "tab");
    press(cx, "enter");
    settle(cx);
    let c = at(cx, "c").left();
    assert!(c < px(40.), "no slide: {c:?}");
}

#[gpui::test]
fn panel_ar2_a_closing_panel_takes_no_pointer(cx: &mut TestAppContext) {
    let clicks = Rc::new(std::cell::Cell::new(0));
    let count = clicks.clone();
    let cx = host(cx, move |window, cx| {
        let region = crate::resizable::use_resizable(
            "nav",
            crate::resizable::UseResizable::new()
                .default_size(200.)
                .collapsible(true),
            window,
            cx,
        );
        let count = count.clone();
        Layout::new()
            .padding(SpacingStep::S0)
            .start(
                div()
                    .flex()
                    .h_full()
                    .child(
                        LayoutPanel::new("panel").resizable(&region).child(
                            div()
                                .id("inside")
                                .debug_selector(|| "inside".into())
                                .size(px(40.))
                                .on_click(move |_, _, _| count.set(count.get() + 1)),
                        ),
                    )
                    .child(crate::resizable::ResizeHandle::new("nav-handle").resizable(&region)),
            )
            .content(LayoutContent::new("main").child(mark("c")))
            .into_any_element()
    });
    let inside = at(cx, "inside").center();
    press(cx, "tab");
    press(cx, "enter");
    frames(cx, 32);
    cx.simulate_click(inside, gpui::Modifiers::none());
    settle(cx);
    assert_eq!(clicks.get(), 0, "inert while it closes");
}

#[gpui::test]
fn resizable_fr8_a_pull_past_the_maximum_bands_the_handle_and_springs_back(
    cx: &mut TestAppContext,
) {
    let cx = host(cx, sliding(false, 300.));
    let c0 = at(cx, "c").left();
    let handle = at(cx, "nav-handle");
    let rest = handle.left();
    let y = handle.center().y;
    cx.simulate_mouse_down(
        handle.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    settle(cx);
    let to = gpui::point(handle.center().x + px(400.), y);
    cx.simulate_mouse_move(to, Some(gpui::MouseButton::Left), gpui::Modifiers::none());
    settle(cx);
    let at_max = rest + px(100.);
    let pulled = at(cx, "nav-handle").left() - at_max;
    assert!(
        pulled > px(1.) && pulled <= px(8.),
        "the band keeps the travel to about 8 px: {pulled:?}"
    );
    assert_eq!(
        at(cx, "c").left(),
        c0 + px(100.),
        "the band does not change the panes' widths"
    );
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    settle(cx);
    frames(cx, 800);
    assert_eq!(at(cx, "nav-handle").left(), at_max, "back at the limit");
}

#[gpui::test]
fn resizable_fr8_an_arrow_key_at_a_limit_draws_no_band(cx: &mut TestAppContext) {
    let cx = host(cx, sliding(false, 300.));
    let rest = at(cx, "nav-handle").left();
    press(cx, "tab");
    press(cx, "end");
    press(cx, "right");
    settle(cx);
    assert_eq!(at(cx, "nav-handle").left(), rest + px(100.));
}

/// A start panel of 200 px (100 to 400, snapping to `snaps`) with its handle.
fn snapping(snaps: Vec<f32>) -> impl Fn(&mut Window, &mut App) -> AnyElement {
    move |window, cx| {
        let region = crate::resizable::use_resizable(
            "nav",
            crate::resizable::UseResizable::new()
                .default_size(200.)
                .min_size(100.)
                .max_size(400.)
                .snaps(snaps.clone())
                .collapsible(true),
            window,
            cx,
        );
        Layout::new()
            .padding(SpacingStep::S0)
            .start(
                div()
                    .flex()
                    .h_full()
                    .child(
                        LayoutPanel::new("panel")
                            .resizable(&region)
                            .has_divider(true)
                            .child(mark("s")),
                    )
                    .child(crate::resizable::ResizeHandle::new("nav-handle").resizable(&region)),
            )
            .content(LayoutContent::new("main").child(mark("c")))
            .into_any_element()
    }
}

fn press_at(cx: &mut VisualTestContext, at: gpui::Point<Pixels>) {
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    settle(cx);
}

fn move_to(cx: &mut VisualTestContext, at: gpui::Point<Pixels>) {
    cx.simulate_mouse_move(at, Some(gpui::MouseButton::Left), gpui::Modifiers::none());
    settle(cx);
}

fn lift_at(cx: &mut VisualTestContext, at: gpui::Point<Pixels>) {
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    settle(cx);
}

#[gpui::test]
fn panel_fr7_the_content_moves_at_its_final_width(cx: &mut TestAppContext) {
    let cx = host(cx, sliding(false, 400.));
    press(cx, "tab");
    press(cx, "enter");
    frames(cx, 96);
    let mid = at(cx, "c");
    frames(cx, 700);
    let rest = at(cx, "c");
    assert_eq!(
        mid.size.width, rest.size.width,
        "no reflow: its final width at once"
    );
    assert!(
        mid.left() > rest.left() + px(1.),
        "it moves beside the edge"
    );
}

#[gpui::test]
fn panel_ar2_tab_skips_a_concealing_and_a_collapsed_pane(cx: &mut TestAppContext) {
    let cx = host(cx, |window, cx| {
        let region = crate::resizable::use_resizable(
            "nav",
            crate::resizable::UseResizable::new()
                .default_size(200.)
                .collapsible(true),
            window,
            cx,
        );
        let inside = window
            .use_keyed_state("inside-focus", cx, |_, cx| cx.focus_handle().tab_stop(true))
            .read(cx)
            .clone();
        let after = window
            .use_keyed_state("after-focus", cx, |_, cx| cx.focus_handle().tab_stop(true))
            .read(cx)
            .clone();
        Layout::new()
            .padding(SpacingStep::S0)
            .start(
                div()
                    .flex()
                    .h_full()
                    .child(
                        LayoutPanel::new("panel").resizable(&region).child(
                            div()
                                .id("inside")
                                .debug_selector(|| "inside".into())
                                .size(px(20.))
                                .track_focus(&inside),
                        ),
                    )
                    .child(crate::resizable::ResizeHandle::new("nav-handle").resizable(&region)),
            )
            .content(
                LayoutContent::new("main").child(
                    div()
                        .id("after")
                        .debug_selector(|| "after".into())
                        .size(px(20.))
                        .track_focus(&after),
                ),
            )
            .into_any_element()
    });
    let focused = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| window.focused(cx).map(|f| format!("{f:?}")))
    };
    press(cx, "tab");
    let inside = focused(cx);
    press(cx, "tab");
    let handle = focused(cx);
    assert_ne!(inside, handle);
    // Enter on the handle collapses the panel; while it slides shut Shift-Tab skips it.
    press(cx, "enter");
    frames(cx, 32);
    press(cx, "shift-tab");
    assert_ne!(
        focused(cx),
        inside,
        "a concealing pane is out of the tab order"
    );
    frames(cx, 700);
    press(cx, "tab");
    press(cx, "shift-tab");
    press(cx, "shift-tab");
    assert_ne!(
        focused(cx),
        inside,
        "a collapsed pane is out of the tab order"
    );
}

#[gpui::test]
fn resizable_fr8_a_press_below_the_minimum_holds_the_closing_slide(cx: &mut TestAppContext) {
    let cx = host(cx, snapping(Vec::new()));
    press(cx, "tab");
    press(cx, "enter");
    // Until the edge shows below the 100 px minimum.
    let mut waited = 0;
    while at(cx, "nav-handle").left() > px(80.) && waited < 600 {
        frames(cx, 16);
        waited += 16;
    }
    let handle = at(cx, "nav-handle");
    let c0 = at(cx, "c").left();
    assert!(c0 > px(10.), "still sliding: {c0:?}");
    press_at(cx, handle.center());
    frames(cx, 200);
    let held = at(cx, "c").left();
    assert!(
        (held - c0).abs() < px(2.),
        "the press holds the slide: {held:?} vs {c0:?}"
    );
    move_to(cx, handle.center() + gpui::point(px(10.), px(0.)));
    let moved = at(cx, "c").left();
    assert!(
        (moved - (held + px(10.))).abs() < px(2.),
        "the drag carries the edge: {moved:?}"
    );
    // A lift under the collapsed size lets the slide play on and shut.
    let under = gpui::point(px(20.), handle.center().y);
    move_to(cx, under);
    lift_at(cx, under);
    frames(cx, 1200);
    let shut = at(cx, "c").left();
    assert!(shut < px(20.), "shut: {shut:?}");
}

#[gpui::test]
fn resizable_fr8_a_held_slide_dragged_past_the_minimum_opens_under_the_pointer(
    cx: &mut TestAppContext,
) {
    let cx = host(cx, snapping(Vec::new()));
    press(cx, "tab");
    press(cx, "enter");
    let mut waited = 0;
    while at(cx, "nav-handle").left() > px(80.) && waited < 600 {
        frames(cx, 16);
        waited += 16;
    }
    let handle = at(cx, "nav-handle").center();
    press_at(cx, handle);
    let to = gpui::point(px(150.), handle.y);
    move_to(cx, to);
    let open = at(cx, "nav-handle").left();
    assert!(
        (open - px(150.)).abs() < px(4.),
        "open at the pointer: {open:?}"
    );
    lift_at(cx, to);
    frames(cx, 400);
    assert!(
        (at(cx, "nav-handle").left() - open).abs() < px(1.),
        "it stays"
    );
}

#[gpui::test]
fn resizable_fr8_a_catch_between_snap_points_carries_to_the_nearest_on_the_lift(
    cx: &mut TestAppContext,
) {
    let cx = host(cx, snapping(vec![200., 300.]));
    press(cx, "tab");
    press(cx, "enter");
    let mut waited = 0;
    while at(cx, "nav-handle").left() > px(180.) && waited < 600 {
        frames(cx, 16);
        waited += 16;
    }
    let handle = at(cx, "nav-handle").center();
    press_at(cx, handle);
    // Caught above the minimum it opens where it shows, between snap points, and the
    // edge follows the pointer there.
    let to = handle + gpui::point(px(-10.), px(0.));
    move_to(cx, to);
    let free = at(cx, "nav-handle").center().x;
    assert!(
        (free - to.x).abs() < px(3.),
        "follows the pointer between snaps: {free:?}"
    );
    lift_at(cx, to);
    frames(cx, 1000);
    let rest = at(cx, "nav-handle").left();
    assert!(
        (rest - px(200.)).abs() < px(2.),
        "lands on the nearest snap point: {rest:?}"
    );
}

#[gpui::test]
fn resizable_fr8_the_band_resists_a_longer_pull(cx: &mut TestAppContext) {
    let cx = host(cx, sliding(false, 300.));
    let handle = at(cx, "nav-handle");
    let at_max = handle.left() + px(100.);
    press_at(cx, handle.center());
    move_to(cx, handle.center() + gpui::point(px(120.), px(0.)));
    let short = at(cx, "nav-handle").left() - at_max;
    move_to(cx, handle.center() + gpui::point(px(400.), px(0.)));
    let long = at(cx, "nav-handle").left() - at_max;
    assert!(short > px(0.5) && short < px(20.), "resisting: {short:?}");
    assert!(
        long > short && long - short < px(280.) / 10.,
        "ever harder: {short:?} {long:?}"
    );
    lift_at(cx, handle.center() + gpui::point(px(400.), px(0.)));
    // Caught mid spring-back, the band goes on from where it is drawn.
    frames(cx, 32);
    let drawn = at(cx, "nav-handle").left();
    let now = at(cx, "nav-handle").center();
    press_at(cx, now);
    move_to(cx, now + gpui::point(px(1.), px(0.)));
    let caught = at(cx, "nav-handle").left();
    assert!(
        (caught - drawn).abs() < px(2.),
        "no jump: {caught:?} vs {drawn:?}"
    );
    lift_at(cx, now + gpui::point(px(1.), px(0.)));
    frames(cx, 800);
    assert_eq!(at(cx, "nav-handle").left(), at_max);
}

#[gpui::test]
fn resizable_fr8_a_held_slide_let_go_above_the_threshold_opens_to_the_minimum(
    cx: &mut TestAppContext,
) {
    let cx = host(cx, snapping(Vec::new()));
    press(cx, "tab");
    press(cx, "enter");
    let mut waited = 0;
    while at(cx, "nav-handle").left() > px(80.) && waited < 600 {
        frames(cx, 16);
        waited += 16;
    }
    let handle = at(cx, "nav-handle").center();
    press_at(cx, handle);
    lift_at(cx, handle);
    frames(cx, 1200);
    let open = at(cx, "nav-handle").left();
    assert!(
        (open - px(100.)).abs() < px(2.),
        "open at the minimum: {open:?}"
    );
}

/// The gap between the content's box and the edge of the panel beside it, mid-slide.
fn gap_beside(cx: &mut VisualTestContext, end: bool) -> Pixels {
    let main = at(cx, "main");
    let handle = at(cx, "nav-handle");
    if end {
        handle.left() - main.right()
    } else {
        main.left() - handle.right()
    }
}

#[gpui::test]
fn ast105_fr9_no_gap_opens_beside_a_moving_edge(cx: &mut TestAppContext) {
    for end in [false, true] {
        let cx = host(&mut *cx, sliding(end, 400.));
        press(cx, "tab");
        // Closing, then opening, sampled through each slide.
        for _ in 0..2 {
            press(cx, "enter");
            for _ in 0..12 {
                frames(cx, 32);
                let gap = gap_beside(cx, end);
                assert!(gap <= px(1.), "end {end}: a gap beside the edge: {gap:?}");
                if !end {
                    // The content's trailing edge stays at the row's end.
                    let main = at(cx, "main");
                    assert!(
                        main.right() >= px(799.),
                        "start: a gap at the content's trailing edge: {:?}",
                        main.right()
                    );
                }
            }
            frames(cx, 600);
        }
        let main = at(cx, "main");
        assert!(gap_beside(cx, end).abs() <= px(1.), "at rest: {main:?}");
    }
}
