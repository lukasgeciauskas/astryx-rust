//! Inline formatting: a paragraph's text and the inline content inside it,
//! wrapped together into lines.
//!
//! Upstream's Text is an inline `<span>`: the text, nested Texts, Links and
//! any other inline element in a paragraph share its lines, and a Link longer
//! than a line wraps inside itself. GPUI lays text out a paragraph at a time,
//! so a Text with children lays them out here:
//! - text (strings, and the text of an inline Text or Link) breaks at the
//!   Unicode line-break opportunities (UAX #14) and is shaped a piece at a time
//!   in its own style; a word longer than a line breaks between graphemes;
//! - any other element is an atomic inline box at its max-content size,
//!   standing on the baseline like an inline-block; a block Text gets a line
//!   of its own;
//! - a Link is a group: its fragments hover, press and show a focus ring as
//!   one, and a stand-in over its first fragment takes its tab stop, role and
//!   keyboard activation.
//!
//! Lines align to the paragraph's `text_align`, every line is at least the
//! paragraph's own line height (CSS's strut), `white_space: nowrap` keeps one
//! line, and a line clamp ends its last line with an ellipsis.

use std::cell::RefCell;
use std::ops::Range;
use std::panic::Location;
use std::rc::Rc;

use gpui::{
    AnyElement, App, AvailableSpace, BorderStyle, Bounds, ClickEvent, CursorStyle, DispatchPhase,
    Element, ElementId, Entity, FocusHandle, GlobalElementId, Hitbox, HitboxBehavior, Hsla,
    InspectorElementId, IntoElement, LayoutId, MouseButton, MouseClickEvent, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, ShapedLine, SharedString, Size, Style,
    TextAlign, TextOverflow, TextStyle, TextStyleRefinement, ViewElement, WhiteSpace, Window, div,
    point, px, quad, size, transparent_black,
};
use smallvec::SmallVec;
use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;

use super::{Heading, Text};
use crate::link::Link;

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The hover and press state of an inline group, kept by its owner between
/// frames.
#[derive(Default)]
pub(crate) struct InlineGroupState {
    pub hovered: bool,
    pub pressed: Option<MouseDownEvent>,
}

/// A focus ring drawn around each of a group's fragments.
#[derive(Clone, Copy)]
pub(crate) struct InlineRing {
    pub color: Hsla,
    pub width: Pixels,
    pub offset: Pixels,
    pub dashed: bool,
}

/// Fragments that act as one: a link.
pub(crate) struct InlineGroup {
    pub state: Entity<InlineGroupState>,
    /// Pointer activation; `None` leaves the group inert.
    pub on_click: Option<ClickFn>,
    pub focus: Option<FocusHandle>,
    pub ring: Option<InlineRing>,
    /// Takes the group's tab stop, role, name and keyboard activation; laid
    /// out over its first fragment, under the fragments' own hitboxes.
    pub proxy: Option<AnyElement>,
}

enum Item {
    Text {
        text: SharedString,
        style: Box<TextStyle>,
        group: Option<usize>,
    },
    /// A hard break (a newline) ends even an empty line; a soft one (around
    /// a block) ends a line only if it has content.
    Break {
        hard: bool,
    },
    Box {
        index: usize,
    },
}

struct AtomicBox {
    element: AnyElement,
    size: Size<Pixels>,
    /// The text style the box was laid out in, again for prepaint and paint.
    style: TextStyleRefinement,
}

/// Gathers a flow's content: text in its styles, groups and inline boxes.
pub(crate) struct InlineBuilder {
    items: Vec<Item>,
    boxes: Vec<AtomicBox>,
    groups: Vec<InlineGroup>,
    style: TextStyle,
    refinement: TextStyleRefinement,
    group: Option<usize>,
}

