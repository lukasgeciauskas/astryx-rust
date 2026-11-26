//! Dialog and DialogHeader, upstream `Dialog/`: a modal surface that blocks
//! the page until the user responds.
//!
//! - Modal: a dim backdrop (`--color-overlay`, no blur) covers the window
//!   and takes every press meant for the page; the surface is a focus region
//!   (fork patch R12), so Tab and Shift-Tab wrap inside it (FR7).
//! - Focus enters right after the surface is first drawn (FR1): a control
//!   that asked for it ([`AutoFocus`], or one that focused itself), else the
//!   DialogHeader's title, else the first control, else the surface (FR2,
//!   FR3). Closing returns focus that would be lost to what had it when the
//!   dialog opened (FR4).
//! - `purpose`: `Info` closes on Escape and a backdrop press, `Form` on Escape
//!   only, `Required` on neither (an alert dialog).
//! - Standard dialogs are 400px wide by default, clamped to the viewport less
//!   `spacing-4` gutters, at most 75% of its height, centered unless placed;
//!   `Fullscreen` covers the window. The surface enters with a fade and grow
//!   from the side its opener was on.
//! - Inline dialogs render in the flow, with no modality (FR5).
//!
//! The surface is a padding container (`spacing-4` by default): it pads plain
//! children, and a Layout inside escapes that padding and insets its regions by
//! it, as upstream's Dialog with its Layout does. DialogHeader is a LayoutHeader.

use std::cell::RefCell;
use std::rc::Rc;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{prefers_reduced_motion, provide, use_context};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, Entity, FocusHandle, IntoElement, Length, MouseButton,
    ParentElement, Pixels, RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, div,
    prelude::*, px,
};
use smallvec::SmallVec;

use crate::button::{ButtonVariant, IconButton};
use crate::icon::Icon;
use crate::layout::SpacingStep;
use crate::text::{Heading, HeadingLevel, Text, TextSize, TextType};

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// Upstream `variant`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogVariant {
    #[default]
    Standard,
    /// Covers the whole window.
    Fullscreen,
}

/// Upstream `purpose`: how the dialog may be dismissed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogPurpose {
    /// Escape and a press on the backdrop close it.
    #[default]
    Info,
    /// Escape closes it; a backdrop press does not, so input is never lost.
    Form,
    /// Neither: the user must answer (an alert dialog).
    Required,
}

/// Upstream `position`: offsets from the window's edges; an axis with
/// neither offset is centered.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DialogPosition {
    pub top: Option<Pixels>,
    pub bottom: Option<Pixels>,
    /// From the reading direction's start edge.
    pub start: Option<Pixels>,
    pub end: Option<Pixels>,
}

/// Marks an element whose control asks for the initial focus of the layer
/// it is in, upstream `data-autofocus`. Button honors it.
#[derive(Clone, Copy)]
pub struct AutoFocus;

/// Collects the requests for a focus-taking layer's initial focus.
#[derive(Clone, Default)]
pub(crate) struct FocusScope(Rc<FocusRequests>);

#[derive(Default)]
pub(crate) struct FocusRequests {
    /// A control marked with [`AutoFocus`].
    requested: RefCell<Option<FocusHandle>>,
    /// The DialogHeader's title, the default, with its text (the dialog's
    /// name).
    title: RefCell<Option<(FocusHandle, SharedString)>>,
}

impl FocusScope {
    /// A control asks for the initial focus.
    pub(crate) fn request(&self, focus: FocusHandle) {
        *self.0.requested.borrow_mut() = Some(focus);
    }

    fn title(&self, focus: FocusHandle, text: SharedString) {
        *self.0.title.borrow_mut() = Some((focus, text));
    }

    pub(crate) fn requested(&self) -> Option<FocusHandle> {
        self.0.requested.borrow().clone()
    }

    pub(crate) fn title_focus(&self) -> Option<FocusHandle> {
        self.0.title.borrow().as_ref().map(|(f, _)| f.clone())
    }

