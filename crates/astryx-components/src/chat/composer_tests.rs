//! Tests for ChatComposer, ChatComposerInput, its tokens and trigger menus, after the
//! specs' FR items.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::{Politeness, announce::announcement, announcer};
use gpui::{
    AnyElement, App, ClipboardEntry, ClipboardItem, Context, EntityInputHandler as _,
    ExternalPaths, InteractiveElement as _, IntoElement, Modifiers, ParentElement, Render,
    SharedString, Styled, Task, TestAppContext, VisualTestContext, Window, div, px,
};

use super::composer_token::format_pasted_text_label;
use super::*;
use crate::search::{SearchAnswer, SearchContext, SearchItem, SearchSource, static_source};
use crate::segmented::tests::press;

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(px(20.))
            .w(px(520.))
            .pt(px(260.))
            .child((self.0)())
            .child(announcer())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

type Log = Rc<RefCell<Vec<String>>>;

fn logger(log: &Log) -> impl Fn(SharedString, &mut Window, &mut App) + 'static {
    let log = log.clone();
    move |v, _, _| log.borrow_mut().push(v.to_string())
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<gpui::Pixels> {
    let s: &'static str = Box::leak(selector.to_string().into_boxed_str());
    cx.debug_bounds(s)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
}

fn drawn(cx: &mut VisualTestContext, selector: &str) -> bool {
    let s: &'static str = Box::leak(selector.to_string().into_boxed_str());
    cx.debug_bounds(s).is_some()
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    let b = bounds(cx, selector);
    cx.simulate_click(b.center(), Modifiers::none());
    settle(cx);
}

fn typed(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    settle(cx);
}

fn key(cx: &mut VisualTestContext, k: &'static str) {
    press(cx, k);
    settle(cx);
}

/// A composer whose submits and changes are logged.
fn composer<'a>(
    cx: &'a mut TestAppContext,
    sent: &Log,
    tweak: impl Fn(ChatComposer) -> ChatComposer + 'static,
) -> &'a mut VisualTestContext {
    let sent = sent.clone();
    host(cx, move || {
        tweak(ChatComposer::new("c", logger(&sent))).into_any_element()
    })
}

// =============================================================================
// ChatComposer

#[gpui::test]
fn fr1_uncontrolled_drafts_submit_trimmed_and_clear(cx: &mut TestAppContext) {
    let sent: Log = Rc::default();
    let changes: Log = Rc::default();
    let cx = composer(cx, &sent, {
        let changes = changes.clone();
        move |c| c.on_change(logger(&changes))
    });
    click(cx, "c-input-editor");
    typed(cx, "  hello  ");
    assert_eq!(
        changes.borrow().last().map(String::as_str),
        Some("  hello  ")
    );
    assert!(!drawn(cx, "c-input-placeholder"), "the placeholder hides");
    key(cx, "enter");
    assert_eq!(*sent.borrow(), ["hello"], "trimmed, once");
    assert_eq!(
        changes.borrow().last().map(String::as_str),
        Some(""),
        "cleared"
    );
    assert!(drawn(cx, "c-input-placeholder"));
    key(cx, "enter");
    assert_eq!(sent.borrow().len(), 1, "an empty draft is not sent");
}

#[gpui::test]
fn fr1_controlled_drafts_report_and_clear_through_on_change(cx: &mut TestAppContext) {
    let sent: Log = Rc::default();
    let value = Rc::new(RefCell::new(SharedString::from("draft")));
    let cx = composer(cx, &sent, {
        let value = value.clone();
        move |c| {
            let store = value.clone();
            c.value(value.borrow().clone())
                .on_change(move |v, _, _| *store.borrow_mut() = v)
        }
    });
    click(cx, "c-input-editor");
    typed(cx, "!");
    assert_eq!(value.borrow().as_ref(), "draft!");
    click(cx, "c-send");
    assert_eq!(
        *sent.borrow(),
        ["draft!"],
        "the send button submits the context value"
    );
    assert_eq!(value.borrow().as_ref(), "", "cleared through on_change");
}

