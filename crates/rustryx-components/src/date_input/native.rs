//! The platform's own pickers (Astryx patch R16), upstream
//! `DateInput/NativeDateField.tsx`, `TimeInput`'s native field and
//! `DateTimeInput/NativeDateSegment.tsx` and `NativeTimeSegment.tsx`.
//!
//! A native segment shows the committed value in the field's box; Space,
//! ArrowDown, F4 or a press opens the platform's picker over it, and the
//! picker's answer commits through the field's own rules. A value the
//! constraints refuse never commits: it is held, and Enter or blur shows why,
//! as upstream's control does. Where the platform has no picker, `Native`
//! resolves to the pointer's Astryx surface.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, IntoElement, KeyDownEvent, NativePickerKind,
    NativePickerOutcome, NativePickerRequest, Pixels, Role, SharedString, StyleRefinement, Styled,
    Subscription, Window, div, prelude::*, px,
};
use rustryx_i18n::date::{DateParse, PlainDate};
use rustryx_i18n::{current as i18n, use_direction};
use rustryx_primitives::editing::FocusFn;
use rustryx_primitives::{
    ElementSize, Politeness, announce, control_focus, is_focus_visible, provide, resolve_size,
};
use rustryx_theme::{ResolvedTheme, ThemeStyled, current, scope};

use super::date_input::DateInput;
use super::presentation::Surface;
use super::typed::{IsDisabled, TypedDate};
use crate::button::{ButtonVariant, IconButton};
use crate::choice::action::Optimistic;
use crate::field::entry_report::ValueReplacesEntry;
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::icon::{Icon, IconColor, IconSize};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::input::group::{group_context, group_disabled, group_place, inside_member};
use crate::overlay::Disclosure;
use crate::status::{Spinner, SpinnerShade, SpinnerSize};

/// Commits a value, or none, through a field's change path.
pub(crate) type FireFn<V> = Rc<dyn Fn(Option<V>, &mut Window, &mut App)>;

/// How a native picker closed, for the field that opened it.
pub(crate) type OutcomeFn = Rc<dyn Fn(NativePickerOutcome, &mut Window, &mut App)>;

/// `Native` where the platform has no picker of `kind`: the pointer's
/// Astryx surface, the sheet under a finger and `fine` otherwise.
pub(crate) fn native_or_fallback(
    surface: Surface,
    kind: NativePickerKind,
    coarse: bool,
    fine: Surface,
    window: &Window,
) -> Surface {
    if surface == Surface::Native && !window.native_picker_supported(kind) {
        if coarse { Surface::Sheet } else { fine }
    } else {
        surface
    }
}

/// A native segment's place, whether its picker is open, and what Enter or
/// blur commits.
#[derive(Default)]
pub(crate) struct NativeState {
    bounds: Option<Bounds<Pixels>>,
    open: bool,
    on_commit: Option<FocusFn>,
    blur: Option<Subscription>,
}

/// One native field or segment.
pub(crate) struct NativeSegment {
    pub id: ElementId,
    /// The theme target of the control box.
    pub target: &'static str,
    pub icon: &'static str,
    pub icon_label: SharedString,
    pub role: Role,
    pub name: SharedString,
    pub description: Option<SharedString>,
    pub display: String,
    pub placeholder: SharedString,
    pub size: ElementSize,
    pub status: Option<StatusType>,
    /// Disabled paint.
    pub disabled: bool,
    /// No picker opens (disabled or loading).
    pub inert: bool,
    pub busy: bool,
    pub required: bool,
    /// A refused value shows (`aria-invalid` beside an error status).
    pub invalid: bool,
    /// The picker to open; its anchor is the segment's own box.
    pub request: NativePickerRequest,
    pub on_outcome: OutcomeFn,
    /// Enter or blur: shows a held refusal.
    pub on_commit: Option<FocusFn>,
    /// After the value: a clear button, a spinner, a status icon.
    pub trailing: Vec<AnyElement>,
    pub control_style: StyleRefinement,
}

/// Opens the platform picker over the segment.
fn open(
    state: &Entity<NativeState>,
    mut request: NativePickerRequest,
    on_outcome: OutcomeFn,
    window: &mut Window,
    cx: &mut App,
) {
    let (bounds, open) = state.read_with(cx, |s, _| (s.bounds, s.open));
    if open {
        return;
    }
    if let Some(bounds) = bounds {
        request.anchor = bounds;
    }
    let Some(receiver) = window.show_native_picker(request) else {
        return;
    };
    state.update(cx, |s, _| s.open = true);
    let state = state.clone();
    window
        .spawn(cx, async move |cx| {
            let outcome = receiver.await.unwrap_or(NativePickerOutcome::Dismissed);
            cx.update(|window, cx| {
                state.update(cx, |s, _| s.open = false);
                on_outcome(outcome, window, cx);
                window.refresh();
            })
            .ok();
        })
        .detach();
}

