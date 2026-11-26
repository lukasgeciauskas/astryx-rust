//! Media: Thumbnail. Upstream: `Thumbnail/`.

pub mod examples;

use std::rc::Rc;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{ElementSize, control_focus, is_focus_visible};
use astryx_theme::current;
use gpui::{
    App, ClickEvent, ElementId, ImageSource, IntoElement, ObjectFit, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, StyledImage, Window, div, img, prelude::*, px, svg,
};

use crate::button::{ButtonVariant, IconButton};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconSize};
use crate::status::{Skeleton, SkeletonRadius, Spinner, SpinnerShade, SpinnerSize};

/// Upstream's placeholder glyph: a picture frame.
const PICTURE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><path d="M21 19V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2M8.5 13.5l2.5 3 3.5-4.5 4.5 6H5l3.5-5.5z"/></svg>"#;

/// Upstream `showRemoveOn`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShowRemoveOn {
    Always,
    /// On hover or focus. The default.
    #[default]
    Hover,
}

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A square preview of an image attachment, upstream `Thumbnail`.
#[derive(IntoElement)]
pub struct Thumbnail {
    id: ElementId,
    src: Option<SharedString>,
    alt: Option<SharedString>,
    label: Option<SharedString>,
    on_remove: Option<ClickFn>,
    on_click: Option<ClickFn>,
    loading: bool,
    disabled: bool,
    show_remove_on: ShowRemoveOn,
    style: StyleRefinement,
}

impl Thumbnail {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Thumbnail {
            id: id.into(),
            src: None,
            alt: None,
            label: None,
            on_remove: None,
            on_click: None,
            loading: false,
            disabled: false,
            show_remove_on: ShowRemoveOn::Hover,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `src`.
        ?src: SharedString,
        /// Upstream `alt`: what the image shows. Without it the image is decorative.
        ?alt: SharedString,
        /// Upstream `label`: the file name, its accessible name.
        ?label: SharedString,
        /// Upstream `isLoading`: a skeleton without `src`, an upload overlay with one.
        !loading,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `showRemoveOn`.
        show_remove_on: ShowRemoveOn,
    }

    /// Upstream `onRemove`: shows a remove button over the corner.
    pub fn on_remove(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_remove = Some(Rc::new(f));
        self
    }

    /// Upstream `onClick`: makes the thumbnail a button.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Styled for Thumbnail {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Thumbnail {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tooltip_id = self.id.clone();
        let tooltip = self.label.clone();
        let theme = current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let rem = window.rem_size();
        let i18n = i18n(cx);
        let radius = t.radius.element;
        let name: SharedString = match (&self.label, &self.alt) {
            (Some(label), Some(alt)) => i18n.format(
                "@astryx.thumbnail.nameWithAlt",
                &[
                    ("label", label.to_string().into()),
                    ("alt", alt.to_string().into()),
                ],
            ),
            (Some(label), None) => label.clone(),
            (None, Some(alt)) => alt.clone(),
            (None, None) => i18n.t("@astryx.thumbnail.fallbackName"),
        };
        let has_src = self.src.is_some();
        let skeleton = self.loading && !has_src;

        let placeholder = {
            let icon_color = t.color.icon_secondary;
            move || {
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size_full()
                    .child(svg().data(PICTURE_SVG).size(px(24.)).text_color(icon_color))
                    .into_any_element()
            }
        };
        let content = if skeleton {
            Skeleton::new()
                .radius(SkeletonRadius::R2)
                .into_any_element()
        } else if let Some(src) = self.src.clone() {
            img(ImageSource::from(src))
                .size_full()
                .object_fit(ObjectFit::Cover)
                .with_fallback(placeholder)
                .into_any_element()
        } else {
            placeholder()
        };