#[gpui::test]
fn fr2_shift_enter_breaks_the_line_and_ime_enter_never_sends(cx: &mut TestAppContext) {
    let sent: Log = Rc::default();
    let changes: Log = Rc::default();
    let handle = ChatComposerInputHandle::new();
    let cx = composer(cx, &sent, {
        let (changes, handle) = (changes.clone(), handle.clone());
        move |c| {
            c.on_change(logger(&changes))
                .input(ChatComposerInput::new("c-input").handle(handle.clone()))
        }
    });
    click(cx, "c-input-editor");
    typed(cx, "one");
    key(cx, "shift-enter");
    typed(cx, "two");
    assert_eq!(
        changes.borrow().last().map(String::as_str),
        Some("one\ntwo")
    );
    // An IME composition takes Enter.
    let editor = handle.editor().expect("drawn");
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| {
            s.replace_and_mark_text_in_range(None, "ni", None, window, cx)
        })
    });
    settle(cx);
    key(cx, "enter");
    assert!(sent.borrow().is_empty(), "the composition's Enter");
    cx.update(|window, cx| editor.update(cx, |s, cx| s.unmark_text(window, cx)));
    settle(cx);
    key(cx, "enter");
    assert_eq!(*sent.borrow(), ["one\ntwoni"]);
}

#[gpui::test]
fn fr2_a_caller_key_handler_can_prevent_the_send(cx: &mut TestAppContext) {
    let sent: Log = Rc::default();
    let s2 = sent.clone();
    let cx = host(cx, move || {
        ChatComposer::new("c", logger(&s2))
            .input(
                ChatComposerInput::new("c-input").on_key_down(|event, window, _| {
                    if event.keystroke.key == "enter" {
                        window.prevent_default();
                    }
                }),
            )
            .into_any_element()
    });
    click(cx, "c-input-editor");
    typed(cx, "hi");
    key(cx, "enter");
    assert!(sent.borrow().is_empty(), "prevented");
}

#[gpui::test]
fn fr3_disabled_composers_take_no_edits_or_sends(cx: &mut TestAppContext) {
    let sent: Log = Rc::default();
    let changes: Log = Rc::default();
    let cx = composer(cx, &sent, {
        let changes = changes.clone();
        move |c| c.disabled(true).on_change(logger(&changes))
    });
    let editor = bounds(cx, "c-input-editor");
    cx.simulate_click(editor.center(), Modifiers::none());
    settle(cx);
    typed(cx, "x");
    key(cx, "enter");
    assert!(changes.borrow().is_empty() && sent.borrow().is_empty());
}

#[gpui::test]
fn fr4_a_shown_stop_stays_operable_while_editing_is_disabled(cx: &mut TestAppContext) {
    let sent: Log = Rc::default();
    let stops = Rc::new(RefCell::new(0));
    let cx = composer(cx, &sent, {
        let stops = stops.clone();
        move |c| {
            let stops = stops.clone();
            c.disabled(true)
                .stop_shown(true)
                .on_stop(move |_, _| *stops.borrow_mut() += 1)
        }
    });
    click(cx, "c-send");
    assert_eq!(*stops.borrow(), 1);
}

