//! Infinite scroll (upstream `useTableInfiniteScroll`): a sentinel and a
//! Load more button after the table, inside its scroll region. The sentinel
//! coming into view asks for the next batch; the button always can. The
//! caller owns the rows, the request and its errors.
//!
//! GPUI has no IntersectionObserver: the sentinel checks, as it paints,
//! whether it shows inside every clip above it (the table's region, the
//! page's scroller, the window), which is what the observer reports.

use std::rc::Rc;

use gpui::{
    App, ElementId, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement, Styled,
    Task, Window, canvas, div, px,
};
use rustryx_i18n::current as i18n;
use rustryx_theme::current;

use super::plugin::{
    RowModel, ScrollWrapperProps, TableMode, TablePlugin, TableRenderContext, dev_warn_once,
};
use crate::button::Button;
use crate::doc::{ComponentDoc, PropMapping, RustApi};

/// One batch's work: true when it succeeded.
type LoadFn = Rc<dyn Fn(&mut Window, &mut App) -> Task<bool>>;

/// A request in flight, or settled and waiting for the caller's loading
/// flag to read false on a later render (FR4).
#[derive(Clone, Copy)]
struct Request {
    /// The render count when it settled.
    settled_at: Option<u64>,
    successful: bool,
}

/// The hook's lifetime state (upstream `createRuntime`).
struct Runtime {
    has_more: bool,
    is_loading: bool,
    on_load_more: LoadFn,
    request: Option<Request>,
    /// A failed batch pauses automatic loading until one succeeds.
    auto_paused: bool,
    commit_count: u64,
    /// Whether the table admits loading (data mode, flat rows).
    admitted: bool,
    /// The surface showed last frame.
    surface_shown: bool,
    surface_focus: FocusHandle,
}

impl Runtime {
    fn effective_loading(&self) -> bool {
        self.has_more && (self.is_loading || self.request.is_some())
    }
}

/// The plugin (upstream `useTableInfiniteScroll`).
#[derive(Clone)]
pub struct TableInfiniteScroll {
    runtime: Entity<Runtime>,
    id: ElementId,
}

/// Upstream `useTableInfiniteScroll`: `has_more`, the caller's
/// `is_loading`, and `on_load_more`, whose task finishes when the batch's
/// rows are in (an error pauses automatic loading; the button still works).
/// The state lives under `id`: a reset renders a new id (or remounts its
/// owner), never just the table.
pub fn use_table_infinite_scroll<E: 'static>(
    id: impl Into<ElementId>,
    has_more: bool,
    is_loading: bool,
    on_load_more: impl Fn(&mut Window, &mut App) -> Task<Result<(), E>> + 'static,
    window: &mut Window,
    cx: &mut App,
) -> TableInfiniteScroll {
    let id = id.into();
    let on_load: LoadFn = Rc::new(move |window, cx| {
        let task = on_load_more(window, cx);
        cx.spawn(async move |_| task.await.is_ok())
    });
    let load = on_load.clone();
    let runtime = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-infinite-scroll").into()),
        cx,
        move |_, cx| Runtime {
            has_more,
            is_loading,
            on_load_more: load,
            request: None,
            auto_paused: false,
            commit_count: 0,
            admitted: false,
            surface_shown: false,
            surface_focus: cx.focus_handle().tab_stop(false),
        },
    );
    // Commit the current configuration (upstream's layout-effect commit): a
    // settled request completes once a later render reads loading false.
    runtime.update(cx, |r, _| {
        r.has_more = has_more;
        r.is_loading = is_loading;
        r.on_load_more = on_load;
        r.commit_count += 1;
        if let Some(request) = r.request
            && request.settled_at.is_some_and(|at| r.commit_count > at)
            && !is_loading
        {
            r.request = None;
            if request.successful {
                r.auto_paused = false;
            }
        }
    });
    TableInfiniteScroll { runtime, id }
}

/// Upstream `invoke`: one request at a time, only while more can come and
/// nothing loads; automatic requests also wait out a failure.
fn invoke(runtime: &Entity<Runtime>, automatic: bool, window: &mut Window, cx: &mut App) {
    let load = runtime.update(cx, |r, _| {
        if !r.admitted
            || !r.has_more
            || r.is_loading
            || r.request.is_some()
            || (automatic && r.auto_paused)
        {
            return None;
        }
        // The latch is set before the callback runs.
        r.request = Some(Request {
            settled_at: None,
            successful: false,
        });
        Some(r.on_load_more.clone())
    });
    let Some(load) = load else {
        return;
    };
    window.refresh();
    let task = load(window, cx);
    let weak = runtime.downgrade();
    window
        .spawn(cx, async move |cx| {
            let ok = task.await;
            cx.update(|window, cx| {
                // A reset dropped the runtime: its settlement goes nowhere.
                let _ = weak.update(cx, |r, _| {
                    let count = r.commit_count;
                    if let Some(request) = r.request.as_mut()
                        && request.settled_at.is_none()
                    {
                        request.successful = ok;
                        request.settled_at = Some(count);
                        if !ok {
                            r.auto_paused = true;
                        }
                    }
                });
                window.refresh();
            })
            .ok();
        })
        .detach();
}

