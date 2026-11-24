//! People: Avatar (AvatarStatusDot) and AvatarGroup (AvatarGroupOverflow).
//! Upstream: `Avatar/`, `AvatarGroup/`.

pub mod examples;

use std::f32::consts::SQRT_2;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{provide, use_context, use_link_handler};
use astryx_theme::{ThemeStyled, Tokens, current};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, Hsla, ImageSource, IntoElement, ObjectFit,
    ParentElement, PathBuilder, Pixels, RenderOnce, Role, SharedString, StyleRefinement, Styled,
    StyledImage, Window, canvas, div, img, point, prelude::*, px,
};
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

    astryx_primitives::setters! {
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
        /// Upstream `tooltip`. The tooltip itself lands with Tooltip (M5).
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
            .id(self.id)
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
        if let Some(href) = self.href {
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
        }
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

    astryx_primitives::setters! {
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
        let mut el = div().flex_row_logical(dir).inline_flex().items_center();
        theme.refine(el.style(), "avatar-group", &[], None);
        el.style().refine(&self.style);
        let label = i18n(cx).t("@astryx.avatarGroup.label");
        el.id(self.id).role(Role::Group).aria_label(label).children(
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
                    slot.child(provide(context, child))
                }),
        )
    }
}

/// The "+N" chip that ends an [`AvatarGroup`], upstream `AvatarGroupOverflow`.
#[derive(IntoElement)]
pub struct AvatarGroupOverflow {
    count: usize,
}

impl AvatarGroupOverflow {
    /// Upstream `count`: how many avatars are not shown.
    pub fn new(count: usize) -> Self {
        AvatarGroupOverflow { count }
    }
}

impl RenderOnce for AvatarGroupOverflow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let group = use_context::<GroupContext>(cx);
        let size: Pixels = px(group.map_or(AvatarSize::Md, |g| g.size).pixels());
        let shape = group.map_or(AvatarShape::Circle, |g| g.shape);
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(size)
            .min_w(size)
            .px(t.spacing.s2)
            .rounded(shape.radius(t))
            .border_2()
            .border_color(t.color.background_surface)
            .bg(t.color.neutral)
            .text_color(t.color.text_secondary)
            .text_style(&t.text.supporting)
            .font_weight(t.font.weight.medium)
            .child(format!("+{}", self.count))
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const TOOLTIP: &str = "the tooltip lands with Tooltip in M5";
const NO_LINK_COMPONENT: &str = "the link handler (link_provider) routes every href";

pub const AVATAR_DOC: ComponentDoc = ComponentDoc {
    name: "Avatar",
    rust_type: "astryx::components::people::Avatar",
    props: &[
        p("src", RustApi::Method("src")),
        p("fallbackSrc", RustApi::Method("fallback_src")),
        p("name", RustApi::Method("name")),
        p("alt", RustApi::Method("alt")),
        p("size", RustApi::Method("size")),
        p("shape", RustApi::Method("shape")),
        p("status", RustApi::Method("status")),
        p("tooltip", RustApi::Pending("tooltip", TOOLTIP)),
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
    rust_type: "astryx::components::people::AvatarGroup",
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
