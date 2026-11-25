//! DateTimeInput's bottom-sheet surface, upstream
//! `DateTimeInput/TouchDateTimeField.tsx`: two read-only segments that open
//! one BottomSheet with a Date and a Time panel. The Date panel is
//! DateInput's touch calendar (the month pager, the month and year wheels);
//! the Time panel is TimeInput's wheels. A day keeps the time, clamped into
//! a bound on that day; a time before any date waits as a draft (FR2). Only
//! the disabled and loading states lock it: a pending Action keeps the
//! sheet editable (FR9). Under a finger the month wheel moves a chosen date,
//! keeping its day and its time (FR14).

use std::rc::Rc;

use astryx_i18n::date::{
    HourFormat, PlainDate, PlainDateTime, PlainTime, SharedDateFormat, calendar_day_of_month,
    clamp_time, format_calendar_date_text, format_calendar_month_label, format_locale_time,
    resolve_hour_format,
};
use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{ElementSize, control_focus, is_focus_visible, provide, resolve_size};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, Entity, FocusHandle, IntoElement, KeyDownEvent, Role, SharedString,
    Styled, Window, div, prelude::*, px,
};

use super::date_time_input::{DateTimeBound, DateTimeInput};
use crate::button::{Button, ButtonVariant, IconButton};
use crate::calendar::weekday_headers;
use crate::choice::action::{ActionFn, Optimistic};
use crate::date_input::month_model::{DEFAULT_MONTH_REACH, MonthModel, clamp_index, date_in_month};
use crate::date_input::month_scroller::{MonthScroller, MonthScrollerHandle};
use crate::date_input::month_year_wheels::MonthYearWheels;
use crate::date_input::sheet::header_button;
use crate::field::{Field, StatusPlacement, StatusType};
use crate::icon::{Icon, IconColor, IconSize};
use crate::input::control::{Chrome, control_box, described_by};
use crate::layout::SpacingStep;
use crate::overlay::{BottomSheet, Disclosure, SheetHeight};
use crate::segmented::{SegmentLayout, SegmentedControl, SegmentedControlItem};
use crate::status::{Spinner, SpinnerShade, SpinnerSize};
use crate::time_input::sheet::{MouseWheels, mouse_time_wheels};
use crate::time_input::wheels::{TimeCommit, TimeWheels};
use crate::wheel::options::{DAY_SIZE, PANE_HEIGHT};

type ValueFn = Rc<dyn Fn(Option<PlainDateTime>, &mut Window, &mut App)>;
type IsDisabled = Rc<dyn Fn(PlainDate) -> bool>;
/// The segments' basis, as the pointer field's.
const SEGMENT_BASIS: f32 = 196.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Panel {
    #[default]
    Date,
    Time,
}

#[derive(Default)]
pub(super) struct SheetState {
    open: bool,
    panel: Panel,
    wheels: bool,
    month: Option<i64>,
    anchor: Option<i64>,
    /// A time chosen before any date (upstream `draftTime`).
    draft: Option<PlainTime>,
    /// The value the draft last followed.
    source: Option<(Option<PlainTime>, Option<PlainDateTime>)>,
}

#[derive(Clone)]
struct Ctx {
    state: Entity<SheetState>,
    /// Disabled or loading (a pending Action still edits).
    inert: bool,
    value: Option<PlainDateTime>,
    seconds: bool,
    min: Option<DateTimeBound>,
    max: Option<DateTimeBound>,
    wheel_time: PlainTime,
    on_change: Option<ValueFn>,
    action: Option<ActionFn<Option<PlainDateTime>>>,
    optimistic: Rc<Optimistic<Option<PlainDateTime>>>,
    scroller: MonthScrollerHandle,
    model: MonthModel,
    min_index: i64,
    max_index: i64,
    month: i64,
    is_disabled: IsDisabled,
    prev_focus: FocusHandle,
    next_focus: FocusHandle,
    title_focus: FocusHandle,
    time_segment: ElementId,
}

impl Ctx {
    fn fire(&self, value: Option<PlainDateTime>, window: &mut Window, cx: &mut App) {
        if self.inert {
            return;
        }
        let change = self
            .optimistic
            .change(self.on_change.clone(), self.action.clone());
        change(value, window, cx);
        window.refresh();
    }

