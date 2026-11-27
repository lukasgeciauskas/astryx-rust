//! ChatLayout on the software keyboard: upstream `Chat/useChatKeyboard.test.tsx`, through
//! the visual viewport the test platform shrinks (AST-099 FR3, FR4).

use gpui::{
    Bounds, InteractiveElement as _, IntoElement, ParentElement, Styled, TestAppContext,
    TouchEvent, TouchId, TouchPhase, VisualTestContext, div, point, px, size,
};

use super::*;
use crate::input::TextInput;
use crate::segmented::tests::host;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn chat(cx: &mut TestAppContext) -> &mut VisualTestContext {
    cx.update(astryx_primitives::editing::init);
    let cx = host(cx, move || {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                ChatLayout::new(
                    "chat",
                    div().debug_selector(|| "composer".into()).child(
                        TextInput::new("draft", "Message", "")
                            .label_hidden(true)
                            .width(px(240.)),
                    ),
                )
                .child(
                    ChatMessageList::new("log").child(
                        ChatMessage::new("m0", ChatMessageSender::User)
                            .child(ChatMessageBubble::new().child("Hello")),
                    ),
                ),
            )
            .into_any_element()
    });
    settle(cx);
    cx
}

fn keyboard(cx: &mut VisualTestContext, height: f32) {
    let handle = cx.update(|window, _| window.window_handle());
    let full = cx.update(|window, _| window.viewport_size());
    cx.simulate_window_visual_viewport_change(
        handle,
        Bounds::new(
            point(px(0.), px(0.)),
            size(full.width, full.height - px(height)),
        ),
    );
    settle(cx);
}

#[gpui::test]
fn fr3_the_dock_rests_above_the_keyboard_and_back_once_it_goes(cx: &mut TestAppContext) {
    let cx = chat(cx);
    let full = cx.update(|window, _| window.viewport_size());
    let resting = cx.debug_bounds("chat-dock").expect("the dock");
    keyboard(cx, 300.);
    let lifted = cx.debug_bounds("chat-dock").unwrap();
    assert!(
        f32::from(lifted.bottom()) <= f32::from(full.height) - 300. + 1.,
        "the dock ends at {} over a keyboard from {}",
        f32::from(lifted.bottom()),
        f32::from(full.height) - 300.
    );
    keyboard(cx, 0.);
    let back = cx.debug_bounds("chat-dock").unwrap();
    assert!((f32::from(back.bottom()) - f32::from(resting.bottom())).abs() < 1.);
}

#[gpui::test]
fn fr4_done_parks_focus_on_the_composer_region_under_a_finger(cx: &mut TestAppContext) {
    let cx = chat(cx);
    let composer = cx.debug_bounds("composer").unwrap();
    let at = point(composer.left() + px(20.), composer.center().y);
    for phase in [TouchPhase::Started, TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: at,
            predicted_position: None,
            force: None,
        });
        cx.run_until_parked();
    }
    settle(cx);
    assert!(cx.update(astryx_primitives::keyboard::is_text_entry_focused));
    // The keyboard's Done: focus leaves the field for nothing.
    cx.update(|window, cx| window.blur(cx));
    settle(cx);
    assert!(
        cx.update(|window, cx| window.focused(cx).is_some()),
        "focus parked on the composer region"
    );
    assert!(!cx.update(astryx_primitives::keyboard::is_text_entry_focused));
}

#[gpui::test]
fn fr4_a_desktop_keeps_its_own_focus_and_parks_nothing(cx: &mut TestAppContext) {
    let cx = chat(cx);
    let composer = cx.debug_bounds("composer").unwrap();
    cx.simulate_click(
        point(composer.left() + px(20.), composer.center().y),
        gpui::Modifiers::none(),
    );
    settle(cx);
    cx.update(|window, cx| window.blur(cx));
    settle(cx);
    assert!(cx.update(|window, cx| window.focused(cx).is_none()));
}

#[gpui::test]
fn fr3_the_dock_rests_above_the_platforms_keyboard_inset(cx: &mut TestAppContext) {
    let cx = chat(cx);
    let full = cx.update(|window, _| window.viewport_size());
    cx.simulate_insets_change(gpui::WindowInsets {
        ime: gpui::Edges {
            bottom: px(300.),
            ..Default::default()
        },
        ..Default::default()
    });
    settle(cx);
    let lifted = cx.debug_bounds("chat-dock").unwrap();
    assert!(f32::from(lifted.bottom()) <= f32::from(full.height) - 300. + 1.);
}

// --- ChatPastedTextToken.test.tsx: the tap stop ----------------------------------------

/// A pasted-text token in a stand-in for the composer, which takes focus on a press.
fn token_in_composer(cx: &mut TestAppContext) -> (&mut VisualTestContext, gpui::FocusHandle) {
    let composer = cx.update(|cx| cx.focus_handle());
    let handle = composer.clone();
    let cx = host(cx, move || {
        let focus = handle.clone();
        div()
            .id("composer")
            .debug_selector(|| "stand-in".into())
            .track_focus(&handle)
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            })
            .p(px(20.))
            .child(ChatPastedTextToken::new(
                "pasted",
                "a long pasted text ".repeat(20),
            ))
            .into_any_element()
    });
    settle(cx);
    (cx, composer)
}

#[gpui::test]
fn a_finger_tap_on_the_token_stops_there_and_the_composer_takes_no_focus(cx: &mut TestAppContext) {
    let (cx, composer) = token_in_composer(cx);
    let at = cx
        .debug_bounds("pasted-tap-stop")
        .expect("the token")
        .center();
    for phase in [TouchPhase::Started, TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: at,
            predicted_position: None,
            force: None,
        });
        cx.run_until_parked();
    }
    settle(cx);
    assert!(!cx.update(|window, _| composer.is_focused(window)));
}

#[gpui::test]
fn a_mouse_press_on_the_token_still_reaches_the_composer(cx: &mut TestAppContext) {
    let (cx, composer) = token_in_composer(cx);
    let at = cx
        .debug_bounds("pasted-tap-stop")
        .expect("the token")
        .center();
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
    assert!(cx.update(|window, _| composer.is_focused(window)));
}

#[gpui::test]
fn a_finger_tap_beside_the_token_still_focuses_the_composer(cx: &mut TestAppContext) {
    let (cx, composer) = token_in_composer(cx);
    let stand_in = cx.debug_bounds("stand-in").expect("the composer");
    let at = point(stand_in.left() + px(5.), stand_in.top() + px(5.));
    for phase in [TouchPhase::Started, TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: at,
            predicted_position: None,
            force: None,
        });
        cx.run_until_parked();
    }
    settle(cx);
    assert!(cx.update(|window, _| composer.is_focused(window)));
}