impl InlineBuilder {
    /// Text in the current style; a newline breaks the line.
    pub fn text(&mut self, text: SharedString) {
        if !text.contains('\n') {
            if !text.is_empty() {
                self.items.push(Item::Text {
                    text,
                    style: Box::new(self.style.clone()),
                    group: self.group,
                });
            }
            return;
        }
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                self.items.push(Item::Break { hard: true });
            }
            let line = line.strip_suffix('\r').unwrap_or(line);
            if !line.is_empty() {
                self.items.push(Item::Text {
                    text: SharedString::from(line.to_string()),
                    style: Box::new(self.style.clone()),
                    group: self.group,
                });
            }
        }
    }

    /// Runs `f` with `refinement` over the current style.
    pub fn styled(&mut self, refinement: &TextStyleRefinement, f: impl FnOnce(&mut Self)) {
        let (style, outer) = (self.style.clone(), self.refinement.clone());
        gpui::Refineable::refine(&mut self.style, refinement);
        gpui::Refineable::refine(&mut self.refinement, refinement);
        f(self);
        self.style = style;
        self.refinement = outer;
    }

    /// Runs `f` inside a group: the text it adds acts as one.
    pub fn grouped(&mut self, group: InlineGroup, f: impl FnOnce(&mut Self)) {
        let index = self.groups.len();
        self.groups.push(group);
        let outer = self.group.replace(index);
        f(self);
        self.group = outer;
    }

    /// Any child: text and inline Texts and Links join the flow; anything
    /// else is an inline box.
    pub fn child(&mut self, mut element: AnyElement, window: &mut Window, cx: &mut App) {
        if let Some(text) = element.downcast_mut::<&'static str>() {
            let text = *text;
            self.text(text.into());
            return;
        }
        if let Some(text) = element.downcast_mut::<SharedString>() {
            let text = text.clone();
            self.text(text);
            return;
        }
        let mut block = false;
        if let Some(view) = element.downcast_mut::<ViewElement<Text>>() {
            match view.view_mut().map(|text| text.flows_inline()) {
                Some(true) => {
                    if let Some(text) = view.take_view() {
                        text.inline(self, window, cx);
                    }
                    return;
                }
                Some(false) => block = true,
                None => {}
            }
        }
        if let Some(view) = element.downcast_mut::<ViewElement<Heading>>() {
            match view.view_mut().map(|heading| heading.flows_inline()) {
                Some(true) => {
                    if let Some(heading) = view.take_view() {
                        heading.inline(self, window, cx);
                    }
                    return;
                }
                Some(false) => block = true,
                None => {}
            }
        }
        if let Some(view) = element.downcast_mut::<ViewElement<Link>>()
            && view.view_mut().is_some_and(|link| link.flows_inline())
            && let Some(link) = view.take_view()
        {
            link.inline(self, window, cx);
            return;
        }
        self.inline_box(element, block, window, cx);
    }

    /// An atomic inline box at its max-content size; `block` gives it a line
    /// of its own.
    fn inline_box(&mut self, element: AnyElement, block: bool, window: &mut Window, cx: &mut App) {
        // A plain wrapper is the layout root: the element's own root may lay
        // out only as part of a parent (`display: contents`).
        let mut boxed = div().child(element).into_any_element();
        let size = window.with_text_style(Some(self.refinement.clone()), |window| {
            boxed.layout_as_root(
                size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
                window,
                cx,
            )
        });
        if block {
            self.items.push(Item::Break { hard: false });
        }
        self.items.push(Item::Box {
            index: self.boxes.len(),
        });
        self.boxes.push(AtomicBox {
            element: boxed,
            size,
            style: self.refinement.clone(),
        });
        if block {
            self.items.push(Item::Break { hard: false });
        }
    }
}

/// A paragraph whose children flow inline. See the module docs.
pub(crate) struct InlineFlow {
    children: SmallVec<[AnyElement; 2]>,
}

impl InlineFlow {
    pub fn new(children: impl IntoIterator<Item = AnyElement>) -> Self {
        InlineFlow {
            children: children.into_iter().collect(),
        }
    }
}