    pub(crate) fn title_text(&self) -> Option<SharedString> {
        self.0.title.borrow().as_ref().map(|(_, t)| t.clone())
    }
}

/// The focus scope of the layer being rendered, if any.
pub(crate) fn focus_scope(cx: &App) -> Option<FocusScope> {
    use_context::<FocusScope>(cx)
}

/// A modal surface that blocks the page until the user responds, upstream
/// `Dialog`.
///
/// ```ignore
/// Dialog::new("rename", open)
///     .on_open_change(|open, _, cx| ...)
///     .child(
///         Layout::new()
///             .header(DialogHeader::new("Rename file").on_open_change(...))
///             .content(LayoutContent::new("body").child(...)),
///     )
///     .child(TextInput::new("name", "Name", value))
/// ```
#[derive(IntoElement)]
pub struct Dialog {
    id: ElementId,
    open: bool,
    on_open_change: Option<OpenFn>,
    children: SmallVec<[AnyElement; 2]>,
    width: Option<Length>,
    max_height: Option<Length>,
    position: Option<DialogPosition>,
    variant: DialogVariant,
    purpose: DialogPurpose,
    padding: Option<SpacingStep>,
    inline: bool,
    label: Option<SharedString>,
    description: Option<SharedString>,
    alert: bool,
    style: StyleRefinement,
}

impl Dialog {
    /// Upstream `isOpen`: a dialog is always controlled.
    pub fn new(id: impl Into<ElementId>, open: bool) -> Self {
        Dialog {
            id: id.into(),
            open,
            on_open_change: None,
            children: SmallVec::new(),
            width: None,
            max_height: None,
            position: None,
            variant: DialogVariant::Standard,
            purpose: DialogPurpose::Info,
            padding: None,
            inline: false,
            label: None,
            description: None,
            alert: false,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `width`: 400px by default, clamped to the viewport.
        ?width: Length,
        /// Upstream `maxHeight`: 75% of the viewport by default.
        ?max_height: Length,
        /// Upstream `position`: centered when unset.
        ?position: DialogPosition,
        /// Upstream `variant`.
        variant: DialogVariant,
        /// Upstream `purpose`.
        purpose: DialogPurpose,
        /// Upstream `padding`: a spacing step; `spacing-4` by default.
        ?padding: SpacingStep,
        /// Upstream `isInline`: in the flow, with no modality (previews).
        !inline,
        /// Upstream `aria-label`: else the DialogHeader's title names it.
        ?label: SharedString,
    }

    /// Upstream `onOpenChange`: asked to close (Escape, a backdrop press, a
    /// header's close button), per `purpose`.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `aria-describedby`'s text.
    pub(crate) fn description(mut self, description: SharedString) -> Self {
        self.description = Some(description);
        self
    }

    /// Exposes it as an alert dialog.
    pub(crate) fn alert(mut self) -> Self {
        self.alert = true;
        self
    }
}

impl ParentElement for Dialog {
    /// Upstream `children`: the content.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Dialog {
    /// Upstream `xstyle`: the surface.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        let fullscreen = self.variant == DialogVariant::Fullscreen;
        let padding = match (self.padding, fullscreen) {
            (Some(step), _) => step.token(t).to_pixels(rem),
            (None, _) => t.spacing.s4.to_pixels(rem),
        };

        if self.inline {
            let mut surface = div()
                .id(self.id.clone())
                .flex()
                .flex_col()
                .w(self.width.unwrap_or(px(400.).into()))
                .max_w_full()
                .p(padding)
                .bg(t.color.background_surface)
                .text_color(t.color.text_primary)
                .rounded(t.radius.container)
                .shadow_token(&t.shadow.high)
                .font_stack(&t.font.family.body)
                .text_style(&t.text.body)
                .role(Role::Group)
                .child(crate::layout::padding_container(
                    all(padding),
                    Some(crate::layout::LayoutContainerKind::Dialog),
                    crate::layout::host_children(
                        self.children,
                        all(padding),
                        crate::layout::ContainerCorners::all(t.radius.container.to_pixels(rem)),
                    ),
                    cx,
                ));
            theme.refine(surface.style(), "dialog", &[], None);
            surface.style().refine(&self.style);
            return surface.into_any_element();
        }

