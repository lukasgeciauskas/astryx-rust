//! The MediaPlayer gallery (upstream's `Lab/MediaPlayer` stories).

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Hsla, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window,
    div, px, relative,
};
use rustryx_components::layout::{SpacingStep, Stack};
use rustryx_components::text::{Heading, HeadingLevel, Text, TextColor, TextType};
use rustryx_components::theming::{MediaThemeMode, media_theme};
use rustryx_i18n::{Direction, I18n, provider};
use rustryx_primitives::media::{MediaKind, MediaPreload};
use rustryx_theme::current;

use super::{
    AudioPlayer, MediaPlayButton, MediaPlaybackRate, MediaPlaybackRateVariant, MediaPlayer,
    MediaPlayerElement, MediaTime, MediaTimeDisplay, MediaTimeSlider, MediaTimelineChapter,
    MediaTimelineMarker, MediaVolume, VideoPlayer, VideoPlayerCaptionTrack,
    use_media_playback_time, use_media_player,
};
use crate::comments::{CommentThread, CommentThreadData, CommentThreadShared as _, CommentUser};

// CC0 sample media.
const VIDEO_SRC: &str = "https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4";
const AUDIO_SRC: &str =
    "https://interactive-examples.mdn.mozilla.net/media/cc0-audio/t-rex-roar.mp3";
const ARTWORK_SRC: &str =
    "https://interactive-examples.mdn.mozilla.net/media/cc0-images/grapefruit-slice-332-332.jpg";

/// The sample captions, as a `data:` URL.
const CAPTIONS_VTT: &str = "data:text/vtt,WEBVTT%0A%0A00:00.000 --> 00:02.500%0AA bee lands on a pink flower.%0A%0A00:02.500 --> 00:05.000%0AIt gathers pollen and flies away.%0A";

fn chapters() -> Vec<MediaTimelineChapter> {
    [(0., "Arrival"), (1.6, "Pollen"), (3.4, "Take-off")]
        .into_iter()
        .map(|(start, title)| MediaTimelineChapter {
            start,
            title: title.into(),
        })
        .collect()
}

fn comment_markers() -> Vec<MediaTimelineMarker> {
    let viewer = CommentUser::new("sam", "Sam Lee");
    let ana = CommentUser::new("ana", "Ana Ruiz");
    let lin = CommentUser::new("lin", "Lin Park");
    let mira = CommentUser::new("mira", "Mira Chen");
    let threads = [
        (
            0.8,
            CommentThreadData::new(
                "c1",
                ana.clone(),
                "Love the light here.",
                "2026-11-04T17:02:00Z",
            )
            .replies([crate::comments::CommentData::new(
                "c1-r1",
                lin.clone(),
                "Same. Can we keep this take?",
                "2026-11-04T17:10:00Z",
            )]),
        ),
        (
            2.2,
            CommentThreadData::new(
                "c2",
                lin,
                "Can we hold on this for a beat longer?",
                "2026-11-04T17:05:00Z",
            ),
        ),
        (
            3.9,
            CommentThreadData::new("c3", mira, "Cut before the blur.", "2026-11-04T17:20:00Z")
                .unread(true),
        ),
    ];
    threads
        .into_iter()
        .map(|(time, thread)| {
            let label = format!("{}: {}", thread.author.name, thread.body);
            let viewer = viewer.clone();
            MediaTimelineMarker::new(time, label).content(move |_, _| {
                CommentThread::new(format!("media-marker-{}", thread.id), thread.clone())
                    .current_user(viewer.clone())
                    .into_any_element()
            })
        })
        .collect()
}

fn story(title: &'static str, width: f32, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(Heading::new(HeadingLevel::H3, title))
        .child(div().w(px(width)).max_w_full().child(body))
        .into_any_element()
}

fn video(id: &'static str) -> VideoPlayer {
    VideoPlayer::new(id, VIDEO_SRC, "Bee on a flower")
}

