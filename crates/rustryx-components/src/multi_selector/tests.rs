//! MultiSelector behavior tests, numbered after the spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    Context, InteractiveElement, IntoElement, Modifiers, ParentElement, Pixels, Point, Render,
    SharedString, Styled, TestAppContext, VisualTestContext, Window, div, px,
};
use rustryx_primitives::Politeness;

use super::component::trigger_text;
use super::*;
use crate::button::{Button, IconButton};
use crate::icon::Icon;
use crate::selector::{SelectorEntry, SelectorOptionData};

type Log = Rc<RefCell<Vec<String>>>;
type Held = Rc<RefCell<Option<MultiSelectorController>>>;
type Format = Rc<dyn Fn(&[(SharedString, SharedString)]) -> SharedString>;
type Value = Rc<RefCell<Vec<SharedString>>>;
type Adjust = Rc<dyn Fn(MultiSelector, &mut Window, &mut gpui::App) -> MultiSelector>;

struct Host {
    value: Value,
    options: Vec<SelectorEntry>,
    log: Log,
    adjust: Adjust,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (store, log) = (self.value.clone(), self.log.clone());
        let selector = MultiSelector::new(
            "m",
            "Fruit",
            self.options.clone(),
            self.value.borrow().clone(),
        )
        .on_change(move |next, change, window, _| {
            let line = match change {
                Some(MultiSelectorChange::Create { query }) => format!("create {query}"),
                None => format!("change {}", next.join(",")),
            };
            log.borrow_mut().push(line);
            *store.borrow_mut() = next;
            window.refresh();
        });
        let selector = (self.adjust)(selector, window, cx);
        let after = self.log.clone();
        div()
            .flex()
            .flex_row()
            .gap(px(40.))
            .items_start()
            .pt(px(120.))
            .pl(px(120.))
            .child(rustryx_primitives::announcer())
            .child(div().w(px(240.)).child(selector))
            .child(
                div().debug_selector(|| "after".into()).child(
                    Button::new("after", "After")
                        .on_click(move |_, _, _| after.borrow_mut().push("after".into())),
                ),
            )
    }
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
    value: &[&'static str],
    adjust: impl Fn(MultiSelector, &mut Window, &mut gpui::App) -> MultiSelector + 'static,
) -> (&'a mut VisualTestContext, Value, Log) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
    });
    let value: Value = Rc::new(RefCell::new(
        value.iter().map(|v| SharedString::from(*v)).collect(),
    ));
    let log = Log::default();
    let (v, l) = (value.clone(), log.clone());
    let adjust: Adjust = Rc::new(adjust);
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        value: v,
        options,
        log: l,
        adjust,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, value, log)
}

fn plain(m: MultiSelector, _: &mut Window, _: &mut gpui::App) -> MultiSelector {
    m
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

fn center(cx: &mut VisualTestContext, selector: &str) -> Point<Pixels> {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
        .center()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let at = center(cx, selector);
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn is_open(cx: &mut VisualTestContext) -> bool {
    shown(cx, "m-listbox")
}

fn highlighted(cx: &mut VisualTestContext) -> Option<&'static str> {
    [
        "Apple",
        "Banana",
        "Cherry",
        "Grape",
        "Mango",
        "select-all",
        "create",
    ]
    .into_iter()
    .find(|v| shown(cx, &format!("m-highlighted-{v}")))
}

fn announced(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        rustryx_primitives::announce::announcement(Politeness::Polite, window, cx).to_string()
    })
}

fn values(value: &Value) -> Vec<String> {
    value.borrow().iter().map(|v| v.to_string()).collect()
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    settle(cx);
}

