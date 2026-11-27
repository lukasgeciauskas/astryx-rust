//! Upstream lab `MediaPlayer/AudioPlayer.tsx`: a ready-made player for spoken
//! audio and music, in a Section container.

use std::cell::Cell;
use std::rc::Rc;

use astryx_components::button::{ButtonVariant, IconButton};
use astryx_components::containers::Section;
use astryx_components::layout::{AspectFit, AspectRatio};
use astryx_i18n::current as i18n;
use astryx_primitives::media::{MediaCrossOrigin, MediaKind, MediaPreload};
use astryx_primitives::{ElementSize, provide};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, ImageSource, IntoElement, ObjectFit, ParentElement, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, img, prelude::*, px,
};

use super::context::{element_key, use_player_context};
use super::controls::{
    MediaContainerWidth, MediaPlayButton, MediaPlaybackRate, MediaSeekButton, MediaTime,
    MediaTimeDisplay, MediaVolume,
};
use super::element::MediaPlayerElement;
use super::icons::{MediaIcon, MediaIconName};
use super::player::{Callbacks, MediaPlayer};
use super::store::MediaPlayerError;
use super::timeline::{MediaTimeSlider, MediaTimelineChapter, MediaTimelineMarker};
use super::video_player::width_probe;

/// Upstream `ARTWORK_HEIGHT`: the artwork is as tall as the title and subtitle.
const ARTWORK_HEIGHT: f32 = 48.;

type Action = Rc<dyn Fn(&mut Window, &mut App)>;
type ArtworkFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// Upstream lab `AudioPlayer`: a ready-made audio player for episodes,
/// recordings and music: title and artwork, a timeline, skip back and forward,
/// playback speed, and volume. It sits in a Section container, so its background
/// and padding follow the theme. Queues and playlists belong to the product.
///
/// ```ignore
/// AudioPlayer::new("episode", "media/episode-12.mp3", "Designing for every screen")
///     .subtitle("The Interface Show · Episode 12")
/// ```
#[derive(IntoElement)]
pub struct AudioPlayer {
    id: ElementId,
    src: SharedString,
    title: SharedString,
    subtitle: Option<SharedString>,
    artwork_src: Option<ImageSource>,
    artwork: Option<ArtworkFn>,
    artwork_aspect_ratio: f32,
    label: Option<SharedString>,
    skip_back_seconds: f64,
    skip_forward_seconds: f64,
    preload: MediaPreload,
    cross_origin: Option<MediaCrossOrigin>,
    on_previous_track: Option<Action>,
    on_next_track: Option<Action>,
    chapters: Vec<MediaTimelineChapter>,
    markers: Vec<MediaTimelineMarker>,
    callbacks: Callbacks,
    style: StyleRefinement,
}