/// Upstream's `ElapsedPercent`: a custom readout through `use_media_player`.
#[derive(IntoElement)]
struct ElapsedPercent;

impl RenderOnce for ElapsedPercent {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = use_media_player(cx).state;
        let percent = if state.duration > 0. {
            state.current_time / state.duration
        } else {
            0.
        };
        Text::new(format!("{}%", (percent * 100.).round()))
            .text_type(TextType::Supporting)
            .color(TextColor::Secondary)
    }
}

/// A progress line: thinner than ProgressBar's track.
fn progress_line(fraction: f64) -> gpui::Div {
    div()
        .relative()
        .h(px(3.))
        .overflow_hidden()
        .rounded_full()
        .bg(Hsla {
            h: 0.,
            s: 0.,
            l: 1.,
            a: 0.35,
        })
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(relative(fraction.clamp(0., 1.) as f32))
                .bg(gpui::white()),
        )
}

/// Upstream's `HoverProgress`: the only chrome of a controlless player.
#[derive(IntoElement)]
struct HoverProgress;

impl RenderOnce for HoverProgress {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = current(window, cx).tokens().clone();
        let state = use_media_player(cx).state;
        let time = use_media_playback_time(window, cx);
        div()
            .absolute()
            .left(t.spacing.s2)
            .right(t.spacing.s2)
            .bottom(t.spacing.s2)
            .child(progress_line(if state.duration > 0. {
                time / state.duration
            } else {
                0.
            }))
    }
}

/// Upstream's `FeedStepper`: one segment per clip, done, playing, then to come.
#[derive(IntoElement)]
struct FeedStepper {
    count: usize,
    index: usize,
}

impl RenderOnce for FeedStepper {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = current(window, cx).tokens().clone();
        let state = use_media_player(cx).state;
        let time = use_media_playback_time(window, cx);
        let playing = if state.duration > 0. {
            time / state.duration
        } else {
            0.
        };
        div()
            .absolute()
            .left(t.spacing.s2)
            .right(t.spacing.s2)
            .top(t.spacing.s2)
            .flex()
            .gap(t.spacing.s1)
            .children((0..self.count).map(|step| {
                let fraction = if step < self.index {
                    1.
                } else if step == self.index {
                    playing
                } else {
                    0.
                };
                div().flex_1().child(progress_line(fraction))
            }))
    }
}

const FEED_CLIPS: [&str; 3] = ["Bee on a flower", "Second clip", "Third clip"];

fn feed(window: &mut Window, cx: &mut App) -> AnyElement {
    let index = window
        .use_keyed_state("media-feed-index", cx, |_, _| Rc::new(Cell::new(0usize)))
        .read(cx)
        .clone();
    let t = current(window, cx).tokens().clone();
    let at = index.get();
    let next = index.clone();
    MediaPlayer::new(
        SharedString::from(format!("media-feed-{at}")),
        FEED_CLIPS[at],
    )
    .on_ended(move |window, _| {
        next.set((next.get() + 1) % FEED_CLIPS.len());
        window.refresh();
    })
    .relative()
    .w(px(360.))
    .max_w_full()
    .aspect_ratio(9. / 12.)
    .overflow_hidden()
    .rounded(t.radius.container)
    .bg(gpui::black())
    .child(
        MediaPlayerElement::new(MediaKind::Video)
            .src(VIDEO_SRC)
            .auto_play(true)
            .muted(true),
    )
    .child(FeedStepper {
        count: FEED_CLIPS.len(),
        index: at,
    })
    // Controls over the video take the dark media colors.
    .child(
        div()
            .absolute()
            .top(t.spacing.s4)
            .right(t.spacing.s2)
            .rounded_full()
            .bg(Hsla {
                h: 0.,
                s: 0.,
                l: 0.,
                a: 0.45,
            })
            .child(media_theme(
                MediaThemeMode::Dark,
                MediaVolume::new().has_slider(false),
            )),
    )
    .into_any_element()
}

