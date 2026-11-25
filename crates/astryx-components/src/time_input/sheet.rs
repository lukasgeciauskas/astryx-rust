//! TimeInput's bottom-sheet surface, upstream `TimeInput/TouchTimeField.tsx`
//! (FR7): a read-only combobox that opens hour, minute, optional second and
//! (12-hour) AM/PM wheels in a BottomSheet. Each wheel commits as it rests,
//! clamped into `min` and `max`; Save only closes the sheet. Under a finger
//! the wheels are UIDatePicker's (the AM/PM wheel follows the hour, the
//! minutes loop); under a mouse each turns only its own part of the time.

use std::rc::Rc;

use astryx_i18n::date::{HourFormat, PlainTime, clamp_time, format_locale_time, is_time_in_range};
use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{ElementSize, control_focus, is_focus_visible, provide, resolve_size};
use astryx_theme::{ThemeStyled, current, scope};
use gpui::{
    AnyElement, App, ElementId, IntoElement, KeyDownEvent, Role, SharedString, Styled, Window, div,
    prelude::*, px,
};

use super::time_input::TimeInput;
use super::wheel_rules::{
    hour_rows_in_half, hour12_from_24, hour24_from_12, meridiem_rows, minute_rows, second_rows,
    wheel_bounds,
};
use super::wheels::{TimeCommit, TimeWheels};
use crate::button::{Button, ButtonVariant, IconButton};
use crate::choice::action::{ActionFn, Optimistic};
use crate::field::{Field, StatusPlacement, StatusType};
use crate::icon::{Icon, IconColor, IconSize};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::input::group::{group_context, group_disabled, group_place, inside_member};
use crate::layout::SpacingStep;
use crate::overlay::{BottomSheet, Disclosure, SheetHeight};
use crate::status::{Spinner, SpinnerShade, SpinnerSize};
use crate::wheel::{Wheel, WheelOption};

type ValueFn = Rc<dyn Fn(Option<PlainTime>, &mut Window, &mut App)>;

/// The sheet field's change pipeline.
#[derive(Clone)]
struct SheetCtx {
    /// Disabled or loading: no change at all (a pending Action still edits).
    inert: bool,
    value: Option<PlainTime>,
    min: Option<PlainTime>,
    max: Option<PlainTime>,
    seconds: bool,
    on_change: Option<ValueFn>,
    action: Option<ActionFn<Option<PlainTime>>>,
    optimistic: Rc<Optimistic<Option<PlainTime>>>,
}

impl SheetCtx {
    fn fire(&self, value: Option<PlainTime>, window: &mut Window, cx: &mut App) {
        if self.inert {
            return;
        }
        let change = self
            .optimistic
            .change(self.on_change.clone(), self.action.clone());
        change(value, window, cx);
        window.refresh();
    }

    /// Upstream `commitWheelTime`: clamped into the bounds, a new time fires.
    fn commit(&self, raw: PlainTime, window: &mut Window, cx: &mut App) {
        let next = clamp_time(raw, self.min, self.max, self.seconds);
        if Some(next) != self.value && is_time_in_range(next, self.min, self.max) {
            self.fire(Some(next), window, cx);
        }
    }
}

