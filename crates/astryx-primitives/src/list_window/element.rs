//! The GPUI adapter of the list-windowing engine: a list that mounts only
//! the rows covering its visible part (upstream `useListWindow`,
//! `useInlineListWindow` and `useListFocusIsland`).
//!
//! The list reads its geometry each frame from where it sits: its bounds and
//! the clip of every scroller above it. So it windows inside a container
//! scroll area and on a scrolling page alike, with no scroll-ref API asked of
//! its caller (`architecture:list-windowing` DEC-2, INV4). To hold its anchor
//! through changes (FR1, FR2), it also needs to move that scroller: it takes
//! the [`ScrollHandle`] passed to [`WindowedList::scroll`], else the nearest
//! [`ScrollHost`], which a page provides around its scrolled content.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Context, ElementId, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Pixels, Refineable as _, RenderOnce, ScrollHandle, StyleRefinement, Styled,
    Window, canvas, div, point, px,
};

use super::{
    LIST_WINDOW_OVERSCAN, ListAnchor, ListAnchorOptions, ListWindowKey, ListWindowOptions,
    ListWindowSegment, VariableListRangeOptions, calculate_list_window,
    calculate_variable_list_range, create_variable_segments, is_positive_finite, key_indices,
    logical_inline_scroll, reconcile_anchor,
};
use crate::context::use_context;

/// How a windowed list sizes its rows along its axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListPitch {
    /// Every row has this size.
    Fixed(Pixels),
    /// Rows have their own sizes, measured as they render (FR3); a row not
    /// yet measured counts as this.
    Estimated(Pixels),
}

impl ListPitch {
    fn size(self) -> f64 {
        match self {
            Self::Fixed(p) | Self::Estimated(p) => f32::from(p) as f64,
        }
    }
}

/// The axis a windowed list runs along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowedListAxis {
    /// Rows stack down the block axis.
    #[default]
    Block,
    /// Items run along the inline axis, from the inline start (a table's
    /// columns).
    Inline,
}

/// The scroller a page or region scrolls with. Provided around a
/// scroller's content (`astryx_primitives::provide(ScrollHost(handle), ..)`),
/// it lets a windowed list inside hold its anchor.
#[derive(Clone)]
pub struct ScrollHost(pub ScrollHandle);

type RenderItem = Rc<dyn Fn(usize, &mut Window, &mut App) -> AnyElement>;

/// A long list that renders only the rows covering what shows, plus
/// overscan, and sizes itself for every row.
#[derive(IntoElement)]
pub struct WindowedList {
    id: ElementId,
    keys: Rc<[ListWindowKey]>,
    pitch: ListPitch,
    axis: WindowedListAxis,
    initial_viewport: Pixels,
    overscan: usize,
    inset_start: Pixels,
    inset_end: Pixels,
    anchor: ListAnchor,
    can_window: bool,
    rtl: bool,
    scroll: Option<ScrollHandle>,
    viewport_focus: Option<FocusHandle>,
    render_item: RenderItem,
    style: StyleRefinement,
}

impl WindowedList {
    /// `keys` gives one key per row, in order; `render_item` renders the row
    /// at an index.
    pub fn new(
        id: impl Into<ElementId>,
        keys: impl Into<Rc<[ListWindowKey]>>,
        pitch: ListPitch,
        render_item: impl Fn(usize, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            keys: keys.into(),
            pitch,
            axis: WindowedListAxis::Block,
            initial_viewport: px(0.),
            overscan: LIST_WINDOW_OVERSCAN,
            inset_start: px(0.),
            inset_end: px(0.),
            anchor: ListAnchor::FirstVisible,
            can_window: true,
            rtl: false,
            scroll: None,
            viewport_focus: None,
            render_item: Rc::new(render_item),
            style: StyleRefinement::default(),
        }
    }

    pub fn axis(mut self, axis: WindowedListAxis) -> Self {
        self.axis = axis;
        self
    }

    /// The viewport size to plan the first frame with, before measuring.
    pub fn initial_viewport(mut self, size: Pixels) -> Self {
        self.initial_viewport = size;
        self
    }

