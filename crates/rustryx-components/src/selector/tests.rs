//! Selector behavior tests, numbered after the spec's FR and AR items.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    Context, InteractiveElement, IntoElement, Modifiers, MouseButton, ParentElement, Pixels, Point,
    Render, SharedString, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};
use rustryx_primitives::{Placement, Politeness};

use super::*;
use crate::button::Button;
use crate::overlay::PopoverPresentation;

type Log = Rc<RefCell<Vec<String>>>;
type Value = Rc<RefCell<Option<SharedString>>>;
type Options = Rc<RefCell<Vec<SelectorEntry>>>;
type Adjust = Rc<dyn Fn(Selector) -> Selector>;

struct Host {
    value: Value,
    options: Options,
    log: Log,
    adjust: Adjust,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (store, log) = (self.value.clone(), self.log.clone());
        let selector = Selector::new("s", "Fruit", self.options.borrow().clone())
            .value(self.value.borrow().clone())
            .on_change(move |next, window, _| {
                log.borrow_mut()
                    .push(format!("change {}", next.as_deref().unwrap_or("none")));
                *store.borrow_mut() = next;
                window.refresh();
            });
        let after = self.log.clone();
        div()
            .flex()
            .flex_row()
            .gap(px(40.))
            .items_start()
            .pt(px(120.))
            .pl(px(120.))
            .child(rustryx_primitives::announcer())
            .child(div().w(px(240.)).child((self.adjust)(selector)))
            .child(
                div().debug_selector(|| "after".into()).child(
                    Button::new("after", "After")
                        .on_click(move |_, _, _| after.borrow_mut().push("after".into())),
                ),
            )
    }
}

struct Setup {
    value: Value,
    options: Options,
    log: Log,
}

fn fruits() -> Vec<SelectorEntry> {
    ["Apple", "Banana", "Cherry", "Grape", "Mango"]
        .into_iter()
        .map(SelectorEntry::from)
        .collect()
}

fn host<'a>(
    cx: &'a mut TestAppContext,
    options: Vec<SelectorEntry>,
    value: Option<&'static str>,
    adjust: impl Fn(Selector) -> Selector + 'static,
) -> (&'a mut VisualTestContext, Setup) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
    });
    let setup = Setup {
        value: Rc::new(RefCell::new(value.map(SharedString::from))),
        options: Rc::new(RefCell::new(options)),
        log: Log::default(),
    };
    let (v, o, l) = (
        setup.value.clone(),
        setup.options.clone(),
        setup.log.clone(),
    );
    let adjust: Adjust = Rc::new(adjust);
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        value: v,
        options: o,
        log: l,
        adjust,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, setup)
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

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Option<gpui::Bounds<Pixels>> {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
}

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    bounds(cx, selector).is_some()
}

fn center(cx: &mut VisualTestContext, selector: &str) -> Point<Pixels> {
    bounds(cx, selector)
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
        .center()
}

fn is_open(cx: &mut VisualTestContext) -> bool {
    shown(cx, "s-listbox")
}

fn highlighted(cx: &mut VisualTestContext) -> Option<&'static str> {
    ["Apple", "Banana", "Cherry", "Grape", "Mango", "Pear"]
        .into_iter()
        .find(|v| shown(cx, &format!("s-highlighted-{v}")))
}

fn click(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn click_trigger(cx: &mut VisualTestContext) {
    let at = center(cx, "s-control");
    click(cx, at);
}

fn logged(setup: &Setup) -> Vec<String> {
    setup.log.borrow().clone()
}

fn announced(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        rustryx_primitives::announce::announcement(Politeness::Polite, window, cx).to_string()
    })
}

fn focus_trigger(cx: &mut VisualTestContext) {
    press(cx, "tab");
}

