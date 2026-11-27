//! Upstream lab `MediaPlayer/MediaPlayerControls.tsx`: the control parts. Each reads
//! the nearest MediaPlayer; arrange them in any layout inside one.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, Entity, IntoElement, ParentElement, Pixels, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, prelude::*, px, rems,
};
use rustryx_components::button::{Button, ButtonVariant, IconButton, ToggleButton};
use rustryx_components::container_reveal::{
    ContentRevealOptions, UseContainerRevealOptions, use_container_reveal,
};
use rustryx_components::overlay::{DropdownMenu, MenuRadioGroup, MenuRadioItem};
use rustryx_components::slider::{Slider, SliderValue, ValueDisplay};
use rustryx_i18n::{Arg, current as i18n};
use rustryx_primitives::{ElementSize, OutOfTabOrder, provide, use_context};
use rustryx_theme::{ThemeStyled, TokenValue, current, with_tokens};

use super::context::{DEFAULT_PLAYBACK_RATES, PlayerContext, element_key, use_player_context};
use super::icons::{MediaIcon, MediaIconName};
use super::player::{PlayerState, focus_player};

/// Upstream `MediaControlSize`: a control's height on the shared element-size
/// scale, 28, 32 or 36px.
pub type MediaControlSize = ElementSize;

/// Upstream `MediaSliderPlacement`: which side of its button a revealed slider
/// opens on, in a row that runs left to right.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaSliderPlacement {
    #[default]
    Start,
    End,
}

