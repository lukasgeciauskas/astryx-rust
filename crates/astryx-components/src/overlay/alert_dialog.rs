//! AlertDialog, upstream `AlertDialog/`: confirms a destructive or
//! irreversible action before it happens.
//!
//! A Dialog with purpose `Form` (Escape cancels, a backdrop press does not)
//! exposed as an alert dialog, named by its title and described by its
//! description. Cancel takes the initial focus, the least destructive
//! choice. The action does not close it: hold it open while the work runs
//! (`action_loading`) and close it when the work settles. At 640px or
//! narrower the buttons stack full width with the action above Cancel.
//!
//! The touch presentation (a coarse primary pointer, upstream `useAlertMotion`): on a
//! phone (a compact window) the default width is iOS's alert width, 320. The alert
//! scales in from 1.2 (never wider than the window) and fades in with its dimming over
//! `--duration-medium-max` on `--ease-entrance`, a finger's input held until it has
//! entered; a close fades it and its dimming out over `--duration-exit` on
//! `--ease-exit`, its content inert, and Escape or Cancel then does nothing. Reopened
//! during the exit, it fades back from where it is. Under Reduce Motion both are the
//! 200 ms opacity cross-fade. A mouse keeps the theme's entrance.

use std::rc::Rc;

use astryx_i18n::current as i18n;
use astryx_primitives::provide;
use astryx_theme::current;
use gpui::{
    App, ElementId, IntoElement, Length, ParentElement, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, div, prelude::*,
};

use super::dialog::{AutoFocus, Dialog, DialogPurpose, SurfaceMotion};
use crate::button::{Button, ButtonVariant};

type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type ActionFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Upstream `PHONE_WIDTH`: iOS's alert width, the default on a phone.
pub const PHONE_WIDTH: f32 = 320.;
/// Upstream `alertController.actions.highlightOnsetMs`: the touched action highlights in
/// the touch-down frame.
pub const ALERT_HIGHLIGHT_ONSET_MS: f64 = 0.;
/// Upstream `alertController.actions.highlightExpansion`: no margin around an action.
pub const ALERT_HIGHLIGHT_EXPANSION: f32 = 0.;
/// Upstream `PRESENT_SCALE_FROM`: iOS's alert starts at 1.2 times its size.
pub const PRESENT_SCALE_FROM: f32 = 1.2;

/// Upstream's default `width`: iOS's alert width on a phone (a compact window under a
/// coarse primary pointer), in every theme; 400 elsewhere.
pub(crate) fn default_alert_width(touch: super::presentation::TouchPresentation) -> f32 {
    if touch.is_touch && touch.is_compact {
        PHONE_WIDTH
    } else {
        400.
    }
}

/// Upstream `useAlertMotion`'s status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AlertStatus {
    #[default]
    Hidden,
    Open,
    Leaving,
}

/// The touch presentation's motion between frames.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AlertMotion {
    pub status: AlertStatus,
    since: Option<web_time::Instant>,
    from_opacity: f32,
    scale_from: f32,
    opacity: f32,
    scale: f32,
}

/// One frame's timing: a duration and its easing.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AlertTiming {
    pub entrance: (std::time::Duration, astryx_theme::CubicBezier),
    pub exit: (std::time::Duration, astryx_theme::CubicBezier),
    /// Under Reduce Motion: an opacity-only cross-fade, no scale.
    pub reduced: bool,
}

/// What a frame draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AlertFrame {
    /// The Dialog is open: shown or leaving.
    pub presented: bool,
    /// The motion that drives it, while enabled.
    pub motion: Option<SurfaceMotion>,
    /// The motion still runs: another frame is wanted.
    pub moving: bool,
    /// The entrance runs: a finger's input is held until `hold_until`.
    pub hold_until: Option<web_time::Instant>,
}

impl AlertMotion {
    /// Upstream `presentScaleFrom`: the 1.2 start, never wider than the window.
    pub(crate) fn scale_from(width: f32, viewport: f32) -> f32 {
        if width > 0. && viewport > 0. {
            (viewport / width).clamp(1., PRESENT_SCALE_FROM)
        } else {
            PRESENT_SCALE_FROM
        }
    }

