//! NumberInput tests, numbered by the upstream spec's FR items.

use std::cell::RefCell;

use gpui::{MouseButton, ScrollDelta, ScrollEvent, TouchPhase};

use super::*;
use crate::input::number_input::{Decision, Rules, rejection, resolve_commit, stepped};

fn rules(min: Option<f64>, max: Option<f64>, integer_only: bool) -> Rules {
    Rules {
        min,
        max,
        integer_only,
    }
}

fn en() -> astryx_i18n::I18n {
    astryx_i18n::I18n::new("en")
}

// ----------------------------------------------------------------------------
// The commit and step rules

#[test]
fn fr3_commits_clamp_clear_or_revert() {
    let r = rules(Some(0.), Some(100.), false);
    assert_eq!(
        resolve_commit("150", r, false, &en()),
        Decision::Commit {
            value: 100.,
            clamped: true
        }
    );
    assert_eq!(
        resolve_commit("1,234", rules(None, None, false), false, &en()),
        Decision::Commit {
            value: 1234.,
            clamped: false
        }
    );
    assert_eq!(resolve_commit("  ", r, true, &en()), Decision::Clear);
    assert_eq!(resolve_commit("", r, false, &en()), Decision::Revert);
    assert_eq!(resolve_commit("abc", r, false, &en()), Decision::Revert);
    // An integer-only range rounds inward before it clamps.
    assert_eq!(
        resolve_commit("0", rules(Some(0.5), None, true), false, &en()),
        Decision::Commit {
            value: 1.,
            clamped: true
        }
    );
}

#[test]
fn fr15_rejections_name_their_kind() {
    let kind = |text: &str, r: Rules| rejection(text, r, &en()).map(|e| e.kind);
    assert_eq!(
        kind("1a", rules(None, None, false)),
        Some(EntryErrorKind::Unreadable)
    );
    assert_eq!(
        kind("-", rules(None, None, false)),
        Some(EntryErrorKind::Unreadable)
    );
    assert_eq!(
        kind("1.5", rules(None, None, true)),
        Some(EntryErrorKind::Fractional)
    );
    assert_eq!(
        kind("5", rules(Some(10.), Some(1.), false)),
        Some(EntryErrorKind::OutOfRange)
    );
    // Out of an ordinary range is no rejection: it clamps at commit.
    assert_eq!(kind("150", rules(Some(0.), Some(100.), false)), None);
    assert_eq!(kind("", rules(None, None, false)), None);
}

#[test]
fn fr4_steps_follow_the_grid() {
    let free = rules(None, None, false);
    assert_eq!(stepped(Some(2.), 1, free, None), Some(3.));
    assert_eq!(stepped(Some(2.5), 1, free, None), Some(3.));
    assert_eq!(stepped(Some(2.5), -1, free, None), Some(2.));
    // Decimal steps stay exact.
    assert_eq!(stepped(Some(0.1), 1, free, Some(0.1)), Some(0.2));
    assert_eq!(stepped(Some(0.3), -1, free, Some(0.1)), Some(0.2));
    // The grid starts at min.
    let from_one = rules(Some(1.), Some(10.), false);
    assert_eq!(stepped(Some(4.), 1, from_one, Some(2.)), Some(5.));
    assert_eq!(stepped(Some(10.), 1, from_one, Some(2.)), Some(10.));
    // From nothing: up to min (or 0), down to max (or 0).
    assert_eq!(stepped(None, 1, from_one, None), Some(1.));
    assert_eq!(stepped(None, -1, from_one, None), Some(10.));
    assert_eq!(stepped(None, 1, free, None), Some(0.));
}

// ----------------------------------------------------------------------------
// The field

#[derive(Clone, Default)]
struct Numbers {
    value: Rc<RefCell<Option<f64>>>,
    log: Rc<RefCell<Vec<String>>>,
}

