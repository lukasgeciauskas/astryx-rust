//! Tokenizer tests, numbered after the spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};

use super::*;
use crate::search::{SearchItem, Searchable, static_source};

type Log = Rc<RefCell<Vec<String>>>;
type Value = Rc<RefCell<Vec<SearchItem>>>;
type Adjust = Rc<dyn Fn(Tokenizer<SearchItem>) -> Tokenizer<SearchItem>>;

fn tags() -> Vec<SearchItem> {
    ["Design", "Engineering", "Research", "Sales"]
        .into_iter()
        .map(|t| SearchItem::new(t.to_lowercase(), t))
        .collect()
}

struct Host {
    value: Value,
    log: Log,
    adjust: Adjust,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (store, log) = (self.value.clone(), self.log.clone());
        let tokenizer = Tokenizer::new(
            "k",
            "Teams",
            static_source(tags()),
            self.value.borrow().clone(),
        )
        .debounce(Duration::ZERO)
        .on_change(move |items, change, window, _| {
            log.borrow_mut()
                .push(format!("{:?} {}", change.kind, change.item.label()));
            *store.borrow_mut() = items;
            window.refresh();
        });
        div()
            .p(px(40.))
            .flex()
            .gap(px(40.))
            .child(astryx_primitives::announcer())
            .child(div().w(px(360.)).child((self.adjust)(tokenizer)))
            .child(
                div()
                    .debug_selector(|| "after".into())
                    .child(crate::button::Button::new("after", "After")),
            )
    }
}

fn host<'a>(
    cx: &'a mut TestAppContext,
    value: &[&'static str],
    adjust: impl Fn(Tokenizer<SearchItem>) -> Tokenizer<SearchItem> + 'static,
) -> (&'a mut VisualTestContext, Value, Log) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let value: Value = Rc::new(RefCell::new(
        value
            .iter()
            .map(|v| SearchItem::new(v.to_lowercase(), *v))
            .collect(),
    ));
    let log = Log::default();
    let (v, l) = (value.clone(), log.clone());
    let adjust: Adjust = Rc::new(adjust);
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        value: v,
        log: l,
        adjust,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, value, log)
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

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .is_some()
}

fn labels(value: &Value) -> Vec<String> {
    value
        .borrow()
        .iter()
        .map(|v| v.label().to_string())
        .collect()
}

fn announced(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        astryx_primitives::announce::announcement(astryx_primitives::Politeness::Polite, window, cx)
            .to_string()
    })
}

/// Tab to the field (past the tokens' remove buttons).
fn focus_field(cx: &mut VisualTestContext, tokens: usize) {
    for _ in 0..=tokens {
        press(cx, "tab");
    }
}

#[gpui::test]
fn fr13_chosen_items_are_never_offered(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, &["Design"], |t| t.entries_on_focus(true));
    focus_field(cx, 1);
    assert!(shown(cx, "k-listbox"), "the focus list opens");
    assert!(!shown(cx, "k-option-design"), "Design is chosen");
    assert!(shown(cx, "k-option-research"));
}

#[gpui::test]
fn fr14_a_pick_from_the_focus_list_keeps_it_open(cx: &mut TestAppContext) {
    let (cx, value, log) = host(cx, &[], |t| t.entries_on_focus(true));
    focus_field(cx, 0);
    assert!(shown(cx, "k-highlighted-design"));
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["Add Design"]);
    assert_eq!(labels(&value), ["Design"]);
    assert!(shown(cx, "k-listbox"), "the list stays open");
    assert!(shown(cx, "k-highlighted-engineering"), "on the next choice");
    assert_eq!(announced(cx), "Added Design");
}

#[gpui::test]
fn backspace_in_the_empty_field_removes_the_last_token(cx: &mut TestAppContext) {
    let (cx, value, log) = host(cx, &["Design", "Sales"], |t| t);
    focus_field(cx, 2);
    press(cx, "backspace");
    assert_eq!(*log.borrow(), ["Remove Sales"]);
    assert_eq!(labels(&value), ["Design"]);
    assert_eq!(announced(cx), "Removed Sales");
}

#[gpui::test]
fn create_offers_a_row_for_unmatched_text(cx: &mut TestAppContext) {
    let (cx, value, log) = host(cx, &[], |t| {
        t.create(|text| SearchItem::new(text.to_lowercase(), text))
    });
    focus_field(cx, 0);
    cx.simulate_input("Ops");
    settle(cx);
    assert!(shown(cx, "k-option-ops"), "the create row");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["Create Ops"]);
    assert_eq!(labels(&value), ["Ops"]);
}

#[gpui::test]
fn fr15_the_limit_closes_the_list(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, &[], |t| t.entries_on_focus(true).max_entries(1));
    focus_field(cx, 0);
    press(cx, "enter");
    assert!(!shown(cx, "k-listbox"), "at the limit the list closes");
}

#[gpui::test]
fn clear_all_empties_the_field(cx: &mut TestAppContext) {
    let (cx, value, log) = host(cx, &["Design", "Sales"], |t| t.clear(true));
    let clear = cx
        .debug_bounds("k-clear")
        .expect("a Clear all button")
        .center();
    cx.simulate_click(clear, gpui::Modifiers::none());
    settle(cx);
    assert_eq!(*log.borrow(), ["Remove Sales"]);
    assert!(labels(&value).is_empty());
    assert_eq!(announced(cx), "All items removed");
}

#[gpui::test]
fn fr3_blurred_tokens_fold_into_one_row(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, &["Design", "Engineering", "Research", "Sales"], |t| {
        t.token_overflow(TokenOverflowBehavior::UnfocusedInline)
            .w(px(160.))
    });
    assert!(shown(cx, "k-folded"), "folded while blurred");
    focus_field(cx, 0);
    assert!(!shown(cx, "k-folded"), "every token while focused");
}
