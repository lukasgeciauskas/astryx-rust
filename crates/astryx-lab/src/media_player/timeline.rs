//! Upstream lab `MediaTimeSlider` (in `MediaPlayerControls.tsx`) and
//! `MediaMarkerPopover.tsx`: the timeline, with its buffered range, chapters,
//! markers and pointer preview, and the popover a marker's content opens in.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;

use astryx_components::layout::SpacingStep;
use astryx_components::overlay::{Popover, PopoverRole};
use astryx_components::slider::{Slider, SliderValue, ValueDisplay};
use astryx_i18n::{Arg, I18n, current as i18n, use_locale};
use astryx_primitives::{LayerAlign, Placement, control_focus, is_focus_visible, use_context};
use astryx_theme::{ResolvedTheme, ThemeStyled, TokenValue, current, scope, with_tokens};
use gpui::{
    AnyElement, App, Bounds, BoxShadow, DispatchPhase, ElementId, Hsla, IntoElement, MouseUpEvent,
    ParentElement, Pixels, RenderOnce, SharedString, StyleRefinement, Styled, Window, canvas, div,
    point, prelude::*, px, relative,
};

use super::context::{
    PlayerContext, element_key, format_clock, format_spoken_time, playback_time, use_player_context,
};
use super::player::MARKER_CONTENT_CONTEXT;

/// Upstream `SLIDER_THUMB_SIZE`: the core Slider's thumb; the drawn timeline maps
/// time through the same inset, so the drawn thumb sits under the real one.
const SLIDER_THUMB_SIZE: f32 = 20.;
/// Upstream `TIMELINE_TRACK_SIZE`.
const TIMELINE_TRACK_SIZE: f32 = 4.;
/// Upstream `CHAPTER_GAP`.
const CHAPTER_GAP: f32 = 2.;
/// Upstream `TIMELINE_MARKER_SIZE`.
const TIMELINE_MARKER_SIZE: f32 = 6.;
/// Upstream `MARKER_REACH`: how near, in pixels, the pointer or the value must be
/// to name a marker.
const MARKER_REACH: f64 = 6.;
/// Upstream `MARKER_SPOKEN_REACH`: half the 5-second arrow step, so each step
/// speaks the markers nearest where it lands.
const MARKER_SPOKEN_REACH: f64 = 2.5;
/// Upstream `MARKER_HIT_SIZE`.
const MARKER_HIT_SIZE: f32 = 24.;
/// Upstream `MARKER_POPOVER_WIDTH`: fits a comment thread.
const MARKER_POPOVER_WIDTH: f32 = 360.;

/// Upstream `MediaTimelineChapter`: a chapter, a segment of the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaTimelineChapter {
    /// Upstream `start`, in seconds.
    pub start: f64,
    /// Upstream `title`: the pointer preview shows it and the timeline speaks it.
    pub title: SharedString,
}

type ContentFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// Upstream `MediaTimelineMarker`: a point of interest, such as a timestamped
/// comment.
#[derive(Clone)]
pub struct MediaTimelineMarker {
    /// Upstream `time`, in seconds.
    pub time: f64,
    /// Upstream `label`: names the marker in the preview and the spoken value.
    pub label: SharedString,
    /// Upstream `content`: a popover's content, such as a CommentThread. A marker
    /// with content is a button that also jumps to its time; one without is a dot.
    pub content: Option<ContentFn>,
}

impl MediaTimelineMarker {
    /// A dot at `time`.
    pub fn new(time: f64, label: impl Into<SharedString>) -> Self {
        MediaTimelineMarker {
            time,
            label: label.into(),
            content: None,
        }
    }

    /// Upstream `content`, built while its popover is open.
    pub fn content(
        mut self,
        content: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.content = Some(Rc::new(content));
        self
    }

    fn key(&self) -> String {
        format!("marker-{}-{}", self.time, self.label)
    }
}

/// Upstream `findChapter`: the last chapter that starts at or before `time`.
pub(crate) fn find_chapter(
    chapters: &[MediaTimelineChapter],
    time: f64,
) -> Option<&MediaTimelineChapter> {
    chapters.iter().rfind(|c| time >= c.start)
}

