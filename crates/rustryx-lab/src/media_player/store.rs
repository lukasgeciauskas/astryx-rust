//! Upstream lab `MediaPlayer/mediaStore.ts`: one player's playback state, read from
//! its media element's events, and the actions that drive the element.

use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, SharedString, Subscription, Window,
};
use rustryx_primitives::media::{
    MediaElement, MediaElementEvent, MediaEventKind, TextTrackKind, TextTrackMode,
};
use web_time::Instant;

/// Upstream `MediaTextTrackInfo`: a caption or subtitle track the media offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaTextTrackInfo {
    /// The track's `id`, or `language:label:index` when it has none.
    pub id: SharedString,
    pub label: SharedString,
    pub language: SharedString,
    pub kind: MediaTextTrackKind,
}

/// A text track's `kind`: `'captions'` or `'subtitles'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaTextTrackKind {
    Captions,
    Subtitles,
}

/// Upstream `MediaPlayerError`: the media element's error. `code` follows
/// `MediaError`: 1 aborted, 2 network, 3 decode, 4 source not supported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaPlayerError {
    pub code: u16,
    pub message: SharedString,
}

/// Upstream `MediaPlayerState`: the playback state a player reads from its media.
#[derive(Clone, Debug)]
pub struct MediaPlayerState {
    /// Upstream `isPaused`.
    pub is_paused: bool,
    /// Upstream `isEnded`.
    pub is_ended: bool,
    /// Upstream `isWaiting`: playback stalled for data.
    pub is_waiting: bool,
    /// Upstream `hasStarted`: playback started since the source loaded.
    pub has_started: bool,
    /// Upstream `currentTime`, in seconds.
    pub current_time: f64,
    /// Upstream `duration`, in seconds: 0 before metadata, `INFINITY` for a live stream.
    pub duration: f64,
    /// Upstream `bufferedEnd`: the end of the buffered range at the playhead, in seconds.
    pub buffered_end: f64,
    /// Upstream `volume`, 0 to 1.
    pub volume: f64,
    /// Upstream `isMuted`.
    pub is_muted: bool,
    /// Upstream `playbackRate`.
    pub playback_rate: f64,
    /// Upstream `isFullscreen`.
    pub is_fullscreen: bool,
    /// Upstream `canFullscreen`.
    pub can_fullscreen: bool,
    /// Upstream `canSetVolume`: whether the platform lets the volume be set.
    pub can_set_volume: bool,
    /// Upstream `textTracks`: the caption and subtitle tracks.
    pub text_tracks: Vec<MediaTextTrackInfo>,
    /// Upstream `activeTextTrackId`: the showing track.
    pub active_text_track_id: Option<SharedString>,
    /// Upstream `error`.
    pub error: Option<MediaPlayerError>,
}

/// Upstream `INITIAL_MEDIA_STATE`.
pub const INITIAL_MEDIA_STATE: MediaPlayerState = MediaPlayerState {
    is_paused: true,
    is_ended: false,
    is_waiting: false,
    has_started: false,
    current_time: 0.,
    duration: 0.,
    buffered_end: 0.,
    volume: 1.,
    is_muted: false,
    playback_rate: 1.,
    is_fullscreen: false,
    can_fullscreen: false,
    can_set_volume: true,
    text_tracks: Vec::new(),
    active_text_track_id: None,
    error: None,
};

impl Default for MediaPlayerState {
    fn default() -> Self {
        INITIAL_MEDIA_STATE
    }
}

/// `Object.is` for numbers.
fn same_number(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

/// Upstream `sameTracks`: the same ids, labels and languages, in order.
fn same_tracks(a: &[MediaTextTrackInfo], b: &[MediaTextTrackInfo]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.id == y.id && x.label == y.label && x.language == y.language)
}

impl PartialEq for MediaPlayerState {
    /// Each field by `Object.is`, the tracks by [`same_tracks`].
    fn eq(&self, other: &Self) -> bool {
        self.is_paused == other.is_paused
            && self.is_ended == other.is_ended
            && self.is_waiting == other.is_waiting
            && self.has_started == other.has_started
            && same_number(self.current_time, other.current_time)
            && same_number(self.duration, other.duration)
            && same_number(self.buffered_end, other.buffered_end)
            && same_number(self.volume, other.volume)
            && self.is_muted == other.is_muted
            && same_number(self.playback_rate, other.playback_rate)
            && self.is_fullscreen == other.is_fullscreen
            && self.can_fullscreen == other.can_fullscreen
            && self.can_set_volume == other.can_set_volume
            && same_tracks(&self.text_tracks, &other.text_tracks)
            && self.active_text_track_id == other.active_text_track_id
            && self.error == other.error
    }
}

