//! Upstream lab `MediaPlayer/MediaPlayerContext.ts`: what a MediaPlayer gives the
//! parts inside it, the hooks that read it, and the time formats.

use gpui::{App, ElementId, Entity, SharedString, Window};
use icu_experimental::dimension::units::categorized_formatter::CategorizedFormatter;
use icu_experimental::dimension::units::options::Width;
use icu_experimental::measure::category::Duration as DurationCategory;
use icu_list::ListFormatter;
use icu_list::options::{ListFormatterOptions, ListLength};
use rustryx_primitives::{prefers_reduced_motion, use_context};

use super::player::PlayerState;
use super::store::{MediaPlayerState, MediaStore};

/// Upstream `MediaControlsHold`: what keeps the controls shown while media plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MediaControlsHold {
    /// Keyboard focus inside the player.
    Focus,
    /// A drag on the timeline.
    Scrub,
    /// An open menu or marker popover.
    Menu,
}

/// Upstream `DEFAULT_PLAYBACK_RATES`: the speeds MediaPlaybackRate offers and the
/// `<` and `>` shortcuts step through.
pub const DEFAULT_PLAYBACK_RATES: [f64; 7] = [0.5, 0.75, 1., 1.25, 1.5, 1.75, 2.];

/// What a MediaPlayer provides its parts (upstream `MediaPlayerContextValue`).
#[derive(Clone)]
pub(crate) struct PlayerContext {
    pub id: ElementId,
    pub player: Entity<PlayerState>,
    pub store: Entity<MediaStore>,
}

impl PlayerContext {
    /// An id for a part of this player.
    pub fn part(&self, name: &str) -> ElementId {
        ElementId::Name(format!("{}-{name}", element_key(&self.id)).into())
    }
}

/// An element id as text, for the ids and selectors of the parts.
pub(crate) fn element_key(id: &ElementId) -> String {
    match id {
        ElementId::Name(name) => name.to_string(),
        other => format!("{other:?}"),
    }
}

/// Upstream `useMediaPlayerContext`: the nearest player's context. Panics outside
/// a MediaPlayer, as upstream throws.
pub(crate) fn use_player_context(part: &str, cx: &App) -> PlayerContext {
    use_context::<PlayerContext>(cx)
        .unwrap_or_else(|| panic!("{part} must be rendered inside a MediaPlayer."))
}

/// The actions that drive the nearest MediaPlayer (upstream `MediaPlayerActions`).
#[derive(Clone)]
pub struct MediaPlayerActions(pub(crate) Entity<MediaStore>);

impl MediaPlayerActions {
    /// Upstream `play`.
    pub fn play(&self, cx: &mut App) {
        self.0.update(cx, |s, cx| s.play(cx));
    }
    /// Upstream `pause`.
    pub fn pause(&self, cx: &mut App) {
        self.0.update(cx, |s, cx| s.pause(cx));
    }
    /// Upstream `togglePaused`.
    pub fn toggle_paused(&self, cx: &mut App) {
        self.0.update(cx, |s, cx| s.toggle_paused(cx));
    }
    /// Upstream `seek`: to `time` seconds.
    pub fn seek(&self, time: f64, cx: &mut App) {
        self.0.update(cx, |s, cx| s.seek(time, cx));
    }
    /// Upstream `seekBy`: `offset` seconds from the playhead.
    pub fn seek_by(&self, offset: f64, cx: &mut App) {
        self.0.update(cx, |s, cx| s.seek_by(offset, cx));
    }
    /// Upstream `setVolume`: 0 to 1.
    pub fn set_volume(&self, volume: f64, cx: &mut App) {
        self.0.update(cx, |s, cx| s.set_volume(volume, cx));
    }
    /// Upstream `setMuted`.
    pub fn set_muted(&self, muted: bool, cx: &mut App) {
        self.0.update(cx, |s, cx| s.set_muted(muted, cx));
    }
    /// Upstream `toggleMuted`.
    pub fn toggle_muted(&self, cx: &mut App) {
        self.0.update(cx, |s, cx| s.toggle_muted(cx));
    }
    /// Upstream `setPlaybackRate`.
    pub fn set_playback_rate(&self, rate: f64, cx: &mut App) {
        self.0.update(cx, |s, cx| s.set_playback_rate(rate, cx));
    }
    /// Upstream `toggleFullscreen`.
    pub fn toggle_fullscreen(&self, window: &mut Window, cx: &mut App) {
        self.0.update(cx, |s, cx| s.toggle_fullscreen(window, cx));
    }
    /// Upstream `setTextTrack`: the id of the track to show, or none.
    pub fn set_text_track(&self, id: Option<SharedString>, cx: &mut App) {
        self.0.update(cx, |s, cx| s.set_text_track(id, cx));
    }
    /// Upstream `toggleCaptions`.
    pub fn toggle_captions(&self, cx: &mut App) {
        self.0.update(cx, |s, cx| s.toggle_captions(cx));
    }
    /// Upstream `reload`.
    pub fn reload(&self, cx: &mut App) {
        self.0.update(cx, |s, cx| s.reload(cx));
    }
}

