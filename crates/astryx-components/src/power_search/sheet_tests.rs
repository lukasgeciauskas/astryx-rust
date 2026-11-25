//! PowerSearch sheet flow tests (FR11-FR15c).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Context, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};

use super::model::*;
use super::*;

type Log = Rc<RefCell<Vec<String>>>;
type Filters = Rc<RefCell<Vec<PowerSearchFilter>>>;

fn config() -> Rc<PowerSearchConfig> {
    Rc::new(
        PowerSearchConfig::new("books")
            .field(
                PowerSearchField::new("title", "Title")
                    .operator(PowerSearchOperator::new(
                        "contains",
                        "contains",
                        OperatorValue::string(),
                    ))
                    .operator(PowerSearchOperator::new(
                        "is",
                        "is",
                        OperatorValue::string(),
                    )),
            )
            .field(
                PowerSearchField::new("starred", "Starred").operator(PowerSearchOperator::new(
                    "is_true",
                    "is true",
                    OperatorValue::Empty,
                )),
            )
            .field(
                PowerSearchField::new("genre", "Genre").operator(PowerSearchOperator::new(
                    "is",
                    "is",
                    OperatorValue::Enum {
                        values: vec![
                            EnumItem::new("fiction", "Fiction"),
                            EnumItem::new("history", "History"),
                        ],
                    },
                )),
            ),
    )
}

struct Host {
    filters: Filters,
    log: Log,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (store, log) = (self.filters.clone(), self.log.clone());
        div()
            .p(px(20.))
            .w(px(380.))
            .child(astryx_primitives::announcer())
            .child(
                PowerSearch::new(
                    "ps",
                    config(),
                    self.filters.borrow().clone(),
                    move |next, kind, index, window, _| {
                        log.borrow_mut()
                            .push(format!("{kind:?} {index} {}", next.len()));
                        *store.borrow_mut() = next;
                        window.refresh();
                    },
                )
                .presentation(crate::overlay::PopoverPresentation::BottomSheet),
            )
    }
}

