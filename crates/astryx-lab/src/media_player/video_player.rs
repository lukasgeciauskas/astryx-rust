//! Upstream lab `MediaPlayer/VideoPlayer.tsx`: a ready-made video player whose
//! controls float in a frosted bar over the video.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use astryx_components::button::{Button, ButtonVariant};
use astryx_components::status::{Spinner, SpinnerShade, SpinnerSize};
use astryx_components::text::{Text, TextColor, TextType};
use astryx_components::theming::{MediaThemeMode, media_theme};
use astryx_i18n::current as i18n;
use astryx_primitives::media::{MediaCrossOrigin, MediaKind, MediaPreload, TextTrackKind};
use astryx_primitives::{prefers_reduced_motion, provide};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, FocusHandle, Hsla, ImageSource, IntoElement,
    ParentElement, Pixels, RenderOnce, SharedString, StyleRefinement, Styled, Window, canvas, div,
    prelude::*, px,
};
use web_time::Instant;

use super::context::{PlayerContext, element_key, use_player_context};
use super::controls::{
    CompactControls, MediaCaptionsButton, MediaContainerWidth, MediaFullscreenButton,
    MediaPlayButton, MediaTime, MediaVolume, big_play_focus,
};
use super::element::{CueClearance, MediaPlayerElement, MediaTrack};
use super::player::{Callbacks, MediaPlayer, focus_player};
use super::store::MediaPlayerError;
use super::timeline::{
    ControlsInert, MediaTimeSlider, MediaTimelineChapter, MediaTimelineMarker, PageTheme,
    TimelineTrack,
};

/// Upstream `COMPACT_PLAYER`: a player this narrow, such as a small attachment
/// tile, gets a compact bar.
const COMPACT_PLAYER: f32 = 360.;

/// Upstream `VideoPlayerCaptionTrack`: one caption or subtitle file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VideoPlayerCaptionTrack {
    /// Upstream `src`: a WebVTT file, a local path or a `data:text/vtt,` URL.
    pub src: SharedString,
    /// Upstream `label`: the name shown to people, such as "English".
    pub label: SharedString,
    /// Upstream `language`: a BCP 47 tag, such as "en".
    pub language: SharedString,
    /// Upstream `kind`: captions by default.
    pub kind: TextTrackKind,
    /// Upstream `isDefault`: shows this track when the video loads.
    pub is_default: bool,
}

impl VideoPlayerCaptionTrack {
    /// A captions track.
    pub fn new(
        src: impl Into<SharedString>,
        label: impl Into<SharedString>,
        language: impl Into<SharedString>,
    ) -> Self {
        VideoPlayerCaptionTrack {
            src: src.into(),
            label: label.into(),
            language: language.into(),
            kind: TextTrackKind::Captions,
            is_default: false,
        }
    }

    /// Upstream `kind`.
    pub fn kind(mut self, kind: TextTrackKind) -> Self {
        self.kind = kind;
        self
    }

    /// Upstream `isDefault`.
    pub fn is_default(mut self, is_default: bool) -> Self {
        self.is_default = is_default;
        self
    }
}

/// The player's width as last painted, which the bar's container queries read.
pub(crate) fn width_probe(cell: Rc<Cell<f32>>) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let width = f32::from(bounds.size.width);
            if cell.replace(width) != width {
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// A value that eases to its target over a duration, for the bar's show and hide.
#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    target: f32,
    at: Option<Instant>,
}

impl Tween {
    fn value(&self, now: Instant, duration: Duration, ease: astryx_theme::CubicBezier) -> f32 {
        let Some(at) = self.at else {
            return self.target;
        };
        if duration.is_zero() {
            return self.target;
        }
        let progress =
            (now.saturating_duration_since(at).as_secs_f32() / duration.as_secs_f32()).min(1.);
        self.from + (self.target - self.from) * ease.ease(progress)
    }
}

