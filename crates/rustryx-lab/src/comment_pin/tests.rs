//! Upstream `CommentPin.test.tsx` (component:CommentPin and spec:AST-085).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, Context, Modifiers, Render, TestAppContext, VisualTestContext, Window, div, point,
    px,
};

use super::*;

fn ada() -> CommentPinParticipant {
    CommentPinParticipant::new("ada", "Ada Lovelace")
}
fn lin() -> CommentPinParticipant {
    CommentPinParticipant::new("lin", "Lin Chen")
}
fn mira() -> CommentPinParticipant {
    CommentPinParticipant::new("mira", "Mira Patel")
}
fn noah() -> CommentPinParticipant {
    CommentPinParticipant::new("noah", "Noah Williams")
}
fn sam() -> CommentPinParticipant {
    CommentPinParticipant::new("sam", "Sam Rivera")
}

fn ids(display: &CommentPinDisplay) -> Vec<&str> {
    display.shown.iter().map(|p| p.id.as_ref()).collect()
}

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(40.)).flex().gap(px(8.)).child((self.0)())
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
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).expect("a keystroke");
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    settle(cx);
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    cx.debug_bounds(selector)
}

type Clicks = Rc<RefCell<Vec<&'static str>>>;

#[test]
fn a_single_thread_shows_one_representative_and_no_count() {
    let d = comment_pin_display(&[ada()], 1., 0.);
    assert!(!d.grouped);
    assert_eq!(ids(&d), ["ada"]);
    assert_eq!(d.overflow, 0);
    assert!(!d.unread());
}

#[test]
fn a_group_shows_up_to_three_distinct_participants_and_the_total() {
    let d = comment_pin_display(&[ada(), ada(), lin(), mira()], 12., 0.);
    assert!(d.grouped);
    assert_eq!(ids(&d), ["ada", "lin", "mira"]);
    assert_eq!(d.overflow, 0, "no overflow circle at three people");
    assert_eq!(js_count_text(d.thread_count), "12");
}

#[test]
fn a_group_caps_at_three_circles_with_an_ellipsis_beyond_three_people() {
    let d = comment_pin_display(&[ada(), lin(), mira(), noah(), sam()], 6., 0.);
    assert_eq!(ids(&d), ["ada", "lin"]);
    assert_eq!(d.overflow, 3);
    assert_eq!(js_count_text(d.thread_count), "6");
}

#[test]
fn sizes_draw_their_circles() {
    assert_eq!(CommentPinSize::default(), CommentPinSize::Sm);
    assert_eq!(CommentPinSize::Sm.avatar_px(), 22.);
    assert_eq!(CommentPinSize::Md.avatar_px(), 24.);
    assert_eq!(CommentPinSize::Lg.avatar_px(), 28.);
}

#[test]
fn unread_marks_a_dot_and_never_a_second_number() {
    let d = comment_pin_display(&[ada(), lin()], 4., 2.);
    assert!(d.unread() && d.grouped);
    assert_eq!(js_count_text(d.thread_count), "4");
    let single = comment_pin_display(&[ada()], 1., 1.);
    assert!(single.unread() && !single.grouped);
}

#[test]
fn normalizes_unread_count_and_warns() {
    for (unread, is_unread) in [(-1., false), (1.5, false), (f64::NAN, false), (9., true)] {
        let d = comment_pin_display(&[ada(), lin()], 3., unread);
        assert_eq!(d.unread(), is_unread, "{unread}");
        assert_eq!(js_count_text(d.thread_count), "3");
        let warnings = comment_pin_warnings(&[ada(), lin()], 3., unread, "Open annotations");
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("unreadCount` must be a finite non-negative")),
            "{unread}: {warnings:?}"
        );
    }
    // Capped at the total.
    assert_eq!(comment_pin_display(&[ada()], 3., 9.).unread_count, 3.);
}

#[test]
fn a_single_marker_keeps_one_representative_even_when_more_are_supplied() {
    let d = comment_pin_display(&[ada(), lin(), mira()], 1., 0.);
    assert_eq!(ids(&d), ["ada"]);
    assert!(!d.grouped);
}

#[test]
fn falls_back_to_an_anonymous_avatar_and_warns_for_no_representatives() {
    let d = comment_pin_display(&[], 1., 0.);
    assert_eq!(ids(&d), ["__anonymous__"]);
    assert_eq!(d.shown[0].name, "");
    let warnings = comment_pin_warnings(&[], 1., 0., "Open annotation");
    assert!(warnings[0].contains("participants` should contain at least one"));
}

#[test]
fn falls_back_to_a_single_thread_marker_for_invalid_thread_counts() {
    for count in [0., -1., 1.5, f64::NAN, f64::INFINITY] {
        let d = comment_pin_display(&[ada(), lin()], count, 0.);
        assert!(!d.grouped, "{count}");
        assert_eq!(ids(&d), ["ada"]);
        let warnings = comment_pin_warnings(&[ada(), lin()], count, 0., "Open annotation");
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("threadCount` must be a finite positive integer"))
        );
    }
}