    /// A bound's time applies only on its own date.
    fn clamp_for(&self, date: Option<PlainDate>, time: PlainTime) -> PlainTime {
        let bound = |b: Option<DateTimeBound>| match (b, date) {
            (
                Some(DateTimeBound {
                    date: d,
                    time: Some(t),
                }),
                Some(date),
            ) if d == date => Some(t),
            _ => None,
        };
        clamp_time(time, bound(self.min), bound(self.max), self.seconds)
    }

    fn select_date(&self, date: PlainDate, window: &mut Window, cx: &mut App) {
        let time = self.clamp_for(Some(date), self.wheel_time);
        self.state.update(cx, |s, _| s.draft = Some(time));
        let combined = PlainDateTime::new(date, time);
        if Some(combined) != self.value {
            self.fire(Some(combined), window, cx);
        }
    }

    fn commit_time(&self, raw: PlainTime, window: &mut Window, cx: &mut App) {
        let date = self.value.map(|v| v.date);
        let time = self.clamp_for(date, raw);
        self.state.update(cx, |s, _| s.draft = Some(time));
        if let Some(date) = date {
            let combined = PlainDateTime::new(date, time);
            if Some(combined) != self.value {
                self.fire(Some(combined), window, cx);
            }
        }
        window.refresh();
    }

    fn set_month(&self, month: i64, cx: &mut App) {
        self.state.update(cx, |s, _| s.month = Some(month));
    }

    fn step(&self, delta: i64, window: &mut Window, cx: &mut App) {
        let target = clamp_index(self.month + delta, self.min_index, self.max_index);
        if target == self.month {
            return;
        }
        let prev_hides = target <= self.min_index;
        let next_hides = target >= self.max_index;
        let destination = if prev_hides && self.prev_focus.is_focused(window) {
            Some(if next_hides {
                &self.title_focus
            } else {
                &self.next_focus
            })
        } else if next_hides && self.next_focus.is_focused(window) {
            Some(if prev_hides {
                &self.title_focus
            } else {
                &self.prev_focus
            })
        } else {
            None
        };
        if let Some(destination) = destination.cloned() {
            window.defer(cx, move |window, cx| window.focus(&destination, cx));
        }
        self.set_month(target, cx);
        self.scroller.scroll_to_month(target, true, window, cx);
        window.refresh();
    }

    fn visible_month(&self, month: i64, window: &mut Window, cx: &mut App) {
        let s = self.state.read(cx);
        if s.panel == Panel::Date && !s.wheels {
            self.set_month(month, cx);
            window.refresh();
        }
    }

    fn wheel_month(&self, month: i64, window: &mut Window, cx: &mut App) {
        self.set_month(month, cx);
        self.scroller.scroll_to_month(month, false, window, cx);
        if astryx_primitives::coarse_primary_pointer(window, cx)
            && let Some(selected) = self.value.map(|v| v.date)
        {
            let moved = date_in_month(
                &self.model,
                selected,
                month,
                self.min.map(|b| b.date),
                self.max.map(|b| b.date),
            );
            if moved != selected && !(self.is_disabled)(moved) {
                self.select_date(moved, window, cx);
            }
        }
        window.refresh();
    }

    fn reset(&self, window: &mut Window, cx: &mut App) {
        self.fire(None, window, cx);
        self.state.update(cx, |s, _| s.draft = None);
        let current = self.model.index_of(PlainDate::today());
        if current < self.min_index || current > self.max_index {
            return;
        }
        self.set_month(current, cx);
        self.scroller.scroll_to_month(current, true, window, cx);
    }

    fn show_panel(&self, panel: Panel, window: &mut Window, cx: &mut App) {
        let month = self.state.update(cx, |s, _| {
            s.panel = panel;
            s.wheels = false;
            s.month
        });
        if panel == Panel::Date
            && let Some(month) = month
        {
            self.scroller.scroll_to_month(month, false, window, cx);
        }
        window.refresh();
    }

    fn toggle_wheels(&self, window: &mut Window, cx: &mut App) {
        let (closing, month) = self.state.update(cx, |s, _| {
            s.wheels = !s.wheels;
            (!s.wheels, s.month)
        });
        if closing && let Some(month) = month {
            self.scroller.scroll_to_month(month, false, window, cx);
        }
        window.refresh();
    }

