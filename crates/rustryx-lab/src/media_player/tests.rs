//! Upstream lab `MediaPlayer.test.tsx` against a fake media engine (upstream's
//! `fakePlayback`), the parts a GPUI window can drive, and the playback-time
//! formats checked against upstream's code under Node.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Context, ElementId, Entity, Keystroke, Modifiers, Pixels, Render,
    SharedString, TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext, Window, div,
    point, px,
};
use rustryx_primitives::Politeness;
use rustryx_primitives::media::{
    FakeMediaBackend, FakeMediaHandle, MediaError, MediaEventKind, MediaKind, TextTrackKind,
    TextTrackMode, set_media_backend_factory,
};

use super::player::PlayerState;
use super::*;

/// A drawn player: its state, its store, and whether its controls show.
type Root = (Entity<PlayerState>, Entity<MediaStore>, bool);

thread_local! {
    pub(crate) static ANNOUNCED: RefCell<Vec<(String, Politeness)>> = const { RefCell::new(Vec::new()) };
    pub(crate) static PARTS: RefCell<HashMap<String, (String, bool, bool)>> = RefCell::new(HashMap::new());
    pub(crate) static BARS: RefCell<HashMap<String, (bool, bool)>> = RefCell::new(HashMap::new());
    pub(crate) static ROOTS: RefCell<HashMap<String, Root>> = RefCell::new(HashMap::new());
}

pub(crate) fn record(id: &ElementId, label: &SharedString, pressed: bool, disabled: bool) {
    let key = super::context::element_key(id);
    PARTS.with(|p| {
        p.borrow_mut()
            .insert(key, (label.to_string(), pressed, disabled))
    });
}

pub(crate) fn record_bar(key: &str, visible: bool, shown: bool) {
    BARS.with(|b| b.borrow_mut().insert(key.to_string(), (visible, shown)));
}

pub(crate) fn record_root(
    id: &ElementId,
    player: &Entity<PlayerState>,
    store: &Entity<MediaStore>,
    visible: bool,
) {
    let key = super::context::element_key(id);
    ROOTS.with(|r| {
        r.borrow_mut()
            .insert(key, (player.clone(), store.clone(), visible))
    });
}

// --- The formats ------------------------------------------------------------

#[test]
fn format_clock_matches_upstream() {
    assert_eq!(format_clock(0., false), "0:00");
    assert_eq!(format_clock(65.9, false), "1:05");
    assert_eq!(format_clock(3725., false), "1:02:05");
    assert_eq!(format_clock(5., true), "0:00:05");
    assert_eq!(format_clock(f64::NAN, false), "0:00");
    assert_eq!(format_clock(f64::INFINITY, true), "0:00:00");
    assert_eq!(format_clock(-4., false), "0:00");
}

#[test]
fn format_spoken_time_reads_words() {
    assert_eq!(format_spoken_time(0., "en"), "0 seconds");
    assert_eq!(format_spoken_time(65., "en"), "1 minute, 5 seconds");
    assert_eq!(
        format_spoken_time(3725., "en"),
        "1 hour, 2 minutes, 5 seconds"
    );
    assert_eq!(format_spoken_time(3600., "en"), "1 hour");
    assert_eq!(
        format_spoken_time(125., "fr"),
        "2 minutes et 5\u{a0}secondes"
    );
}

/// The cases `media-cases.mjs` checks against upstream's `formatClock` and
/// `formatSpokenTime` under Node (`cargo test -p rustryx-lab -- --ignored
/// dump_media_formats` writes them).
#[test]
#[ignore]
fn dump_media_formats() {
    let mut rows = Vec::new();
    let seconds = [
        0.,
        0.4,
        1.,
        1.9,
        59.,
        60.,
        61.,
        119.5,
        599.,
        3599.,
        3600.,
        3601.,
        3661.,
        7322.7,
        86399.,
        90061.,
        -1.,
        f64::NAN,
        f64::INFINITY,
    ];
    for s in seconds {
        for hours in [false, true] {
            rows.push(serde_json::json!({"fn": "clock", "s": if s.is_finite() { serde_json::json!(s) } else { serde_json::json!(s.to_string()) }, "hours": hours, "out": format_clock(s, hours)}));
        }
        for locale in ["en", "fr", "de", "es", "ja", "ar", "pt-BR"] {
            rows.push(serde_json::json!({"fn": "spoken", "s": if s.is_finite() { serde_json::json!(s) } else { serde_json::json!(s.to_string()) }, "locale": locale, "out": format_spoken_time(s, locale)}));
        }
    }
    let path = std::env::var("MEDIA_DUMP").unwrap_or_else(|_| "media-formats.json".into());
    std::fs::write(path, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
}

#[test]
fn chapters_and_markers_are_found_as_upstream_finds_them() {
    let chapters = sorted_chapters(&[
        MediaTimelineChapter {
            start: 20.,
            title: "Pollen".into(),
        },
        MediaTimelineChapter {
            start: f64::NAN,
            title: "Lost".into(),
        },
        MediaTimelineChapter {
            start: 0.,
            title: "Intro".into(),
        },
    ]);
    assert_eq!(
        chapters
            .iter()
            .map(|c| c.title.as_ref())
            .collect::<Vec<_>>(),
        ["Intro", "Pollen"]
    );
    assert_eq!(
        timeline::find_chapter(&chapters, 25.).map(|c| c.title.as_ref()),
        Some("Pollen")
    );
    assert_eq!(
        timeline::find_chapter(&chapters, -1.).map(|c| c.title.as_ref()),
        None
    );
    let markers = [
        MediaTimelineMarker::new(10., "a"),
        MediaTimelineMarker::new(12., "b"),
        MediaTimelineMarker::new(8., "c"),
    ];
    // Of two at the same distance, the later in the list.
    assert_eq!(
        timeline::find_marker(&markers, 9., 1.).map(|m| m.label.as_ref()),
        Some("c")
    );
    assert_eq!(
        timeline::find_marker(&markers, 11.5, 1.).map(|m| m.label.as_ref()),
        Some("b")
    );
    assert!(timeline::find_marker(&markers, 30., 1.).is_none());
    let near: Vec<&str> = timeline::find_markers_near(&markers, 10., 2.)
        .iter()
        .map(|m| m.label.as_ref())
        .collect();
    assert_eq!(near, ["c", "a", "b"]);
}

use super::timeline::sorted_chapters;

#[test]
fn speaks_the_time_and_the_duration_as_its_value() {
    let i18n = rustryx_i18n::english();
    let value = timeline::spoken_value(5., 60., &[], &[], 0., &i18n, "en");
    assert_eq!(value.as_ref(), "5 seconds of 1 minute");
}

#[test]
fn speaks_the_chapter_and_the_marker_at_the_position_with_the_time() {
    let i18n = rustryx_i18n::english();
    let chapters = [
        MediaTimelineChapter {
            start: 0.,
            title: "Intro".into(),
        },
        MediaTimelineChapter {
            start: 20.,
            title: "Pollen".into(),
        },
    ];
    let markers = [MediaTimelineMarker::new(25., "Lin: hold on this")];
    let at =
        |time| timeline::spoken_value(time, 60., &chapters, &markers, 0., &i18n, "en").to_string();
    assert!(
        at(25.).ends_with(", Pollen, Lin: hold on this"),
        "{}",
        at(25.)
    );
    assert!(at(5.).ends_with(", Intro"), "{}", at(5.));
    // Within half an arrow step, a marker is spoken with the time.
    let markers = [MediaTimelineMarker::new(22., "Ana: love this shot")];
    assert!(
        timeline::spoken_value(20., 60., &[], &markers, 0., &i18n, "en")
            .contains("Ana: love this shot")
    );
}

// --- In a window --------------------------------------------------------------

type BuildFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Host {
    build: BuildFn,
    width: f32,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(rustryx_primitives::announcer())
            .child(rustryx_components::button::Button::new("before", "before"))
            .child(div().w(px(self.width)).child((self.build)(window, cx)))
            .child(
                div()
                    .debug_selector(|| "after".into())
                    .child(rustryx_components::button::Button::new("after", "after")),
            )
    }
}

