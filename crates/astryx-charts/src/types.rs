//! The chart's core types (upstream `types.ts`): a series (a mark's definition), the
//! context the layout pass hands each series, the positions it resolves, and the chart
//! context chrome and interactions read.

use std::rc::Rc;

use astryx_theme::Tokens;
use gpui::{AnyElement, App, Hsla, SharedString, Window};

use crate::Datum;
use crate::scale::{BandScale, LinearScale};
use crate::shape::ChartPath;

/// The chart's x scale (upstream `ChartScale`): linear for numeric x, a band scale for
/// categories.
#[derive(Clone, Debug)]
pub enum ChartScale {
    Linear(LinearScale),
    Band(BandScale),
}

impl ChartScale {
    /// Upstream `isBandScale`.
    pub fn is_band(&self) -> bool {
        matches!(self, ChartScale::Band(_))
    }

    /// The band scale, when it is one.
    pub fn as_band(&self) -> Option<&BandScale> {
        match self {
            ChartScale::Band(band) => Some(band),
            ChartScale::Linear(_) => None,
        }
    }

    /// The linear scale, when it is one.
    pub fn as_linear(&self) -> Option<&LinearScale> {
        match self {
            ChartScale::Linear(linear) => Some(linear),
            ChartScale::Band(_) => None,
        }
    }
}

/// A per-datum color (upstream a `ColorAccessor` function): `None` asks for the
/// series' palette color, as a blank string or a throwing accessor does upstream.
pub type ColorFn = Rc<dyn Fn(&Datum, usize) -> Option<Hsla>>;

/// A mark's color (upstream `ColorAccessor`): one color, or one per datum.
#[derive(Clone)]
pub enum ColorAccessor {
    Color(Hsla),
    PerDatum(ColorFn),
}

impl ColorAccessor {
    /// A color for each datum, from `f(datum, index)`.
    pub fn per_datum(f: impl Fn(&Datum, usize) -> Option<Hsla> + 'static) -> Self {
        ColorAccessor::PerDatum(Rc::new(f))
    }

    /// The one color, when it is not per datum.
    pub fn fixed(&self) -> Option<Hsla> {
        match self {
            ColorAccessor::Color(color) => Some(*color),
            ColorAccessor::PerDatum(_) => None,
        }
    }
}

impl From<Hsla> for ColorAccessor {
    fn from(color: Hsla) -> Self {
        ColorAccessor::Color(color)
    }
}

impl std::fmt::Debug for ColorAccessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColorAccessor::Color(color) => f.debug_tuple("Color").field(color).finish(),
            ColorAccessor::PerDatum(_) => f.write_str("PerDatum(..)"),
        }
    }
}

/// How the y domain is derived when no `y_domain` is given (upstream `YBaseline`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum YBaseline {
    /// Bar and area marks pin a zero baseline; continuous-only charts get a little
    /// headroom so points do not touch the edges.
    #[default]
    Auto,
    /// Symmetric around zero, for signed data such as profit and loss.
    Zero,
    /// A tight fit to the data, with no zero and no headroom.
    Data,
}

/// The plot's inset inside the chart, in pixels (upstream `ChartMargin`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartMargin {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Default for ChartMargin {
    /// Upstream `DEFAULT_MARGIN`: 24 above and right, 32 below, 48 left.
    fn default() -> Self {
        ChartMargin {
            top: 24.,
            right: 24.,
            bottom: 32.,
            left: 48.,
        }
    }
}

/// Part of a margin (upstream `Partial<ChartMargin>`): the sides it sets replace the
/// default's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MarginOverride {
    pub top: Option<f64>,
    pub right: Option<f64>,
    pub bottom: Option<f64>,
    pub left: Option<f64>,
}

impl MarginOverride {
    pub fn top(mut self, top: f64) -> Self {
        self.top = Some(top);
        self
    }

    pub fn right(mut self, right: f64) -> Self {
        self.right = Some(right);
        self
    }

    pub fn bottom(mut self, bottom: f64) -> Self {
        self.bottom = Some(bottom);
        self
    }

    pub fn left(mut self, left: f64) -> Self {
        self.left = Some(left);
        self
    }

    /// The default margin with these sides set.
    pub fn over(&self, base: ChartMargin) -> ChartMargin {
        ChartMargin {
            top: self.top.unwrap_or(base.top),
            right: self.right.unwrap_or(base.right),
            bottom: self.bottom.unwrap_or(base.bottom),
            left: self.left.unwrap_or(base.left),
        }
    }
}

