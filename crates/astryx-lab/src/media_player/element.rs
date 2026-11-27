//! Upstream lab `MediaPlayer/MediaPlayerElement.tsx`: the media element part of a
//! MediaPlayer, the counterpart of its `<video>` or `<audio>`.

use astryx_primitives::media::{
    MediaCrossOrigin, MediaFrame, MediaKind, MediaPreload, MediaSource, MediaTrackSource,
    TextTrackKind,
};
use astryx_primitives::use_context;
use astryx_theme::current;
use gpui::{
    App, Hsla, ImageSource, IntoElement, ObjectFit, ParentElement, Pixels, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, img, prelude::*, px,
};

use super::context::{element_key, use_player_context};

/// How far a VideoPlayer's caption cues sit above the bottom edge: clear of its
/// control bar (upstream `layOutCues`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct CueClearance(pub Pixels);

/// A `<track>` child: a caption or subtitle file (WebVTT).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MediaTrack {
    /// `kind`: captions or subtitles.
    pub kind: TextTrackKind,
    /// `src`: a WebVTT file path, or a `data:text/vtt,` URL.
    pub src: SharedString,
    /// `label`: the name shown to people.
    pub label: SharedString,
    /// `srcLang`: a BCP 47 language tag.
    pub language: SharedString,
    /// `default`: shows this track when the media loads.
    pub is_default: bool,
}

/// Upstream lab `MediaPlayerElement`: the media of a MediaPlayer. A video draws its
/// frames (its box fills the player, the picture fitted inside on the inverted
/// background) and its showing captions; audio draws nothing. The browser's own
/// controls never show: the player's parts are the controls.
///
/// ```ignore
/// MediaPlayerElement::new(MediaKind::Video).src("media/clip.mp4").poster("media/clip.jpg")
/// ```
#[derive(IntoElement)]
pub struct MediaPlayerElement {
    kind: MediaKind,
    src: SharedString,
    poster: Option<ImageSource>,
    preload: MediaPreload,
    cross_origin: Option<MediaCrossOrigin>,
    auto_play: bool,
    muted: bool,
    looping: bool,
    plays_inline: bool,
    tracks: Vec<MediaTrack>,
    style: StyleRefinement,
}