#[gpui::test]
fn fr1_picking_an_option_reports_it_and_closes(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s);
    assert!(!is_open(cx));
    click_trigger(cx);
    assert!(is_open(cx), "a click opens the list");
    let cherry = center(cx, "s-option-Cherry");
    click(cx, cherry);
    assert_eq!(logged(&setup), ["change Cherry"]);
    assert!(!is_open(cx), "picking closes the list");
    assert!(shown(cx, "s-control"));
    click_trigger(cx);
    assert!(
        shown(cx, "s-check-Cherry") && !shown(cx, "s-check-Apple"),
        "FR3: only the selected option draws its mark"
    );
}

#[gpui::test]
fn fr2_the_selected_option_opens_over_the_trigger(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, fruits(), Some("Cherry"), |s| s);
    let trigger = center(cx, "s-control");
    click_trigger(cx);
    let cherry = center(cx, "s-option-Cherry");
    // One optical pixel up from the trigger's center.
    let off = (cherry.y - (trigger.y - px(1.))).abs();
    assert!(
        off <= px(1.),
        "Cherry at {cherry:?}, trigger at {trigger:?}"
    );
}

#[gpui::test]
fn fr2_explicit_placement_and_search_use_layer_positioning(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, fruits(), Some("Cherry"), |s| {
        s.placement(Placement::Below)
    });
    let trigger = bounds(cx, "s-control").unwrap();
    click_trigger(cx);
    let list = bounds(cx, "s-listbox").unwrap();
    assert!(list.top() >= trigger.bottom(), "below the trigger");

    let mut app = gpui::TestAppContext::single();
    let (cx, _) = host(&mut app, fruits(), Some("Cherry"), |s| s.search(true));
    let trigger = bounds(cx, "s-control").unwrap();
    click_trigger(cx);
    let list = bounds(cx, "s-listbox").unwrap();
    assert!(list.top() > trigger.bottom(), "a search list opens below");
}

#[gpui::test]
fn fr4_the_bottom_sheet_holds_the_list(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| {
        s.presentation(PopoverPresentation::BottomSheet)
    });
    focus_trigger(cx);
    press(cx, "enter");
    assert!(is_open(cx), "Enter opens the sheet");
    assert_eq!(highlighted(cx), Some("Apple"));
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(
        logged(&setup),
        ["change Banana"],
        "the sheet's list takes the keys"
    );
    assert!(!is_open(cx));
}

#[gpui::test]
fn fr5_loading_shows_no_empty_state(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, Vec::new(), None, |s| s.loading(true));
    click_trigger(cx);
    assert!(is_open(cx));
    assert!(!shown(cx, "s-empty"), "nothing to claim while loading");

    let mut app = gpui::TestAppContext::single();
    let (cx, _) = host(&mut app, Vec::new(), None, |s| s);
    click_trigger(cx);
    assert!(shown(cx, "s-empty"));
    assert_eq!(announced(cx), "No options");
}

#[gpui::test]
fn fr6_read_only_keeps_the_value_and_offers_no_change(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), Some("Apple"), |s| {
        s.read_only(true).clear(true)
    });
    click_trigger(cx);
    assert!(!is_open(cx), "no list");
    // The click focused it.
    for key in ["down", "enter", "space", "delete", "b"] {
        press(cx, key);
    }
    assert!(!is_open(cx));
    assert!(
        logged(&setup).is_empty(),
        "no value change: {:?}",
        logged(&setup)
    );
    assert!(!shown(cx, "s-clear"), "no clear button");
}

#[gpui::test]
fn fr7_the_release_picks_the_option_under_it(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s);
    click_trigger(cx);
    let apple = center(cx, "s-option-Apple");
    let grape = center(cx, "s-option-Grape");
    cx.simulate_mouse_down(apple, MouseButton::Left, Modifiers::none());
    settle(cx);
    assert_eq!(highlighted(cx), Some("Apple"));
    cx.simulate_mouse_move(grape, MouseButton::Left, Modifiers::none());
    settle(cx);
    assert_eq!(
        highlighted(cx),
        Some("Grape"),
        "the highlight follows the held pointer"
    );
    cx.simulate_mouse_up(grape, MouseButton::Left, Modifiers::none());
    settle(cx);
    assert_eq!(
        logged(&setup),
        ["change Grape"],
        "never the option the press began on"
    );
}

