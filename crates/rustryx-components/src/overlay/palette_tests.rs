//! CommandPalette tests, numbered by the upstream spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    TestAppContext, VisualTestContext, Window, div,
};

use super::*;
use crate::search::{
    SearchAnswer, SearchContext, SearchItem, SearchSource, Searchable, static_source,
};

type Log = Rc<RefCell<Vec<String>>>;

struct Host {
    build: Rc<dyn Fn() -> AnyElement>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child((self.build)())
            .child(rustryx_primitives::announcer())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
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

fn typed(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    settle(cx);
}

fn said(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        rustryx_primitives::announce::announcement(
            rustryx_primitives::Politeness::Polite,
            window,
            cx,
        )
        .to_string()
    })
}

fn shown(cx: &mut VisualTestContext, id: &str) -> bool {
    settle(cx);
    cx.debug_bounds(Box::leak(
        format!("command-palette-item-{id}").into_boxed_str(),
    ))
    .is_some()
}

fn commands() -> Vec<SearchItem> {
    vec![
        SearchItem::new("new", "New file").group("File"),
        SearchItem::new("open", "Open file").group("File"),
        SearchItem::new("theme", "Toggle theme").group("View"),
        SearchItem::new("help", "Help"),
    ]
}

/// A palette kept open until it asks to close; it logs picks and closes.
fn palette(log: &Log) -> impl Fn() -> AnyElement + 'static {
    let log = log.clone();
    let open = Rc::new(RefCell::new(true));
    move || {
        let (l1, l2, o) = (log.clone(), log.clone(), open.clone());
        CommandPalette::new("cp", *open.borrow(), static_source(commands()))
            .on_open_change(move |value, _, _| {
                *o.borrow_mut() = value;
                l1.borrow_mut().push(format!("open:{value}"));
            })
            .on_value_change(move |value, _, _| l2.borrow_mut().push(format!("pick:{value}")))
            .into_any_element()
    }
}

fn logged(log: &Log) -> Vec<String> {
    log.borrow().clone()
}

#[gpui::test]
fn fr1_the_bootstrap_shows_then_typing_filters_and_announces(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, palette(&log));
    assert!(shown(cx, "new") && shown(cx, "help"), "the bootstrap items");
    typed(cx, "file");
    assert!(shown(cx, "new") && shown(cx, "open"));
    assert!(!shown(cx, "theme"), "filtered out");
    assert_eq!(said(cx), "2 results");
}

#[gpui::test]
fn fr7_arrows_highlight_and_enter_picks_and_closes(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, palette(&log));
    press(cx, "enter");
    assert!(logged(&log).is_empty(), "nothing is highlighted at first");
    press(cx, "down");
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(
        logged(&log),
        ["pick:open", "open:false"],
        "the second row, in group order"
    );
}

#[gpui::test]
fn fr6_page_keys_jump_to_the_ends(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, palette(&log));
    press(cx, "pagedown");
    press(cx, "enter");
    assert_eq!(
        logged(&log),
        ["pick:help", "open:false"],
        "the ungrouped row is last"
    );
}

#[gpui::test]
fn fr8_an_empty_search_shows_and_announces_the_empty_text(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, palette(&log));
    typed(cx, "zzz");
    assert!(!shown(cx, "new"));
    assert_eq!(said(cx), "No results");
}

#[gpui::test]
fn fr5_escape_closes_through_the_dialog(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, palette(&log));
    press(cx, "escape");
    assert_eq!(logged(&log), ["open:false"]);
}

/// A source that answers later, logging what it was asked.
struct Slow {
    asked: Log,
}

impl SearchSource<SearchItem> for Slow {
    fn search(
        &self,
        query: &str,
        _: &SearchContext<SearchItem>,
        cx: &mut App,
    ) -> SearchAnswer<SearchItem> {
        self.asked.borrow_mut().push(query.to_string());
        let query = query.to_string();
        let timer = cx.background_executor().timer(Duration::from_millis(100));
        SearchAnswer::Pending(cx.spawn(async move |_| {
            timer.await;
            Ok(vec![SearchItem::new(
                format!("hit-{query}"),
                format!("Hit {query}"),
            )])
        }))
    }

    fn bootstrap(&self, _: &mut App) -> SearchAnswer<SearchItem> {
        SearchAnswer::Ready(Vec::new())
    }
}

#[gpui::test]
fn fr1_an_async_answer_commits_and_a_stale_one_is_dropped(cx: &mut TestAppContext) {
    let asked = Log::default();
    let cx = host(cx, {
        let asked = asked.clone();
        move || {
            CommandPalette::new(
                "cp",
                true,
                Slow {
                    asked: asked.clone(),
                },
            )
            .into_any_element()
        }
    });
    typed(cx, "a");
    typed(cx, "b");
    cx.executor().advance_clock(Duration::from_millis(150));
    settle(cx);
    assert_eq!(*asked.borrow(), ["a", "ab"]);
    assert!(shown(cx, "hit-ab"), "the current query's answer");
    assert!(!shown(cx, "hit-a"), "the older answer was dropped");
    let _ = SharedString::default();
    let _ = SearchItem::new("x", "x").id();
}

#[gpui::test]
fn fr9_a_new_source_while_open_reruns_the_query(cx: &mut TestAppContext) {
    let version = Rc::new(RefCell::new(1u32));
    let cx = host(cx, {
        let version = version.clone();
        move || {
            let v = *version.borrow();
            let items = if v == 1 {
                vec![SearchItem::new("old", "Old match")]
            } else {
                vec![SearchItem::new("new", "New match")]
            };
            CommandPalette::new("cp", true, static_source(items))
                .source_key(v)
                .into_any_element()
        }
    });
    typed(cx, "match");
    assert!(shown(cx, "old"));
    *version.borrow_mut() = 2;
    settle(cx);
    assert!(shown(cx, "new"), "the query ran against the new source");
    assert!(!shown(cx, "old"), "no stale rows");
}

/// A source whose searches fail.
struct Failing;

impl SearchSource<SearchItem> for Failing {
    fn search(
        &self,
        _: &str,
        _: &SearchContext<SearchItem>,
        cx: &mut App,
    ) -> SearchAnswer<SearchItem> {
        SearchAnswer::Pending(cx.spawn(async move |_| Err(anyhow::anyhow!("offline"))))
    }

    fn bootstrap(&self, _: &mut App) -> SearchAnswer<SearchItem> {
        SearchAnswer::Ready(vec![SearchItem::new("recent", "Recent")])
    }
}

#[gpui::test]
fn fr10_a_failed_search_settles_as_an_empty_answer(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        CommandPalette::new("cp", true, Failing).into_any_element()
    });
    assert!(shown(cx, "recent"));
    typed(cx, "x");
    assert!(!shown(cx, "recent"), "the empty state, not an error");
    assert_eq!(said(cx), "No results");
}