/// The segment's box: the icon, the value, then `trailing`.
pub(crate) fn native_segment(
    seg: NativeSegment,
    theme: &ResolvedTheme,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Stateful<gpui::Div> {
    let t = theme.tokens().clone();
    let dir = use_direction(cx);
    let state = window.use_keyed_state(
        ElementId::Name(format!("{:?}-native", seg.id).into()),
        cx,
        |_, _| NativeState::default(),
    );
    let open_now = state.read(cx).open;
    let focus = control_focus(&seg.id, window, cx);
    // Blur commits through whatever this frame's segment commits.
    state.update(cx, |s, _| s.on_commit = seg.on_commit.clone());
    if state.read(cx).blur.is_none() {
        let weak = state.downgrade();
        let blur = window.on_focus_out(&focus, cx, move |_, window, cx| {
            let commit = weak.upgrade().and_then(|s| s.read(cx).on_commit.clone());
            // Focus listeners run inside a draw, where a refresh is lost.
            if let Some(commit) = commit {
                window.defer(cx, move |window, cx| commit(window, cx));
            }
        });
        state.update(cx, |s, _| s.blur = Some(blur));
    }
    let chrome = Chrome {
        id: ElementId::Name(format!("{}-box", seg.id).into()),
        target: seg.target,
        variant: None,
        size: seg.size,
        status: seg.status,
        disabled: seg.disabled,
        read_only: false,
        focused: focus.is_focused(window) || open_now,
        focus_visible: is_focus_visible(&focus, window),
        row: true,
        place: None,
        dir,
    };
    let mut control = control_box(&chrome, theme, window, cx).gap(t.spacing.s2);
    control.style().refine(&seg.control_style);
    let opener = {
        let (state, request, on_outcome, inert) = (
            state.clone(),
            seg.request.clone(),
            seg.on_outcome.clone(),
            seg.inert,
        );
        Rc::new(move |window: &mut Window, cx: &mut App| {
            if !inert {
                open(&state, request.clone(), on_outcome.clone(), window, cx);
            }
        })
    };
    let inert = seg.inert;
    // The icon is no tab stop: a press focuses the value and opens.
    let (open_icon, icon_focus) = (opener.clone(), focus.clone());
    control = control.child(
        div()
            .id(ElementId::Name(format!("{}-icon", seg.id).into()))
            .flex()
            .flex_none()
            .items_center()
            .when(!seg.inert, |el| el.cursor_pointer())
            .role(Role::Button)
            .aria_label(seg.icon_label.clone())
            .aria_disabled(seg.inert)
            .child(
                Icon::new(seg.icon)
                    .size(IconSize::Sm)
                    .color(IconColor::Secondary),
            )
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                if !inert {
                    window.focus(&icon_focus, cx);
                }
                open_icon(window, cx);
            }),
    );
    let (open_click, open_key, commit_key) = (opener.clone(), opener, seg.on_commit.clone());
    let mut value = div()
        .id(ElementId::Name(format!("{}-value", seg.id).into()))
        .debug_selector({
            let selector = format!("{}-value", seg.id);
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
        .text_color(if seg.display.is_empty() {
            t.color.text_secondary
        } else {
            t.color.text_primary
        })
        .when(!seg.inert, |el| el.cursor_pointer())
        .role(seg.role)
        .aria_label(seg.name.clone())
        .aria_value(SharedString::from(seg.display.clone()))
        .aria_disabled(seg.inert)
        .aria_busy(seg.busy)
        .aria_required(seg.required)
        .aria_invalid(seg.status == Some(StatusType::Error) || seg.invalid)
        .aria_expanded(open_now)
        .aria_has_popup(gpui::HasPopup::Dialog)
        .aria_placeholder(seg.placeholder.clone())
        .child(if seg.display.is_empty() {
            seg.placeholder.clone()
        } else {
            SharedString::from(seg.display.clone())
        })
        // The keys below open it; GPUI's keyboard click would turn Enter's
        // commit into an open.
        .on_click(move |event, window, cx| {
            if !event.is_keyboard() {
                open_click(window, cx);
            }
        })
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            let keys = &event.keystroke;
            if keys.modifiers.control || keys.modifiers.platform {
                return;
            }
            match keys.key.as_str() {
                // Enter commits, as in upstream's control.
                "enter" => {
                    if let Some(commit) = &commit_key {
                        commit(window, cx);
                    }
                }
                "space" | "down" | "f4" => {
                    cx.stop_propagation();
                    open_key(window, cx);
                }
                _ => {}
            }
        });
    if let Some(description) = seg.description {
        value = value.aria_description(description);
    }
    // The probe: the box's place, where the picker opens.
    let probe = state.clone();
    control.child(value).children(seg.trailing).child(
        gpui::canvas(
            move |bounds, _, cx| probe.update(cx, |s, _| s.bounds = Some(bounds)),
            |_, _, _, _| {},
        )
        .absolute()
        .size_full(),
    )
}