/// Upstream `VOLUME_SLIDER_WIDTH`.
const VOLUME_SLIDER_WIDTH: f32 = 72.;
/// Upstream `RATE_SLIDER_WIDTH`.
const RATE_SLIDER_WIDTH: f32 = 88.;

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The width of the container the parts sit in (upstream's `@container` queries),
/// provided by the presets: under 480px the revealed sliders stay closed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MediaContainerWidth(pub Pixels);

/// Whether the parts sit in a container narrower than 480px.
pub(crate) fn is_narrow(cx: &App) -> bool {
    use_context::<MediaContainerWidth>(cx).is_some_and(|w| w.0 <= px(480.))
}

/// Parts inside a compact video bar take the small element size (upstream
/// `compactControl`, `--size-element-md` set to `--size-element-sm` on each button).
#[derive(Clone, Copy, Debug)]
pub(crate) struct CompactControls;

/// Draws `child` with the small size in place of the medium one inside a compact
/// bar.
pub(crate) fn compact(child: impl IntoElement, window: &Window, cx: &App) -> AnyElement {
    if use_context::<CompactControls>(cx).is_none() {
        return child.into_any_element();
    }
    let small = current(window, cx).tokens().size.element_sm;
    with_tokens(
        [("--size-element-md", TokenValue::Length(small))],
        child,
        window,
        cx,
    )
    .into_any_element()
}

/// A part's element id: its own, or the player's id and the part's name.
fn part_id(own: &Option<ElementId>, context: &PlayerContext, name: &str) -> ElementId {
    own.clone().unwrap_or_else(|| context.part(name))
}

/// The part's box: a test selector and its theme target.
fn part_box(
    id: &ElementId,
    target: &'static str,
    props: &[(&str, &str)],
    window: &Window,
    cx: &App,
) -> gpui::Div {
    let key = element_key(id);
    let mut el = div().flex().flex_none().debug_selector(move || key);
    current(window, cx).refine(el.style(), target, props, None);
    el
}

/// Upstream `onClick`, then the part's own action unless the handler called
/// `window.prevent_default()` (upstream `composeEventHandlers`).
fn compose(
    first: Option<ClickFn>,
    then: impl Fn(&mut Window, &mut App) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |event, window, cx| {
        if let Some(f) = &first {
            f(event, window, cx);
            if window.default_prevented() {
                return;
            }
        }
        then(window, cx);
    }
}

/// Upstream lab `MediaPlayButton`: plays and pauses the media; after the end it
/// replays from the start.
///
/// ```ignore
/// MediaPlayButton::new().variant(ButtonVariant::Primary)
/// ```
#[derive(IntoElement)]
pub struct MediaPlayButton {
    id: Option<ElementId>,
    size: Option<ElementSize>,
    variant: ButtonVariant,
    on_click: Option<ClickFn>,
    tab_stop: bool,
    style: StyleRefinement,
}

impl Default for MediaPlayButton {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaPlayButton {
    pub fn new() -> Self {
        MediaPlayButton {
            id: None,
            size: None,
            variant: ButtonVariant::Ghost,
            on_click: None,
            tab_stop: true,
            style: StyleRefinement::default(),
        }
    }

    /// The element id; by default the player's id and `play-button`.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Upstream `size`: else the surrounding size, else `md`.
    pub fn size(mut self, size: MediaControlSize) -> Self {
        self.size = Some(size);
        self
    }

    /// Upstream `variant`: `ghost` by default.
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Upstream `onClick`: runs before the toggle; `window.prevent_default()` skips it.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `tabIndex={-1}`: out of the tab order.
    pub(crate) fn out_of_tab_order(mut self) -> Self {
        self.tab_stop = false;
        self
    }
}

impl Styled for MediaPlayButton {
    /// Upstream `xstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MediaPlayButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let context = use_player_context("MediaPlayButton", cx);
        let id = part_id(&self.id, &context, "play-button");
        let state = context.store.read(cx).state();
        let (paused, ended) = (state.is_paused, state.is_ended);
        let i18n = i18n(cx);
        let label = i18n.t(if ended {
            "@astryx.mediaPlayer.replay"
        } else if paused {
            "@astryx.mediaPlayer.play"
        } else {
            "@astryx.mediaPlayer.pause"
        });
        let icon = MediaIcon::new(if ended {
            MediaIconName::Replay
        } else if paused {
            MediaIconName::Play
        } else {
            MediaIconName::Pause
        });
        #[cfg(test)]
        super::tests::record(&id, &label, false, false);
        let store = context.store.clone();
        let mut button = IconButton::new(id.clone(), label.clone(), icon)
            .tooltip(label)
            .variant(self.variant)
            .on_click(compose(self.on_click, move |_, cx| {
                store.update(cx, |s, cx| s.toggle_paused(cx))
            }));
        if let Some(size) = self.size {
            button = button.size(size);
        }
        let mut el = part_box(&id, "media-play-button", &[], window, cx);
        el.style().refine(&self.style);
        let button = compact(button, window, cx);
        if self.tab_stop {
            el.child(button)
        } else {
            el.child(provide(OutOfTabOrder, button))
        }
    }
}

/// Upstream lab `MediaSeekButton`: skips back or forward by a fixed number of
/// seconds; disabled until the media reports its duration.
///
/// ```ignore
/// MediaSeekButton::new(-15.)
/// ```
#[derive(IntoElement)]
pub struct MediaSeekButton {
    id: Option<ElementId>,
    offset: f64,
    size: Option<ElementSize>,
    on_click: Option<ClickFn>,
    style: StyleRefinement,
}

impl MediaSeekButton {
    /// Upstream `offset`: seconds to move, negative back and positive forward.
    pub fn new(offset: f64) -> Self {
        MediaSeekButton {
            id: None,
            offset,
            size: None,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    /// The element id; by default the player's id and `seek-back` or `seek-forward`.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Upstream `size`.
    pub fn size(mut self, size: MediaControlSize) -> Self {
        self.size = Some(size);
        self
    }

    /// Upstream `onClick`.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Styled for MediaSeekButton {
    /// Upstream `xstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MediaSeekButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let context = use_player_context("MediaSeekButton", cx);
        let back = self.offset < 0.;
        let id = part_id(
            &self.id,
            &context,
            if back { "seek-back" } else { "seek-forward" },
        );
        let duration = context.store.read(cx).state().duration;
        let seconds = js_round(self.offset).abs();
        let label = i18n(cx).format(
            if back {
                "@astryx.mediaPlayer.skipBack"
            } else {
                "@astryx.mediaPlayer.skipForward"
            },
            &[("seconds", Arg::Num(seconds))],
        );
        let disabled = !(duration.is_finite() && duration > 0.);
        #[cfg(test)]
        super::tests::record(&id, &label, false, disabled);
        let (store, offset) = (context.store.clone(), self.offset);
        let mut button = IconButton::new(
            id.clone(),
            label.clone(),
            MediaIcon::new(if back {
                MediaIconName::SkipBack
            } else {
                MediaIconName::SkipForward
            }),
        )
        .tooltip(label)
        .variant(ButtonVariant::Ghost)
        .disabled(disabled)
        .on_click(compose(self.on_click, move |_, cx| {
            store.update(cx, |s, cx| s.seek_by(offset, cx))
        }));
        if let Some(size) = self.size {
            button = button.size(size);
        }
        let mut el = part_box(&id, "media-seek-button", &[], window, cx);
        el.style().refine(&self.style);
        el.child(compact(button, window, cx))
    }
}

/// `Math.round`: halves round up.
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

/// Mutes and unmutes the media; its icon follows the volume.
fn mute_button(
    context: &PlayerContext,
    size: Option<ElementSize>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = context.part("mute-button");
    let state = context.store.read(cx).state();
    let silent = state.is_muted || state.volume == 0.;
    let label = i18n(cx).t(if silent {
        "@astryx.mediaPlayer.unmute"
    } else {
        "@astryx.mediaPlayer.mute"
    });
    let icon = if silent {
        MediaIconName::VolumeOff
    } else if state.volume < 0.5 {
        MediaIconName::VolumeLow
    } else {
        MediaIconName::VolumeHigh
    };
    #[cfg(test)]
    super::tests::record(&id, &label, false, false);
    let store = context.store.clone();
    let mut button = IconButton::new(id.clone(), label.clone(), MediaIcon::new(icon))
        .tooltip(label)
        .variant(ButtonVariant::Ghost)
        .on_click(move |_, _, cx| store.update(cx, |s, cx| s.toggle_muted(cx)));
    if let Some(size) = size {
        button = button.size(size);
    }
    part_box(&id, "media-volume-button", &[], window, cx)
        .child(compact(button, window, cx))
        .into_any_element()
}

/// The focus handle a control with `id` takes (`control_focus`'s), read without its
/// side effects. Call it where the control renders: the handle is keyed by the
/// element ids around it.
pub(crate) fn focus_handle_of(
    id: &ElementId,
    window: &mut Window,
    cx: &mut App,
) -> gpui::FocusHandle {
    window
        .use_keyed_state(
            ElementId::Name(format!("{id:?}-focus").into()),
            cx,
            |_, cx| cx.focus_handle().tab_stop(true),
        )
        .read(cx)
        .clone()
}

/// The thumb of a core Slider with `slider` as its id.
pub(crate) fn thumb_id(slider: &ElementId) -> ElementId {
    ElementId::Name(format!("{slider:?}-thumb-0").into())
}

type FocusSink = Rc<std::cell::RefCell<Option<gpui::FocusHandle>>>;

/// Draws a Slider and registers its thumb with the player, so the shortcuts leave
/// the slider's own keys to it. It renders where the Slider does, so it reads the
/// thumb's own focus handle.
#[derive(IntoElement)]
pub(crate) struct SliderProbe {
    pub player: Entity<PlayerState>,
    pub slider_id: ElementId,
    pub slider: Slider,
    pub sink: Option<FocusSink>,
}

impl RenderOnce for SliderProbe {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // GPUI renders each component under its type's name, so the Slider's own
        // handle is keyed one name deeper than this probe.
        let thumb = thumb_id(&self.slider_id);
        let focus = window.with_id(ElementId::from(std::any::type_name::<Slider>()), |window| {
            focus_handle_of(&thumb, window, cx)
        });
        if let Some(sink) = &self.sink {
            sink.replace(Some(focus.clone()));
        }
        self.player.update(cx, |p, _| p.sliders.push(focus));
        self.slider
    }
}

/// Sets the volume and shows it as a percentage in a tooltip.
fn volume_slider(context: &PlayerContext, cx: &mut App) -> AnyElement {
    let id = context.part("volume-slider");
    #[cfg(test)]
    super::tests::record(&id, &SharedString::from("Volume"), false, false);
    let state = context.store.read(cx).state();
    let volume = if state.is_muted { 0. } else { state.volume };
    let i18n = i18n(cx);
    let store = context.store.clone();
    let percent = i18n.clone();
    let key = element_key(&id);
    let player = context.player.clone();
    div()
        .debug_selector(move || key)
        .child(SliderProbe {
            player,
            slider_id: id.clone(),
            sink: None,
            slider: Slider::new(
                id,
                i18n.t("@astryx.mediaPlayer.volume"),
                js_round(volume * 100.),
            )
            .label_hidden(true)
            .min(0.)
            .max(100.)
            .step(1.)
            .value_display(ValueDisplay::Tooltip)
            .format_value(move |value| percent.percent(value).into())
            .on_change(move |value, _, cx| {
                if let SliderValue::Single(value) = value {
                    store.update(cx, |s, cx| s.set_volume(value / 100., cx));
                }
            })
            .width(px(VOLUME_SLIDER_WIDTH)),
        })
        .into_any_element()
}

/// Upstream `useSliderReveal`: a button with a slider that opens beside it while
/// the group is hovered or holds focus, and stays open under a coarse pointer. The
/// slider's column grows with the reveal, so the button holds still.
#[allow(clippy::too_many_arguments)]
fn reveal_group(
    id: &ElementId,
    has_slider: bool,
    placement: MediaSliderPlacement,
    slider_width: f32,
    slider: impl FnOnce(&mut Window, &mut App) -> AnyElement,
    button: AnyElement,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Stateful<gpui::Div> {
    let t = current(window, cx).tokens().clone();
    let reveal = use_container_reveal(
        ElementId::Name(format!("{}-reveal", element_key(id)).into()),
        UseContainerRevealOptions {
            enabled: has_slider,
        },
        window,
        cx,
    );
    let group = reveal
        .container(Default::default())
        .flex()
        .flex_none()
        .items_center();
    let before = placement == MediaSliderPlacement::Start;
    let slot = (has_slider && !is_narrow(cx)).then(|| {
        let mut content = reveal.content(ContentRevealOptions {
            layout_preserved: true,
            ..Default::default()
        });
        let gap = t.spacing.s2.to_pixels(window.rem_size());
        let open = content.style().opacity.unwrap_or(1.);
        let full = px(slider_width) + gap;
        let mut column = div().flex_none().w(full * open);
        if open < 1. {
            column = column.overflow_hidden();
        }
        let inner = if before { div().pr(gap) } else { div().pl(gap) };
        column.child(content.child(inner.child(slider(window, cx))))
    });
    let (start, end) = if before { (slot, None) } else { (None, slot) };
    group.children(start).child(button).children(end)
}

/// Upstream lab `MediaVolume`: Mute and volume. Mute switches the sound off and on
/// and its icon follows the volume. The volume slider opens beside Mute while the
/// group is hovered or holds focus, stays open on touch, and shows the volume in a
/// tooltip; in a container narrower than 480px it stays closed. The slider never
/// renders where the platform ignores volume changes.
///
/// ```ignore
/// MediaVolume::new().slider_placement(MediaSliderPlacement::End)
/// ```
#[derive(IntoElement)]
pub struct MediaVolume {
    size: Option<ElementSize>,
    has_slider: bool,
    slider_placement: MediaSliderPlacement,
    style: StyleRefinement,
}

impl Default for MediaVolume {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaVolume {
    pub fn new() -> Self {
        MediaVolume {
            size: None,
            has_slider: true,
            slider_placement: MediaSliderPlacement::Start,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `size`.
    pub fn size(mut self, size: MediaControlSize) -> Self {
        self.size = Some(size);
        self
    }

    /// Upstream `hasSlider`: on by default.
    pub fn has_slider(mut self, has_slider: bool) -> Self {
        self.has_slider = has_slider;
        self
    }

    /// Upstream `sliderPlacement`: `start` by default, so Mute at the end of a row
    /// stays under the pointer while the slider opens.
    pub fn slider_placement(mut self, placement: MediaSliderPlacement) -> Self {
        self.slider_placement = placement;
        self
    }
}

impl Styled for MediaVolume {
    /// Upstream `xstyle`: the group.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MediaVolume {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let context = use_player_context("MediaVolume", cx);
        let id = context.part("volume");
        let shows_slider = self.has_slider && context.store.read(cx).state().can_set_volume;
        let button = mute_button(&context, self.size, window, cx);
        let slider_context = context.clone();
        let mut group = reveal_group(
            &id,
            shows_slider,
            self.slider_placement,
            VOLUME_SLIDER_WIDTH,
            move |_, cx| volume_slider(&slider_context, cx),
            button,
            window,
            cx,
        );
        current(window, cx).refine(group.style(), "media-volume", &[], None);
        group.style().refine(&self.style);
        let key = element_key(&id);
        group.debug_selector(move || key)
    }
}

/// Upstream `MediaTime`'s `display`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaTimeDisplay {
    Current,
    Duration,
    Remaining,
    /// The default.
    #[default]
    CurrentDuration,
}

impl MediaTimeDisplay {
    fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Duration => "duration",
            Self::Remaining => "remaining",
            Self::CurrentDuration => "current-duration",
        }
    }
}

/// The text a MediaTime shows.
pub(crate) fn time_text(
    display: MediaTimeDisplay,
    time: f64,
    duration: f64,
    cx: &App,
) -> SharedString {
    let i18n = i18n(cx);
    let hours = duration.is_finite() && duration >= 3600.;
    let clock = |s: f64| super::format_clock(s, hours);
    if duration == f64::INFINITY {
        return i18n.t("@astryx.mediaPlayer.live");
    }
    match display {
        MediaTimeDisplay::Current => clock(time).into(),
        MediaTimeDisplay::Duration => clock(duration).into(),
        MediaTimeDisplay::Remaining => i18n.format(
            "@astryx.mediaPlayer.timeRemaining",
            &[(
                "remaining",
                Arg::from(clock((duration - time).max(0.)).as_str()),
            )],
        ),
        MediaTimeDisplay::CurrentDuration => i18n.format(
            "@astryx.mediaPlayer.timeReadout",
            &[
                ("current", Arg::from(clock(time).as_str())),
                ("duration", Arg::from(clock(duration).as_str())),
            ],
        ),
    }
}

/// Upstream lab `MediaTime`: the playback time; a live stream shows "Live". It
/// reads left to right in every direction.
///
/// ```ignore
/// MediaTime::new().display(MediaTimeDisplay::Remaining)
/// ```
#[derive(IntoElement)]
pub struct MediaTime {
    id: Option<ElementId>,
    display: MediaTimeDisplay,
    style: StyleRefinement,
}

impl Default for MediaTime {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaTime {
    pub fn new() -> Self {
        MediaTime {
            id: None,
            display: MediaTimeDisplay::CurrentDuration,
            style: StyleRefinement::default(),
        }
    }

    /// The element id; by default the player's id and `time-` with the display.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Upstream `display`: `current-duration` by default.
    pub fn display(mut self, display: MediaTimeDisplay) -> Self {
        self.display = display;
        self
    }
}

impl Styled for MediaTime {
    /// Upstream `xstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MediaTime {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let context = use_player_context("MediaTime", cx);
        let id = part_id(
            &self.id,
            &context,
            &format!("time-{}", self.display.as_str()),
        );
        let scrub = context.player.read(cx).scrub_time;
        let state = context.store.read(cx).state();
        let text = time_text(
            self.display,
            scrub.unwrap_or(state.current_time),
            state.duration,
            cx,
        );
        #[cfg(test)]
        super::tests::record(&id, &text, false, false);
        let key = element_key(&id);
        let mut el = div()
            .debug_selector(move || key)
            .flex_none()
            .whitespace_nowrap()
            .text_style(&t.text.supporting)
            .text_color(t.color.text_primary)
            .child(text);
        theme.refine(
            el.style(),
            "media-time",
            &[("display", self.display.as_str())],
            None,
        );
        el.style().refine(&self.style);
        el
    }
}

/// Upstream lab `MediaCaptionsButton`: turns captions on and off. It renders
/// nothing when the media has no caption or subtitle track.
#[derive(IntoElement)]
pub struct MediaCaptionsButton {
    size: Option<ElementSize>,
    style: StyleRefinement,
}

impl Default for MediaCaptionsButton {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaCaptionsButton {
    pub fn new() -> Self {
        MediaCaptionsButton {
            size: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `size`.
    pub fn size(mut self, size: MediaControlSize) -> Self {
        self.size = Some(size);
        self
    }
}

impl Styled for MediaCaptionsButton {
    /// Upstream `xstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MediaCaptionsButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let context = use_player_context("MediaCaptionsButton", cx);
        let id = context.part("captions-button");
        let state = context.store.read(cx).state();
        if state.text_tracks.is_empty() {
            return div().into_any_element();
        }
        let on = state.active_text_track_id.is_some();
        let label = i18n(cx).t("@astryx.mediaPlayer.captions");
        #[cfg(test)]
        super::tests::record(&id, &label, on, false);
        let store = context.store.clone();
        let mut toggle = ToggleButton::new(id.clone(), label.clone())
            .icon(MediaIcon::new(MediaIconName::Captions))
            .icon_only(true)
            .tooltip(label)
            .pressed(on)
            .on_pressed_change(move |_, _, _, cx| store.update(cx, |s, cx| s.toggle_captions(cx)));
        if let Some(size) = self.size {
            toggle = toggle.size(size);
        }
        let mut el = part_box(&id, "media-captions-button", &[], window, cx);
        el.style().refine(&self.style);
        el.child(compact(toggle, window, cx)).into_any_element()
    }
}

/// Upstream lab `MediaFullscreenButton`: enters and exits fullscreen. The player's
/// own root goes fullscreen, so the controls come along. It renders nothing where
/// fullscreen is not available.
#[derive(IntoElement)]
pub struct MediaFullscreenButton {
    size: Option<ElementSize>,
    on_click: Option<ClickFn>,
    style: StyleRefinement,
}

impl Default for MediaFullscreenButton {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaFullscreenButton {
    pub fn new() -> Self {
        MediaFullscreenButton {
            size: None,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `size`.
    pub fn size(mut self, size: MediaControlSize) -> Self {
        self.size = Some(size);
        self
    }

    /// Upstream `onClick`.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Styled for MediaFullscreenButton {
    /// Upstream `xstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MediaFullscreenButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let context = use_player_context("MediaFullscreenButton", cx);
        let id = context.part("fullscreen-button");
        let state = context.store.read(cx).state();
        if !state.can_fullscreen {
            return div().into_any_element();
        }
        let full = state.is_fullscreen;
        let label = i18n(cx).t(if full {
            "@astryx.mediaPlayer.exitFullscreen"
        } else {
            "@astryx.mediaPlayer.enterFullscreen"
        });
        #[cfg(test)]
        super::tests::record(&id, &label, false, false);
        let store = context.store.clone();
        let mut button = IconButton::new(
            id.clone(),
            label.clone(),
            MediaIcon::new(if full {
                MediaIconName::FullscreenExit
            } else {
                MediaIconName::FullscreenEnter
            }),
        )
        .tooltip(label)
        .variant(ButtonVariant::Ghost)
        .on_click({
            let first = self.on_click.clone();
            move |event, window, cx| {
                if let Some(f) = &first {
                    f(event, window, cx);
                    if window.default_prevented() {
                        return;
                    }
                }
                store.update(cx, |s, cx| s.toggle_fullscreen(window, cx));
            }
        });
        if let Some(size) = self.size {
            button = button.size(size);
        }
        let mut el = part_box(&id, "media-fullscreen-button", &[], window, cx);
        el.style().refine(&self.style);
        el.child(compact(button, window, cx)).into_any_element()
    }
}

/// Upstream `useRateLabel`: a speed as its short label ("1.5×") and as the
/// button's name ("Playback speed, 1.5×").
fn rate_label(rate: f64, cx: &App) -> SharedString {
    let i18n = i18n(cx);
    let number = i18n.number(rate);
    i18n.format(
        "@astryx.mediaPlayer.playbackRate",
        &[("rate", Arg::from(number.as_str()))],
    )
}

fn rate_name(rate: f64, cx: &App) -> SharedString {
    let i18n = i18n(cx);
    let number = i18n.number(rate);
    i18n.format(
        "@astryx.mediaPlayer.playbackSpeedValue",
        &[("rate", Arg::from(number.as_str()))],
    )
}

/// `String(rate)`: a speed as a menu value.
fn rate_value(rate: f64) -> SharedString {
    format!("{rate}").into()
}

/// Upstream `rateLabel`: the button fits the widest label ("1.75×") so it keeps
/// its width when the speed changes (4.5ch and the padding).
fn rate_min_width(window: &Window, cx: &App) -> Pixels {
    let t = current(window, cx).tokens().clone();
    let rem = window.rem_size();
    rems(4.5 * 0.55).to_pixels(rem) + t.spacing.s3.to_pixels(rem) * 2.
}

/// The speed button of the slider variant: a press switches between normal speed
/// and the last other speed, or from normal speed with no other yet, one step up.
fn rate_toggle(
    context: &PlayerContext,
    rates: Rc<Vec<f64>>,
    size: Option<ElementSize>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = context.part("playback-rate-button");
    let rate = context.store.read(cx).state().playback_rate;
    let last = window
        .use_keyed_state(
            ElementId::Name(format!("{}-last", element_key(&id)).into()),
            cx,
            |_, _| Rc::new(Cell::new(None::<f64>)),
        )
        .read(cx)
        .clone();
    let label = rate_label(rate, cx);
    let name = rate_name(rate, cx);
    #[cfg(test)]
    super::tests::record(&id, &name, false, false);
    let store = context.store.clone();
    let mut button = Button::new(id.clone(), label)
        .aria_label(name)
        .tooltip(i18n(cx).t("@astryx.mediaPlayer.playbackSpeed"))
        .variant(ButtonVariant::Ghost)
        .min_w(rate_min_width(window, cx))
        .on_click(move |_, _, cx| {
            let current = store.read(cx).state().playback_rate;
            if current != 1. {
                last.set(Some(current));
                store.update(cx, |s, cx| s.set_playback_rate(1., cx));
                return;
            }
            let next = last
                .get()
                .or_else(|| rates.iter().copied().find(|r| *r > 1.))
                .unwrap_or(1.);
            store.update(cx, |s, cx| s.set_playback_rate(next, cx));
        });
    if let Some(size) = size {
        button = button.size(size);
    }
    part_box(&id, "media-playback-rate-button", &[], window, cx)
        .child(compact(button, window, cx))
        .into_any_element()
}

/// The index of the listed speed nearest `rate`, so a speed set elsewhere still
/// places the thumb.
fn nearest_rate(rates: &[f64], rate: f64) -> usize {
    rates
        .iter()
        .enumerate()
        .fold(0, |best, (candidate, option)| {
            if (option - rate).abs() < (rates.get(best).copied().unwrap_or(1.) - rate).abs() {
                candidate
            } else {
                best
            }
        })
}

/// Sets the speed in steps and shows it in a tooltip.
fn rate_slider(context: &PlayerContext, rates: Rc<Vec<f64>>, cx: &mut App) -> AnyElement {
    let id = context.part("playback-rate-slider");
    let rate = context.store.read(cx).state().playback_rate;
    let index = nearest_rate(&rates, rate);
    let store = context.store.clone();
    let labels: Vec<SharedString> = rates.iter().map(|r| rate_label(*r, cx)).collect();
    let fallback = rate_label(1., cx);
    let key = element_key(&id);
    let set_rates = rates.clone();
    let player = context.player.clone();
    div()
        .debug_selector(move || key)
        .child(SliderProbe {
            player,
            slider_id: id.clone(),
            sink: None,
            slider: Slider::new(
                id,
                i18n(cx).t("@astryx.mediaPlayer.playbackSpeed"),
                index as f64,
            )
            .label_hidden(true)
            .min(0.)
            .max((rates.len().saturating_sub(1)).max(1) as f64)
            .step(1.)
            .value_display(ValueDisplay::Tooltip)
            .format_value(move |value| {
                labels
                    .get(value as usize)
                    .cloned()
                    .unwrap_or_else(|| fallback.clone())
            })
            .on_change(move |value, _, cx| {
                if let SliderValue::Single(value) = value {
                    let rate = set_rates.get(value as usize).copied().unwrap_or(1.);
                    store.update(cx, |s, cx| s.set_playback_rate(rate, cx));
                }
            })
            .width(px(RATE_SLIDER_WIDTH)),
        })
        .into_any_element()
}

/// The menu variant: the speed button opens a menu of speeds, the current one
/// checked. The open menu keeps the controls shown.
fn rate_menu(
    context: &PlayerContext,
    rates: Rc<Vec<f64>>,
    size: Option<ElementSize>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = context.part("playback-rate-menu");
    let rate = context.store.read(cx).state().playback_rate;
    let tooltip = i18n(cx).t("@astryx.mediaPlayer.playbackSpeed");
    let (label, name) = (rate_label(rate, cx), rate_name(rate, cx));
    #[cfg(test)]
    super::tests::record(&id, &name, false, false);
    let min_width = rate_min_width(window, cx);
    let store = context.store.clone();
    let mut group = MenuRadioGroup::new(rate_value(rate))
        .title(tooltip.clone())
        .on_change(move |value, _, cx| {
            if let Ok(rate) = value.parse::<f64>() {
                store.update(cx, |s, cx| s.set_playback_rate(rate, cx));
            }
        });
    for option in rates.iter() {
        group = group.item(MenuRadioItem::new(
            rate_value(*option),
            rate_label(*option, cx),
        ));
    }
    let (player, owner) = (context.player.clone(), SharedString::from(element_key(&id)));
    let menu = DropdownMenu::new(id.clone())
        .button_label(label)
        .button(move |b| {
            let mut b = b
                .aria_label(name)
                .tooltip(tooltip)
                .variant(ButtonVariant::Ghost)
                .min_w(min_width);
            if let Some(size) = size {
                b = b.size(size);
            }
            b
        })
        .on_open_change(move |open, _, cx| {
            player.update(cx, |p, cx| p.hold_layer(owner.clone(), open, cx))
        })
        .entry(group);
    part_box(&id, "media-playback-rate-button", &[], window, cx)
        .child(compact(menu, window, cx))
        .into_any_element()
}

/// Upstream `MediaPlaybackRate`'s `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaPlaybackRateVariant {
    /// A button that switches between normal speed and the last other speed, with
    /// a slider of speeds beside it while hovered or focused, always on touch.
    #[default]
    Slider,
    /// A button that opens a menu of speeds, the current one checked.
    Menu,
}

/// Upstream lab `MediaPlaybackRate`: playback speed. The button names the current
/// speed and keeps one width at every speed. In a container narrower than 480px
/// the slider stays closed.
///
/// ```ignore
/// MediaPlaybackRate::new().variant(MediaPlaybackRateVariant::Menu).rates([0.75, 1., 1.25, 1.5, 2.])
/// ```
#[derive(IntoElement)]
pub struct MediaPlaybackRate {
    variant: MediaPlaybackRateVariant,
    rates: Vec<f64>,
    size: Option<ElementSize>,
    slider_placement: MediaSliderPlacement,
    style: StyleRefinement,
}

impl Default for MediaPlaybackRate {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaPlaybackRate {
    pub fn new() -> Self {
        MediaPlaybackRate {
            variant: MediaPlaybackRateVariant::Slider,
            rates: DEFAULT_PLAYBACK_RATES.to_vec(),
            size: None,
            slider_placement: MediaSliderPlacement::End,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `variant`: `slider` by default.
    pub fn variant(mut self, variant: MediaPlaybackRateVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Upstream `rates`: the speeds offered, in order; [`DEFAULT_PLAYBACK_RATES`] by
    /// default.
    pub fn rates(mut self, rates: impl IntoIterator<Item = f64>) -> Self {
        self.rates = rates.into_iter().collect();
        self
    }

    /// Upstream `size`.
    pub fn size(mut self, size: MediaControlSize) -> Self {
        self.size = Some(size);
        self
    }

    /// Upstream `sliderPlacement`: `end` by default.
    pub fn slider_placement(mut self, placement: MediaSliderPlacement) -> Self {
        self.slider_placement = placement;
        self
    }
}

impl Styled for MediaPlaybackRate {
    /// Upstream `xstyle`: the group.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MediaPlaybackRate {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let context = use_player_context("MediaPlaybackRate", cx);
        let id = context.part("playback-rate");
        let rates = Rc::new(self.rates);
        let has_slider = self.variant == MediaPlaybackRateVariant::Slider;
        let button = if has_slider {
            rate_toggle(&context, rates.clone(), self.size, window, cx)
        } else {
            rate_menu(&context, rates.clone(), self.size, window, cx)
        };
        let slider_context = context.clone();
        let variant = if has_slider { "slider" } else { "menu" };
        let mut group = reveal_group(
            &id,
            has_slider,
            self.slider_placement,
            RATE_SLIDER_WIDTH,
            move |_, cx| rate_slider(&slider_context, rates, cx),
            button,
            window,
            cx,
        );
        current(window, cx).refine(
            group.style(),
            "media-playback-rate",
            &[("variant", variant)],
            None,
        );
        group.style().refine(&self.style);
        #[cfg(test)]
        super::tests::record(&id, &SharedString::from(variant), false, false);
        let key = element_key(&id);
        group.debug_selector(move || key)
    }
}

/// Focus moves to the player, then the large play control's toggle runs.
pub(crate) fn big_play_focus(
    player: Entity<PlayerState>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| focus_player(&player, window, cx)
}