#[gpui::test]
fn fr7_a_mouse_released_outside_closes_and_picks_nothing(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s);
    click_trigger(cx);
    let apple = center(cx, "s-option-Apple");
    let outside = point(px(900.), px(600.));
    cx.simulate_mouse_down(apple, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::none());
    settle(cx);
    assert_eq!(highlighted(cx), None, "nothing is lit outside the list");
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    settle(cx);
    assert!(logged(&setup).is_empty());
    assert!(!is_open(cx));
}

#[gpui::test]
fn fr8_opening_highlights_the_selection_or_an_enabled_edge(cx: &mut TestAppContext) {
    let options = vec![
        SelectorOptionData::new("Apple").disabled(true).into(),
        "Banana".into(),
        "Cherry".into(),
        SelectorOptionData::new("Grape").disabled(true).into(),
    ];
    let (cx, _) = host(cx, options.clone(), None, |s| s);
    focus_trigger(cx);
    press(cx, "down");
    assert_eq!(highlighted(cx), Some("Banana"), "the first enabled option");
    press(cx, "escape");
    press(cx, "up");
    assert_eq!(
        highlighted(cx),
        Some("Cherry"),
        "ArrowUp: the last enabled one"
    );

    let mut app = gpui::TestAppContext::single();
    let (cx, _) = host(&mut app, options.clone(), Some("Cherry"), |s| s);
    focus_trigger(cx);
    press(cx, "enter");
    assert_eq!(highlighted(cx), Some("Cherry"), "the selected option");

    let mut app = gpui::TestAppContext::single();
    let (cx, _) = host(&mut app, options.clone(), Some("Apple"), |s| s);
    focus_trigger(cx);
    press(cx, "space");
    assert_eq!(
        highlighted(cx),
        Some("Banana"),
        "never a disabled selection"
    );

    let mut app = gpui::TestAppContext::single();
    let (cx, _) = host(&mut app, options, Some("Cherry"), |s| s.search(true));
    focus_trigger(cx);
    press(cx, "enter");
    assert!(is_open(cx));
    assert_eq!(
        highlighted(cx),
        None,
        "a search list opened by Enter lights nothing"
    );
}

#[gpui::test]
fn fr9_each_query_clears_the_highlight(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s.search(true));
    focus_trigger(cx);
    press(cx, "down");
    press(cx, "down");
    press(cx, "down");
    assert_eq!(highlighted(cx), Some("Banana"));
    cx.simulate_input("an");
    settle(cx);
    assert_eq!(highlighted(cx), None, "a new list has no highlight");
    assert!(!shown(cx, "s-option-Apple") && shown(cx, "s-option-Mango"));
    assert_eq!(announced(cx), "2 results");
    press(cx, "enter");
    assert!(
        logged(&setup).is_empty(),
        "Enter with nothing lit picks nothing"
    );
    press(cx, "down");
    assert_eq!(
        highlighted(cx),
        Some("Banana"),
        "ArrowDown starts at the first match"
    );
    press(cx, "enter");
    assert_eq!(logged(&setup), ["change Banana"]);
}

#[gpui::test]
fn fr9_typing_on_the_trigger_seeds_the_query(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, fruits(), None, |s| s.search(true));
    focus_trigger(cx);
    press(cx, "m");
    assert!(is_open(cx), "a typed letter opens the list");
    assert!(shown(cx, "s-option-Mango") && !shown(cx, "s-option-Apple"));
}

