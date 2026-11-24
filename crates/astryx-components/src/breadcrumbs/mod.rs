//! Breadcrumbs and BreadcrumbItem. Upstream: `Breadcrumbs/`.
//!
//! An item's `menu` (a DropdownMenu trigger) lands with DropdownMenu (M5).

use std::rc::Rc;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{control_focus, is_focus_visible, provide, use_link_handler};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, IntoElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Window, div, prelude::*, px, relative,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::icon::{IconDefaultSize, IconSize};

/// Upstream Breadcrumbs `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BreadcrumbsVariant {
    #[default]
    Default,
    /// Smaller, in secondary text.
    Supporting,
}

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type SeparatorFn = Rc<dyn Fn() -> AnyElement>;

/// One step of the trail, upstream `BreadcrumbItem`.
pub struct BreadcrumbItem {
    label: SharedString,
    href: Option<SharedString>,
    on_click: Option<ClickFn>,
    current: Option<bool>,
    start_icon: Option<AnyElement>,
}

impl BreadcrumbItem {
    /// Upstream `children`: the label.
    pub fn new(label: impl Into<SharedString>) -> Self {
        BreadcrumbItem {
            label: label.into(),
            href: None,
            on_click: None,
            current: None,
            start_icon: None,
        }
    }

    astryx_primitives::setters! {
        /// Upstream `href`: a link through the link handler.
        ?href: SharedString,
        /// Upstream `isCurrent`: the current page; else the last item is.
        ?current: bool,
    }

    /// Upstream `onClick`.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `startIcon`: before the label.
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }
}

/// The trail from the root to the current page, upstream `Breadcrumbs`.
///
/// ```ignore
/// Breadcrumbs::new("trail")
///     .item(BreadcrumbItem::new("Home").href("/"))
///     .item(BreadcrumbItem::new("Projects").href("/projects"))
///     .item(BreadcrumbItem::new("Apollo"))
/// ```
#[derive(IntoElement)]
pub struct Breadcrumbs {
    id: ElementId,
    items: Vec<BreadcrumbItem>,
    separator: Option<SeparatorFn>,
    variant: BreadcrumbsVariant,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl Breadcrumbs {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Breadcrumbs {
            id: id.into(),
            items: Vec::new(),
            separator: None,
            variant: BreadcrumbsVariant::Default,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `variant`.
        variant: BreadcrumbsVariant,
        /// Upstream `label`: the landmark's name; "Breadcrumb" by default.
        ?label: SharedString,
    }

    /// Upstream `separator`: drawn between items; a slash by default,
    /// mirrored in right-to-left text.
    pub fn separator(mut self, f: impl Fn() -> AnyElement + 'static) -> Self {
        self.separator = Some(Rc::new(f));
        self
    }

    /// Upstream `children`: one item.
    pub fn item(mut self, item: BreadcrumbItem) -> Self {
        self.items.push(item);
        self
    }

    /// Upstream `children`: items.
    pub fn items(mut self, items: impl IntoIterator<Item = BreadcrumbItem>) -> Self {
        self.items.extend(items);
        self
    }
}

impl Styled for Breadcrumbs {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Breadcrumbs {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let supporting = self.variant == BreadcrumbsVariant::Supporting;
        let text = if supporting {
            &t.text.supporting
        } else {
            &t.text.body
        };
        let count = self.items.len();
        let current_color = if supporting {
            t.color.text_secondary
        } else {
            t.color.text_primary
        };
        let separator = self.separator.clone();
        let handler = use_link_handler(cx);
        let name = self
            .label
            .clone()
            .unwrap_or_else(|| i18n(cx).t("@astryx.breadcrumbs.label"));
        let id = self.id.clone();

        let items = self
            .items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let is_current = item.current.unwrap_or(i + 1 == count);
                let sep = (i > 0).then(|| {
                    let glyph = match &separator {
                        Some(f) => f(),
                        None => div().child("/").mirror_in_rtl(dir).into_any_element(),
                    };
                    div()
                        .flex()
                        .items_center()
                        .py(t.spacing.s1)
                        .text_color(t.color.text_secondary)
                        .child(glyph)
                });
                let content = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(t.spacing.s1)
                    .children(
                        item.start_icon
                            .map(|icon| provide(IconDefaultSize(IconSize::Sm), icon)),
                    )
                    .child(item.label.clone());
                let interactive = !is_current && (item.href.is_some() || item.on_click.is_some());
                let item_id = ElementId::Name(format!("{id:?}-{i}").into());
                let body = if is_current {
                    content
                        .font_weight(t.font.weight.semibold)
                        .text_color(current_color)
                        .into_any_element()
                } else if interactive {
                    let focus = control_focus(&item_id, window, cx);
                    let on_click = item.on_click.clone();
                    let navigation = item.href.clone().map(|href| (href, handler.clone()));
                    let mut link = content
                        .relative()
                        .py(t.spacing.s1)
                        .text_color(t.color.text_secondary)
                        .id(item_id.clone())
                        .track_focus(&focus)
                        .cursor_pointer()
                        .hover(|s| s.underline())
                        .on_click(move |event, window, cx| {
                            cx.stop_propagation();
                            if let Some(f) = &on_click {
                                f(event, window, cx);
                                if window.default_prevented() {
                                    return;
                                }
                            }
                            if let Some((href, handler)) = &navigation {
                                handler.open(href, None, window, cx);
                            }
                        });
                    if is_focus_visible(&focus, window) {
                        link = link.child(focus_ring(t, px(0.), rem));
                    }
                    let role = if item.href.is_some() {
                        Role::Link
                    } else {
                        Role::Button
                    };
                    link.role(role).aria_label(item.label).into_any_element()
                } else {
                    content
                        .text_color(t.color.text_secondary)
                        .into_any_element()
                };
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(t.spacing.s1)
                    .children(sep)
                    .child(body)
            })
            .collect::<Vec<_>>();

        let mut el = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(t.spacing.s1)
            .text_size(text.size)
            .line_height(relative(text.leading));
        theme.refine(
            el.style(),
            "breadcrumbs",
            &[("variant", if supporting { "supporting" } else { "default" })],
            None,
        );
        el.style().refine(&self.style);
        el.id(self.id)
            .role(Role::Navigation)
            .aria_label(name)
            .children(items)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const BREADCRUMBS_DOC: ComponentDoc = ComponentDoc {
    name: "Breadcrumbs",
    rust_type: "astryx::components::breadcrumbs::Breadcrumbs",
    props: &[
        p("children", RustApi::Method("item / items")),
        p("separator", RustApi::Method("separator")),
        p("variant", RustApi::Method("variant")),
        p("label", RustApi::Method("label")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("../pagination/examples.rs"),
};

pub const BREADCRUMB_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "BreadcrumbItem",
    rust_type: "astryx::components::breadcrumbs::BreadcrumbItem",
    props: &[
        p("children", RustApi::Argument("label")),
        p("href", RustApi::Method("href")),
        p("onClick", RustApi::Method("on_click")),
        p("isCurrent", RustApi::Method("current")),
        p("startIcon", RustApi::Method("start_icon")),
        p(
            "menu",
            RustApi::Waived("not yet: the item menu lands with DropdownMenu (M5)"),
        ),
        p(
            "menuSize",
            RustApi::Waived("not yet: the item menu lands with DropdownMenu (M5)"),
        ),
        p(
            "as",
            RustApi::Waived("the link handler (link_provider) routes every href"),
        ),
    ],
    example: include_str!("../pagination/examples.rs"),
};
