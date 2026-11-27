//! People: Avatar (AvatarStatusDot) and AvatarGroup (AvatarGroupOverflow).
//! Upstream: `Avatar/`, `AvatarGroup/`.

pub mod examples;

use std::f32::consts::SQRT_2;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, Hsla, ImageSource, IntoElement, ObjectFit,
    ParentElement, PathBuilder, Pixels, RenderOnce, Role, SharedString, StyleRefinement, Styled,
    StyledImage, Window, canvas, div, img, point, prelude::*, px,
};
use rustryx_i18n::{LogicalStyled, current as i18n, use_direction};
use rustryx_primitives::{
    ListKeys, RovingScope, control_focus, is_focus_visible, provide, use_context, use_link_handler,
};
use rustryx_theme::{ThemeStyled, Tokens, current};
use smallvec::SmallVec;

use crate::doc::{ComponentDoc, PropMapping, RustApi};

/// Upstream `AvatarSize`: a named size or pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AvatarSize {
    /// 20 px.
    Xsm,
    /// 24 px.
    Sm,
    /// 36 px.
    #[default]
    Md,
    /// 48 px.
    Lg,
    /// 128 px.
    Xl,
    /// Any size in pixels.
    Px(f32),
}

impl AvatarSize {
    /// Upstream `resolveSize`.
    pub fn pixels(self) -> f32 {
        match self {
            AvatarSize::Xsm => 20.,
            AvatarSize::Sm => 24.,
            AvatarSize::Md => 36.,
            AvatarSize::Lg => 48.,
            AvatarSize::Xl => 128.,
            AvatarSize::Px(px) => px,
        }
    }
}

/// Upstream `shape`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AvatarShape {
    #[default]
    Circle,
    /// The element radius token.
    Rounded,
    Square,
}

impl AvatarShape {
    fn radius(self, t: &Tokens) -> gpui::AbsoluteLength {
        match self {
            AvatarShape::Circle => t.radius.full,
            AvatarShape::Rounded => t.radius.element,
            AvatarShape::Square => t.radius.none,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            AvatarShape::Circle => "circle",
            AvatarShape::Rounded => "rounded",
            AvatarShape::Square => "square",
        }
    }
}

/// Upstream `getInitials`: the first letters of the first and last words.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let first = |word: &str| word.chars().next().map(|c| c.to_uppercase().to_string());
    match words.as_slice() {
        [] => String::new(),
        [only] => first(only).unwrap_or_default(),
        [head, .., last] => format!(
            "{}{}",
            first(head).unwrap_or_default(),
            first(last).unwrap_or_default()
        ),
    }
}

/// What a group sets on every avatar in it.
#[derive(Clone, Copy)]
struct GroupContext {
    size: AvatarSize,
    shape: AvatarShape,
}

/// The avatar size a status dot sizes itself from.
#[derive(Clone, Copy)]
struct StatusHost(f32);

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A person's or team's picture: photo, then fallback photo, then initials,
/// then an icon. Upstream `Avatar`.
#[derive(IntoElement)]
pub struct Avatar {
    id: ElementId,
    src: Option<SharedString>,
    fallback_src: Option<SharedString>,
    name: Option<SharedString>,
    alt: Option<SharedString>,
    size: AvatarSize,
    shape: AvatarShape,
    status: Option<AnyElement>,
    tooltip: Option<SharedString>,
    href: Option<SharedString>,
    target: Option<SharedString>,
    on_click: Option<ClickHandler>,
    style: StyleRefinement,
}