        let state = super::modal::modal_state(&self.id, window, cx);
        super::modal::sync_open(&state, self.open, window, cx);
        if !self.open {
            return div()
                .id(self.id.clone())
                .display_contents()
                .into_any_element();
        }

        let viewport = window.viewport_size();
        let now = cx.background_executor().now();
        let (surface_focus, since, direction, scope, title) = state.update(cx, |s, _| {
            let since = *s.shown_at.get_or_insert(now);
            (
                s.surface.clone(),
                since,
                s.direction,
                s.focus.clone(),
                s.title.clone(),
            )
        });
        let report = self.on_open_change.clone();
        let close = move |window: &mut Window, cx: &mut App| {
            if let Some(f) = &report {
                f(false, window, cx);
            }
        };

        // The entrance: medium-max fade and grow from the opener's side;
        // full-screen dialogs fade. Under reduced motion, a finger gets a
        // fade and a pointer nothing.
        let reduced = prefers_reduced_motion(cx);
        let touch = window.last_input_was_touch();
        let duration = if reduced {
            std::time::Duration::from_millis(200)
        } else {
            t.duration.medium_max
        };
        let progress = if reduced && !touch || duration.is_zero() {
            1.
        } else {
            (now.saturating_duration_since(since).as_secs_f32() / duration.as_secs_f32()).min(1.)
        };
        if progress < 1. {
            window.request_animation_frame();
        }
        let eased = t.ease.standard.ease(progress);

        let gutter = t.spacing.s4.to_pixels(rem);
        let scroller_handle = crate::gestures::scroller::use_scroll_handle(
            ElementId::Name(format!("{}-surface", self.id).into()),
            window,
            cx,
        );
        let mut surface = div()
            .id(ElementId::Name(format!("{}-surface", self.id).into()))
            .track_focus(&surface_focus)
            .tab_trap()
            .occlude()
            .relative()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&scroller_handle)
            .bg(t.color.background_surface)
            .text_color(t.color.text_primary)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .p(padding)
            .opacity(eased)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        surface = if fullscreen {
            surface.w(viewport.width).h(viewport.height)
        } else {
            let mut surface = surface
                .w(self.width.unwrap_or(px(400.).into()))
                .max_w((viewport.width - gutter * 2.).max(px(0.)))
                .max_h(self.max_height.unwrap_or((viewport.height * 0.75).into()))
                .rounded(t.radius.container)
                .shadow_token(&t.shadow.high);
            if !reduced {
                let travel = 1. - eased;
                surface = surface
                    .translate(direction.x * travel, direction.y * travel)
                    .scale(0.95 + 0.05 * eased);
            }
            surface
        };
        let role = if self.alert || self.purpose == DialogPurpose::Required {
            Role::AlertDialog
        } else {
            Role::Dialog
        };
        surface = surface.role(role).aria_modal(true);
        if let Some(name) = self.label.clone().or(title) {
            surface = surface.aria_label(name);
        }
        if let Some(description) = &self.description {
            surface = surface.aria_description(description.clone());
        }
        theme.refine(surface.style(), "dialog", &[], None);
        surface.style().refine(&self.style);
        let surface = surface.children(super::modal::probes(&state, cx));
        let corners = if fullscreen {
            crate::layout::ContainerCorners::default()
        } else {
            crate::layout::ContainerCorners::all(t.radius.container.to_pixels(rem))
        };
        let surface = surface
            .child(crate::gestures::scroller::scroll_begin(
                ElementId::Name(format!("{}-surface", self.id).into()),
                Some(&scroller_handle),
                (false, true),
            ))
            .child(provide(
                scope,
                crate::layout::padding_container(
                    all(padding),
                    Some(crate::layout::LayoutContainerKind::Dialog),
                    crate::layout::host_children(self.children, all(padding), corners),
                    cx,
                ),
            ))
            .child(crate::gestures::scroller::scroll_end());

