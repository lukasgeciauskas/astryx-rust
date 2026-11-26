//! Streaming text, upstream `hooks/useStreamingText.ts` and `Markdown/streaming.ts`: the
//! steady reveal of bursty streamed text, and the fade boundaries Markdown draws its
//! newest text with.

use std::time::Duration;

use astryx_theme::current;
use gpui::{App, ElementId, SharedString, Window};
use unicode_segmentation::UnicodeSegmentation;
use web_time::Instant;

/// Upstream `StreamingTextSpeed`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StreamingTextSpeed {
    /// A steady reveal.
    #[default]
    Natural,
    /// A faster reveal.
    Fast,
    /// No animation: the whole text at once.
    Instant,
}

/// Upstream `UseStreamingTextOptions`.
#[derive(Clone, Copy, Debug, Default)]
pub struct UseStreamingTextOptions {
    pub speed: StreamingTextSpeed,
}

/// Upstream's fallbacks when the theme has no `--duration-fast-min`.
const FALLBACK_TICK_MS: f32 = 50.;
const FALLBACK_TICK_MS_FAST: f32 = 8.;

/// What the reveal remembers between frames.
struct Reveal {
    /// UTF-16 code units shown, as upstream counts.
    len: usize,
    last_tick: Option<Instant>,
    was_streaming: bool,
}

/// `offset` UTF-16 code units into `text`, snapped down to the start of the grapheme
/// cluster it falls inside, as a byte index (upstream `snapToGraphemeBoundary`).
pub(crate) fn snap_to_grapheme_boundary(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (start, grapheme) in text.grapheme_indices(true) {
        let width: usize = grapheme.chars().map(char::len_utf16).sum();
        if units + width > offset {
            return start;
        }
        units += width;
    }
    text.len()
}

/// Upstream `useStreamingText`: text that grows steadily toward `target` while
/// `streaming`, by a fixed number of UTF-16 units per tick of the theme's motion tokens,
/// cut at a grapheme boundary. Not streaming, at `Instant` speed, or with reduced motion,
/// it is the whole `target`. The reveal restarts when the target empties, and finishes
/// at once when streaming stops.
///
/// ```ignore
/// let shown = use_streaming_text("reply", &reply, is_streaming, Default::default(), window, cx);
/// Markdown::new("reply-md", shown)
/// ```
pub fn use_streaming_text(
    id: impl Into<ElementId>,
    target: &str,
    streaming: bool,
    options: UseStreamingTextOptions,
    window: &mut Window,
    cx: &mut App,
) -> SharedString {
    let id = id.into();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-streaming-text").into()),
        cx,
        |_, _| Reveal {
            len: 0,
            last_tick: None,
            was_streaming: streaming,
        },
    );
    let total: usize = target.encode_utf16().count();
    let speed = options.speed;
    let animates = streaming
        && speed != StreamingTextSpeed::Instant
        && !astryx_primitives::prefers_reduced_motion(cx);
    let tick = {
        let base = current(window, cx).tokens().duration.fast_min.as_secs_f32() * 1000.;
        let ms = if base > 0. {
            let tick = if speed == StreamingTextSpeed::Fast {
                base / 20.
            } else {
                base / 10.
            };
            tick.round().max(4.)
        } else if speed == StreamingTextSpeed::Fast {
            FALLBACK_TICK_MS_FAST
        } else {
            FALLBACK_TICK_MS
        };
        Duration::from_secs_f32(ms / 1000.)
    };
    let per_tick = if speed == StreamingTextSpeed::Fast {
        4
    } else {
        10
    };
    let now = cx.background_executor().now();
    let len = state.update(cx, |reveal, _| {
        if total == 0 {
            reveal.len = 0;
        }
        if streaming != reveal.was_streaming {
            reveal.was_streaming = streaming;
            if !streaming && total > 0 {
                reveal.len = total;
            }
        }
        if animates && reveal.len < total {
            match reveal.last_tick {
                Some(last) if now.duration_since(last) < tick => {}
                _ => {
                    reveal.last_tick = Some(now);
                    reveal.len = (reveal.len + per_tick).min(total);
                }
            }
        }
        reveal.len
    });
    if !animates {
        return SharedString::from(target.to_string());
    }
    if len < total {
        window.request_animation_frame();
    }
    SharedString::from(target[..snap_to_grapheme_boundary(target, len)].to_string())
}

