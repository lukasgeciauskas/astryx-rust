//! TimeInput, upstream `TimeInput/TimeInput.tsx`: a typed time field
//! (the `text-input` and fine-pointer surface), FR1-FR15.
//!
//! Typing keeps a draft that commits as soon as it parses to an in-range
//! time; ArrowUp and ArrowDown step by `increment` minutes and announce the
//! time; Enter or blur commit the rest, and a rejected draft stays with the
//! invalid-time message (AST-117, AST-118).

use std::rc::Rc;

use astryx_i18n::date::{
    HourFormat, PlainTime, adjust_time, format_locale_time, is_time_in_range, resolve_hour_format,
};
use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::editing::{EditorStyle, FieldSettings, FocusFn, TextEditState, text_editor};
use astryx_primitives::{
    ElementSize, Politeness, announce, is_focus_visible, prefers_reduced_motion, provide,
    resolve_size,
};
use astryx_theme::{ThemeStyled, current, scope};
use gpui::{
    App, ElementId, Focusable, IntoElement, KeyDownEvent, Length, MouseButton, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Task, Window, div, prelude::*, px,
};

use super::typed::{TimeInputEntryError, TimeRules, Trigger, TypedTimeState};
use crate::button::{ButtonVariant, IconButton};
use crate::choice::action::{ActionFn, Optimistic};
use crate::date_input::LegacyNativePicker;
use crate::date_input::presentation::{
    InputPresentation, Surface, coarse_pointer, effective_input, resolve_input,
};
use crate::field::entry_report::{CallerErrorPrecedence, EntryReport, ReportFn, ResolveInput};
use crate::field::{
    EntryErrorBehavior, Field, InputStatus, StatusPlacement, StatusType, entry_error_behavior,
};
use crate::icon::{Icon, IconColor, IconDefaultSize, IconSize};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::input::group::{group_context, group_disabled, group_place, inside_member};
use crate::input::selection_color;
use crate::overlay::Disclosure;
use crate::status::{Spinner, SpinnerShade, SpinnerSize};

type ValueFn = Rc<dyn Fn(Option<PlainTime>, &mut Window, &mut App)>;

/// A time field, upstream `TimeInput`.
///
/// ```ignore
/// TimeInput::new("start", "Start time", time).on_change(|time, _, _| println!("{time:?}"))
/// ```
#[derive(IntoElement)]
pub struct TimeInput {
    pub(super) id: ElementId,
    pub(super) label: SharedString,
    pub(super) value: Option<PlainTime>,
    pub(super) on_change: Option<ValueFn>,
    pub(super) change_action: Option<ActionFn<Option<PlainTime>>>,
    pub(super) label_hidden: bool,
    pub(super) description: Option<SharedString>,
    pub(super) optional: bool,
    pub(super) required: bool,
    pub(super) disabled: bool,
    pub(super) disabled_message: Option<SharedString>,
    pub(super) loading: bool,
    pub(super) min: Option<PlainTime>,
    pub(super) max: Option<PlainTime>,
    pub(super) seconds: bool,
    pub(super) clear: bool,
    pub(super) hour_format: Option<HourFormat>,
    pub(super) increment: i64,
    pub(super) presentation: Option<InputPresentation>,
    pub(super) native_picker: Option<LegacyNativePicker>,
    pub(super) placeholder: Option<SharedString>,
    pub(super) size: Option<ElementSize>,
    pub(super) status: Option<InputStatus>,
    pub(super) on_entry_error_change: Option<ReportFn<TimeInputEntryError>>,
    pub(super) entry_error_behavior: Option<EntryErrorBehavior>,
    pub(super) status_placement: StatusPlacement,
    pub(super) label_tooltip: Option<SharedString>,
    pub(super) auto_focus: bool,
    pub(super) width: Option<Length>,
    pub(super) control_style: StyleRefinement,
    pub(super) style: StyleRefinement,
}

