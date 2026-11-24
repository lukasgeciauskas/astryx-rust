//! Center, Divider, AspectRatio and VisuallyHidden. Upstream: `Center/`,
//! `Divider/`, `AspectRatio/`, `VisuallyHidden/`.

use astryx_i18n::{LogicalStyled, use_direction};
use astryx_primitives::use_context;
use astryx_theme::{ResolvedTheme, ThemeStyled, current};
use gpui::{
    AnyElement, App, Div, ElementId, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px,
};
use smallvec::SmallVec;

use super::spacing::{ContainerPadding, Padding, Sizing, padding_setters, size_setters};

macro_rules! styled_parent {
    ($ty:ty) => {
        impl Styled for $ty {
            fn style(&mut self) -> &mut StyleRefinement {
                &mut self.style
            }
        }

        impl ParentElement for $ty {
            fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
                self.children.extend(elements);
            }
        }
    };
}

/// Upstream `Center` `axis`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CenterAxis {
    #[default]
    Both,
    Horizontal,
    Vertical,
}

impl CenterAxis {
    fn as_str(self) -> &'static str {
        match self {
            CenterAxis::Both => "both",
            CenterAxis::Horizontal => "horizontal",
            CenterAxis::Vertical => "vertical",
        }
    }
}

/// Centers its content on one or both axes, upstream `Center`.
#[derive(IntoElement)]
pub struct Center {
    axis: CenterAxis,
    size: Sizing,
    padding: Padding,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl Center {
    pub fn new() -> Self {
        Center {
            axis: CenterAxis::Both,
            size: Sizing::default(),
            padding: Padding::default(),
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `axis`.
        axis: CenterAxis,
    }

    padding_setters!();
    size_setters!();

    pub(crate) fn root(&self, theme: &ResolvedTheme, dir: astryx_i18n::Direction) -> Div {
        let t = theme.tokens();
        let mut el = div().flex();
        el = match self.axis {
            CenterAxis::Both => el.justify_center().items_center(),
            CenterAxis::Horizontal => el.justify_center(),
            CenterAxis::Vertical => el.items_center(),
        };
        el = self.padding.apply(el, t, dir);
        el = self.size.apply(el);
        theme.refine(el.style(), "center", &[("axis", self.axis.as_str())], None);
        el.style().refine(&self.style);
        el
    }
}

impl Default for Center {
    fn default() -> Self {
        Self::new()
    }
}

styled_parent!(Center);

impl RenderOnce for Center {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let el = self.root(&theme, use_direction(cx));
        el.children(self.children)
    }
}

/// Upstream `Divider` `orientation`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

impl Orientation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Orientation::Horizontal => "horizontal",
            Orientation::Vertical => "vertical",
        }
    }
}

/// Upstream `Divider` `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DividerVariant {
    #[default]
    Subtle,
    Strong,
}

/// A separator line with an optional label, upstream `Divider`.
#[derive(IntoElement)]
pub struct Divider {
    orientation: Orientation,
    label: Option<SharedString>,
    variant: DividerVariant,
    full_bleed: bool,
    id: Option<ElementId>,
    style: StyleRefinement,
}

