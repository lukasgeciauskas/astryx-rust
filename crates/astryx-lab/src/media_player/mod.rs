//! Upstream lab `MediaPlayer/`: accessible audio and video players.
//!
//! - [`VideoPlayer`] and [`AudioPlayer`] are the ready-made presets.
//! - [`MediaPlayer`] is the root a custom layout composes with
//!   [`MediaPlayerElement`] and the control parts: [`MediaPlayButton`],
//!   [`MediaSeekButton`], [`MediaVolume`], [`MediaTimeSlider`], [`MediaTime`],
//!   [`MediaCaptionsButton`], [`MediaFullscreenButton`] and
//!   [`MediaPlaybackRate`].
//! - [`use_media_player`] and [`use_media_playback_time`] read the nearest player
//!   for custom controls.
//!
//! The media plays through `astryx_primitives::media`, the counterpart of the
//! browser's `HTMLMediaElement`: Media Foundation on Windows, AVFoundation on
//! macOS, an `HTMLMediaElement` on the web. Linux has no engine in this build,
//! so a player there shows the media error state.

mod audio_player;
mod context;
mod controls;
mod element;
pub mod examples;
mod icons;
mod player;
mod store;
mod timeline;
mod video_player;

#[cfg(test)]
mod tests;

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};

pub use audio_player::AudioPlayer;
pub use context::{
    DEFAULT_PLAYBACK_RATES, MediaControlsHold, MediaPlayerActions, UseMediaPlayerResult,
    format_clock, format_spoken_time, use_media_playback_time, use_media_player,
};
pub use controls::{
    MediaCaptionsButton, MediaControlSize, MediaFullscreenButton, MediaPlayButton,
    MediaPlaybackRate, MediaPlaybackRateVariant, MediaSeekButton, MediaSliderPlacement, MediaTime,
    MediaTimeDisplay, MediaVolume,
};
pub use element::{MediaPlayerElement, MediaTrack};
pub use icons::{MediaIcon, MediaIconName};
pub use player::MediaPlayer;
pub use store::{
    INITIAL_MEDIA_STATE, MediaPlayerError, MediaPlayerState, MediaStore, MediaStoreEvent,
    MediaTextTrackInfo, MediaTextTrackKind,
};
pub use timeline::{MediaTimeSlider, MediaTimelineChapter, MediaTimelineMarker};
pub use video_player::{VideoPlayer, VideoPlayerCaptionTrack};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const CROSS_ORIGIN: RustApi =
    RustApi::Method("cross_origin (MediaCrossOrigin; the web build's fetch)");
const CLASS_NAME: PropMapping = p("className", RustApi::Trait("Styled"));
const STYLE: PropMapping = p("style", RustApi::Trait("Styled"));
const XSTYLE: PropMapping = p("xstyle", RustApi::Trait("Styled"));