#[gpui::test]
fn fr10_the_highlight_follows_its_option(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s);
    focus_trigger(cx);
    press(cx, "down");
    press(cx, "down");
    assert_eq!(highlighted(cx), Some("Banana"));
    *setup.options.borrow_mut() = ["Mango", "Banana", "Apple"]
        .into_iter()
        .map(SelectorEntry::from)
        .collect();
    settle(cx);
    assert_eq!(
        highlighted(cx),
        Some("Banana"),
        "reordered, it stays on Banana"
    );
    *setup.options.borrow_mut() = ["Mango", "Apple"]
        .into_iter()
        .map(SelectorEntry::from)
        .collect();
    settle(cx);
    assert_eq!(
        highlighted(cx),
        Some("Apple"),
        "removed: the next one follows"
    );
}

#[gpui::test]
fn keys_move_the_highlight_and_stop_at_the_ends(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s);
    focus_trigger(cx);
    press(cx, "down");
    press(cx, "up");
    assert_eq!(highlighted(cx), Some("Apple"), "stops at the first");
    press(cx, "end");
    assert_eq!(highlighted(cx), Some("Mango"));
    press(cx, "down");
    assert_eq!(highlighted(cx), Some("Mango"), "stops at the last");
    press(cx, "home");
    assert_eq!(highlighted(cx), Some("Apple"));
    assert!(
        shown(cx, "s-ring-Apple"),
        "AR5: the keyboard's highlight draws the ring"
    );
    press(cx, "escape");
    assert!(!is_open(cx), "Escape closes");
    press(cx, "space");
    assert!(is_open(cx));
    press(cx, "tab");
    assert!(!is_open(cx), "Tab closes");
    assert!(logged(&setup).is_empty());
}

#[gpui::test]
fn ar5_a_pointer_highlight_draws_no_ring(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, fruits(), None, |s| s);
    click_trigger(cx);
    let cherry = center(cx, "s-option-Cherry");
    cx.simulate_mouse_move(cherry, None, Modifiers::none());
    settle(cx);
    assert_eq!(highlighted(cx), Some("Cherry"));
    assert!(!shown(cx, "s-ring-Cherry"));
}

#[gpui::test]
fn typing_on_a_closed_trigger_picks_like_a_native_select(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s);
    focus_trigger(cx);
    press(cx, "g");
    assert!(!is_open(cx), "the list stays closed");
    assert_eq!(logged(&setup), ["change Grape"]);
    assert_eq!(announced(cx), "Grape", "the new value is announced");
}

#[gpui::test]
fn clearing_reports_none(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), Some("Apple"), |s| s.clear(true));
    focus_trigger(cx);
    press(cx, "delete");
    assert_eq!(
        logged(&setup),
        ["change none"],
        "Delete clears a closed value"
    );
    *setup.value.borrow_mut() = Some("Banana".into());
    settle(cx);
    let clear = center(cx, "s-clear");
    click(cx, clear);
    assert_eq!(logged(&setup), ["change none", "change none"]);
    assert!(!is_open(cx), "the clear button does not open the list");
}

#[gpui::test]
fn disabled_selectors_never_open(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), Some("Apple"), |s| s.disabled(true));
    click_trigger(cx);
    assert!(!is_open(cx));
    press(cx, "down");
    press(cx, "b");
    assert!(!is_open(cx));
    assert!(logged(&setup).is_empty());
}

#[gpui::test]
fn a_change_action_shows_the_new_value_busy(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), Some("Apple"), |s| {
        s.change_action(|_, _, cx| {
            let timer = cx
                .background_executor()
                .timer(std::time::Duration::from_secs(1));
            cx.background_executor().spawn(timer)
        })
    });
    // The host keeps its own value, so undo it to see the optimistic one.
    focus_trigger(cx);
    press(cx, "down");
    press(cx, "down");
    press(cx, "enter");
    *setup.value.borrow_mut() = Some("Apple".into());
    settle(cx);
    assert!(shown(cx, "s-busy"), "busy while the action runs");
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    settle(cx);
    assert!(!shown(cx, "s-busy"));
}

