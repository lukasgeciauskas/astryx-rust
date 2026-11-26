//! DateTimeInput, upstream `DateTimeInput/DateTimeInput.tsx`: a date segment
//! and a time segment under one label (the pointer surface), FR1-FR16.
//!
//! The date segment types and picks dates as DateInput's field does; the
//! time segment types and steps times as TimeInput's does, with an optional
//! listbox of preset times. Every change emits one combined date-time;
//! changing the time before a date exists emits nothing (FR2).

use std::rc::Rc;

use astryx_i18n::date::{
    HourFormat, PlainDate, PlainDateTime, PlainTime, Weekday, adjust_time, format_locale_time,
    is_time_in_range, parse_locale_time, resolve_hour_format,
};
use astryx_i18n::{CalendarSystem, current as i18n, use_direction};
use astryx_primitives::editing::{EditorStyle, FieldSettings, FocusFn, TextEditState, text_editor};
use astryx_primitives::{
    ElementSize, Placement, Politeness, announce, control_focus, is_focus_visible,
    prefers_reduced_motion, provide, resolve_size,
};
use astryx_theme::{ResolvedTheme, ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, Entity, FocusHandle, Focusable, IntoElement, KeyDownEvent, Length,
    MouseButton, RenderOnce, Role, SharedString, StyleRefinement, Styled, Task, Window, div,
    prelude::*, px,
};

use crate::button::{ButtonVariant, IconButton};
use crate::calendar::{Calendar, CalendarHandle, DateConstraint};
use crate::choice::action::{ActionFn, Optimistic};
use crate::date_input::presentation::{
    LegacyNativePicker, PickerPresentation, Surface, calendar_picker, coarse_pointer,
    effective_picker, resolve_picker,
};
use crate::date_input::segment::{DateSegment, SegmentProps};
use crate::date_input::typed::{DateInputEntryError, DateInputFormat, IsDisabled, TypedDate};
use crate::field::entry_report::{CallerErrorPrecedence, EntryReport, ReportFn, ResolveInput};
use crate::field::{
    EntryErrorBehavior, Field, InputStatus, StatusPlacement, StatusType, entry_error_behavior,
};
use crate::icon::{Icon, IconColor, IconDefaultSize, IconSize};
use crate::input::control::{Chrome, control_box, described_by};
use crate::input::selection_color;
use crate::overlay::{Disclosure, Popover, PopoverController, PopoverRole};
use crate::press::{Press, apply_press};
use crate::status::{Spinner, SpinnerShade, SpinnerSize};
use crate::time_input::typed::{TimeInputEntryError, TimeRules, Trigger, TypedTimeState};

type ValueFn = Rc<dyn Fn(Option<PlainDateTime>, &mut Window, &mut App)>;

/// A bound of a date-time field: a date, with a time on that date or not
/// (upstream's `min` and `max`, with or without their `T` part).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTimeBound {
    pub date: PlainDate,
    pub time: Option<PlainTime>,
}

impl From<PlainDate> for DateTimeBound {
    fn from(date: PlainDate) -> Self {
        Self { date, time: None }
    }
}

impl From<PlainDateTime> for DateTimeBound {
    fn from(dt: PlainDateTime) -> Self {
        Self {
            date: dt.date,
            time: Some(dt.time),
        }
    }
}

/// Upstream `timeOptionInterval`: the preset list's cadence in minutes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeOptionInterval {
    Five = 5,
    Ten = 10,
    Fifteen = 15,
    Thirty = 30,
    Sixty = 60,
}

const MINUTES_PER_DAY: u32 = 24 * 60;
/// Two segments and the 8px gap fit at 400px; below that they wrap (FR5).
const SEGMENT_BASIS: f32 = 196.;

/// A combined date and time field, upstream `DateTimeInput`.
///
/// ```ignore
/// DateTimeInput::new("meeting", "Meeting", value).on_change(|dt, _, _| println!("{dt:?}"))
/// ```
#[derive(IntoElement)]
pub struct DateTimeInput {
    pub(super) id: ElementId,
    pub(super) label: SharedString,
    pub(super) value: Option<PlainDateTime>,
    pub(super) on_change: Option<ValueFn>,
    pub(super) change_action: Option<ActionFn<Option<PlainDateTime>>>,
    pub(super) label_hidden: bool,
    pub(super) description: Option<SharedString>,
    pub(super) optional: bool,
    pub(super) required: bool,
    pub(super) disabled: bool,
    pub(super) disabled_message: Option<SharedString>,
    pub(super) loading: bool,
    pub(super) min: Option<DateTimeBound>,
    pub(super) max: Option<DateTimeBound>,
    pub(super) date_constraints: Vec<DateConstraint>,
    pub(super) seconds: bool,
    pub(super) hour_format: Option<HourFormat>,
    pub(super) time_increment: i64,
    pub(super) time_option_interval: Option<TimeOptionInterval>,
    pub(super) clear: bool,
    pub(super) placeholder: Option<SharedString>,
    pub(super) calendar: Option<CalendarSystem>,
    pub(super) time_placeholder: Option<SharedString>,
    pub(super) time_label: Option<SharedString>,
    pub(super) size: Option<ElementSize>,
    pub(super) status: Option<InputStatus>,
    pub(super) on_entry_error_change: Option<ReportFn<DateInputEntryError>>,
    pub(super) entry_error_behavior: Option<EntryErrorBehavior>,
    pub(super) label_tooltip: Option<SharedString>,
    pub(super) number_of_months: u8,
    pub(super) week_starts_on: Weekday,
    pub(super) presentation: Option<PickerPresentation>,
    pub(super) native_picker: Option<LegacyNativePicker>,
    pub(super) width: Option<Length>,
    pub(super) control_style: StyleRefinement,
    pub(super) style: StyleRefinement,
}

