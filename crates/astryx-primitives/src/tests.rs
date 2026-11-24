//! M0 spikes: the GPUI assumptions the rest of the port is built on.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, AvailableSpace, Context, ElementId, FocusHandle, IntoElement, KeyBinding, Modifiers,
    Render, RenderOnce, TestAppContext, Window, actions, div, point, prelude::*, px, size,
    uniform_list,
};

use crate::{ElementSize, provide, resolve_size};

type Seen = Rc<RefCell<Vec<(String, ElementSize)>>>;

#[derive(IntoElement)]
struct Probe {
    label: String,
    explicit: Option<ElementSize>,
    seen: Seen,
}

impl Probe {
    fn new(label: impl Into<String>, seen: &Seen) -> Self {
        Self {
            label: label.into(),
            explicit: None,
            seen: seen.clone(),
        }
    }
}

impl RenderOnce for Probe {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = resolve_size(self.explicit, ElementSize::Md, cx);
        self.seen.borrow_mut().push((self.label, size));
        div().w(px(10.)).h(px(10.))
    }
}

fn space() -> gpui::Size<AvailableSpace> {
    size(
        AvailableSpace::Definite(px(400.)),
        AvailableSpace::Definite(px(400.)),
    )
}

/// Spike (a): a provider's value reaches `RenderOnce` descendants, nested
/// providers shadow outer ones, an explicit prop wins, and siblings after a
/// provider no longer see it.
#[gpui::test]
fn context_reaches_render_once_children(cx: &mut TestAppContext) {
    let seen: Seen = Rc::default();
    let cx = cx.add_empty_window();
    let s = seen.clone();
    cx.draw(point(px(0.), px(0.)), space(), move |_, _| {
        div()
            .child(Probe::new("outside", &s))
            .child(provide(
                ElementSize::Sm,
                div()
                    .child(Probe::new("inner-sm", &s))
                    .child(provide(ElementSize::Lg, Probe::new("inner-lg", &s)))
                    .child(Probe {
                        explicit: Some(ElementSize::Md),
                        ..Probe::new("explicit", &s)
                    }),
            ))
            .child(Probe::new("after", &s))
    });

    let seen = seen.borrow();
    let expect = [
        ("outside", ElementSize::Md),
        ("inner-sm", ElementSize::Sm),
        ("inner-lg", ElementSize::Lg),
        ("explicit", ElementSize::Md),
        ("after", ElementSize::Md),
    ];
    for (label, size) in expect {
        let found = seen.iter().find(|(l, _)| l == label);
        assert_eq!(found.map(|(_, s)| *s), Some(size), "probe {label}");
    }
}

/// Spike (a): virtualized lists render their items during prepaint, which a
/// provider also covers.
#[gpui::test]
fn context_reaches_uniform_list_items(cx: &mut TestAppContext) {
    struct ListHost {
        seen: Seen,
    }

    impl Render for ListHost {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let seen = self.seen.clone();
            div().size_full().child(provide(
                ElementSize::Lg,
                uniform_list("items", 3, move |range, _, _| {
                    range
                        .map(|i| Probe::new(format!("item-{i}"), &seen))
                        .collect::<Vec<_>>()
                })
                .h(px(100.)),
            ))
        }
    }

    let seen: Seen = Rc::default();
    let s = seen.clone();
    let (_view, cx) = cx.add_window_view(|_, _| ListHost { seen: s });
    cx.run_until_parked();

    let seen = seen.borrow();
    assert!(
        seen.iter().any(|(l, _)| l == "item-2"),
        "items rendered: {seen:?}"
    );
    assert!(
        seen.iter().all(|(_, s)| *s == ElementSize::Lg),
        "every item sees the provider: {seen:?}"
    );
}

#[derive(IntoElement)]
struct Counter {
    id: ElementId,
    rendered: Rc<RefCell<Vec<usize>>>,
}

impl RenderOnce for Counter {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let count = window.use_keyed_state(self.id.clone(), cx, |_, _| 0usize);
        self.rendered.borrow_mut().push(*count.read(cx));
        div()
            .id(self.id)
            .size(px(100.))
            .on_click(move |_, _, cx| count.update(cx, |count, _| *count += 1))
    }
}

struct CounterHost {
    rendered: Rc<RefCell<Vec<usize>>>,
}

