//! Typeahead tests, numbered after the specs' FR items.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, div, px,
};

use super::*;
use crate::search::{
    SearchAnswer, SearchContext, SearchItem, SearchSource, Searchable, static_source,
};

type Log = Rc<RefCell<Vec<String>>>;
type Value = Rc<RefCell<Option<SearchItem>>>;

fn people() -> Vec<SearchItem> {
    ["Ada", "Alan", "Grace", "Greta", "Linus"]
        .into_iter()
        .map(|n| SearchItem::new(n.to_lowercase(), n))
        .collect()
}

struct Host {
    build: Rc<dyn Fn(Value, Log) -> AnyElement>,
    value: Value,
    log: Log,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(px(40.))
            .flex()
            .flex_row()
            .gap(px(40.))
            .items_start()
            .child(astryx_primitives::announcer())
            .child(
                div()
                    .w(px(260.))
                    .child((self.build)(self.value.clone(), self.log.clone())),
            )
            .child(
                div()
                    .debug_selector(|| "after".into())
                    .child(crate::button::Button::new("after", "After")),
            )
    }
}

fn host<'a>(
    cx: &'a mut TestAppContext,
    value: Option<&'static str>,
    build: impl Fn(Value, Log) -> AnyElement + 'static,
) -> (&'a mut VisualTestContext, Value, Log) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let value: Value = Rc::new(RefCell::new(
        value.map(|v| SearchItem::new(v.to_lowercase(), v)),
    ));
    let log = Log::default();
    let (v, l) = (value.clone(), log.clone());
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        build: Rc::new(build),
        value: v,
        log: l,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, value, log)
}

fn typeahead(value: Value, log: Log) -> Typeahead<SearchItem> {
    let current = value.borrow().clone();
    Typeahead::new("t", "Person", static_source(people()))
        .value(current)
        .debounce(Duration::from_millis(150))
        .on_change(move |item, window, _| {
            log.borrow_mut().push(format!(
                "change {}",
                item.as_ref().map_or("none".into(), |i| i.label())
            ));
            *value.borrow_mut() = item;
            window.refresh();
        })
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

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .is_some()
}

fn open(cx: &mut VisualTestContext) -> bool {
    shown(cx, "t-listbox")
}

fn highlighted(cx: &mut VisualTestContext) -> Option<&'static str> {
    ["ada", "alan", "grace", "greta", "linus"]
        .into_iter()
        .find(|id| shown(cx, &format!("t-highlighted-{id}")))
}

fn announced(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        astryx_primitives::announce::announcement(astryx_primitives::Politeness::Polite, window, cx)
            .to_string()
    })
}

#[gpui::test]
fn fr1_typing_searches_and_a_pick_reports_and_clears(cx: &mut TestAppContext) {
    let (cx, value, log) = host(cx, None, |v, l| typeahead(v, l).into_any_element());
    press(cx, "tab");
    cx.simulate_input("gr");
    settle(cx);
    assert!(!open(cx), "the debounce holds the search");
    wait(cx, 200);
    assert!(open(cx), "the answer opens the list");
    assert_eq!(highlighted(cx), Some("grace"), "on the first result");
    assert!(!shown(cx, "t-option-ada"), "only matches");
    assert_eq!(announced(cx), "2 results");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["change Grace"]);
    assert!(!open(cx), "a pick closes the list");
    assert_eq!(
        value.borrow().as_ref().map(|v| v.label()),
        Some("Grace".into())
    );
    assert!(shown(cx, "t-token"), "the value shows as a Token");
}

#[gpui::test]
fn fr2_a_short_query_searches_nothing(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, None, |v, l| {
        typeahead(v, l).min_query_length(3).into_any_element()
    });
    press(cx, "tab");
    cx.simulate_input("gr");
    wait(cx, 300);
    assert!(!open(cx));
    cx.simulate_input("a");
    wait(cx, 300);
    assert!(open(cx), "three graphemes search");
}

/// A source that answers after a delay set by the query's length.
struct Slow;

impl SearchSource<SearchItem> for Slow {
    fn search(
        &self,
        query: &str,
        _: &SearchContext<SearchItem>,
        cx: &mut App,
    ) -> SearchAnswer<SearchItem> {
        let delay = Duration::from_millis(if query.len() == 1 { 500 } else { 100 });
        let timer = cx.background_executor().timer(delay);
        let answer: Vec<SearchItem> = vec![SearchItem::new(
            format!("r-{query}"),
            format!("Result {query}"),
        )];
        SearchAnswer::Pending(cx.spawn(async move |_| {
            timer.await;
            Ok(answer)
        }))
    }

