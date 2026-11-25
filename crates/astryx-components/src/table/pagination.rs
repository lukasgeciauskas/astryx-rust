//! Pagination (upstream `useTablePagination` and `paginateData`): the
//! Pagination controls above, below or around the table. The caller owns
//! the page and the rows of it.

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::ElementSize;
use astryx_theme::current;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, Window,
    div,
};

use super::plugin::TablePlugin;
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::pagination::{Pagination, PaginationVariant};

type PageFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// Where the controls go (upstream `position`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TablePaginationPosition {
    #[default]
    Below,
    Above,
    Both,
    /// Not rendered: the caller places a Pagination itself.
    None,
}

/// How the controls align (upstream `align`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TablePaginationAlign {
    Start,
    #[default]
    Center,
    End,
}

/// Upstream `paginateData`: the rows of `page` (from 1) at `page_size`
/// rows a page (at least 1).
pub fn paginate_data<T: Clone>(data: &[T], page: usize, page_size: usize) -> Vec<T> {
    let size = page_size.max(1);
    let start = (page.max(1) - 1).saturating_mul(size);
    data.iter().skip(start).take(size).cloned().collect()
}

/// The plugin (upstream `useTablePagination`).
#[derive(Clone)]
pub struct TablePagination {
    page: usize,
    on_page_change: PageFn,
    total_items: Option<usize>,
    total_pages: Option<usize>,
    has_more: Option<bool>,
    page_size: usize,
    on_page_size_change: Option<PageFn>,
    page_size_options: Vec<usize>,
    variant: PaginationVariant,
    size: ElementSize,
    position: TablePaginationPosition,
    align: TablePaginationAlign,
    label: Option<SharedString>,
}

impl TablePagination {
    /// Upstream `page` (from 1) and `onPageChange`.
    pub fn new(
        page: usize,
        on_page_change: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            page: page.max(1),
            on_page_change: Rc::new(on_page_change),
            total_items: None,
            total_pages: None,
            has_more: None,
            page_size: 10,
            on_page_size_change: None,
            page_size_options: Vec::new(),
            variant: PaginationVariant::Pages,
            size: ElementSize::Md,
            position: TablePaginationPosition::Below,
            align: TablePaginationAlign::Center,
            label: None,
        }
    }

    /// Upstream `totalItems`: the page count follows from it.
    pub fn total_items(mut self, items: usize) -> Self {
        self.total_items = Some(items);
        self
    }

    /// Upstream `totalPages`, when the item count is unknown.
    pub fn total_pages(mut self, pages: usize) -> Self {
        self.total_pages = Some(pages);
        self
    }

    /// Upstream `hasMore`: cursor pagination without a total.
    pub fn has_more(mut self, more: bool) -> Self {
        self.has_more = Some(more);
        self
    }

    /// Upstream `pageSize` (10 by default, at least 1).
    pub fn page_size(mut self, size: usize) -> Self {
        self.page_size = size.max(1);
        self
    }

    /// Upstream `onPageSizeChange`: with `page_size_options`, a page-size
    /// selector shows.
    pub fn on_page_size_change(
        mut self,
        f: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_page_size_change = Some(Rc::new(f));
        self
    }

    /// Upstream `pageSizeOptions`.
    pub fn page_size_options(mut self, options: impl IntoIterator<Item = usize>) -> Self {
        self.page_size_options = options.into_iter().collect();
        self
    }

    astryx_primitives::setters! {
        /// Upstream `variant` (`pages` by default).
        variant: PaginationVariant,
        /// Upstream `size`: `Sm` or `Md` (the default).
        size: ElementSize,
        /// Upstream `position` (below by default).
        position: TablePaginationPosition,
        /// Upstream `align` (centered by default).
        align: TablePaginationAlign,
    }

    /// Upstream `label`: the landmark's name ("Table pagination"); with
    /// both positions, each side's name says which it is.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The page count it knows.
    fn page_count(&self) -> Option<usize> {
        self.total_pages
            .or(self.total_items.map(|items| items.div_ceil(self.page_size)))
    }

    fn controls(
        &self,
        above: bool,
        label: SharedString,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = current(window, cx);
        let gap = theme.tokens().spacing.s2.to_pixels(window.rem_size());
        let on_change = self.on_page_change.clone();
        let mut pagination = Pagination::new(
            if above {
                "table-pagination-above"
            } else {
                "table-pagination-below"
            },
            self.page,
        )
        .page_size(self.page_size)
        .variant(self.variant)
        .size(self.size)
        .label(label)
        .on_change(move |page, window, cx| on_change(page, window, cx));
        if let Some(n) = self.total_items {
            pagination = pagination.total_items(n);
        }
        if let Some(n) = self.page_count() {
            pagination = pagination.total_pages(n);
        }
        if let Some(more) = self.has_more {
            pagination = pagination.has_more(more);
        }
        if !self.page_size_options.is_empty() {
            pagination = pagination.page_size_options(self.page_size_options.clone());
        }
        if let Some(f) = self.on_page_size_change.clone() {
            pagination =
                pagination.on_page_size_change(move |size, window, cx| f(size, window, cx));
        }
        let selector = if above {
            "table-pagination-above"
        } else {
            "table-pagination-below"
        };
        let row = div()
            .debug_selector(move || selector.into())
            .flex()
            .w_full();
        let row = if above { row.mb(gap) } else { row.mt(gap) };
        match self.align {
            TablePaginationAlign::Start => row.justify_start(),
            TablePaginationAlign::Center => row.justify_center(),
            TablePaginationAlign::End => row.justify_end(),
        }
        .child(pagination)
        .into_any_element()
    }
}

