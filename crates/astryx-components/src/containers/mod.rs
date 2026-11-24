//! Containers: Card, Section and EmptyState.
//! Upstream: `Card/`, `Section/`, `EmptyState/`.

pub mod examples;

use astryx_i18n::{Direction, LogicalStyled, use_direction};
use astryx_primitives::provide;
use astryx_theme::{ResolvedTheme, ThemeStyled, Tokens, current, scope};
use gpui::{
    AbsoluteLength, AnyElement, App, DefiniteLength, Div, Hsla, IntoElement, ParentElement, Pixels,
    RenderOnce, SharedString, StyleRefinement, Styled, Window, div, prelude::*, px,
};
use smallvec::SmallVec;

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::layout::{ContainerPadding, Padding, Sizing, SpacingStep};
use crate::text::{Heading, HeadingLevel};

/// The padding a container's style ended with, as `ContainerPadding` for
/// the full-bleed children inside (upstream `--container-padding-*`),
/// adding back the border the padding was reduced by.
fn container_padding(
    style: &mut StyleRefinement,
    border: Pixels,
    rem: Pixels,
    dir: Direction,
) -> ContainerPadding {
    let edge = |length: Option<DefiniteLength>| match length {
        Some(DefiniteLength::Absolute(length)) => length.to_pixels(rem) + border,
        _ => border,
    };
    let padding = &style.padding;
    let (left, right) = (edge(padding.left), edge(padding.right));
    let (start, end) = if dir.is_rtl() {
        (right, left)
    } else {
        (left, right)
    };
    ContainerPadding {
        inline_start: start,
        inline_end: end,
        block_start: edge(padding.top),
        block_end: edge(padding.bottom),
    }
}

/// Children wrapped in the container's context: its padding, and the
/// token re-definitions its theme rule gives its parts.
fn container_children(
    children: SmallVec<[AnyElement; 2]>,
    padding: ContainerPadding,
    part_theme: Option<ResolvedTheme>,
) -> AnyElement {
    let content = div().flex().flex_col().children(children);
    let content = provide(padding, content).into_any_element();
    match part_theme {
        Some(theme) => scope(theme, content).into_any_element(),
        None => content,
    }
}

// ----------------------------------------------------------------------------
// Card

/// Upstream `Card` `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CardVariant {
    #[default]
    Default,
    Transparent,
    Muted,
    Blue,
    Cyan,
    Gray,
    Green,
    Orange,
    Pink,
    Purple,
    Red,
    Teal,
    Yellow,
}

impl CardVariant {
    fn background(self, t: &Tokens) -> Option<Hsla> {
        let c = &t.color;
        Some(match self {
            CardVariant::Default => c.background_card,
            CardVariant::Transparent => return None,
            CardVariant::Muted => c.background_muted,
            CardVariant::Blue => c.background_blue,
            CardVariant::Cyan => c.background_cyan,
            CardVariant::Gray => c.background_gray,
            CardVariant::Green => c.background_green,
            CardVariant::Orange => c.background_orange,
            CardVariant::Pink => c.background_pink,
            CardVariant::Purple => c.background_purple,
            CardVariant::Red => c.background_red,
            CardVariant::Teal => c.background_teal,
            CardVariant::Yellow => c.background_yellow,
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            CardVariant::Default => "default",
            CardVariant::Transparent => "transparent",
            CardVariant::Muted => "muted",
            CardVariant::Blue => "blue",
            CardVariant::Cyan => "cyan",
            CardVariant::Gray => "gray",
            CardVariant::Green => "green",
            CardVariant::Orange => "orange",
            CardVariant::Pink => "pink",
            CardVariant::Purple => "purple",
            CardVariant::Red => "red",
            CardVariant::Teal => "teal",
            CardVariant::Yellow => "yellow",
        }
    }
}

/// Upstream `elevation`: a resting shadow from the shadow scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Elevation {
    #[default]
    None,
    Low,
    Med,
    High,
}

impl Elevation {
    pub(crate) fn apply<E: Styled>(self, el: E, t: &Tokens) -> E {
        match self {
            Elevation::None => el,
            Elevation::Low => el.shadow_token(&t.shadow.low),
            Elevation::Med => el.shadow_token(&t.shadow.med),
            Elevation::High => el.shadow_token(&t.shadow.high),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Elevation::None => "none",
            Elevation::Low => "low",
            Elevation::Med => "med",
            Elevation::High => "high",
        }
    }
}