/// Upstream `findMarker`: the nearest marker within `reach` seconds; of equals,
/// the later in the list.
pub(crate) fn find_marker(
    markers: &[MediaTimelineMarker],
    time: f64,
    reach: f64,
) -> Option<&MediaTimelineMarker> {
    let mut found = None;
    let mut nearest = reach;
    for marker in markers {
        let distance = (marker.time - time).abs();
        if distance <= nearest {
            nearest = distance;
            found = Some(marker);
        }
    }
    found
}

/// Upstream `findMarkersNear`: the markers within `reach` seconds, in time order.
pub(crate) fn find_markers_near(
    markers: &[MediaTimelineMarker],
    time: f64,
    reach: f64,
) -> Vec<&MediaTimelineMarker> {
    let mut near: Vec<&MediaTimelineMarker> = markers
        .iter()
        .filter(|m| (m.time - time).abs() <= reach)
        .collect();
    near.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    near
}

/// The chapters with a finite start, by start.
pub(crate) fn sorted_chapters(chapters: &[MediaTimelineChapter]) -> Vec<MediaTimelineChapter> {
    let mut sorted: Vec<MediaTimelineChapter> = chapters
        .iter()
        .filter(|c| c.start.is_finite())
        .cloned()
        .collect();
    sorted.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted
}

/// Upstream's `formatValue`: the time and the duration, then the chapter, then each
/// marker within reach.
pub(crate) fn spoken_value(
    time: f64,
    duration: f64,
    chapters: &[MediaTimelineChapter],
    markers: &[MediaTimelineMarker],
    marker_reach: f64,
    i18n: &I18n,
    locale: &str,
) -> SharedString {
    let current = format_spoken_time(time, locale);
    let total = format_spoken_time(duration, locale);
    let mut spoken = i18n.format(
        "@astryx.mediaPlayer.timeValue",
        &[
            ("current", Arg::from(current.as_str())),
            ("duration", Arg::from(total.as_str())),
        ],
    );
    let context = |value: &SharedString, context: &str| {
        i18n.format(
            "@astryx.mediaPlayer.timeValueContext",
            &[
                ("value", Arg::from(value.as_ref())),
                ("context", Arg::from(context)),
            ],
        )
    };
    if let Some(chapter) = find_chapter(chapters, time) {
        spoken = context(&spoken, &chapter.title);
    }
    for marker in find_markers_near(markers, time, marker_reach.max(MARKER_SPOKEN_REACH)) {
        spoken = context(&spoken, &marker.label);
    }
    spoken
}

/// The color the timeline's empty track takes on a surface that sets its own
/// (upstream `--_media-timeline-track`): the video bar's accent tint.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TimelineTrack(pub Hsla);

/// The page's theme around a media surface: a marker's popover shows page content,
/// so it takes this theme instead of the surface's.
#[derive(Clone)]
pub(crate) struct PageTheme(pub ResolvedTheme);

/// Whether the surface the timeline sits on is hidden, so its markers take no
/// presses (upstream `--_media-controls-pointer-events`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ControlsInert(pub bool);

/// What the timeline keeps between frames.
#[derive(Default)]
struct TimelineData {
    /// The timeline's box as last painted.
    bounds: Cell<Option<Bounds<Pixels>>>,
    /// Upstream `isPointerDownRef` and `pendingTimeRef`.
    pointer_down: Cell<bool>,
    pending: Cell<Option<f64>>,
    /// Upstream `previewX`.
    preview_x: Cell<Option<f32>>,
    /// The markers whose popovers are open.
    open: std::cell::RefCell<BTreeSet<String>>,
    /// The slider thumb's focus, as the slider drew it.
    thumb: Rc<std::cell::RefCell<Option<gpui::FocusHandle>>>,
}

/// Upstream lab `MediaTimeSlider`: the timeline. Dragging previews the time and
/// seeks on release; the arrow keys skip 5 seconds and Page Up and Page Down 10. It
/// runs left to right in every direction, since it shows the direction of the
/// media. The track shows the buffered range and optional chapters and markers; a
/// pointer over it previews the time, the chapter and the nearest marker.
///
/// ```ignore
/// MediaTimeSlider::new().chapters(chapters).markers(markers)
/// ```
#[derive(IntoElement)]
pub struct MediaTimeSlider {
    chapters: Vec<MediaTimelineChapter>,
    markers: Vec<MediaTimelineMarker>,
    style: StyleRefinement,
}