/// Upstream `VideoStatusLayer`: the surface that a click plays or pauses, and the
/// error, the waiting spinner, or the large play control.
fn status_layer(
    context: &PlayerContext,
    window: &mut Window,
    cx: &mut App,
) -> (AnyElement, AnyElement) {
    let key = element_key(&context.id);
    let state = context.store.read(cx).state().clone();
    let (store, player) = (context.store.clone(), context.player.clone());
    let surface_selector = format!("{key}-surface");
    let surface = div()
        .id(ElementId::Name(format!("{key}-surface").into()))
        .debug_selector(move || surface_selector)
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .occlude()
        .on_click(move |event: &ClickEvent, window, cx| {
            // A tap shows or hides the controls, as native players do; a mouse click
            // plays or pauses, and a double click goes fullscreen.
            if window.last_input_was_touch() {
                player.update(cx, |p, cx| p.toggle_controls(cx));
                return;
            }
            store.update(cx, |s, cx| s.toggle_paused(cx));
            if event.click_count() == 2 {
                store.update(cx, |s, cx| s.toggle_fullscreen(window, cx));
            }
        })
        .into_any_element();
    let center = || {
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
    };
    let t = current(window, cx).tokens().clone();
    let status: AnyElement = if state.error.is_some() {
        let (store, player) = (context.store.clone(), context.player.clone());
        let i18n = i18n(cx);
        center()
            .gap(t.spacing.s3)
            .child(
                Text::new(i18n.t("@astryx.mediaPlayer.error"))
                    .text_type(TextType::Body)
                    .color(TextColor::Primary),
            )
            .child(
                div().debug_selector(move || format!("{key}-retry")).child(
                    Button::new(context.part("retry"), i18n.t("@astryx.mediaPlayer.retry"))
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| {
                            // The message and this button leave once the media reloads.
                            focus_player(&player, window, cx);
                            store.update(cx, |s, cx| s.reload(cx));
                        }),
                ),
            )
            .into_any_element()
    } else if state.is_waiting && state.has_started {
        let selector = format!("{key}-spinner");
        center()
            .child(
                div().debug_selector(move || selector).child(
                    Spinner::new(context.part("spinner"))
                        .size(SpinnerSize::Lg)
                        .shade(SpinnerShade::OnMedia)
                        .decorative(),
                ),
            )
            .into_any_element()
    } else if !state.has_started || state.is_ended {
        let frosted = frosted(&t);
        center()
            .child(
                div().p(t.spacing.s2).rounded_full().bg(frosted).child(
                    // The bar's play control does the same job, so this one stays out
                    // of the tab order.
                    MediaPlayButton::new()
                        .id(context.part("big-play"))
                        .size(astryx_primitives::ElementSize::Lg)
                        .out_of_tab_order()
                        .on_click(big_play_focus(context.player.clone())),
                ),
            )
            .into_any_element()
    } else {
        div().into_any_element()
    };
    (
        surface,
        media_theme(MediaThemeMode::Dark, status).into_any_element(),
    )
}

/// Upstream `FROSTED_SURFACE`: the surface color at 80%. GPUI draws no backdrop
/// blur, so the frosted surface is the tint alone.
fn frosted(t: &astryx_theme::Tokens) -> Hsla {
    let c = t.color.background_surface;
    Hsla { a: c.a * 0.8, ..c }
}

