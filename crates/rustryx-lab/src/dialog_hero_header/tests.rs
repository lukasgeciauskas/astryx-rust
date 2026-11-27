//! Upstream `DialogHeroHeader.test.tsx` and the `MediaBleedGeometry` story's geometry
//! guard.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, Modifiers, Render, SharedString, TestAppContext, VisualTestContext,
    Window, div, px,
};
use rustryx_components::button::Button;
use rustryx_components::layout::{Layout, LayoutContent, SpacingStep};
use rustryx_components::overlay::{AutoFocus, Dialog};
use rustryx_i18n::{Direction, I18n};

use super::*;

struct Host(Rc<dyn Fn() -> AnyElement>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    cx
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).expect("a keystroke");
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

fn media(name: &'static str) -> impl IntoElement {
    div()
        .w_full()
        .h(px(80.))
        .debug_selector(move || name.into())
}

type Log = Rc<RefCell<Vec<bool>>>;

#[test]
fn media_mode_maps_to_media_theme() {
    assert_eq!(
        media_theme_mode(Some(DialogHeroHeaderMediaMode::Dark)),
        MediaThemeMode::Dark
    );
    assert_eq!(
        media_theme_mode(Some(DialogHeroHeaderMediaMode::Light)),
        MediaThemeMode::Light
    );
    // Omitted: no MediaTheme surface (upstream `'off'`).
    assert_eq!(media_theme_mode(None), MediaThemeMode::Off);
}

#[gpui::test]
fn renders_a_string_title_and_the_media(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .w(px(400.))
            .child(DialogHeroHeader::new("Welcome aboard", media("hero")))
            .into_any_element()
    });
    assert!(cx.debug_bounds("dialog-hero-header-title").is_some());
    assert!(cx.debug_bounds("hero").is_some(), "the media slot renders");
}

#[gpui::test]
fn renders_a_caller_provided_heading_as_is(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .w(px(400.))
            .child(
                DialogHeroHeader::new("Custom heading", media("hero")).title_element(
                    div()
                        .debug_selector(|| "custom-heading".into())
                        .child(Heading::new(HeadingLevel::H3, "Custom heading")),
                ),
            )
            .into_any_element()
    });
    assert!(cx.debug_bounds("custom-heading").is_some());
}

#[gpui::test]
fn renders_start_content_before_the_title(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .w(px(400.))
            .child(
                DialogHeroHeader::new("Title", media("hero"))
                    .start_content(div().size(px(16.)).debug_selector(|| "start".into())),
            )
            .into_any_element()
    });
    let start = cx.debug_bounds("start").expect("the start content renders");
    let title = cx.debug_bounds("dialog-hero-header-title").expect("drawn");
    assert!(start.right() <= title.left(), "{start:?} before {title:?}");
}

#[gpui::test]
fn does_not_move_page_focus_outside_a_dialog(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .w(px(400.))
            .child(DialogHeroHeader::new("Title", media("hero")))
            .into_any_element()
    });
    assert!(cx.update(|window, cx| window.focused(cx)).is_none());
}

#[gpui::test]
fn the_close_button_renders_with_on_open_change_and_reports_false(cx: &mut TestAppContext) {
    let log = Log::default();
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let log = log.clone();
            div()
                .w(px(400.))
                .child(
                    DialogHeroHeader::new("Title", media("hero"))
                        .on_open_change(move |open, _, _| log.borrow_mut().push(open)),
                )
                .into_any_element()
        }
    });
    let close = cx
        .debug_bounds("dialog-hero-header-close")
        .expect("a close button");
    let hero = cx.debug_bounds("dialog-hero-header-media").expect("drawn");
    // Over the media's top-trailing corner.
    assert!(close.top() >= hero.top() && close.bottom() <= hero.bottom());
    assert!(close.right() <= hero.right() && close.center().x > hero.center().x);
    cx.simulate_click(close.center(), Modifiers::none());
    settle(cx);
    assert_eq!(*log.borrow(), [false], "called once, with false");
}

#[gpui::test]
fn no_close_button_without_on_open_change(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .w(px(400.))
            .child(DialogHeroHeader::new("Title", media("hero")))
            .into_any_element()
    });
    assert!(cx.debug_bounds("dialog-hero-header-close").is_none());
}