impl DateTimeInput {
    /// Upstream `label` and the controlled `value` (`None` when empty).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<Option<PlainDateTime>>,
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
            seconds: false,
            hour_format: None,
            time_increment: 1,
            time_option_interval: None,
            clear: false,
            placeholder: None,
            calendar: None,
            time_placeholder: None,
            time_label: None,
            size: None,
            status: None,
            on_entry_error_change: None,
            entry_error_behavior: None,
            label_tooltip: None,
            number_of_months: 1,
            week_starts_on: Weekday::Sunday,
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
        /// Upstream `isDisabled` (FR9).
        !disabled,
        /// Upstream `disabledMessage`.
        ?disabled_message: SharedString,
        /// Upstream `isLoading`.
        !loading,
        /// Upstream `hasSeconds`.
        !seconds,
        /// Upstream `hourFormat`: else the locale's hour cycle.
        ?hour_format: HourFormat,
        /// Upstream `timeOptionInterval`: a listbox of preset times.
        ?time_option_interval: TimeOptionInterval,
        /// Upstream `hasClear`.
        !clear,
        /// Upstream `placeholder` of the date segment.
        ?placeholder: SharedString,
        /// Upstream `calendar`.
        ?calendar: CalendarSystem,
        /// Upstream `timePlaceholder`.
        ?time_placeholder: SharedString,
        /// Upstream `timeLabel`: else "{label} time".
        ?time_label: SharedString,
        /// Upstream `size`.
        ?size: ElementSize,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `entryErrorBehavior` of both segments (FR15).
        ?entry_error_behavior: EntryErrorBehavior,
        /// Upstream `labelTooltip`.
        ?label_tooltip: SharedString,
        /// Upstream `presentation`.
        ?presentation: PickerPresentation,
        /// Upstream's deprecated `nativePicker`.
        ?native_picker: LegacyNativePicker,
        /// Upstream `width`.
        ?width: Length,
    }

    /// Upstream `min`: a date, or a date and time.
    pub fn min(mut self, bound: impl Into<DateTimeBound>) -> Self {
        self.min = Some(bound.into());
        self
    }

    /// Upstream `max`: a date, or a date and time.
    pub fn max(mut self, bound: impl Into<DateTimeBound>) -> Self {
        self.max = Some(bound.into());
        self
    }

    /// Upstream `timeIncrement`: minutes the arrow keys step.
    pub fn time_increment(mut self, minutes: i64) -> Self {
        self.time_increment = minutes;
        self
    }

    /// Upstream `onChange`: the combined date-time, `None` when cleared.
    pub fn on_change(
        mut self,
        f: impl Fn(Option<PlainDateTime>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction` (FR4).
    pub fn change_action(
        mut self,
        f: impl Fn(Option<PlainDateTime>, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `dateConstraints`.
    pub fn date_constraints(mut self, rules: impl IntoIterator<Item = DateConstraint>) -> Self {
        self.date_constraints.extend(rules);
        self
    }

    /// One rule of `date_constraints`.
    pub fn date_constraint(mut self, rule: impl Fn(PlainDate) -> bool + 'static) -> Self {
        self.date_constraints.push(Rc::new(rule));
        self
    }

    /// Upstream `onEntryErrorChange`: the date segment's rejections (FR11).
    pub fn on_entry_error_change(
        mut self,
        f: impl Fn(Option<DateInputEntryError>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_entry_error_change = Some(Rc::new(f));
        self
    }

    /// Upstream `numberOfMonths` of the date popover.
    pub fn number_of_months(mut self, months: u8) -> Self {
        self.number_of_months = if months == 2 { 2 } else { 1 };
        self
    }

    /// Upstream `weekStartsOn`.
    pub fn week_starts_on(mut self, day: impl Into<Weekday>) -> Self {
        self.week_starts_on = day.into();
        self
    }

    /// Upstream `controlXstyle`: the row holding both segments.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl Styled for DateTimeInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[derive(Default)]
struct TimeState {
    typed: TypedTimeState,
    focused: bool,
    /// The preset list's highlighted option.
    highlighted: Option<usize>,
    /// The highlight still follows what was typed.
    follows_typing: bool,
    last_alert: Option<SharedString>,
}

/// The field's behavior, shared by its handlers.
#[derive(Clone)]
struct Ctx {
    value: Option<PlainDateTime>,
    locked: bool,
    seconds: bool,
    min: Option<DateTimeBound>,
    max: Option<DateTimeBound>,
    on_change: Option<ValueFn>,
    action: Option<ActionFn<Option<PlainDateTime>>>,
    optimistic: Rc<Optimistic<Option<PlainDateTime>>>,
    /// A native time chosen before any date.
    time_draft: Option<PlainTime>,
    date_popover: PopoverController,
    time_popover: PopoverController,
    time: Entity<TimeState>,
    time_rules: TimeRules,
    time_report: EntryReport<TimeInputEntryError>,
    behavior: EntryErrorBehavior,
    options: Rc<Vec<(PlainTime, String)>>,
    time_increment: i64,
    time_focus: FocusHandle,
}

impl Ctx {
    /// Upstream `fireChange` (ORD2).
    fn fire(&self, value: Option<PlainDateTime>, window: &mut Window, cx: &mut App) {
        if self.locked {
            return;
        }
        let change = self
            .optimistic
            .change(self.on_change.clone(), self.action.clone());
        change(value, window, cx);
        window.refresh();
    }

    /// A new date keeps the time, clamped into a bound on that date; an
    /// empty field takes the current time (upstream `handleDateChange`).
    fn date_changed(
        &self,
        date: PlainDate,
        from_calendar: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut time = self
            .value
            .map(|v| v.time)
            .or(self.time_draft)
            .unwrap_or_else(|| PlainTime::now().truncated(self.seconds));
        if let Some(DateTimeBound {
            date: d,
            time: Some(t),
        }) = self.min
            && d == date
            && time < t
        {
            time = t;
        }
        if let Some(DateTimeBound {
            date: d,
            time: Some(t),
        }) = self.max
            && d == date
            && time > t
        {
            time = t;
        }
        self.fire(Some(PlainDateTime::new(date, time)), window, cx);
        if from_calendar {
            self.date_popover.hide(window, cx);
        }
    }

    /// A typed time joins the date; an emptied time keeps the value (a
    /// date-time is never half cleared).
    fn time_committed(&self, time: Option<Option<PlainTime>>, window: &mut Window, cx: &mut App) {
        if let (Some(Some(time)), Some(value)) = (time, self.value)
            && time != value.time
        {
            self.fire(Some(PlainDateTime::new(value.date, time)), window, cx);
        }
    }

    fn report_time(
        &self,
        error: Option<Option<TimeInputEntryError>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(error) = error
            && !self.locked
        {
            self.time_report.report(error, window, cx);
        }
    }

    fn time_typed(&self, text: String, window: &mut Window, cx: &mut App) {
        if self.locked {
            return;
        }
        let value_time = self.value.map(|v| v.time);
        let (time, report) = self.time.update(cx, |s, _| {
            s.typed
                .change(text.clone(), &self.time_rules, value_time, self.behavior)
        });
        self.report_time(report, window, cx);
        // The list follows what is typed, so Enter lands where expected.
        if self.time_popover.is_open(cx)
            && let Some(parsed) = parse_locale_time(&text, &self.time_rules.locale, self.seconds)
        {
            let index = self.closest_option(Some(parsed));
            self.time.update(cx, |s, _| {
                s.follows_typing = true;
                s.highlighted = index;
            });
        }
        self.time_committed(time.map(Some), window, cx);
        window.refresh();
    }

    fn time_commit(&self, trigger: Trigger, window: &mut Window, cx: &mut App) {
        if self.locked {
            return;
        }
        let value_time = self.value.map(|v| v.time);
        let (time, report) = self.time.update(cx, |s, _| {
            s.typed.commit(trigger, &self.time_rules, value_time)
        });
        self.report_time(report, window, cx);
        self.time_committed(time, window, cx);
        window.refresh();
    }

    /// The option at or just before `time` (`closestOptionIndex`).
    fn closest_option(&self, time: Option<PlainTime>) -> Option<usize> {
        if self.options.is_empty() {
            return None;
        }
        let Some(time) = time else {
            return Some(0);
        };
        Some(
            self.options
                .iter()
                .rposition(|(t, _)| *t <= time)
                .unwrap_or(0),
        )
    }

    fn selected_option(&self) -> Option<PlainTime> {
        self.value.map(|v| v.time.truncated(self.seconds))
    }

    fn show_options(&self, window: &mut Window, cx: &mut App) {
        if self.locked || self.options.is_empty() || self.time_popover.is_open(cx) {
            return;
        }
        let index = self.closest_option(self.selected_option());
        self.time.update(cx, |s, _| {
            s.highlighted = index;
            s.follows_typing = true;
        });
        window.focus(&self.time_focus, cx);
        self.time_popover.show_without_focus(window, cx);
    }

    /// A time picked from the list: the same path as a typed one (FR7).
    fn commit_option(&self, time: PlainTime, window: &mut Window, cx: &mut App) {
        if self.locked || !is_time_in_range(time, self.time_rules.min, self.time_rules.max) {
            return;
        }
        self.time.update(cx, |s, _| s.typed.drop_draft());
        self.time_report.report(None, window, cx);
        if Some(time) != self.selected_option()
            && let Some(value) = self.value
        {
            self.fire(Some(PlainDateTime::new(value.date, time)), window, cx);
        }
        self.time_popover.hide(window, cx);
        window.focus(&self.time_focus, cx);
    }

    /// ArrowUp and ArrowDown step the time within the bounds and announce it.
    fn step(&self, up: bool, window: &mut Window, cx: &mut App) {
        let Some(value) = self.value else {
            return;
        };
        let from = value.time;
        let delta = if up {
            self.time_increment
        } else {
            -self.time_increment
        };
        let next = adjust_time(from, delta, self.seconds);
        if !is_time_in_range(next, self.time_rules.min, self.time_rules.max) {
            return;
        }
        self.fire(Some(PlainDateTime::new(value.date, next)), window, cx);
        let text = format_locale_time(
            next,
            &self.time_rules.locale,
            self.time_rules.hour_format,
            self.seconds,
        );
        announce(text, Politeness::Polite, window, cx);
    }

    /// The time segment's keys: the list's when open, else steps (FR7).
    fn time_key(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App) -> bool {
        if self.locked {
            return false;
        }
        let keys = &event.keystroke;
        let open = self.time_popover.is_open(cx);
        if !self.options.is_empty() {
            match keys.key.as_str() {
                "escape" if open => {
                    self.time_popover.hide(window, cx);
                    return true;
                }
                "down" if keys.modifiers.alt => {
                    self.show_options(window, cx);
                    return true;
                }
                _ => {}
            }
            if open {
                let last = self.options.len() - 1;
                let current = self.time.read(cx).highlighted.unwrap_or(0).min(last);
                let moved = match keys.key.as_str() {
                    "down" => Some((current + 1).min(last)),
                    "up" => Some(current.saturating_sub(1)),
                    "home" => Some(0),
                    "end" => Some(last),
                    _ => None,
                };
                if let Some(index) = moved {
                    self.time.update(cx, |s, _| {
                        s.follows_typing = false;
                        s.highlighted = Some(index);
                    });
                    window.refresh();
                    return true;
                }
                match keys.key.as_str() {
                    "enter" => {
                        // Typed text wins over a highlight that only follows it.
                        let s = self.time.read(cx);
                        let typed = s
                            .follows_typing
                            .then(|| s.typed.pending.clone())
                            .flatten()
                            .and_then(|p| {
                                parse_locale_time(&p, &self.time_rules.locale, self.seconds)
                            });
                        let option = self.options.get(current).map(|(t, _)| *t);
                        if let Some(time) = typed.or(option) {
                            self.commit_option(time, window, cx);
                        }
                        return true;
                    }
                    "tab" => {
                        self.time_popover.hide(window, cx);
                        return false;
                    }
                    _ => {}
                }
            }
        }
        match keys.key.as_str() {
            "up" | "down" if !keys.modifiers.modified() => {
                self.step(keys.key == "up", window, cx);
                true
            }
            _ => false,
        }
    }
}

/// The time bounds on the selected date: a bound's time applies only on its
/// own date.
fn time_bound(bound: Option<DateTimeBound>, date: Option<PlainDate>) -> Option<PlainTime> {
    match (bound, date) {
        (
            Some(DateTimeBound {
                date: d,
                time: Some(t),
            }),
            Some(date),
        ) if d == date => Some(t),
        _ => None,
    }
}

/// A segment's control box.
struct SegmentChrome {
    id: ElementId,
    target: &'static str,
    size: ElementSize,
    status: Option<StatusType>,
    locked: bool,
    focused: bool,
    focus_visible: bool,
}

fn segment_box(
    s: SegmentChrome,
    theme: &ResolvedTheme,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Stateful<gpui::Div> {
    let dir = use_direction(cx);
    let chrome = Chrome {
        id: s.id,
        target: s.target,
        variant: None,
        size: s.size,
        status: s.status,
        disabled: s.locked,
        read_only: false,
        focused: s.focused,
        focus_visible: s.focus_visible,
        row: true,
        place: None,
        dir,
    };
    control_box(&chrome, theme, window, cx)
        .flex_1()
        .flex_basis(px(SEGMENT_BASIS))
        .min_w_0()
}

impl RenderOnce for DateTimeInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let i18n = i18n(cx);
        let locale = i18n.locale().clone();
        let calendar = self.calendar.unwrap_or_else(|| i18n.calendar_system());
        let hour_format = resolve_hour_format(&locale, self.hour_format);

        // ORD1: the surfaces. Native where the platform has a picker (R16);
        // the time segment goes native where the picker can step it: always
        // under a forced Native, else without seconds or an increment.
        let policy = calendar_picker(
            effective_picker(self.presentation, self.native_picker, "DateTimeInput"),
            calendar,
            "DateTimeInput",
        );
        let coarse = coarse_pointer(window, cx);
        let surface = crate::date_input::native::native_or_fallback(
            resolve_picker(policy, coarse),
            gpui::NativePickerKind::Date,
            coarse,
            Surface::Desktop,
            window,
        );
        if surface == Surface::Sheet {
            return super::sheet::render_sheet(self, window, cx);
        }
        let native_date = surface == Surface::Native;
        // Forced is an explicit `presentation` of Native; the adaptive and
        // deprecated policies keep the Astryx time where the picker cannot
        // step it or list presets.
        let forced = self.presentation == Some(PickerPresentation::Native)
            && policy == PickerPresentation::Native;
        let native_time = native_date
            && window.native_picker_supported(gpui::NativePickerKind::Time)
            && (forced
                || (self.time_increment == 1
                    && !self.seconds
                    && self.time_option_interval.is_none()));
        let native_rejection = window.use_keyed_state(
            ElementId::Name(format!("{:?}-native-rejection", self.id).into()),
            cx,
            |_, _| crate::date_input::native::NativeDateRejection::default(),
        );

        let optimistic = Rc::new(Optimistic::<Option<PlainDateTime>>::new(
            &self.id, window, cx,
        ));
        let shown_value = optimistic.shown(self.value, cx);
        let pending_change = optimistic.pending(cx);
        let busy = self.loading || pending_change;
        // FR9: a pending action makes the popover surface inert too; a
        // native segment keeps taking edits.
        let locked = self.disabled || self.loading || (pending_change && !native_date);
        // The two pickers are separate: a time chosen before any date waits
        // for the date's commit, and a new value drops it.
        let time_draft_state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-native-time-draft", self.id).into()),
            cx,
            |_, _| (None::<PlainTime>, shown_value),
        );
        let time_draft = time_draft_state.update(cx, |d, _| {
            if d.1 != shown_value {
                *d = (None, shown_value);
            }
            d.0
        });
        optimistic.announce_busy(busy, i18n.t("@astryx.spinner.loading"), window, cx);
        let behavior = entry_error_behavior(self.entry_error_behavior, cx);

        let (min_date, max_date) = (self.min.map(|b| b.date), self.max.map(|b| b.date));
        let rules = self.date_constraints.clone();
        let is_disabled: IsDisabled = Rc::new(move |date: PlainDate| {
            min_date.is_some_and(|m| date < m)
                || max_date.is_some_and(|m| date > m)
                || rules.iter().any(|rule| !rule(date))
        });
        let typed = TypedDate::new(
            calendar,
            DateInputFormat::default(),
            min_date,
            max_date,
            is_disabled,
            &i18n,
        );
        let shown_date = shown_value.map(|v| v.date);
        let time_min = time_bound(self.min, shown_date);
        let time_max = time_bound(self.max, shown_date);

        let date_popover_id = ElementId::Name(format!("{}-date-popover", self.id).into());
        let time_popover_id = ElementId::Name(format!("{}-time-popover", self.id).into());
        let date_popover = PopoverController::new(date_popover_id.clone(), window, cx);
        let time_popover = PopoverController::new(time_popover_id.clone(), window, cx);
        let calendar_handle = window
            .use_keyed_state(
                ElementId::Name(format!("{:?}-calendar-handle", self.id).into()),
                cx,
                |_, _| CalendarHandle::new(),
            )
            .read(cx)
            .clone();

        // The time segment: its typing state and its preset list.
        let time_rules = TimeRules {
            locale: locale.clone(),
            hour_format,
            seconds: self.seconds,
            min: time_min,
            max: time_max,
        };
        let time = window.use_keyed_state(
            ElementId::Name(format!("{:?}-time", self.id).into()),
            cx,
            |_, _| TimeState::default(),
        );
        let (time_error, time_rejected) = time.update(cx, |s, _| {
            s.typed
                .reconcile(shown_value.map(|v| v.time), false, !native_time);
            s.typed.resolve(&time_rules, locked || busy, behavior)
        });
        let options: Rc<Vec<(PlainTime, String)>> = Rc::new(match self.time_option_interval {
            Some(interval) => (0..MINUTES_PER_DAY)
                .step_by(interval as usize)
                .filter_map(|minutes| {
                    let time = PlainTime::new((minutes / 60) as u8, (minutes % 60) as u8, 0)?;
                    is_time_in_range(time, time_min, time_max).then(|| {
                        (
                            time,
                            format_locale_time(time, &locale, hour_format, self.seconds),
                        )
                    })
                })
                .collect(),
            None => Vec::new(),
        });
        let time_editor = window.use_keyed_state(
            ElementId::Name(format!("{:?}-time-editor", self.id).into()),
            cx,
            |window, cx| TextEditState::reporting("", false, window, cx),
        );
        let time_focus = time_editor.read(cx).focus_handle(cx);
        let time_report = EntryReport::<TimeInputEntryError>::new(
            &ElementId::Name(format!("{}-time", self.id).into()),
            window,
            cx,
        );

        let ctx = Ctx {
            value: shown_value,
            locked,
            seconds: self.seconds,
            min: self.min,
            max: self.max,
            on_change: self.on_change.clone(),
            action: self.change_action.clone(),
            optimistic: optimistic.clone(),
            date_popover: date_popover.clone(),
            time_popover: time_popover.clone(),
            time: time.clone(),
            time_rules: time_rules.clone(),
            time_report: time_report.clone(),
            behavior,
            options: options.clone(),
            time_increment: self.time_increment,
            time_focus: time_focus.clone(),
            time_draft,
        };

        // The date segment: DateInput's typing session (FR11).
        let segment = {
            let (on_date, on_clear) = (ctx.clone(), ctx.clone());
            let value = self.value;
            let handle = calendar_handle.clone();
            DateSegment::new(
                &self.id,
                SegmentProps {
                    typed: typed.clone(),
                    value: self.value.map(|v| v.date),
                    locked,
                    behavior,
                    on_date: Rc::new(move |date, window, cx| {
                        on_date.date_changed(date, false, window, cx)
                    }),
                    on_clear: Rc::new(move |window, cx| {
                        if value.is_some() {
                            on_clear.fire(None, window, cx);
                        }
                    }),
                    on_typed: Some(Rc::new(move |date, window, cx| {
                        handle.navigate_to(date, window, cx)
                    })),
                },
                window,
                cx,
            )
        };
        let date_shown = segment.shown(cx).is_some();
        let date_resolved = segment.resolve(
            self.status.as_ref(),
            true,
            self.on_entry_error_change.clone(),
            window,
            cx,
        );
        let time_resolved = time_report.resolve(
            ResolveInput {
                entry_error: time_error,
                rejection_message: time_rejected.then(|| i18n.t("@astryx.timeInput.invalidTime")),
                status: self.status.as_ref(),
                status_announced: true,
                precedence: CallerErrorPrecedence::Always,
                callback: None,
            },
            window,
            cx,
        );
        // FR16: the date segment's message comes first; a time rejection
        // under it speaks through the time alert.
        let date_status = date_resolved.status.clone();
        // On the native surface the date's message is its refusal.
        let native_view = native_date.then(|| {
            native_rejection.update(cx, |r, _| {
                r.resolve(
                    self.value.map(|v| v.date),
                    locked || busy,
                    self.status.as_ref(),
                    self.on_entry_error_change.is_some(),
                    true,
                    &typed,
                )
            })
        });
        if let Some(alert) = native_view.as_ref().and_then(|v| v.alert.clone()) {
            window.defer(cx, move |window, cx| {
                announce(alert, Politeness::Assertive, window, cx)
            });
        }
        let native_date_invalid = native_view.as_ref().is_some_and(|v| v.message.is_some());
        let (date_status, date_shown) = match native_view {
            Some(view) => (view.field_status, native_date_invalid),
            None => (date_status, date_shown),
        };
        let time_status = if date_shown {
            self.status.clone()
        } else {
            time_resolved.status.clone()
        };
        let shown_status = if date_shown {
            date_status.clone()
        } else {
            time_status.clone()
        };
        let time_alert = if date_shown && time_resolved.status != self.status {
            Some(i18n.t("@astryx.timeInput.invalidTime"))
        } else {
            time_resolved.alert.clone()
        };
        let alert_changed = time.update(cx, |s, _| {
            let changed = s.last_alert != time_alert;
            s.last_alert = time_alert.clone();
            changed
        });
        if alert_changed && let Some(alert) = time_alert {
            window.defer(cx, move |window, cx| {
                announce(alert, Politeness::Assertive, window, cx)
            });
        }
        // A list that empties, or a field that turns inert, closes the list.
        if time_popover.is_open(cx) && (locked || options.is_empty()) {
            let popover = time_popover.clone();
            window.defer(cx, move |window, cx| popover.hide(window, cx));
        }

        let mut described = described_by(
            self.description.as_ref(),
            shown_status.as_ref(),
            self.disabled
                .then_some(self.disabled_message.as_ref())
                .flatten(),
            &i18n,
        );
        if let Some(name) = typed.calendar_description() {
            described.insert(usize::from(self.description.is_some()), name);
        }
        let description = (!described.is_empty()).then(|| SharedString::from(described.join(" ")));
        let required = crate::field::resolve(self.required, self.optional, cx).required;

        // ---- The date segment -------------------------------------------
        let date_display = segment.display(shown_date, cx);
        let date_placeholder = segment.placeholder(
            self.placeholder.clone(),
            i18n.t("@astryx.dateTimeInput.placeholder"),
            &date_display,
            cx,
        );
        let date_editor = window.use_keyed_state(
            ElementId::Name(format!("{:?}-date-editor", self.id).into()),
            cx,
            |window, cx| TextEditState::reporting("", false, window, cx),
        );
        let date_focus = date_editor.read(cx).focus_handle(cx);
        let date_open = date_popover.is_open(cx);
        date_editor.update(cx, |s, cx| {
            s.set_text(&date_display, cx);
            s.set_settings(FieldSettings {
                disabled: self.disabled,
                read_only: locked,
                masked: false,
                filter: None,
                spell_check: false,
                keyboard: Default::default(),
            });
            s.on_change = (!locked).then(|| {
                let segment = segment.clone();
                Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                    segment.typed(text, window, cx)
                }) as astryx_primitives::editing::ChangeFn
            });
            s.on_enter = Some({
                let segment = segment.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| segment.commit(window, cx))
                    as FocusFn
            });
            s.on_blur = Some({
                let segment = segment.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    segment.set_focused(false, cx);
                    segment.commit(window, cx);
                })
            });
            s.on_focus = Some({
                let segment = segment.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    segment.set_focused(true, cx);
                    window.refresh();
                })
            });
            s.on_key_down = Some({
                let (popover, editor) = (date_popover.clone(), cx.entity());
                Rc::new(
                    move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                        if editor.read(cx).buffer().marked().is_some() {
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
                                if !locked && !popover.is_open(cx) {
                                    popover.show_without_focus(window, cx);
                                }
                            }
                            "escape" if popover.is_open(cx) => {
                                cx.stop_propagation();
                                popover.hide(window, cx);
                            }
                            _ => {}
                        }
                    },
                ) as astryx_primitives::editing::KeyDownFn
            });
        });
        let date_status_type = date_status.as_ref().map(|s| s.status);
        let mut date_box = segment_box(
            SegmentChrome {
                id: ElementId::Name(format!("{}-date-segment", self.id).into()),
                target: "date-time-input-date-segment",
                size,
                status: date_status_type,
                locked,
                focused: date_focus.is_focused(window) || date_open,
                focus_visible: is_focus_visible(&date_focus, window),
            },
            &theme,
            window,
            cx,
        );
        date_box = crate::input::field_menu::with_field_menu(
            date_box,
            &self.id,
            &date_editor,
            &date_focus,
            self.disabled,
            window,
            cx,
        );
        let toggle_id = ElementId::Name(format!("{}-toggle", self.id).into());
        let toggle_focus = control_focus(&toggle_id, window, cx);
        let mut glyph = div().flex().child(
            Icon::new("calendar")
                .size(IconSize::Sm)
                .color(IconColor::Secondary),
        );
        theme.refine(
            glyph.style(),
            "date-time-input-toggle-icon",
            &[("state", if date_open { "expanded" } else { "collapsed" })],
            None,
        );
        let toggle_popover = date_popover.clone();
        // A finger's press paints as UIKit tracks a control (upstream `usePressFeedback`).
        let toggle_feedback = crate::gestures::use_press_feedback(
            gpui::ElementId::Name(format!("{toggle_id:?}-press").into()),
            window,
            cx,
        )
        .disabled(locked)
        .space_activates(&toggle_focus);
        let toggle = apply_press(
            div()
                .id(toggle_id)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(t.radius.element)
                .role(Role::Button)
                .aria_label(if date_open {
                    i18n.t("@astryx.dateInput.toggleCalendarClose")
                } else {
                    i18n.t("@astryx.dateInput.openCalendar")
                })
                .aria_expanded(date_open)
                .aria_has_popup(gpui::HasPopup::Dialog)
                .aria_disabled(locked)
                .child(glyph),
            &toggle_focus,
            Press {
                feedback: Some(&toggle_feedback),
                fill: None,
                scale: None,
                disabled: false,
                busy: locked,
                ring: Some((t.radius.element.to_pixels(rem), None)),
                raise_ring: false,
                theme_target: None,
            },
            Some(Rc::new(move |_, window, cx| {
                if !locked {
                    toggle_popover.toggle(window, cx);
                }
            })),
            &theme,
            window,
            cx,
        );
        let toggle = toggle.child(
            toggle_feedback
                .probe()
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
        );
        let reduced_motion = prefers_reduced_motion(cx);
        let style = |rejected: bool, placeholder: SharedString| EditorStyle {
            text_color: if rejected {
                t.color.text_secondary
            } else {
                t.color.text_primary
            },
            placeholder: Some(placeholder),
            placeholder_color: t.color.text_secondary,
            selection_color: selection_color(&t),
            caret_color: t.color.text_primary,
            multiline: false,
            min_lines: 1,
            max_lines: Some(1),
            height: None,
            reduced_motion,
            spelling_color: t.color.error,
        };
        let open_popover = date_popover.clone();
        let mut date_input = text_editor(
            "date-editor",
            &date_editor,
            style(date_shown, date_placeholder.clone()),
            cx,
        )
        .flex_1()
        .min_w_0()
        .font_stack(&t.font.family.body)
        .text_style(&t.text.body)
        .role(Role::ComboBox)
        .aria_label(self.label.clone())
        .aria_value(SharedString::from(date_display.clone()))
        .aria_disabled(locked)
        .aria_busy(busy)
        .aria_expanded(date_open)
        .aria_has_popup(gpui::HasPopup::Dialog)
        .aria_required(required)
        .aria_invalid(date_status_type == Some(StatusType::Error) || date_shown)
        .aria_placeholder(date_placeholder)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if !locked && !open_popover.is_open(cx) {
                open_popover.show_without_focus(window, cx);
            }
        });
        if let Some(description) = &description {
            date_input = date_input.aria_description(description.clone());
        }
        date_box = date_box.child(toggle).child(date_input);
        if self.clear && self.value.is_some() && !locked {
            let label = i18n.format(
                "@astryx.dateInput.clear",
                &[("label", self.label.as_ref().into())],
            );
            let (ctx, focus) = (ctx.clone(), date_focus.clone());
            date_box = date_box.child(provide(
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
        if busy {
            date_box = date_box.child(
                Spinner::new(ElementId::Name(format!("{}-spinner", self.id).into()))
                    .size(SpinnerSize::Sm)
                    .shade(SpinnerShade::Inherit)
                    .decorative(),
            );
        }
        let calendar_el = {
            let (ctx, segment) = (ctx.clone(), segment.clone());
            let id = self.id.clone();
            let handle = calendar_handle.clone();
            let (min, max, months, week) = (
                min_date,
                max_date,
                self.number_of_months,
                self.week_starts_on,
            );
            let rules = self.date_constraints.clone();
            move |_: &mut Window, _: &mut App| {
                let mut el = Calendar::new(ElementId::Name(format!("{id}-calendar").into()))
                    .calendar(calendar)
                    .value(shown_date)
                    .handle(handle);
                if let Some(min) = min {
                    el = el.min(min);
                }
                if let Some(max) = max {
                    el = el.max(max);
                }
                el.date_constraints(rules)
                    .number_of_months(months)
                    .week_starts_on(week)
                    .on_change(move |date, window, cx| {
                        ctx.date_changed(date, true, window, cx);
                        segment.reset(window, cx);
                    })
                    .into_any_element()
            }
        };
        let date_segment =
            Popover::new(date_popover_id, i18n.t("@astryx.dateTimeInput.dialogLabel"))
                .controller(date_popover.clone())
                .placement(Placement::Below)
                .close_button(true)
                .close_button_label(i18n.t("@astryx.dateInput.closeCalendar"))
                .auto_focus(true)
                .light_dismiss(true)
                .return_focus(date_focus.clone())
                .padding(crate::layout::SpacingStep::S0)
                .content_fn(calendar_el)
                .child(date_box)
                .flex_1()
                .flex_basis(px(SEGMENT_BASIS))
                .min_w_0();

        // ---- The time segment -------------------------------------------
        let time_label = self.time_label.clone().unwrap_or_else(|| {
            i18n.format(
                "@astryx.dateTimeInput.timeSuffix",
                &[("label", self.label.as_ref().into())],
            )
        });
        let (time_pending, time_focused_state, highlighted) = {
            let s = time.read(cx);
            (s.typed.pending.clone(), s.focused, s.highlighted)
        };
        let time_display = time_pending.unwrap_or_else(|| {
            shown_value
                .map(|v| format_locale_time(v.time, &locale, hour_format, self.seconds))
                .unwrap_or_default()
        });
        let time_placeholder = if time_focused_state && time_display.is_empty() {
            match hour_format {
                HourFormat::H12 => i18n.t("@astryx.dateTimeInput.timeHint12h"),
                HourFormat::H24 => i18n.t("@astryx.dateTimeInput.timeHint24h"),
            }
        } else {
            self.time_placeholder
                .clone()
                .unwrap_or_else(|| i18n.t("@astryx.dateTimeInput.timePlaceholder"))
        };
        let time_open = time_popover.is_open(cx);
        let has_options = !options.is_empty() || self.time_option_interval.is_some();
        time_editor.update(cx, |s, cx| {
            s.set_text(&time_display, cx);
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
                    ctx.time_typed(text, window, cx)
                }) as astryx_primitives::editing::ChangeFn
            });
            s.on_enter = Some({
                let ctx = ctx.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    if !ctx.time_popover.is_open(cx) {
                        ctx.time_commit(Trigger::Enter, window, cx);
                    }
                }) as FocusFn
            });
            s.on_blur = Some({
                let ctx = ctx.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    ctx.time.update(cx, |s, _| s.focused = false);
                    ctx.time_commit(Trigger::Blur, window, cx);
                })
            });
            s.on_focus = Some({
                let ctx = ctx.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    if !ctx.locked {
                        ctx.time.update(cx, |s, _| s.focused = true);
                        window.refresh();
                    }
                })
            });
            s.on_key_down = Some({
                let (ctx, editor) = (ctx.clone(), cx.entity());
                Rc::new(
                    move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                        if editor.read(cx).buffer().marked().is_some() {
                            return;
                        }
                        if ctx.time_key(event, window, cx) {
                            cx.stop_propagation();
                        }
                    },
                ) as astryx_primitives::editing::KeyDownFn
            });
        });
        let time_status_type = time_status.as_ref().map(|s| s.status);
        let mut time_box = segment_box(
            SegmentChrome {
                id: ElementId::Name(format!("{}-time-segment", self.id).into()),
                target: "date-time-input-time-segment",
                size,
                status: time_status_type,
                locked,
                focused: time_focus.is_focused(window),
                focus_visible: is_focus_visible(&time_focus, window),
            },
            &theme,
            window,
            cx,
        );
        time_box = crate::input::field_menu::with_field_menu(
            time_box,
            &ElementId::Name(format!("{}-time", self.id).into()),
            &time_editor,
            &time_focus,
            self.disabled,
            window,
            cx,
        );
        if !locked {
            let (focus, ctx) = (time_focus.clone(), ctx.clone());
            time_box = time_box.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx);
                if !ctx.options.is_empty() {
                    ctx.show_options(window, cx);
                }
            });
        }
        let mut clock = div().flex().child(provide(
            IconDefaultSize(IconSize::Sm),
            Icon::new("clock").color(IconColor::Secondary),
        ));
        theme.refine(clock.style(), "date-time-input-clock-icon", &[], None);
        let mut time_input = text_editor(
            "time-editor",
            &time_editor,
            style(time_rejected, time_placeholder.clone()),
            cx,
        )
        .flex_1()
        .min_w_0()
        .font_stack(&t.font.family.body)
        .text_style(&t.text.body)
        .role(if has_options {
            Role::ComboBox
        } else {
            Role::TextInput
        })
        .aria_label(time_label.clone())
        .aria_value(SharedString::from(time_display.clone()))
        .aria_disabled(locked)
        .aria_busy(busy)
        .aria_required(required)
        .aria_invalid(
            matches!(
                &self.status,
                Some(InputStatus {
                    status: StatusType::Error,
                    ..
                })
            ) || time_rejected,
        )
        .aria_placeholder(time_placeholder);
        if has_options {
            time_input = time_input
                .aria_expanded(time_open)
                .aria_has_popup(gpui::HasPopup::Listbox);
        }
        if let Some(description) = &description {
            time_input = time_input.aria_description(description.clone());
        }
        time_box = time_box.child(clock).child(time_input);
        let time_segment: AnyElement = if has_options {
            let list = {
                let ctx = ctx.clone();
                let id = self.id.clone();
                let options = options.clone();
                let owner = time_focus.clone();
                let label = i18n.format(
                    "@astryx.dateTimeInput.timeOptionsLabel",
                    &[("label", time_label.as_ref().into())],
                );
                let selected = ctx.selected_option();
                let (pad_y, pad_x) = match size {
                    ElementSize::Sm => (t.spacing.s1, t.spacing.s2),
                    ElementSize::Md => (t.spacing.s1_5, t.spacing.s2),
                    ElementSize::Lg => (t.spacing.s2, t.spacing.s2),
                };
                let theme = theme.clone();
                move |_: &mut Window, _: &mut App| {
                    let t = theme.tokens();
                    let mut list = div()
                        .id(ElementId::Name(format!("{id}-time-listbox").into()))
                        .flex()
                        .flex_col()
                        .max_h(px(300.))
                        .overflow_y_scroll()
                        .p(t.spacing.s1)
                        .role(Role::ListBox)
                        .aria_label(label.clone());
                    theme.refine(list.style(), "date-time-input-time-listbox", &[], None);
                    let last = options.len().saturating_sub(1);
                    for (index, (time, text)) in options.iter().enumerate() {
                        let lit = highlighted.map(|h| h.min(last)) == Some(index);
                        let is_selected = Some(*time) == selected;
                        let selector = format!("{id}-time-option-{time}");
                        let pick = ctx.clone();
                        let hover = ctx.clone();
                        let time = *time;
                        let mut option = div()
                            .id(ElementId::Name(format!("{id}-time-option-{index}").into()))
                            .debug_selector(move || selector.clone())
                            .flex()
                            .items_center()
                            .w_full()
                            .py(pad_y)
                            .px(pad_x)
                            .rounded(t.radius.element)
                            .cursor_pointer()
                            .text_style(&t.text.body)
                            .text_color(t.color.text_primary)
                            .role(Role::ListBoxOption)
                            .aria_selected(is_selected)
                            .child(text.clone())
                            // The input keeps focus: a press commits on the
                            // pointer, before any blur.
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                cx.stop_propagation();
                                window.prevent_default();
                                pick.commit_option(time, window, cx);
                            })
                            .on_hover(move |over, window, cx| {
                                if *over {
                                    hover.time.update(cx, |s, _| s.highlighted = Some(index));
                                    window.refresh();
                                }
                            });
                        if is_selected {
                            option = option.font_weight(t.font.weight.medium);
                        }
                        if lit {
                            option = option
                                .bg(t.color.overlay_hover)
                                .aria_active_descendant_of(&owner);
                        }
                        theme.refine(option.style(), "date-time-input-time-option", &[], None);
                        list = list.child(option);
                    }
                    list.into_any_element()
                }
            };
            Popover::new(time_popover_id, time_label.clone())
                .controller(time_popover.clone())
                .role(PopoverRole::None)
                .placement(Placement::Below)
                .close_button(false)
                .auto_focus(false)
                .light_dismiss(true)
                .padding(crate::layout::SpacingStep::S0)
                .content_fn(list)
                .child(time_box)
                .flex_1()
                .flex_basis(px(SEGMENT_BASIS))
                .min_w_0()
                .into_any_element()
        } else {
            time_box.into_any_element()
        };

        // R16: the platform's pickers in place of the Astryx segments.
        let date_segment: AnyElement = if native_date {
            let on_outcome: crate::date_input::native::OutcomeFn = {
                let (ctx, rejection, typed) =
                    (ctx.clone(), native_rejection.clone(), typed.clone());
                let (value, draft_state) = (self.value, time_draft_state.clone());
                Rc::new(move |outcome, window, cx| {
                    if locked {
                        return;
                    }
                    match outcome {
                        gpui::NativePickerOutcome::Dismissed => {}
                        gpui::NativePickerOutcome::Cleared => {
                            rejection.update(cx, |r, _| r.clear());
                            if value.is_some() {
                                draft_state.update(cx, |d, _| d.0 = None);
                                ctx.fire(None, window, cx);
                            }
                        }
                        gpui::NativePickerOutcome::Picked(iso) => {
                            let Some(date) = PlainDate::from_iso(&iso) else {
                                return;
                            };
                            if typed.is_disabled(date) {
                                rejection.update(cx, |r, _| r.refuse(date));
                                return;
                            }
                            rejection.update(cx, |r, _| r.clear());
                            if value.map(|v| v.date) != Some(date) {
                                ctx.date_changed(date, false, window, cx);
                            }
                        }
                    }
                    window.refresh();
                })
            };
            let on_commit: astryx_primitives::editing::FocusFn = {
                let (rejection, optimistic, loading) =
                    (native_rejection.clone(), optimistic.clone(), self.loading);
                Rc::new(move |window, cx| {
                    let locked = locked || loading || optimistic.pending(cx);
                    rejection.update(cx, |r, _| r.commit(locked));
                    window.refresh();
                })
            };
            let mut trailing: Vec<AnyElement> = Vec::new();
            if self.clear && self.value.is_some() && !locked {
                let (ctx, draft_state) = (ctx.clone(), time_draft_state.clone());
                // A clear never takes focus back: focus would reopen a picker.
                trailing.push(
                    provide(
                        Disclosure::detached(),
                        IconButton::new(
                            ElementId::Name(format!("{}-clear", self.id).into()),
                            i18n.format(
                                "@astryx.dateInput.clear",
                                &[("label", self.label.as_ref().into())],
                            ),
                            Icon::new("close")
                                .size(IconSize::Sm)
                                .color(IconColor::Secondary),
                        )
                        .variant(ButtonVariant::Ghost)
                        .size(ElementSize::Sm)
                        .tooltip(i18n.t("@astryx.inputClearButton.tooltip"))
                        .h(px(20.))
                        .on_click(move |_, window, cx| {
                            draft_state.update(cx, |d, _| d.0 = None);
                            ctx.fire(None, window, cx);
                        }),
                    )
                    .into_any_element(),
                );
            }
            if busy {
                trailing.push(
                    Spinner::new(ElementId::Name(format!("{}-spinner", self.id).into()))
                        .size(SpinnerSize::Sm)
                        .shade(SpinnerShade::Inherit)
                        .decorative()
                        .into_any_element(),
                );
            }
            crate::date_input::native::native_segment(
                crate::date_input::native::NativeSegment {
                    id: ElementId::Name(format!("{}-native-date", self.id).into()),
                    target: "date-time-input-date-segment",
                    icon: "calendar",
                    icon_label: i18n.t("@astryx.dateInput.openCalendar"),
                    role: Role::DateInput,
                    name: self.label.clone(),
                    description: description.clone(),
                    display: shown_value
                        .map(|v| typed.format_committed(v.date))
                        .unwrap_or_default(),
                    placeholder: self
                        .placeholder
                        .clone()
                        .unwrap_or_else(|| i18n.t("@astryx.dateTimeInput.placeholder")),
                    size,
                    status: date_status_type,
                    disabled: self.disabled,
                    inert: locked,
                    busy,
                    required,
                    invalid: native_date_invalid,
                    request: gpui::NativePickerRequest {
                        kind: gpui::NativePickerKind::Date,
                        value: shown_value.map(|v| v.date.to_string()),
                        min: min_date.map(|d| d.to_string()),
                        max: max_date.map(|d| d.to_string()),
                        seconds: false,
                        anchor: gpui::Bounds::default(),
                    },
                    on_outcome,
                    on_commit: Some(on_commit),
                    trailing,
                    control_style: StyleRefinement::default(),
                },
                &theme,
                window,
                cx,
            )
            .flex_1()
            .flex_basis(px(SEGMENT_BASIS))
            .min_w_0()
            .into_any_element()
        } else {
            date_segment.into_any_element()
        };
        let time_segment: AnyElement = if native_time {
            // The time the picker shows: a held draft, else the value's.
            let native_time_value = time_draft.or(shown_value.map(|v| v.time));
            let on_outcome: crate::date_input::native::OutcomeFn = {
                let (ctx, time_state, draft_state) =
                    (ctx.clone(), time.clone(), time_draft_state.clone());
                let value = shown_value;
                Rc::new(move |outcome, window, cx| {
                    if locked {
                        return;
                    }
                    let set_rejected = |rejected: bool, cx: &mut App| {
                        time_state
                            .update(cx, |s, _| s.typed.set_surface_rejected(rejected, behavior))
                    };
                    match outcome {
                        gpui::NativePickerOutcome::Dismissed => {}
                        gpui::NativePickerOutcome::Cleared => {
                            set_rejected(false, cx);
                            if native_time_value.is_some() {
                                draft_state.update(cx, |d, _| d.0 = None);
                                if value.is_some() {
                                    ctx.fire(None, window, cx);
                                }
                            }
                        }
                        gpui::NativePickerOutcome::Picked(iso) => {
                            let Some(time) = PlainTime::from_iso(&iso) else {
                                return;
                            };
                            if !is_time_in_range(time, time_min, time_max) {
                                set_rejected(true, cx);
                                window.refresh();
                                return;
                            }
                            set_rejected(false, cx);
                            if Some(time) != native_time_value {
                                match value {
                                    Some(value) => ctx.fire(
                                        Some(PlainDateTime::new(value.date, time)),
                                        window,
                                        cx,
                                    ),
                                    // A time before any date waits for the date (FR2).
                                    None => draft_state.update(cx, |d, _| d.0 = Some(time)),
                                }
                            }
                        }
                    }
                    window.refresh();
                })
            };
            let on_commit: astryx_primitives::editing::FocusFn = {
                let time_state = time.clone();
                Rc::new(move |window, cx| {
                    time_state.update(cx, |s, _| s.typed.commit_surface());
                    window.refresh();
                })
            };
            crate::date_input::native::native_segment(
                crate::date_input::native::NativeSegment {
                    id: ElementId::Name(format!("{}-native-time", self.id).into()),
                    target: "date-time-input-time-segment",
                    icon: "clock",
                    icon_label: i18n.format(
                        "@astryx.dateTimeInput.openTimePicker",
                        &[("label", time_label.as_ref().into())],
                    ),
                    role: Role::TimeInput,
                    name: time_label.clone(),
                    description: description.clone(),
                    display: native_time_value
                        .map(|t| format_locale_time(t, &locale, hour_format, false))
                        .unwrap_or_default(),
                    placeholder: self
                        .time_placeholder
                        .clone()
                        .unwrap_or_else(|| i18n.t("@astryx.dateTimeInput.timePlaceholder")),
                    size,
                    status: time_status_type,
                    disabled: self.disabled,
                    inert: locked,
                    busy,
                    required,
                    invalid: time_rejected,
                    // The picker steps whole minutes, as upstream's `step=60`.
                    request: gpui::NativePickerRequest {
                        kind: gpui::NativePickerKind::Time,
                        value: native_time_value.map(|t| t.to_iso(false)),
                        min: time_min.map(|t| t.to_iso(false)),
                        max: time_max.map(|t| t.to_iso(false)),
                        seconds: false,
                        anchor: gpui::Bounds::default(),
                    },
                    on_outcome,
                    on_commit: Some(on_commit),
                    trailing: Vec::new(),
                    control_style: StyleRefinement::default(),
                },
                &theme,
                window,
                cx,
            )
            .flex_1()
            .flex_basis(px(SEGMENT_BASIS))
            .min_w_0()
            .into_any_element()
        } else {
            time_segment
        };

        let mut row = div().flex().flex_row().flex_wrap().gap(t.spacing.s2);
        theme.refine(
            row.style(),
            "date-time-input",
            &[
                ("size", size.as_str()),
                (
                    "status",
                    shown_status.as_ref().map_or("", |s| s.status.as_str()),
                ),
                ("disabled", if self.disabled { "disabled" } else { "" }),
            ],
            None,
        );
        row.style().refine(&self.control_style);
        let row = row.child(date_segment).child(time_segment);
        let row = crate::overlay::disabled_reason(
            &self.id,
            self.disabled,
            self.disabled_message.as_ref(),
            true,
            row,
        );

        let mut field = Field::new(self.id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(self.disabled)
            .optional(self.optional)
            .required(self.required)
            .status_placement(StatusPlacement::Detached);
        if let Some(status) = shown_status {
            field = field.status(status);
        }
        if let Some(tooltip) = self.label_tooltip {
            field = field.label_tooltip(tooltip);
        }
        if let Some(width) = self.width {
            field = field.width(width);
        }
        field.style().refine(&self.style);
        field.child(row).into_any_element()
    }
}
