//! DateInput, upstream `DateInput/DateInput.tsx`: a text field you can type
//! a date into in the field's calendar, with a Calendar in a popover
//! (the pointer surface), FR1-FR25.
//!
//! Typing edits a draft that reports `on_change` only for a complete date,
//! once per date (AST-111 FR12); Enter or blur commits the rest. Rejected
//! text stays in the field with its message as the field's status
//! (AST-118), revealed when the entry error behavior says (AST-117).

use std::rc::Rc;

use astryx_i18n::date::{DateParse, PlainDate, Weekday};
use astryx_i18n::{CalendarSystem, current as i18n, use_direction};
use astryx_primitives::editing::{EditorStyle, FieldSettings, FocusFn, TextEditState, text_editor};
use astryx_primitives::{
    ElementSize, Placement, Politeness, announce, control_focus, is_focus_visible,
    prefers_reduced_motion, provide, resolve_size,
};
use astryx_theme::{ThemeStyled, current, scope};
use gpui::{
    AnyElement, App, ElementId, Focusable, IntoElement, KeyDownEvent, Length, MouseButton,
    RenderOnce, Role, SharedString, StyleRefinement, Styled, Task, Window, div, prelude::*, px,
};

use super::presentation::{
    LegacyNativePicker, PickerPresentation, Surface, calendar_picker, coarse_pointer,
    effective_picker, resolve_picker,
};
use super::typed::{
    DateInputEntryError, DateInputFormat, IsDisabled, TypedDate, TypedState, is_accepted,
    shown_rejection,
};
use crate::button::{ButtonVariant, IconButton};
use crate::calendar::{Calendar, CalendarHandle, DateConstraint};
use crate::choice::action::{ActionFn, Optimistic};
use crate::field::entry_report::{CallerErrorPrecedence, EntryReport, ReportFn, ResolveInput};
use crate::field::{EntryErrorBehavior, Field, InputStatus, StatusPlacement, entry_error_behavior};
use crate::icon::{Icon, IconColor, IconSize};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::input::group::{group_context, group_disabled, group_place, inside_member};
use crate::input::selection_color;
use crate::overlay::{Disclosure, Popover, PopoverController};
use crate::press::{Press, apply_press};
use crate::status::{Spinner, SpinnerShade, SpinnerSize};

type ValueFn = Rc<dyn Fn(Option<PlainDate>, &mut Window, &mut App)>;

