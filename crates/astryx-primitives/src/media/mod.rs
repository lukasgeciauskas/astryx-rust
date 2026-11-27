//! Media playback: the counterpart of the browser's `HTMLMediaElement` that upstream's
//! media components play through.
//!
//! [`MediaElement`] is one media element: it loads a [`MediaSource`], plays it through
//! a platform [`MediaBackend`], reports the element's events ([`MediaEventKind`], the
//! same names as the DOM's) as GPUI events, holds the caption and subtitle tracks
//! ([`TextTrack`], parsed from WebVTT), and keeps the current video frame, which
//! [`media_view`] draws.
//!
//! The backends:
//! - Windows: Media Foundation's `IMFMediaEngine` in frame-server mode, each frame
//!   copied into a GPUI image.
//! - macOS: AVFoundation's `AVPlayer` with an `AVPlayerItemVideoOutput`, each frame a
//!   `CVPixelBuffer` GPUI draws as a surface.
//! - The web: an `HTMLVideoElement` or `HTMLAudioElement`, each frame read through a
//!   canvas.
//! - Elsewhere (Linux): none; every source fails as an unsupported one (`MediaError`
//!   code 4), as a browser reports media it cannot play.
//!
//! [`FakeMediaBackend`] plays nothing and is driven by hand, for tests.

mod fake;
#[cfg(target_os = "macos")]
mod macos;
#[cfg_attr(
    any(windows, target_os = "macos", target_family = "wasm"),
    allow(dead_code)
)]
mod unsupported;
pub mod vtt;
#[cfg(target_family = "wasm")]
mod web;
#[cfg(windows)]
mod windows;

use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    App, Context, Entity, EventEmitter, IntoElement, ObjectFit, ParentElement as _, RenderImage,
    SharedString, Size, Styled as _, StyledImage as _, Task, div, img,
};

#[cfg(test)]
mod tests;

pub use fake::{FakeMediaBackend, FakeMediaHandle, FakeMediaState};
pub use vtt::{VttCue, parse_vtt};

/// Upstream's `kind` of a media element: `<video>` or `<audio>`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MediaKind {
    #[default]
    Video,
    Audio,
}

/// The `preload` attribute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MediaPreload {
    None,
    /// The default.
    #[default]
    Metadata,
    Auto,
}

/// The `kind` of a text track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextTrackKind {
    Captions,
    Subtitles,
    Descriptions,
    Chapters,
    Metadata,
}

impl TextTrackKind {
    /// The DOM's name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Captions => "captions",
            Self::Subtitles => "subtitles",
            Self::Descriptions => "descriptions",
            Self::Chapters => "chapters",
            Self::Metadata => "metadata",
        }
    }
}

/// A text track's `mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextTrackMode {
    #[default]
    Disabled,
    Hidden,
    Showing,
}

/// A `<track>` child: where a caption or subtitle track comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MediaTrackSource {
    pub kind: TextTrackKind,
    /// A WebVTT file: a local path, or a `data:text/vtt,` URL.
    pub src: SharedString,
    /// `srclang`.
    pub language: SharedString,
    pub label: SharedString,
    /// `default`: shown when the source loads.
    pub default: bool,
    /// The track's `id` attribute, empty when unset.
    pub id: SharedString,
}

/// The `crossOrigin` attribute: the CORS mode of a web fetch. Native engines fetch
/// without CORS and ignore it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaCrossOrigin {
    Anonymous,
    UseCredentials,
}

/// What a media element plays: its `src` and attributes, and its `<track>` children.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct MediaSource {
    pub kind: MediaKind,
    /// A URL or a local path.
    pub src: SharedString,
    pub preload: MediaPreload,
    /// `crossOrigin`.
    pub cross_origin: Option<MediaCrossOrigin>,
    /// `autoplay`.
    pub autoplay: bool,
    /// `loop`.
    pub looping: bool,
    /// `muted`, the initial muted state.
    pub muted: bool,
    pub tracks: Vec<MediaTrackSource>,
}

/// One text track the element offers (the DOM's `TextTrack`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextTrack {
    /// The `id` attribute, empty when unset.
    pub id: SharedString,
    pub kind: TextTrackKind,
    pub label: SharedString,
    pub language: SharedString,
    pub mode: TextTrackMode,
    pub cues: Vec<VttCue>,
}

/// The DOM's `MediaError`: code 1 aborted, 2 network, 3 decode, 4 unsupported source.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MediaError {
    pub code: u16,
    pub message: SharedString,
}