/// Upstream `computeBoundaries`: the offsets where the recent renders' text ended,
/// oldest first, at most `max_spans` of them. Text before the first has settled; each
/// later span fades in on its own.
pub(crate) fn compute_boundaries(
    previous: &[usize],
    rendered_len: usize,
    max_spans: usize,
) -> Vec<usize> {
    if previous.last().is_none_or(|&last| rendered_len > last) {
        let mut next = previous.to_vec();
        next.push(rendered_len);
        while next.len() > max_spans {
            next.remove(0);
        }
        return next;
    }
    previous.to_vec()
}

/// One piece of a text node: settled, or fading in from the boundary it starts at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextSegment {
    pub text: String,
    /// The boundary a fading piece starts at; `None` once settled.
    pub fading_from: Option<usize>,
}

/// Upstream `computeSegments`: `content`, which starts `start` units into the rendered
/// text, split at the boundaries; `None` when all of it has settled. Offsets count what
/// the renderer counts (bytes here).
pub(crate) fn compute_segments(
    content: &str,
    start: usize,
    boundaries: &[usize],
) -> Option<Vec<TextSegment>> {
    let end = start + content.len();
    let &oldest = boundaries.first()?;
    if end <= oldest {
        return None;
    }
    let cut = |at: usize| {
        let mut i = at - start;
        while !content.is_char_boundary(i) {
            i -= 1;
        }
        i
    };
    let mut segments = Vec::new();
    let mut pos = start;
    if pos < oldest && oldest < end {
        segments.push(TextSegment {
            text: content[..cut(oldest)].to_string(),
            fading_from: None,
        });
        pos = oldest;
    }
    for (i, &from) in boundaries.iter().enumerate() {
        let to = boundaries.get(i + 1).copied().unwrap_or(end);
        if from >= end || to <= pos {
            continue;
        }
        let (a, b) = (from.max(pos), to.min(end));
        if a >= b {
            continue;
        }
        segments.push(TextSegment {
            text: content[cut(a)..cut(b)].to_string(),
            fading_from: Some(from),
        });
        pos = b;
    }
    (!segments.is_empty()).then_some(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_keep_the_recent_ends() {
        assert_eq!(compute_boundaries(&[], 5, 3), [5]);
        assert_eq!(
            compute_boundaries(&[5], 5, 3),
            [5],
            "no growth, no boundary"
        );
        assert_eq!(compute_boundaries(&[1, 2, 3], 4, 3), [2, 3, 4]);
    }

    #[test]
    fn segments_split_at_the_boundaries() {
        assert_eq!(compute_segments("abc", 0, &[5]), None, "settled");
        assert_eq!(
            compute_segments("hello world", 0, &[6, 9]),
            Some(vec![
                TextSegment {
                    text: "hello ".into(),
                    fading_from: None
                },
                TextSegment {
                    text: "wor".into(),
                    fading_from: Some(6)
                },
                TextSegment {
                    text: "ld".into(),
                    fading_from: Some(9)
                },
            ])
        );
        assert_eq!(
            compute_segments("ld!", 9, &[6, 9]),
            Some(vec![TextSegment {
                text: "ld!".into(),
                fading_from: Some(9)
            }])
        );
    }

    #[test]
    fn the_reveal_never_splits_a_grapheme() {
        let text = "a👍🏽b";
        assert_eq!(snap_to_grapheme_boundary(text, 1), 1);
        assert_eq!(snap_to_grapheme_boundary(text, 2), 1, "inside the emoji");
        assert_eq!(snap_to_grapheme_boundary(text, 5), text.len() - 1);
    }
}