impl Numbers {
    fn new(value: Option<f64>) -> Self {
        let numbers = Numbers::default();
        *numbers.value.borrow_mut() = value;
        numbers
    }

    fn value(&self) -> Option<f64> {
        *self.value.borrow()
    }

    fn log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }

    fn store(&self) -> impl Fn(Option<f64>, &mut Window, &mut App) + 'static {
        let (value, log) = (self.value.clone(), self.log.clone());
        move |next, window, _| {
            log.borrow_mut().push(format!("{next:?}"));
            *value.borrow_mut() = next;
            window.refresh();
        }
    }

    fn errors(&self) -> impl Fn(Option<NumberInputEntryError>, &mut Window, &mut App) + 'static {
        let log = self.log.clone();
        move |error, _, _| {
            log.borrow_mut()
                .push(format!("error:{:?}", error.map(|e| e.kind)))
        }
    }
}

struct NumberHost(Rc<dyn Fn() -> NumberInput>);

impl Render for NumberHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_start()
            .p(px(20.))
            .child((self.0)().label_hidden(true).width(px(300.)))
    }
}

/// A 300px field with a hidden label: its control spans x 20..320 and
/// y 20..52; with steppers, they fill x 303..319, increment above y 36.
fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> NumberInput + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| NumberHost(Rc::new(build)));
    // Blur and focus events reach listeners only in the active window.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx
}

fn focus_field(cx: &mut VisualTestContext) {
    cx.simulate_click(point(px(100.), px(36.)), Modifiers::none());
    cx.run_until_parked();
}

fn blur(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
}

fn select_all_and_type(cx: &mut VisualTestContext, text: &str) {
    press(cx, "secondary-a");
    type_text(cx, text);
}

#[gpui::test]
fn fr2_fr3_typing_drafts_and_blur_commits(cx: &mut TestAppContext) {
    let numbers = Numbers::new(Some(5.));
    let cx = host(cx, {
        let numbers = numbers.clone();
        move || NumberInput::new("n", "Count", numbers.value()).on_change(numbers.store())
    });
    focus_field(cx);
    select_all_and_type(cx, "42");
    assert!(numbers.log().is_empty(), "typing never reports");
    blur(cx);
    assert_eq!(numbers.log(), ["Some(42.0)"]);
}

#[gpui::test]
fn fr3_enter_commits_clamped_and_calls_on_enter(cx: &mut TestAppContext) {
    let numbers = Numbers::new(Some(5.));
    let cx = host(cx, {
        let numbers = numbers.clone();
        move || {
            let entered = numbers.log.clone();
            NumberInput::new("n", "Count", numbers.value())
                .max(100.)
                .on_change(numbers.store())
                .on_enter(move |_, _| entered.borrow_mut().push("enter".into()))
        }
    });
    focus_field(cx);
    select_all_and_type(cx, "150");
    press(cx, "enter");
    assert_eq!(numbers.log(), ["Some(100.0)", "enter"]);
}

#[gpui::test]
fn fr4_arrows_step_and_commit_at_once(cx: &mut TestAppContext) {
    let numbers = Numbers::new(Some(2.));
    let cx = host(cx, {
        let numbers = numbers.clone();
        move || {
            NumberInput::new("n", "Count", numbers.value())
                .min(0.)
                .max(3.)
                .on_change(numbers.store())
        }
    });
    focus_field(cx);
    press(cx, "up");
    press(cx, "up");
    assert_eq!(numbers.value(), Some(3.), "stepping stops at max");
    press(cx, "shift-down");
    assert_eq!(numbers.value(), Some(3.), "a modified arrow does not step");
    press(cx, "down");
    assert_eq!(numbers.log(), ["Some(3.0)", "Some(2.0)"]);
}