impl MediaError {
    /// `MEDIA_ERR_SRC_NOT_SUPPORTED`.
    pub fn unsupported(message: impl Into<SharedString>) -> Self {
        MediaError {
            code: 4,
            message: message.into(),
        }
    }
}

/// A media element's events, the DOM's names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaEventKind {
    LoadStart,
    LoadedMetadata,
    DurationChange,
    TimeUpdate,
    Progress,
    Play,
    Playing,
    Pause,
    Ended,
    Waiting,
    CanPlay,
    Seeking,
    Seeked,
    VolumeChange,
    RateChange,
    Error,
    Emptied,
}

impl MediaEventKind {
    /// The DOM's event type.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LoadStart => "loadstart",
            Self::LoadedMetadata => "loadedmetadata",
            Self::DurationChange => "durationchange",
            Self::TimeUpdate => "timeupdate",
            Self::Progress => "progress",
            Self::Play => "play",
            Self::Playing => "playing",
            Self::Pause => "pause",
            Self::Ended => "ended",
            Self::Waiting => "waiting",
            Self::CanPlay => "canplay",
            Self::Seeking => "seeking",
            Self::Seeked => "seeked",
            Self::VolumeChange => "volumechange",
            Self::RateChange => "ratechange",
            Self::Error => "error",
            Self::Emptied => "emptied",
        }
    }
}

/// What a [`MediaElement`] emits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaElementEvent {
    /// A media event.
    Media(MediaEventKind),
    /// A text track was added, removed or changed mode (the track list's `addtrack`,
    /// `removetrack` and `change`).
    TracksChanged,
}

/// One decoded video frame.
#[derive(Clone)]
pub enum MediaFrame {
    /// A BGRA image.
    Image(Arc<RenderImage>),
    /// A CoreVideo buffer GPUI draws as a surface.
    #[cfg(target_os = "macos")]
    PixelBuffer(objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>),
}

/// The platform half of a media element: what the browser's media pipeline does.
/// Every call comes from the main thread.
pub trait MediaBackend {
    /// Loads `source` (setting `src` and calling `load()`).
    fn load(&mut self, source: &MediaSource);
    /// Starts playback; `false` when the platform refused it.
    fn play(&mut self) -> bool;
    fn pause(&mut self);
    fn paused(&self) -> bool;
    fn ended(&self) -> bool;
    /// Seconds.
    fn current_time(&self) -> f64;
    fn set_current_time(&mut self, time: f64);
    /// Seconds: `NaN` before metadata, `INFINITY` for a live stream.
    fn duration(&self) -> f64;
    /// The buffered ranges, in seconds.
    fn buffered(&self) -> Vec<Range<f64>>;
    fn volume(&self) -> f64;
    fn set_volume(&mut self, volume: f64);
    fn muted(&self) -> bool;
    fn set_muted(&mut self, muted: bool);
    fn playback_rate(&self) -> f64;
    fn set_playback_rate(&mut self, rate: f64);
    fn error(&self) -> Option<MediaError>;
    /// The video's natural size, once known.
    fn video_size(&self) -> Option<Size<u32>>;
    /// The events since the last call, in order.
    fn take_events(&mut self) -> Vec<MediaEventKind>;
    /// A new video frame, when one is ready since the last call.
    fn take_frame(&mut self) -> Option<MediaFrame>;
    /// Whether script can set the volume.
    fn can_set_volume(&self) -> bool {
        true
    }
}

/// The platform's backend.
pub fn platform_backend() -> Box<dyn MediaBackend> {
    #[cfg(windows)]
    return Box::new(windows::MediaEngineBackend::new());
    #[cfg(target_os = "macos")]
    return Box::new(macos::AvPlayerBackend::new());
    #[cfg(target_family = "wasm")]
    return Box::new(web::HtmlMediaBackend::new());
    #[cfg(not(any(windows, target_os = "macos", target_family = "wasm")))]
    return Box::new(unsupported::UnsupportedBackend::default());
}

type BackendFactory = std::rc::Rc<dyn Fn() -> Box<dyn MediaBackend>>;

struct MediaBackends(BackendFactory);

impl gpui::Global for MediaBackends {}

/// Makes every media element from now on play through backends `factory` makes, in
/// place of the platform's: tests drive a [`FakeMediaBackend`] this way.
pub fn set_media_backend_factory(
    factory: impl Fn() -> Box<dyn MediaBackend> + 'static,
    cx: &mut App,
) {
    cx.set_global(MediaBackends(std::rc::Rc::new(factory)));
}

