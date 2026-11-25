//! PowerSearchFilterEditor tests.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    Context, IntoElement, Modifiers, ParentElement, Render, SharedString, Styled, TestAppContext,
    VisualTestContext, Window, div, px,
};

use super::editor::{EditorMode, PowerSearchFilterEditor};
use super::model::*;

type Log = Rc<RefCell<Vec<String>>>;

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
    log: Log,
    filter: PartialFilter,
    mode: EditorMode,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (save, cancel) = (self.log.clone(), self.log.clone());
        div()
            .p(px(40.))
            .w(px(600.))
            .child(PowerSearchFilterEditor::new(
                "e",
                config(),
                self.filter.clone(),
                self.mode,
                move |filter, _, _| {
                    save.borrow_mut().push(match filter {
                        Some(f) => format!("save {} {} {:?}", f.field, f.operator, f.value),
                        None => "delete".into(),
                    })
                },
                move |_, _| cancel.borrow_mut().push("cancel".into()),
            ))
    }
}

fn host(
    cx: &mut TestAppContext,
    filter: PartialFilter,
    mode: EditorMode,
) -> (&mut VisualTestContext, Log) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let log = Log::default();
    let l = log.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        log: l,
        filter,
        mode,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, log)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
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

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    settle(cx);
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn partial(field: &str, operator: Option<&str>, value: Option<FilterValue>) -> PartialFilter {
    PartialFilter {
        field: SharedString::from(field.to_string()),
        operator: operator.map(|o| SharedString::from(o.to_string())),
        value,
        read_only: false,
    }
}

#[gpui::test]
fn a_complete_draft_applies_and_enter_saves(cx: &mut TestAppContext) {
    let (cx, log) = host(
        cx,
        partial("title", Some("contains"), None),
        EditorMode::Create,
    );
    // Focus starts in the value field.
    cx.simulate_input("dune");
    settle(cx);
    press(cx, "enter");
    assert_eq!(
        log.borrow().last().unwrap(),
        "save title contains String(\"dune\")"
    );
}

#[gpui::test]
fn escape_cancels_and_edit_mode_deletes(cx: &mut TestAppContext) {
    let (cx, log) = host(
        cx,
        partial(
            "title",
            Some("is"),
            Some(FilterValue::String("Emma".into())),
        ),
        EditorMode::Edit,
    );
    click(cx, "e-apply");
    assert_eq!(
        log.borrow().last().unwrap(),
        "save title is String(\"Emma\")"
    );
    press(cx, "tab");
    press(cx, "escape");
    assert_eq!(log.borrow().last().unwrap(), "cancel");
}

#[gpui::test]
fn an_enum_pick_saves_at_once(cx: &mut TestAppContext) {
    let (cx, log) = host(cx, partial("genre", Some("is"), None), EditorMode::Create);
    click(cx, "e-value-genre-is-value-control");
    click(cx, "e-value-genre-is-value-option-history");
    assert_eq!(
        log.borrow().last().unwrap(),
        "save genre is Enum(\"history\")"
    );
}

#[gpui::test]
fn a_field_whose_operator_takes_no_value_saves_when_chosen(cx: &mut TestAppContext) {
    let (cx, log) = host(
        cx,
        partial("title", Some("contains"), None),
        EditorMode::Create,
    );
    click(cx, "e-field-control");
    click(cx, "e-field-option-starred");
    assert_eq!(log.borrow().last().unwrap(), "save starred is_true Empty");
}

// ---- Dates and times (FR25, FR26) ------------------------------------------

fn dates_config() -> Rc<PowerSearchConfig> {
    Rc::new(
        PowerSearchConfig::new("events")
            .field(
                PowerSearchField::new("when", "When").operator(PowerSearchOperator::new(
                    "on",
                    "on",
                    OperatorValue::DateAbsolute { date_only: None },
                )),
            )
            .field(
                PowerSearchField::new("at", "At").operator(PowerSearchOperator::new(
                    "at",
                    "at",
                    OperatorValue::DateAbsolute {
                        date_only: Some(false),
                    },
                )),
            )
            .field(
                PowerSearchField::new("start", "Start").operator(PowerSearchOperator::new(
                    "is",
                    "is",
                    OperatorValue::Time {
                        min: None,
                        max: None,
                    },
                )),
            )
            .field(
                PowerSearchField::new("span", "Span").operator(PowerSearchOperator::new(
                    "in",
                    "in",
                    OperatorValue::DateRange {
                        interval_presets: Vec::new(),
                        relative_presets: Vec::new(),
                    },
                )),
            ),
    )
}

struct ZoneHost {
    log: Log,
    filter: PartialFilter,
    zone: Option<&'static str>,
}

