//! useEntryAnimation, upstream `hooks/useEntryAnimation.ts`: a short entrance for an element
//! that appears after the window's first paint (a validation message, a toast, an expanded
//! section); elements drawn in the first frame never animate.

use std::collections::HashSet;

use astryx_primitives::prefers_reduced_motion;
use astryx_theme::current;
use gpui::{App, ElementId, Global, Styled, Window, WindowId};
use web_time::Instant;

/// Upstream `EntryAnimationPreset`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EntryAnimationPreset {
    /// Fades in from `--spacing-2` above.
    #[default]
    SlideDown,
    /// Fades in from `--spacing-2` below.
    SlideUp,
    FadeIn,
    /// Fades in from 95%.
    ScaleIn,
}

/// The windows whose first frame has painted (upstream's `initialPaintComplete`).
#[derive(Default)]
struct Painted(HashSet<WindowId>);

impl Global for Painted {}

/// What an entering element remembers: whether it animates, and from when.
struct Entry {
    animates: bool,
    since: Instant,
}

/// Upstream `useEntryAnimation`'s return: the entrance, applied to the element with
/// [`EntryAnimation::apply`].
#[derive(Clone, Copy)]
pub struct EntryAnimation {
    preset: EntryAnimationPreset,
    /// 0 at the start, 1 at rest; `None` when the element does not animate.
    progress: Option<f32>,
}

/// Upstream `useEntryAnimation`. Call it where the element renders; `id` names the element.
///
/// ```ignore
/// let entry = use_entry_animation("error", EntryAnimationPreset::SlideDown, window, cx);
/// entry.apply(div().child(message), window, cx)
/// ```
pub fn use_entry_animation(
    id: impl Into<ElementId>,
    preset: EntryAnimationPreset,
    window: &mut Window,
    cx: &mut App,
) -> EntryAnimation {
    let id = id.into();
    let handle = window.window_handle().window_id();
    let painted = cx.default_global::<Painted>().0.contains(&handle);
    if !painted {
        // After this frame: what renders in it was on the first paint.
        window.defer(cx, move |_, cx| {
            cx.default_global::<Painted>().0.insert(handle);
        });
    }
    let now = cx.background_executor().now();
    // Captured once, on the element's first render.
    let entry = window.use_keyed_state(
        ElementId::Name(format!("{id}-entry-animation").into()),
        cx,
        move |_, _| Entry {
            animates: painted,
            since: now,
        },
    );
    let (animates, since) = {
        let e = entry.read(cx);
        (e.animates, e.since)
    };
    if !animates || prefers_reduced_motion(cx) {
        return EntryAnimation {
            preset,
            progress: None,
        };
    }
    let duration = current(window, cx).tokens().duration.fast_max;
    let progress = if duration.is_zero() {
        1.
    } else {
        (now.saturating_duration_since(since).as_secs_f32() / duration.as_secs_f32()).min(1.)
    };
    if progress < 1. {
        window.request_animation_frame();
    }
    EntryAnimation {
        preset,
        progress: Some(progress),
    }
}

impl EntryAnimation {
    /// Whether the element is still entering.
    pub fn is_animating(&self) -> bool {
        self.progress.is_some_and(|p| p < 1.)
    }

    /// The entrance's opacity and transform on `el` (fork patch R1's translate and scale).
    pub fn apply<E: Styled>(&self, el: E, window: &Window, cx: &App) -> E {
        let Some(progress) = self.progress.filter(|p| *p < 1.) else {
            return el;
        };
        let theme = current(window, cx);
        let t = theme.tokens();
        let eased = t.ease.standard.ease(progress);
        let shift = t.spacing.s2.to_pixels(window.rem_size()) * (1. - eased);
        let el = el.opacity(eased);
        match self.preset {
            EntryAnimationPreset::SlideDown => el.translate_y(-shift),
            EntryAnimationPreset::SlideUp => el.translate_y(shift),
            EntryAnimationPreset::FadeIn => el,
            EntryAnimationPreset::ScaleIn => el.scale(0.95 + 0.05 * eased),
        }
    }
}
