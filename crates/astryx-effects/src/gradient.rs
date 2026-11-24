//! Linear gradients with any number of stops at lengths or fractions, CSS
//! `linear-gradient(to right, transparent 0%, rgba(0,0,0,.3) 2px, black
//! 8px, black calc(100% - 8px), ..)`. GPUI draws two-stop gradients; this
//! draws one per pair of adjacent stops, side by side, which is exact for
//! the four axis directions upstream uses.

use std::rc::Rc;

use gpui::{
    App, Bounds, Corners, Hsla, IntoElement, Pixels, RenderOnce, StyleRefinement, Styled, Window,
    canvas, div, fill, linear_color_stop, linear_gradient, point, prelude::*, px, size,
};
use smallvec::SmallVec;

/// Where a gradient runs, CSS `to right` and friends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientDirection {
    ToTop,
    ToRight,
    ToBottom,
    ToLeft,
}

impl GradientDirection {
    /// The CSS angle in degrees, as GPUI's `linear_gradient` takes it.
    fn angle(self) -> f32 {
        match self {
            GradientDirection::ToTop => 0.,
            GradientDirection::ToRight => 90.,
            GradientDirection::ToBottom => 180.,
            GradientDirection::ToLeft => 270.,
        }
    }

    /// The direction mirrored left to right, for right-to-left layouts.
    pub fn mirrored(self) -> Self {
        match self {
            GradientDirection::ToRight => GradientDirection::ToLeft,
            GradientDirection::ToLeft => GradientDirection::ToRight,
            other => other,
        }
    }

    fn horizontal(self) -> bool {
        matches!(self, GradientDirection::ToRight | GradientDirection::ToLeft)
    }
}

/// A stop's place along the gradient line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StopPosition {
    /// A fraction of the line, CSS `40%`.
    Fraction(f32),
    /// A length from the start, CSS `36px`.
    Start(Pixels),
    /// A length back from the end, CSS `calc(100% - 8px)`.
    End(Pixels),
    /// Spaced evenly between its neighbors, CSS's omitted position.
    Auto,
}

/// One color stop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub color: Hsla,
    pub position: StopPosition,
}

/// A stop of `color` at `position`.
pub fn stop(color: impl Into<Hsla>, position: StopPosition) -> GradientStop {
    GradientStop {
        color: color.into(),
        position,
    }
}

/// Resolves stop positions on a line of `length`, as CSS does: the first and
/// last `Auto` stops sit at the ends, the other `Auto` stops spread evenly
/// between their neighbors, and no stop comes before the one ahead of it.
pub fn resolve_positions(stops: &[GradientStop], length: Pixels) -> SmallVec<[Pixels; 8]> {
    let n = stops.len();
    let mut resolved: SmallVec<[Option<Pixels>; 8]> = stops
        .iter()
        .map(|stop| match stop.position {
            StopPosition::Fraction(f) => Some(length * f),
            StopPosition::Start(p) => Some(p),
            StopPosition::End(p) => Some(length - p),
            StopPosition::Auto => None,
        })
        .collect();
    if n == 0 {
        return SmallVec::new();
    }
    if resolved[0].is_none() {
        resolved[0] = Some(px(0.));
    }
    if resolved[n - 1].is_none() {
        resolved[n - 1] = Some(length);
    }
    // Monotonic: clamp each fixed position to the largest before it.
    let mut max = px(f32::NEG_INFINITY);
    for position in resolved.iter_mut().flatten() {
        if *position < max {
            *position = max;
        }
        max = *position;
    }
    // Spread runs of `Auto` stops between the fixed stops around them.
    let mut i = 0;
    while i < n {
        if resolved[i].is_some() {
            i += 1;
            continue;
        }
        let start = i - 1;
        let mut end = i;
        while resolved[end].is_none() {
            end += 1;
        }
        let (a, b) = (resolved[start].unwrap(), resolved[end].unwrap());
        let steps = (end - start) as f32;
        for (k, slot) in resolved[start + 1..end].iter_mut().enumerate() {
            *slot = Some(a + (b - a) * ((k + 1) as f32 / steps));
        }
        i = end;
    }
    resolved.into_iter().map(Option::unwrap).collect()
}

/// One band of a gradient: a rectangle filled from `from` to `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub bounds: Bounds<Pixels>,
    pub from: Hsla,
    pub to: Hsla,
}