#[gpui::test]
fn sections_group_and_search_hides_empty_ones(cx: &mut TestAppContext) {
    let options = vec![
        SelectorEntry::section("Citrus", ["Lemon", "Lime"]),
        SelectorEntry::divider(),
        SelectorEntry::section("Berries", ["Cherry", "Grape"]),
    ];
    let (cx, setup) = host(cx, options, None, |s| s.search(true));
    focus_trigger(cx);
    press(cx, "enter");
    assert!(shown(cx, "s-option-Lemon") && shown(cx, "s-option-Grape"));
    cx.simulate_input("gr");
    settle(cx);
    assert!(!shown(cx, "s-option-Lemon") && shown(cx, "s-option-Grape"));
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(logged(&setup), ["change Grape"]);
}

fn many() -> Vec<SelectorEntry> {
    (1..=12)
        .map(|i| SelectorEntry::from(format!("Item {i}")))
        .collect()
}

#[gpui::test]
fn fr2_the_overlay_stays_inside_the_window(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, many(), Some("Item 12"), |s| s);
    click_trigger(cx);
    let list = bounds(cx, "s-listbox").unwrap();
    assert!(list.top() >= px(0.), "clamped to the window: {list:?}");
    let last = bounds(cx, "s-option-Item 12").unwrap();
    assert!(
        last.bottom() <= list.bottom() + px(1.),
        "the selected option was scrolled into view"
    );
}

#[gpui::test]
fn ar2_escape_returns_focus_to_the_trigger(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s.search(true));
    focus_trigger(cx);
    press(cx, "enter");
    cx.simulate_input("ch");
    settle(cx);
    press(cx, "escape");
    assert!(!is_open(cx));
    press(cx, "down");
    assert!(is_open(cx), "focus came back to the trigger");
    assert!(logged(&setup).is_empty());
}

#[gpui::test]
fn ar2_a_press_on_another_control_keeps_its_focus(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s.search(true));
    click_trigger(cx);
    assert!(is_open(cx));
    let after = center(cx, "after");
    click(cx, after);
    assert!(!is_open(cx), "a press outside closes the list");
    assert_eq!(logged(&setup), ["after"]);
    press(cx, "enter");
    assert_eq!(
        logged(&setup),
        ["after", "after"],
        "focus stayed on the button"
    );
}

#[gpui::test]
fn tab_from_the_search_field_reaches_its_clear_button_then_closes(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, fruits(), None, |s| s.search(true));
    focus_trigger(cx);
    press(cx, "enter");
    cx.simulate_input("a");
    settle(cx);
    press(cx, "tab");
    assert!(is_open(cx), "Tab moves to the query's clear button");
    press(cx, "tab");
    assert!(!is_open(cx), "and Tab from there closes the list");
}

#[gpui::test]
fn the_callers_handlers_run(cx: &mut TestAppContext) {
    let calls = Log::default();
    let (a, b, c) = (calls.clone(), calls.clone(), calls.clone());
    let (cx, _) = host(cx, fruits(), None, move |s| {
        let (a, b, c) = (a.clone(), b.clone(), c.clone());
        s.on_click(move |_, _, _| a.borrow_mut().push("click".into()))
            .on_focus(move |_, _| b.borrow_mut().push("focus".into()))
            .on_key_down(move |event, _, _| {
                c.borrow_mut().push(format!("key {}", event.keystroke.key))
            })
    });
    focus_trigger(cx);
    press(cx, "f1");
    press(cx, "down");
    press(cx, "escape");
    let at = center(cx, "s-control");
    click(cx, at);
    let calls = calls.borrow().clone();
    assert!(calls.contains(&"focus".to_string()), "{calls:?}");
    assert!(
        calls.contains(&"key f1".to_string()),
        "unhandled keys reach it"
    );
    assert!(
        !calls.contains(&"key down".to_string()),
        "handled ones do not"
    );
    assert!(calls.contains(&"click".to_string()));
}