fn host(
    cx: &mut TestAppContext,
    filters: Vec<PowerSearchFilter>,
) -> (&mut VisualTestContext, Filters, Log) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let filters: Filters = Rc::new(RefCell::new(filters));
    let log = Log::default();
    let (f, l) = (filters.clone(), log.clone());
    let (_, cx) = cx.add_window_view(move |_, _| Host { filters: f, log: l });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, filters, log)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    cx.executor().advance_clock(Duration::from_millis(500));
    for _ in 0..2 {
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

fn click(cx: &mut VisualTestContext, selector: &str) {
    settle(cx);
    let at = cx
        .debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn logged(log: &Log) -> Vec<String> {
    log.borrow().clone()
}

fn emma() -> PowerSearchFilter {
    PowerSearchFilter::new("title", "is", FilterValue::String("Emma".into()))
}

#[gpui::test]
fn fr11_fr12_the_trigger_opens_filters_and_removals_wait_for_done(cx: &mut TestAppContext) {
    let (cx, filters, log) = host(
        cx,
        vec![
            emma(),
            PowerSearchFilter::new("genre", "is", FilterValue::Enum("fiction".into())),
        ],
    );
    assert!(shown(cx, "ps-trigger"));
    assert!(!shown(cx, "ps-sheet-filters"));
    click(cx, "ps-trigger");
    assert!(shown(cx, "ps-sheet-filters"), "Filters opens");
    assert!(shown(cx, "ps-sheet-row-0") && shown(cx, "ps-sheet-row-1"));
    click(cx, "ps-sheet-remove-0");
    assert!(logged(&log).is_empty(), "the draft, not onChange");
    assert!(!shown(cx, "ps-sheet-row-1"), "one row left");
    click(cx, "ps-sheet-done");
    assert_eq!(logged(&log), ["Remove 0 1"]);
    assert_eq!(filters.borrow().len(), 1);
    assert!(!shown(cx, "ps-sheet-filters"), "Done closes");
}

#[gpui::test]
fn fr15c_dismissing_a_changed_draft_asks_first(cx: &mut TestAppContext) {
    let (cx, _, log) = host(cx, vec![emma()]);
    click(cx, "ps-trigger");
    click(cx, "ps-sheet-remove-0");
    press(cx, "escape");
    assert!(shown(cx, "ps-sheet-destroy"), "the discard confirmation");
    click(cx, "ps-sheet-destroy");
    assert!(!shown(cx, "ps-sheet-filters"));
    assert!(logged(&log).is_empty(), "nothing applied");
}

#[gpui::test]
fn fr12_clear_all_asks_then_keeps_read_only_filters(cx: &mut TestAppContext) {
    let (cx, filters, log) = host(
        cx,
        vec![
            emma().read_only(true),
            PowerSearchFilter::new("genre", "is", FilterValue::Enum("history".into())),
        ],
    );
    click(cx, "ps-trigger");
    click(cx, "ps-sheet-clear");
    click(cx, "ps-sheet-destroy");
    assert!(shown(cx, "ps-sheet-row-0") && !shown(cx, "ps-sheet-row-1"));
    click(cx, "ps-sheet-done");
    assert_eq!(logged(&log), ["Remove 1 1"]);
    assert!(filters.borrow()[0].read_only);
}

#[gpui::test]
fn fr13_fr14_adding_through_the_value_step(cx: &mut TestAppContext) {
    let (cx, filters, log) = host(cx, vec![emma()]);
    click(cx, "ps-trigger");
    click(cx, "ps-sheet-add");
    assert!(
        shown(cx, "ps-sheet-fields"),
        "Add filter opens above Filters"
    );
    click(cx, "ps-sheet-field-genre");
    assert!(shown(cx, "ps-sheet-value"), "the value step");
    // RadioList keys its rows by its id's Debug form and the value.
    click(cx, "Name(\"ps-sheet-value-is\")-history");
    click(cx, "ps-sheet-confirm");
    assert!(
        !shown(cx, "ps-sheet-value") && !shown(cx, "ps-sheet-fields"),
        "the creation closes both"
    );
    assert!(shown(cx, "ps-sheet-row-1"), "and shows the new row");
    click(cx, "ps-sheet-done");
    assert_eq!(logged(&log), ["Add 1 2"]);
    assert_eq!(
        filters.borrow()[1].value,
        FilterValue::Enum("history".into())
    );
}

#[gpui::test]
fn fr13_an_empty_field_opens_on_add_filter_and_a_one_tap_field_adds_at_once(
    cx: &mut TestAppContext,
) {
    let (cx, _, log) = host(cx, Vec::new());
    click(cx, "ps-trigger");
    assert!(shown(cx, "ps-sheet-fields"), "the entry is Add filter");
    click(cx, "ps-sheet-field-starred");
    assert!(shown(cx, "ps-sheet-filters"), "Filters shows the added row");
    assert!(shown(cx, "ps-sheet-row-0"));
    click(cx, "ps-sheet-done");
    assert_eq!(logged(&log), ["Add 0 1"]);
}

struct SearchHost {
    filters: Filters,
    log: Log,
}

impl Render for SearchHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let config =
            Rc::new(
                PowerSearchConfig::new("books")
                    .content_search_field("text")
                    .field(PowerSearchField::new("text", "Text").operator(
                        PowerSearchOperator::new("contains", "contains", OperatorValue::string()),
                    ))
                    .field(
                        PowerSearchField::new("genre", "Genre").operator(
                            PowerSearchOperator::new(
                                "is",
                                "is",
                                OperatorValue::Enum {
                                    values: vec![
                                        EnumItem::new("fiction", "Fiction"),
                                        EnumItem::new("history", "History"),
                                    ],
                                },
                            )
                            .value_only_match(true),
                        ),
                    ),
            );
        let (store, log) = (self.filters.clone(), self.log.clone());
        div().p(px(20.)).w(px(380.)).child(
            PowerSearch::new(
                "ps",
                config,
                self.filters.borrow().clone(),
                move |next, kind, index, window, _| {
                    log.borrow_mut()
                        .push(format!("{kind:?} {index} {}", next.len()));
                    *store.borrow_mut() = next;
                    window.refresh();
                },
            )
            .presentation(crate::overlay::PopoverPresentation::BottomSheet),
        )
    }
}

#[gpui::test]
fn fr13_fr30_the_content_search_lists_suggestions_inline(cx: &mut TestAppContext) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (filters, log): (Filters, Log) = Default::default();
    let (f, l) = (filters.clone(), log.clone());
    let (_, cx) = cx.add_window_view(move |_, _| SearchHost { filters: f, log: l });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    click(cx, "ps-trigger");
    assert!(
        shown(cx, "ps-sheet-content-search"),
        "a search box, not the field list"
    );
    assert!(
        shown(cx, "ps-sheet-result-genre"),
        "the fields before anything is typed"
    );
    click(cx, "ps-sheet-content-search");
    cx.simulate_input("history");
    settle(cx);
    assert!(
        shown(cx, "ps-sheet-result-__content_search__:history"),
        "the content search first"
    );
    assert!(
        shown(cx, "ps-sheet-result-genre:is:only:history"),
        "a value-only match (FR30)"
    );
    click(cx, "ps-sheet-result-genre:is:only:history");
    assert!(shown(cx, "ps-sheet-row-0"), "added; Filters shows it");
    click(cx, "ps-sheet-done");
    assert_eq!(logged(&log), ["Add 0 1"]);
    assert_eq!(
        filters.borrow()[0].value,
        FilterValue::Enum("history".into())
    );
}

