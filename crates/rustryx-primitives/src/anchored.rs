//! Where an anchored layer goes, upstream's CSS anchor positioning with its
//! `position-try` fallbacks: on the requested side of its anchor, flipped to
//! the opposite side when only that one fits, and slid along its side to stay
//! inside the window. [`crate::layer::anchored_layer`] draws one.

use gpui::{Bounds, Pixels, Point, Size, point, px};

/// Which side of its anchor a layer sits on (upstream `placement`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Placement {
    #[default]
    Above,
    Below,
    /// Before the anchor in the reading direction.
    Start,
    /// After the anchor in the reading direction.
    End,
}

impl Placement {
    pub fn as_str(self) -> &'static str {
        match self {
            Placement::Above => "above",
            Placement::Below => "below",
            Placement::Start => "start",
            Placement::End => "end",
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Placement::Above => Placement::Below,
            Placement::Below => Placement::Above,
            Placement::Start => Placement::End,
            Placement::End => Placement::Start,
        }
    }

    /// Whether the layer sits above or below its anchor.
    pub fn is_vertical(self) -> bool {
        matches!(self, Placement::Above | Placement::Below)
    }
}

/// How a layer lines up with its anchor along its side (upstream
/// `alignment`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LayerAlign {
    /// Its start edge meets the anchor's start edge.
    Start,
    #[default]
    Center,
    End,
}

impl LayerAlign {
    pub fn as_str(self) -> &'static str {
        match self {
            LayerAlign::Start => "start",
            LayerAlign::Center => "center",
            LayerAlign::End => "end",
        }
    }
}

/// Where a layer landed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerPosition {
    pub origin: Point<Pixels>,
    /// The side it landed on, after any flip.
    pub side: Placement,
}

/// How a layer is placed.
#[derive(Clone, Copy, Debug)]
pub struct LayerOptions {
    pub placement: Placement,
    pub align: LayerAlign,
    /// The gap between the anchor and the layer.
    pub offset: Pixels,
    /// The margin the layer keeps from the window's edges.
    pub margin: Pixels,
    /// Right-to-left: start and end swap.
    pub rtl: bool,
    /// Deferred draw priority: higher draws later, on top.
    pub priority: usize,
}

impl Default for LayerOptions {
    fn default() -> Self {
        LayerOptions {
            placement: Placement::Above,
            align: LayerAlign::Center,
            offset: px(4.),
            margin: px(8.),
            rtl: false,
            priority: 1,
        }
    }
}

/// The origin of a `layer` of `layer` size on `side` of `anchor`, before any
/// fitting.
fn place(
    anchor: Bounds<Pixels>,
    layer: Size<Pixels>,
    side: Placement,
    options: &LayerOptions,
) -> Point<Pixels> {
    let offset = options.offset;
    // Start and end in the reading direction, as physical left and right.
    let (left_side, right_side) = if options.rtl {
        (Placement::End, Placement::Start)
    } else {
        (Placement::Start, Placement::End)
    };
    let align_on = |start: Pixels, length: Pixels, extent: Pixels, flip: bool| -> Pixels {
        let align = match (options.align, flip) {
            (LayerAlign::Start, true) => LayerAlign::End,
            (LayerAlign::End, true) => LayerAlign::Start,
            (align, _) => align,
        };
        match align {
            LayerAlign::Start => start,
            LayerAlign::Center => start + (length - extent) / 2.,
            LayerAlign::End => start + length - extent,
        }
    };
    match side {
        Placement::Above | Placement::Below => {
            let x = align_on(anchor.origin.x, anchor.size.width, layer.width, options.rtl);
            let y = if side == Placement::Above {
                anchor.origin.y - offset - layer.height
            } else {
                anchor.origin.y + anchor.size.height + offset
            };
            point(x, y)
        }
        side => {
            let y = align_on(anchor.origin.y, anchor.size.height, layer.height, false);
            let x = if side == left_side {
                anchor.origin.x - offset - layer.width
            } else {
                debug_assert_eq!(side, right_side);
                anchor.origin.x + anchor.size.width + offset
            };
            point(x, y)
        }
    }
}