        let group: SharedString = format!("thumbnail-{:?}", self.id).into();
        let mut el = div()
            .id(self.id.clone())
            .group(group.clone())
            .inline_flex()
            .relative()
            .flex_none()
            .size(px(64.))
            .rounded(radius)
            .bg(t.color.neutral)
            .child(
                div()
                    .size_full()
                    .rounded(radius)
                    .overflow_hidden()
                    .child(content),
            )
            // Upstream's inset 1px border, drawn over the image.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .rounded(radius)
                    .border_1()
                    .border_color(t.color.border),
            );
        if self.loading && has_src {
            el = el.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius)
                    .bg(t.color.overlay)
                    .child(
                        Spinner::new("upload")
                            .size(SpinnerSize::Md)
                            .shade(SpinnerShade::OnMedia),
                    ),
            );
        }
        if self.disabled {
            el = el.opacity(0.5);
        }
        el.style().refine(&self.style);
        theme.refine(el.style(), "thumbnail", &[], None);

        let el = el.role(if self.on_click.is_some() {
            Role::Button
        } else {
            Role::Image
        });
        let el = el.aria_label(name.clone());
        let el = match (self.on_click, self.disabled) {
            (Some(on_click), false) => {
                let handle = control_focus(&ElementId::Name(group.clone()), window, cx);
                let ring = is_focus_visible(&handle, window)
                    .then(|| focus_ring(t, radius.to_pixels(rem), rem));
                crate::press::touch_press(
                    el,
                    ElementId::Name(format!("{group}-press").into()),
                    false,
                    gpui::transparent_black(),
                    t.color.overlay_pressed,
                    window,
                    cx,
                )
                .track_focus(&handle)
                .cursor_pointer()
                .hover(|s| s.bg(t.color.overlay_hover))
                .on_click(move |event, window, cx| on_click(event, window, cx))
                .children(ring)
            }
            _ => el,
        };
        let remove = self.on_remove.filter(|_| !self.disabled).map(|on_remove| {
            let label = i18n.format(
                "@astryx.thumbnail.remove",
                &[("accessibleName", name.to_string().into())],
            );
            // Upstream `removeButtonOverrides`: a 20 px secondary button on
            // the image's dark overlay, its corner nested inside the thumbnail's.
            let radius = t.radius.element.to_pixels(rem) - t.spacing.s1.to_pixels(rem);
            let button = IconButton::new(
                ElementId::Name(format!("{group}-remove").into()),
                label,
                Icon::new("close").size(IconSize::Xsm),
            )
            .variant(ButtonVariant::Secondary)
            .size(ElementSize::Sm)
            .on_click(move |event, window, cx| {
                cx.stop_propagation();
                on_remove(event, window, cx)
            })
            .h(px(20.))
            .min_w(px(20.))
            .rounded(radius)
            .bg(t.color.overlay)
            .text_color(t.color.on_dark);
            let slot = div()
                .absolute()
                .top(t.spacing.s1)
                .inset_end(dir, t.spacing.s1)
                .child(button);
            match self.show_remove_on {
                ShowRemoveOn::Always => slot,
                ShowRemoveOn::Hover => slot
                    .opacity(0.)
                    .group_hover(group.clone(), |s| s.opacity(1.)),
            }
        });
        // Upstream: the label shows on hover.
        crate::overlay::with_tooltip(&tooltip_id, tooltip, false, el.children(remove))
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const THUMBNAIL_DOC: ComponentDoc = ComponentDoc {
    name: "Thumbnail",
    rust_type: "astryx::components::media::Thumbnail",
    props: &[
        p("src", RustApi::Method("src")),
        p("alt", RustApi::Method("alt")),
        p("label", RustApi::Method("label")),
        p("onRemove", RustApi::Method("on_remove")),
        p("onClick", RustApi::Method("on_click")),
        p("isLoading", RustApi::Method("loading")),
        p("isDisabled", RustApi::Method("disabled")),
        p("showRemoveOn", RustApi::Method("show_remove_on")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
        p(
            "data-testid",
            RustApi::Waived("GPUI tests find elements by id"),
        ),
    ],
    example: include_str!("examples.rs"),
};