#[gpui::test]
fn the_focused_close_button_stays_focused_across_media_modes(cx: &mut TestAppContext) {
    let mode = Rc::new(Cell::new(None));
    let log = Log::default();
    let cx = host(cx, {
        let (mode, log) = (mode.clone(), log.clone());
        move || {
            let log = log.clone();
            let mut header = DialogHeroHeader::new("Title", media("hero"))
                .on_open_change(move |open, _, _| log.borrow_mut().push(open));
            if let Some(m) = mode.get() {
                header = header.media_mode(m);
            }
            div().w(px(400.)).child(header).into_any_element()
        }
    });
    press(cx, "tab");
    for next in [
        Some(DialogHeroHeaderMediaMode::Dark),
        Some(DialogHeroHeaderMediaMode::Light),
        None,
    ] {
        mode.set(next);
        settle(cx);
        // Still focused: Enter presses it.
        let before = log.borrow().len();
        press(cx, "enter");
        assert_eq!(
            log.borrow().len(),
            before + 1,
            "the close button kept focus at {next:?}"
        );
    }
}

fn hidden_header(cx: &mut TestAppContext, hidden: bool) -> (f32, bool) {
    let cx = host(cx, move || {
        div()
            .w(px(400.))
            .child(DialogHeroHeader::new("Same title", media("hero")).title_hidden(hidden))
            .into_any_element()
    });
    let header = cx.debug_bounds("layout-header").expect("drawn");
    (
        f32::from(header.size.height),
        cx.debug_bounds("dialog-hero-header-title").is_some(),
    )
}

#[gpui::test]
fn a_hidden_title_keeps_the_title_and_changes_the_row_presentation(cx: &mut TestAppContext) {
    let (hidden, kept) = hidden_header(cx, true);
    let (visible, _) = hidden_header(cx, false);
    assert!(kept, "the title stays rendered for screen readers");
    assert!(
        hidden < visible,
        "the hidden row takes no space: {hidden} vs {visible}"
    );
}

/// A page with a modal dialog holding a hero header and, after it, a Continue button.
fn modal(
    cx: &mut TestAppContext,
    header: impl Fn() -> DialogHeroHeader + 'static,
    continue_autofocus: bool,
    inline: bool,
) -> (&mut VisualTestContext, Rc<RefCell<Vec<&'static str>>>) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cx = host(cx, {
        let log = log.clone();
        move || {
            let (cont, close) = (log.clone(), log.clone());
            let button = Button::new("continue", "Continue")
                .on_click(move |_, _, _| cont.borrow_mut().push("continue"));
            let button = if continue_autofocus {
                rustryx_primitives::provide(AutoFocus, button).into_any_element()
            } else {
                button.into_any_element()
            };
            Dialog::new("d", true)
                .inline(inline)
                .on_open_change(|_, _, _| {})
                .child(header().on_open_change(move |_, _, _| close.borrow_mut().push("close")))
                .child(button)
                .into_any_element()
        }
    });
    (cx, log)
}

#[gpui::test]
fn focuses_the_visible_title_after_opening(cx: &mut TestAppContext) {
    let (cx, log) = modal(
        cx,
        || DialogHeroHeader::new("Welcome", media("hero")),
        false,
        false,
    );
    // The title holds focus: Enter presses nothing, and Shift-Tab reaches the close
    // button before it.
    press(cx, "enter");
    assert!(
        log.borrow().is_empty(),
        "focus is on the title, not a control"
    );
    press(cx, "shift-tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["close"]);
}

#[gpui::test]
fn a_custom_heading_takes_the_focus_too(cx: &mut TestAppContext) {
    let (cx, log) = modal(
        cx,
        || {
            DialogHeroHeader::new("Custom", media("hero"))
                .title_element(Heading::new(HeadingLevel::H1, "Custom"))
                .start_content(div().child("New"))
        },
        false,
        false,
    );
    press(cx, "enter");
    assert!(log.borrow().is_empty());
    press(cx, "tab");
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["continue"], "Tab moves on from the heading");
}

#[gpui::test]
fn honors_an_explicit_descendant_focus_request(cx: &mut TestAppContext) {
    let (cx, log) = modal(
        cx,
        || DialogHeroHeader::new("Welcome", media("hero")),
        true,
        false,
    );
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["continue"]);
}

#[gpui::test]
fn a_visually_hidden_title_requests_no_focus(cx: &mut TestAppContext) {
    let (cx, log) = modal(
        cx,
        || DialogHeroHeader::new("Hidden", media("hero")).title_hidden(true),
        false,
        false,
    );
    // Focus falls to the first control, the close button.
    press(cx, "enter");
    assert_eq!(*log.borrow(), ["close"]);
}

#[gpui::test]
fn does_not_auto_focus_the_title_in_inline_dialogs(cx: &mut TestAppContext) {
    let (cx, _) = modal(
        cx,
        || DialogHeroHeader::new("Inline title", media("hero")),
        false,
        true,
    );
    assert!(cx.update(|window, cx| window.focused(cx)).is_none());
}