/// A bordered container for one discrete item, upstream `Card`.
#[derive(IntoElement)]
pub struct Card {
    size: Sizing,
    padding: Option<SpacingStep>,
    variant: CardVariant,
    elevation: Elevation,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl Card {
    pub fn new() -> Self {
        Card {
            size: Sizing::default(),
            padding: None,
            variant: CardVariant::Default,
            elevation: Elevation::None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `padding`. Defaults to the theme's card padding (step 4 without one).
        ?padding: SpacingStep,
        /// Upstream `variant`: the background.
        variant: CardVariant,
        /// Upstream `elevation`.
        elevation: Elevation,
    }

    crate::layout::size_setters!();

    pub(crate) fn root(
        &self,
        theme: &ResolvedTheme,
        dir: Direction,
        rem: Pixels,
    ) -> (Div, ContainerPadding) {
        let t = theme.tokens();
        let border = t.border.width.to_pixels(rem);
        let inset = |step: SpacingStep| (step.token(t).to_pixels(rem) - border).max(px(0.));
        let mut el = div()
            .flex()
            .flex_col()
            .rounded(t.radius.container)
            .overflow_hidden()
            .border(t.border.width)
            .border_color(t.color.border)
            .p(inset(SpacingStep::S4));
        if let Some(bg) = self.variant.background(t) {
            el = el.bg(bg);
        }
        el = self.elevation.apply(el, t);
        el = self.size.apply(el);
        theme.refine(
            el.style(),
            "card",
            &[
                ("variant", self.variant.as_str()),
                ("elevation", self.elevation.as_str()),
            ],
            None,
        );
        if let Some(step) = self.padding {
            el = el.p(inset(step));
        }
        el.style().refine(&self.style);
        let padding = container_padding(el.style(), border, rem, dir);
        (el, padding)
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let (el, padding) = self.root(&theme, use_direction(cx), window.rem_size());
        let part = theme.for_part("card", &[("variant", self.variant.as_str())]);
        el.child(container_children(self.children, padding, part))
    }
}

// ----------------------------------------------------------------------------
// Section

/// Upstream `Section` `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SectionVariant {
    #[default]
    Section,
    Transparent,
    Muted,
}

impl SectionVariant {
    fn as_str(self) -> &'static str {
        match self {
            SectionVariant::Section => "section",
            SectionVariant::Transparent => "transparent",
            SectionVariant::Muted => "muted",
        }
    }
}

/// Upstream `dividers`: a side with a border.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionSide {
    Top,
    Bottom,
    Start,
    End,
}

