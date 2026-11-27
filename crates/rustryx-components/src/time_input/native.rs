//! TimeInput's native surface (Astryx patch R16), upstream's
//! `NativeTimeSegment` inside `TimeField`: the platform's time picker over the
//! field. Its answer commits through the field's bounds; a time outside them
//! is a surface rejection that shows at once under `Complete`, else at
//! Enter or blur.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, ElementId, IntoElement, NativePickerKind, NativePickerOutcome,
    NativePickerRequest, Role, SharedString, Styled, Window, prelude::*, px,
};
use rustryx_i18n::current as i18n;
use rustryx_i18n::date::{HourFormat, PlainTime, format_locale_time, is_time_in_range};
use rustryx_primitives::editing::FocusFn;
use rustryx_primitives::{ElementSize, provide};
use rustryx_theme::{current, scope};

use super::time_input::TimeInput;
use crate::button::{ButtonVariant, IconButton};
use crate::date_input::native::{FireFn, NativeSegment, OutcomeFn, native_segment};
use crate::field::{Field, InputStatus, StatusPlacement};
use crate::icon::{Icon, IconColor, IconSize};
use crate::input::control::{described_by, status_indicator};
use crate::input::group::{GroupContext, inside_member};
use crate::overlay::Disclosure;
use crate::status::{Spinner, SpinnerShade, SpinnerSize};

/// Upstream `setSurfaceRejected`.
pub(super) type SetRejectedFn = Rc<dyn Fn(bool, &mut App)>;

/// What TimeInput's render resolved for its native surface.
pub(super) struct NativeTime {
    pub hour_format: HourFormat,
    pub size: ElementSize,
    pub shown: Option<PlainTime>,
    pub busy: bool,
    pub field_status: Option<InputStatus>,
    /// The surface rejection shows.
    pub rejected: bool,
    pub in_group: bool,
    pub group: Option<GroupContext>,
    pub fire: FireFn<PlainTime>,
    /// Upstream `setSurfaceRejected`.
    pub set_rejected: SetRejectedFn,
    /// Upstream `commitSurface`: Enter or blur.
    pub commit: FocusFn,
}

pub(super) fn render_native(
    input: TimeInput,
    native: NativeTime,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let i18n = i18n(cx);
    let locale = i18n.locale().clone();
    // Only disabled locks the picker; a pending Action keeps it.
    let inert = input.disabled;
    let on_outcome: OutcomeFn = {
        let (fire, set_rejected) = (native.fire.clone(), native.set_rejected.clone());
        let (value, min, max) = (native.shown, input.min, input.max);
        Rc::new(move |outcome, window, cx| {
            if inert {
                return;
            }
            match outcome {
                NativePickerOutcome::Dismissed => {}
                NativePickerOutcome::Cleared => {
                    set_rejected(false, cx);
                    if value.is_some() {
                        fire(None, window, cx);
                    }
                }
                NativePickerOutcome::Picked(iso) => {
                    let Some(time) = PlainTime::from_iso(&iso) else {
                        return;
                    };
                    if !is_time_in_range(time, min, max) {
                        set_rejected(true, cx);
                        return;
                    }
                    set_rejected(false, cx);
                    if Some(time) != value {
                        fire(Some(time), window, cx);
                    }
                }
            }
            window.refresh();
        })
    };
    let mut trailing: Vec<AnyElement> = Vec::new();
    if native.busy {
        trailing.push(
            Spinner::new(ElementId::Name(format!("{}-spinner", input.id).into()))
                .size(SpinnerSize::Sm)
                .shade(SpinnerShade::Inherit)
                .decorative()
                .into_any_element(),
        );
    }
    if input.clear && native.shown.is_some() && !inert {
        let fire = native.fire.clone();
        // A clear never takes focus back: focus would reopen the picker.
        trailing.push(
            provide(
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
                .on_click(move |_, window, cx| fire(None, window, cx)),
            )
            .into_any_element(),
        );
    }
    let field_status = native.field_status;
    let status = field_status.as_ref().map(|s| s.status);
    if let Some(status) = status
        && input.status_placement != StatusPlacement::Detached
        && !native.in_group
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
    let name = match &native.group {
        Some(group) => {
            described = group.described.iter().cloned().chain(described).collect();
            SharedString::from(format!("{} {}", group.label, input.label))
        }
        None => input.label.clone(),
    };
    // The picker steps whole minutes, as upstream's `step=60`.
    let iso = |t: Option<PlainTime>| t.map(|t| t.to_iso(false));
    let control = native_segment(
        NativeSegment {
            id: input.id.clone(),
            target: "time-input",
            icon: "clock",
            icon_label: i18n.format(
                "@astryx.timeInput.openPicker",
                &[("label", input.label.as_ref().into())],
            ),
            role: Role::TimeInput,
            name,
            description: (!described.is_empty()).then(|| SharedString::from(described.join(" "))),
            display: native
                .shown
                .map(|v| format_locale_time(v, &locale, native.hour_format, false))
                .unwrap_or_default(),
            placeholder: input
                .placeholder
                .clone()
                .unwrap_or_else(|| i18n.t("@astryx.timeInput.placeholder")),
            size: native.size,
            status,
            disabled: input.disabled,
            inert,
            busy: native.busy,
            required: crate::field::resolve(input.required, input.optional, cx).required,
            invalid: native.rejected,
            request: NativePickerRequest {
                kind: NativePickerKind::Time,
                value: iso(native.shown),
                min: iso(input.min),
                max: iso(input.max),
                seconds: false,
                anchor: Bounds::default(),
            },
            on_outcome,
            on_commit: Some(native.commit),
            trailing,
            control_style: input.control_style.clone(),
        },
        &theme,
        window,
        cx,
    )
    .min_w(px(120.));
    let control = scope(theme.clone(), control).into_any_element();
    let control = crate::overlay::disabled_reason(
        &input.id,
        input.disabled,
        input.disabled_message.as_ref(),
        true,
        control,
    );
    if native.in_group {
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