/// Upstream `useNativeDateRejection`: a date the native picker chose and the
/// field refuses. It is held until Enter or blur shows it, and a new value
/// ends it.
#[derive(Default)]
pub(crate) struct NativeDateRejection {
    held: Option<PlainDate>,
    shown: Option<PlainDate>,
    /// Each Enter or blur, so a value that arrives with one keeps the refusal.
    commits: u64,
    replaces: ValueReplacesEntry<Option<PlainDate>>,
    /// Whether a refusal shows, and the caller's message it started under.
    repeat: (bool, Option<SharedString>),
    last_alert: Option<SharedString>,
}

/// What a native date rejection shows this frame.
pub(crate) struct NativeRejectionView {
    pub message: Option<SharedString>,
    pub field_status: Option<InputStatus>,
    /// An alert to announce now.
    pub alert: Option<SharedString>,
}

impl NativeDateRejection {
    pub fn refuse(&mut self, date: PlainDate) {
        self.held = Some(date);
    }

    pub fn clear(&mut self) {
        self.held = None;
        self.shown = None;
    }

    /// Enter or blur shows a held refusal.
    pub fn commit(&mut self, locked: bool) {
        if !locked {
            self.commits += 1;
            if let Some(held) = self.held.take() {
                self.shown = Some(held);
            }
        }
    }

    pub fn resolve(
        &mut self,
        value: Option<PlainDate>,
        locked: bool,
        status: Option<&InputStatus>,
        has_entry_error_callback: bool,
        status_announced: bool,
        typed: &TypedDate,
    ) -> NativeRejectionView {
        if self.replaces.check(&value, self.commits) {
            self.clear();
        }
        let message = self.shown.filter(|_| !locked).map(|date| {
            typed.error_message(&DateParse::OutOfRange {
                date,
                is_complete: true,
            })
        });
        let caller = status
            .filter(|s| has_entry_error_callback && s.status == StatusType::Error)
            .and_then(|s| s.message.clone());
        // A refusal under the caller's message changes no status, so the alert
        // repeats that message once.
        let rejected = message.is_some();
        if self.repeat.0 != rejected {
            self.repeat = (rejected, if rejected { caller.clone() } else { None });
        } else if self.repeat.1.is_some() && self.repeat.1 != caller {
            self.repeat.1 = None;
        }
        let alert = match &message {
            Some(message) if !status_announced => Some(caller.clone().unwrap_or(message.clone())),
            Some(_) if caller.is_some() && caller == self.repeat.1 => caller.clone(),
            _ => None,
        };
        let changed = alert != self.last_alert;
        self.last_alert = alert.clone();
        NativeRejectionView {
            field_status: match (&message, &caller) {
                (Some(message), None) => Some(InputStatus::error(message.clone())),
                _ => status.cloned(),
            },
            message,
            alert: alert.filter(|_| changed),
        }
    }
}