/// What the line breaker reads: the flow's items, without their elements.
struct FlowData {
    items: Vec<Item>,
    box_sizes: Vec<Size<Pixels>>,
    base: TextStyle,
    rem: Pixels,
    cache: RefCell<Option<CachedLines>>,
}

/// Lines laid out for a width, and whether long words could split.
type CachedLines = (Option<Pixels>, bool, Rc<Lines>);

#[derive(Default)]
struct Lines {
    lines: Vec<Line>,
    width: Pixels,
    height: Pixels,
}

struct Line {
    top: Pixels,
    baseline: Pixels,
    width: Pixels,
    fragments: Vec<Fragment>,
}

enum Fragment {
    Text {
        shaped: Box<ShapedLine>,
        /// The item it shows; `None` for an ellipsis.
        item: Option<usize>,
        x: Pixels,
        line_height: Pixels,
        /// From the top of the fragment's own line box.
        baseline: Pixels,
        group: Option<usize>,
    },
    Box {
        index: usize,
        x: Pixels,
        /// Above the baseline.
        above: Pixels,
    },
}

/// A piece of a word: part of one item.
#[derive(Clone)]
enum Seg {
    Text {
        item: usize,
        range: Range<usize>,
        width: Pixels,
        /// The width of its trailing white space.
        space: Pixels,
    },
    Box {
        index: usize,
        width: Pixels,
    },
}

impl Seg {
    fn width(&self) -> Pixels {
        match self {
            Seg::Text { width, .. } | Seg::Box { width, .. } => *width,
        }
    }
}

impl FlowData {
    fn shape(&self, item: usize, range: Range<usize>, window: &Window) -> ShapedLine {
        let Item::Text { text, style, .. } = &self.items[item] else {
            unreachable!("only text is shaped")
        };
        let slice: SharedString = if range.start == 0 && range.end == text.len() {
            text.clone()
        } else {
            text[range.clone()].to_string().into()
        };
        let font_size = style.font_size.to_pixels(self.rem);
        window
            .text_system()
            .shape_line(slice, font_size, &[style.to_run(range.len())], None)
    }

    fn text_seg(&self, item: usize, range: Range<usize>, window: &Window) -> Seg {
        let Item::Text { text, .. } = &self.items[item] else {
            unreachable!("a text segment is text")
        };
        let shaped = self.shape(item, range.clone(), window);
        let slice = &text[range.clone()];
        let trimmed = slice.trim_end().len();
        let space = if trimmed < slice.len() {
            shaped.width - shaped.x_for_index(trimmed)
        } else {
            px(0.)
        };
        Seg::Text {
            item,
            range,
            width: shaped.width,
            space,
        }
    }

    /// The line height and baseline of a text style.
    fn metrics(&self, style: &TextStyle, window: &Window) -> (Pixels, Pixels) {
        let line_height = window.pixel_snap(style.line_height_in_pixels(self.rem));
        let font_size = style.font_size.to_pixels(self.rem);
        let text_system = window.text_system();
        let font = text_system.resolve_font(&style.font());
        let baseline = text_system.baseline_offset(font, font_size, line_height);
        (line_height, baseline)
    }

    /// The lines for `width` (`None`: no wrapping). `split` lets a word
    /// longer than a line break between graphemes.
    fn lines(&self, width: Option<Pixels>, split: bool, window: &Window) -> Rc<Lines> {
        if let Some((w, s, lines)) = self.cache.borrow().as_ref()
            && *w == width
            && *s == split
        {
            return lines.clone();
        }
        let lines = Rc::new(self.lay_out(width, split, window));
        *self.cache.borrow_mut() = Some((width, split, lines.clone()));
        lines
    }

