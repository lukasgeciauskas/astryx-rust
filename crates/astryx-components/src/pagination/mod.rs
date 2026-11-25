//! Pagination. Upstream: `Pagination/`.
//!
//! The page-size selector lands with Selector (M5).

pub mod examples;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{
    ElementSize, ListFocus, ListKeys, Politeness, announce, is_focus_visible, use_link_handler,
};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, IntoElement, RenderOnce, Role, SharedString, StyleRefinement,
    Styled, Task, Toggled, Window, div, prelude::*,
};

use crate::button::{Button, ButtonVariant, IconButton};
use crate::choice::action::{ActionFn, Optimistic};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::field::StatusPlacement;
use crate::focus::focus_ring;
use crate::icon::{Icon, IconSize};
use crate::input::NumberInput;
use crate::text::{Text, TextColor, TextSize, TextType};

/// Upstream `variant`: what sits between previous and next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaginationVariant {
    /// Page buttons with ellipses.
    #[default]
    Pages,
    /// "1–10 of 42".
    Count,
    /// "Page 2 of 5".
    Compact,
    /// One dot per page.
    Dots,
    /// "Page [n] / N" with first and last.
    Input,
    /// Previous and next only.
    None,
}

impl PaginationVariant {
    fn as_str(self) -> &'static str {
        match self {
            PaginationVariant::Pages => "pages",
            PaginationVariant::Count => "count",
            PaginationVariant::Compact => "compact",
            PaginationVariant::Dots => "dots",
            PaginationVariant::Input => "input",
            PaginationVariant::None => "none",
        }
    }
}

/// An entry of the page buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSlot {
    Page(usize),
    Ellipsis,
}

/// Upstream `generatePageRange`: the pages to show around `current`, with
/// ellipses standing for the rest; always 5 + 2 × siblings slots or fewer.
pub fn page_range(current: usize, total: usize, siblings: usize) -> Vec<PageSlot> {
    let slots = 5 + 2 * siblings;
    if total <= slots {
        return (1..=total).map(PageSlot::Page).collect();
    }
    let left = current.saturating_sub(siblings).max(1);
    let right = (current + siblings).min(total);
    let left_gap = left > 2;
    let right_gap = right < total - 1;
    let pages = |range: std::ops::RangeInclusive<usize>| range.map(PageSlot::Page);
    match (left_gap, right_gap) {
        (false, true) => pages(1..=3 + 2 * siblings)
            .chain([PageSlot::Ellipsis, PageSlot::Page(total)])
            .collect(),
        (true, false) => [PageSlot::Page(1), PageSlot::Ellipsis]
            .into_iter()
            .chain(pages(total + 1 - (3 + 2 * siblings)..=total))
            .collect(),
        _ => [PageSlot::Page(1), PageSlot::Ellipsis]
            .into_iter()
            .chain(pages(left..=right))
            .chain([PageSlot::Ellipsis, PageSlot::Page(total)])
            .collect(),
    }
}

type PageFn = Rc<dyn Fn(usize, &mut Window, &mut App)>;
type HrefFn = Rc<dyn Fn(usize) -> SharedString>;

