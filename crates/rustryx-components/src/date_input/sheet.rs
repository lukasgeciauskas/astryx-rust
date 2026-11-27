//! DateInput's bottom-sheet surface, upstream `DateInput/TouchDateField.tsx`:
//! a read-only combobox that opens a BottomSheet with the month pager
//! (`MonthScroller`), the month and year wheels over it, and Save. A tapped
//! day commits and the sheet stays open to correct it (FR9); Save, the
//! handle, the scrim and Escape only close. The wheels move a chosen date
//! with them under a finger (FR23); an arrow at the range's end hides and
//! hands its focus on (FR22).

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, Entity, FocusHandle, IntoElement, KeyDownEvent, Role, SharedString,
    Styled, Window, div, prelude::*, px,
};
use rustryx_i18n::date::{PlainDate, calendar_day_of_month, format_calendar_month_label};
use rustryx_i18n::{current as i18n, use_direction};
use rustryx_primitives::{
    ElementSize, control_focus, is_focus_visible, prefers_reduced_motion, provide, resolve_size,
};
use rustryx_theme::{ThemeStyled, current, scope};
use web_time::Instant;

use super::date_input::DateInput;
use super::month_model::{DEFAULT_MONTH_REACH, MonthModel, clamp_index, date_in_month};
use super::month_scroller::{MonthScroller, MonthScrollerHandle};
use super::month_year_wheels::MonthYearWheels;
use crate::button::{Button, ButtonVariant, IconButton};
use crate::calendar::weekday_headers;
use crate::choice::action::{ActionFn, Optimistic};
use crate::field::{Field, StatusPlacement, StatusType};
use crate::icon::{Icon, IconColor, IconSize};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::input::group::{group_context, group_disabled, group_place, inside_member};
use crate::layout::SpacingStep;
use crate::overlay::{BottomSheet, Disclosure, SheetHeight};
use crate::press::{Press, apply_press};
use crate::status::{Spinner, SpinnerShade, SpinnerSize};
use crate::wheel::options::{DAY_SIZE, PANE_HEIGHT};

type ValueFn = Rc<dyn Fn(Option<PlainDate>, &mut Window, &mut App)>;
type IsDisabled = Rc<dyn Fn(PlainDate) -> bool>;

/// The sheet's own state.
#[derive(Default)]
pub(super) struct SheetState {
    open: bool,
    wheels: bool,
    /// When the wheels last showed or hid: their fade.
    swapped: Option<Instant>,
    /// The month the person moved to; until then the opening month.
    month: Option<i64>,
    /// The opening month, anchored once so the range never shifts.
    anchor: Option<i64>,
}

/// The sheet field's change pipeline and navigation.
#[derive(Clone)]
struct Ctx {
    state: Entity<SheetState>,
    /// Disabled or busy: no change (`isEffectivelyDisabled`).
    locked: bool,
    disabled: bool,
    on_change: Option<ValueFn>,
    action: Option<ActionFn<Option<PlainDate>>>,
    optimistic: Rc<Optimistic<Option<PlainDate>>>,
    scroller: MonthScrollerHandle,
    model: MonthModel,
    min_index: i64,
    max_index: i64,
    month: i64,
    selected: Option<PlainDate>,
    min: Option<PlainDate>,
    max: Option<PlainDate>,
    is_disabled: IsDisabled,
    prev_focus: FocusHandle,
    next_focus: FocusHandle,
    title_focus: FocusHandle,
}

impl Ctx {
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

    fn set_month(&self, month: i64, cx: &mut App) {
        self.state.update(cx, |s, _| s.month = Some(month));
    }

    /// One month either way, from an arrow (FR22: an arrow that hides hands
    /// its focus to the other arrow, else to the month-and-year button).
    fn step(&self, delta: i64, window: &mut Window, cx: &mut App) {
        let target = clamp_index(self.month + delta, self.min_index, self.max_index);
        if target == self.month {
            return;
        }
        self.hand_off_focus(target, window, cx);
        self.set_month(target, cx);
        self.scroller.scroll_to_month(target, true, window, cx);
        window.refresh();
    }

    fn hand_off_focus(&self, target: i64, window: &mut Window, cx: &mut App) {
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
    }

