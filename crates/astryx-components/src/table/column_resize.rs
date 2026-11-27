//! Column resize (upstream `useTableColumnResize`): a boundary control at
//! each resizable column's trailing edge, dragged or moved with the arrow
//! keys. Widths are the caller's (`column_widths`, reported on
//! `on_column_resize_end`); while a drag runs the plugin holds them.
//!
//! The boundary reaches from the header through the body rows (FR2): GPUI
//! draws it deferred, over the rows, inside the table's scroll region.
//! Header cells and the table are measured as they paint (upstream's resize
//! observer).

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use web_time::Instant;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{control_focus, is_focus_visible};
use astryx_theme::current;
use gpui::{
    App, Bounds, CursorStyle, ElementId, Entity, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, canvas, deferred, div, px,
};

use super::model::{
    ColumnWidth, DEFAULT_FLEXIBLE_COLUMN_MIN_WIDTH, DEFAULT_MIN_COLUMN_WIDTH, TableColumn, pixel,
};
use super::plugin::{
    HeaderCellProps, HeaderRowProps, ScrollWrapperProps, TablePlugin, TableRenderContext,
    TableRenderProps,
};
use crate::doc::{ComponentDoc, PropMapping, RustApi};

const KEYBOARD_STEP: f32 = 10.;
const KEYBOARD_LARGE_STEP: f32 = 50.;
/// How long after the last scroll the disclosure stays held (FR10).
const SCROLL_SETTLE: Duration = Duration::from_millis(120);

type EndFn = Rc<dyn Fn(HashMap<SharedString, f32>, &mut Window, &mut App)>;

/// Upstream `resolveColumnMinWidth`: the global minimum, else the column's
/// own (a proportional column's `min_width`, a pixel column's width, the
/// flexible floor without a width).
pub fn resolve_column_min_width(width: Option<&ColumnWidth>, global: Option<f32>) -> f32 {
    if let Some(min) = global {
        return min;
    }
    match width {
        None => DEFAULT_FLEXIBLE_COLUMN_MIN_WIDTH,
        Some(ColumnWidth::Proportional { min_width, .. }) => {
            min_width.unwrap_or(DEFAULT_MIN_COLUMN_WIDTH)
        }
        Some(ColumnWidth::Pixel(value)) => *value,
    }
}

fn is_proportional(width: Option<&ColumnWidth>) -> bool {
    !matches!(width, Some(ColumnWidth::Pixel(_)))
}

/// One column at a resize's start.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Snapshot {
    pub key: SharedString,
    pub initial: f32,
    pub min: f32,
    pub max: f32,
}

/// A resize in progress, from its snapshot.
#[derive(Clone, Debug)]
pub(crate) struct Resize {
    pub snapshots: Vec<Snapshot>,
    pub resize: usize,
    pub neighbor: Option<usize>,
    pub table_width: f32,
}

impl Resize {
    /// Upstream `computeColumnWidths`: every column's width for a drag of
    /// `delta`. A proportional column trades width with its neighbor; any
    /// other grows or shrinks within its limits. The last column takes the
    /// remainder of the table's width, at least its minimum.
    pub fn widths(&self, delta: f32) -> Vec<f32> {
        let mut widths: Vec<f32> = self.snapshots.iter().map(|s| s.initial).collect();
        let me = &self.snapshots[self.resize];
        match self.neighbor {
            Some(n) => {
                let neighbor = &self.snapshots[n];
                let max_delta = neighbor.initial - neighbor.min;
                let min_delta = me.min - me.initial;
                let delta = delta.min(max_delta).max(min_delta);
                widths[n] = neighbor.initial - delta;
                widths[self.resize] = me.initial + delta;
            }
            None => {
                widths[self.resize] = (me.initial + delta).max(me.min).min(me.max);
            }
        }
        if let Some(last) = widths.len().checked_sub(1)
            && self.table_width > 0.
        {
            let others: f32 = widths.iter().take(last).sum();
            widths[last] = self.snapshots[last].min.max(self.table_width - others);
        }
        widths
    }

