//! What the components that adopt the animated layout share: the edge reveal as its
//! adopters play it (FR9: the element fades with what it hides, and held content is
//! inert), a disclosure indicator's turn on the shared motion (upstream
//! `applyLayoutChangeTransition`), and the height transition of the disclosures upstream
//! animates with a CSS grid row (`grid-template-rows` between `0fr` and `1fr`):
//! CodeBlock's body, ChatComposerDrawer's content, ChatToolCalls' group and TopNav's
//! drawer groups.

use std::time::Duration;

use astryx_theme::CubicBezier;
use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, Styled, Window, div, prelude::FluentBuilder as _, px,
};

use super::{AnimatedLayout, EdgeReveal, LayoutMotion};

/// `content` inside `reveal`'s clip, fading with what the clip hides (upstream's
/// `opacity: 1 - hidden` on the revealed element) unless Reduce Motion cross-fades it
/// already. `inert` content (a conceal in progress) is inert (fork patch R20): out of the
/// tab order, the pointer's targets and the accessibility tree. Nothing when the reveal is
/// closed at rest.
pub(crate) fn reveal_content(
    reveal: EdgeReveal,
    content: impl IntoElement,
    inert: bool,
    window: &Window,
    cx: &mut App,
) -> Option<AnyElement> {
    if !reveal.mounted {
        return None;
    }
    let motion = LayoutMotion::read(window, cx);
    let fade = reveal.is_moving() && motion.cross_fade.is_none();
    let shown = reveal.shown();
    let content = div()
        .relative()
        .when(fade, |el| el.opacity(shown.clamp(0., 1.)))
        .child(content)
        .inert(inert);
    reveal.wrap(content)
}

/// A disclosure indicator's turn, from 0 (closed) to 1 (open). With a handle it turns on
/// the handle's shared motion, the theme's `--ease-move` over `--duration-medium`, and at
/// once under Reduce Motion (upstream `applyLayoutChangeTransition`); without one, on the
/// component's own `duration` and `ease`. Reduce Motion stops any turn.
pub(crate) fn indicator_turn(
    key: impl Into<String>,
    open: bool,
    layout: Option<&AnimatedLayout>,
    own: (Duration, CubicBezier),
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    let (duration, ease) = match layout {
        Some(_) => {
            let motion = LayoutMotion::read(window, cx);
            let duration = if motion.reduced {
                Duration::ZERO
            } else {
                motion.duration
            };
            (duration, motion.ease)
        }
        None => own,
    };
    crate::motion::transition(key, open, duration, ease, window, cx)
}

struct Measured(Pixels);

/// A height that eases between none and the content's own, upstream's grid row
/// transition: the content stays at its own height under a clip at the moving edge, and
/// the clip goes once it is fully open. Under Reduce Motion it changes at once.
pub(crate) struct HeightTransition {
    open: bool,
    fraction: f32,
    height: Pixels,
    measured: Entity<Measured>,
}

/// The height transition of the content `key` names, `open` or not, over `duration`
/// on `ease`. Call it where the content renders, every frame.
pub(crate) fn use_height_transition(
    key: impl Into<String>,
    open: bool,
    duration: Duration,
    ease: CubicBezier,
    window: &mut Window,
    cx: &mut App,
) -> HeightTransition {
    let key = key.into();
    let fraction =
        crate::motion::transition(format!("{key}-height"), open, duration, ease, window, cx);
    let measured = window.use_keyed_state(
        ElementId::Name(format!("{key}-height-measured").into()),
        cx,
        |_, _| Measured(px(0.)),
    );
    let height = measured.read(cx).0;
    HeightTransition {
        open,
        fraction,
        height,
        measured,
    }
}

impl HeightTransition {
    /// Whether the height is moving.
    pub fn is_moving(&self) -> bool {
        let target = if self.open { 1. } else { 0. };
        (self.fraction - target).abs() > f32::EPSILON
    }

    /// Whether the content is drawn: open, or closing.
    pub fn mounted(&self) -> bool {
        self.open || self.is_moving()
    }

