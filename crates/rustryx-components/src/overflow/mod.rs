//! OverflowList (M6). Upstream: `OverflowList/` and `hooks/useOverflow.ts`.
//!
//! - A hidden copy of every item, and of the indicator for all of them, is
//!   measured each frame (FR1); the visible row shows the items that fit its
//!   width (or its parent's, observing the parent), between
//!   `min_visible_items` and `max_visible_items` (FR2). Before the first
//!   measure it shows them all, with no indicator.
//! - The indicator the renderer returns for the collapsed items goes after
//!   the visible ones, or before them collapsing from the start (FR3).
//! - One row clips and never wraps, at least as wide as its folded width;
//!   `max_rows` wraps into that many measured rows (FR4).
//! - `on_overflow_change` hears each change of the collapsed set (FR5).
//!
//! Items are builders, since the measuring copy renders each one too.

mod compute;
pub mod examples;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, SharedString, Size,
    StyleRefinement, Styled, Window, canvas, div, prelude::*, px,
};
use rustryx_theme::current;

pub use compute::CollapseFrom;
use compute::{Fit, FitInput, compute, inline_bounds};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::layout::SpacingStep;

type ItemFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type RendererFn = Rc<dyn Fn(&[OverflowItem], &mut Window, &mut App) -> AnyElement>;
type ChangeFn = Rc<dyn Fn(&[OverflowItem], &mut Window, &mut App)>;

/// A collapsed item, upstream `OverflowItem`: its index among the items,
/// and its key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverflowItem {
    pub index: usize,
    pub key: Option<SharedString>,
}

/// Upstream `behavior`: whose width the row fits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverflowBehavior {
    /// Its own.
    #[default]
    ObserveSelf,
    /// Its parent's, for a row sized by its content.
    ObserveParent,
}

#[derive(Default)]
struct OverflowState {
    sizes: Vec<Option<Size<Pixels>>>,
    indicator: Option<Size<Pixels>>,
    available: Option<Pixels>,
    /// The collapsed set last reported.
    reported: Vec<OverflowItem>,
}

/// A row that hides what does not fit behind an indicator, upstream
/// `OverflowList`.
///
/// ```ignore
/// OverflowList::new("tags")
///     .item(|_, _| Token::new("tag-a", "Design").into_any_element())
///     .item(|_, _| Token::new("tag-b", "Engineering").into_any_element())
///     .overflow_renderer(|hidden, _, _| Badge::new(format!("+{}", hidden.len())).into_any_element())
/// ```
#[derive(IntoElement)]
pub struct OverflowList {
    id: ElementId,
    items: Vec<(Option<SharedString>, ItemFn)>,
    gap: SpacingStep,
    min_visible_items: usize,
    max_visible_items: Option<usize>,
    max_rows: Option<usize>,
    collapse_from: CollapseFrom,
    behavior: OverflowBehavior,
    overflow_renderer: Option<RendererFn>,
    on_overflow_change: Option<ChangeFn>,
    style: StyleRefinement,
}

