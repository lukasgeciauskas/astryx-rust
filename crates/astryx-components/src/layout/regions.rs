//! Layout and its regions, upstream `Layout/` (component specs `Layout`,
//! `LayoutHeader`, `LayoutContent`, `LayoutFooter`, `LayoutPanel`; family
//! `layout-regions`; architecture `container-padding`).
//!
//! A Layout escapes its parent's padding and arranges header, start, content, end
//! and footer slots. Each region applies an inset per edge: the Layout-edge inset on
//! an outer edge (the Layout's `padding`, else the nearest padding container's, else
//! spacing step 4) and the inner inset toward another region (the nearest padding
//! container's, else step 4), collapsing beside a header or footer without a divider.
//! Regions publish the padding they apply as `ContainerPadding`.

use std::cell::Cell;
use std::rc::Rc;

use astryx_i18n::{Direction, LogicalStyled, current as i18n, use_direction};
use astryx_primitives::scroll_area::{
    ScrollAxis, ScrollKeyboardAccess, ScrollRole, UseScrollableAreaOptions, use_scrollable_area,
};
use astryx_primitives::{provide, provide_in_layer, use_context, use_layer_context};
use astryx_theme::current;
use gpui::{
    AbsoluteLength, AnyElement, App, Div, ElementId, IntoElement, Length, ParentElement, Pixels,
    RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, div, prelude::*, px,
};

use super::spacing::{ContainerCorners, ContainerPadding, SpacingStep};
use crate::resizable::ResizableProps;

/// The padding of the nearest padding container (Card, Section, Dialog, Drawer,
/// BottomSheet), which a Layout's regions inset by. A Layout provides none of its own,
/// so a nested Layout never inherits a parent Layout's padding (Layout FR3).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaddingContainer(pub ContainerPadding);

/// Upstream `LayoutContainer`: the container a Layout sits in, which regions reflect
/// as their `container` theme prop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutContainerKind {
    Card,
    Dialog,
    BottomSheet,
}

impl LayoutContainerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LayoutContainerKind::Card => "card",
            LayoutContainerKind::Dialog => "dialog",
            LayoutContainerKind::BottomSheet => "bottom-sheet",
        }
    }
}

/// The divider default a Layout gives its headers and footers; a layer resets it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutDividerDefault(pub Option<bool>);

/// Upstream `LayoutSlots`: which slots render.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutSlots {
    pub header: bool,
    pub footer: bool,
    pub start: bool,
    pub end: bool,
}

/// Upstream `LayoutArea`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutArea {
    Header,
    Footer,
    Content,
    Start,
    End,
}

/// What a Layout tells its regions.
#[derive(Clone)]
pub(crate) struct LayoutFrame {
    /// The Layout-edge inset per edge.
    edge: ContainerPadding,
    /// The inner inset per edge.
    inner: ContainerPadding,
    slots: LayoutSlots,
    content_width: Option<Pixels>,
    /// Written by this frame's header and footer, read by the regions beside them.
    header_divider: Rc<Cell<bool>>,
    footer_divider: Rc<Cell<bool>>,
    area: LayoutArea,
}

/// The layer contexts a Layout reads, each unset at a layout root (Layout FR8).
#[derive(Clone)]
struct FrameSlot(Option<LayoutFrame>);

#[derive(Clone, Copy)]
struct KindSlot(Option<LayoutContainerKind>);

#[derive(Clone, Copy)]
struct PaddingSlot(Option<ContainerPadding>);

/// Upstream `layoutReset` (Layout FR8): below a layout root (AppShell), a Layout lays out
/// as at the page root. The published container padding and corners reset to zero; the
/// container, its padding, the frame and the divider default are unset.
pub(crate) fn layout_reset(child: impl IntoElement, cx: &App) -> AnyElement {
    let child = provide(
        ContainerPadding::default(),
        provide(ContainerCorners::default(), child),
    )
    .into_any_element();
    let child = provide_in_layer(FrameSlot(None), child, cx).into_any_element();
    let child = provide_in_layer(KindSlot(None), child, cx).into_any_element();
    let child = provide_in_layer(PaddingSlot(None), child, cx).into_any_element();
    provide_in_layer(LayoutDividerDefault(None), child, cx).into_any_element()
}

/// Every edge the same.
fn all(len: Pixels) -> ContainerPadding {
    ContainerPadding {
        inline_start: len,
        inline_end: len,
        block_start: len,
        block_end: len,
    }
}

