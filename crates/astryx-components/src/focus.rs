//! The focus ring, upstream `focusOutlineStyles`: a CSS outline drawn
//! `--focus-outline-offset` outside the element, `--focus-outline-width`
//! wide, following the element's corner radius.
//!
//! GPUI has no outline, so the ring is an absolutely positioned bordered box
//! as the element's last child; the element must be `relative`. Show it when
//! [`astryx_primitives::is_focus_visible`] holds.

use astryx_theme::Tokens;
use gpui::{Div, Pixels, Styled, div};

/// The ring for an element whose corner radius is `radius`.
pub fn focus_ring(t: &Tokens, radius: Pixels, rem: Pixels) -> Div {
    let width = t.focus.outline_width.to_pixels(rem);
    let offset = t.focus.outline_offset.to_pixels(rem);
    let grow = width + offset;
    let ring = div()
        .absolute()
        .top(-grow)
        .left(-grow)
        .right(-grow)
        .bottom(-grow)
        .border(width)
        .border_color(t.focus.outline_color)
        .rounded(radius + grow);
    if t.focus.outline_style.0 == "dashed" {
        ring.border_dashed()
    } else {
        ring
    }
}

/// The ring drawn inside an element by its own width, for rows a menu or
/// list paints edge to edge (upstream's negative outline offset).
pub fn focus_ring_inset(t: &Tokens, radius: Pixels, rem: Pixels) -> Div {
    let width = t.focus.outline_width.to_pixels(rem);
    let ring = div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .border(width)
        .border_color(t.focus.outline_color)
        .rounded(radius);
    if t.focus.outline_style.0 == "dashed" {
        ring.border_dashed()
    } else {
        ring
    }
}
