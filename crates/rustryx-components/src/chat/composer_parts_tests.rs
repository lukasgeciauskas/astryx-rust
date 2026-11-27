//! Tests for ChatComposerDrawer, ChatSendButton and ChatTokenizedText, after their
//! specs' FR items.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, Bounds, Edges, InteractiveElement as _, IntoElement, Modifiers, ParentElement,
    Pixels, Styled, TestAppContext, VisualTestContext, div, px,
};
use rustryx_primitives::provide;
use rustryx_theme::{
    Appearance, ComponentRule, Declaration, Pointer, ResolvedTheme, StyleProp, Theme,
};

use super::composer_drawer::toggle_name;
use super::send_button::{SendState, presentation, resolve, root_style};
use super::tokenized_text::{Segment, split};
use super::*;
use crate::button::{Button, ButtonVariant};
use crate::segmented::tests::{host, press};
use rustryx_primitives::ElementSize;

type Log = Rc<RefCell<Vec<String>>>;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

fn activate(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.activate_window());
    settle(cx);
}

fn tab(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("tab");
    settle(cx);
}

fn key(cx: &mut VisualTestContext, k: &'static str) {
    press(cx, k);
    settle(cx);
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(b.center(), Modifiers::none());
    settle(cx);
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

fn px_of(
    cx: &mut VisualTestContext,
    f: impl Fn(&rustryx_theme::Tokens) -> gpui::AbsoluteLength,
) -> Pixels {
    cx.update(|window, cx| {
        f(rustryx_theme::current(window, cx).tokens()).to_pixels(window.rem_size())
    })
}

/// Installs a theme with one component rule before the host draws.
fn with_rule(cx: &mut TestAppContext, rule: ComponentRule) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_theme::set_theme(Theme::default().with_rules([rule]), cx);
    });
}

fn rule(
    target: &'static str,
    when: &'static [(Cow<'static, str>, Cow<'static, str>)],
    prop: StyleProp,
) -> ComponentRule {
    ComponentRule {
        target: Cow::Borrowed(target),
        when: Cow::Borrowed(when),
        state: None,
        light: Cow::Owned(vec![Declaration::Style(prop.clone())]),
        dark: Cow::Owned(vec![Declaration::Style(prop)]),
    }
}

fn neutral() -> ResolvedTheme {
    ResolvedTheme::new(
        Arc::new(rustryx_themes::neutral::theme()),
        Appearance::Light,
        1280.,
        Pointer::Fine,
    )
}

// =============================================================================
// ChatComposerDrawer

fn chip(name: &'static str) -> AnyElement {
    div()
        .debug_selector(move || name.into())
        .w(px(60.))
        .h(px(24.))
        .into_any_element()
}

fn drawer_page(
    cx: &mut TestAppContext,
    build: impl Fn() -> ChatComposerDrawer + 'static,
) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        div()
            .w(px(400.))
            .flex()
            .flex_col()
            .child(build())
            .child(div().debug_selector(|| "after".into()).w_full().h(px(40.)))
            .into_any_element()
    });
    settle(cx);
    cx
}

fn logged(log: &Log) -> impl Fn(bool, &mut gpui::Window, &mut gpui::App) + 'static {
    let log = log.clone();
    move |next, _, _| log.borrow_mut().push(next.to_string())
}

#[gpui::test]
fn fr1_any_count_even_zero_makes_one_named_disclosure(cx: &mut TestAppContext) {
    let cx = drawer_page(cx, || {
        ChatComposerDrawer::new("d")
            .count(0usize)
            .child(chip("chip"))
    });
    assert!(
        cx.debug_bounds("d-toggle").is_some(),
        "count 0 enables collapse"
    );
    assert!(
        cx.debug_bounds("d-handle").is_some(),
        "expanded: the bar handle"
    );
    assert!(
        cx.debug_bounds("d-content").is_some(),
        "the controlled region"
    );
    let (expand, collapse, default) = cx.update(|_, cx| {
        (
            toggle_name(true, "Files", cx),
            toggle_name(false, "Files", cx),
            rustryx_i18n::current(cx).t("@astryx.chat.composerDrawer.label"),
        )
    });
    assert_eq!(expand.as_ref(), "Expand Files");
    assert_eq!(collapse.as_ref(), "Collapse Files");
    assert_eq!(default.as_ref(), "Items");
}