fn step_px(step: SpacingStep, window: &Window, cx: &App) -> Pixels {
    let t = current(window, cx);
    step.token(t.tokens()).to_pixels(window.rem_size())
}

/// The frame of the nearest Layout in the area the region sits in.
fn frame(cx: &App) -> Option<LayoutFrame> {
    use_layer_context::<FrameSlot>(cx).and_then(|s| s.0)
}

fn container_kind(cx: &App) -> &'static str {
    use_layer_context::<KindSlot>(cx)
        .and_then(|k| k.0)
        .map_or("", |k| k.as_str())
}

/// Provides a container's kind and padding to a Layout inside it, in its layer.
pub(crate) fn padding_container(
    padding: ContainerPadding,
    kind: Option<LayoutContainerKind>,
    child: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let child = provide_in_layer(PaddingSlot(Some(padding)), child, cx).into_any_element();
    match kind {
        Some(kind) => provide_in_layer(KindSlot(Some(kind)), child, cx).into_any_element(),
        None => child,
    }
}

/// Applies padding per logical edge.
fn pad(el: Div, p: ContainerPadding, dir: Direction) -> Div {
    el.padding_start(dir, p.inline_start)
        .padding_end(dir, p.inline_end)
        .pt(p.block_start)
        .pb(p.block_end)
}

// ----------------------------------------------------------------------------
// Layout

/// Upstream `height`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutHeight {
    /// Fills the container's height (the default).
    #[default]
    Fill,
    /// Grows with its content, at least as tall as its container.
    Auto,
}