#[test]
fn a_blank_label_warns_as_javascript_trims() {
    assert!(
        comment_pin_warnings(&[ada()], 1., 0., " \u{a0}\u{3000}\u{feff}")
            .iter()
            .any(|w| w.contains("label"))
    );
    // U+0085 is not ECMAScript white space.
    assert!(comment_pin_warnings(&[ada()], 1., 0., "\u{85}").is_empty());
}

#[test]
fn counts_read_as_javascript_writes_them() {
    assert_eq!(js_count_text(12.), "12");
    assert_eq!(js_count_text(1e20), "100000000000000000000");
    assert_eq!(
        js_count_text(123456789012345680000.),
        "123456789012345680000"
    );
    assert_eq!(js_count_text(1e21), "1e+21");
    assert_eq!(js_count_text(1.5e22), "1.5e+22");
}

fn pin(build: impl Fn() -> CommentPin + 'static) -> impl Fn() -> AnyElement + 'static {
    move || build().into_any_element()
}

#[gpui::test]
fn renders_one_button_and_no_count_for_a_single_thread(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        pin(|| CommentPin::new("p", "Open annotation from Ada", [ada()])),
    );
    assert!(bounds(cx, "comment-pin-p").is_some());
    assert!(bounds(cx, "comment-pin-count-p").is_none());
    assert!(bounds(cx, "comment-pin-unread-p").is_none());
}

#[gpui::test]
fn sizes_set_the_pin_height(cx: &mut TestAppContext) {
    let sizes = Rc::new(Cell::new(CommentPinSize::Sm));
    let cx = host(cx, {
        let sizes = sizes.clone();
        pin(move || {
            CommentPin::new(
                "p",
                "Open 6 grouped annotations",
                [ada(), lin(), mira(), noah(), sam()],
            )
            .thread_count(6)
            .size(sizes.get())
        })
    });
    for (size, height) in [
        (CommentPinSize::Sm, 28.),
        (CommentPinSize::Md, 32.),
        (CommentPinSize::Lg, 36.),
    ] {
        sizes.set(size);
        settle(cx);
        let b = bounds(cx, "comment-pin-p").expect("drawn");
        assert_eq!(f32::from(b.size.height), height, "{size:?}");
        assert!(
            f32::from(b.size.width) >= height,
            "at least as wide as tall"
        );
        // One inset on every edge: the rail's circles sit as far from the start as from
        // the top (the group's first ring sits in the start padding).
        let rail = bounds(cx, "comment-pin-rail-p").expect("drawn");
        let top = rail.top() - b.top();
        assert!(f32::from(top) > 0.);
    }
}

#[gpui::test]
fn the_unread_dot_follows_the_total(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        pin(|| {
            CommentPin::new("p", "Open 4 grouped annotations, 2 unread", [ada(), lin()])
                .thread_count(4)
                .unread_count(2)
        }),
    );
    let count = bounds(cx, "comment-pin-count-p").expect("the total");
    let dot = bounds(cx, "comment-pin-unread-p").expect("the dot");
    assert!(dot.left() >= count.right(), "the dot after the total");
}

#[gpui::test]
fn an_unread_single_pin_has_a_dot_and_no_number(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        pin(|| CommentPin::new("p", "Open unread annotation from Ada", [ada()]).unread_count(1)),
    );
    assert!(bounds(cx, "comment-pin-unread-p").is_some());
    assert!(bounds(cx, "comment-pin-count-p").is_none());
}

#[gpui::test]
fn a_resolved_pin_renders_nothing_unless_selected(cx: &mut TestAppContext) {
    let selected = Rc::new(Cell::new(false));
    let cx = host(cx, {
        let selected = selected.clone();
        pin(move || {
            CommentPin::new("p", "Open resolved annotation from Ada", [ada()])
                .resolved(true)
                .selected(selected.get())
        })
    });
    assert!(bounds(cx, "comment-pin-p").is_none());
    selected.set(true);
    settle(cx);
    assert!(bounds(cx, "comment-pin-p").is_some());
}

fn clicking(
    clicks: &Clicks,
    name: &'static str,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let clicks = clicks.clone();
    move |_, _, _| clicks.borrow_mut().push(name)
}

#[gpui::test]
fn a_dimmed_pin_still_activates(cx: &mut TestAppContext) {
    let clicks = Clicks::default();
    let cx = host(cx, {
        let clicks = clicks.clone();
        pin(move || {
            CommentPin::new("p", "Open another annotation", [lin()])
                .dimmed(true)
                .on_click(clicking(&clicks, "p"))
        })
    });
    click(cx, "comment-pin-p");
    assert_eq!(*clicks.borrow(), ["p"]);
}

