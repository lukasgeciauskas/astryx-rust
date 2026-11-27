//! DateRangeInput, upstream `DateRangeInput/DateRangeInput.tsx`: a trigger
//! that shows the range and opens a range Calendar, with optional presets
//! beside it (FR1-FR6). The field has no typed entry (FR6).

use std::rc::Rc;

use gpui::{
    App, ElementId, IntoElement, Length, RenderOnce, Role, SharedString, StyleRefinement, Styled,
    Task, Window, div, prelude::*, px,
};
use rustryx_i18n::date::{
    DateRange, DateStyle, PlainDate, Weekday, calendar_month_of, calendar_year_month_of,
    format_calendar_date,
};
use rustryx_i18n::{CalendarSystem, I18n, current as i18n, use_direction};
use rustryx_primitives::{
    ElementSize, LayerAlign, Placement, control_focus, is_focus_visible, provide, resolve_size,
};
use rustryx_theme::{ThemeStyled, current, scope};

use crate::button::{ButtonVariant, IconButton};
use crate::calendar::{Calendar, DateConstraint};
use crate::choice::action::{ActionFn, Optimistic};
use crate::date_input::presentation::coarse_pointer;
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconColor, IconSize};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::overlay::{Disclosure, Popover, PopoverController};
use crate::press::{Press, apply_press};
use crate::status::{Spinner, SpinnerShade, SpinnerSize};

type RangeFn = Rc<dyn Fn(Option<DateRange>, &mut Window, &mut App)>;
type GetRange = Rc<dyn Fn() -> DateRange>;
type IsDisabled = Rc<dyn Fn(PlainDate) -> bool>;

/// A labeled range the field offers beside its calendar, upstream
/// `DateRangePreset`.
#[derive(Clone)]
pub struct DateRangePreset {
    pub label: SharedString,
    pub get_range: GetRange,
}

impl DateRangePreset {
    /// Upstream `{label, getRange}`: `get_range` runs once per render.
    pub fn new(
        label: impl Into<SharedString>,
        get_range: impl Fn() -> DateRange + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            get_range: Rc::new(get_range),
        }
    }
}

