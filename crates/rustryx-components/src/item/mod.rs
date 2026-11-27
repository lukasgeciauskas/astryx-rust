//! Item: the row layout lists, menus and choice lists build on. Upstream:
//! `Item/`.
//!
//! Start content, a label over a description, end content, alignment, density,
//! line clamps, the interactive states, and swipe actions (`swipe`, spec:AST-057):
//! a row given a [`create_swipe`](crate::swipe::create_swipe) value moves aside
//! under a finger, a pen or a trackpad's swipe and uncovers its panels, drawn into
//! its own root. A verb reachable only by swipe is unreachable by keyboard and by
//! mouse, so give every swipe action a second home in the row (a menu, or a button
//! in the end content). The element containing the rows clips in the inline axis:
//! List does; any other host sets `overflow_x_hidden` once (FR9).

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, FocusHandle, IntoElement, ParentElement, Pixels,
    RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, div, prelude::*, px, relative,
};
use rustryx_i18n::{LogicalStyled, use_direction};
use rustryx_primitives::{control_focus, is_focus_visible, use_link_handler};
use rustryx_theme::current;

use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::swipe::{
    ItemSwipeActions, ItemSwipeBehavior, Swipe, SwipeHost, SwipeId, SwipeRowFacts,
    resolve_row_swipe, use_swipe_row,
};

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
type ActivateFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// The least hit target a row offers under a finger (List FR10).
const TOUCH_TARGET: f32 = 44.;

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
    delegate: Option<(FocusHandle, ActivateFn)>,
    touch_band: Option<(Option<Pixels>, Option<Pixels>)>,
    swipe: Option<Rc<SwipeHost>>,
    swipe_actions: Option<ItemSwipeActions>,
    swipe_behavior: Option<ItemSwipeBehavior>,
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
            delegate: None,
            touch_band: None,
            swipe: None,
            swipe_actions: None,
            swipe_behavior: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `swipe`: the row's swipe actions, one
    /// [`create_swipe`](crate::swipe::create_swipe) value (spec:AST-057). A finger, a
    /// pen or a trackpad's two-finger swipe drags the row aside and uncovers the side's
    /// panel; a mouse drag and a mouse wheel's notches never do. An open row closes on a
    /// touch tap on its own content (which activates nothing), a press elsewhere, a
    /// scroll that moves it with focus outside its panel, and Escape. Served on a row
    /// that is not disabled and not the enlarged target of a control inside it.
    pub fn swipe<Id: SwipeId>(mut self, swipe: Swipe<Id>) -> Self {
        self.swipe = Some(swipe.host);
        self
    }

    /// Deprecated upstream `swipeActions`: use [`Self::swipe`]. Still works, with the
    /// swipe's current behavior; ignored when `swipe` is set.
    pub fn swipe_actions(mut self, actions: ItemSwipeActions) -> Self {
        self.swipe_actions = Some(actions);
        self
    }

    /// Deprecated upstream `swipeBehavior`: pass it in `CreateSwipeOptions`. Ignored
    /// when `swipe` is set.
    pub fn swipe_behavior(mut self, behavior: ItemSwipeBehavior) -> Self {
        self.swipe_behavior = Some(behavior);
        self
    }

    pub(crate) fn swipe_host(mut self, host: Option<Rc<SwipeHost>>) -> Self {
        self.swipe = host;
        self
    }

    /// Upstream `interactiveRef`: a control inside the row (a checkbox in
    /// the start content) that owns its keyboard access and action. The row
    /// becomes a larger target for it, with no tab stop of its own: a press
    /// on the row's surface focuses `focus` and runs `activate`, while a
    /// press on a control inside the row stays that control's. It takes the
    /// place of `on_click` and `href`.
    pub fn interactive(
        mut self,
        focus: FocusHandle,
        activate: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.delegate = Some((focus, Rc::new(activate)));
        self
    }

    /// Under a finger, how far the row's hit target may grow up and down
    /// (`None`: as far as it needs) to reach 44px.
    pub(crate) fn touch_band(mut self, up: Option<Pixels>, down: Option<Pixels>) -> Self {
        self.touch_band = Some((up, down));
        self
    }

    rustryx_primitives::setters! {
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

    fn is_interactive(&self) -> bool {
        self.on_click.is_some() || self.href.is_some() || self.delegate.is_some()
    }
}