/// The fake engines the players made, in order.
type Engines = Rc<RefCell<Vec<FakeMediaHandle>>>;

fn host_at(
    cx: &mut TestAppContext,
    width: f32,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> (&mut VisualTestContext, Engines) {
    ANNOUNCED.with(|a| a.borrow_mut().clear());
    PARTS.with(|p| p.borrow_mut().clear());
    BARS.with(|b| b.borrow_mut().clear());
    ROOTS.with(|r| r.borrow_mut().clear());
    let engines: Engines = Rc::default();
    let made = engines.clone();
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
        rustryx_primitives::editing::init(cx);
        set_media_backend_factory(
            move || {
                let (backend, handle) = FakeMediaBackend::new();
                made.borrow_mut().push(handle);
                Box::new(backend)
            },
            cx,
        );
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host {
        build: Rc::new(build),
        width,
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, engines)
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> (&mut VisualTestContext, Engines) {
    host_at(cx, 720., build)
}

/// Draws and delivers the media's events (the element reads its engine every
/// 100ms while paused).
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(40));
        cx.run_until_parked();
    }
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn advance(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    settle(cx);
}

fn engine(engines: &Engines) -> FakeMediaHandle {
    engines
        .borrow()
        .first()
        .cloned()
        .expect("a player made an engine")
}

/// Upstream `fakePlayback`: the media reports its metadata.
fn fake_playback(cx: &mut VisualTestContext, engines: &Engines, duration: f64) -> FakeMediaHandle {
    let media = engine(engines);
    media.load_metadata(duration);
    settle(cx);
    media
}

fn leak(selector: &str) -> &'static str {
    Box::leak(selector.to_string().into_boxed_str())
}

fn bounds(cx: &mut VisualTestContext, selector: &str) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(leak(selector))
}

fn click(cx: &mut VisualTestContext, selector: &str) {
    settle(cx);
    let b = bounds(cx, selector).unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_mouse_move(b.center(), None, Modifiers::none());
    cx.simulate_click(b.center(), Modifiers::none());
    settle(cx);
}

fn tap(cx: &mut VisualTestContext, selector: &str) {
    settle(cx);
    let at = bounds(cx, selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    for phase in [TouchPhase::Started, TouchPhase::Ended] {
        cx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: at,
            predicted_position: None,
            force: None,
        });
        cx.run_until_parked();
    }
    settle(cx);
}

/// A full key press: GPUI fires keyboard clicks on key-up.
fn send(cx: &mut VisualTestContext, keystroke: Keystroke) {
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

fn key(cx: &mut VisualTestContext, k: &'static str) {
    send(
        cx,
        Keystroke::parse(k).unwrap_or_else(|_| panic!("not a keystroke: {k}")),
    );
}

/// A typed character, such as `>`.
fn typed(cx: &mut VisualTestContext, character: &str, key: &str) {
    send(
        cx,
        Keystroke {
            modifiers: Modifiers::shift(),
            key: key.into(),
            key_char: Some(character.into()),
        },
    );
}

fn focus_id(cx: &mut VisualTestContext, id: &str) {
    let id = ElementId::Name(id.to_string().into());
    cx.update(|_, cx| rustryx_primitives::request_focus(&id, cx));
    settle(cx);
}

fn root(id: &str) -> Root {
    ROOTS
        .with(|r| r.borrow().get(id).cloned())
        .unwrap_or_else(|| panic!("player {id} drawn"))
}

fn focus_root(cx: &mut VisualTestContext, id: &str) {
    let (player, _, _) = root(id);
    cx.update(|window, cx| super::player::focus_player(&player, window, cx));
    settle(cx);
}

fn root_focused(cx: &mut VisualTestContext, id: &str) -> bool {
    let (player, _, _) = root(id);
    cx.update(|window, cx| player.read(cx).focus.is_focused(window))
}

fn label(id: &str) -> String {
    PARTS
        .with(|p| p.borrow().get(id).map(|r| r.0.clone()))
        .unwrap_or_else(|| panic!("{id} drawn"))
}

fn part(id: &str) -> Option<(String, bool, bool)> {
    PARTS.with(|p| p.borrow().get(id).cloned())
}

fn bar_visible(id: &str) -> bool {
    BARS.with(|b| b.borrow().get(id).map(|v| v.0))
        .unwrap_or_else(|| panic!("bar of {id} drawn"))
}

fn bar_shown(id: &str) -> bool {
    BARS.with(|b| b.borrow().get(id).map(|v| v.1))
        .unwrap_or_else(|| panic!("bar of {id} drawn"))
}

fn said() -> Vec<(String, Politeness)> {
    ANNOUNCED.with(|a| a.borrow().clone())
}

fn last_said() -> Option<String> {
    said().last().map(|(m, _)| m.clone())
}

fn store_state(cx: &mut VisualTestContext, id: &str) -> MediaPlayerState {
    let (_, store, _) = root(id);
    cx.update(|_, cx| store.read(cx).state().clone())
}

fn video(id: &'static str) -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |_, _| VideoPlayer::new(id, "media/clip.mp4", "Product tour").into_any_element()
}

fn play(cx: &mut VisualTestContext, id: &str) {
    let (_, store, _) = root(id);
    cx.update(|_, cx| store.update(cx, |s, cx| s.play(cx)));
    settle(cx);
}

// --- VideoPlayer ----------------------------------------------------------

#[gpui::test]
fn renders_a_focusable_group_around_the_video(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        VideoPlayer::new("tour", "media/clip.mp4", "Product tour")
            .poster("media/poster.jpg")
            .into_any_element()
    });
    assert!(bounds(cx, "tour").is_some(), "the player's group");
    assert!(bounds(cx, "tour-video").is_some(), "the video");
    let loaded = engine(&engines).state().loads;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].src.as_ref(), "media/clip.mp4");
    assert_eq!(loaded[0].kind, MediaKind::Video);
    // The group is a tab stop.
    key(cx, "tab");
    key(cx, "tab");
    assert!(root_focused(cx, "tour"));
}

#[gpui::test]
fn plays_from_the_play_button_and_then_offers_pause(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    let media = fake_playback(cx, &engines, 120.);
    click(cx, "tour-big-play");
    assert!(!media.state().paused, "playback started");
    assert_eq!(label("tour-play-button"), "Pause");
}

#[gpui::test]
fn offers_replay_after_the_media_ends(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    let media = fake_playback(cx, &engines, 120.);
    media.end();
    settle(cx);
    assert_eq!(label("tour-play-button"), "Replay");
    assert_eq!(label("tour-big-play"), "Replay");
}

#[gpui::test]
fn runs_keyboard_shortcuts_scoped_to_the_player_and_announces_the_change(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    let media = fake_playback(cx, &engines, 120.);
    focus_root(cx, "tour");
    key(cx, "k");
    assert!(!media.state().paused);
    key(cx, "m");
    assert!(media.state().muted);
    assert_eq!(last_said().as_deref(), Some("Muted"));
    key(cx, "right");
    assert_eq!(media.state().current_time, 5.);
    key(cx, "l");
    assert_eq!(media.state().current_time, 15.);
    key(cx, "5");
    assert_eq!(media.state().current_time, 60.);
    typed(cx, ">", ".");
    assert_eq!(media.state().playback_rate, 1.25);
    assert_eq!(last_said().as_deref(), Some("Speed 1.25×"));
    key(cx, "m");
    assert_eq!(last_said().as_deref(), Some("Unmuted"));
}

