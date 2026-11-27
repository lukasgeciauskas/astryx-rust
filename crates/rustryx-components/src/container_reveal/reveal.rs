//! The reveal state machine and its two element builders.

use std::time::Duration;

use gpui::{
    App, Div, ElementId, Entity, FocusHandle, InteractiveElement as _, Stateful,
    StatefulInteractiveElement as _, Styled as _, Visibility, Window, div,
};
use rustryx_primitives::{coarse_primary_pointer, prefers_reduced_motion};
use rustryx_theme::current;
use web_time::Instant;

/// Upstream `UseContainerRevealOptions`.
#[derive(Clone, Copy, Debug)]
pub struct UseContainerRevealOptions {
    /// Upstream `isEnabled`: when false, content always shows and the container
    /// takes no reveal behavior.
    pub enabled: bool,
}

impl Default for UseContainerRevealOptions {
    fn default() -> Self {
        UseContainerRevealOptions { enabled: true }
    }
}

/// Upstream `forceState`: something else owns the interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevealState {
    /// Revealed whatever the pointer does.
    Active,
    /// Hover does not reveal; keyboard focus and a coarse pointer still do.
    Inactive,
}

/// Upstream `forceVisibility`: one piece of content ignores the container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevealVisibility {
    Shown,
    Hidden,
}

/// Upstream `ContainerRevealOptions`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContainerRevealOptions {
    pub force_state: Option<RevealState>,
    /// Upstream `hoverDelay`: how long the pointer rests before hover reveals.
    pub hover_delay: Duration,
}

/// Upstream `ContentRevealOptions`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContentRevealOptions {
    /// Upstream `isRevealInverted`: shown at rest, hidden while the pointer is on the
    /// container (hover only; focus does not conceal).
    pub inverted: bool,
    /// Upstream `isLayoutPreserved`: hidden content keeps its box (no layout shift).
    pub layout_preserved: bool,
    pub force_visibility: Option<RevealVisibility>,
}

/// What the container remembers between frames.
#[derive(Default)]
struct RevealData {
    /// Whether a frame has rendered: the first takes its state without a fade.
    mounted: bool,
    hovered_since: Option<Instant>,
    /// The reveal's opacity when it last changed direction, and when.
    from: f32,
    target: f32,
    changed_at: Option<Instant>,
    /// The concealed content's opacity, likewise.
    conceal_from: f32,
    conceal_target: f32,
    conceal_changed_at: Option<Instant>,
}

/// One container's reveal, upstream `useContainerReveal`'s return value.
#[derive(Clone)]
pub struct ContainerReveal {
    id: ElementId,
    enabled: bool,
    state: Entity<RevealData>,
    focus: FocusHandle,
    /// The reveal's opacity this frame (0 hidden, 1 shown), and the conceal's.
    reveal: f32,
    conceal: f32,
}

/// Upstream `useContainerReveal`.
///
/// ```ignore
/// let reveal = use_container_reveal("row", Default::default(), window, cx);
/// reveal
///     .container(Default::default())
///     .flex()
///     .child("Row")
///     .child(reveal.content(Default::default()).child(Button::new("edit", "Edit")))
/// ```
pub fn use_container_reveal(
    id: impl Into<ElementId>,
    options: UseContainerRevealOptions,
    window: &mut Window,
    cx: &mut App,
) -> ContainerReveal {
    use_container_reveal_with(id, options, ContainerRevealOptions::default(), window, cx)
}

