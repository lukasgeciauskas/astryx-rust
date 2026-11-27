//! Upstream `Lightbox`: a full-window viewer for photos and videos, one at
//! a time, with gallery paging and zoom.
//!
//! - The viewer is a modal layer: a focus region (Tab wraps, FR4), Escape
//!   and a press beside the photo close it, focus returns to the opener. It
//!   draws on the dark media surface (FR19), the backdrop in the overlay
//!   color.
//! - Controls: Close, and in a gallery Previous, Next and a counter; a
//!   caption band two lines tall under every page when any has a caption,
//!   with "more" opening a longer one over the photo (FR1).
//! - Paging: Previous and Next, the arrow keys (mirrored right to left), and
//!   a horizontal drag of the row of pages, which sit apart by 40px (FR7).
//! - Dismissal: a drag down on a fitted photo shrinks it and fades the
//!   backdrop; a release past the commit point closes the viewer (FR8).
//! - Zoom (`has_zoom`): the photo is a toggle button (Enter or Space zooms
//!   it), `+`/`-`/`0` zoom in, out and back, the arrows pan a zoomed photo,
//!   a double click zooms 2× about the pointer, Ctrl+wheel or a trackpad
//!   pinch zooms about the pointer, a drag or a trackpad's scroll pans with
//!   rubber-band edges (FR6, FR11).
//! - The index is clamped to the media (FR17).
//!
//! - Video (upstream `<video controls>`): a video item plays in place through
//!   `astryx_primitives::media`, with the platform-style controls over the bottom of
//!   the picture. A video left on another page, or in a closed viewer, pauses; with
//!   `has_auto_play` the video shown plays, also when paged back to or reopened.
//!   A paged-away video keeps its element and time.

use std::cell::Cell;
use std::f64::consts::E;
use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{Arg, current as i18n, use_direction};
use astryx_primitives::media::{
    MediaElement, MediaKind, MediaPreload, MediaSource, new_media_backend,
};
use astryx_primitives::motion::paging::{
    paging_settle_velocity, solve_animated_page_scroll, solve_paging,
};
use astryx_primitives::motion::uikit::springs::role::SETTLE as SETTLE_SPRING;
use astryx_primitives::motion::uikit::zoom_media::role::{
    DISMISS_POSITION_SPRING, DISMISS_SIZE_SPRING,
};
use astryx_primitives::motion::{FnCurve, MotionSpring, SharedCurve, resolve_spring, solve_spring};
use astryx_primitives::{Politeness, announce, is_focus_visible, prefers_reduced_motion};
use astryx_theme::{Appearance, Pointer, ThemeStyled, current, scope};
use gpui::{
    AnyElement, App, Bounds, DispatchPhase, ElementId, Entity, FocusHandle, ImageSource,
    ImgResourceLoader, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ObjectFit, ParentElement, Pixels, Point, RenderOnce, Role, ScrollDelta,
    ScrollEvent, SharedString, StyleRefinement, Styled, Window, div, img, prelude::*, px,
};
use web_time::Instant;

use super::dialog::DialogPurpose;
use super::lightbox_video::LightboxVideo;
use super::modal::{Phase, backdrop, faded, modal_layer, modal_state, phase, probes, sync_open};
use super::zoom_stage::{
    IDENTITY, PAGE_GAP, Rect, StagePoint, ZoomGeometry, ZoomTransform, dismiss_backdrop_alpha,
    dismiss_chrome_alpha, dismiss_commits, dismiss_transform, double_click_target,
    double_tap_scale_for, dragged_transform, is_zoomed, max_scale_for, overshoot_scale, panned_by,
    pinch_scale_for, raw_for_dragged, release_target, released_zoom_target, resting_transform,
    transform_for_resize, transform_keeping_point,
};
use crate::button::{ButtonVariant, IconButton};
use crate::focus::focus_ring_inset;
use crate::icon::{Icon, IconSize};
use crate::link::Link;
use crate::text::{Text, TextType};

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type IndexFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;
type Action = Rc<dyn Fn(&mut Window, &mut App)>;
/// Pages to an item, the row starting `offset` px from it; a drag's release gives its
/// velocity (px/s).
type PageFn = Rc<dyn Fn(usize, f64, Option<f64>, &mut Window, &mut App)>;
/// A photo's natural size and the device pixel ratio.
type Natural = Option<((f64, f64), f64)>;

/// Upstream `KEYBOARD_PAN_STEP`.
const KEYBOARD_PAN_STEP: f64 = 50.;
/// How far a press moves before it is a drag.
const DRAG_SLOP: f64 = 4.;
/// A touch close's exit: the viewer shrinks a little and fades.
const TOUCH_EXIT: Duration = Duration::from_millis(280);
/// A wheel pinch ends when the wheel has been still this long.
const WHEEL_PINCH_END: Duration = Duration::from_millis(160);

/// Upstream `LightboxMediaType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightboxMediaKind {
    #[default]
    Image,
    Video,
}

/// One item, upstream `LightboxMedia`.
#[derive(Clone)]
pub struct LightboxMedia {
    src: SharedString,
    source: ImageSource,
    alt: SharedString,
    caption: Option<SharedString>,
    kind: LightboxMediaKind,
}

impl LightboxMedia {
    /// A photo, upstream `{src, alt}`.
    pub fn image(src: impl Into<SharedString>, alt: impl Into<SharedString>) -> Self {
        let src = src.into();
        LightboxMedia {
            source: ImageSource::from(src.clone()),
            src,
            alt: alt.into(),
            caption: None,
            kind: LightboxMediaKind::Image,
        }
    }

    /// A photo from any image source (one decoded in memory, say).
    pub fn from_source(source: impl Into<ImageSource>, alt: impl Into<SharedString>) -> Self {
        LightboxMedia {
            src: SharedString::default(),
            source: source.into(),
            alt: alt.into(),
            caption: None,
            kind: LightboxMediaKind::Image,
        }
    }

    /// A video, upstream `{src, alt, type: 'video'}`.
    pub fn video(src: impl Into<SharedString>, alt: impl Into<SharedString>) -> Self {
        let src = src.into();
        LightboxMedia {
            source: ImageSource::from(src.clone()),
            src,
            alt: alt.into(),
            caption: None,
            kind: LightboxMediaKind::Video,
        }
    }

    /// Upstream `caption`.
    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }
}

/// Where the photo, the row of pages and a drag to dismiss are.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Snapshot {
    zoom: ZoomTransform,
    /// The row's offset from the current page, px.
    page: f64,
    /// A drag to dismiss: the photo's offset.
    dismiss: (f64, f64),
    /// A dismissal's fade of the backdrop and the controls: 0 shown, 1 gone.
    leave: f64,
}

const REST: Snapshot = Snapshot {
    zoom: IDENTITY,
    page: 0.,
    dismiss: (0., 0.),
    leave: 0.,
};

/// The snapshot's channels, each settling on its own curve: the zoom's x, y and scale,
/// the row, the dismissal's x and y, and its fade.
const CHANNELS: usize = 7;
const ZOOM_X: usize = 0;
const ZOOM_Y: usize = 1;
const ZOOM_SCALE: usize = 2;
const PAGE: usize = 3;
const DISMISS_X: usize = 4;
const DISMISS_Y: usize = 5;
const LEAVE: usize = 6;

impl Snapshot {
    fn channels(&self) -> [f64; CHANNELS] {
        [
            self.zoom.x,
            self.zoom.y,
            self.zoom.scale,
            self.page,
            self.dismiss.0,
            self.dismiss.1,
            self.leave,
        ]
    }

    fn from_channels(v: [f64; CHANNELS]) -> Self {
        Snapshot {
            zoom: ZoomTransform {
                x: v[ZOOM_X],
                y: v[ZOOM_Y],
                scale: v[ZOOM_SCALE],
            },
            page: v[PAGE],
            dismiss: (v[DISMISS_X], v[DISMISS_Y]),
            leave: v[LEAVE],
        }
    }
}