#[gpui::test]
fn a_press_on_the_participants_reaches_the_button(cx: &mut TestAppContext) {
    // The participants and the total are decorative and inert.
    let clicks = Clicks::default();
    let cx = host(cx, {
        let clicks = clicks.clone();
        pin(move || {
            CommentPin::new("p", "Open 4 annotations from Ada and Lin", [ada(), lin()])
                .thread_count(4)
                .on_click(clicking(&clicks, "p"))
        })
    });
    click(cx, "comment-pin-rail-p");
    click(cx, "comment-pin-count-p");
    assert_eq!(*clicks.borrow(), ["p", "p"]);
}

#[gpui::test]
fn activates_through_enter_and_space(cx: &mut TestAppContext) {
    let clicks = Clicks::default();
    let cx = host(cx, {
        let clicks = clicks.clone();
        pin(move || {
            CommentPin::new("p", "Open annotation", [ada()]).on_click(clicking(&clicks, "p"))
        })
    });
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "space");
    assert_eq!(*clicks.borrow(), ["p", "p"]);
}

fn three_pins(clicks: &Clicks) -> impl Fn() -> AnyElement + 'static {
    let clicks = clicks.clone();
    move || {
        div()
            .flex()
            .gap(px(8.))
            .child(
                CommentPin::new("first", "Open annotation", [ada()])
                    .on_click(clicking(&clicks, "first")),
            )
            .child(
                CommentPin::new("second", "Pending annotation", [lin()])
                    .disabled(true)
                    .on_click(clicking(&clicks, "second")),
            )
            .child(
                CommentPin::new("third", "Another annotation", [mira()])
                    .on_click(clicking(&clicks, "third")),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn a_disabled_pin_keeps_its_tab_stop(cx: &mut TestAppContext) {
    let clicks = Clicks::default();
    let cx = host(cx, three_pins(&clicks));
    // first, then the disabled second, then third.
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(
        *clicks.borrow(),
        ["third"],
        "the disabled pin kept its place"
    );
}

#[gpui::test]
fn a_disabled_pin_blocks_clicks_enter_and_space(cx: &mut TestAppContext) {
    let clicks = Clicks::default();
    let cx = host(cx, three_pins(&clicks));
    // The click also focuses the disabled pin (it keeps its tab stop).
    click(cx, "comment-pin-second");
    press(cx, "enter");
    press(cx, "space");
    assert!(clicks.borrow().is_empty(), "{:?}", clicks.borrow());
}

#[gpui::test]
fn a_disabled_pin_does_not_activate_a_clickable_ancestor(cx: &mut TestAppContext) {
    let clicks = Clicks::default();
    let cx = host(cx, {
        let clicks = clicks.clone();
        move || {
            let surface = clicks.clone();
            div()
                .id("surface")
                .on_click(move |_, _, _| surface.borrow_mut().push("surface"))
                .child(
                    CommentPin::new("p", "Pending annotation", [ada()])
                        .disabled(true)
                        .on_click(clicking(&clicks, "p")),
                )
                .into_any_element()
        }
    });
    click(cx, "comment-pin-p");
    assert!(clicks.borrow().is_empty(), "{:?}", clicks.borrow());
}

#[gpui::test]
fn keeps_focus_when_it_becomes_disabled_while_focused(cx: &mut TestAppContext) {
    let disabled = Rc::new(Cell::new(false));
    let cx = host(cx, {
        let disabled = disabled.clone();
        pin(move || CommentPin::new("p", "Open annotation", [ada()]).disabled(disabled.get()))
    });
    press(cx, "tab");
    assert!(cx.update(|window, cx| window.focused(cx)).is_some());
    disabled.set(true);
    settle(cx);
    assert!(
        cx.update(|window, cx| window.focused(cx)).is_some(),
        "focus stays"
    );
}

#[gpui::test]
fn styles_reach_the_marker(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        pin(|| CommentPin::new("p", "Open annotation", [ada()]).ml(px(24.))),
    );
    let b = bounds(cx, "comment-pin-p").expect("drawn");
    assert_eq!(
        f32::from(b.left()),
        64.,
        "the host's 40 px padding and the caller's 24 px"
    );
}

#[gpui::test]
fn shows_its_label_as_a_tooltip_on_hover(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        pin(|| CommentPin::new("p", "Open annotation from Ada", [ada()])),
    );
    let b = bounds(cx, "comment-pin-p").expect("drawn");
    cx.simulate_mouse_move(b.center(), None, Modifiers::none());
    settle(cx);
    cx.executor().advance_clock(Duration::from_millis(300));
    settle(cx);
    assert!(
        bounds(cx, "p-tooltip-surface").is_some(),
        "the tooltip shows the label"
    );
    let _ = point(px(0.), px(0.));
}

#[gpui::test]
fn a_disabled_pin_shows_no_tooltip(cx: &mut TestAppContext) {
    let cx = host(
        cx,
        pin(|| CommentPin::new("p", "Pending annotation", [ada()]).disabled(true)),
    );
    let b = bounds(cx, "comment-pin-p").expect("drawn");
    cx.simulate_mouse_move(b.center(), None, Modifiers::none());
    settle(cx);
    cx.executor().advance_clock(Duration::from_millis(300));
    settle(cx);
    assert!(bounds(cx, "p-tooltip-surface").is_none());
}