/// CSS interpolates in premultiplied alpha, so a fully transparent stop
/// takes its neighbor's color: `transparent` to white never turns gray.
fn premultiplied_ends(from: Hsla, to: Hsla) -> (Hsla, Hsla) {
    match (from.a == 0., to.a == 0.) {
        (true, false) => (to.alpha(0.), to),
        (false, true) => (from, from.alpha(0.)),
        _ => (from, to),
    }
}

/// The bands that draw a gradient in `bounds`, in order along the line:
/// solid before the first stop and after the last, a two-stop gradient
/// between each pair.
pub fn bands(
    bounds: Bounds<Pixels>,
    direction: GradientDirection,
    stops: &[GradientStop],
) -> SmallVec<[Band; 8]> {
    let mut out = SmallVec::new();
    if stops.is_empty() {
        return out;
    }
    let length = if direction.horizontal() {
        bounds.size.width
    } else {
        bounds.size.height
    };
    let positions = resolve_positions(stops, length);
    // A band from `a` to `b` along the line, in window coordinates.
    let rect = |a: Pixels, b: Pixels| -> Bounds<Pixels> {
        let (a, b) = (a.clamp(px(0.), length), b.clamp(px(0.), length));
        match direction {
            GradientDirection::ToRight => Bounds::new(
                point(bounds.left() + a, bounds.top()),
                size(b - a, bounds.size.height),
            ),
            GradientDirection::ToLeft => Bounds::new(
                point(bounds.right() - b, bounds.top()),
                size(b - a, bounds.size.height),
            ),
            GradientDirection::ToBottom => Bounds::new(
                point(bounds.left(), bounds.top() + a),
                size(bounds.size.width, b - a),
            ),
            GradientDirection::ToTop => Bounds::new(
                point(bounds.left(), bounds.bottom() - b),
                size(bounds.size.width, b - a),
            ),
        }
    };
    let mut push = |a: Pixels, b: Pixels, from: Hsla, to: Hsla| {
        if b > a && a < length && b > px(0.) {
            let (from, to) = premultiplied_ends(from, to);
            out.push(Band {
                bounds: rect(a, b),
                from,
                to,
            });
        }
    };
    let first = stops[0].color;
    push(px(0.), positions[0], first, first);
    for i in 0..stops.len() - 1 {
        push(
            positions[i],
            positions[i + 1],
            stops[i].color,
            stops[i + 1].color,
        );
    }
    let last = stops[stops.len() - 1].color;
    push(positions[stops.len() - 1], length, last, last);
    out
}

/// The radii of `band`'s corners that sit on `bounds`' corners, so a
/// rounded gradient keeps its outline across bands.
fn band_radii(
    band: Bounds<Pixels>,
    bounds: Bounds<Pixels>,
    radii: Corners<Pixels>,
) -> Corners<Pixels> {
    let near = |a: Pixels, b: Pixels| (a - b).abs() < px(0.01);
    let (top, bottom) = (
        near(band.top(), bounds.top()),
        near(band.bottom(), bounds.bottom()),
    );
    let (left, right) = (
        near(band.left(), bounds.left()),
        near(band.right(), bounds.right()),
    );
    Corners {
        top_left: if top && left { radii.top_left } else { px(0.) },
        top_right: if top && right {
            radii.top_right
        } else {
            px(0.)
        },
        bottom_right: if bottom && right {
            radii.bottom_right
        } else {
            px(0.)
        },
        bottom_left: if bottom && left {
            radii.bottom_left
        } else {
            px(0.)
        },
    }
    .clamp_radii_for_quad_size(band.size)
}

/// Paints a gradient into `bounds`, rounded by `radii`. Call during paint.
pub fn paint_gradient(
    bounds: Bounds<Pixels>,
    direction: GradientDirection,
    stops: &[GradientStop],
    radii: Corners<Pixels>,
    window: &mut Window,
) {
    for band in bands(bounds, direction, stops) {
        let background = if band.from == band.to {
            band.from.into()
        } else {
            linear_gradient(
                direction.angle(),
                linear_color_stop(band.from, 0.),
                linear_color_stop(band.to, 1.),
            )
        };
        window.paint_quad(fill(band.bounds, background).corner_radii(band_radii(
            band.bounds,
            bounds,
            radii,
        )));
    }
}

