//! The element that draws a field's text: shaped (and in multiline fields,
//! wrapped) lines, the selection, the IME composition's underline, the caret
//! and the placeholder. It scrolls to keep the caret in view and registers the
//! field as the window's IME client while it is focused.
//!
//! Adapted from GPUI's `examples/input.rs` (Apache-2.0, Copyright Zed
//! Industries).

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, ElementInputHandler,
    Entity, GlobalElementId, Hsla, InspectorElementId, IntoElement as _, LayoutId, LineLayout,
    ParentElement as _, Pixels, Point, ShapedRun, SharedString, Size, Style, Styled as _, TextRun,
    UnderlineStyle, Window, WrapBoundary, WrappedLine, WrappedLineLayout, div, fill, point, px,
    relative, size,
};
use unicode_linebreak::linebreaks;
use unicode_segmentation::UnicodeSegmentation;

use super::buffer::ObjectId;
use super::state::{EditorLayout, OBJECT_STAND_IN, TextEditState};

/// How a field's text looks, set by its component.
#[derive(Clone)]
pub struct EditorStyle {
    pub text_color: Hsla,
    pub placeholder: Option<SharedString>,
    pub placeholder_color: Hsla,
    pub selection_color: Hsla,
    pub caret_color: Hsla,
    /// Wraps and breaks lines; grows from `min_lines` to `max_lines`.
    pub multiline: bool,
    pub min_lines: usize,
    pub max_lines: Option<usize>,
    /// A multiline field's height, overriding its lines (a field the user
    /// resized). It scrolls when its text is taller.
    pub height: Option<Pixels>,
    /// No caret blink.
    pub reduced_motion: bool,
    /// The wavy underline under misspelled words.
    pub spelling_color: Hsla,
}

/// The caret blinks this often, after holding solid as long after a change.
const BLINK: Duration = Duration::from_millis(500);
const CARET_WIDTH: Pixels = px(1.5);

pub(crate) struct EditorElement {
    pub state: Entity<TextEditState>,
    pub style: EditorStyle,
}

/// An inline object's box for this frame.
pub(crate) struct ObjectBox {
    /// Where its character is in the buffer.
    offset: usize,
    element: AnyElement,
    width: Pixels,
}

/// Each object's display offset and width, for shaping.
type ObjectWidths = Rc<[(usize, Pixels)]>;

pub(crate) struct Prepainted {
    lines: Rc<[(usize, WrappedLine)]>,
    placeholder: Option<WrappedLine>,
    selection: Vec<Bounds<Pixels>>,
    caret: Option<Bounds<Pixels>>,
    scroll: Point<Pixels>,
    line_height: Pixels,
}

/// The visual rows of shaped lines.
fn rows<'a>(lines: impl IntoIterator<Item = &'a WrappedLine>) -> usize {
    lines
        .into_iter()
        .map(|line| line.wrap_boundaries.len() + 1)
        .sum::<usize>()
        .max(1)
}

/// Shapes `text` as [`shape`] does, with each inline object's stand-in
/// widened to its box (`objects`: display offsets and widths, in order). A
/// logical line holding an object is shaped unwrapped, widened, then wrapped
/// here at the line-break opportunities (UAX #14), a word longer than the line
/// breaking between graphemes; other lines wrap as GPUI wraps them.
fn shape_lines(
    text: &str,
    runs: &[TextRun],
    font_size: Pixels,
    wrap_width: Option<Pixels>,
    objects: &[(usize, Pixels)],
    window: &Window,
) -> Vec<(usize, WrappedLine)> {
    if objects.is_empty() {
        return shape(text, runs, font_size, wrap_width, window);
    }
    let mut lines = shape(text, runs, font_size, None, window);
    let mut wrapped: Vec<Option<WrappedLine>> = match wrap_width {
        Some(width) => shape(text, runs, font_size, Some(width), window)
            .into_iter()
            .map(|(_, line)| Some(line))
            .collect(),
        None => Vec::new(),
    };
    for (i, (start, line)) in lines.iter_mut().enumerate() {
        let end = *start + line.len();
        let here: Vec<(usize, Pixels)> = objects
            .iter()
            .filter(|(offset, _)| *offset >= *start && *offset < end)
            .map(|(offset, width)| (offset - *start, *width))
            .collect();
        if here.is_empty() {
            if let Some(plain) = wrapped.get_mut(i).and_then(Option::take) {
                *line = plain;
            }
            continue;
        }
        let widened = widen(&line.unwrapped_layout, &here);
        let wrap_boundaries = match wrap_width {
            Some(width) => wrap(&widened, &text[*start..end], width)
                .into_iter()
                .collect(),
            None => Default::default(),
        };
        let layout: &mut Arc<WrappedLineLayout> = line;
        *layout = Arc::new(WrappedLineLayout {
            unwrapped_layout: Arc::new(widened),
            wrap_boundaries,
            wrap_width,
        });
    }
    lines
}