/// Upstream `Layout`: header, start, content, end and footer slots.
#[derive(IntoElement)]
pub struct Layout {
    header: Option<AnyElement>,
    footer: Option<AnyElement>,
    start: Option<AnyElement>,
    end: Option<AnyElement>,
    content: Option<AnyElement>,
    children: Vec<AnyElement>,
    height: LayoutHeight,
    content_width: Option<Pixels>,
    padding: Option<SpacingStep>,
    default_has_dividers: Option<bool>,
    style: StyleRefinement,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    pub fn new() -> Self {
        Layout {
            header: None,
            footer: None,
            start: None,
            end: None,
            content: None,
            children: Vec::new(),
            height: LayoutHeight::Fill,
            content_width: None,
            padding: None,
            default_has_dividers: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `header`.
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// Upstream `footer`.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Upstream `start`: the logical-start slot.
    pub fn start(mut self, start: impl IntoElement) -> Self {
        self.start = Some(start.into_any_element());
        self
    }

    /// Upstream `end`: the logical-end slot.
    pub fn end(mut self, end: impl IntoElement) -> Self {
        self.end = Some(end.into_any_element());
        self
    }

    /// Upstream `content`; `children` fill the slot when it is omitted.
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }

    /// Upstream `height`.
    pub fn height(mut self, height: LayoutHeight) -> Self {
        self.height = height;
        self
    }

    /// Upstream `contentWidth`: the aligned content's maximum width, in pixels.
    pub fn content_width(mut self, width: Pixels) -> Self {
        self.content_width = Some(width);
        self
    }

    /// Upstream `padding`: the inset at this Layout's outer edges; `S0` is full bleed.
    pub fn padding(mut self, step: SpacingStep) -> Self {
        self.padding = Some(step);
        self
    }

    /// Upstream `defaultHasDividers`: the divider default for its headers and footers.
    pub fn default_has_dividers(mut self, dividers: bool) -> Self {
        self.default_has_dividers = Some(dividers);
        self
    }
}

impl ParentElement for Layout {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Layout {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Layout {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let dir = use_direction(cx);
        let parent = use_context::<ContainerPadding>(cx).unwrap_or_default();
        let container = use_layer_context::<PaddingSlot>(cx).and_then(|c| c.0);
        let step4 = all(step_px(SpacingStep::S4, window, cx));
        let inner = container.unwrap_or(step4);
        let edge = match self.padding {
            Some(step) => all(step_px(step, window, cx)),
            None => container.unwrap_or(step4),
        };
        let slots = LayoutSlots {
            header: self.header.is_some(),
            footer: self.footer.is_some(),
            start: self.start.is_some(),
            end: self.end.is_some(),
        };
        let (header_divider, footer_divider) =
            (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        let frame = |area| {
            FrameSlot(Some(LayoutFrame {
                edge,
                inner,
                slots,
                content_width: self.content_width,
                header_divider: header_divider.clone(),
                footer_divider: footer_divider.clone(),
                area,
            }))
        };
        let content = self.content.or_else(|| {
            (!self.children.is_empty()).then(|| div().children(self.children).into_any_element())
        });
        let both = slots.start && slots.end;
        let mut middle = div().flex().flex_row().flex_1().min_h_0().w_full();
        // With panels the whole middle composition keeps to the content width.
        if let Some(width) = self.content_width.filter(|_| slots.start || slots.end) {
            middle = middle.max_w(width).mx_auto();
        }
        let _ = both;
        let middle = middle
            .children(
                self.start
                    .map(|s| provide_in_layer(frame(LayoutArea::Start), s, cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .children(content.map(|c| provide_in_layer(frame(LayoutArea::Content), c, cx))),
            )
            .children(
                self.end
                    .map(|e| provide_in_layer(frame(LayoutArea::End), e, cx)),
            );
        // The inner stack publishes zero container padding (FR2).
        let inner_stack = div()
            .flex()
            .flex_col()
            .size_full()
            .children(
                self.header
                    .map(|h| provide_in_layer(frame(LayoutArea::Header), h, cx)),
            )
            .child(middle)
            .children(
                self.footer
                    .map(|f| provide_in_layer(frame(LayoutArea::Footer), f, cx)),
            );
        let inner_stack = provide(
            ContainerPadding::default(),
            provide(ContainerCorners::default(), inner_stack),
        );
        // The outer box escapes the parent's published padding on every edge.
        let mut outer = div()
            .flex()
            .flex_col()
            .self_stretch()
            .margin_start(dir, -parent.inline_start)
            .margin_end(dir, -parent.inline_end)
            .mt(-parent.block_start)
            .mb(-parent.block_end);
        outer = match self.height {
            LayoutHeight::Fill => outer.flex_1().h_full().min_h_0(),
            LayoutHeight::Auto => outer.min_h_full(),
        };
        theme.refine(
            outer.style(),
            "layout",
            &[(
                "height",
                match self.height {
                    LayoutHeight::Fill => "fill",
                    LayoutHeight::Auto => "auto",
                },
            )],
            None,
        );
        outer.style().refine(&self.style);
        let outer = outer.child(inner_stack);
        match self.default_has_dividers {
            Some(d) => {
                provide_in_layer(LayoutDividerDefault(Some(d)), outer, cx).into_any_element()
            }
            None => outer.into_any_element(),
        }
    }
}

// ----------------------------------------------------------------------------
// LayoutHeader and LayoutFooter

/// A header or footer bar: one shape for both.
struct Bar {
    children: Vec<AnyElement>,
    has_divider: Option<bool>,
    height: Option<Length>,
    label: Option<SharedString>,
    padding: Option<SpacingStep>,
    padding_block_end: Option<SpacingStep>,
    role: Option<Role>,
    style: StyleRefinement,
}

impl Bar {
    fn new() -> Self {
        Bar {
            children: Vec::new(),
            has_divider: None,
            height: None,
            label: None,
            padding: None,
            padding_block_end: None,
            role: None,
            style: StyleRefinement::default(),
        }
    }

    fn render(self, footer: bool, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let frame = frame(cx);
        let step4 = all(step_px(SpacingStep::S4, window, cx));
        let (edge, inner) = frame.as_ref().map_or((step4, step4), |f| (f.edge, f.inner));
        // ORD1: hasDivider, then the Layout's default, else none.
        let divider = self
            .has_divider
            .or_else(|| use_layer_context::<LayoutDividerDefault>(cx).and_then(|d| d.0))
            .unwrap_or(false);
        if let Some(f) = &frame {
            if footer {
                f.footer_divider.set(divider);
            } else {
                f.header_divider.set(divider);
            }
        }
        let mut applied = ContainerPadding {
            inline_start: edge.inline_start,
            inline_end: edge.inline_end,
            block_start: if footer {
                inner.block_start
            } else {
                edge.block_start
            },
            block_end: if footer {
                edge.block_end
            } else {
                inner.block_end
            },
        };
        if let Some(step) = self.padding {
            applied = all(step_px(step, window, cx));
        }
        if let (Some(step), false) = (self.padding_block_end, footer) {
            applied.block_end = step_px(step, window, cx);
        }
        let mut wrapper = pad(div().w_full(), applied, dir);
        if let Some(width) = frame.as_ref().and_then(|f| f.content_width) {
            wrapper = wrapper.max_w(width).mx_auto();
        }
        let wrapper = wrapper.child(provide(
            applied,
            provide(ContainerCorners::default(), div().children(self.children)),
        ));
        let target = if footer {
            "layout-footer"
        } else {
            "layout-header"
        };
        let mut root = div().flex_none().w_full().border_color(t.color.border);
        if let Some(height) = self.height {
            root = root.h(height);
        }
        if divider {
            root = if footer {
                root.border_t_1()
            } else {
                root.border_b_1()
            };
        }
        theme.refine(
            root.style(),
            target,
            &[
                ("container", container_kind(cx)),
                ("divider", if divider { "true" } else { "" }),
            ],
            None,
        );
        root.style().refine(&self.style);
        let mut root = root
            .id(ElementId::Name(target.into()))
            .debug_selector(move || target.to_string())
            .child(wrapper);
        if let Some(role) = self.role {
            root = root.role(role);
        }
        if let Some(label) = self.label {
            root = root.aria_label(label);
        }
        root.into_any_element()
    }
}

macro_rules! bar_component {
    ($name:ident, $footer:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(IntoElement)]
        pub struct $name(Bar);

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $name {
            pub fn new() -> Self {
                $name(Bar::new())
            }

            /// Upstream `hasDivider`; unset, the Layout's default decides.
            pub fn has_divider(mut self, divider: bool) -> Self {
                self.0.has_divider = Some(divider);
                self
            }

            /// Upstream `height`.
            pub fn height(mut self, height: impl Into<Length>) -> Self {
                self.0.height = Some(height.into());
                self
            }

            /// Upstream `label`: the landmark's name.
            pub fn label(mut self, label: impl Into<SharedString>) -> Self {
                self.0.label = Some(label.into());
                self
            }

            /// Upstream `padding`: every edge; `S0` is full bleed.
            pub fn padding(mut self, step: SpacingStep) -> Self {
                self.0.padding = Some(step);
                self
            }

            /// Upstream `role`: a landmark role.
            pub fn role(mut self, role: Role) -> Self {
                self.0.role = Some(role);
                self
            }
        }

        impl ParentElement for $name {
            fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
                self.0.children.extend(elements);
            }
        }

        impl Styled for $name {
            fn style(&mut self) -> &mut StyleRefinement {
                &mut self.0.style
            }
        }

        impl RenderOnce for $name {
            fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
                self.0.render($footer, window, cx)
            }
        }
    };
}

bar_component!(
    LayoutHeader,
    false,
    "Upstream `LayoutHeader`: the top bar for titles, app bars and toolbars."
);
bar_component!(
    LayoutFooter,
    true,
    "Upstream `LayoutFooter`: the bottom bar for actions, pagination and status."
);

impl LayoutHeader {
    /// Upstream `paddingBlockEnd`: the block-end edge alone; `S0` docks the last child.
    pub fn padding_block_end(mut self, step: SpacingStep) -> Self {
        self.0.padding_block_end = Some(step);
        self
    }
}

// ----------------------------------------------------------------------------
// LayoutContent and LayoutPanel

/// A scrolling region's shared parts.
struct Region {
    id: ElementId,
    children: Vec<AnyElement>,
    padding: Option<SpacingStep>,
    scrollable: bool,
    label: Option<SharedString>,
    role: Option<Role>,
    style: StyleRefinement,
}

impl Region {
    fn new(id: ElementId) -> Self {
        Region {
            id,
            children: Vec::new(),
            padding: None,
            scrollable: true,
            label: None,
            role: None,
            style: StyleRefinement::default(),
        }
    }

