//! `DialogHeroHeader`: the high-emphasis header for a Dialog (upstream lab
//! `DialogHeroHeader`). A full-bleed media slot sits above the title, and the close
//! button floats over the media's top-trailing corner.
//!
//! - The media bleeds to the dialog's edges by cancelling the header's published
//!   container padding (`ContainerPadding`, upstream's `--container-padding-*`) with
//!   negative margins on its inline-start, inline-end and block-start edges, so it follows
//!   the theme's and the dialog's padding.
//! - `media_mode` names the media's luminance; the close button sits in a [`MediaTheme`]
//!   of that mode, so its glyph and focus ring read on dark or light media.
//! - In a modal Dialog the title names the dialog, shown or hidden; a visible title is
//!   also where focus enters by default (a control that asks for focus wins). A hidden
//!   title, an inline dialog and a header outside any dialog take no focus, and an
//!   inline dialog, a plain preview upstream, takes no name either.

pub mod examples;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, SharedString, StyleRefinement, Window, div};
use rustryx_components::button::{ButtonVariant, IconButton};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_components::icon::{Icon, IconColor};
use rustryx_components::layout::{ContainerPadding, LayoutHeader, VisuallyHidden};
use rustryx_components::overlay::focus_scope;
use rustryx_components::text::{Heading, HeadingLevel};
use rustryx_components::theming::{MediaThemeMode, media_theme};
use rustryx_i18n::{LogicalStyled, current as i18n, use_direction};
use rustryx_primitives::use_context;

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// The media surface's luminance (upstream `DialogHeroHeaderMediaMode`), forwarded to
/// the [`MediaTheme`] around content drawn over the media (the close button).
///
/// [`MediaTheme`]: rustryx_components::theming::MediaTheme
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogHeroHeaderMediaMode {
    /// Light media: dark overlay content.
    Light,
    /// Dark media: light overlay content.
    Dark,
}

/// The MediaTheme mode for a `media_mode` (upstream `mediaMode ?? 'off'`): no media
/// context keeps the ambient theme.
pub fn media_theme_mode(mode: Option<DialogHeroHeaderMediaMode>) -> MediaThemeMode {
    match mode {
        Some(DialogHeroHeaderMediaMode::Light) => MediaThemeMode::Light,
        Some(DialogHeroHeaderMediaMode::Dark) => MediaThemeMode::Dark,
        None => MediaThemeMode::Off,
    }
}

/// Whether the title takes the dialog's initial focus (upstream `shouldAutoFocus`):
/// only a visible title in a modal (not inline) dialog.
pub fn requests_title_focus(in_modal_dialog: bool, title_hidden: bool) -> bool {
    in_modal_dialog && !title_hidden
}

/// Hero-style header for a Dialog (upstream lab `DialogHeroHeader`): the
/// high-emphasis sibling of `DialogHeader`, a LayoutHeader that drops into a Layout's
/// header slot.
///
/// ```ignore
/// Dialog::new("welcome", open).on_open_change(set_open).child(
///     Layout::new()
///         .header(
///             DialogHeroHeader::new("Welcome aboard", hero_image)
///                 .media_mode(DialogHeroHeaderMediaMode::Dark)
///                 .on_open_change(set_open),
///         )
///         .content(LayoutContent::new("body").child(..)),
/// )
/// ```
#[derive(IntoElement)]
pub struct DialogHeroHeader {
    title: SharedString,
    title_element: Option<AnyElement>,
    media: AnyElement,
    media_mode: Option<DialogHeroHeaderMediaMode>,
    title_hidden: bool,
    start_content: Option<AnyElement>,
    max_lines: Option<usize>,
    on_open_change: Option<OpenFn>,
    divider: Option<bool>,
    style: StyleRefinement,
}