    fn bootstrap(&self, _: &mut App) -> SearchAnswer<SearchItem> {
        SearchAnswer::Ready(Vec::new())
    }
}

#[gpui::test]
fn fr3_a_newer_query_drops_the_older_answer(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, None, |_, _| {
        Typeahead::new("t", "Thing", Slow)
            .debounce(Duration::ZERO)
            .into_any_element()
    });
    press(cx, "tab");
    cx.simulate_input("a");
    settle(cx);
    assert!(shown(cx, "t-busy"), "FR6: busy while the source works");
    cx.simulate_input("b");
    wait(cx, 150);
    assert!(shown(cx, "t-option-r-ab"), "the newer answer");
    wait(cx, 600);
    assert!(!shown(cx, "t-option-r-a"), "the older one is dropped");
    assert!(!shown(cx, "t-busy"));
}

#[gpui::test]
fn fr4_an_empty_search_shows_and_announces_its_message(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, None, |v, l| {
        typeahead(v, l).debounce(Duration::ZERO).into_any_element()
    });
    press(cx, "tab");
    cx.simulate_input("zzz");
    settle(cx);
    assert!(shown(cx, "t-empty"));
    assert_eq!(announced(cx), "No results found");
}

#[gpui::test]
fn fr5_keys_wrap_and_escape_closes(cx: &mut TestAppContext) {
    let (cx, _, log) = host(cx, None, |v, l| {
        typeahead(v, l).debounce(Duration::ZERO).into_any_element()
    });
    press(cx, "tab");
    cx.simulate_input("a");
    settle(cx);
    assert_eq!(highlighted(cx), Some("ada"));
    press(cx, "up");
    assert_ne!(highlighted(cx), Some("ada"), "ArrowUp wraps to the last");
    press(cx, "down");
    assert_eq!(highlighted(cx), Some("ada"), "ArrowDown wraps to the first");
    press(cx, "escape");
    assert!(!open(cx));
    press(cx, "down");
    assert!(open(cx), "ArrowDown reopens the kept list");
    press(cx, "tab");
    assert!(!open(cx), "Tab hides it");
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn fr8_the_token_edits_and_escape_restores_it(cx: &mut TestAppContext) {
    let (cx, _, log) = host(cx, Some("Ada"), |v, l| {
        typeahead(v, l).debounce(Duration::ZERO).into_any_element()
    });
    assert!(shown(cx, "t-token"));
    press(cx, "tab");
    press(cx, "enter");
    assert!(!shown(cx, "t-token"), "edit mode drops the Token");
    assert!(open(cx), "and searches the value's label");
    press(cx, "escape");
    assert!(!open(cx), "Escape hides the list first");
    press(cx, "escape");
    assert!(shown(cx, "t-token"), "then restores the Token");
    assert!(log.borrow().is_empty());
}

#[gpui::test]
fn fr12_backspace_on_the_token_clears(cx: &mut TestAppContext) {
    let (cx, _, log) = host(cx, Some("Ada"), |v, l| typeahead(v, l).into_any_element());
    press(cx, "tab");
    press(cx, "backspace");
    assert_eq!(*log.borrow(), ["change none"]);
    assert!(
        announced(cx).contains("Ada"),
        "the cleared value is announced"
    );
}

#[gpui::test]
fn fr7_a_disabled_field_opens_nothing(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, None, |v, l| {
        typeahead(v, l)
            .disabled(true)
            .debounce(Duration::ZERO)
            .into_any_element()
    });
    press(cx, "tab");
    cx.simulate_input("a");
    press(cx, "down");
    assert!(!open(cx));
}

#[gpui::test]
fn entries_on_focus_load_the_bootstrap(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, None, |v, l| {
        typeahead(v, l).entries_on_focus(true).into_any_element()
    });
    press(cx, "tab");
    assert!(open(cx));
    assert!(shown(cx, "t-option-linus"), "every entry");
}

#[gpui::test]
fn fr14_the_controller_opens_a_dialog_menu(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, None, |v, l| {
        typeahead(v, l)
            .menu(TypeaheadMenu::dialog("Filters", |info, _, _| {
                div()
                    .debug_selector(|| "dialog-body".into())
                    .child(format!("Query: {}", info.query))
                    .into_any_element()
            }))
            .into_any_element()
    });
    press(cx, "tab");
    cx.simulate_input("a");
    wait(cx, 300);
    assert!(
        !shown(cx, "dialog-body"),
        "a search does not open a dialog menu"
    );
}