/// Upstream `useTablePagination`.
pub fn use_table_pagination(
    page: usize,
    on_page_change: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> TablePagination {
    TablePagination::new(page, on_page_change)
}

impl<T> TablePlugin<T> for TablePagination {
    fn transform_table_context(
        &self,
        child: AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        if self.position == TablePaginationPosition::None {
            return child;
        }
        // One page and no more to come: no controls.
        if self.page_count() == Some(1) && self.has_more != Some(true) {
            return child;
        }
        let t = i18n(cx);
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| t.t("@astryx.table.pagination.label"));
        let both = self.position == TablePaginationPosition::Both;
        // Two landmarks need two names.
        let side_label = |key: &str| {
            if both {
                t.format(key, &[("label", label.as_ref().into())])
            } else {
                label.clone()
            }
        };
        let (above_label, below_label) = (
            side_label("@astryx.table.pagination.labelAbove"),
            side_label("@astryx.table.pagination.labelBelow"),
        );
        let mut out = div().flex().flex_col().w_full().min_w_0();
        if matches!(
            self.position,
            TablePaginationPosition::Above | TablePaginationPosition::Both
        ) {
            out = out.child(self.controls(true, above_label, window, cx));
        }
        out = out.child(child);
        if matches!(
            self.position,
            TablePaginationPosition::Below | TablePaginationPosition::Both
        ) {
            out = out.child(self.controls(false, below_label, window, cx));
        }
        out.into_any_element()
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_PAGINATION_DOC: ComponentDoc = ComponentDoc {
    name: "useTablePagination",
    rust_type: "astryx::components::table::TablePagination",
    props: &[
        p("page", RustApi::Argument("page")),
        p("onPageChange", RustApi::Argument("on_page_change")),
        p("totalItems", RustApi::Method("total_items")),
        p("totalPages", RustApi::Method("total_pages")),
        p("hasMore", RustApi::Method("has_more")),
        p("pageSize", RustApi::Method("page_size")),
        p("onPageSizeChange", RustApi::Method("on_page_size_change")),
        p("pageSizeOptions", RustApi::Method("page_size_options")),
        p("variant", RustApi::Method("variant")),
        p("position", RustApi::Method("position")),
        p("align", RustApi::Method("align")),
        p("label", RustApi::Method("label")),
    ],
    example: include_str!("examples.rs"),
};
