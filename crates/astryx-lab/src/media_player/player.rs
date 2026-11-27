//! Upstream lab `MediaPlayer/MediaPlayer.tsx`: the root that owns one player's
//! playback state, keyboard shortcuts, announcements and control visibility.

use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{Arg, current as i18n, use_locale};
use astryx_primitives::modality::is_typing_target_focused;
use astryx_primitives::{Politeness, interaction_modality, layer, provide};
use astryx_theme::current;
use gpui::{
    AnyElement, App, Context, ElementId, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement,
    StyleRefinement, Styled, Subscription, Task, Window, div, prelude::*,
};

use super::context::{
    DEFAULT_PLAYBACK_RATES, MediaControlsHold, PlayerContext, element_key, format_spoken_time,
};
use super::store::{MediaPlayerError, MediaPlayerState, MediaStore, MediaStoreEvent};

/// Upstream `IDLE_DELAY_MS`: idle time before playing media hides its controls.
pub(crate) const IDLE_DELAY: Duration = Duration::from_millis(2000);

/// Upstream `VOLUME_STEP`: the volume change, 0 to 1, of one Up or Down press.
const VOLUME_STEP: f64 = 0.05;

/// The key context the content of a timeline marker's popover carries (upstream
/// `MARKER_CONTENT_ATTRIBUTE`): its keys stay its own.
pub(crate) const MARKER_CONTENT_CONTEXT: &str = "MediaMarkerContent";

type Action = Rc<dyn Fn(&mut Window, &mut App)>;
type ErrorFn = Rc<dyn Fn(&MediaPlayerError, &mut Window, &mut App)>;

#[derive(Clone, Default)]
pub(crate) struct Callbacks {
    pub on_play: Option<Action>,
    pub on_pause: Option<Action>,
    pub on_ended: Option<Action>,
    pub on_error: Option<ErrorFn>,
}

/// A message for screen readers. Tests read what the player said.
pub(crate) fn say(
    message: SharedString,
    politeness: Politeness,
    window: &mut Window,
    cx: &mut App,
) {
    #[cfg(test)]
    super::tests::ANNOUNCED.with(|a| a.borrow_mut().push((message.to_string(), politeness)));
    astryx_primitives::announce(message, politeness, window, cx);
}

/// One player's state beside its store: the control visibility (upstream
/// `useControlsVisibility`), the scrub time, the callbacks and the root's focus.
pub(crate) struct PlayerState {
    pub store: Entity<MediaStore>,
    pub focus: FocusHandle,
    idle: bool,
    layer_open: bool,
    layers: BTreeSet<SharedString>,
    holds: BTreeSet<MediaControlsHold>,
    timer: Option<Task<()>>,
    touch: bool,
    playing: bool,
    pub scrub_time: Option<f64>,
    pub callbacks: Callbacks,
    /// The slider thumbs inside the player, registered as they draw.
    pub sliders: Vec<FocusHandle>,
    /// The focus the player saw last, so a move of focus inside it re-decides the
    /// focus hold, as upstream's `onFocus` does.
    last_focus: Option<FocusHandle>,
    /// Whether the mouse is over the player.
    pointer_inside: bool,
    _subscriptions: Vec<Subscription>,
}

