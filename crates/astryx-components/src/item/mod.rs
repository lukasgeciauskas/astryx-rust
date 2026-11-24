//! Item: the row layout lists, menus and choice lists build on. Upstream:
//! `Item/`.
//!
//! This is Item's core: start content, a label over a description, end
//! content, alignment, density, line clamps, and the interactive states.
//! Swipe actions land with the touch layer.

use std::rc::Rc;

use astryx_i18n::{LogicalStyled, use_direction};
use astryx_primitives::{control_focus, is_focus_visible, use_link_handler};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Window, div, prelude::*, relative,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;

/// Upstream `align`: how start and end content sit against the text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItemAlign {
    /// Centered on the whole text block.
    #[default]
    Center,
    /// At the top of the text block.
    Start,
    /// Centered on the label's first line.
    FirstLine,
}

/// Upstream `density`: the row's block padding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItemDensity {
    Compact,
    #[default]
    Balanced,
    Spacious,
}

impl ItemDensity {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemDensity::Compact => "compact",
            ItemDensity::Balanced => "balanced",
            ItemDensity::Spacious => "spacious",
        }
    }
}

/// Upstream `layout`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItemLayout {
    /// The description under the label.
    #[default]
    Stacked,
    /// The description after the label on one line.
    Inline,
}

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A row with start content, label, description and end content, upstream
/// `Item`.
#[derive(IntoElement)]
pub struct Item {
    id: ElementId,
    label: AnyElement,
    description: Option<AnyElement>,
    start_content: Option<AnyElement>,
    end_content: Option<AnyElement>,
    marker: Option<AnyElement>,
    align: ItemAlign,
    density: ItemDensity,
    label_lines: Option<usize>,
    description_lines: Option<usize>,
    layout: ItemLayout,
    highlighted: bool,
    selected: bool,
    disabled: bool,
    on_click: Option<ClickFn>,
    href: Option<SharedString>,
    target: Option<SharedString>,
    bare: bool,
    style: StyleRefinement,
}

impl Item {
    /// Upstream `label`: text, or any element.
    pub fn new(id: impl Into<ElementId>, label: impl IntoElement) -> Self {
        Item {
            id: id.into(),
            label: label.into_any_element(),
            description: None,
            start_content: None,
            end_content: None,
            marker: None,
            align: ItemAlign::Center,
            density: ItemDensity::Balanced,
            label_lines: None,
            description_lines: None,
            layout: ItemLayout::Stacked,
            highlighted: false,
            selected: false,
            disabled: false,
            on_click: None,
            href: None,
            target: None,
            bare: false,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `align`.
        align: ItemAlign,
        /// Upstream `density`.
        density: ItemDensity,
        /// Upstream `labelLines`: lines before the label truncates.
        ?label_lines: usize,
        /// Upstream `descriptionLines`: lines before the description truncates.
        ?description_lines: usize,
        /// Upstream `layout`.
        layout: ItemLayout,
        /// Upstream `isHighlighted`: the hover tint, for keyboard-driven lists.
        !highlighted,
        /// Upstream `isSelected`: the accent tint.
        !selected,
        /// Upstream `isDisabled`: dimmed, never activates.
        !disabled,
        /// Upstream `href`: the row is a link through the link handler.
        ?href: SharedString,
        /// Upstream `target`, with `href`.
        ?target: SharedString,
    }

    /// Upstream `description`: text, or any element.
    pub fn description(mut self, description: impl IntoElement) -> Self {
        self.description = Some(description.into_any_element());
        self
    }

    /// Upstream `startContent`.
    pub fn start_content(mut self, content: impl IntoElement) -> Self {
        self.start_content = Some(content.into_any_element());
        self
    }

    /// Upstream `endContent`.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }

    /// Upstream `marker`: a list marker before everything else.
    pub fn marker(mut self, marker: impl IntoElement) -> Self {
        self.marker = Some(marker.into_any_element());
        self
    }

    /// Upstream `onClick`: the whole row activates.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    pub(crate) fn end_content_opt(mut self, content: Option<AnyElement>) -> Self {
        self.end_content = content;
        self
    }

    pub(crate) fn description_opt(mut self, description: Option<AnyElement>) -> Self {
        self.description = description;
        self
    }

    /// No padding, radius or row tint: the row is a bare surface (the
    /// radio list's look).
    pub(crate) fn bare(mut self, bare: bool) -> Self {
        self.bare = bare;
        self
    }

    fn interactive(&self) -> bool {
        self.on_click.is_some() || self.href.is_some()
    }
}

impl Styled for Item {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Item {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let interactive = self.interactive();

