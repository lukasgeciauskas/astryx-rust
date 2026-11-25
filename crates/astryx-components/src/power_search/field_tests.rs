//! PowerSearch field tests (popover presentation), numbered after the
//! spec's FR items.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_primitives::{LayerAlign, Politeness};
use gpui::{
    Bounds, Context, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, point, px, size,
};

use super::field::{ResultCount, editor_geometry};
use super::model::*;
use super::*;

type Log = Rc<RefCell<Vec<String>>>;
type Filters = Rc<RefCell<Vec<PowerSearchFilter>>>;

fn config() -> Rc<PowerSearchConfig> {
    Rc::new(
        PowerSearchConfig::new("books")
            .content_search_field("text")
            .field(
                PowerSearchField::new("text", "Text").operator(PowerSearchOperator::new(
                    "contains",
                    "contains",
                    OperatorValue::string(),
                )),
            )
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
    read_only: bool,
    count: Rc<RefCell<Option<usize>>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (store, log) = (self.filters.clone(), self.log.clone());
        let mut ps = PowerSearch::new(
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
        .read_only(self.read_only)
        .presentation(crate::overlay::PopoverPresentation::Popover);
        if let Some(n) = *self.count.borrow() {
            ps = ps.result_count(ResultCount::Count(n));
        }
        div()
            .p(px(40.))
            .w(px(600.))
            .child(astryx_primitives::announcer())
            .child(ps)
    }
}

struct Setup {
    filters: Filters,
    log: Log,
    count: Rc<RefCell<Option<usize>>>,
}

fn host(
    cx: &mut TestAppContext,
    filters: Vec<PowerSearchFilter>,
    read_only: bool,
) -> (&mut VisualTestContext, Setup) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let setup = Setup {
        filters: Rc::new(RefCell::new(filters)),
        log: Log::default(),
        count: Rc::new(RefCell::new(None)),
    };
    let (f, l, c) = (
        setup.filters.clone(),
        setup.log.clone(),
        setup.count.clone(),
    );
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        filters: f,
        log: l,
        read_only,
        count: c,
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

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .is_some()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    settle(cx);
    let b = cx
        .debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .unwrap_or_else(|| panic!("{selector} is not drawn"));
    cx.simulate_click(point(b.left() + px(8.), b.center().y), Modifiers::none());
    settle(cx);
}

fn announced(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| {
        astryx_primitives::announce::announcement(Politeness::Polite, window, cx).to_string()
    })
}

fn logged(s: &Setup) -> Vec<String> {
    s.log.borrow().clone()
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    settle(cx);
}

#[test]
fn fr1_to_fr5_the_editor_is_400_to_720_wide_at_the_nearer_edge() {
    let outer = |w: f32| Bounds::new(point(px(100.), px(0.)), size(px(w), px(32.)));
    let at = |x: f32| Some(Bounds::new(point(px(x), px(0.)), size(px(40.), px(20.))));
    assert_eq!(
        editor_geometry(outer(300.), at(110.), px(1200.), false),
        (px(400.), LayerAlign::Start),
        "the 400 px floor"
    );
    assert_eq!(
        editor_geometry(outer(300.), at(110.), px(350.), false).0,
        px(350.),
        "the floor yields to a narrow window"
    );
    assert_eq!(
        editor_geometry(outer(600.), at(600.), px(1200.), false),
        (px(600.), LayerAlign::Start),
        "uncapped: the outer width, at its start"
    );
    assert_eq!(
        editor_geometry(outer(1000.), at(120.), px(1400.), false),
        (px(720.), LayerAlign::Start),
        "capped, near the left"
    );
    assert_eq!(
        editor_geometry(outer(1000.), at(1000.), px(1400.), false),
        (px(720.), LayerAlign::End),
        "capped, near the right"
    );
    assert_eq!(
        editor_geometry(outer(1000.), at(1000.), px(1400.), true),
        (px(720.), LayerAlign::Start),
        "logical under RTL"
    );
}