impl Avatar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Avatar {
            id: id.into(),
            src: None,
            fallback_src: None,
            name: None,
            alt: None,
            size: AvatarSize::Md,
            shape: AvatarShape::Circle,
            status: None,
            tooltip: None,
            href: None,
            target: None,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `src`: the photo.
        ?src: SharedString,
        /// Upstream `fallbackSrc`: the photo if `src` fails to load.
        ?fallback_src: SharedString,
        /// Upstream `name`: for initials and the accessible name.
        ?name: SharedString,
        /// Upstream `alt`: the accessible name; falls back to `name`.
        ?alt: SharedString,
        /// Upstream `size`.
        size: AvatarSize,
        /// Upstream `shape`.
        shape: AvatarShape,
        /// Upstream `tooltip`: shown on hover and keyboard focus.
        ?tooltip: SharedString,
        /// Upstream `href`: makes the avatar a link through the link handler.
        ?href: SharedString,
        /// Upstream `target`, with `href`.
        ?target: SharedString,
    }

    /// Upstream `status`: corner content, usually an [`AvatarStatusDot`].
    pub fn status(mut self, status: impl IntoElement) -> Self {
        self.status = Some(status.into_any_element());
        self
    }

    /// Upstream `onClick`: makes the avatar a button.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl Styled for Avatar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Upstream's `DefaultIcon`: a person silhouette for avatars without a name.
const PERSON_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/></svg>"#;

fn fallback_face(
    initials_text: String,
    size: f32,
    radius: gpui::AbsoluteLength,
    t: &Tokens,
) -> AnyElement {
    let face = div()
        .flex()
        .items_center()
        .justify_center()
        .size_full()
        .rounded(radius)
        .bg(t.color.neutral)
        .text_color(t.color.text_primary)
        .font_stack(&t.font.family.body)
        .font_weight(t.font.weight.medium);
    if initials_text.is_empty() {
        face.child(
            gpui::svg()
                .data(PERSON_SVG)
                .size(px(size * 0.6))
                .text_color(t.color.text_primary),
        )
        .into_any_element()
    } else {
        face.text_size(px(size * 0.4))
            .line_height(px(size * 0.4 * 1.2))
            .child(initials_text)
            .into_any_element()
    }
}

impl RenderOnce for Avatar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let group = use_context::<GroupContext>(cx);
        let size = group.map_or(self.size, |g| g.size).pixels();
        let shape = group.map_or(self.shape, |g| g.shape);
        let radius = shape.radius(t);
        let initials_text = self.name.as_deref().map(initials).unwrap_or_default();

        let fallback = {
            let t = t.clone();
            let initials_text = initials_text.clone();
            move || fallback_face(initials_text.clone(), size, radius, &t)
        };
        let content: AnyElement = match (self.src.clone(), self.fallback_src.clone()) {
            (Some(src), Some(second)) => {
                let fallback = fallback.clone();
                img(ImageSource::from(src))
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(move || {
                        let fallback = fallback.clone();
                        img(ImageSource::from(second.clone()))
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .with_fallback(fallback)
                            .into_any_element()
                    })
                    .into_any_element()
            }
            (Some(src), None) | (None, Some(src)) => img(ImageSource::from(src))
                .size_full()
                .object_fit(ObjectFit::Cover)
                .with_fallback(fallback)
                .into_any_element(),
            (None, None) => fallback(),
        };

        let name = self.alt.clone().or_else(|| self.name.clone());
        let mut el = div()
            .relative()
            .inline_flex()
            .flex_none()
            .size(px(size))
            .rounded(radius);
        if group.is_some() {
            // A grouped face is ringed in the surface color so overlaps read.
            el = el.border_2().border_color(t.color.background_surface);
        }
        theme.refine(el.style(), "avatar", &[("shape", shape.as_str())], None);
        el.style().refine(&self.style);

        let status = self.status.map(|status| {
            let inset = if shape == AvatarShape::Circle {
                px(size * (1. - 1. / SQRT_2) / 2.)
            } else {
                px(0.)
            };
            let push = if shape == AvatarShape::Circle {
                0.5
            } else {
                0.25
            };
            let toward_end = if dir.is_rtl() { -push } else { push };
            div()
                .absolute()
                .bottom(inset)
                .inset_end(dir, inset)
                .translate(gpui::relative(toward_end), gpui::relative(push))
                .child(provide(StatusHost(size), status))
        });

        let el = el
            .id(self.id.clone())
            .child(
                div()
                    .size_full()
                    .rounded(radius)
                    .overflow_hidden()
                    .child(content),
            )
            .children(status);
        let el = match name.clone() {
            Some(name) => el.aria_label(name),
            None => el,
        };
        let tooltip_id = self.id.clone();
        let tooltip = self.tooltip.clone();
        let acts = self.href.is_some() || self.on_click.is_some();
        // A link or button avatar takes focus (and, in a group, joins its
        // one tab stop: AvatarGroup FR4); Enter and Space activate it.
        let el = if acts {
            let focus = control_focus(&self.id, window, cx);
            let rem = window.rem_size();
            let ring = is_focus_visible(&focus, window)
                .then(|| crate::focus::focus_ring(t, radius.to_pixels(rem), rem));
            el.track_focus(&focus).children(ring)
        } else {
            el
        };
        let el = if let Some(href) = self.href {
            let handler = use_link_handler(cx);
            let target = self.target;
            el.role(Role::Link)
                .cursor_pointer()
                .on_click(move |_, window, cx| handler.open(&href, target.clone(), window, cx))
                .into_any_element()
        } else if let Some(on_click) = self.on_click {
            el.role(Role::Button)
                .cursor_pointer()
                .on_click(on_click)
                .into_any_element()
        } else {
            el.role(Role::Image).into_any_element()
        };
        crate::overlay::with_tooltip(&tooltip_id, tooltip, acts, el)
    }
}