/// Places a layer next to its anchor inside `viewport`: on the requested
/// side, or the opposite one when only that one fits, then slid along the
/// side (and clamped across it) to stay inside the viewport's margins.
pub fn position_layer(
    anchor: Bounds<Pixels>,
    layer: Size<Pixels>,
    viewport: Bounds<Pixels>,
    options: &LayerOptions,
) -> LayerPosition {
    let inner = Bounds::from_corners(
        viewport.origin + point(options.margin, options.margin),
        viewport.bottom_right() - point(options.margin, options.margin),
    );
    let fits = |origin: Point<Pixels>, side: Placement| -> bool {
        if side.is_vertical() {
            origin.y >= inner.origin.y && origin.y + layer.height <= inner.bottom()
        } else {
            origin.x >= inner.origin.x && origin.x + layer.width <= inner.right()
        }
    };
    let mut side = options.placement;
    let mut origin = place(anchor, layer, side, options);
    if !fits(origin, side) {
        let flipped = side.opposite();
        let flipped_origin = place(anchor, layer, flipped, options);
        if fits(flipped_origin, flipped) {
            side = flipped;
            origin = flipped_origin;
        }
    }
    let clamp = |value: Pixels, low: Pixels, high: Pixels, extent: Pixels| -> Pixels {
        if extent >= high - low {
            low
        } else {
            value.clamp(low, high - extent)
        }
    };
    origin.x = clamp(origin.x, inner.origin.x, inner.right(), layer.width);
    origin.y = clamp(origin.y, inner.origin.y, inner.bottom(), layer.height);
    LayerPosition { origin, side }
}

#[cfg(test)]
mod tests {
    use gpui::size;

    use super::*;

    fn anchor() -> Bounds<Pixels> {
        Bounds::new(point(px(100.), px(100.)), size(px(40.), px(20.)))
    }

    fn viewport() -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(0.)), size(px(400.), px(300.)))
    }

    fn options(placement: Placement, align: LayerAlign) -> LayerOptions {
        LayerOptions {
            placement,
            align,
            offset: px(4.),
            margin: px(8.),
            rtl: false,
            priority: 1,
        }
    }

    #[test]
    fn layers_sit_on_their_side_and_align() {
        let layer = size(px(60.), px(30.));
        let above = position_layer(
            anchor(),
            layer,
            viewport(),
            &options(Placement::Above, LayerAlign::Center),
        );
        assert_eq!(above.origin, point(px(90.), px(66.)));
        assert_eq!(above.side, Placement::Above);
        let below = position_layer(
            anchor(),
            layer,
            viewport(),
            &options(Placement::Below, LayerAlign::Start),
        );
        assert_eq!(below.origin, point(px(100.), px(124.)));
        let end = position_layer(
            anchor(),
            layer,
            viewport(),
            &options(Placement::End, LayerAlign::End),
        );
        assert_eq!(end.origin, point(px(144.), px(90.)));
    }

    #[test]
    fn a_layer_that_does_not_fit_flips() {
        // Too close to the top for 120px above.
        let layer = size(px(60.), px(120.));
        let placed = position_layer(
            anchor(),
            layer,
            viewport(),
            &options(Placement::Above, LayerAlign::Center),
        );
        assert_eq!(placed.side, Placement::Below);
        assert_eq!(placed.origin.y, px(124.));
    }

    #[test]
    fn a_layer_slides_inside_the_window() {
        let near_edge = Bounds::new(point(px(2.), px(100.)), size(px(20.), px(20.)));
        let placed = position_layer(
            near_edge,
            size(px(100.), px(30.)),
            viewport(),
            &options(Placement::Above, LayerAlign::Center),
        );
        assert_eq!(placed.origin.x, px(8.), "kept inside the margin");
    }

    #[test]
    fn start_and_end_follow_the_reading_direction() {
        let layer = size(px(60.), px(30.));
        let mut rtl = options(Placement::Start, LayerAlign::Center);
        rtl.rtl = true;
        let start = position_layer(anchor(), layer, viewport(), &rtl);
        assert_eq!(start.origin.x, px(144.), "start is on the right in RTL");
        let mut below = options(Placement::Below, LayerAlign::Start);
        below.rtl = true;
        let aligned = position_layer(anchor(), layer, viewport(), &below);
        assert_eq!(
            aligned.origin.x,
            px(80.),
            "start-aligned to the anchor's right edge"
        );
    }
}
