//! Dialog and AlertDialog tests, numbered by the upstream specs' FR items.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, Modifiers, ParentElement, Render,
    Styled, TestAppContext, VisualTestContext, Window, div, point, prelude::FluentBuilder as _, px,
};

use super::*;
use crate::button::Button;

type Log = Rc<RefCell<Vec<&'static str>>>;

/// A page with an opener; the dialog is open while `open` holds.
struct Page {
    open: Rc<Cell<bool>>,
    build: Rc<dyn Fn(Rc<Cell<bool>>) -> AnyElement>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let open = self.open.clone();
        div()
            .p(px(20.))
            .child(Button::new("opener", "Open").on_click(move |_, window, _| {
                open.set(true);
                window.refresh();
            }))
            .child((self.build)(self.open.clone()))
    }
}

fn page(
    cx: &mut TestAppContext,
    build: impl Fn(Rc<Cell<bool>>) -> AnyElement + 'static,
) -> (&mut VisualTestContext, Rc<Cell<bool>>) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
    });
    let open = Rc::new(Cell::new(false));
    let (_, cx) = cx.add_window_view({
        let open = open.clone();
        move |_, _| Page {
            open: open.clone(),
            build: Rc::new(build),
        }
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, open)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

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

fn shown(cx: &mut VisualTestContext) -> bool {
    settle(cx);
    cx.debug_bounds("d-content").is_some()
}

fn dialog(purpose: DialogPurpose, log: Log) -> impl Fn(Rc<Cell<bool>>) -> AnyElement + 'static {
    move |open| {
        let (first, second) = (log.clone(), log.clone());
        let close = open.clone();
        Dialog::new("d", open.get())
            .purpose(purpose)
            .on_open_change(move |is_open, window, _| {
                close.set(is_open);
                window.refresh();
            })
            .child(DialogHeader::new("Rename file"))
            .child(
                div()
                    .debug_selector(|| "d-content".into())
                    .flex()
                    .gap(px(8.))
                    .child(
                        Button::new("first", "First")
                            .on_click(move |_, _, _| first.borrow_mut().push("first")),
                    )
                    .child(
                        Button::new("second", "Second")
                            .on_click(move |_, _, _| second.borrow_mut().push("second")),
                    ),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn fr2_focus_enters_on_the_title_and_fr7_tab_wraps_inside(cx: &mut TestAppContext) {
    let log = Log::default();
    let (cx, _) = page(cx, dialog(DialogPurpose::Info, log.clone()));
    press(cx, "tab");
    press(cx, "enter");
    assert!(shown(cx));
    // From the title, Tab reaches the first control.
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["first"]);
    press(cx, "tab");
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(
        *log.borrow(),
        ["first", "first"],
        "Tab wrapped inside the dialog"
    );
}

#[gpui::test]
fn fr4_escape_closes_an_info_dialog_and_focus_returns(cx: &mut TestAppContext) {
    let (cx, open) = page(cx, dialog(DialogPurpose::Info, Log::default()));
    press(cx, "tab");
    press(cx, "enter");
    assert!(shown(cx));
    press(cx, "escape");
    assert!(!open.get() && !shown(cx));
    press(cx, "enter");
    assert!(shown(cx), "focus was back on the opener");
}

#[gpui::test]
fn a_backdrop_press_closes_an_info_dialog(cx: &mut TestAppContext) {
    let (cx, open) = page(cx, dialog(DialogPurpose::Info, Log::default()));
    open.set(true);
    assert!(shown(cx));
    cx.simulate_click(point(px(5.), px(5.)), Modifiers::none());
    assert!(!open.get());
}

#[gpui::test]
fn a_form_dialog_keeps_its_input_on_a_backdrop_press(cx: &mut TestAppContext) {
    let (cx, open) = page(cx, dialog(DialogPurpose::Form, Log::default()));
    open.set(true);
    assert!(shown(cx));
    cx.simulate_click(point(px(5.), px(5.)), Modifiers::none());
    assert!(open.get());
    press(cx, "escape");
    assert!(!open.get(), "Escape still closes it");
}

#[gpui::test]
fn a_required_dialog_ignores_escape_and_the_backdrop(cx: &mut TestAppContext) {
    let (cx, open) = page(cx, dialog(DialogPurpose::Required, Log::default()));
    open.set(true);
    assert!(shown(cx));
    press(cx, "escape");
    cx.simulate_click(point(px(5.), px(5.)), Modifiers::none());
    assert!(open.get() && shown(cx));
}

#[gpui::test]
fn alert_dialog_focuses_cancel_and_its_action_does_not_close(cx: &mut TestAppContext) {
    let log = Log::default();
    let (cx, open) = page(cx, {
        let log = log.clone();
        move |open| {
            let close = open.clone();
            let log = log.clone();
            div()
                .debug_selector(|| "d-content".into())
                .when(open.get(), |el| el.child("open"))
                .child(
                    AlertDialog::new(
                        "a",
                        open.get(),
                        "Delete project?",
                        "This cannot be undone.",
                        "Delete project",
                    )
                    .on_action(move |_, _| log.borrow_mut().push("delete"))
                    .on_open_change(move |is_open, window, _| {
                        close.set(is_open);
                        window.refresh();
                    }),
                )
                .into_any_element()
        }
    });
    press(cx, "tab");
    press(cx, "enter");
    assert!(open.get());
    // Cancel has focus: Tab reaches the action, Enter runs it.
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["delete"]);
    assert!(open.get(), "the action leaves it open");
    press(cx, "shift-tab");
    press(cx, "enter");
    assert!(!open.get(), "Cancel closes it");
}

#[gpui::test]
fn the_imperative_dialog_shows_and_hides(cx: &mut TestAppContext) {
    let controller: Rc<RefCell<Option<DialogController>>> = Rc::default();
    let (cx, _) = page(cx, {
        let controller = controller.clone();
        move |_| {
            let controller = controller.clone();
            div().child(DialogElement(controller)).into_any_element()
        }
    });
    let shown_after = |cx: &mut VisualTestContext| shown(cx);
    assert!(!shown_after(cx));
    let c = controller.borrow().clone().expect("rendered");
    cx.update(|window, cx| {
        c.show(
            |_, _| {
                div()
                    .debug_selector(|| "d-content".into())
                    .child("Hello")
                    .into_any_element()
            },
            None,
            window,
            cx,
        )
    });
    assert!(shown_after(cx));
    assert!(cx.update(|_, cx| c.is_open(cx)));
    press(cx, "escape");
    assert!(!shown_after(cx), "Escape closed it");
}

/// Renders a controller's dialog, creating the controller on first render.
#[derive(IntoElement)]
struct DialogElement(Rc<RefCell<Option<DialogController>>>);

impl gpui::RenderOnce for DialogElement {
    fn render(self, window: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        let controller = DialogController::new("imperative", window, cx);
        *self.0.borrow_mut() = Some(controller.clone());
        controller.element(window, cx)
    }
}

#[gpui::test]
fn overlay_shows_on_hover_and_always(cx: &mut TestAppContext) {
    let (cx, _) = page(cx, |_| {
        div()
            .flex()
            .child(
                Overlay::new("hover")
                    .show_on(OverlayShowOn::Hover)
                    .content(
                        div()
                            .debug_selector(|| "hover-scrim".into())
                            .child("Caption"),
                    )
                    .child(div().w(px(200.)).h(px(100.))),
            )
            .child(
                Overlay::new("always")
                    .content(
                        div()
                            .debug_selector(|| "always-scrim".into())
                            .child("12 photos"),
                    )
                    .child(div().w(px(200.)).h(px(100.))),
            )
            .into_any_element()
    });
    settle(cx);
    assert!(cx.debug_bounds("always-scrim").is_some());
    assert!(
        cx.debug_bounds("hover-scrim").is_none(),
        "hidden until hovered"
    );
    // The hover overlay sits under the opener row: 20px padding, then the
    // 32px button, then the overlays.
    cx.simulate_mouse_move(point(px(100.), px(100.)), None, Modifiers::none());
    settle(cx);
    assert!(cx.debug_bounds("hover-scrim").is_some());
}