/// The MediaPlayer examples: the video player in its sizes and options, the audio
/// player, a custom layout, chapters, comment markers, and controlless players.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let t = current(window, cx).tokens().clone();
    let rtl = provider(
        I18n::new("ar").dir(Direction::Rtl),
        Stack::vertical()
            .gap(SpacingStep::S2)
            .child(
                Text::new("The page reads right to left; the timeline still runs left to right")
                    .text_type(TextType::Supporting)
                    .color(TextColor::Secondary),
            )
            .child(video("media-rtl")),
    );
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(story("Video", 720., video("media-video")))
        .child(story(
            "Small player",
            240.,
            video("media-small").captions([VideoPlayerCaptionTrack::new(
                CAPTIONS_VTT,
                "English",
                "en",
            )]),
        ))
        .child(story(
            "Video with captions",
            720.,
            video("media-captions").captions([VideoPlayerCaptionTrack::new(
                CAPTIONS_VTT,
                "English",
                "en",
            )
            .is_default(true)]),
        ))
        .child(story(
            "Muted autoplay loop",
            720.,
            video("media-autoplay").has_auto_play(true).is_looping(true),
        ))
        .child(story("Right to left", 720., rtl))
        .child(story(
            "Audio",
            720.,
            AudioPlayer::new("media-audio", AUDIO_SRC, "Designing for every screen")
                .subtitle("The Interface Show · Episode 12")
                .on_previous_track(|_, _| {})
                .on_next_track(|_, _| {}),
        ))
        .child(story(
            "Audio with artwork",
            720.,
            AudioPlayer::new(
                "media-audio-artwork",
                AUDIO_SRC,
                "Designing for every screen",
            )
            .subtitle("The Interface Show · Episode 12")
            .artwork_src(ARTWORK_SRC)
            .on_previous_track(|_, _| {})
            .on_next_track(|_, _| {}),
        ))
        .child(story(
            "Audio with wide artwork",
            720.,
            AudioPlayer::new("media-audio-wide", AUDIO_SRC, "Designing for every screen")
                .subtitle("The Interface Show · Episode 12")
                .artwork_src(ARTWORK_SRC)
                .artwork_aspect_ratio(16. / 9.),
        ))
        .child(story(
            "Custom layout",
            720.,
            MediaPlayer::new("media-custom", "Voice memo")
                .child(MediaPlayerElement::new(MediaKind::Audio).src(AUDIO_SRC))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.spacing.s2)
                        .child(MediaPlayButton::new())
                        .child(MediaTimeSlider::new())
                        .child(MediaTime::new().display(MediaTimeDisplay::Remaining))
                        .child(ElapsedPercent)
                        .child(MediaPlaybackRate::new().variant(MediaPlaybackRateVariant::Menu))
                        .child(MediaVolume::new()),
                ),
        ))
        .child(story(
            "Chapters",
            720.,
            video("media-chapters").chapters(chapters()),
        ))
        .child(story(
            "Comment markers",
            720.,
            video("media-markers").markers(comment_markers()),
        ))
        .child(story(
            "Buffered range",
            720.,
            video("media-buffered").preload(MediaPreload::Auto),
        ))
        .child(story(
            "Progress ticker",
            720.,
            MediaPlayer::new("media-ticker", "Bee on a flower")
                .relative()
                .w_full()
                .aspect_ratio(16. / 9.)
                .overflow_hidden()
                .rounded(t.radius.container)
                .bg(gpui::black())
                .child(
                    MediaPlayerElement::new(MediaKind::Video)
                        .src(VIDEO_SRC)
                        .auto_play(true)
                        .muted(true)
                        .looping(true),
                )
                .child(HoverProgress),
        ))
        .child(story("Feed clip", 360., feed(window, cx)))
        .into_any_element()
}