/// A date range field, upstream `DateRangeInput`.
///
/// ```ignore
/// DateRangeInput::new("stay", "Stay", range)
///     .presets([DateRangePreset::new("Next 7 days", next_week)])
///     .on_change(|range, _, _| println!("{range:?}"))
/// ```
#[derive(IntoElement)]
pub struct DateRangeInput {
    id: ElementId,
    label: SharedString,
    value: Option<DateRange>,
    on_change: Option<RangeFn>,
    change_action: Option<ActionFn<Option<DateRange>>>,
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
    max_range_span: Option<u32>,
    min_range_span: Option<u32>,
    presets: Vec<DateRangePreset>,
    clear: bool,
    placeholder: Option<SharedString>,
    size: Option<ElementSize>,
    status: Option<InputStatus>,
    status_placement: StatusPlacement,
    label_tooltip: Option<SharedString>,
    number_of_months: u8,
    week_starts_on: Weekday,
    calendar: Option<CalendarSystem>,
    width: Option<Length>,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl DateRangeInput {
    /// Upstream `label` and the controlled `value` (`None` when empty).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<Option<DateRange>>,
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
            max_range_span: None,
            min_range_span: None,
            presets: Vec::new(),
            clear: true,
            placeholder: None,
            size: None,
            status: None,
            status_placement: StatusPlacement::Attached,
            label_tooltip: None,
            number_of_months: 2,
            week_starts_on: Weekday::Sunday,
            calendar: None,
            width: None,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isDisabled`.
        !disabled,
        /// Upstream `disabledMessage`.
        ?disabled_message: SharedString,
        /// Upstream `isLoading`.
        !loading,
        /// Upstream `min`: the first selectable date.
        ?min: PlainDate,
        /// Upstream `max`: the last selectable date.
        ?max: PlainDate,
        /// Upstream `hasClear` (on by default).
        !clear,
        /// Upstream `placeholder`.
        ?placeholder: SharedString,
        /// Upstream `size`.
        ?size: ElementSize,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `labelTooltip`.
        ?label_tooltip: SharedString,
        /// Upstream `calendar`: the trigger text and the popover calendar's.
        ?calendar: CalendarSystem,
        /// Upstream `width`.
        ?width: Length,
    }

    /// Upstream `maxRangeSpan`: the most days a range spans, both ends
    /// counted; a preset past it is disabled.
    pub fn max_range_span(mut self, days: u32) -> Self {
        self.max_range_span = Some(days);
        self
    }

    /// Upstream `minRangeSpan`: the fewest days a range spans, both ends
    /// counted.
    pub fn min_range_span(mut self, days: u32) -> Self {
        self.min_range_span = Some(days);
        self
    }

    /// Upstream `statusVariant`.
    pub fn status_placement(mut self, placement: StatusPlacement) -> Self {
        self.status_placement = placement;
        self
    }

    /// Upstream `onChange`: the range, `None` when cleared.
    pub fn on_change(
        mut self,
        f: impl Fn(Option<DateRange>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`.
    pub fn change_action(
        mut self,
        f: impl Fn(Option<DateRange>, &mut Window, &mut App) -> Task<()> + 'static,
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

    /// Upstream `presets`.
    pub fn presets(mut self, presets: impl IntoIterator<Item = DateRangePreset>) -> Self {
        self.presets.extend(presets);
        self
    }

    /// Upstream `numberOfMonths` (2 by default).
    pub fn number_of_months(mut self, months: u8) -> Self {
        self.number_of_months = if months == 1 { 1 } else { 2 };
        self
    }

    /// Upstream `weekStartsOn`.
    pub fn week_starts_on(mut self, day: impl Into<Weekday>) -> Self {
        self.week_starts_on = day.into();
        self
    }

    /// Upstream `controlXstyle`: the visual control box.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl Styled for DateRangeInput {
    /// Upstream `containerXstyle`: the Field box.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The trigger's text, upstream `formatRangeDisplay`: each end in the
/// calendar, with the year when either end falls outside today's calendar
/// year (FR6).
pub(crate) fn format_range(
    range: Option<DateRange>,
    calendar: CalendarSystem,
    locale: &str,
    today: PlainDate,
    i18n: &I18n,
) -> String {
    let Some(range) = range else {
        return String::new();
    };
    let year_of =
        |date: PlainDate| calendar_year_month_of(calendar, &calendar_month_of(calendar, date)).year;
    let same_year =
        year_of(range.start) == year_of(range.end) && year_of(range.start) == year_of(today);
    let style = if same_year {
        DateStyle::MONTH_DAY
    } else {
        DateStyle::MEDIUM
    };
    let start = format_calendar_date(calendar, range.start, style, locale, false);
    let end = format_calendar_date(calendar, range.end, style, locale, false);
    i18n.format(
        "@astryx.dateRangeInput.rangeValue",
        &[
            ("start", start.as_str().into()),
            ("end", end.as_str().into()),
        ],
    )
    .to_string()
}

/// Whether a range's span, both ends counted, keeps within the bounds
/// (upstream `isRangeWithinSpan`).
fn is_range_within_span(range: DateRange, max: Option<u32>, min: Option<u32>) -> bool {
    let span = range.start.days_until(range.end).unsigned_abs() + 1;
    max.is_none_or(|max| span <= u64::from(max)) && min.is_none_or(|min| span >= u64::from(min))
}

/// The field's behavior, shared by its handlers.
#[derive(Clone)]
struct Ctx {
    locked: bool,
    disabled: bool,
    on_change: Option<RangeFn>,
    action: Option<ActionFn<Option<DateRange>>>,
    optimistic: Rc<Optimistic<Option<DateRange>>>,
    popover: PopoverController,
}

impl Ctx {
    /// Upstream `fireChange`.
    fn fire(&self, value: Option<DateRange>, window: &mut Window, cx: &mut App) {
        if self.locked {
            return;
        }
        let change = self
            .optimistic
            .change(self.on_change.clone(), self.action.clone());
        change(value, window, cx);
        window.refresh();
    }

    /// Upstream `handleRangeSelect`: a calendar range or a preset, then the
    /// popover closes.
    fn select(&self, range: DateRange, window: &mut Window, cx: &mut App) {
        if self.disabled {
            return;
        }
        self.fire(Some(range), window, cx);
        self.popover.hide(window, cx);
    }
}

/// What the popover's content needs.
struct Content {
    id: ElementId,
    ctx: Ctx,
    calendar: CalendarSystem,
    value: Option<DateRange>,
    shown: Option<DateRange>,
    disabled: bool,
    disabled_message: Option<SharedString>,
    min: Option<PlainDate>,
    max: Option<PlainDate>,
    rules: Vec<DateConstraint>,
    is_disabled: IsDisabled,
    max_range_span: Option<u32>,
    min_range_span: Option<u32>,
    presets: Vec<DateRangePreset>,
    number_of_months: u8,
    week_starts_on: Weekday,
}

impl Content {
    fn render(self, window: &mut Window, cx: &mut App) -> gpui::AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let i18n = i18n(cx);
        let rtl = use_direction(cx).is_rtl();
        let border = t.border.width.to_pixels(rem);
        let coarse = coarse_pointer(window, cx);

        // FR5: the presets and the calendar share a wrapping row. When they
        // do not fit side by side, the presets stack above. The group draws
        // a divider on its inline end and its block end, each moved past its
        // margin box; the row clips, so only the one between the parts shows.
        let mut row = div().flex().flex_row().flex_wrap().overflow_hidden();
        if !self.presets.is_empty() {
            let mut group = div()
                .id(ElementId::Name(format!("{}-presets", self.id).into()))
                .flex()
                .flex_col()
                .gap(t.spacing.s1)
                .p(t.spacing.s3)
                .flex_grow(1.)
                .min_w(px(140.))
                .border_b(border)
                .mb(-border)
                .border_color(t.color.border_emphasized)
                .role(Role::Group)
                .aria_label(i18n.t("@astryx.dateRangeInput.presetDateRanges"));
            group = if rtl {
                group.border_l(border).ml(-border)
            } else {
                group.border_r(border).mr(-border)
            };
            theme.refine(group.style(), "date-range-input-presets", &[], None);
            let radius = t.radius.element.to_pixels(rem);
            for (index, preset) in self.presets.iter().enumerate() {
                // ORD1: one resolution, compared and checked; ORD2: a press
                // emits this range.
                let range = (preset.get_range)();
                let active = self.value == Some(range);
                let preset_disabled = self.disabled
                    || !is_range_within_span(range, self.max_range_span, self.min_range_span)
                    || (self.is_disabled)(range.start)
                    || (self.is_disabled)(range.end);
                let id = ElementId::Name(format!("{}-preset-{index}", self.id).into());
                let focus = control_focus(&id, window, cx);
                let selector = format!("{}-preset-{index}", self.id);
                let props = [
                    ("selected", if active { "selected" } else { "" }),
                    ("disabled", if preset_disabled { "disabled" } else { "" }),
                ];
                let mut button = div()
                    .id(id)
                    .debug_selector(move || selector.clone())
                    .track_focus(&focus)
                    .flex()
                    .w_full()
                    .py(t.spacing.s1)
                    .px(t.spacing.s2)
                    .rounded(t.radius.element)
                    .font_stack(&t.font.family.body)
                    .text_size(t.text.label.size)
                    .line_height(gpui::DefiniteLength::Fraction(t.text.label.leading))
                    .role(Role::Button)
                    // AR3: focusable while disabled.
                    .aria_disabled(preset_disabled)
                    .child(preset.label.clone());
                // AR2: the applied preset is current.
                if active {
                    button = button.aria_current(gpui::AriaCurrent::True);
                }
                if let Some(message) = self
                    .disabled
                    .then_some(self.disabled_message.clone())
                    .flatten()
                {
                    button = button.aria_description(message);
                }
                button = if preset_disabled {
                    button.text_color(t.color.text_disabled).cursor_default()
                } else if active {
                    button
                        .bg(t.color.accent_muted)
                        .text_color(t.color.text_accent)
                        .cursor_pointer()
                } else {
                    let hover = t.color.overlay_hover;
                    button
                        .text_color(t.color.text_primary)
                        .cursor_pointer()
                        .when(!coarse, |el| el.hover(move |s| s.bg(hover)))
                };
                let ctx = self.ctx.clone();
                button = button.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if !preset_disabled {
                        ctx.select(range, window, cx);
                    }
                });
                if is_focus_visible(&focus, window) {
                    button = button.child(focus_ring(&t, radius, rem));
                }
                theme.refine(button.style(), "date-range-input-preset", &props, None);
                group = group.child(button);
            }
            row = row.child(group);
        }

        let ctx = self.ctx.clone();
        let mut calendar = Calendar::new(ElementId::Name(format!("{}-calendar", self.id).into()))
            .calendar(self.calendar)
            .range_value(self.shown)
            .on_range_change(move |range, window, cx| ctx.select(range, window, cx))
            .date_constraints(self.rules)
            .number_of_months(self.number_of_months)
            .week_starts_on(self.week_starts_on);
        if let Some(min) = self.min {
            calendar = calendar.min(min);
        }
        if let Some(max) = self.max {
            calendar = calendar.max(max);
        }
        if let Some(span) = self.max_range_span {
            calendar = calendar.max_range_span(span);
        }
        if let Some(span) = self.min_range_span {
            calendar = calendar.min_range_span(span);
        }
        if self.presets.is_empty() {
            row = row.child(calendar);
        } else {
            // The row wraps on the calendar's one-month width (its min-content
            // size, from a zero basis), then the calendar takes the free space.
            // Its margin gives back the border the group's margin takes.
            let pane = div()
                .flex_basis(px(0.))
                .flex_grow(1_000_000.)
                .flex_shrink(1.);
            let pane = if rtl {
                pane.mr(border)
            } else {
                pane.ml(border)
            };
            row = row.child(pane.child(calendar));
        }
        row.into_any_element()
    }
}