impl Render for CounterHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(Counter {
            id: "counter".into(),
            rendered: self.rendered.clone(),
        })
    }
}

/// Spike (b): uncontrolled state lives in `use_keyed_state`, survives
/// re-renders, and re-renders its owner when it changes.
#[gpui::test]
fn keyed_state_persists_across_frames(cx: &mut TestAppContext) {
    let rendered = Rc::new(RefCell::new(Vec::new()));
    let r = rendered.clone();
    let (_view, cx) = cx.add_window_view(|_, _| CounterHost { rendered: r });
    cx.run_until_parked();
    cx.simulate_click(point(px(10.), px(10.)), Modifiers::none());
    cx.run_until_parked();
    cx.simulate_click(point(px(10.), px(10.)), Modifiers::none());
    cx.run_until_parked();

    assert_eq!(rendered.borrow().last(), Some(&2), "renders: {rendered:?}");
}

actions!(spike, [Increment]);

struct KeyView {
    focus: FocusHandle,
    count: usize,
}

impl Render for KeyView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Spike")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Increment, _, cx| {
                this.count += 1;
                cx.notify();
            }))
            .size_full()
    }
}

/// Spike (f): keystrokes simulated in a test reach an action bound in a key
/// context, the way every component keymap will be tested.
#[gpui::test]
fn keystrokes_dispatch_actions(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys([KeyBinding::new("up", Increment, Some("Spike"))]));
    let (view, cx) = cx.add_window_view(|window, cx| {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        KeyView { focus, count: 0 }
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("up up up");

    assert_eq!(view.read_with(cx, |view, _| view.count), 3);
}

mod hotkey_tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{Context, IntoElement, Render, TestAppContext, Window, div, prelude::*};

    use crate::{Hotkey, hotkeys};

    struct Shortcuts {
        presses: Rc<Cell<usize>>,
        disabled: bool,
    }

    impl Render for Shortcuts {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let presses = self.presses.clone();
            hotkeys(
                [
                    Hotkey::new("ctrl+k", move |_, _, _| presses.set(presses.get() + 1))
                        .disabled(self.disabled),
                ],
                div().size_full(),
            )
        }
    }

    #[gpui::test]
    fn root_hotkeys_fire_without_focus(cx: &mut TestAppContext) {
        let presses = Rc::new(Cell::new(0));
        let (view, cx) = cx.add_window_view({
            let presses = presses.clone();
            move |_, _| Shortcuts {
                presses,
                disabled: false,
            }
        });
        cx.simulate_keystrokes("ctrl-k");
        assert_eq!(presses.get(), 1);
        cx.simulate_keystrokes("k");
        assert_eq!(presses.get(), 1);
        view.update(cx, |view, cx| {
            view.disabled = true;
            cx.notify();
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("ctrl-k");
        assert_eq!(presses.get(), 1);
    }

    struct Focused {
        presses: Rc<Cell<usize>>,
        handle: gpui::FocusHandle,
        consume: bool,
        typing: bool,
    }

    impl Render for Focused {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let presses = self.presses.clone();
            let consume = self.consume;
            let mut field = div().track_focus(&self.handle).size_full();
            if self.typing {
                field = field.key_context(crate::modality::TYPING_TARGET);
            }
            hotkeys(
                [Hotkey::new("escape", move |_, _, _| {
                    presses.set(presses.get() + 1)
                })],
                field.on_key_down(move |_, _, cx| {
                    if consume {
                        cx.stop_propagation();
                    }
                }),
            )
        }
    }

    fn focused_presses(cx: &mut TestAppContext, consume: bool, typing: bool) -> usize {
        let presses = Rc::new(Cell::new(0));
        let (view, cx) = cx.add_window_view({
            let presses = presses.clone();
            move |_, cx| Focused {
                presses,
                handle: cx.focus_handle(),
                consume,
                typing,
            }
        });
        cx.update(|window, cx| {
            let handle = view.read(cx).handle.clone();
            window.focus(&handle, cx);
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("escape");
        presses.get()
    }

    #[gpui::test]
    fn focused_handlers_and_typing_targets_win(cx: &mut TestAppContext) {
        assert_eq!(focused_presses(cx, false, false), 1);
        assert_eq!(focused_presses(cx, true, false), 0);
        assert_eq!(focused_presses(cx, false, true), 0);
    }
}

mod layer_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        Anchor, Context, IntoElement, Modifiers, Render, TestAppContext, Window, div, point,
        prelude::*, px,
    };

    use crate::{EscapeBehavior, layer};

    /// Two nested layers; records which one was dismissed.
    struct Nested {
        outer: bool,
        inner: bool,
        block_inner: bool,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Render for Nested {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let mut root = div().size_full();
            if self.outer {
                let entity = cx.entity();
                let dismiss = |name: &'static str, inner: bool| {
                    let log = self.log.clone();
                    let entity = entity.clone();
                    move |_: &mut Window, cx: &mut gpui::App| {
                        log.borrow_mut().push(name);
                        entity.update(cx, |this, cx| {
                            if inner {
                                this.inner = false;
                            } else {
                                this.outer = false;
                            }
                            cx.notify();
                        });
                    }
                };
                let escape = if self.block_inner {
                    EscapeBehavior::Block
                } else {
                    EscapeBehavior::Close
                };
                let inner = self.inner.then(|| {
                    layer(div().w(px(50.)).h(px(50.)), dismiss("inner", true))
                        .anchor(point(px(100.), px(100.)), Anchor::TopLeft)
                        .escape(escape)
                });
                root = root.child(
                    layer(
                        div().w(px(80.)).h(px(80.)).children(inner),
                        dismiss("outer", false),
                    )
                    .anchor(point(px(0.), px(0.)), Anchor::TopLeft),
                );
            }
            root
        }
    }

    fn open(
        cx: &mut TestAppContext,
        block_inner: bool,
    ) -> (Rc<RefCell<Vec<&'static str>>>, &mut gpui::VisualTestContext) {
        let log = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let log = log.clone();
            move |_, _| Nested {
                outer: true,
                inner: true,
                block_inner,
                log,
            }
        });
        cx.run_until_parked();
        (log, cx)
    }

    #[gpui::test]
    fn escape_closes_the_topmost_layer_first(cx: &mut TestAppContext) {
        let (log, cx) = open(cx, false);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(*log.borrow(), ["inner"]);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(*log.borrow(), ["inner", "outer"]);
    }

    #[gpui::test]
    fn a_blocking_layer_keeps_escape(cx: &mut TestAppContext) {
        let (log, cx) = open(cx, true);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(log.borrow().is_empty());
    }

    #[gpui::test]
    fn outside_press_closes_the_topmost_layer(cx: &mut TestAppContext) {
        let (log, cx) = open(cx, false);
        // Inside the inner layer: nothing closes.
        cx.simulate_click(point(px(120.), px(120.)), Modifiers::none());
        cx.run_until_parked();
        assert!(log.borrow().is_empty());
        // Inside the outer layer but outside the inner one: the inner closes.
        cx.simulate_click(point(px(10.), px(10.)), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(*log.borrow(), ["inner"]);
        // Outside both: the outer closes.
        cx.simulate_click(point(px(300.), px(300.)), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(*log.borrow(), ["inner", "outer"]);
    }
}

mod announce_tests {
    use std::time::Duration;

    use gpui::{Context, IntoElement, Render, TestAppContext, Window, div, prelude::*};

    use crate::announce::announcement;
    use crate::{Politeness, announce, announcer};

    struct Root;

    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(announcer())
        }
    }

    #[gpui::test]
    fn same_frame_messages_join_then_clear(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Root);
        cx.update(|window, cx| {
            announce("Saved", Politeness::Polite, window, cx);
            announce("3 items", Politeness::Polite, window, cx);
            announce("Saved", Politeness::Polite, window, cx);
        });
        let text = cx.update(|window, cx| announcement(Politeness::Polite, window, cx));
        assert_eq!(text, "Saved 3 items");
        cx.update(|window, cx| window.draw(cx).clear(cx));
        // A later frame's message replaces the text.
        cx.update(|window, cx| announce("Deleted", Politeness::Polite, window, cx));
        let text = cx.update(|window, cx| announcement(Politeness::Polite, window, cx));
        assert_eq!(text, "Deleted");
        cx.executor().advance_clock(Duration::from_millis(2100));
        cx.run_until_parked();
        let text = cx.update(|window, cx| announcement(Politeness::Polite, window, cx));
        assert_eq!(text, "");
        let assertive = cx.update(|window, cx| announcement(Politeness::Assertive, window, cx));
        assert_eq!(assertive, "");
    }
}