impl Divider {
    pub fn new() -> Self {
        Divider {
            orientation: Orientation::Horizontal,
            label: None,
            variant: DividerVariant::Subtle,
            full_bleed: false,
            id: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `orientation`.
        orientation: Orientation,
        /// Upstream `label`: text after the line.
        ?label: SharedString,
        /// Upstream `variant`.
        variant: DividerVariant,
        /// Upstream `isFullBleed`: reaches past the container's padding to its edges.
        !full_bleed,
    }

    /// The element id. With one, the divider is a `separator` in the
    /// accessibility tree.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl Default for Divider {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Divider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Divider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let horizontal = self.orientation == Orientation::Horizontal;
        let mut el = div().flex().items_center();
        el = if horizontal {
            el.w_full()
        } else {
            el.flex_col().h_full()
        };
        if self.full_bleed {
            let pad = use_context::<ContainerPadding>(cx).unwrap_or_default();
            // Upstream widens the box by `calc(100% + padding)`. GPUI has no
            // calc, but an auto-sized item stretched across its parent grows
            // by its negative margins, which is the same box.
            el = if horizontal {
                el.style().size.width = None;
                el.self_stretch()
                    .margin_start(dir, -pad.inline_start)
                    .margin_end(dir, -pad.inline_end)
            } else {
                el.style().size.height = None;
                el.self_stretch().mt(-pad.block_start).mb(-pad.block_end)
            };
        }
        let color = match self.variant {
            DividerVariant::Subtle => t.color.border,
            DividerVariant::Strong => t.color.border_emphasized,
        };
        let line = div().flex_grow(1.).flex_shrink(1.).bg(color);
        let line = if horizontal {
            line.h(t.border.width)
        } else {
            line.w(t.border.width)
        };
        let variant = match self.variant {
            DividerVariant::Subtle => "subtle",
            DividerVariant::Strong => "strong",
        };
        theme.refine(
            el.style(),
            "divider",
            &[
                ("variant", variant),
                ("orientation", self.orientation.as_str()),
            ],
            None,
        );
        el.style().refine(&self.style);
        let label = self.label.map(|label| {
            let label_el = div()
                .flex_none()
                .text_style(&t.text.supporting)
                .text_color(t.color.text_secondary)
                .child(label);
            if horizontal {
                label_el.px(t.spacing.s3)
            } else {
                label_el.py(t.spacing.s3)
            }
        });
        let el = el.child(line).children(label);
        match self.id {
            Some(id) => el
                .id(id)
                .role(Role::Splitter)
                .aria_orientation(if horizontal {
                    gpui::Orientation::Horizontal
                } else {
                    gpui::Orientation::Vertical
                })
                .into_any_element(),
            None => el.into_any_element(),
        }
    }
}

/// Upstream `AspectRatio` `shape`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AspectShape {
    #[default]
    Rectangle,
    /// A circle at ratio 1.
    Ellipse,
}

/// Upstream `AspectRatio` `fit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AspectFit {
    /// Fills the box and crops. Set `object_fit(ObjectFit::Cover)` on an image.
    Cover,
    /// Fills the box and letterboxes. Set `object_fit(ObjectFit::Contain)` on an image.
    Contain,
    /// Keeps the child's natural size, centered.
    Center,
}

/// A box with a fixed width-to-height ratio, upstream `AspectRatio`. Its
/// width comes from the container; children fill it.
#[derive(IntoElement)]
pub struct AspectRatio {
    ratio: f32,
    shape: AspectShape,
    fit: Option<AspectFit>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
}

impl AspectRatio {
    /// Upstream `ratio`: width / height, such as `16. / 9.`.
    pub fn new(ratio: f32) -> Self {
        AspectRatio {
            ratio,
            shape: AspectShape::Rectangle,
            fit: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `shape`.
        shape: AspectShape,
        /// Upstream `fit`.
        ?fit: AspectFit,
    }
}

styled_parent!(AspectRatio);

impl RenderOnce for AspectRatio {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let mut el = div()
            .relative()
            .w_full()
            .overflow_hidden()
            .aspect_ratio(self.ratio);
        if self.shape == AspectShape::Ellipse {
            el = el.rounded_full();
        }
        let shape = match self.shape {
            AspectShape::Rectangle => "rectangle",
            AspectShape::Ellipse => "ellipse",
        };
        theme.refine(el.style(), "aspect-ratio", &[("shape", shape)], None);
        el.style().refine(&self.style);
        let fit = self.fit;
        let children = self.children.into_iter().map(move |child| {
            let slot = div().absolute().top_0().left_0().size_full();
            match fit {
                Some(AspectFit::Center) => slot.flex().items_center().justify_center().child(child),
                Some(AspectFit::Cover | AspectFit::Contain) => {
                    slot.child(div().size_full().child(child))
                }
                None => slot.child(child),
            }
        });
        el.children(children)
    }
}

/// Content for screen readers only, upstream `VisuallyHidden`: it keeps its
/// accessibility nodes but paints nothing and takes no space.
#[derive(IntoElement)]
pub struct VisuallyHidden {
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
}

impl VisuallyHidden {
    pub fn new() -> Self {
        VisuallyHidden {
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }
}

impl Default for VisuallyHidden {
    fn default() -> Self {
        Self::new()
    }
}

styled_parent!(VisuallyHidden);

impl RenderOnce for VisuallyHidden {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        // Upstream's clip: a 1px absolute box that shows nothing.
        let mut el = div()
            .absolute()
            .w(px(1.))
            .h(px(1.))
            .overflow_hidden()
            .opacity(0.);
        el.style().refine(&self.style);
        el.children(self.children)
    }
}