/// Upstream `AvatarStatusDot` `variant`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AvatarStatusVariant {
    Success,
    /// Drawn with a ring, so it reads without color.
    Neutral,
    /// Drawn with a minus, so it reads without color.
    Error,
}

/// A status dot for an avatar's corner, sized from the avatar. Upstream
/// `AvatarStatusDot`.
#[derive(IntoElement)]
pub struct AvatarStatusDot {
    id: ElementId,
    variant: AvatarStatusVariant,
    label: Option<SharedString>,
    icon: Option<AnyElement>,
}

impl AvatarStatusDot {
    pub fn new(id: impl Into<ElementId>, variant: AvatarStatusVariant) -> Self {
        AvatarStatusDot {
            id: id.into(),
            variant,
            label: None,
            icon: None,
        }
    }

    /// Upstream `label`: the status, added to the avatar's accessible name.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `icon`: shown from the medium tier (avatars over 36 px) up.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }
}

/// Upstream `resolveStatusDotSize`: (dot, border, icon, glyph stroke).
fn status_tier(avatar: f32) -> (f32, f32, f32, f32) {
    if avatar <= 36. {
        (10., 1., 0., 1.)
    } else if avatar <= 72. {
        (20., 2., 12., 1.5)
    } else {
        (32., 4., 18., 2.)
    }
}

impl RenderOnce for AvatarStatusDot {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let host = use_context::<StatusHost>(cx).map_or(36., |h| h.0);
        let (dot, border, icon_size, stroke) = status_tier(host);
        let (bg, ink): (Hsla, Hsla) = match self.variant {
            AvatarStatusVariant::Success => (t.color.success, t.color.on_success),
            AvatarStatusVariant::Neutral => (t.color.background_surface, t.color.text_secondary),
            AvatarStatusVariant::Error => (t.color.error, t.color.on_error),
        };
        let field = dot - border * 2.;
        let variant = self.variant;
        let glyph = (self.icon.is_none() || icon_size == 0.).then(|| {
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let c = bounds.center();
                    match variant {
                        AvatarStatusVariant::Neutral => {
                            let r = px(field / 2. - stroke);
                            let mut ring = PathBuilder::stroke(px(stroke));
                            ring.move_to(point(c.x + r, c.y));
                            ring.arc_to(point(r, r), px(0.), false, true, point(c.x - r, c.y));
                            ring.arc_to(point(r, r), px(0.), false, true, point(c.x + r, c.y));
                            if let Ok(path) = ring.build() {
                                window.paint_path(path, ink);
                            }
                        }
                        AvatarStatusVariant::Error => {
                            let half = px(field * 0.3);
                            let mut bar = PathBuilder::stroke(px(stroke));
                            bar.move_to(point(c.x - half, c.y));
                            bar.line_to(point(c.x + half, c.y));
                            if let Ok(path) = bar.build() {
                                window.paint_path(path, ink);
                            }
                        }
                        AvatarStatusVariant::Success => {}
                    }
                },
            )
            .size_full()
        });
        let el = div()
            .id(self.id)
            .flex()
            .items_center()
            .justify_center()
            .size(px(dot))
            .rounded_full()
            .border(px(border))
            .border_color(t.color.background_surface)
            .bg(bg)
            .text_color(ink)
            .children(glyph)
            .children(
                self.icon
                    .filter(|_| icon_size > 0.)
                    .map(|icon| div().size(px(icon_size)).child(icon)),
            );
        match self.label {
            Some(label) => el.role(Role::Status).aria_label(label).into_any_element(),
            None => el.into_any_element(),
        }
    }
}

