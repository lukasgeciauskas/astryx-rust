//! DropdownMenu and MoreMenu tests, numbered by the upstream spec's FR
//! items.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, Context, IntoElement, Modifiers, MouseButton, ParentElement, Pixels, Point, Render,
    Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::*;
use crate::button::Button;

type Log = Rc<RefCell<Vec<String>>>;

struct Host {
    build: Rc<dyn Fn() -> AnyElement>,
    log: Log,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = self.log.clone();
        div()
            .flex()
            .flex_col()
            .items_start()
            .pt(px(120.))
            .pl(px(120.))
            .child((self.build)())
            .child(
                Button::new("after", "After")
                    .on_click(move |_, _, _| log.borrow_mut().push("after".into())),
            )
    }
}

/// The trigger sits at x 120, y 120 in a 1024 × 768 window.
fn host<'a>(
    cx: &'a mut TestAppContext,
    log: &Log,
    build: impl Fn() -> AnyElement + 'static,
) -> &'a mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let log = log.clone();
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        build: Rc::new(build),
        log,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
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

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    settle(cx);
}

const TRIGGER: (f32, f32) = (130., 130.);

fn at(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

fn row(cx: &mut VisualTestContext, selector: &str) -> Option<Point<Pixels>> {
    settle(cx);
    cx.debug_bounds(Box::leak(selector.to_string().into_boxed_str()))
        .map(|b| b.center())
}

fn shown(cx: &mut VisualTestContext, selector: &str) -> bool {
    row(cx, selector).is_some()
}

fn logged(log: &Log) -> Vec<String> {
    log.borrow().clone()
}

fn action(id: &'static str, label: &'static str, log: &Log) -> MenuEntry {
    let log = log.clone();
    MenuItem::new(id, label)
        .on_click(move |_, _| log.borrow_mut().push(id.into()))
        .into()
}

fn entries(log: &Log) -> Vec<MenuEntry> {
    vec![
        action("newest", "Newest", log),
        action("oldest", "Oldest", log),
        MenuItem::new("archived", "Archived").disabled(true).into(),
        MenuEntry::Divider,
        action("delete", "Delete", log),
    ]
}

fn menu(log: &Log) -> impl Fn() -> AnyElement + 'static {
    let log = log.clone();
    move || {
        DropdownMenu::new("m")
            .button_label("Sort")
            .items(entries(&log))
            .into_any_element()
    }
}

fn open(cx: &mut VisualTestContext) -> bool {
    shown(cx, "m-list")
}

fn keyboard_open(cx: &mut VisualTestContext) {
    press(cx, "tab");
    press(cx, "enter");
    assert!(open(cx), "Enter on the trigger opens the menu");
}

#[gpui::test]
fn fr8_a_press_down_opens_the_menu_and_its_click_keeps_it(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, menu(&log));
    assert!(!open(cx));
    cx.simulate_click(at(TRIGGER.0, TRIGGER.1), Modifiers::none());
    assert!(
        open(cx),
        "the press opened it, and its own click did not close it"
    );
    cx.simulate_click(at(TRIGGER.0, TRIGGER.1), Modifiers::none());
    assert!(!open(cx), "pressing the trigger of an open menu closes it");
    cx.simulate_click(at(TRIGGER.0, TRIGGER.1), Modifiers::none());
    assert!(open(cx), "and the next press opens it again");
}