#[gpui::test]
fn fr5_slots_stack_in_order_and_an_empty_header_is_omitted(cx: &mut TestAppContext) {
    let sent: Log = Rc::default();
    let cx = composer(cx, &sent, |c| {
        c.drawer(div().debug_selector(|| "drawer".into()).h(px(30.)))
            .footer_actions(
                div()
                    .debug_selector(|| "footer-action".into())
                    .size(px(20.)),
            )
            .send_actions(div().debug_selector(|| "send-action".into()).size(px(20.)))
            .status(ChatComposerStatus::error("Failed"))
    });
    let (drawer, body, status) = (
        bounds(cx, "drawer"),
        bounds(cx, "c-body"),
        bounds(cx, "c-status"),
    );
    assert!(drawer.top() < body.top(), "the drawer precedes the body");
    assert!(status.top() > body.top(), "a bottom status follows it");
    let (input, footer) = (bounds(cx, "c-input"), bounds(cx, "c-footer"));
    assert!(
        input.bottom() <= footer.top() + px(1.),
        "the input precedes the footer"
    );
    let (action, send_action, send) = (
        bounds(cx, "footer-action"),
        bounds(cx, "send-action"),
        bounds(cx, "c-send"),
    );
    assert!(action.left() < send_action.left() && send_action.right() <= send.left());
    // No header slots: the input is the body's first row.
    let pad = input.top() - body.top();
    assert!(pad < px(16.), "no header row: {pad:?}");
}

#[gpui::test]
fn fr5_a_top_status_precedes_the_body(cx: &mut TestAppContext) {
    let sent: Log = Rc::default();
    let cx = composer(cx, &sent, |c| {
        c.status(ChatComposerStatus::warning("Careful"))
            .status_position(ChatComposerStatusPosition::Top)
            .header_context(div().debug_selector(|| "context".into()).size(px(10.)))
    });
    assert!(bounds(cx, "c-status").top() < bounds(cx, "c-body").top());
    assert!(
        bounds(cx, "context").bottom() < bounds(cx, "c-input").top(),
        "the header row"
    );
}

#[gpui::test]
fn fr6_compact_tightens_and_spacious_matches_balanced(cx: &mut TestAppContext) {
    let inset = |density: ChatDensity, cx: &mut TestAppContext| {
        let sent: Log = Rc::default();
        let cx = composer(cx, &sent, move |c| c.density(density));
        bounds(cx, "c-input").left() - bounds(cx, "c-body").left()
    };
    let compact = inset(ChatDensity::Compact, cx);
    let balanced = inset(ChatDensity::Balanced, cx);
    let spacious = inset(ChatDensity::Spacious, cx);
    assert!(compact < balanced, "{compact:?} < {balanced:?}");
    assert_eq!(balanced, spacious);
}

#[gpui::test]
fn fr7_a_flat_body_keeps_the_raised_geometry(cx: &mut TestAppContext) {
    let inset = |elevation: ChatComposerElevation, cx: &mut TestAppContext| {
        let sent: Log = Rc::default();
        let cx = composer(cx, &sent, move |c| c.elevation(elevation));
        bounds(cx, "c-input").left() - bounds(cx, "c-body").left()
    };
    assert_eq!(
        inset(ChatComposerElevation::Low, cx),
        inset(ChatComposerElevation::None, cx),
        "the border is drawn inside the padding"
    );
}

#[gpui::test]
fn fr8_a_body_click_focuses_the_input_and_only_keyboard_focus_rings(cx: &mut TestAppContext) {
    let sent: Log = Rc::default();
    let changes: Log = Rc::default();
    let cx = composer(cx, &sent, {
        let changes = changes.clone();
        move |c| c.on_change(logger(&changes))
    });
    // The footer's empty middle is body space.
    let footer = bounds(cx, "c-footer");
    cx.simulate_click(
        gpui::point(footer.left() + px(40.), footer.center().y),
        Modifiers::none(),
    );
    settle(cx);
    typed(cx, "a");
    assert_eq!(
        changes.borrow().last().map(String::as_str),
        Some("a"),
        "focused by the click"
    );
    assert!(!drawn(cx, "c-ring"), "no ring for pointer focus");
    // Tab away and back: keyboard focus rings the body.
    key(cx, "tab");
    key(cx, "shift-tab");
    assert!(
        drawn(cx, "c-ring"),
        "keyboard focus in the editor rings the body"
    );
}