impl TimeInput {
    /// Upstream `label` and the controlled `value` (`None` when empty).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<Option<PlainTime>>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            on_change: None,
            change_action: None,
            label_hidden: false,
            description: None,
            optional: false,
            required: false,
            disabled: false,
            disabled_message: None,
            loading: false,
            min: None,
            max: None,
            seconds: false,
            clear: false,
            hour_format: None,
            increment: 1,
            presentation: None,
            native_picker: None,
            placeholder: None,
            size: None,
            status: None,
            on_entry_error_change: None,
            entry_error_behavior: None,
            status_placement: StatusPlacement::Attached,
            label_tooltip: None,
            auto_focus: false,
            width: None,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isDisabled` (FR9).
        !disabled,
        /// Upstream `disabledMessage`.
        ?disabled_message: SharedString,
        /// Upstream `isLoading`.
        !loading,
        /// Upstream `min`.
        ?min: PlainTime,
        /// Upstream `max`.
        ?max: PlainTime,
        /// Upstream `hasSeconds`.
        !seconds,
        /// Upstream `hasClear`.
        !clear,
        /// Upstream `hourFormat`: else the locale's hour cycle.
        ?hour_format: HourFormat,
        /// Upstream `presentation`.
        ?presentation: InputPresentation,
        /// Upstream's deprecated `nativePicker`.
        ?native_picker: LegacyNativePicker,
        /// Upstream `placeholder` (default "Select a time").
        ?placeholder: SharedString,
        /// Upstream `size`.
        ?size: ElementSize,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `entryErrorBehavior`.
        ?entry_error_behavior: EntryErrorBehavior,
        /// Upstream `statusVariant`.
        status_placement: StatusPlacement,
        /// Upstream `labelTooltip`.
        ?label_tooltip: SharedString,
        /// Upstream `hasAutoFocus`.
        !auto_focus,
        /// Upstream `width`.
        ?width: Length,
    }

    /// Upstream `increment`: minutes ArrowUp and ArrowDown step by.
    pub fn increment(mut self, minutes: i64) -> Self {
        self.increment = minutes;
        self
    }

    /// Upstream `onChange`: the time, or `None` when cleared.
    pub fn on_change(
        mut self,
        f: impl Fn(Option<PlainTime>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction` (FR2).
    pub fn change_action(
        mut self,
        f: impl Fn(Option<PlainTime>, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `onEntryErrorChange` (FR15).
    pub fn on_entry_error_change(
        mut self,
        f: impl Fn(Option<TimeInputEntryError>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_entry_error_change = Some(Rc::new(f));
        self
    }

    /// Upstream `controlXstyle`.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl Styled for TimeInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[derive(Default)]
struct FieldState {
    typed: TypedTimeState,
    focused: bool,
    last_alert: Option<SharedString>,
    last_group_status: Option<SharedString>,
}

#[derive(Clone)]
struct Ctx {
    state: gpui::Entity<FieldState>,
    rules: TimeRules,
    value: Option<PlainTime>,
    disabled: bool,
    behavior: EntryErrorBehavior,
    on_change: Option<ValueFn>,
    action: Option<ActionFn<Option<PlainTime>>>,
    optimistic: Rc<Optimistic<Option<PlainTime>>>,
    report: EntryReport<TimeInputEntryError>,
    locked: bool,
    increment: i64,
}

impl Ctx {
    /// Upstream `fireChange` (ORD2).
    fn fire(&self, value: Option<PlainTime>, window: &mut Window, cx: &mut App) {
        let change = self
            .optimistic
            .change(self.on_change.clone(), self.action.clone());
        change(value, window, cx);
        window.refresh();
    }

    fn report(
        &self,
        error: Option<Option<TimeInputEntryError>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(error) = error
            && !self.locked
        {
            self.report.report(error, window, cx);
        }
    }

    fn typed(&self, text: String, window: &mut Window, cx: &mut App) {
        if self.disabled {
            return;
        }
        let (time, report) = self.state.update(cx, |s, _| {
            s.typed.change(text, &self.rules, self.value, self.behavior)
        });
        self.report(report, window, cx);
        if let Some(time) = time {
            self.fire(Some(time), window, cx);
        }
        window.refresh();
    }

    fn commit(&self, trigger: Trigger, window: &mut Window, cx: &mut App) {
        if self.disabled {
            return;
        }
        let (time, report) = self
            .state
            .update(cx, |s, _| s.typed.commit(trigger, &self.rules, self.value));
        self.report(report, window, cx);
        if let Some(time) = time {
            self.fire(time, window, cx);
        }
        window.refresh();
    }

    /// FR4: ArrowUp and ArrowDown step from the value, or now.
    fn step(&self, up: bool, window: &mut Window, cx: &mut App) {
        if self.disabled {
            return;
        }
        let from = self
            .value
            .unwrap_or_else(|| PlainTime::now().truncated(self.rules.seconds));
        let delta = if up { self.increment } else { -self.increment };
        let next = adjust_time(from, delta, self.rules.seconds);
        if !is_time_in_range(next, self.rules.min, self.rules.max) {
            return;
        }
        self.state.update(cx, |s, _| s.typed.drop_draft());
        self.report.report(None, window, cx);
        self.fire(Some(next), window, cx);
        let text = format_locale_time(
            next,
            &self.rules.locale,
            self.rules.hour_format,
            self.rules.seconds,
        );
        announce(text, Politeness::Polite, window, cx);
    }
}

impl RenderOnce for TimeInput {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.disabled |= group_disabled(cx);
        let place = group_place(cx);
        let group = place.and_then(|_| group_context(cx));
        let dir = use_direction(cx);
        let theme = current(window, cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let i18n = i18n(cx);
        let locale = i18n.locale().clone();
        // Every surface gets the resolved hour format (FR5).
        let hour_format = resolve_hour_format(&locale, self.hour_format);

        // ORD1: the surface. An adaptive policy keeps the typed field where
        // the platform picker cannot step (seconds, an increment); Native
        // falls back where the platform has no picker (R16).
        let policy = effective_input(self.presentation, self.native_picker, "TimeInput");
        let coarse = coarse_pointer(window, cx);
        let mut surface: Surface = resolve_input(policy, coarse);
        let forced = self.presentation == Some(InputPresentation::Native);
        if surface == Surface::Native && !forced && (self.seconds || self.increment != 1) {
            surface = Surface::TextInput;
        }
        let surface = crate::date_input::native::native_or_fallback(
            surface,
            gpui::NativePickerKind::Time,
            coarse,
            Surface::TextInput,
            window,
        );
        if surface == Surface::Sheet {
            return super::sheet::render_sheet(self, hour_format, window, cx);
        }
        let native = surface == Surface::Native;

        let optimistic = Rc::new(Optimistic::<Option<PlainTime>>::new(&self.id, window, cx));
        let shown_value = optimistic.shown(self.value, cx);
        let busy = self.loading || optimistic.pending(cx);
        let locked = self.disabled || busy;
        optimistic.announce_busy(busy, i18n.t("@astryx.spinner.loading"), window, cx);
        let behavior = entry_error_behavior(self.entry_error_behavior, cx);
        let rules = TimeRules {
            locale: locale.clone(),
            hour_format,
            seconds: self.seconds,
            min: self.min,
            max: self.max,
        };
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-time-input", self.id).into()),
            cx,
            |_, _| FieldState::default(),
        );
        let (entry_error, rejected) = state.update(cx, |s, _| {
            s.typed.reconcile(self.value, self.disabled, !native);
            s.typed.resolve(&rules, locked, behavior)
        });

        let report = EntryReport::<TimeInputEntryError>::new(&self.id, window, cx);
        let caller_announces = matches!(
            &self.status,
            Some(InputStatus {
                status: StatusType::Error,
                message: Some(_)
            })
        );
        let resolved = report.resolve(
            ResolveInput {
                entry_error,
                rejection_message: rejected.then(|| i18n.t("@astryx.timeInput.invalidTime")),
                status: self.status.as_ref(),
                status_announced: place.is_none() || caller_announces,
                precedence: CallerErrorPrecedence::Always,
                callback: self.on_entry_error_change.clone(),
            },
            window,
            cx,
        );
        // In a group no Field shows the status: the caller's status speaks
        // through the shared channel (FR10).
        let field_status = if place.is_some() {
            self.status.clone()
        } else {
            resolved.status
        };
        let (alert_changed, group_changed) = state.update(cx, |s, _| {
            let alert = s.last_alert != resolved.alert;
            s.last_alert = resolved.alert.clone();
            let group_message = place.and(self.status.as_ref().and_then(|s| s.message.clone()));
            let group = s.last_group_status != group_message;
            s.last_group_status = group_message;
            (alert, group)
        });
        if alert_changed && let Some(alert) = resolved.alert.clone() {
            window.defer(cx, move |window, cx| {
                announce(alert, Politeness::Assertive, window, cx)
            });
        }
        if group_changed
            && place.is_some()
            && let Some(status) = self.status.clone()
            && let Some(message) = status.message
        {
            let politeness = if status.status == StatusType::Error {
                Politeness::Assertive
            } else {
                Politeness::Polite
            };
            window.defer(cx, move |window, cx| {
                announce(message, politeness, window, cx)
            });
        }

        let ctx = Ctx {
            state: state.clone(),
            rules: rules.clone(),
            value: self.value,
            disabled: self.disabled,
            behavior,
            on_change: self.on_change.clone(),
            action: self.change_action.clone(),
            optimistic: optimistic.clone(),
            report: report.clone(),
            locked,
            increment: self.increment,
        };

        if native {
            if self.auto_focus {
                let done = window.use_keyed_state(
                    ElementId::Name(format!("{:?}-autofocused", self.id).into()),
                    cx,
                    |_, _| false,
                );
                if !*done.read(cx) {
                    done.update(cx, |done, _| *done = true);
                    let focus = astryx_primitives::control_focus(&self.id, window, cx);
                    window.defer(cx, move |window, cx| window.focus(&focus, cx));
                }
            }
            let (fire, set, commit) = (ctx.clone(), ctx.clone(), ctx.clone());
            let props = super::native::NativeTime {
                hour_format,
                size,
                shown: shown_value,
                busy,
                field_status,
                rejected,
                in_group: place.is_some(),
                group,
                fire: Rc::new(move |value, window, cx| fire.fire(value, window, cx)),
                set_rejected: Rc::new(move |rejected, cx| {
                    set.state.update(cx, |s, _| {
                        s.typed.set_surface_rejected(rejected, set.behavior)
                    })
                }),
                commit: Rc::new(move |window, cx| {
                    commit.state.update(cx, |s, _| s.typed.commit_surface());
                    window.refresh();
                }),
            };
            return super::native::render_native(self, props, window, cx);
        }

        let (pending, focused_state) = {
            let s = state.read(cx);
            (s.typed.pending.clone(), s.focused)
        };
        let display = pending.unwrap_or_else(|| {
            shown_value
                .map(|v| format_locale_time(v, &locale, hour_format, self.seconds))
                .unwrap_or_default()
        });
        // FR5: the format hint while focused, enabled and empty.
        let placeholder = if focused_state && display.is_empty() {
            match hour_format {
                HourFormat::H12 => i18n.t("@astryx.dateTimeInput.timeHint12h"),
                HourFormat::H24 => i18n.t("@astryx.dateTimeInput.timeHint24h"),
            }
        } else {
            self.placeholder
                .clone()
                .unwrap_or_else(|| i18n.t("@astryx.timeInput.placeholder"))
        };

        let editor_state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-editor", self.id).into()),
            cx,
            |window, cx| TextEditState::reporting("", false, window, cx),
        );
        let focus = editor_state.read(cx).focus_handle(cx);
        let focused = focus.is_focused(window);
        editor_state.update(cx, |s, cx| {
            s.set_text(&display, cx);
            s.set_settings(FieldSettings {
                disabled: self.disabled,
                read_only: self.disabled,
                masked: false,
                filter: None,
                spell_check: false,
                keyboard: Default::default(),
            });
            s.on_change = (!self.disabled).then(|| {
                let ctx = ctx.clone();
                Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                    ctx.typed(text, window, cx)
                }) as astryx_primitives::editing::ChangeFn
            });
            // Enter commits the draft; it never blocks a form (AST-118 FR9).
            s.on_enter = (!self.disabled).then(|| {
                let ctx = ctx.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    ctx.commit(Trigger::Enter, window, cx)
                }) as FocusFn
            });
            s.on_blur = Some({
                let ctx = ctx.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    ctx.state.update(cx, |s, _| s.focused = false);
                    ctx.commit(Trigger::Blur, window, cx);
                })
            });
            s.on_focus = Some({
                let ctx = ctx.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    // A disabled field keeps its static placeholder.
                    if !ctx.disabled {
                        ctx.state.update(cx, |s, _| s.focused = true);
                        window.refresh();
                    }
                })
            });
            s.on_key_down = (!self.disabled).then(|| {
                let (ctx, editor) = (ctx.clone(), cx.entity());
                Rc::new(
                    move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                        let keys = &event.keystroke;
                        let up = match keys.key.as_str() {
                            "up" => true,
                            "down" => false,
                            _ => return,
                        };
                        let composing = editor.read(cx).buffer().marked().is_some();
                        if composing || keys.modifiers.modified() {
                            return;
                        }
                        cx.stop_propagation();
                        ctx.step(up, window, cx);
                    },
                ) as astryx_primitives::editing::KeyDownFn
            });
        });
        if self.auto_focus {
            let done = window.use_keyed_state(
                ElementId::Name(format!("{:?}-autofocused", self.id).into()),
                cx,
                |_, _| false,
            );
            if !*done.read(cx) {
                done.update(cx, |done, _| *done = true);
                let focus = focus.clone();
                window.defer(cx, move |window, cx| window.focus(&focus, cx));
            }
        }

        let status = field_status.as_ref().map(|s| s.status);
        let chrome = Chrome {
            id: ElementId::Name(format!("{}-control", self.id).into()),
            target: "time-input",
            variant: None,
            size,
            status,
            disabled: self.disabled,
            read_only: false,
            focused,
            focus_visible: is_focus_visible(&focus, window),
            row: true,
            place,
            dir,
        };
        let part_override = theme.for_part("time-input", &chrome.props());
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let mut control = control_box(&chrome, &part, window, cx);
        control.style().refine(&self.control_style);
        // Click-to-focus anywhere in the box (FR12).
        if !self.disabled {
            let focus = focus.clone();
            control = control.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            });
        }
        control = crate::input::field_menu::with_field_menu(
            control,
            &self.id,
            &editor_state,
            &focus,
            self.disabled,
            window,
            cx,
        );
        control = control.child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .text_color(t.color.icon_secondary)
                .child(provide(
                    IconDefaultSize(IconSize::Sm),
                    Icon::new("clock").color(IconColor::Secondary),
                )),
        );

        let invalid = status == Some(StatusType::Error) || rejected;
        let mut described = described_by(
            self.description.as_ref(),
            field_status.as_ref(),
            self.disabled
                .then_some(self.disabled_message.as_ref())
                .flatten(),
            &i18n,
        );
        let (name, described) = match &group {
            Some(group) => (
                SharedString::from(format!("{} {}", group.label, self.label)),
                group
                    .described
                    .iter()
                    .cloned()
                    .chain(described.drain(..))
                    .collect::<Vec<_>>(),
            ),
            None => (self.label.clone(), described),
        };
        let style = EditorStyle {
            text_color: if rejected {
                t.color.text_secondary
            } else {
                t.color.text_primary
            },
            placeholder: Some(placeholder.clone()),
            placeholder_color: t.color.text_secondary,
            selection_color: selection_color(t),
            caret_color: t.color.text_primary,
            multiline: false,
            min_lines: 1,
            max_lines: Some(1),
            height: None,
            reduced_motion: prefers_reduced_motion(cx),
            spelling_color: t.color.error,
        };
        let mut editor = text_editor("editor", &editor_state, style, cx)
            .flex_1()
            .min_w_0()
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .role(Role::TextInput)
            .aria_label(name)
            .aria_value(SharedString::from(display.clone()))
            .aria_disabled(self.disabled)
            .aria_busy(busy)
            .aria_required(crate::field::resolve(self.required, self.optional, cx).required)
            .aria_invalid(invalid)
            .aria_placeholder(placeholder);
        if !described.is_empty() {
            editor = editor.aria_description(SharedString::from(described.join(" ")));
        }
        control = control.child(editor);
        if busy {
            control = control.child(
                Spinner::new(ElementId::Name(format!("{}-spinner", self.id).into()))
                    .size(SpinnerSize::Sm)
                    .shade(SpinnerShade::Inherit)
                    .decorative(),
            );
        }
        if self.clear && shown_value.is_some() && !self.disabled {
            let label = i18n.format(
                "@astryx.timeInput.clearLabel",
                &[("label", self.label.as_ref().into())],
            );
            let (ctx, focus) = (ctx.clone(), focus.clone());
            control = control.child(provide(
                Disclosure::detached(),
                IconButton::new(
                    ElementId::Name(format!("{}-clear", self.id).into()),
                    label,
                    Icon::new("close")
                        .size(IconSize::Sm)
                        .color(IconColor::Secondary),
                )
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
                .tooltip(i18n.t("@astryx.inputClearButton.tooltip"))
                .h(px(20.))
                .on_click(move |_, window, cx| {
                    ctx.fire(None, window, cx);
                    window.focus(&focus, cx);
                }),
            ));
        }
        if let Some(status) = status
            && self.status_placement != StatusPlacement::Detached
            && place.is_none()
        {
            let message = field_status.as_ref().and_then(|s| s.message.clone());
            control = control.child(status_indicator(
                &self.id,
                status,
                self.status_placement,
                message.as_ref(),
                &part,
                window,
                cx,
            ));
        }

        let control = match part_override {
            Some(part) => scope(part, control).into_any_element(),
            None => control.into_any_element(),
        };
        let control = crate::overlay::disabled_reason(
            &self.id,
            self.disabled,
            self.disabled_message.as_ref(),
            true,
            control,
        );
        if place.is_some() {
            return inside_member(control).into_any_element();
        }
        let mut field = Field::new(self.id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(self.disabled)
            .optional(self.optional)
            .required(self.required)
            .status_placement(self.status_placement);
        if let Some(status) = field_status {
            field = field.status(status);
        }
        if let Some(tooltip) = self.label_tooltip {
            field = field.label_tooltip(tooltip);
        }
        if let Some(width) = self.width {
            field = field.width(width);
        }
        field.style().refine(&self.style);
        field.child(control).into_any_element()
    }
}
