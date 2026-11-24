//! Edge fades, upstream's `mask-image: linear-gradient(to bottom,
//! transparent, black 36px)` on scroll regions (Chat, TabList, Carousel,
//! DateInput).
//!
//! GPUI cannot mask content by alpha, so the fade is an overlay in the
//! surface's color: opaque at the edge, clear at `length`. Over a surface of
//! that color it looks the same as the mask.

use gpui::{Hsla, Pixels, Styled as _, px};

use crate::gradient::{Gradient, GradientDirection, StopPosition, gradient, stop};

/// The edge a fade sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FadeEdge {
    Top,
    Bottom,
    Left,
    Right,
}

/// An overlay that fades content into `surface` over `length` from `edge`.
/// It is absolutely positioned; put it in a `relative` container above the
/// content. It takes no pointer events.
pub fn edge_fade(edge: FadeEdge, length: Pixels, surface: Hsla) -> Gradient {
    let direction = match edge {
        FadeEdge::Top => GradientDirection::ToBottom,
        FadeEdge::Bottom => GradientDirection::ToTop,
        FadeEdge::Left => GradientDirection::ToRight,
        FadeEdge::Right => GradientDirection::ToLeft,
    };
    let fade = gradient(
        direction,
        [
            stop(surface, StopPosition::Fraction(0.)),
            stop(surface.alpha(0.), StopPosition::Fraction(1.)),
        ],
    )
    .absolute();
    match edge {
        FadeEdge::Top => fade.top_0().left_0().right_0().h(length),
        FadeEdge::Bottom => fade.bottom_0().left_0().right_0().h(length),
        FadeEdge::Left => fade.left_0().top_0().bottom_0().w(length),
        FadeEdge::Right => fade.right_0().top_0().bottom_0().w(length),
    }
}

/// Upstream's scroll-edge fade length: 36 px.
pub const SCROLL_FADE: Pixels = px(36.);