/// `Math.min(Math.max(value, min), max)`: `NaN` stays `NaN`.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value.is_nan() {
        value
    } else {
        value.max(min).min(max)
    }
}

/// What a [`MediaStore`] emits.
#[derive(Clone, Debug)]
pub enum MediaStoreEvent {
    /// The state changed (upstream's `subscribe` listeners); `previous` is the
    /// state before.
    Changed { previous: Box<MediaPlayerState> },
}

/// Upstream `readTextTracks`: the caption and subtitle tracks, and the showing one.
fn read_text_tracks(media: &MediaElement) -> (Vec<MediaTextTrackInfo>, Option<SharedString>) {
    let mut tracks = Vec::new();
    let mut active = None;
    for (index, track) in media.text_tracks().iter().enumerate() {
        let kind = match track.kind {
            TextTrackKind::Captions => MediaTextTrackKind::Captions,
            TextTrackKind::Subtitles => MediaTextTrackKind::Subtitles,
            _ => continue,
        };
        let id: SharedString = if track.id.is_empty() {
            format!("{}:{}:{index}", track.language, track.label).into()
        } else {
            track.id.clone()
        };
        if track.mode == TextTrackMode::Showing {
            active = Some(id.clone());
        }
        tracks.push(MediaTextTrackInfo {
            id,
            label: track.label.clone(),
            language: track.language.clone(),
            kind,
        });
    }
    (tracks, active)
}

/// Upstream `readBufferedEnd`: the furthest end of a buffered range that starts
/// within half a second past the playhead.
fn read_buffered_end(media: &MediaElement) -> f64 {
    let time = media.current_time();
    // Half a second of tolerance keeps the range that starts at the playhead.
    media
        .buffered()
        .iter()
        .filter(|r| r.start <= time + 0.5)
        .fold(0., |end: f64, r| end.max(r.end))
}

/// Upstream `createMediaStore`: one player's state and actions. It reads its media
/// element on each of the element's events and notifies on a change.
pub struct MediaStore {
    state: MediaPlayerState,
    media: Option<Entity<MediaElement>>,
    media_subscriptions: Vec<Subscription>,
    /// Whether the player's root is drawn (upstream `container`).
    container: bool,
    /// Whether the player's root is the fullscreen element.
    fullscreen: bool,
    /// Whether the player put the window in fullscreen, so leaving gives it back.
    owns_window_fullscreen: bool,
    /// Whether the window was seen in fullscreen since the player entered it.
    saw_window_fullscreen: bool,
    last_text_track_id: Option<SharedString>,
    /// The platform does not change while the app runs, so one probe serves every
    /// media event.
    can_set_volume: Option<bool>,
    /// When the time the smoothed playback time grows from was last reported.
    reported_at: Instant,
}

impl EventEmitter<MediaStoreEvent> for MediaStore {}

impl MediaStore {
    /// A store with no media.
    pub fn new(cx: &mut Context<Self>) -> Self {
        MediaStore {
            state: INITIAL_MEDIA_STATE,
            media: None,
            media_subscriptions: Vec::new(),
            container: false,
            fullscreen: false,
            owns_window_fullscreen: false,
            saw_window_fullscreen: false,
            last_text_track_id: None,
            can_set_volume: None,
            reported_at: cx.background_executor().now(),
        }
    }

    /// Upstream `getState`.
    pub fn state(&self) -> &MediaPlayerState {
        &self.state
    }

    /// The media element the store reads, once attached.
    pub fn media(&self) -> Option<&Entity<MediaElement>> {
        self.media.as_ref()
    }

    /// When the reported playback time last changed, or playback started or stopped
    /// advancing: where [`super::use_media_playback_time`] measures from.
    pub fn reported_at(&self) -> Instant {
        self.reported_at
    }