#[gpui::test]
fn fr1_the_trigger_shows_a_count_labels_or_the_placeholder(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, fruits(), &[], plain);
    cx.update(|_, cx| {
        let pick = |n: usize| -> Vec<(SharedString, SharedString)> {
            ["Apple", "Banana", "Cherry", "Grape"]
                .into_iter()
                .take(n)
                .map(|l| (l.into(), l.into()))
                .collect()
        };
        let text = |d, n, cx: &gpui::App| {
            trigger_text(d, &pick(n), None, "Select…".into(), cx).to_string()
        };
        assert_eq!(text(TriggerDisplay::Count, 0, cx), "Select…");
        assert_eq!(text(TriggerDisplay::Count, 2, cx), "2 selected");
        assert_eq!(text(TriggerDisplay::Labels, 2, cx), "Apple, Banana");
        assert_eq!(
            text(TriggerDisplay::Labels, 4, cx),
            "Apple, Banana, Cherry, +1"
        );
        let f: Format = Rc::new(|items| format!("{} fruit", items.len()).into());
        assert_eq!(
            trigger_text(TriggerDisplay::Labels, &pick(3), Some(&f), "".into(), cx),
            "3 fruit"
        );
    });
}

#[gpui::test]
fn fr1_badges_stop_at_max_badges(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, fruits(), &["Apple", "Banana", "Cherry"], |m, _, _| {
        m.trigger_display(TriggerDisplay::Badges)
            .max_badges(2)
            .clear(true)
    });
    assert!(shown(cx, "m-value"));
    assert!(shown(cx, "m-clear"), "a clear button with a value");
}

#[gpui::test]
fn fr2_an_empty_list_says_so_and_loading_says_nothing(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, Vec::new(), &[], plain);
    click(cx, "m-control");
    assert!(is_open(cx));
    assert!(shown(cx, "m-empty"));
    assert_eq!(announced(cx), "No options");

    let mut app = gpui::TestAppContext::single();
    let (cx, _, _) = host(&mut app, Vec::new(), &[], |m, _, _| m.loading(true));
    click(cx, "m-control");
    assert!(!shown(cx, "m-empty"), "nothing to claim while loading");
    assert!(shown(cx, "m-busy"), "FR11: busy shows the spinner");
}

#[gpui::test]
fn fr2_sections_and_dividers_group_the_rows(cx: &mut TestAppContext) {
    let options = vec![
        SelectorEntry::from("Apple"),
        SelectorEntry::divider(),
        SelectorEntry::section("Berries", ["Blueberry", "Raspberry"]),
        SelectorEntry::section("Stone", ["Cherry", "Plum"]),
    ];
    let (cx, _, _) = host(cx, options, &[], |m, _, _| m.search(true));
    click(cx, "m-control");
    assert!(shown(cx, "m-option-Blueberry") && shown(cx, "m-option-Plum"));
    type_text(cx, "berry");
    assert!(shown(cx, "m-option-Raspberry"));
    assert!(!shown(cx, "m-option-Plum"), "a section with no match hides");
    type_text(cx, "zz");
    assert!(shown(cx, "m-empty"));
    assert_eq!(announced(cx), "No results found");
}

#[gpui::test]
fn fr7_read_only_keeps_the_value_and_offers_no_change(cx: &mut TestAppContext) {
    let (cx, value, log) = host(cx, fruits(), &["Apple"], |m, _, _| {
        m.read_only(true).clear(true)
    });
    click(cx, "m-control");
    assert!(!is_open(cx), "no list");
    for key in ["down", "enter", "space", "delete", "b"] {
        press(cx, key);
    }
    assert!(!is_open(cx));
    assert!(log.borrow().is_empty());
    assert_eq!(values(&value), ["Apple"]);
    assert!(!shown(cx, "m-clear"), "no clear button");
}

#[gpui::test]
fn fr8_a_caller_control_opens_the_list_and_takes_focus_back(cx: &mut TestAppContext) {
    let opens = Log::default();
    let o = opens.clone();
    let (cx, value, _) = host(cx, fruits(), &[], move |m, _, _| {
        let o = o.clone();
        m.render_trigger(|t, _, _| {
            t.attach(
                div()
                    .id("glyph")
                    .debug_selector(|| "glyph".into())
                    .size(px(24.)),
            )
            .into_any_element()
        })
        .on_open_change(move |open, _, _| o.borrow_mut().push(format!("{open}")))
    });
    assert!(!shown(cx, "m-control"), "no field of its own");
    click(cx, "glyph");
    assert!(is_open(cx));
    assert_eq!(
        highlighted(cx),
        Some("Apple"),
        "a press lights the first option"
    );
    // The list owns the keys.
    press(cx, "down");
    assert_eq!(highlighted(cx), Some("Banana"));
    press(cx, "enter");
    assert_eq!(values(&value), ["Banana"]);
    assert!(is_open(cx), "a toggle keeps the list open");
    press(cx, "escape");
    assert!(!is_open(cx));
    // Focus is back on the control: its keys open the list again.
    press(cx, "down");
    assert!(is_open(cx));
    assert_eq!(*opens.borrow(), ["true", "false", "true"]);
}

