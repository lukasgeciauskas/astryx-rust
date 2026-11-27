//! No media engine (Linux and other platforms without one in this build): every
//! source fails as an unsupported one, as a browser reports media it cannot play.

use std::ops::Range;

use gpui::Size;

use super::{MediaBackend, MediaError, MediaEventKind, MediaFrame, MediaSource};

/// The backend where the platform has no media engine.
pub struct UnsupportedBackend {
    error: Option<MediaError>,
    events: Vec<MediaEventKind>,
    volume: f64,
    muted: bool,
}

impl Default for UnsupportedBackend {
    fn default() -> Self {
        UnsupportedBackend {
            error: None,
            events: Vec::new(),
            volume: 1.,
            muted: false,
        }
    }
}

impl MediaBackend for UnsupportedBackend {
    fn load(&mut self, _: &MediaSource) {
        self.error = Some(MediaError::unsupported(
            "This platform has no media engine.",
        ));
        self.events.extend([
            MediaEventKind::Emptied,
            MediaEventKind::LoadStart,
            MediaEventKind::Error,
        ]);
    }
    fn play(&mut self) -> bool {
        false
    }
    fn pause(&mut self) {}
    fn paused(&self) -> bool {
        true
    }
    fn ended(&self) -> bool {
        false
    }
    fn current_time(&self) -> f64 {
        0.
    }
    fn set_current_time(&mut self, _: f64) {}
    fn duration(&self) -> f64 {
        f64::NAN
    }
    fn buffered(&self) -> Vec<Range<f64>> {
        Vec::new()
    }
    fn volume(&self) -> f64 {
        self.volume
    }
    fn set_volume(&mut self, volume: f64) {
        self.volume = volume;
    }
    fn muted(&self) -> bool {
        self.muted
    }
    fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }
    fn playback_rate(&self) -> f64 {
        1.
    }
    fn set_playback_rate(&mut self, _: f64) {}
    fn error(&self) -> Option<MediaError> {
        self.error.clone()
    }
    fn video_size(&self) -> Option<Size<u32>> {
        None
    }
    fn take_events(&mut self) -> Vec<MediaEventKind> {
        std::mem::take(&mut self.events)
    }
    fn take_frame(&mut self) -> Option<MediaFrame> {
        None
    }
}