impl Default for MediaTimeSlider {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaTimeSlider {
    pub fn new() -> Self {
        MediaTimeSlider {
            chapters: Vec::new(),
            markers: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `chapters`: segments of the track with a small gap between them.
    pub fn chapters(mut self, chapters: impl IntoIterator<Item = MediaTimelineChapter>) -> Self {
        self.chapters = chapters.into_iter().collect();
        self
    }

    /// Upstream `markers`: points of interest, drawn as dots.
    pub fn markers(mut self, markers: impl IntoIterator<Item = MediaTimelineMarker>) -> Self {
        self.markers = markers.into_iter().collect();
        self
    }
}

impl Styled for MediaTimeSlider {
    /// Upstream `xstyle`: the slider's container.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The time a key moves the timeline by (upstream `TIMELINE_KEY_OFFSETS`).
fn key_offset(key: &str) -> Option<f64> {
    Some(match key {
        "left" | "down" => -5.,
        "right" | "up" => 5.,
        "pagedown" => -10.,
        "pageup" => 10.,
        _ => return None,
    })
}

fn faded(color: Hsla, alpha: f32) -> Hsla {
    Hsla {
        a: color.a * alpha,
        ..color
    }
}

impl RenderOnce for MediaTimeSlider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let context = use_player_context("MediaTimeSlider", cx);
        let id = context.part("time-slider");
        let key = element_key(&id);
        let data = window
            .use_keyed_state(ElementId::Name(format!("{key}-data").into()), cx, |_, _| {
                Rc::new(TimelineData::default())
            })
            .read(cx)
            .clone();
        let slider_id = ElementId::Name(format!("{key}-slider").into());
        let state = context.store.read(cx).state().clone();
        let duration = state.duration;
        let seekable = duration.is_finite() && duration > 0.;
        let scrub = context.player.read(cx).scrub_time;
        let playback = playback_time(&context.store, window, cx);
        let value = if seekable {
            scrub.unwrap_or(playback).min(duration)
        } else {
            0.
        };
        let chapters = sorted_chapters(&self.chapters);
        let markers = self.markers;

        let width = data.bounds.get().map_or(0., |b| f32::from(b.size.width));
        let travel = (width - SLIDER_THUMB_SIZE).max(0.);
        let to_x = |time: f64| -> f32 {
            SLIDER_THUMB_SIZE / 2.
                + if seekable {
                    (time / duration).clamp(0., 1.) as f32
                } else {
                    0.
                } * travel
        };
        let to_time = |x: f32| -> f64 {
            if seekable && travel > 0. {
                f64::from(((x - SLIDER_THUMB_SIZE / 2.) / travel).clamp(0., 1.)) * duration
            } else {
                0.
            }
        };
        let marker_reach = if seekable && travel > 0. {
            MARKER_REACH / f64::from(travel) * duration
        } else {
            0.
        };

        let mut edges = vec![0.];
        if seekable {
            edges.extend(
                chapters
                    .iter()
                    .filter(|c| c.start > 0. && c.start < duration)
                    .map(|c| to_x(c.start)),
            );
        }
        edges.push(width);
        let segments: Vec<(f32, f32)> = (0..edges.len() - 1)
            .map(|i| {
                let start = if i == 0 {
                    edges[i]
                } else {
                    edges[i] + CHAPTER_GAP / 2.
                };
                let next = edges[i + 1];
                let end = if i == edges.len() - 2 {
                    next
                } else {
                    next - CHAPTER_GAP / 2.
                };
                (start, (end - start).max(0.))
            })
            .collect();
        let played_x = to_x(value);
        let buffered_x = if state.buffered_end > 0. {
            to_x(state.buffered_end)
        } else {
            0.
        };
        let preview_x = data.preview_x.get();
        let preview_time = match preview_x {
            Some(x) => Some(to_time(x)),
            None => scrub.map(|_| value),
        };
        let preview_at = preview_x.or(scrub.map(|_| played_x));
        let track = use_context::<TimelineTrack>(cx).map_or(t.color.track, |c| c.0);

        // The drawn timeline: the slider keeps the interaction, the focus ring and the
        // tints; this layer paints the track, the ranges, the markers and the thumb.
        let mut layer = div().absolute().top_0().left_0().size_full();
        if !seekable {
            layer = layer.opacity(0.5);
        }
        for (start, w) in &segments {
            let (start, w) = (*start, *w);
            layer = layer.child(
                div()
                    .absolute()
                    .top(relative(0.5))
                    .left(px(start))
                    .w(px(w))
                    .h(px(TIMELINE_TRACK_SIZE))
                    .mt(px(-TIMELINE_TRACK_SIZE / 2.))
                    .overflow_hidden()
                    .rounded_full()
                    .bg(track)
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .w(px((buffered_x - start).clamp(0., w)))
                            .bg(faded(t.color.accent, 0.4)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .w(px((played_x - start).clamp(0., w)))
                            .bg(t.color.accent),
                    ),
            );
        }
        let ring = |color: Hsla| BoxShadow {
            color,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(2.),
            inset: false,
        };
        if seekable {
            for marker in markers.iter().filter(|m| m.content.is_none()) {
                let selector = format!("{key}-{}", marker.key());
                layer = layer.child(
                    div()
                        .debug_selector(move || selector)
                        .absolute()
                        .top(relative(0.5))
                        .left(px(to_x(marker.time) - TIMELINE_MARKER_SIZE / 2.))
                        .mt(px(-TIMELINE_MARKER_SIZE / 2.))
                        .size(px(TIMELINE_MARKER_SIZE))
                        .rounded_full()
                        .bg(t.color.text_primary)
                        .shadow(vec![ring(t.color.background_surface)]),
                );
            }
        }
        layer = layer.child(
            div()
                .absolute()
                .top(relative(0.5))
                .left(px(played_x - SLIDER_THUMB_SIZE / 2.))
                .mt(px(-SLIDER_THUMB_SIZE / 2.))
                .size(px(SLIDER_THUMB_SIZE))
                .rounded_full()
                .bg(t.color.accent),
        );
        if let (Some(time), Some(at)) = (preview_time, preview_at) {
            let chapter = find_chapter(&chapters, time).map(|c| c.title.clone());
            let marker = find_marker(&markers, time, marker_reach).map(|m| m.label.clone());
            let selector = format!("{key}-preview");
            let line = |text: SharedString| {
                div()
                    .max_w_full()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(text)
            };
            layer = layer.child(
                div()
                    .debug_selector(move || selector)
                    .absolute()
                    .left(px(at))
                    .bottom(relative(1.))
                    .mb(t.spacing.s2)
                    .translate_x(relative(-0.5))
                    .flex()
                    .flex_col()
                    .items_center()
                    .py(t.spacing.s1)
                    .px(t.spacing.s2)
                    .max_w(px(240.))
                    .rounded(t.radius.container)
                    .bg(t.color.text_primary)
                    .text_color(t.color.background_surface)
                    .font_stack(&t.font.family.body)
                    .text_style(&t.text.supporting)
                    .text_center()
                    .whitespace_nowrap()
                    .child(div().child(SharedString::from(format_clock(time, duration >= 3600.))))
                    .children(chapter.map(line))
                    .children(marker.map(line)),
            );
        }

        // The slider, its paint cleared: the drawn layer above is its sibling and
        // keeps the theme's colors.
        let i18n = i18n(cx);
        let locale = use_locale(cx);
        let (spoken_chapters, spoken_markers) =
            (Rc::new(chapters.clone()), Rc::new(markers.clone()));
        let format_i18n = i18n.clone();
        let (store, player) = (context.store.clone(), context.player.clone());
        let on_change_data = data.clone();
        let mut slider = Slider::new(slider_id.clone(), i18n.t("@astryx.mediaPlayer.seek"), value)
            .label_hidden(true)
            .min(0.)
            .max(if seekable { duration } else { 1. })
            .step(0.01)
            .disabled(!seekable)
            .value_display(ValueDisplay::None)
            .format_value(move |time| {
                spoken_value(
                    time,
                    duration,
                    &spoken_chapters,
                    &spoken_markers,
                    marker_reach,
                    &format_i18n,
                    &locale,
                )
            })
            .on_change(move |next, _, cx| {
                let SliderValue::Single(time) = next else {
                    return;
                };
                if on_change_data.pointer_down.get() {
                    on_change_data.pending.set(Some(time));
                    player.update(cx, |p, cx| p.set_scrub_time(Some(time), cx));
                } else {
                    store.update(cx, |s, cx| s.seek(time, cx));
                }
            })
            .width(relative(1.));
        slider.style().refine(&self.style);
        let transparent = Hsla::transparent_black();
        let slider = with_tokens(
            [
                ("--color-track", TokenValue::Color(transparent)),
                ("--color-accent", TokenValue::Color(transparent)),
            ],
            super::controls::SliderProbe {
                player: context.player.clone(),
                slider_id,
                slider,
                sink: Some(data.thumb.clone()),
            },
            window,
            cx,
        );

        // Markers with content: buttons that jump to their time and open a popover.
        let inert = use_context::<ControlsInert>(cx).is_some_and(|i| i.0);
        let page_theme = use_context::<PageTheme>(cx);
        let keys: BTreeSet<String> = markers
            .iter()
            .filter(|m| m.content.is_some())
            .map(|m| m.key())
            .collect();
        // A marker removed while its popover is open reports the close, so its layer
        // does not keep the controls shown.
        let gone: Vec<String> = data
            .open
            .borrow()
            .iter()
            .filter(|k| !keys.contains(*k))
            .cloned()
            .collect();
        for gone in gone {
            data.open.borrow_mut().remove(&gone);
            let owner = SharedString::from(gone);
            context
                .player
                .update(cx, |p, cx| p.hold_layer(owner, false, cx));
        }
        let buttons = (seekable && !keys.is_empty()).then(|| {
            let mut buttons = div().absolute().top_0().left_0().size_full();
            for marker in markers.iter().filter(|m| m.content.is_some()) {
                buttons = buttons.child(marker_button(
                    &context,
                    &key,
                    marker,
                    to_x(marker.time),
                    &data,
                    inert,
                    page_theme.as_ref().map(|p| p.0.clone()),
                    window,
                    cx,
                ));
            }
            buttons
        });

        let (key_store, key_player, key_thumb) = (
            context.store.clone(),
            context.player.clone(),
            data.thumb.clone(),
        );
        let (down_data, down_player) = (data.clone(), context.player.clone());
        let (measure_data, commit_data, commit_store, commit_player) = (
            data.clone(),
            data.clone(),
            context.store.clone(),
            context.player.clone(),
        );
        let (move_data, leave_data) = (data.clone(), data.clone());
        let mut wrapper = div()
            .id(ElementId::Name(format!("{key}-timeline").into()))
            .debug_selector(move || key.clone())
            .relative()
            .flex_grow(1.)
            .flex_shrink(1.)
            .min_w_0()
            .on_hover(move |hovered, window, _| {
                if !*hovered {
                    leave_data.preview_x.set(None);
                    window.refresh();
                }
            })
            .on_mouse_move(move |event, window, _| {
                if window.last_input_was_touch() || !seekable {
                    return;
                }
                if let Some(b) = move_data.bounds.get() {
                    move_data
                        .preview_x
                        .set(Some(f32::from(event.position.x - b.left())));
                    window.refresh();
                }
            })
            .capture_key_down(move |event, window, cx| {
                let m = &event.keystroke.modifiers;
                // Keys typed in a marker's popover reach the timeline too; only the
                // slider's own keys seek.
                let Some(offset) = key_offset(&event.keystroke.key) else {
                    return;
                };
                let on_thumb = key_thumb
                    .borrow()
                    .as_ref()
                    .is_some_and(|f| f.is_focused(window));
                if !on_thumb || !seekable || m.alt || m.control || m.platform {
                    return;
                }
                cx.stop_propagation();
                key_store.update(cx, |s, cx| s.seek_by(offset, cx));
                key_player.update(cx, |p, cx| p.wake_controls(cx));
            })
            .capture_any_mouse_down(move |_, _, cx| {
                if seekable {
                    down_data.pointer_down.set(true);
                    down_player.update(cx, |p, cx| {
                        p.hold_controls(super::MediaControlsHold::Scrub, true, cx)
                    });
                }
            })
            .child(
                canvas(
                    move |bounds, window, _| {
                        let before = measure_data.bounds.replace(Some(bounds));
                        if before.map(|b| b.size.width) != Some(bounds.size.width) {
                            window.refresh();
                        }
                    },
                    move |_, _, window, _| {
                        // The press ends anywhere (upstream's pointer-up capture on the
                        // slider, which holds the pointer).
                        let (data, store, player) = (
                            commit_data.clone(),
                            commit_store.clone(),
                            commit_player.clone(),
                        );
                        window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                            if phase != DispatchPhase::Capture || !data.pointer_down.get() {
                                return;
                            }
                            data.pointer_down.set(false);
                            let time = data.pending.take();
                            player.update(cx, |p, cx| {
                                p.set_scrub_time(None, cx);
                                p.hold_controls(super::MediaControlsHold::Scrub, false, cx);
                            });
                            if let Some(time) = time {
                                store.update(cx, |s, cx| s.seek(time, cx));
                            }
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(layer)
            .child(div().relative().w_full().child(slider))
            .children(buttons);
        theme.refine(wrapper.style(), "media-time-slider", &[], None);
        let _ = rem;
        wrapper
    }
}

/// Upstream `MarkerButton`: a dot on the timeline that jumps to its time and opens
/// its content in a popover above it (upstream `MediaMarkerPopover`).
#[allow(clippy::too_many_arguments)]
fn marker_button(
    context: &PlayerContext,
    key: &str,
    marker: &MediaTimelineMarker,
    x: f32,
    data: &Rc<TimelineData>,
    inert: bool,
    page_theme: Option<ResolvedTheme>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let marker_key = marker.key();
    let id = ElementId::Name(format!("{key}-{marker_key}").into());
    let open = data.open.borrow().contains(&marker_key);
    let focus = control_focus(&id, window, cx);
    // The dot keeps the colors of the surface the timeline sits on.
    let (fill, ring) = (t.color.text_primary, t.color.background_surface);
    let set_open = {
        let (data, player, marker_key) = (data.clone(), context.player.clone(), marker_key.clone());
        Rc::new(move |next: bool, window: &mut Window, cx: &mut App| {
            let changed = if next {
                data.open.borrow_mut().insert(marker_key.clone())
            } else {
                data.open.borrow_mut().remove(&marker_key)
            };
            if changed {
                let owner = SharedString::from(marker_key.clone());
                player.update(cx, |p, cx| p.hold_layer(owner, next, cx));
                window.refresh();
            }
        })
    };
    let (store, time, toggle) = (context.store.clone(), marker.time, set_open.clone());
    let selector = element_key(&id);
    let mut button = div()
        .id(id.clone())
        .debug_selector(move || selector)
        .track_focus(&focus)
        .role(gpui::Role::Button)
        .aria_label(marker.label.clone())
        .aria_has_popup(gpui::HasPopup::Dialog)
        .aria_expanded(open)
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .size(px(MARKER_HIT_SIZE))
        .rounded_full()
        .child(
            div()
                .size(px(TIMELINE_MARKER_SIZE))
                .rounded_full()
                .bg(fill)
                .shadow(vec![BoxShadow {
                    color: ring,
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(2.),
                    inset: false,
                }]),
        );
    if !inert {
        button = button.cursor_pointer().on_click(move |_, window, cx| {
            store.update(cx, |s, cx| s.seek(time, cx));
            let is_open = !open;
            toggle(is_open, window, cx);
        });
    }
    if is_focus_visible(&focus, window) {
        button = button.child(astryx_components::focus::focus_ring(
            &t,
            px(MARKER_HIT_SIZE / 2.),
            window.rem_size(),
        ));
    }
    #[cfg(test)]
    super::tests::record(&id, &marker.label, open, false);
    let content = marker.content.clone();
    let popover = Popover::new(
        ElementId::Name(format!("{}-popover", element_key(&id)).into()),
        marker.label.clone(),
    )
    .placement(Placement::Above)
    .alignment(LayerAlign::Center)
    .width(px(MARKER_POPOVER_WIDTH))
    .padding(SpacingStep::S0)
    .role(PopoverRole::Dialog)
    .open(open)
    .on_open_change(move |next, window, cx| set_open(next, window, cx))
    .child(button)
    .content_fn(move |window, cx| {
        let content = content.as_ref().map(|f| f(window, cx));
        div()
            .key_context(MARKER_CONTENT_CONTEXT)
            .children(content)
            .into_any_element()
    });
    let parts: AnyElement = match page_theme {
        Some(page) => scope(page, popover).into_any_element(),
        None => popover.into_any_element(),
    };
    div()
        .absolute()
        .top(relative(0.5))
        .left(px(x - MARKER_HIT_SIZE / 2.))
        .mt(px(-MARKER_HIT_SIZE / 2.))
        .flex()
        .child(parts)
        .into_any_element()
}
