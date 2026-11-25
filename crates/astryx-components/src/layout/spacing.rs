//! Shared layout props: spacing steps and the padding family every layout
//! component takes.

use astryx_i18n::{Direction, LogicalStyled};
use astryx_theme::Tokens;
use gpui::{AbsoluteLength, Styled};

/// A step on the spacing scale, upstream `SpacingStep`
/// (`0 | 0.5 | 1 | 1.5 | 2 | 3 | 4 | 5 | 6 | 8 | 10`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpacingStep {
    S0,
    S0_5,
    S1,
    S1_5,
    S2,
    S3,
    S4,
    S5,
    S6,
    S8,
    S10,
}

impl SpacingStep {
    /// The `--spacing-*` token.
    pub fn token(self, t: &Tokens) -> AbsoluteLength {
        let s = &t.spacing;
        match self {
            SpacingStep::S0 => s.s0,
            SpacingStep::S0_5 => s.s0_5,
            SpacingStep::S1 => s.s1,
            SpacingStep::S1_5 => s.s1_5,
            SpacingStep::S2 => s.s2,
            SpacingStep::S3 => s.s3,
            SpacingStep::S4 => s.s4,
            SpacingStep::S5 => s.s5,
            SpacingStep::S6 => s.s6,
            SpacingStep::S8 => s.s8,
            SpacingStep::S10 => s.s10,
        }
    }

    /// The upstream prop value, for theme rules.
    pub fn as_str(self) -> &'static str {
        match self {
            SpacingStep::S0 => "0",
            SpacingStep::S0_5 => "0.5",
            SpacingStep::S1 => "1",
            SpacingStep::S1_5 => "1.5",
            SpacingStep::S2 => "2",
            SpacingStep::S3 => "3",
            SpacingStep::S4 => "4",
            SpacingStep::S5 => "5",
            SpacingStep::S6 => "6",
            SpacingStep::S8 => "8",
            SpacingStep::S10 => "10",
        }
    }
}

/// Upstream's padding props. Per edge, the most specific wins: the edge
/// prop, then the axis prop, then `padding`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Padding {
    pub all: Option<SpacingStep>,
    pub inline: Option<SpacingStep>,
    pub inline_start: Option<SpacingStep>,
    pub inline_end: Option<SpacingStep>,
    pub block: Option<SpacingStep>,
    pub block_start: Option<SpacingStep>,
    pub block_end: Option<SpacingStep>,
}

impl Padding {
    /// Applies the resolved padding, mirroring the inline edges in RTL.
    pub fn apply<E: Styled>(&self, mut el: E, t: &Tokens, dir: Direction) -> E {
        let start = self.inline_start.or(self.inline).or(self.all);
        let end = self.inline_end.or(self.inline).or(self.all);
        let top = self.block_start.or(self.block).or(self.all);
        let bottom = self.block_end.or(self.block).or(self.all);
        if let Some(step) = start {
            el = el.padding_start(dir, step.token(t));
        }
        if let Some(step) = end {
            el = el.padding_end(dir, step.token(t));
        }
        if let Some(step) = top {
            el = el.pt(step.token(t));
        }
        if let Some(step) = bottom {
            el = el.pb(step.token(t));
        }
        el
    }

    /// The resolved (start, end, top, bottom) lengths, for a container's
    /// `ContainerPadding`.
    pub fn resolve(&self) -> [Option<SpacingStep>; 4] {
        [
            self.inline_start.or(self.inline).or(self.all),
            self.inline_end.or(self.inline).or(self.all),
            self.block_start.or(self.block).or(self.all),
            self.block_end.or(self.block).or(self.all),
        ]
    }
}

/// Builder methods for the padding family, shared by the layout components.
macro_rules! padding_setters {
    () => {
        /// Upstream `padding`: all four sides.
        pub fn padding(mut self, step: $crate::layout::SpacingStep) -> Self {
            self.padding.all = Some(step);
            self
        }
        /// Upstream `paddingInline`: both inline sides.
        pub fn padding_inline(mut self, step: $crate::layout::SpacingStep) -> Self {
            self.padding.inline = Some(step);
            self
        }
        /// Upstream `paddingInlineStart`: the start side, mirrored in RTL.
        pub fn padding_inline_start(mut self, step: $crate::layout::SpacingStep) -> Self {
            self.padding.inline_start = Some(step);
            self
        }
        /// Upstream `paddingInlineEnd`: the end side, mirrored in RTL.
        pub fn padding_inline_end(mut self, step: $crate::layout::SpacingStep) -> Self {
            self.padding.inline_end = Some(step);
            self
        }
        /// Upstream `paddingBlock`: top and bottom.
        pub fn padding_block(mut self, step: $crate::layout::SpacingStep) -> Self {
            self.padding.block = Some(step);
            self
        }
        /// Upstream `paddingBlockStart`: the top.
        pub fn padding_block_start(mut self, step: $crate::layout::SpacingStep) -> Self {
            self.padding.block_start = Some(step);
            self
        }
        /// Upstream `paddingBlockEnd`: the bottom.
        pub fn padding_block_end(mut self, step: $crate::layout::SpacingStep) -> Self {
            self.padding.block_end = Some(step);
            self
        }
    };
}
pub(crate) use padding_setters;