#[gpui::test]
fn steps_the_volume_and_the_speed_and_seeks_by_digit(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    let media = fake_playback(cx, &engines, 120.);
    focus_root(cx, "tour");
    key(cx, "down");
    assert_eq!(media.state().volume, 0.95);
    assert_eq!(last_said().as_deref(), Some("Volume 95%"));
    key(cx, "up");
    key(cx, "up");
    assert_eq!(media.state().volume, 1.);
    assert_eq!(last_said().as_deref(), Some("Volume 100%"));
    // Muted, the volume steps from zero, and a step above it unmutes.
    key(cx, "m");
    key(cx, "up");
    assert_eq!(media.state().volume, 0.05);
    assert!(!media.state().muted);
    typed(cx, "<", ",");
    assert_eq!(media.state().playback_rate, 0.75);
    typed(cx, "<", ",");
    typed(cx, "<", ",");
    assert_eq!(media.state().playback_rate, 0.5, "the slowest speed holds");
    key(cx, "0");
    assert_eq!(media.state().current_time, 0.);
    key(cx, "9");
    assert_eq!(media.state().current_time, 108.);
    assert_eq!(last_said().as_deref(), Some("1 minute, 48 seconds"));
    key(cx, "j");
    assert_eq!(media.state().current_time, 98.);
    key(cx, "left");
    assert_eq!(media.state().current_time, 93.);
    // A modifier leaves the key to the app.
    key(cx, "ctrl-k");
    assert!(media.state().paused);
}

#[gpui::test]
fn seeks_nothing_before_the_media_reports_its_duration(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    let media = engine(&engines);
    focus_root(cx, "tour");
    key(cx, "right");
    key(cx, "5");
    assert_eq!(media.state().current_time, 0.);
}

/// A field that takes typed text, inside the player.
#[derive(IntoElement)]
struct Field;

impl RenderOnce for Field {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus = rustryx_primitives::control_focus(&ElementId::Name("note".into()), window, cx);
        div()
            .id("note")
            .debug_selector(|| "note".into())
            .track_focus(&focus)
            .key_context(rustryx_primitives::modality::TYPING_TARGET)
            .w(px(100.))
            .h(px(20.))
    }
}

#[gpui::test]
fn leaves_shortcut_keys_alone_while_a_person_types_in_a_field_inside_the_player(
    cx: &mut TestAppContext,
) {
    let (cx, engines) = host(cx, |_, _| {
        MediaPlayer::new("clip", "Annotated clip")
            .child(MediaPlayerElement::new(MediaKind::Video).src("media/clip.mp4"))
            .child(Field)
            .into_any_element()
    });
    let media = fake_playback(cx, &engines, 60.);
    focus_id(cx, "note");
    key(cx, "k");
    assert!(media.state().paused, "the key went to the field");
}

#[gpui::test]
fn keeps_the_large_play_button_out_of_the_tab_order_and_moves_focus_to_the_player(
    cx: &mut TestAppContext,
) {
    let (cx, engines) = host(cx, video("tour"));
    let media = fake_playback(cx, &engines, 120.);
    key(cx, "tab");
    key(cx, "tab");
    assert!(root_focused(cx, "tour"));
    key(cx, "tab");
    assert!(!root_focused(cx, "tour"));
    // The bar's Play: Enter plays and focus stays on it (the large one sends focus
    // to the player).
    key(cx, "enter");
    assert!(!media.state().paused);
    assert!(
        !root_focused(cx, "tour"),
        "Tab reached the bar's Play, not the large one"
    );
    key(cx, "enter");
    assert!(media.state().paused);
}

#[gpui::test]
fn the_large_play_button_moves_focus_to_the_player_when_it_starts_playback(
    cx: &mut TestAppContext,
) {
    let (cx, engines) = host(cx, video("tour"));
    let media = fake_playback(cx, &engines, 120.);
    click(cx, "tour-big-play");
    assert!(!media.state().paused);
    assert!(root_focused(cx, "tour"));
    key(cx, "k");
    assert!(media.state().paused);
}

#[gpui::test]
fn reports_playback_through_its_callbacks_once_per_change(cx: &mut TestAppContext) {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let l = log.clone();
    let (cx, engines) = host(cx, move |_, _| {
        let (a, b, c, d) = (l.clone(), l.clone(), l.clone(), l.clone());
        VideoPlayer::new("tour", "media/clip.mp4", "Product tour")
            .on_play(move |_, _| a.borrow_mut().push("play".into()))
            .on_pause(move |_, _| b.borrow_mut().push("pause".into()))
            .on_ended(move |_, _| c.borrow_mut().push("ended".into()))
            .on_error(move |e, _, _| d.borrow_mut().push(format!("error {}", e.code)))
            .into_any_element()
    });
    let media = fake_playback(cx, &engines, 120.);
    // Before playback, the bar sits behind the large play control, so its Play is
    // reached with the keyboard.
    focus_id(cx, "tour-play-button");
    key(cx, "enter");
    assert_eq!(*log.borrow(), ["play"]);
    key(cx, "enter");
    assert_eq!(*log.borrow(), ["play", "pause"]);
    key(cx, "enter");
    media.end();
    settle(cx);
    assert_eq!(*log.borrow(), ["play", "pause", "play", "ended"]);
    media.fail(MediaError::unsupported("no"));
    settle(cx);
    assert_eq!(log.borrow().last().map(String::as_str), Some("error 4"));
}

#[gpui::test]
fn keeps_the_bar_behind_the_large_play_control_until_playback_starts(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    let media = fake_playback(cx, &engines, 120.);
    assert!(bar_visible("tour"));
    assert!(!bar_shown("tour"), "before playback the bar waits below");
    // A press where the bar's Play waits lands on the video, which plays.
    click(cx, "tour-play-button");
    assert!(!media.state().paused, "one toggle, from the video surface");
    assert!(bar_shown("tour"));
    // Once playing, the bar's own Play takes the press.
    click(cx, "tour-play-button");
    assert!(media.state().paused);
    assert_eq!(label("tour-play-button"), "Play");
}

#[gpui::test]
fn keyboard_focus_inside_the_bar_shows_it_before_playback(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    fake_playback(cx, &engines, 120.);
    focus_id(cx, "tour-play-button");
    assert!(bar_shown("tour"));
}

#[gpui::test]
fn shows_the_bar_beside_the_replay_control_after_the_end(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    let media = fake_playback(cx, &engines, 120.);
    focus_id(cx, "tour-play-button");
    key(cx, "enter");
    media.end();
    settle(cx);
    assert!(bar_shown("tour"));
    click(cx, "tour-play-button");
    assert_eq!(label("tour-play-button"), "Pause");
}

#[gpui::test]
fn shows_a_spinner_while_started_media_waits(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    let media = fake_playback(cx, &engines, 120.);
    assert!(bounds(cx, "tour-spinner").is_none());
    play(cx, "tour");
    media.fire([MediaEventKind::Waiting]);
    settle(cx);
    assert!(bounds(cx, "tour-spinner").is_some());
    assert!(store_state(cx, "tour").is_waiting);
    media.fire([MediaEventKind::Playing]);
    settle(cx);
    assert!(bounds(cx, "tour-spinner").is_none());
}

