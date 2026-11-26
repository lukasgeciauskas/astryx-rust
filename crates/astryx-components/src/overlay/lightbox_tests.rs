//! Lightbox tests, numbered by the upstream spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, Context, IntoElement, Modifiers, MouseButton, ParentElement, Pixels, Point, Render,
    RenderImage, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};
use smallvec::smallvec;

use super::*;
use crate::button::Button;

type Log = Rc<RefCell<Vec<String>>>;

struct Host {
    build: Rc<dyn Fn() -> AnyElement>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(Button::new("opener", "Open"))
            .child((self.build)())
            .child(astryx_primitives::announcer())
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
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        build: Rc::new(build),
    });
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

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    settle(cx);
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

/// A decoded photo of `w` × `h` pixels.
fn photo(w: u32, h: u32, alt: &str) -> LightboxMedia {
    let buffer = image::RgbaImage::from_pixel(w, h, image::Rgba([120, 140, 160, 255]));
    let frame = image::Frame::new(buffer);
    let render = Arc::new(RenderImage::new(smallvec![frame]));
    LightboxMedia::from_source(render, alt.to_string())
}

/// A lightbox kept open until it asks to close; it logs open changes and
/// pages.
fn viewer(log: &Log, count: usize, zoom: bool) -> impl Fn() -> AnyElement + 'static {
    let log = log.clone();
    let open = Rc::new(RefCell::new(true));
    let index = Rc::new(RefCell::new(0usize));
    move || {
        let (l1, l2) = (log.clone(), log.clone());
        let (o, i) = (open.clone(), index.clone());
        let media = (0..count).map(|n| photo(400, 200, &format!("Photo {}", n + 1)));
        Lightbox::new("lb", *open.borrow(), media)
            .index(*index.borrow())
            .has_zoom(zoom)
            .on_open_change(move |value, _, _| {
                *o.borrow_mut() = value;
                l1.borrow_mut().push(format!("open:{value}"));
            })
            .on_index_change(move |value, _, _| {
                *i.borrow_mut() = value;
                l2.borrow_mut().push(format!("index:{value}"));
            })
            .into_any_element()
    }
}

fn logged(log: &Log) -> Vec<String> {
    log.borrow().clone()
}

fn said(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        astryx_primitives::announce::announcement(astryx_primitives::Politeness::Polite, window, cx)
            .to_string()
    })
}

fn centre(cx: &mut VisualTestContext) -> Point<Pixels> {
    let size = cx.update(|window, _| window.viewport_size());
    // The photo sits at the centre of the stage, above the (absent) caption
    // band: the centre of the window.
    point(size.width / 2., size.height / 2.)
}

#[gpui::test]
fn fr4_escape_closes_the_viewer(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 1, false));
    press(cx, "escape");
    assert_eq!(logged(&log), ["open:false"]);
}

#[gpui::test]
fn fr7_arrow_keys_page_and_report(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 3, false));
    press(cx, "right");
    press(cx, "right");
    press(cx, "right");
    assert_eq!(
        logged(&log),
        ["index:1", "index:2"],
        "no page past the last"
    );
    assert!(said(cx).contains("Photo 3"), "the page turn is announced");
    press(cx, "left");
    assert_eq!(logged(&log), ["index:1", "index:2", "index:1"]);
}

#[gpui::test]
fn fr17_an_index_past_the_end_opens_the_last_item(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let l = log.clone();
            Lightbox::new(
                "lb",
                true,
                (0..3).map(|n| photo(400, 200, &format!("Photo {n}"))),
            )
            .index(10)
            .on_index_change(move |value, _, _| l.borrow_mut().push(format!("index:{value}")))
            .into_any_element()
        }
    });
    press(cx, "left");
    assert_eq!(logged(&log), ["index:1"], "it was on the last item, 2");
}

#[gpui::test]
fn fr7_a_horizontal_drag_pages(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 3, false));
    let c = centre(cx);
    cx.simulate_mouse_down(c, MouseButton::Left, Modifiers::none());
    for step in 1..=6 {
        cx.simulate_mouse_move(
            point(c.x - px(80. * step as f32), c.y),
            MouseButton::Left,
            Modifiers::none(),
        );
    }
    cx.simulate_mouse_up(
        point(c.x - px(480.), c.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert_eq!(logged(&log), ["index:1"]);
}

#[gpui::test]
fn fr8_a_drag_down_closes_a_fitted_photo(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 1, false));
    let c = centre(cx);
    cx.simulate_mouse_down(c, MouseButton::Left, Modifiers::none());
    for step in 1..=5 {
        cx.simulate_mouse_move(
            point(c.x, c.y + px(30. * step as f32)),
            MouseButton::Left,
            Modifiers::none(),
        );
    }
    cx.simulate_mouse_up(
        point(c.x, c.y + px(150.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    // The photo goes on down on the dismissal's spring and the viewer closes once it has
    // left.
    assert!(logged(&log).is_empty(), "still leaving");
    let top = |cx: &mut VisualTestContext| cx.debug_bounds("lb-photo").unwrap().top();
    let at_release = top(cx);
    wait(cx, 48);
    assert!(top(cx) > at_release, "it carries on down");
    for _ in 0..40 {
        wait(cx, 16);
    }
    assert_eq!(logged(&log), ["open:false"]);
}

/// `springs.settle`'s progress `ms` in from rest: SwiftUI's smooth spring.
fn settle_progress(ms: f64) -> f64 {
    use astryx_primitives::motion::{solve_spring, uikit::springs::SETTLE};
    solve_spring(SETTLE.to_spring(), 0., 1., 0.).value(ms / 1000.)
}

#[gpui::test]
fn fr6_a_zoom_settles_on_springs_settle(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 1, true));
    let width =
        |cx: &mut VisualTestContext| f32::from(cx.debug_bounds("lb-photo").unwrap().size.width);
    let fitted = width(cx);
    press(cx, "+");
    wait(cx, 1000);
    let zoomed = width(cx);
    assert!(zoomed > fitted * 1.5, "zoomed: {zoomed} vs {fitted}");
    press(cx, "0");
    wait(cx, 100);
    let progress = f64::from((zoomed - width(cx)) / (zoomed - fitted));
    let expected = settle_progress(100.);
    // About 0.36 at 100 ms, where an ease over 280 ms is past 0.5.
    assert!(
        (progress - expected).abs() < 0.12,
        "the spring's own course: {progress} vs {expected}"
    );
    wait(cx, 1000);
    assert!((width(cx) - fitted).abs() < 0.5);
}