impl Render for ZoneHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let save = self.log.clone();
        let mut editor = PowerSearchFilterEditor::new(
            "e",
            dates_config(),
            self.filter.clone(),
            EditorMode::Create,
            move |filter, _, _| {
                save.borrow_mut().push(match filter {
                    Some(f) => format!("save {} {} {:?}", f.field, f.operator, f.value),
                    None => "delete".into(),
                })
            },
            |_, _| {},
        );
        if let Some(zone) = self.zone {
            editor = editor.timezone_id(zone);
        }
        div().p(px(40.)).w(px(600.)).child(editor)
    }
}

fn zone_host<'a>(
    cx: &'a mut TestAppContext,
    filter: PartialFilter,
    zone: Option<&'static str>,
) -> (&'a mut VisualTestContext, Log) {
    astryx_i18n::date::set_today(astryx_i18n::date::PlainDate::new(2026, 11, 25));
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let log = Log::default();
    let l = log.clone();
    let (_, cx) = cx.add_window_view(move |_, _| ZoneHost {
        log: l,
        filter,
        zone,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, log)
}

/// Types into the date field: focus starts on its calendar toggle.
fn type_date(cx: &mut VisualTestContext, text: &str) {
    press(cx, "tab");
    cx.simulate_input(text);
    settle(cx);
}

#[gpui::test]
fn fr25_a_picked_day_is_its_midnight_in_timezone_id(cx: &mut TestAppContext) {
    let (cx, log) = zone_host(
        cx,
        partial("when", Some("on"), None),
        Some("America/Los_Angeles"),
    );
    type_date(cx, "3/5/2024");
    click(cx, "e-apply");
    // 2024-03-05T00:00-08:00.
    assert_eq!(
        log.borrow().last().unwrap(),
        "save when on DateAbsolute { unix_seconds: 1709625600 }"
    );
}

#[gpui::test]
fn fr25_without_a_zone_a_day_is_its_utc_midnight(cx: &mut TestAppContext) {
    let (cx, log) = zone_host(cx, partial("when", Some("on"), None), None);
    type_date(cx, "3/5/2024");
    click(cx, "e-apply");
    assert_eq!(
        log.borrow().last().unwrap(),
        "save when on DateAbsolute { unix_seconds: 1709596800 }"
    );
}

#[gpui::test]
fn fr25_reopening_a_day_keeps_its_instant(cx: &mut TestAppContext) {
    // 2024-03-05T03:00Z: the 4th in Los Angeles, stored before the zone.
    let stored = FilterValue::DateAbsolute {
        unix_seconds: 1_709_607_600,
    };
    let (cx, log) = zone_host(
        cx,
        partial("when", Some("on"), Some(stored)),
        Some("America/Los_Angeles"),
    );
    click(cx, "e-apply");
    assert_eq!(
        log.borrow().last().unwrap(),
        "save when on DateAbsolute { unix_seconds: 1709607600 }",
        "nothing rewrites the value"
    );
}

#[gpui::test]
fn fr26_an_instant_takes_the_wall_clock_in_timezone_id(cx: &mut TestAppContext) {
    let (cx, log) = zone_host(cx, partial("at", Some("at"), None), Some("UTC"));
    type_date(cx, "3/5/2024");
    assert!(log.borrow().is_empty(), "the date waits for a time");
    press(cx, "tab");
    cx.simulate_input("2:30 PM");
    settle(cx);
    click(cx, "e-apply");
    assert_eq!(
        log.borrow().last().unwrap(),
        "save at at DateAbsolute { unix_seconds: 1709649000 }"
    );
}

#[gpui::test]
fn a_time_value_saves_its_iso_time(cx: &mut TestAppContext) {
    let (cx, log) = zone_host(cx, partial("start", Some("is"), None), None);
    cx.simulate_input("2:30 PM");
    settle(cx);
    click(cx, "e-apply");
    assert_eq!(
        log.borrow().last().unwrap(),
        "save start is Time(\"14:30\")"
    );
}

#[gpui::test]
fn fr25_a_picked_range_stores_each_day_s_midnight(cx: &mut TestAppContext) {
    let (cx, log) = zone_host(cx, partial("span", Some("in"), None), None);
    click(cx, "e-value-span-in-value-trigger");
    let day = |d: u8| -> &'static str {
        Box::leak(
            crate::calendar::day_selector(
                &"e-value-span-in-value-calendar".into(),
                astryx_i18n::date::PlainDate::new(2026, 11, d).unwrap(),
                false,
            )
            .into_boxed_str(),
        )
    };
    click(cx, day(10));
    click(cx, day(12));
    click(cx, "e-apply");
    assert_eq!(
        log.borrow().last().unwrap(),
        "save span in DateRange(DateTimeRange { start: Absolute { unix_seconds: 1794268800 }, end: Absolute { unix_seconds: 1794441600 } })"
    );
}