#[gpui::test]
fn hides_the_controls_after_two_idle_seconds_of_playback_and_shows_them_on_pointer_movement(
    cx: &mut TestAppContext,
) {
    let (cx, engines) = host(cx, video("tour"));
    fake_playback(cx, &engines, 120.);
    assert!(bar_visible("tour"));
    play(cx, "tour");
    advance(cx, 2000);
    assert!(!bar_visible("tour"));
    assert!(!root("tour").2, "the root reports hidden controls");
    let at = bounds(cx, "tour").unwrap().center();
    cx.simulate_mouse_move(at, None, Modifiers::none());
    settle(cx);
    assert!(bar_visible("tour"));
}

#[gpui::test]
fn lets_a_mouse_that_leaves_the_player_hide_the_controls_after_a_click_gave_the_timeline_focus(
    cx: &mut TestAppContext,
) {
    let (cx, engines) = host(cx, video("tour"));
    fake_playback(cx, &engines, 60.);
    play(cx, "tour");
    click(cx, "tour-time-slider");
    let outside = bounds(cx, "after").unwrap().center();
    cx.simulate_mouse_move(outside, None, Modifiers::none());
    settle(cx);
    assert!(!bar_visible("tour"));
}

#[gpui::test]
fn holds_the_controls_while_keyboard_focus_is_inside_the_player(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    fake_playback(cx, &engines, 60.);
    play(cx, "tour");
    let inside = bounds(cx, "tour").unwrap().center();
    cx.simulate_mouse_move(inside, None, Modifiers::none());
    focus_root(cx, "tour");
    key(cx, "tab");
    let outside = bounds(cx, "after").unwrap().center();
    cx.simulate_mouse_move(outside, None, Modifiers::none());
    settle(cx);
    assert!(bar_visible("tour"));
    advance(cx, 2500);
    assert!(bar_visible("tour"), "the hold outlasts the idle time");
}

#[gpui::test]
fn keeps_the_controls_visible_while_paused(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    fake_playback(cx, &engines, 120.);
    advance(cx, 5000);
    assert!(bar_visible("tour"));
}

#[gpui::test]
fn shows_a_retry_action_and_announces_a_playback_failure_once(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, video("tour"));
    let media = fake_playback(cx, &engines, 120.);
    media.fail(MediaError::unsupported("unsupported"));
    settle(cx);
    let errors: Vec<_> = said()
        .into_iter()
        .filter(|(m, _)| m == "This media can't be played.")
        .collect();
    assert_eq!(
        errors,
        [(
            "This media can't be played.".to_string(),
            Politeness::Assertive
        )]
    );
    assert!(store_state(cx, "tour").error.is_some());
    click(cx, "tour-retry");
    assert_eq!(media.state().loads.len(), 2, "the source loaded again");
    assert!(media.state().paused, "a retry does not play");
    assert!(root_focused(cx, "tour"));
    assert!(
        store_state(cx, "tour").error.is_none(),
        "the reload cleared the error"
    );
}

#[gpui::test]
fn on_touch_shows_the_controls_on_a_tap_and_hides_them_on_the_next_tap_only(
    cx: &mut TestAppContext,
) {
    let (cx, engines) = host(cx, video("tour"));
    fake_playback(cx, &engines, 120.);
    play(cx, "tour");
    advance(cx, 2000);
    assert!(!bar_visible("tour"));
    tap(cx, "tour-surface");
    assert!(bar_visible("tour"));
    advance(cx, 10_000);
    assert!(bar_visible("tour"), "no idle timer after touch");
    tap(cx, "tour-surface");
    assert!(!bar_visible("tour"));
}

const VTT: &str = "data:text/vtt,WEBVTT%0A%0A00:00.000 --> 00:02.500%0AA bee lands on a flower.%0A";

#[gpui::test]
fn shows_the_captions_toggle_only_when_the_media_has_a_caption_track(cx: &mut TestAppContext) {
    let with: Rc<std::cell::Cell<bool>> = Rc::default();
    let w = with.clone();
    let (cx, engines) = host(cx, move |_, _| {
        let mut player = VideoPlayer::new("tour", "media/clip.mp4", "Product tour");
        if w.get() {
            player = player.captions([VideoPlayerCaptionTrack::new(VTT, "English", "en")]);
        }
        player.into_any_element()
    });
    fake_playback(cx, &engines, 120.);
    assert!(part("tour-captions-button").is_none());
    with.set(true);
    settle(cx);
    let (_, pressed, _) = part("tour-captions-button").expect("the toggle shows");
    assert!(!pressed);
    focus_id(cx, "tour-captions-button");
    key(cx, "enter");
    let (_, store, _) = root("tour");
    let mode = cx.update(|_, cx| store.read(cx).media().unwrap().read(cx).text_tracks()[0].mode);
    assert_eq!(mode, TextTrackMode::Showing);
    assert!(part("tour-captions-button").unwrap().1);
    assert_eq!(
        store_state(cx, "tour").text_tracks[0].id.as_ref(),
        "en:English:0"
    );
}

#[gpui::test]
fn toggles_captions_with_c_back_to_the_last_track(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        VideoPlayer::new("tour", "media/clip.mp4", "Product tour")
            .captions([
                VideoPlayerCaptionTrack::new(VTT, "English", "en"),
                VideoPlayerCaptionTrack::new(VTT, "Deutsch", "de").kind(TextTrackKind::Subtitles),
            ])
            .into_any_element()
    });
    fake_playback(cx, &engines, 120.);
    focus_root(cx, "tour");
    key(cx, "c");
    assert_eq!(last_said().as_deref(), Some("Captions on"));
    assert_eq!(
        store_state(cx, "tour").active_text_track_id.as_deref(),
        Some("en:English:0")
    );
    let (_, store, _) = root("tour");
    cx.update(|_, cx| {
        store.update(cx, |s, cx| {
            s.set_text_track(Some("de:Deutsch:1".into()), cx)
        })
    });
    settle(cx);
    key(cx, "c");
    assert_eq!(last_said().as_deref(), Some("Captions off"));
    assert_eq!(store_state(cx, "tour").active_text_track_id, None);
    key(cx, "c");
    assert_eq!(
        store_state(cx, "tour").active_text_track_id.as_deref(),
        Some("de:Deutsch:1"),
        "the last shown track"
    );
}

#[gpui::test]
fn shows_the_showing_cue_over_the_video(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        VideoPlayer::new("tour", "media/clip.mp4", "Product tour")
            .captions([VideoPlayerCaptionTrack::new(VTT, "English", "en").is_default(true)])
            .into_any_element()
    });
    let media = fake_playback(cx, &engines, 120.);
    media.advance_to(1.);
    settle(cx);
    let (_, store, _) = root("tour");
    let cues = cx.update(|_, cx| store.read(cx).media().unwrap().read(cx).active_cues());
    assert_eq!(cues, [SharedString::from("A bee lands on a flower.")]);
}

// --- The control bar ------------------------------------------------------------

#[gpui::test]
fn a_wide_player_keeps_the_8px_inset_and_the_40px_bar(cx: &mut TestAppContext) {
    let (cx, engines) = host_at(cx, 720., video("tour"));
    fake_playback(cx, &engines, 120.);
    play(cx, "tour");
    settle(cx);
    let (player, bar) = (bounds(cx, "tour").unwrap(), bounds(cx, "tour-bar").unwrap());
    assert_eq!(bar.left() - player.left(), px(8.));
    assert_eq!(player.right() - bar.right(), px(8.));
    assert_eq!(player.bottom() - bar.bottom(), px(8.));
    assert_eq!(bar.size.height, px(40.));
    assert!(bounds(cx, "tour-time-current-duration").is_some());
    assert!(bounds(cx, "tour-volume").is_some());
}