#[gpui::test]
fn a_field_row_opens_the_editor_and_apply_adds_the_filter(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, Vec::new(), false);
    press(cx, "tab");
    type_text(cx, "Title");
    click(cx, "ps-field-option-title");
    assert!(shown(cx, "ps-editor-1-editor"), "the editor opens");
    // Focus is in the value field.
    type_text(cx, "dune");
    press(cx, "enter");
    assert_eq!(logged(&s), ["Add 0 1"]);
    let f = s.filters.borrow()[0].clone();
    assert_eq!(
        (f.field.as_ref(), f.operator.as_ref()),
        ("title", "contains")
    );
    assert_eq!(f.value, FilterValue::String("dune".into()));
    assert!(!shown(cx, "ps-editor-1-editor"), "and closes");
}

#[gpui::test]
fn value_rows_and_empty_operators_add_at_once(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, Vec::new(), false);
    press(cx, "tab");
    type_text(cx, "Genre his");
    click(cx, "ps-field-option-genre:is:value:History");
    assert_eq!(logged(&s), ["Add 0 1"]);
    assert_eq!(
        s.filters.borrow()[0].value,
        FilterValue::Enum("history".into())
    );
    type_text(cx, "Starred");
    click(cx, "ps-field-option-starred");
    assert_eq!(logged(&s).last().unwrap(), "Add 1 2");
    assert_eq!(s.filters.borrow()[1].value, FilterValue::Empty);
}

#[gpui::test]
fn fr24_a_token_opens_its_editor_and_delete_removes_it(cx: &mut TestAppContext) {
    let (cx, s) = host(
        cx,
        vec![PowerSearchFilter::new(
            "title",
            "is",
            FilterValue::String("Emma".into()),
        )],
        false,
    );
    assert!(shown(cx, "ps-token-0"));
    click(cx, "ps-token-0");
    assert!(shown(cx, "ps-editor-1-editor"));
    let delete = cx.debug_bounds("ps-editor-1-delete").unwrap().center();
    cx.simulate_click(delete, Modifiers::none());
    settle(cx);
    assert_eq!(logged(&s), ["Remove 0 0"]);
}

#[gpui::test]
fn fr22_a_read_only_field_removes_nothing(cx: &mut TestAppContext) {
    let (cx, s) = host(
        cx,
        vec![PowerSearchFilter::new(
            "title",
            "is",
            FilterValue::String("Emma".into()),
        )],
        true,
    );
    assert!(!shown(cx, "ps-field-clear"), "no Clear all");
    press(cx, "tab");
    press(cx, "backspace");
    assert!(logged(&s).is_empty(), "Backspace removes nothing");
}

#[gpui::test]
fn fr23_clear_all_keeps_read_only_filters(cx: &mut TestAppContext) {
    let (cx, s) = host(
        cx,
        vec![
            PowerSearchFilter::new("title", "is", FilterValue::String("Emma".into()))
                .read_only(true),
            PowerSearchFilter::new("genre", "is", FilterValue::Enum("fiction".into())),
        ],
        false,
    );
    click(cx, "ps-field-clear");
    assert_eq!(logged(&s), ["Remove 1 1"]);
    assert!(s.filters.borrow()[0].read_only);
    assert_eq!(announced(cx), "Editable filters removed");
}

#[gpui::test]
fn fr19_the_result_count_shows_and_speaks_only_when_it_changes(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, Vec::new(), false);
    *s.count.borrow_mut() = Some(12);
    settle(cx);
    assert!(shown(cx, "ps-result-count"));
    assert_eq!(announced(cx), "12 results");
    *s.count.borrow_mut() = Some(3);
    settle(cx);
    assert_eq!(announced(cx), "3 results");
}

/// A people source that answers after 300 ms and counts its calls.
struct People {
    calls: Rc<RefCell<Vec<String>>>,
}

impl crate::search::SearchSource<crate::search::SearchItem> for People {
    fn search(
        &self,
        query: &str,
        _: &crate::search::SearchContext<crate::search::SearchItem>,
        cx: &mut gpui::App,
    ) -> crate::search::SearchAnswer<crate::search::SearchItem> {
        self.calls.borrow_mut().push(query.to_string());
        let found: Vec<crate::search::SearchItem> = ["Jane Doe", "Janet Lee"]
            .into_iter()
            .filter(|n| n.to_lowercase().contains(&query.to_lowercase()))
            .map(|n| crate::search::SearchItem::new(n.to_lowercase().replace(' ', "-"), n))
            .collect();
        let timer = cx
            .background_executor()
            .timer(std::time::Duration::from_millis(300));
        crate::search::SearchAnswer::Pending(cx.spawn(async move |_| {
            timer.await;
            Ok(found)
        }))
    }