#[gpui::test]
fn fr8_the_controller_opens_closes_and_toggles(cx: &mut TestAppContext) {
    let held: Held = Rc::default();
    let h = held.clone();
    let (cx, _, _) = host(cx, fruits(), &[], move |m, window, cx| {
        let c = MultiSelectorController::new("m", window, cx);
        *h.borrow_mut() = Some(c.clone());
        m.controller(c)
    });
    let c = held.borrow().clone().unwrap();
    cx.update(|window, cx| c.open(window, cx));
    assert!(is_open(cx));
    assert!(cx.update(|_, cx| c.is_open(cx)));
    cx.update(|window, cx| c.toggle(window, cx));
    assert!(!is_open(cx));
    cx.update(|window, cx| c.toggle(window, cx));
    assert!(is_open(cx));
    cx.update(|window, cx| c.close(window, cx));
    assert!(!is_open(cx));
}

#[gpui::test]
fn fr9_a_create_row_turns_the_query_into_a_value(cx: &mut TestAppContext) {
    let (cx, value, log) = host(cx, fruits(), &["Apple"], |m, _, _| {
        m.search(true).create(true)
    });
    click(cx, "m-control");
    type_text(cx, "Kiwi");
    assert!(shown(cx, "m-option-create"));
    assert_eq!(
        announced(cx),
        "Create \"Kiwi\"",
        "the only result is announced"
    );
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["create Kiwi"]);
    assert_eq!(values(&value), ["Apple", "Kiwi"]);
    assert_eq!(announced(cx), "Kiwi created");
    assert!(!shown(cx, "m-option-create"), "the search clears");
    // An option's own label offers nothing to create.
    type_text(cx, "apple");
    assert!(!shown(cx, "m-option-create"));

    let mut app = gpui::TestAppContext::single();
    let (cx, _, log) = host(&mut app, fruits(), &[], |m, _, _| {
        m.search(true).create(true).loading(true)
    });
    click(cx, "m-control");
    type_text(cx, "Kiwi");
    assert!(!shown(cx, "m-option-create"), "no row while loading");
    press(cx, "enter");
    assert!(log.borrow().is_empty(), "Enter commits nothing");
}

fn with_actions() -> Vec<SelectorEntry> {
    vec![
        SelectorEntry::from("Apple"),
        SelectorEntry::Option(SelectorOptionData::new("Banana").action(|_, _| {
            IconButton::new("banana-more", "More", Icon::new("moreHorizontal")).into_any_element()
        })),
        SelectorEntry::from("Cherry"),
    ]
}

#[gpui::test]
fn fr10_an_action_makes_a_grid_whose_arrows_reach_the_action(cx: &mut TestAppContext) {
    let (cx, value, _) = host(cx, with_actions(), &[], |m, _, _| m);
    // The action reports through the option's control.
    click(cx, "m-control");
    press(cx, "down");
    assert_eq!(highlighted(cx), Some("Banana"));
    press(cx, "right");
    assert!(shown(cx, "m-highlighted-Banana-action"), "the action cell");
    press(cx, "left");
    assert!(!shown(cx, "m-highlighted-Banana-action"));
    press(cx, "right");
    press(cx, "down");
    assert!(
        !shown(cx, "m-highlighted-Banana-action"),
        "a row move lands on the option cell"
    );
    assert_eq!(highlighted(cx), Some("Cherry"));
    // Pressing the option cell toggles; the action's own press does not.
    click(cx, "m-option-Apple");
    assert_eq!(values(&value), ["Apple"]);
    click(cx, "m-action-Banana");
    assert_eq!(
        values(&value),
        ["Apple"],
        "the action fires its control and nothing else"
    );
}