/// A backend for a new media element: the app's factory's, else the platform's.
pub fn new_media_backend(cx: &App) -> Box<dyn MediaBackend> {
    match cx.try_global::<MediaBackends>() {
        Some(factory) => (factory.0)(),
        None => platform_backend(),
    }
}

/// A media element: the counterpart of `HTMLMediaElement`, as an entity.
pub struct MediaElement {
    backend: Box<dyn MediaBackend>,
    source: Option<MediaSource>,
    tracks: Vec<TextTrack>,
    frame: Option<MediaFrame>,
    _poll: Task<()>,
}

impl EventEmitter<MediaElementEvent> for MediaElement {}

/// How often an element reads its backend: every frame while it plays, less often
/// otherwise.
const PLAYING_POLL: Duration = Duration::from_millis(16);
const IDLE_POLL: Duration = Duration::from_millis(100);

impl MediaElement {
    /// An element playing through `backend`.
    pub fn new(backend: Box<dyn MediaBackend>, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                let wait = this
                    .update(cx, |el, _| {
                        if el.backend.paused() {
                            IDLE_POLL
                        } else {
                            PLAYING_POLL
                        }
                    })
                    .unwrap_or(IDLE_POLL);
                cx.background_executor().timer(wait).await;
                if this.update(cx, |el, cx| el.poll(cx)).is_err() {
                    break;
                }
            }
        });
        MediaElement {
            backend,
            source: None,
            tracks: Vec::new(),
            frame: None,
            _poll: poll,
        }
    }

    /// An element playing through the platform's backend.
    pub fn platform(cx: &mut Context<Self>) -> Self {
        Self::new(platform_backend(), cx)
    }

    /// Reads the backend: its events, and a new frame.
    pub fn poll(&mut self, cx: &mut Context<Self>) {
        let events = self.backend.take_events();
        if let Some(frame) = self.backend.take_frame() {
            if let Some(MediaFrame::Image(old)) = self.frame.replace(frame) {
                cx.drop_image(old, None);
            }
            cx.notify();
        }
        for event in events {
            cx.emit(MediaElementEvent::Media(event));
        }
    }

    /// The source it plays.
    pub fn source(&self) -> Option<&MediaSource> {
        self.source.as_ref()
    }

    /// Sets the source and loads it, unless it already plays this source. Its tracks
    /// load from their `src`; a `default` track shows. A change of attributes alone
    /// (`preload`, `autoplay`, `loop`, `muted`) loads nothing, as a browser keeps
    /// playing when they change; they apply at the next load.
    pub fn set_source(&mut self, source: MediaSource, cx: &mut Context<Self>) {
        if let Some(current) = &self.source
            && current.src == source.src
            && current.kind == source.kind
            && current.cross_origin == source.cross_origin
            && current.tracks == source.tracks
        {
            self.source = Some(source);
            return;
        }
        self.tracks = load_tracks(&source.tracks);
        self.backend.load(&source);
        self.source = Some(source);
        if let Some(MediaFrame::Image(old)) = self.frame.take() {
            cx.drop_image(old, None);
        }
        cx.emit(MediaElementEvent::TracksChanged);
        cx.notify();
    }

    /// `load()`: loads the source again.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        if let Some(source) = self.source.clone() {
            self.backend.load(&source);
            cx.notify();
        }
    }

    /// `play()`: `false` when the platform refused playback.
    pub fn play(&mut self, cx: &mut Context<Self>) -> bool {
        let accepted = self.backend.play();
        cx.notify();
        accepted
    }

    /// `pause()`.
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        self.backend.pause();
        cx.notify();
    }

    /// `paused`.
    pub fn paused(&self) -> bool {
        self.backend.paused()
    }

    /// `ended`.
    pub fn ended(&self) -> bool {
        self.backend.ended()
    }

    /// `currentTime`, in seconds.
    pub fn current_time(&self) -> f64 {
        self.backend.current_time()
    }

    /// Sets `currentTime`.
    pub fn set_current_time(&mut self, time: f64, cx: &mut Context<Self>) {
        self.backend.set_current_time(time);
        cx.notify();
    }

    /// `duration`: `NaN` before metadata, `INFINITY` for a live stream.
    pub fn duration(&self) -> f64 {
        self.backend.duration()
    }

    /// `buffered`: the buffered ranges, in seconds.
    pub fn buffered(&self) -> Vec<Range<f64>> {
        self.backend.buffered()
    }

    /// `volume`, 0 to 1.
    pub fn volume(&self) -> f64 {
        self.backend.volume()
    }

    /// Sets `volume`.
    pub fn set_volume(&mut self, volume: f64, cx: &mut Context<Self>) {
        self.backend.set_volume(volume);
        cx.notify();
    }

    /// `muted`.
    pub fn muted(&self) -> bool {
        self.backend.muted()
    }

    /// Sets `muted`.
    pub fn set_muted(&mut self, muted: bool, cx: &mut Context<Self>) {
        self.backend.set_muted(muted);
        cx.notify();
    }

    /// `playbackRate`.
    pub fn playback_rate(&self) -> f64 {
        self.backend.playback_rate()
    }

    /// Sets `playbackRate`.
    pub fn set_playback_rate(&mut self, rate: f64, cx: &mut Context<Self>) {
        self.backend.set_playback_rate(rate);
        cx.notify();
    }

    /// `error`.
    pub fn error(&self) -> Option<MediaError> {
        self.backend.error()
    }

    /// Whether script can set the volume.
    pub fn can_set_volume(&self) -> bool {
        self.backend.can_set_volume()
    }

    /// The video's natural size.
    pub fn video_size(&self) -> Option<Size<u32>> {
        self.backend.video_size()
    }

    /// `textTracks`.
    pub fn text_tracks(&self) -> &[TextTrack] {
        &self.tracks
    }

    /// Sets a track's `mode` (the track list fires `change`).
    pub fn set_track_mode(&mut self, index: usize, mode: TextTrackMode, cx: &mut Context<Self>) {
        if let Some(track) = self.tracks.get_mut(index)
            && track.mode != mode
        {
            track.mode = mode;
            cx.emit(MediaElementEvent::TracksChanged);
            cx.notify();
        }
    }

    /// The text of the showing track's cues at the playhead, one string per cue.
    pub fn active_cues(&self) -> Vec<SharedString> {
        let time = self.current_time();
        self.tracks
            .iter()
            .filter(|t| t.mode == TextTrackMode::Showing)
            .flat_map(|t| {
                t.cues
                    .iter()
                    .filter(|c| c.start <= time && time < c.end)
                    .map(|c| c.text.clone())
            })
            .collect()
    }

    /// The current video frame.
    pub fn frame(&self) -> Option<&MediaFrame> {
        self.frame.as_ref()
    }
}