    /// Steps the motion to `now` for the caller's `open`, when the touch motion is
    /// `enabled`.
    pub(crate) fn frame(
        &mut self,
        open: bool,
        enabled: bool,
        scale_from: f32,
        timing: AlertTiming,
        now: web_time::Instant,
    ) -> AlertFrame {
        if !enabled {
            // A mouse took over: the theme's entrance shows again.
            *self = AlertMotion::default();
            return AlertFrame {
                presented: open,
                motion: None,
                moving: false,
                hold_until: None,
            };
        }
        match (self.status, open) {
            (AlertStatus::Hidden, true) => {
                self.status = AlertStatus::Open;
                self.since = Some(now);
                self.from_opacity = 0.;
                self.scale_from = if timing.reduced { 1. } else { scale_from };
            }
            (AlertStatus::Leaving, true) => {
                // Reopened during the exit: it fades back from where it is.
                self.status = AlertStatus::Open;
                self.since = Some(now);
                self.from_opacity = self.opacity;
                self.scale_from = self.scale;
            }
            (AlertStatus::Open, false) => {
                self.status = AlertStatus::Leaving;
                self.since = Some(now);
                self.from_opacity = self.opacity;
            }
            _ => {}
        }
        let since = self.since.unwrap_or(now);
        let progress = |duration: std::time::Duration| {
            if duration.is_zero() {
                1.
            } else {
                (now.saturating_duration_since(since).as_secs_f32() / duration.as_secs_f32())
                    .min(1.)
            }
        };
        let mut hold_until = None;
        let moving = match self.status {
            AlertStatus::Hidden => false,
            AlertStatus::Open => {
                let (duration, ease) = timing.entrance;
                let p = progress(duration);
                let e = ease.ease(p);
                self.opacity = self.from_opacity + (1. - self.from_opacity) * e;
                self.scale = self.scale_from + (1. - self.scale_from) * e;
                if p < 1. {
                    hold_until = Some(since + duration);
                }
                p < 1.
            }
            AlertStatus::Leaving => {
                let (duration, ease) = timing.exit;
                let p = progress(duration);
                self.opacity = self.from_opacity * (1. - ease.ease(p));
                if p >= 1. {
                    *self = AlertMotion::default();
                }
                p < 1.
            }
        };
        let presented = self.status != AlertStatus::Hidden;
        AlertFrame {
            presented,
            motion: presented.then_some(SurfaceMotion {
                opacity: self.opacity,
                scale: self.scale,
                inert: self.status == AlertStatus::Leaving,
            }),
            moving,
            hold_until,
        }
    }
}

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
        // Only under the touch presentation does the alert scale in and fade out; a
        // mouse keeps the theme's entrance.
        let touch = super::presentation::touch_presentation(window, cx);
        let width = self
            .width
            .unwrap_or_else(|| gpui::px(default_alert_width(touch)).into());
        let width_px = match width {
            Length::Definite(gpui::DefiniteLength::Absolute(gpui::AbsoluteLength::Pixels(p))) => {
                f32::from(p)
            }
            _ => 400.,
        };
        let reduced = astryx_primitives::prefers_reduced_motion(cx);
        let cross_fade = std::time::Duration::from_millis(
            astryx_primitives::motion::uikit::timing::REDUCED_MOTION_CROSS_FADE_MS as u64,
        );
        let timing = AlertTiming {
            entrance: if reduced {
                (cross_fade, t.ease.standard)
            } else {
                (t.duration.medium_max, t.ease.entrance)
            },
            exit: if reduced {
                (cross_fade, t.ease.standard)
            } else {
                (t.duration.exit, t.ease.exit)
            },
            reduced,
        };
        let motion_state = window.use_keyed_state(
            ElementId::Name(format!("{}-alert-motion", self.id).into()),
            cx,
            |_, _| AlertMotion::default(),
        );
        let now = cx.background_executor().now();
        let scale_from = AlertMotion::scale_from(width_px, f32::from(window.viewport_size().width));
        let frame = motion_state.update(cx, |m, _| {
            m.frame(
                self.open,
                touch.is_touch && !self.inline,
                scale_from,
                timing,
                now,
            )
        });
        if frame.moving {
            window.request_animation_frame();
        }
        let hold_key = ElementId::Name(format!("{}-alert-hold", self.id).into());
        match frame.hold_until {
            // A finger's or a pen's input waits until the alert has entered; keys and
            // assistive technology stay live.
            Some(until) => astryx_primitives::gestures::input_hold::hold_input(
                window,
                hold_key,
                astryx_primitives::gestures::input_hold::HoldScope::Viewport,
                until,
                cx,
            ),
            None => {
                astryx_primitives::gestures::input_hold::release_input_hold(window, &hold_key, cx)
            }
        }
        // While the alert fades out its content is inert, and a request to close (Escape)
        // is ignored until it is hidden.
        let leaving = frame.motion.is_some_and(|m| m.inert);
        let on_open_change = self.on_open_change.clone().map(|f| -> OpenFn {
            Rc::new(move |open, window: &mut Window, cx: &mut App| {
                if !leaving {
                    f(open, window, cx);
                }
            })
        });
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
            .child({
                // Under the touch presentation a finger's press slides across the actions
                // (upstream `useActionGroupTracking` with the alert's UIKit values: the
                // touched action highlights in the touch-down frame, with no margin).
                let tracking = crate::gestures::action_group::use_action_group_tracking(
                    ElementId::Name(format!("{}-actions", self.id).into()),
                    crate::gestures::action_group::ActionGroupTrackingOptions {
                        highlight_onset_ms: ALERT_HIGHLIGHT_ONSET_MS,
                        highlight_expansion: ALERT_HIGHLIGHT_EXPANSION,
                        pointer_types: Some(
                            crate::gestures::action_group::GROUP_PRESS_POINTERS.to_vec(),
                        ),
                        enabled: self.open && !self.inline && touch.is_touch,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                tracking.group(actions)
            });

        let report = on_open_change;
        let mut dialog = Dialog::new(self.id.clone(), frame.presented)
            .motion(frame.motion)
            .width(width)
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