    /// The widths a resize reports: every column but the last.
    fn updates(&self, widths: &[f32]) -> HashMap<SharedString, f32> {
        self.snapshots
            .iter()
            .zip(widths)
            .take(self.snapshots.len().saturating_sub(1))
            .map(|(s, w)| (s.key.clone(), *w))
            .collect()
    }
}

/// A drag: the resize, where it began and the widths it reached.
struct Drag {
    key: SharedString,
    resize: Resize,
    start_x: Pixels,
    rtl: bool,
    widths: Vec<f32>,
}

/// The plugin's state between frames.
#[derive(Default)]
struct Runtime {
    /// This frame's columns, in order, with whether each resizes.
    columns: Vec<(SharedString, bool, Option<ColumnWidth>)>,
    /// The header cells as they last painted.
    cells: HashMap<SharedString, Bounds<Pixels>>,
    table: Option<Bounds<Pixels>>,
    drag: Option<Drag>,
    header_hover: bool,
    hover_handle: Option<SharedString>,
    /// Disclosure waits until then: the region scrolled (FR10).
    quiet_until: Option<Instant>,
}

/// What [`use_table_column_resize`] reads (upstream
/// `UseTableColumnResizeConfig`).
pub struct ColumnResizeConfig<T> {
    column_widths: HashMap<SharedString, f32>,
    on_end: Option<EndFn>,
    min_width: Option<f32>,
    max_width: f32,
    columns: Option<Vec<TableColumn<T>>>,
}

impl<T> Default for ColumnResizeConfig<T> {
    fn default() -> Self {
        Self {
            column_widths: HashMap::new(),
            on_end: None,
            min_width: None,
            max_width: f32::INFINITY,
            columns: None,
        }
    }
}

impl<T> ColumnResizeConfig<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `columnWidths`: the caller's widths, in pixels, by key.
    pub fn column_widths(mut self, widths: impl IntoIterator<Item = (SharedString, f32)>) -> Self {
        self.column_widths = widths.into_iter().collect();
        self
    }

    /// Upstream `onColumnResizeEnd`: every changed width, to merge in.
    pub fn on_column_resize_end(
        mut self,
        f: impl Fn(HashMap<SharedString, f32>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_end = Some(Rc::new(f));
        self
    }

    /// Upstream `minWidth`: one minimum for every column.
    pub fn min_width(mut self, min: f32) -> Self {
        self.min_width = Some(min);
        self
    }

    /// Upstream `maxWidth` (unbounded by default).
    pub fn max_width(mut self, max: f32) -> Self {
        self.max_width = max;
        self
    }

    /// Upstream `columns`: the column definitions, for each column's own
    /// minimum and for a proportional column to trade width with its
    /// neighbor.
    pub fn columns(mut self, columns: Vec<TableColumn<T>>) -> Self {
        self.columns = Some(columns);
        self
    }
}

struct Shared<T> {
    config: ColumnResizeConfig<T>,
    runtime: Entity<Runtime>,
    id: ElementId,
}

/// The plugin (upstream `useTableColumnResize`).
pub struct TableColumnResize<T> {
    shared: Rc<Shared<T>>,
}

impl<T> Clone for TableColumnResize<T> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

/// Upstream `useTableColumnResize`; its drag state lives under `id`.
pub fn use_table_column_resize<T: 'static>(
    id: impl Into<ElementId>,
    config: ColumnResizeConfig<T>,
    window: &mut Window,
    cx: &mut App,
) -> TableColumnResize<T> {
    let id = id.into();
    let runtime = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-column-resize").into()),
        cx,
        |_, _| Runtime::default(),
    );
    TableColumnResize {
        shared: Rc::new(Shared {
            config,
            runtime,
            id,
        }),
    }
}

