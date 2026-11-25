//! The focus ring, upstream `focusOutlineStyles`: a CSS outline drawn
//! `--focus-outline-offset` outside the element, `--focus-outline-width`
//! wide, following the element's corner radius.
//!
//! GPUI has no outline, so the ring is an absolutely positioned bordered box
//! as the element's last child; the element must be `relative`. Show it when
//! [`astryx_primitives::is_focus_visible`] holds.

use std::collections::VecDeque;

use astryx_theme::Tokens;
use gpui::{App, Bounds, Div, Global, Pixels, Styled, canvas, div, point, prelude::*, size};

/// Where the last focus rings were drawn: Core's keyboard hint anchors to
/// the focused control, and GPUI tells no element's bounds by its focus.
#[derive(Default)]
struct FocusRings {
    seq: u64,
    rings: VecDeque<(u64, Bounds<Pixels>)>,
}

impl Global for FocusRings {}

/// Records a ring's control: the ring's box less what it grows by.
fn probe(grow: Pixels) -> impl IntoElement {
    canvas(
        move |b, _, cx| {
            let control = Bounds::new(
                point(b.origin.x + grow, b.origin.y + grow),
                size(b.size.width - grow * 2., b.size.height - grow * 2.),
            );
            let rings = cx.default_global::<FocusRings>();
            rings.seq += 1;
            let seq = rings.seq;
            rings.rings.push_back((seq, control));
            while rings.rings.len() > 16 {
                rings.rings.pop_front();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The controls whose rings were drawn after `seq`, and the latest seq.
pub(crate) fn rings_since(seq: u64, cx: &App) -> (u64, Vec<Bounds<Pixels>>) {
    match cx.try_global::<FocusRings>() {
        Some(r) => (
            r.seq,
            r.rings
                .iter()
                .filter(|(s, _)| *s > seq)
                .map(|(_, b)| *b)
                .collect(),
        ),
        None => (seq, Vec::new()),
    }
}

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
        .rounded(radius + grow)
        .child(probe(grow));
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
        .rounded(radius)
        .child(probe(gpui::px(0.)));
    if t.focus.outline_style.0 == "dashed" {
        ring.border_dashed()
    } else {
        ring
    }
}