/// Overlapping avatars with an optional "+N" chip, upstream `AvatarGroup`.
#[derive(IntoElement)]
pub struct AvatarGroup {
    id: ElementId,
    size: AvatarSize,
    shape: AvatarShape,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 6]>,
}

impl AvatarGroup {
    pub fn new(id: impl Into<ElementId>) -> Self {
        AvatarGroup {
            id: id.into(),
            size: AvatarSize::Md,
            shape: AvatarShape::Circle,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `size`: applied to every avatar.
        size: AvatarSize,
        /// Upstream `shape`: applied to every avatar, over their own.
        shape: AvatarShape,
    }
}

impl Styled for AvatarGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for AvatarGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// Upstream `OVERLAP_RATIO`.
const OVERLAP_RATIO: f32 = 0.25;

impl RenderOnce for AvatarGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let dir = use_direction(cx);
        let overlap = px((self.size.pixels() * OVERLAP_RATIO).round());
        let context = GroupContext {
            size: self.size,
            shape: self.shape,
        };
        // FR4: link and button avatars and a clickable overflow form one
        // horizontal roving set with a single tab stop: arrows follow the
        // rendered direction and wrap, Home and End jump to the ends.
        let scope = RovingScope::begin(
            &self.id,
            ListKeys::horizontal()
                .wrap(true)
                .home_end(true)
                .rtl(dir.is_rtl()),
            window,
            cx,
        );
        let interactive = scope.has_controls();
        let mut el = div().flex_row_logical(dir).inline_flex().items_center();
        theme.refine(el.style(), "avatar-group", &[], None);
        el.style().refine(&self.style);
        let label = i18n(cx).t("@astryx.avatarGroup.label");
        let mut el = el
            .id(self.id)
            .role(Role::Group)
            .aria_label(label)
            .on_key_down(scope.key_down_listener());
        // FR5: while a member is interactive, the group describes its keys.
        if interactive {
            el = el.aria_description(i18n(cx).t("@astryx.avatarGroup.keyboardHint"));
        }
        let scope_for_children = scope.clone();
        el.children(
            self.children
                .into_iter()
                .enumerate()
                .map(move |(i, child)| {
                    let slot = div().flex_none();
                    let slot = if i == 0 {
                        slot
                    } else {
                        slot.margin_start(dir, -overlap)
                    };
                    slot.child(provide(scope_for_children.clone(), provide(context, child)))
                }),
        )
    }
}

/// The "+N" chip that ends an [`AvatarGroup`], upstream `AvatarGroupOverflow`.
#[derive(IntoElement)]
pub struct AvatarGroupOverflow {
    count: usize,
    children: SmallVec<[AnyElement; 1]>,
    on_click: Option<OverflowClick>,
    style: StyleRefinement,
}

type OverflowClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

