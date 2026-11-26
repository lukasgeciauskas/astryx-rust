//! useAnimatedLayout, upstream `hooks/useAnimatedLayout.ts` over `motion/animatedLayout.ts`
//! (spec AST-105): a handle an app passes to a component whose size changes; when it
//! changes, the content after it inside the container glides there on one shared motion,
//! the theme's `--ease-move` over `--duration-medium`.
//!
//! The glide itself is fork patch R19 (`gpui::LayoutGlide`): the container is a glide area,
//! and a component reports its change through the handle before the frame that shows it.
//! The changed element's own edge reveal (FR9) is [`reveal`]'s.

pub(crate) mod adopt;
pub(crate) mod engine;
pub mod examples;
pub(crate) mod inline;
pub(crate) mod reveal;

#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use astryx_primitives::motion::uikit::timing::REDUCED_MOTION_CROSS_FADE_MS;
use astryx_primitives::{coarse_primary_pointer, prefers_reduced_motion};
use astryx_theme::current;
use gpui::{App, Div, ElementId, LayoutGlide, Stateful, Window};
use web_time::Instant;

#[allow(unused_imports)]
pub(crate) use reveal::{EdgeReveal, use_edge_reveal};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

/// The touch cross-fade's cap (`design:reduced-motion` DR2).
const CROSS_FADE_CAP_MS: f64 = 200.;

struct Shared {
    id: ElementId,
    armed: Rc<Cell<bool>>,
    running: Rc<Cell<bool>>,
    /// When the last reported change started.
    started: Cell<Option<Instant>>,
}

/// Upstream's opaque `AnimatedLayout` handle: no public members. Pass it to a component
/// whose size changes (its `animated_layout` prop), and make the area it moves within its
/// container with [`AnimatedLayout::container`].
#[derive(Clone)]
pub struct AnimatedLayout(Rc<Shared>);

impl std::fmt::Debug for AnimatedLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnimatedLayout")
    }
}

impl PartialEq for AnimatedLayout {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Returns a handle to pass to a component whose size changes, such as TreeList or
/// Collapsible. When that component changes size, it and the content after it inside the
/// container animate together on one shared motion that follows the theme's motion tokens
/// and tempo. Fixed and stuck sticky elements stay put, the glide never scrolls the area,
/// and Reduce Motion is respected.
///
/// Upstream names the area with `containerRef`; here the app applies the handle to the
/// container with [`AnimatedLayout::container`] (a ScrollableArea's scroll viewport, or any
/// `div`). `id` names the area from frame to frame.
pub fn use_animated_layout(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> AnimatedLayout {
    let id = id.into();
    let key = ElementId::Name(format!("{id}-animated-layout").into());
    window
        .use_keyed_state(key, cx, move |_, _| {
            AnimatedLayout(Rc::new(Shared {
                id,
                armed: Rc::default(),
                running: Rc::default(),
                started: Cell::new(None),
            }))
        })
        .read(cx)
        .clone()
}

/// An element the handle can make a glide area: a `div`, with or without an id.
pub trait GlideContainer {
    #[doc(hidden)]
    fn with_layout_glide(self, glide: LayoutGlide) -> Self;
}

impl GlideContainer for Div {
    fn with_layout_glide(self, glide: LayoutGlide) -> Self {
        self.layout_glide(glide)
    }
}

impl GlideContainer for Stateful<Div> {
    fn with_layout_glide(self, glide: LayoutGlide) -> Self {
        self.layout_glide(glide)
    }
}

/// The one motion a change plays (FR4), as the theme and the platform set it now.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LayoutMotion {
    pub duration: Duration,
    pub ease: astryx_theme::CubicBezier,
    /// Reduce Motion: nothing travels.
    pub reduced: bool,
    /// Reduce Motion under a coarse primary pointer: a cross-fade over this time.
    pub cross_fade: Option<Duration>,
}

impl LayoutMotion {
    pub(crate) fn read(window: &Window, cx: &mut App) -> Self {
        let theme = current(window, cx);
        let t = theme.tokens();
        let (duration, ease) = (t.duration.medium, t.ease.r#move);
        let reduced = prefers_reduced_motion(cx);
        let cross_fade = (reduced && coarse_primary_pointer(window, cx)).then(|| {
            Duration::from_secs_f64(REDUCED_MOTION_CROSS_FADE_MS.min(CROSS_FADE_CAP_MS) / 1000.)
        });
        Self {
            duration,
            ease,
            reduced,
            cross_fade,
        }
    }

    /// Whether a change animates at all: it jumps under Reduce Motion with a fine pointer.
    pub(crate) fn animates(&self) -> bool {
        !self.reduced || self.cross_fade.is_some()
    }

    /// The eased progress of a motion started at `since`, at `now` (1 when done).
    pub(crate) fn progress(&self, since: Instant, now: Instant) -> f32 {
        let duration = self.cross_fade.unwrap_or(self.duration);
        if duration.is_zero() {
            return 1.;
        }
        let t =
            (now.saturating_duration_since(since).as_secs_f32() / duration.as_secs_f32()).min(1.);
        if self.cross_fade.is_some() {
            t
        } else {
            self.ease.ease(t)
        }
    }
}

impl AnimatedLayout {
    /// Makes `el` the area the handle's changes glide within (upstream's `containerRef`).
    pub fn container<E: GlideContainer>(&self, el: E, window: &mut Window, cx: &mut App) -> E {
        let motion = LayoutMotion::read(window, cx);
        let ease = motion.ease;
        el.with_layout_glide(LayoutGlide {
            id: self.0.id.clone(),
            armed: self.0.armed.clone(),
            duration: motion.duration,
            ease: Rc::new(move |t| ease.ease(t)),
            cross_fade: motion.cross_fade,
            running: self.0.running.clone(),
        })
    }

    /// A component reports a change before the frame that shows it: the content it moves
    /// glides from where it is drawn. Returns when the shared motion starts (now), or
    /// `None` when the change jumps (Reduce Motion with a fine pointer).
    pub(crate) fn report_change(&self, window: &Window, cx: &mut App) -> Option<Instant> {
        let motion = LayoutMotion::read(window, cx);
        if !motion.animates() {
            return None;
        }
        let now = cx.background_executor().now();
        self.0.armed.set(true);
        self.0.started.set(Some(now));
        cx.refresh_windows();
        Some(now)
    }

    /// Whether a glide in the area is running (as of its last frame).
    pub(crate) fn is_running(&self) -> bool {
        self.0.running.get()
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_ANIMATED_LAYOUT_DOC: ComponentDoc = ComponentDoc {
    name: "useAnimatedLayout",
    rust_type: "astryx::components::animated_layout::use_animated_layout -> AnimatedLayout",
    props: &[p(
        "containerRef",
        RustApi::Method("AnimatedLayout::container (applied to the area's div)"),
    )],
    example: include_str!("examples.rs"),
};
