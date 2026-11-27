//! Upstream lab `MediaPlayer/mediaIcons.tsx`: the glyphs the media control parts
//! draw. A theme restyles one by its key; without one, Lucide's drawing.

use gpui::{App, IntoElement, RenderOnce, Styled, Window, svg};
use rustryx_components::icon::IconDefaultSize;
use rustryx_primitives::use_context;
use rustryx_theme::current;

/// Upstream `MediaIconName`: one media glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaIconName {
    Play,
    Pause,
    Replay,
    SkipBack,
    SkipForward,
    VolumeHigh,
    VolumeLow,
    VolumeOff,
    FullscreenEnter,
    FullscreenExit,
    Captions,
    PreviousTrack,
    NextTrack,
    AudioFile,
}

impl MediaIconName {
    /// Upstream `MEDIA_ICON_KEYS`: the icon-registry key a theme restyles the glyph
    /// by, such as `media:play`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Play => "media:play",
            Self::Pause => "media:pause",
            Self::Replay => "media:replay",
            Self::SkipBack => "media:skip-back",
            Self::SkipForward => "media:skip-forward",
            Self::VolumeHigh => "media:volume-high",
            Self::VolumeLow => "media:volume-low",
            Self::VolumeOff => "media:volume-off",
            Self::FullscreenEnter => "media:fullscreen-enter",
            Self::FullscreenExit => "media:fullscreen-exit",
            Self::Captions => "media:captions",
            Self::PreviousTrack => "media:previous-track",
            Self::NextTrack => "media:next-track",
            Self::AudioFile => "media:audio-file",
        }
    }

    /// The fallback glyph's SVG (Lucide's drawing).
    fn fallback(self) -> String {
        const SPEAKER: &str = "M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z";
        let body = match self {
            Self::Play => r#"<path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z"/>"#.to_string(),
            Self::Pause => r#"<rect x="14" y="3" width="5" height="18" rx="1"/><rect x="5" y="3" width="5" height="18" rx="1"/>"#.to_string(),
            Self::Replay => r#"<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/>"#.to_string(),
            Self::SkipBack => r#"<path d="M12 6a2 2 0 0 0-3.414-1.414l-6 6a2 2 0 0 0 0 2.828l6 6A2 2 0 0 0 12 18z"/><path d="M22 6a2 2 0 0 0-3.414-1.414l-6 6a2 2 0 0 0 0 2.828l6 6A2 2 0 0 0 22 18z"/>"#.to_string(),
            Self::SkipForward => r#"<path d="M12 6a2 2 0 0 1 3.414-1.414l6 6a2 2 0 0 1 0 2.828l-6 6A2 2 0 0 1 12 18z"/><path d="M2 6a2 2 0 0 1 3.414-1.414l6 6a2 2 0 0 1 0 2.828l-6 6A2 2 0 0 1 2 18z"/>"#.to_string(),
            Self::VolumeHigh => format!(r#"<path d="{SPEAKER}"/><path d="M16 9a5 5 0 0 1 0 6"/><path d="M19.364 18.364a9 9 0 0 0 0-12.728"/>"#),
            Self::VolumeLow => format!(r#"<path d="{SPEAKER}"/><path d="M16 9a5 5 0 0 1 0 6"/>"#),
            Self::VolumeOff => format!(r#"<path d="{SPEAKER}"/><line x1="22" x2="16" y1="9" y2="15"/><line x1="16" x2="22" y1="9" y2="15"/>"#),
            Self::FullscreenEnter => r#"<path d="M8 3H5a2 2 0 0 0-2 2v3"/><path d="M21 8V5a2 2 0 0 0-2-2h-3"/><path d="M3 16v3a2 2 0 0 0 2 2h3"/><path d="M16 21h3a2 2 0 0 0 2-2v-3"/>"#.to_string(),
            Self::FullscreenExit => r#"<path d="M8 3v3a2 2 0 0 1-2 2H3"/><path d="M21 8h-3a2 2 0 0 1-2-2V3"/><path d="M3 16h3a2 2 0 0 1 2 2v3"/><path d="M16 21v-3a2 2 0 0 1 2-2h3"/>"#.to_string(),
            Self::Captions => r#"<rect width="18" height="14" x="3" y="5" rx="2" ry="2"/><path d="M7 15h4M15 15h2M7 11h2M13 11h4"/>"#.to_string(),
            Self::PreviousTrack => r#"<path d="M17.971 4.285A2 2 0 0 1 21 6v12a2 2 0 0 1-3.029 1.715l-9.997-5.998a2 2 0 0 1-.003-3.432z"/><path d="M3 20V4"/>"#.to_string(),
            Self::NextTrack => r#"<path d="M21 4v16"/><path d="M6.029 4.285A2 2 0 0 0 3 6v12a2 2 0 0 0 3.029 1.715l9.997-5.998a2 2 0 0 0 .003-3.432z"/>"#.to_string(),
            Self::AudioFile => r#"<path d="M11.65 22H18a2 2 0 0 0 2-2V8a2.4 2.4 0 0 0-.706-1.706l-3.588-3.588A2.4 2.4 0 0 0 14 2H6a2 2 0 0 0-2 2v10.35"/><path d="M14 2v5a1 1 0 0 0 1 1h5"/><path d="M8 20v-7l3 1.474"/><circle cx="6" cy="20" r="2"/>"#.to_string(),
        };
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"#
        )
    }
}

/// Upstream `useMediaIcon`: a media glyph, the active theme's artwork for its key,
/// else the Lucide-drawn fallback. It takes the size its button gives icons (1em
/// otherwise) and the surrounding text color.
#[derive(IntoElement)]
pub struct MediaIcon {
    name: MediaIconName,
    size: Option<gpui::Pixels>,
}

impl MediaIcon {
    /// The glyph `name`.
    pub fn new(name: MediaIconName) -> Self {
        MediaIcon { name, size: None }
    }

    /// A size of its own, in place of its container's icon size.
    pub fn size(mut self, size: gpui::Pixels) -> Self {
        self.size = Some(size);
        self
    }
}

impl RenderOnce for MediaIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let size = self.size.unwrap_or_else(|| {
            let rems = use_context::<IconDefaultSize>(cx)
                .map(|s| s.0)
                .unwrap_or_default()
                .rems();
            rems.to_pixels(window.rem_size())
        });
        let color = window.text_style().color;
        let el = match theme.theme.icon(self.name.key()) {
            Some(path) => svg().path(path.to_string()),
            None => svg().data(self.name.fallback().as_bytes()),
        };
        el.flex_none().size(size).text_color(color)
    }
}