/// An element filled with a multi-stop gradient. Size, place and round it
/// with `Styled` methods.
#[derive(IntoElement)]
pub struct Gradient {
    direction: GradientDirection,
    stops: Rc<[GradientStop]>,
    style: StyleRefinement,
}

/// A gradient element, CSS `background-image: linear-gradient(..)`.
pub fn gradient(
    direction: GradientDirection,
    stops: impl IntoIterator<Item = GradientStop>,
) -> Gradient {
    Gradient {
        direction,
        stops: stops.into_iter().collect(),
        style: StyleRefinement::default(),
    }
}

impl Styled for Gradient {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Gradient {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let (direction, stops) = (self.direction, self.stops);
        let rem = window.rem_size();
        let radius = |r: Option<gpui::AbsoluteLength>| r.map_or(px(0.), |r| r.to_pixels(rem));
        let corners = &self.style.corner_radii;
        let radii = Corners {
            top_left: radius(corners.top_left),
            top_right: radius(corners.top_right),
            bottom_right: radius(corners.bottom_right),
            bottom_left: radius(corners.bottom_left),
        };
        let mut root = div();
        *root.style() = self.style;
        root.child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    paint_gradient(bounds, direction, &stops, radii, window)
                },
            )
            .size_full(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{black, white};

    fn transparent() -> Hsla {
        Hsla::transparent_black()
    }

    #[test]
    fn positions_follow_css() {
        let stops = [
            stop(black(), StopPosition::Auto),
            stop(black(), StopPosition::Start(px(10.))),
            stop(black(), StopPosition::Auto),
            stop(black(), StopPosition::End(px(10.))),
            stop(black(), StopPosition::Auto),
        ];
        assert_eq!(
            &resolve_positions(&stops, px(100.))[..],
            &[px(0.), px(10.), px(50.), px(90.), px(100.)]
        );
        // A stop before an earlier one moves up to it.
        let stops = [
            stop(black(), StopPosition::Fraction(0.5)),
            stop(black(), StopPosition::Start(px(10.))),
        ];
        assert_eq!(
            &resolve_positions(&stops, px(100.))[..],
            &[px(50.), px(50.)]
        );
    }

    #[test]
    fn carousel_edge_mask_bands() {
        // linear-gradient(to right, transparent 0%, rgba(0,0,0,.3) 2px,
        // black 8px, black calc(100% - 8px), rgba(0,0,0,.3) calc(100% - 2px),
        // transparent 100%)
        let dim = black().alpha(0.3);
        let stops = [
            stop(transparent(), StopPosition::Fraction(0.)),
            stop(dim, StopPosition::Start(px(2.))),
            stop(black(), StopPosition::Start(px(8.))),
            stop(black(), StopPosition::End(px(8.))),
            stop(dim, StopPosition::End(px(2.))),
            stop(transparent(), StopPosition::Fraction(1.)),
        ];
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(20.)));
        let bands = bands(bounds, GradientDirection::ToRight, &stops);
        let spans: Vec<(f32, f32)> = bands
            .iter()
            .map(|b| (b.bounds.left().into(), b.bounds.right().into()))
            .collect();
        assert_eq!(
            spans,
            [(0., 2.), (2., 8.), (8., 92.), (92., 98.), (98., 100.)]
        );
        // The transparent end takes its neighbor's color at zero alpha.
        assert_eq!(bands[0].from, dim.alpha(0.));
    }

    #[test]
    fn reversed_directions_measure_from_the_far_edge() {
        let stops = [
            stop(white(), StopPosition::Fraction(0.)),
            stop(black(), StopPosition::Start(px(30.))),
        ];
        let bounds = Bounds::new(point(px(10.), px(10.)), size(px(50.), px(100.)));
        let up = bands(bounds, GradientDirection::ToTop, &stops);
        // From the bottom edge (110) up 30px, then solid black to the top.
        assert_eq!(
            up[0].bounds,
            Bounds::new(point(px(10.), px(80.)), size(px(50.), px(30.)))
        );
        assert_eq!(
            up[1].bounds,
            Bounds::new(point(px(10.), px(10.)), size(px(50.), px(70.)))
        );
        assert_eq!((up[1].from, up[1].to), (black(), black()));
    }
}
