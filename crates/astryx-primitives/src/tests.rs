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
