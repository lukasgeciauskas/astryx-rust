//! Filled shapes, the counterpart of upstream's `clip-path: polygon(..)`
//! surfaces (layer arrows, stepper chevrons, token tails), drawn as GPUI
//! paths.

use std::rc::Rc;

use gpui::{
    App, Background, Bounds, IntoElement, PathBuilder, Pixels, Point, RenderOnce, StyleRefinement,
    Styled, Window, canvas, div, point, prelude::*,
};

/// A polygon whose corners are fractions of the element's box: `(0, 0)` is
/// the top left, `(1, 1)` the bottom right, as CSS `polygon(0% 0%, ..)`.
#[derive(IntoElement)]
pub struct Shape {
    corners: Rc<[Point<f32>]>,
    fill: Background,
    style: StyleRefinement,
}

/// A polygon filled with `fill`.
pub fn polygon(
    corners: impl IntoIterator<Item = Point<f32>>,
    fill: impl Into<Background>,
) -> Shape {
    Shape {
        corners: corners.into_iter().collect(),
        fill: fill.into(),
        style: StyleRefinement::default(),
    }
}

/// Which way a triangle points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointing {
    Up,
    Down,
    Left,
    Right,
}

/// A triangle filling its box and pointing one way, as a layer's arrow.
pub fn triangle(pointing: Pointing, fill: impl Into<Background>) -> Shape {
    let corners = match pointing {
        Pointing::Up => [point(0., 1.), point(0.5, 0.), point(1., 1.)],
        Pointing::Down => [point(0., 0.), point(1., 0.), point(0.5, 1.)],
        Pointing::Left => [point(1., 0.), point(1., 1.), point(0., 0.5)],
        Pointing::Right => [point(0., 0.), point(1., 0.5), point(0., 1.)],
    };
    polygon(corners, fill)
}

/// The polygon's corners in window coordinates.
pub fn polygon_points(corners: &[Point<f32>], bounds: Bounds<Pixels>) -> Vec<Point<Pixels>> {
    corners
        .iter()
        .map(|c| {
            point(
                bounds.left() + bounds.size.width * c.x,
                bounds.top() + bounds.size.height * c.y,
            )
        })
        .collect()
}

/// Paints a polygon into `bounds`. Call during paint.
pub fn paint_polygon(
    corners: &[Point<f32>],
    bounds: Bounds<Pixels>,
    fill: Background,
    window: &mut Window,
) {
    let points = polygon_points(corners, bounds);
    let Some((first, rest)) = points.split_first() else {
        return;
    };
    let mut builder = PathBuilder::fill();
    builder.move_to(*first);
    for p in rest {
        builder.line_to(*p);
    }
    builder.close();
    if let Ok(path) = builder.build() {
        window.paint_path(path, fill);
    }
}

impl Styled for Shape {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Shape {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (corners, fill) = (self.corners, self.fill);
        let mut root = div();
        *root.style() = self.style;
        root.child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| paint_polygon(&corners, bounds, fill, window),
            )
            .size_full(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size};

    #[test]
    fn corners_scale_to_the_box() {
        let bounds = Bounds::new(point(px(10.), px(20.)), size(px(16.), px(8.)));
        assert_eq!(
            polygon_points(&[point(0., 1.), point(0.5, 0.), point(1., 1.)], bounds),
            [
                point(px(10.), px(28.)),
                point(px(18.), px(20.)),
                point(px(26.), px(28.))
            ]
        );
    }
}
