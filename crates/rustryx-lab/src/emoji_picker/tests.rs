//! Upstream `emojiData.test.ts` (the bundled set) and `EmojiPicker.test.tsx` (the
//! groups, search, picking, the grid's keys and the footer).

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Context, Modifiers, Render, TestAppContext, VisualTestContext};
use rustryx_i18n::{Direction, I18n};

use super::*;

#[test]
fn the_bundled_set_lists_every_cldr_group_but_the_component_group_in_order() {
    let names: Vec<&str> = EMOJI_GROUPS.iter().map(|g| g.name).collect();
    assert_eq!(
        names,
        [
            "Smileys & emotion",
            "People & body",
            "Animals & nature",
            "Food & drink",
            "Travel & places",
            "Activities",
            "Objects",
            "Symbols",
            "Flags"
        ]
    );
}

#[test]
fn the_bundled_set_names_every_emoji_once_with_no_skin_tone_variants() {
    let emojis: Vec<&EmojiDatum> = EMOJI_GROUPS.iter().flat_map(|g| g.emojis.iter()).collect();
    assert!(emojis.len() > 1800);
    let unique: std::collections::HashSet<&str> = emojis.iter().map(|e| e.0).collect();
    assert_eq!(unique.len(), emojis.len());
    assert!(emojis.iter().all(|e| !e.1.trim().is_empty()));
    // Skin tones are U+1F3FB through U+1F3FF.
    assert!(!emojis.iter().any(|e| {
        e.0.chars()
            .any(|c| ('\u{1F3FB}'..='\u{1F3FF}').contains(&c))
    }));
}

#[test]
fn search_matches_every_word_as_the_start_of_a_word() {
    let cat = EmojiOption::new("🐱", "Cat face").keywords(["pet"]);
    let vacation = EmojiOption::new("🏖", "Vacation");
    assert!(emoji_matches(&cat, &query_words("cat")));
    assert!(!emoji_matches(&vacation, &query_words("cat")));
    assert!(emoji_matches(&cat, &query_words("  FACE   pe ")));
    assert!(!emoji_matches(&cat, &query_words("cat dog")));
    let flag = EmojiOption::new("🇺🇸", "Flag: United States");
    assert!(emoji_matches(&flag, &query_words("states")));
    assert!(emoji_matches(&flag, &query_words("flag")));
}

type Picks = Rc<RefCell<Vec<(SharedString, SharedString)>>>;

struct Host {
    frequent: Option<Vec<EmojiOption>>,
    categories: Option<Vec<EmojiCategory>>,
    auto_focus: bool,
    rtl: bool,
    picks: Picks,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let picks = self.picks.clone();
        let mut picker = EmojiPicker::new("picker")
            .auto_focus(self.auto_focus)
            .on_select(move |emoji, option, _, _| {
                picks.borrow_mut().push((emoji, option.name.clone()))
            });
        if let Some(f) = &self.frequent {
            picker = picker.frequent_emojis(f.clone());
        }
        if let Some(c) = &self.categories {
            picker = picker.categories(c.clone());
        }
        let el = div().w(px(352.)).child(picker);
        if self.rtl {
            rustryx_i18n::provider(I18n::new("en").dir(Direction::Rtl), el).into_any_element()
        } else {
            el.into_any_element()
        }
    }
}

fn small() -> Vec<EmojiCategory> {
    vec![
        EmojiCategory::new(
            "Animals",
            [
                EmojiOption::new("🐱", "Cat face").keywords(["pet", "kitten"]),
                EmojiOption::new("🐶", "Dog face").keywords(["pet", "puppy"]),
                EmojiOption::new("🐭", "Mouse face"),
            ],
        ),
        EmojiCategory::new(
            "Places",
            (0..10).map(|i| EmojiOption::new(format!("P{i}"), format!("Place {i}"))),
        ),
    ]
}