    /// `el` as the box of content that stays drawn while closed (CodeBlock's body): its
    /// own height when open at rest (measured then), the shown part of that height under
    /// a clip while it moves, and none when closed. Its children may shrink with it, as a
    /// grid row's `min-height: 0` content does.
    pub fn clip_box(&self, el: gpui::Div) -> gpui::Div {
        let measured = self.measured.clone();
        if self.is_moving() {
            return el
                .flex_none()
                .h(self.height * self.fraction)
                .min_h(px(0.))
                .overflow_hidden();
        }
        if !self.open {
            return el.flex_none().h(px(0.)).min_h(px(0.)).overflow_hidden();
        }
        el.on_children_prepainted(move |bounds, _, cx| {
            if let Some(b) = bounds.first() {
                let h = b.size.height;
                if measured.read(cx).0 != h {
                    measured.update(cx, |m, _| m.0 = h);
                }
            }
        })
    }

    /// Like [`wrap`](Self::wrap), but `content` is always there (a region a control
    /// names): the caller leaves its children out once it is closed at rest.
    pub fn wrap_kept(self, content: impl IntoElement) -> AnyElement {
        let measured = self.measured.clone();
        let moving = self.is_moving();
        div()
            .when(moving, |el| {
                el.h(self.height * self.fraction).overflow_hidden()
            })
            .child(content)
            .on_children_prepainted(move |bounds, _, cx| {
                if let Some(b) = bounds.first() {
                    let h = b.size.height;
                    if measured.read(cx).0 != h {
                        measured.update(cx, |m, _| m.0 = h);
                    }
                }
            })
            .into_any_element()
    }

    /// The content in its box: in flow when open at rest, under a clip while it moves,
    /// and an empty box when closed at rest (`content` is not built then).
    pub fn wrap(self, content: impl FnOnce() -> AnyElement) -> AnyElement {
        if !self.mounted() {
            return div().into_any_element();
        }
        let measured = self.measured.clone();
        let measure = move |bounds: Vec<Bounds<Pixels>>, _: &mut Window, cx: &mut App| {
            if let Some(b) = bounds.first() {
                let h = b.size.height;
                if measured.read(cx).0 != h {
                    measured.update(cx, |m, _| m.0 = h);
                }
            }
        };
        let moving = self.is_moving();
        // A block box: its child keeps its own height under the clip.
        div()
            .when(moving, |el| {
                el.h(self.height * self.fraction).overflow_hidden()
            })
            .child(content())
            .on_children_prepainted(measure)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Duration;

    use gpui::{IntoElement, TestAppContext, VisualTestContext, div};

    use super::*;
    use crate::animated_layout::use_animated_layout;
    use crate::segmented::tests::host;

    #[derive(IntoElement)]
    struct Turn {
        open: Rc<Cell<bool>>,
        handle: bool,
        seen: Rc<RefCell<f32>>,
    }

    impl gpui::RenderOnce for Turn {
        fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
            let layout = use_animated_layout("area", window, cx);
            let t = astryx_theme::current(window, cx).tokens().clone();
            let turn = indicator_turn(
                "chevron",
                self.open.get(),
                self.handle.then_some(&layout),
                (t.duration.fast, t.ease.standard),
                window,
                cx,
            );
            *self.seen.borrow_mut() = turn;
            div()
        }
    }

    fn settle(cx: &mut VisualTestContext) {
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update(|window, _| window.refresh());
        }
        cx.run_until_parked();
    }

    fn turned_after(cx: &mut TestAppContext, handle: bool, ms: u64) -> f32 {
        let (open, seen) = (Rc::new(Cell::new(false)), Rc::new(RefCell::new(0.)));
        let (o, s) = (open.clone(), seen.clone());
        let cx = host(cx, move || {
            Turn {
                open: o.clone(),
                handle,
                seen: s.clone(),
            }
            .into_any_element()
        });
        settle(cx);
        open.set(true);
        settle(cx);
        for _ in 0..(ms / 16) {
            cx.executor().advance_clock(Duration::from_millis(16));
            settle(cx);
        }
        *seen.borrow()
    }

    #[gpui::test]
    fn without_a_handle_an_indicator_turns_on_the_theme_s_fast_duration(cx: &mut TestAppContext) {
        // --duration-fast has passed: the turn is done.
        assert_eq!(turned_after(cx, false, 240), 1.);
    }

    #[gpui::test]
    fn with_a_handle_an_indicator_turns_on_the_glide_s_medium_duration(cx: &mut TestAppContext) {
        let turn = turned_after(cx, true, 240);
        assert!(turn > 0. && turn < 1., "still turning: {turn}");
    }

    #[gpui::test]
    fn with_a_handle_under_reduced_motion_an_indicator_turns_at_once(cx: &mut TestAppContext) {
        cx.update(|cx| astryx_primitives::set_reduced_motion(true, cx));
        assert_eq!(turned_after(cx, true, 0), 1.);
    }
}