struct NumberHost {
    filters: Filters,
    log: Log,
}

impl Render for NumberHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let config =
            Rc::new(
                PowerSearchConfig::new("books")
                    .field(PowerSearchField::new("pages", "Pages").operator(
                        PowerSearchOperator::new("over", "over", OperatorValue::float()),
                    ))
                    .field(PowerSearchField::new("group", "Group").operator(
                        PowerSearchOperator::new("all", "all of", OperatorValue::Nested),
                    )),
            );
        let (store, log) = (self.filters.clone(), self.log.clone());
        div().p(px(20.)).w(px(380.)).child(
            PowerSearch::new(
                "ps",
                config,
                self.filters.borrow().clone(),
                move |next, kind, index, window, _| {
                    log.borrow_mut()
                        .push(format!("{kind:?} {index} {}", next.len()));
                    *store.borrow_mut() = next;
                    window.refresh();
                },
            )
            .presentation(crate::overlay::PopoverPresentation::BottomSheet),
        )
    }
}

fn number_host(cx: &mut TestAppContext) -> (&mut VisualTestContext, Filters, Log) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (filters, log): (Filters, Log) = Default::default();
    let (f, l) = (filters.clone(), log.clone());
    let (_, cx) = cx.add_window_view(move |_, _| NumberHost { filters: f, log: l });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, filters, log)
}

#[gpui::test]
fn fr17_numbers_stage_as_they_are_typed(cx: &mut TestAppContext) {
    let (cx, filters, log) = number_host(cx);
    click(cx, "ps-trigger");
    click(cx, "ps-sheet-field-pages");
    assert!(shown(cx, "ps-sheet-value"));
    // Focus the number field and type; no Enter, no blur.
    click(cx, "ps-sheet-value-pages-over-value-control");
    cx.simulate_input("12");
    settle(cx);
    click(cx, "ps-sheet-confirm");
    assert!(shown(cx, "ps-sheet-row-0"), "the staged number confirmed");
    click(cx, "ps-sheet-done");
    assert_eq!(logged(&log), ["Add 0 1"]);
    assert_eq!(filters.borrow()[0].value, FilterValue::Float(12.));
}

#[gpui::test]
fn fr14_a_nested_operator_edits_its_group_in_the_step(cx: &mut TestAppContext) {
    let (cx, _, log) = number_host(cx);
    click(cx, "ps-trigger");
    click(cx, "ps-sheet-field-group");
    assert!(
        shown(cx, "ps-sheet-nested-group-nested-tree"),
        "the sub-filter tree"
    );
    // Nothing to confirm until the group has a complete sub-filter.
    click(cx, "ps-sheet-confirm");
    assert!(shown(cx, "ps-sheet-value"), "the step stays");
    assert!(logged(&log).is_empty());
}

#[gpui::test]
fn fr16_focus_follows_the_step(cx: &mut TestAppContext) {
    let (cx, _, log) = host(
        cx,
        vec![
            emma(),
            PowerSearchFilter::new("genre", "is", FilterValue::Enum("fiction".into())),
            PowerSearchFilter::new("genre", "is", FilterValue::Enum("history".into())),
        ],
    );
    click(cx, "ps-trigger");
    // A removal: focus moves to the nearest remaining remove action, so
    // Enter removes the next one.
    click(cx, "ps-sheet-remove-1");
    press(cx, "enter");
    assert!(
        shown(cx, "ps-sheet-row-0") && !shown(cx, "ps-sheet-row-1"),
        "two removals by one aim"
    );
    // A one-tap field adds at once: focus lands on its row, whose tap
    // removes it.
    click(cx, "ps-sheet-add");
    click(cx, "ps-sheet-field-starred");
    assert!(shown(cx, "ps-sheet-row-1"));
    press(cx, "enter");
    assert!(
        !shown(cx, "ps-sheet-row-1"),
        "Enter on the new row removed it"
    );
    // A creation through the value step lands on the header's Add filter.
    click(cx, "ps-sheet-add");
    click(cx, "ps-sheet-field-genre");
    click(cx, "Name(\"ps-sheet-value-is\")-history");
    click(cx, "ps-sheet-confirm");
    press(cx, "enter");
    assert!(
        shown(cx, "ps-sheet-fields"),
        "Enter on Add filter opened it again"
    );
    press(cx, "escape");
    // Done closes the flow; focus returns to the trigger.
    click(cx, "ps-sheet-done");
    assert!(!logged(&log).is_empty());
    press(cx, "enter");
    assert!(
        shown(cx, "ps-sheet-filters"),
        "Enter on the trigger reopens the flow"
    );
}