impl From<ChartMargin> for MarginOverride {
    fn from(m: ChartMargin) -> Self {
        MarginOverride {
            top: Some(m.top),
            right: Some(m.right),
            bottom: Some(m.bottom),
            left: Some(m.left),
        }
    }
}

/// One data point's pixel position in a series (upstream `ResolvedPoint`), in plot
/// coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedPoint {
    /// The x pixel.
    pub px: f64,
    /// The y pixel of the mark's top.
    pub py: f64,
    /// The y pixel of the mark's baseline (its bottom), which differs from `py` when
    /// stacked.
    pub py0: f64,
    /// The row's index in the data.
    pub data_index: usize,
}

/// What a series resolves and renders against (upstream `SeriesContext`).
#[derive(Clone, Debug)]
pub struct SeriesContext {
    pub data: Rc<[Datum]>,
    pub x_key: SharedString,
    pub x_scale: ChartScale,
    pub y_scale: LinearScale,
    /// The categorical y band scale of row-based marks (a heatmap), present when a
    /// series sets `SeriesLayout::y_band_key`; the linear `y_scale` is then unused.
    pub y_band_scale: Option<BandScale>,
    /// The plot's width.
    pub width: f64,
    /// The plot's height.
    pub height: f64,
}

/// A stacked series' bounds at one row (d3's `[y0, y1]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackOffset {
    pub y0: f64,
    pub y1: f64,
}

/// A grouped bar's slot among its group's (upstream `groupInfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupInfo {
    pub index: usize,
    pub count: usize,
}

/// The hints the chart reads to coordinate series (upstream `SeriesDef.layout`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeriesLayout {
    /// Series with the same stack accumulate.
    pub stack: Option<SharedString>,
    /// Bars with the same group share the band side by side.
    pub group: Option<SharedString>,
    /// The series pins the y domain's zero.
    pub include_zero: bool,
    /// The y axis is a band scale over this key's unique values (a heatmap's rows)
    /// rather than a linear value scale.
    pub y_band_key: Option<SharedString>,
    /// How far the mark draws past a point's x (a dot's radius): a linear x range is
    /// inset by the largest, so marks at the ends draw whole inside the clipped plot.
    pub x_overhang: Option<f64>,
}

/// What the chart's layout pass gives each series (upstream's `_uid`,
/// `_isTopOfStack` and `_resolvedColor`, which it writes on the definition).
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesInfo {
    /// The series' identity, from its position: `"{index}:{key}"`.
    pub uid: SharedString,
    /// The series draws on top of its stack (or is not stacked).
    pub is_top_of_stack: bool,
    /// The palette color the chart gave a series without a static color.
    pub resolved_color: Option<Hsla>,
    /// Its slot in its bar group.
    pub group: Option<GroupInfo>,
}

/// One shape a mark paints, in plot coordinates: what one SVG element of its
/// `render` is upstream. [`Series::shapes`] lists them.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkShape {
    /// A filled path (`<path fill>`): a bar, an area or a band.
    Fill {
        /// The outline.
        path: ChartPath,
        /// The fill.
        color: Hsla,
        /// What the color's alpha is multiplied by (`fill-opacity`).
        opacity: f64,
    },
    /// A path filled from `color` at `opacity` at its top to transparent at its
    /// bottom (an area's `linearGradient` fill).
    GradientFill {
        /// The outline.
        path: ChartPath,
        /// The gradient's color.
        color: Hsla,
        /// The gradient's opacity at the top.
        opacity: f64,
    },
    /// A stroked path (`<path fill="none" stroke>`).
    Stroke {
        /// The line.
        path: ChartPath,
        /// The stroke.
        color: Hsla,
        /// The stroke's width (`stroke-width`).
        width: f64,
    },
    /// A filled circle (`<circle>`).
    Circle {
        /// The center's x.
        cx: f64,
        /// The center's y.
        cy: f64,
        /// The radius.
        r: f64,
        /// The fill.
        color: Hsla,
        /// What the color's alpha is multiplied by (`opacity`).
        opacity: f64,
    },
    /// A straight segment (`<line>`).
    Segment {
        /// One end.
        from: (f64, f64),
        /// The other end.
        to: (f64, f64),
        /// The stroke.
        color: Hsla,
        /// The stroke's width (`stroke-width`).
        width: f64,
        /// The dash pattern (`stroke-dasharray`); empty for a solid line.
        dash: Vec<f64>,
    },
    /// A filled rectangle (`<rect>`).
    Rect {
        /// The left edge.
        x: f64,
        /// The top edge.
        y: f64,
        /// The width.
        width: f64,
        /// The height.
        height: f64,
        /// The fill.
        color: Hsla,
        /// What the color's alpha is multiplied by (`opacity`).
        opacity: f64,
    },
    /// A label badge (a reference line's): `text` centered in a rounded chip.
    Badge {
        /// The left edge.
        x: f64,
        /// The top edge.
        y: f64,
        /// The width.
        width: f64,
        /// The height.
        height: f64,
        /// The label.
        text: SharedString,
        /// The border and the text's color.
        color: Hsla,
    },
}