    fn bootstrap(
        &self,
        _: &mut gpui::App,
    ) -> crate::search::SearchAnswer<crate::search::SearchItem> {
        Vec::new().into()
    }
}

struct PeopleHost {
    filters: Filters,
    log: Log,
    calls: Rc<RefCell<Vec<String>>>,
}

impl Render for PeopleHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let source: crate::power_search::model::ValueSource = Rc::new(People {
            calls: self.calls.clone(),
        });
        let config = Rc::new(
            PowerSearchConfig::new("people").field(
                PowerSearchField::new("owner", "Owner").operator(
                    PowerSearchOperator::new(
                        "is",
                        "is",
                        OperatorValue::EntityList {
                            search_source: Some(source),
                            arbitrary_allowed: false,
                            tokenization: None,
                            render_item: None,
                        },
                    )
                    .value_only_match(true),
                ),
            ),
        );
        let (store, log) = (self.filters.clone(), self.log.clone());
        div().p(px(40.)).w(px(600.)).child(
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
            .presentation(crate::overlay::PopoverPresentation::Popover),
        )
    }
}

#[gpui::test]
fn fr28_a_value_only_query_asks_the_operator_source_once_held(cx: &mut TestAppContext) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (filters, log, calls): (Filters, Log, Rc<RefCell<Vec<String>>>) = Default::default();
    let (f, l, c) = (filters.clone(), log.clone(), calls.clone());
    let (_, cx) = cx.add_window_view(move |_, _| PeopleHost {
        filters: f,
        log: l,
        calls: c,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    press(cx, "tab");
    type_text(cx, "jan");
    assert!(calls.borrow().is_empty(), "held for 150 ms");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(160));
    settle(cx);
    assert_eq!(*calls.borrow(), ["jan"], "one call for the query");
    assert!(shown(cx, "ps-field-busy"), "busy until the source settles");
    assert!(!shown(cx, "ps-field-option-owner:is:src:jane-doe"));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(320));
    settle(cx);
    assert!(
        shown(cx, "ps-field-option-owner:is:src:jane-doe"),
        "its rows join"
    );
    assert!(!shown(cx, "ps-field-busy"));
    click(cx, "ps-field-option-owner:is:src:jane-doe");
    assert_eq!(*log.borrow(), ["Add 0 1"]);
    match &filters.borrow()[0].value {
        FilterValue::EntityList(list) => assert_eq!(list[0].label, "Jane Doe"),
        other => panic!("{other:?}"),
    }
}

struct MoreHost {
    filters: Filters,
    log: Log,
}

impl Render for MoreHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let op = |key: &'static str| {
            PowerSearchOperator::new(key, key, OperatorValue::string()).value_only_match(true)
        };
        let config = Rc::new(PowerSearchConfig::new("books").field(
            PowerSearchField::new("title", "Title").operators([
                op("contains"),
                op("starts"),
                op("ends"),
                op("is"),
            ]),
        ));
        let (store, log) = (self.filters.clone(), self.log.clone());
        div().p(px(40.)).w(px(600.)).child(
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
            .presentation(crate::overlay::PopoverPresentation::Popover),
        )
    }
}

#[gpui::test]
fn fr29_a_more_row_opens_the_editor_with_the_typed_text(cx: &mut TestAppContext) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (filters, log): (Filters, Log) = Default::default();
    let (f, l) = (filters.clone(), log.clone());
    let (_, cx) = cx.add_window_view(move |_, _| MoreHost { filters: f, log: l });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    press(cx, "tab");
    type_text(cx, "dune");
    assert!(
        shown(cx, "ps-field-option-__more__:title"),
        "three rows, then More"
    );
    click(cx, "ps-field-option-__more__:title");
    assert!(shown(cx, "ps-editor-1-editor"));
    // The fourth operator's text field holds the typed text: Enter saves it.
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["Add 0 1"]);
    let f = filters.borrow()[0].clone();
    assert_eq!(
        (f.operator.as_ref(), f.value.clone()),
        ("is", FilterValue::String("dune".into()))
    );
}