impl OverflowList {
    pub fn new(id: impl Into<ElementId>) -> Self {
        OverflowList {
            id: id.into(),
            items: Vec::new(),
            gap: SpacingStep::S2,
            min_visible_items: 0,
            max_visible_items: None,
            max_rows: None,
            collapse_from: CollapseFrom::End,
            behavior: OverflowBehavior::ObserveSelf,
            overflow_renderer: None,
            on_overflow_change: None,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `gap`: a spacing step, 2 by default.
        gap: SpacingStep,
        /// Upstream `collapseFrom`.
        collapse_from: CollapseFrom,
        /// Upstream `behavior`.
        behavior: OverflowBehavior,
    }

    /// Upstream `children`: one item, built by `f`.
    pub fn item(mut self, f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.items.push((None, Rc::new(f)));
        self
    }

    /// An item with a key, which the collapsed items report (upstream's
    /// child `key`).
    pub fn keyed_item(
        mut self,
        key: impl Into<SharedString>,
        f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.items.push((Some(key.into()), Rc::new(f)));
        self
    }

    /// Upstream `minVisibleItems`: always shown, overflowing or not.
    pub fn min_visible_items(mut self, n: usize) -> Self {
        self.min_visible_items = n;
        self
    }

    /// Upstream `maxVisibleItems`: never more than this (the floor wins).
    pub fn max_visible_items(mut self, n: usize) -> Self {
        self.max_visible_items = Some(n);
        self
    }

    /// Upstream `maxRows`: wrap into up to this many rows.
    pub fn max_rows(mut self, n: usize) -> Self {
        self.max_rows = Some(n);
        self
    }

    /// Upstream `overflowRenderer`: the indicator for the collapsed items.
    pub fn overflow_renderer(
        mut self,
        f: impl Fn(&[OverflowItem], &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.overflow_renderer = Some(Rc::new(f));
        self
    }

    /// Upstream `onOverflowChange`: the collapsed items, when they change.
    pub fn on_overflow_change(
        mut self,
        f: impl Fn(&[OverflowItem], &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_overflow_change = Some(Rc::new(f));
        self
    }
}

impl Styled for OverflowList {
    /// Upstream `xstyle`: the visible row.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for OverflowList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let gap = self.gap.token(t).to_pixels(rem);
        let count = self.items.len();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-overflow", self.id).into()),
            cx,
            |_, _| OverflowState::default(),
        );
        state.update(cx, |s, _| s.sizes.resize(count, None));
        let multi = self.max_rows.is_some_and(|r| r > 1);
        let all: Vec<OverflowItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, (key, _))| OverflowItem {
                index,
                key: key.clone(),
            })
            .collect();

        // The fit, once every item is measured.
        let measured = {
            let s = state.read(cx);
            let sizes: Option<Vec<Size<Pixels>>> = s.sizes.iter().copied().collect();
            sizes.zip(s.available).map(|(sizes, available)| {
                (sizes, available, s.indicator.map_or(px(0.), |i| i.width))
            })
        };
        let (fit, bounds, row_height) = match &measured {
            Some((sizes, available, indicator)) => {
                let widths: Vec<f32> = sizes.iter().map(|s| f32::from(s.width)).collect();
                let fit = compute(&FitInput {
                    widths: &widths,
                    gap: f32::from(gap),
                    available: f32::from(*available) + 0.05,
                    indicator: f32::from(*indicator),
                    min_visible: self.min_visible_items,
                    max_visible: self.max_visible_items,
                    max_rows: self.max_rows,
                    collapse_from: self.collapse_from,
                });
                let bounds = inline_bounds(
                    &widths,
                    f32::from(*indicator),
                    f32::from(gap),
                    self.min_visible_items,
                    self.collapse_from,
                    self.max_visible_items,
                );
                let row_height = sizes.iter().map(|s| s.height).fold(px(0.), Pixels::max);
                (fit, Some(bounds), row_height)
            }
            None => (
                Fit {
                    visible: count,
                    rows: 1,
                },
                None,
                px(0.),
            ),
        };
        let (visible, collapsed): (Vec<usize>, Vec<OverflowItem>) = match self.collapse_from {
            CollapseFrom::End => ((0..fit.visible).collect(), all[fit.visible..].to_vec()),
            CollapseFrom::Start => (
                (count - fit.visible..count).collect(),
                all[..count - fit.visible].to_vec(),
            ),
        };
        let overflowing = measured.is_some() && !collapsed.is_empty();

        // FR5: the collapsed set, when it changes after a measure.
        if measured.is_some()
            && let Some(report) = self.on_overflow_change.clone()
            && state.read(cx).reported != collapsed
        {
            state.update(cx, |s, _| s.reported = collapsed.clone());
            let collapsed = collapsed.clone();
            window.defer(cx, move |window, cx| report(&collapsed, window, cx));
        }

