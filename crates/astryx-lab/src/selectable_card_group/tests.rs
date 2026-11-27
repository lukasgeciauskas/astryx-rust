//! Upstream `SelectableCardGroup.test.tsx` and the radio keyboard of
//! `SelectableCardGroupRadio.a11y.chromium.spec.ts`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Context, Modifiers, Render, TestAppContext, VisualTestContext, point, px};

use super::*;

const PLANS: [&str; 3] = ["Basic", "Pro", "Team"];

#[test]
fn several_choices_add_the_newest_last_and_drop_a_deselected_one() {
    let values: Vec<SharedString> = vec!["Basic".into()];
    let added = toggle_value(&values, &"Team".into(), true);
    assert_eq!(added, vec![SharedString::from("Basic"), "Team".into()]);
    assert_eq!(
        toggle_value(&added, &"Basic".into(), false),
        vec![SharedString::from("Team")]
    );
    // Choosing a chosen value again moves it last.
    assert_eq!(
        toggle_value(&added, &"Basic".into(), true),
        vec![SharedString::from("Team"), "Basic".into()]
    );
}

type Changes = Rc<RefCell<Vec<SharedString>>>;

/// A single-choice group of the three plans that keeps its own value.
struct Single {
    value: Rc<RefCell<Option<SharedString>>>,
    changes: Changes,
    disabled: bool,
}

impl Render for Single {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (value, changes) = (self.value.clone(), self.changes.clone());
        let view = cx.entity();
        div().w(px(600.)).child(
            SelectableCardGroup::new("plan", "Plan", self.value.borrow().clone())
                .description("Billed monthly")
                .disabled(self.disabled)
                .on_change(move |v, _, cx| {
                    changes.borrow_mut().push(v.clone());
                    *value.borrow_mut() = Some(v);
                    view.update(cx, |_, cx| cx.notify());
                })
                .items(PLANS.map(|plan| {
                    SelectableCardGroupItem::new(plan, plan)
                        .width(px(150.))
                        .child(div().h(px(40.)).child(plan))
                })),
        )
    }
}

type Chosen = Rc<RefCell<Option<SharedString>>>;

fn single<'a>(
    cx: &'a mut TestAppContext,
    initial: Option<&'static str>,
    disabled: bool,
) -> (&'a mut VisualTestContext, Changes, Chosen) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let changes = Changes::default();
    let value = Rc::new(RefCell::new(initial.map(SharedString::from)));
    let (_, cx) = cx.add_window_view({
        let (changes, value) = (changes.clone(), value.clone());
        move |_, _| Single {
            value,
            changes,
            disabled,
        }
    });
    cx.run_until_parked();
    (cx, changes, value)
}

/// The center of card `i` (150 px wide, 12 px apart).
fn card(cx: &mut VisualTestContext, i: usize) -> gpui::Point<gpui::Pixels> {
    let bounds = cx.debug_bounds("plan-group").expect("the group is drawn");
    bounds.origin + point(px(75. + 162. * i as f32), px(30.))
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    cx.run_until_parked();
}

#[gpui::test]
fn choosing_a_card_moves_the_selection_and_reports_its_value(cx: &mut TestAppContext) {
    let (cx, changes, value) = single(cx, Some("Basic"), false);
    let at = card(cx, 2);
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(changes.borrow().as_slice(), [SharedString::from("Team")]);
    assert_eq!(value.borrow().as_deref(), Some("Team"));
}

#[gpui::test]
fn choosing_the_chosen_card_again_changes_nothing(cx: &mut TestAppContext) {
    let (cx, changes, _) = single(cx, Some("Basic"), false);
    let at = card(cx, 0);
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    assert!(changes.borrow().is_empty());
}

#[gpui::test]
fn a_disabled_group_selects_nothing(cx: &mut TestAppContext) {
    let (cx, changes, _) = single(cx, None, true);
    let at = card(cx, 1);
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    assert!(changes.borrow().is_empty());
    // Its cards keep their focus: the arrows reach them, and select nothing.
    press(cx, "tab");
    press(cx, "right");
    assert!(changes.borrow().is_empty());
}

#[gpui::test]
fn one_tab_stop_on_the_chosen_card_and_arrows_move_and_select_wrapping(cx: &mut TestAppContext) {
    let (cx, changes, value) = single(cx, Some("Pro"), false);
    press(cx, "tab");
    press(cx, "right");
    assert_eq!(
        value.borrow().as_deref(),
        Some("Team"),
        "from Pro, the tab stop"
    );
    press(cx, "right");
    assert_eq!(value.borrow().as_deref(), Some("Basic"), "wrapping");
    press(cx, "left");
    assert_eq!(value.borrow().as_deref(), Some("Team"));
    assert_eq!(changes.borrow().len(), 3);
}

#[gpui::test]
fn space_or_enter_selects_the_focused_card(cx: &mut TestAppContext) {
    // Space as a native radio does; Enter as SelectableCard adds it (upstream
    // `SelectableCardRadio.test.tsx`, "selects on Enter when unselected").
    let (cx, changes, value) = single(cx, None, false);
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(value.borrow().as_deref(), Some("Basic"));
    // The chosen card never deselects itself.
    press(cx, "space");
    press(cx, "enter");
    assert_eq!(changes.borrow().len(), 1);
    press(cx, "right");
    press(cx, "space");
    assert_eq!(value.borrow().as_deref(), Some("Pro"));
}

struct Multiple(Rc<RefCell<Vec<SharedString>>>);

impl Render for Multiple {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.0.clone();
        let view = cx.entity();
        div().w(px(600.)).child(
            SelectableCardGroup::multiple("addons", "Add-ons", self.0.borrow().clone())
                .on_change_values(move |v, _, cx| {
                    *value.borrow_mut() = v;
                    view.update(cx, |_, cx| cx.notify());
                })
                .items(PLANS.map(|plan| {
                    SelectableCardGroupItem::new(plan, plan)
                        .width(px(150.))
                        .child(div().h(px(40.)).child(plan))
                })),
        )
    }
}

#[gpui::test]
fn several_choices_are_checkboxes_that_toggle(cx: &mut TestAppContext) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let value = Rc::new(RefCell::new(vec![SharedString::from("Basic")]));
    let (_, cx) = cx.add_window_view({
        let value = value.clone();
        move |_, _| Multiple(value)
    });
    cx.run_until_parked();
    let group = cx.debug_bounds("addons-group").expect("the group is drawn");
    let at = |i: usize| group.origin + point(px(75. + 162. * i as f32), px(30.));
    cx.simulate_click(at(2), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        value.borrow().as_slice(),
        [SharedString::from("Basic"), "Team".into()]
    );
    cx.simulate_click(at(0), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(value.borrow().as_slice(), [SharedString::from("Team")]);
}