#[gpui::test]
fn a_player_360px_wide_or_narrower_gets_a_compact_bar(cx: &mut TestAppContext) {
    let (cx, engines) = host_at(cx, 240., video("tour"));
    fake_playback(cx, &engines, 120.);
    play(cx, "tour");
    settle(cx);
    let (player, bar) = (bounds(cx, "tour").unwrap(), bounds(cx, "tour-bar").unwrap());
    assert_eq!(bar.left() - player.left(), px(4.));
    assert_eq!(player.bottom() - bar.bottom(), px(4.));
    assert_eq!(bar.size.height, px(32.));
    assert!(
        bounds(cx, "tour-time-current-duration").is_none(),
        "the time leaves at 360px"
    );
    assert!(
        bounds(cx, "tour-volume").is_none(),
        "volume leaves at 480px"
    );
    let play_button = bounds(cx, "tour-play-button").unwrap();
    assert_eq!(play_button.size.height, px(28.), "small buttons");
}

#[gpui::test]
fn drops_by_its_own_inset_while_hidden(cx: &mut TestAppContext) {
    let (cx, engines) = host_at(cx, 720., video("tour"));
    fake_playback(cx, &engines, 120.);
    play(cx, "tour");
    advance(cx, 2000);
    advance(cx, 400);
    assert!(!bar_visible("tour"));
    let (player, bar) = (bounds(cx, "tour").unwrap(), bounds(cx, "tour-bar").unwrap());
    assert_eq!(
        player.bottom() - bar.bottom(),
        px(0.),
        "one inset below its place"
    );
}

fn marker_with_reply(time: f64, label: &'static str) -> MediaTimelineMarker {
    MediaTimelineMarker::new(time, label).content(|_, _| Field.into_any_element())
}

#[gpui::test]
fn keeps_the_controls_through_idle_playback_while_a_marker_popover_is_open(
    cx: &mut TestAppContext,
) {
    let (cx, engines) = host(cx, |_, _| {
        VideoPlayer::new("tour", "media/clip.mp4", "Product tour")
            .markers([marker_with_reply(20., "Lin: hold on this")])
            .into_any_element()
    });
    fake_playback(cx, &engines, 60.);
    play(cx, "tour");
    focus_id(cx, "tour-time-slider-marker-20-Lin: hold on this");
    key(cx, "enter");
    assert!(
        part("tour-time-slider-marker-20-Lin: hold on this")
            .unwrap()
            .1,
        "the popover opened"
    );
    focus_id(cx, "note");
    advance(cx, 2000);
    assert!(bar_visible("tour"));
}

#[gpui::test]
fn lets_the_controls_idle_hide_again_after_a_marker_is_removed_while_its_popover_is_open(
    cx: &mut TestAppContext,
) {
    let with: Rc<std::cell::Cell<bool>> = Rc::new(std::cell::Cell::new(true));
    let w = with.clone();
    let (cx, engines) = host(cx, move |_, _| {
        let markers = if w.get() {
            vec![marker_with_reply(20., "Lin: hold on this")]
        } else {
            vec![]
        };
        VideoPlayer::new("tour", "media/clip.mp4", "Product tour")
            .markers(markers)
            .into_any_element()
    });
    fake_playback(cx, &engines, 60.);
    play(cx, "tour");
    click(cx, "tour-time-slider-marker-20-Lin: hold on this");
    assert_eq!(engine(&engines).state().current_time, 20.);
    with.set(false);
    settle(cx);
    cx.update(|window, cx| window.blur(cx));
    let outside = bounds(cx, "after").unwrap().center();
    cx.simulate_mouse_move(outside, None, Modifiers::none());
    advance(cx, 2000);
    assert!(!bar_visible("tour"));
}

// --- MediaTimeSlider ----------------------------------------------------------

fn timeline(
    markers: Vec<MediaTimelineMarker>,
) -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |_, _| {
        MediaPlayer::new("clip", "Clip")
            .child(MediaPlayerElement::new(MediaKind::Video).src("media/clip.mp4"))
            .child(MediaTimeSlider::new().markers(markers.clone()))
            .into_any_element()
    }
}

fn thumb(cx: &mut VisualTestContext, slider: &str) {
    let id = ElementId::Name(slider.to_string().into());
    let thumb = ElementId::Name(format!("{id:?}-thumb-0").into());
    cx.update(|_, cx| rustryx_primitives::request_focus(&thumb, cx));
    settle(cx);
}

#[gpui::test]
fn skips_five_seconds_per_arrow_key_and_ten_per_page_key(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, timeline(vec![]));
    let media = fake_playback(cx, &engines, 120.);
    thumb(cx, "clip-time-slider-slider");
    key(cx, "right");
    assert_eq!(media.state().current_time, 5.);
    key(cx, "pageup");
    assert_eq!(media.state().current_time, 15.);
    key(cx, "left");
    assert_eq!(media.state().current_time, 10.);
    key(cx, "down");
    assert_eq!(media.state().current_time, 5.);
    key(cx, "pagedown");
    assert_eq!(media.state().current_time, 0.);
}

#[gpui::test]
fn leaves_arrow_keys_typed_in_a_marker_popover_to_the_popover(cx: &mut TestAppContext) {
    let (cx, engines) = host(
        cx,
        timeline(vec![marker_with_reply(20., "Lin: hold on this")]),
    );
    let media = fake_playback(cx, &engines, 60.);
    click(cx, "clip-time-slider-marker-20-Lin: hold on this");
    assert_eq!(media.state().current_time, 20.);
    focus_id(cx, "note");
    key(cx, "right");
    key(cx, "pageup");
    assert_eq!(media.state().current_time, 20.);
}

/// A button inside a marker's popover.
fn marker_with_button(time: f64, label: &'static str) -> MediaTimelineMarker {
    MediaTimelineMarker::new(time, label).content(|_, _| {
        rustryx_components::button::Button::new("reply", "Reply").into_any_element()
    })
}

#[gpui::test]
fn leaves_shortcut_keys_pressed_on_a_button_in_a_marker_popover_to_that_button(
    cx: &mut TestAppContext,
) {
    let (cx, engines) = host(
        cx,
        timeline(vec![marker_with_button(20., "Lin: hold on this")]),
    );
    let media = fake_playback(cx, &engines, 60.);
    click(cx, "clip-time-slider-marker-20-Lin: hold on this");
    assert_eq!(media.state().current_time, 20.);
    focus_id(cx, "reply");
    for k in ["right", "left", "j", "l", "5", "up", "down", "m", "k"] {
        key(cx, k);
    }
    let state = media.state();
    assert_eq!(state.current_time, 20.);
    assert_eq!(state.volume, 1.);
    assert!(!state.muted);
    assert!(state.paused);
}

#[gpui::test]
fn draws_a_dot_for_each_marker_once_the_duration_is_known(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        VideoPlayer::new("clip", "media/clip.mp4", "Clip")
            .markers([
                MediaTimelineMarker::new(10., "Ana: love this"),
                MediaTimelineMarker::new(30., "Lin: hold on this"),
            ])
            .into_any_element()
    });
    assert!(bounds(cx, "clip-time-slider-marker-10-Ana: love this").is_none());
    fake_playback(cx, &engines, 60.);
    assert!(bounds(cx, "clip-time-slider-marker-10-Ana: love this").is_some());
    assert!(bounds(cx, "clip-time-slider-marker-30-Lin: hold on this").is_some());
}