/// A copy of `layout` with each object's stand-in as wide as its box: every
/// glyph after it moves right by the difference.
fn widen(layout: &LineLayout, objects: &[(usize, Pixels)]) -> LineLayout {
    let stand_in = OBJECT_STAND_IN.len_utf8();
    let deltas: Vec<(usize, Pixels)> = objects
        .iter()
        .map(|(offset, width)| {
            let advance = layout.x_for_index(offset + stand_in) - layout.x_for_index(*offset);
            (offset + stand_in, *width - advance)
        })
        .collect();
    let shift = |index: usize| -> Pixels {
        deltas
            .iter()
            .filter(|(after, _)| *after <= index)
            .map(|(_, delta)| *delta)
            .fold(px(0.), |a, b| a + b)
    };
    let runs = layout
        .runs
        .iter()
        .map(|run| ShapedRun {
            font_id: run.font_id,
            glyphs: run
                .glyphs
                .iter()
                .map(|glyph| {
                    let mut glyph = glyph.clone();
                    glyph.position.x += shift(glyph.index);
                    glyph
                })
                .collect(),
        })
        .collect();
    LineLayout {
        font_size: layout.font_size,
        width: layout.width + shift(layout.len),
        ascent: layout.ascent,
        descent: layout.descent,
        runs,
        len: layout.len,
    }
}

/// Where a widened line wraps at `width`: greedy, at the last break
/// opportunity that fits, else between graphemes; trailing white space hangs.
fn wrap(layout: &LineLayout, text: &str, width: Pixels) -> Vec<WrapBoundary> {
    let opportunities: Vec<usize> = linebreaks(text).map(|(i, _)| i).collect();
    let mut boundaries: Vec<usize> = Vec::new();
    let mut row_start = 0;
    let mut row_x = px(0.);
    let mut last_break: Option<usize> = None;
    let mut previous = 0;
    let ends: Vec<usize> = text
        .grapheme_indices(true)
        .map(|(i, g)| i + g.len())
        .collect();
    for end in ends {
        let grapheme = &text[previous..end];
        let overflows = layout.x_for_index(end) - row_x > width;
        if overflows && !grapheme.trim().is_empty() {
            let at = match last_break {
                Some(at) if at > row_start => at,
                _ if previous > row_start => previous,
                _ => end,
            };
            if at < text.len() {
                boundaries.push(at);
                row_start = at;
                row_x = layout.x_for_index(at);
            }
            last_break = None;
        }
        if opportunities.binary_search(&end).is_ok() {
            last_break = Some(end);
        }
        previous = end;
    }
    let mut out: Vec<WrapBoundary> = Vec::new();
    for at in boundaries {
        let found = layout.runs.iter().enumerate().find_map(|(run_ix, run)| {
            run.glyphs
                .iter()
                .position(|glyph| glyph.index >= at)
                .map(|glyph_ix| WrapBoundary { run_ix, glyph_ix })
        });
        if let Some(boundary) = found
            && out.last() != Some(&boundary)
        {
            out.push(boundary);
        }
    }
    out
}

/// Shapes `text` into one wrapped line per logical line, with each one's start.
fn shape(
    text: &str,
    runs: &[TextRun],
    font_size: Pixels,
    wrap_width: Option<Pixels>,
    window: &Window,
) -> Vec<(usize, WrappedLine)> {
    let shaped = window
        .text_system()
        .shape_text(
            SharedString::from(text.to_string()),
            font_size,
            runs,
            wrap_width,
            None,
        )
        .unwrap_or_default();
    let mut start = 0;
    shaped
        .into_iter()
        .map(|line| {
            let here = start;
            start += line.len() + 1;
            (here, line)
        })
        .collect()
}