impl<T: 'static> Shared<T> {
    /// The definitions' resizable columns (upstream `resizableColumns`).
    fn resizable(&self) -> Option<Vec<&TableColumn<T>>> {
        self.config
            .columns
            .as_ref()
            .map(|c| c.iter().filter(|c| c.resizable).collect())
    }

    fn definition(&self, key: &str) -> Option<&TableColumn<T>> {
        self.config
            .columns
            .as_ref()?
            .iter()
            .find(|c| c.key.as_ref() == key)
    }

    /// The column a proportional column trades width with: the next
    /// resizable one.
    fn neighbor(&self, key: &str, width: Option<&ColumnWidth>) -> Option<SharedString> {
        let resizable = self.resizable()?;
        if !is_proportional(width) {
            return None;
        }
        let i = resizable.iter().position(|c| c.key.as_ref() == key)?;
        resizable.get(i + 1).map(|c| c.key.clone())
    }

    /// A resize of `key` from the columns as they last painted.
    fn snapshot(&self, key: &SharedString, cx: &App) -> Option<Resize> {
        let r = self.runtime.read(cx);
        let mut snapshots = Vec::new();
        for (col_key, resizable, width) in &r.columns {
            if !resizable {
                continue;
            }
            let rendered = r.cells.get(col_key).map_or(0., |b| f32::from(b.size.width));
            let width_config = self.definition(col_key).map_or(*width, |c| c.width);
            snapshots.push(Snapshot {
                key: col_key.clone(),
                initial: self
                    .config
                    .column_widths
                    .get(col_key)
                    .copied()
                    .unwrap_or(rendered),
                min: resolve_column_min_width(width_config.as_ref(), self.config.min_width),
                max: self.config.max_width,
            });
        }
        let resize = snapshots.iter().position(|s| &s.key == key)?;
        let own_width = self.definition(key).and_then(|c| c.width);
        let neighbor = self
            .neighbor(key, own_width.as_ref())
            .and_then(|n| snapshots.iter().position(|s| s.key == n));
        let table_width = r.table.map_or(0., |b| f32::from(b.size.width));
        Some(Resize {
            snapshots,
            resize,
            neighbor,
            table_width,
        })
    }

    fn end(&self, updates: HashMap<SharedString, f32>, window: &mut Window, cx: &mut App) {
        if !updates.is_empty()
            && let Some(f) = &self.config.on_end
        {
            f(updates, window, cx);
        }
    }

    /// Keyboard resize: the same computation as a drag, reported at once.
    fn nudge(&self, key: &SharedString, delta: f32, window: &mut Window, cx: &mut App) {
        if let Some(resize) = self.snapshot(key, cx) {
            let widths = resize.widths(delta);
            self.end(resize.updates(&widths), window, cx);
            window.refresh();
        }
    }
}

/// The boundary control of one column.
#[derive(IntoElement)]
struct Handle<T: 'static> {
    shared: Rc<Shared<T>>,
    key: SharedString,
    label: SharedString,
    min: f32,
}