/// Upstream `VideoControlBar`: the frosted bar of controls inset over the video.
/// Returns it and whether it shows.
#[allow(clippy::too_many_arguments)]
fn control_bar(
    context: &PlayerContext,
    chapters: Vec<MediaTimelineChapter>,
    markers: Vec<MediaTimelineMarker>,
    compact_bar: bool,
    narrow: bool,
    bar_focus: &FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> (AnyElement, bool) {
    let theme = current(window, cx);
    let page = theme.clone();
    let key = element_key(&context.id);
    let (visible, layer_open) = {
        let p = context.player.read(cx);
        (p.is_controls_visible(), p.is_layer_open())
    };
    let state = context.store.read(cx).state();
    // After the end the bar stays beside the replay control.
    let before_playback = state.error.is_none() && !state.has_started;
    // Before playback the large play control takes the bar's place; keyboard focus
    // inside the bar, or a menu or popover it opened, still shows it.
    let focus_within = bar_focus.contains_focused(window, cx);
    let shown = visible && (!before_playback || layer_open || focus_within);
    #[cfg(test)]
    super::tests::record_bar(&key, visible, shown);

    let now = cx.background_executor().now();
    let tween = window.use_keyed_state(
        ElementId::Name(format!("{key}-bar-tween").into()),
        cx,
        move |_, _| {
            Rc::new(Cell::new(Tween {
                from: if shown { 1. } else { 0. },
                target: if shown { 1. } else { 0. },
                at: None,
            }))
        },
    );
    let tween = tween.read(cx).clone();
    let t = theme.tokens().clone();
    let duration = if prefers_reduced_motion(cx) {
        Duration::ZERO
    } else {
        t.duration.medium
    };
    let mut tw = tween.get();
    let goal = if shown { 1. } else { 0. };
    if goal != tw.target {
        tw.from = tw.value(now, duration, t.ease.standard);
        tw.target = goal;
        tw.at = Some(now);
        tween.set(tw);
    }
    let shown_amount = tw.value(now, duration, t.ease.standard);
    if shown_amount != tw.target {
        window.request_animation_frame();
    }
    let rem = window.rem_size();
    let inset = if compact_bar {
        t.spacing.s1
    } else {
        t.spacing.s2
    }
    .to_pixels(rem);
    let gap = if compact_bar {
        t.spacing.s1
    } else {
        t.spacing.s2
    }
    .to_pixels(rem);
    let padding = if compact_bar {
        t.spacing.s1.to_pixels(rem) / 2.
    } else {
        t.spacing.s1.to_pixels(rem)
    };
    let frosted_track = Hsla {
        a: t.color.accent.a * 0.24,
        ..t.color.accent
    };
    let selector = format!("{key}-bar");
    let mut bar = div()
        .id(ElementId::Name(format!("{key}-bar").into()))
        .debug_selector(move || selector)
        .track_focus(bar_focus)
        .absolute()
        .left(inset)
        .right(inset)
        .bottom(inset - inset * (1. - shown_amount))
        .flex()
        .items_center()
        .gap(gap)
        .p(padding)
        .rounded(t.radius.container)
        .bg(frosted(&t))
        .text_color(t.color.text_primary)
        .opacity(shown_amount);
    if shown {
        // The bar takes its own presses, so a press on it never reaches the surface.
        bar = bar.occlude();
    }
    let compact_wrap = |el: AnyElement| -> AnyElement {
        if compact_bar {
            provide(CompactControls, el).into_any_element()
        } else {
            el
        }
    };
    let mut row: Vec<AnyElement> = vec![
        compact_wrap(MediaPlayButton::new().into_any_element()),
        provide(
            PageTheme(page),
            MediaTimeSlider::new().chapters(chapters).markers(markers),
        )
        .into_any_element(),
    ];
    if !compact_bar {
        row.push(MediaTime::new().into_any_element());
    }
    if !narrow {
        row.push(MediaVolume::new().into_any_element());
    }
    row.push(compact_wrap(MediaCaptionsButton::new().into_any_element()));
    row.push(compact_wrap(
        MediaFullscreenButton::new().into_any_element(),
    ));
    let bar = bar.children(row);
    let bar = provide(
        TimelineTrack(frosted_track),
        provide(ControlsInert(!shown), bar),
    );
    (
        media_theme(MediaThemeMode::Dark, bar).into_any_element(),
        shown,
    )
}

type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream lab `VideoPlayer`: a ready-made video player. Its controls float in a
/// frosted bar inset over the video; the bar shows on hover, pointer movement and
/// keyboard focus, and hides after two idle seconds while the video plays. On touch
/// a tap shows the bar and the next tap hides it.
///
/// ```ignore
/// VideoPlayer::new("tour", "media/product-tour.mp4", "Product tour").poster("media/product-tour.jpg")
/// ```
#[derive(IntoElement)]
pub struct VideoPlayer {
    id: ElementId,
    src: SharedString,
    label: SharedString,
    poster: Option<ImageSource>,
    captions: Vec<VideoPlayerCaptionTrack>,
    aspect_ratio: f32,
    has_auto_play: bool,
    is_looping: bool,
    preload: MediaPreload,
    cross_origin: Option<MediaCrossOrigin>,
    chapters: Vec<MediaTimelineChapter>,
    markers: Vec<MediaTimelineMarker>,
    callbacks: Callbacks,
    style: StyleRefinement,
}

impl VideoPlayer {
    /// Upstream `src` and `label` (the player's accessible name, such as the
    /// video's title).
    pub fn new(
        id: impl Into<ElementId>,
        src: impl Into<SharedString>,
        label: impl Into<SharedString>,
    ) -> Self {
        VideoPlayer {
            id: id.into(),
            src: src.into(),
            label: label.into(),
            poster: None,
            captions: Vec::new(),
            aspect_ratio: 16. / 9.,
            has_auto_play: false,
            is_looping: false,
            preload: MediaPreload::Metadata,
            cross_origin: None,
            chapters: Vec::new(),
            markers: Vec::new(),
            callbacks: Callbacks::default(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `crossOrigin`: the CORS mode for the video and its caption files in
    /// the web build; native engines ignore it.
    pub fn cross_origin(mut self, cross_origin: MediaCrossOrigin) -> Self {
        self.cross_origin = Some(cross_origin);
        self
    }

    /// Upstream `poster`: the image shown before playback starts.
    pub fn poster(mut self, poster: impl Into<ImageSource>) -> Self {
        self.poster = Some(poster.into());
        self
    }

    /// Upstream `captions`: caption and subtitle files. The captions control appears
    /// when there is one.
    pub fn captions(mut self, captions: impl IntoIterator<Item = VideoPlayerCaptionTrack>) -> Self {
        self.captions = captions.into_iter().collect();
        self
    }

    /// Upstream `aspectRatio`: width divided by height, 16 / 9 by default.
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.aspect_ratio = ratio;
        self
    }

    /// Upstream `hasAutoPlay`: starts playback when the video loads, muted, as
    /// browsers block autoplay with sound. The play control stays available if the
    /// platform still refuses.
    pub fn has_auto_play(mut self, auto_play: bool) -> Self {
        self.has_auto_play = auto_play;
        self
    }

    /// Upstream `isLooping`: starts again from the beginning at the end.
    pub fn is_looping(mut self, looping: bool) -> Self {
        self.is_looping = looping;
        self
    }

    /// Upstream `preload`: `metadata` by default.
    pub fn preload(mut self, preload: MediaPreload) -> Self {
        self.preload = preload;
        self
    }

    /// Upstream `chapters`: segments of the timeline, named in its preview.
    pub fn chapters(mut self, chapters: impl IntoIterator<Item = MediaTimelineChapter>) -> Self {
        self.chapters = chapters.into_iter().collect();
        self
    }

    /// Upstream `markers`: points of interest, such as timestamped comments.
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

impl Styled for VideoPlayer {
    /// Upstream `xstyle`: the player's root.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The parts inside the VideoPlayer's MediaPlayer, which read its context.
#[derive(IntoElement)]
struct VideoParts {
    poster: Option<ImageSource>,
    src: SharedString,
    preload: MediaPreload,
    cross_origin: Option<MediaCrossOrigin>,
    has_auto_play: bool,
    is_looping: bool,
    captions: Vec<VideoPlayerCaptionTrack>,
    chapters: Vec<MediaTimelineChapter>,
    markers: Vec<MediaTimelineMarker>,
    width: Rc<Cell<f32>>,
}

impl RenderOnce for VideoParts {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let context = use_player_context("VideoPlayer", cx);
        let key = element_key(&context.id);
        let t = current(window, cx).tokens().clone();
        let width = self.width.get();
        let compact_bar = width > 0. && width <= COMPACT_PLAYER;
        let narrow = width > 0. && width <= 480.;
        let bar_focus = window
            .use_keyed_state(
                ElementId::Name(format!("{key}-bar-focus").into()),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        let rem = window.rem_size();
        // The cues sit above the bar, shown or hidden, so they never jump and the bar
        // never covers them: the bar's height and twice its inset.
        let inset = if compact_bar {
            t.spacing.s1
        } else {
            t.spacing.s2
        }
        .to_pixels(rem);
        let padding = if compact_bar {
            t.spacing.s1.to_pixels(rem) / 2.
        } else {
            t.spacing.s1.to_pixels(rem)
        };
        let control = if compact_bar {
            t.size.element_sm
        } else {
            t.size.element_md
        }
        .to_pixels(rem);
        let clearance: Pixels = control + padding * 2. + inset * 2.;
        let mut video = MediaPlayerElement::new(MediaKind::Video)
            .src(self.src)
            .preload(self.preload)
            .auto_play(self.has_auto_play)
            .muted(self.has_auto_play)
            .looping(self.is_looping);
        if let Some(poster) = self.poster {
            video = video.poster(poster);
        }
        if let Some(cross_origin) = self.cross_origin {
            video = video.cross_origin(cross_origin);
        }
        for track in self.captions {
            video = video.track(MediaTrack {
                kind: track.kind,
                src: track.src,
                label: track.label,
                language: track.language,
                is_default: track.is_default,
            });
        }
        let (surface, status) = status_layer(&context, window, cx);
        let (bar, shown) = control_bar(
            &context,
            self.chapters,
            self.markers,
            compact_bar,
            narrow,
            &bar_focus,
            window,
            cx,
        );
        let media = provide(CueClearance(clearance), video).into_any_element();
        let probe = width_probe(self.width).into_any_element();
        // A hidden bar lies under the surface, which takes the presses meant for the
        // video (upstream's `pointer-events: none`).
        let children: Vec<AnyElement> = if shown {
            vec![probe, media, surface, status, bar]
        } else {
            vec![probe, media, bar, surface, status]
        };
        provide(
            MediaContainerWidth(px(width)),
            div().size_full().children(children),
        )
    }
}

impl RenderOnce for VideoPlayer {
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
        let mut player = MediaPlayer::new(self.id, self.label)
            .callbacks(self.callbacks)
            .w_full()
            .overflow_hidden()
            .rounded(t.radius.element)
            .bg(t.color.background_inverted)
            .aspect_ratio(self.aspect_ratio);
        player.style().refine(&self.style);
        player.child(VideoParts {
            poster: self.poster,
            src: self.src,
            preload: self.preload,
            cross_origin: self.cross_origin,
            has_auto_play: self.has_auto_play,
            is_looping: self.is_looping,
            captions: self.captions,
            chapters: self.chapters,
            markers: self.markers,
            width,
        })
    }
}