    fn lay_out(&self, width: Option<Pixels>, split: bool, window: &Window) -> Lines {
        let nowrap = self.base.white_space == WhiteSpace::Nowrap;
        let wrap_width = if nowrap { None } else { width };

        // The paragraph as one string, for UAX #14: a box is an object
        // replacement character, a break a newline.
        let mut combined = String::new();
        let mut starts = Vec::with_capacity(self.items.len());
        for item in &self.items {
            starts.push(combined.len());
            match item {
                Item::Text { text, .. } => combined.push_str(text),
                Item::Break { .. } => combined.push('\n'),
                Item::Box { .. } => combined.push('\u{FFFC}'),
            }
        }

        // Words: the runs between break opportunities, as segments of items.
        struct Word {
            segs: Vec<Seg>,
            hard: Option<bool>,
        }
        let mut words: Vec<Word> = Vec::new();
        let mut from = 0;
        let mut item = 0;
        for (at, opportunity) in linebreaks(&combined) {
            if nowrap && opportunity == BreakOpportunity::Allowed {
                continue;
            }
            let mut segs = Vec::new();
            let mut hard = None;
            while item < self.items.len() {
                let start = starts[item];
                let end = starts.get(item + 1).copied().unwrap_or(combined.len());
                if start >= at {
                    break;
                }
                let lo = from.max(start) - start;
                let hi = at.min(end) - start;
                if lo < hi {
                    match &self.items[item] {
                        Item::Text { .. } => segs.push(self.text_seg(item, lo..hi, window)),
                        Item::Box { index } => segs.push(Seg::Box {
                            index: *index,
                            width: self.box_sizes[*index].width,
                        }),
                        Item::Break { hard: h } => hard = Some(*h),
                    }
                }
                if end <= at {
                    item += 1;
                } else {
                    break;
                }
            }
            words.push(Word { segs, hard });
            from = at;
        }

        // Greedy filling.
        let eps = px(0.5);
        let mut raw: Vec<Vec<Seg>> = Vec::new();
        let mut current: Vec<Seg> = Vec::new();
        let mut x = px(0.);
        for word in words {
            let mut segs = word.segs;
            loop {
                if segs.is_empty() {
                    break;
                }
                let full: Pixels = segs.iter().map(Seg::width).sum();
                let space = match segs.last() {
                    Some(Seg::Text { space, .. }) => *space,
                    _ => px(0.),
                };
                let trimmed = full - space;
                if let Some(limit) = wrap_width
                    && !current.is_empty()
                    && x + trimmed > limit + eps
                {
                    raw.push(std::mem::take(&mut current));
                    x = px(0.);
                }
                match wrap_width {
                    Some(limit) if split && current.is_empty() && trimmed > limit + eps => {
                        let (head, tail) = self.split_word(segs, limit, window);
                        raw.push(head);
                        segs = tail;
                    }
                    _ => {
                        x += full;
                        current.extend(segs);
                        break;
                    }
                }
            }
            match word.hard {
                Some(true) => {
                    raw.push(std::mem::take(&mut current));
                    x = px(0.);
                }
                Some(false) if !current.is_empty() => {
                    raw.push(std::mem::take(&mut current));
                    x = px(0.);
                }
                _ => {}
            }
        }
        if !current.is_empty() || raw.is_empty() {
            raw.push(current);
        }

        // A line clamp, or one line that must not wrap, ends in an ellipsis.
        let clamp = self
            .base
            .line_clamp
            .or_else(|| (nowrap && self.base.text_overflow.is_some()).then_some(1));
        let mut ellipsis_on = None;
        if let Some(n) = clamp.filter(|n| *n > 0)
            && self.base.text_overflow.is_some()
        {
            let overflows_width = |segs: &Vec<Seg>| {
                width.is_some_and(|w| segs.iter().map(Seg::width).sum::<Pixels>() > w + eps)
            };
            if raw.len() > n || raw.get(n - 1).is_some_and(overflows_width) {
                raw.truncate(n);
                ellipsis_on = Some(n - 1);
            }
        }

        // Shape each line's runs and stack the lines.
        let (strut_height, strut_baseline) = self.metrics(&self.base, window);
        let mut out = Lines::default();
        let mut top = px(0.);
        for (index, segs) in raw.into_iter().enumerate() {
            let mut fragments = self.fragments(segs, window);
            if ellipsis_on == Some(index)
                && let Some(w) = width
            {
                self.ellipsize(&mut fragments, w, window);
            }
            // Baseline alignment, never shorter than the strut.
            let mut above = strut_baseline;
            let mut below = strut_height - strut_baseline;
            for fragment in &fragments {
                match fragment {
                    Fragment::Text {
                        line_height,
                        baseline,
                        ..
                    } => {
                        above = above.max(*baseline);
                        below = below.max(*line_height - *baseline);
                    }
                    Fragment::Box {
                        index, above: a, ..
                    } => {
                        above = above.max(*a);
                        below = below.max(self.box_sizes[*index].height - *a);
                    }
                }
            }
            let line_width = fragments
                .iter()
                .map(|f| match f {
                    Fragment::Text { shaped, x, .. } => *x + shaped.width,
                    Fragment::Box { index, x, .. } => *x + self.box_sizes[*index].width,
                })
                .fold(px(0.), Pixels::max);
            out.width = out.width.max(line_width);
            out.lines.push(Line {
                top,
                baseline: above,
                width: line_width,
                fragments,
            });
            top += above + below;
        }
        out.height = top;
        out
    }