        // The backdrop: dims the page (not behind a full-screen dialog),
        // takes its presses, and places the surface.
        let position = self.position.unwrap_or_default();
        let info = self.purpose == DialogPurpose::Info;
        let press: super::modal::CloseFn = Rc::new(close.clone());
        let mut backdrop = super::modal::backdrop(
            &self.id,
            (!fullscreen).then(|| super::modal::faded(t.color.overlay, eased)),
            info.then_some(press),
            window,
        )
        .flex();
        let (start, end) = if rtl {
            (position.end, position.start)
        } else {
            (position.start, position.end)
        };
        backdrop = match (start, end) {
            (Some(start), _) => backdrop.justify_start().pl(start),
            (None, Some(end)) => backdrop.justify_end().pr(end),
            (None, None) => backdrop.justify_center(),
        };
        backdrop = match (position.top, position.bottom) {
            (Some(top), _) => backdrop.items_start().pt(top),
            (None, Some(bottom)) => backdrop.items_end().pb(bottom),
            (None, None) => backdrop.items_center(),
        };
        let backdrop = backdrop.child(surface);

        super::modal::modal_layer(&self.id, backdrop, true, self.purpose, close)
    }
}

/// The same padding on every edge.
fn all(padding: Pixels) -> crate::layout::ContainerPadding {
    crate::layout::ContainerPadding {
        inline_start: padding,
        inline_end: padding,
        block_start: padding,
        block_end: padding,
    }
}

/// A dialog's header, upstream `DialogHeader`: a focusable title (where
/// focus enters by default, and the dialog's name), an optional subtitle,
/// start and end slots, a close button, and a divider.
#[derive(IntoElement)]
pub struct DialogHeader {
    title: SharedString,
    subtitle: Option<SharedString>,
    on_open_change: Option<OpenFn>,
    start_content: Option<AnyElement>,
    end_content: Option<AnyElement>,
    divider: bool,
    /// The title takes focus first and names the surface (a dialog's; a
    /// drawer's does neither).
    names: bool,
}

impl DialogHeader {
    /// Upstream `title`.
    pub fn new(title: impl Into<SharedString>) -> Self {
        DialogHeader {
            title: title.into(),
            subtitle: None,
            on_open_change: None,
            start_content: None,
            end_content: None,
            divider: true,
            names: true,
        }
    }

    /// A header whose title neither takes focus nor names the surface, and
    /// with no divider unless asked (DrawerHeader).
    pub(crate) fn plain(mut self) -> Self {
        self.names = false;
        self.divider = false;
        self
    }

    astryx_primitives::setters! {
        /// Upstream `subtitle`.
        ?subtitle: SharedString,
        /// Upstream `hasDivider`: a border under the header.
        divider: bool,
    }

    /// Upstream `onOpenChange`: a close button that asks to close.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `startContent`: before the title (a back button).
    pub fn start_content(mut self, content: impl IntoElement) -> Self {
        self.start_content = Some(content.into_any_element());
        self
    }

    /// Upstream `endContent`: after the title, before the close button.
    pub fn end_content(mut self, content: impl IntoElement) -> Self {
        self.end_content = Some(content.into_any_element());
        self
    }
}

impl RenderOnce for DialogHeader {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let title_focus = window
            .use_keyed_state(
                ElementId::Name(format!("dialog-header-{}", self.title).into()),
                cx,
                |_, cx| cx.focus_handle().tab_stop(false),
            )
            .read(cx)
            .clone();
        if self.names
            && let Some(scope) = focus_scope(cx)
        {
            scope.title(title_focus.clone(), self.title.clone());
        }

