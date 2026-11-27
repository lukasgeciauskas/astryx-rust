//! Stack, HStack, VStack and StackItem. Upstream: `Stack/`, `HStack/`,
//! `VStack/`.

use std::panic::Location;

use gpui::{
    AnyElement, App, Div, ElementId, IntoElement, ParentElement, RenderOnce, StyleRefinement,
    Styled, Window, div, prelude::*,
};
use rustryx_i18n::{Direction, LogicalStyled, use_direction};
use rustryx_primitives::{provide, use_context};
use rustryx_theme::{ResolvedTheme, current};
use smallvec::SmallVec;

use super::spacing::{Padding, Sizing, SpacingStep, padding_setters, size_setters};

/// Upstream `direction`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StackDirection {
    Horizontal,
    #[default]
    Vertical,
}

impl StackDirection {
    fn as_str(self) -> &'static str {
        match self {
            StackDirection::Horizontal => "horizontal",
            StackDirection::Vertical => "vertical",
        }
    }
}

/// Alignment along an axis, upstream `StackAlignment`: the main-axis values
/// (`between`, `around`, `evenly`) and the cross-axis one (`stretch`) apply
/// only on their axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackAlign {
    Start,
    Center,
    End,
    Stretch,
    Between,
    Around,
    Evenly,
}

/// Upstream `wrap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

impl StackWrap {
    fn as_str(self) -> &'static str {
        match self {
            StackWrap::NoWrap => "nowrap",
            StackWrap::Wrap => "wrap",
            StackWrap::WrapReverse => "wrap-reverse",
        }
    }
}

/// The stack an item is in, for its cross-axis alignment.
#[derive(Clone, Copy)]
struct StackContext {
    direction: StackDirection,
}

/// Arranges children in a row or column with token spacing.
///
/// ```ignore
/// Stack::vertical().gap(SpacingStep::S4).child(Text::new("One")).child(Text::new("Two"))
/// ```
#[derive(IntoElement)]
pub struct Stack {
    direction: StackDirection,
    h_align: Option<StackAlign>,
    v_align: Option<StackAlign>,
    justify: Option<StackAlign>,
    align: Option<StackAlign>,
    size: Sizing,
    gap: Option<SpacingStep>,
    padding: Padding,
    scrollable: bool,
    wrap: Option<StackWrap>,
    id: Option<ElementId>,
    source: &'static Location<'static>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 4]>,
}