/// Within this of its target a channel is at rest (upstream `SETTLE_THRESHOLD`: px, scale,
/// opacity).
fn settle_threshold(channel: usize) -> f64 {
    match channel {
        ZOOM_SCALE => 0.0001,
        LEAVE => 0.001,
        _ => 0.05,
    }
}

/// `spring` (a motion role, at the theme's `tempo`) from `from` to `to`, starting at
/// `velocity` (units/s), as a curve that ends exactly on `to` once it settles.
fn spring_curve(
    spring: MotionSpring,
    tempo: f64,
    from: f64,
    to: f64,
    velocity: f64,
    epsilon: f64,
) -> SharedCurve {
    let curve = solve_spring(
        resolve_spring(spring, tempo).to_spring(),
        from,
        to,
        velocity,
    );
    let duration = curve.settle_time(epsilon);
    FnCurve::new(
        duration,
        move |t| if t >= duration { to } else { curve.value(t) },
        move |t| if t >= duration { 0. } else { curve.velocity(t) },
    )
    .shared()
}

/// One channel settling: its curve, from when it started.
struct Channel {
    curve: SharedCurve,
    start: Instant,
}

/// The channels settling toward `to`.
struct Motion {
    channels: [Option<Channel>; CHANNELS],
    to: Snapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragKind {
    Undecided,
    Pan,
    Page,
    Dismiss,
}

/// A press on the photo, and the drag it became.
struct Drag {
    kind: DragKind,
    origin: Point<Pixels>,
    last: Point<Pixels>,
    last_at: Instant,
    /// px/s.
    velocity: (f64, f64),
    from: Snapshot,
}

/// The viewer's state between frames.
struct LightboxState {
    index: usize,
    shown: Snapshot,
    motion: Option<Motion>,
    drag: Option<Drag>,
    /// The current page's media area, as of last frame.
    stage: Option<Bounds<Pixels>>,
    controls_hidden: bool,
    caption_open: bool,
    /// The index last announced, and whether the viewer was open then.
    announced: Option<usize>,
    was_zoomed: bool,
    photo: FocusHandle,
    /// A Ctrl+wheel pinch: its centroid and when it last moved.
    wheel: Option<(StagePoint, Instant)>,
    /// A released drag to dismiss: the backdrop's and the controls' opacity at the lift,
    /// and the photo's travel then. The viewer closes once the photo has left.
    dismissing: Option<Dismissal>,
    _wheel_end: Option<gpui::Task<()>>,
    /// The theme's motion tempo, which every settle and the pager play at
    /// (`ResolvedTheme::motion_tempo`), as of the last frame.
    tempo: f64,
    /// The video items' media elements, by source, kept while the viewer lives so a
    /// paged-away video keeps its time.
    videos: Vec<(SharedString, Entity<MediaElement>)>,
    /// The open state, index and autoplay the videos were last synced to.
    video_sync: Option<(bool, usize, bool)>,
    /// The current video's picture, in window coordinates, as last painted.
    video_picture: Rc<Cell<Option<Bounds<Pixels>>>>,
}

/// A released drag to dismiss, flying out.
#[derive(Clone, Copy, Debug)]
struct Dismissal {
    backdrop: f64,
    chrome: f64,
    /// The photo has left and the viewer asked to close.
    closed: bool,
    /// The travel at the lift, which sets the photo's size while it leaves.
    travel: f64,
}

impl LightboxState {
    /// The snapshot now, finishing the channels that have settled.
    fn current(&mut self, now: Instant) -> (Snapshot, bool) {
        let Some(m) = &mut self.motion else {
            return (self.shown, false);
        };
        let mut values = self.shown.channels();
        let targets = m.to.channels();
        let mut moving = false;
        for (i, slot) in m.channels.iter_mut().enumerate() {
            let Some(channel) = slot else { continue };
            let t = now.saturating_duration_since(channel.start).as_secs_f64();
            if t >= channel.curve.duration() {
                values[i] = targets[i];
                *slot = None;
            } else {
                values[i] = channel.curve.at(t);
                moving = true;
            }
        }
        if moving {
            self.shown = Snapshot::from_channels(values);
        } else {
            self.shown = m.to;
            self.motion = None;
        }
        (self.shown, moving)
    }

    /// A channel's velocity on screen now, units/s.
    fn velocity(&self, channel: usize, now: Instant) -> f64 {
        self.motion
            .as_ref()
            .and_then(|m| m.channels[channel].as_ref())
            .map_or(0., |c| {
                c.curve
                    .velocity(now.saturating_duration_since(c.start).as_secs_f64())
            })
    }

    /// Settles every channel to `to` from what it shows, each on the curve `curve` gives
    /// for (channel, from, to, velocity on screen); at once under reduced motion.
    fn settle_with(
        &mut self,
        to: Snapshot,
        now: Instant,
        reduced: bool,
        curve: impl Fn(usize, f64, f64, f64) -> SharedCurve,
    ) {
        self.current(now);
        if reduced {
            self.shown = to;
            self.motion = None;
            return;
        }
        let from = self.shown.channels();
        let targets = to.channels();
        let channels = std::array::from_fn(|i| {
            let velocity = self.velocity(i, now);
            ((from[i] - targets[i]).abs() > 1e-9 || velocity != 0.).then(|| Channel {
                curve: curve(i, from[i], targets[i], velocity),
                start: now,
            })
        });
        self.motion = Some(Motion { channels, to });
    }

    /// Settles to `to` on `springs.settle`, every zoom's spring (upstream
    /// `useZoomCore.settleTo`): from what shows, at the speed it has, or with a release's
    /// velocity where `velocity` gives one.
    fn settle_from(
        &mut self,
        to: Snapshot,
        now: Instant,
        reduced: bool,
        velocity: [Option<f64>; CHANNELS],
    ) {
        let tempo = self.tempo;
        self.settle_with(to, now, reduced, |i, from, to, v| {
            spring_curve(
                SETTLE_SPRING,
                tempo,
                from,
                to,
                velocity[i].unwrap_or(v),
                settle_threshold(i),
            )
        });
    }

    /// Settles to `to` on `springs.settle` from what shows.
    fn settle(&mut self, to: Snapshot, now: Instant, reduced: bool) {
        self.settle_from(to, now, reduced, [None; CHANNELS]);
    }

    /// Turns the row to rest: a drag's release on UIKit's paging settle from its velocity,
    /// a button's or a key's on `setContentOffset(_:animated:)` (upstream `usePager`).
    fn settle_page(&mut self, now: Instant, reduced: bool, release: Option<f64>) {
        let to = Snapshot {
            page: 0.,
            ..self.target()
        };
        let tempo = self.tempo;
        self.settle_with(to, now, reduced, |i, from, to, v| {
            if i != PAGE {
                return spring_curve(SETTLE_SPRING, tempo, from, to, v, settle_threshold(i));
            }
            match release {
                Some(velocity) => Rc::new(solve_paging(
                    from,
                    paging_settle_velocity(velocity),
                    to,
                    tempo,
                )) as SharedCurve,
                None => solve_animated_page_scroll(from, to, None, None, (v != 0.).then_some(v))
                    .shared(),
            }
        });
    }