#[gpui::test]
fn fr2_pointer_enter_and_space_each_toggle_once(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = drawer_page(cx, {
        let log = log.clone();
        move || {
            ChatComposerDrawer::new("d")
                .count(2usize)
                .on_collapsed_change(logged(&log))
                .child(chip("chip"))
        }
    });
    activate(cx);
    click(cx, "d-toggle");
    assert_eq!(*log.borrow(), ["true"]);
    rest(cx);
    assert!(cx.debug_bounds("chip").is_none(), "collapsed");
    key(cx, "enter");
    assert_eq!(*log.borrow(), ["true", "false"]);
    rest(cx);
    assert!(cx.debug_bounds("chip").is_some(), "expanded");
    key(cx, "space");
    assert_eq!(*log.borrow(), ["true", "false", "true"]);
    rest(cx);
    assert!(cx.debug_bounds("chip").is_none());
}

/// Lets the drawer's collapse motion end.
fn rest(cx: &mut VisualTestContext) {
    for _ in 0..40 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

#[gpui::test]
fn the_drawer_s_height_eases_and_its_items_fade_as_it_collapses(cx: &mut TestAppContext) {
    let cx = drawer_page(cx, || {
        ChatComposerDrawer::new("d")
            .count(2usize)
            .child(div().debug_selector(|| "tall".into()).h(px(120.)))
    });
    activate(cx);
    let open = cx.debug_bounds("d").unwrap().size.height;
    click(cx, "d-toggle");
    for _ in 0..6 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    let mid = cx.debug_bounds("d").unwrap().size.height;
    assert!(mid < open, "easing closed: {mid:?} of {open:?}");
    assert!(
        cx.debug_bounds("tall").is_some(),
        "the items stay while it collapses"
    );
    rest(cx);
    assert!(cx.debug_bounds("tall").is_none(), "gone once collapsed");
    assert!(cx.debug_bounds("d").unwrap().size.height < mid);
}

#[gpui::test]
fn fr2_tab_reaches_the_toggle_and_enter_toggles(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = drawer_page(cx, {
        let log = log.clone();
        move || {
            ChatComposerDrawer::new("d")
                .count(1usize)
                .default_collapsed(true)
                .on_collapsed_change(logged(&log))
                .child(chip("chip"))
        }
    });
    activate(cx);
    tab(cx);
    key(cx, "enter");
    assert_eq!(*log.borrow(), ["false"], "one request per key press");
    assert!(cx.debug_bounds("chip").is_some());
}

#[gpui::test]
fn fr3_controlled_state_stays_authoritative(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = drawer_page(cx, {
        let log = log.clone();
        move || {
            ChatComposerDrawer::new("d")
                .count(2usize)
                .collapsed(false)
                .default_collapsed(true)
                .on_collapsed_change(logged(&log))
                .child(chip("chip"))
        }
    });
    assert!(
        cx.debug_bounds("chip").is_some(),
        "controlled wins over the default"
    );
    click(cx, "d-toggle");
    assert_eq!(*log.borrow(), ["true"], "reports the request");
    assert!(cx.debug_bounds("chip").is_some(), "the caller decides");
}

#[gpui::test]
fn fr3_uncontrolled_starts_from_the_default_and_follows_activation(cx: &mut TestAppContext) {
    let cx = drawer_page(cx, || {
        ChatComposerDrawer::new("d")
            .count(2usize)
            .default_collapsed(true)
            .child(chip("chip"))
    });
    assert!(cx.debug_bounds("chip").is_none());
    click(cx, "d-toggle");
    assert!(cx.debug_bounds("chip").is_some());
}

fn drawer_with_button(collapsed: bool, log: &Log) -> ChatComposerDrawer {
    let log = log.clone();
    ChatComposerDrawer::new("d")
        .count(1usize)
        .default_collapsed(collapsed)
        .child(
            Button::new("inner", "Inner")
                .on_click(move |_, _, _| log.borrow_mut().push("inner".into())),
        )
}

#[gpui::test]
fn fr4_collapsed_content_takes_no_height_and_no_focus(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = drawer_page(cx, {
        let log = log.clone();
        move || drawer_with_button(true, &log)
    });
    assert_eq!(bounds(cx, "d-content").size.height, px(0.));
    activate(cx);
    tab(cx);
    tab(cx);
    // Enter lands on the toggle (the only tab stop), never on the hidden button.
    key(cx, "enter");
    assert!(log.borrow().is_empty(), "the hidden button was not reached");
}

#[gpui::test]
fn fr4_expanded_content_is_back_in_the_tab_order(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = drawer_page(cx, {
        let log = log.clone();
        move || drawer_with_button(false, &log)
    });
    assert!(bounds(cx, "d-content").size.height > px(0.));
    activate(cx);
    tab(cx);
    tab(cx);
    key(cx, "enter");
    assert_eq!(*log.borrow(), ["inner"]);
}

#[gpui::test]
fn fr5_keyboard_focus_shows_a_ring_and_a_pointer_does_not(cx: &mut TestAppContext) {
    let cx = drawer_page(cx, || {
        ChatComposerDrawer::new("d")
            .count(1usize)
            .child(chip("chip"))
    });
    activate(cx);
    let seq = cx.update(|_, cx| crate::focus::rings_since(0, cx).0);
    click(cx, "d-toggle");
    let (seq, rings) = cx.update(|_, cx| crate::focus::rings_since(seq, cx));
    assert!(rings.is_empty(), "no ring after a pointer press");
    tab(cx);
    let toggle = bounds(cx, "d-toggle");
    let (_, rings) = cx.update(|_, cx| crate::focus::rings_since(seq, cx));
    // The ring is drawn inside the row (the drawer clips what lies outside it).
    assert!(
        rings
            .iter()
            .any(|r| r.center() == toggle.center() && toggle.size.width - r.size.width < px(8.)),
        "a ring on the toggle: {rings:?} {toggle:?}"
    );
}

#[gpui::test]
fn fr6_without_a_count_content_shows_directly(cx: &mut TestAppContext) {
    let cx = drawer_page(cx, || {
        ChatComposerDrawer::new("d")
            .collapsed(true)
            .collapsed_summary(chip("custom"))
            .child(chip("chip"))
    });
    assert!(cx.debug_bounds("d-toggle").is_none(), "no disclosure");
    assert!(cx.debug_bounds("d-summary").is_none());
    assert!(cx.debug_bounds("custom").is_none(), "no summary");
    assert!(
        cx.debug_bounds("chip").is_some(),
        "collapse inputs have no effect"
    );
}

#[gpui::test]
fn fr7_caller_style_reaches_the_root(cx: &mut TestAppContext) {
    let cx = drawer_page(cx, || {
        ChatComposerDrawer::new("d")
            .count(1usize)
            .w(px(300.))
            .child(chip("chip"))
    });
    assert_eq!(bounds(cx, "d").size.width, px(300.));
}

static COLLAPSED: &[(Cow<'static, str>, Cow<'static, str>)] =
    &[(Cow::Borrowed("collapsed"), Cow::Borrowed("collapsed"))];

#[gpui::test]
fn fr7_the_theme_target_carries_the_collapsed_prop(cx: &mut TestAppContext) {
    with_rule(
        cx,
        rule(
            "chat-composer-drawer",
            COLLAPSED,
            StyleProp::Height(px(120.).into()),
        ),
    );
    let cx = drawer_page(cx, || {
        ChatComposerDrawer::new("d")
            .count(1usize)
            .default_collapsed(true)
            .child(chip("chip"))
    });
    assert_eq!(bounds(cx, "d").size.height, px(120.));
    click(cx, "d-toggle");
    assert_ne!(bounds(cx, "d").size.height, px(120.), "expanded: no prop");
}

#[gpui::test]
fn layout_the_drawer_tucks_behind_the_composer_by_its_corner(cx: &mut TestAppContext) {
    let cx = drawer_page(cx, || {
        ChatComposerDrawer::new("d")
            .count(1usize)
            .child(chip("chip"))
    });
    let radius = px_of(cx, |t| t.radius.chat);
    let s3 = px_of(cx, |t| t.spacing.s3);
    let s4 = px_of(cx, |t| t.spacing.s4);
    let (root, after) = (bounds(cx, "d"), bounds(cx, "after"));
    let (toggle, content) = (bounds(cx, "d-toggle"), bounds(cx, "d-content"));
    assert_eq!(
        after.top(),
        root.bottom() - radius,
        "negative margin of the corner"
    );
    assert_eq!(
        content.bottom(),
        root.bottom() - s3 - radius,
        "padding s3 + corner"
    );
    assert_eq!(toggle.top(), root.top() + s3);
    assert_eq!(
        toggle.size.width, root.size.width,
        "the row spans the padding"
    );
    assert_eq!(content.left(), root.left() + s4);
}

#[gpui::test]
fn fr8_the_default_summary_is_the_count_badge_and_label(cx: &mut TestAppContext) {
    let cx = drawer_page(cx, || {
        ChatComposerDrawer::new("d")
            .count(3usize)
            .label("Files")
            .default_collapsed(true)
            .child(chip("chip"))
    });
    assert!(cx.debug_bounds("d-summary").is_some());
    assert!(cx.debug_bounds("d-label").is_some());
    assert!(
        cx.debug_bounds("d-handle").is_none(),
        "collapsed: no handle"
    );
    click(cx, "d-toggle");
    rest(cx);
    assert!(
        cx.debug_bounds("d-summary").is_none(),
        "expanded: summary hidden"
    );
    assert!(cx.debug_bounds("d-handle").is_some());
}

#[gpui::test]
fn fr8_fr9_a_custom_summary_replaces_only_the_visuals(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = drawer_page(cx, {
        let log = log.clone();
        move || {
            ChatComposerDrawer::new("d")
                .count(3usize)
                .default_collapsed(true)
                .collapsed_summary(chip("custom"))
                .on_collapsed_change(logged(&log))
                .child(chip("chip"))
        }
    });
    assert!(cx.debug_bounds("custom").is_some(), "the caller's summary");
    assert!(
        cx.debug_bounds("d-label").is_none(),
        "replaces the default whole"
    );
    click(cx, "d-toggle");
    assert_eq!(*log.borrow(), ["false"], "the toggle still works the same");
    rest(cx);
    assert!(cx.debug_bounds("custom").is_none());
    assert!(cx.debug_bounds("chip").is_some());
}

#[gpui::test]
fn fr10_a_custom_summary_is_never_a_tab_stop(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = drawer_page(cx, {
        let log = log.clone();
        move || {
            let inner = log.clone();
            ChatComposerDrawer::new("d")
                .count(3usize)
                .default_collapsed(true)
                .collapsed_summary(
                    Button::new("summary-button", "Do")
                        .on_click(move |_, _, _| inner.borrow_mut().push("summary".into())),
                )
                .on_collapsed_change(logged(&log))
                .child(chip("chip"))
        }
    });
    activate(cx);
    tab(cx);
    tab(cx);
    key(cx, "enter");
    assert!(
        !log.borrow().iter().any(|e| e == "summary"),
        "the summary's control is not reachable: {:?}",
        log.borrow()
    );
}

// =============================================================================
// ChatSendButton

fn send_page(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        div()
            .flex()
            .debug_selector(|| "send".into())
            .child(build())
            .into_any_element()
    });
    settle(cx);
    cx
}