#[gpui::test]
fn previews_the_time_and_the_chapter_under_the_pointer(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        MediaPlayer::new("clip", "Clip")
            .child(MediaPlayerElement::new(MediaKind::Video).src("media/clip.mp4"))
            .child(div().pt(px(80.)).child(MediaTimeSlider::new().chapters([
                MediaTimelineChapter {
                    start: 0.,
                    title: "Intro".into(),
                },
                MediaTimelineChapter {
                    start: 30.,
                    title: "Pollen".into(),
                },
            ])))
            .into_any_element()
    });
    fake_playback(cx, &engines, 60.);
    assert!(bounds(cx, "clip-time-slider-preview").is_none());
    let b = bounds(cx, "clip-time-slider").unwrap();
    cx.simulate_mouse_move(
        point(b.right() - px(12.), b.center().y),
        None,
        Modifiers::none(),
    );
    settle(cx);
    let preview = bounds(cx, "clip-time-slider-preview").expect("the preview shows");
    assert!(preview.bottom() <= b.top(), "above the timeline");
    cx.simulate_mouse_move(
        point(b.left(), b.bottom() + px(100.)),
        None,
        Modifiers::none(),
    );
    settle(cx);
    assert!(bounds(cx, "clip-time-slider-preview").is_none());
}

// --- AudioPlayer --------------------------------------------------------------

fn audio(cx: &mut TestAppContext) -> (&mut VisualTestContext, Engines) {
    host(cx, |_, _| {
        AudioPlayer::new("episode", "media/episode.mp3", "Designing for every screen")
            .subtitle("The Interface Show")
            .into_any_element()
    })
}

#[gpui::test]
fn shows_its_artwork_and_titles(cx: &mut TestAppContext) {
    let (cx, engines) = audio(cx);
    assert!(bounds(cx, "episode-artwork").is_some());
    assert!(bounds(cx, "episode-titles").is_some());
    assert!(bounds(cx, "episode-audio").is_some());
    assert_eq!(engine(&engines).state().loads[0].kind, MediaKind::Audio);
}

#[gpui::test]
fn offers_previous_and_next_track_controls_only_when_their_handlers_are_set(
    cx: &mut TestAppContext,
) {
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let with: Rc<std::cell::Cell<bool>> = Rc::default();
    let (l, w) = (log.clone(), with.clone());
    let (cx, _) = host(cx, move |_, _| {
        let mut player = AudioPlayer::new("episode", "media/episode.mp3", "Episode");
        if w.get() {
            let (a, b) = (l.clone(), l.clone());
            player = player
                .on_previous_track(move |_, _| a.borrow_mut().push("previous"))
                .on_next_track(move |_, _| b.borrow_mut().push("next"));
        }
        player.into_any_element()
    });
    assert!(part("episode-previous-track").is_none());
    with.set(true);
    settle(cx);
    assert_eq!(label("episode-previous-track"), "Previous track");
    click(cx, "episode-previous-track");
    click(cx, "episode-next-track");
    assert_eq!(*log.borrow(), ["previous", "next"]);
}

#[gpui::test]
fn skips_back_15_seconds_and_forward_30_seconds_by_default(cx: &mut TestAppContext) {
    let (cx, engines) = audio(cx);
    let media = fake_playback(cx, &engines, 600.);
    assert_eq!(label("episode-seek-forward"), "Forward 30 seconds");
    assert_eq!(label("episode-seek-back"), "Back 15 seconds");
    click(cx, "episode-seek-forward");
    assert_eq!(media.state().current_time, 30.);
    click(cx, "episode-seek-back");
    assert_eq!(media.state().current_time, 15.);
}

#[gpui::test]
fn its_seek_buttons_wait_for_the_duration(cx: &mut TestAppContext) {
    let (cx, engines) = audio(cx);
    assert!(
        part("episode-seek-forward").unwrap().2,
        "disabled before metadata"
    );
    fake_playback(cx, &engines, 600.);
    assert!(!part("episode-seek-forward").unwrap().2);
}

#[gpui::test]
fn offers_a_volume_slider(cx: &mut TestAppContext) {
    let (_cx, _) = audio(cx);
    assert!(part("episode-volume-slider").is_some());
}

#[gpui::test]
fn switches_between_normal_speed_and_the_chosen_speed_with_its_speed_button_and_slider(
    cx: &mut TestAppContext,
) {
    let (cx, engines) = audio(cx);
    let media = fake_playback(cx, &engines, 600.);
    thumb(cx, "episode-playback-rate-slider");
    key(cx, "right");
    assert_eq!(media.state().playback_rate, 1.25);
    assert_eq!(
        label("episode-playback-rate-button"),
        "Playback speed, 1.25×"
    );
    click(cx, "episode-playback-rate-button");
    assert_eq!(media.state().playback_rate, 1.);
    assert_eq!(label("episode-playback-rate-button"), "Playback speed, 1×");
    click(cx, "episode-playback-rate-button");
    assert_eq!(media.state().playback_rate, 1.25);
}

#[gpui::test]
fn from_normal_speed_with_no_other_speed_its_button_steps_up_one_speed(cx: &mut TestAppContext) {
    let (cx, engines) = audio(cx);
    let media = fake_playback(cx, &engines, 600.);
    click(cx, "episode-playback-rate-button");
    assert_eq!(media.state().playback_rate, 1.25);
}

// --- MediaPlaybackRate, MediaVolume, MediaPlayerElement ------------------------

#[gpui::test]
fn sets_the_playback_speed_from_a_menu_that_checks_the_current_speed(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        MediaPlayer::new("episode", "Episode")
            .child(MediaPlayerElement::new(MediaKind::Audio).src("media/episode.mp3"))
            .child(MediaPlaybackRate::new().variant(MediaPlaybackRateVariant::Menu))
            .into_any_element()
    });
    let media = fake_playback(cx, &engines, 600.);
    assert_eq!(label("episode-playback-rate"), "menu");
    assert!(
        bounds(cx, "episode-playback-rate-slider").is_none(),
        "no slider in the menu variant"
    );
    assert_eq!(label("episode-playback-rate-menu"), "Playback speed, 1×");
    click(cx, "episode-playback-rate-menu");
    assert!(
        bounds(cx, "episode-playback-rate-menu-list-1").is_some(),
        "the menu opened"
    );
    click(cx, "episode-playback-rate-menu-list-1.5");
    assert_eq!(media.state().playback_rate, 1.5);
    assert_eq!(label("episode-playback-rate-menu"), "Playback speed, 1.5×");
}

#[gpui::test]
fn holds_the_controls_while_the_speed_menu_is_open(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        MediaPlayer::new("episode", "Episode")
            .child(MediaPlayerElement::new(MediaKind::Video).src("media/clip.mp4"))
            .child(MediaPlaybackRate::new().variant(MediaPlaybackRateVariant::Menu))
            .into_any_element()
    });
    fake_playback(cx, &engines, 600.);
    play(cx, "episode");
    click(cx, "episode-playback-rate-menu");
    let (player, _, _) = root("episode");
    assert!(cx.update(|_, cx| player.read(cx).is_layer_open()));
    advance(cx, 2500);
    assert!(root("episode").2, "the open menu keeps the controls shown");
}

