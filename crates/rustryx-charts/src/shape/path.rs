//! A recorded d3-path: the commands a curve draws, written out as d3-path writes them
//! or built into a GPUI path.

use gpui::{FillOptions, FillRule, PathBuilder, Pixels, Point, point, px};
use rustryx_theme::authoring::js::{js_number_to_string, js_round};

/// One drawing command (d3-path's `moveTo`, `lineTo`, `quadraticCurveTo`,
/// `bezierCurveTo` and `closePath`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCommand {
    /// Starts a subpath at (x, y) (`M`).
    MoveTo { x: f64, y: f64 },
    /// A straight line to (x, y) (`L`).
    LineTo { x: f64, y: f64 },
    /// A quadratic Bézier curve through the control point (x1, y1) to (x, y) (`Q`).
    QuadraticCurveTo { x1: f64, y1: f64, x: f64, y: f64 },
    /// A cubic Bézier curve through the control points (x1, y1) and (x2, y2) to (x, y)
    /// (`C`).
    BezierCurveTo {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    /// Closes the subpath with a line back to its start (`Z`).
    ClosePath,
}

/// How [`ChartPath::build`] paints a path, as SVG's `fill` or `stroke` would.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathStyle {
    /// Fills the shape with SVG's default `nonzero` rule, each subpath closed.
    Fill,
    /// Strokes the outline `width` pixels wide, with SVG's default butt caps and
    /// miter joins.
    Stroke {
        /// The stroke width, in pixels.
        width: f32,
    },
}

/// The commands of a d3 path (d3-path `Path`), as a line or area generator records
/// them. An empty path is d3's `null` output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartPath {
    commands: Vec<PathCommand>,
}

impl ChartPath {
    /// An empty path (d3-path `path()`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a subpath (d3-path `moveTo`).
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.commands.push(PathCommand::MoveTo { x, y });
    }

    /// Draws a line to a point (d3-path `lineTo`).
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.commands.push(PathCommand::LineTo { x, y });
    }

    /// Draws a quadratic curve (d3-path `quadraticCurveTo`).
    pub fn quadratic_curve_to(&mut self, x1: f64, y1: f64, x: f64, y: f64) {
        self.commands
            .push(PathCommand::QuadraticCurveTo { x1, y1, x, y });
    }

    /// Draws a cubic curve (d3-path `bezierCurveTo`).
    pub fn bezier_curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        self.commands.push(PathCommand::BezierCurveTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        });
    }

    /// Closes the subpath (d3-path `closePath`); does nothing on an empty path, which
    /// has no subpath to close.
    pub fn close_path(&mut self) {
        if !self.commands.is_empty() {
            self.commands.push(PathCommand::ClosePath);
        }
    }

    /// The recorded commands.
    pub fn commands(&self) -> &[PathCommand] {
        &self.commands
    }

    /// Whether nothing was drawn (where d3's generators return `null`).
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// The SVG path data d3-path writes (`path.toString()`): each number rounded to
    /// `digits` decimals (d3-path `pathRound(digits)`; d3-shape's generators use 3)
    /// and written as JavaScript writes numbers, or unrounded for `None` or more than
    /// 15 digits.
    pub fn to_svg(&self, digits: Option<u32>) -> String {
        let k = digits.filter(|d| *d <= 15).map(|d| 10f64.powi(d as i32));
        let n = |v: f64| match k {
            Some(k) => js_number_to_string(js_round(v * k) / k),
            None => js_number_to_string(v),
        };
        let mut out = String::new();
        for command in &self.commands {
            match *command {
                PathCommand::MoveTo { x, y } => out += &format!("M{},{}", n(x), n(y)),
                PathCommand::LineTo { x, y } => out += &format!("L{},{}", n(x), n(y)),
                PathCommand::QuadraticCurveTo { x1, y1, x, y } => {
                    out += &format!("Q{},{},{},{}", n(x1), n(y1), n(x), n(y));
                }
                PathCommand::BezierCurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    out += &format!("C{},{},{},{},{},{}", n(x1), n(y1), n(x2), n(y2), n(x), n(y));
                }
                PathCommand::ClosePath => out.push('Z'),
            }
        }
        out
    }

    /// The path as a GPUI path, offset by `origin`. A browser draws SVG path data up
    /// to the first command it cannot read, and d3 writes a non-finite coordinate as
    /// `NaN` or `Infinity`, so the path stops before the first command with a
    /// coordinate that is not finite in pixels. `None` when nothing is left to draw
    /// or GPUI cannot build it.
    pub fn build(&self, origin: Point<Pixels>, style: PathStyle) -> Option<gpui::Path> {
        let (ox, oy) = (f64::from(origin.x), f64::from(origin.y));
        let at = |x: f64, y: f64| {
            let (x, y) = ((ox + x) as f32, (oy + y) as f32);
            (x.is_finite() && y.is_finite()).then(|| point(px(x), px(y)))
        };
        let mut builder = match style {
            PathStyle::Fill => PathBuilder::fill().with_style(gpui::PathStyle::Fill(FillOptions {
                fill_rule: FillRule::NonZero,
            })),
            PathStyle::Stroke { width } => PathBuilder::stroke(px(width)),
        };
        let mut drawn = false;
        for command in &self.commands {
            match *command {
                PathCommand::MoveTo { x, y } => {
                    let Some(to) = at(x, y) else { break };
                    builder.move_to(to);
                }
                PathCommand::LineTo { x, y } => {
                    let Some(to) = at(x, y) else { break };
                    builder.line_to(to);
                }
                PathCommand::QuadraticCurveTo { x1, y1, x, y } => {
                    let (Some(ctrl), Some(to)) = (at(x1, y1), at(x, y)) else {
                        break;
                    };
                    builder.curve_to(to, ctrl);
                }
                PathCommand::BezierCurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    let (Some(a), Some(b), Some(to)) = (at(x1, y1), at(x2, y2), at(x, y)) else {
                        break;
                    };
                    builder.cubic_bezier_to(to, a, b);
                }
                PathCommand::ClosePath => builder.close(),
            }
            drawn = true;
        }
        if !drawn {
            return None;
        }
        builder.build().ok()
    }
}
