//! The Chat lab parts: upstream `ChatTypingIndicator.test.tsx`, `ChatUnreadDivider.test.tsx`,
//! `ChatEmojiPicker.test.tsx` and `ChatReactionBar.test.tsx`.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_components::button::Button;
use astryx_i18n::I18n;
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Modifiers, Render, SharedString, TestAppContext, VisualTestContext,
    Window, div, px,
};

use super::emoji_picker::PICKERS;
use super::reaction_bar::BARS;
use super::typing_indicator::{ANNOUNCED, TYPING};
use super::unread_divider::DIVIDERS;
use super::*;

type Build = Rc<dyn Fn() -> AnyElement>;

struct Host(Build);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(40.)).child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> (gpui::Entity<Host>, &mut VisualTestContext) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (view, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (view, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn rebuild(
    view: &gpui::Entity<Host>,
    cx: &mut VisualTestContext,
    build: impl Fn() -> AnyElement + 'static,
) {
    view.update(cx, |h, cx| {
        h.0 = Rc::new(build);
        cx.notify();
    });
    settle(cx);
}

// ---- ChatTypingIndicator ----------------------------------------------------------

fn label_for(names: &[&str], i18n: &I18n) -> Option<String> {
    let names: Vec<SharedString> = names
        .iter()
        .map(|n| SharedString::from(n.to_string()))
        .collect();
    typing_label(&names, i18n, i18n.locale()).map(|s| s.to_string())
}

#[test]
fn renders_x_is_typing_for_one_name() {
    assert_eq!(
        label_for(&["Ana"], &I18n::new("en")).as_deref(),
        Some("Ana is typing…")
    );
}

#[test]
fn renders_both_names_for_two_people() {
    assert_eq!(
        label_for(&["Ana", "Ben"], &I18n::new("en")).as_deref(),
        Some("Ana and Ben are typing…")
    );
}

#[test]
fn collapses_three_or_more_names_to_and_n_others() {
    assert_eq!(
        label_for(&["Ana", "Ben", "Casey"], &I18n::new("en")).as_deref(),
        Some("Ana and 2 others are typing…")
    );
}

#[test]
fn localizes_the_one_name_sentence_through_the_catalog() {
    let fr = I18n::new("fr").overrides(
        "fr",
        [(
            "@astryx.chatTypingIndicator.one",
            "{name} est en train d’écrire…",
        )],
    );
    assert_eq!(
        label_for(&["Ana"], &fr).as_deref(),
        Some("Ana est en train d’écrire…")
    );
}

#[test]
fn joins_two_names_with_the_locales_list_format() {
    let fr = I18n::new("fr").overrides(
        "fr",
        [("@astryx.chatTypingIndicator.many", "{names} écrivent…")],
    );
    // The French list joins with "et", not the English "and".
    assert_eq!(
        label_for(&["Ana", "Ben"], &fr).as_deref(),
        Some("Ana et Ben écrivent…")
    );
}

#[test]
fn localizes_the_collapsed_overflow_sentence() {
    let fr = I18n::new("fr").overrides(
        "fr",
        [
            (
                "@astryx.chatTypingIndicator.others",
                "{count, number} autres",
            ),
            ("@astryx.chatTypingIndicator.many", "{names} écrivent…"),
        ],
    );
    assert_eq!(
        label_for(&["Ana", "Ben", "Casey"], &fr).as_deref(),
        Some("Ana et 2 autres écrivent…")
    );
}

#[test]
fn renders_no_label_without_names() {
    assert_eq!(label_for(&[], &I18n::new("en")), None);
}

#[test]
fn bounces_each_dot_up_three_px_at_thirty_percent_after_its_delay() {
    // Before its delay a dot rests at its own style.
    assert_eq!(dot_frame(100., 160., 500.), (0., 1.));
    // 0 %: resting, faint; 30 %: up 3 px, opaque; 60 % on: resting, faint.
    assert_eq!(dot_frame(0., 0., 500.), (0., 0.35));
    let (y, o) = dot_frame(150., 0., 500.);
    assert!((y + 3.).abs() < 1e-6 && (o - 1.).abs() < 1e-6);
    assert_eq!(dot_frame(300., 0., 500.), (0., 0.35));
    assert_eq!(dot_frame(450., 0., 500.), (0., 0.35));
    // The cycle repeats, each dot on its own delay.
    let (y2, _) = dot_frame(150. + 160. + 500., 160., 500.);
    assert!((y2 + 3.).abs() < 1e-6);
}

fn typing<'a>(
    cx: &'a mut TestAppContext,
    names: Vec<&'static str>,
) -> (gpui::Entity<Host>, &'a mut VisualTestContext) {
    ANNOUNCED.with(|a| a.borrow_mut().clear());
    host(cx, move || {
        ChatTypingIndicator::new("typing")
            .names(names.clone())
            .into_any_element()
    })
}

fn announced() -> Vec<String> {
    ANNOUNCED.with(|a| a.borrow().iter().map(|s| s.to_string()).collect())
}

#[gpui::test]
fn renders_the_dots_and_the_label(cx: &mut TestAppContext) {
    let (_view, cx) = typing(cx, vec!["Ana"]);
    let (label, dots) = TYPING.with(|t| t.borrow()["typing"].clone());
    assert_eq!(label.as_deref(), Some("Ana is typing…"));
    assert_eq!(dots, 3);
    assert!(cx.debug_bounds("typing-dot-0").is_some());
    assert!(cx.debug_bounds("typing-typing-indicator").is_some());
}

#[gpui::test]
fn renders_dots_only_when_names_is_empty(cx: &mut TestAppContext) {
    let (_view, cx) = typing(cx, vec![]);
    assert_eq!(TYPING.with(|t| t.borrow()["typing"].0.clone()), None);
    assert!(cx.debug_bounds("typing-dot-2").is_some());
}

#[gpui::test]
fn announces_a_mounted_label_once_through_the_polite_channel(cx: &mut TestAppContext) {
    let (view, cx) = typing(cx, vec!["Ana"]);
    rebuild(&view, cx, || {
        ChatTypingIndicator::new("typing")
            .names(["Ana"])
            .into_any_element()
    });
    assert_eq!(announced(), ["Ana is typing…"]);
}

#[gpui::test]
fn announces_each_new_label_nothing_on_clear_and_the_same_people_again(cx: &mut TestAppContext) {
    let (view, cx) = typing(cx, vec![]);
    assert!(announced().is_empty());
    for names in [vec!["Ana"], vec!["Ana", "Ben"], vec![], vec!["Ana", "Ben"]] {
        rebuild(&view, cx, move || {
            ChatTypingIndicator::new("typing")
                .names(names.clone())
                .into_any_element()
        });
    }
    assert_eq!(
        announced(),
        [
            "Ana is typing…",
            "Ana and Ben are typing…",
            "Ana and Ben are typing…"
        ]
    );
}

#[gpui::test]
fn leaves_delivery_to_an_enclosing_live_container(cx: &mut TestAppContext) {
    ANNOUNCED.with(|a| a.borrow_mut().clear());
    let (view, cx) = host(cx, || {
        provide_live_container(ChatTypingIndicator::new("typing").names(["Ana"])).into_any_element()
    });
    rebuild(&view, cx, || {
        provide_live_container(ChatTypingIndicator::new("typing").names(["Ana", "Ben"]))
            .into_any_element()
    });
    assert!(announced().is_empty());
}

// ---- ChatUnreadDivider ------------------------------------------------------------

#[gpui::test]
fn shows_the_default_label_and_names_the_separator(cx: &mut TestAppContext) {
    let (_view, cx) = host(cx, || ChatUnreadDivider::new("divider").into_any_element());
    assert!(cx.debug_bounds("divider").is_some());
    let (label, name) = DIVIDERS.with(|d| d.borrow()["divider"].clone());
    assert_eq!(
        (label.as_ref(), name.as_ref()),
        ("New", "New messages below")
    );
}

#[gpui::test]
fn shows_a_custom_label(cx: &mut TestAppContext) {
    let (_view, _cx) = host(cx, || {
        ChatUnreadDivider::new("divider")
            .label("Unread")
            .into_any_element()
    });
    let (label, name) = DIVIDERS.with(|d| d.borrow()["divider"].clone());
    assert_eq!(
        (label.as_ref(), name.as_ref()),
        ("Unread", "Unread messages below")
    );
}

// ---- ChatEmojiPicker -------------------------------------------------------------

type Picks = Rc<RefCell<Vec<SharedString>>>;

fn picker(
    cx: &mut TestAppContext,
    emojis: Option<Vec<ChatEmojiOption>>,
) -> (Picks, &mut VisualTestContext) {
    let picks = Picks::default();
    let p = picks.clone();
    let (_view, cx) = host(cx, move || {
        let p = p.clone();
        let mut picker = ChatEmojiPicker::new("picker")
            .on_select(move |emoji, _, _| p.borrow_mut().push(emoji))
            .child(
                div()
                    .debug_selector(|| "trigger".into())
                    .child(Button::new("open", "Open picker")),
            );
        if let Some(e) = &emojis {
            picker = picker.emojis(e.clone());
        }
        picker.into_any_element()
    });
    (picks, cx)
}

fn drawn_picker() -> super::emoji_picker::PickerDrawn {
    PICKERS.with(|p| p.borrow()["picker"].clone())
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
}

fn open_picker(cx: &mut VisualTestContext) {
    // The trigger's box spans the row; its button starts at its start.
    let at = cx
        .debug_bounds("trigger")
        .expect("the trigger is drawn")
        .origin
        + gpui::point(px(10.), px(10.));
    cx.simulate_click(at, Modifiers::none());
    settle(cx);
    // The surface's entrance runs out before the grid is pressed.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    settle(cx);
    assert!(drawn_picker().open, "the trigger opens the picker");
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

fn focused_cell(cx: &mut VisualTestContext) -> Option<usize> {
    let handles = drawn_picker().handles;
    cx.update(|window, _| handles.iter().position(|h| h.is_focused(window)))
}

#[gpui::test]
fn renders_the_trigger_closed(cx: &mut TestAppContext) {
    let (_picks, cx) = picker(cx, None);
    assert!(cx.debug_bounds("trigger").is_some());
    assert!(!drawn_picker().open);
}

#[gpui::test]
fn renders_the_default_emoji_set_in_the_grid(cx: &mut TestAppContext) {
    let (_picks, cx) = picker(cx, None);
    open_picker(cx);
    let cells = drawn_picker().cells;
    assert_eq!(cells.len(), default_chat_emojis().len());
    assert!(
        cells
            .iter()
            .any(|(e, l)| e.as_ref() == "🎉" && l.as_ref() == "React with tada")
    );
}

#[gpui::test]
fn renders_custom_emojis_when_provided(cx: &mut TestAppContext) {
    let (_picks, cx) = picker(cx, Some(vec![ChatEmojiOption::new("🦄", "unicorn")]));
    open_picker(cx);
    assert_eq!(
        drawn_picker().cells,
        [(
            SharedString::from("🦄"),
            SharedString::from("React with unicorn")
        )]
    );
}

#[gpui::test]
fn filters_the_grid_by_shortname(cx: &mut TestAppContext) {
    let (_picks, cx) = picker(cx, None);
    open_picker(cx);
    cx.simulate_input("fire");
    settle(cx);
    let cells = drawn_picker().cells;
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].0.as_ref(), "🔥");
}

