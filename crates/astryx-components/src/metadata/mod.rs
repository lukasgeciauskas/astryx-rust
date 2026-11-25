//! MetadataList and MetadataListItem (M6). Upstream: `MetadataList/`.
//!
//! - Vertical lists are grids (fork patch R7's track lists): start labels in
//!   an `auto` track beside a `minmax(0, 1fr)` value track, top labels in
//!   one track; `multi` fills as many 280px columns as fit, counted from
//!   the list's measured width as Grid does; a number makes that many.
//! - Horizontal lists wrap their entries in one row, labels on top.
//! - `max_items` collapses a vertical list behind a Show more toggle.

pub mod examples;

#[cfg(test)]
mod tests;

use astryx_i18n::current as i18n;
use astryx_primitives::{control_focus, is_focus_visible, provide, use_context};
use astryx_theme::current;
use gpui::{
    AbsoluteLength, AnyElement, App, ElementId, GridTrack, IntoElement, ParentElement, Pixels,
    RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, canvas, div, prelude::*, px,
    relative,
};
use smallvec::SmallVec;

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::layout::GridColumns;

/// Upstream `columns`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MetadataColumns {
    #[default]
    Single,
    /// As many 280px columns as fit.
    Multi,
    /// Exactly this many; below 2 it is `Multi`'s template, at 1 `Single`.
    Count(u16),
}

impl MetadataColumns {
    fn as_str(self) -> String {
        match self {
            MetadataColumns::Single => "single".into(),
            MetadataColumns::Multi => "multi".into(),
            MetadataColumns::Count(n) => n.to_string(),
        }
    }

    fn is_multi(self) -> bool {
        matches!(self, MetadataColumns::Multi) || matches!(self, MetadataColumns::Count(n) if n > 1)
    }

    fn is_single(self) -> bool {
        matches!(self, MetadataColumns::Single | MetadataColumns::Count(1))
    }
}

/// Upstream `label.position`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataLabelPosition {
    /// Beside the value.
    Start,
    /// Above the value.
    Top,
}

/// Upstream `orientation`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MetadataOrientation {
    #[default]
    Vertical,
    /// One wrapping row, labels on top.
    Horizontal,
}

impl MetadataOrientation {
    fn as_str(self) -> &'static str {
        match self {
            MetadataOrientation::Vertical => "vertical",
            MetadataOrientation::Horizontal => "horizontal",
        }
    }
}

/// What a MetadataList tells its items: whether labels stack.
#[derive(Clone, Copy, Debug)]
struct Stacked(bool);

/// The least width of a `multi` column.
const COLUMN_MIN: f32 = 280.;