/// Builder methods for upstream's `SizeValue` sizing props.
macro_rules! size_setters {
    () => {
        /// Upstream `width`: pixels (`px(..)`) or a fraction (`relative(..)`).
        pub fn width(mut self, width: impl Into<gpui::Length>) -> Self {
            self.size.width = Some(width.into());
            self
        }
        /// Upstream `height`.
        pub fn height(mut self, height: impl Into<gpui::Length>) -> Self {
            self.size.height = Some(height.into());
            self
        }
        /// Upstream `maxWidth`.
        pub fn max_width(mut self, width: impl Into<gpui::Length>) -> Self {
            self.size.max_width = Some(width.into());
            self
        }
        /// Upstream `minHeight`.
        pub fn min_height(mut self, height: impl Into<gpui::Length>) -> Self {
            self.size.min_height = Some(height.into());
            self
        }
    };
}
pub(crate) use size_setters;

/// Upstream's `width`, `height`, `maxWidth` and `minHeight`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sizing {
    pub width: Option<gpui::Length>,
    pub height: Option<gpui::Length>,
    pub max_width: Option<gpui::Length>,
    pub min_height: Option<gpui::Length>,
}

impl Sizing {
    pub fn apply<E: Styled>(&self, mut el: E) -> E {
        if let Some(width) = self.width {
            el = el.w(width);
        }
        if let Some(height) = self.height {
            el = el.h(height);
        }
        if let Some(width) = self.max_width {
            el = el.max_w(width);
        }
        if let Some(height) = self.min_height {
            el = el.min_h(height);
        }
        el
    }
}

/// The corners of the container an element sits in, inside its border
/// (upstream's `--container-corner-*`, logical): a rounded host publishes
/// them so a bleeding List's first and last rows can follow its curve.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContainerCorners {
    pub start_start: gpui::Pixels,
    pub start_end: gpui::Pixels,
    pub end_start: gpui::Pixels,
    pub end_end: gpui::Pixels,
}

impl ContainerCorners {
    /// The same radius at every corner.
    pub fn all(radius: gpui::Pixels) -> Self {
        ContainerCorners {
            start_start: radius,
            start_end: radius,
            end_start: radius,
            end_end: radius,
        }
    }
}

/// A host's direct child's place among its siblings (CSS `:first-child` and
/// `:last-child` of the host's content), which hosts provide around each
/// child.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContainerChild {
    pub first: bool,
    pub last: bool,
}

/// A host's children with what it publishes: its padding and corners
/// around all of them, and each one's place.
pub(crate) fn host_children(
    children: impl IntoIterator<Item = gpui::AnyElement>,
    padding: ContainerPadding,
    corners: ContainerCorners,
) -> gpui::AnyElement {
    use gpui::{IntoElement, ParentElement, Styled};
    let children: Vec<gpui::AnyElement> = children.into_iter().collect();
    let count = children.len();
    let placed = children.into_iter().enumerate().map(|(i, child)| {
        astryx_primitives::provide(
            ContainerChild {
                first: i == 0,
                last: i + 1 == count,
            },
            child,
        )
        .into_any_element()
    });
    let content = gpui::div().flex().flex_col().children(placed);
    astryx_primitives::provide(padding, astryx_primitives::provide(corners, content))
        .into_any_element()
}

/// The padding of the container an element sits in, upstream's
/// `--container-padding-*` variables. Cards and layout containers provide
/// it; a full-bleed Divider reads it to reach the container's edges.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContainerPadding {
    pub inline_start: gpui::Pixels,
    pub inline_end: gpui::Pixels,
    pub block_start: gpui::Pixels,
    pub block_end: gpui::Pixels,
}
