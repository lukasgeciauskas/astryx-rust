//! Logical (start/end) styles, the counterpart of CSS's `padding-inline-start`,
//! `inset-inline-end`, `text-align: start` and RTL flex rows. GPUI styles are
//! physical (left/right), so each method takes the [`Direction`] in effect,
//! read with [`crate::use_direction`] during render.

use gpui::{AbsoluteLength, DefiniteLength, FlexDirection, Length, Styled, TextAlign};

use crate::Direction;

/// Start/end styles that mirror in right-to-left layouts.
pub trait LogicalStyled: Styled + Sized {
    /// Padding at the start side, CSS `padding-inline-start`.
    fn padding_start(mut self, dir: Direction, length: impl Into<DefiniteLength>) -> Self {
        let edges = &mut self.style().padding;
        let side = if dir.is_rtl() {
            &mut edges.right
        } else {
            &mut edges.left
        };
        *side = Some(length.into());
        self
    }

    /// Padding at the end side, CSS `padding-inline-end`.
    fn padding_end(mut self, dir: Direction, length: impl Into<DefiniteLength>) -> Self {
        let edges = &mut self.style().padding;
        let side = if dir.is_rtl() {
            &mut edges.left
        } else {
            &mut edges.right
        };
        *side = Some(length.into());
        self
    }

    /// Margin at the start side, CSS `margin-inline-start`.
    fn margin_start(mut self, dir: Direction, length: impl Into<Length>) -> Self {
        let edges = &mut self.style().margin;
        let side = if dir.is_rtl() {
            &mut edges.right
        } else {
            &mut edges.left
        };
        *side = Some(length.into());
        self
    }

    /// Margin at the end side, CSS `margin-inline-end`.
    fn margin_end(mut self, dir: Direction, length: impl Into<Length>) -> Self {
        let edges = &mut self.style().margin;
        let side = if dir.is_rtl() {
            &mut edges.left
        } else {
            &mut edges.right
        };
        *side = Some(length.into());
        self
    }

    /// Offset from the start side, CSS `inset-inline-start`.
    fn inset_start(mut self, dir: Direction, length: impl Into<Length>) -> Self {
        let edges = &mut self.style().inset;
        let side = if dir.is_rtl() {
            &mut edges.right
        } else {
            &mut edges.left
        };
        *side = Some(length.into());
        self
    }

    /// Offset from the end side, CSS `inset-inline-end`.
    fn inset_end(mut self, dir: Direction, length: impl Into<Length>) -> Self {
        let edges = &mut self.style().inset;
        let side = if dir.is_rtl() {
            &mut edges.left
        } else {
            &mut edges.right
        };
        *side = Some(length.into());
        self
    }

    /// Border width at the start side, CSS `border-inline-start-width`.
    fn border_start(mut self, dir: Direction, width: impl Into<AbsoluteLength>) -> Self {
        let edges = &mut self.style().border_widths;
        let side = if dir.is_rtl() {
            &mut edges.right
        } else {
            &mut edges.left
        };
        *side = Some(width.into());
        self
    }

    /// Border width at the end side, CSS `border-inline-end-width`.
    fn border_end(mut self, dir: Direction, width: impl Into<AbsoluteLength>) -> Self {
        let edges = &mut self.style().border_widths;
        let side = if dir.is_rtl() {
            &mut edges.left
        } else {
            &mut edges.right
        };
        *side = Some(width.into());
        self
    }

    /// Radius of both start corners, CSS `border-start-start-radius` and
    /// `border-end-start-radius`.
    fn rounded_start(mut self, dir: Direction, radius: impl Into<AbsoluteLength>) -> Self {
        let radius = radius.into();
        let corners = &mut self.style().corner_radii;
        if dir.is_rtl() {
            corners.top_right = Some(radius);
            corners.bottom_right = Some(radius);
        } else {
            corners.top_left = Some(radius);
            corners.bottom_left = Some(radius);
        }
        self
    }

    /// Radius of both end corners.
    fn rounded_end(mut self, dir: Direction, radius: impl Into<AbsoluteLength>) -> Self {
        let radius = radius.into();
        let corners = &mut self.style().corner_radii;
        if dir.is_rtl() {
            corners.top_left = Some(radius);
            corners.bottom_left = Some(radius);
        } else {
            corners.top_right = Some(radius);
            corners.bottom_right = Some(radius);
        }
        self
    }

    /// Text aligned to the start, CSS `text-align: start`.
    fn text_start(self, dir: Direction) -> Self {
        self.text_align(if dir.is_rtl() {
            TextAlign::Right
        } else {
            TextAlign::Left
        })
    }

    /// Text aligned to the end, CSS `text-align: end`.
    fn text_end(self, dir: Direction) -> Self {
        self.text_align(if dir.is_rtl() {
            TextAlign::Left
        } else {
            TextAlign::Right
        })
    }

    /// A row that runs in the reading direction, as CSS `flex-direction: row`
    /// does: start-aligned items sit at the right in RTL.
    fn flex_row_logical(mut self, dir: Direction) -> Self {
        self.style().flex_direction = Some(if dir.is_rtl() {
            FlexDirection::RowReverse
        } else {
            FlexDirection::Row
        });
        self.flex()
    }

    /// Mirrors horizontally in RTL, for directional icons (arrows, chevrons),
    /// as upstream's `:is([dir="rtl"] *) { transform: scaleX(-1) }`.
    fn mirror_in_rtl(self, dir: Direction) -> Self {
        if dir.is_rtl() {
            self.scale_xy(-1., 1.)
        } else {
            self
        }
    }
}

impl<T: Styled> LogicalStyled for T {}
