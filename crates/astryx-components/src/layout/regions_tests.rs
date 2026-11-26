//! Layout region tests, numbered after `Layout.spec.md` and its regions' specs.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Context, InteractiveElement, IntoElement, ParentElement, Pixels,
    Render, Styled, TestAppContext, VisualTestContext, Window, div, px,
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
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::storage::use_memory_storage(cx);
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