    /// The region's box: scrolling and named while it scrolls (AR3), or clipped.
    fn viewport(&self, el: Div, window: &mut Window, cx: &mut App) -> gpui::Stateful<Div> {
        let i18n = i18n(cx);
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.scrollRegion.label"));
        if !self.scrollable {
            let mut el = el.id(self.id.clone()).overflow_hidden();
            if let Some(role) = self.role {
                el = el.role(role);
            }
            if let Some(label) = &self.label {
                el = el.aria_label(label.clone());
            }
            return el;
        }
        let area = use_scrollable_area(
            &self.id,
            UseScrollableAreaOptions::new(
                ScrollAxis::Block,
                ScrollKeyboardAccess::Viewport {
                    label,
                    role: ScrollRole::Group,
                },
            ),
            window,
            cx,
        );
        let mut el = area.viewport(el);
        // A caller's role takes only the caller's label (AR1).
        if let Some(role) = self.role {
            el = el.role(role);
            if let Some(label) = &self.label {
                el = el.aria_label(label.clone());
            }
        }
        el
    }
}

/// Upstream `LayoutContent`: the main content area, scrolling by default.
#[derive(IntoElement)]
pub struct LayoutContent(Region);

impl LayoutContent {
    pub fn new(id: impl Into<ElementId>) -> Self {
        LayoutContent(Region::new(id.into()))
    }
}

/// Upstream `LayoutPanel`: a sidebar for navigation, settings or an inspector.
#[derive(IntoElement)]
pub struct LayoutPanel {
    region: Region,
    divider: bool,
    width: Option<Length>,
    resizable: Option<ResizableProps>,
}

impl LayoutPanel {
    pub fn new(id: impl Into<ElementId>) -> Self {
        LayoutPanel {
            region: Region::new(id.into()),
            divider: false,
            width: None,
            resizable: None,
        }
    }

