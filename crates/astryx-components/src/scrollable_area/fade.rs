//! Scroll-edge fades, upstream `hooks/scrollEdgeFade.ts` (AST-025 FR22): each inline
//! edge that hides content fades toward the surface. With its scrollbar showing, a
//! viewport fades only under a coarse pointer, where the bar overlays the content;
//! a viewport that hides its bar fades under every pointer. A keyboard focus ring
//! turns the fades off, so they never cover it.

use astryx_effects::{FadeEdge, edge_fade};
use astryx_i18n::Direction;
use astryx_primitives::scroll_area::ScrollAxisState;
use astryx_theme::Tokens;
use gpui::{
    AnyElement, Div, InteractiveElement, IntoElement, ParentElement, Pixels, ScrollHandle, Styled,
    div,
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

/// The fades for one viewport. `off` turns an edge off where something else marks
/// its overflow (upstream `scrollEdgeFadeOff`, Table's pinned columns).
#[allow(clippy::too_many_arguments)]
pub(crate) fn scroll_edge_fades(
    id: &str,
    inline: ScrollAxisState,
    scrollbar: FadeScrollbar,
    coarse: bool,
    focus_visible: bool,
    off: (bool, bool),
    length: Pixels,
    dir: Direction,
    t: &Tokens,
) -> Vec<AnyElement> {
    if !inline.is_scrollable
        || (inline.at_start && inline.at_end)
        || focus_visible
        || (scrollbar == FadeScrollbar::Native && !coarse)
    {
        return Vec::new();
    }
    let surface = t.color.background_body;
    let (start, end) = if dir.is_rtl() {
        (FadeEdge::Right, FadeEdge::Left)
    } else {
        (FadeEdge::Left, FadeEdge::Right)
    };
    let mut out = Vec::new();
    for (shows, edge, name) in [
        (!inline.at_start && !off.0, start, "start"),
        (!inline.at_end && !off.1, end, "end"),
    ] {
        if shows {
            let selector = format!("{id}-fade-{name}");
            out.push(
                div()
                    .debug_selector(move || selector)
                    .absolute()
                    .inset_0()
                    .child(edge_fade(edge, length, surface))
                    .into_any_element(),
            );
        }
    }
    out
}
