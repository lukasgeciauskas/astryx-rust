//! AlertDialog, upstream `AlertDialog/`: confirms a destructive or
//! irreversible action before it happens.
//!
//! A Dialog with purpose `Form` (Escape cancels, a backdrop press does not)
//! exposed as an alert dialog, named by its title and described by its
//! description. Cancel takes the initial focus, the least destructive
//! choice. The action does not close it: hold it open while the work runs
//! (`action_loading`) and close it when the work settles. At 640px or
//! narrower the buttons stack full width with the action above Cancel.

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::provide;
use astryx_theme::current;
use gpui::{
    App, ElementId, IntoElement, Length, ParentElement, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, div, prelude::*,
};

use super::dialog::{AutoFocus, Dialog, DialogPurpose};
use crate::button::{Button, ButtonVariant};

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type ActionFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Confirms a destructive action, upstream `AlertDialog`.
///
/// ```ignore
/// AlertDialog::new("delete", open, "Delete project?", "This cannot be undone.", "Delete project")
///     .on_action(|window, cx| ...)
///     .on_open_change(|open, window, cx| ...)
/// ```
#[derive(IntoElement)]
pub struct AlertDialog {
    id: ElementId,
    open: bool,
    title: SharedString,
    description: SharedString,
    action_label: SharedString,
    on_action: Option<ActionFn>,
    on_open_change: Option<OpenFn>,
    cancel_label: Option<SharedString>,
    action_variant: ButtonVariant,
    action_loading: bool,
    width: Option<Length>,
    inline: bool,
    style: StyleRefinement,
}

impl AlertDialog {
    /// Upstream `isOpen`, `title`, `description` and `actionLabel`.
    pub fn new(
        id: impl Into<ElementId>,
        open: bool,
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
        action_label: impl Into<SharedString>,
    ) -> Self {
        AlertDialog {
            id: id.into(),
            open,
            title: title.into(),
            description: description.into(),
            action_label: action_label.into(),
            on_action: None,
            on_open_change: None,
            cancel_label: None,
            action_variant: ButtonVariant::Destructive,
            action_loading: false,
            width: None,
            inline: false,
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `cancelLabel`: "Cancel" by default.
        ?cancel_label: SharedString,
        /// Upstream `actionVariant`: destructive by default.
        action_variant: ButtonVariant,
        /// Upstream `isActionLoading`: a spinner on the action.
        !action_loading,
        /// Upstream `width`: 400px by default.
        ?width: Length,
        /// Upstream `isInline`: in the flow, for previews.
        !inline,
    }

    /// Upstream `onAction`: the action button; it does not close the dialog.
    pub fn on_action(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    /// Upstream `onOpenChange`: Cancel and Escape ask to close.
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }
}

impl Styled for AlertDialog {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for AlertDialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let small = window.viewport_size().width <= gpui::px(640.);
        let cancel_label = self
            .cancel_label
            .clone()
            .unwrap_or_else(|| i18n(cx).t("@astryx.alertDialog.cancel"));

        let report = self.on_open_change.clone();
        let cancel = Button::new(
            ElementId::Name(format!("{}-cancel", self.id).into()),
            cancel_label,
        )
        .variant(ButtonVariant::Ghost)
        .on_click(move |_, window, cx| {
            if let Some(f) = &report {
                f(false, window, cx);
            }
        });
        let on_action = self.on_action.clone();
        let action = Button::new(
            ElementId::Name(format!("{}-action", self.id).into()),
            self.action_label.clone(),
        )
        .variant(self.action_variant)
        .loading(self.action_loading)
        .on_click(move |_, window, cx| {
            if let Some(f) = &on_action {
                f(window, cx);
            }
        });
        let (cancel, action) = if small {
            (cancel.w_full(), action.w_full())
        } else {
            (cancel, action)
        };
        // Cancel takes the initial focus wherever it sits.
        let cancel = provide(AutoFocus, cancel);
        let mut actions = div().flex().gap(t.spacing.s2);
        actions = if small {
            actions
                .flex_col()
                .items_stretch()
                .child(action)
                .child(cancel)
        } else {
            actions
                .flex_row()
                .flex_wrap()
                .justify_end()
                .child(cancel)
                .child(action)
        };

