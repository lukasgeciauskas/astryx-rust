//! List and ListItem tests, numbered after the specs' FR items.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, Modifiers, MouseButton, ParentElement,
    Render, StatefulInteractiveElement, Styled, TestAppContext, VisualTestContext, Window, div, px,
};

use super::list::{ListMarker, RowCorners, bleed_corners, counter_text};
use super::*;
use crate::containers::Card;
use crate::layout::{ContainerCorners, ContainerPadding};
use crate::overlay::MenuItem;

type Log = Rc<RefCell<Vec<String>>>;

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().pt(px(40.)).pl(px(40.)).child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<gpui::Pixels> {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

fn in_card(edge: Option<ListEdge>) -> impl Fn() -> AnyElement + 'static {
    move || {
        let mut list = List::new("l");
        if let Some(edge) = edge {
            list = list.edge(edge);
        }
        div()
            .debug_selector(|| "host".into())
            .w(px(300.))
            .child(
                Card::new().child(
                    list.child(ListItem::new("a", "Alpha").on_click(|_, _, _| {}))
                        .child(ListItem::new("b", "Beta")),
                ),
            )
            .into_any_element()
    }
}

/// The host's outer left edge, its content line, a row's left edge and its
/// label's left edge.
fn geometry(cx: &mut VisualTestContext) -> (f32, f32, f32, f32) {
    let host = bounds(cx, "host");
    let row = bounds(cx, "a");
    let label = bounds(cx, "a-label");
    let outer = f32::from(host.left());
    (
        outer,
        outer + 16.,
        f32::from(row.left()),
        f32::from(label.left()),
    )
}

#[gpui::test]
fn fr2_padded_keeps_the_rows_inset(cx: &mut TestAppContext) {
    let cx = host(cx, in_card(None));
    let (_, line, row, label) = geometry(cx);
    assert_eq!(row, line, "the row sits on the content line");
    assert_eq!(label, line + 8., "its text inside its inset");
}

#[gpui::test]
fn fr3_flush_drops_the_inset(cx: &mut TestAppContext) {
    let cx = host(cx, in_card(Some(ListEdge::Flush)));
    let (_, line, row, label) = geometry(cx);
    assert_eq!((row, label), (line, line));
}

#[gpui::test]
fn fr4_outset_reaches_into_the_hosts_padding(cx: &mut TestAppContext) {
    let cx = host(cx, in_card(Some(ListEdge::Outset)));
    let (_, line, row, label) = geometry(cx);
    assert_eq!(row, line - 8., "the row reaches past the line by its inset");
    assert_eq!(label, line, "its text on the host's line");
}

#[gpui::test]
fn fr5_bleed_reaches_the_hosts_padding_edge(cx: &mut TestAppContext) {
    let cx = host(cx, in_card(Some(ListEdge::Bleed)));
    let (outer, line, row, label) = geometry(cx);
    assert!(
        (row - outer).abs() <= 1.,
        "the row reaches the edge: {row} vs {outer}"
    );
    assert_eq!(label, line, "its text on the host's line");
}

#[test]
fn fr5_bleeding_rows_follow_a_rounded_hosts_corners() {
    let corners = ContainerCorners::all(px(20.));
    let host = ContainerPadding {
        inline_start: px(16.),
        inline_end: px(16.),
        block_start: px(12.),
        block_end: px(4.),
    };
    let first = bleed_corners(corners, host, true, false);
    assert_eq!(
        first.start_start,
        px(8.),
        "the corner less the block padding"
    );
    assert_eq!(first.end_end, px(0.), "square away from a corner");
    let last = bleed_corners(corners, host, false, true);
    assert_eq!(last.end_start, px(16.));
    assert_eq!(
        bleed_corners(corners, host, false, false),
        RowCorners::default()
    );
}

#[gpui::test]
fn fr6_without_a_host_outset_and_bleed_look_padded(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .debug_selector(|| "host".into())
            .w(px(300.))
            .child(
                List::new("l")
                    .edge(ListEdge::Bleed)
                    .child(ListItem::new("a", "Alpha")),
            )
            .into_any_element()
    });
    let host = bounds(cx, "host");
    let row = bounds(cx, "a");
    let label = bounds(cx, "a-label");
    assert_eq!(row.left(), host.left());
    assert_eq!(label.left(), host.left() + px(8.));
}