#[gpui::test]
fn fr7_a_key_page_turn_slides_the_row_on_the_animated_page_scroll(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 3, false));
    let left =
        |cx: &mut VisualTestContext, s: &'static str| f32::from(cx.debug_bounds(s).unwrap().left());
    let rest = left(cx, "lb-page-0");
    press(cx, "right");
    assert_eq!(logged(&log), ["index:1"]);
    let start = left(cx, "lb-page-1");
    assert!(
        start > rest + 100.,
        "the new page starts where it was: {start}"
    );
    wait(cx, 150);
    let mid = left(cx, "lb-page-1");
    assert!(mid < start && mid > rest, "sliding: {mid}");
    wait(cx, 400);
    assert!((left(cx, "lb-page-1") - rest).abs() < 0.5, "at rest");
}

#[gpui::test]
fn fr7_a_drag_release_settles_on_the_paging_settle(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 3, false));
    let left =
        |cx: &mut VisualTestContext, s: &'static str| f32::from(cx.debug_bounds(s).unwrap().left());
    let rest = left(cx, "lb-page-0");
    let c = centre(cx);
    cx.simulate_mouse_down(c, MouseButton::Left, Modifiers::none());
    for step in 1..=3 {
        cx.simulate_mouse_move(
            point(c.x - px(20. * step as f32), c.y),
            MouseButton::Left,
            Modifiers::none(),
        );
    }
    wait(cx, 100);
    cx.simulate_mouse_up(
        point(c.x - px(60.), c.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert!(logged(&log).is_empty(), "a short drag stays");
    let back = left(cx, "lb-page-0");
    assert!(back < rest - 1., "it starts where the drag left it: {back}");
    for _ in 0..40 {
        wait(cx, 16);
    }
    assert!((left(cx, "lb-page-0") - rest).abs() < 0.5, "back at rest");
}

#[gpui::test]
fn fr8_a_short_drag_down_springs_back(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 1, false));
    let c = centre(cx);
    cx.simulate_mouse_down(c, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        point(c.x, c.y + px(6.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    wait(cx, 200);
    cx.simulate_mouse_up(
        point(c.x, c.y + px(6.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert!(logged(&log).is_empty(), "it stays open");
}

#[gpui::test]
fn a_press_beside_the_photo_closes_and_one_on_it_does_not(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 1, false));
    let c = centre(cx);
    cx.simulate_click(c, Modifiers::none());
    settle(cx);
    assert!(logged(&log).is_empty(), "on the photo");
    cx.simulate_click(point(c.x - px(300.), c.y), Modifiers::none());
    settle(cx);
    assert_eq!(logged(&log), ["open:false"], "beside it");
}

#[gpui::test]
fn fr6_the_keyboard_zooms_and_announces_it(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 1, true));
    press(cx, "+");
    wait(cx, 400);
    assert!(said(cx).contains("Zoomed in"), "said: {}", said(cx));
    press(cx, "0");
    wait(cx, 400);
    assert!(said(cx).contains("Zoomed out"), "said: {}", said(cx));
}

#[gpui::test]
fn fr11_a_double_click_zooms_and_a_second_returns(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, viewer(&log, 1, true));
    let c = centre(cx);
    cx.simulate_event(gpui::MouseDownEvent {
        position: c,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui::MouseUpEvent {
        position: c,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    wait(cx, 400);
    assert!(said(cx).contains("Zoomed in"));
    assert!(logged(&log).is_empty(), "a double click does not close");
}

/// A page with two triggers from `use_lightbox`.
struct Gallery;

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lightbox = use_lightbox(
            "lb",
            [
                photo(400, 200, "First"),
                photo(400, 200, "Second"),
                photo(400, 200, "Third"),
            ],
            window,
            cx,
        );
        div()
            .child(lightbox.trigger(0, div().w(px(40.)).h(px(40.))))
            .child(lightbox.trigger(1, div().w(px(40.)).h(px(40.))))
            .child(lightbox.trigger(2, div().w(px(40.)).h(px(40.))))
            .child(lightbox.element(cx))
            .child(astryx_primitives::announcer())
    }
}

#[gpui::test]
fn use_lightbox_opens_at_the_trigger_and_closes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(|_, _| Gallery);
    cx.update(|window, _| window.activate_window());
    settle(cx);
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "right");
    assert!(said(cx).contains("Third"), "it opened on the second photo");
    press(cx, "escape");
    press(cx, "enter");
    press(cx, "left");
    assert!(
        said(cx).contains("First"),
        "focus came back to the second trigger, which reopened it there"
    );
}