        // FR1: the hidden measuring copy.
        let probe = |slot: Option<usize>, el: AnyElement| {
            let state = state.clone();
            div()
                .relative()
                .flex_none()
                .child(el)
                .child(
                    canvas(
                        move |bounds, window, cx| {
                            let changed = state.update(cx, |s, _| {
                                let target = match slot {
                                    Some(i) => s.sizes.get_mut(i),
                                    None => Some(&mut s.indicator),
                                };
                                match target {
                                    Some(size) if *size != Some(bounds.size) => {
                                        *size = Some(bounds.size);
                                        true
                                    }
                                    _ => false,
                                }
                            });
                            if changed {
                                window.refresh();
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element()
        };
        let mut copy = div()
            .absolute()
            .top_0()
            .left_0()
            .h(px(0.))
            .overflow_hidden()
            .invisible()
            .flex()
            .items_center()
            .gap(gap)
            .whitespace_nowrap();
        for (i, (_, build)) in self.items.iter().enumerate() {
            copy = copy.child(probe(Some(i), build(window, cx)));
        }
        if let Some(render) = &self.overflow_renderer {
            copy = copy.child(probe(None, render(&all, window, cx)));
        }

        // The visible row.
        let indicator = overflowing
            .then(|| {
                self.overflow_renderer
                    .as_ref()
                    .map(|f| f(&collapsed, window, cx))
            })
            .flatten();
        let mut row = div()
            .id(self.id.clone())
            .flex()
            .gap(gap)
            .overflow_hidden()
            .min_w_0();
        row = if multi {
            let mut row = row.flex_wrap().content_start();
            if let Some(max) = self.max_rows
                && row_height > px(0.)
            {
                row = row.max_h(row_height * max as f32 + gap * (max - 1) as f32);
            }
            row
        } else {
            let mut row = row.items_center().whitespace_nowrap();
            // FR4: never narrower than its folded width, so the indicator
            // is never clipped.
            if let Some((collapsed, rendered)) = bounds {
                row = row.min_w(px(collapsed.min(rendered)));
            }
            row
        };
        if self.behavior == OverflowBehavior::ObserveParent && overflowing {
            row = row.w_full();
        }
        theme.refine(row.style(), "overflow-list", &[], None);
        row.style().refine(&self.style);
        let (before, after) = match self.collapse_from {
            CollapseFrom::Start => (indicator, None),
            CollapseFrom::End => (None, indicator),
        };
        let shown: Vec<AnyElement> = visible
            .iter()
            .map(|&i| (self.items[i].1)(window, cx))
            .collect();
        let row = row
            .child(copy)
            .children(before)
            .children(shown)
            .children(after);
        // The width the row fits: its own, or its parent's.
        let available = {
            let state = state.clone();
            canvas(
                move |bounds, window, cx| {
                    if state.read(cx).available != Some(bounds.size.width) {
                        state.update(cx, |s, _| s.available = Some(bounds.size.width));
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };
        match self.behavior {
            OverflowBehavior::ObserveSelf => row.child(available).into_any_element(),
            OverflowBehavior::ObserveParent => div()
                .relative()
                .w_full()
                .child(available)
                .child(row)
                .into_any_element(),
        }
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const OVERFLOW_LIST_DOC: ComponentDoc = ComponentDoc {
    name: "OverflowList",
    rust_type: "rustryx::components::overflow::OverflowList",
    props: &[
        p(
            "children",
            RustApi::Method("item (or keyed_item), one per child"),
        ),
        p("overflowRenderer", RustApi::Method("overflow_renderer")),
        p("onOverflowChange", RustApi::Method("on_overflow_change")),
        p("gap", RustApi::Method("gap")),
        p("minVisibleItems", RustApi::Method("min_visible_items")),
        p("maxVisibleItems", RustApi::Method("max_visible_items")),
        p("maxRows", RustApi::Method("max_rows")),
        p("collapseFrom", RustApi::Method("collapse_from")),
        p("behavior", RustApi::Method("behavior")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