fn header_height(cx: &mut TestAppContext, divider: Option<bool>, context: Option<bool>) -> f32 {
    let cx = host(cx, move || {
        let mut header = DialogHeroHeader::new("Title", media("hero"));
        if let Some(divider) = divider {
            header = header.divider(divider);
        }
        // The divider default comes from a Layout (upstream's `LayoutDividerContext`).
        match context {
            Some(d) => div()
                .w(px(400.))
                .h(px(300.))
                .child(Layout::new().default_has_dividers(d).header(header))
                .into_any_element(),
            None => div()
                .w(px(400.))
                .h(px(300.))
                .child(Layout::new().header(header))
                .into_any_element(),
        }
    });
    let bounds = cx.debug_bounds("layout-header").expect("a LayoutHeader");
    f32::from(bounds.size.height)
}

#[gpui::test]
fn renders_without_a_divider_by_default(cx: &mut TestAppContext) {
    let none = header_height(cx, None, None);
    let explicit_false = header_height(cx, Some(false), None);
    assert_eq!(none, explicit_false);
}

#[gpui::test]
fn inherits_the_divider_from_the_layout_and_takes_an_override(cx: &mut TestAppContext) {
    let context = header_height(cx, None, Some(true));
    let explicit = header_height(cx, Some(true), None);
    let overridden = header_height(cx, Some(false), Some(true));
    assert_eq!(context, explicit);
    assert_ne!(overridden, context);
}

#[gpui::test]
fn styles_reach_the_root(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        div()
            .w(px(400.))
            .child(DialogHeroHeader::new("Title", media("hero")).w(px(240.)))
            .into_any_element()
    });
    let root = cx.debug_bounds("layout-header").expect("drawn");
    assert_eq!(f32::from(root.size.width), 240.);
}

/// The `MediaBleedGeometry` story's guard: the media reaches the surface's
/// inline-start, inline-end and block-start edges for each dialog padding and in RTL,
/// while the title keeps the header's inset.
fn bleed(cx: &mut TestAppContext, padding: Option<SpacingStep>, rtl: bool) -> f32 {
    let cx = host(cx, move || {
        let mut dialog = Dialog::new("bleed", true).inline(true).width(px(320.));
        if let Some(padding) = padding {
            dialog = dialog.padding(padding);
        }
        let dialog = dialog.child(
            Layout::new()
                .header(
                    DialogHeroHeader::new("Fixture", media("hero"))
                        .media_mode(DialogHeroHeaderMediaMode::Dark)
                        .on_open_change(|_, _, _| {}),
                )
                .content(LayoutContent::new("body").child("Media bleed fixture")),
        );
        let page = div().p(px(24.)).child(
            div()
                .w(px(320.))
                .debug_selector(|| "surface".into())
                .child(dialog),
        );
        if rtl {
            rustryx_i18n::provider(I18n::new("ar").dir(Direction::Rtl), page).into_any_element()
        } else {
            page.into_any_element()
        }
    });
    let surface = cx.debug_bounds("surface").expect("the dialog");
    let media = cx
        .debug_bounds("dialog-hero-header-media")
        .expect("the media area");
    let title = cx
        .debug_bounds("dialog-hero-header-title")
        .expect("the title");
    let close = |a: gpui::Pixels, b: gpui::Pixels| (f32::from(a) - f32::from(b)).abs() <= 0.5;
    assert!(
        close(media.left(), surface.left()),
        "inline edge: {media:?} in {surface:?}"
    );
    assert!(
        close(media.right(), surface.right()),
        "inline edge: {media:?} in {surface:?}"
    );
    assert!(
        close(media.top(), surface.top()),
        "block-start edge: {media:?} in {surface:?}"
    );
    // The title keeps the inset the media cancels.
    let inset = if rtl {
        surface.right() - title.right()
    } else {
        title.left() - surface.left()
    };
    f32::from(inset)
}

#[gpui::test]
fn the_media_bleeds_to_the_dialog_edges_for_every_padding(cx: &mut TestAppContext) {
    let zero = bleed(cx, Some(SpacingStep::S0), false);
    let small = bleed(cx, Some(SpacingStep::S2), false);
    let large = bleed(cx, Some(SpacingStep::S8), false);
    let default = bleed(cx, None, false);
    assert_eq!(zero, 0.);
    assert!(
        small > zero && large > small,
        "0={zero}, 2={small}, 8={large}"
    );
    assert!(default > 0.);
    let rtl = bleed(cx, Some(SpacingStep::S8), true);
    assert!(
        (rtl - large).abs() <= 0.5,
        "RTL mirrors LTR: {rtl} vs {large}"
    );
}

