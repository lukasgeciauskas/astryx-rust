//! The pull-to-refresh gallery (upstream's `Lab/Pull to refresh` stories): an inbox in
//! the app's own scroll container, a controlled refresh behind a button, and a short list.

use std::time::Duration;

use gpui::{
    AnyElement, App, Entity, InteractiveElement, IntoElement, ParentElement, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px, relative,
};
use rustryx_components::button::Button;
use rustryx_components::layout::{SpacingStep, Stack};
use rustryx_components::list::{List, ListItem};
use rustryx_components::status::{Spinner, SpinnerShade, SpinnerSize};
use rustryx_components::text::{Heading, HeadingLevel};

use super::{RefreshControl, RefreshControlOptions, use_refresh_control};

/// How long a story's refresh takes.
const REFRESH_MS: u64 = 1500;

fn messages(id: &str, rows: usize, count: usize) -> impl IntoElement {
    List::new(SharedString::from(format!("{id}-list")))
        .header(Heading::new(
            HeadingLevel::H4,
            format!("Inbox (refreshed {count} times)"),
        ))
        .children((0..rows).map(|index| {
            ListItem::new(
                SharedString::from(format!("{id}-message-{index}")),
                format!("Message {}", index + 1),
            )
            .description(if index == 0 {
                format!("Refresh {count}")
            } else {
                "Pull down at the top to refresh".into()
            })
        }))
}

/// What an app writes: its own scroll container, the content the control pulls, and
/// its own indicator, here Core's Spinner, in a band at the bottom of an
/// overflow-hidden box on the content's top edge.
fn pull_to_refresh(
    id: &str,
    control: &RefreshControl,
    handle: &ScrollHandle,
    text: SharedString,
    always_scrollable: bool,
    children: impl IntoElement,
    window: &Window,
) -> impl IntoElement {
    let tall = window.viewport_size().height;
    let indicator = div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(relative(1.))
        .h(tall)
        .overflow_hidden()
        .child(
            control.indicator(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    // The app's indicator shows as much of itself as the pull reveals.
                    .opacity(control.reveal() as f32)
                    .child(
                        Spinner::new(SharedString::from(format!("{id}-spinner")))
                            .size(SpinnerSize::Lg)
                            .shade(SpinnerShade::Subtle)
                            .label(text),
                    ),
            ),
        );
    let mut content = div().relative().child(children).child(indicator);
    if always_scrollable {
        // The pull applies only in a container that overflows: one pixel more than it.
        content = content.min_h(px(361.));
    }
    div()
        .id(SharedString::from(format!("{id}-scroller")))
        .h(px(360.))
        .w_full()
        .max_w(px(480.))
        .overflow_y_scroll()
        .overscroll_y_contain()
        .track_scroll(handle)
        .child(control.content(content))
}

/// A refresh that counts itself once it is done.
fn counting(count: &Entity<usize>) -> RefreshControlOptions {
    let count = count.clone();
    RefreshControlOptions::new()
        .announce("Refreshing")
        .on_refresh(move |window, cx| {
            let count = count.clone();
            let timer = cx
                .background_executor()
                .timer(Duration::from_millis(REFRESH_MS));
            Some(window.spawn(cx, async move |cx| {
                timer.await;
                count.update(cx, |count, cx| {
                    *count += 1;
                    cx.notify();
                });
            }))
        })
}

fn inbox(
    id: &'static str,
    rows: usize,
    short: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let handle = window.use_keyed_state(SharedString::from(format!("{id}-handle")), cx, |_, _| {
        ScrollHandle::new()
    });
    let handle = handle.read(cx).clone();
    let count =
        window.use_keyed_state(SharedString::from(format!("{id}-count")), cx, |_, _| 0usize);
    let n = *count.read(cx);
    let control = use_refresh_control(
        SharedString::from(format!("{id}-control")),
        counting(&count).scroll_handle(handle.clone()),
        window,
        cx,
    );
    let text: SharedString = if short {
        "Pull to refresh".into()
    } else {
        format!("Updated {n} times").into()
    };
    pull_to_refresh(
        id,
        &control,
        &handle,
        text,
        short,
        messages(id, rows, n),
        window,
    )
    .into_any_element()
}

/// Controlled: the button refreshes too (the pull has no keyboard or screen-reader
/// equivalent), and the app sets `is_refreshing` itself.
fn controlled(window: &mut Window, cx: &mut App) -> AnyElement {
    let id = "refresh-controlled";
    let handle =
        window.use_keyed_state("refresh-controlled-handle", cx, |_, _| ScrollHandle::new());
    let handle = handle.read(cx).clone();
    let state = window.use_keyed_state("refresh-controlled-state", cx, |_, _| (false, 0usize));
    let (refreshing, n) = *state.read(cx);
    let refresh = {
        let state = state.clone();
        move |window: &mut Window, cx: &mut App| {
            state.update(cx, |s, cx| {
                s.0 = true;
                cx.notify();
            });
            let state = state.clone();
            let timer = cx
                .background_executor()
                .timer(Duration::from_millis(REFRESH_MS));
            window
                .spawn(cx, async move |cx| {
                    timer.await;
                    state.update(cx, |s, cx| {
                        s.0 = false;
                        s.1 += 1;
                        cx.notify();
                    });
                })
                .detach();
        }
    };
    let on_pull = refresh.clone();
    let control = use_refresh_control(
        "refresh-controlled-control",
        RefreshControlOptions::new()
            .announce("Refreshing")
            .is_refreshing(refreshing)
            .scroll_handle(handle.clone())
            .on_refresh(move |window, cx| {
                on_pull(window, cx);
                None
            }),
        window,
        cx,
    );
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(
            Button::new("refresh-controlled-button", "Refresh")
                .disabled(refreshing)
                .on_click(move |_, window, cx| refresh(window, cx)),
        )
        .child(pull_to_refresh(
            id,
            &control,
            &handle,
            format!("Updated {n} times").into(),
            false,
            messages(id, 40, n),
            window,
        ))
        .into_any_element()
}

fn story(title: &'static str, body: AnyElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(Heading::new(HeadingLevel::H3, title))
        .child(body)
        .into_any_element()
}

/// The pull-to-refresh examples: pull a list down past its top on a touch screen, or
/// with a trackpad's two-finger scroll.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(story(
            "In a scroll container",
            inbox("refresh-inbox", 40, false, window, cx),
        ))
        .child(story("Controlled", controlled(window, cx)))
        .child(story(
            "Short list",
            inbox("refresh-short", 3, true, window, cx),
        ))
        .into_any_element()
}
