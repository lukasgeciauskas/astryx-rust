//! The color a mark paints (upstream `markColor.ts`): an explicit color wins, then the
//! palette color the chart gave the series, then the accent token.

use gpui::Hsla;

use crate::Datum;
use crate::types::{ColorAccessor, SeriesInfo};

/// The palette color, else the last-resort accent (upstream `DEFAULT_SERIES_COLOR`,
/// `--color-accent`).
fn fallback_fill(info: &SeriesInfo, accent: Hsla) -> Hsla {
    info.resolved_color.unwrap_or(accent)
}

/// A series' one color (lines, areas, bands), upstream `seriesFill`. A per-datum
/// accessor has no one color, so it takes the palette's.
pub fn series_fill(info: &SeriesInfo, color: Option<&ColorAccessor>, accent: Hsla) -> Hsla {
    match color {
        Some(ColorAccessor::Color(color)) => *color,
        _ => fallback_fill(info, accent),
    }
}

/// One point's color (upstream `pointFill`): the accessor's, then the explicit color,
/// then the palette's.
pub fn point_fill(
    info: &SeriesInfo,
    color: Option<&ColorAccessor>,
    datum: &Datum,
    index: usize,
    accent: Hsla,
) -> Hsla {
    match color {
        Some(ColorAccessor::PerDatum(f)) => {
            f(datum, index).unwrap_or_else(|| fallback_fill(info, accent))
        }
        Some(ColorAccessor::Color(color)) => *color,
        None => fallback_fill(info, accent),
    }
}
