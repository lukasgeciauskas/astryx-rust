//! Upstream `navigationTransition.test.ts` (the layers, the release rule, progress from
//! travel and from a caught screen) and `NavigationStack.test.tsx` (the root, push and
//! pop, Back's label, focus, popping to the root, large titles, end content, the screens
//! kept but not drawn, and `useNavigationStack` outside a stack), with the back swipe of
//! `NavigationStack.interaction.browser.spec.ts`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{Context, Modifiers, MouseButton, Render, TestAppContext, VisualTestContext, point};

use super::*;

fn close(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.
}

// ---------------------------------------------------------------------------
// navigationTransition.test.ts

#[test]
fn layers_run_from_a_landed_push_to_a_landed_pop() {
    let at0 = layers_at(0., 400., Direction::Ltr);
    assert_eq!((at0.top_x, at0.dim), (0., 1.));
    assert!(close(at0.under_x, -120., 9));
    let at1 = layers_at(1., 400., Direction::Ltr);
    assert_eq!((at1.top_x, at1.under_x, at1.dim), (400., 0., 0.));
    // Right to left every x flips.
    let rtl = layers_at(0.5, 400., Direction::Rtl);
    assert_eq!(rtl.top_x, -200.);
    assert!(close(rtl.under_x, 60., 9));
    // The dimming stays within 0 and 1.
    assert_eq!(layers_at(1.5, 400., Direction::Ltr).dim, 0.);
    assert_eq!(layers_at(-0.5, 400., Direction::Ltr).dim, 1.);
}

#[test]
fn layer_velocities_keep_every_layer_on_the_progress_curve() {
    let v = layer_velocities(2., 400., Direction::Ltr);
    assert_eq!(v.top_x, 800.);
    assert!(close(v.under_x, 240., 9));
    assert_eq!(v.dim, -2.);
    assert_eq!(programmatic_velocity(TransitionKind::Push), -8.773);
    assert_eq!(programmatic_velocity(TransitionKind::Pop), 8.773);
}

#[test]
fn a_released_pop_finishes_past_half_with_0_35_s_of_its_velocity() {
    // A still finger at 0.475 cancels; at 0.05 a flick needs about 2 widths a second.
    assert!(!should_finish_pop(0.475, 0.));
    assert!(should_finish_pop(0.51, 0.));
    assert!(!should_finish_pop(0.05, 1.2));
    assert!(should_finish_pop(0.05, 1.3));
    // A release moving back toward the start cancels whatever the travel.
    assert!(!should_finish_pop(0.9, -0.1));
}

#[test]
fn progress_follows_the_finger_and_resists_past_the_start() {
    assert!(close(
        progress_from_travel(0., 100., 400., Direction::Ltr),
        0.25,
        12
    ));
    assert!(close(
        progress_from_travel(0.2, 100., 400., Direction::Ltr),
        0.45,
        12
    ));
    assert_eq!(progress_from_travel(0.5, 1000., 400., Direction::Ltr), 1.);
    assert!(close(
        progress_from_travel(0., -100., 400., Direction::Rtl),
        0.25,
        12
    ));
    let back = progress_from_travel(0., -10., 400., Direction::Ltr);
    assert!(back < 0. && back > -10. / 400.);
    assert_eq!(progress_from_travel(0.3, 10., 0., Direction::Ltr), 0.3);
    // Caught where it shows, the screen resumes from the same progress.
    for x in [-20., 0., 37.5, 200., 800.] {
        let p = progress_from_screen(x, 400., Direction::Ltr);
        let shown = layers_at(p, 400., Direction::Ltr).top_x;
        let expected = if x < 0. {
            rubber_band_with(p * 400., 400., INTERACTIVE_POP_RUBBER_BAND)
        } else {
            x.min(400.)
        };
        assert!(
            close(shown.max(expected.min(shown)), expected.min(400.), 6) || x < 0.,
            "{x}"
        );
    }
    assert!(close(
        progress_from_travel(
            progress_from_screen(-5., 400., Direction::Ltr),
            0.,
            400.,
            Direction::Ltr
        ) * 400.,
        -5.,
        6
    ));
    assert!(close(pop_velocity(800., 400., Direction::Ltr), 2., 12));
    assert!(close(pop_velocity(800., 400., Direction::Rtl), -2., 12));
}