        let clamp = |el: gpui::Div, lines: Option<usize>| match lines {
            Some(1) => el.overflow_hidden().text_ellipsis().whitespace_nowrap(),
            Some(n) if n > 1 => el.overflow_hidden().line_clamp(n),
            _ => el,
        };
        let label = clamp(
            div()
                .text_size(t.text.body.size)
                .line_height(relative(t.text.body.leading))
                .text_color(t.color.text_primary)
                .child(self.label),
            self.label_lines,
        );
        let description = self.description.map(|d| {
            clamp(
                div()
                    .text_size(t.text.supporting.size)
                    .line_height(relative(t.text.supporting.leading))
                    .text_color(t.color.text_secondary)
                    .child(d),
                self.description_lines,
            )
        });
        let content = match self.layout {
            ItemLayout::Stacked => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(label)
                .children(description),
            ItemLayout::Inline => div()
                .flex()
                .flex_row()
                .items_center()
                .gap(t.spacing.s1)
                .flex_1()
                .min_w_0()
                .child(label.flex_none())
                .children(description.map(|d| d.flex_shrink(1.).min_w_0())),
        };
        // First-line alignment centers the slots on the label's first line.
        let first_line = t.text.body.size.to_pixels(rem) * t.text.body.leading;
        let slot = |content: AnyElement| {
            let el = div().flex().flex_none();
            match self.align {
                ItemAlign::FirstLine => el.items_center().h(first_line).child(content),
                _ => el.child(content),
            }
        };
        let start = self.start_content.map(slot);
        let end = self
            .end_content
            .map(|c| slot(c).margin_start(dir, gpui::Length::Auto));

        let mut el = div()
            .relative()
            .flex()
            .flex_row()
            .gap(t.spacing.s2)
            .min_w_0();
        el = match self.align {
            ItemAlign::Center => el.items_center(),
            ItemAlign::Start | ItemAlign::FirstLine => el.items_start(),
        };
        if !self.bare {
            let (block, inline) = match self.density {
                ItemDensity::Compact => (t.spacing.s1, t.spacing.s2),
                ItemDensity::Balanced => (t.spacing.s2, t.spacing.s2),
                ItemDensity::Spacious => (t.spacing.s3, t.spacing.s3),
            };
            el = el.py(block).px(inline).rounded(t.radius.element);
            if self.selected {
                el = el.bg(t.color.accent_muted);
            } else if self.highlighted {
                el = el.bg(t.color.overlay_hover);
            }
        }
        let body = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_w_0()
            .gap(t.spacing.s2)
            .map(|b| match self.align {
                ItemAlign::Center => b.items_center(),
                _ => b.items_start(),
            })
            .children(start)
            .child(content)
            .children(end);
        let body = if self.disabled {
            body.opacity(0.5)
        } else {
            body
        };
        el = el.children(self.marker).child(body);
        theme.refine(
            el.style(),
            "item",
            &[("density", self.density.as_str())],
            None,
        );
        el.style().refine(&self.style);

        let el = el.id(self.id.clone());
        if !interactive {
            return el.into_any_element();
        }
        let focus = control_focus(&self.id, window, cx);
        let mut el = el.track_focus(&focus);
        if self.disabled {
            el = el.on_click(|_, _, cx| cx.stop_propagation());
        } else {
            let on_click = self.on_click.clone();
            let navigation = self
                .href
                .clone()
                .map(|href| (href, self.target.clone(), use_link_handler(cx)));
            let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);
            if !self.bare && !self.selected {
                el = el.hover(move |s| s.bg(hover));
            }
            if !self.bare {
                el = el.active(move |s| s.bg(pressed));
            }
            el = el.cursor_pointer().on_click(move |event, window, cx| {
                cx.stop_propagation();
                if let Some(f) = &on_click {
                    f(event, window, cx);
                    if window.default_prevented() {
                        return;
                    }
                }
                if let Some((href, target, handler)) = &navigation {
                    handler.open(href, target.clone(), window, cx);
                }
            });
        }
        if is_focus_visible(&focus, window) {
            el = el.child(focus_ring(t, t.radius.element.to_pixels(rem), rem));
        }
        let role = if self.href.is_some() {
            Role::Link
        } else {
            Role::Button
        };
        el.role(role).into_any_element()
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const SWIPE: &str = "not yet: swipe actions land with the touch layer (M11)";

pub const ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "Item",
    rust_type: "astryx::components::item::Item",
    props: &[
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("as", RustApi::Waived("GPUI has no element types to choose")),
        p("marker", RustApi::Method("marker")),
        p("startContent", RustApi::Method("start_content")),
        p("label", RustApi::Argument("label")),
        p("description", RustApi::Method("description")),
        p("endContent", RustApi::Method("end_content")),
        p("align", RustApi::Method("align")),
        p("density", RustApi::Method("density")),
        p("labelLines", RustApi::Method("label_lines")),
        p("descriptionLines", RustApi::Method("description_lines")),
        p("layout", RustApi::Method("layout")),
        p("onClick", RustApi::Method("on_click")),
        p(
            "interactiveRef",
            RustApi::Waived("GPUI elements have no refs"),
        ),
        p("href", RustApi::Method("href")),
        p("target", RustApi::Method("target")),
        p("rel", RustApi::Waived("GPUI opens URLs without a referrer")),
        p("isHighlighted", RustApi::Method("highlighted")),
        p("isSelected", RustApi::Method("selected")),
        p("isDisabled", RustApi::Method("disabled")),
        p("swipe", RustApi::Waived(SWIPE)),
        p("swipeActions", RustApi::Waived(SWIPE)),
        p("swipeBehavior", RustApi::Waived(SWIPE)),
    ],
    example: include_str!("../choice/examples.rs"),
};