#[gpui::test]
fn pairs_mute_with_a_volume_slider_before_it_by_default(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        MediaPlayer::new("episode", "Episode")
            .child(MediaPlayerElement::new(MediaKind::Audio).src("media/episode.mp3"))
            .child(MediaVolume::new())
            .into_any_element()
    });
    fake_playback(cx, &engines, 120.);
    // The slider opens while the group is hovered.
    let mute = bounds(cx, "episode-mute-button").unwrap();
    cx.simulate_mouse_move(mute.center(), None, Modifiers::none());
    advance(cx, 300);
    let (slider, mute) = (
        bounds(cx, "episode-volume-slider").unwrap(),
        bounds(cx, "episode-mute-button").unwrap(),
    );
    assert!(slider.left() < mute.left(), "the slider comes first");
    assert_eq!(label("episode-mute-button"), "Mute");
    click(cx, "episode-mute-button");
    assert_eq!(label("episode-mute-button"), "Unmute");
}

#[gpui::test]
fn renders_mute_alone_without_its_slider(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, |_, _| {
        MediaPlayer::new("episode", "Episode")
            .child(MediaPlayerElement::new(MediaKind::Audio).src("media/episode.mp3"))
            .child(MediaVolume::new().has_slider(false))
            .into_any_element()
    });
    assert_eq!(label("episode-mute-button"), "Mute");
    assert!(bounds(cx, "episode-volume-slider").is_none());
}

#[gpui::test]
fn drops_the_volume_slider_where_the_volume_cannot_be_set(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        MediaPlayer::new("episode", "Episode")
            .child(MediaPlayerElement::new(MediaKind::Audio).src("media/episode.mp3"))
            .child(MediaVolume::new())
            .into_any_element()
    });
    let media = engine(&engines);
    media.update(|s| s.can_set_volume = false);
    let (_, store, _) = root("episode");
    // The probe ran once already; a new element asks again.
    cx.update(|_, cx| {
        let el = store.read(cx).media().cloned();
        store.update(cx, |s, cx| {
            s.attach_media(None, cx);
            s.attach_media(el, cx);
        })
    });
    settle(cx);
    assert!(
        store_state(cx, "episode").can_set_volume,
        "one probe serves the store"
    );
}

#[gpui::test]
fn renders_the_media_element_for_its_kind(cx: &mut TestAppContext) {
    let video: Rc<std::cell::Cell<bool>> = Rc::default();
    let v = video.clone();
    let (cx, engines) = host(cx, move |_, _| {
        let element = if v.get() {
            MediaPlayerElement::new(MediaKind::Video)
                .src("media/clip.mp4")
                .poster("media/clip.jpg")
        } else {
            MediaPlayerElement::new(MediaKind::Audio).src("media/episode.mp3")
        };
        MediaPlayer::new("p", "Episode")
            .child(element)
            .into_any_element()
    });
    assert!(bounds(cx, "p-audio").is_some());
    video.set(true);
    settle(cx);
    assert!(bounds(cx, "p-video").is_some());
    let loads = engine(&engines).state().loads;
    assert_eq!(
        loads.last().map(|l| l.src.to_string()).as_deref(),
        Some("media/clip.mp4")
    );
}

// --- useMediaPlayer and useMediaPlaybackTime -------------------------------------

thread_local! {
    static READOUT: RefCell<Vec<(f64, f64, bool, f64)>> = const { RefCell::new(Vec::new()) };
}

/// Upstream's `Readout`: shows the state and drives the player through its actions.
#[derive(IntoElement)]
struct Readout;

impl RenderOnce for Readout {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let player = use_media_player(cx);
        let time = use_media_playback_time(window, cx);
        READOUT.with(|r| {
            r.borrow_mut().push((
                player.state.current_time,
                player.state.volume,
                player.state.is_paused,
                time,
            ))
        });
        let (a, b, c) = (
            player.actions.clone(),
            player.actions.clone(),
            player.actions,
        );
        div()
            .flex()
            .child(
                div().debug_selector(|| "jump".into()).child(
                    rustryx_components::button::Button::new("jump", "Jump")
                        .on_click(move |_, _, cx| a.seek(42., cx)),
                ),
            )
            .child(
                div().debug_selector(|| "half".into()).child(
                    rustryx_components::button::Button::new("half", "Half")
                        .on_click(move |_, _, cx| b.set_volume(0.5, cx)),
                ),
            )
            .child(
                div().debug_selector(|| "toggle".into()).child(
                    rustryx_components::button::Button::new("toggle", "Toggle")
                        .on_click(move |_, _, cx| c.toggle_paused(cx)),
                ),
            )
    }
}

fn readout() -> (f64, f64, bool, f64) {
    READOUT.with(|r| *r.borrow().last().expect("drawn"))
}

fn memo(cx: &mut TestAppContext) -> (&mut VisualTestContext, Engines) {
    READOUT.with(|r| r.borrow_mut().clear());
    host(cx, |_, _| {
        MediaPlayer::new("memo", "Memo")
            .child(MediaPlayerElement::new(MediaKind::Audio).src("media/memo.m4a"))
            .child(Readout)
            .into_any_element()
    })
}

#[gpui::test]
fn reads_the_player_state_and_drives_the_player_through_its_actions(cx: &mut TestAppContext) {
    let (cx, engines) = memo(cx);
    let media = fake_playback(cx, &engines, 120.);
    click(cx, "jump");
    assert_eq!(readout().0, 42.);
    click(cx, "half");
    assert_eq!(readout().1, 0.5);
    click(cx, "toggle");
    assert!(!media.state().paused);
    assert!(!readout().2);
}

#[gpui::test]
fn reports_the_playback_time_while_paused_as_the_media_reports_it(cx: &mut TestAppContext) {
    let (cx, engines) = memo(cx);
    let media = fake_playback(cx, &engines, 60.);
    media.advance_to(12.);
    settle(cx);
    assert_eq!(readout().3, 12.);
}

#[gpui::test]
fn reports_a_seek_in_the_same_render_while_the_media_plays(cx: &mut TestAppContext) {
    let (cx, engines) = memo(cx);
    fake_playback(cx, &engines, 60.);
    play(cx, "memo");
    let (_, store, _) = root("memo");
    READOUT.with(|r| r.borrow_mut().clear());
    cx.update(|_, cx| store.update(cx, |s, cx| s.seek(30., cx)));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let first = READOUT.with(|r| {
        r.borrow()
            .iter()
            .find(|(reported, ..)| *reported == 30.)
            .copied()
    });
    assert_eq!(first.map(|r| r.3), Some(30.));
}

#[gpui::test]
fn advances_the_playback_time_between_time_updates_while_playing(cx: &mut TestAppContext) {
    let (cx, engines) = memo(cx);
    fake_playback(cx, &engines, 60.);
    play(cx, "memo");
    let (_, store, _) = root("memo");
    cx.update(|_, cx| store.update(cx, |s, cx| s.seek(10., cx)));
    settle(cx);
    let before = readout().3;
    advance(cx, 500);
    let after = readout().3;
    assert!(after > before && after <= 60., "{before} then {after}");
    assert_eq!(readout().0, 10., "the reported time holds");
}

#[gpui::test]
fn checks_whether_the_volume_can_be_set_once_not_on_every_media_event(cx: &mut TestAppContext) {
    let (cx, engines) = memo(cx);
    let media = fake_playback(cx, &engines, 60.);
    let probes = media.state().volume_probes;
    for _ in 0..5 {
        media.fire([MediaEventKind::TimeUpdate]);
    }
    settle(cx);
    assert_eq!(media.state().volume_probes, probes);
    assert!(probes <= 1);
}

#[gpui::test]
#[should_panic(expected = "inside a MediaPlayer")]
fn use_media_player_panics_outside_a_media_player(cx: &mut TestAppContext) {
    cx.update(|cx| {
        use_media_player(cx);
    });
}

// --- The store -------------------------------------------------------------------