#[gpui::test]
#[allow(deprecated)]
fn fr1_edge_compensation_is_outset_and_edge_wins(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .debug_selector(|| "host".into())
            .w(px(300.))
            .child(
                Card::new().child(
                    List::new("l")
                        .edge_compensation(true)
                        .child(ListItem::new("a", "Alpha")),
                ),
            )
            .into_any_element()
    });
    let (_, line, row, _) = geometry(cx);
    assert_eq!(row, line - 8., "outset");
}

#[test]
fn fr4_counters_write_their_style() {
    assert_eq!(counter_text(3, ListMarker::Decimal), "3.");
    assert_eq!(counter_text(28, ListMarker::LowerAlpha), "ab.");
    assert_eq!(counter_text(14, ListMarker::LowerRoman), "xiv.");
    assert_eq!(
        counter_text(0, ListMarker::LowerRoman),
        "0.",
        "out of range: decimal"
    );
}

#[gpui::test]
fn fr4_numbered_rows_share_one_column(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        List::new("l")
            .list_style(ListStyle::Decimal)
            .start(9)
            .w(px(300.))
            .child(ListItem::new("a", "Nine"))
            .child(ListItem::new("b", "Ten"))
            .into_any_element()
    });
    let nine = bounds(cx, "a-label");
    let ten = bounds(cx, "b-label");
    assert_eq!(nine.left(), ten.left(), "9. and 10. leave the text in line");
}

#[gpui::test]
fn interactive_rows_click_and_delegate(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let (a, b) = (log.clone(), log.clone());
            List::new("l")
                .w(px(300.))
                .child(
                    ListItem::new("a", "Alpha")
                        .on_click(move |_, _, _| a.borrow_mut().push("a".into())),
                )
                .child(
                    ListItem::new("d", "Disabled")
                        .disabled(true)
                        .on_click(move |_, _, _| b.borrow_mut().push("disabled".into())),
                )
                .into_any_element()
        }
    });
    let a = bounds(cx, "a").center();
    let d = bounds(cx, "d").center();
    cx.simulate_click(a, Modifiers::none());
    cx.simulate_click(d, Modifiers::none());
    settle(cx);
    assert_eq!(*log.borrow(), ["a"], "a disabled row never activates");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["a", "a"], "the row is a button");
}

#[gpui::test]
fn fr13_a_row_menu_opens_on_the_row_and_from_its_button(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        List::new("l")
            .w(px(300.))
            .child(ListItem::new("a", "Inbox").context_menu([
                MenuItem::new("archive", "Archive").into(),
                MenuItem::new("delete", "Delete").into(),
            ]))
            .into_any_element()
    });
    let row = bounds(cx, "a").center();
    cx.simulate_mouse_down(row, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(row, MouseButton::Right, Modifiers::none());
    settle(cx);
    assert!(
        cx.debug_bounds("a-menu-list-archive").is_some(),
        "a right press opens the row menu"
    );
    press(cx, "escape");
    assert!(cx.debug_bounds("a-menu-list-archive").is_none());
    // A row with no focusable part has a More actions button: its path.
    press(cx, "tab");
    press(cx, "enter");
    assert!(
        cx.debug_bounds("a-menu-list-archive").is_some(),
        "More actions opens it"
    );
}

#[gpui::test]
fn a_delegate_row_hands_surface_presses_to_its_control(cx: &mut TestAppContext) {
    let log = Log::default();
    let handle = cx.update(|cx| cx.focus_handle());
    let cx = host(cx, {
        let (log, handle) = (log.clone(), handle.clone());
        move || {
            let (own, row) = (log.clone(), log.clone());
            let control = div()
                .id("control")
                .debug_selector(|| "control".into())
                .track_focus(&handle)
                .size(px(16.))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    own.borrow_mut().push("control".into());
                });
            List::new("l")
                .w(px(300.))
                .child(
                    ListItem::new("a", "Alpha")
                        .start_content(control)
                        .interactive(handle.clone(), move |_, _| {
                            row.borrow_mut().push("delegated".into())
                        }),
                )
                .into_any_element()
        }
    });
    let label = bounds(cx, "a-label").center();
    cx.simulate_click(label, Modifiers::none());
    settle(cx);
    assert_eq!(*log.borrow(), ["delegated"], "the surface delegates");
    assert!(
        cx.update(|window, _| handle.is_focused(window)),
        "and focuses the control"
    );
    let control = bounds(cx, "control").center();
    cx.simulate_click(control, Modifiers::none());
    settle(cx);
    assert_eq!(
        *log.borrow(),
        ["delegated", "control"],
        "the control keeps its own press"
    );
}