    /// Rows mounted past each edge for rows of varying size (fixed rows
    /// always mount four).
    pub fn overscan(mut self, rows: usize) -> Self {
        self.overscan = rows;
        self
    }

    /// Viewport space covered at the inline start and end, such as pinned
    /// columns; only the space between shows items.
    pub fn insets(mut self, start: Pixels, end: Pixels) -> Self {
        self.inset_start = start;
        self.inset_end = end;
        self
    }

    /// What holds its place when the rows change (FR1).
    pub fn anchor(mut self, anchor: ListAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Whether the list windows; `false` renders every row.
    pub fn can_window(mut self, can_window: bool) -> Self {
        self.can_window = can_window;
        self
    }

    /// Right-to-left: the inline start is the right edge.
    pub fn rtl(mut self, rtl: bool) -> Self {
        self.rtl = rtl;
        self
    }

    /// The scroller the list sits in, when it is not the nearest
    /// [`ScrollHost`].
    pub fn scroll(mut self, handle: &ScrollHandle) -> Self {
        self.scroll = Some(handle.clone());
        self
    }

    /// What takes focus when the focused row leaves the data; the list
    /// itself by default.
    pub fn viewport_focus(mut self, handle: &FocusHandle) -> Self {
        self.viewport_focus = Some(handle.clone());
        self
    }
}

impl Styled for WindowedList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What the list read about where it sits, last frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Probe {
    measured: bool,
    /// The distance from the list's start to the viewport's (after the
    /// start inset); negative while the list starts inside the viewport.
    pos: f64,
    /// The space that shows items.
    viewport: f64,
    /// The scroller's logical scroll then, and its limit.
    host_scroll: Option<f64>,
    host_max: f64,
}

struct Planned {
    keys: Rc<[ListWindowKey]>,
    indices: Rc<HashMap<ListWindowKey, usize>>,
    offsets: Option<Rc<Vec<f64>>>,
    pitch: ListPitch,
}

struct WindowState {
    probe: Probe,
    planned: Option<Planned>,
    sizes: HashMap<ListWindowKey, f64>,
    rows: HashMap<ListWindowKey, FocusHandle>,
    focused: Option<ListWindowKey>,
    container: FocusHandle,
    item_opened: bool,
}

fn host_scroll(handle: &ScrollHandle, axis: WindowedListAxis, rtl: bool) -> (f64, f64) {
    let (offset, max) = (handle.offset(), handle.max_offset());
    match axis {
        WindowedListAxis::Block => (-f32::from(offset.y) as f64, f32::from(max.y) as f64),
        WindowedListAxis::Inline => {
            let max = f32::from(max.x) as f64;
            (
                logical_inline_scroll(f32::from(offset.x) as f64, max, rtl),
                max,
            )
        }
    }
}

/// Scrolls the host by `delta` along the list's logical axis.
fn move_host(handle: &ScrollHandle, axis: WindowedListAxis, rtl: bool, delta: f64) {
    let offset = handle.offset();
    let d = px(delta as f32);
    let next = match axis {
        WindowedListAxis::Block => point(offset.x, offset.y - d),
        WindowedListAxis::Inline if rtl => point(offset.x + d, offset.y),
        WindowedListAxis::Inline => point(offset.x - d, offset.y),
    };
    handle.set_offset(next);
}

fn along(bounds: &Bounds<Pixels>, axis: WindowedListAxis) -> f64 {
    f32::from(match axis {
        WindowedListAxis::Block => bounds.size.height,
        WindowedListAxis::Inline => bounds.size.width,
    }) as f64
}

/// The visible range a probe gives, for the probe's own change check.
type Planner = Rc<dyn Fn(&Probe) -> (usize, usize)>;

/// The range for a position, from the engine.
#[allow(clippy::too_many_arguments)]
fn plan_range(
    count: usize,
    pitch: ListPitch,
    offsets: Option<&[f64]>,
    pos: f64,
    viewport: f64,
    measured: bool,
    overscan: usize,
    host: Option<(f64, f64)>,
    focus: Option<usize>,
) -> Option<(Vec<ListWindowSegment>, usize, usize)> {
    match (pitch, offsets) {
        (ListPitch::Estimated(_), Some(offsets)) => {
            let range = calculate_variable_list_range(&VariableListRangeOptions {
                offsets,
                viewport_size: viewport,
                scroll_offset: pos,
                is_measured: measured,
                overscan,
            });
            Some((
                create_variable_segments(offsets, range.start, range.end, focus),
                range.start,
                range.end,
            ))
        }
        _ => {
            let (scroll_top, start_offset, scroll_height) = decompose(pos, viewport, host);
            calculate_list_window(&ListWindowOptions {
                row_count: count,
                row_height: pitch.size(),
                viewport_height: viewport,
                scroll_top,
                scroll_height,
                start_offset,
                is_measured: measured,
                focus_index: focus,
            })
            .map(|plan| (plan.segments, plan.start, plan.end))
        }
    }
}

/// Upstream's geometry (a scroll position, the distance from the scroll
/// origin to the first row, the content extent) for a list position: the
/// scroller's own when it is known, else one relative to the viewport.
fn decompose(pos: f64, viewport: f64, host: Option<(f64, f64)>) -> (f64, f64, Option<f64>) {
    match host {
        Some((scroll, max)) if scroll - pos >= 0. => (scroll, scroll - pos, Some(viewport + max)),
        _ => (pos.max(0.), (-pos).max(0.), None),
    }
}

impl RenderOnce for WindowedList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            keys,
            pitch,
            axis,
            initial_viewport,
            overscan,
            inset_start,
            inset_end,
            anchor,
            can_window,
            rtl,
            scroll,
            viewport_focus,
            render_item,
            style,
        } = self;
        let state = window.use_keyed_state(id.clone(), cx, |_, cx: &mut Context<WindowState>| {
            WindowState {
                probe: Probe::default(),
                planned: None,
                sizes: HashMap::new(),
                rows: HashMap::new(),
                focused: None,
                container: cx.focus_handle(),
                item_opened: false,
            }
        });
        let host = scroll.or_else(|| use_context::<ScrollHost>(cx).map(|h| h.0));
        let count = keys.len();
        let host_now = host.as_ref().map(|h| host_scroll(h, axis, rtl));
        let insets = f32::from(inset_start + inset_end) as f64;

        // The keys' index map, rebuilt only when the keys change.
        let (indices, keys_changed) = match &state.read(cx).planned {
            Some(p) if Rc::ptr_eq(&p.keys, &keys) || *p.keys == *keys => (p.indices.clone(), false),
            Some(_) => (Rc::new(key_indices(&keys)), true),
            None => (Rc::new(key_indices(&keys)), false),
        };
        // Item starts for rows of varying size: measured, else estimated.
        let offsets: Option<Rc<Vec<f64>>> = match pitch {
            ListPitch::Fixed(_) => None,
            ListPitch::Estimated(_) => {
                let sizes = &state.read(cx).sizes;
                let estimate = pitch.size();
                let mut offsets = Vec::with_capacity(count + 1);
                let mut at = 0.;
                offsets.push(at);
                for key in keys.iter() {
                    at += sizes.get(key).copied().unwrap_or(estimate);
                    offsets.push(at);
                }
                Some(Rc::new(offsets))
            }
        };
        let (offsets_changed, pitch_changed) = match &state.read(cx).planned {
            Some(p) => (
                match (&p.offsets, &offsets) {
                    (Some(a), Some(b)) => a != b,
                    (None, None) => false,
                    _ => true,
                },
                p.pitch != pitch,
            ),
            None => (false, false),
        };

        // The focus island: the row holding focus stays mounted; a focused
        // row that leaves the data hands focus to the viewport.
        let (focused_now, container) = {
            let s = state.read(cx);
            let focused = s
                .rows
                .iter()
                .find(|(_, h)| h.contains_focused(window, cx))
                .map(|(k, _)| k.clone());
            (focused, s.container.clone())
        };
        let previous_focus = state.read(cx).focused.clone();
        if let Some(previous) = previous_focus
            && !indices.contains_key(&previous)
            && (focused_now.as_ref() == Some(&previous) || window.focused(cx).is_none())
        {
            let target = viewport_focus.clone().unwrap_or_else(|| container.clone());
            window.defer(cx, move |window, cx| target.focus(window, cx));
        }
        let focused = focused_now.filter(|k| indices.contains_key(k));
        let focus_index = focused.as_ref().and_then(|k| indices.get(k).copied());

        // Where the list sits now: last frame's reading, moved by any scroll
        // since.
        let probe = state.read(cx).probe;
        let measured = can_window && probe.measured && is_positive_finite(probe.viewport);
        let mut pos = probe.pos
            + match (host_now, probe.host_scroll) {
                (Some((now, _)), Some(then)) => now - then,
                _ => 0.,
            };
        let viewport = if measured {
            probe.viewport
        } else {
            (f32::from(initial_viewport) as f64 - insets).max(0.)
        };

        // The anchor holds through a change of rows or sizes (FR1, FR2).
        let mut delta = 0.;
        if can_window
            && measured
            && let (Some(handle), Some((host_pos, host_max))) = (&host, host_now)
        {
            let s = state.read(cx);
            if let Some(previous) = &s.planned
                && (keys_changed || offsets_changed || pitch_changed)
            {
                let previous_offsets = previous.offsets.as_deref().map(|v| v.as_slice());
                let new_offsets = offsets.as_deref().map(|v| v.as_slice());
                let start_in = |o: Option<&[f64]>, pitch: ListPitch, i: usize| match o {
                    Some(o) => o[i],
                    None => i as f64 * pitch.size(),
                };
                let total = |o: Option<&[f64]>, pitch: ListPitch, n: usize| match o {
                    Some(o) => o[n],
                    None => n as f64 * pitch.size(),
                };
                let previous_total = total(previous_offsets, previous.pitch, previous.keys.len());
                let item_starts = |key: &ListWindowKey| {
                    let before = previous.indices.get(key)?;
                    let after = indices.get(key)?;
                    Some((
                        start_in(previous_offsets, previous.pitch, *before),
                        start_in(new_offsets, pitch, *after),
                    ))
                };
                let (scroll_top, start_offset, scroll_height) =
                    decompose(pos, viewport, Some((host_pos, host_max)));
                let to = match (previous_offsets, new_offsets) {
                    (Some(prev), Some(next)) => {
                        // Rows of varying size: the first visible key and
                        // the fraction of it scrolled past.
                        let at = pos.max(0.).min((previous_total - viewport).max(0.));
                        let range = calculate_variable_list_range(&VariableListRangeOptions {
                            offsets: prev,
                            viewport_size: viewport,
                            scroll_offset: pos,
                            is_measured: true,
                            overscan,
                        });
                        let first = range.first_visible_index;
                        let size = prev.get(first + 1).copied().unwrap_or(0.)
                            - prev.get(first).copied().unwrap_or(0.);
                        let options = ListAnchorOptions {
                            previous_keys: &previous.keys,
                            keys: &keys,
                            first_visible_index: first,
                            offset: if size > 0. { range.offset / size } else { 0. },
                            viewport_height: viewport,
                            trailing_content_height: f64::INFINITY,
                            key_indices: Some(&indices),
                            offsets: Some(next),
                            ..ListAnchorOptions::default()
                        };
                        let previous_max = host_max;
                        let target =
                            reconcile_anchor(&anchor, &options, host_pos, previous_max, |k| {
                                item_starts(k)
                                    .map(|(b, a)| (b + host_pos - pos, a + host_pos - pos))
                            });
                        match &anchor {
                            ListAnchor::End if host_pos >= host_max - 0.5 => {
                                total(new_offsets, pitch, keys.len()) - previous_total
                            }
                            ListAnchor::Item(key) if item_starts(key).is_some() => {
                                target - host_pos
                            }
                            _ => target - at,
                        }
                    }
                    _ => {
                        let row_height = pitch.size();
                        let plan = calculate_list_window(&ListWindowOptions {
                            row_count: previous.keys.len(),
                            row_height: previous.pitch.size(),
                            viewport_height: viewport,
                            scroll_top,
                            scroll_height,
                            start_offset,
                            is_measured: true,
                            focus_index: None,
                        });
                        match plan {
                            Some(plan) => {
                                let offset = if plan.offset < 0. {
                                    plan.offset
                                } else {
                                    plan.offset / previous.pitch.size() * row_height
                                };
                                let trailing = scroll_height
                                    .map_or(0., |h| (h - start_offset - previous_total).max(0.));
                                let options = ListAnchorOptions {
                                    previous_keys: &previous.keys,
                                    keys: &keys,
                                    first_visible_index: plan.first_visible_index,
                                    offset,
                                    row_height,
                                    viewport_height: viewport,
                                    start_offset,
                                    trailing_content_height: trailing,
                                    key_indices: Some(&indices),
                                    offsets: None,
                                };
                                let target = reconcile_anchor(
                                    &anchor,
                                    &options,
                                    scroll_top,
                                    host_max,
                                    |k| {
                                        item_starts(k)
                                            .map(|(b, a)| (b + start_offset, a + start_offset))
                                    },
                                );
                                match &anchor {
                                    ListAnchor::End if host_pos >= host_max - 0.5 => {
                                        total(None, pitch, keys.len()) - previous_total
                                    }
                                    _ => target - scroll_top,
                                }
                            }
                            None => 0.,
                        }
                    }
                };
                delta = to;
            }
            // A given item opens in view, at the viewport's start.
            if let ListAnchor::Item(key) = &anchor
                && !s.item_opened
                && let Some(&i) = indices.get(key)
            {
                let start = match offsets.as_deref() {
                    Some(o) => o[i],
                    None => i as f64 * pitch.size(),
                };
                delta = start - pos;
            }
            if delta.abs() > 0.5 {
                move_host(handle, axis, rtl, delta);
                pos += delta;
            } else {
                delta = 0.;
            }
        }
        if measured && matches!(anchor, ListAnchor::Item(_)) {
            state.update(cx, |s, _| s.item_opened = true);
        }

        // The window.
        let host_for_plan = host_now.map(|(s, m)| (s + delta, m));
        let planned = if can_window {
            plan_range(
                count,
                pitch,
                offsets.as_deref().map(|v| v.as_slice()),
                pos,
                viewport,
                measured,
                overscan,
                host_for_plan,
                focus_index,
            )
        } else {
            None
        };
        let (segments, start, end) = planned.unwrap_or_else(|| {
            let all = if count == 0 {
                Vec::new()
            } else {
                vec![ListWindowSegment::Rows {
                    start: 0,
                    end: count,
                }]
            };
            (all, 0, count)
        });
        state.update(cx, |s, _| {
            s.planned = Some(Planned {
                keys: keys.clone(),
                indices: indices.clone(),
                offsets: offsets.clone(),
                pitch,
            });
            s.focused = focused.clone();
            if delta != 0. {
                s.probe.pos += delta;
                if let Some(h) = &mut s.probe.host_scroll {
                    *h += delta;
                }
            }
        });

        // The rows, each in a focus wrapper so the island can find focus.
        let mut child_keys: Vec<Option<ListWindowKey>> = Vec::new();
        let mut children: Vec<AnyElement> = Vec::new();
        let mut handles: HashMap<ListWindowKey, FocusHandle> = HashMap::new();
        let gap = |size: f64| {
            let d = div().flex_none();
            match axis {
                WindowedListAxis::Block => d.w_full().h(px(size as f32)),
                WindowedListAxis::Inline => d.h_full().w(px(size as f32)),
            }
            .into_any_element()
        };
        for segment in &segments {
            match segment {
                ListWindowSegment::Gap { size, .. } => {
                    child_keys.push(None);
                    children.push(gap(*size));
                }
                ListWindowSegment::Rows { start, end } => {
                    for i in *start..*end {
                        let key = keys[i].clone();
                        let handle = state
                            .read(cx)
                            .rows
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| cx.focus_handle());
                        handles.insert(key.clone(), handle.clone());
                        child_keys.push(Some(key));
                        children.push(
                            div()
                                .flex_none()
                                .track_focus(&handle)
                                .child(render_item(i, window, cx))
                                .into_any_element(),
                        );
                    }
                }
            }
        }
        state.update(cx, |s, _| s.rows = handles);

        // The probe: reads where the list sits among the clips above it, and
        // asks for a frame when the rows to mount change.
        let planner: Planner = {
            let offsets = offsets.clone();
            Rc::new(move |p: &Probe| {
                let measured = p.measured && is_positive_finite(p.viewport);
                plan_range(
                    count,
                    pitch,
                    offsets.as_deref().map(|v| v.as_slice()),
                    p.pos,
                    p.viewport,
                    measured,
                    overscan,
                    p.host_scroll.map(|s| (s, p.host_max)),
                    None,
                )
                .map_or((0, count), |(_, s, e)| (s, e))
            })
        };
        let probe_state = state.clone();
        let probe_host = host.clone();
        let (inset_s, rendered) = (f32::from(inset_start) as f64, (start, end));
        let probe = canvas(
            move |bounds, window, cx| {
                let clip = window.content_mask().bounds;
                let (list_start, clip_start, clip_size) = match axis {
                    WindowedListAxis::Block => (
                        f32::from(bounds.top()) as f64,
                        f32::from(clip.top()) as f64,
                        f32::from(clip.size.height) as f64,
                    ),
                    WindowedListAxis::Inline if rtl => (
                        -(f32::from(bounds.right()) as f64),
                        -(f32::from(clip.right()) as f64),
                        f32::from(clip.size.width) as f64,
                    ),
                    WindowedListAxis::Inline => (
                        f32::from(bounds.left()) as f64,
                        f32::from(clip.left()) as f64,
                        f32::from(clip.size.width) as f64,
                    ),
                };
                let host_reading = probe_host.as_ref().map(|h| host_scroll(h, axis, rtl));
                let next = Probe {
                    measured: clip_size > 0.,
                    pos: clip_start - list_start + inset_s,
                    viewport: (clip_size - insets).max(0.),
                    host_scroll: host_reading.map(|(s, _)| s),
                    host_max: host_reading.map_or(0., |(_, m)| m),
                };
                let before = probe_state.read(cx).probe;
                if before != next {
                    probe_state.update(cx, |s, _| s.probe = next);
                    if can_window
                        && (planner(&next) != rendered || before.measured != next.measured)
                    {
                        window.defer(cx, |window, _| window.refresh());
                    }
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        // Rows of varying size report their sizes as they lay out.
        let measure_state = state.clone();
        let measure = matches!(pitch, ListPitch::Estimated(_));
        let mut list = div().track_focus(&container).relative().flex().flex_none();
        list = match axis {
            WindowedListAxis::Block => list.flex_col(),
            WindowedListAxis::Inline if rtl => list.flex_row_reverse(),
            WindowedListAxis::Inline => list.flex_row(),
        };
        list.style().refine(&style);
        list.children(children)
            .child(probe)
            .on_children_prepainted(move |bounds, window, cx| {
                if !measure {
                    return;
                }
                let mut changed = false;
                measure_state.update(cx, |s, _| {
                    for (key, b) in child_keys.iter().zip(bounds.iter()) {
                        if let Some(key) = key {
                            let size = along(b, axis);
                            if s.sizes.get(key).is_none_or(|old| (old - size).abs() > 0.5) {
                                s.sizes.insert(key.clone(), size);
                                changed = true;
                            }
                        }
                    }
                });
                if changed {
                    window.defer(cx, |window, _| window.refresh());
                }
            })
            .id(id)
    }
}