#[gpui::test]
fn shows_a_no_match_message_for_an_empty_filter_result(cx: &mut TestAppContext) {
    let (_picks, cx) = picker(cx, None);
    open_picker(cx);
    cx.simulate_input("zzzz");
    settle(cx);
    let d = drawn_picker();
    assert!(d.cells.is_empty());
    assert_eq!(d.empty.as_deref(), Some("No emoji match “zzzz”."));
}

#[gpui::test]
fn calls_on_select_with_the_picked_emoji_and_closes(cx: &mut TestAppContext) {
    let (picks, cx) = picker(cx, None);
    open_picker(cx);
    click(cx, "picker-emoji-3");
    assert_eq!(*picks.borrow(), [SharedString::from("🎉")]);
    assert!(!drawn_picker().open, "picking closes the popover");
}

#[gpui::test]
fn clears_the_filter_after_selection(cx: &mut TestAppContext) {
    let (_picks, cx) = picker(cx, None);
    open_picker(cx);
    cx.simulate_input("fire");
    settle(cx);
    click(cx, "picker-emoji-0");
    open_picker(cx);
    assert_eq!(drawn_picker().cells.len(), default_chat_emojis().len());
}

#[gpui::test]
fn moves_focus_right_and_left_with_the_arrow_keys(cx: &mut TestAppContext) {
    let (_picks, cx) = picker(cx, None);
    open_picker(cx);
    let first = drawn_picker().handles[0].clone();
    cx.update(|window, cx| window.focus(&first, cx));
    settle(cx);
    press(cx, "right");
    assert_eq!(focused_cell(cx), Some(1));
    press(cx, "left");
    assert_eq!(focused_cell(cx), Some(0));
}