#[gpui::test]
fn fr10_enter_on_the_action_cell_presses_its_control(cx: &mut TestAppContext) {
    let pressed = Log::default();
    let p = pressed.clone();
    let options = vec![
        SelectorEntry::from("Apple"),
        SelectorEntry::Option(SelectorOptionData::new("Banana").action(move |_, _| {
            let p = p.clone();
            IconButton::new("banana-more", "More", Icon::new("moreHorizontal"))
                .on_click(move |_, _, _| p.borrow_mut().push("more".into()))
                .into_any_element()
        })),
    ];
    let (cx, value, _) = host(cx, options, &[], |m, _, _| m);
    click(cx, "m-control");
    press(cx, "down");
    press(cx, "right");
    press(cx, "enter");
    assert_eq!(*pressed.borrow(), ["more"]);
    assert!(values(&value).is_empty(), "the selection does not change");
    // No action control is a tab stop: Tab leaves the list for the next control.
    press(cx, "tab");
    assert!(!is_open(cx));
}

#[gpui::test]
fn fr11_a_disabled_trigger_opens_nothing(cx: &mut TestAppContext) {
    let clicks = Log::default();
    let c = clicks.clone();
    let (cx, _, _) = host(cx, fruits(), &[], move |m, _, _| {
        let c = c.clone();
        m.disabled(true)
            .on_click(move |_, _, _| c.borrow_mut().push("click".into()))
    });
    click(cx, "m-control");
    assert!(!is_open(cx));
    press(cx, "down");
    assert!(!is_open(cx));
    assert!(clicks.borrow().is_empty(), "no press reaches the caller");
}

#[gpui::test]
fn fr12_the_keys_open_move_toggle_and_close(cx: &mut TestAppContext) {
    let (cx, value, _) = host(cx, fruits(), &[], |m, _, _| m.clear(true));
    press(cx, "tab");
    press(cx, "down");
    assert!(is_open(cx));
    assert_eq!(highlighted(cx), Some("Apple"));
    press(cx, "down");
    assert_eq!(highlighted(cx), Some("Banana"));
    press(cx, "end");
    assert_eq!(highlighted(cx), Some("Mango"));
    press(cx, "home");
    assert_eq!(highlighted(cx), Some("Apple"));
    press(cx, "enter");
    press(cx, "down");
    press(cx, "space");
    assert_eq!(values(&value), ["Apple", "Banana"]);
    assert!(is_open(cx), "toggles keep it open");
    press(cx, "escape");
    assert!(!is_open(cx));
    // Delete on the closed, clearable trigger clears every value.
    press(cx, "delete");
    assert!(values(&value).is_empty());
    assert_eq!(announced(cx), "Selection cleared");
    // Typing opens on the typed match; Tab closes (and forgets the text).
    press(cx, "c");
    assert!(is_open(cx));
    assert_eq!(highlighted(cx), Some("Cherry"));
    press(cx, "tab");
    assert!(!is_open(cx));
    press(cx, "shift-tab");
    press(cx, "down");
    press(cx, "g");
    assert_eq!(
        highlighted(cx),
        Some("Grape"),
        "typeahead moves an open list"
    );
}

#[gpui::test]
fn fr12_with_search_page_keys_jump_and_space_types(cx: &mut TestAppContext) {
    let (cx, value, _) = host(cx, fruits(), &[], |m, _, _| m.search(true));
    click(cx, "m-control");
    assert!(is_open(cx));
    assert_eq!(
        highlighted(cx),
        None,
        "a pressed search list highlights nothing"
    );
    press(cx, "pagedown");
    assert_eq!(highlighted(cx), Some("Mango"));
    press(cx, "pageup");
    assert_eq!(highlighted(cx), Some("Apple"));
    press(cx, "enter");
    assert_eq!(values(&value), ["Apple"]);
    press(cx, "space");
    assert_eq!(values(&value), ["Apple"], "Space types into the search");
    press(cx, "escape");
    assert!(!is_open(cx));
}