#[gpui::test]
fn fr9_status_announces_once_per_occasion_with_its_urgency(cx: &mut TestAppContext) {
    let status: Rc<RefCell<Option<ChatComposerStatus>>> = Rc::new(RefCell::new(Some(
        ChatComposerStatus::error("Shown at load"),
    )));
    let sent: Log = Rc::default();
    let cx = composer(cx, &sent, {
        let status = status.clone();
        move |c| match status.borrow().clone() {
            Some(s) => c.status(s),
            None => c,
        }
    });
    let heard = |cx: &mut VisualTestContext, p: Politeness| {
        cx.update(|window, cx| announcement(p, window, cx).to_string())
    };
    assert_eq!(heard(cx, Politeness::Assertive), "", "not at load");
    *status.borrow_mut() = None;
    settle(cx);
    *status.borrow_mut() = Some(ChatComposerStatus::warning("Slow network"));
    settle(cx);
    assert_eq!(heard(cx, Politeness::Polite), "Slow network");
    *status.borrow_mut() = Some(ChatComposerStatus::error("Send failed"));
    settle(cx);
    assert_eq!(heard(cx, Politeness::Assertive), "Send failed");
}

/// A custom input that shows the context's value and sends a fixed one.
#[derive(gpui::IntoElement)]
struct Custom;

impl gpui::RenderOnce for Custom {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let context = use_chat_composer_context(cx).expect("inside a composer");
        let submit = context.on_submit.clone();
        div()
            .child(
                div()
                    .debug_selector(|| "custom-value".into())
                    .child(format!("[{}]", context.value)),
            )
            .child(
                div()
                    .debug_selector(|| "custom-send".into())
                    .flex()
                    .w_fit()
                    .child(
                        crate::button::Button::new("custom-send", "Send it").on_click(
                            move |_, window, cx| submit("  from custom ".into(), window, cx),
                        ),
                    ),
            )
    }
}

#[gpui::test]
fn fr11_a_custom_input_reads_and_submits_through_the_context(cx: &mut TestAppContext) {
    let sent: Log = Rc::default();
    let cx = composer(cx, &sent, |c| c.input(Custom).value("abc"));
    assert!(drawn(cx, "custom-value"));
    click(cx, "custom-send");
    assert_eq!(
        *sent.borrow(),
        ["from custom"],
        "the supplied value, trimmed"
    );
}

// =============================================================================
// ChatComposerInput

fn people() -> Vec<SearchItem> {
    vec![
        SearchItem::new("ada", "Ada Lovelace"),
        SearchItem::new("alan", "Alan Turing"),
        SearchItem::new("grace", "Grace Hopper"),
    ]
}

fn mention() -> ChatComposerTrigger {
    ChatComposerTrigger::new('@', static_source(people()), |item| {
        ChatComposerToken::badge(format!("@{}", item.id()), format!("@{}", item.label())).into()
    })
    .deserialize(|text| {
        text.strip_prefix('@')
            .map(|id| ChatComposerToken::badge(text.to_string(), format!("@{id}")))
    })
}

fn input_host<'a>(
    cx: &'a mut TestAppContext,
    log: &Log,
    tweak: impl Fn(ChatComposerInput) -> ChatComposerInput + 'static,
) -> &'a mut VisualTestContext {
    let log = log.clone();
    host(cx, move || {
        tweak(
            ChatComposerInput::new("i")
                .on_change(logger(&log))
                .on_submit({
                    let log = log.clone();
                    move |v, _, _| log.borrow_mut().push(format!("sent:{v}"))
                }),
        )
        .into_any_element()
    })
}

fn last(log: &Log) -> String {
    log.borrow().last().cloned().unwrap_or_default()
}