pub(super) fn render_sheet(
    mut input: TimeInput,
    hour_format: HourFormat,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    input.disabled |= group_disabled(cx);
    let place = group_place(cx);
    let group = place.and_then(|_| group_context(cx));
    let dir = use_direction(cx);
    let theme = current(window, cx);
    let size = resolve_size(input.size, ElementSize::Md, cx);
    let i18n = i18n(cx);
    let locale = i18n.locale().clone();

    let optimistic = Rc::new(Optimistic::<Option<PlainTime>>::new(&input.id, window, cx));
    let shown = optimistic.shown(input.value, cx);
    let busy = input.loading || optimistic.pending(cx);
    let inert = input.disabled || input.loading;
    optimistic.announce_busy(busy, i18n.t("@astryx.spinner.loading"), window, cx);
    let ctx = SheetCtx {
        inert,
        value: shown,
        min: input.min,
        max: input.max,
        seconds: input.seconds,
        on_change: input.on_change.clone(),
        action: input.change_action.clone(),
        optimistic: optimistic.clone(),
    };

    let open_state = window.use_keyed_state(
        ElementId::Name(format!("{:?}-sheet-open", input.id).into()),
        cx,
        |_, _| false,
    );
    let open = *open_state.read(cx);
    let display = shown
        .map(|v| format_locale_time(v, &locale, hour_format, input.seconds))
        .unwrap_or_default();
    let placeholder = input
        .placeholder
        .clone()
        .unwrap_or_else(|| i18n.t("@astryx.timeInput.placeholder"));
    let required = crate::field::resolve(input.required, input.optional, cx).required;

    let trigger_id = ElementId::Name(format!("{}-trigger", input.id).into());
    let focus = control_focus(&trigger_id, window, cx);
    let status = input.status.as_ref().map(|s| s.status);
    let chrome = Chrome {
        id: ElementId::Name(format!("{}-control", input.id).into()),
        target: "time-input",
        variant: None,
        size,
        status,
        disabled: input.disabled,
        read_only: false,
        focused: focus.is_focused(window),
        focus_visible: is_focus_visible(&focus, window),
        row: true,
        place,
        dir,
    };
    let part_override = theme.for_part("time-input", &chrome.props());
    let part = part_override.clone().unwrap_or_else(|| theme.clone());
    let t = part.tokens();
    let mut control = control_box(&chrome, &part, window, cx).min_w(px(120.));
    control.style().refine(&input.control_style);

    let show = {
        let open_state = open_state.clone();
        move |window: &mut Window, cx: &mut App| {
            if !inert {
                open_state.update(cx, |o, _| *o = true);
                window.refresh();
            }
        }
    };
    // The clock opens the sheet; the field is the tab stop.
    let clock_id = ElementId::Name(format!("{}-clock", input.id).into());
    let clock_focus = control_focus(&clock_id, window, cx).tab_stop(false);
    let open_clock = show.clone();
    control = control.child(
        div()
            .id(clock_id)
            .track_focus(&clock_focus)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .when(!inert, |el| el.cursor_pointer())
            .role(Role::Button)
            .aria_label(i18n.format(
                "@astryx.timeInput.openPicker",
                &[("label", input.label.as_ref().into())],
            ))
            .aria_disabled(inert)
            .child(
                Icon::new("clock")
                    .size(IconSize::Sm)
                    .color(IconColor::Secondary),
            )
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                open_clock(window, cx);
            }),
    );

    let mut described = described_by(
        input.description.as_ref(),
        input.status.as_ref(),
        input
            .disabled
            .then_some(input.disabled_message.as_ref())
            .flatten(),
        &i18n,
    );
    let name = match &group {
        Some(group) => {
            described = group.described.iter().cloned().chain(described).collect();
            SharedString::from(format!("{} {}", group.label, input.label))
        }
        None => input.label.clone(),
    };
    let open_click = show.clone();
    let open_key = show.clone();
    let mut trigger = div()
        .id(trigger_id)
        .debug_selector({
            let selector = format!("{}-trigger", input.id);
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
        .text_color(if display.is_empty() {
            t.color.text_secondary
        } else {
            t.color.text_primary
        })
        .when(!inert, |el| el.cursor_pointer())
        .role(Role::ComboBox)
        .aria_label(name)
        .aria_value(SharedString::from(display.clone()))
        .aria_disabled(inert)
        .aria_busy(busy)
        .aria_expanded(open)
        .aria_has_popup(gpui::HasPopup::Dialog)
        .aria_required(required)
        .aria_invalid(status == Some(StatusType::Error))
        .aria_placeholder(placeholder.clone())
        .child(if display.is_empty() {
            placeholder
        } else {
            SharedString::from(display.clone())
        })
        .on_click(move |_, window, cx| open_click(window, cx))
        // The keyboard opens the sheet as a combobox opens its popup.
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
    if !described.is_empty() {
        trigger = trigger.aria_description(SharedString::from(described.join(" ")));
    }
    control = control.child(trigger);
    if busy {
        control = control.child(
            Spinner::new(ElementId::Name(format!("{}-spinner", input.id).into()))
                .size(SpinnerSize::Sm)
                .shade(SpinnerShade::Inherit)
                .decorative(),
        );
    }
    if input.clear && shown.is_some() && !inert {
        let (clear, focus) = (ctx.clone(), focus.clone());
        control = control.child(provide(
            Disclosure::detached(),
            IconButton::new(
                ElementId::Name(format!("{}-clear", input.id).into()),
                i18n.format(
                    "@astryx.timeInput.clearLabel",
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
            .on_click(move |event, window, cx| {
                clear.fire(None, window, cx);
                if event.is_keyboard() {
                    window.focus(&focus, cx);
                }
            }),
        ));
    }
    if let Some(status) = status
        && input.status_placement != StatusPlacement::Detached
        && place.is_none()
    {
        let message = input.status.as_ref().and_then(|s| s.message.clone());
        control = control.child(status_indicator(
            &input.id,
            status,
            input.status_placement,
            message.as_ref(),
            &part,
            window,
            cx,
        ));
    }

    // The sheet: the wheels, then Save.
    let wheel_time = shown.unwrap_or_else(|| PlainTime::now().truncated(input.seconds));
    let touch = astryx_primitives::coarse_primary_pointer(window, cx);
    let mut wheels = div()
        .flex()
        .w_full()
        .min_w_0()
        .h(px(crate::wheel::options::PANE_HEIGHT as f32))
        .gap(t.spacing.s2);
    if open {
        if touch {
            let commit = ctx.clone();
            wheels = wheels.child(TimeWheels {
                id: ElementId::Name(format!("{}-wheels", input.id).into()),
                time: wheel_time,
                hour_format,
                seconds: input.seconds,
                min: input.min,
                max: input.max,
                active: open,
                on_commit: Rc::new(move |time, window, cx| commit.commit(time, window, cx)),
            });
        } else {
            let commit = ctx.clone();
            wheels = mouse_time_wheels(
                wheels,
                MouseWheels {
                    id: &input.id,
                    time: wheel_time,
                    hour_format,
                    seconds: input.seconds,
                    min: input.min,
                    max: input.max,
                    commit: Rc::new(move |time, window, cx| commit.commit(time, window, cx)),
                },
                &i18n,
            );
        }
    }
    let close = open_state.clone();
    let save = open_state.clone();
    let body = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .gap(t.spacing.s2)
        .px(t.spacing.s4)
        .pb(t.spacing.s4)
        .child(wheels)
        .child(
            div().flex().mt_auto().pt(t.spacing.s1).child(
                Button::new(
                    ElementId::Name(format!("{}-save", input.id).into()),
                    i18n.t("@astryx.dateInput.savePicking"),
                )
                .variant(ButtonVariant::Primary)
                .size(ElementSize::Md)
                .width(gpui::relative(1.))
                .on_click(move |_, window, cx| {
                    save.update(cx, |o, _| *o = false);
                    window.refresh();
                }),
            ),
        );
    let sheet = BottomSheet::new(
        ElementId::Name(format!("{}-sheet", input.id).into()),
        open,
        input.label.clone(),
    )
    .height(SheetHeight::Hug)
    .padding(SpacingStep::S0)
    .on_open_change(move |open, window, cx| {
        close.update(cx, |o, _| *o = open);
        window.refresh();
    })
    .child(body);
    let control = control.child(sheet);

    let control = match part_override {
        Some(part) => scope(part, control).into_any_element(),
        None => control.into_any_element(),
    };
    let control = crate::overlay::disabled_reason(
        &input.id,
        input.disabled,
        input.disabled_message.as_ref(),
        true,
        control,
    );
    if place.is_some() {
        return inside_member(control).into_any_element();
    }
    let mut field = Field::new(input.id, input.label)
        .description_opt(input.description)
        .label_hidden(input.label_hidden)
        .disabled(input.disabled)
        .optional(input.optional)
        .required(input.required)
        .status_placement(input.status_placement);
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
    field.child(control).into_any_element()
}

/// What a commit from the mouse wheels needs.
pub(crate) struct MouseWheels<'a> {
    pub id: &'a ElementId,
    pub time: PlainTime,
    pub hour_format: HourFormat,
    pub seconds: bool,
    pub min: Option<PlainTime>,
    pub max: Option<PlainTime>,
    pub commit: TimeCommit,
}

/// The wheels under a mouse: each turns only its own part of the time.
pub(crate) fn mouse_time_wheels(
    mut wheels: gpui::Div,
    m: MouseWheels<'_>,
    i18n: &astryx_i18n::I18n,
) -> gpui::Div {
    let bounds = wheel_bounds(m.min, m.max);
    let (time, seconds, hour_format) = (m.time, m.seconds, m.hour_format);
    let compose = move |hour: u8, minute: u8, second: u8| PlainTime {
        hour,
        minute,
        second: if seconds { second } else { 0 },
    };
    let wheel = |key: &str,
                 label: &str,
                 rows: Vec<WheelOption>,
                 value: i64,
                 make: Rc<dyn Fn(i64) -> PlainTime>| {
        let commit = m.commit.clone();
        Wheel::new(
            ElementId::Name(format!("{}-{key}", m.id).into()),
            i18n.t(label),
            rows.into(),
            value,
            move |value, window, cx| commit(make(value), window, cx),
        )
    };
    let hour_value = match hour_format {
        HourFormat::H24 => i64::from(time.hour),
        HourFormat::H12 => i64::from(hour12_from_24(time.hour)),
    };
    let meridiem = i64::from(time.hour >= 12);
    wheels = wheels.child(wheel(
        "hour",
        "@astryx.dateTimeInput.hourWheel",
        hour_rows_in_half(time.hour, hour_format, &bounds),
        hour_value,
        Rc::new(move |v| {
            let hour = match hour_format {
                HourFormat::H24 => v as u8,
                HourFormat::H12 => hour24_from_12(v as u8, meridiem),
            };
            compose(hour, time.minute, time.second)
        }),
    ));
    wheels = wheels.child(wheel(
        "minute",
        "@astryx.dateTimeInput.minuteWheel",
        minute_rows(time.hour, seconds, &bounds),
        i64::from(time.minute),
        Rc::new(move |v| compose(time.hour, v as u8, time.second)),
    ));
    if seconds {
        wheels = wheels.child(wheel(
            "second",
            "@astryx.dateTimeInput.secondWheel",
            second_rows(time.hour, time.minute, &bounds),
            i64::from(time.second),
            Rc::new(move |v| compose(time.hour, time.minute, v as u8)),
        ));
    }
    if hour_format == HourFormat::H12 {
        wheels = wheels.child(wheel(
            "meridiem",
            "@astryx.dateTimeInput.meridiemWheel",
            meridiem_rows(
                &i18n.t("@astryx.dateTimeInput.meridiemAM"),
                &i18n.t("@astryx.dateTimeInput.meridiemPM"),
                &bounds,
            ),
            meridiem,
            Rc::new(move |v| {
                compose(
                    hour24_from_12(hour12_from_24(time.hour), v),
                    time.minute,
                    time.second,
                )
            }),
        ));
    }
    wheels
}
