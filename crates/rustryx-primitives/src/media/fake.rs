//! A backend that plays nothing and is driven by hand, for tests: the test sets what
//! the media reports and fires its events.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::Size;

use super::{MediaBackend, MediaError, MediaEventKind, MediaFrame, MediaSource};

/// What a [`FakeMediaBackend`] reports.
#[derive(Clone, Debug, PartialEq)]
pub struct FakeMediaState {
    pub paused: bool,
    pub ended: bool,
    pub current_time: f64,
    pub duration: f64,
    pub buffered: Vec<Range<f64>>,
    pub volume: f64,
    pub muted: bool,
    pub playback_rate: f64,
    pub error: Option<MediaError>,
    pub video_size: Option<Size<u32>>,
    /// Whether `play()` is accepted.
    pub accepts_play: bool,
    pub can_set_volume: bool,
    /// How many times the element asked whether the volume can be set.
    pub volume_probes: usize,
    /// The sources loaded, in order.
    pub loads: Vec<MediaSource>,
    pub events: Vec<MediaEventKind>,
}

impl Default for FakeMediaState {
    fn default() -> Self {
        FakeMediaState {
            paused: true,
            ended: false,
            current_time: 0.,
            duration: f64::NAN,
            buffered: Vec::new(),
            volume: 1.,
            muted: false,
            playback_rate: 1.,
            error: None,
            video_size: None,
            accepts_play: true,
            can_set_volume: true,
            volume_probes: 0,
            loads: Vec::new(),
            events: Vec::new(),
        }
    }
}

/// The test's hold on a fake backend.
#[derive(Clone, Default)]
pub struct FakeMediaHandle(Rc<RefCell<FakeMediaState>>);

impl FakeMediaHandle {
    /// Changes what the media reports.
    pub fn update(&self, f: impl FnOnce(&mut FakeMediaState)) {
        f(&mut self.0.borrow_mut());
    }

    /// What the media reports.
    pub fn state(&self) -> FakeMediaState {
        self.0.borrow().clone()
    }

    /// Queues events for the element's next read.
    pub fn fire(&self, events: impl IntoIterator<Item = MediaEventKind>) {
        self.0.borrow_mut().events.extend(events);
    }

    /// Reports loaded metadata of `duration` seconds, as a source that loads does.
    pub fn load_metadata(&self, duration: f64) {
        self.update(|s| s.duration = duration);
        self.fire([
            MediaEventKind::LoadedMetadata,
            MediaEventKind::DurationChange,
            MediaEventKind::CanPlay,
        ]);
    }

    /// Ends playback, as the media reaching its end does.
    pub fn end(&self) {
        self.update(|s| {
            s.paused = true;
            s.ended = true;
            s.current_time = if s.duration.is_finite() {
                s.duration
            } else {
                s.current_time
            };
        });
        self.fire([MediaEventKind::Pause, MediaEventKind::Ended]);
    }

    /// Fails, as a source that cannot play does.
    pub fn fail(&self, error: MediaError) {
        self.update(|s| s.error = Some(error));
        self.fire([MediaEventKind::Error]);
    }

    /// Moves the playhead, as playback does, and fires `timeupdate`.
    pub fn advance_to(&self, time: f64) {
        self.update(|s| s.current_time = time);
        self.fire([MediaEventKind::TimeUpdate]);
    }
}

/// A backend that plays nothing, driven through its [`FakeMediaHandle`]: `play`,
/// `pause`, seeking and the volume setters change its state and fire the events a
/// browser fires for them.
pub struct FakeMediaBackend(FakeMediaHandle);

impl FakeMediaBackend {
    /// A fake backend and the test's handle on it.
    pub fn new() -> (Self, FakeMediaHandle) {
        let handle = FakeMediaHandle::default();
        (FakeMediaBackend(handle.clone()), handle)
    }

    fn with<T>(&self, f: impl FnOnce(&mut FakeMediaState) -> T) -> T {
        f(&mut self.0.0.borrow_mut())
    }
}

impl MediaBackend for FakeMediaBackend {
    fn load(&mut self, source: &MediaSource) {
        self.with(|s| {
            s.loads.push(source.clone());
            s.paused = true;
            s.ended = false;
            s.current_time = 0.;
            s.duration = f64::NAN;
            s.error = None;
            s.muted = source.muted;
            s.events
                .extend([MediaEventKind::Emptied, MediaEventKind::LoadStart]);
        });
    }
    fn play(&mut self) -> bool {
        self.with(|s| {
            if !s.accepts_play {
                return false;
            }
            if s.paused {
                s.paused = false;
                if s.ended {
                    s.ended = false;
                    s.current_time = 0.;
                }
                s.events
                    .extend([MediaEventKind::Play, MediaEventKind::Playing]);
            }
            true
        })
    }
    fn pause(&mut self) {
        self.with(|s| {
            if !s.paused {
                s.paused = true;
                s.events.push(MediaEventKind::Pause);
            }
        });
    }
    fn paused(&self) -> bool {
        self.with(|s| s.paused)
    }
    fn ended(&self) -> bool {
        self.with(|s| s.ended)
    }
    fn current_time(&self) -> f64 {
        self.with(|s| s.current_time)
    }
    fn set_current_time(&mut self, time: f64) {
        self.with(|s| {
            s.current_time = time;
            s.ended = false;
            s.events.extend([
                MediaEventKind::Seeking,
                MediaEventKind::TimeUpdate,
                MediaEventKind::Seeked,
            ]);
        });
    }
    fn duration(&self) -> f64 {
        self.with(|s| s.duration)
    }
    fn buffered(&self) -> Vec<Range<f64>> {
        self.with(|s| s.buffered.clone())
    }
    fn volume(&self) -> f64 {
        self.with(|s| s.volume)
    }
    fn set_volume(&mut self, volume: f64) {
        self.with(|s| {
            if s.can_set_volume && s.volume != volume {
                s.volume = volume;
                s.events.push(MediaEventKind::VolumeChange);
            }
        });
    }
    fn muted(&self) -> bool {
        self.with(|s| s.muted)
    }
    fn set_muted(&mut self, muted: bool) {
        self.with(|s| {
            if s.muted != muted {
                s.muted = muted;
                s.events.push(MediaEventKind::VolumeChange);
            }
        });
    }
    fn playback_rate(&self) -> f64 {
        self.with(|s| s.playback_rate)
    }
    fn set_playback_rate(&mut self, rate: f64) {
        self.with(|s| {
            if s.playback_rate != rate {
                s.playback_rate = rate;
                s.events.push(MediaEventKind::RateChange);
            }
        });
    }
    fn error(&self) -> Option<MediaError> {
        self.with(|s| s.error.clone())
    }
    fn video_size(&self) -> Option<Size<u32>> {
        self.with(|s| s.video_size)
    }
    fn take_events(&mut self) -> Vec<MediaEventKind> {
        self.with(|s| std::mem::take(&mut s.events))
    }
    fn take_frame(&mut self) -> Option<MediaFrame> {
        None
    }
    fn can_set_volume(&self) -> bool {
        self.with(|s| {
            s.volume_probes += 1;
            s.can_set_volume
        })
    }
}
