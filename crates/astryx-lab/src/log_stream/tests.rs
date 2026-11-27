//! Upstream `LogStream.test.tsx`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use astryx_primitives::announce::announcement;
use gpui::{
    Context, Modifiers, Render, ScrollDelta, ScrollEvent, ScrollSource, TestAppContext, TouchPhase,
    VisualTestContext, Window, div, point, px,
};

use super::*;

fn entries() -> Vec<LogEntry> {
    vec![
        LogEntry::new(
            "a",
            "14:02:11.482",
            LogStreamLevel::Info,
            "request accepted",
        )
        .source("api-gateway"),
        LogEntry::new(
            "b",
            "14:02:11.617",
            LogStreamLevel::Warn,
            "retrying upstream call",
        )
        .source("billing"),
        LogEntry::new(
            "c",
            "14:02:12.044",
            LogStreamLevel::Error,
            "upstream call failed",
        )
        .source("billing")
        .detail(|_, _| {
            div()
                .debug_selector(|| "stack-detail".into())
                .child("stack: billing.charge failed with 502")
                .into_any_element()
        }),
        LogEntry::new(
            "d",
            "14:02:12.371",
            LogStreamLevel::Debug,
            "connection pool at 40%",
        )
        .source("api-gateway"),
    ]
}

fn extra(id: &'static str, message: &'static str) -> LogEntry {
    LogEntry::new(id, "14:02:13.005", LogStreamLevel::Info, message).source("api-gateway")
}

/// A page whose stream the test changes: extra entries, the controlled pinning, and
/// other props.
#[derive(Clone, Default)]
struct Props {
    extra: Rc<RefCell<Vec<LogEntry>>>,
    following: Rc<Cell<Option<bool>>>,
    max_height: Option<f32>,
    timestamps: Rc<Cell<bool>>,
    custom: bool,
    log: Rc<RefCell<Vec<bool>>>,
}

struct Page(Props);

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.0.clone();
        let mut all = entries();
        all.extend(p.extra.borrow().iter().cloned());
        let log = p.log.clone();
        let mut stream = LogStream::new("logs", all)
            .timestamps(p.timestamps.get())
            .on_follow_change(move |following, _, _| log.borrow_mut().push(following));
        if let Some(following) = p.following.get() {
            stream = stream.following(following);
        }
        if let Some(max) = p.max_height {
            stream = stream.max_height(px(max));
        }
        if p.custom {
            stream = stream.render_entry(|entry, _, _| {
                let name = format!("custom-{}", entry.id);
                div()
                    .debug_selector(move || name)
                    .child(format!("CUSTOM {}", entry.id))
                    .into_any_element()
            });
        }
        div().w(px(600.)).child(stream)
    }
}

fn page(cx: &mut TestAppContext, props: Props) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    props.timestamps.set(true);
    let (_, cx) = cx.add_window_view(move |_, _| Page(props));
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    settle(cx);
}