use rustryx_primitives::motion::rubber_band_with;

// ---------------------------------------------------------------------------
// NavigationStack.test.tsx

type Path = Rc<RefCell<Vec<SharedString>>>;

/// A row that pushes `next` through `use_navigation_stack`.
#[derive(IntoElement)]
struct PushRow {
    id: &'static str,
    next: SharedString,
}

thread_local! {
    /// Each row's focus handle, as last rendered.
    static ROWS: RefCell<std::collections::HashMap<&'static str, FocusHandle>> = RefCell::default();
}

impl RenderOnce for PushRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let api = use_navigation_stack::<SharedString>(cx);
        let id = ElementId::Name(self.id.into());
        let focus = control_focus(&id, window, cx);
        ROWS.with(|r| r.borrow_mut().insert(self.id, focus.clone()));
        let selector = self.id;
        div()
            .id(id)
            .debug_selector(move || selector.to_string())
            .track_focus(&focus)
            .h(px(44.))
            .child(self.next.clone())
            .on_click(move |_, window, cx| api.push(self.next.clone(), window, cx))
    }
}

struct Host {
    path: Path,
    back_titles: bool,
    large: bool,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (path, view) = (self.path.clone(), cx.entity());
        let (back_titles, large) = (self.back_titles, self.large);
        let mut root = NavigationScreen::new("Settings").child(PushRow {
            id: "push-general",
            next: "General".into(),
        });
        if back_titles {
            root = root.back_title("Back home");
        }
        if large {
            root = root.title_display_mode(TitleDisplayMode::Large);
        }
        div().w(px(400.)).h(px(700.)).child(
            NavigationStack::new("stack", root, self.path.borrow().clone())
                .label("Settings navigation")
                .get_key(|entry: &SharedString, _| entry.clone())
                .render_screen(move |entry: &SharedString, index, _, _| {
                    let title = if entry.as_ref() == "Untitled" {
                        SharedString::default()
                    } else {
                        entry.clone()
                    };
                    let mut screen = NavigationScreen::new(title)
                        .end_content(div().debug_selector(|| "end-content".into()).child("Edit"))
                        .child(PushRow {
                            id: if index == 0 {
                                "push-about"
                            } else {
                                "push-deeper"
                            },
                            next: format!("{entry} {index}").into(),
                        });
                    if large {
                        screen = screen.title_display_mode(TitleDisplayMode::Large);
                    }
                    screen
                })
                .on_path_change(move |next, _, cx| {
                    *path.borrow_mut() = next;
                    view.update(cx, |_, cx| cx.notify());
                }),
        )
    }
}

fn mount<'a>(
    cx: &'a mut TestAppContext,
    initial: &[&str],
    back_titles: bool,
    large: bool,
) -> (Entity<Host>, &'a mut VisualTestContext, Path) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let path: Path = Rc::new(RefCell::new(
        initial
            .iter()
            .map(|s| SharedString::from(s.to_string()))
            .collect(),
    ));
    let (view, cx) = cx.add_window_view({
        let path = path.clone();
        move |_, _| Host {
            path,
            back_titles,
            large,
        }
    });
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    (view, cx, path)
}