    fn close(&self, window: &mut Window, cx: &mut App) {
        let no_date = self.value.is_none();
        self.state.update(cx, |s, _| {
            if no_date {
                s.draft = None;
            }
            s.open = false;
        });
        window.refresh();
    }
}

/// One closed segment: a read-only combobox in its own control box.
struct Segment {
    id: ElementId,
    target: &'static str,
    icon: &'static str,
    icon_label: SharedString,
    name: SharedString,
    display: String,
    placeholder: SharedString,
    expanded: bool,
    panel: Panel,
}

pub(super) fn render_sheet(input: DateTimeInput, window: &mut Window, cx: &mut App) -> AnyElement {
    let dir = use_direction(cx);
    let rtl = dir.is_rtl();
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let size = resolve_size(input.size, ElementSize::Md, cx);
    let i18n = i18n(cx);
    let locale = i18n.locale().clone();
    let calendar = input.calendar.unwrap_or_else(|| i18n.calendar_system());
    let hour_format = resolve_hour_format(&locale, input.hour_format);
    let coarse = astryx_primitives::coarse_primary_pointer(window, cx);

    let optimistic = Rc::new(Optimistic::<Option<PlainDateTime>>::new(
        &input.id, window, cx,
    ));
    let shown = optimistic.shown(input.value, cx);
    let busy = input.loading || optimistic.pending(cx);
    let inert = input.disabled || input.loading;
    optimistic.announce_busy(busy, i18n.t("@astryx.spinner.loading"), window, cx);

    let model = MonthModel::new(calendar);
    let today = PlainDate::today();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{:?}-date-time-sheet", input.id).into()),
        cx,
        |_, _| SheetState::default(),
    );
    let value_time = shown.map(|v| v.time.truncated(input.seconds));
    // The draft follows the value's time, and an outside clear drops it.
    let anchor = state.update(cx, |s, _| {
        if s.source != Some((value_time, shown)) {
            s.source = Some((value_time, shown));
            if value_time.is_some() {
                s.draft = value_time;
            } else if shown.is_none() {
                s.draft = None;
            }
        }
        // The open sheet closes when the field locks.
        if inert {
            s.open = false;
        }
        *s.anchor
            .get_or_insert_with(|| model.index_of(input.value.map_or(today, |v| v.date)))
    });
    let min_index = input
        .min
        .map_or(anchor - DEFAULT_MONTH_REACH, |b| model.index_of(b.date));
    let max_index = input
        .max
        .map_or(anchor + DEFAULT_MONTH_REACH, |b| model.index_of(b.date));
    let (open, panel, wheels, chosen, draft) = {
        let s = state.read(cx);
        (s.open, s.panel, s.wheels, s.month, s.draft)
    };
    let month = clamp_index(chosen.unwrap_or(anchor), min_index, max_index);
    let wheel_time = value_time
        .or(draft)
        .unwrap_or_else(|| PlainTime::now().truncated(input.seconds));

    let (min_date, max_date) = (input.min.map(|b| b.date), input.max.map(|b| b.date));
    let rules = input.date_constraints.clone();
    let is_disabled: IsDisabled = Rc::new(move |date: PlainDate| {
        min_date.is_some_and(|m| date < m)
            || max_date.is_some_and(|m| date > m)
            || rules.iter().any(|rule| !rule(date))
    });
    let id = input.id.clone();
    let scroller = window
        .use_keyed_state(
            ElementId::Name(format!("{:?}-scroller-handle", id).into()),
            cx,
            |_, _| MonthScrollerHandle::default(),
        )
        .read(cx)
        .clone();
    let mode_id = ElementId::Name(format!("{id}-mode").into());
    let ctx = Ctx {
        state: state.clone(),
        inert,
        value: shown,
        seconds: input.seconds,
        min: input.min,
        max: input.max,
        wheel_time,
        on_change: input.on_change.clone(),
        action: input.change_action.clone(),
        optimistic: optimistic.clone(),
        scroller,
        model: model.clone(),
        min_index,
        max_index,
        month,
        is_disabled: is_disabled.clone(),
        prev_focus: control_focus(
            &ElementId::Name(format!("{id}-prev-month").into()),
            window,
            cx,
        ),
        next_focus: control_focus(
            &ElementId::Name(format!("{id}-next-month").into()),
            window,
            cx,
        ),
        title_focus: control_focus(
            &ElementId::Name(format!("{id}-month-year").into()),
            window,
            cx,
        ),
        time_segment: ElementId::Name(format!("{mode_id:?}-time").into()),
    };

    // ---- The closed segments ----------------------------------------------
    let time_label = input.time_label.clone().unwrap_or_else(|| {
        i18n.format(
            "@astryx.dateTimeInput.timeSuffix",
            &[("label", input.label.as_ref().into())],
        )
    });
    let date_display = shown
        .map(|v| format_calendar_date_text(v.date, SharedDateFormat::DateLong, &locale, calendar))
        .unwrap_or_default();
    let time_display = value_time
        .or(draft)
        .map(|time| format_locale_time(time, &locale, hour_format, input.seconds))
        .unwrap_or_default();
    let described = described_by(
        input.description.as_ref(),
        input.status.as_ref(),
        input
            .disabled
            .then_some(input.disabled_message.as_ref())
            .flatten(),
        &i18n,
    );
    let description = (!described.is_empty()).then(|| SharedString::from(described.join(" ")));
    let required = crate::field::resolve(input.required, input.optional, cx).required;
    let status = input.status.as_ref().map(|s| s.status);
    let segments = [
        Segment {
            id: ElementId::Name(format!("{id}-date").into()),
            target: "date-time-input-date-segment",
            icon: "calendar",
            icon_label: i18n.t("@astryx.dateInput.openCalendar"),
            name: input.label.clone(),
            display: date_display,
            placeholder: input
                .placeholder
                .clone()
                .unwrap_or_else(|| i18n.t("@astryx.dateTimeInput.placeholder")),
            expanded: open && panel == Panel::Date,
            panel: Panel::Date,
        },
        Segment {
            id: ElementId::Name(format!("{id}-time").into()),
            target: "date-time-input-time-segment",
            icon: "clock",
            icon_label: i18n.format(
                "@astryx.dateTimeInput.openTimePicker",
                &[("label", time_label.as_ref().into())],
            ),
            name: time_label.clone(),
            display: time_display,
            placeholder: input
                .time_placeholder
                .clone()
                .unwrap_or_else(|| i18n.t("@astryx.dateTimeInput.timePlaceholder")),
            expanded: open && panel == Panel::Time,
            panel: Panel::Time,
        },
    ];
    let mut row = div()
        .flex()
        .flex_wrap()
        .w_full()
        .min_w_0()
        .gap(t.spacing.s2);
    theme.refine(
        row.style(),
        "date-time-input",
        &[
            ("size", size.as_str()),
            ("status", status.map_or("", |s| s.as_str())),
            ("disabled", if input.disabled { "disabled" } else { "" }),
        ],
        None,
    );
    row.style().refine(&input.control_style);
    for segment in segments {
        let focus = control_focus(&segment.id, window, cx);
        let chrome = Chrome {
            id: ElementId::Name(format!("{}-box", segment.id).into()),
            target: segment.target,
            variant: None,
            size,
            status,
            disabled: inert,
            read_only: false,
            focused: focus.is_focused(window),
            focus_visible: is_focus_visible(&focus, window),
            row: true,
            place: None,
            dir,
        };
        let open_panel = {
            let (state, panel) = (state.clone(), segment.panel);
            move |window: &mut Window, cx: &mut App| {
                if !inert {
                    state.update(cx, |s, _| {
                        s.open = true;
                        s.wheels = false;
                        s.panel = panel;
                    });
                    window.refresh();
                }
            }
        };
        let mut boxed = control_box(&chrome, &theme, window, cx)
            .flex_1()
            .flex_basis(px(SEGMENT_BASIS))
            .min_w_0();
        let icon_focus = control_focus(
            &ElementId::Name(format!("{}-icon", segment.id).into()),
            window,
            cx,
        )
        .tab_stop(false);
        let (open_icon, open_click, open_key) =
            (open_panel.clone(), open_panel.clone(), open_panel.clone());
        let mut glyph = div().flex().child(
            Icon::new(segment.icon)
                .size(IconSize::Sm)
                .color(IconColor::Secondary),
        );
        if segment.panel == Panel::Date {
            theme.refine(
                glyph.style(),
                "date-time-input-toggle-icon",
                &[("state", if open { "expanded" } else { "collapsed" })],
                None,
            );
        } else {
            theme.refine(glyph.style(), "date-time-input-clock-icon", &[], None);
        }
        boxed = boxed.child(
            div()
                .id(ElementId::Name(format!("{}-icon", segment.id).into()))
                .track_focus(&icon_focus)
                .flex()
                .flex_none()
                .items_center()
                .when(!inert, |el| el.cursor_pointer())
                .role(Role::Button)
                .aria_label(segment.icon_label.clone())
                .aria_disabled(inert)
                .child(glyph)
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    open_icon(window, cx);
                }),
        );
        let mut trigger = div()
            .id(ElementId::Name(format!("{}-trigger", segment.id).into()))
            .debug_selector({
                let selector = format!("{}-trigger", segment.id);
                move || selector.clone()
            })
            .track_focus(&focus)
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .text_color(if segment.display.is_empty() {
                t.color.text_secondary
            } else {
                t.color.text_primary
            })
            .when(!inert, |el| el.cursor_pointer())
            .role(Role::ComboBox)
            .aria_label(segment.name.clone())
            .aria_value(SharedString::from(segment.display.clone()))
            .aria_disabled(inert)
            .aria_busy(busy)
            .aria_expanded(segment.expanded)
            .aria_has_popup(gpui::HasPopup::Dialog)
            .aria_required(required)
            .aria_invalid(status == Some(StatusType::Error))
            .aria_placeholder(segment.placeholder.clone())
            .child(if segment.display.is_empty() {
                segment.placeholder.clone()
            } else {
                SharedString::from(segment.display.clone())
            })
            .on_click(move |_, window, cx| open_click(window, cx))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let keys = &event.keystroke;
                if matches!(keys.key.as_str(), "enter" | "space" | "down")
                    && !keys.modifiers.control
                    && !keys.modifiers.platform
                {
                    cx.stop_propagation();
                    open_key(window, cx);
                }
            });
        if let Some(description) = &description {
            trigger = trigger.aria_description(description.clone());
        }
        boxed = boxed.child(trigger);
        if segment.panel == Panel::Date {
            if input.clear && input.value.is_some() && !inert {
                let (clear, focus) = (ctx.clone(), focus.clone());
                boxed = boxed.child(provide(
                    Disclosure::detached(),
                    IconButton::new(
                        ElementId::Name(format!("{id}-clear").into()),
                        i18n.format(
                            "@astryx.dateInput.clear",
                            &[("label", input.label.as_ref().into())],
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
                        clear.fire(None, window, cx);
                        clear.state.update(cx, |s, _| {
                            s.draft = None;
                            s.open = false;
                        });
                        let focus = focus.clone();
                        window.defer(cx, move |window, cx| window.focus(&focus, cx));
                    }),
                ));
            }
            if busy {
                boxed = boxed.child(
                    Spinner::new(ElementId::Name(format!("{id}-spinner").into()))
                        .size(SpinnerSize::Sm)
                        .shade(SpinnerShade::Inherit)
                        .decorative(),
                );
            }
        }
        row = row.child(boxed);
    }

    // ---- The sheet --------------------------------------------------------
    let body = open.then(|| {
        sheet_body(
            &input,
            &ctx,
            &t,
            &theme,
            rtl,
            coarse,
            panel,
            wheels,
            today,
            hour_format,
            &time_label,
            &locale,
            &i18n,
            window,
            cx,
        )
    });
    let close = ctx.clone();
    let mut sheet = BottomSheet::new(
        ElementId::Name(format!("{id}-sheet").into()),
        open,
        i18n.t("@astryx.dateTimeInput.dialogLabel"),
    )
    .height(SheetHeight::Hug)
    .padding(SpacingStep::S0)
    .on_open_change(move |open, window, cx| {
        if open {
            close.state.update(cx, |s, _| s.open = true);
            window.refresh();
        } else {
            close.close(window, cx);
        }
    });
    if let Some(body) = body {
        sheet = sheet.child(body);
    }
    let row = row.child(sheet);
    let row = crate::overlay::disabled_reason(
        &input.id,
        input.disabled,
        input.disabled_message.as_ref(),
        true,
        row,
    );
    let mut field = Field::new(input.id, input.label)
        .description_opt(input.description)
        .label_hidden(input.label_hidden)
        .disabled(input.disabled)
        .optional(input.optional)
        .required(input.required)
        .status_placement(StatusPlacement::Detached);
    if let Some(status) = input.status {
        field = field.status(status);
    }
    if let Some(tooltip) = input.label_tooltip {
        field = field.label_tooltip(tooltip);
    }
    if let Some(width) = input.width {
        field = field.width(width);
    }
    field.style().refine(&input.style);
    field.child(row).into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn sheet_body(
    input: &DateTimeInput,
    ctx: &Ctx,
    t: &astryx_theme::Tokens,
    theme: &astryx_theme::ResolvedTheme,
    rtl: bool,
    coarse: bool,
    panel: Panel,
    wheels: bool,
    today: PlainDate,
    hour_format: HourFormat,
    time_label: &SharedString,
    locale: &SharedString,
    i18n: &astryx_i18n::I18n,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = &input.id;
    let calendar = ctx.model.calendar;
    let selected = ctx.value.map(|v| v.date);
    let kept_day = selected.map_or(1, |d| calendar_day_of_month(calendar, d));
    let month_label = format_calendar_month_label(
        calendar,
        &ctx.model.month_at(ctx.month),
        locale,
        Some(kept_day),
    );
    let on_date = panel == Panel::Date;

    // The Date and Time switch.
    let switch_ctx = ctx.clone();
    let mode = SegmentedControl::new(
        ElementId::Name(format!("{id}-mode").into()),
        i18n.t("@astryx.dateTimeInput.pickerMode"),
        if on_date { "date" } else { "time" },
    )
    .layout(SegmentLayout::Fill)
    .item(SegmentedControlItem::new(
        "date",
        i18n.t("@astryx.dateTimeInput.dateTab"),
    ))
    .item(SegmentedControlItem::new(
        "time",
        i18n.t("@astryx.dateTimeInput.timeTab"),
    ))
    .on_change(move |value, window, cx| {
        let panel = if value.as_ref() == "time" {
            Panel::Time
        } else {
            Panel::Date
        };
        switch_ctx.show_panel(panel, window, cx);
    });

    // The Date panel: the header, then the calendar or the wheels.
    let title_ctx = ctx.clone();
    let title = header_button(
        ElementId::Name(format!("{id}-month-year").into()),
        &ctx.title_focus,
        i18n.format(
            "@astryx.dateInput.chooseMonthYear",
            &[("monthYear", month_label.as_str().into())],
        ),
        theme,
        window,
        cx,
        move |window, cx| title_ctx.toggle_wheels(window, cx),
    )
    .debug_selector({
        let selector = format!("{id}-month-year");
        move || selector.clone()
    })
    .aria_expanded(wheels)
    .gap(t.spacing.s1)
    .h_full()
    .min_w_0()
    .overflow_hidden()
    .whitespace_nowrap()
    .text_style(&t.text.large)
    .font_weight(t.font.weight.semibold)
    .text_color(t.color.text_primary)
    .child(
        div()
            .min_w_0()
            .overflow_hidden()
            .text_ellipsis()
            .child(month_label.clone()),
    )
    .child(
        div()
            .flex()
            .flex_none()
            .when(wheels, |el| el.scale_xy(1., -1.))
            .child(
                Icon::new("chevronDown")
                    .size(IconSize::Sm)
                    .color(IconColor::Secondary),
            ),
    );
    let can_back = ctx.month > ctx.min_index;
    let can_forward = ctx.month < ctx.max_index;
    let arrow = |key: &str,
                 label: &str,
                 icon: &'static str,
                 focus: &FocusHandle,
                 delta: i64,
                 shown: bool,
                 window: &mut Window,
                 cx: &mut App| {
        let ctx = ctx.clone();
        let el = header_button(
            ElementId::Name(format!("{id}-{key}").into()),
            &focus.clone().tab_stop(shown && !wheels && on_date),
            i18n.t(label),
            theme,
            window,
            cx,
            move |window, cx| ctx.step(delta, window, cx),
        )
        .debug_selector({
            let selector = format!("{id}-{key}");
            move || selector.clone()
        })
        .size(px(32.))
        .when(coarse, |el| {
            el.min_w(px(DAY_SIZE as f32)).min_h(px(DAY_SIZE as f32))
        })
        .child(
            div()
                .flex()
                .when(rtl, |el| el.scale_xy(-1., 1.))
                .child(Icon::new(icon).size(IconSize::Sm).color(IconColor::Primary)),
        );
        if shown && !wheels { el } else { el.invisible() }
    };
    let prev = arrow(
        "prev-month",
        "@astryx.calendar.previousMonth",
        "chevronLeft",
        &ctx.prev_focus,
        -1,
        can_back,
        window,
        cx,
    );
    let next = arrow(
        "next-month",
        "@astryx.calendar.nextMonth",
        "chevronRight",
        &ctx.next_focus,
        1,
        can_forward,
        window,
        cx,
    );
    let reset_ctx = ctx.clone();
    let actions = div()
        .flex()
        .items_center()
        .gap(t.spacing.s0_5)
        .ml_auto()
        .when(wheels, |el| el.invisible())
        .child(prev)
        .child(next)
        .child(
            Button::new(
                ElementId::Name(format!("{id}-reset").into()),
                i18n.t("@astryx.dateInput.resetPicking"),
            )
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .on_click(move |_, window, cx| reset_ctx.reset(window, cx)),
        );
    let header = div()
        .flex()
        .items_center()
        .gap(t.spacing.s1)
        .h(t.size.element_lg)
        .child(title)
        .child(actions);
    let mut weekdays = div().flex().h(t.size.element_sm).items_center();
    if rtl {
        weekdays = weekdays.flex_row_reverse();
    }
    for (_, name) in weekday_headers(locale, input.week_starts_on) {
        weekdays = weekdays.child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_style(&t.text.supporting)
                .font_weight(t.font.weight.normal)
                .text_color(t.color.text_secondary)
                .child(name),
        );
    }
    let footer = |el: AnyElement| div().flex().w_full().mt_auto().pt(t.spacing.s1).child(el);
    let (visible, select, save_date) = (ctx.clone(), ctx.clone(), ctx.clone());
    let calendar_surface = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(t.spacing.s2)
        .col_start(1)
        .col_end(2)
        .row_start(1)
        .row_end(2)
        .when(wheels, |el| el.invisible())
        .child(weekdays)
        .child(MonthScroller {
            id: ElementId::Name(format!("{id}-months").into()),
            handle: ctx.scroller.clone(),
            model: ctx.model.clone(),
            min_index: ctx.min_index,
            max_index: ctx.max_index,
            initial_index: ctx.month,
            on_visible_month_change: Rc::new(move |month, window, cx| {
                visible.visible_month(month, window, cx)
            }),
            selected,
            today,
            is_disabled: ctx.is_disabled.clone(),
            week_starts_on: input.week_starts_on,
            on_select: Rc::new(move |date, window, cx| select.select_date(date, window, cx)),
            active: on_date && !wheels,
        })
        .child(footer(
            div()
                .flex()
                .w_full()
                .debug_selector({
                    let selector = format!("{id}-save-date");
                    move || selector.clone()
                })
                .child(
                    Button::new(
                        ElementId::Name(format!("{id}-save-date").into()),
                        i18n.t("@astryx.dateTimeInput.saveDatePicking"),
                    )
                    .variant(ButtonVariant::Primary)
                    .size(ElementSize::Md)
                    .width(gpui::relative(1.))
                    .disabled(selected.is_none())
                    .on_click(move |_, window, cx| {
                        // On to the time, with focus on its segment.
                        save_date.show_panel(Panel::Time, window, cx);
                        astryx_primitives::request_focus(&save_date.time_segment, cx);
                    }),
                )
                .into_any_element(),
        ));
    let (wheel, done) = (ctx.clone(), ctx.clone());
    let wheels_surface = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(t.spacing.s2)
        .col_start(1)
        .col_end(2)
        .row_start(1)
        .row_end(2)
        .when(!wheels, |el| el.invisible())
        .child(div().h(t.size.element_sm))
        .child(MonthYearWheels {
            id: ElementId::Name(format!("{id}-wheels").into()),
            model: ctx.model.clone(),
            month_index: ctx.month,
            min_index: ctx.min_index,
            max_index: ctx.max_index,
            on_change: Rc::new(move |month, window, cx| wheel.wheel_month(month, window, cx)),
            month_label: i18n.t("@astryx.dateInput.monthWheel"),
            year_label: i18n.t("@astryx.dateInput.yearWheel"),
            active: on_date && wheels,
            kept_day,
        })
        .child(footer(
            Button::new(
                ElementId::Name(format!("{id}-done").into()),
                i18n.t("@astryx.dateInput.doneChoosingMonth"),
            )
            .variant(ButtonVariant::Secondary)
            .size(ElementSize::Md)
            .width(gpui::relative(1.))
            .on_click(move |_, window, cx| done.toggle_wheels(window, cx))
            .into_any_element(),
        ));
    let date_panel = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .gap(t.spacing.s2)
        .col_start(1)
        .col_end(2)
        .row_start(1)
        .row_end(2)
        .when(!on_date, |el| el.invisible())
        .child(header)
        .child(
            div()
                .grid()
                .grid_cols(1)
                .w_full()
                .child(calendar_surface)
                .child(wheels_surface),
        );

    // The Time panel: the wheels, then Save.
    let mut time_wheels = div()
        .flex()
        .w_full()
        .min_w_0()
        .h(px(PANE_HEIGHT as f32))
        .gap(t.spacing.s2);
    let date = selected;
    let bound = |b: Option<DateTimeBound>| match (b, date) {
        (
            Some(DateTimeBound {
                date: d,
                time: Some(t),
            }),
            Some(date),
        ) if d == date => Some(t),
        _ => None,
    };
    let (tmin, tmax) = (bound(ctx.min), bound(ctx.max));
    let commit = ctx.clone();
    let commit: TimeCommit = Rc::new(move |time, window, cx| commit.commit_time(time, window, cx));
    if coarse {
        time_wheels = time_wheels.child(TimeWheels {
            id: ElementId::Name(format!("{id}-time-wheels").into()),
            time: ctx.wheel_time,
            hour_format,
            seconds: input.seconds,
            min: tmin,
            max: tmax,
            active: !on_date,
            on_commit: commit,
        });
    } else {
        time_wheels = mouse_time_wheels(
            time_wheels,
            MouseWheels {
                id: &ElementId::Name(format!("{id}-time").into()),
                time: ctx.wheel_time,
                hour_format,
                seconds: input.seconds,
                min: tmin,
                max: tmax,
                commit,
            },
            i18n,
        );
    }
    let save = ctx.clone();
    let time_panel = div()
        .id(ElementId::Name(format!("{id}-time-panel").into()))
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .gap(t.spacing.s2)
        .col_start(1)
        .col_end(2)
        .row_start(1)
        .row_end(2)
        .role(Role::Group)
        .aria_label(time_label.clone())
        .when(on_date, |el| el.invisible())
        .child(time_wheels)
        .child(footer(
            div()
                .flex()
                .w_full()
                .debug_selector({
                    let selector = format!("{id}-save");
                    move || selector.clone()
                })
                .child(
                    Button::new(
                        ElementId::Name(format!("{id}-save").into()),
                        i18n.t("@astryx.dateInput.savePicking"),
                    )
                    .variant(ButtonVariant::Primary)
                    .size(ElementSize::Md)
                    .width(gpui::relative(1.))
                    .on_click(move |_, window, cx| save.close(window, cx)),
                )
                .into_any_element(),
        ));
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .gap(t.spacing.s3)
        .px(t.spacing.s4)
        .pb(t.spacing.s4)
        .child(mode)
        .child(
            div()
                .grid()
                .grid_cols(1)
                .w_full()
                .min_w_0()
                .child(date_panel)
                .child(time_panel),
        )
        .into_any_element()
}