fn mount(
    cx: &mut TestAppContext,
    frequent: Option<Vec<EmojiOption>>,
    categories: Option<Vec<EmojiCategory>>,
    auto_focus: bool,
    rtl: bool,
) -> (&mut VisualTestContext, Picks) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
    });
    let picks = Picks::default();
    let (_, cx) = cx.add_window_view({
        let picks = picks.clone();
        move |_, _| Host {
            frequent,
            categories,
            auto_focus,
            rtl,
            picks,
        }
    });
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    (cx, picks)
}

fn drawn() -> Drawn {
    DRAWN
        .with(|d| d.borrow().get("picker").cloned())
        .expect("the picker drew")
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

fn focused_cell(cx: &mut VisualTestContext) -> Option<usize> {
    let handles = drawn().handles;
    cx.update(|window, _| handles.iter().position(|h| h.is_focused(window)))
}

fn cell(cx: &mut VisualTestContext, index: usize) -> gpui::Point<gpui::Pixels> {
    let selector: &'static str = Box::leak(format!("picker-cell-{index}").into_boxed_str());
    cx.debug_bounds(selector)
        .expect("the cell is drawn")
        .center()
}

#[gpui::test]
fn lists_the_frequent_emoji_then_every_group_as_a_grid_under_its_heading(cx: &mut TestAppContext) {
    let _ = mount(cx, None, None, false, false);
    let d = drawn();
    let labels: Vec<_> = d.sections.iter().map(|(l, _)| l.to_string()).collect();
    assert_eq!(labels[0], "Frequently used");
    assert_eq!(
        labels.len(),
        10,
        "the frequent row and the nine bundled groups"
    );
    assert_eq!(d.sections[0].1.len(), 8);
    assert_eq!(d.sections[1].1.len(), EMOJI_GROUPS[0].emojis.len());
    // Every section's last row is padded to the grid's eight columns.
    assert_eq!(d.cells.len() % COLUMNS, 0);
    assert_eq!(
        d.cells.iter().flatten().count(),
        d.sections.iter().map(|(_, e)| e.len()).sum::<usize>()
    );
}

#[gpui::test]
fn leaves_the_frequent_row_out_when_it_is_empty(cx: &mut TestAppContext) {
    let _ = mount(cx, Some(Vec::new()), Some(small()), false, false);
    let labels: Vec<_> = drawn()
        .sections
        .iter()
        .map(|(l, _)| l.to_string())
        .collect();
    assert_eq!(labels, ["Animals", "Places"]);
}

#[gpui::test]
fn calls_on_select_with_the_emoji_and_its_option(cx: &mut TestAppContext) {
    let (cx, picks) = mount(cx, Some(Vec::new()), Some(small()), false, false);
    let at = cell(cx, 1);
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        *picks.borrow(),
        [(SharedString::from("🐶"), SharedString::from("Dog face"))]
    );
}

#[gpui::test]
fn searches_names_and_keywords_by_word_start_and_announces_when_nothing_matches(
    cx: &mut TestAppContext,
) {
    let (cx, _) = mount(cx, Some(Vec::new()), Some(small()), true, false);
    cx.simulate_input("pet");
    cx.run_until_parked();
    let d = drawn();
    assert_eq!(d.sections.len(), 1);
    assert_eq!(d.sections[0].0.as_ref(), "Search results");
    assert_eq!(d.sections[0].1, [SharedString::from("🐱"), "🐶".into()]);
    cx.simulate_input("zzz");
    cx.run_until_parked();
    let d = drawn();
    assert!(d.sections.is_empty());
    assert_eq!(d.status.as_deref(), Some("No emoji match “petzzz”"));
}

#[gpui::test]
fn picks_the_first_match_with_enter_in_the_search_field(cx: &mut TestAppContext) {
    let (cx, picks) = mount(cx, Some(Vec::new()), Some(small()), true, false);
    cx.simulate_keystrokes("d o g");
    cx.run_until_parked();
    press(cx, "enter");
    assert_eq!(
        picks.borrow().first().map(|p| p.0.clone()),
        Some("🐶".into())
    );
}