#[gpui::test]
fn fr1_an_inserted_token_emits_once_and_serializes_to_its_value(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let handle = ChatComposerInputHandle::new();
    let cx = input_host(cx, &log, {
        let handle = handle.clone();
        move |i| i.handle(handle.clone())
    });
    click(cx, "i-editor");
    typed(cx, "hi ");
    let before = log.borrow().len();
    cx.update(|window, cx| {
        handle.insert_token(ChatComposerToken::badge("@ada", "@Ada"), window, cx);
    });
    settle(cx);
    assert_eq!(log.borrow().len(), before + 1, "one change");
    assert_eq!(last(&log), "hi @ada\u{A0}");
    assert!(
        drawn(cx, "chat-token-@ada"),
        "the token is drawn in the line"
    );
    assert_eq!(
        cx.update(|_, cx| handle.value(cx)).as_ref(),
        "hi @ada\u{A0}"
    );
    key(cx, "enter");
    assert!(log.borrow().contains(&"sent:hi @ada".to_string()));
}

#[gpui::test]
fn fr4_a_controlled_echo_keeps_the_caret_and_an_override_replaces(cx: &mut TestAppContext) {
    let value = Rc::new(RefCell::new(SharedString::from("")));
    let cx = host(cx, {
        let value = value.clone();
        move || {
            let store = value.clone();
            ChatComposerInput::new("i")
                .value(value.borrow().clone())
                .on_change(move |v, _, _| *store.borrow_mut() = v)
                .into_any_element()
        }
    });
    click(cx, "i-editor");
    typed(cx, "abc");
    key(cx, "left");
    typed(cx, "X");
    assert_eq!(
        value.borrow().as_ref(),
        "abXc",
        "the echo left the caret in place"
    );
    *value.borrow_mut() = "replaced".into();
    settle(cx);
    typed(cx, "!");
    assert_eq!(
        value.borrow().as_ref(),
        "replaced!",
        "an override, the caret at its end"
    );
}

#[gpui::test]
fn fr5_paste_goes_to_the_caller_then_tokens_then_text(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let taken: Log = Rc::default();
    let cx = input_host(cx, &log, {
        let taken = taken.clone();
        move |i| {
            let taken = taken.clone();
            i.on_paste(move |text, _, _| {
                let take = text.starts_with("mine");
                if take {
                    taken.borrow_mut().push(text.to_string());
                }
                take
            })
        }
    });
    click(cx, "i-editor");
    let paste = |cx: &mut VisualTestContext, text: &str| {
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.to_string())));
        key(cx, "secondary-v");
    };
    paste(cx, "mine: keep out");
    assert_eq!(*taken.borrow(), ["mine: keep out"]);
    assert_eq!(last(&log), "", "the caller took it");
    paste(cx, "short");
    assert_eq!(last(&log), "short");
    let long = "x".repeat(240);
    paste(cx, &long);
    assert_eq!(
        last(&log),
        format!("short{long}\u{A0}"),
        "a long paste is one token"
    );
    let label = cx.update(|_, cx| format_pasted_text_label(&long, cx));
    assert_eq!(label.as_ref(), "240 chars");
}

#[gpui::test]
fn fr6_pasted_and_dropped_files_go_to_on_files_unless_disabled(cx: &mut TestAppContext) {
    let files: Log = Rc::default();
    let disabled = Rc::new(RefCell::new(false));
    let log: Log = Rc::default();
    let cx = input_host(cx, &log, {
        let (files, disabled) = (files.clone(), disabled.clone());
        move |i| {
            let files = files.clone();
            i.disabled(*disabled.borrow())
                .on_files(move |picked, _, _| {
                    for f in picked {
                        files.borrow_mut().push(f.name.to_string());
                    }
                })
        }
    });
    click(cx, "i-editor");
    let item = ClipboardItem {
        entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(
            vec![
                std::path::PathBuf::from("a.png"),
                std::path::PathBuf::from("b.txt"),
            ]
            .into(),
        ))],
    };
    cx.update(|_, cx| cx.write_to_clipboard(item.clone()));
    key(cx, "secondary-v");
    assert_eq!(*files.borrow(), ["a.png", "b.txt"], "in order");
    assert_eq!(last(&log), "", "nothing inserted");
    *disabled.borrow_mut() = true;
    settle(cx);
    cx.update(|_, cx| cx.write_to_clipboard(item));
    key(cx, "secondary-v");
    assert_eq!(files.borrow().len(), 2, "not while disabled");
}

