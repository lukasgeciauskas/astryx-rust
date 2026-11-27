//! The media element over a fake engine, the WebVTT parser and the track sources.

use gpui::{AppContext as _, Entity, TestAppContext};

use super::unsupported::UnsupportedBackend;
use super::*;

#[test]
fn parses_webvtt_cues() {
    let text = "\u{FEFF}WEBVTT - Sample\r\n\r\nNOTE a comment\r\n\r\n1\r\n00:00.000 --> 00:02.500 align:start\r\nA bee <i>lands</i> on a\r\npink flower &amp; rests.\r\n\r\n00:01:02.250 --> 01:00:00.000\r\nLater\r\n\r\n00:05.000 --> bad\r\nskipped\r\n";
    let cues = parse_vtt(text);
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].start, 0.);
    assert_eq!(cues[0].end, 2.5);
    assert_eq!(
        cues[0].text.as_ref(),
        "A bee lands on a\npink flower & rests."
    );
    assert_eq!(cues[1].start, 62.25);
    assert_eq!(cues[1].end, 3600.);
}

#[test]
fn reads_track_text_from_data_urls() {
    assert_eq!(
        track_text("data:text/vtt,WEBVTT%0A%0A00:00.000%20--%3E%2000:01.000%0AHi").as_deref(),
        Some("WEBVTT\n\n00:00.000 --> 00:01.000\nHi")
    );
    // "WEBVTT" in base64.
    assert_eq!(
        track_text("data:text/vtt;base64,V0VCVlRU").as_deref(),
        Some("WEBVTT")
    );
    assert_eq!(track_text("data:text/vtt"), None);
}

fn element(cx: &mut TestAppContext) -> (Entity<MediaElement>, FakeMediaHandle) {
    let (backend, handle) = FakeMediaBackend::new();
    let element = cx.update(|cx| cx.new(|cx| MediaElement::new(Box::new(backend), cx)));
    (element, handle)
}

fn source(src: &str) -> MediaSource {
    MediaSource {
        kind: MediaKind::Video,
        src: src.to_string().into(),
        tracks: vec![MediaTrackSource {
            kind: TextTrackKind::Captions,
            src: "data:text/vtt,WEBVTT%0A%0A00:00.000 --> 00:02.000%0AHello".into(),
            language: "en".into(),
            label: "English".into(),
            default: true,
            id: "".into(),
        }],
        ..Default::default()
    }
}

#[gpui::test]
fn loads_a_source_once_and_keeps_playing_through_attribute_changes(cx: &mut TestAppContext) {
    let (element, media) = element(cx);
    cx.update(|cx| element.update(cx, |e, cx| e.set_source(source("a.mp4"), cx)));
    cx.update(|cx| element.update(cx, |e, cx| e.set_source(source("a.mp4"), cx)));
    assert_eq!(media.state().loads.len(), 1);
    let mut autoplay = source("a.mp4");
    autoplay.autoplay = true;
    autoplay.preload = MediaPreload::Auto;
    cx.update(|cx| element.update(cx, |e, cx| e.set_source(autoplay, cx)));
    assert_eq!(
        media.state().loads.len(),
        1,
        "attributes alone load nothing"
    );
    cx.update(|cx| element.update(cx, |e, cx| e.set_source(source("b.mp4"), cx)));
    assert_eq!(media.state().loads.len(), 2, "a new src loads");
}

#[gpui::test]
fn shows_a_default_track_and_its_cue_at_the_playhead(cx: &mut TestAppContext) {
    let (element, media) = element(cx);
    cx.update(|cx| element.update(cx, |e, cx| e.set_source(source("a.mp4"), cx)));
    media.advance_to(1.);
    let cues = cx.update(|cx| element.read(cx).active_cues());
    assert_eq!(cues, [SharedString::from("Hello")]);
    cx.update(|cx| element.update(cx, |e, cx| e.set_track_mode(0, TextTrackMode::Disabled, cx)));
    assert!(cx.update(|cx| element.read(cx).active_cues()).is_empty());
    media.advance_to(3.);
    cx.update(|cx| element.update(cx, |e, cx| e.set_track_mode(0, TextTrackMode::Showing, cx)));
    assert!(
        cx.update(|cx| element.read(cx).active_cues()).is_empty(),
        "past the cue"
    );
}

#[gpui::test]
fn reports_the_engines_events_in_order(cx: &mut TestAppContext) {
    let (element, media) = element(cx);
    let seen: std::rc::Rc<std::cell::RefCell<Vec<MediaElementEvent>>> = Default::default();
    let log = seen.clone();
    let _subscription =
        cx.update(|cx| cx.subscribe(&element, move |_, event, _| log.borrow_mut().push(*event)));
    cx.update(|cx| element.update(cx, |e, cx| e.set_source(source("a.mp4"), cx)));
    media.load_metadata(10.);
    cx.update(|cx| element.update(cx, |e, cx| e.poll(cx)));
    cx.run_until_parked();
    assert_eq!(
        *seen.borrow(),
        [
            MediaElementEvent::TracksChanged,
            MediaElementEvent::Media(MediaEventKind::Emptied),
            MediaElementEvent::Media(MediaEventKind::LoadStart),
            MediaElementEvent::Media(MediaEventKind::LoadedMetadata),
            MediaElementEvent::Media(MediaEventKind::DurationChange),
            MediaElementEvent::Media(MediaEventKind::CanPlay),
        ]
    );
    // Its poll delivers them without a call.
    media.fire([MediaEventKind::Progress]);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(
        seen.borrow().last(),
        Some(&MediaElementEvent::Media(MediaEventKind::Progress))
    );
}

#[test]
fn a_platform_without_an_engine_fails_every_source_as_unsupported() {
    let mut backend = UnsupportedBackend::default();
    backend.load(&source("a.mp4"));
    assert_eq!(
        backend.take_events(),
        [
            MediaEventKind::Emptied,
            MediaEventKind::LoadStart,
            MediaEventKind::Error
        ]
    );
    assert_eq!(backend.error().map(|e| e.code), Some(4));
    assert!(!backend.play());
    assert!(backend.paused());
    assert!(backend.duration().is_nan());
    assert_eq!(backend.volume(), 1.);
}