/// DateInput's native surface (`NativeDateField`).
pub(super) fn render_native(mut input: DateInput, window: &mut Window, cx: &mut App) -> AnyElement {
    input.disabled |= group_disabled(cx);
    let place = group_place(cx);
    let group = place.and_then(|_| group_context(cx));
    let theme = current(window, cx);
    let size = resolve_size(input.size, ElementSize::Md, cx);
    let i18n = i18n(cx);
    let calendar = input.calendar.unwrap_or_else(|| i18n.calendar_system());

    let optimistic = Rc::new(Optimistic::<Option<PlainDate>>::new(&input.id, window, cx));
    let shown = optimistic.shown(input.value, cx);
    let busy = input.loading || optimistic.pending(cx);
    // The picker stays open to a pending Action; its commits are refused.
    let inert = input.disabled || input.loading;
    optimistic.announce_busy(busy, i18n.t("@astryx.spinner.loading"), window, cx);
    let (min, max) = (input.min, input.max);
    let rules = input.date_constraints.clone();
    let is_disabled: IsDisabled = Rc::new(move |date: PlainDate| {
        min.is_some_and(|m| date < m)
            || max.is_some_and(|m| date > m)
            || rules.iter().any(|rule| !rule(date))
    });
    let typed = TypedDate::new(
        calendar,
        input.format.clone(),
        min,
        max,
        is_disabled.clone(),
        &i18n,
    );

    let rejection = window.use_keyed_state(
        ElementId::Name(format!("{:?}-native-rejection", input.id).into()),
        cx,
        |_, _| NativeDateRejection::default(),
    );
    let view = rejection.update(cx, |r, _| {
        r.resolve(
            input.value,
            inert || busy,
            input.status.as_ref(),
            input.on_entry_error_change.is_some(),
            place.is_none(),
            &typed,
        )
    });
    if let Some(alert) = view.alert {
        window.defer(cx, move |window, cx| {
            announce(alert, Politeness::Assertive, window, cx)
        });
    }
    let field_status = view.field_status;

    let fire: FireFn<PlainDate> = {
        let (optimistic, on_change, action) = (
            optimistic.clone(),
            input.on_change.clone(),
            input.change_action.clone(),
        );
        Rc::new(move |next, window, cx| {
            if optimistic.pending(cx) {
                return;
            }
            let change = optimistic.change(on_change.clone(), action.clone());
            change(next, window, cx);
        })
    };
    let on_outcome: OutcomeFn = {
        let (optimistic, fire, value, rejection) = (
            optimistic.clone(),
            fire.clone(),
            input.value,
            rejection.clone(),
        );
        Rc::new(move |outcome, window, cx| {
            if inert || optimistic.pending(cx) {
                return;
            }
            match outcome {
                NativePickerOutcome::Dismissed => {}
                NativePickerOutcome::Cleared => {
                    rejection.update(cx, |r, _| r.clear());
                    if value.is_some() {
                        fire(None, window, cx);
                    }
                }
                NativePickerOutcome::Picked(iso) => {
                    let Some(date) = PlainDate::from_iso(&iso) else {
                        return;
                    };
                    if is_disabled(date) {
                        // The picker may not enforce the bounds; refuse here.
                        rejection.update(cx, |r, _| r.refuse(date));
                        return;
                    }
                    rejection.update(cx, |r, _| r.clear());
                    if Some(date) != value {
                        fire(Some(date), window, cx);
                    }
                }
            }
            window.refresh();
        })
    };
    let on_commit: FocusFn = {
        let (rejection, optimistic) = (rejection.clone(), optimistic.clone());
        Rc::new(move |window, cx| {
            let locked = inert || optimistic.pending(cx);
            rejection.update(cx, |r, _| r.commit(locked));
            window.refresh();
        })
    };

    let mut trailing: Vec<AnyElement> = Vec::new();
    if input.clear && shown.is_some() && !inert {
        let fire = fire.clone();
        // A clear never takes focus back: focus would reopen the picker.
        trailing.push(
            provide(
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
                .on_click(move |_, window, cx| fire(None, window, cx)),
            )
            .into_any_element(),
        );
    }
    if busy {
        trailing.push(
            Spinner::new(ElementId::Name(format!("{}-spinner", input.id).into()))
                .size(SpinnerSize::Sm)
                .shade(SpinnerShade::Inherit)
                .decorative()
                .into_any_element(),
        );
    }
    let status = field_status.as_ref().map(|s| s.status);
    if let Some(status) = status
        && input.status_placement != StatusPlacement::Detached
        && place.is_none()
    {
        let message = field_status.as_ref().and_then(|s| s.message.clone());
        trailing.push(status_indicator(
            &input.id,
            status,
            input.status_placement,
            message.as_ref(),
            &theme,
            window,
            cx,
        ));
    }
    let mut described = described_by(
        input.description.as_ref(),
        field_status.as_ref(),
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
    let iso = |d: Option<PlainDate>| d.map(|d| d.to_string());
    let control = native_segment(
        NativeSegment {
            id: input.id.clone(),
            target: "date-input",
            icon: "calendar",
            icon_label: i18n.t("@astryx.dateInput.openCalendar"),
            role: Role::DateInput,
            name,
            description: (!described.is_empty()).then(|| SharedString::from(described.join(" "))),
            display: shown.map(|v| typed.format_committed(v)).unwrap_or_default(),
            placeholder: input
                .placeholder
                .clone()
                .unwrap_or_else(|| i18n.t("@astryx.dateInput.placeholder")),
            size,
            status,
            disabled: input.disabled,
            inert,
            busy,
            required: crate::field::resolve(input.required, input.optional, cx).required,
            invalid: view.message.is_some(),
            request: NativePickerRequest {
                kind: NativePickerKind::Date,
                value: iso(shown),
                min: iso(input.min),
                max: iso(input.max),
                seconds: false,
                anchor: Bounds::default(),
            },
            on_outcome,
            on_commit: Some(on_commit),
            trailing,
            control_style: input.control_style.clone(),
        },
        &theme,
        window,
        cx,
    )
    .min_w(px(180.));
    let control = scope(theme.clone(), control).into_any_element();
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
    if let Some(status) = field_status {
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
