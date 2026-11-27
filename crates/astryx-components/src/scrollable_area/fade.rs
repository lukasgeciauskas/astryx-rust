//! Scroll-edge fades, upstream `hooks/scrollEdgeFade.ts` (AST-025 FR22): each inline
//! edge that hides content fades. With its scrollbar showing, a viewport fades only
//! under a coarse pointer, where the bar overlays the content; a viewport that hides
//! its bar fades under every pointer. A keyboard focus ring turns the fades off, so they
//! never cover it.
//!
//! The fade is upstream's alpha mask on the viewport (fork patch R3's `mask_linear`),
//! so the content fades to whatever lies behind it. A renderer without offscreen layers
//! (Metal) draws no mask; there each edge gets an overlay in `background-body` instead.

use astryx_effects::{EdgeStop, FadeEdge, MaskAxis, edge_fade, edge_mask};
use astryx_i18n::Direction;
use astryx_primitives::scroll_area::ScrollAxisState;
use astryx_theme::Tokens;
use gpui::{
    AnyElement, Div, InteractiveElement, IntoElement, LinearMask, ParentElement, ScrollHandle,
    Styled, div, px,
};

/// Upstream `ScrollEdgeFadeScrollbar`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FadeScrollbar {
    /// The viewport shows its scrollbar: fades only under a coarse pointer.
    Native,
    /// The viewport hides it: fades under every pointer.
    Hidden,
}

/// An overlay box over a scroll viewport's visible area. GPUI moves a scrolling
/// element's absolute children with its content, so this one moves back by the
/// offset.
pub(crate) fn visible_box(handle: &ScrollHandle) -> Div {
    let offset = handle.offset();
    let size = handle.bounds().size;
    div()
        .absolute()
        .left(-offset.x)
        .top(-offset.y)
        .w(size.width)
        .h(size.height)
}

/// One viewport's fades: the mask for the viewport itself, and the boxes that mark (or,
/// without offscreen layers, draw) each fading edge, to place over its visible area.
#[derive(Default)]
pub(crate) struct ScrollEdgeFades {
    pub mask: Option<LinearMask>,
    pub edges: Vec<AnyElement>,
}

impl ScrollEdgeFades {
    /// Masks `el`, the viewport, when an edge fades.
    pub fn mask<E: Styled>(&self, el: E) -> E {
        match &self.mask {
            Some(mask) => el.mask_linear(mask.clone()),
            None => el,
        }
    }
}

/// The fades for one viewport. `fade` is an edge's stops from the edge inward (upstream
/// `transparent <inset>, black <inset + length>`, or Carousel's own). `off` turns an edge
/// off where something else marks its overflow (upstream `scrollEdgeFadeOff`, Table's
/// pinned columns). `layers`: the renderer draws masks (`supports_offscreen_layers`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn scroll_edge_fades(
    id: &str,
    inline: ScrollAxisState,
    scrollbar: FadeScrollbar,
    coarse: bool,
    focus_visible: bool,
    off: (bool, bool),
    fade: &[EdgeStop],
    dir: Direction,
    layers: bool,
    t: &Tokens,
) -> ScrollEdgeFades {
    if !inline.is_scrollable
        || (inline.at_start && inline.at_end)
        || focus_visible
        || (scrollbar == FadeScrollbar::Native && !coarse)
    {
        return ScrollEdgeFades::default();
    }
    let length = fade.last().map_or(px(0.), |s| s.offset);
    let surface = t.color.background_body;
    let (start, end) = if dir.is_rtl() {
        (FadeEdge::Right, FadeEdge::Left)
    } else {
        (FadeEdge::Left, FadeEdge::Right)
    };
    let shows = (!inline.at_start && !off.0, !inline.at_end && !off.1);
    let mut edges = Vec::new();
    for (shows, edge, name) in [(shows.0, start, "start"), (shows.1, end, "end")] {
        if shows {
            let selector = format!("{id}-fade-{name}");
            let mut band = div().debug_selector(move || selector).absolute().inset_0();
            if !layers {
                band = band.child(edge_fade(edge, length, surface));
            }
            edges.push(band.into_any_element());
        }
    }
    let axis = if dir.is_rtl() {
        MaskAxis::RightToLeft
    } else {
        MaskAxis::LeftToRight
    };
    let mask = (layers && (shows.0 || shows.1))
        .then(|| edge_mask(axis, shows.0.then_some(fade), shows.1.then_some(fade)));
    ScrollEdgeFades { mask, edges }
}