    /// The scroller passed a month; ignored while the wheels steer it.
    fn visible_month(&self, month: i64, window: &mut Window, cx: &mut App) {
        if self.state.read(cx).wheels {
            return;
        }
        self.hand_off_focus(month, window, cx);
        self.set_month(month, cx);
        window.refresh();
    }

    /// A wheel rested on a month: the calendar follows at once, and under a
    /// finger a chosen date moves with it, keeping its day (FR23).
    fn wheel_month(&self, month: i64, window: &mut Window, cx: &mut App) {
        self.set_month(month, cx);
        self.scroller.scroll_to_month(month, false, window, cx);
        if !rustryx_primitives::coarse_primary_pointer(window, cx) {
            window.refresh();
            return;
        }
        if let Some(selected) = self.selected {
            let moved = date_in_month(&self.model, selected, month, self.min, self.max);
            if moved != selected && !(self.is_disabled)(moved) {
                self.fire(Some(moved), window, cx);
            }
        }
        window.refresh();
    }

    /// Reset: no date, and today's month when the range reaches it.
    fn reset(&self, window: &mut Window, cx: &mut App) {
        if self.disabled {
            return;
        }
        self.fire(None, window, cx);
        let current = self.model.index_of(PlainDate::today());
        if current < self.min_index || current > self.max_index || current == self.month {
            return;
        }
        self.set_month(current, cx);
        self.scroller.scroll_to_month(current, true, window, cx);
    }

    fn toggle_wheels(&self, window: &mut Window, cx: &mut App) {
        let at = cx.background_executor().now();
        let (closing, month) = self.state.update(cx, |s, _| {
            s.wheels = !s.wheels;
            s.swapped = Some(at);
            (!s.wheels, s.month)
        });
        // Back on the calendar, it rests on the wheels' month.
        if closing && let Some(month) = month {
            self.scroller.scroll_to_month(month, false, window, cx);
        }
        window.refresh();
    }
}

/// A small pressable in the sheet's header: an arrow or the title.
pub(crate) fn header_button(
    id: ElementId,
    focus: &FocusHandle,
    label: SharedString,
    theme: &rustryx_theme::ResolvedTheme,
    window: &mut Window,
    cx: &mut App,
    on_press: impl Fn(&mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let t = theme.tokens();
    let rem = window.rem_size();
    // Upstream TouchDateField's `usePressFeedback`: a finger's press paints as UIKit
    // tracks a control.
    let feedback = crate::gestures::use_press_feedback(
        ElementId::Name(format!("{id:?}-press").into()),
        window,
        cx,
    )
    .space_activates(focus);
    let el = div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.element)
        .role(Role::Button)
        .aria_label(label);
    apply_press(
        el,
        focus,
        Press {
            feedback: Some(&feedback),
            fill: None,
            scale: None,
            disabled: false,
            busy: false,
            ring: Some((t.radius.element.to_pixels(rem), None)),
            raise_ring: false,
            theme_target: None,
        },
        Some(Rc::new(move |_, window, cx| on_press(window, cx))),
        theme,
        window,
        cx,
    )
    .child(feedback.probe().absolute().top_0().left_0().size_full())
}