impl<T: 'static> RenderOnce for Handle<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rtl = use_direction(cx).is_rtl();
        let id = ElementId::Name(format!("{}-resize-{}", self.shared.id, self.key).into());
        let focus = control_focus(&id, window, cx);
        let runtime = self.shared.runtime.clone();
        let now = cx.background_executor().now();
        let (rendered, height, dragging, hinted, hovered) = {
            let r = runtime.read(cx);
            let cell = r.cells.get(&self.key).copied();
            let height = match (cell, r.table) {
                (Some(cell), Some(table)) => Some(table.bottom() - cell.top()),
                _ => None,
            };
            let quiet = r.quiet_until.is_some_and(|until| now < until);
            // Upstream DEC-4: under a coarse primary pointer the hairline takes no pointer.
            let touch = astryx_primitives::modality::coarse_primary_pointer_now(window, cx);
            let pointer_ok = !quiet && !touch;
            (
                cell.map(|b| f32::from(b.size.width).round()),
                height,
                r.drag.as_ref().is_some_and(|d| d.key == self.key),
                r.header_hover && pointer_ok,
                r.hover_handle.as_ref() == Some(&self.key) && pointer_ok,
            )
        };
        let focused = is_focus_visible(&focus, window);
        // Emphasized (accent) over hinted (border weight) over hidden.
        let (color, width) = if dragging {
            (Some(t.color.accent), px(2.))
        } else if focused || hovered {
            (Some(t.color.accent), px(1.))
        } else if hinted {
            (Some(t.color.border), px(1.))
        } else {
            (None, px(1.))
        };
        let max = self.shared.config.max_width;
        let value = rendered
            .or_else(|| self.shared.config.column_widths.get(&self.key).copied())
            .unwrap_or(self.min);
        let unbounded = !max.is_finite();
        let announced_max = if unbounded {
            f32::from(window.viewport_size().width)
                .max(value)
                .max(self.min)
        } else {
            max.max(value)
        };
        let tr = i18n(cx);
        let mut handle = div()
            .id(id.clone())
            .debug_selector({
                let s = format!("{id}");
                move || s
            })
            .track_focus(&focus)
            .absolute()
            .top_0()
            .right_0()
            .w(px(8.))
            .cursor(CursorStyle::ResizeLeftRight)
            .role(Role::Splitter)
            .aria_orientation(gpui::Orientation::Vertical)
            .aria_label(tr.format(
                "@astryx.table.columnResize.resizeColumn",
                &[("label", self.label.as_ref().into())],
            ))
            .aria_numeric_value(value as f64)
            .aria_min_numeric_value(self.min.min(value) as f64)
            .aria_max_numeric_value(announced_max as f64);
        handle = match height {
            Some(h) => handle.h(h),
            None => handle.h_full(),
        };
        if unbounded {
            handle = handle.aria_value(tr.format(
                "@astryx.table.columnResize.widthValue",
                &[("width", (value.round() as i64).into())],
            ));
        }
        let (hover_rt, down_rt) = (runtime.clone(), runtime.clone());
        let (hover_key, down_key) = (self.key.clone(), self.key.clone());
        let down_shared = self.shared.clone();
        let key_shared = self.shared.clone();
        let key_key = self.key.clone();
        let (min, current_override) = (
            self.min,
            self.shared.config.column_widths.get(&self.key).copied(),
        );
        handle = handle
            .on_hover(move |over, window, cx| {
                hover_rt.update(cx, |r, _| {
                    r.hover_handle = over
                        .then(|| hover_key.clone())
                        .or_else(|| r.hover_handle.clone().filter(|k| *k != hover_key));
                });
                window.refresh();
            })
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                // Dragging a hairline by finger is not offered (DEC-4): under a coarse
                // primary pointer the handle takes no pointer at all.
                if window.last_input_was_touch()
                    || astryx_primitives::coarse_primary_pointer(window, cx)
                {
                    return;
                }
                window.prevent_default();
                cx.stop_propagation();
                let Some(resize) = down_shared.snapshot(&down_key, cx) else {
                    return;
                };
                let widths = resize.widths(0.);
                down_rt.update(cx, |r, _| {
                    r.drag = Some(Drag {
                        key: down_key.clone(),
                        resize,
                        start_x: event.position.x,
                        rtl,
                        widths,
                    })
                });
                window.refresh();
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let stroke = &event.keystroke;
                let step = if stroke.modifiers.shift {
                    KEYBOARD_LARGE_STEP
                } else {
                    KEYBOARD_STEP
                };
                let flip = if rtl { -1. } else { 1. };
                let current = current_override.unwrap_or(value);
                let delta = match stroke.key.as_str() {
                    "right" => step * flip,
                    "left" => -step * flip,
                    "home" => min - current,
                    "end" if max.is_finite() => max - current,
                    _ => return,
                };
                cx.stop_propagation();
                key_shared.nudge(&key_key, delta, window, cx);
            });
        if dragging {
            // The drag follows the pointer anywhere in the window.
            let (move_rt, up_rt, up_shared) =
                (runtime.clone(), runtime.clone(), self.shared.clone());
            handle = handle.child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let move_rt = move_rt.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, _, window, cx| {
                            move_rt.update(cx, |r, _| {
                                if let Some(drag) = r.drag.as_mut() {
                                    let mut delta = f32::from(event.position.x - drag.start_x);
                                    if drag.rtl {
                                        delta = -delta;
                                    }
                                    drag.widths = drag.resize.widths(delta);
                                }
                            });
                            window.refresh();
                        });
                        let (up_rt, up_shared) = (up_rt.clone(), up_shared.clone());
                        window.on_mouse_event(move |event: &MouseUpEvent, _, window, cx| {
                            let Some(drag) = up_rt.update(cx, |r, _| r.drag.take()) else {
                                return;
                            };
                            let mut delta = f32::from(event.position.x - drag.start_x);
                            if drag.rtl {
                                delta = -delta;
                            }
                            let widths = drag.resize.widths(delta);
                            up_shared.end(drag.resize.updates(&widths), window, cx);
                            window.refresh();
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }
        if let Some(color) = color {
            handle = handle.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(width)
                    .bg(color),
            );
        }
        // Over the body rows, within the table's scroll region.
        deferred(handle).with_priority(1)
    }
}