impl RenderOnce for DateRangeInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let dir = use_direction(cx);
        let theme = current(window, cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let i18n = i18n(cx);
        let locale = i18n.locale().clone();
        let calendar = self.calendar.unwrap_or_else(|| i18n.calendar_system());
        let required = crate::field::resolve(self.required, self.optional, cx).required;
        let placeholder = self
            .placeholder
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.dateRangeInput.placeholder"));

        let optimistic = Rc::new(Optimistic::<Option<DateRange>>::new(&self.id, window, cx));
        let shown_value = optimistic.shown(self.value, cx);
        let busy = self.loading || optimistic.pending(cx);
        // A disabled or busy field keeps its buttons focusable and blocks
        // activation (AST-085 FR1).
        let locked = self.disabled || busy;
        optimistic.announce_busy(busy, i18n.t("@astryx.spinner.loading"), window, cx);
        let shows_disabled_message = self.disabled && self.disabled_message.is_some();

        let display = format_range(shown_value, calendar, &locale, PlainDate::today(), &i18n);
        let popover_id = ElementId::Name(format!("{}-popover", self.id).into());
        let popover = PopoverController::new(popover_id.clone(), window, cx);
        let open = popover.is_open(cx);
        let ctx = Ctx {
            locked,
            disabled: self.disabled,
            on_change: self.on_change.clone(),
            action: self.change_action.clone(),
            optimistic: optimistic.clone(),
            popover: popover.clone(),
        };

        let trigger_id = ElementId::Name(format!("{}-trigger", self.id).into());
        let trigger_focus = control_focus(&trigger_id, window, cx);
        let status = self.status.as_ref().map(|s| s.status);
        let chrome = Chrome {
            id: ElementId::Name(format!("{}-control", self.id).into()),
            target: "date-range-input",
            variant: None,
            size,
            status,
            disabled: locked,
            read_only: false,
            focused: trigger_focus.is_focused(window) || open,
            focus_visible: is_focus_visible(&trigger_focus, window),
            row: true,
            place: None,
            dir,
        };
        let part_override = theme.for_part("date-range-input", &chrome.props());
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let rem = window.rem_size();
        let mut control = control_box(&chrome, &part, window, cx);
        control.style().refine(&self.control_style);

        // The calendar toggle: out of the tab order, the trigger is the stop.
        let toggle_id = ElementId::Name(format!("{}-toggle", self.id).into());
        let toggle_focus = control_focus(&toggle_id, window, cx).tab_stop(false);
        let mut glyph = div().flex().child(
            Icon::new("calendar")
                .size(IconSize::Sm)
                .color(IconColor::Secondary),
        );
        part.refine(
            glyph.style(),
            "date-range-input-toggle-icon",
            &[("state", if open { "expanded" } else { "collapsed" })],
            None,
        );
        let toggle_popover = popover.clone();
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
                .aria_label(if open {
                    i18n.t("@astryx.dateInput.toggleCalendarClose")
                } else {
                    i18n.t("@astryx.dateInput.openCalendar")
                })
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
            &part,
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
        control = control.child(toggle);

        // The trigger: a button named by the label and the range.
        let mut described =
            described_by(self.description.as_ref(), self.status.as_ref(), None, &i18n);
        if required {
            described.push(i18n.t("@astryx.field.required"));
        }
        if shows_disabled_message && let Some(message) = &self.disabled_message {
            described.push(message.clone());
        }
        let trigger_label = i18n.format(
            "@astryx.dateRangeInput.triggerLabel",
            &[
                ("label", self.label.as_ref().into()),
                (
                    "value",
                    if shown_value.is_some() {
                        display.as_str().into()
                    } else {
                        placeholder.as_ref().into()
                    },
                ),
            ],
        );
        let trigger_popover = popover.clone();
        let mut trigger = div()
            .id(trigger_id)
            .debug_selector({
                let selector = format!("{}-trigger", self.id);
                move || selector.clone()
            })
            .track_focus(&trigger_focus)
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
            .role(Role::Button)
            .aria_label(trigger_label)
            .aria_disabled(locked)
            .aria_busy(busy)
            .aria_expanded(open)
            .aria_has_popup(gpui::HasPopup::Dialog)
            .aria_invalid(status == Some(StatusType::Error))
            .child(if display.is_empty() {
                placeholder.clone()
            } else {
                SharedString::from(display.clone())
            });
        trigger = if locked {
            trigger.cursor_default()
        } else {
            trigger.cursor_pointer()
        };
        trigger = trigger.on_click(move |_, window, cx| {
            if !locked {
                trigger_popover.toggle(window, cx);
            }
        });
        if open {
            trigger = trigger.aria_controls([format!("{popover_id}-surface")]);
        }
        if !described.is_empty() {
            trigger = trigger.aria_description(SharedString::from(described.join(" ")));
        }
        control = control.child(trigger);

        if self.clear && self.value.is_some() && !locked {
            let label = i18n.format(
                "@astryx.dateInput.clear",
                &[("label", self.label.as_ref().into())],
            );
            let (ctx, focus) = (ctx.clone(), trigger_focus.clone());
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
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    ctx.fire(None, window, cx);
                    // A keyboard press leaves focus on the button, which goes
                    // away with the range; the trigger takes it.
                    if event.is_keyboard() {
                        window.focus(&focus, cx);
                    }
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
        {
            let message = self.status.as_ref().and_then(|s| s.message.clone());
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

        let (min, max) = (self.min, self.max);
        let rules = self.date_constraints.clone();
        let is_disabled: IsDisabled = Rc::new(move |date: PlainDate| {
            min.is_some_and(|m| date < m)
                || max.is_some_and(|m| date > m)
                || rules.iter().any(|rule| !rule(date))
        });
        let content = Content {
            id: self.id.clone(),
            ctx: ctx.clone(),
            calendar,
            value: self.value,
            shown: shown_value,
            disabled: self.disabled,
            disabled_message: self.disabled_message.clone(),
            min: self.min,
            max: self.max,
            rules: self.date_constraints.clone(),
            is_disabled,
            max_range_span: self.max_range_span,
            min_range_span: self.min_range_span,
            presets: self.presets.clone(),
            number_of_months: self.number_of_months,
            week_starts_on: self.week_starts_on,
        };
        let control = Popover::new(popover_id, i18n.t("@astryx.dateRangeInput.dialogLabel"))
            .controller(popover.clone())
            .placement(Placement::Below)
            .alignment(LayerAlign::Start)
            .close_button(true)
            .close_button_label(i18n.t("@astryx.dateInput.closeCalendar"))
            .auto_focus(true)
            .light_dismiss(true)
            .return_focus(trigger_focus.clone())
            .padding(crate::layout::SpacingStep::S0)
            .content_fn(move |window, cx| content.render(window, cx))
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

        let mut field = Field::new(self.id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(locked)
            .optional(self.optional)
            .required(self.required)
            .status_placement(self.status_placement);
        if let Some(status) = self.status {
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