/// A mark's definition (upstream `SeriesDef`): it resolves its points against the
/// chart's scales and renders itself. The mark factories (`bar`, `line`, …) make them.
pub trait Series: 'static {
    /// The mark type (`"bar"`, `"line"`, …), upstream `type`.
    fn kind(&self) -> &'static str;
    /// The series' key, usually its data key (not unique; [`SeriesInfo::uid`] is).
    fn key(&self) -> SharedString;
    /// The data keys it reads, which feed the y domain.
    fn data_keys(&self) -> Vec<SharedString>;
    /// Its static color, for the legend; `None` when automatic or per datum.
    fn color(&self) -> Option<Hsla> {
        None
    }
    /// Its legend label (else its key).
    fn label(&self) -> Option<SharedString> {
        None
    }
    /// Its layout hints.
    fn layout(&self) -> SeriesLayout {
        SeriesLayout::default()
    }
    /// It draws above the whole chart, axes and tooltip included, as upstream's
    /// WebGL marks do (their canvas sits over the chart's SVG), still inside the plot.
    fn draws_above(&self) -> bool {
        false
    }
    /// Its points' pixel positions, from the stack offsets when stacked and its slot
    /// when grouped.
    fn resolve(
        &self,
        ctx: &SeriesContext,
        stack: Option<&[StackOffset]>,
        group: Option<GroupInfo>,
    ) -> Vec<ResolvedPoint>;
    /// What the mark paints from its resolved points, in plot coordinates, with
    /// `tokens` giving its default colors (upstream the SVG elements `render`
    /// returns); `render` paints these. A mark drawn another way (the WebGL marks)
    /// lists none.
    fn shapes(
        &self,
        resolved: &[ResolvedPoint],
        ctx: &SeriesContext,
        info: &SeriesInfo,
        tokens: &Tokens,
    ) -> Vec<MarkShape> {
        let _ = (resolved, ctx, info, tokens);
        Vec::new()
    }
    /// Draws the mark from its resolved points, as an element filling the plot.
    fn render(
        &self,
        resolved: &[ResolvedPoint],
        ctx: &SeriesContext,
        info: &SeriesInfo,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement>;
}

/// A shared series definition, as the chart's `series` list holds them.
pub type SeriesDef = Rc<dyn Series>;

/// Resolved positions by series uid (upstream `ResolvedPositions`), in series order.
pub type ResolvedPositions = Vec<(SharedString, Vec<ResolvedPoint>)>;

/// The point nearest the pointer, with its series' uid.
#[derive(Clone, Debug, PartialEq)]
pub struct NearestPoint {
    pub point: ResolvedPoint,
    pub series_key: SharedString,
}

/// A pointer event in plot coordinates (upstream `ChartPointerEvent`).
#[derive(Clone, Debug, PartialEq)]
pub struct ChartPointerEvent {
    pub x: f64,
    pub y: f64,
    /// The point nearest by x (the whole column is its hover zone).
    pub nearest: Option<NearestPoint>,
    /// A button is down.
    pub active: bool,
}

/// Marks that annotate rather than carry data (upstream `UTILITY_MARK_TYPES`): the
/// legend and tooltip skip them.
pub const UTILITY_MARK_TYPES: [&str; 3] = ["referenceLine", "errorBar", "band"];

/// Upstream `isUtilityMarkType`.
pub fn is_utility_mark_type(kind: &str) -> bool {
    UTILITY_MARK_TYPES.contains(&kind)
}