fn admitted(context: &TableRenderContext) -> bool {
    context.mode == TableMode::Data && context.row_model == RowModel::Flat
}

impl<T> TablePlugin<T> for TableInfiniteScroll {
    fn row_model(&self) -> Option<RowModel> {
        Some(RowModel::Flat)
    }

    fn is_loading(&self, cx: &App) -> Option<bool> {
        Some(self.runtime.read(cx).effective_loading())
    }

    fn has_more(&self, cx: &App) -> Option<bool> {
        Some(self.runtime.read(cx).has_more)
    }

    fn transform_scroll_wrapper(
        &self,
        mut props: ScrollWrapperProps,
        context: &TableRenderContext,
        window: &mut Window,
        cx: &mut App,
    ) -> ScrollWrapperProps {
        let ok = admitted(context);
        if !ok {
            dev_warn_once("useTableInfiniteScroll:admission", || {
                if context.mode != TableMode::Data {
                    "useTableInfiniteScroll: loading requires a data-driven Table; children mode \
                     remains unchanged."
                        .into()
                } else {
                    "useTableInfiniteScroll: loading requires admitted flat product rows; body \
                     modifiers must declare a flat row model."
                        .into()
                }
            });
        }
        let (shows, was_shown, focus, loading) = self.runtime.update(cx, |r, _| {
            r.admitted = ok;
            let shows = ok && r.has_more;
            let was = std::mem::replace(&mut r.surface_shown, shows);
            (shows, was, r.surface_focus.clone(), r.effective_loading())
        });
        if !shows {
            // The last batch took away a focused button: focus goes to the
            // table's region (FR6), and other focus stays where it is.
            if was_shown
                && focus.contains_focused(window, cx)
                && let Some(viewport) = context.viewport_focus.clone()
            {
                window.defer(cx, move |window, cx| window.focus(&viewport, cx));
            }
            return props;
        }
        let theme = current(window, cx);
        let gap = theme.tokens().spacing.s2.to_pixels(window.rem_size());
        // Kept in view while the table scrolls sideways (upstream's sticky
        // command).
        let offset = -context.viewport.offset().x;
        let width = context.viewport.bounds().size.width;
        let probe_runtime = self.runtime.clone();
        let press_runtime = self.runtime.clone();
        let sentinel = div()
            .debug_selector(|| "table-load-more-sentinel".into())
            .h(px(1.))
            .w_full()
            .flex_shrink_0()
            .child(
                canvas(
                    move |bounds, window, cx| {
                        // Inside every clip above it: in view.
                        if window.content_mask().bounds.intersects(&bounds) {
                            let runtime = probe_runtime.clone();
                            window.defer(cx, move |window, cx| invoke(&runtime, true, window, cx));
                        }
                    },
                    |_, _, _, _| {},
                )
                .size_full(),
            );
        let mut surface = div()
            .id(ElementId::Name(format!("{}-load-more", self.id).into()))
            .track_focus(&focus)
            .relative()
            .left(offset)
            .flex()
            .flex_col()
            .items_center()
            .py(gap)
            .gap(gap)
            .child(sentinel)
            .child(
                div()
                    .debug_selector({
                        let s = format!("{}-load-more-button", self.id);
                        move || s
                    })
                    .child(
                        Button::new(
                            ElementId::Name(format!("{}-load-more-button", self.id).into()),
                            i18n(cx).t("@astryx.table.loadMore"),
                        )
                        .loading(loading)
                        .on_click(move |_, window, cx| invoke(&press_runtime, false, window, cx)),
                    ),
            );
        if width > px(0.) {
            surface = surface.w(width);
        }
        props.after_table.push(surface.into_any_element());
        props
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_INFINITE_SCROLL_DOC: ComponentDoc = ComponentDoc {
    name: "useTableInfiniteScroll",
    rust_type: "rustryx::components::table::use_table_infinite_scroll",
    props: &[
        p("hasMore", RustApi::Argument("has_more")),
        p("isLoading", RustApi::Argument("is_loading")),
        p(
            "onLoadMore",
            RustApi::Argument("on_load_more (a Task of Result)"),
        ),
    ],
    example: include_str!("examples.rs"),
};