#[gpui::test]
fn clamps_seeks_and_volumes_as_upstream_does(cx: &mut TestAppContext) {
    let (cx, engines) = memo(cx);
    let media = fake_playback(cx, &engines, 60.);
    let (_, store, _) = root("memo");
    cx.update(|_, cx| store.update(cx, |s, cx| s.seek(90., cx)));
    assert_eq!(media.state().current_time, 60.);
    cx.update(|_, cx| store.update(cx, |s, cx| s.seek(-5., cx)));
    assert_eq!(media.state().current_time, 0.);
    cx.update(|_, cx| store.update(cx, |s, cx| s.seek(f64::NAN, cx)));
    assert_eq!(media.state().current_time, 0., "NaN is ignored");
    cx.update(|_, cx| store.update(cx, |s, cx| s.set_volume(1.5, cx)));
    assert_eq!(media.state().volume, 1.);
    cx.update(|_, cx| store.update(cx, |s, cx| s.set_volume(0., cx)));
    cx.update(|_, cx| store.update(cx, |s, cx| s.set_muted(true, cx)));
    cx.update(|_, cx| store.update(cx, |s, cx| s.set_muted(false, cx)));
    assert_eq!(
        media.state().volume,
        1.,
        "unmuting at volume 0 restores the volume"
    );
    cx.update(|_, cx| store.update(cx, |s, cx| s.set_playback_rate(0., cx)));
    assert_eq!(media.state().playback_rate, 1., "a rate of 0 is refused");
    // A live stream seeks forward without an end.
    media.update(|s| s.duration = f64::INFINITY);
    cx.update(|_, cx| store.update(cx, |s, cx| s.seek(500., cx)));
    assert_eq!(media.state().current_time, 500.);
    settle(cx);
    assert_eq!(store_state(cx, "memo").duration, f64::INFINITY);
}

#[gpui::test]
fn reads_the_buffered_range_at_the_playhead(cx: &mut TestAppContext) {
    let (cx, engines) = memo(cx);
    let media = fake_playback(cx, &engines, 60.);
    media.update(|s| {
        s.buffered = vec![0. ..10., 20. ..30.];
        s.current_time = 19.6;
    });
    media.fire([MediaEventKind::Progress]);
    settle(cx);
    // Half a second of tolerance keeps the range that starts at the playhead.
    assert_eq!(store_state(cx, "memo").buffered_end, 30.);
    media.update(|s| s.current_time = 5.);
    media.fire([MediaEventKind::Progress]);
    settle(cx);
    assert_eq!(store_state(cx, "memo").buffered_end, 10.);
}

#[gpui::test]
fn a_refused_play_keeps_the_state_paused(cx: &mut TestAppContext) {
    let (cx, engines) = memo(cx);
    let media = fake_playback(cx, &engines, 60.);
    media.update(|s| s.accepts_play = false);
    click(cx, "toggle");
    assert!(store_state(cx, "memo").is_paused);
    assert!(!store_state(cx, "memo").has_started);
}

#[gpui::test]
fn shows_live_for_a_stream_without_an_end(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        MediaPlayer::new("radio", "Radio")
            .child(MediaPlayerElement::new(MediaKind::Audio).src("media/live.mp3"))
            .child(MediaTime::new())
            .child(MediaTime::new().display(MediaTimeDisplay::Remaining))
            .into_any_element()
    });
    let media = fake_playback(cx, &engines, 3700.);
    media.advance_to(65.);
    settle(cx);
    assert_eq!(label("radio-time-current-duration"), "0:01:05 / 1:01:40");
    assert_eq!(label("radio-time-remaining"), "-1:00:35");
    media.load_metadata(f64::INFINITY);
    settle(cx);
    assert_eq!(label("radio-time-current-duration"), "Live");
}

#[gpui::test]
fn the_examples_render(cx: &mut TestAppContext) {
    let (cx, engines) = host_at(cx, 900., super::examples::examples);
    assert!(
        engines.borrow().len() >= 14,
        "every story made its media: {}",
        engines.borrow().len()
    );
    for media in engines.borrow().iter() {
        media.load_metadata(5.);
    }
    settle(cx);
    assert!(bounds(cx, "media-video").is_some());
    assert!(
        bounds(
            cx,
            "media-markers-time-slider-marker-0.8-Ana Ruiz: Love the light here."
        )
        .is_some()
    );
    // The small player's bar is compact.
    play(cx, "media-small");
    settle(cx);
    assert_eq!(bounds(cx, "media-small-bar").unwrap().size.height, px(32.));
}

#[gpui::test]
fn goes_fullscreen_with_f_and_the_fullscreen_button_and_comes_back(cx: &mut TestAppContext) {
    let (cx, engines) = host_at(cx, 480., video("tour"));
    fake_playback(cx, &engines, 120.);
    assert!(store_state(cx, "tour").can_fullscreen);
    assert_eq!(label("tour-fullscreen-button"), "Full screen");
    focus_root(cx, "tour");
    key(cx, "f");
    assert!(store_state(cx, "tour").is_fullscreen);
    assert_eq!(label("tour-fullscreen-button"), "Exit full screen");
    let viewport = cx.update(|window, _| window.viewport_size());
    let player = bounds(cx, "tour").expect("the player fills the window");
    assert_eq!(player.size, viewport);
    assert_eq!(player.origin, point(px(0.), px(0.)));
    // Escape leaves fullscreen, as a browser's does.
    key(cx, "escape");
    assert!(!store_state(cx, "tour").is_fullscreen);
    assert_eq!(bounds(cx, "tour").unwrap().size.width, px(480.));
    // Once playing, the bar takes presses.
    play(cx, "tour");
    click(cx, "tour-fullscreen-button");
    assert!(store_state(cx, "tour").is_fullscreen);
    click(cx, "tour-fullscreen-button");
    assert!(!store_state(cx, "tour").is_fullscreen);
}

#[gpui::test]
fn a_drag_on_the_timeline_previews_the_time_and_seeks_on_release(cx: &mut TestAppContext) {
    let (cx, engines) = host(cx, |_, _| {
        MediaPlayer::new("clip", "Clip")
            .child(MediaPlayerElement::new(MediaKind::Video).src("media/clip.mp4"))
            .child(div().pt(px(60.)).w(px(400.)).child(MediaTimeSlider::new()))
            .child(MediaTime::new())
            .into_any_element()
    });
    let media = fake_playback(cx, &engines, 100.);
    let b = bounds(cx, "clip-time-slider").unwrap();
    let at = |fraction: f32| {
        point(
            b.left() + px(10.) + (b.size.width - px(20.)) * fraction,
            b.center().y,
        )
    };
    cx.simulate_mouse_down(at(0.25), gpui::MouseButton::Left, Modifiers::none());
    settle(cx);
    cx.simulate_mouse_move(at(0.5), Some(gpui::MouseButton::Left), Modifiers::none());
    settle(cx);
    let (player, _, _) = root("clip");
    let scrub = cx.update(|_, cx| player.read(cx).scrub_time);
    assert!(
        scrub.is_some_and(|t| (t - 50.).abs() < 2.),
        "scrubbing at {scrub:?}"
    );
    assert_eq!(
        media.state().current_time,
        0.,
        "no seek while the drag lasts"
    );
    assert!(
        label("clip-time-current-duration").starts_with("0:5"),
        "{}",
        label("clip-time-current-duration")
    );
    cx.simulate_mouse_up(at(0.5), gpui::MouseButton::Left, Modifiers::none());
    settle(cx);
    assert!(
        (media.state().current_time - 50.).abs() < 2.,
        "seeked to {}",
        media.state().current_time
    );
    assert_eq!(cx.update(|_, cx| player.read(cx).scrub_time), None);
}