fn drawn() -> Drawn {
    DRAWN
        .with(|d| d.borrow().get("stack").cloned())
        .expect("the stack drew")
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    for _ in 0..(ms / 16).max(1) {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn set_path(view: &Entity<Host>, cx: &mut VisualTestContext, path: &Path, next: &[&str]) {
    *path.borrow_mut() = next
        .iter()
        .map(|s| SharedString::from(s.to_string()))
        .collect();
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    cx.run_until_parked();
}

fn focused_title(cx: &mut VisualTestContext) -> Option<SharedString> {
    let screens = drawn().screens;
    cx.update(|window, _| {
        screens
            .iter()
            .find(|s| s.title_focus.is_focused(window))
            .map(|s| s.key.clone())
    })
}

#[gpui::test]
fn renders_the_root_screen_with_its_title_and_no_back_button(cx: &mut TestAppContext) {
    let (_, cx, _) = mount(cx, &[], false, false);
    let d = drawn();
    assert_eq!(d.screens.len(), 1);
    assert_eq!(d.screens[0].title.as_ref(), "Settings");
    assert!(d.back.is_none());
    assert!(cx.debug_bounds("navigation-back").is_none());
}

#[gpui::test]
fn pushes_a_screen_its_title_shows_back_reads_the_previous_title_the_root_is_covered(
    cx: &mut TestAppContext,
) {
    let (_, cx, path) = mount(cx, &[], false, false);
    let row = cx.debug_bounds("push-general").unwrap().center();
    cx.simulate_click(row, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*path.borrow(), [SharedString::from("General")]);
    let d = drawn();
    assert_eq!(d.screens.len(), 2);
    assert_eq!(d.screens[1].title.as_ref(), "General");
    assert!(d.screens[0].covered && !d.screens[1].covered);
    assert_eq!(d.back.as_deref(), Some("Settings"));
    // The push never waits: the new screen starts sliding in from the trailing edge.
    assert!(d.screens[1].card_x > 0.);
    wait(cx, 1000);
    assert!(close(drawn().screens[1].card_x, 0., 3));
    assert!(close(drawn().screens[0].card_x, -120., 1));
}

#[gpui::test]
fn pops_with_the_back_button_the_popped_screen_leaves_once_its_slide_out_ends(
    cx: &mut TestAppContext,
) {
    let (_, cx, path) = mount(cx, &["General"], false, false);
    let back = cx
        .debug_bounds("navigation-back")
        .expect("Back shows")
        .center();
    cx.simulate_click(back, Modifiers::none());
    cx.run_until_parked();
    assert!(path.borrow().is_empty());
    let d = drawn();
    assert_eq!(
        d.leaving.as_deref(),
        Some("screen-General"),
        "the popped screen slides out"
    );
    assert_eq!(d.screens.len(), 2);
    wait(cx, 1000);
    let d = drawn();
    assert!(d.leaving.is_none());
    assert_eq!(d.screens.len(), 1);
    assert!(close(d.screens[0].card_x, 0., 3));
}

#[gpui::test]
fn moves_focus_to_the_new_title_after_a_push_and_back_to_the_pushing_control_after_the_pop(
    cx: &mut TestAppContext,
) {
    let (view, cx, path) = mount(cx, &[], false, false);
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*path.borrow(), [SharedString::from("General")]);
    wait(cx, 1000);
    assert_eq!(focused_title(cx).as_deref(), Some("screen-General"));
    set_path(&view, cx, &path, &[]);
    wait(cx, 1000);
    let row_focus = ROWS
        .with(|r| r.borrow().get("push-general").cloned())
        .expect("the row rendered");
    assert!(
        cx.update(|window, _| row_focus.is_focused(window)),
        "focus returns to the row that pushed"
    );
}

#[gpui::test]
fn keeps_focus_a_person_moved_into_the_new_screen_before_the_push_settles(cx: &mut TestAppContext) {
    let (_, cx, path) = mount(cx, &[], false, false);
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(path.borrow().len(), 1);
    // The covered root is inert: Tab reaches the new screen's row at once.
    press(cx, "tab");
    wait(cx, 1000);
    let row = ROWS
        .with(|r| r.borrow().get("push-about").cloned())
        .expect("the new screen's row rendered");
    assert!(
        cx.update(|window, _| row.is_focused(window)),
        "the push leaves focus where the person put it"
    );
}

#[gpui::test]
fn labels_back_with_the_previous_screens_back_title_when_it_has_one(cx: &mut TestAppContext) {
    let _ = mount(cx, &["General"], true, false);
    assert_eq!(drawn().back.as_deref(), Some("Back home"));
}

#[gpui::test]
fn names_back_with_the_catalogs_back_when_the_previous_screens_title_is_empty(
    cx: &mut TestAppContext,
) {
    let _ = mount(cx, &["Untitled", "Next"], false, false);
    assert_eq!(drawn().back.as_deref(), Some("Back"));
}

#[gpui::test]
fn pops_several_screens_back_to_the_root_at_once(cx: &mut TestAppContext) {
    let (view, cx, path) = mount(cx, &["General", "About", "Legal"], false, false);
    set_path(&view, cx, &path, &[]);
    wait(cx, 1000);
    let d = drawn();
    assert_eq!(d.screens.len(), 1);
    assert_eq!(d.screens[0].key.as_ref(), "root");
    assert!(d.back.is_none());
}

#[gpui::test]
fn a_large_title_shows_above_the_content_and_hides_the_bar_title(cx: &mut TestAppContext) {
    let _ = mount(cx, &[], false, true);
    let d = drawn();
    assert!(d.screens[0].large);
    assert_eq!(
        d.screens[0].title_opacity, 0.,
        "the bar's title hides while the large title shows"
    );
}

#[gpui::test]
fn moves_focus_to_the_pushed_screens_large_title(cx: &mut TestAppContext) {
    let (_, cx, path) = mount(cx, &[], false, true);
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(path.borrow().len(), 1);
    wait(cx, 1000);
    let d = drawn();
    assert!(
        d.screens[1].large && d.screens[1].title_opacity < 0.5,
        "the large title shows"
    );
    assert_eq!(focused_title(cx).as_deref(), Some("screen-General"));
}

#[gpui::test]
fn shows_a_screens_end_content_in_its_bar(cx: &mut TestAppContext) {
    let (_, cx, _) = mount(cx, &["General"], false, false);
    assert!(cx.debug_bounds("end-content").is_some());
}

#[gpui::test]
fn keeps_covered_screens_beyond_the_one_underneath_out_of_rendering(cx: &mut TestAppContext) {
    let _ = mount(cx, &["General", "About", "Legal"], false, false);
    let d = drawn();
    assert_eq!(
        d.screens.iter().map(|s| s.hidden).collect::<Vec<_>>(),
        [true, true, false, false]
    );
    assert_eq!(
        d.screens.iter().map(|s| s.covered).collect::<Vec<_>>(),
        [true, true, true, false]
    );
}

#[gpui::test]
fn a_back_swipe_past_half_pops_and_a_short_one_springs_back(cx: &mut TestAppContext) {
    let (_, cx, path) = mount(cx, &["General"], false, false);
    wait(cx, 100);
    let drag = |cx: &mut VisualTestContext, to: f32| {
        cx.simulate_mouse_down(
            point(px(40.), px(300.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        for i in 1..=12 {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.simulate_mouse_move(
                point(px(40. + to * i as f32 / 12.), px(300.)),
                Some(MouseButton::Left),
                Modifiers::default(),
            );
            cx.run_until_parked();
        }
        // Rest before the lift, so no flick velocity remains.
        for _ in 0..10 {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.simulate_mouse_move(
                point(px(40. + to), px(300.)),
                Some(MouseButton::Left),
                Modifiers::default(),
            );
            cx.run_until_parked();
        }
        cx.simulate_mouse_up(
            point(px(40. + to), px(300.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.run_until_parked();
    };
    drag(cx, 80.);
    wait(cx, 1000);
    assert_eq!(
        *path.borrow(),
        [SharedString::from("General")],
        "a short swipe springs back"
    );
    assert!(close(drawn().screens[1].card_x, 0., 2));
    drag(cx, 300.);
    wait(cx, 1000);
    assert!(path.borrow().is_empty(), "past half the width it pops");
}

#[gpui::test]
#[should_panic(expected = "useNavigationStack must be used inside a NavigationStack")]
fn use_navigation_stack_panics_outside_a_stack(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let _ = use_navigation_stack::<SharedString>(cx);
    });
}