#[gpui::test]
fn fr3_the_mark_sits_at_the_indicator_edge(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, fruits(), Some("Banana"), |s| {
        s.indicator_position(SelectorIndicatorPosition::Start)
            .placement(Placement::Below)
    });
    click_trigger(cx);
    let mark = bounds(cx, "s-check-Banana").unwrap();
    let row = bounds(cx, "s-option-Banana").unwrap();
    assert!(mark.left() - row.left() < px(16.), "the mark leads the row");
}

fn finger(cx: &mut VisualTestContext, id: u64, phase: gpui::TouchPhase, at: Point<Pixels>) {
    cx.simulate_event(gpui::TouchEvent {
        id: gpui::TouchId(id),
        phase,
        position: at,
        predicted_position: None,
        force: None,
    });
    settle(cx);
}

#[gpui::test]
fn fr7_a_finger_slides_to_the_option_it_lifts_on(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s);
    click_trigger(cx);
    let (apple, grape) = (center(cx, "s-option-Apple"), center(cx, "s-option-Grape"));
    finger(cx, 1, gpui::TouchPhase::Started, apple);
    assert_eq!(highlighted(cx), Some("Apple"));
    finger(cx, 1, gpui::TouchPhase::Moved, grape);
    assert_eq!(
        highlighted(cx),
        Some("Grape"),
        "the highlight follows the finger"
    );
    finger(cx, 1, gpui::TouchPhase::Ended, grape);
    assert_eq!(
        logged(&setup),
        ["change Grape"],
        "once, the option under the lift"
    );
}

#[gpui::test]
fn fr7_a_finger_released_outside_leaves_the_list_open_and_picks_nothing(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s);
    click_trigger(cx);
    let apple = center(cx, "s-option-Apple");
    let outside = point(px(900.), px(600.));
    finger(cx, 1, gpui::TouchPhase::Started, apple);
    finger(cx, 1, gpui::TouchPhase::Moved, outside);
    finger(cx, 1, gpui::TouchPhase::Ended, outside);
    assert!(logged(&setup).is_empty());
    assert!(is_open(cx), "a finger leaves the list open");
    // A tap on an option picks it.
    let banana = center(cx, "s-option-Banana");
    finger(cx, 2, gpui::TouchPhase::Started, banana);
    finger(cx, 2, gpui::TouchPhase::Ended, banana);
    assert_eq!(logged(&setup), ["change Banana"]);
}

#[gpui::test]
fn fr7_a_cancelled_finger_picks_nothing(cx: &mut TestAppContext) {
    let (cx, setup) = host(cx, fruits(), None, |s| s);
    click_trigger(cx);
    let apple = center(cx, "s-option-Apple");
    finger(cx, 1, gpui::TouchPhase::Started, apple);
    finger(cx, 1, gpui::TouchPhase::Cancelled, apple);
    assert!(logged(&setup).is_empty());
    assert!(is_open(cx));
}

#[gpui::test]
fn fr4_a_sheet_that_closes_after_the_pointer_turned_fine_plays_its_exit(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, fruits(), None, |s| {
        s.presentation(PopoverPresentation::Adaptive)
    });
    // A phone: a finger and a compact window.
    cx.update(|_, cx| rustryx_primitives::modality::set_primary_pointer_coarse(Some(true), cx));
    cx.simulate_resize(gpui::size(px(393.), px(852.)));
    focus_trigger(cx);
    press(cx, "enter");
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let sheet = gpui::ElementId::Name("s-sheet".into());
    assert!(
        crate::overlay::is_sheet_presented(&sheet),
        "it opened as a sheet"
    );
    // A trackpad docks, then the sheet closes: it slides away as the sheet it is.
    cx.update(|_, cx| rustryx_primitives::modality::set_primary_pointer_coarse(Some(false), cx));
    press(cx, "escape");
    assert!(
        crate::overlay::is_sheet_presented(&sheet),
        "the closing sheet stays through its exit"
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    assert!(
        !crate::overlay::is_sheet_presented(&sheet),
        "then it is gone"
    );
}