    /// A released drag to dismiss that commits: the photo goes on down on the dismissal's
    /// measured position spring from the release velocity until it has left, while the
    /// backdrop and the controls fade on its size spring (upstream `useZoomStage`).
    fn dismiss(&mut self, exit: f64, velocity: (f64, f64), now: Instant, reduced: bool) {
        self.current(now);
        let travel = self.shown.dismiss.1.max(0.);
        self.dismissing = Some(Dismissal {
            backdrop: dismiss_backdrop_alpha(travel),
            chrome: dismiss_chrome_alpha(travel),
            closed: false,
            travel,
        });
        let to = Snapshot {
            dismiss: (self.shown.dismiss.0, exit.max(travel)),
            leave: 1.,
            ..self.shown
        };
        let tempo = self.tempo;
        self.settle_with(to, now, reduced, |i, from, to, v| match i {
            DISMISS_X => spring_curve(DISMISS_POSITION_SPRING, tempo, from, to, velocity.0, 0.05),
            DISMISS_Y => spring_curve(DISMISS_POSITION_SPRING, tempo, from, to, velocity.1, 0.05),
            LEAVE => spring_curve(DISMISS_SIZE_SPRING, tempo, from, to, v, 0.001),
            _ => spring_curve(SETTLE_SPRING, tempo, from, to, v, settle_threshold(i)),
        });
    }