#[gpui::test]
fn moves_focus_by_a_row_with_down_and_up(cx: &mut TestAppContext) {
    let (_picks, cx) = picker(cx, None);
    open_picker(cx);
    let first = drawn_picker().handles[0].clone();
    cx.update(|window, cx| window.focus(&first, cx));
    settle(cx);
    press(cx, "down");
    assert_eq!(focused_cell(cx), Some(8));
    press(cx, "up");
    assert_eq!(focused_cell(cx), Some(0));
}

#[test]
fn filters_by_shortname_or_the_emoji_itself() {
    let all = default_chat_emojis();
    let names = |q: &str| -> Vec<String> {
        visible_chat_emojis(&all, q)
            .iter()
            .map(|o| o.name.to_string())
            .collect()
    };
    assert_eq!(names("").len(), 16);
    assert_eq!(names("  HEART "), ["heart", "heart eyes"]);
    assert_eq!(names("🔥"), ["fire"]);
    assert!(names("zzzz").is_empty());
}

// ---- ChatReactionBar -------------------------------------------------------------

fn reactions() -> Vec<ChatReaction> {
    vec![
        ChatReaction::new("🎉", 4)
            .selected(true)
            .label("You and Dana reacted with 🎉"),
        ChatReaction::new("👀", 2),
    ]
}