    /// Splits a word wider than `limit`: as much as fits (at least one
    /// grapheme), and the rest.
    fn split_word(&self, segs: Vec<Seg>, limit: Pixels, window: &Window) -> (Vec<Seg>, Vec<Seg>) {
        let mut head = Vec::new();
        let mut x = px(0.);
        let mut rest = segs.into_iter();
        while let Some(seg) = rest.next() {
            if x + seg.width() <= limit {
                x += seg.width();
                head.push(seg);
                continue;
            }
            match seg {
                Seg::Box { .. } => {
                    if head.is_empty() {
                        head.push(seg);
                        return (head, rest.collect());
                    }
                    let mut tail = vec![seg];
                    tail.extend(rest);
                    return (head, tail);
                }
                Seg::Text { item, range, .. } => {
                    let Item::Text { text, .. } = &self.items[item] else {
                        unreachable!("a text segment is text")
                    };
                    let shaped = self.shape(item, range.clone(), window);
                    let slice = &text[range.clone()];
                    let at = shaped.index_for_x(limit - x).unwrap_or(slice.len());
                    // Down to a grapheme boundary; at least one grapheme on
                    // an empty line.
                    let mut cut = slice
                        .grapheme_indices(true)
                        .map(|(i, _)| i)
                        .take_while(|i| *i <= at)
                        .last()
                        .unwrap_or(0);
                    if cut == 0 && head.is_empty() {
                        cut = slice
                            .grapheme_indices(true)
                            .nth(1)
                            .map_or(slice.len(), |(i, _)| i);
                    }
                    if cut > 0 {
                        head.push(self.text_seg(item, range.start..range.start + cut, window));
                    }
                    let mut tail = Vec::new();
                    if range.start + cut < range.end {
                        tail.push(self.text_seg(item, range.start + cut..range.end, window));
                    }
                    tail.extend(rest);
                    return (head, tail);
                }
            }
        }
        (head, Vec::new())
    }

