//! Edge fades, upstream's `mask-image: linear-gradient(to bottom,
//! transparent, black 36px)` on scroll regions (Chat, TabList, Carousel,
//! DateInput).
//!
//! [`edge_mask`] is the mask itself, an alpha gradient over the element's box
//! drawn through an offscreen layer (fork patch R3's `mask_linear`), so the
//! content fades to whatever lies behind it. A renderer without offscreen
//! layers (Metal) draws no mask; there [`edge_fade`] stands in: an overlay in
//! the surface's color, opaque at the edge and clear at `length`, which looks
//! the same as the mask over a surface of that color.

use gpui::{Hsla, LinearMask, MaskStop, Pixels, Styled as _, px};

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

/// One stop of an edge's fade, counted from the edge inward: the mask's alpha
/// at `offset` from it. An edge's last stop is where the content shows in full.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeStop {
    pub offset: Pixels,
    pub alpha: f32,
}

/// An edge stop.
pub const fn edge_stop(offset: Pixels, alpha: f32) -> EdgeStop {
    EdgeStop { offset, alpha }
}

/// The plain fade: clear at the edge (after an `inset` band), shown in full
/// `length` further in (upstream `transparent <inset>, black <inset + length>`).
pub fn edge_fade_stops(inset: Pixels, length: Pixels) -> Vec<EdgeStop> {
    vec![edge_stop(inset, 0.), edge_stop(inset + length, 1.)]
}

/// The axis an [`edge_mask`] runs along, and which way it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskAxis {
    /// From the left edge to the right (CSS `to right`).
    LeftToRight,
    /// From the right edge to the left (`to left`).
    RightToLeft,
    /// From the top edge down (`to bottom`).
    TopToBottom,
}

/// Upstream's edge `mask-image`: `start` fades the edge the axis starts at and
/// `end` the far one, each by its stops from that edge inward; `None` leaves
/// the edge shown. Apply it with `mask_linear` (fork patch R3).
pub fn edge_mask(
    axis: MaskAxis,
    start: Option<&[EdgeStop]>,
    end: Option<&[EdgeStop]>,
) -> LinearMask {
    let angle = match axis {
        MaskAxis::LeftToRight => 90.,
        MaskAxis::RightToLeft => 270.,
        MaskAxis::TopToBottom => 180.,
    };
    let mut mask = LinearMask::new(angle);
    match start {
        Some(stops) => {
            for s in stops {
                mask = mask.stop(MaskStop::px(s.offset, s.alpha));
            }
        }
        None => mask = mask.stop(MaskStop::px(px(0.), 1.)),
    }
    match end {
        Some(stops) => {
            for s in stops.iter().rev() {
                mask = mask.stop(MaskStop::from_end(s.offset, s.alpha));
            }
        }
        None => mask = mask.stop(MaskStop::from_end(px(0.), 1.)),
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edge_mask_fades_only_the_edges_it_names() {
        let both = edge_mask(
            MaskAxis::LeftToRight,
            Some(&edge_fade_stops(px(0.), px(36.))),
            Some(&edge_fade_stops(px(0.), px(36.))),
        );
        assert_eq!(both.angle, 90.);
        assert_eq!(
            both.stops,
            vec![
                MaskStop::px(px(0.), 0.),
                MaskStop::px(px(36.), 1.),
                MaskStop::from_end(px(36.), 1.),
                MaskStop::from_end(px(0.), 0.),
            ]
        );
        let start = edge_mask(
            MaskAxis::RightToLeft,
            Some(&edge_fade_stops(px(4.), px(8.))),
            None,
        );
        assert_eq!(start.angle, 270.);
        assert_eq!(start.stops.last(), Some(&MaskStop::from_end(px(0.), 1.)));
        assert_eq!(start.stops[0], MaskStop::px(px(4.), 0.));
    }
}