#[gpui::test]
fn fr5_a_drag_from_the_trigger_releases_on_a_row_and_picks_it(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, menu(&log));
    cx.simulate_mouse_down(
        at(TRIGGER.0, TRIGGER.1),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    let oldest = row(cx, "m-list-oldest").expect("the press opened the menu");
    cx.simulate_mouse_move(oldest, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(oldest, MouseButton::Left, Modifiers::none());
    settle(cx);
    assert_eq!(logged(&log), ["oldest"]);
    assert!(!open(cx), "picking closes the menu");
}

#[gpui::test]
fn fr5_the_opening_release_acts_only_once_it_settled(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, menu(&log));
    // Released at once, off the trigger: nothing is picked or closed.
    cx.simulate_mouse_down(
        at(TRIGGER.0, TRIGGER.1),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    cx.simulate_mouse_up(at(900., 700.), MouseButton::Left, Modifiers::none());
    settle(cx);
    assert!(open(cx), "an unsettled release leaves the menu open");
    // Close, then hold past the settle time and release outside.
    cx.simulate_click(at(TRIGGER.0, TRIGGER.1), Modifiers::none());
    assert!(!open(cx));
    cx.simulate_mouse_down(
        at(TRIGGER.0, TRIGGER.1),
        MouseButton::Left,
        Modifiers::none(),
    );
    wait(cx, 400);
    cx.simulate_mouse_move(at(900., 700.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at(900., 700.), MouseButton::Left, Modifiers::none());
    settle(cx);
    assert!(!open(cx), "a settled mouse release outside closes it");
    assert!(logged(&log).is_empty());
}

#[gpui::test]
fn fr6_a_keyboard_pick_returns_focus_to_the_trigger(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, menu(&log));
    keyboard_open(cx);
    press(cx, "enter");
    assert_eq!(logged(&log), ["newest"], "focus entered on the first row");
    assert!(!open(cx));
    press(cx, "enter");
    assert!(open(cx), "focus was back on the trigger");
}

#[gpui::test]
fn fr12_arrows_wrap_and_arrow_up_on_the_trigger_opens_on_the_last_row(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, menu(&log));
    press(cx, "tab");
    press(cx, "up");
    assert!(open(cx), "ArrowUp opens the menu");
    press(cx, "enter");
    assert_eq!(logged(&log), ["delete"], "on the last enabled row");
    press(cx, "down");
    assert!(open(cx), "ArrowDown opens it too");
    press(cx, "up");
    press(cx, "enter");
    assert_eq!(
        logged(&log),
        ["delete", "delete"],
        "ArrowUp on the first row wrapped"
    );
}

#[gpui::test]
fn fr12_typeahead_jumps_to_a_row(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, menu(&log));
    keyboard_open(cx);
    press(cx, "o");
    press(cx, "enter");
    assert_eq!(logged(&log), ["oldest"]);
}

#[gpui::test]
fn fr17_a_disabled_row_is_reached_and_stays_inert(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, menu(&log));
    keyboard_open(cx);
    press(cx, "down");
    press(cx, "down");
    press(cx, "enter");
    assert!(logged(&log).is_empty(), "the disabled row did nothing");
    assert!(open(cx), "and left the menu open");
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(logged(&log), ["delete"], "it was a row like any other");
}

#[gpui::test]
fn fr23_tab_closes_the_menu_and_moves_on_from_the_trigger(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, menu(&log));
    keyboard_open(cx);
    press(cx, "tab");
    assert!(!open(cx));
    press(cx, "enter");
    assert_eq!(logged(&log), ["after"], "focus moved to the next control");
}

#[gpui::test]
fn checkbox_rows_stay_open_and_radio_rows_close(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, {
        let log = log.clone();
        move || {
            let (check, radio) = (log.clone(), log.clone());
            DropdownMenu::new("m")
                .button_label("View")
                .entry(
                    MenuCheckboxItem::new("wrap", "Word wrap", false)
                        .on_change(move |on, _, _| check.borrow_mut().push(format!("wrap:{on}"))),
                )
                .entry(
                    MenuRadioGroup::new("light")
                        .title("Theme")
                        .on_change(move |v, _, _| radio.borrow_mut().push(format!("theme:{v}")))
                        .item(MenuRadioItem::new("light", "Light"))
                        .item(MenuRadioItem::new("dark", "Dark")),
                )
                .into_any_element()
        }
    });
    keyboard_open(cx);
    press(cx, "enter");
    assert_eq!(logged(&log), ["wrap:true"]);
    assert!(open(cx), "a checkbox row keeps the menu open");
    press(cx, "down");
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(logged(&log), ["wrap:true", "theme:dark"]);
    assert!(!open(cx), "a radio row closes it");
}