    /// Where the motion ends: the keyboard works from there (UIKit's model
    /// values).
    fn target(&self) -> Snapshot {
        self.motion.as_ref().map_or(self.shown, |m| m.to)
    }
}

/// The photo fitted in the stage: contained, never enlarged past its own
/// size (upstream's `max-width`/`max-height: 100%`).
fn geometry(stage: (f64, f64), natural: (f64, f64), device_pixel_ratio: f64) -> ZoomGeometry {
    let (w, h) = stage;
    let (nw, nh) = natural;
    let fit_scale = (w / nw).min(h / nh).min(1.);
    let (fw, fh) = (nw * fit_scale, nh * fit_scale);
    let double_tap_scale = double_tap_scale_for(w, h, fw, fh);
    ZoomGeometry {
        width: w,
        height: h,
        fit: Rect {
            x: (w - fw) / 2.,
            y: (h - fh) / 2.,
            width: fw,
            height: fh,
        },
        min_scale: 1.,
        max_scale: max_scale_for(nw, fw, device_pixel_ratio, double_tap_scale),
        double_tap_scale,
    }
}

/// The natural size of an image source, once it has loaded.
fn natural_size(source: &ImageSource, window: &mut Window, cx: &mut App) -> Option<(f64, f64)> {
    let image = match source {
        ImageSource::Resource(resource) => window
            .use_asset::<ImgResourceLoader>(resource, cx)
            .and_then(|r| r.ok()),
        ImageSource::Render(image) => Some(image.clone()),
        _ => None,
    }?;
    let size = image.size(0);
    let (w, h) = (f64::from(size.width.0), f64::from(size.height.0));
    (w > 0. && h > 0.).then_some((w, h))
}

/// A full-window viewer for photos and videos, upstream `Lightbox`.
///
/// ```ignore
/// Lightbox::new("photos", open, [LightboxMedia::image("a.jpg", "A lake")])
///     .on_open_change(|open, _, _| { /* store it */ })
///     .has_zoom(true)
/// ```
#[derive(IntoElement)]
pub struct Lightbox {
    id: ElementId,
    open: bool,
    on_open_change: Option<OpenFn>,
    media: Vec<LightboxMedia>,
    index: Option<usize>,
    default_index: usize,
    on_index_change: Option<IndexFn>,
    zoom: bool,
    auto_play: bool,
    style: StyleRefinement,
}

impl Lightbox {
    /// Upstream `isOpen` and `media`.
    pub fn new(
        id: impl Into<ElementId>,
        open: bool,
        media: impl IntoIterator<Item = LightboxMedia>,
    ) -> Self {
        Lightbox {
            id: id.into(),
            open,
            on_open_change: None,
            media: media.into_iter().collect(),
            index: None,
            default_index: 0,
            on_index_change: None,
            zoom: false,
            auto_play: false,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onOpenChange`: `false` on Close, Escape, a press beside the
    /// photo or a drag down.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `index`: a controlled gallery.
    pub fn index(mut self, index: usize) -> Self {
        self.index = Some(index);
        self
    }

    /// Upstream `defaultIndex`.
    pub fn default_index(mut self, index: usize) -> Self {
        self.default_index = index;
        self
    }

    /// Upstream `onIndexChange`: a button, a key or a drag paged.
    pub fn on_index_change(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_index_change = Some(Rc::new(f));
        self
    }

    /// Upstream `hasZoom`.
    pub fn has_zoom(mut self, zoom: bool) -> Self {
        self.zoom = zoom;
        self
    }

    /// Upstream `hasAutoPlay`: a video starts as it shows.
    pub fn has_auto_play(mut self, auto_play: bool) -> Self {
        self.auto_play = auto_play;
        self
    }
}

impl Styled for Lightbox {
    /// Upstream `xstyle`: the viewer.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Lightbox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rtl = use_direction(cx).is_rtl();
        let now = cx.background_executor().now();
        let reduced = prefers_reduced_motion(cx);
        let count = self.media.len();
        let modal = modal_state(&self.id, window, cx);
        sync_open(&modal, self.open && count > 0, window, cx);
        // The touch presentation's exit: under a coarse primary pointer.
        let exit = if astryx_primitives::coarse_primary_pointer(window, cx)
            && !prefers_reduced_motion(cx)
        {
            TOUCH_EXIT
        } else {
            Duration::ZERO
        };
        let shown_phase = phase(&modal, Duration::ZERO, exit, window, cx);
        // The touch exit: the photo shrinks a little and everything fades.
        let leaving = match shown_phase {
            Phase::Closing { progress } => progress,
            _ => 0.,
        };
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-lightbox", self.id).into()),
            cx,
            {
                let default = self.default_index;
                move |_, cx| LightboxState {
                    index: default,
                    shown: REST,
                    motion: None,
                    drag: None,
                    stage: None,
                    controls_hidden: false,
                    caption_open: false,
                    announced: None,
                    was_zoomed: false,
                    photo: cx.focus_handle().tab_stop(true),
                    wheel: None,
                    dismissing: None,
                    _wheel_end: None,
                    tempo: 1.,
                    videos: Vec::new(),
                    video_sync: None,
                    video_picture: Rc::default(),
                }
            },
        );
        sync_videos(
            &state,
            &self.media,
            self.open && count > 0,
            self.index,
            self.auto_play,
            cx,
        );
        // The theme's motion tempo, which this frame's settles plan against.
        let tempo = theme.motion_tempo();
        state.update(cx, |s, _| s.tempo = tempo);
        if shown_phase == Phase::Hidden || count == 0 {
            state.update(cx, |s, _| {
                s.announced = None;
                s.shown = REST;
                s.motion = None;
                s.drag = None;
                s.dismissing = None;
                s.caption_open = false;
                s.controls_hidden = false;
            });
            return div().into_any_element();
        }

        // FR17: the nearest valid item.
        let index = self
            .index
            .unwrap_or_else(|| state.read(cx).index)
            .min(count - 1);
        let item = self.media[index].clone();
        let gallery = count > 1;
        let (can_prev, can_next) = (index > 0, index + 1 < count);
        let zoom_target = self.zoom && item.kind == LightboxMediaKind::Image;
        let is_video = item.kind == LightboxMediaKind::Video;

        // Announce a page turn (not the opening).
        let announced = state.read(cx).announced;
        if announced != Some(index) {
            if announced.is_some() {
                let t18 = i18n(cx);
                let position = [
                    ("index", Arg::Num((index + 1) as f64)),
                    ("total", Arg::Num(count as f64)),
                ];
                let text = if item.alt.is_empty() {
                    t18.format("@astryx.lightbox.imagePosition", &position)
                } else {
                    let mut args = vec![("alt", Arg::from(item.alt.as_ref()))];
                    args.extend(position);
                    t18.format("@astryx.lightbox.mediaPosition", &args)
                };
                announce(text, Politeness::Polite, window, cx);
            }
            state.update(cx, |s, _| {
                s.announced = Some(index);
                s.caption_open = false;
            });
        }

        let (snap, moving) = state.update(cx, |s, _| s.current(now));
        if moving {
            window.request_animation_frame();
        }

        // The photo's geometry, once it and the stage are known.
        let stage = state.read(cx).stage;
        let natural = (item.kind == LightboxMediaKind::Image)
            .then(|| natural_size(&item.source, window, cx))
            .flatten();
        let dpr = f64::from(window.scale_factor());
        let geo = match (stage, natural) {
            (Some(b), Some(n)) => Some(geometry(
                (f64::from(b.size.width), f64::from(b.size.height)),
                n,
                dpr,
            )),
            _ => None,
        };
        let zoomed = geo.is_some_and(|g| is_zoomed(&g, &snap.zoom));
        // Announce zooming in and out.
        if zoom_target && state.read(cx).was_zoomed != is_zoomed_target(&state, geo, cx) {
            let now_zoomed = is_zoomed_target(&state, geo, cx);
            state.update(cx, |s, _| s.was_zoomed = now_zoomed);
            let text = if now_zoomed {
                i18n(cx).t("@astryx.lightbox.zoomedIn")
            } else {
                i18n(cx).t("@astryx.lightbox.zoomedOut")
            };
            announce(text, Politeness::Polite, window, cx);
        }

        let close: Action = {
            let f = self.on_open_change.clone();
            Rc::new(move |window, cx| {
                if let Some(f) = &f {
                    f(false, window, cx);
                }
            })
        };
        let set_index: IndexFn = {
            let (f, state, controlled) = (
                self.on_index_change.clone(),
                state.clone(),
                self.index.is_some(),
            );
            Rc::new(move |to, window, cx| {
                if !controlled {
                    state.update(cx, |s, _| s.index = to);
                }
                if let Some(f) = &f {
                    f(to, window, cx);
                }
                window.refresh();
            })
        };
        // Paging resets the zoom; the row settles from where it was.
        let page_to: PageFn = {
            let (state, set_index) = (state.clone(), set_index.clone());
            Rc::new(move |to, from_offset, release, window, cx| {
                let now = cx.background_executor().now();
                state.update(cx, |s, _| {
                    s.shown = Snapshot {
                        page: from_offset,
                        ..REST
                    };
                    s.motion = None;
                    s.settle_page(now, reduced, release);
                });
                set_index(to, window, cx);
            })
        };

        // A released dismissal closes the viewer once the photo has left.
        let dismissing = state.read(cx).dismissing;
        if !moving && let Some(d) = dismissing.filter(|d| !d.closed) {
            state.update(cx, |s, _| {
                if let Some(d) = &mut s.dismissing {
                    d.closed = true;
                }
            });
            let _ = d;
            let close = close.clone();
            window.defer(cx, move |window, cx| close(window, cx));
        }
        // The backdrop and the controls follow the drag's travel; once released to
        // dismiss they fade from what they showed on the dismissal's spring.
        let travel = snap.dismiss.1.max(0.);
        let (backdrop_alpha, controls_alpha) = match dismissing {
            Some(d) => (d.backdrop * (1. - snap.leave), d.chrome * (1. - snap.leave)),
            None => (dismiss_backdrop_alpha(travel), dismiss_chrome_alpha(travel)),
        };
        let dim = faded(
            t.color.overlay,
            backdrop_alpha.clamp(0., 1.) as f32 * (1. - leaving),
        );
        let chrome_alpha = if state.read(cx).controls_hidden {
            0.
        } else {
            controls_alpha.clamp(0., 1.) as f32
        };

        // ------------------------------------------------------------------
        // The pages: the current one and its neighbours, in a row.
        let stage_width = stage.map_or(0., |b| f64::from(b.size.width));
        let stride = stage_width + PAGE_GAP;
        let any_caption = self.media.iter().any(|m| m.caption.is_some());
        let caption_open = state.read(cx).caption_open;
        let mut pages: Vec<AnyElement> = Vec::new();
        for i in index.saturating_sub(1)..(index + 2).min(count) {
            let media = &self.media[i];
            let current_page = i == index;
            let offset = (i as f64 - index as f64) * stride + snap.page;
            let offset = if rtl { -offset } else { offset };
            let media_area = self.media_area(
                &state,
                media,
                current_page,
                (geo, natural.map(|n| (n, dpr))),
                snap,
                zoomed,
                zoom_target,
                window,
                cx,
            );
            let band = any_caption.then(|| {
                caption_band(
                    &self.id,
                    &state,
                    media.caption.clone(),
                    current_page && caption_open,
                    t,
                    window,
                    cx,
                )
            });
            let page_selector = format!("{}-page-{i}", self.id);
            pages.push(
                div()
                    .debug_selector(move || page_selector.clone())
                    .absolute()
                    .top_0()
                    .left(px(offset as f32))
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(media_area)
                    .children(band)
                    .into_any_element(),
            );
        }

        // Drags follow the pointer anywhere, once begun.
        let drag_listener = state.read(cx).drag.is_some().then(|| {
            let (state, close, page_to) = (state.clone(), close.clone(), page_to.clone());
            gpui::canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let (s1, s2) = (state.clone(), state.clone());
                    let (close, page_to) = (close.clone(), page_to.clone());
                    window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble {
                            drag_move(&s1, e.position, geo, gallery, zoom_target, cx);
                            window.refresh();
                        }
                    });
                    window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                            let release = Release {
                                close: close.clone(),
                                page_to: page_to.clone(),
                                index,
                                count,
                                stride,
                                rtl,
                                reduced,
                                zoom_target,
                                is_video,
                            };
                            drag_end(&s2, e.position, geo, release, window, cx);
                        }
                    });
                },
            )
            .absolute()
            .size_0()
        });

        let track = div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .children(pages)
            .children(drag_listener);

        // ------------------------------------------------------------------
        // The controls.
        let control_backing = faded(t.color.on_light, 0.75);
        let control = |id: &str,
                       icon: &'static str,
                       label: SharedString,
                       disabled: bool,
                       mirrored: bool,
                       on: Action| {
            let mut icon_el = Icon::new(icon).size(IconSize::Sm);
            if mirrored && rtl {
                icon_el = icon_el.scale_xy(-1., 1.);
            }
            if self.zoom {
                icon_el = icon_el.text_color(t.color.on_dark);
            }
            let button = IconButton::new(
                ElementId::Name(format!("{}-{id}", self.id).into()),
                label,
                icon_el,
            )
            .variant(ButtonVariant::Ghost)
            .disabled(disabled)
            .on_click(move |_, window, cx| on(window, cx));
            if self.zoom {
                div()
                    .flex()
                    .rounded_full()
                    .bg(control_backing)
                    .child(button)
                    .into_any_element()
            } else {
                button.into_any_element()
            }
        };
        let t18 = i18n(cx);
        let close_button = control(
            "close",
            "close",
            t18.t("@astryx.lightbox.close"),
            false,
            false,
            close.clone(),
        );
        let mut chrome = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .opacity(chrome_alpha)
            .child(
                div()
                    .absolute()
                    .top(t.spacing.s3)
                    .when(!rtl, |el| el.right(t.spacing.s3))
                    .when(rtl, |el| el.left(t.spacing.s3))
                    .child(close_button),
            );
        if gallery {
            let prev = {
                let page_to = page_to.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    if can_prev {
                        page_to(index - 1, -stride, None, window, cx);
                    }
                }) as Action
            };
            let next = {
                let page_to = page_to.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    if can_next {
                        page_to(index + 1, stride, None, window, cx);
                    }
                }) as Action
            };
            let side = |el: gpui::Div, start: bool| {
                let el = el.absolute().top_0().bottom_0().flex().items_center();
                if start != rtl {
                    el.left(t.spacing.s3)
                } else {
                    el.right(t.spacing.s3)
                }
            };
            let counter = {
                let text =
                    Text::new(format!("{} / {}", index + 1, count)).text_type(TextType::Body);
                let el = div().absolute().top(t.spacing.s3);
                let el = if rtl {
                    el.right(t.spacing.s3)
                } else {
                    el.left(t.spacing.s3)
                };
                if self.zoom {
                    el.child(
                        div()
                            .rounded_full()
                            .bg(control_backing)
                            .px(t.spacing.s2)
                            .text_color(t.color.on_dark)
                            .child(text),
                    )
                } else {
                    el.child(text)
                }
            };
            chrome = chrome
                .child(side(div(), true).child(control(
                    "previous",
                    "chevronLeft",
                    t18.t("@astryx.lightbox.previous"),
                    !can_prev,
                    true,
                    prev,
                )))
                .child(side(div(), false).child(control(
                    "next",
                    "chevronRight",
                    t18.t("@astryx.lightbox.next"),
                    !can_next,
                    true,
                    next,
                )))
                .child(counter);
        }

        // ------------------------------------------------------------------
        // Keys: zoom and pan (the photo a zoom target), then paging.
        let key_state = state.clone();
        let key_page = page_to.clone();
        let on_key = move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
            let k = &event.keystroke;
            let key = k.key.as_str();
            let now = cx.background_executor().now();
            if zoom_target && let Some(g) = geo {
                let target = key_state.read(cx).target();
                let to = match key {
                    "+" | "=" => Some(released_zoom_target(&g, &target.zoom)),
                    "-" => Some(rest_zoom(&g)),
                    "0" if !k.modifiers.control && !k.modifiers.platform && !k.modifiers.alt => {
                        Some(rest_zoom(&g))
                    }
                    _ => None,
                };
                let to = to.or_else(|| {
                    is_zoomed(&g, &target.zoom).then(|| {
                        let (dx, dy) = match key {
                            "left" => (KEYBOARD_PAN_STEP, 0.),
                            "right" => (-KEYBOARD_PAN_STEP, 0.),
                            "up" => (0., KEYBOARD_PAN_STEP),
                            "down" => (0., -KEYBOARD_PAN_STEP),
                            _ => return None,
                        };
                        Some(panned_by(&g, &target.zoom, dx, dy))
                    })?
                });
                if let Some(to) = to {
                    cx.stop_propagation();
                    key_state.update(cx, |s, _| {
                        s.settle(
                            Snapshot {
                                zoom: to,
                                ..s.target()
                            },
                            now,
                            reduced,
                        )
                    });
                    window.refresh();
                    return;
                }
            }
            if key == "left" || key == "right" {
                cx.stop_propagation();
                let toward_end = (key == "right") != rtl;
                if toward_end && can_next {
                    key_page(index + 1, stride, None, window, cx);
                } else if !toward_end && can_prev {
                    key_page(index - 1, -stride, None, window, cx);
                }
            }
        };

        let viewer = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .on_key_down(on_key)
            .child(track)
            .child(chrome);
        let media_theme = theme.on_surface(
            Appearance::Dark,
            f32::from(window.viewport_size().width),
            Pointer::Fine,
        );
        let label = if item.alt.is_empty() {
            i18n(cx).t("@astryx.lightbox.mediaViewer")
        } else {
            item.alt.clone()
        };
        let surface_focus = modal.read(cx).surface.clone();
        let mut surface = div()
            .id(ElementId::Name(format!("{}-viewer", self.id).into()))
            .track_focus(&surface_focus)
            .tab_trap()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .role(Role::Dialog)
            .aria_modal(true)
            .aria_label(label)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .children(probes(&modal, cx))
            .child(scope(media_theme, viewer));
        if let Some(caption) = &item.caption {
            surface = surface.aria_description(caption.clone());
        }
        theme.refine(surface.style(), "lightbox", &[], None);
        surface.style().refine(&self.style);
        if leaving > 0. {
            surface = surface.opacity(1. - leaving).scale(1. - 0.06 * leaving);
        }

        let content = backdrop(&self.id, Some(dim), None, window).child(surface);
        let close_layer = close.clone();
        modal_layer(
            &self.id,
            content,
            true,
            DialogPurpose::Info,
            move |window, cx| close_layer(window, cx),
        )
    }
}

fn is_zoomed_target(state: &Entity<LightboxState>, geo: Option<ZoomGeometry>, cx: &App) -> bool {
    geo.is_some_and(|g| is_zoomed(&g, &state.read(cx).target().zoom))
}

/// The fitted size, centred.
fn rest_zoom(g: &ZoomGeometry) -> ZoomTransform {
    resting_transform(
        g,
        ZoomTransform {
            scale: g.min_scale,
            ..IDENTITY
        },
    )
}

/// A drag in progress: decides what it is, then pans, pages or dismisses.
fn drag_move(
    state: &Entity<LightboxState>,
    p: Point<Pixels>,
    geo: Option<ZoomGeometry>,
    gallery: bool,
    zoom_target: bool,
    cx: &mut App,
) {
    let now = cx.background_executor().now();
    state.update(cx, |s, _| {
        let Some(drag) = &mut s.drag else { return };
        let dt = now
            .saturating_duration_since(drag.last_at)
            .as_secs_f64()
            .max(0.008);
        let (vx, vy) = (
            f64::from(p.x - drag.last.x) / dt,
            f64::from(p.y - drag.last.y) / dt,
        );
        drag.velocity = (
            drag.velocity.0 * 0.3 + vx * 0.7,
            drag.velocity.1 * 0.3 + vy * 0.7,
        );
        drag.last = p;
        drag.last_at = now;
        let (dx, dy) = (
            f64::from(p.x - drag.origin.x),
            f64::from(p.y - drag.origin.y),
        );
        if drag.kind == DragKind::Undecided {
            if dx.hypot(dy) < DRAG_SLOP {
                return;
            }
            let zoomed = geo.is_some_and(|g| is_zoomed(&g, &drag.from.zoom));
            drag.kind = if zoom_target && zoomed {
                DragKind::Pan
            } else if gallery && dx.abs() > dy.abs() {
                DragKind::Page
            } else if dy > 0. && geo.is_some() {
                DragKind::Dismiss
            } else if gallery {
                DragKind::Page
            } else {
                DragKind::Undecided
            };
            if drag.kind == DragKind::Undecided {
                return;
            }
        }
        let from = drag.from;
        let kind = drag.kind;
        s.motion = None;
        s.shown = match (kind, geo) {
            (DragKind::Pan, Some(g)) => {
                let raw = raw_for_dragged(&g, from.zoom);
                Snapshot {
                    zoom: dragged_transform(
                        &g,
                        ZoomTransform {
                            x: raw.x + dx,
                            y: raw.y + dy,
                            scale: raw.scale,
                        },
                    ),
                    ..from
                }
            }
            (DragKind::Page, _) => Snapshot {
                page: from.page + dx,
                ..from
            },
            (DragKind::Dismiss, Some(_)) => Snapshot {
                dismiss: (dx, dy),
                ..from
            },
            _ => from,
        };
    });
}

/// What a release needs to know about the viewer, and what it can do.
#[derive(Clone)]
struct Release {
    close: Action,
    page_to: PageFn,
    index: usize,
    count: usize,
    /// A page's width plus the gap between pages.
    stride: f64,
    rtl: bool,
    reduced: bool,
    zoom_target: bool,
    /// The current item is a video, whose picture decides a press beside it.
    is_video: bool,
}

/// A drag's release: pages, closes or settles back.
fn drag_end(
    state: &Entity<LightboxState>,
    p: Point<Pixels>,
    geo: Option<ZoomGeometry>,
    release: Release,
    window: &mut Window,
    cx: &mut App,
) {
    let Release {
        close,
        page_to,
        index,
        count,
        stride,
        rtl,
        reduced,
        zoom_target,
        is_video,
    } = release;
    let now = cx.background_executor().now();
    let Some(mut drag) = state.update(cx, |s, _| s.drag.take()) else {
        return;
    };
    // A pointer held still before its release flings nothing.
    if now.saturating_duration_since(drag.last_at) > Duration::from_millis(50) {
        drag.velocity = (0., 0.);
    }
    let (dx, dy) = (
        f64::from(p.x - drag.origin.x),
        f64::from(p.y - drag.origin.y),
    );
    match drag.kind {
        DragKind::Undecided => {
            // A click: beside the photo it closes the viewer.
            let video = state.read(cx).video_picture.get();
            let beside = match (geo, state.read(cx).stage) {
                (None, _) if is_video => video.is_some_and(|picture| !picture.contains(&p)),
                (Some(g), Some(stage)) => {
                    let shown = state.read(cx).shown.zoom;
                    let local = (
                        f64::from(p.x - stage.origin.x),
                        f64::from(p.y - stage.origin.y),
                    );
                    let (x0, y0) = (g.fit.x + shown.x, g.fit.y + shown.y);
                    let (x1, y1) = (
                        x0 + g.fit.width * shown.scale,
                        y0 + g.fit.height * shown.scale,
                    );
                    !(local.0 >= x0 && local.0 <= x1 && local.1 >= y0 && local.1 <= y1)
                }
                _ => false,
            };
            // FR10: under a finger, a tap on a zoomable photo hides the
            // controls; while hidden, any tap brings them back, and a tap
            // beside the photo then does not close it.
            let touch = window.last_input_was_touch();
            if touch && zoom_target && state.read(cx).controls_hidden {
                state.update(cx, |s, _| s.controls_hidden = false);
            } else if beside {
                close(window, cx);
            } else if touch && zoom_target {
                state.update(cx, |s, _| s.controls_hidden = true);
            }
        }
        DragKind::Pan => {
            // The pan's release settles on `springs.settle` from the release velocity.
            if let Some(g) = geo {
                state.update(cx, |s, _| {
                    let to = resting_transform(&g, s.shown.zoom);
                    let mut velocity = [None; CHANNELS];
                    velocity[ZOOM_X] = Some(drag.velocity.0);
                    velocity[ZOOM_Y] = Some(drag.velocity.1);
                    s.settle_from(
                        Snapshot {
                            zoom: to,
                            ..s.shown
                        },
                        now,
                        reduced,
                        velocity,
                    )
                });
            }
        }
        DragKind::Page => {
            // Lands on at most one neighbour: past half a page, or flung.
            let projected = dx + drag.velocity.0 * 0.2;
            let toward_end = (projected < 0.) != rtl;
            let past = projected.abs() > stride / 2.;
            if past && toward_end && index + 1 < count {
                page_to(
                    index + 1,
                    dx + stride * if rtl { -1. } else { 1. },
                    Some(drag.velocity.0),
                    window,
                    cx,
                );
            } else if past && !toward_end && index > 0 {
                page_to(
                    index - 1,
                    dx - stride * if rtl { -1. } else { 1. },
                    Some(drag.velocity.0),
                    window,
                    cx,
                );
            } else {
                // Back to the page, on the paging settle from the release velocity.
                state.update(cx, |s, _| {
                    s.settle_page(now, reduced, Some(drag.velocity.0))
                });
            }
        }
        DragKind::Dismiss => {
            if dismiss_commits(dy.max(0.), drag.velocity.1) {
                match (geo, reduced) {
                    // Straight on down until the photo has left the stage.
                    (Some(g), false) => {
                        let exit = (g.height - g.fit.y).max(dy) + 1.;
                        state.update(cx, |s, _| s.dismiss(exit, drag.velocity, now, reduced));
                    }
                    _ => close(window, cx),
                }
            } else {
                // A cancelled drag's return is no exit: `springs.settle` from the release
                // velocity.
                state.update(cx, |s, _| {
                    let mut velocity = [None; CHANNELS];
                    velocity[DISMISS_X] = Some(drag.velocity.0);
                    velocity[DISMISS_Y] = Some(drag.velocity.1);
                    s.settle_from(
                        Snapshot {
                            dismiss: (0., 0.),
                            ..s.shown
                        },
                        now,
                        reduced,
                        velocity,
                    )
                });
            }
        }
    }
    window.refresh();
}

/// A video item's media element, made on first use and kept by its source. The
/// current page's loads fully (upstream `preload="auto"`), the others their
/// metadata; the current one starts by itself in an open viewer with `has_auto_play`.
fn video_element(
    state: &Entity<LightboxState>,
    media: &LightboxMedia,
    is_current: bool,
    auto_play: bool,
    cx: &mut App,
) -> Entity<MediaElement> {
    let existing = state
        .read(cx)
        .videos
        .iter()
        .find(|(src, _)| *src == media.src)
        .map(|(_, e)| e.clone());
    let element = existing.unwrap_or_else(|| {
        let element = cx.new(|cx| MediaElement::new(new_media_backend(cx), cx));
        // A new video frame draws the viewer again.
        let observed = element.clone();
        state.update(cx, |s, cx| {
            s.videos.push((media.src.clone(), element.clone()));
            cx.observe(&observed, |_, _, cx| cx.notify()).detach();
        });
        element
    });
    let source = MediaSource {
        kind: MediaKind::Video,
        src: media.src.clone(),
        preload: if is_current {
            MediaPreload::Auto
        } else {
            MediaPreload::Metadata
        },
        cross_origin: None,
        autoplay: is_current && auto_play,
        looping: false,
        muted: false,
        tracks: Vec::new(),
    };
    element.update(cx, |e, cx| e.set_source(source, cx));
    element
}

/// Upstream's video effect: a video left on another page stops, since the pager
/// keeps the neighbours drawn and a playing one would go on, with its sound, off
/// screen; with `has_auto_play`, the video paged to plays again. A closed viewer
/// stops a playing video.
fn sync_videos(
    state: &Entity<LightboxState>,
    media: &[LightboxMedia],
    open: bool,
    index: Option<usize>,
    auto_play: bool,
    cx: &mut App,
) {
    let index = index
        .unwrap_or_else(|| state.read(cx).index)
        .min(media.len().saturating_sub(1));
    let key = (open, index, auto_play);
    if state.read(cx).video_sync == Some(key) {
        return;
    }
    state.update(cx, |s, _| {
        s.video_sync = Some(key);
        s.videos
            .retain(|(src, _)| media.iter().any(|m| m.src == *src));
    });
    let current = media
        .get(index)
        .filter(|m| open && m.kind == LightboxMediaKind::Video)
        .map(|m| m.src.clone());
    for (src, element) in state.read(cx).videos.clone() {
        let is_current = current.as_ref() == Some(&src);
        element.update(cx, |e, cx| {
            if !is_current && !e.paused() {
                e.pause(cx);
            } else if is_current && auto_play && e.paused() {
                // A platform can refuse playback without a person's gesture.
                e.play(cx);
            }
        });
    }
}

impl Lightbox {
    /// One page's media: the photo (a zoom button when it zooms), or the
    /// video's interim frame.
    #[allow(clippy::too_many_arguments)]
    fn media_area(
        &self,
        state: &Entity<LightboxState>,
        media: &LightboxMedia,
        is_current: bool,
        (geo, natural): (Option<ZoomGeometry>, Natural),
        snap: Snapshot,
        zoomed: bool,
        zoom_target: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let mut area = div()
            .id(ElementId::Name(
                format!(
                    "{}-page-{}",
                    self.id,
                    if is_current {
                        "current"
                    } else {
                        media.src.as_ref()
                    }
                )
                .into(),
            ))
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden();
        if media.kind == LightboxMediaKind::Video {
            let element = video_element(state, media, is_current, self.open && self.auto_play, cx);
            let picture = state.read(cx).video_picture.clone();
            area = area.child(LightboxVideo {
                id: ElementId::Name(format!("{}-video-{}", self.id, media.src).into()),
                element,
                alt: media.alt.clone(),
                // Only the current page's picture decides a press beside the media.
                picture: if is_current { picture } else { Rc::default() },
            });
            return self.with_press(area, state, is_current);
        }
        let image = match (is_current.then_some(geo).flatten(), snap) {
            (Some(g), snap) => {
                let z = match state.read(cx).dismissing {
                    // Released to dismiss: the photo keeps the size it had at the lift.
                    Some(d) => {
                        let mut z = dismiss_transform(&g, snap.dismiss.0, d.travel);
                        z.y += snap.dismiss.1 - d.travel;
                        z
                    }
                    None if snap.dismiss != (0., 0.) => {
                        dismiss_transform(&g, snap.dismiss.0, snap.dismiss.1)
                    }
                    None => snap.zoom,
                };
                let photo = format!("{}-photo", self.id);
                img(media.source.clone())
                    .debug_selector(move || photo.clone())
                    .absolute()
                    .left(px((g.fit.x + z.x) as f32))
                    .top(px((g.fit.y + z.y) as f32))
                    .w(px((g.fit.width * z.scale) as f32))
                    .h(px((g.fit.height * z.scale) as f32))
                    .object_fit(ObjectFit::Fill)
                    .into_any_element()
            }
            _ => img(media.source.clone())
                .max_w_full()
                .max_h_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
        };
        area = area.child(image);
        if is_current {
            // The stage: where the photo is fitted. A resize keeps a zoomed
            // photo's zoom and the point at the centre.
            let record = state.clone();
            area = area.child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        record.update(cx, |s, _| {
                            if let (Some(before), Some((n, dpr))) = (s.stage, natural)
                                && before.size != bounds.size
                            {
                                let size = |b: Bounds<Pixels>| {
                                    (f64::from(b.size.width), f64::from(b.size.height))
                                };
                                let (g0, g1) = (
                                    geometry(size(before), n, dpr),
                                    geometry(size(bounds), n, dpr),
                                );
                                s.motion = None;
                                s.shown.zoom = transform_for_resize(&g0, &g1, &s.shown.zoom);
                            }
                            s.stage = Some(bounds);
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
            if zoom_target {
                let photo = state.read(cx).photo.clone();
                let toggle_state = state.clone();
                area = area
                    .track_focus(&photo)
                    .role(Role::Button)
                    .aria_label(i18n(cx).t("@astryx.lightbox.zoom"))
                    .aria_toggled(if zoomed {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .cursor(if zoomed {
                        gpui::CursorStyle::OpenHand
                    } else {
                        gpui::CursorStyle::PointingHand
                    })
                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                        let key = event.keystroke.key.as_str();
                        if key != "enter" && key != "space" {
                            return;
                        }
                        let Some(g) = geo else { return };
                        cx.stop_propagation();
                        let now = cx.background_executor().now();
                        let reduced = prefers_reduced_motion(cx);
                        toggle_state.update(cx, |s, _| {
                            let target = s.target();
                            let to = if is_zoomed(&g, &target.zoom) {
                                rest_zoom(&g)
                            } else {
                                released_zoom_target(&g, &target.zoom)
                            };
                            s.settle(Snapshot { zoom: to, ..target }, now, reduced)
                        });
                        window.refresh();
                    });
                if is_focus_visible(&photo, window) {
                    area = area.child(focus_ring_inset(t, px(0.), rem));
                }
                // Ctrl+wheel (or a trackpad pinch) zooms about the pointer;
                // a trackpad's scroll pans a zoomed photo.
                let wheel_state = state.clone();
                area = area.on_scroll(move |event: &ScrollEvent, window, cx| {
                    let (Some(g), Some(stage)) = (geo, wheel_state.read(cx).stage) else {
                        return;
                    };
                    let at = StagePoint {
                        x: f64::from(event.position.x - stage.origin.x),
                        y: f64::from(event.position.y - stage.origin.y),
                    };
                    let shown = wheel_state.read(cx).shown.zoom;
                    let zooming = event.modifiers.control || event.modifiers.platform;
                    let to = match (zooming, event.delta) {
                        // A pinch: past the limits it gives way, and it
                        // settles back once the wheel is still.
                        (true, delta) => {
                            let dy = f64::from(delta.pixel_delta(px(16.)).y);
                            let pinch = pinch_scale_for(&g, shown.scale) * E.powf(dy * 0.01);
                            let scale = overshoot_scale(&g, pinch.max(1e-3));
                            let now = cx.background_executor().now();
                            let timer = cx.background_executor().timer(WHEEL_PINCH_END);
                            let weak = wheel_state.downgrade();
                            let task = window.spawn(cx, async move |cx| {
                                timer.await;
                                cx.update(|window, cx| {
                                    let Some(state) = weak.upgrade() else { return };
                                    let now = cx.background_executor().now();
                                    let reduced = prefers_reduced_motion(cx);
                                    state.update(cx, |s, _| {
                                        let Some((centroid, at)) = s.wheel else {
                                            return;
                                        };
                                        if now.saturating_duration_since(at) < WHEEL_PINCH_END {
                                            return;
                                        }
                                        s.wheel = None;
                                        let to = release_target(&g, s.shown.zoom, centroid);
                                        s.settle(
                                            Snapshot {
                                                zoom: to,
                                                ..s.shown
                                            },
                                            now,
                                            reduced,
                                        );
                                    });
                                    window.refresh();
                                })
                                .ok();
                            });
                            wheel_state.update(cx, |s, _| {
                                s.wheel = Some((at, now));
                                s._wheel_end = Some(task);
                            });
                            transform_keeping_point(&g, &shown, at, at, scale)
                        }
                        (false, ScrollDelta::Pixels(d)) if is_zoomed(&g, &shown) => {
                            panned_by(&g, &shown, f64::from(d.x), f64::from(d.y))
                        }
                        _ => return,
                    };
                    cx.stop_propagation();
                    wheel_state.update(cx, |s, _| {
                        s.motion = None;
                        s.shown = Snapshot {
                            zoom: to,
                            ..s.shown
                        };
                    });
                    window.refresh();
                });
                // A double click zooms 2× about the pointer, or back.
                let double_state = state.clone();
                area = area.on_mouse_down(
                    MouseButton::Left,
                    move |event: &MouseDownEvent, window, cx| {
                        if event.click_count != 2 {
                            return;
                        }
                        let (Some(g), Some(stage)) = (geo, double_state.read(cx).stage) else {
                            return;
                        };
                        let at = StagePoint {
                            x: f64::from(event.position.x - stage.origin.x),
                            y: f64::from(event.position.y - stage.origin.y),
                        };
                        let now = cx.background_executor().now();
                        let reduced = prefers_reduced_motion(cx);
                        double_state.update(cx, |s, _| {
                            s.drag = None;
                            let target = s.target();
                            let to = double_click_target(&g, &target.zoom, at);
                            s.settle(Snapshot { zoom: to, ..target }, now, reduced)
                        });
                        window.refresh();
                    },
                );
            }
        }
        self.with_press(area, state, is_current)
    }

    /// A press on the current page starts a drag.
    fn with_press(
        &self,
        mut area: gpui::Stateful<gpui::Div>,
        state: &Entity<LightboxState>,
        is_current: bool,
    ) -> AnyElement {
        if is_current {
            let press_state = state.clone();
            area = area.on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    if event.click_count > 1 {
                        return;
                    }
                    let now = cx.background_executor().now();
                    press_state.update(cx, |s, _| {
                        let from = s.shown;
                        s.motion = None;
                        s.drag = Some(Drag {
                            kind: DragKind::Undecided,
                            origin: event.position,
                            last: event.position,
                            last_at: now,
                            velocity: (0., 0.),
                            from,
                        });
                    });
                    window.refresh();
                },
            );
        }
        area.into_any_element()
    }
}

/// The caption band under a page: two body lines, its backing only where
/// the page has a caption; a longer caption ends in "more", which opens it
/// in full over the photo.
fn caption_band(
    id: &ElementId,
    state: &Entity<LightboxState>,
    caption: Option<SharedString>,
    open: bool,
    t: &astryx_theme::Tokens,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let rem = window.rem_size();
    let line = t.text.body.size.to_pixels(rem) * t.text.body.leading;
    let band_height = t.spacing.s2.to_pixels(rem) + line * 2.;
    let backing = faded(t.color.on_light, 0.9);
    let mut band = div()
        .relative()
        .flex_none()
        .flex()
        .justify_center()
        .items_start()
        .h(band_height + t.spacing.s3.to_pixels(rem))
        .pb(t.spacing.s3);
    let Some(caption) = caption else {
        return band.into_any_element();
    };
    band = band.bg(backing);
    // Whether it needs more than two lines at the caption's width.
    let width = (window.viewport_size().width - t.spacing.s3.to_pixels(rem) * 2.).min(px(600.));
    let runs = [window.text_style().to_run(caption.len())];
    let lines = window
        .text_system()
        .shape_text(
            caption.clone(),
            t.text.body.size.to_pixels(rem),
            &runs,
            Some(width),
            None,
        )
        .map(|lines| {
            lines
                .iter()
                .map(|l| l.wrap_boundaries().len() + 1)
                .sum::<usize>()
        })
        .unwrap_or(1);
    let long = lines > 2;
    let toggle = long.then(|| {
        let state = state.clone();
        let label = if open {
            i18n(cx).t("@astryx.lightbox.captionLess")
        } else {
            i18n(cx).t("@astryx.lightbox.captionMore")
        };
        // Upstream's caption toggle `useTouchHitArea`: under a coarse pointer it takes
        // taps out toward 44 px.
        let area = {
            let state = state.clone();
            crate::gestures::touch_hit_area::hit_area(
                &ElementId::Name(format!("{id}-caption-toggle").into()),
                true,
                Some(std::rc::Rc::new(move |window, cx| {
                    state.update(cx, |s, _| s.caption_open = !s.caption_open);
                    window.refresh();
                })),
                None,
                window,
                cx,
            )
        };
        div()
            .absolute()
            .right(t.spacing.s3)
            .bottom(t.spacing.s3)
            .child(
                Link::new(
                    ElementId::Name(format!("{id}-caption-toggle").into()),
                    label,
                )
                .on_click(move |_, window, cx| {
                    state.update(cx, |s, _| s.caption_open = !s.caption_open);
                    window.refresh();
                }),
            )
            .children(area)
    });
    let text = div()
        .pt(t.spacing.s2)
        .px(t.spacing.s3)
        .max_w(px(600.))
        .text_color(t.color.on_dark)
        .child(
            Text::new(caption.clone())
                .text_type(TextType::Body)
                .max_lines(2),
        );
    if open {
        // The whole caption over the bottom of the photo, on its backing.
        let full = div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .justify_center()
            .pb(t.spacing.s3)
            .bg(backing)
            .child(
                div()
                    .id(ElementId::Name(format!("{id}-caption-open").into()))
                    .pt(t.spacing.s2)
                    .px(t.spacing.s3)
                    .max_w(px(600.))
                    .max_h(window.viewport_size().height * 0.6)
                    .overflow_y_scroll()
                    .text_color(t.color.on_dark)
                    .child(Text::new(caption).text_type(TextType::Body)),
            );
        return band
            .bg(gpui::transparent_black())
            .child(full)
            .children(toggle)
            .into_any_element();
    }
    band.child(text).children(toggle).into_any_element()
}

/// Open state and index for a Lightbox, upstream `useLightbox`: open it at
/// an item, render its pre-wired [`Lightbox`] once, and wrap each photo in
/// an accessible trigger.
///
/// ```ignore
/// let lightbox = use_lightbox("photos", media.clone(), window, cx);
/// div()
///     .child(lightbox.trigger(0, thumbnail_a))
///     .child(lightbox.trigger(1, thumbnail_b))
///     .child(lightbox.element().has_zoom(true))
/// ```
#[derive(Clone)]
pub struct LightboxHandle {
    id: ElementId,
    media: Vec<LightboxMedia>,
    state: Entity<(bool, usize)>,
}

/// Upstream `useLightbox(options)`.
pub fn use_lightbox(
    id: impl Into<ElementId>,
    media: impl IntoIterator<Item = LightboxMedia>,
    window: &mut Window,
    cx: &mut App,
) -> LightboxHandle {
    let id = id.into();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-use-lightbox").into()),
        cx,
        |_, _| (false, 0usize),
    );
    LightboxHandle {
        id,
        media: media.into_iter().collect(),
        state,
    }
}

impl LightboxHandle {
    /// Upstream `open(index)`.
    pub fn open(&self, index: usize, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| *s = (true, index));
        window.refresh();
    }

    /// Upstream `close()`.
    pub fn close(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| s.0 = false);
        window.refresh();
    }