#[gpui::test]
fn fr7_fr12_the_menu_takes_enter_first_and_a_pick_replaces_the_query(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = input_host(cx, &log, |i| i.triggers([mention()]));
    click(cx, "i-editor");
    typed(cx, "hi @al");
    assert!(drawn(cx, "i-trigger-menu-alan"), "the menu shows the match");
    assert!(!drawn(cx, "i-trigger-menu-grace"));
    key(cx, "enter");
    assert_eq!(
        last(&log),
        "hi @alan\u{A0}",
        "the pick replaced the trigger and query"
    );
    assert!(
        !log.borrow().iter().any(|l| l.starts_with("sent:")),
        "no send"
    );
    assert!(!drawn(cx, "i-trigger-menu"), "closed");
    key(cx, "enter");
    assert!(log.borrow().contains(&"sent:hi @alan".to_string()));
}

#[gpui::test]
fn fr8_history_recalls_at_the_boundaries_and_restores_the_draft(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = input_host(cx, &log, |i| i);
    click(cx, "i-editor");
    for message in ["one", "two"] {
        typed(cx, message);
        key(cx, "enter");
    }
    typed(cx, "dr");
    key(cx, "home");
    key(cx, "up");
    assert_eq!(last(&log), "two");
    key(cx, "up");
    assert_eq!(last(&log), "one");
    key(cx, "down");
    assert_eq!(last(&log), "two");
    key(cx, "down");
    assert_eq!(last(&log), "dr", "past the newest: the draft again");
}

#[gpui::test]
fn fr3_mid_text_arrows_move_the_caret(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = input_host(cx, &log, |i| i);
    click(cx, "i-editor");
    typed(cx, "sent");
    key(cx, "enter");
    typed(cx, "ab");
    key(cx, "shift-enter");
    typed(cx, "cd");
    key(cx, "up");
    typed(cx, "!");
    assert_eq!(last(&log), "ab!\ncd", "up moved a line, no recall");
}

/// A source that answers after 100 ms.
struct Slow;

impl SearchSource<SearchItem> for Slow {
    fn search(
        &self,
        query: &str,
        _: &SearchContext<SearchItem>,
        cx: &mut App,
    ) -> SearchAnswer<SearchItem> {
        let query = query.to_lowercase();
        let task: Task<anyhow::Result<Vec<SearchItem>>> = cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            Ok(people()
                .into_iter()
                .filter(|p| p.id().contains(&query))
                .collect())
        });
        SearchAnswer::Pending(task)
    }

    fn bootstrap(&self, _: &mut App) -> SearchAnswer<SearchItem> {
        SearchAnswer::Ready(Vec::new())
    }
}

use crate::search::Searchable as _;

#[gpui::test]
fn fr13_enter_while_a_search_is_pending_sends_as_typed(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = input_host(cx, &log, |i| {
        i.triggers([ChatComposerTrigger::new('@', Slow, |item| {
            ChatComposerTriggerResult::Text(format!("@{}", item.id()).into())
        })])
    });
    click(cx, "i-editor");
    typed(cx, "@ad");
    assert!(drawn(cx, "i-trigger-menu-loading"), "searching");
    key(cx, "enter");
    assert!(
        log.borrow().contains(&"sent:@ad".to_string()),
        "sent as typed"
    );
    cx.executor().advance_clock(Duration::from_millis(400));
    settle(cx);
    assert!(
        !drawn(cx, "i-trigger-menu"),
        "a late result never reopens the menu"
    );
}