fn with_submenu(log: &Log) -> impl Fn() -> AnyElement + 'static {
    let log = log.clone();
    move || {
        DropdownMenu::new("m")
            .button_label("Actions")
            .entry(
                MenuItem::new("move", "Move to")
                    .submenu([action("inbox", "Inbox", &log), action("spam", "Spam", &log)]),
            )
            .items([action("copy", "Copy", &log)])
            .into_any_element()
    }
}

#[gpui::test]
fn submenus_open_with_arrow_right_and_close_with_arrow_left(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, with_submenu(&log));
    keyboard_open(cx);
    press(cx, "right");
    assert!(shown(cx, "m-list-move-sub"), "the flyout opened");
    press(cx, "left");
    assert!(!shown(cx, "m-list-move-sub"), "ArrowLeft closed it");
    assert!(open(cx), "and kept the menu");
    press(cx, "right");
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(
        logged(&log),
        ["spam"],
        "focus entered the flyout on its first row"
    );
    assert!(!open(cx), "picking in a flyout closes the whole menu");
}

#[gpui::test]
fn escape_in_a_flyout_closes_only_the_flyout(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, with_submenu(&log));
    keyboard_open(cx);
    press(cx, "enter");
    assert!(
        shown(cx, "m-list-move-sub"),
        "Enter on the row opens its flyout"
    );
    press(cx, "escape");
    assert!(!shown(cx, "m-list-move-sub"));
    assert!(open(cx));
    press(cx, "escape");
    assert!(!open(cx));
}

#[gpui::test]
fn fr13_hover_opens_a_flyout_after_the_delay(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, with_submenu(&log));
    cx.simulate_click(at(TRIGGER.0, TRIGGER.1), Modifiers::none());
    let move_row = row(cx, "m-list-move").expect("open");
    cx.simulate_mouse_move(move_row, None, Modifiers::none());
    wait(cx, 100);
    assert!(!shown(cx, "m-list-move-sub"), "not before 150 ms");
    wait(cx, 100);
    assert!(shown(cx, "m-list-move-sub"), "open after 150 ms");
    // Leaving for another row closes it after 200 ms.
    let copy = row(cx, "m-list-copy").expect("open");
    cx.simulate_mouse_move(copy, None, Modifiers::none());
    wait(cx, 100);
    assert!(shown(cx, "m-list-move-sub"), "not yet");
    wait(cx, 150);
    assert!(!shown(cx, "m-list-move-sub"), "closed");
}

#[gpui::test]
fn fr13_the_safe_triangle_keeps_the_flyout_open(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, with_submenu(&log));
    cx.simulate_click(at(TRIGGER.0, TRIGGER.1), Modifiers::none());
    let move_row = row(cx, "m-list-move").expect("open");
    cx.simulate_mouse_move(move_row, None, Modifiers::none());
    wait(cx, 200);
    let spam = row(cx, "m-list-move-sub-spam").expect("the flyout opened");
    // Head down-right toward the flyout's lower rows, crossing Copy.
    let copy = row(cx, "m-list-copy").expect("open");
    let flyout = cx.debug_bounds("m-list-move-sub").expect("open");
    let toward = point(flyout.left() - px(6.), copy.y);
    assert!(
        in_safe_triangle(toward, move_row, flyout),
        "the test point is on the way to the flyout"
    );
    cx.simulate_mouse_move(toward, None, Modifiers::none());
    wait(cx, 400);
    assert!(
        shown(cx, "m-list-move-sub"),
        "travelling toward it keeps it"
    );
    // The same distance straight down, outside the triangle, closes it.
    let away = point(move_row.x, copy.y + px(1.));
    cx.simulate_mouse_move(away, None, Modifiers::none());
    wait(cx, 400);
    assert!(
        !shown(cx, "m-list-move-sub"),
        "leaving the triangle closes it"
    );
    cx.simulate_mouse_move(move_row, None, Modifiers::none());
    wait(cx, 200);
    cx.simulate_mouse_move(spam, None, Modifiers::none());
    cx.simulate_click(spam, Modifiers::none());
    settle(cx);
    assert_eq!(logged(&log), ["spam"]);
}