    /// A line's fragments: adjacent pieces of one item shaped together, the
    /// trailing white space of the line dropped.
    fn fragments(&self, segs: Vec<Seg>, window: &Window) -> Vec<Fragment> {
        // Merge runs of one item.
        let mut runs: Vec<Seg> = Vec::new();
        for seg in segs {
            if let (
                Some(Seg::Text {
                    item: last_item,
                    range: last_range,
                    ..
                }),
                Seg::Text { item, range, .. },
            ) = (runs.last_mut(), &seg)
                && *last_item == *item
                && last_range.end == range.start
            {
                last_range.end = range.end;
                continue;
            }
            runs.push(seg);
        }
        // Drop the line's trailing white space.
        if let Some(Seg::Text { item, range, .. }) = runs.last_mut()
            && let Item::Text { text, .. } = &self.items[*item]
        {
            let trimmed = text[range.clone()].trim_end().len();
            range.end = range.start + trimmed;
            if range.start == range.end {
                runs.pop();
            }
        }
        let mut x = px(0.);
        let mut fragments = Vec::with_capacity(runs.len());
        for run in runs {
            match run {
                Seg::Text { item, range, .. } => {
                    let Item::Text { style, group, .. } = &self.items[item] else {
                        unreachable!("a text segment is text")
                    };
                    let shaped = self.shape(item, range, window);
                    let (line_height, baseline) = self.metrics(style, window);
                    let width = shaped.width;
                    fragments.push(Fragment::Text {
                        shaped: Box::new(shaped),
                        item: Some(item),
                        x,
                        line_height,
                        baseline,
                        group: *group,
                    });
                    x += width;
                }
                Seg::Box { index, width } => {
                    // An inline box stands on the baseline: its bottom sits
                    // the paragraph's descent below it, as a line of text
                    // in the paragraph's style would.
                    let (strut_height, strut_baseline) = self.metrics(&self.base, window);
                    let height = self.box_sizes[index].height;
                    let below = (strut_height - strut_baseline).min(height);
                    fragments.push(Fragment::Box {
                        index,
                        x,
                        above: height - below,
                    });
                    x += width;
                }
            }
        }
        fragments
    }

    /// Ends a clamped line in an ellipsis within `width`.
    fn ellipsize(&self, fragments: &mut Vec<Fragment>, width: Pixels, window: &Window) {
        let ellipsis: SharedString = match &self.base.text_overflow {
            Some(TextOverflow::Truncate(s))
            | Some(TextOverflow::TruncateStart(s))
            | Some(TextOverflow::TruncateMiddle(s)) => s.clone(),
            None => "\u{2026}".into(),
        };
        // In the style of the line's last text.
        let style = fragments
            .iter()
            .rev()
            .find_map(|f| match f {
                Fragment::Text {
                    item: Some(item), ..
                } => match &self.items[*item] {
                    Item::Text { style, .. } => Some((**style).clone()),
                    _ => None,
                },
                _ => None,
            })
            .unwrap_or_else(|| self.base.clone());
        let mark = window.text_system().shape_line(
            ellipsis.clone(),
            style.font_size.to_pixels(self.rem),
            &[style.to_run(ellipsis.len())],
            None,
        );
        let target = width - mark.width;
        while let Some(last) = fragments.last_mut() {
            let (x, end) = match last {
                Fragment::Text { shaped, x, .. } => (*x, *x + shaped.width),
                Fragment::Box { index, x, .. } => (*x, *x + self.box_sizes[*index].width),
            };
            if end <= target {
                break;
            }
            let Fragment::Text {
                shaped,
                item: Some(item),
                ..
            } = last
            else {
                fragments.pop();
                continue;
            };
            if x >= target {
                fragments.pop();
                continue;
            }
            let Item::Text { style, .. } = &self.items[*item] else {
                unreachable!("a text fragment is text")
            };
            let at = shaped.index_for_x(target - x).unwrap_or(0);
            let text = shaped.text.clone();
            let keep = text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|i| *i <= at)
                .last()
                .unwrap_or(0);
            let kept = text[..keep].trim_end();
            if kept.is_empty() {
                fragments.pop();
                continue;
            }
            **shaped = window.text_system().shape_line(
                kept.to_string().into(),
                style.font_size.to_pixels(self.rem),
                &[style.to_run(kept.len())],
                None,
            );
            break;
        }
        let x = fragments
            .last()
            .map(|f| match f {
                Fragment::Text { shaped, x, .. } => *x + shaped.width,
                Fragment::Box { index, x, .. } => *x + self.box_sizes[*index].width,
            })
            .unwrap_or(px(0.));
        let (line_height, baseline) = self.metrics(&style, window);
        fragments.push(Fragment::Text {
            shaped: Box::new(mark),
            item: None,
            x,
            line_height,
            baseline,
            group: None,
        });
    }
}