impl DialogHeroHeader {
    /// A header titled `title` (upstream `title`: drawn as a level-2 Heading, and the
    /// dialog's name) over `media` (upstream `media`: the full-bleed visual; size it
    /// to fill the slot, full width for an image).
    pub fn new(title: impl Into<SharedString>, media: impl IntoElement) -> Self {
        DialogHeroHeader {
            title: title.into(),
            title_element: None,
            media: media.into_any_element(),
            media_mode: None,
            title_hidden: false,
            start_content: None,
            max_lines: None,
            on_open_change: None,
            divider: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `title` as an element (a pre-styled Heading): drawn in place of the
    /// level-2 Heading, taking the focus a visible title takes. The `title` text still
    /// names the dialog, since GPUI cannot read an element's text.
    pub fn title_element(mut self, heading: impl IntoElement) -> Self {
        self.title_element = Some(heading.into_any_element());
        self
    }

    /// Upstream `mediaMode`: the media's luminance, so the close button over it reads;
    /// unset keeps the ambient theme.
    pub fn media_mode(mut self, mode: DialogHeroHeaderMediaMode) -> Self {
        self.media_mode = Some(mode);
        self
    }

    /// Upstream `isTitleHidden`: hides the title row visually, keeping it for screen
    /// readers.
    pub fn title_hidden(mut self, hidden: bool) -> Self {
        self.title_hidden = hidden;
        self
    }

    /// Upstream `startContent`: before the title (an icon), inline with it and outside
    /// the dialog's name.
    pub fn start_content(mut self, content: impl IntoElement) -> Self {
        self.start_content = Some(content.into_any_element());
        self
    }

    /// Upstream `maxLines`: lines before a text title truncates with an ellipsis; a
    /// `title_element` ignores it.
    pub fn max_lines(mut self, lines: usize) -> Self {
        self.max_lines = Some(lines);
        self
    }

    /// Upstream `onOpenChange`: a close button over the media that asks to close
    /// (`false`); without it there is no close button.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `hasDivider`: a border at the bottom edge; unset, the Layout's
    /// `default_has_dividers` decides.
    pub fn divider(mut self, divider: bool) -> Self {
        self.divider = Some(divider);
        self
    }
}

impl Styled for DialogHeroHeader {
    /// Upstream `xstyle`: the LayoutHeader root.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The media slot: it reads the padding the header publishes around its children, so it
/// renders as their child.
#[derive(IntoElement)]
struct MediaArea {
    media: AnyElement,
    close: Option<AnyElement>,
}

impl RenderOnce for MediaArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        let padding = use_context::<ContainerPadding>(cx).unwrap_or_default();
        let close = self.close.map(|close| {
            // Over the media's top-trailing corner (logical, so it stays trailing in RTL).
            div()
                .absolute()
                .top(t.spacing.s2)
                .inset_end(dir, t.spacing.s2)
                .debug_selector(|| "dialog-hero-header-close".into())
                .child(close)
        });
        div()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .margin_start(dir, -padding.inline_start)
            .margin_end(dir, -padding.inline_end)
            .mt(-padding.block_start)
            .debug_selector(|| "dialog-hero-header-media".into())
            .child(self.media)
            .children(close)
    }
}

impl RenderOnce for DialogHeroHeader {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let dir = use_direction(cx);
        // A modal dialog collects its initial focus and its name (upstream's dialog
        // context with `isInline === false`); inline dialogs and the page provide none.
        let scope = focus_scope(cx);
        let auto_focus = requests_title_focus(scope.is_some(), self.title_hidden);
        let title_focus = window
            .use_keyed_state(
                ElementId::Name("dialog-hero-header-title-focus".into()),
                cx,
                |_, cx| cx.focus_handle().tab_stop(false),
            )
            .read(cx)
            .clone();
        // The title names the dialog (upstream's `titleId`) whether or not it is shown;
        // only a visible one takes the initial focus.
        if let Some(scope) = scope {
            if auto_focus {
                scope.title(title_focus.clone(), self.title.clone());
            } else {
                scope.name(self.title.clone());
            }
        }

        let close = self.on_open_change.map(|f| {
            let label = i18n(cx).t("@astryx.dialog.close");
            media_theme(
                media_theme_mode(self.media_mode),
                IconButton::new(
                    "dialog-hero-header-close",
                    label.clone(),
                    Icon::new("close").color(IconColor::Inherit),
                )
                .variant(ButtonVariant::Ghost)
                .tooltip(label)
                .on_click(move |_, window, cx| f(false, window, cx)),
            )
            .into_any_element()
        });

        // The title: a level-2 Heading for text, else the caller's heading as it is. The
        // focus a visible title takes lands on its own box, never on the start content.
        let mut title = div()
            .id(ElementId::Name("dialog-hero-header-title".into()))
            .min_w_0()
            .debug_selector(|| "dialog-hero-header-title".into());
        if auto_focus {
            title = title.track_focus(&title_focus);
        }
        let title = match self.title_element {
            Some(heading) => title.child(heading),
            None => {
                let mut heading = Heading::new(HeadingLevel::H2, self.title.clone());
                if let Some(lines) = self.max_lines {
                    heading = heading.max_lines(lines);
                }
                title.child(heading)
            }
        };
        let title_row = div()
            .flex()
            .flex_row_logical(dir)
            .items_center()
            .gap(t.spacing.s3)
            .mt(t.spacing.s4)
            .debug_selector(|| "dialog-hero-header-title-row".into())
            .children(self.start_content.map(|content| {
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .debug_selector(|| "dialog-hero-header-start".into())
                    .child(content)
            }))
            .child(title);
        let title_row = if self.title_hidden {
            VisuallyHidden::new().child(title_row).into_any_element()
        } else {
            title_row.into_any_element()
        };

        let mut header = LayoutHeader::new();
        if let Some(divider) = self.divider {
            header = header.has_divider(divider);
        }
        header.style().refine(&self.style);
        header
            .child(MediaArea {
                media: self.media,
                close,
            })
            .child(title_row)
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `DialogHeroHeader`.
pub const DIALOG_HERO_HEADER_DOC: ComponentDoc = ComponentDoc {
    name: "DialogHeroHeader",
    rust_type: "rustryx::lab::dialog_hero_header::DialogHeroHeader",
    props: &[
        p(
            "title",
            RustApi::Argument("title (text; an element through title_element)"),
        ),
        p("media", RustApi::Argument("media")),
        p("mediaMode", RustApi::Method("media_mode")),
        p("isTitleHidden", RustApi::Method("title_hidden")),
        p("startContent", RustApi::Method("start_content")),
        p("maxLines", RustApi::Method("max_lines")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("hasDivider", RustApi::Method("divider")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