impl<T: 'static> TablePlugin<T> for TableColumnResize<T> {
    fn transform_columns(
        &self,
        columns: Vec<TableColumn<T>>,
        _: &mut Window,
        cx: &mut App,
    ) -> Vec<TableColumn<T>> {
        let s = &self.shared;
        let live: Option<HashMap<SharedString, f32>> = s.runtime.update(cx, |r, _| {
            r.columns = columns
                .iter()
                .map(|c| (c.key.clone(), c.resizable, c.width))
                .collect();
            r.drag.as_ref().map(|d| {
                d.resize
                    .snapshots
                    .iter()
                    .zip(&d.widths)
                    .map(|(s, w)| (s.key.clone(), *w))
                    .collect()
            })
        });
        columns
            .into_iter()
            .map(|col| {
                // A drag holds every column it snapshot; otherwise the
                // caller's widths.
                let width = match &live {
                    Some(live) => live.get(&col.key).copied(),
                    None => s.config.column_widths.get(&col.key).copied(),
                };
                match width {
                    Some(w) => col.width(pixel(w)),
                    None => col,
                }
            })
            .collect()
    }

    fn transform_table(
        &self,
        mut props: TableRenderProps,
        _: &mut Window,
        _: &mut App,
    ) -> TableRenderProps {
        let runtime = self.shared.runtime.clone();
        props.hooks.push(Box::new(move |table| {
            table.relative().child(
                canvas(
                    move |bounds, _, cx| runtime.update(cx, |r, _| r.table = Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
        }));
        props
    }

    fn transform_header_row(
        &self,
        mut props: HeaderRowProps,
        _: &mut Window,
        _: &mut App,
    ) -> HeaderRowProps {
        let runtime = self.shared.runtime.clone();
        props.hooks.push(Box::new(move |row| {
            row.on_hover(move |over, window, cx| {
                runtime.update(cx, |r, _| r.header_hover = *over);
                window.refresh();
            })
        }));
        props
    }

    fn transform_scroll_wrapper(
        &self,
        mut props: ScrollWrapperProps,
        _: &TableRenderContext,
        _: &mut Window,
        _: &mut App,
    ) -> ScrollWrapperProps {
        // While the region scrolls, no new disclosure (FR10).
        let runtime = self.shared.runtime.clone();
        props.hooks.push(Box::new(move |region| {
            region.on_scroll(move |_, window, cx| {
                let until = cx.background_executor().now() + SCROLL_SETTLE;
                runtime.update(cx, |r, _| r.quiet_until = Some(until));
                let timer = cx.background_executor().timer(SCROLL_SETTLE);
                window
                    .spawn(cx, async move |cx| {
                        timer.await;
                        cx.update(|window, _| window.refresh()).ok();
                    })
                    .detach();
            })
        }));
        props
    }

    fn transform_header_cell(
        &self,
        mut props: HeaderCellProps,
        column: &TableColumn<T>,
        _: &[TableColumn<T>],
        _: &mut Window,
        _: &mut App,
    ) -> HeaderCellProps {
        let s = &self.shared;
        // Every header is measured: a drag snapshots them all.
        let (runtime, key) = (s.runtime.clone(), column.key.clone());
        props.overlay.push(
            canvas(
                move |bounds, _, cx| {
                    runtime.update(cx, |r, _| {
                        r.cells.insert(key.clone(), bounds);
                    })
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .into_any_element(),
        );
        if !column.resizable {
            return props;
        }
        // The last resizable column, when proportional, just flexes.
        if let Some(resizable) = s.resizable()
            && resizable.last().is_some_and(|c| c.key == column.key)
            && is_proportional(s.definition(&column.key).and_then(|c| c.width.as_ref()))
        {
            return props;
        }
        let width = s.definition(&column.key).map_or(column.width, |c| c.width);
        let min = resolve_column_min_width(width.as_ref(), s.config.min_width);
        // The handle reaches past the cell, over the body rows.
        props.style.overflow.x = Some(gpui::Overflow::Visible);
        props.style.overflow.y = Some(gpui::Overflow::Visible);
        props.overlay.push(
            Handle {
                shared: s.clone(),
                key: column.key.clone(),
                label: column
                    .header_text()
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| column.key.clone()),
                min,
            }
            .into_any_element(),
        );
        props
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_COLUMN_RESIZE_DOC: ComponentDoc = ComponentDoc {
    name: "useTableColumnResize",
    rust_type: "astryx::components::table::use_table_column_resize",
    props: &[
        p("columnWidths", RustApi::Method("column_widths")),
        p("onColumnResizeEnd", RustApi::Method("on_column_resize_end")),
        p("minWidth", RustApi::Method("min_width")),
        p("maxWidth", RustApi::Method("max_width")),
        p("columns", RustApi::Method("columns")),
    ],
    example: include_str!("examples.rs"),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(key: &'static str, initial: f32, min: f32) -> Snapshot {
        Snapshot {
            key: key.into(),
            initial,
            min,
            max: f32::INFINITY,
        }
    }

    #[test]
    fn a_direct_resize_stays_in_its_limits_and_the_last_column_fills() {
        let resize = Resize {
            snapshots: vec![
                snap("a", 100., 60.),
                snap("b", 100., 60.),
                snap("c", 200., 60.),
            ],
            resize: 0,
            neighbor: None,
            table_width: 400.,
        };
        assert_eq!(resize.widths(30.), [130., 100., 170.]);
        assert_eq!(
            resize.widths(-80.),
            [60., 100., 240.],
            "held at its minimum"
        );
        // The last column never goes below its own minimum: the table grows.
        assert_eq!(resize.widths(200.), [300., 100., 60.]);
        assert_eq!(
            resize.updates(&resize.widths(30.)).len(),
            2,
            "every column but the last"
        );
    }

    #[test]
    fn a_proportional_column_trades_with_its_neighbor() {
        let resize = Resize {
            snapshots: vec![
                snap("a", 200., 120.),
                snap("b", 200., 120.),
                snap("c", 100., 60.),
            ],
            resize: 0,
            neighbor: Some(1),
            table_width: 500.,
        };
        assert_eq!(resize.widths(50.), [250., 150., 100.]);
        // Neither drops below its minimum.
        assert_eq!(resize.widths(150.), [280., 120., 100.]);
        assert_eq!(resize.widths(-150.), [120., 280., 100.]);
    }

    #[test]
    fn minimums_follow_the_column() {
        assert_eq!(
            resolve_column_min_width(None, None),
            DEFAULT_FLEXIBLE_COLUMN_MIN_WIDTH
        );
        assert_eq!(resolve_column_min_width(Some(&pixel(200.)), None), 200.);
        assert_eq!(
            resolve_column_min_width(Some(&super::super::model::proportional(1.)), None),
            DEFAULT_MIN_COLUMN_WIDTH
        );
        assert_eq!(resolve_column_min_width(Some(&pixel(200.)), Some(40.)), 40.);
    }
}
