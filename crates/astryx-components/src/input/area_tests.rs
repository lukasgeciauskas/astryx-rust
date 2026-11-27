//! TextArea tests, numbered by the upstream spec's FR items.

use astryx_primitives::{Politeness, announce::announcement, announcer};
use gpui::MouseButton;

use super::*;
use crate::input::text_area::{characters_for_test, utf16_limit};

struct AreaHost(Rc<dyn Fn() -> TextArea>);

impl Render for AreaHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_start()
            .p(px(20.))
            .child((self.0)().label_hidden(true).width(px(300.)))
            .child(announcer())
    }
}

/// A 300px, 3-row area with a hidden label: its box spans x 20..320 and
/// y 20..90 (three 20px lines, 4px padding above and below, borders).
fn host(cx: &mut TestAppContext, build: impl Fn() -> TextArea + 'static) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| AreaHost(Rc::new(build)));
    cx.run_until_parked();
    cx
}

fn click(cx: &mut VisualTestContext, x: f32, y: f32) {
    cx.simulate_click(point(px(x), px(y)), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn fr4_typing_and_enter_edit_lines(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || TextArea::new("a", "Notes", probe.value()).on_change(probe.store())
    });
    click(cx, 100., 40.);
    type_text(cx, "one");
    press(cx, "enter");
    type_text(cx, "two");
    assert_eq!(probe.value(), "one\ntwo");
}

#[gpui::test]
fn fr5_fr6_the_counter_announces_each_new_zone(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            TextArea::new("a", "Post", probe.value())
                .max_length(10)
                .on_change(probe.store())
        }
    });
    click(cx, 100., 40.);
    type_text(cx, "1234567");
    let polite = cx.update(|window, cx| announcement(Politeness::Polite, window, cx));
    assert_eq!(polite, "", "under the limit says nothing");
    type_text(cx, "8");
    let polite = cx.update(|window, cx| announcement(Politeness::Polite, window, cx));
    assert_eq!(polite, "2 characters remaining");
    // Typing never stops at the limit: it warns, it does not block.
    type_text(cx, "901");
    assert_eq!(probe.value(), "12345678901");
    let assertive = cx.update(|window, cx| announcement(Politeness::Assertive, window, cx));
    assert_eq!(assertive, "1 character over the limit");
}

#[test]
fn fr5_characters_count_as_people_see_them() {
    // A thumbs-up with a skin tone is two code points, one character.
    assert_eq!(characters_for_test("a\u{1F44D}\u{1F3FD}b"), 3);
}

#[test]
fn fr14_the_hard_limit_counts_utf16_units() {
    let filter = utf16_limit(5);
    assert_eq!(filter("abc", 3..3, "defg"), "de");
    // Replacing a selection frees its room.
    assert_eq!(filter("abcde", 1..4, "xyz"), "xyz");
    // An emoji is two units and is never split.
    assert_eq!(filter("abcd", 4..4, "\u{1F600}"), "");
    assert_eq!(filter("abc", 3..3, "\u{1F600}!"), "\u{1F600}");
}

#[gpui::test]
fn fr14_typing_stops_at_the_hard_limit(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            TextArea::new("a", "Code", probe.value())
                .html_max_length(4)
                .on_change(probe.store())
        }
    });
    click(cx, 100., 40.);
    type_text(cx, "abcdef");
    assert_eq!(probe.value(), "abcd");
}

#[gpui::test]
fn fr9_disabled_areas_take_no_edits_or_pastes(cx: &mut TestAppContext) {
    let probe = Probe::new("fixed");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            let pasted = probe.clone();
            TextArea::new("a", "Terms", probe.value())
                .disabled(true)
                .on_change(probe.store())
                .on_paste(move |text, _, _| pasted.note(&format!("paste:{text}")))
        }
    });
    click(cx, 100., 40.);
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("x".into())));
    press(cx, "secondary-v");
    type_text(cx, "y");
    press(cx, "enter");
    assert!(probe.log().is_empty());
}

#[gpui::test]
fn pastes_are_reported_before_the_change(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            let pasted = probe.clone();
            TextArea::new("a", "Notes", probe.value())
                .on_change(probe.store())
                .on_paste(move |text, _, _| pasted.note(&format!("paste:{text}")))
        }
    });
    click(cx, 100., 40.);
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("hi".into())));
    press(cx, "secondary-v");
    assert_eq!(probe.log(), ["paste:hi", "hi"]);
}

#[gpui::test]
fn focus_and_blur_are_reported(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || {
            let (focused, blurred) = (probe.clone(), probe.clone());
            TextArea::new("a", "Notes", probe.value())
                .on_focus(move |_, _| focused.note("focus"))
                .on_blur(move |_, _| blurred.note("blur"))
        }
    });
    // Focus events reach listeners only in the active window.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    click(cx, 100., 40.);
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    assert_eq!(probe.log(), ["focus", "blur"]);
}

#[gpui::test]
fn fr3_the_grip_resizes_the_area(cx: &mut TestAppContext) {
    let probe = Probe::new("");
    let cx = host(cx, {
        let probe = probe.clone();
        move || TextArea::new("a", "Notes", probe.value()).on_change(probe.store())
    });
    // Below the box: nothing to focus yet.
    click(cx, 100., 115.);
    type_text(cx, "x");
    assert!(probe.log().is_empty());
    // Drag the grip in the bottom end corner 40px down.
    cx.simulate_mouse_down(
        point(px(314.), px(84.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_move(
        point(px(314.), px(124.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_up(
        point(px(314.), px(124.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    click(cx, 100., 115.);
    type_text(cx, "y");
    assert_eq!(probe.value(), "y", "the box now reaches the click");
}