#[gpui::test]
fn fr14_unmatched_queries_hide_multi_word_menus_and_show_the_empty_message(
    cx: &mut TestAppContext,
) {
    let log: Log = Rc::default();
    let cx = input_host(cx, &log, |i| {
        i.triggers([
            mention(),
            ChatComposerTrigger::new('#', static_source(people()), |item| {
                ChatComposerTriggerResult::Text(item.label())
            })
            .multi_word_query(true),
        ])
    });
    click(cx, "i-editor");
    typed(cx, "@zz");
    assert!(
        drawn(cx, "i-trigger-menu-empty"),
        "a single-token query shows No results"
    );
    key(cx, "escape");
    typed(cx, " #Ada Love");
    assert!(
        drawn(cx, "i-trigger-menu-ada"),
        "a multi-word query runs through spaces"
    );
    typed(cx, "zz");
    assert!(!drawn(cx, "i-trigger-menu"), "unmatched: hidden");
}

#[gpui::test]
fn fr15_composition_keys_never_pick_or_move(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let handle = ChatComposerInputHandle::new();
    let cx = input_host(cx, &log, {
        let handle = handle.clone();
        move |i| i.triggers([mention()]).handle(handle.clone())
    });
    click(cx, "i-editor");
    typed(cx, "@a");
    assert!(
        drawn(cx, "i-trigger-menu-highlighted-ada"),
        "the first row is highlighted"
    );
    // An IME composition is in progress: its keys are the IME's.
    let editor = handle.editor().expect("drawn");
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| {
            s.replace_and_mark_text_in_range(None, "l", None, window, cx)
        })
    });
    settle(cx);
    key(cx, "down");
    key(cx, "enter");
    assert!(
        !last(&log).contains('\u{A0}'),
        "no pick during the composition"
    );
    cx.update(|window, cx| editor.update(cx, |s, cx| s.unmark_text(window, cx)));
    settle(cx);
    key(cx, "down");
    assert!(
        drawn(cx, "i-trigger-menu-highlighted-alan") || drawn(cx, "i-trigger-menu-highlighted-ada")
    );
}

#[gpui::test]
fn fr16_escape_dismisses_while_the_query_grows_and_outside_presses_close(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = input_host(cx, &log, |i| i.triggers([mention()]));
    click(cx, "i-editor");
    typed(cx, "@a");
    assert!(drawn(cx, "i-trigger-menu"));
    key(cx, "escape");
    assert!(!drawn(cx, "i-trigger-menu"));
    assert_eq!(last(&log), "@a", "the text stays");
    typed(cx, "d");
    assert!(
        !drawn(cx, "i-trigger-menu"),
        "hidden while the same query grows"
    );
    typed(cx, " @g");
    assert!(drawn(cx, "i-trigger-menu-grace"), "a new query opens it");
    // A press in the editor keeps it; one elsewhere closes it.
    click(cx, "i-editor");
    typed(cx, "");
    cx.simulate_click(gpui::point(px(5.), px(5.)), Modifiers::none());
    settle(cx);
    assert!(!drawn(cx, "i-trigger-menu"), "closed by an outside press");
}

#[gpui::test]
fn fr17_the_return_key_says_send_by_default_and_enter_sends_under_any_label(
    cx: &mut TestAppContext,
) {
    let hint = |cx: &mut TestAppContext, set: Option<gpui::TextInputAction>| {
        let log: Log = Rc::default();
        let handle = ChatComposerInputHandle::new();
        let cx = input_host(cx, &log, {
            let handle = handle.clone();
            move |i| {
                let i = i.handle(handle.clone());
                match set {
                    Some(h) => i.enter_key_hint(h),
                    None => i,
                }
            }
        });
        let editor = handle.editor().expect("drawn");
        let action = cx.update(|window, cx| {
            editor.update(cx, |s, cx| {
                s.text_input_configuration(window, cx).input_action
            })
        });
        click(cx, "i-editor");
        typed(cx, "go");
        key(cx, "enter");
        assert!(log.borrow().contains(&"sent:go".to_string()), "Enter sends");
        action
    };
    assert_eq!(hint(cx, None), gpui::TextInputAction::Send);
    assert_eq!(
        hint(cx, Some(gpui::TextInputAction::Enter)),
        gpui::TextInputAction::Enter
    );
}