#[gpui::test]
fn fr15_a_header_makes_the_menu_a_dialog_that_keeps_tab(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, {
        let log = log.clone();
        move || {
            DropdownMenu::new("m")
                .button_label("Sort")
                .header(div().child("Sort by"))
                .items(entries(&log))
                .into_any_element()
        }
    });
    keyboard_open(cx);
    press(cx, "tab");
    assert!(open(cx), "Tab stays inside a menu with a header");
}

#[gpui::test]
fn the_sheet_drills_into_submenus(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, {
        let log = log.clone();
        move || {
            DropdownMenu::new("m")
                .button_label("Actions")
                .presentation(PopoverPresentation::BottomSheet)
                .entry(
                    MenuItem::new("move", "Move to")
                        .submenu([action("inbox", "Inbox", &log), action("spam", "Spam", &log)]),
                )
                .into_any_element()
        }
    });
    cx.simulate_click(at(TRIGGER.0, TRIGGER.1), Modifiers::none());
    wait(cx, 1000);
    let move_row = row(cx, "m-list-move").expect("the sheet shows the rows");
    cx.simulate_click(move_row, Modifiers::none());
    wait(cx, 300);
    let spam = row(cx, "m-list-move-drill-spam").expect("the submenu replaced the rows");
    assert!(!shown(cx, "m-list-move"), "the parent rows are gone");
    cx.simulate_click(spam, Modifiers::none());
    settle(cx);
    assert_eq!(logged(&log), ["spam"]);
}

#[gpui::test]
fn more_menu_opens_from_its_icon_button(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, {
        let log = log.clone();
        move || MoreMenu::new("m", entries(&log)).into_any_element()
    });
    cx.simulate_click(at(130., 130.), Modifiers::none());
    assert!(open(cx));
    let delete = row(cx, "m-list-delete").expect("open");
    cx.simulate_click(delete, Modifiers::none());
    settle(cx);
    assert_eq!(logged(&log), ["delete"]);
}

// ----------------------------------------------------------------------------
// ContextMenu

fn context(log: &Log, disabled: bool) -> impl Fn() -> AnyElement + 'static {
    let log = log.clone();
    move || {
        ContextMenu::new("c")
            .disabled(disabled)
            .items(entries(&log))
            .child(
                div()
                    .w(px(240.))
                    .h(px(120.))
                    .child(Button::new("inside", "Inside")),
            )
            .into_any_element()
    }
}

fn context_open(cx: &mut VisualTestContext) -> bool {
    shown(cx, "c-list")
}

fn right_click(cx: &mut VisualTestContext, p: Point<Pixels>) {
    cx.simulate_mouse_down(p, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(p, MouseButton::Right, Modifiers::none());
    settle(cx);
}

#[gpui::test]
fn context_fr1_a_right_press_opens_the_menu_at_the_pointer(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, context(&log, false));
    assert!(!context_open(cx));
    right_click(cx, at(300., 200.));
    let list = cx.debug_bounds("c-list").expect("the menu opened");
    assert!(
        (f32::from(list.left()) - 300.).abs() <= 8. && (f32::from(list.top()) - 200.).abs() <= 8.,
        "it opened at the pointer, at {list:?}"
    );
    let oldest = row(cx, "c-list-oldest").expect("open");
    cx.simulate_click(oldest, Modifiers::none());
    settle(cx);
    assert_eq!(logged(&log), ["oldest"]);
    assert!(!context_open(cx));
}