#[gpui::test]
fn keeps_one_tab_stop_across_the_groups_and_moves_it_with_the_arrow_keys(cx: &mut TestAppContext) {
    let (cx, _) = mount(cx, Some(Vec::new()), Some(small()), true, false);
    // Down from the search field enters the grid at its first emoji.
    press(cx, "down");
    assert_eq!(focused_cell(cx), Some(0));
    press(cx, "right");
    assert_eq!(focused_cell(cx), Some(1));
    // Down moves a row: from the first group's row (3 emoji padded to 8) into the next
    // group's first row, the same column.
    press(cx, "down");
    assert_eq!(focused_cell(cx), Some(9));
    assert_eq!(drawn().tab_stop, Some(9));
    press(cx, "end");
    assert_eq!(focused_cell(cx), Some(15));
    press(cx, "up");
    // The cell above (index 7) is padding: up continues to the next focusable cell in
    // that column, and past the first row returns to the search field.
    assert_eq!(focused_cell(cx), None);
}

#[gpui::test]
fn mirrors_the_arrow_keys_in_right_to_left_layouts(cx: &mut TestAppContext) {
    let (cx, _) = mount(cx, Some(Vec::new()), Some(small()), true, true);
    press(cx, "down");
    press(cx, "left");
    assert_eq!(
        focused_cell(cx),
        Some(1),
        "left moves forward right to left"
    );
    press(cx, "right");
    assert_eq!(focused_cell(cx), Some(0));
}

#[gpui::test]
fn names_the_emoji_under_the_pointer_or_focus_in_its_footer(cx: &mut TestAppContext) {
    let (cx, _) = mount(cx, Some(Vec::new()), Some(small()), false, false);
    assert_eq!(drawn().preview, None);
    let at = cell(cx, 2);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(drawn().preview.as_deref(), Some("Mouse face"));
}

#[gpui::test]
fn loads_the_bundled_set_when_no_categories_are_passed(cx: &mut TestAppContext) {
    let _ = mount(cx, Some(Vec::new()), None, false, false);
    let d = drawn();
    assert_eq!(d.sections.len(), 9);
    assert_eq!(d.sections[0].0.as_ref(), "Smileys & emotion");
}

struct Gallery;

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(super::examples::examples(window, cx))
    }
}

#[gpui::test]
fn the_gallery_renders(cx: &mut TestAppContext) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
    });
    let (_view, cx) = cx.add_window_view(|_, _| Gallery);
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(DRAWN.with(|d| d.borrow().contains_key("emoji-example-inline")));
    assert!(DRAWN.with(|d| d.borrow().contains_key("emoji-example-custom")));
}

#[gpui::test]
fn keeps_a_groups_heading_at_the_top_while_its_emoji_scroll_under_it(cx: &mut TestAppContext) {
    let (cx, _) = mount(cx, None, None, false, false);
    let heading = |cx: &mut VisualTestContext, key: &str| {
        let selector: &'static str = Box::leak(format!("picker-heading-{key}").into_boxed_str());
        cx.debug_bounds(selector).expect("the heading is drawn")
    };
    // The list's top: where the first heading rests.
    let top = heading(cx, "frequent").top();
    let rest = heading(cx, "group-0");
    assert!(rest.top() > top);
    let at = rest.center();
    cx.simulate_event(gpui::ScrollEvent {
        position: gpui::point(at.x, at.y + px(60.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-200.))),
        modifiers: Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
        source: gpui::ScrollSource::Pointer,
    });
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    // Scrolled 200 px into a group that runs far longer: its heading holds the top.
    let scrolled = heading(cx, "group-0");
    assert!(
        (scrolled.top() - top).abs() < px(1.),
        "{top:?}: {rest:?} then {scrolled:?}"
    );
}