/// A page region for grouping content, upstream `Section`.
#[derive(IntoElement)]
pub struct Section {
    variant: SectionVariant,
    size: Sizing,
    dividers: SmallVec<[SectionSide; 4]>,
    padding: Padding,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl Section {
    pub fn new() -> Self {
        Section {
            variant: SectionVariant::Section,
            size: Sizing::default(),
            dividers: SmallVec::new(),
            padding: Padding::default(),
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `variant`.
        variant: SectionVariant,
    }

    /// Upstream `dividers`: the sides with a border.
    pub fn dividers(mut self, sides: impl IntoIterator<Item = SectionSide>) -> Self {
        self.dividers = sides.into_iter().collect();
        self
    }

    crate::layout::size_setters!();
    crate::layout::padding_setters!();
}

impl Default for Section {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Section {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Section {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Section {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let rem = window.rem_size();
        let mut el = div().flex().flex_col().border_color(t.color.border);
        el = match self.variant {
            SectionVariant::Section => el.bg(t.color.background_surface),
            SectionVariant::Muted => el.bg(t.color.background_muted),
            SectionVariant::Transparent => el,
        };
        let one: AbsoluteLength = px(1.).into();
        for side in &self.dividers {
            el = match side {
                SectionSide::Top => el.border_t_1(),
                SectionSide::Bottom => el.border_b_1(),
                SectionSide::Start => el.border_start(dir, one),
                SectionSide::End => el.border_end(dir, one),
            };
        }
        el = el.p(t.spacing.s4);
        el = self.size.apply(el);
        theme.refine(
            el.style(),
            "section",
            &[("variant", self.variant.as_str())],
            None,
        );
        el = self.padding.apply(el, t, dir);
        el.style().refine(&self.style);
        let padding = container_padding(el.style(), px(0.), rem, dir);
        el.child(container_children(self.children, padding, None))
    }
}

// ----------------------------------------------------------------------------
// EmptyState

/// A placeholder for an area with no data, upstream `EmptyState`.
#[derive(IntoElement)]
pub struct EmptyState {
    title: SharedString,
    description: Option<SharedString>,
    icon: Option<AnyElement>,
    actions: Option<AnyElement>,
    heading_level: HeadingLevel,
    compact: bool,
    style: StyleRefinement,
}

impl EmptyState {
    /// Upstream `title`.
    pub fn new(title: impl Into<SharedString>) -> Self {
        EmptyState {
            title: title.into(),
            description: None,
            icon: None,
            actions: None,
            heading_level: HeadingLevel::H3,
            compact: false,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `headingLevel`: the title's level for the outline; not its size.
        heading_level: HeadingLevel,
        /// Upstream `isCompact`: less spacing, stacked actions.
        !compact,
    }

    /// Upstream `icon`: decorative art above the title.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `actions`: buttons below the description.
    pub fn actions(mut self, actions: impl IntoElement) -> Self {
        self.actions = Some(actions.into_any_element());
        self
    }
}

impl Styled for EmptyState {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for EmptyState {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let compact = self.compact;
        let mut el = div()
            .flex()
            .flex_col()
            .items_center()
            .text_center()
            .gap(if compact { t.spacing.s2 } else { t.spacing.s4 })
            .py(if compact { t.spacing.s4 } else { t.spacing.s8 })
            .px(if compact { t.spacing.s4 } else { t.spacing.s6 });
        let props: Vec<(&str, &str)> = compact
            .then_some(("variant", "compact"))
            .into_iter()
            .collect();
        theme.refine(el.style(), "empty-state", &props, None);
        el.style().refine(&self.style);
        let title_style = if compact {
            &t.text.label
        } else {
            &t.text.large
        };
        let title = Heading::new(self.heading_level, self.title)
            .text_style(title_style)
            .font_weight(t.font.weight.semibold)
            .text_color(t.color.text_primary);
        let description = self.description.map(|text| {
            div()
                .text_style(if compact {
                    &t.text.supporting
                } else {
                    &t.text.body
                })
                .text_color(t.color.text_secondary)
                .child(text)
        });
        let actions = self.actions.map(|actions| {
            let row = div()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .mt(t.spacing.s1);
            if compact { row.flex_col() } else { row }.child(actions)
        });
        el.children(self.icon)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .max_w(px(360.))
                    .child(title)
                    .children(description),
            )
            .children(actions)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CARD_DOC: ComponentDoc = ComponentDoc {
    name: "Card",
    rust_type: "astryx::components::containers::Card",
    props: &[
        p("width", RustApi::Method("width")),
        p("height", RustApi::Method("height")),
        p("maxWidth", RustApi::Method("max_width")),
        p("minHeight", RustApi::Method("min_height")),
        p("children", RustApi::Trait("ParentElement")),
        p("padding", RustApi::Method("padding")),
        p("variant", RustApi::Method("variant")),
        p("elevation", RustApi::Method("elevation")),
    ],
    example: include_str!("examples.rs"),
};

pub const SECTION_DOC: ComponentDoc = ComponentDoc {
    name: "Section",
    rust_type: "astryx::components::containers::Section",
    props: &[
        p("variant", RustApi::Method("variant")),
        p("width", RustApi::Method("width")),
        p("height", RustApi::Method("height")),
        p("maxWidth", RustApi::Method("max_width")),
        p("minHeight", RustApi::Method("min_height")),
        p("children", RustApi::Trait("ParentElement")),
        p("dividers", RustApi::Method("dividers")),
        p("padding", RustApi::Method("padding")),
        p("paddingInline", RustApi::Method("padding_inline")),
        p(
            "paddingInlineStart",
            RustApi::Method("padding_inline_start"),
        ),
        p("paddingInlineEnd", RustApi::Method("padding_inline_end")),
        p("paddingBlock", RustApi::Method("padding_block")),
        p("paddingBlockStart", RustApi::Method("padding_block_start")),
        p("paddingBlockEnd", RustApi::Method("padding_block_end")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const EMPTY_STATE_DOC: ComponentDoc = ComponentDoc {
    name: "EmptyState",
    rust_type: "astryx::components::containers::EmptyState",
    props: &[
        p("title", RustApi::Argument("title")),
        p("description", RustApi::Method("description")),
        p("icon", RustApi::Method("icon")),
        p("actions", RustApi::Method("actions")),
        p("headingLevel", RustApi::Method("heading_level")),
        p("isCompact", RustApi::Method("compact")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> ResolvedTheme {
        ResolvedTheme::new(
            std::sync::Arc::new(astryx_themes::neutral::theme()),
            astryx_theme::Appearance::Light,
            1280.,
            astryx_theme::Pointer::Fine,
        )
    }

    #[test]
    fn card_padding_comes_back_as_container_padding() {
        let theme = theme();
        let rem = px(16.);
        let border = theme.tokens().border.width.to_pixels(rem);
        let (_, padding) = Card::new()
            .padding(SpacingStep::S6)
            .root(&theme, Direction::Ltr, rem);
        let six = theme.tokens().spacing.s6.to_pixels(rem);
        // The padding is reduced by the border; the container padding adds it back.
        assert_eq!(padding.inline_start, six);
        assert!(border > px(0.));
    }
}

#[cfg(test)]
mod context_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use astryx_primitives::use_context;
    use gpui::{
        AppContext as _, Context, IntoElement, ParentElement, Render, RenderOnce, TestAppContext,
        Window, div,
    };

    use super::Card;
    use crate::layout::{ContainerPadding, SpacingStep};

    #[derive(IntoElement)]
    struct Probe(Rc<RefCell<Option<ContainerPadding>>>);

    impl RenderOnce for Probe {
        fn render(self, _: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
            *self.0.borrow_mut() = use_context::<ContainerPadding>(cx);
            div()
        }
    }

    struct Root(Rc<RefCell<Option<ContainerPadding>>>);

    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            Card::new()
                .padding(SpacingStep::S4)
                .child(Probe(self.0.clone()))
        }
    }

    #[gpui::test]
    fn card_children_see_its_padding(cx: &mut TestAppContext) {
        cx.update(|cx| astryx_theme::init(cx));
        let seen = Rc::new(RefCell::new(None));
        let window = cx.add_window({
            let seen = seen.clone();
            move |_, _| Root(seen)
        });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let padding = seen.borrow().expect("the card provides its padding");
        assert!(padding.inline_start > gpui::px(0.), "{padding:?}");
    }
}

#[cfg(test)]
mod bleed_tests {
    use gpui::{
        Context, IntoElement, ParentElement, Render, Styled, TestAppContext, Window, div, px,
    };

    use super::Card;
    use crate::layout::{Divider, SpacingStep};

    struct Root;

    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().p(px(50.)).child(
                Card::new()
                    .padding(SpacingStep::S4)
                    .width(px(300.))
                    .child(Divider::new().full_bleed(true)),
            )
        }
    }

    #[gpui::test]
    fn full_bleed_dividers_reach_the_card_edges(cx: &mut TestAppContext) {
        cx.update(|cx| astryx_theme::init(cx));
        let (_, cx) = cx.add_window_view(|_, _| Root);
        cx.run_until_parked();
        let quads = cx.update(|window, _| window.painted_quads());
        let scale = cx.update(|window, _| window.scale_factor());
        // The line: the thinnest quad.
        let line = quads
            .iter()
            .min_by(|a, b| a.bounds.size.height.0.total_cmp(&b.bounds.size.height.0))
            .unwrap();
        let left = line.bounds.origin.x.0 / scale;
        let width = line.bounds.size.width.0 / scale;
        // The card's whole box, 50..350, as CSS's `calc(100% + padding)`
        // with margins of minus the container padding gives.
        assert!((left - 50.).abs() < 0.5, "left {left}");
        assert!((width - 300.).abs() < 0.5, "width {width}");
    }
}