impl MediaPlayerElement {
    /// Upstream `kind`: video or audio.
    pub fn new(kind: MediaKind) -> Self {
        MediaPlayerElement {
            kind,
            src: SharedString::default(),
            poster: None,
            preload: MediaPreload::Metadata,
            cross_origin: None,
            auto_play: false,
            muted: false,
            looping: false,
            plays_inline: true,
            tracks: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `src`: a URL or a local path.
    pub fn src(mut self, src: impl Into<SharedString>) -> Self {
        self.src = src.into();
        self
    }

    /// Upstream `poster` (video): the image shown before playback starts.
    pub fn poster(mut self, poster: impl Into<ImageSource>) -> Self {
        self.poster = Some(poster.into());
        self
    }

    /// Upstream `preload`: `metadata` by default.
    pub fn preload(mut self, preload: MediaPreload) -> Self {
        self.preload = preload;
        self
    }

    /// Upstream `crossOrigin`: the CORS mode of the web build's fetch, for media
    /// (and its frames) from another origin; native engines ignore it.
    pub fn cross_origin(mut self, cross_origin: MediaCrossOrigin) -> Self {
        self.cross_origin = Some(cross_origin);
        self
    }

    /// Upstream `autoPlay`.
    pub fn auto_play(mut self, auto_play: bool) -> Self {
        self.auto_play = auto_play;
        self
    }

    /// Upstream `muted`: the initial muted state.
    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    /// Upstream `loop`.
    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Upstream `playsInline` (video, on by default): native players always play
    /// in place, so it changes nothing here.
    pub fn plays_inline(mut self, plays_inline: bool) -> Self {
        self.plays_inline = plays_inline;
        self
    }

    /// Upstream `children`: a `<track>`.
    pub fn track(mut self, track: MediaTrack) -> Self {
        self.tracks.push(track);
        self
    }

    fn source(&self) -> MediaSource {
        MediaSource {
            kind: self.kind,
            src: self.src.clone(),
            preload: self.preload,
            cross_origin: self.cross_origin,
            autoplay: self.auto_play,
            looping: self.looping,
            muted: self.muted,
            tracks: self
                .tracks
                .iter()
                .map(|t| MediaTrackSource {
                    kind: t.kind,
                    src: t.src.clone(),
                    language: t.language.clone(),
                    label: t.label.clone(),
                    default: t.is_default,
                    id: SharedString::default(),
                })
                .collect(),
        }
    }
}

impl Styled for MediaPlayerElement {
    /// Upstream `xstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The browser's default cue: light text on a dark band, 5% of the video's height.
const CUE_TEXT: Hsla = Hsla {
    h: 0.,
    s: 0.,
    l: 1.,
    a: 1.,
};
const CUE_BAND: Hsla = Hsla {
    h: 0.,
    s: 0.,
    l: 0.,
    a: 0.8,
};

impl RenderOnce for MediaPlayerElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let context = use_player_context("MediaPlayerElement", cx);
        let source = self.source();
        let media = context.store.update(cx, |s, cx| s.element(cx));
        media.update(cx, |m, cx| m.set_source(source, cx));
        let kind = match self.kind {
            MediaKind::Video => "video",
            MediaKind::Audio => "audio",
        };
        let selector = format!("{}-{kind}", element_key(&context.id));
        if self.kind == MediaKind::Audio {
            let mut el = div().debug_selector(move || selector).absolute().size_0();
            theme.refine(el.style(), "media-player-element", &[("kind", kind)], None);
            return el.into_any_element();
        }
        let has_started = context.store.read(cx).state().has_started;
        let (frame, cues) = {
            let m = media.read(cx);
            (m.frame().cloned(), m.active_cues())
        };
        let picture: Option<gpui::AnyElement> = match (&self.poster, has_started, frame) {
            (Some(poster), false, _) => Some(
                img(poster.clone())
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
            ),
            (_, _, Some(MediaFrame::Image(image))) => Some(
                img(image)
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
            ),
            #[cfg(target_os = "macos")]
            (_, _, Some(MediaFrame::PixelBuffer(buffer))) => Some(
                gpui::surface(buffer)
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
            ),
            _ => None,
        };
        let clearance = use_context::<CueClearance>(cx).map_or(px(0.), |c| c.0);
        // The browser sizes cues to 5% of the video's height.
        let height = window
            .use_keyed_state(
                gpui::ElementId::Name(format!("{selector}-height").into()),
                cx,
                |_, _| std::rc::Rc::new(std::cell::Cell::new(0f32)),
            )
            .read(cx)
            .clone();
        let cue_size = (height.get() > 0.).then(|| px(height.get() * 0.05));
        let measure = height.clone();
        let mut el = div()
            .debug_selector(move || selector)
            .relative()
            .block()
            .size_full()
            .overflow_hidden()
            .bg(t.color.background_inverted)
            .child(
                gpui::canvas(
                    move |bounds, window, _| {
                        let h = f32::from(bounds.size.height);
                        if measure.replace(h) != h {
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .children(picture);
        theme.refine(el.style(), "media-player-element", &[("kind", kind)], None);
        el.style().refine(&self.style);
        if !cues.is_empty() {
            let band = canvas_cues(cues, clearance, cue_size);
            el = el.child(band);
        }
        el.into_any_element()
    }
}

/// The showing cues, centered over the bottom of the video, `clearance` above it,
/// at `size` (5% of the video's height, once measured).
fn canvas_cues(cues: Vec<SharedString>, clearance: Pixels, size: Option<Pixels>) -> gpui::Div {
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(clearance)
        .flex()
        .flex_col()
        .items_center()
        .when_some(size, |el, size| el.text_size(size))
        .children(cues.into_iter().map(|cue| {
            div()
                .px(px(4.))
                .bg(CUE_BAND)
                .text_color(CUE_TEXT)
                .text_center()
                .child(cue)
        }))
}