/// Scrolls the log by `dy` (negative scrolls toward the bottom).
fn wheel(cx: &mut VisualTestContext, dy: f32) {
    let scroller = cx.debug_bounds("log-stream-scroller").expect("the log");
    cx.simulate_event(ScrollEvent {
        position: scroller.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    settle(cx);
}

/// How far the last row's bottom is from the scroller's.
fn tail_gap(cx: &mut VisualTestContext, last: &'static str) -> f32 {
    let scroller = cx.debug_bounds("log-stream-scroller").expect("the log");
    let row = cx.debug_bounds(last).expect("the last row");
    f32::from(row.bottom() - scroller.bottom()).abs()
}

fn top_of(cx: &mut VisualTestContext, row: &'static str) -> f32 {
    f32::from(cx.debug_bounds(row).expect("drawn").top())
}

fn polite(cx: &mut VisualTestContext) -> SharedString {
    cx.update(|window, cx| announcement(astryx_primitives::Politeness::Polite, window, cx))
}

#[test]
fn columns_follow_the_visible_cells() {
    assert_eq!(columns(true, true), [96., 52., 128.]);
    assert_eq!(columns(true, false), [96., 52.]);
    assert_eq!(columns(false, true), [52., 128.]);
    assert_eq!(columns(false, false), [52.]);
}

#[test]
fn the_bottom_is_within_24_px() {
    // scrollHeight 1000, clientHeight 200: the bottom is 800 down.
    assert!(at_bottom(px(-800.), px(800.)));
    assert!(at_bottom(px(-776.), px(800.)));
    assert!(!at_bottom(px(-775.), px(800.)));
    assert!(!at_bottom(px(-100.), px(800.)));
    assert!(at_bottom(px(0.), px(0.)), "no overflow is the bottom");
}

#[test]
fn the_label_defaults_to_log_stream() {
    assert_eq!(LogStream::new("l", []).label, "Log stream");
    assert_eq!(
        LogStream::new("l", []).label("Build output").label,
        "Build output"
    );
}

#[gpui::test]
fn renders_one_row_per_entry_with_level_and_message(cx: &mut TestAppContext) {
    let cx = page(cx, Props::default());
    for id in ["a", "b", "c", "d"] {
        let name: &'static str = Box::leak(format!("log-stream-message-{id}").into_boxed_str());
        assert!(cx.debug_bounds(name).is_some(), "row {id}");
    }
    // The level lands on the row's level cell.
    assert!(cx.debug_bounds("log-stream-level-c-error").is_some());
    assert!(cx.debug_bounds("log-stream-level-a-info").is_some());
    assert!(cx.debug_bounds("log-stream-level-b-warn").is_some());
    assert!(cx.debug_bounds("log-stream-level-d-debug").is_some());
}

#[gpui::test]
fn hides_timestamps_with_timestamps_false(cx: &mut TestAppContext) {
    let props = Props::default();
    let cx = page(cx, props.clone());
    let with = cx
        .debug_bounds("log-stream-message-a")
        .expect("drawn")
        .left();
    props.timestamps.set(false);
    settle(cx);
    let without = cx
        .debug_bounds("log-stream-message-a")
        .expect("drawn")
        .left();
    // The 96 px timestamp column and its gap leave.
    assert!(with > without, "{with:?} then {without:?}");
}

#[gpui::test]
fn expands_and_collapses_detail_rows_via_the_row_button(cx: &mut TestAppContext) {
    let cx = page(cx, Props::default());
    assert!(cx.debug_bounds("stack-detail").is_none());
    click(cx, "log-stream-row-c");
    assert!(cx.debug_bounds("stack-detail").is_some());
    click(cx, "log-stream-row-c");
    assert!(cx.debug_bounds("stack-detail").is_none());
    // Rows without detail are not buttons: a click opens nothing.
    click(cx, "log-stream-row-a");
    assert!(cx.debug_bounds("stack-detail").is_none());
}

#[gpui::test]
fn the_keyboard_toggles_a_detail_row(cx: &mut TestAppContext) {
    let cx = page(cx, Props::default());
    // The detail row is the only tab stop.
    for key in ["tab", "enter"] {
        let keystroke = gpui::Keystroke::parse(key).expect("a key");
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(gpui::KeyUpEvent { keystroke });
        settle(cx);
    }
    assert!(cx.debug_bounds("stack-detail").is_some());
}

#[gpui::test]
fn pins_scroll_to_the_bottom_on_append_while_following(cx: &mut TestAppContext) {
    let props = Props {
        max_height: Some(60.),
        ..Props::default()
    };
    props.following.set(Some(true));
    let cx = page(cx, props.clone());
    assert!(
        tail_gap(cx, "log-stream-row-d") < 1.,
        "pinned from the start"
    );
    props
        .extra
        .borrow_mut()
        .push(extra("e", "request completed"));
    settle(cx);
    assert!(
        tail_gap(cx, "log-stream-row-e") < 1.,
        "the new row is at the bottom"
    );
}

#[gpui::test]
fn unpins_when_the_user_scrolls_away_from_the_bottom(cx: &mut TestAppContext) {
    let props = Props {
        max_height: Some(60.),
        ..Props::default()
    };
    props.following.set(Some(true));
    let cx = page(cx, props.clone());
    wheel(cx, 100.);
    assert_eq!(*props.log.borrow(), [false]);
}

#[gpui::test]
fn does_not_unpin_on_scroll_events_that_stay_at_the_bottom(cx: &mut TestAppContext) {
    let props = Props {
        max_height: Some(60.),
        ..Props::default()
    };
    props.following.set(Some(true));
    let cx = page(cx, props.clone());
    wheel(cx, -100.);
    assert!(props.log.borrow().is_empty());
}

#[gpui::test]
fn re_pins_via_jump_to_latest(cx: &mut TestAppContext) {
    let props = Props {
        max_height: Some(60.),
        ..Props::default()
    };
    props.following.set(Some(false));
    let cx = page(cx, props.clone());
    // No affordance while the viewport is (assumed) at the bottom.
    assert!(cx.debug_bounds("log-stream-jump").is_none());
    wheel(cx, -10.);
    assert!(cx.debug_bounds("log-stream-jump").is_some());
    click(cx, "log-stream-jump");
    assert_eq!(*props.log.borrow(), [true]);
    assert!(
        tail_gap(cx, "log-stream-row-d") < 1.,
        "scrolled to the bottom"
    );
}

#[gpui::test]
fn supports_uncontrolled_follow_state(cx: &mut TestAppContext) {
    let props = Props {
        max_height: Some(60.),
        ..Props::default()
    };
    let cx = page(cx, props.clone());
    // Unfollowed by default: appends do not move the viewport.
    wheel(cx, -10.);
    let before = top_of(cx, "log-stream-row-a");
    props.extra.borrow_mut().push(extra("e", "tick"));
    settle(cx);
    assert_eq!(top_of(cx, "log-stream-row-a"), before);
    // Jump to latest pins; the next append follows.
    click(cx, "log-stream-jump");
    assert_eq!(*props.log.borrow(), [true]);
    assert!(tail_gap(cx, "log-stream-row-e") < 1.);
    props.extra.borrow_mut().push(extra("f", "tock"));
    settle(cx);
    assert!(tail_gap(cx, "log-stream-row-f") < 1.);
}

#[gpui::test]
fn announces_appends_only_while_following(cx: &mut TestAppContext) {
    // Upstream's `aria-live` is `off` while unfollowed and `polite` while following.
    let props = Props::default();
    props.following.set(Some(false));
    let cx = page(cx, props.clone());
    props.extra.borrow_mut().push(extra("e", "quiet line"));
    settle(cx);
    assert!(polite(cx).is_empty(), "an unfollowed stream is silent");
    props.following.set(Some(true));
    props.extra.borrow_mut().push(extra("f", "loud line"));
    settle(cx);
    let said = polite(cx);
    assert!(said.contains("loud line"), "{said}");
    assert!(!said.contains("quiet line"), "{said}");
}

#[gpui::test]
fn the_initial_rows_are_not_announced(cx: &mut TestAppContext) {
    let props = Props::default();
    props.following.set(Some(true));
    let cx = page(cx, props);
    assert!(polite(cx).is_empty());
}

#[gpui::test]
fn render_entry_replaces_the_default_row(cx: &mut TestAppContext) {
    let cx = page(
        cx,
        Props {
            custom: true,
            ..Props::default()
        },
    );
    assert!(cx.debug_bounds("custom-a").is_some());
    assert!(cx.debug_bounds("custom-c").is_some());
    assert!(cx.debug_bounds("log-stream-message-a").is_none());
    assert!(
        cx.debug_bounds("log-stream-row-c").is_none(),
        "no row buttons"
    );
}