impl AudioPlayer {
    /// Upstream `src` and `title` (the episode's or track's title, which also names
    /// the player).
    pub fn new(
        id: impl Into<ElementId>,
        src: impl Into<SharedString>,
        title: impl Into<SharedString>,
    ) -> Self {
        AudioPlayer {
            id: id.into(),
            src: src.into(),
            title: title.into(),
            subtitle: None,
            artwork_src: None,
            artwork: None,
            artwork_aspect_ratio: 1.,
            label: None,
            skip_back_seconds: 15.,
            skip_forward_seconds: 30.,
            preload: MediaPreload::Metadata,
            cross_origin: None,
            on_previous_track: None,
            on_next_track: None,
            chapters: Vec::new(),
            markers: Vec::new(),
            callbacks: Callbacks::default(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `subtitle`: the show, the artist or another supporting line.
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Upstream `artworkSrc`: artwork shown in front of the title, such as cover art.
    pub fn artwork_src(mut self, src: impl Into<ImageSource>) -> Self {
        self.artwork_src = Some(src.into());
        self
    }

    /// Upstream `artwork`: media in front of the title in place of `artworkSrc`,
    /// such as a video still. Without either, a generic audio file icon shows.
    pub fn artwork(
        mut self,
        artwork: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.artwork = Some(Rc::new(artwork));
        self
    }

    /// Upstream `artworkAspectRatio`: width divided by height, 1 by default.
    pub fn artwork_aspect_ratio(mut self, ratio: f32) -> Self {
        self.artwork_aspect_ratio = ratio;
        self
    }

    /// Upstream `label`: the accessible name, the title by default.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `skipBackSeconds`: 15 by default.
    pub fn skip_back_seconds(mut self, seconds: f64) -> Self {
        self.skip_back_seconds = seconds;
        self
    }

    /// Upstream `skipForwardSeconds`: 30 by default.
    pub fn skip_forward_seconds(mut self, seconds: f64) -> Self {
        self.skip_forward_seconds = seconds;
        self
    }

    /// Upstream `preload`: `metadata` by default.
    pub fn preload(mut self, preload: MediaPreload) -> Self {
        self.preload = preload;
        self
    }

    /// Upstream `crossOrigin`: the CORS mode for the audio in the web build; native
    /// engines ignore it.
    pub fn cross_origin(mut self, cross_origin: MediaCrossOrigin) -> Self {
        self.cross_origin = Some(cross_origin);
        self
    }

    /// Upstream `onPreviousTrack`: the previous-track control appears when it is set.
    pub fn on_previous_track(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_previous_track = Some(Rc::new(f));
        self
    }

    /// Upstream `onNextTrack`: the next-track control appears when it is set.
    pub fn on_next_track(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_next_track = Some(Rc::new(f));
        self
    }

    /// Upstream `chapters`.
    pub fn chapters(mut self, chapters: impl IntoIterator<Item = MediaTimelineChapter>) -> Self {
        self.chapters = chapters.into_iter().collect();
        self
    }

    /// Upstream `markers`.
    pub fn markers(mut self, markers: impl IntoIterator<Item = MediaTimelineMarker>) -> Self {
        self.markers = markers.into_iter().collect();
        self
    }

    /// Upstream `onPlay`.
    pub fn on_play(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.callbacks.on_play = Some(Rc::new(f) as Action);
        self
    }

    /// Upstream `onPause`.
    pub fn on_pause(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.callbacks.on_pause = Some(Rc::new(f) as Action);
        self
    }

    /// Upstream `onEnded`.
    pub fn on_ended(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.callbacks.on_ended = Some(Rc::new(f) as Action);
        self
    }

    /// Upstream `onError`.
    pub fn on_error(
        mut self,
        f: impl Fn(&MediaPlayerError, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.callbacks.on_error = Some(Rc::new(f));
        self
    }
}

impl Styled for AudioPlayer {
    /// Upstream `xstyle`: the Section.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Upstream `TrackButton`: the previous or next track in the product's queue.
fn track_button(id: ElementId, previous: bool, on_click: Action, cx: &App) -> AnyElement {
    let label = i18n(cx).t(if previous {
        "@astryx.mediaPlayer.previousTrack"
    } else {
        "@astryx.mediaPlayer.nextTrack"
    });
    #[cfg(test)]
    super::tests::record(&id, &label, false, false);
    let key = element_key(&id);
    div()
        .flex()
        .debug_selector(move || key)
        .child(
            IconButton::new(
                id,
                label.clone(),
                MediaIcon::new(if previous {
                    MediaIconName::PreviousTrack
                } else {
                    MediaIconName::NextTrack
                }),
            )
            .tooltip(label)
            .variant(ButtonVariant::Ghost)
            .on_click(move |_, window, cx| on_click(window, cx)),
        )
        .into_any_element()
}

/// Upstream `AudioPlayerLayout`.
#[derive(IntoElement)]
struct AudioParts {
    title: SharedString,
    subtitle: Option<SharedString>,
    artwork_src: Option<ImageSource>,
    artwork: Option<ArtworkFn>,
    ratio: f32,
    skip_back: f64,
    skip_forward: f64,
    on_previous_track: Option<Action>,
    on_next_track: Option<Action>,
    chapters: Vec<MediaTimelineChapter>,
    markers: Vec<MediaTimelineMarker>,
    width: Rc<Cell<f32>>,
}

impl RenderOnce for AudioParts {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let context = use_player_context("AudioPlayer", cx);
        let key = element_key(&context.id);
        let artwork: AnyElement = match (&self.artwork, &self.artwork_src) {
            (Some(f), _) => f(window, cx),
            (None, Some(src)) => img(src.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            (None, None) => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(t.color.text_secondary)
                .child(MediaIcon::new(MediaIconName::AudioFile).size(px(24.)))
                .into_any_element(),
        };
        let artwork_selector = format!("{key}-artwork");
        let mut art = div()
            .debug_selector(move || artwork_selector)
            .flex_none()
            .w(px(ARTWORK_HEIGHT * self.ratio))
            .overflow_hidden()
            .rounded(t.radius.element)
            .bg(t.color.background_muted)
            .child(
                AspectRatio::new(self.ratio)
                    .fit(AspectFit::Cover)
                    .child(artwork),
            );
        theme.refine(art.style(), "media-artwork", &[], None);
        let title_selector = format!("{key}-titles");
        let header = div()
            .flex()
            .items_center()
            .gap(t.spacing.s3)
            .min_w_0()
            .child(art)
            .child(
                div()
                    .debug_selector(move || title_selector)
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_style(&t.text.body)
                            .font_weight(t.font.weight.semibold)
                            .child(self.title),
                    )
                    .children(self.subtitle.map(|s| {
                        div()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_style(&t.text.supporting)
                            .text_color(t.color.text_secondary)
                            .child(s)
                    })),
            );
        let secondary = t.color.text_secondary;
        let times = div()
            .flex()
            .justify_between()
            .child(
                MediaTime::new()
                    .display(MediaTimeDisplay::Current)
                    .text_color(secondary),
            )
            .child(
                MediaTime::new()
                    .display(MediaTimeDisplay::Remaining)
                    .text_color(secondary),
            );
        let mut transport = div().flex().items_center().gap(t.spacing.s2);
        if let Some(previous) = self.on_previous_track {
            transport = transport.child(track_button(
                context.part("previous-track"),
                true,
                previous,
                cx,
            ));
        }
        transport = transport
            .child(MediaSeekButton::new(-self.skip_back))
            .child(
                MediaPlayButton::new()
                    .size(ElementSize::Lg)
                    .variant(ButtonVariant::Primary),
            )
            .child(MediaSeekButton::new(self.skip_forward));
        if let Some(next) = self.on_next_track {
            transport = transport.child(track_button(context.part("next-track"), false, next, cx));
        }
        // A grid of 1fr, auto and 1fr: the transport centered, speed at the start and
        // volume at the end.
        let controls = div()
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_start()
                    .child(MediaPlaybackRate::new()),
            )
            .child(transport)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_end()
                    .child(MediaVolume::new()),
            );
        let width = self.width.get();
        provide(
            MediaContainerWidth(px(width)),
            div()
                .relative()
                .flex()
                .flex_col()
                .gap(t.spacing.s2)
                .child(width_probe(self.width))
                .child(header)
                .child(
                    MediaTimeSlider::new()
                        .chapters(self.chapters)
                        .markers(self.markers),
                )
                .child(times)
                .child(controls),
        )
    }
}

impl RenderOnce for AudioPlayer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = current(window, cx).tokens().clone();
        let key = element_key(&self.id);
        let width = window
            .use_keyed_state(
                ElementId::Name(format!("{key}-width").into()),
                cx,
                |_, _| Rc::new(Cell::new(0.)),
            )
            .read(cx)
            .clone();
        let label = self.label.clone().unwrap_or_else(|| self.title.clone());
        let mut section = Section::new();
        section.style().refine(&self.style);
        section.child(
            MediaPlayer::new(self.id, label)
                .callbacks(self.callbacks)
                .text_color(t.color.text_primary)
                .child({
                    let element = MediaPlayerElement::new(MediaKind::Audio)
                        .src(self.src)
                        .preload(self.preload);
                    match self.cross_origin {
                        Some(cross_origin) => element.cross_origin(cross_origin),
                        None => element,
                    }
                })
                .child(AudioParts {
                    title: self.title,
                    subtitle: self.subtitle,
                    artwork_src: self.artwork_src,
                    artwork: self.artwork,
                    ratio: self.artwork_aspect_ratio,
                    skip_back: self.skip_back_seconds,
                    skip_forward: self.skip_forward_seconds,
                    on_previous_track: self.on_previous_track,
                    on_next_track: self.on_next_track,
                    chapters: self.chapters,
                    markers: self.markers,
                    width,
                }),
        )
    }
}