#[gpui::test]
fn fr14_each_change_and_search_is_announced(cx: &mut TestAppContext) {
    let (cx, value, _) = host(cx, fruits(), &[], |m, _, _| m.select_all(true).search(true));
    click(cx, "m-control");
    click(cx, "m-option-Apple");
    assert_eq!(announced(cx), "1 of 5 selected");
    click(cx, "m-option-select-all");
    assert_eq!(announced(cx), "All selected");
    assert_eq!(values(&value).len(), 5);
    click(cx, "m-option-select-all");
    assert_eq!(announced(cx), "Selection cleared");
    type_text(cx, "an");
    assert_eq!(announced(cx), "2 results");
    // Select-all covers what the query shows.
    click(cx, "m-option-select-all");
    assert_eq!(values(&value), ["Banana", "Mango"]);
}

#[gpui::test]
fn fr15_closing_returns_focus_only_when_it_would_be_lost(cx: &mut TestAppContext) {
    let (cx, _, log) = host(cx, fruits(), &[], |m, _, _| m.search(true));
    click(cx, "m-control");
    assert!(is_open(cx));
    // The search field holds focus; Escape returns it to the trigger.
    press(cx, "escape");
    assert!(!is_open(cx));
    press(cx, "down");
    assert!(is_open(cx), "the trigger has focus");
    // A press elsewhere moves focus there and closes.
    click(cx, "after");
    assert!(!is_open(cx));
    assert_eq!(*log.borrow(), ["after"]);
}

#[gpui::test]
fn fr16_each_query_clears_the_highlight(cx: &mut TestAppContext) {
    let (cx, _, _) = host(cx, fruits(), &[], |m, _, _| m.search(true));
    click(cx, "m-control");
    press(cx, "down");
    assert_eq!(highlighted(cx), Some("Apple"));
    type_text(cx, "a");
    assert_eq!(highlighted(cx), None);
    press(cx, "down");
    assert_eq!(
        highlighted(cx),
        Some("Apple"),
        "then starts at the first row"
    );
}

#[gpui::test]
fn fr18_opening_skips_disabled_options_and_leads_with_the_selection(cx: &mut TestAppContext) {
    let options = vec![
        SelectorEntry::Option(SelectorOptionData::new("Apple").disabled(true)),
        SelectorEntry::from("Banana"),
        SelectorEntry::from("Cherry"),
        SelectorEntry::Option(SelectorOptionData::new("Mango").disabled(true)),
    ];
    let (cx, _, _) = host(cx, options, &[], plain);
    press(cx, "tab");
    press(cx, "down");
    assert_eq!(highlighted(cx), Some("Banana"));
    press(cx, "escape");
    press(cx, "up");
    assert_eq!(
        highlighted(cx),
        Some("Cherry"),
        "ArrowUp: the last enabled option"
    );

    let mut app = gpui::TestAppContext::single();
    let (cx, _, _) = host(&mut app, fruits(), &["Mango"], plain);
    click(cx, "m-control");
    assert_eq!(highlighted(cx), Some("Mango"), "the selected options lead");
    let mango = center(cx, "m-option-Mango").y;
    let apple = center(cx, "m-option-Apple").y;
    assert!(mango < apple);
}

#[gpui::test]
fn fr5_the_sheet_hosts_the_same_panel(cx: &mut TestAppContext) {
    let (cx, value, _) = host(cx, fruits(), &[], |m, _, _| {
        m.presentation(crate::overlay::PopoverPresentation::BottomSheet)
            .select_all(true)
    });
    press(cx, "tab");
    press(cx, "enter");
    assert!(is_open(cx), "Enter opens the sheet");
    assert!(shown(cx, "m-option-select-all"));
    assert_eq!(highlighted(cx), Some("select-all"));
    // The sheet's list takes the keys and stays open as options toggle.
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(values(&value), ["Apple"]);
    assert!(is_open(cx));
    press(cx, "escape");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(600));
    assert!(!is_open(cx));
}