fn push(log: &Log, entry: &'static str) -> impl Fn(&mut gpui::Window, &mut gpui::App) + 'static {
    let log = log.clone();
    move |_, _| log.borrow_mut().push(entry.into())
}

fn context(can_send: bool, stop: bool, log: &Log) -> ChatComposerContext {
    let (submit, stop_log) = (log.clone(), log.clone());
    ChatComposerContext {
        value: "hello".into(),
        on_change: Rc::new(|_, _, _| {}),
        on_submit: Rc::new(move |value, _, _| submit.borrow_mut().push(format!("submit:{value}"))),
        placeholder: "".into(),
        disabled: false,
        stop_shown: stop,
        can_send,
        on_stop: Some(Rc::new(move |_, _| {
            stop_log.borrow_mut().push("ctx-stop".into())
        })),
        input_control: Rc::new(RefCell::new(None)),
    }
}

#[test]
fn fr1_fr2_state_resolution() {
    // ORD3: outside a composer, send is disabled.
    assert_eq!(
        resolve(None, None, None),
        SendState {
            stop: false,
            disabled: true
        }
    );
    assert_eq!(
        resolve(None, Some(false), None),
        SendState {
            stop: false,
            disabled: false
        }
    );
    // Stop ignores disabled.
    assert_eq!(
        resolve(Some(true), Some(true), None),
        SendState {
            stop: true,
            disabled: false
        }
    );
    assert_eq!(
        presentation(false),
        (
            "@astryx.chatSendButton.send",
            ButtonVariant::Primary,
            "arrowUp"
        )
    );
    assert_eq!(
        presentation(true),
        (
            "@astryx.chatSendButton.stop",
            ButtonVariant::Secondary,
            "stop"
        )
    );
}

#[test]
fn fr5_explicit_props_win_over_the_context() {
    let log: Log = Rc::default();
    let ready = context(true, false, &log);
    assert_eq!(
        resolve(None, None, Some(&ready)),
        SendState {
            stop: false,
            disabled: false
        }
    );
    assert_eq!(
        resolve(None, Some(true), Some(&ready)),
        SendState {
            stop: false,
            disabled: true
        }
    );
    let streaming = context(false, true, &log);
    assert_eq!(
        resolve(None, None, Some(&streaming)),
        SendState {
            stop: true,
            disabled: false
        }
    );
    assert_eq!(
        resolve(Some(false), None, Some(&streaming)),
        SendState {
            stop: false,
            disabled: true
        },
        "explicit send state; readiness still from the composer"
    );
}

#[gpui::test]
fn fr1_ar3_a_disabled_send_runs_nothing(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = send_page(cx, {
        let log = log.clone();
        move || {
            let click = log.clone();
            ChatSendButton::new("s")
                .on_send(push(&log, "send"))
                .on_click(move |_, _, _| click.borrow_mut().push("click".into()))
                .into_any_element()
        }
    });
    let names = cx.update(|_, cx| {
        let i18n = rustryx_i18n::current(cx);
        (
            i18n.t("@astryx.chatSendButton.send"),
            i18n.t("@astryx.chatSendButton.stop"),
        )
    });
    assert_eq!((names.0.as_ref(), names.1.as_ref()), ("Send", "Stop"));
    click(cx, "send");
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
}

#[gpui::test]
fn fr1_an_enabled_send_calls_on_send(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = send_page(cx, {
        let log = log.clone();
        move || {
            ChatSendButton::new("s")
                .disabled(false)
                .on_send(push(&log, "send"))
                .on_stop(push(&log, "stop"))
                .into_any_element()
        }
    });
    click(cx, "send");
    assert_eq!(*log.borrow(), ["send"]);
}

#[gpui::test]
fn fr2_stop_stays_enabled_and_calls_only_on_stop(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = send_page(cx, {
        let log = log.clone();
        move || {
            ChatSendButton::new("s")
                .stop_shown(true)
                .disabled(true)
                .on_send(push(&log, "send"))
                .on_stop(push(&log, "stop"))
                .into_any_element()
        }
    });
    click(cx, "send");
    assert_eq!(*log.borrow(), ["stop"]);
}

#[gpui::test]
fn fr2_stop_without_a_handler_is_an_enabled_no_op(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = send_page(cx, {
        let log = log.clone();
        move || {
            let click = log.clone();
            ChatSendButton::new("s")
                .stop_shown(true)
                .on_click(move |_, _, _| click.borrow_mut().push("click".into()))
                .into_any_element()
        }
    });
    click(cx, "send");
    assert_eq!(*log.borrow(), ["click"], "the generic click still runs");
}

fn icon(name: &'static str) -> impl IntoElement {
    div().debug_selector(move || name.into()).size(px(12.))
}

#[gpui::test]
fn fr3_an_explicit_send_icon_wins(cx: &mut TestAppContext) {
    let cx = send_page(cx, || {
        ChatSendButton::new("s")
            .send_icon(icon("my-send"))
            .stop_icon(icon("my-stop"))
            .into_any_element()
    });
    assert!(cx.debug_bounds("my-send").is_some());
    assert!(
        cx.debug_bounds("my-stop").is_none(),
        "only the state's icon"
    );
}

#[gpui::test]
fn fr4_an_explicit_stop_icon_wins(cx: &mut TestAppContext) {
    let cx = send_page(cx, || {
        ChatSendButton::new("s")
            .stop_shown(true)
            .send_icon(icon("my-send"))
            .stop_icon(icon("my-stop"))
            .into_any_element()
    });
    assert!(cx.debug_bounds("my-stop").is_some());
    assert!(cx.debug_bounds("my-send").is_none());
}

#[gpui::test]
fn fr5_the_composer_context_supplies_readiness_submit_and_stop(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let stop = Rc::new(Cell::new(false));
    let cx = send_page(cx, {
        let (log, stop) = (log.clone(), stop.clone());
        move || {
            provide(context(true, stop.get(), &log), ChatSendButton::new("s")).into_any_element()
        }
    });
    click(cx, "send");
    assert_eq!(
        *log.borrow(),
        ["submit:hello"],
        "onSubmit with the composer's value"
    );
    stop.set(true);
    settle(cx);
    click(cx, "send");
    assert_eq!(*log.borrow(), ["submit:hello", "ctx-stop"]);
}

#[gpui::test]
fn fr5_explicit_callbacks_and_state_win_over_the_context(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = send_page(cx, {
        let log = log.clone();
        move || {
            provide(
                context(false, true, &log),
                ChatSendButton::new("s")
                    .stop_shown(false)
                    .disabled(false)
                    .on_send(push(&log, "send")),
            )
            .into_any_element()
        }
    });
    click(cx, "send");
    assert_eq!(*log.borrow(), ["send"]);
}

#[gpui::test]
fn fr5_on_click_composes_after_the_state_action(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = send_page(cx, {
        let log = log.clone();
        move || {
            let click = log.clone();
            ChatSendButton::new("s")
                .disabled(false)
                .on_send(push(&log, "send"))
                .on_click(move |_, window, _| {
                    click.borrow_mut().push("click".into());
                    // Too late to cancel the state action, which already ran.
                    window.prevent_default();
                })
                .into_any_element()
        }
    });
    click(cx, "send");
    assert_eq!(*log.borrow(), ["send", "click"]);
}

#[gpui::test]
fn fr5_ar4_enter_and_space_take_the_same_path(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = send_page(cx, {
        let log = log.clone();
        move || {
            let click = log.clone();
            ChatSendButton::new("s")
                .disabled(false)
                .on_send(push(&log, "send"))
                .on_click(move |_, _, _| click.borrow_mut().push("click".into()))
                .into_any_element()
        }
    });
    activate(cx);
    tab(cx);
    key(cx, "enter");
    assert_eq!(*log.borrow(), ["send", "click"]);
    key(cx, "space");
    assert_eq!(*log.borrow(), ["send", "click", "send", "click"]);
}

#[gpui::test]
fn fr5_both_sizes_reach_the_button(cx: &mut TestAppContext) {
    let small = Rc::new(Cell::new(true));
    let cx = send_page(cx, {
        let small = small.clone();
        move || {
            let size = if small.get() {
                ChatSendButtonSize::Sm
            } else {
                ChatSendButtonSize::Md
            };
            ChatSendButton::new("s").size(size).into_any_element()
        }
    });
    let (sm, md) = (
        px_of(cx, |t| t.size.element_sm),
        px_of(cx, |t| t.size.element_md),
    );
    let b = bounds(cx, "send");
    assert_eq!((b.size.width, b.size.height), (sm, sm), "square sm");
    small.set(false);
    settle(cx);
    let b = bounds(cx, "send");
    assert_eq!(
        (b.size.width, b.size.height),
        (md, md),
        "square md by default size"
    );
    assert_eq!(ChatSendButtonSize::default(), ChatSendButtonSize::Md);
}

#[gpui::test]
fn fr6_caller_style_reaches_the_button(cx: &mut TestAppContext) {
    let cx = send_page(cx, || {
        ChatSendButton::new("s").h(px(50.)).into_any_element()
    });
    assert_eq!(bounds(cx, "send").size.height, px(50.));
}

#[gpui::test]
fn fr7_the_theme_target_styles_the_button_and_the_caller_wins(cx: &mut TestAppContext) {
    with_rule(
        cx,
        rule("chat-send-button", &[], StyleProp::Height(px(60.).into())),
    );
    let caller = Rc::new(Cell::new(false));
    let cx = send_page(cx, {
        let caller = caller.clone();
        move || {
            let b = ChatSendButton::new("s");
            if caller.get() { b.h(px(44.)) } else { b }.into_any_element()
        }
    });
    let b = bounds(cx, "send");
    assert_eq!(
        (b.size.width, b.size.height),
        (px(60.), px(60.)),
        "still square"
    );
    caller.set(true);
    settle(cx);
    assert_eq!(bounds(cx, "send").size.height, px(44.));
}

#[test]
fn fr7_the_button_is_circular_unless_a_theme_rounds_buttons() {
    let theme = neutral();
    let t = theme.tokens();
    let style = root_style(&theme, ButtonVariant::Primary, ElementSize::Md);
    assert_eq!(style.corner_radii.top_left, Some(t.radius.full));
    assert_eq!(style.corner_radii.bottom_right, Some(t.radius.full));
    assert_eq!(style.flex_shrink, Some(0.));

    let rounded = ResolvedTheme::new(
        Arc::new(Theme::default().with_rules([rule(
            "button",
            &[],
            StyleProp::Radius(px(4.).into()),
        )])),
        Appearance::Light,
        1280.,
        Pointer::Fine,
    );
    let style = root_style(&rounded, ButtonVariant::Primary, ElementSize::Md);
    assert_eq!(
        style.corner_radii.top_left, None,
        "the theme's button radius wins"
    );

    let target = ResolvedTheme::new(
        Arc::new(Theme::default().with_rules([rule(
            "chat-send-button",
            &[],
            StyleProp::Radius(px(6.).into()),
        )])),
        Appearance::Light,
        1280.,
        Pointer::Fine,
    );
    let style = root_style(&target, ButtonVariant::Secondary, ElementSize::Sm);
    assert_eq!(style.corner_radii.top_left, Some(px(6.).into()));
}

// =============================================================================
// ChatTokenizedText

fn text(s: &str) -> Segment {
    Segment::Text(s.into())
}

#[test]
fn fr1_plain_text_without_tokens_is_kept() {
    let none: [&str; 0] = [];
    assert_eq!(split("hello world", &none), [text("hello world")]);
    assert_eq!(split("hello world", &["@ana"]), [text("hello world")]);
    assert!(split("", &["@ana"]).is_empty(), "empty text, empty root");
}

#[test]
fn fr1_tokens_replace_matches_in_reading_order() {
    assert_eq!(
        split("Hi @ana and @bo!", &["@ana", "@bo"]),
        [
            text("Hi "),
            Segment::Token(0),
            text(" and "),
            Segment::Token(1),
            text("!")
        ]
    );
    assert_eq!(split("@ana", &["@ana"]), [Segment::Token(0)]);
    assert_eq!(
        split("@ana@ana x @ana", &["@ana"]),
        [
            Segment::Token(0),
            Segment::Token(0),
            text(" x "),
            Segment::Token(0)
        ],
        "every occurrence"
    );
}

#[test]
fn fr1_values_match_literally() {
    assert_eq!(
        split("axb a.b", &["a.b"]),
        [text("axb "), Segment::Token(0)]
    );
    assert_eq!(split("(x)|[y]", &["(x)|[y]"]), [Segment::Token(0)]);
    assert_eq!(
        split("$^*+?", &["*+"]),
        [text("$^"), Segment::Token(0), text("?")]
    );
}

#[test]
fn fr1_oq1_the_first_listed_value_wins_and_duplicates_take_the_last() {
    // A shorter prefix listed first splits a longer value (current-state impact).
    assert_eq!(
        split("@u12", &["@u1", "@u12"]),
        [Segment::Token(0), text("2")]
    );
    assert_eq!(split("@u12", &["@u12", "@u1"]), [Segment::Token(0)]);
    // The earliest position wins over list order.
    assert_eq!(
        split("b a", &["a", "b"]),
        [Segment::Token(1), text(" "), Segment::Token(0)]
    );
    // Two tokens with one value: the later definition renders.
    assert_eq!(split("@x", &["@x", "@y", "@x"]), [Segment::Token(2)]);
}

#[test]
fn fr2_empty_values_are_ignored() {
    assert_eq!(
        split("a @b c", &["", "@b"]),
        [text("a "), Segment::Token(1), text(" c")]
    );
    assert_eq!(
        split("abc", &["", ""]),
        [text("abc")],
        "terminates, text kept"
    );
}

#[test]
fn fr1_multibyte_text_is_kept_exactly() {
    assert_eq!(
        split("héllo 👋@ana ünï", &["@ana"]),
        [text("héllo 👋"), Segment::Token(0), text(" ünï")]
    );
    assert_eq!(
        split("日本語", &["本"]),
        [text("日"), Segment::Token(0), text("語")]
    );
}

fn text_page(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    let cx = host(cx, move || {
        div()
            .w(px(400.))
            .flex()
            .flex_col()
            .child(div().debug_selector(|| "plain".into()).child("Hi there"))
            .child(div().debug_selector(|| "tt".into()).child(build()))
            .into_any_element()
    });
    settle(cx);
    cx
}

#[gpui::test]
fn fr1_fr3_badge_tokens_render_inline_in_the_text(cx: &mut TestAppContext) {
    let cx = text_page(cx, || {
        ChatTokenizedText::new("Hi @ana there")
            .tokens([ChatComposerToken::badge("@ana", "Ana")])
            .into_any_element()
    });
    let (plain, tt, token) = (
        bounds(cx, "plain"),
        bounds(cx, "tt"),
        bounds(cx, "chat-token-@ana"),
    );
    assert!(
        tt.size.height <= plain.size.height * 1.5,
        "one line: {plain:?} {tt:?}"
    );
    assert!(token.left() > tt.left(), "after the leading text");
    assert!(token.top() >= tt.top() && token.bottom() <= tt.bottom());
}

#[gpui::test]
fn fr2_empty_token_values_render_the_text(cx: &mut TestAppContext) {
    let cx = text_page(cx, || {
        ChatTokenizedText::new("Hi there")
            .tokens([ChatComposerToken::badge("", "Empty")])
            .into_any_element()
    });
    assert!(cx.debug_bounds("chat-token-").is_none());
    assert_eq!(bounds(cx, "tt").size, bounds(cx, "plain").size);
}

#[gpui::test]
fn fr3_custom_tokens_render_only_for_a_match(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0usize));
    let matching = Rc::new(Cell::new(false));
    let cx = text_page(cx, {
        let (calls, matching) = (calls.clone(), matching.clone());
        move || {
            let calls = calls.clone();
            let text = if matching.get() { "Hi @x" } else { "Hi there" };
            ChatTokenizedText::new(text)
                .tokens([ChatComposerToken::custom("@x", move |_, _| {
                    calls.set(calls.get() + 1);
                    div()
                        .debug_selector(|| "custom-token".into())
                        .child("X")
                        .into_any_element()
                })])
                .into_any_element()
        }
    });
    assert_eq!(calls.get(), 0, "no match, no render");
    matching.set(true);
    settle(cx);
    assert!(calls.get() > 0);
    assert!(cx.debug_bounds("custom-token").is_some());
}