type Log = Rc<RefCell<Vec<(&'static str, SharedString)>>>;

fn bar(cx: &mut TestAppContext, add: bool) -> (Log, &mut VisualTestContext) {
    let log = Log::default();
    let l = log.clone();
    let (_view, cx) = host(cx, move || {
        let (toggled, added) = (l.clone(), l.clone());
        let mut bar = ChatReactionBar::new("bar", reactions())
            .on_toggle(move |emoji, _, _| toggled.borrow_mut().push(("toggle", emoji)));
        if add {
            bar = bar.on_add(move |emoji, _, _| added.borrow_mut().push(("add", emoji)));
        }
        bar.into_any_element()
    });
    (log, cx)
}

fn drawn_bar() -> super::reaction_bar::BarDrawn {
    BARS.with(|b| b.borrow()["bar"].clone())
}

#[gpui::test]
fn renders_a_pill_per_reaction_with_its_count_and_name(cx: &mut TestAppContext) {
    let (_log, _cx) = bar(cx, false);
    let pills = drawn_bar().pills;
    assert_eq!(pills[0].0.as_ref(), "You and Dana reacted with 🎉");
    assert_eq!(pills[1].0.as_ref(), "2 reactions with 👀");
    assert_eq!((pills[0].2, pills[1].2), (4, 2));
    // A labelled pill carries a tooltip.
    assert_eq!((pills[0].3, pills[1].3), (true, false));
}

#[gpui::test]
fn reflects_is_selected_as_pressed(cx: &mut TestAppContext) {
    let (_log, _cx) = bar(cx, false);
    let pills = drawn_bar().pills;
    assert_eq!((pills[0].1, pills[1].1), (true, false));
}

#[test]
fn uses_a_singular_name_for_a_single_reaction() {
    assert_eq!(
        pill_label(&ChatReaction::new("🔥", 1)).as_ref(),
        "1 reaction with 🔥"
    );
}

#[gpui::test]
fn calls_on_toggle_with_the_pills_emoji(cx: &mut TestAppContext) {
    let (log, cx) = bar(cx, false);
    click(cx, "bar-pill-👀");
    assert_eq!(*log.borrow(), [("toggle", SharedString::from("👀"))]);
}

#[gpui::test]
fn renders_as_a_labelled_group(cx: &mut TestAppContext) {
    let (_log, _cx) = bar(cx, false);
    assert_eq!(drawn_bar().label.as_ref(), "Reactions");
}

#[gpui::test]
fn hides_the_add_button_without_on_add(cx: &mut TestAppContext) {
    let (_log, cx) = bar(cx, false);
    assert_eq!(drawn_bar().add, None);
    assert!(cx.debug_bounds("bar-add").is_none());
}

#[gpui::test]
fn shows_the_add_button_with_on_add(cx: &mut TestAppContext) {
    let (_log, cx) = bar(cx, true);
    assert_eq!(drawn_bar().add.as_deref(), Some("Add reaction"));
    assert!(cx.debug_bounds("bar-add").is_some());
}

#[gpui::test]
fn calls_on_add_with_the_emoji_picked_from_the_picker(cx: &mut TestAppContext) {
    let (log, cx) = bar(cx, true);
    click(cx, "bar-add");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    settle(cx);
    let fire = PICKERS
        .with(|p| {
            p.borrow()["bar-picker"]
                .cells
                .iter()
                .position(|(e, _)| e.as_ref() == "🔥")
        })
        .expect("the picker opened");
    click(cx, &format!("bar-picker-emoji-{fire}"));
    assert_eq!(*log.borrow(), [("add", SharedString::from("🔥"))]);
}

// ---- The gallery -----------------------------------------------------------------

struct Gallery;

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(super::examples::examples(window, cx))
    }
}

#[gpui::test]
fn the_gallery_renders_and_anchors_its_history_to_the_first_message(cx: &mut TestAppContext) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (_view, cx) = cx.add_window_view(|_, _| Gallery);
    cx.simulate_resize(gpui::size(px(800.), px(2400.)));
    settle(cx);
    settle(cx);
    let (_, shell) =
        super::virtualizer::list_of("chat-example-virtualizer").expect("the history drew");
    let first = shell
        .borrow()
        .painted
        .first()
        .cloned()
        .expect("rows painted");
    assert_eq!(
        first.0.as_ref(),
        "0",
        "the history opens at its first message"
    );
}
