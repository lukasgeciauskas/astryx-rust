//! Text the chart paints itself (an SVG `<text>`): shaped in the inherited text style
//! and placed by its baseline and anchor.

use gpui::{App, Hsla, Pixels, SharedString, TextAlign, Window, point, px};

/// Where a label sits against its x (SVG `text-anchor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Anchor {
    Start,
    Middle,
    End,
}

/// Paints `text` at `font_size` with its baseline at (`x`, `baseline`) from `origin`,
/// anchored by `anchor`, in `color`, at `opacity`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_label(
    window: &mut Window,
    cx: &mut App,
    origin: gpui::Point<Pixels>,
    text: &SharedString,
    (x, baseline): (f64, f64),
    anchor: Anchor,
    font_size: f64,
    color: Hsla,
) {
    if text.is_empty() || !x.is_finite() || !baseline.is_finite() || color.a <= 0. {
        return;
    }
    let mut style = window.text_style();
    style.color = color;
    let size = px(font_size as f32);
    let run = style.to_run(text.len());
    let line = window
        .text_system()
        .shape_line(text.clone(), size, &[run], None);
    let left = match anchor {
        Anchor::Start => x,
        Anchor::Middle => x - f64::from(line.width) / 2.,
        Anchor::End => x - f64::from(line.width),
    };
    let line_height = line.ascent + line.descent;
    let at = origin + point(px(left as f32), px(baseline as f32) - line.ascent);
    let _ = line.paint(at, line_height, TextAlign::Left, None, window, cx);
}