pub(crate) struct FlowFrame {
    data: Rc<FlowData>,
    boxes: Vec<AtomicBox>,
    groups: Vec<InlineGroup>,
}

pub(crate) struct FlowPaint {
    lines: Rc<Lines>,
    offsets: Vec<Pixels>,
    /// Each group's fragment bounds and hitboxes.
    fragments: Vec<Vec<(Bounds<Pixels>, Hitbox)>>,
    proxies: Vec<bool>,
}

impl Element for InlineFlow {
    type RequestLayoutState = FlowFrame;
    type PrepaintState = FlowPaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, FlowFrame) {
        let base = window.text_style();
        let mut builder = InlineBuilder {
            items: Vec::new(),
            boxes: Vec::new(),
            groups: Vec::new(),
            style: base.clone(),
            refinement: TextStyleRefinement::default(),
            group: None,
        };
        for child in self.children.drain(..) {
            builder.child(child, window, cx);
        }
        let data = Rc::new(FlowData {
            items: builder.items,
            box_sizes: builder.boxes.iter().map(|b| b.size).collect(),
            base,
            rem: window.rem_size(),
            cache: RefCell::new(None),
        });
        let measure = data.clone();
        let layout_id =
            window.request_measured_layout(Style::default(), move |known, available, window, _| {
                let min_content =
                    known.width.is_none() && available.width == AvailableSpace::MinContent;
                let width = if min_content {
                    Some(px(0.))
                } else {
                    known.width.or(match available.width {
                        AvailableSpace::Definite(w) => Some(w),
                        _ => None,
                    })
                };
                let lines = measure.lines(width, !min_content, window);
                size(known.width.unwrap_or(lines.width), lines.height)
            });
        let (_, baseline) = data.metrics(&data.base, window);
        let first = data.items.iter().find_map(|item| match item {
            Item::Text { style, .. } => Some(data.metrics(style, window).1),
            _ => None,
        });
        window.set_layout_baseline(layout_id, first.unwrap_or(baseline).max(baseline));
        (
            layout_id,
            FlowFrame {
                data,
                boxes: builder.boxes,
                groups: builder.groups,
            },
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut FlowFrame,
        window: &mut Window,
        cx: &mut App,
    ) -> FlowPaint {
        let lines = frame.data.lines(Some(bounds.size.width), true, window);
        let align = frame.data.base.text_align;
        let mut offsets = Vec::with_capacity(lines.lines.len());
        let mut placed: Vec<Vec<Bounds<Pixels>>> = vec![Vec::new(); frame.groups.len()];
        for line in &lines.lines {
            let dx = match align {
                TextAlign::Left => px(0.),
                TextAlign::Center => ((bounds.size.width - line.width) / 2.).max(px(0.)),
                TextAlign::Right => (bounds.size.width - line.width).max(px(0.)),
            };
            offsets.push(dx);
            for fragment in &line.fragments {
                match fragment {
                    Fragment::Text {
                        shaped,
                        x,
                        line_height,
                        baseline,
                        group: Some(group),
                        ..
                    } => placed[*group].push(Bounds {
                        origin: bounds.origin
                            + point(dx + *x, line.top + line.baseline - *baseline),
                        size: size(shaped.width, *line_height),
                    }),
                    Fragment::Text { .. } => {}
                    Fragment::Box { index, x, above } => {
                        let origin =
                            bounds.origin + point(dx + *x, line.top + line.baseline - *above);
                        let atomic = &mut frame.boxes[*index];
                        window.with_text_style(Some(atomic.style.clone()), |window| {
                            atomic.element.prepaint_at(origin, window, cx)
                        });
                    }
                }
            }
        }
        // The stand-ins first, so the fragments' hitboxes sit over them.
        let mut proxies = vec![false; frame.groups.len()];
        for (index, group) in frame.groups.iter_mut().enumerate() {
            if let (Some(proxy), Some(first)) = (group.proxy.as_mut(), placed[index].first()) {
                proxy.layout_as_root(first.size.map(AvailableSpace::Definite), window, cx);
                proxy.prepaint_at(first.origin, window, cx);
                proxies[index] = true;
            }
        }
        let fragments = placed
            .into_iter()
            .map(|bounds| {
                bounds
                    .into_iter()
                    .map(|b| (b, window.insert_hitbox(b, HitboxBehavior::Normal)))
                    .collect()
            })
            .collect();
        FlowPaint {
            lines,
            offsets,
            fragments,
            proxies,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        frame: &mut FlowFrame,
        paint: &mut FlowPaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        for (line, dx) in paint.lines.lines.iter().zip(&paint.offsets) {
            for fragment in &line.fragments {
                match fragment {
                    Fragment::Text {
                        shaped,
                        x,
                        line_height,
                        baseline,
                        ..
                    } => {
                        let origin =
                            bounds.origin + point(*dx + *x, line.top + line.baseline - *baseline);
                        shaped
                            .paint_background(
                                origin,
                                *line_height,
                                TextAlign::Left,
                                None,
                                window,
                                cx,
                            )
                            .ok();
                        shaped
                            .paint(origin, *line_height, TextAlign::Left, None, window, cx)
                            .ok();
                    }
                    Fragment::Box { index, .. } => {
                        let atomic = &mut frame.boxes[*index];
                        window.with_text_style(Some(atomic.style.clone()), |window| {
                            atomic.element.paint(window, cx)
                        });
                    }
                }
            }
        }

        for (index, group) in frame.groups.iter_mut().enumerate() {
            if paint.proxies[index]
                && let Some(proxy) = group.proxy.as_mut()
            {
                proxy.paint(window, cx);
            }
            let fragments = &paint.fragments[index];
            if let Some(ring) = group.ring {
                let grow = ring.width + ring.offset;
                for (b, _) in fragments {
                    window.paint_quad(quad(
                        b.dilate(grow),
                        px(0.),
                        transparent_black(),
                        ring.width,
                        ring.color,
                        if ring.dashed {
                            BorderStyle::Dashed
                        } else {
                            BorderStyle::Solid
                        },
                    ));
                }
            }
            let hitboxes: Rc<Vec<Hitbox>> =
                Rc::new(fragments.iter().map(|(_, h)| h.clone()).collect());
            let state = group.state.clone();
            window.on_mouse_event({
                let hitboxes = hitboxes.clone();
                let state = state.clone();
                move |_: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let hovered = hitboxes.iter().any(|h| h.is_hovered(window));
                    if state.read(cx).hovered != hovered {
                        state.update(cx, |s, _| s.hovered = hovered);
                        window.refresh();
                    }
                }
            });
            let Some(on_click) = group.on_click.clone() else {
                continue;
            };
            for hitbox in hitboxes.iter() {
                window.set_cursor_style(CursorStyle::PointingHand, hitbox);
            }
            let focus = group.focus.clone();
            window.on_mouse_event({
                let hitboxes = hitboxes.clone();
                let state = state.clone();
                move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble
                        || event.button != MouseButton::Left
                        || !hitboxes.iter().any(|h| h.is_hovered(window))
                    {
                        return;
                    }
                    if let Some(focus) = &focus {
                        window.focus(focus, cx);
                    }
                    state.update(cx, |s, _| s.pressed = Some(event.clone()));
                    window.refresh();
                    cx.stop_propagation();
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                    return;
                }
                let Some(down) = state.update(cx, |s, _| s.pressed.take()) else {
                    return;
                };
                window.refresh();
                if hitboxes.iter().any(|h| h.is_hovered(window)) {
                    let click = ClickEvent::Mouse(MouseClickEvent {
                        down,
                        up: event.clone(),
                    });
                    on_click(&click, window, cx);
                }
            });
        }
    }
}