/// Keeps the scope of the dialog it renders in: the focus requests and the name the
/// dialog takes from its header (what upstream's `aria-labelledby` points at).
#[derive(IntoElement)]
struct ScopeProbe(Rc<RefCell<Option<rustryx_components::overlay::FocusScope>>>);

impl RenderOnce for ScopeProbe {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        *self.0.borrow_mut() = rustryx_components::overlay::focus_scope(cx);
        div()
    }
}

type Scope = Rc<RefCell<Option<rustryx_components::overlay::FocusScope>>>;

/// A modal (or inline) dialog holding `header` and a probe of its scope.
fn named(
    cx: &mut TestAppContext,
    inline: bool,
    header: impl Fn() -> DialogHeroHeader + 'static,
) -> (&mut VisualTestContext, Scope) {
    let scope = Scope::default();
    let cx = host(cx, {
        let scope = scope.clone();
        move || {
            Dialog::new("d", true)
                .inline(inline)
                .on_open_change(|_, _, _| {})
                .child(header())
                .child(ScopeProbe(scope.clone()))
                .into_any_element()
        }
    });
    (cx, scope)
}

fn name_of(scope: &Scope) -> Option<SharedString> {
    scope.borrow().as_ref().and_then(|s| s.title_text())
}

#[gpui::test]
fn names_the_open_dialog_by_its_title(cx: &mut TestAppContext) {
    let (_, scope) = named(cx, false, || {
        DialogHeroHeader::new("Welcome aboard", media("hero"))
    });
    assert_eq!(name_of(&scope).as_deref(), Some("Welcome aboard"));
}

#[gpui::test]
fn still_names_the_dialog_when_the_title_is_hidden(cx: &mut TestAppContext) {
    let (_, scope) = named(cx, false, || {
        DialogHeroHeader::new("Hidden name", media("hero"))
            .title_hidden(true)
            .on_open_change(|_, _, _| {})
    });
    assert_eq!(name_of(&scope).as_deref(), Some("Hidden name"));
}

#[gpui::test]
fn uses_the_title_alone_as_the_name_including_custom_headings(cx: &mut TestAppContext) {
    let (_, scope) = named(cx, false, || {
        DialogHeroHeader::new("Welcome", media("hero")).start_content(div().child("New"))
    });
    assert_eq!(
        name_of(&scope).as_deref(),
        Some("Welcome"),
        "the start content is not part of it"
    );
}

#[gpui::test]
fn a_custom_heading_names_the_dialog(cx: &mut TestAppContext) {
    let (_, scope) = named(cx, false, || {
        DialogHeroHeader::new("Custom", media("hero"))
            .title_element(Heading::new(HeadingLevel::H3, "Custom"))
            .start_content(div().child("New"))
    });
    assert_eq!(name_of(&scope).as_deref(), Some("Custom"));
}

#[gpui::test]
fn a_hidden_custom_heading_names_the_dialog_without_focus(cx: &mut TestAppContext) {
    let (cx, scope) = named(cx, false, || {
        DialogHeroHeader::new("Custom", media("hero"))
            .title_element(Heading::new(HeadingLevel::H3, "Custom"))
            .title_hidden(true)
    });
    assert_eq!(name_of(&scope).as_deref(), Some("Custom"));
    // No title focus: with no control to take it, focus stays on the surface.
    assert!(cx.update(|window, cx| window.focused(cx)).is_some());
}

#[gpui::test]
fn an_inline_dialog_takes_no_name_or_focus_from_the_title(cx: &mut TestAppContext) {
    // Upstream's inline preview is a plain wrapper: no dialog role, so nothing names it.
    let (cx, scope) = named(cx, true, || {
        DialogHeroHeader::new("Inline title", media("hero"))
    });
    assert!(scope.borrow().is_none(), "an inline dialog keeps no scope");
    assert!(cx.update(|window, cx| window.focused(cx)).is_none());
}

#[gpui::test]
fn the_name_follows_a_replaced_title(cx: &mut TestAppContext) {
    // Upstream: "moves the handshake to a replacement custom heading".
    let title = Rc::new(RefCell::new("First"));
    let scope = Scope::default();
    let cx = host(cx, {
        let (title, scope) = (title.clone(), scope.clone());
        move || {
            let text = *title.borrow();
            Dialog::new("d", true)
                .on_open_change(|_, _, _| {})
                .child(
                    DialogHeroHeader::new(text, media("hero"))
                        .title_element(Heading::new(HeadingLevel::H3, text)),
                )
                .child(ScopeProbe(scope.clone()))
                .into_any_element()
        }
    });
    assert_eq!(name_of(&scope).as_deref(), Some("First"));
    *title.borrow_mut() = "Second";
    settle(cx);
    assert_eq!(name_of(&scope).as_deref(), Some("Second"));
}