pub const VIDEO_PLAYER_DOC: ComponentDoc = ComponentDoc {
    name: "VideoPlayer",
    rust_type: "astryx::lab::media_player::VideoPlayer",
    props: &[
        p("src", RustApi::Argument("src")),
        p("label", RustApi::Argument("label")),
        p("poster", RustApi::Method("poster")),
        p(
            "captions",
            RustApi::Method("captions (VideoPlayerCaptionTrack)"),
        ),
        p("aspectRatio", RustApi::Method("aspect_ratio")),
        p("hasAutoPlay", RustApi::Method("has_auto_play")),
        p("isLooping", RustApi::Method("is_looping")),
        p("preload", RustApi::Method("preload (MediaPreload)")),
        p("crossOrigin", CROSS_ORIGIN),
        p(
            "chapters",
            RustApi::Method("chapters (MediaTimelineChapter)"),
        ),
        p("markers", RustApi::Method("markers (MediaTimelineMarker)")),
        p("onPlay", RustApi::Method("on_play")),
        p("onPause", RustApi::Method("on_pause")),
        p("onEnded", RustApi::Method("on_ended")),
        p("onError", RustApi::Method("on_error")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const AUDIO_PLAYER_DOC: ComponentDoc = ComponentDoc {
    name: "AudioPlayer",
    rust_type: "astryx::lab::media_player::AudioPlayer",
    props: &[
        p("src", RustApi::Argument("src")),
        p("title", RustApi::Argument("title")),
        p("subtitle", RustApi::Method("subtitle")),
        p("artworkSrc", RustApi::Method("artwork_src")),
        p("artwork", RustApi::Method("artwork")),
        p(
            "artworkAspectRatio",
            RustApi::Method("artwork_aspect_ratio"),
        ),
        p("onPreviousTrack", RustApi::Method("on_previous_track")),
        p("onNextTrack", RustApi::Method("on_next_track")),
        p("label", RustApi::Method("label")),
        p("skipBackSeconds", RustApi::Method("skip_back_seconds")),
        p(
            "skipForwardSeconds",
            RustApi::Method("skip_forward_seconds"),
        ),
        p("preload", RustApi::Method("preload (MediaPreload)")),
        p("crossOrigin", CROSS_ORIGIN),
        p("chapters", RustApi::Method("chapters")),
        p("markers", RustApi::Method("markers")),
        p("onPlay", RustApi::Method("on_play")),
        p("onPause", RustApi::Method("on_pause")),
        p("onEnded", RustApi::Method("on_ended")),
        p("onError", RustApi::Method("on_error")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_PLAYER_DOC: ComponentDoc = ComponentDoc {
    name: "MediaPlayer",
    rust_type: "astryx::lab::media_player::MediaPlayer",
    props: &[
        p("label", RustApi::Argument("label")),
        p("children", RustApi::Trait("ParentElement")),
        p("onPlay", RustApi::Method("on_play")),
        p("onPause", RustApi::Method("on_pause")),
        p("onEnded", RustApi::Method("on_ended")),
        p("onError", RustApi::Method("on_error")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_PLAYER_ELEMENT_DOC: ComponentDoc = ComponentDoc {
    name: "MediaPlayerElement",
    rust_type: "astryx::lab::media_player::MediaPlayerElement",
    props: &[
        p("kind", RustApi::Argument("kind (MediaKind)")),
        p("src", RustApi::Method("src")),
        p("poster", RustApi::Method("poster")),
        p("playsInline", RustApi::Method("plays_inline")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_PLAY_BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "MediaPlayButton",
    rust_type: "astryx::lab::media_player::MediaPlayButton",
    props: &[
        p("size", RustApi::Method("size (MediaControlSize)")),
        p("variant", RustApi::Method("variant (ButtonVariant)")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_SEEK_BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "MediaSeekButton",
    rust_type: "astryx::lab::media_player::MediaSeekButton",
    props: &[
        p("offset", RustApi::Argument("offset")),
        p("size", RustApi::Method("size")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_VOLUME_DOC: ComponentDoc = ComponentDoc {
    name: "MediaVolume",
    rust_type: "astryx::lab::media_player::MediaVolume",
    props: &[
        p("size", RustApi::Method("size")),
        p("hasSlider", RustApi::Method("has_slider")),
        p(
            "sliderPlacement",
            RustApi::Method("slider_placement (MediaSliderPlacement)"),
        ),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_TIME_SLIDER_DOC: ComponentDoc = ComponentDoc {
    name: "MediaTimeSlider",
    rust_type: "astryx::lab::media_player::MediaTimeSlider",
    props: &[
        p("chapters", RustApi::Method("chapters")),
        p("markers", RustApi::Method("markers")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_TIME_DOC: ComponentDoc = ComponentDoc {
    name: "MediaTime",
    rust_type: "astryx::lab::media_player::MediaTime",
    props: &[
        p("display", RustApi::Method("display (MediaTimeDisplay)")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_CAPTIONS_BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "MediaCaptionsButton",
    rust_type: "astryx::lab::media_player::MediaCaptionsButton",
    props: &[
        p("size", RustApi::Method("size")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_FULLSCREEN_BUTTON_DOC: ComponentDoc = ComponentDoc {
    name: "MediaFullscreenButton",
    rust_type: "astryx::lab::media_player::MediaFullscreenButton",
    props: &[
        p("size", RustApi::Method("size")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const MEDIA_PLAYBACK_RATE_DOC: ComponentDoc = ComponentDoc {
    name: "MediaPlaybackRate",
    rust_type: "astryx::lab::media_player::MediaPlaybackRate",
    props: &[
        p(
            "variant",
            RustApi::Method("variant (MediaPlaybackRateVariant)"),
        ),
        p("rates", RustApi::Method("rates")),
        p("size", RustApi::Method("size")),
        p("sliderPlacement", RustApi::Method("slider_placement")),
        XSTYLE,
        CLASS_NAME,
        STYLE,
    ],
    example: include_str!("examples.rs"),
};

pub const USE_MEDIA_PLAYER_DOC: ComponentDoc = ComponentDoc {
    name: "useMediaPlayer",
    rust_type: "astryx::lab::media_player::use_media_player -> UseMediaPlayerResult",
    props: &[],
    example: include_str!("examples.rs"),
};

pub const USE_MEDIA_PLAYBACK_TIME_DOC: ComponentDoc = ComponentDoc {
    name: "useMediaPlaybackTime",
    rust_type: "astryx::lab::media_player::use_media_playback_time",
    props: &[],
    example: include_str!("examples.rs"),
};