    /// Upstream `isOpen`.
    pub fn is_open(&self, cx: &App) -> bool {
        self.state.read(cx).0
    }

    /// Upstream `index`.
    pub fn index(&self, cx: &App) -> usize {
        self.state.read(cx).1
    }

    /// Upstream `element`: the Lightbox, its state wired; add `has_zoom`,
    /// `has_auto_play` or styles to it.
    pub fn element(&self, cx: &App) -> Lightbox {
        let (open, index) = *self.state.read(cx);
        let (closing, paging) = (self.state.clone(), self.state.clone());
        Lightbox::new(self.id.clone(), open, self.media.clone())
            .index(index)
            .on_open_change(move |open, window, cx| {
                if !open {
                    closing.update(cx, |s, _| s.0 = false);
                    window.refresh();
                }
            })
            .on_index_change(move |index, window, cx| {
                paging.update(cx, |s, _| s.1 = index);
                window.refresh();
            })
    }

    /// Upstream `getTriggerProps(index)`: `content` as a button that opens
    /// the viewer at `index` (`aria-haspopup="dialog"`; Enter and Space).
    pub fn trigger(&self, index: usize, content: impl IntoElement) -> LightboxTrigger {
        LightboxTrigger {
            id: ElementId::NamedInteger(format!("{}-trigger", self.id).into(), index as u64),
            handle: self.clone(),
            index,
            content: content.into_any_element(),
        }
    }
}

/// A photo that opens a Lightbox, from [`LightboxHandle::trigger`].
#[derive(IntoElement)]
pub struct LightboxTrigger {
    id: ElementId,
    handle: LightboxHandle,
    index: usize,
    content: AnyElement,
}

impl RenderOnce for LightboxTrigger {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let focus = astryx_primitives::control_focus(&self.id, window, cx);
        let alt = self
            .handle
            .media
            .get(self.index)
            .map(|m| m.alt.clone())
            .unwrap_or_default();
        let (handle, index) = (self.handle, self.index);
        let mut el = div()
            .id(self.id)
            .relative()
            .track_focus(&focus)
            .cursor_pointer()
            .role(Role::Button)
            .aria_label(alt)
            .aria_has_popup(gpui::HasPopup::Dialog)
            .on_click(move |_, window, cx| handle.open(index, window, cx))
            .child(self.content);
        if is_focus_visible(&focus, window) {
            el = el.child(crate::focus::focus_ring(
                t,
                t.radius.element.to_pixels(rem),
                rem,
            ));
        }
        el
    }
}