/// Upstream `UseMediaPlayerResult`: the nearest player's state and its actions.
#[derive(Clone)]
pub struct UseMediaPlayerResult {
    pub state: MediaPlayerState,
    pub actions: MediaPlayerActions,
}

/// Upstream `useMediaPlayer`: reads the nearest MediaPlayer's playback state and
/// returns the actions that drive it, for custom controls inside a MediaPlayer. Call
/// it while rendering; the view draws again when the state changes. Panics outside
/// a MediaPlayer.
///
/// ```ignore
/// let player = use_media_player(cx);
/// let percent = if player.state.duration > 0. { player.state.current_time / player.state.duration } else { 0. };
/// ```
pub fn use_media_player(cx: &App) -> UseMediaPlayerResult {
    let context = use_player_context("useMediaPlayer", cx);
    UseMediaPlayerResult {
        state: context.store.read(cx).state().clone(),
        actions: MediaPlayerActions(context.store),
    }
}

/// Upstream `useMediaPlaybackTime`: the playback position, advanced on every
/// frame while the media plays, so progress moves smoothly between the time
/// updates. Each update, seek, pause and stall snaps it back to the reported time.
/// Under reduced motion it follows the time updates only. Panics outside a
/// MediaPlayer.
pub fn use_media_playback_time(window: &mut Window, cx: &App) -> f64 {
    let context = use_player_context("useMediaPlaybackTime", cx);
    playback_time(&context.store, window, cx)
}

pub(crate) fn playback_time(store: &Entity<MediaStore>, window: &mut Window, cx: &App) -> f64 {
    let store = store.read(cx);
    let state = store.state();
    let advancing = !state.is_paused && !state.is_ended && !state.is_waiting;
    if !advancing || prefers_reduced_motion(cx) {
        return state.current_time;
    }
    window.request_animation_frame();
    let end = if state.duration.is_finite() && state.duration > 0. {
        state.duration
    } else {
        f64::INFINITY
    };
    let elapsed = cx
        .background_executor()
        .now()
        .saturating_duration_since(store.reported_at())
        .as_secs_f64();
    (state.current_time + elapsed * state.playback_rate).min(end)
}

fn pad(value: u64) -> String {
    format!("{value:02}")
}

/// Whole seconds as upstream counts them: non-finite is 0, negative is 0, floored.
fn whole_seconds(seconds: f64) -> u64 {
    if seconds.is_finite() {
        seconds.max(0.).floor() as u64
    } else {
        0
    }
}

/// Upstream `formatClock`: seconds as `m:ss`, or `h:mm:ss` with `has_hours` or
/// past an hour.
pub fn format_clock(seconds: f64, has_hours: bool) -> String {
    let total = whole_seconds(seconds);
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let rest = total % 60;
    if has_hours || hours > 0 {
        format!("{hours}:{}:{}", pad(minutes), pad(rest))
    } else {
        format!("{minutes}:{}", pad(rest))
    }
}

/// Upstream `formatSpokenTime`: seconds as words for assistive technology, such as
/// "1 minute, 5 seconds" (`Intl.DurationFormat`, long style). Zero shows its
/// seconds. Falls back to the clock form where the locale has no data.
pub fn format_spoken_time(seconds: f64, locale: &str) -> String {
    use writeable::Writeable as _;
    let total = whole_seconds(seconds);
    let locale: icu_locale::Locale = locale.parse().unwrap_or(icu_locale::Locale::UNKNOWN);
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let rest = total % 60;
    // DurationFormat leaves out zero units, so zero needs its seconds shown.
    let units: Vec<_> = if total == 0 {
        vec![(DurationCategory::second(), 0u64)]
    } else {
        [
            (DurationCategory::hour(), hours),
            (DurationCategory::minute(), minutes),
            (DurationCategory::second(), rest),
        ]
        .into_iter()
        .filter(|(_, value)| *value > 0)
        .collect::<Vec<_>>()
    };
    let mut texts = Vec::with_capacity(units.len());
    for (category, value) in units {
        let Ok(formatter) =
            CategorizedFormatter::try_new_core((&locale).into(), category, Width::Long.into())
        else {
            return format_clock(total as f64, false);
        };
        let formatter: CategorizedFormatter<DurationCategory> = formatter;
        texts.push(
            formatter
                .format_fixed_decimal(&icu_decimal::input::Decimal::from(value))
                .write_to_string()
                .into_owned(),
        );
    }
    match ListFormatter::try_new_unit(
        (&locale).into(),
        ListFormatterOptions::default().with_length(ListLength::Wide),
    ) {
        Ok(list) => list
            .format(texts.iter().map(String::as_str))
            .write_to_string()
            .into_owned(),
        Err(_) => format_clock(total as f64, false),
    }
}