/// `use_container_reveal` with the container's options decided up front, since the
/// reveal's opacity this frame depends on them.
pub(crate) fn use_container_reveal_with(
    id: impl Into<ElementId>,
    options: UseContainerRevealOptions,
    container: ContainerRevealOptions,
    window: &mut Window,
    cx: &mut App,
) -> ContainerReveal {
    let id = id.into();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-container-reveal").into()),
        cx,
        |_, _| RevealData {
            from: 0.,
            target: 0.,
            conceal_from: 1.,
            conceal_target: 1.,
            ..RevealData::default()
        },
    );
    let focus = window
        .use_keyed_state(
            ElementId::Name(format!("{id}-container-reveal-focus").into()),
            cx,
            |_, cx| cx.focus_handle(),
        )
        .read(cx)
        .clone();
    if !options.enabled {
        return ContainerReveal {
            id,
            enabled: false,
            state,
            focus,
            reveal: 1.,
            conceal: 1.,
        };
    }
    let now = cx.background_executor().now();
    let duration = if prefers_reduced_motion(cx) {
        Duration::ZERO
    } else {
        current(window, cx).tokens().duration.fast
    };
    let ease = current(window, cx).tokens().ease.standard;
    let coarse = coarse_primary_pointer(window, cx);
    let focused = focus.contains_focused(window, cx);
    let mut animating = false;
    let (reveal, conceal) = state.update(cx, |s, _| {
        let resting = s
            .hovered_since
            .is_some_and(|since| now.duration_since(since) >= container.hover_delay);
        if s.hovered_since.is_some() && !resting {
            animating = true;
        }
        let hover = resting && container.force_state != Some(RevealState::Inactive);
        let active = match container.force_state {
            Some(RevealState::Active) => true,
            _ => hover || focused || coarse,
        };
        let conceal_hidden = match container.force_state {
            Some(RevealState::Active) => true,
            Some(RevealState::Inactive) => false,
            None => hover,
        };
        let mounted = std::mem::replace(&mut s.mounted, true);
        let mut step = |from: &mut f32, target: &mut f32, at: &mut Option<Instant>, goal: f32| {
            if !mounted {
                *from = goal;
                *target = goal;
                return goal;
            }
            let current = value(*from, *target, *at, now, duration, ease);
            if goal != *target {
                *from = current;
                *target = goal;
                *at = Some(now);
            }
            let value = value(*from, *target, *at, now, duration, ease);
            if value != *target {
                animating = true;
            }
            value
        };
        let reveal = step(
            &mut s.from,
            &mut s.target,
            &mut s.changed_at,
            if active { 1. } else { 0. },
        );
        let conceal = step(
            &mut s.conceal_from,
            &mut s.conceal_target,
            &mut s.conceal_changed_at,
            if conceal_hidden { 0. } else { 1. },
        );
        (reveal, conceal)
    });
    if animating {
        window.request_animation_frame();
    }
    ContainerReveal {
        id,
        enabled: true,
        state,
        focus,
        reveal,
        conceal,
    }
}

/// The opacity between `from` and `target`, eased over `duration` since `at`.
fn value(
    from: f32,
    target: f32,
    at: Option<Instant>,
    now: Instant,
    duration: Duration,
    ease: rustryx_theme::CubicBezier,
) -> f32 {
    let Some(at) = at else { return target };
    if duration.is_zero() {
        return target;
    }
    let progress = (now.duration_since(at).as_secs_f32() / duration.as_secs_f32()).min(1.);
    from + (target - from) * ease.ease(progress)
}

impl ContainerReveal {
    /// Upstream `getContainerProps`: the container, which notices the pointer resting
    /// on it and focus inside it. The options must match those the reveal was made with.
    pub fn container(&self, _options: ContainerRevealOptions) -> Stateful<Div> {
        let el = div().id(ElementId::Name(
            format!("{}-reveal-container", self.id).into(),
        ));
        if !self.enabled {
            return el;
        }
        let state = self.state.clone();
        el.track_focus(&self.focus)
            .on_hover(move |hovered, window, cx| {
                let now = cx.background_executor().now();
                state.update(cx, |s, _| s.hovered_since = hovered.then_some(now));
                window.refresh();
            })
    }

    /// Upstream `getContentRevealProps`: content that shows with the container's
    /// reveal. Hidden content leaves the layout unless `layout_preserved`, and never
    /// takes the pointer.
    pub fn content(&self, options: ContentRevealOptions) -> Div {
        let el = div();
        if !self.enabled {
            return el;
        }
        let opacity = match options.force_visibility {
            Some(RevealVisibility::Shown) => 1.,
            Some(RevealVisibility::Hidden) => 0.,
            None if options.inverted => self.conceal,
            None => self.reveal,
        };
        let mut el = el.opacity(opacity);
        if opacity > 0. {
            return el;
        }
        el.style().visibility = Some(Visibility::Hidden);
        if options.layout_preserved {
            el
        } else {
            el.absolute()
        }
    }

    /// Whether the content is fully shown this frame.
    pub fn is_revealed(&self) -> bool {
        !self.enabled || self.reveal >= 1.
    }
}