        let mut title = div()
            .text_size(t.text.heading_2.size)
            .line_height(gpui::relative(t.text.heading_2.leading))
            .font_weight(t.text.heading_2.weight)
            .text_color(t.color.text_primary)
            .child(self.title.clone());
        theme.refine(title.style(), "alert-dialog-title", &[], None);
        let mut description = div()
            .text_size(t.text.body.size)
            .line_height(gpui::relative(t.text.body.leading))
            .text_color(t.color.text_secondary)
            .child(self.description.clone());
        theme.refine(description.style(), "alert-dialog-description", &[], None);

        let body = div()
            .flex()
            .flex_col()
            .gap(t.spacing.s4)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.spacing.s2)
                    .child(title)
                    .child(description),
            )
            .child(actions);

        let report = self.on_open_change.clone();
        let mut dialog = Dialog::new(self.id.clone(), self.open)
            .purpose(DialogPurpose::Form)
            .label(self.title.clone())
            .description(self.description.clone())
            .alert()
            .inline(self.inline)
            .on_open_change(move |open, window, cx| {
                if let Some(f) = &report {
                    f(open, window, cx);
                }
            })
            .child(body);
        if let Some(width) = self.width {
            dialog = dialog.width(width);
        }
        theme.refine(dialog.style(), "alert-dialog", &[], None);
        dialog.style().refine(&self.style);
        dialog
    }
}

/// What [`AlertDialogController::show`] opens: an alert dialog's props.
pub struct AlertOptions {
    pub title: SharedString,
    pub description: SharedString,
    pub action_label: SharedString,
    pub on_action: ActionFn,
    pub cancel_label: Option<SharedString>,
    pub action_variant: ButtonVariant,
    pub action_loading: bool,
    pub width: Option<Length>,
}

impl AlertOptions {
    pub fn new(
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
        action_label: impl Into<SharedString>,
        on_action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        AlertOptions {
            title: title.into(),
            description: description.into(),
            action_label: action_label.into(),
            on_action: Rc::new(on_action),
            cancel_label: None,
            action_variant: ButtonVariant::Destructive,
            action_loading: false,
            width: None,
        }
    }
}

/// Shows an alert dialog without managing its open state, upstream
/// `useImperativeAlertDialog`: `show` it with its props, `hide` it, and
/// render its [`AlertDialogController::element`] in the tree.
#[derive(Clone)]
pub struct AlertDialogController {
    id: ElementId,
    state: gpui::Entity<Option<Rc<AlertOptions>>>,
}

impl AlertDialogController {
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id: ElementId = id.into();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id}-imperative-alert").into()),
            cx,
            |_, _| None,
        );
        AlertDialogController { id, state }
    }

    pub fn show(&self, options: AlertOptions, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| *s = Some(Rc::new(options)));
        window.refresh();
    }

    pub fn hide(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| *s = None);
        window.refresh();
    }

    pub fn is_open(&self, cx: &App) -> bool {
        self.state.read(cx).is_some()
    }

    /// The alert dialog itself: render it in your tree.
    pub fn element(&self, cx: &App) -> gpui::AnyElement {
        let options = self.state.read(cx).clone();
        let state = self.state.clone();
        let close = move |open: bool, window: &mut Window, cx: &mut App| {
            if !open {
                state.update(cx, |s, _| *s = None);
                window.refresh();
            }
        };
        match options {
            Some(o) => {
                let on_action = o.on_action.clone();
                let mut dialog = AlertDialog::new(
                    self.id.clone(),
                    true,
                    o.title.clone(),
                    o.description.clone(),
                    o.action_label.clone(),
                )
                .on_action(move |window, cx| on_action(window, cx))
                .on_open_change(close)
                .action_variant(o.action_variant)
                .action_loading(o.action_loading);
                if let Some(label) = &o.cancel_label {
                    dialog = dialog.cancel_label(label.clone());
                }
                if let Some(width) = o.width {
                    dialog = dialog.width(width);
                }
                dialog.into_any_element()
            }
            None => AlertDialog::new(self.id.clone(), false, "", "", "")
                .on_open_change(close)
                .into_any_element(),
        }
    }
}