#[gpui::test]
fn context_fr1_a_disabled_area_opens_nothing(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, context(&log, true));
    right_click(cx, at(300., 200.));
    assert!(!context_open(cx));
}

#[gpui::test]
fn context_fr5_a_held_right_press_picks_the_row_it_is_released_on(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, context(&log, false));
    cx.simulate_mouse_down(at(300., 200.), MouseButton::Right, Modifiers::none());
    settle(cx);
    let delete = row(cx, "c-list-delete").expect("the press opened it");
    cx.simulate_mouse_move(delete, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(delete, MouseButton::Right, Modifiers::none());
    settle(cx);
    assert_eq!(logged(&log), ["delete"]);
}

#[gpui::test]
fn context_fr5_a_press_outside_closes_it(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, context(&log, false));
    right_click(cx, at(300., 200.));
    assert!(context_open(cx));
    cx.simulate_click(at(900., 700.), Modifiers::none());
    assert!(!context_open(cx));
}

#[gpui::test]
fn context_shift_f10_opens_on_the_first_row_and_escape_returns_focus(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, context(&log, false));
    press(cx, "tab");
    press(cx, "shift-f10");
    assert!(context_open(cx), "Shift+F10 in the area opens it");
    press(cx, "escape");
    assert!(!context_open(cx));
    press(cx, "shift-f10");
    press(cx, "enter");
    assert_eq!(logged(&log), ["newest"], "focus entered on the first row");
}

#[gpui::test]
fn context_fr11_tab_leaves_the_menu(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, context(&log, false));
    press(cx, "tab");
    press(cx, "shift-f10");
    press(cx, "tab");
    assert!(!context_open(cx));
}

// ----------------------------------------------------------------------------
// A text field's context menu

struct Speller(Rc<RefCell<Vec<String>>>);

impl astryx_primitives::spell::SpellChecker for Speller {
    fn check(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        let added = self.0.borrow();
        astryx_primitives::spell::words(text)
            .filter(|(_, w)| *w == "teh" && !added.iter().any(|a| a == w))
            .map(|(r, _)| r)
            .collect()
    }
    fn suggest(&self, _: &str) -> Vec<String> {
        vec!["the".into(), "ten".into()]
    }
    fn add(&self, word: &str) {
        self.0.borrow_mut().push(word.into());
    }
}

fn field(
    log: &Log,
    added: &Rc<RefCell<Vec<String>>>,
    cx: &mut TestAppContext,
) -> impl Fn() -> AnyElement + 'static {
    let speller = Rc::new(Speller(added.clone()));
    cx.update(|cx| {
        astryx_primitives::editing::init(cx);
        astryx_primitives::spell::set_spell_checker(Some(speller), cx);
    });
    let log = log.clone();
    let value: Rc<RefCell<String>> = Rc::new(RefCell::new("teh cat".into()));
    move || {
        let (log, value) = (log.clone(), value.clone());
        let shown = value.borrow().clone();
        crate::input::TextInput::new("name", "Name", shown)
            .width(px(300.))
            .on_change(move |text, _, _| {
                *value.borrow_mut() = text.to_string();
                log.borrow_mut().push(text.to_string());
            })
            .into_any_element()
    }
}

#[gpui::test]
fn a_text_field_offers_spelling_suggestions_on_a_right_press(cx: &mut TestAppContext) {
    let log = Log::default();
    let added = Rc::default();
    let build = field(&log, &added, cx);
    let cx = host(cx, &log, build);
    let control = cx.debug_bounds("name-control").expect("the field");
    // On "teh", the first word.
    right_click(cx, point(control.left() + px(18.), control.center().y));
    let the = row(cx, "name-menu-list-suggestion-0").expect("a suggestion for the word");
    assert!(shown(cx, "name-menu-list-add-to-dictionary"));
    cx.simulate_click(the, Modifiers::none());
    settle(cx);
    assert_eq!(
        logged(&log),
        ["the cat"],
        "the suggestion replaced the word"
    );
}