/// Labeled values, upstream `MetadataList`.
///
/// ```ignore
/// MetadataList::new("details")
///     .child(MetadataListItem::new("Name", "MetadataList"))
///     .child(MetadataListItem::new("Status", "Active"))
/// ```
#[derive(IntoElement)]
pub struct MetadataList {
    id: ElementId,
    children: SmallVec<[AnyElement; 8]>,
    columns: MetadataColumns,
    label_position: Option<MetadataLabelPosition>,
    label_width: Option<AbsoluteLength>,
    max_items: Option<usize>,
    orientation: MetadataOrientation,
    title: Option<AnyElement>,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl MetadataList {
    pub fn new(id: impl Into<ElementId>) -> Self {
        MetadataList {
            id: id.into(),
            children: SmallVec::new(),
            columns: MetadataColumns::Single,
            label_position: None,
            label_width: None,
            max_items: None,
            orientation: MetadataOrientation::Vertical,
            title: None,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `columns`.
        columns: MetadataColumns,
        /// Upstream `orientation`.
        orientation: MetadataOrientation,
    }

    /// Upstream `maxNumOfItems`: more than this collapse behind Show more.
    pub fn max_items(mut self, max: usize) -> Self {
        self.max_items = Some(max);
        self
    }

    /// Upstream `label.position`: start for one column, top otherwise.
    pub fn label_position(mut self, position: MetadataLabelPosition) -> Self {
        self.label_position = Some(position);
        self
    }

    /// Upstream `label.width`: the label track's width, for start labels.
    pub fn label_width(mut self, width: impl Into<AbsoluteLength>) -> Self {
        self.label_width = Some(width.into());
        self
    }

    /// Upstream `title`: content above the list; not its name.
    pub fn title(mut self, title: impl IntoElement) -> Self {
        self.title = Some(title.into_any_element());
        self
    }

    /// Upstream `aria-label`: the list's name (on its `dl`).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for MetadataList {
    /// Upstream `children`: MetadataListItems.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for MetadataList {
    /// Upstream `xstyle`, `className` and `style`: the root.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MetadataList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let horizontal = self.orientation == MetadataOrientation::Horizontal;
        // FR3: top labels for several columns and horizontal lists.
        let position = if horizontal {
            MetadataLabelPosition::Top
        } else {
            self.label_position.unwrap_or(if self.columns.is_multi() {
                MetadataLabelPosition::Top
            } else {
                MetadataLabelPosition::Start
            })
        };
        let stacked = position == MetadataLabelPosition::Top;

        // FR4: the collapse, vertical only.
        let show_all = window.use_keyed_state(
            ElementId::Name(format!("{}-show-all", self.id).into()),
            cx,
            |_, _| false,
        );
        let total = self.children.len();
        let collapses = !horizontal && self.max_items.is_some_and(|max| total > max);
        let shown_all = *show_all.read(cx);
        let visible = if collapses && !shown_all {
            self.max_items.unwrap_or(total)
        } else {
            total
        };
        let children: Vec<AnyElement> = self.children.into_iter().take(visible).collect();

        // FR2: the list's tracks.
        let width = window.use_keyed_state(
            ElementId::Name(format!("{}-width", self.id).into()),
            cx,
            |_, _| None::<Pixels>,
        );
        let s2 = t.spacing.s2.to_pixels(rem);
        let s3 = t.spacing.s3.to_pixels(rem);
        let s4 = t.spacing.s4.to_pixels(rem);
        let pair = |label: GridTrack| {
            [
                label,
                GridTrack::MinMax(
                    Box::new(GridTrack::Length(px(0.).into())),
                    Box::new(GridTrack::Fr(1.)),
                ),
            ]
        };
        let auto_fill = |w: Option<Pixels>| {
            GridColumns::min_width(px(COLUMN_MIN))
                .count(w, s4, visible)
                .max(1)
        };
        let mut list = div()
            .id(ElementId::Name(format!("{}-list", self.id).into()))
            .relative()
            .role(Role::DescriptionList);
        if let Some(label) = self.label.clone() {
            list = list.aria_label(label);
        }
        list = if horizontal {
            list.flex().flex_row().flex_wrap().gap(s4)
        } else {
            let label_track = || match self.label_width {
                Some(w) => GridTrack::Length(w),
                None => GridTrack::Auto,
            };
            match (self.columns, stacked) {
                (MetadataColumns::Count(n), true) if n > 1 => list
                    .grid()
                    .grid_template_columns(vec![GridTrack::Fr(1.); n as usize])
                    .gap(s4),
                (MetadataColumns::Count(n), false) if n > 1 => list
                    .grid()
                    .grid_template_columns(
                        (0..n)
                            .flat_map(|_| pair(GridTrack::Auto))
                            .collect::<Vec<_>>(),
                    )
                    .gap(s4),
                (columns, true) if columns.is_single() => list
                    .grid()
                    .grid_template_columns([GridTrack::Fr(1.)])
                    .gap(s3),
                (columns, false) if columns.is_single() || self.label_width.is_some() => list
                    .grid()
                    .grid_template_columns(pair(label_track()))
                    .gap_y(s2)
                    .gap_x(s4)
                    .items_baseline(),
                // Multi: as many 280px columns as fit.
                _ => {
                    let n = auto_fill(*width.read(cx));
                    list.grid()
                        .grid_template_columns(vec![GridTrack::Fr(1.); n as usize])
                        .gap(s4)
                        .child(
                            canvas(
                                move |bounds, _, cx| {
                                    if *width.read(cx) != Some(bounds.size.width) {
                                        width.update(cx, |w, _| *w = Some(bounds.size.width));
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        )
                }
            }
        };
        let list = provide(Stacked(stacked), list.children(children));

        let mut root = div().id(self.id.clone()).flex().flex_col();
        let columns = self.columns.as_str();
        theme.refine(
            root.style(),
            "metadata-list",
            &[
                ("columns", columns.as_str()),
                ("orientation", self.orientation.as_str()),
            ],
            None,
        );
        root.style().refine(&self.style);
        if let Some(title) = self.title {
            root = root.child(div().mb(s3).child(title));
        }
        root = root.child(list);
        if collapses {
            let toggle_id = ElementId::Name(format!("{}-toggle", self.id).into());
            let focus = control_focus(&toggle_id, window, cx);
            let text = i18n(cx).t(if shown_all {
                "@astryx.metadataList.showLess"
            } else {
                "@astryx.metadataList.showMore"
            });
            let state = show_all.clone();
            let toggle_selector = format!("{}-toggle", self.id);
            let mut button = div()
                .id(toggle_id)
                .debug_selector(move || toggle_selector)
                .track_focus(&focus)
                .relative()
                .self_start()
                .py(s2)
                .cursor_pointer()
                .text_color(t.color.text_accent)
                .text_size(t.text.body.size)
                .line_height(relative(t.text.body.leading))
                .font_weight(t.font.weight.medium)
                .role(Role::Button)
                .aria_expanded(shown_all)
                .aria_label(text.clone())
                .on_click(move |_, window, cx| {
                    state.update(cx, |s, _| *s = !*s);
                    window.refresh();
                })
                .child(text);
            if is_focus_visible(&focus, window) {
                button = button.child(focus_ring(t, t.radius.element.to_pixels(rem), rem));
            }
            root = root.child(button);
        }
        root
    }
}

/// One labeled value, upstream `MetadataListItem`.
#[derive(IntoElement)]
pub struct MetadataListItem {
    label: SharedString,
    value: AnyElement,
    icon: Option<AnyElement>,
    style: StyleRefinement,
}

impl MetadataListItem {
    /// Upstream `label` and `children` (the value).
    pub fn new(label: impl Into<SharedString>, value: impl IntoElement) -> Self {
        MetadataListItem {
            label: label.into(),
            value: value.into_any_element(),
            icon: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `icon`: before the label.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }
}

impl Styled for MetadataListItem {
    /// Upstream `xstyle`: the label cell beside a value, or the wrapper
    /// of a stacked item.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MetadataListItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let stacked = use_context::<Stacked>(cx).is_some_and(|s| s.0);
        let mut term = div()
            .id(ElementId::Name(
                format!("metadata-term-{}", self.label).into(),
            ))
            .flex()
            .items_center()
            .gap(t.spacing.s2)
            .min_h(px(24.))
            .text_size(t.text.body.size)
            .line_height(relative(t.text.body.leading))
            .font_weight(t.font.weight.medium)
            .text_color(t.color.text_secondary)
            .role(Role::Term)
            .children(self.icon)
            .child(self.label.clone());
        let value = div()
            .id(ElementId::Name(
                format!("metadata-value-{}", self.label).into(),
            ))
            .min_h(px(24.))
            .min_w_0()
            .text_size(t.text.body.size)
            .line_height(relative(t.text.body.leading))
            .text_color(t.color.text_primary)
            .role(Role::Definition)
            .child(self.value);
        if stacked {
            let mut wrapper = div().flex().flex_col().gap(t.spacing.s0_5).min_w_0();
            theme.refine(wrapper.style(), "metadata-list-item", &[], None);
            wrapper.style().refine(&self.style);
            wrapper.child(term).child(value).into_any_element()
        } else {
            // FR1: the term and value are the list grid's cells.
            theme.refine(term.style(), "metadata-list-item", &[], None);
            term.style().refine(&self.style);
            div()
                .display_contents()
                .child(term)
                .child(value)
                .into_any_element()
        }
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const METADATA_LIST_DOC: ComponentDoc = ComponentDoc {
    name: "MetadataList",
    rust_type: "astryx::components::metadata::MetadataList",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("columns", RustApi::Method("columns")),
        p("label", RustApi::Method("label_position and label_width")),
        p("maxNumOfItems", RustApi::Method("max_items")),
        p("orientation", RustApi::Method("orientation")),
        p("title", RustApi::Method("title")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const METADATA_LIST_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "MetadataListItem",
    rust_type: "astryx::components::metadata::MetadataListItem",
    props: &[
        p("children", RustApi::Argument("value")),
        p("label", RustApi::Argument("label")),
        p("icon", RustApi::Method("icon")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p(
            "data-testid",
            RustApi::Waived("GPUI tests find elements by id"),
        ),
    ],
    example: include_str!("examples.rs"),
};