impl PlayerState {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.new(MediaStore::new);
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe_in(&store, window, |this, store, event, window, cx| {
                let MediaStoreEvent::Changed { previous } = event;
                let next = store.read(cx).state().clone();
                this.on_change(previous, &next, window, cx);
            }),
        ];
        PlayerState {
            store,
            focus: cx.focus_handle().tab_stop(true),
            idle: false,
            layer_open: false,
            layers: BTreeSet::new(),
            holds: BTreeSet::new(),
            timer: None,
            touch: false,
            playing: false,
            scrub_time: None,
            callbacks: Callbacks::default(),
            sliders: Vec::new(),
            last_focus: None,
            pointer_inside: false,
            _subscriptions: subscriptions,
        }
    }

    /// Upstream's store listener: the playback callbacks once per transition, the
    /// error announcement, and the idle timer, which runs only while media plays.
    fn on_change(
        &mut self,
        previous: &MediaPlayerState,
        next: &MediaPlayerState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let callbacks = self.callbacks.clone();
        if previous.is_paused
            && !next.is_paused
            && let Some(f) = &callbacks.on_play
        {
            f(window, cx);
        }
        if !previous.is_paused
            && next.is_paused
            && !next.is_ended
            && let Some(f) = &callbacks.on_pause
        {
            f(window, cx);
        }
        if !previous.is_ended
            && next.is_ended
            && let Some(f) = &callbacks.on_ended
        {
            f(window, cx);
        }
        if previous.error.is_none()
            && let Some(error) = &next.error
        {
            say(
                i18n(cx).t("@astryx.mediaPlayer.error"),
                Politeness::Assertive,
                window,
                cx,
            );
            if let Some(f) = &callbacks.on_error {
                f(error, window, cx);
            }
        }
        let playing = !next.is_paused && !next.is_ended;
        if playing != self.playing {
            self.playing = playing;
            if playing {
                self.schedule_idle(cx);
            } else {
                self.clear_timer();
            }
            cx.notify();
        }
    }

    /// Whether the controls show: always while media is paused, else until idle.
    pub fn is_controls_visible(&self) -> bool {
        !self.playing || !self.idle
    }

    /// Whether a menu or marker popover the player opened is open.
    pub fn is_layer_open(&self) -> bool {
        self.layer_open
    }

    fn clear_timer(&mut self) {
        self.timer = None;
    }

    fn schedule_idle(&mut self, cx: &mut Context<Self>) {
        self.clear_timer();
        if self.touch {
            return;
        }
        self.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(IDLE_DELAY).await;
            this.update(cx, |s, cx| {
                s.timer = None;
                if s.holds.is_empty() {
                    s.idle = true;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Upstream `wakeControls`: shows the controls and restarts the idle timer.
    pub fn wake_controls(&mut self, cx: &mut Context<Self>) {
        self.touch = false;
        if self.idle {
            self.idle = false;
            cx.notify();
        }
        self.schedule_idle(cx);
    }

    /// Upstream `markTouch`: after touch input there is no idle timer.
    fn mark_touch(&mut self) {
        self.touch = true;
        self.clear_timer();
    }

    /// Upstream `holdControls`.
    pub fn hold_controls(&mut self, hold: MediaControlsHold, held: bool, cx: &mut Context<Self>) {
        if held {
            self.holds.insert(hold);
            self.clear_timer();
            self.idle = false;
            cx.notify();
        } else if self.holds.remove(&hold) {
            self.schedule_idle(cx);
        }
    }

    /// Upstream `holdLayer`: each layer reports itself by its owner, so one closing
    /// leaves another open.
    pub fn hold_layer(&mut self, owner: SharedString, open: bool, cx: &mut Context<Self>) {
        if open {
            self.layers.insert(owner);
        } else {
            self.layers.remove(&owner);
        }
        let any = !self.layers.is_empty();
        if self.layer_open != any {
            self.layer_open = any;
            cx.notify();
        }
        self.hold_controls(MediaControlsHold::Menu, any, cx);
    }

    /// Upstream `hideControls`: a mouse that leaves the player hides them.
    fn hide_controls(&mut self, cx: &mut Context<Self>) {
        if self.holds.is_empty() {
            self.clear_timer();
            self.idle = true;
            cx.notify();
        }
    }

    /// Upstream `toggleControls`: a tap shows or hides them; paused media keeps them.
    pub fn toggle_controls(&mut self, cx: &mut Context<Self>) {
        let state = self.store.read(cx).state();
        if state.is_paused || state.is_ended {
            self.idle = false;
            cx.notify();
            return;
        }
        self.clear_timer();
        self.idle = if self.idle {
            false
        } else {
            self.holds.is_empty()
        };
        cx.notify();
    }

    /// Upstream `setScrubTime`.
    pub fn set_scrub_time(&mut self, time: Option<f64>, cx: &mut Context<Self>) {
        if self.scrub_time != time {
            self.scrub_time = time;
            cx.notify();
        }
    }
}

/// Upstream `focusPlayer`: focus moves to the player's root.
pub(crate) fn focus_player(player: &Entity<PlayerState>, window: &mut Window, cx: &mut App) {
    let focus = player.read(cx).focus.clone();
    window.focus(&focus, cx);
}

/// The player state a MediaPlayer with `id` keeps.
pub(crate) fn player_state(
    id: &ElementId,
    window: &mut Window,
    cx: &mut App,
) -> Entity<PlayerState> {
    window.use_keyed_state(
        ElementId::Name(format!("{}-media-player", element_key(id)).into()),
        cx,
        PlayerState::new,
    )
}

/// Upstream's root key handler: the shortcuts, scoped to the player.
fn handle_key(context: &PlayerContext, event: &KeyDownEvent, window: &mut Window, cx: &mut App) {
    let player = context.player.clone();
    player.update(cx, |p, cx| p.wake_controls(cx));
    let m = &event.keystroke.modifiers;
    if m.alt || m.control || m.platform {
        return;
    }
    let in_marker = window
        .context_stack()
        .iter()
        .any(|c| c.contains(MARKER_CONTENT_CONTEXT));
    if is_typing_target_focused(window) || in_marker {
        return;
    }
    let focused = window.focused(cx);
    let (root, on_slider) = {
        let p = player.read(cx);
        let on_slider = focused.as_ref().is_some_and(|f| p.sliders.contains(f));
        (p.focus.clone(), on_slider)
    };
    // Anything focusable inside the player other than its root and its sliders is a
    // control: a button, a link or a menu row.
    let on_control = focused.as_ref().is_some_and(|f| *f != root) && !on_slider;
    let store = context.store.clone();
    let state = store.read(cx).state().clone();
    let locale = use_locale(cx);
    let has_duration = state.duration.is_finite() && state.duration > 0.;
    let speak_time = |window: &mut Window, cx: &mut App| {
        let time = store.read(cx).state().current_time;
        say(
            format_spoken_time(time, &locale).into(),
            Politeness::Polite,
            window,
            cx,
        );
    };
    let keystroke = &event.keystroke;
    let key = keystroke.key.as_str();
    let typed = keystroke.key_char.as_deref().unwrap_or(key);
    match (key, typed) {
        ("space", _) => {
            if on_control || on_slider {
                return;
            }
            store.update(cx, |s, cx| s.toggle_paused(cx));
        }
        (_, "k" | "K") => store.update(cx, |s, cx| s.toggle_paused(cx)),
        (_, "m" | "M") => {
            store.update(cx, |s, cx| s.toggle_muted(cx));
            let key = if store.read(cx).state().is_muted {
                "@astryx.mediaPlayer.announceMuted"
            } else {
                "@astryx.mediaPlayer.announceUnmuted"
            };
            say(i18n(cx).t(key), Politeness::Polite, window, cx);
        }
        (_, "f" | "F") => {
            if !state.can_fullscreen {
                return;
            }
            store.update(cx, |s, cx| s.toggle_fullscreen(window, cx));
        }
        (_, "c" | "C") => {
            if state.text_tracks.is_empty() {
                return;
            }
            store.update(cx, |s, cx| s.toggle_captions(cx));
            let key = if store.read(cx).state().active_text_track_id.is_none() {
                "@astryx.mediaPlayer.announceCaptionsOff"
            } else {
                "@astryx.mediaPlayer.announceCaptionsOn"
            };
            say(i18n(cx).t(key), Politeness::Polite, window, cx);
        }
        ("left" | "right", _) | (_, "j" | "J" | "l" | "L") => {
            let offset = match (key, typed) {
                ("left", _) => -5.,
                ("right", _) => 5.,
                (_, "j" | "J") => -10.,
                _ => 10.,
            };
            if matches!(key, "left" | "right") && on_slider {
                return;
            }
            if has_duration {
                store.update(cx, |s, cx| s.seek_by(offset, cx));
                speak_time(window, cx);
            }
        }
        ("up" | "down", _) => {
            if on_slider {
                return;
            }
            if state.can_set_volume {
                let offset = if key == "up" {
                    VOLUME_STEP
                } else {
                    -VOLUME_STEP
                };
                let base = if state.is_muted { 0. } else { state.volume };
                let next = (base * 100. + offset * 100.).round() / 100.;
                store.update(cx, |s, cx| s.set_volume(next, cx));
                let i18n = i18n(cx);
                let percent = i18n.percent(next.clamp(0., 1.) * 100.);
                say(
                    i18n.format(
                        "@astryx.mediaPlayer.announceVolume",
                        &[("percent", Arg::from(percent.as_str()))],
                    ),
                    Politeness::Polite,
                    window,
                    cx,
                );
            }
        }
        (_, "<" | ">") => {
            let direction: isize = if typed == "<" { -1 } else { 1 };
            let rates = DEFAULT_PLAYBACK_RATES;
            let current = rates
                .iter()
                .position(|r| *r >= state.playback_rate)
                .unwrap_or(rates.len() - 1);
            let next =
                rates[(current as isize + direction).clamp(0, rates.len() as isize - 1) as usize];
            store.update(cx, |s, cx| s.set_playback_rate(next, cx));
            let i18n = i18n(cx);
            let rate = i18n.number(next);
            say(
                i18n.format(
                    "@astryx.mediaPlayer.announceSpeed",
                    &[("rate", Arg::from(rate.as_str()))],
                ),
                Politeness::Polite,
                window,
                cx,
            );
        }
        _ => {
            let digit = (typed.len() == 1)
                .then(|| typed.chars().next().and_then(|c| c.to_digit(10)))
                .flatten();
            let Some(digit) = digit else { return };
            if !has_duration || on_slider {
                return;
            }
            store.update(cx, |s, cx| {
                s.seek(state.duration * f64::from(digit) / 10., cx)
            });
            speak_time(window, cx);
        }
    }
    cx.stop_propagation();
}

/// Upstream's root pointer handlers: a mouse that moves over the player wakes the
/// controls, one that leaves hides them, and a press marks touch input. They watch
/// the window, since the parts above the root take the pointer.
fn pointer_watch(player: Entity<PlayerState>) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let moved = player.clone();
            window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
                if phase != gpui::DispatchPhase::Capture || window.last_input_was_touch() {
                    return;
                }
                let inside = bounds.contains(&event.position);
                moved.update(cx, |p, cx| {
                    let was = std::mem::replace(&mut p.pointer_inside, inside);
                    if inside {
                        p.wake_controls(cx);
                    } else if was {
                        p.hide_controls(cx);
                    }
                });
            });
            let pressed = player.clone();
            window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
                if phase != gpui::DispatchPhase::Capture || !bounds.contains(&event.position) {
                    return;
                }
                let touch = window.last_input_was_touch();
                pressed.update(cx, |p, cx| {
                    if touch {
                        p.mark_touch()
                    } else {
                        p.wake_controls(cx)
                    }
                });
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// Upstream lab `MediaPlayer`: the root of one media player. It owns the playback
/// state of the media element inside it, scopes keyboard shortcuts to itself,
/// announces the changes those shortcuts make, and decides when the controls show.
///
/// ```ignore
/// MediaPlayer::new("memo", "Voice memo")
///     .child(MediaPlayerElement::new(MediaKind::Audio).src("media/memo.m4a"))
///     .child(Stack::horizontal().gap(SpacingStep::S2).child(MediaPlayButton::new()).child(MediaTimeSlider::new()))
/// ```
#[derive(IntoElement)]
pub struct MediaPlayer {
    id: ElementId,
    label: SharedString,
    children: Vec<AnyElement>,
    callbacks: Callbacks,
    style: StyleRefinement,
}

impl MediaPlayer {
    /// Upstream `label`: the accessible name of the player, such as the media's
    /// title.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        MediaPlayer {
            id: id.into(),
            label: label.into(),
            children: Vec::new(),
            callbacks: Callbacks::default(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onPlay`: playback starts or resumes.
    pub fn on_play(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.callbacks.on_play = Some(Rc::new(f));
        self
    }

    /// Upstream `onPause`: playback pauses.
    pub fn on_pause(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.callbacks.on_pause = Some(Rc::new(f));
        self
    }

    /// Upstream `onEnded`: playback reaches the end.
    pub fn on_ended(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.callbacks.on_ended = Some(Rc::new(f));
        self
    }

    /// Upstream `onError`: the media element reports an error.
    pub fn on_error(
        mut self,
        f: impl Fn(&MediaPlayerError, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.callbacks.on_error = Some(Rc::new(f));
        self
    }

    pub(crate) fn callbacks(mut self, callbacks: Callbacks) -> Self {
        self.callbacks = callbacks;
        self
    }
}

impl ParentElement for MediaPlayer {
    /// Upstream `children`: the media element part and the control parts.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for MediaPlayer {
    /// Upstream `xstyle`: the root.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MediaPlayer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let player = player_state(&self.id, window, cx);
        let store = player.read(cx).store.clone();
        store.update(cx, |s, cx| {
            s.attach_container(true, cx);
            s.sync_fullscreen(window, cx);
        });
        let focus = player.read(cx).focus.clone();
        // The focus hold, decided on each move of focus (upstream `onFocus` and
        // `onBlur`): focus a pointer gives does not hold the controls.
        let focused = window.focused(cx);
        player.update(cx, |p, cx| {
            p.callbacks = self.callbacks.clone();
            p.sliders.clear();
            let inside = focused
                .as_ref()
                .is_some_and(|f| *f == focus || focus.contains(f, window));
            let seen = focused.clone().filter(|_| inside);
            if seen != p.last_focus {
                p.last_focus = seen;
                let keyboard = inside
                    && interaction_modality(window)
                        == astryx_primitives::InteractionModality::Keyboard;
                p.hold_controls(MediaControlsHold::Focus, keyboard, cx);
            }
        });
        let visible = player.read(cx).is_controls_visible();
        #[cfg(test)]
        super::tests::record_root(&self.id, &player, &store, visible);
        let context = PlayerContext {
            id: self.id.clone(),
            player: player.clone(),
            store: store.clone(),
        };
        let fullscreen = store.read(cx).state().is_fullscreen;
        let key_context = context.clone();
        let mut root = div()
            .id(self.id.clone())
            .debug_selector({
                let key = element_key(&self.id);
                move || key
            })
            .track_focus(&focus)
            .role(Role::Group)
            .aria_label(self.label.clone())
            .relative()
            .min_w_0()
            .on_key_down(move |event, window, cx| handle_key(&key_context, event, window, cx))
            .child(pointer_watch(player.clone()));
        theme.refine(
            root.style(),
            "media-player",
            &[("controls", if visible { "visible" } else { "hidden" })],
            None,
        );
        root.style().refine(&self.style);
        if fullscreen {
            root = root.size_full();
        }
        if astryx_primitives::is_focus_visible(&focus, window) {
            root = root.child(astryx_components::focus::focus_ring(
                t,
                t.radius.element.to_pixels(window.rem_size()),
                window.rem_size(),
            ));
        }
        let root = root.children(self.children);
        let content = provide(context, root);
        if fullscreen {
            // The root fills the window, as the fullscreen element fills the screen.
            let size = window.viewport_size();
            let exit = store.clone();
            return div()
                .child(
                    layer(
                        div()
                            .w(size.width)
                            .h(size.height)
                            .flex()
                            .flex_col()
                            .child(content),
                        move |window, cx| exit.update(cx, |s, cx| s.toggle_fullscreen(window, cx)),
                    )
                    .anchor(
                        gpui::point(gpui::px(0.), gpui::px(0.)),
                        gpui::Anchor::TopLeft,
                    )
                    .light_dismiss(false),
                )
                .into_any_element();
        }
        content.into_any_element()
    }
}