        // The title's center lines up with the close button's (upstream's
        // title and end offsets).
        let control = t.size.element_md.to_pixels(rem);
        let title_line = t.text.heading_2.size.to_pixels(rem) * t.text.heading_2.leading;
        let title_offset = control / 2. - t.spacing.s2.to_pixels(rem) - title_line / 2.;
        let has_end = self.end_content.is_some() || self.on_open_change.is_some();
        let end_offset = px(0.);
        let row_offset = title_offset.min(end_offset).min(px(0.));

        let close = self.on_open_change.map(|f| {
            let label = i18n(cx).t("@astryx.dialog.close");
            IconButton::new("dialog-header-close", label.clone(), Icon::new("close"))
                .variant(ButtonVariant::Ghost)
                .tooltip(label)
                .on_click(move |_, window, cx| f(false, window, cx))
        });
        let mut title = div().id(ElementId::Name(
            format!("dialog-header-title-{}", self.title).into(),
        ));
        if self.names {
            title = title.track_focus(&title_focus);
        }
        let title = title.child(Heading::new(HeadingLevel::H2, self.title.clone()).id(
            ElementId::Name(format!("dialog-title-{}", self.title).into()),
        ));
        let title_block = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .my(title_offset - row_offset)
            .child(title)
            .children(self.subtitle.map(|s| {
                Text::new(s)
                    .text_type(TextType::Body)
                    .size(TextSize::Sm)
                    .color(crate::text::TextColor::Secondary)
            }));
        let actions = has_end.then(|| {
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .flex_none()
                .my(end_offset - row_offset)
                .children(self.end_content)
                .children(close)
        });
        let start = self.start_content.map(|c| {
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s2)
                .flex_none()
                .my(-row_offset)
                .child(c)
        });
        let row = div()
            .flex()
            .items_start()
            .justify_between()
            .gap(t.spacing.s3)
            .my(row_offset)
            .children(start)
            .child(title_block)
            .children(actions);
        // A LayoutHeader: the Layout's insets, and the divider under it.
        let mut row = row;
        theme.refine(row.style(), "dialog-header", &[], None);
        crate::layout::LayoutHeader::new()
            .has_divider(self.divider)
            .child(row)
    }
}

/// Shows a dialog without managing its open state, upstream
/// `useImperativeDialog`: `show` it with content, `hide` it, and render its
/// [`DialogController::element`] somewhere in the tree.
#[derive(Clone)]
pub struct DialogController {
    id: ElementId,
    state: Entity<ImperativeDialog>,
}

type BuildFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type OptionsFn = Rc<dyn Fn(Dialog) -> Dialog>;

#[derive(Default)]
struct ImperativeDialog {
    content: Option<(BuildFn, Option<OptionsFn>)>,
}

impl DialogController {
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id: ElementId = id.into();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id}-imperative").into()),
            cx,
            |_, _| ImperativeDialog::default(),
        );
        DialogController { id, state }
    }

    /// Opens the dialog with `content`; `options` sets its props (width,
    /// purpose and the rest).
    pub fn show(
        &self,
        content: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
        options: Option<Rc<dyn Fn(Dialog) -> Dialog>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.state
            .update(cx, |s, _| s.content = Some((Rc::new(content), options)));
        window.refresh();
    }

    pub fn hide(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| s.content = None);
        window.refresh();
    }

    pub fn is_open(&self, cx: &App) -> bool {
        self.state.read(cx).content.is_some()
    }

    /// The dialog itself: render it in your tree.
    pub fn element(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let content = self.state.read(cx).content.clone();
        let state = self.state.clone();
        let mut dialog = Dialog::new(self.id.clone(), content.is_some()).on_open_change(
            move |open, window, cx| {
                if !open {
                    state.update(cx, |s, _| s.content = None);
                    window.refresh();
                }
            },
        );
        if let Some((build, options)) = content {
            dialog = dialog.child(build(window, cx));
            if let Some(options) = options {
                dialog = options(dialog);
            }
        }
        dialog.into_any_element()
    }
}
