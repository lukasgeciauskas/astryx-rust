//! Dialog and AlertDialog tests, numbered by the upstream specs' FR items.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, Modifiers, MouseButton,
    ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window, div, point,
    prelude::FluentBuilder as _, px,
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
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
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
            .child(
                crate::layout::Layout::new()
                    .header(DialogHeader::new("Rename file"))
                    .content(
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

// ----------------------------------------------------------------------------
// Drawer and BottomSheet

fn drawer(scrim: bool, clicks: Rc<Cell<usize>>) -> impl Fn(Rc<Cell<bool>>) -> AnyElement + 'static {
    move |open| {
        let close = open.clone();
        let clicks = clicks.clone();
        div()
            .child(
                div().debug_selector(|| "page-button".into()).child(
                    Button::new("page-button", "Page")
                        .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
                ),
            )
            .child(
                Drawer::new("d", open.get(), "Order 1042")
                    .scrim(scrim)
                    .on_open_change(move |is_open, window, _| {
                        close.set(is_open);
                        window.refresh();
                    })
                    .child(
                        crate::layout::Layout::new()
                            .header(DrawerHeader::new("Order 1042"))
                            .content(div().debug_selector(|| "d-content".into()).child("Details")),
                    ),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn a_drawer_closes_on_escape(cx: &mut TestAppContext) {
    let (cx, open) = page(cx, drawer(true, Rc::default()));
    open.set(true);
    assert!(shown(cx));
    press(cx, "escape");
    assert!(!open.get());
}

#[gpui::test]
fn a_drawer_without_a_scrim_leaves_the_page_interactive(cx: &mut TestAppContext) {
    let clicks: Rc<Cell<usize>> = Rc::default();
    let (cx, open) = page(cx, drawer(false, clicks.clone()));
    open.set(true);
    assert!(shown(cx));
    // The page button sits on its own line under the opener.
    cx.simulate_click(point(px(40.), px(68.)), Modifiers::none());
    assert_eq!(clicks.get(), 1, "the page took the press");
    assert!(open.get(), "and the drawer stayed open");
}

#[gpui::test]
fn a_bottom_sheet_swiped_down_dismisses(cx: &mut TestAppContext) {
    let (cx, open) = page(cx, |open| {
        let close = open.clone();
        BottomSheet::new("s", open.get(), "Filters")
            .on_open_change(move |is_open, window, _| {
                close.set(is_open);
                window.refresh();
            })
            .child(div().debug_selector(|| "d-content".into()).child("Filters"))
            .into_any_element()
    });
    open.set(true);
    assert!(shown(cx));
    let content = cx.debug_bounds("d-content").expect("shown");
    // The handle band is the sheet's top 24px, above the content.
    let handle = point(content.center().x, content.top() - px(20.));
    cx.simulate_mouse_down(handle, MouseButton::Left, Modifiers::none());
    settle(cx);
    cx.simulate_mouse_move(
        point(handle.x, px(760.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        point(handle.x, px(760.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert!(!open.get(), "pulled away, it closed");
}

#[gpui::test]
fn a_popover_can_present_as_a_sheet(cx: &mut TestAppContext) {
    let (cx, _) = page(cx, |_| {
        div()
            .child(
                Popover::new("p", "Filters")
                    .presentation(PopoverPresentation::BottomSheet)
                    .content(div().debug_selector(|| "d-content".into()).child("Options"))
                    .child(Button::new("trigger", "Filters")),
            )
            .into_any_element()
    });
    // The trigger sits on its own line under the opener.
    cx.simulate_click(point(px(40.), px(68.)), Modifiers::none());
    assert!(shown(cx));
    press(cx, "escape");
    // It slides away, then is gone.
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    assert!(!shown(cx));
}

// ----------------------------------------------------------------------------
// Toast

fn toast_page(cx: &mut TestAppContext) -> &mut VisualTestContext {
    let (cx, _) = page(cx, |_| ToastViewport::new().into_any_element());
    cx
}

fn saved(id: &'static str) -> ToastOptions {
    ToastOptions::element("Saved", move |_, _| {
        div()
            .debug_selector(move || id.into())
            .child("Saved")
            .into_any_element()
    })
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(ms));
    settle(cx);
}

#[gpui::test]
fn an_info_toast_shows_then_hides_after_five_seconds(cx: &mut TestAppContext) {
    let cx = toast_page(cx);
    cx.update(|window, cx| {
        show_toast(saved("toast-a"), window, cx);
    });
    settle(cx);
    assert!(cx.debug_bounds("toast-a").is_some());
    wait(cx, 4900);
    assert_eq!(cx.update(|_, cx| super::toast::queued(cx)), 1);
    wait(cx, 200);
    assert_eq!(
        cx.update(|_, cx| super::toast::queued(cx)),
        0,
        "it timed out"
    );
}

#[gpui::test]
fn hovering_a_toast_holds_it(cx: &mut TestAppContext) {
    let cx = toast_page(cx);
    cx.update(|window, cx| {
        show_toast(saved("toast-a"), window, cx);
    });
    settle(cx);
    let bounds = cx.debug_bounds("toast-a").expect("shown");
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    settle(cx);
    wait(cx, 8000);
    assert_eq!(
        cx.update(|_, cx| super::toast::queued(cx)),
        1,
        "held while hovered"
    );
    cx.simulate_mouse_move(point(px(5.), px(5.)), None, Modifiers::none());
    settle(cx);
    wait(cx, 5100);
    assert_eq!(cx.update(|_, cx| super::toast::queued(cx)), 0);
}

#[gpui::test]
fn an_error_toast_stays_and_a_handle_dismisses_it(cx: &mut TestAppContext) {
    let cx = toast_page(cx);
    let handle =
        cx.update(|window, cx| show_toast(saved("toast-a").kind(ToastType::Error), window, cx));
    settle(cx);
    wait(cx, 20_000);
    assert_eq!(
        cx.update(|_, cx| super::toast::queued(cx)),
        1,
        "errors stay"
    );
    cx.update(|window, cx| handle.dismiss(window, cx));
    settle(cx);
    assert_eq!(cx.update(|_, cx| super::toast::queued(cx)), 0);
}

#[gpui::test]
fn a_unique_id_deduplicates(cx: &mut TestAppContext) {
    let cx = toast_page(cx);
    cx.update(|window, cx| {
        show_toast(saved("toast-a").unique_id("save"), window, cx);
        show_toast(saved("toast-b").unique_id("save"), window, cx);
    });
    settle(cx);
    assert_eq!(cx.update(|_, cx| super::toast::queued(cx)), 1);
    assert!(
        cx.debug_bounds("toast-b").is_some(),
        "the new one replaced it"
    );
    cx.update(|window, cx| {
        show_toast(
            saved("toast-c")
                .unique_id("save")
                .collision(ToastCollision::Ignore),
            window,
            cx,
        );
    });
    settle(cx);
    assert!(
        cx.debug_bounds("toast-b").is_some(),
        "an ignored one is dropped"
    );
    assert!(cx.debug_bounds("toast-c").is_none());
}

#[gpui::test]
fn bottom_sheet_fr5_fr7_fr9_a_stack_dismisses_from_the_top(cx: &mut TestAppContext) {
    let inner = Rc::new(Cell::new(false));
    let log: Log = Log::default();
    let (cx, open) = page(cx, {
        let (inner, log) = (inner.clone(), log.clone());
        move |open| {
            let (close, inner_open, inner_close, opened_log) =
                (open.clone(), inner.clone(), inner.clone(), log.clone());
            div()
                .child(
                    BottomSheet::new("outer", open.get(), "Filters")
                        .on_open_change(move |is_open, window, _| {
                            close.set(is_open);
                            window.refresh();
                        })
                        .child(div().debug_selector(|| "d-content".into()).child(
                            Button::new("add", "Add filter").on_click(move |_, window, _| {
                                inner_open.set(true);
                                window.refresh();
                            }),
                        )),
                )
                .child(
                    BottomSheet::new("inner", inner.get(), "Add filter")
                        .on_open_change(move |is_open, window, _| {
                            inner_close.set(is_open);
                            window.refresh();
                        })
                        .child(
                            div()
                                .debug_selector(|| "inner-content".into())
                                .child(Button::new("inner-button", "Inner").on_click(
                                    move |_, _, _| opened_log.borrow_mut().push("inner"),
                                )),
                        ),
                )
                .into_any_element()
        }
    });
    open.set(true);
    assert!(shown(cx));
    // Open the second sheet from a control inside the first.
    press(cx, "tab");
    press(cx, "enter");
    settle(cx);
    assert!(inner.get());
    assert!(cx.debug_bounds("inner-content").is_some());
    // Escape reaches only the top sheet.
    press(cx, "escape");
    assert!(!inner.get(), "the top sheet closed");
    assert!(open.get(), "the sheet beneath stays");
    // Focus comes back, once its exit ends, to its opener inside the revealed
    // sheet (FR9): Enter opens the second sheet again.
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    settle(cx);
    press(cx, "enter");
    assert!(inner.get());
}