#[gpui::test]
fn add_to_dictionary_accepts_the_word(cx: &mut TestAppContext) {
    let log = Log::default();
    let added: Rc<RefCell<Vec<String>>> = Rc::default();
    let build = field(&log, &added, cx);
    let cx = host(cx, &log, build);
    let control = cx.debug_bounds("name-control").expect("the field");
    let on_word = point(control.left() + px(18.), control.center().y);
    right_click(cx, on_word);
    let add = row(cx, "name-menu-list-add-to-dictionary").expect("open");
    cx.simulate_click(add, Modifiers::none());
    settle(cx);
    assert_eq!(*added.borrow(), ["teh"]);
    right_click(cx, on_word);
    assert!(
        !shown(cx, "name-menu-list-suggestion-0"),
        "a word in the dictionary has no suggestions"
    );
    assert!(
        shown(cx, "name-menu-list-select-all"),
        "the clipboard commands remain"
    );
}

#[gpui::test]
fn a_breadcrumb_item_with_a_menu_opens_it(cx: &mut TestAppContext) {
    use crate::breadcrumbs::{BreadcrumbItem, Breadcrumbs};
    let log = Log::default();
    let cx = host(cx, &log, {
        let log = log.clone();
        move || {
            Breadcrumbs::new("trail")
                .item(BreadcrumbItem::new("Home").href("/"))
                .item(BreadcrumbItem::new("Projects").menu(entries(&log)))
                .item(BreadcrumbItem::new("Apollo"))
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    press(cx, "down");
    press(cx, "enter");
    assert_eq!(
        logged(&log),
        ["oldest"],
        "the item opened its menu on its first row"
    );
}

#[gpui::test]
fn a_submenu_reports_opening_and_a_header_keeps_tab_in_its_flyout(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, {
        let log = log.clone();
        move || {
            let report = log.clone();
            DropdownMenu::new("m")
                .button_label("Actions")
                .entry(
                    MenuItem::new("move", "Move to")
                        .on_submenu_open_change(move |open, _, _| {
                            report.borrow_mut().push(format!("sub:{open}"))
                        })
                        .submenu_header(|_, _| div().child("Folders").into_any_element())
                        .submenu([action("inbox", "Inbox", &log)]),
                )
                .into_any_element()
        }
    });
    keyboard_open(cx);
    press(cx, "right");
    assert!(shown(cx, "m-list-move-sub"));
    assert_eq!(logged(&log), ["sub:true"]);
    press(cx, "tab");
    assert!(
        shown(cx, "m-list-move-sub"),
        "Tab stays in a flyout with a header"
    );
    press(cx, "escape");
    assert!(!shown(cx, "m-list-move-sub"));
    assert_eq!(logged(&log), ["sub:true", "sub:false"]);
    assert!(open(cx));
}

#[gpui::test]
fn a_drill_in_submenu_replaces_the_rows_behind_a_back_row(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, &log, {
        let log = log.clone();
        move || {
            DropdownMenu::new("m")
                .button_label("Actions")
                .entry(
                    MenuItem::new("move", "Move to")
                        .submenu_presentation(SubmenuPresentation::DrillIn)
                        .submenu([action("inbox", "Inbox", &log)]),
                )
                .into_any_element()
        }
    });
    keyboard_open(cx);
    press(cx, "enter");
    assert!(
        shown(cx, "m-list-move-drill-back"),
        "a Back row leads the drilled rows"
    );
    assert!(!shown(cx, "m-list-move"), "in place of the menu's rows");
    press(cx, "escape");
    assert!(shown(cx, "m-list-move"), "Escape stepped back");
    assert!(open(cx), "and kept the menu open");
    press(cx, "enter");
    press(cx, "enter");
    assert_eq!(
        logged(&log),
        ["inbox"],
        "a keyboard drill lands on the first row after Back"
    );
}