impl AvatarGroupOverflow {
    /// Upstream `count`: how many avatars are not shown.
    pub fn new(count: usize) -> Self {
        AvatarGroupOverflow {
            count,
            children: SmallVec::new(),
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `onClick`: the indicator becomes a button, in its group's
    /// roving set.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl ParentElement for AvatarGroupOverflow {
    /// Upstream `children`: count text in place of "+N".
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for AvatarGroupOverflow {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for AvatarGroupOverflow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let group = use_context::<GroupContext>(cx);
        let size: Pixels = px(group.map_or(AvatarSize::Md, |g| g.size).pixels());
        let shape = group.map_or(AvatarShape::Circle, |g| g.shape);
        let rem = window.rem_size();
        let radius = shape.radius(t);
        let label = i18n(cx).format(
            "@astryx.avatarGroup.overflow",
            &[("count", rustryx_i18n::Arg::Num(self.count as f64))],
        );
        let mut el = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(size)
            .min_w(size)
            .px(t.spacing.s2)
            .rounded(radius)
            .border_2()
            .border_color(t.color.background_surface)
            .bg(t.color.neutral)
            .text_color(t.color.text_secondary)
            .text_style(&t.text.supporting)
            .font_weight(t.font.weight.medium);
        el.style().refine(&self.style);
        let el = if self.children.is_empty() {
            el.child(format!("+{}", self.count))
        } else {
            el.children(self.children)
        };
        match self.on_click {
            None => el.into_any_element(),
            Some(on_click) => {
                let id = ElementId::Name("avatar-group-overflow".into());
                let focus = control_focus(&id, window, cx);
                let ring = is_focus_visible(&focus, window)
                    .then(|| crate::focus::focus_ring(t, radius.to_pixels(rem), rem));
                let el = el
                    .id(id)
                    .track_focus(&focus)
                    .role(Role::Button)
                    .aria_label(label)
                    .cursor_pointer();
                // A finger's press paints as UIKit tracks a control (upstream
                // `usePressFeedback`).
                crate::press::touch_press(
                    el,
                    ElementId::Name("avatar-group-overflow-press".into()),
                    false,
                    gpui::transparent_black(),
                    t.color.overlay_pressed,
                    window,
                    cx,
                )
                .on_click(on_click)
                .children(ring)
                .into_any_element()
            }
        }
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const NO_LINK_COMPONENT: &str = "the link handler (link_provider) routes every href";

pub const AVATAR_DOC: ComponentDoc = ComponentDoc {
    name: "Avatar",
    rust_type: "rustryx::components::people::Avatar",
    props: &[
        p("src", RustApi::Method("src")),
        p("fallbackSrc", RustApi::Method("fallback_src")),
        p("name", RustApi::Method("name")),
        p("alt", RustApi::Method("alt")),
        p("size", RustApi::Method("size")),
        p("shape", RustApi::Method("shape")),
        p("status", RustApi::Method("status")),
        p("tooltip", RustApi::Method("tooltip")),
        p("href", RustApi::Method("href")),
        p("as", RustApi::Waived(NO_LINK_COMPONENT)),
        p("target", RustApi::Method("target")),
        p("rel", RustApi::Waived("GPUI opens URLs without a referrer")),
        p("onClick", RustApi::Method("on_click")),
    ],
    example: include_str!("examples.rs"),
};

pub const AVATAR_GROUP_DOC: ComponentDoc = ComponentDoc {
    name: "AvatarGroup",
    rust_type: "rustryx::components::people::AvatarGroup",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("size", RustApi::Method("size")),
        p("shape", RustApi::Method("shape")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("xstyle", RustApi::Trait("Styled")),
        p(
            "data-testid",
            RustApi::Waived("GPUI tests find elements by id"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const AVATAR_GROUP_OVERFLOW_DOC: ComponentDoc = ComponentDoc {
    name: "AvatarGroupOverflow",
    rust_type: "rustryx::components::people::AvatarGroupOverflow",
    props: &[
        p("count", RustApi::Argument("count")),
        p("children", RustApi::Trait("ParentElement")),
        p("onClick", RustApi::Method("on_click")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

#[cfg(test)]
mod group_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Context, Render, TestAppContext, VisualTestContext};

    use super::*;
    use crate::button::Button;

    type Log = Rc<RefCell<Vec<&'static str>>>;

    struct Host {
        log: Log,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let press = |name: &'static str, log: &Log| {
                let log = log.clone();
                move |_: &ClickEvent, _: &mut Window, _: &mut App| log.borrow_mut().push(name)
            };
            div()
                .p(px(20.))
                .flex()
                .gap(px(20.))
                .child(Button::new("before", "Before"))
                .child(
                    AvatarGroup::new("team")
                        .child(
                            Avatar::new("ada")
                                .name("Ada Lovelace")
                                .on_click(press("ada", &self.log)),
                        )
                        .child(
                            Avatar::new("grace")
                                .name("Grace Hopper")
                                .on_click(press("grace", &self.log)),
                        )
                        .child(Avatar::new("plain").name("Alan Turing"))
                        .child(AvatarGroupOverflow::new(4).on_click(press("more", &self.log))),
                )
                .child(Button::new("after", "After"))
        }
    }

    fn host(cx: &mut TestAppContext) -> (&mut VisualTestContext, Log) {
        cx.update(|cx| {
            rustryx_theme::init(cx);
            rustryx_primitives::focus::init(cx);
        });
        let log: Log = Rc::default();
        let l = log.clone();
        let (_, cx) = cx.add_window_view(move |_, _| Host { log: l });
        cx.update(|window, _| window.activate_window());
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        (cx, log)
    }

    /// Each key down and up, as a person presses it (GPUI activates on
    /// Enter's release).
    fn press(cx: &mut VisualTestContext, keys: &'static str) {
        for key in keys.split(' ') {
            let keystroke = gpui::Keystroke::parse(key).unwrap();
            cx.simulate_keystrokes(key);
            cx.simulate_event(gpui::KeyUpEvent { keystroke });
            for _ in 0..2 {
                cx.update(|window, _| window.refresh());
                cx.run_until_parked();
            }
        }
    }

    #[gpui::test]
    fn fr4_interactive_members_share_one_tab_stop_and_the_arrows(cx: &mut TestAppContext) {
        let (cx, log) = host(cx);
        // Before, then the group's one stop (Ada), then After.
        press(cx, "tab tab enter");
        assert_eq!(*log.borrow(), ["ada"]);
        press(cx, "right enter");
        assert_eq!(log.borrow().last(), Some(&"grace"));
        // The static avatar is skipped: the overflow button is next.
        press(cx, "right enter");
        assert_eq!(log.borrow().last(), Some(&"more"));
        // The arrows wrap; Home and End reach the ends.
        press(cx, "right enter");
        assert_eq!(log.borrow().last(), Some(&"ada"));
        press(cx, "end enter");
        assert_eq!(log.borrow().last(), Some(&"more"));
        press(cx, "home enter");
        assert_eq!(log.borrow().last(), Some(&"ada"));
        // One tab stop: Tab leaves the group.
        press(cx, "tab");
        let before = log.borrow().len();
        press(cx, "enter");
        assert_eq!(
            log.borrow().len(),
            before,
            "focus is on After, outside the group"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_take_first_and_last_words() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("Grace Brewster Murray Hopper"), "GH");
        assert_eq!(initials("cher"), "C");
        assert_eq!(initials("  "), "");
        assert_eq!(initials("Émile Zola"), "ÉZ");
    }

    #[test]
    fn sizes_and_status_tiers() {
        assert_eq!(AvatarSize::Md.pixels(), 36.);
        assert_eq!(AvatarSize::Px(40.).pixels(), 40.);
        assert_eq!(status_tier(36.).0, 10.);
        assert_eq!(status_tier(48.).0, 20.);
        assert_eq!(status_tier(128.).0, 32.);
    }
}