#[gpui::test]
fn fr18_one_backspace_removes_a_token_and_its_space(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = input_host(cx, &log, |i| i.triggers([mention()]));
    click(cx, "i-editor");
    typed(cx, "hi @gr");
    key(cx, "enter");
    assert_eq!(last(&log), "hi @grace\u{A0}");
    key(cx, "backspace");
    assert_eq!(last(&log), "hi ", "the text as before the token");
}

#[gpui::test]
fn ar5_a_hidden_menu_leaves_the_arrows_to_the_editor(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = input_host(cx, &log, |i| i.triggers([mention()]));
    click(cx, "i-editor");
    typed(cx, "earlier");
    key(cx, "enter");
    typed(cx, "@a");
    key(cx, "escape");
    key(cx, "home");
    key(cx, "up");
    assert_eq!(
        last(&log),
        "earlier",
        "Up recalls history under a dismissed menu"
    );
}

#[gpui::test]
fn restored_drafts_deserialize_their_tokens(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let cx = input_host(cx, &log, |i| i.triggers([mention()]).value("ping @ada now"));
    assert!(
        drawn(cx, "chat-token-@ada"),
        "the value's token is a token again"
    );
}

// =============================================================================
// ChatComposerTokenElement and the pasted-text token

#[gpui::test]
fn token_element_fr1_fr3_renders_its_badge_or_custom_content(cx: &mut TestAppContext) {
    let renders = Rc::new(RefCell::new(0));
    let cx = host(cx, {
        let renders = renders.clone();
        move || {
            let renders = renders.clone();
            div()
                .child(ChatComposerTokenElement::new(ChatComposerToken::badge(
                    "@ada", "@Ada",
                )))
                .child(ChatComposerTokenElement::new(ChatComposerToken::custom(
                    "#x",
                    move |_, _| {
                        *renders.borrow_mut() += 1;
                        div()
                            .debug_selector(|| "custom".into())
                            .size(px(12.))
                            .into_any_element()
                    },
                )))
                .into_any_element()
        }
    });
    assert!(drawn(cx, "chat-token-@ada"));
    assert!(drawn(cx, "custom") && *renders.borrow() > 0);
    let token = ChatComposerToken::badge("@ada", "@Ada");
    assert_eq!(token.value().as_ref(), "@ada", "the value is kept exactly");
    assert!(!token.is_custom());
}

#[gpui::test]
fn pasted_text_labels_count_lines_and_characters(cx: &mut TestAppContext) {
    let cx = host(cx, || div().into_any_element());
    let (one, three) = cx.update(|_, cx| {
        (
            format_pasted_text_label(&"a".repeat(240), cx),
            format_pasted_text_label("ab\ncd\nef", cx),
        )
    });
    assert_eq!(one.as_ref(), "240 chars");
    assert_eq!(three.as_ref(), "3 lines, 8 chars");
}

#[gpui::test]
fn expanding_a_pasted_token_dissolves_it_into_text(cx: &mut TestAppContext) {
    let log: Log = Rc::default();
    let handle = ChatComposerInputHandle::new();
    let cx = input_host(cx, &log, {
        let handle = handle.clone();
        move |i| i.handle(handle.clone())
    });
    click(cx, "i-editor");
    let id = cx.update(|window, cx| {
        handle.insert_token(ChatComposerToken::badge("long text", "9 chars"), window, cx)
    });
    settle(cx);
    cx.update(|window, cx| handle.expand_token(id.unwrap(), window, cx));
    settle(cx);
    assert_eq!(last(&log), "long text");
}