/// Controls for moving between pages, upstream `Pagination`.
///
/// ```ignore
/// Pagination::new("results", page).total_pages(12).on_change(|page, _, _| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct Pagination {
    id: ElementId,
    page: usize,
    on_change: Option<PageFn>,
    change_action: Option<ActionFn<usize>>,
    page_href: Option<HrefFn>,
    total_items: Option<usize>,
    total_pages: Option<usize>,
    has_more: Option<bool>,
    page_size: usize,
    variant: PaginationVariant,
    page_label: Option<SharedString>,
    first_last: Option<bool>,
    step: usize,
    siblings: usize,
    size: ElementSize,
    disabled: bool,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl Pagination {
    /// Upstream `page`: the current page, from 1.
    pub fn new(id: impl Into<ElementId>, page: usize) -> Self {
        Pagination {
            id: id.into(),
            page: page.max(1),
            on_change: None,
            change_action: None,
            page_href: None,
            total_items: None,
            total_pages: None,
            has_more: None,
            page_size: 10,
            variant: PaginationVariant::Pages,
            page_label: None,
            first_last: None,
            step: 1,
            siblings: 1,
            size: ElementSize::Md,
            disabled: false,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `hasMore`: for cursor pagination without a total.
        ?has_more: bool,
        /// Upstream `variant`.
        variant: PaginationVariant,
        /// Upstream `pageLabel`: the noun before the input box.
        ?page_label: SharedString,
        /// Upstream `hasFirstLast`: on by default for `input`.
        ?first_last: bool,
        /// Upstream `size`: `Sm` or `Md`.
        size: ElementSize,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `label`: the landmark's name; "Pagination" by default.
        ?label: SharedString,
    }

    /// Upstream `totalItems`: the page count follows, unless `total_pages`.
    pub fn total_items(mut self, items: usize) -> Self {
        self.total_items = Some(items);
        self
    }

    /// Upstream `totalPages`.
    pub fn total_pages(mut self, pages: usize) -> Self {
        self.total_pages = Some(pages);
        self
    }

    /// Upstream `siblingCount`: pages each side of the current one.
    pub fn siblings(mut self, siblings: usize) -> Self {
        self.siblings = siblings;
        self
    }

    /// Upstream `pageSize`: items per page (at least 1).
    pub fn page_size(mut self, size: usize) -> Self {
        self.page_size = size.max(1);
        self
    }

    /// Upstream `step`: pages previous and next move (at least 1).
    pub fn step(mut self, step: usize) -> Self {
        self.step = step.max(1);
        self
    }

    /// Upstream `onChange`: the page asked for.
    pub fn on_change(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`; the new page
    /// shows until it finishes.
    pub fn change_action(
        mut self,
        f: impl Fn(usize, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `getPageHref`: the pages become links through the link
    /// handler; `on_change` still runs.
    pub fn page_href(mut self, f: impl Fn(usize) -> SharedString + 'static) -> Self {
        self.page_href = Some(Rc::new(f));
        self
    }

    fn page_count(&self) -> Option<usize> {
        self.total_pages
            .or(self.total_items.map(|items| items.div_ceil(self.page_size)))
    }
}

impl Styled for Pagination {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Pagination {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let i18n = i18n(cx);
        let total = self.page_count();
        if total == Some(0) || self.total_items == Some(0) {
            return div().into_any_element();
        }
        let optimistic = Optimistic::<usize>::new(&self.id, window, cx);
        let page = optimistic.shown(self.page, cx);
        let has_previous = page > 1;
        let has_next = match total {
            Some(total) => page < total,
            None => self.has_more.unwrap_or(false),
        };
        let clamp = move |p: usize| match total {
            Some(total) => p.clamp(1, total),
            None => p.max(1),
        };

        // Every activation: on_change, the announcement, the action, the link.
        let change = optimistic.change(
            self.on_change
                .clone()
                .map(|f| -> crate::choice::action::ChangeFn<usize> { f }),
            self.change_action.clone(),
        );
        let disabled = self.disabled;
        let href = self.page_href.clone();
        let handler = use_link_handler(cx);
        let announce_total = total;
        let go: PageFn = Rc::new(move |target, window, cx| {
            if disabled {
                return;
            }
            change(target, window, cx);
            let i18n = astryx_i18n::current(cx);
            let text = match announce_total {
                Some(total) => i18n.format(
                    "@astryx.pagination.pageOfTotal",
                    &[("current", target.into()), ("total", total.into())],
                ),
                None => i18n.format(
                    "@astryx.pagination.pageAnnounce",
                    &[("current", target.into())],
                ),
            };
            announce(text, Politeness::Polite, window, cx);
            if let Some(href) = &href {
                handler.open(&href(target), None, window, cx);
            }
        });

        let sm = self.size == ElementSize::Sm;
        let icon_size = if sm { IconSize::Sm } else { IconSize::Md };
        let nav_button =
            |key: &str, label: SharedString, icon: &'static str, target: usize, enabled: bool| {
                let go = go.clone();
                IconButton::new(
                    ElementId::Name(format!("{:?}-{key}", self.id).into()),
                    label,
                    Icon::new(icon).size(icon_size).mirror_in_rtl(dir),
                )
                .variant(ButtonVariant::Ghost)
                .size(self.size)
                .disabled(self.disabled || !enabled)
                .on_click(move |_, window, cx| go(target, window, cx))
            };
        let first_last = total.is_some()
            && match self.variant {
                PaginationVariant::Input => self.first_last.unwrap_or(true),
                PaginationVariant::Compact => self.first_last == Some(true),
                _ => false,
            };
        let (previous_label, next_label) = if self.step > 1 {
            (
                i18n.format(
                    "@astryx.pagination.previousBy",
                    &[("step", self.step.into())],
                ),
                i18n.format("@astryx.pagination.nextBy", &[("step", self.step.into())]),
            )
        } else {
            (
                i18n.t("@astryx.pagination.previous"),
                i18n.t("@astryx.pagination.next"),
            )
        };

        let text = |s: String| {
            div().flex().items_center().whitespace_nowrap().child(
                Text::new(s)
                    .text_type(TextType::Body)
                    .size(TextSize::Sm)
                    .color(TextColor::Secondary),
            )
        };
        let indicator: Option<AnyElement> = match (self.variant, total) {
            (PaginationVariant::Pages, Some(total)) => Some(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(t.spacing.s1)
                    .children(
                        page_range(page, total, self.siblings)
                            .into_iter()
                            .enumerate()
                            .map(|(i, slot)| match slot {
                                PageSlot::Ellipsis => div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .min_w(if sm {
                                        t.size.element_sm
                                    } else {
                                        t.size.element_md
                                    })
                                    .min_h(if sm {
                                        t.size.element_sm
                                    } else {
                                        t.size.element_md
                                    })
                                    .text_color(t.color.text_secondary)
                                    .child("…")
                                    .into_any_element(),
                                PageSlot::Page(n) => {
                                    let go = go.clone();
                                    let current = n == page;
                                    let mut button = Button::new(
                                        ElementId::Name(
                                            format!("{:?}-page-{i}-{n}", self.id).into(),
                                        ),
                                        i18n.format(
                                            "@astryx.pagination.goToPage",
                                            &[("page", n.into())],
                                        ),
                                    )
                                    .variant(ButtonVariant::Ghost)
                                    .size(self.size)
                                    .disabled(self.disabled)
                                    .on_click(move |_, window, cx| go(n, window, cx))
                                    .child(n.to_string());
                                    if current {
                                        button = button
                                            .bg(t.color.neutral)
                                            .font_weight(t.font.weight.medium);
                                    }
                                    button.into_any_element()
                                }
                            }),
                    )
                    .into_any_element(),
            ),
            (PaginationVariant::Count, _) => self.total_items.map(|items| {
                let from = (page - 1) * self.page_size + 1;
                let to = (page * self.page_size).min(items);
                text(
                    i18n.format(
                        "@astryx.pagination.count",
                        &[
                            ("from", from.into()),
                            ("to", to.into()),
                            ("total", items.into()),
                        ],
                    )
                    .to_string(),
                )
                .into_any_element()
            }),
            (PaginationVariant::Compact, Some(total)) => Some(
                text(
                    i18n.format(
                        "@astryx.pagination.pageOfTotal",
                        &[("current", page.into()), ("total", total.into())],
                    )
                    .to_string(),
                )
                .into_any_element(),
            ),
            (PaginationVariant::Input, _) => {
                // "Page [n] of N": a NumberInput named "Go to page" that
                // commits a page in range; while it rejects what was typed
                // it widens to show its status icon.
                let noun = self
                    .page_label
                    .clone()
                    .unwrap_or_else(|| i18n.t("@astryx.pagination.pageLabel"));
                let rejected = window.use_keyed_state(
                    ElementId::Name(format!("{}-page-rejected", self.id).into()),
                    cx,
                    |_, _| false,
                );
                let box_width = if sm {
                    t.size.element_sm
                } else {
                    t.size.element_md
                }
                .to_pixels(rem);
                let width = if *rejected.read(cx) {
                    box_width + t.spacing.s10.to_pixels(rem)
                } else {
                    box_width
                };
                let label_style = if sm {
                    &t.text.supporting
                } else {
                    &t.text.label
                };
                let label = |content: SharedString, target: &str| {
                    let mut el = div()
                        .whitespace_nowrap()
                        .text_style(label_style)
                        .text_color(t.color.text_secondary);
                    theme.refine(el.style(), target, &[("size", self.size.as_str())], None);
                    el.child(content)
                };
                let go = go.clone();
                let mut input = NumberInput::new(
                    ElementId::Name(format!("{}-page", self.id).into()),
                    i18n.t("@astryx.pagination.goToPageInput"),
                    Some(page as f64),
                )
                .label_hidden(true)
                .min(1.)
                .integer_only(true)
                .size(self.size)
                .status_placement(StatusPlacement::Tooltip)
                .width(width)
                .disabled(self.disabled || total.is_none())
                .on_entry_error_change({
                    let rejected = rejected.clone();
                    move |error, window, cx| {
                        rejected.update(cx, |r, _| *r = error.is_some());
                        window.refresh();
                    }
                })
                .on_change(move |next, window, cx| {
                    if let Some(next) = next.filter(|n| *n >= 1.)
                        && next as usize != page
                    {
                        go(next as usize, window, cx);
                    }
                });
                if let Some(total) = total {
                    input = input.max(total as f64);
                }
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.spacing.s1)
                        .child(label(noun, "pagination-input-label"))
                        .child(input)
                        .children(total.map(|total| {
                            label(
                                i18n.format(
                                    "@astryx.pagination.ofTotalPages",
                                    &[("total", total.into())],
                                ),
                                "pagination-input-total",
                            )
                        }))
                        .into_any_element(),
                )
            }
            (PaginationVariant::Dots, Some(total)) => {
                let list = ListFocus::new(&self.id, total, ListKeys::horizontal(), window, cx)
                    .rtl(dir.is_rtl())
                    .on_move({
                        let go = go.clone();
                        move |index, window, cx| go(index + 1, window, cx)
                    });
                let tab_stop = list.tab_stop(Some(page - 1), window);
                let dot_size = if sm { t.spacing.s1_5 } else { t.spacing.s2 };
                let dots = (0..total).map(|i| {
                    let active = i + 1 == page;
                    let handle = list.item_handle(i, tab_stop);
                    let go = go.clone();
                    let mut dot = div()
                        .id(ElementId::Name(format!("{:?}-dot-{i}", self.id).into()))
                        .track_focus(&handle)
                        .relative()
                        .size(dot_size)
                        .rounded_full()
                        .bg(if active {
                            t.color.accent
                        } else {
                            t.color.neutral
                        })
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            go(i + 1, window, cx)
                        });
                    if is_focus_visible(&handle, window) {
                        dot = dot.child(focus_ring(t, dot_size.to_pixels(rem) / 2., rem));
                    }
                    dot.role(Role::RadioButton)
                        .aria_label(
                            i18n.format("@astryx.pagination.goToPage", &[("page", (i + 1).into())]),
                        )
                        .aria_toggled(if active {
                            Toggled::True
                        } else {
                            Toggled::False
                        })
                });
                Some(
                    div()
                        .id(ElementId::Name(format!("{:?}-dots", self.id).into()))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(t.spacing.s1)
                        .on_key_down(list.key_down_listener())
                        .role(Role::RadioGroup)
                        .aria_label(i18n.t("@astryx.pagination.pageIndicators"))
                        .children(dots)
                        .into_any_element(),
                )
            }
            _ => None,
        };

        let mut controls = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.s1)
            .min_w_0();
        theme.refine(
            controls.style(),
            "pagination-list",
            &[
                ("variant", self.variant.as_str()),
                ("size", self.size.as_str()),
            ],
            None,
        );
        let controls = controls
            .when(first_last, |el| {
                el.child(nav_button(
                    "first",
                    i18n.t("@astryx.pagination.first"),
                    "chevronsLeft",
                    1,
                    has_previous,
                ))
            })
            .child(nav_button(
                "previous",
                previous_label,
                "chevronLeft",
                clamp(page.saturating_sub(self.step)),
                has_previous,
            ))
            .children(indicator)
            .child(nav_button(
                "next",
                next_label,
                "chevronRight",
                clamp(page + self.step),
                has_next,
            ))
            .when(first_last, |el| {
                el.child(nav_button(
                    "last",
                    i18n.t("@astryx.pagination.last"),
                    "chevronsRight",
                    total.unwrap_or(page),
                    has_next,
                ))
            });

        let mut el = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(t.spacing.s4);
        if self.disabled {
            el = el.opacity(0.5);
        }
        theme.refine(
            el.style(),
            "pagination",
            &[("variant", self.variant.as_str())],
            None,
        );
        el.style().refine(&self.style);
        let name = self
            .label
            .unwrap_or_else(|| i18n.t("@astryx.pagination.label"));
        el.id(self.id)
            .role(Role::Navigation)
            .aria_label(name)
            .child(controls)
            .into_any_element()
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const SELECTOR: &str = "not yet: the page-size selector lands with Selector (M5)";

pub const PAGINATION_DOC: ComponentDoc = ComponentDoc {
    name: "Pagination",
    rust_type: "astryx::components::pagination::Pagination",
    props: &[
        p("page", RustApi::Argument("page")),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("getPageHref", RustApi::Method("page_href")),
        p(
            "as",
            RustApi::Waived("the link handler (link_provider) routes every href"),
        ),
        p("totalItems", RustApi::Method("total_items")),
        p("totalPages", RustApi::Method("total_pages")),
        p("hasMore", RustApi::Method("has_more")),
        p("pageSize", RustApi::Method("page_size")),
        p("pageSizeOptions", RustApi::Waived(SELECTOR)),
        p("onPageSizeChange", RustApi::Waived(SELECTOR)),
        p("variant", RustApi::Method("variant")),
        p("pageLabel", RustApi::Method("page_label")),
        p("hasFirstLast", RustApi::Method("first_last")),
        p("step", RustApi::Method("step")),
        p("siblingCount", RustApi::Method("siblings")),
        p("size", RustApi::Method("size")),
        p("isDisabled", RustApi::Method("disabled")),
        p("label", RustApi::Method("label")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