/// A track's WebVTT text: a local file, or a `data:` URL.
fn track_text(src: &str) -> Option<String> {
    if let Some(rest) = src.strip_prefix("data:") {
        let (meta, data) = rest.split_once(',')?;
        return Some(if meta.ends_with(";base64") {
            String::from_utf8(base64_decode(data)?).ok()?
        } else {
            percent_decode(data)
        });
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let path = src.strip_prefix("file://").unwrap_or(src);
        std::fs::read_to_string(path).ok()
    }
    #[cfg(target_family = "wasm")]
    None
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[i + 1..(i + 3).min(text.len())], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let clean: Vec<u8> = text
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
        .collect();
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= value(*c)? << (18 - 6 * i);
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..chunk.len().saturating_sub(1)]);
    }
    Some(out)
}

fn load_tracks(sources: &[MediaTrackSource]) -> Vec<TextTrack> {
    sources
        .iter()
        .map(|s| TextTrack {
            id: s.id.clone(),
            kind: s.kind,
            label: s.label.clone(),
            language: s.language.clone(),
            mode: if s.default {
                TextTrackMode::Showing
            } else {
                TextTrackMode::Disabled
            },
            cues: track_text(&s.src)
                .map(|t| parse_vtt(&t))
                .unwrap_or_default(),
        })
        .collect()
}

/// Draws a media element's current video frame, fitted into its box (`object-fit:
/// contain`), with the showing track's cues over its bottom.
pub fn media_view(element: &Entity<MediaElement>, cx: &App) -> gpui::Div {
    let el = element.read(cx);
    let frame: Option<gpui::AnyElement> = match el.frame() {
        Some(MediaFrame::Image(image)) => Some(
            img(image.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
        ),
        #[cfg(target_os = "macos")]
        Some(MediaFrame::PixelBuffer(buffer)) => Some(
            gpui::surface(buffer.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
        ),
        None => None,
    };
    div().relative().size_full().children(frame)
}