impl Stack {
    /// A stack in `direction`, upstream `<Stack direction>`.
    #[track_caller]
    pub fn new(direction: StackDirection) -> Self {
        Stack {
            direction,
            h_align: None,
            v_align: None,
            justify: None,
            align: None,
            size: Sizing::default(),
            gap: None,
            padding: Padding::default(),
            scrollable: false,
            wrap: None,
            id: None,
            source: Location::caller(),
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    /// A row, upstream `HStack`.
    #[track_caller]
    pub fn horizontal() -> Self {
        Self::new(StackDirection::Horizontal)
    }

    /// A column, upstream `VStack`.
    #[track_caller]
    pub fn vertical() -> Self {
        Self::new(StackDirection::Vertical)
    }

    rustryx_primitives::setters! {
        /// Upstream `hAlign`: horizontal alignment, whichever axis that is.
        ?h_align: StackAlign,
        /// Upstream `vAlign`: vertical alignment.
        ?v_align: StackAlign,
        /// Upstream `justify`: main-axis alignment; `hAlign`/`vAlign` win.
        ?justify: StackAlign,
        /// Upstream `align`: cross-axis alignment; `hAlign`/`vAlign` win.
        ?align: StackAlign,
        /// Upstream `gap`: the space between items.
        ?gap: SpacingStep,
        /// Upstream `isScrollable`: scrolls when the content overflows.
        !scrollable,
        /// Upstream `wrap`.
        ?wrap: StackWrap,
    }

    padding_setters!();
    size_setters!();

    /// Upstream `id`: the element id, for tests, accessibility and scroll state.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// (main, cross) alignment after resolving `hAlign`/`vAlign` against
    /// `justify`/`align` for the direction, as upstream does.
    fn alignment(&self) -> (Option<StackAlign>, Option<StackAlign>) {
        let horizontal = self.direction == StackDirection::Horizontal;
        let h = self
            .h_align
            .or(if horizontal { self.justify } else { self.align });
        let v = self
            .v_align
            .or(if horizontal { self.align } else { self.justify });
        if horizontal { (h, v) } else { (v, h) }
    }

    pub(crate) fn root(&self, theme: &ResolvedTheme, dir: Direction) -> Div {
        let t = theme.tokens();
        let horizontal = self.direction == StackDirection::Horizontal;
        let mut el = if horizontal {
            div().flex_row_logical(dir)
        } else {
            div().flex().flex_col()
        };
        let (main, cross) = self.alignment();
        el = match main {
            Some(StackAlign::Start) => el.justify_start(),
            Some(StackAlign::Center) => el.justify_center(),
            Some(StackAlign::End) => el.justify_end(),
            Some(StackAlign::Between) => el.justify_between(),
            Some(StackAlign::Around) => el.justify_around(),
            Some(StackAlign::Evenly) => el.justify_evenly(),
            Some(StackAlign::Stretch) | None => el,
        };
        // A column's cross axis is the inline axis: start is the right in RTL.
        let flip = !horizontal && dir.is_rtl();
        el = match cross {
            Some(StackAlign::Start) if flip => el.items_end(),
            Some(StackAlign::End) if flip => el.items_start(),
            Some(StackAlign::Start) => el.items_start(),
            Some(StackAlign::Center) => el.items_center(),
            Some(StackAlign::End) => el.items_end(),
            Some(StackAlign::Stretch) => el.items_stretch(),
            _ => el,
        };
        if let Some(gap) = self.gap {
            el = el.gap(gap.token(t));
        }
        el = match self.wrap {
            Some(StackWrap::Wrap) => el.flex_wrap(),
            Some(StackWrap::WrapReverse) => el.flex_wrap_reverse(),
            Some(StackWrap::NoWrap) => el.flex_nowrap(),
            None => el,
        };
        el = self.padding.apply(el, t, dir);
        el = self.size.apply(el);
        let mut props = vec![("direction", self.direction.as_str())];
        if let Some(gap) = self.gap {
            props.push(("gap", gap.as_str()));
        }
        if let Some(wrap) = self.wrap {
            props.push(("wrap", wrap.as_str()));
        }
        theme.refine(el.style(), "stack", &props, None);
        el.style().refine(&self.style);
        el
    }
}

impl Styled for Stack {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Stack {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Stack {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let dir = use_direction(cx);
        let el = self.root(&theme, dir);
        let context = StackContext {
            direction: self.direction,
        };
        let children = self
            .children
            .into_iter()
            .map(|child| provide(context, child).into_any_element());
        let id = self
            .id
            .clone()
            .unwrap_or(ElementId::CodeLocation(*self.source));
        if self.scrollable {
            let scroller_handle =
                crate::gestures::scroller::use_scroll_handle(id.clone(), window, cx);
            el.id(id.clone())
                .overflow_scroll()
                .track_scroll(&scroller_handle)
                .child(crate::gestures::scroller::scroll_begin(
                    id,
                    Some(&scroller_handle),
                    (true, true),
                ))
                .children(children)
                .child(crate::gestures::scroller::scroll_end())
                .into_any_element()
        } else if let Some(id) = self.id {
            el.id(id).children(children).into_any_element()
        } else {
            el.children(children).into_any_element()
        }
    }
}

/// Upstream `StackItem` `size`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackItemSize {
    /// Grows to fill the space left over.
    Fill,
    /// Keeps its size: neither grows nor shrinks.
    Static,
}

/// A stack child with its own sizing and alignment, upstream `StackItem`.
#[derive(IntoElement)]
pub struct StackItem {
    size: Option<StackItemSize>,
    cross_align_self: Option<StackAlign>,
    scrollable: bool,
    id: Option<ElementId>,
    source: &'static Location<'static>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl StackItem {
    #[track_caller]
    pub fn new() -> Self {
        StackItem {
            size: None,
            cross_align_self: None,
            scrollable: false,
            id: None,
            source: Location::caller(),
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `size`.
        ?size: StackItemSize,
        /// Upstream `crossAlignSelf`: this item's cross-axis alignment.
        ?cross_align_self: StackAlign,
        /// Upstream `isScrollable`.
        !scrollable,
    }

    /// The element id, for scroll state.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl Default for StackItem {
    #[track_caller]
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for StackItem {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for StackItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for StackItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let dir = use_direction(cx);
        let parent = use_context::<StackContext>(cx).map(|c| c.direction);
        // Flex items default to `min-width: auto`; upstream resets it so a
        // filling item can shrink below its content.
        let mut el = div().min_w_0().min_h_0();
        el = match self.size {
            Some(StackItemSize::Fill) => el.flex_grow(1.),
            Some(StackItemSize::Static) => el.flex_none(),
            None => el,
        };
        let flip = parent == Some(StackDirection::Vertical) && dir.is_rtl();
        el = match self.cross_align_self {
            Some(StackAlign::Start) if flip => el.self_end(),
            Some(StackAlign::End) if flip => el.self_start(),
            Some(StackAlign::Start) => el.self_start(),
            Some(StackAlign::Center) => el.self_center(),
            Some(StackAlign::End) => el.self_end(),
            Some(StackAlign::Stretch) => el.self_stretch(),
            _ => el,
        };
        let size = self.size.map(|s| match s {
            StackItemSize::Fill => "fill",
            StackItemSize::Static => "static",
        });
        let props: Vec<(&str, &str)> = size.map(|s| ("size", s)).into_iter().collect();
        theme.refine(el.style(), "stack-item", &props, None);
        el.style().refine(&self.style);
        let id = self
            .id
            .clone()
            .unwrap_or(ElementId::CodeLocation(*self.source));
        if self.scrollable {
            el.id(id)
                .overflow_scroll()
                .children(self.children)
                .into_any_element()
        } else {
            el.children(self.children).into_any_element()
        }
    }
}
