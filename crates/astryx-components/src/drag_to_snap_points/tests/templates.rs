//! Upstream `snapTemplates.test.tsx` (design:bidirectionality DR13): the two
//! playing-card examples run their places in reading order, so a card sent to the
//! next place moves toward the inline end, rightward or leftward, and its place has
//! the same name; without a provider, the places follow the direction the card renders
//! in. The places' buttons are pressed from the keyboard (Tab, then Enter), and where
//! the card landed is read from the announcement. Left out: the plain-JavaScript
//! example's DOM teardown and inline-style checks (its card and buttons take the
//! theme's tokens and Button here).

use astryx_i18n::{Direction, I18n};
use astryx_primitives::announce::announcement;
use astryx_primitives::{Politeness, announcer};
use gpui::{IntoElement, ParentElement, TestAppContext, VisualTestContext, div};

use super::harness::*;
use crate::drag_to_snap_points::examples::{self, places_for_test};

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

fn page(cx: &mut TestAppContext, dir: Option<Direction>) -> &mut VisualTestContext {
    host(cx, move |window, cx| {
        let cards = examples::examples(window, cx);
        let page = div().child(announcer()).child(cards);
        match dir {
            Some(dir) => astryx_i18n::provider(I18n::new("en").dir(dir), page).into_any_element(),
            None => page.into_any_element(),
        }
    })
}

/// Presses the hook card's place button `index` (in reading order) and lets it land.
fn send_to(cx: &mut VisualTestContext, index: usize) -> String {
    cx.update(|window, cx| {
        window.blur(cx);
        window.refresh();
    });
    settle(cx);
    for _ in 0..=index {
        press(cx, "tab");
    }
    press(cx, "enter");
    // The live region clears a while after it speaks: keep what it said.
    let mut said = String::new();
    for _ in 0..190 {
        frames(cx, 16);
        let now = cx.update(|window, cx| announcement(Politeness::Polite, window, cx).to_string());
        if !now.is_empty() {
            said = now;
        }
    }
    said
}

#[test]
fn the_places_run_in_reading_order() {
    // Left to right the snap points ascend from the hand; right to left they are
    // negative, so the hand is the last of them.
    let (points, place_at) = places_for_test(false);
    assert_eq!(points, [0., 110., 220.]);
    assert_eq!((0..3).map(&place_at).collect::<Vec<_>>(), [0, 1, 2]);
    let (points, place_at) = places_for_test(true);
    assert_eq!(points, [-0., -110., -220.]);
    // Sorted ascending they are [-220, -110, 0]: index 0 is the discard pile.
    assert_eq!((0..3).map(&place_at).collect::<Vec<_>>(), [2, 1, 0]);
}

#[gpui::test]
fn left_to_right_the_next_place_is_named_the_same(cx: &mut TestAppContext) {
    let cx = page(cx, Some(Direction::Ltr));
    assert!(send_to(cx, 1).contains("The card is on the table."));
    assert!(send_to(cx, 2).contains("The card is on the discard pile."));
    assert!(send_to(cx, 0).contains("The card is on the hand."));
}

#[gpui::test]
fn right_to_left_the_next_place_is_named_the_same(cx: &mut TestAppContext) {
    let cx = page(cx, Some(Direction::Rtl));
    assert!(send_to(cx, 1).contains("The card is on the table."));
    assert!(send_to(cx, 2).contains("The card is on the discard pile."));
    assert!(send_to(cx, 0).contains("The card is on the hand."));
}