    /// The media element, made on first use with the app's media backend.
    pub(crate) fn element(&mut self, cx: &mut Context<Self>) -> Entity<MediaElement> {
        if let Some(media) = &self.media {
            return media.clone();
        }
        let media = cx.new(|cx| {
            let backend = rustryx_primitives::media::new_media_backend(cx);
            MediaElement::new(backend, cx)
        });
        self.attach_media(Some(media.clone()), cx);
        media
    }

    fn set_state(&mut self, mut next: MediaPlayerState, cx: &mut Context<Self>) {
        if same_tracks(&self.state.text_tracks, &next.text_tracks) {
            next.text_tracks = self.state.text_tracks.clone();
        }
        if next == self.state {
            return;
        }
        let advancing = |s: &MediaPlayerState| !s.is_paused && !s.is_ended && !s.is_waiting;
        if !same_number(next.current_time, self.state.current_time)
            || advancing(&next) != advancing(&self.state)
            || !same_number(next.duration, self.state.duration)
            || !same_number(next.playback_rate, self.state.playback_rate)
        {
            self.reported_at = cx.background_executor().now();
        }
        let previous = std::mem::replace(&mut self.state, next);
        cx.emit(MediaStoreEvent::Changed {
            previous: Box::new(previous),
        });
        cx.notify();
    }