impl EditorElement {
    /// Lays out each inline object's element, one line high.
    fn layout_objects(
        &self,
        line_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<ObjectBox> {
        let state = self.state.read(cx);
        let Some(objects) = state.inline_objects.clone() else {
            return Vec::new();
        };
        let offsets: Vec<(usize, ObjectId)> = state.buffer().object_offsets();
        offsets
            .into_iter()
            .map(|(offset, id)| {
                let mut element = div()
                    .h(line_height)
                    .flex()
                    .items_center()
                    .child((objects.render)(id, window, cx))
                    .into_any_element();
                let measured = element.layout_as_root(
                    size(
                        AvailableSpace::MaxContent,
                        AvailableSpace::Definite(line_height),
                    ),
                    window,
                    cx,
                );
                ObjectBox {
                    offset,
                    element,
                    width: measured.width,
                }
            })
            .collect()
    }

    fn object_widths(&self, boxes: &[ObjectBox], cx: &App) -> ObjectWidths {
        let state = self.state.read(cx);
        boxes
            .iter()
            .map(|b| (state.offset_to_display(b.offset), b.width))
            .collect()
    }
}

impl Element for EditorElement {
    type RequestLayoutState = Vec<ObjectBox>;
    type PrepaintState = Prepainted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let line_height = window.line_height();
        let boxes = self.layout_objects(line_height, window, cx);
        let widths = self.object_widths(&boxes, cx);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let no_wrap = self.style.multiline && self.state.read(cx).no_wrap;
        if no_wrap && self.style.height.is_none() {
            // As wide as its longest line, never narrower than its container, and as
            // tall as its lines, within its rows.
            let text_style = window.text_style();
            let font_size = text_style.font_size.to_pixels(window.rem_size());
            let text = self.state.read(cx).display_text();
            let runs = [text_style.to_run(text.len())];
            let lines = shape_lines(&text, &runs, font_size, None, &widths, window);
            let widest = lines
                .iter()
                .map(|(_, line)| line.size(line_height).width)
                .fold(px(0.), Pixels::max);
            let (min, max) = (self.style.min_lines.max(1), self.style.max_lines);
            let mut count = rows(lines.iter().map(|(_, l)| l)).max(min);
            if let Some(max) = max {
                count = count.min(max.max(min));
            }
            style.size.width = (widest + CARET_WIDTH).into();
            style.min_size.width = relative(1.).into();
            style.size.height = (line_height * count as f32).into();
            return (window.request_layout(style, [], cx), boxes);
        }
        if !self.style.multiline {
            style.size.height = line_height.into();
            return (window.request_layout(style, [], cx), boxes);
        }
        if let Some(height) = self.style.height {
            style.size.height = height.into();
            return (window.request_layout(style, [], cx), boxes);
        }
        // A multiline field is as tall as its wrapped text, within its rows.
        let state = self.state.clone();
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let (min, max) = (self.style.min_lines.max(1), self.style.max_lines);
        let layout = window.request_measured_layout(style, move |known, available, window, cx| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(width) => Some(width),
                _ => None,
            });
            let text = state.read(cx).display_text();
            let runs = [text_style.to_run(text.len())];
            let lines: Vec<WrappedLine> =
                shape_lines(&text, &runs, font_size, width, &widths, window)
                    .into_iter()
                    .map(|(_, line)| line)
                    .collect();
            let mut count = rows(&lines[..]).max(min);
            if let Some(max) = max {
                count = count.min(max.max(min));
            }
            let widest = lines
                .iter()
                .map(|line| line.size(line_height).width)
                .fold(px(0.), Pixels::max);
            size(width.unwrap_or(widest), line_height * count as f32)
        });
        (layout, boxes)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        boxes: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let widths = self.object_widths(boxes, cx);
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let wrap =
            (self.style.multiline && !self.state.read(cx).no_wrap).then_some(bounds.size.width);
        let focus = gpui::Focusable::focus_handle(self.state.read(cx), cx);
        let focused = focus.is_focused(window);
        // Every Core field registers with the keyboard module where it is drawn, so the
        // keyboard's surfaces find it (AST-099 FR9: no prop turns this on).
        let kind = self.state.read(cx).keyboard_field_kind();
        let drawn = window.element_transform().apply_bounds(bounds);
        crate::keyboard::register_field(&focus, kind, drawn, true, window, cx);
        let misspelled = self
            .state
            .update(cx, |state, cx| state.spelling_errors(focused, cx));
        let state = self.state.read(cx);
        let display = state.display_text();
        let buffer = state.buffer();
        let marked = buffer
            .marked()
            .map(|r| state.offset_to_display(r.start)..state.offset_to_display(r.end));

        // The composition is underlined, as platform fields do; misspelled
        // words get a wavy line, as browsers draw them.
        let run = |len: usize| TextRun {
            len,
            ..text_style.to_run(len)
        };
        let mut decorations: Vec<(std::ops::Range<usize>, UnderlineStyle)> = Vec::new();
        if let Some(marked) = &marked {
            decorations.push((
                marked.clone(),
                UnderlineStyle {
                    color: Some(self.style.text_color),
                    thickness: px(1.),
                    wavy: false,
                },
            ));
        }
        for range in misspelled {
            let range = state.offset_to_display(range.start)..state.offset_to_display(range.end);
            decorations.push((
                range,
                UnderlineStyle {
                    color: Some(self.style.spelling_color),
                    thickness: px(1.),
                    wavy: true,
                },
            ));
        }
        // The component's color ranges, as display ranges.
        let painted: Vec<(std::ops::Range<usize>, Hsla)> = state
            .highlights
            .iter()
            .filter(|(r, _)| r.start < r.end && r.end <= buffer.text().len())
            .map(|(r, color)| {
                (
                    state.offset_to_display(r.start)..state.offset_to_display(r.end),
                    *color,
                )
            })
            .collect();
        let mut cuts: Vec<usize> = decorations
            .iter()
            .flat_map(|(r, _)| [r.start, r.end])
            .chain(painted.iter().flat_map(|(r, _)| [r.start, r.end]))
            .chain([0, display.len()])
            .filter(|at| *at <= display.len())
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        let mut runs: Vec<TextRun> = cuts
            .windows(2)
            .map(|pair| {
                let (from, to) = (pair[0], pair[1]);
                TextRun {
                    color: painted
                        .iter()
                        .find(|(r, _)| r.start <= from && to <= r.end)
                        .map_or(self.style.text_color, |(_, color)| *color),
                    underline: decorations
                        .iter()
                        .find(|(r, _)| r.start <= from && to <= r.end)
                        .map(|(_, u)| *u),
                    ..run(to - from)
                }
            })
            .collect();
        runs.retain(|r| r.len > 0);
        let lines: Rc<[(usize, WrappedLine)]> =
            shape_lines(&display, &runs, font_size, wrap, &widths, window).into();
        let placeholder = match (&self.style.placeholder, display.is_empty()) {
            (Some(placeholder), true) => {
                let runs = [TextRun {
                    color: self.style.placeholder_color,
                    ..run(placeholder.len())
                }];
                shape(placeholder, &runs, font_size, wrap, window)
                    .into_iter()
                    .next()
                    .map(|(_, line)| line)
            }
            _ => None,
        };
        let content = Size {
            width: lines
                .iter()
                .map(|(_, l)| l.size(line_height).width)
                .fold(px(0.), Pixels::max),
            height: line_height * rows(lines.iter().map(|(_, l)| l)) as f32,
        };
        let selection = buffer.selection();
        let cursor = buffer.cursor();
        let collapsed = buffer.is_collapsed();
        let previous_scroll = state.layout.as_ref().map(|l| l.scroll).unwrap_or_default();

        // Lay out first, so positions can be read from it.
        self.state.update(cx, |state, _| {
            state.layout = Some(EditorLayout {
                lines: lines.clone(),
                bounds,
                line_height,
                scroll: previous_scroll,
            });
        });
        let state = self.state.read(cx);
        let caret_at = state.position_for_offset(cursor).unwrap_or_default();

        // Scroll the caret into view.
        let mut scroll = previous_scroll;
        if self.style.multiline {
            let max = (content.height - bounds.size.height).max(px(0.));
            if caret_at.y < scroll.y {
                scroll.y = caret_at.y;
            } else if caret_at.y + line_height > scroll.y + bounds.size.height {
                scroll.y = caret_at.y + line_height - bounds.size.height;
            }
            scroll.y = scroll.y.clamp(px(0.), max);
            scroll.x = px(0.);
        } else {
            let max = (content.width + CARET_WIDTH - bounds.size.width).max(px(0.));
            if caret_at.x < scroll.x {
                scroll.x = caret_at.x;
            } else if caret_at.x + CARET_WIDTH > scroll.x + bounds.size.width {
                scroll.x = caret_at.x + CARET_WIDTH - bounds.size.width;
            }
            scroll.x = scroll.x.clamp(px(0.), max);
            scroll.y = px(0.);
        }
        let origin = bounds.origin - scroll;

        let mut rects = Vec::new();
        if !collapsed {
            let start = state
                .position_for_offset(selection.start)
                .unwrap_or_default();
            let end = state.position_for_offset(selection.end).unwrap_or_default();
            if start.y == end.y {
                rects.push(Bounds::from_corners(
                    origin + start,
                    origin + point(end.x, end.y + line_height),
                ));
            } else {
                let right = bounds.size.width.max(content.width);
                rects.push(Bounds::from_corners(
                    origin + start,
                    origin + point(right, start.y + line_height),
                ));
                if end.y > start.y + line_height {
                    rects.push(Bounds::from_corners(
                        origin + point(px(0.), start.y + line_height),
                        origin + point(right, end.y),
                    ));
                }
                rects.push(Bounds::from_corners(
                    origin + point(px(0.), end.y),
                    origin + point(end.x, end.y + line_height),
                ));
            }
        }
        let caret =
            collapsed.then(|| Bounds::new(origin + caret_at, size(CARET_WIDTH, line_height)));
        let places: Vec<Point<Pixels>> = boxes
            .iter()
            .map(|b| origin + state.position_for_offset(b.offset).unwrap_or_default())
            .collect();
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (object, at) in boxes.iter_mut().zip(places) {
                object.element.prepaint_at(at, window, cx);
            }
        });
        self.state.update(cx, |state, _| {
            if let Some(layout) = &mut state.layout {
                layout.scroll = scroll;
            }
        });
        Prepainted {
            lines,
            placeholder,
            selection: rects,
            caret,
            scroll,
            line_height,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        boxes: &mut Self::RequestLayoutState,
        prepainted: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus, disabled, since) = {
            let state = self.state.read(cx);
            (
                gpui::Focusable::focus_handle(state, cx),
                state.settings.disabled,
                state.caret_since,
            )
        };
        let focused = focus.is_focused(window);
        if !disabled {
            window.handle_input(
                &focus,
                ElementInputHandler::new(bounds, self.state.clone()),
                cx,
            );
        }
        let line_height = prepainted.line_height;
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            if focused {
                for rect in prepainted.selection.drain(..) {
                    window.paint_quad(fill(rect, self.style.selection_color));
                }
            }
            let origin = bounds.origin - prepainted.scroll;
            if let Some(placeholder) = &prepainted.placeholder {
                placeholder
                    .paint(
                        origin,
                        line_height,
                        gpui::TextAlign::Left,
                        Some(bounds),
                        window,
                        cx,
                    )
                    .ok();
            }
            let mut y = px(0.);
            for (_, line) in prepainted.lines.iter() {
                line.paint(
                    origin + point(px(0.), y),
                    line_height,
                    gpui::TextAlign::Left,
                    Some(bounds),
                    window,
                    cx,
                )
                .ok();
                y += line.size(line_height).height;
            }
            for object in boxes.iter_mut() {
                object.element.paint(window, cx);
            }

            // The caret: solid for a moment after each change, then blinking
            // (never under reduced motion).
            if focused && let Some(caret) = prepainted.caret {
                let now = cx.background_executor().now();
                let elapsed =
                    since.map_or(Duration::ZERO, |since| now.saturating_duration_since(since));
                let phase = elapsed.as_millis() / BLINK.as_millis();
                let visible = self.style.reduced_motion || phase.is_multiple_of(2);
                if visible {
                    window.paint_quad(fill(caret, self.style.caret_color));
                }
                if !self.style.reduced_motion {
                    let wait = BLINK
                        - Duration::from_millis((elapsed.as_millis() % BLINK.as_millis()) as u64);
                    let state = self.state.clone();
                    if !state.read(cx).blink_scheduled {
                        state.update(cx, |s, _| s.blink_scheduled = true);
                        window
                            .spawn(cx, async move |cx| {
                                cx.background_executor().timer(wait).await;
                                cx.update(|window, cx| {
                                    state.update(cx, |s, _| s.blink_scheduled = false);
                                    window.refresh();
                                })
                                .ok();
                            })
                            .detach();
                    }
                }
            }
        });
    }
}