#[gpui::test]
fn fr4_the_wheel_steps_a_focused_field(cx: &mut TestAppContext) {
    let numbers = Numbers::new(Some(2.));
    let cx = host(cx, {
        let numbers = numbers.clone();
        move || NumberInput::new("n", "Count", numbers.value()).on_change(numbers.store())
    });
    let wheel = |cx: &mut VisualTestContext, y: f32| {
        cx.simulate_event(ScrollEvent {
            position: point(px(100.), px(36.)),
            delta: ScrollDelta::Lines(point(0., y)),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        });
        cx.run_until_parked();
    };
    wheel(cx, 1.);
    assert!(
        numbers.log().is_empty(),
        "an unfocused field lets the page scroll"
    );
    focus_field(cx);
    wheel(cx, 1.);
    wheel(cx, -1.);
    wheel(cx, -1.);
    assert_eq!(numbers.log(), ["Some(3.0)", "Some(2.0)", "Some(1.0)"]);
}

#[gpui::test]
fn fr5_steppers_click_and_hold(cx: &mut TestAppContext) {
    let numbers = Numbers::new(Some(2.));
    let cx = host(cx, {
        let numbers = numbers.clone();
        move || {
            NumberInput::new("n", "Count", numbers.value())
                .number_steppers(true)
                .on_change(numbers.store())
        }
    });
    let up = point(px(311.), px(28.));
    cx.simulate_click(up, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(numbers.log(), ["Some(3.0)"]);
    // Held: repeats after 0.5 s, then commits once on release.
    cx.simulate_mouse_down(up, MouseButton::Left, Modifiers::none());
    cx.executor()
        .advance_clock(Duration::from_millis(500 + 250 + 225));
    cx.run_until_parked();
    assert_eq!(
        numbers.log(),
        ["Some(3.0)"],
        "a hold shows a draft, no commit"
    );
    cx.simulate_mouse_up(up, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(numbers.log(), ["Some(3.0)", "Some(6.0)"]);
}

#[gpui::test]
fn fr8_clear_reports_none(cx: &mut TestAppContext) {
    let numbers = Numbers::new(Some(7.));
    let cx = host(cx, {
        let numbers = numbers.clone();
        move || {
            NumberInput::new("n", "Count", numbers.value())
                .clear(true)
                .on_change(numbers.store())
        }
    });
    cx.simulate_click(point(px(300.), px(36.)), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(numbers.log(), ["None"]);
}

#[gpui::test]
fn fr9_fr10_disabled_and_read_only_fields_never_step(cx: &mut TestAppContext) {
    for disabled in [true, false] {
        let numbers = Numbers::new(Some(2.));
        let mut app = cx.clone();
        let cx = host(&mut app, {
            let numbers = numbers.clone();
            move || {
                NumberInput::new("n", "Count", numbers.value())
                    .disabled(disabled)
                    .read_only(!disabled)
                    .number_steppers(true)
                    .on_change(numbers.store())
            }
        });
        focus_field(cx);
        press(cx, "up");
        select_all_and_type(cx, "9");
        blur(cx);
        cx.simulate_click(point(px(311.), px(28.)), Modifiers::none());
        cx.run_until_parked();
        assert!(numbers.log().is_empty(), "disabled: {disabled}");
    }
}

#[gpui::test]
fn fr15_fr16_a_rejected_draft_stays_and_reports(cx: &mut TestAppContext) {
    let numbers = Numbers::new(Some(5.));
    let cx = host(cx, {
        let numbers = numbers.clone();
        move || {
            NumberInput::new("n", "Count", numbers.value())
                .on_change(numbers.store())
                .on_entry_error_change(numbers.errors())
        }
    });
    focus_field(cx);
    select_all_and_type(cx, "12a");
    assert!(numbers.log().is_empty(), "nothing shows while typing");
    press(cx, "enter");
    assert_eq!(numbers.log(), ["error:Some(Unreadable)"]);
    // Fixing the draft clears the rejection at once; Enter then commits.
    press(cx, "backspace");
    press(cx, "enter");
    assert_eq!(
        numbers.log(),
        ["error:Some(Unreadable)", "error:None", "Some(12.0)"]
    );
}
