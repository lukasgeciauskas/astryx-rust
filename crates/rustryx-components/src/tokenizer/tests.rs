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
            .child(rustryx_primitives::announcer())
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
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
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
        rustryx_primitives::announce::announcement(
            rustryx_primitives::Politeness::Polite,
            window,
            cx,
        )
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

fn sheet_field(t: Tokenizer<SearchItem>) -> Tokenizer<SearchItem> {
    t.presentation(crate::overlay::PopoverPresentation::BottomSheet)
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    // Sheets and dialogs finish their entrance first.
    cx.executor().advance_clock(Duration::from_millis(600));
    settle(cx);
    let at = cx
        .debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
        .center();
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
}

#[gpui::test]
fn fr17_fr19_fr26_the_sheet_edits_a_draft_that_save_reports(cx: &mut TestAppContext) {
    let (cx, value, log) = host(cx, &["Design"], sheet_field);
    assert!(!shown(cx, "k-sheet-list"));
    click(cx, "k-trigger");
    assert!(shown(cx, "k-sheet-list"), "the trigger opens the sheet");
    assert!(shown(cx, "k-sheet-row-design"), "the chosen item leads");
    click(cx, "k-sheet-row-research");
    assert!(log.borrow().is_empty(), "the draft is not reported yet");
    assert_eq!(announced(cx), "Added Research");
    click(cx, "k-sheet-save");
    assert_eq!(*log.borrow(), ["Add Research"]);
    assert_eq!(labels(&value), ["Design", "Research"]);
    assert!(!shown(cx, "k-sheet-list"), "Save closes the sheet");
}

#[gpui::test]
fn fr27_dismissing_a_changed_draft_asks_first(cx: &mut TestAppContext) {
    let (cx, _, log) = host(cx, &[], sheet_field);
    click(cx, "k-trigger");
    click(cx, "k-sheet-row-sales");
    cx.executor().advance_clock(Duration::from_millis(600));
    press(cx, "escape");
    cx.executor().advance_clock(Duration::from_millis(600));
    assert!(shown(cx, "k-sheet-destroy"), "the discard confirmation");
    click(cx, "k-sheet-destroy");
    assert!(!shown(cx, "k-sheet-list"), "discarded and closed");
    assert!(log.borrow().is_empty(), "nothing reported");
}

#[gpui::test]
fn fr24_clear_all_asks_then_empties_the_draft(cx: &mut TestAppContext) {
    let (cx, _, log) = host(cx, &["Design", "Sales"], sheet_field);
    click(cx, "k-trigger");
    click(cx, "k-sheet-clear");
    assert!(shown(cx, "k-sheet-destroy"));
    click(cx, "k-sheet-destroy");
    assert_eq!(announced(cx), "All items removed");
    click(cx, "k-sheet-save");
    assert_eq!(*log.borrow(), ["Remove Sales"]);
}

#[gpui::test]
fn fr23_the_limit_holds_in_the_draft(cx: &mut TestAppContext) {
    let (cx, _, log) = host(cx, &["Design"], |t| sheet_field(t).max_entries(1));
    click(cx, "k-trigger");
    click(cx, "k-sheet-row-sales");
    click(cx, "k-sheet-save");
    assert!(
        log.borrow().is_empty(),
        "an unchecked row at the limit does nothing"
    );
}

// ---- In an InputGroup: one scrolling track (AST-025 FR22) ---------------------

#[gpui::test]
fn a_grouped_row_scrolls_its_tokens_in_one_track_with_faded_edges(cx: &mut TestAppContext) {
    cx.update(rustryx_primitives::editing::init);
    let names = [
        "Design",
        "Engineering",
        "Research",
        "Sales",
        "Support",
        "Marketing",
        "Legal",
        "Finance",
    ];
    let items: Vec<SearchItem> = names
        .iter()
        .map(|n| SearchItem::new(n.to_lowercase(), *n))
        .collect();
    let cx = crate::segmented::tests::host(cx, move || {
        div()
            .w(px(320.))
            .child(
                crate::input::InputGroup::new("g", "Teams").child(
                    Tokenizer::new("k", "Teams", static_source(tags()), items.clone())
                        .debounce(Duration::ZERO),
                ),
            )
            .into_any_element()
    });
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    let track = cx.debug_bounds("k-track").expect("a track in a group");
    assert!(
        cx.debug_bounds("k-track-fade-end").is_some(),
        "hidden tokens fade"
    );
    assert!(cx.debug_bounds("k-track-fade-start").is_none());
    // Tab to the last token's remove button: its token comes into view.
    for _ in 0..names.len() {
        press(cx, "tab");
    }
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    assert!(
        cx.debug_bounds("k-track-fade-start").is_some(),
        "the track scrolled"
    );
    let _ = track;
}