impl Styled for Item {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Item {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let interactive = self.is_interactive();
        // Swipe actions, when they serve this row (spec:AST-057 DEC-3).
        let swipe = resolve_row_swipe(
            self.swipe.take(),
            self.swipe_actions.as_ref(),
            self.swipe_behavior,
        )
        .filter(|host| {
            host.serves(SwipeRowFacts {
                role: None,
                disabled: self.disabled,
                delegate: self.delegate.is_some(),
                link_root: false,
            })
        })
        .map(|host| use_swipe_row(&self.id, host, window, cx));
        // An open row's root takes no press: a tap on its content only closes it.
        let press_held = swipe.as_ref().is_some_and(|row| row.is_open());

        let clamp = |el: gpui::Div, lines: Option<usize>| match lines {
            Some(1) => el.overflow_hidden().text_ellipsis().whitespace_nowrap(),
            Some(n) if n > 1 => el.overflow_hidden().line_clamp(n),
            _ => el,
        };
        let label_selector = format!("{}-label", self.id);
        let label = clamp(
            div()
                .debug_selector(move || label_selector)
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

        let row_selector = self.id.to_string();
        let mut el = el.id(self.id.clone()).debug_selector(move || row_selector);
        let radii = el.style().corner_radii.clone();
        let finish = |el: gpui::Stateful<gpui::Div>,
                      own: Option<FocusHandle>,
                      window: &mut Window,
                      cx: &mut App|
         -> AnyElement {
            match &swipe {
                Some(row) => {
                    crate::swipe::decorate(row, el, &self.id, radii.clone(), own, window, cx)
                }
                None => el.into_any_element(),
            }
        };
        if !interactive {
            return finish(el, None, window, cx);
        }
        let touch = window.last_input_was_touch();
        let radius = t.radius.element.to_pixels(rem);
        // What activating the row does: the caller's click, its link, or its
        // delegate control (upstream `interactiveRef`).
        let activate: ClickFn = {
            let on_click = self.on_click.clone();
            let navigation = self
                .href
                .clone()
                .map(|href| (href, self.target.clone(), use_link_handler(cx)));
            let delegate = self.delegate.clone();
            Rc::new(
                move |event: &ClickEvent, window: &mut Window, cx: &mut App| {
                    cx.stop_propagation();
                    if let Some((focus, activate)) = &delegate {
                        focus.focus(window, cx);
                        activate(window, cx);
                        return;
                    }
                    if let Some(f) = &on_click {
                        f(event, window, cx);
                        if window.default_prevented() {
                            return;
                        }
                    }
                    if let Some((href, target, handler)) = &navigation {
                        handler.open(href, target.clone(), window, cx);
                    }
                },
            )
        };
        let mut own_focus = None;
        let mut el = match &self.delegate {
            // A delegate row is a larger target for its control: no tab stop
            // of its own, its ring when the control has keyboard focus.
            Some((focus, _)) => {
                let mut el = el;
                if is_focus_visible(focus, window) {
                    el = el.child(focus_ring(t, radius, rem));
                }
                el
            }
            None => {
                let focus = control_focus(&self.id, window, cx);
                own_focus = Some(focus.clone());
                let mut el = el.track_focus(&focus);
                if is_focus_visible(&focus, window) {
                    el = el.child(focus_ring(t, radius, rem));
                }
                el
            }
        };
        if self.disabled {
            el = el.on_click(|_, _, cx| cx.stop_propagation());
        } else {
            let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);
            if !self.bare && !self.selected && !touch {
                el = el.hover(move |s| s.bg(hover));
            }
            let mut row_press = None;
            if !self.bare && !press_held {
                // FR11: under a finger the press paints as UIKit tracks a row (at once
                // outside a scroller, after the content-touch delay inside one) and its
                // release fades on the exit motion (upstream `usePressFeedback`).
                let base = if self.selected {
                    t.color.accent_muted
                } else if self.highlighted {
                    t.color.overlay_hover
                } else {
                    gpui::transparent_black()
                };
                let press = crate::gestures::use_press_feedback(
                    ElementId::Name(format!("{:?}-pressed", self.id).into()),
                    window,
                    cx,
                );
                el = crate::press::touch_press_with(el, &press, base, pressed, window, cx);
                row_press = Some(press);
            }
            el = el.cursor_pointer().on_click({
                let activate = activate.clone();
                move |event, window, cx| activate(event, window, cx)
            });
            // FR10: under a finger the row's hit target reaches 44px where
            // its neighbors leave room, with no visual change.
            if touch && let Some((up, down)) = self.touch_band {
                let height = window.use_keyed_state(
                    ElementId::Name(format!("{:?}-height", self.id).into()),
                    cx,
                    |_, _| px(0.),
                );
                let need = ((px(TOUCH_TARGET) - *height.read(cx)) / 2.).max(px(0.));
                // Past its list the band also stops halfway to any neighbour the window
                // drew (upstream `useTouchHitArea`'s clamp against the page's controls).
                let key = crate::gestures::touch_hit_area::key_of(&self.id);
                let clamp = crate::gestures::touch_hit_area::hit_insets(
                    window,
                    key,
                    cx.background_executor().now(),
                );
                let noted = crate::gestures::touch_hit_area::noted_rect(window, key).is_some();
                let (top, bottom) = (
                    up.map_or(need, |u| need.min(u)),
                    down.map_or(need, |d| need.min(d)),
                );
                let (top, bottom) = if noted {
                    (top.min(px(clamp.top)), bottom.min(px(clamp.bottom)))
                } else {
                    (top, bottom)
                };
                el = el.child(crate::gestures::touch_hit_area::target_probe(key));
                el = el.child(
                    gpui::canvas(
                        move |bounds, _, cx| {
                            if *height.read(cx) != bounds.size.height {
                                height.update(cx, |h, _| *h = bounds.size.height);
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                );
                if top > px(0.) || bottom > px(0.) {
                    let activate = activate.clone();
                    el = el.child(
                        div()
                            .id(ElementId::Name(format!("{:?}-hit", self.id).into()))
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(-top)
                            .bottom(-bottom)
                            // A finger on the band presses the row and shows its paint.
                            .children(row_press.as_ref().map(|p| {
                                p.grown_area_probe().absolute().top_0().left_0().size_full()
                            }))
                            .on_click(move |event, window, cx| activate(event, window, cx)),
                    );
                }
            }
        }
        if self.delegate.is_some() {
            return el.into_any_element();
        }
        if self.selected {
            el = el.aria_current(gpui::AriaCurrent::True);
        }
        let role = if self.href.is_some() {
            Role::Link
        } else {
            Role::Button
        };
        finish(el.role(role), own_focus, window, cx)
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "Item",
    rust_type: "rustryx::components::item::Item",
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
        p("interactiveRef", RustApi::Method("interactive")),
        p("href", RustApi::Method("href")),
        p("target", RustApi::Method("target")),
        p("rel", RustApi::Waived("GPUI opens URLs without a referrer")),
        p("isHighlighted", RustApi::Method("highlighted")),
        p("isSelected", RustApi::Method("selected")),
        p("isDisabled", RustApi::Method("disabled")),
        p("swipe", RustApi::Method("swipe (a create_swipe value)")),
        p(
            "swipeActions",
            RustApi::Method("swipe_actions (deprecated: use swipe)"),
        ),
        p(
            "swipeBehavior",
            RustApi::Method("swipe_behavior (deprecated: CreateSwipeOptions::swipe_behavior)"),
        ),
    ],
    example: include_str!("../choice/examples.rs"),
};