    /// Upstream `hasDivider`: a line on the content-facing edge.
    pub fn has_divider(mut self, divider: bool) -> Self {
        self.divider = divider;
        self
    }

    /// Upstream `width`; a `resizable` region's size wins.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// Upstream `resizable`: `use_resizable`'s region or its props.
    pub fn resizable(mut self, resizable: impl Into<ResizableProps>) -> Self {
        self.resizable = Some(resizable.into());
        self
    }
}

macro_rules! region_setters {
    ($name:ident, $($field:tt)+) => {
        impl $name {
            /// Upstream `padding`: every edge; `S0` is full bleed.
            pub fn padding(mut self, step: SpacingStep) -> Self {
                self.$($field)+.padding = Some(step);
                self
            }

            /// Upstream `isScrollable`: `false` clips the overflow.
            pub fn scrollable(mut self, scrollable: bool) -> Self {
                self.$($field)+.scrollable = scrollable;
                self
            }

            /// Upstream `label`: the landmark's name, and the name while it scrolls.
            pub fn label(mut self, label: impl Into<SharedString>) -> Self {
                self.$($field)+.label = Some(label.into());
                self
            }

            /// Upstream `role`: a landmark role.
            pub fn role(mut self, role: Role) -> Self {
                self.$($field)+.role = Some(role);
                self
            }
        }

        impl ParentElement for $name {
            fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
                self.$($field)+.children.extend(elements);
            }
        }

        impl Styled for $name {
            fn style(&mut self) -> &mut StyleRefinement {
                &mut self.$($field)+.style
            }
        }
    };
}

region_setters!(LayoutContent, 0);
region_setters!(LayoutPanel, region);

impl RenderOnce for LayoutContent {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let dir = use_direction(cx);
        let frame = frame(cx);
        let step4 = all(step_px(SpacingStep::S4, window, cx));
        let (edge, inner, slots) = frame
            .as_ref()
            .map_or((step4, step4, LayoutSlots::default()), |f| {
                (f.edge, f.inner, f.slots)
            });
        let header_divider = frame.as_ref().is_some_and(|f| f.header_divider.get());
        let footer_divider = frame.as_ref().is_some_and(|f| f.footer_divider.get());
        // FR2: inner toward a region, the edge inset on an open edge, and zero beside a
        // header or footer without a divider.
        let mut applied = ContainerPadding {
            inline_start: if slots.start {
                inner.inline_start
            } else {
                edge.inline_start
            },
            inline_end: if slots.end {
                inner.inline_end
            } else {
                edge.inline_end
            },
            block_start: match (slots.header, header_divider) {
                (false, _) => edge.block_start,
                (true, true) => inner.block_start,
                (true, false) => px(0.),
            },
            block_end: match (slots.footer, footer_divider) {
                (false, _) => edge.block_end,
                (true, true) => inner.block_end,
                (true, false) => px(0.),
            },
        };
        if let Some(step) = self.0.padding {
            applied = all(step_px(step, window, cx));
        }
        // FR3: children align to the content width through the inline padding,
        // never less than the inset; the gutter is applied, not published.
        let mut shown = applied;
        let measured = window.use_keyed_state(
            ElementId::Name(format!("{:?}-width", self.0.id).into()),
            cx,
            |_, _| Rc::new(Cell::new(px(0.))),
        );
        let measured = measured.read(cx).clone();
        if let (Some(width), false) = (
            frame
                .as_ref()
                .and_then(|f| f.content_width)
                .filter(|_| !slots.start && !slots.end),
            self.0.padding == Some(SpacingStep::S0),
        ) {
            let gutter = ((measured.get() - width) / 2.).max(px(0.));
            shown.inline_start = shown.inline_start.max(gutter);
            shown.inline_end = shown.inline_end.max(gutter);
        }
        let mut el = pad(div().size_full().relative(), shown, dir);
        theme.refine(
            el.style(),
            "layout-content",
            &[("container", container_kind(cx))],
            None,
        );
        el.style().refine(&self.0.style);
        let probe = gpui::canvas(
            move |bounds, window, cx| {
                if measured.get() != bounds.size.width {
                    measured.set(bounds.size.width);
                    window.defer(cx, |window, _| window.refresh());
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(px(0.));
        let children = provide(
            applied,
            provide(
                ContainerCorners::default(),
                div().children(std::mem::take(&mut self.0.children)),
            ),
        );
        let el = el.child(probe).child(children);
        self.0.viewport(el, window, cx).flex_1().min_h_0()
    }
}

impl RenderOnce for LayoutPanel {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let dir = use_direction(cx);
        let frame = frame(cx);
        let step4 = all(step_px(SpacingStep::S4, window, cx));
        let (edge, inner, slots, at_end) = frame
            .as_ref()
            .map_or((step4, step4, LayoutSlots::default(), false), |f| {
                (f.edge, f.inner, f.slots, f.area == LayoutArea::End)
            });
        // FR3: the outer inline edge and an open block edge take the edge inset.
        let mut applied = ContainerPadding {
            inline_start: if at_end {
                inner.inline_start
            } else {
                edge.inline_start
            },
            inline_end: if at_end {
                edge.inline_end
            } else {
                inner.inline_end
            },
            block_start: if slots.header {
                inner.block_start
            } else {
                edge.block_start
            },
            block_end: if slots.footer {
                inner.block_end
            } else {
                edge.block_end
            },
        };
        let automatic = self.region.padding.is_none();
        if let Some(step) = self.region.padding {
            applied = all(step_px(step, window, cx));
        }
        let collapsed = self.resizable.as_ref().is_some_and(|r| r.is_collapsed);
        // AR2: focus inside a collapsing panel moves to its handle.
        let panel_focus = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-panel-focus", self.region.id).into()),
                cx,
                |_, cx| cx.focus_handle().tab_stop(false),
            )
            .read(cx)
            .clone();
        let was_collapsed = window.use_keyed_state(
            ElementId::Name(format!("{:?}-was-collapsed", self.region.id).into()),
            cx,
            |_, _| collapsed,
        );
        if *was_collapsed.read(cx) != collapsed {
            was_collapsed.update(cx, |w, _| *w = collapsed);
            if collapsed
                && panel_focus.contains_focused(window, cx)
                && let Some(handle) = self.resizable.as_ref().and_then(|r| r.handle_focus(cx))
            {
                window.defer(cx, move |window, cx| window.focus(&handle, cx));
            }
        }
        let mut el = pad(div().h_full().flex_none().relative(), applied, dir);
        // ORD1: the region's size, then width.
        if let Some(region) = &self.resizable {
            el = el.w(px(region.size));
        } else if let Some(width) = self.width {
            el = el.w(width);
        }
        // FR4: the divider on the content-facing edge; without one and with the
        // automatic inset, the panel overlaps the content by the inner inset.
        let one: AbsoluteLength = px(1.).into();
        if self.divider {
            el = el.border_color(t.color.border);
            el = if at_end {
                el.border_start(dir, one)
            } else {
                el.border_end(dir, one)
            };
        } else if automatic {
            el = if at_end {
                el.margin_start(dir, -inner.inline_start)
            } else {
                el.margin_end(dir, -inner.inline_end)
            };
        }
        theme.refine(el.style(), "layout-panel", &[], None);
        el.style().refine(&self.region.style);
        if collapsed {
            // Inert while collapsed (AR2).
            el = el.invisible();
        }
        let children = provide(
            applied,
            provide(
                ContainerCorners::default(),
                div().children(std::mem::take(&mut self.region.children)),
            ),
        );
        self.region
            .viewport(el.child(children), window, cx)
            .track_focus(&panel_focus)
    }
}