/// A date field with a calendar, upstream `DateInput`.
///
/// ```ignore
/// DateInput::new("due", "Due date", due).on_change(|date, _, _| println!("{date:?}"))
/// ```
#[derive(IntoElement)]
pub struct DateInput {
    id: ElementId,
    label: SharedString,
    value: Option<PlainDate>,
    on_change: Option<ValueFn>,
    change_action: Option<ActionFn<Option<PlainDate>>>,
    label_hidden: bool,
    description: Option<SharedString>,
    optional: bool,
    required: bool,
    disabled: bool,
    disabled_message: Option<SharedString>,
    loading: bool,
    min: Option<PlainDate>,
    max: Option<PlainDate>,
    date_constraints: Vec<DateConstraint>,
    placeholder: Option<SharedString>,
    size: Option<ElementSize>,
    status: Option<InputStatus>,
    on_entry_error_change: Option<ReportFn<DateInputEntryError>>,
    entry_error_behavior: Option<EntryErrorBehavior>,
    status_placement: StatusPlacement,
    label_tooltip: Option<SharedString>,
    clear: bool,
    number_of_months: u8,
    week_starts_on: Weekday,
    format: DateInputFormat,
    calendar: Option<CalendarSystem>,
    presentation: Option<PickerPresentation>,
    native_picker: Option<LegacyNativePicker>,
    width: Option<Length>,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl DateInput {
    /// Upstream `label` and the controlled `value` (`None` when empty).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<Option<PlainDate>>,
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
            date_constraints: Vec::new(),
            placeholder: None,
            size: None,
            status: None,
            on_entry_error_change: None,
            entry_error_behavior: None,
            status_placement: StatusPlacement::Attached,
            label_tooltip: None,
            clear: false,
            number_of_months: 1,
            week_starts_on: Weekday::Sunday,
            format: DateInputFormat::default(),
            calendar: None,
            presentation: None,
            native_picker: None,
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
        /// Upstream `isDisabled`: focusable, blocks typing, picking, clearing
        /// and opening (FR12).
        !disabled,
        /// Upstream `disabledMessage`.
        ?disabled_message: SharedString,
        /// Upstream `isLoading`: busy, focusable, blocks new commits.
        !loading,
        /// Upstream `min`.
        ?min: PlainDate,
        /// Upstream `max`.
        ?max: PlainDate,
        /// Upstream `placeholder` (default "Select a date"); it replaces the
        /// calendar format hint.
        ?placeholder: SharedString,
        /// Upstream `size`.
        ?size: ElementSize,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `entryErrorBehavior`: else the nearest FormLayout's
        /// default, else `Complete`.
        ?entry_error_behavior: EntryErrorBehavior,
        /// Upstream `statusVariant`.
        status_placement: StatusPlacement,
        /// Upstream `labelTooltip`.
        ?label_tooltip: SharedString,
        /// Upstream `hasClear`.
        !clear,
        /// Upstream `calendar`: else the provider's calendar.
        ?calendar: CalendarSystem,
        /// Upstream `presentation`.
        ?presentation: PickerPresentation,
        /// Upstream's deprecated `nativePicker`.
        ?native_picker: LegacyNativePicker,
        /// Upstream `width`: the whole field.
        ?width: Length,
    }

