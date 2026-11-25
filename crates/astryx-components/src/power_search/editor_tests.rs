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
    // Focus the value field and type.
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "tab");
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