    fn patch(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut MediaPlayerState)) {
        let mut next = self.state.clone();
        f(&mut next);
        self.set_state(next, cx);
    }

    fn read_can_fullscreen(&self) -> bool {
        self.container
    }

    /// Upstream `readAll`: every state the element reports.
    fn read_all(&mut self, cx: &App) -> MediaPlayerState {
        let mut next = self.state.clone();
        let Some(media) = &self.media else {
            return next;
        };
        let element = media.read(cx);
        let (tracks, active) = read_text_tracks(element);
        let duration = element.duration();
        next.is_paused = element.paused();
        next.is_ended = element.ended();
        next.current_time = element.current_time();
        next.duration = if duration.is_nan() { 0. } else { duration };
        next.buffered_end = read_buffered_end(element);
        next.volume = element.volume();
        next.is_muted = element.muted();
        next.playback_rate = element.playback_rate();
        next.is_fullscreen = self.fullscreen;
        next.can_fullscreen = self.container;
        next.can_set_volume = *self
            .can_set_volume
            .get_or_insert_with(|| element.can_set_volume());
        next.text_tracks = tracks;
        next.active_text_track_id = active;
        next
    }

    /// Upstream `handleMediaEvent`.
    fn handle_media_event(&mut self, event: MediaEventKind, cx: &mut Context<Self>) {
        let Some(media) = self.media.clone() else {
            return;
        };
        let mut next = self.read_all(cx);
        match event {
            MediaEventKind::LoadStart | MediaEventKind::Emptied => {
                next.has_started = false;
                next.is_waiting = false;
                next.error = None;
            }
            MediaEventKind::Play => next.has_started = true,
            MediaEventKind::Waiting => next.is_waiting = true,
            MediaEventKind::Playing
            | MediaEventKind::CanPlay
            | MediaEventKind::Pause
            | MediaEventKind::Seeked
            | MediaEventKind::Ended => next.is_waiting = false,
            MediaEventKind::Error => {
                next.is_waiting = false;
                let error = media.read(cx).error();
                next.error = Some(MediaPlayerError {
                    code: error.as_ref().map_or(0, |e| e.code),
                    message: error.map(|e| e.message).unwrap_or_default(),
                });
            }
            _ => {}
        }
        self.set_state(next, cx);
    }

    /// Upstream `handleTrackChange`.
    fn handle_track_change(&mut self, cx: &mut Context<Self>) {
        let Some(media) = &self.media else { return };
        let (tracks, active) = read_text_tracks(media.read(cx));
        if active.is_some() {
            self.last_text_track_id = active.clone();
        }
        self.patch(cx, |s| {
            s.text_tracks = tracks;
            s.active_text_track_id = active;
        });
    }

    /// Upstream `attachMedia`: reads `element`'s state and follows its events;
    /// `None` resets to the initial state.
    pub fn attach_media(&mut self, element: Option<Entity<MediaElement>>, cx: &mut Context<Self>) {
        if element == self.media {
            return;
        }
        self.media_subscriptions.clear();
        self.media = element.clone();
        let Some(element) = element else {
            let can_fullscreen = self.read_can_fullscreen();
            self.set_state(
                MediaPlayerState {
                    can_fullscreen,
                    ..INITIAL_MEDIA_STATE
                },
                cx,
            );
            return;
        };
        self.media_subscriptions
            .push(cx.subscribe(&element, |store, _, event, cx| match event {
                MediaElementEvent::Media(kind) => store.handle_media_event(*kind, cx),
                MediaElementEvent::TracksChanged => store.handle_track_change(cx),
            }));
        // A new video frame draws the player again.
        self.media_subscriptions
            .push(cx.observe(&element, |_, _, cx| cx.notify()));
        let mut next = self.read_all(cx);
        let (paused, time) = {
            let e = element.read(cx);
            (e.paused(), e.current_time())
        };
        next.has_started = !paused || time > 0.;
        next.error = None;
        self.set_state(next, cx);
    }

    /// Upstream `attachContainer`: whether the player's root is drawn, which decides
    /// whether it can go fullscreen.
    pub fn attach_container(&mut self, attached: bool, cx: &mut Context<Self>) {
        self.container = attached;
        let (can, is) = (self.read_can_fullscreen(), self.fullscreen);
        self.patch(cx, |s| {
            s.can_fullscreen = can;
            s.is_fullscreen = is;
        });
    }

    /// Follows the window leaving fullscreen by other means (upstream's
    /// `fullscreenchange`): the player leaves it too.
    pub(crate) fn sync_fullscreen(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.fullscreen || !self.owns_window_fullscreen {
            return;
        }
        if window.is_fullscreen() {
            self.saw_window_fullscreen = true;
        } else if self.saw_window_fullscreen {
            self.fullscreen = false;
            self.owns_window_fullscreen = false;
            self.saw_window_fullscreen = false;
            self.patch(cx, |s| s.is_fullscreen = false);
        }
    }

    /// Delivers the events the element queued, so an action's events apply at once.
    fn flush(&self, cx: &mut Context<Self>) {
        if let Some(media) = &self.media {
            media.update(cx, |m, cx| m.poll(cx));
        }
    }

    fn with_media<R>(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut MediaElement, &mut Context<MediaElement>) -> R,
    ) -> Option<R> {
        let media = self.media.clone()?;
        let result = media.update(cx, f);
        self.flush(cx);
        Some(result)
    }

    /// Upstream `actions.play`. A refused playback (an autoplay policy) leaves the
    /// state paused, so the play control stays available.
    pub fn play(&mut self, cx: &mut Context<Self>) {
        self.with_media(cx, |m, cx| {
            m.play(cx);
        });
    }

    /// Upstream `actions.pause`.
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        self.with_media(cx, |m, cx| m.pause(cx));
    }

    /// Upstream `actions.togglePaused`: plays paused or ended media, else pauses.
    pub fn toggle_paused(&mut self, cx: &mut Context<Self>) {
        let Some(media) = &self.media else { return };
        let element = media.read(cx);
        if element.paused() || element.ended() {
            self.play(cx);
        } else {
            self.pause(cx);
        }
    }

    /// Upstream `actions.seek`: to `time`, clamped to the media; `NaN` is ignored.
    pub fn seek(&mut self, time: f64, cx: &mut Context<Self>) {
        let Some(media) = self.media.clone() else {
            return;
        };
        if time.is_nan() {
            return;
        }
        let duration = media.read(cx).duration();
        let end = if duration.is_finite() {
            duration
        } else {
            time.max(0.)
        };
        let target = clamp(time, 0., end);
        media.update(cx, |m, cx| m.set_current_time(target, cx));
        let ended = media.read(cx).ended();
        self.patch(cx, |s| {
            s.current_time = target;
            s.is_ended = ended;
        });
        self.flush(cx);
    }

    /// Upstream `actions.seekBy`: `offset` seconds from the playhead.
    pub fn seek_by(&mut self, offset: f64, cx: &mut Context<Self>) {
        if let Some(media) = &self.media {
            let time = media.read(cx).current_time();
            self.seek(time + offset, cx);
        }
    }

    fn read_volume(&mut self, cx: &mut Context<Self>) {
        let Some(media) = &self.media else { return };
        let (volume, muted) = {
            let m = media.read(cx);
            (m.volume(), m.muted())
        };
        self.patch(cx, |s| {
            s.volume = volume;
            s.is_muted = muted;
        });
        self.flush(cx);
    }

    /// Upstream `actions.setVolume`: 0 to 1; a volume above 0 unmutes.
    pub fn set_volume(&mut self, volume: f64, cx: &mut Context<Self>) {
        let Some(media) = self.media.clone() else {
            return;
        };
        let next = clamp(volume, 0., 1.);
        media.update(cx, |m, cx| {
            m.set_volume(next, cx);
            if next > 0. && m.muted() {
                m.set_muted(false, cx);
            }
        });
        self.read_volume(cx);
    }

    /// Upstream `actions.setMuted`: unmuting at volume 0 sets the volume to 1.
    pub fn set_muted(&mut self, muted: bool, cx: &mut Context<Self>) {
        let Some(media) = self.media.clone() else {
            return;
        };
        media.update(cx, |m, cx| {
            m.set_muted(muted, cx);
            if !muted && m.volume() == 0. {
                m.set_volume(1., cx);
            }
        });
        self.read_volume(cx);
    }

    /// Upstream `actions.toggleMuted`.
    pub fn toggle_muted(&mut self, cx: &mut Context<Self>) {
        if let Some(media) = &self.media {
            let muted = media.read(cx).muted();
            self.set_muted(!muted, cx);
        }
    }

    /// Upstream `actions.setPlaybackRate`: rates above 0 only.
    pub fn set_playback_rate(&mut self, rate: f64, cx: &mut Context<Self>) {
        let Some(media) = self.media.clone() else {
            return;
        };
        if rate > 0. {
            media.update(cx, |m, cx| m.set_playback_rate(rate, cx));
            let rate = media.read(cx).playback_rate();
            self.patch(cx, |s| s.playback_rate = rate);
            self.flush(cx);
        }
    }

    /// Upstream `actions.toggleFullscreen`: the player's root fills the window, and
    /// the window goes fullscreen; again, both come back.
    pub fn toggle_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.fullscreen {
            self.fullscreen = false;
            if self.owns_window_fullscreen && window.is_fullscreen() {
                window.toggle_fullscreen();
            }
            self.owns_window_fullscreen = false;
            self.saw_window_fullscreen = false;
        } else if self.container {
            self.fullscreen = true;
            self.owns_window_fullscreen = !window.is_fullscreen();
            self.saw_window_fullscreen = false;
            if self.owns_window_fullscreen {
                window.toggle_fullscreen();
            }
        } else {
            return;
        }
        let is = self.fullscreen;
        self.patch(cx, |s| s.is_fullscreen = is);
    }

    /// Upstream `actions.setTextTrack`: shows the track with `id`, or none.
    pub fn set_text_track(&mut self, id: Option<SharedString>, cx: &mut Context<Self>) {
        let Some(media) = self.media.clone() else {
            return;
        };
        let (tracks, _) = read_text_tracks(media.read(cx));
        let kinds: Vec<TextTrackKind> = media
            .read(cx)
            .text_tracks()
            .iter()
            .map(|t| t.kind)
            .collect();
        let mut position = 0;
        media.update(cx, |m, cx| {
            for (index, kind) in kinds.iter().enumerate() {
                if !matches!(kind, TextTrackKind::Captions | TextTrackKind::Subtitles) {
                    continue;
                }
                let showing = tracks.get(position).map(|t| &t.id) == id.as_ref() && id.is_some();
                m.set_track_mode(
                    index,
                    if showing {
                        TextTrackMode::Showing
                    } else {
                        TextTrackMode::Disabled
                    },
                    cx,
                );
                position += 1;
            }
        });
        if id.is_some() {
            self.last_text_track_id = id.clone();
        }
        self.patch(cx, |s| s.active_text_track_id = id);
    }

    /// Upstream `actions.toggleCaptions`: off when a track shows, else the last
    /// shown track, else the first.
    pub fn toggle_captions(&mut self, cx: &mut Context<Self>) {
        if self.state.active_text_track_id.is_some() {
            self.set_text_track(None, cx);
            return;
        }
        let tracks = &self.state.text_tracks;
        let next = tracks
            .iter()
            .find(|t| Some(&t.id) == self.last_text_track_id.as_ref())
            .or_else(|| tracks.first())
            .map(|t| t.id.clone());
        if next.is_some() {
            self.set_text_track(next, cx);
        }
    }

    /// Upstream `actions.reload`: loads the source again, after an error.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.with_media(cx, |m, cx| m.load(cx));
    }
}