    /// Upstream `onChange`: the date, or `None` when cleared.
    pub fn on_change(
        mut self,
        f: impl Fn(Option<PlainDate>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`; the field
    /// shows the new date and is busy until the task ends (FR7).
    pub fn change_action(
        mut self,
        f: impl Fn(Option<PlainDate>, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `dateConstraints`: a date is disabled when any rule returns
    /// `false`.
    pub fn date_constraints(mut self, rules: impl IntoIterator<Item = DateConstraint>) -> Self {
        self.date_constraints.extend(rules);
        self
    }

    /// One rule of `date_constraints`.
    pub fn date_constraint(mut self, rule: impl Fn(PlainDate) -> bool + 'static) -> Self {
        self.date_constraints.push(Rc::new(rule));
        self
    }

    /// Upstream `onEntryErrorChange`: the field's rejection of typed text
    /// each time it changes, `None` when it ends.
    pub fn on_entry_error_change(
        mut self,
        f: impl Fn(Option<DateInputEntryError>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_entry_error_change = Some(Rc::new(f));
        self
    }

    /// Upstream `numberOfMonths` of the popover's calendar.
    pub fn number_of_months(mut self, months: u8) -> Self {
        self.number_of_months = if months == 2 { 2 } else { 1 };
        self
    }

    /// Upstream `weekStartsOn`.
    pub fn week_starts_on(mut self, day: impl Into<Weekday>) -> Self {
        self.week_starts_on = day.into();
        self
    }

    /// Upstream `format`: a named format, or a function of the ISO date.
    pub fn format(mut self, format: impl Into<DateInputFormat>) -> Self {
        self.format = format.into();
        self
    }

    /// Upstream `format` as a function: display only (FR14).
    pub fn format_with(mut self, f: impl Fn(PlainDate) -> String + 'static) -> Self {
        self.format = DateInputFormat::Custom(Rc::new(f));
        self
    }

    /// Upstream `controlXstyle`: styles the bordered control box.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl Styled for DateInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The field's per-frame state for its handlers.
#[derive(Clone)]
struct Ctx {
    state: gpui::Entity<FieldState>,
    typed: TypedDate,
    value: Option<PlainDate>,
    locked: bool,
    disabled: bool,
    behavior: EntryErrorBehavior,
    on_change: Option<ValueFn>,
    action: Option<ActionFn<Option<PlainDate>>>,
    optimistic: Rc<Optimistic<Option<PlainDate>>>,
    report: EntryReport<DateInputEntryError>,
    calendar_handle: CalendarHandle,
    popover: PopoverController,
}

/// DateInput's state between frames.
#[derive(Default)]
struct FieldState {
    typed: TypedState,
    last_alert: Option<SharedString>,
}

impl Ctx {
    /// Upstream `fireChange`: `on_change`, then the action (ORD3).
    fn fire(&self, value: Option<PlainDate>, window: &mut Window, cx: &mut App) {
        if self.locked {
            return;
        }
        let change = self
            .optimistic
            .change(self.on_change.clone(), self.action.clone());
        change(value, window, cx);
        window.refresh();
    }

    /// The rejection a frame shows, as reported.
    fn current_error(&self, cx: &App) -> Option<DateInputEntryError> {
        let state = &self.state.read(cx).typed;
        shown_rejection(state, &self.typed, self.locked, self.behavior).and_then(|s| {
            self.typed
                .entry_error(&s.result, true, &s.text, self.behavior)
        })
    }

    fn reset(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| s.typed.reset());
        self.report.report(None, window, cx);
    }

    /// Reports a typed date once per typing session (upstream `report`).
    fn report_date(&self, date: PlainDate, window: &mut Window, cx: &mut App) -> bool {
        let state = &self.state.read(cx).typed;
        if Some(date) == self.value || Some(date) == state.last_fired_date() {
            return false;
        }
        self.state.update(cx, |s, _| {
            s.typed.last_fired = (Some(date), s.typed.fired_epoch)
        });
        self.fire(Some(date), window, cx);
        true
    }

    /// Upstream `handleChange`: a date only once more typing cannot change
    /// it (AST-111 FR12).
    fn typed(&self, text: String, window: &mut Window, cx: &mut App) {
        if self.locked {
            return;
        }
        let result = self.typed.parse(&text);
        let empty = text.trim().is_empty();
        let error = (!empty)
            .then(|| self.typed.entry_error(&result, false, &text, self.behavior))
            .flatten();
        let shown_text = {
            let state = &self.state.read(cx).typed;
            shown_rejection(state, &self.typed, self.locked, self.behavior).map(|s| s.text)
        };
        let current = self.current_error(cx);
        let kept = if error.is_some() || empty || is_accepted(&result) {
            None
        } else {
            shown_text
        };
        let keeps = kept.is_some();
        self.state.update(cx, |s, _| {
            s.typed.pending = Some(text);
            s.typed.rejection_committed = false;
            s.typed.kept_text = kept;
        });
        self.report
            .report(error.or(if keeps { current } else { None }), window, cx);
        if let DateParse::Value {
            date,
            is_complete: true,
        } = result
            && self.report_date(date, window, cx)
        {
            // The open calendar shows the typed date's month.
            self.calendar_handle.navigate_to(date, window, cx);
        }
        window.refresh();
    }

    /// Upstream `commit`: Enter or blur.
    fn commit(&self, window: &mut Window, cx: &mut App) {
        let Some(pending) = self.state.read(cx).typed.pending.clone() else {
            return;
        };
        if self.locked {
            return;
        }
        if pending.trim().is_empty() {
            if self.value.is_some() {
                self.fire(None, window, cx);
            }
            self.reset(window, cx);
            window.refresh();
            return;
        }
        let result = self.typed.parse(&pending);
        if matches!(
            result,
            DateParse::Unreadable
                | DateParse::Incomplete
                | DateParse::Nonexistent { .. }
                | DateParse::OutOfRange { .. }
        ) {
            // Keep the text and say why it cannot commit; never finish or
            // adjust the date (AST-118 FR4).
            self.state.update(cx, |s, _| {
                s.typed.rejection_committed = true;
                s.typed.kept_commits += 1;
                s.typed.kept_text = None;
            });
            let error = self
                .typed
                .entry_error(&result, true, &pending, self.behavior);
            self.report.report(error, window, cx);
            window.refresh();
            return;
        }
        if let DateParse::Value { date, .. } = result {
            self.report_date(date, window, cx);
        }
        self.state
            .update(cx, |s, _| s.typed.last_fired = (None, s.typed.fired_epoch));
        self.reset(window, cx);
        window.refresh();
    }

    /// A calendar pick commits and closes the popover (FR9).
    fn pick(&self, date: PlainDate, window: &mut Window, cx: &mut App) {
        if self.disabled {
            return;
        }
        self.fire(Some(date), window, cx);
        self.reset(window, cx);
        self.popover.hide(window, cx);
    }
}

impl RenderOnce for DateInput {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.disabled |= group_disabled(cx);
        let place = group_place(cx);
        let group = place.and_then(|_| group_context(cx));
        let dir = use_direction(cx);
        let theme = current(window, cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let i18n = i18n(cx);
        let calendar = self.calendar.unwrap_or_else(|| i18n.calendar_system());

        // ORD1: the surface. The bottom sheet and the platform picker land in
        // later M8 stages; until then every surface is the pointer field.
        let policy = calendar_picker(
            effective_picker(self.presentation, self.native_picker, "DateInput"),
            calendar,
            "DateInput",
        );
        let _surface: Surface = resolve_picker(policy, coarse_pointer(window));

        let optimistic = Rc::new(Optimistic::<Option<PlainDate>>::new(&self.id, window, cx));
        let shown_value = optimistic.shown(self.value, cx);
        let busy = self.loading || optimistic.pending(cx);
        let locked = self.disabled || busy;
        optimistic.announce_busy(busy, i18n.t("@astryx.spinner.loading"), window, cx);

        let (min, max) = (self.min, self.max);
        let rules = self.date_constraints.clone();
        let is_disabled: IsDisabled = Rc::new(move |date: PlainDate| {
            min.is_some_and(|m| date < m)
                || max.is_some_and(|m| date > m)
                || rules.iter().any(|rule| !rule(date))
        });
        let typed = TypedDate::new(
            calendar,
            self.format.clone(),
            self.min,
            self.max,
            is_disabled,
            &i18n,
        );
        let behavior = entry_error_behavior(self.entry_error_behavior, cx);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-date-input", self.id).into()),
            cx,
            |_, _| FieldState::default(),
        );
        let calendar_handle = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-calendar-handle", self.id).into()),
                cx,
                |_, _| CalendarHandle::new(),
            )
            .read(cx)
            .clone();
        let popover_id = ElementId::Name(format!("{}-popover", self.id).into());
        let popover = PopoverController::new(popover_id.clone(), window, cx);

        // A new value drops typed text and retires the reported date; a
        // re-enabled field shows its value for text typing already reported
        // (FR12, AST-118 FR6).
        let value = self.value;
        let disabled = locked;
        let typed_for_state = typed.clone();
        state.update(cx, |s, _| {
            let t = &mut s.typed;
            let replaced = t.replaces.check(&value, t.kept_commits);
            if t.prev_value != Some(value) {
                let first = t.prev_value.is_none();
                t.prev_value = Some(value);
                if !first {
                    let retires = value != t.last_fired_date();
                    if retires {
                        t.fired_epoch += 1;
                    }
                    if t.pending.is_some() && replaced && (retires || value.is_none()) {
                        t.reset();
                    }
                }
            }
            if disabled != t.was_disabled {
                t.was_disabled = disabled;
                let reported = t.pending.as_deref().map(|p| typed_for_state.parse(p));
                if !disabled
                    && !t.focused
                    && matches!(reported, Some(DateParse::Value { date, .. }) if Some(date) == t.last_fired_date())
                {
                    t.fired_epoch += 1;
                    t.reset();
                }
            }
        });

        let report = EntryReport::<DateInputEntryError>::new(&self.id, window, cx);
        let shown = shown_rejection(&state.read(cx).typed, &typed, locked, behavior);
        let rejection_message = shown.as_ref().map(|s| typed.error_message(&s.result));
        let entry_error = shown
            .as_ref()
            .and_then(|s| typed.entry_error(&s.result, true, &s.text, behavior));
        let resolved = report.resolve(
            ResolveInput {
                entry_error,
                rejection_message,
                status: self.status.as_ref(),
                status_announced: place.is_none(),
                precedence: CallerErrorPrecedence::WithCallback,
                callback: self.on_entry_error_change.clone(),
            },
            window,
            cx,
        );
        let field_status = resolved.status;
        // The field's alert: the message no shown status announces.
        let alert_changed = state.update(cx, |s, _| {
            let changed = s.last_alert != resolved.alert;
            s.last_alert = resolved.alert.clone();
            changed
        });
        if alert_changed && let Some(alert) = resolved.alert.clone() {
            window.defer(cx, move |window, cx| {
                announce(alert, Politeness::Assertive, window, cx)
            });
        }

        let ctx = Ctx {
            state: state.clone(),
            typed: typed.clone(),
            value: self.value,
            locked,
            disabled: self.disabled,
            behavior,
            on_change: self.on_change.clone(),
            action: self.change_action.clone(),
            optimistic: optimistic.clone(),
            report: report.clone(),
            calendar_handle: calendar_handle.clone(),
            popover: popover.clone(),
        };

        // ORD2: typed text while editing, else the committed date.
        let (pending, focused_state) = {
            let s = &state.read(cx).typed;
            (s.pending.clone(), s.focused)
        };
        let committed = shown_value
            .map(|v| typed.format_committed(v))
            .unwrap_or_default();
        let display = pending.clone().unwrap_or(committed);
        let format_hint = (self.placeholder.is_none() && focused_state && display.is_empty())
            .then(|| typed.format_hint())
            .flatten();
        let placeholder = format_hint
            .or(self.placeholder.clone())
            .unwrap_or_else(|| i18n.t("@astryx.dateInput.placeholder"));

        let editor_state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-editor", self.id).into()),
            cx,
            |window, cx| TextEditState::reporting("", false, window, cx),
        );
        let focus = editor_state.read(cx).focus_handle(cx);
        let focused = focus.is_focused(window);
        let open = popover.is_open(cx);
        editor_state.update(cx, |s, cx| {
            s.set_text(&display, cx);
            s.set_settings(FieldSettings {
                disabled: self.disabled,
                read_only: locked,
                masked: false,
                filter: None,
                spell_check: false,
                keyboard: Default::default(),
            });
            s.on_change = (!locked).then(|| {
                let ctx = ctx.clone();
                Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                    ctx.typed(text, window, cx)
                }) as astryx_primitives::editing::ChangeFn
            });
            // FR16: Enter commits typed text.
            s.on_enter = Some({
                let ctx = ctx.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| ctx.commit(window, cx)) as FocusFn
            });
            s.on_blur = Some({
                let ctx = ctx.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    ctx.state.update(cx, |s, _| s.typed.focused = false);
                    ctx.commit(window, cx);
                })
            });
            s.on_focus = Some({
                let ctx = ctx.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    ctx.state.update(cx, |s, _| s.typed.focused = true);
                    ctx.typed.prepare();
                    window.refresh();
                })
            });
            // AR2: ArrowDown and Alt+ArrowDown open the calendar, keeping the
            // caret; Escape closes it.
            s.on_key_down = Some({
                let (ctx, editor) = (ctx.clone(), cx.entity());
                Rc::new(
                    move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                        let composing = editor.read(cx).buffer().marked().is_some();
                        if composing {
                            return;
                        }
                        let keys = &event.keystroke;
                        match keys.key.as_str() {
                            "down"
                                if !keys.modifiers.control
                                    && !keys.modifiers.platform
                                    && !keys.modifiers.shift =>
                            {
                                cx.stop_propagation();
                                if !ctx.locked && !ctx.popover.is_open(cx) {
                                    ctx.popover.show_without_focus(window, cx);
                                }
                            }
                            "escape" if ctx.popover.is_open(cx) => {
                                cx.stop_propagation();
                                ctx.popover.hide(window, cx);
                            }
                            _ => {}
                        }
                    },
                ) as astryx_primitives::editing::KeyDownFn
            });
        });

        let status = field_status.as_ref().map(|s| s.status);
        let chrome = Chrome {
            id: ElementId::Name(format!("{}-control", self.id).into()),
            target: "date-input",
            variant: None,
            size,
            status,
            disabled: self.disabled,
            read_only: false,
            focused: focused || open,
            focus_visible: is_focus_visible(&focus, window),
            row: true,
            place,
            dir,
        };
        let part_override = theme.for_part("date-input", &chrome.props());
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let rem = window.rem_size();
        let mut control = control_box(&chrome, &part, window, cx);
        control = control.min_w(px(180.));
        control.style().refine(&self.control_style);
        // FR8: the field's own focus handle, to tell when focus leaves it.
        let field_focus = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-field-focus", self.id).into()),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        control = control.track_focus(&field_focus.clone().tab_stop(false));
        // The browser's text-field context menu.
        control = crate::input::field_menu::with_field_menu(
            control,
            &self.id,
            &editor_state,
            &focus,
            self.disabled,
            window,
            cx,
        );

        // The calendar toggle, before the text (upstream's `<button>`).
        let toggle_id = ElementId::Name(format!("{}-toggle", self.id).into());
        let toggle_focus = control_focus(&toggle_id, window, cx);
        let toggle_label = if open {
            i18n.t("@astryx.dateInput.toggleCalendarClose")
        } else {
            i18n.t("@astryx.dateInput.openCalendar")
        };
        let mut glyph = div().flex().child(
            Icon::new("calendar")
                .size(IconSize::Sm)
                .color(IconColor::Secondary),
        );
        part.refine(
            glyph.style(),
            "date-input-toggle-icon",
            &[("state", if open { "expanded" } else { "collapsed" })],
            None,
        );
        let toggle_ctx = ctx.clone();
        let toggle = div()
            .id(toggle_id.clone())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(t.radius.element)
            .role(Role::Button)
            .aria_label(toggle_label)
            .aria_expanded(open)
            .aria_has_popup(gpui::HasPopup::Dialog)
            .aria_disabled(locked)
            .child(glyph);
        let toggle = apply_press(
            toggle,
            &toggle_focus,
            Press {
                fill: None,
                scale: None,
                disabled: false,
                busy: locked,
                ring: Some((t.radius.element.to_pixels(rem), None)),
                raise_ring: false,
                theme_target: None,
            },
            Some(Rc::new(move |_, window, cx| {
                if !toggle_ctx.locked {
                    toggle_ctx.popover.toggle(window, cx);
                }
            })),
            &part,
            window,
            cx,
        );
        control = control.child(toggle);

        // The input: a combobox with a dialog popup (AR2).
        let shown_invalid = shown.is_some();
        let invalid = status == Some(crate::field::StatusType::Error) || shown_invalid;
        let mut described = described_by(
            self.description.as_ref(),
            field_status.as_ref(),
            self.disabled
                .then_some(self.disabled_message.as_ref())
                .flatten(),
            &i18n,
        );
        if let Some(name) = typed.calendar_description() {
            described.insert(usize::from(self.description.is_some()), name);
        }
        let (name, described) = match &group {
            Some(group) => (
                SharedString::from(format!("{} {}", group.label, self.label)),
                group.described.iter().cloned().chain(described).collect(),
            ),
            None => (self.label.clone(), described),
        };
        let style = EditorStyle {
            text_color: if shown_invalid {
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
        let open_ctx = ctx.clone();
        let mut editor = text_editor("editor", &editor_state, style, cx)
            .flex_1()
            .min_w_0()
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .role(Role::ComboBox)
            .aria_label(name)
            .aria_value(SharedString::from(display.clone()))
            .aria_disabled(locked)
            .aria_busy(busy)
            .aria_expanded(open)
            .aria_has_popup(gpui::HasPopup::Dialog)
            .aria_required(crate::field::resolve(self.required, self.optional, cx).required)
            .aria_invalid(invalid)
            .aria_placeholder(placeholder)
            // FR8: a press in the input opens the calendar and keeps the caret.
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                if !open_ctx.locked && !open_ctx.popover.is_open(cx) {
                    open_ctx.popover.show_without_focus(window, cx);
                }
            });
        if !described.is_empty() {
            editor = editor.aria_description(SharedString::from(described.join(" ")));
        }
        control = control.child(editor);

        if self.clear && self.value.is_some() && !locked {
            let label = i18n.format(
                "@astryx.dateInput.clear",
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
                    // ORD4: the clear goes through the change pipeline, then
                    // focus returns to the input.
                    ctx.fire(None, window, cx);
                    ctx.reset(window, cx);
                    window.focus(&focus, cx);
                }),
            ));
        }
        if busy {
            control = control.child(
                Spinner::new(ElementId::Name(format!("{}-spinner", self.id).into()))
                    .size(SpinnerSize::Sm)
                    .shade(SpinnerShade::Inherit)
                    .decorative(),
            );
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

        // FR8: focus leaving the field and its calendar closes the calendar;
        // focus going nowhere (another window) leaves it open.
        if open
            && window.focused(cx).is_some()
            && !field_focus.contains_focused(window, cx)
            && !popover.contains_focus(window, cx)
        {
            let popover = popover.clone();
            window.defer(cx, move |window, cx| popover.hide(window, cx));
        }

        let calendar_el = {
            let ctx = ctx.clone();
            let id = self.id.clone();
            let handle = calendar_handle.clone();
            let (min, max, months, week) = (
                self.min,
                self.max,
                self.number_of_months,
                self.week_starts_on,
            );
            let rules = self.date_constraints.clone();
            move |_: &mut Window, _: &mut App| {
                let pick = ctx.clone();
                let mut calendar_el =
                    Calendar::new(ElementId::Name(format!("{id}-calendar").into()))
                        .calendar(calendar)
                        .value(shown_value)
                        .handle(handle);
                if let Some(min) = min {
                    calendar_el = calendar_el.min(min);
                }
                if let Some(max) = max {
                    calendar_el = calendar_el.max(max);
                }
                calendar_el
                    .date_constraints(rules)
                    .number_of_months(months)
                    .week_starts_on(week)
                    .on_change(move |date, window, cx| pick.pick(date, window, cx))
                    .into_any_element()
            }
        };
        let control = Popover::new(popover_id, i18n.t("@astryx.dateInput.dialogLabel"))
            .controller(popover.clone())
            .placement(Placement::Below)
            .close_button(true)
            .close_button_label(i18n.t("@astryx.dateInput.closeCalendar"))
            .auto_focus(true)
            .light_dismiss(true)
            .return_focus(focus.clone())
            .padding(crate::layout::SpacingStep::S0)
            .content_fn(calendar_el)
            .child(control);

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

/// The element a test reads by its debug selector.
#[allow(dead_code)]
pub(crate) fn editor_selector(id: &str) -> String {
    format!("{id}-editor")
}

#[allow(dead_code)]
fn _unused(_: AnyElement) {}