#[gpui::test]
fn fr4_caller_style_reaches_the_root(cx: &mut TestAppContext) {
    let cx = text_page(cx, || {
        ChatTokenizedText::new("Hi there")
            .pt(px(15.))
            .into_any_element()
    });
    assert_eq!(
        bounds(cx, "tt").size.height,
        bounds(cx, "plain").size.height + px(15.)
    );
}

#[gpui::test]
fn fr4_the_theme_target_styles_the_root_and_the_caller_wins(cx: &mut TestAppContext) {
    with_rule(
        cx,
        rule(
            "chat-tokenized-text",
            &[],
            StyleProp::Padding(Edges::all(px(10.).into())),
        ),
    );
    let caller = Rc::new(Cell::new(false));
    let cx = text_page(cx, {
        let caller = caller.clone();
        move || {
            let el = ChatTokenizedText::new("Hi there");
            if caller.get() { el.py(px(3.)) } else { el }.into_any_element()
        }
    });
    let plain = bounds(cx, "plain").size.height;
    assert_eq!(bounds(cx, "tt").size.height, plain + px(20.));
    caller.set(true);
    settle(cx);
    assert_eq!(bounds(cx, "tt").size.height, plain + px(6.));
}

// FR5 (render-only) is read from source, as upstream's verification map does: the
// component holds its text, tokens and style, and keeps no keyed state.

#[gpui::test]
fn fr4_collapsing_items_are_inert(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = drawer_page(cx, {
        let log = log.clone();
        move || {
            let log = log.clone();
            ChatComposerDrawer::new("d").count(1usize).child(
                div().debug_selector(|| "item".into()).child(
                    Button::new("item-button", "Item")
                        .on_click(move |_, _, _| log.borrow_mut().push("item".into())),
                ),
            )
        }
    });
    activate(cx);
    // The toggle takes focus from the press and collapses the drawer.
    click(cx, "d-toggle");
    for _ in 0..3 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        settle(cx);
    }
    let item = cx.debug_bounds("item").expect("held while it collapses");
    cx.simulate_click(item.center(), Modifiers::none());
    settle(cx);
    assert!(
        log.borrow().is_empty(),
        "a press reaches nothing: {:?}",
        log.borrow()
    );
    // Tab skips the items: it comes back to the toggle, which Enter expands again.
    key(cx, "tab");
    key(cx, "enter");
    rest(cx);
    assert!(log.borrow().is_empty());
    assert!(cx.debug_bounds("item").is_some(), "expanded again");
}
