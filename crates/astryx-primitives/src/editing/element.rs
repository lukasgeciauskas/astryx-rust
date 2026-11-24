//! The element that draws a field's text: shaped (and in multiline fields,
//! wrapped) lines, the selection, the IME composition's underline, the caret
//! and the placeholder. It scrolls to keep the caret in view and registers the
//! field as the window's IME client while it is focused.
//!
//! Adapted from GPUI's `examples/input.rs` (Apache-2.0, Copyright Zed
//! Industries).

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, AvailableSpace, Bounds, Element, ElementId, ElementInputHandler, Entity, GlobalElementId,
    Hsla, InspectorElementId, LayoutId, Pixels, Point, SharedString, Size, Style, TextRun,
    UnderlineStyle, Window, WrappedLine, fill, point, px, relative, size,
};

use super::state::{EditorLayout, TextEditState};

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
    /// No caret blink.
    pub reduced_motion: bool,
}

/// The caret blinks this often, after holding solid as long after a change.
const BLINK: Duration = Duration::from_millis(500);
const CARET_WIDTH: Pixels = px(1.5);

pub(crate) struct EditorElement {
    pub state: Entity<TextEditState>,
    pub style: EditorStyle,
}

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

impl Element for EditorElement {
    type RequestLayoutState = ();
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
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        if !self.style.multiline {
            style.size.height = line_height.into();
            return (window.request_layout(style, [], cx), ());
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
            let lines: Vec<WrappedLine> = shape(&text, &runs, font_size, width, window)
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
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let wrap = self.style.multiline.then_some(bounds.size.width);
        let state = self.state.read(cx);
        let display = state.display_text();
        let buffer = state.buffer();
        let marked = buffer
            .marked()
            .map(|r| state.offset_to_display(r.start)..state.offset_to_display(r.end));

        // The composition is underlined, as platform fields do.
        let run = |len: usize| TextRun {
            len,
            ..text_style.to_run(len)
        };
        let mut runs = Vec::new();
        match &marked {
            Some(marked) => {
                runs.push(TextRun {
                    color: self.style.text_color,
                    ..run(marked.start)
                });
                runs.push(TextRun {
                    color: self.style.text_color,
                    underline: Some(UnderlineStyle {
                        color: Some(self.style.text_color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..run(marked.end - marked.start)
                });
                runs.push(TextRun {
                    color: self.style.text_color,
                    ..run(display.len() - marked.end)
                });
            }
            None => runs.push(TextRun {
                color: self.style.text_color,
                ..run(display.len())
            }),
        }
        runs.retain(|r| r.len > 0);
        let lines: Rc<[(usize, WrappedLine)]> =
            shape(&display, &runs, font_size, wrap, window).into();
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
        _: &mut Self::RequestLayoutState,
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