pub(super) fn render_sheet(mut input: DateInput, window: &mut Window, cx: &mut App) -> AnyElement {
    input.disabled |= group_disabled(cx);
    let place = group_place(cx);
    let group = place.and_then(|_| group_context(cx));
    let dir = use_direction(cx);
    let rtl = dir.is_rtl();
    let theme = current(window, cx);
    let size = resolve_size(input.size, ElementSize::Md, cx);
    let i18n = i18n(cx);
    let locale = i18n.locale().clone();
    let calendar = input.calendar.unwrap_or_else(|| i18n.calendar_system());
    let coarse = rustryx_primitives::coarse_primary_pointer(window, cx);

    let optimistic = Rc::new(Optimistic::<Option<PlainDate>>::new(&input.id, window, cx));
    let shown = optimistic.shown(input.value, cx);
    let busy = input.loading || optimistic.pending(cx);
    let locked = input.disabled || busy;
    optimistic.announce_busy(busy, i18n.t("@astryx.spinner.loading"), window, cx);

    let model = MonthModel::new(calendar);
    let today = PlainDate::today();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{:?}-date-sheet", input.id).into()),
        cx,
        |_, _| SheetState::default(),
    );
    let anchor = state.update(cx, |s, _| {
        *s.anchor
            .get_or_insert_with(|| model.index_of(input.value.unwrap_or(today)))
    });
    let min_index = input
        .min
        .map_or(anchor - DEFAULT_MONTH_REACH, |m| model.index_of(m));
    let max_index = input
        .max
        .map_or(anchor + DEFAULT_MONTH_REACH, |m| model.index_of(m));
    let (open, wheels, swapped, chosen) = {
        let s = state.read(cx);
        (s.open, s.wheels, s.swapped, s.month)
    };
    let month = chosen.unwrap_or_else(|| clamp_index(anchor, min_index, max_index));

    let (min, max) = (input.min, input.max);
    let rules = input.date_constraints.clone();
    let is_disabled: IsDisabled = Rc::new(move |date: PlainDate| {
        min.is_some_and(|m| date < m)
            || max.is_some_and(|m| date > m)
            || rules.iter().any(|rule| !rule(date))
    });
    let prev_id = ElementId::Name(format!("{}-prev-month", input.id).into());
    let next_id = ElementId::Name(format!("{}-next-month", input.id).into());
    let title_id = ElementId::Name(format!("{}-month-year", input.id).into());
    let scroller = window
        .use_keyed_state(
            ElementId::Name(format!("{:?}-scroller-handle", input.id).into()),
            cx,
            |_, _| MonthScrollerHandle::default(),
        )
        .read(cx)
        .clone();
    let ctx = Ctx {
        state: state.clone(),
        locked,
        disabled: input.disabled,
        on_change: input.on_change.clone(),
        action: input.change_action.clone(),
        optimistic: optimistic.clone(),
        scroller: scroller.clone(),
        model: model.clone(),
        min_index,
        max_index,
        month,
        selected: shown,
        min,
        max,
        is_disabled: is_disabled.clone(),
        prev_focus: control_focus(&prev_id, window, cx),
        next_focus: control_focus(&next_id, window, cx),
        title_focus: control_focus(&title_id, window, cx),
    };

    // ---- The closed field ----------------------------------------------
    let display = shown
        .map(|v| {
            super::typed::TypedDate::new(
                calendar,
                input.format.clone(),
                input.min,
                input.max,
                is_disabled.clone(),
                &i18n,
            )
            .format_committed(v)
        })
        .unwrap_or_default();
    let placeholder = input
        .placeholder
        .clone()
        .unwrap_or_else(|| i18n.t("@astryx.dateInput.placeholder"));
    let required = crate::field::resolve(input.required, input.optional, cx).required;
    let trigger_id = ElementId::Name(format!("{}-trigger", input.id).into());
    let focus = control_focus(&trigger_id, window, cx);
    let status = input.status.as_ref().map(|s| s.status);
    let chrome = Chrome {
        id: ElementId::Name(format!("{}-control", input.id).into()),
        target: "date-input",
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
    let part_override = theme.for_part("date-input", &chrome.props());
    let part = part_override.clone().unwrap_or_else(|| theme.clone());
    let t = part.tokens().clone();
    let mut control = control_box(&chrome, &part, window, cx)
        .min_w(px(180.))
        .gap(t.spacing.s2);
    control.style().refine(&input.control_style);

    let show = {
        let state = state.clone();
        move |window: &mut Window, cx: &mut App| {
            if !locked {
                // Always onto the calendar: the wheels are a detour.
                state.update(cx, |s, _| {
                    s.open = true;
                    s.wheels = false;
                    s.swapped = None;
                });
                window.refresh();
            }
        }
    };
    let toggle_id = ElementId::Name(format!("{}-toggle", input.id).into());
    let toggle_focus = control_focus(&toggle_id, window, cx).tab_stop(false);
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
    let open_toggle = show.clone();
    control = control.child(
        div()
            .id(toggle_id)
            .track_focus(&toggle_focus)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(t.radius.element)
            .when(!locked, |el| el.cursor_pointer())
            .role(Role::Button)
            .aria_label(i18n.t("@astryx.dateInput.openCalendar"))
            .aria_disabled(locked)
            .child(glyph)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                open_toggle(window, cx);
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
    let (open_click, open_key) = (show.clone(), show.clone());
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
        .when(!locked, |el| el.cursor_pointer())
        .role(Role::ComboBox)
        .aria_label(name)
        .aria_value(SharedString::from(display.clone()))
        .aria_disabled(locked)
        .aria_busy(busy)
        .aria_expanded(open)
        .aria_has_popup(gpui::HasPopup::Dialog)
        .aria_required(required)
        .aria_invalid(status == Some(StatusType::Error))
        .aria_placeholder(placeholder.clone())
        .child(if display.is_empty() {
            placeholder
        } else {
            SharedString::from(display)
        })
        .on_click(move |_, window, cx| open_click(window, cx))
        // APG combobox keys open the sheet (FR8).
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
    if input.clear && input.value.is_some() && !locked {
        let (clear, focus) = (ctx.clone(), focus.clone());
        control = control.child(provide(
            Disclosure::detached(),
            IconButton::new(
                ElementId::Name(format!("{}-clear", input.id).into()),
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
                // Focus goes back to the field once the button is gone.
                let focus = focus.clone();
                window.defer(cx, move |window, cx| window.focus(&focus, cx));
            }),
        ));
    }
    if busy {
        control = control.child(
            Spinner::new(ElementId::Name(format!("{}-spinner", input.id).into()))
                .size(SpinnerSize::Sm)
                .shade(SpinnerShade::Inherit)
                .decorative(),
        );
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

    // ---- The sheet -------------------------------------------------------
    let body = if open {
        Some(sheet_body(
            &input, &ctx, &t, &theme, rtl, coarse, wheels, swapped, today, &locale, &i18n, window,
            cx,
        ))
    } else {
        None
    };
    let close = state.clone();
    let mut sheet = BottomSheet::new(
        ElementId::Name(format!("{}-sheet", input.id).into()),
        open,
        i18n.t("@astryx.dateInput.dialogLabel"),
    )
    .height(SheetHeight::Hug)
    .padding(SpacingStep::S0)
    .on_open_change(move |open, window, cx| {
        close.update(cx, |s, _| s.open = open);
        window.refresh();
    });
    if let Some(body) = body {
        sheet = sheet.child(body);
    }
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

#[allow(clippy::too_many_arguments)]
fn sheet_body(
    input: &DateInput,
    ctx: &Ctx,
    t: &rustryx_theme::Tokens,
    theme: &rustryx_theme::ResolvedTheme,
    rtl: bool,
    coarse: bool,
    wheels: bool,
    swapped: Option<Instant>,
    today: PlainDate,
    locale: &SharedString,
    i18n: &rustryx_i18n::I18n,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let calendar = ctx.model.calendar;
    let kept_day = ctx
        .selected
        .map_or(1, |d| calendar_day_of_month(calendar, d));
    let month_label = format_calendar_month_label(
        calendar,
        &ctx.model.month_at(ctx.month),
        locale,
        Some(kept_day),
    );
    // The wheels fade in over the calendar, and out again.
    let fade = if prefers_reduced_motion(cx) {
        1.
    } else {
        swapped.map_or(1., |at| {
            let elapsed = cx.background_executor().now().saturating_duration_since(at);
            (elapsed.as_secs_f32() / t.duration.fast.as_secs_f32()).min(1.)
        })
    };
    if fade < 1. {
        window.request_animation_frame();
    }
    let wheel_opacity = if wheels { fade } else { 1. - fade };

    // The header: the month and year (the wheels' toggle), the arrows and
    // Reset.
    let id = &input.id;
    let title_ctx = ctx.clone();
    let chevron = div()
        .flex()
        .flex_none()
        // Upstream rotates it a half turn; the chevron is symmetric, so a
        // vertical flip on the swap's progress is the same turn.
        .scale_xy(1., 1. - 2. * if wheels { fade } else { 1. - fade })
        .child(
            Icon::new("chevronDown")
                .size(IconSize::Sm)
                .color(IconColor::Secondary),
        );
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
    .px(t.spacing.s2)
    .ml(-t.spacing.s2.to_pixels(window.rem_size()))
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
    .child(chevron);
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
            &focus.clone().tab_stop(shown && !wheels),
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
        // A finger gets the sheet's 44px floor.
        .when(coarse, |el| {
            el.min_w(px(DAY_SIZE as f32)).min_h(px(DAY_SIZE as f32))
        })
        .child(
            div()
                .flex()
                .when(rtl, |el| el.scale_xy(-1., 1.))
                .child(Icon::new(icon).size(IconSize::Sm).color(IconColor::Primary)),
        );
        // An arrow with nowhere to go keeps its place, hidden.
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
    let reset = div()
        .flex()
        .items_center()
        .when(wheels, |el| el.invisible())
        .child(
            Button::new(
                ElementId::Name(format!("{id}-reset").into()),
                i18n.t("@astryx.dateInput.resetPicking"),
            )
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Sm)
            .when(coarse, |b| b.min_h(px(DAY_SIZE as f32)))
            .on_click(move |_, window, cx| reset_ctx.reset(window, cx)),
        );
    let header = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(t.spacing.s2)
        .h(t.size.element_lg)
        .child(title)
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s0_5)
                .ml_auto()
                .child(prev)
                .child(next),
        )
        .child(reset);

    // The weekday row (FR19): Calendar's header names, in the panes' order.
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
    if wheels {
        weekdays = weekdays.invisible();
    }

    // The calendar, with the wheels as an opaque layer over it.
    let (visible, select, wheel) = (ctx.clone(), ctx.clone(), ctx.clone());
    let mut body = div()
        .relative()
        .w_full()
        .h(px(PANE_HEIGHT as f32))
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
            selected: ctx.selected,
            today,
            is_disabled: ctx.is_disabled.clone(),
            week_starts_on: input.week_starts_on,
            on_select: Rc::new(move |date, window, cx| select.fire(Some(date), window, cx)),
            active: !wheels,
        });
    if wheels || wheel_opacity > 0. {
        body = body.child(
            div()
                .absolute()
                .inset_0()
                .bg(t.color.background_surface)
                .opacity(wheel_opacity)
                .child(MonthYearWheels {
                    id: ElementId::Name(format!("{id}-wheels").into()),
                    model: ctx.model.clone(),
                    month_index: ctx.month,
                    min_index: ctx.min_index,
                    max_index: ctx.max_index,
                    on_change: Rc::new(move |month, window, cx| {
                        wheel.wheel_month(month, window, cx)
                    }),
                    month_label: i18n.t("@astryx.dateInput.monthWheel"),
                    year_label: i18n.t("@astryx.dateInput.yearWheel"),
                    active: wheels,
                    kept_day,
                }),
        );
    }

    // Save finishes on the calendar; Done finishes the detour through the
    // wheels. Neither commits: a tapped day already has.
    let footer_ctx = ctx.clone();
    let action = if wheels { "done" } else { "save" };
    let footer = div().flex().pt(t.spacing.s2).child(
        div()
            .flex()
            .w_full()
            .debug_selector({
                let selector = format!("{id}-{action}");
                move || selector.clone()
            })
            .child(if wheels {
                Button::new(
                    ElementId::Name(format!("{id}-done").into()),
                    i18n.t("@astryx.dateInput.doneChoosingMonth"),
                )
                .variant(ButtonVariant::Secondary)
                .size(ElementSize::Md)
                .width(gpui::relative(1.))
                .on_click(move |_, window, cx| footer_ctx.toggle_wheels(window, cx))
                .into_any_element()
            } else {
                let close = ctx.state.clone();
                Button::new(
                    ElementId::Name(format!("{id}-save").into()),
                    i18n.t("@astryx.dateInput.savePicking"),
                )
                .variant(ButtonVariant::Primary)
                .size(ElementSize::Md)
                .width(gpui::relative(1.))
                .on_click(move |_, window, cx| {
                    close.update(cx, |s, _| s.open = false);
                    window.refresh();
                })
                .into_any_element()
            }),
    );
    div()
        .flex()
        .flex_col()
        .w_full()
        .px(t.spacing.s4)
        .pb(t.spacing.s4)
        .child(header)
        .child(weekdays)
        .child(body)
        .child(footer)
        .into_any_element()
}
