//! The bordered control box text fields share, upstream `inputWrapperStyles`
//! with the status, disabled-focus and row-size styles of
//! `Field/inputStyles.stylex.ts`.

use std::time::Duration;

use astryx_i18n::Direction;
use astryx_primitives::ElementSize;
use astryx_theme::color::mix_srgb;
use astryx_theme::{ResolvedTheme, Tokens};
use gpui::{
    App, BoxShadow, Div, ElementId, Hsla, InteractiveElement, ParentElement, Pixels, SharedString,
    Stateful, Styled, Window, div, point, px,
};

use super::group::{GroupPlace, join, raised_ring};
use crate::button::element_height;
use crate::field::{InputStatus, StatusType};
use crate::icon::{Icon, IconColor, IconSize};
use crate::motion::transition;

/// The state a control box draws.
pub(crate) struct Chrome {
    pub id: ElementId,
    /// The theming target: `text-input`, `text-area`, `number-input`.
    pub target: &'static str,
    /// A `variant` theming prop, for a target that has one (`selector`).
    pub variant: Option<&'static str>,
    pub size: ElementSize,
    pub status: Option<StatusType>,
    pub disabled: bool,
    pub read_only: bool,
    /// Focus is in the field.
    pub focused: bool,
    /// Keyboard focus is in the field: a disabled field shows it.
    pub focus_visible: bool,
    /// A fixed-height row (TextInput, NumberInput); TextArea grows.
    pub row: bool,
    /// Its place among an InputGroup's members, if it is one.
    pub place: Option<GroupPlace>,
    pub dir: Direction,
}

impl Chrome {
    /// The `(prop, value)` pairs themes match on (FR14: `data-size`,
    /// `data-status`, `data-disabled`, `data-readonly`).
    pub fn props(&self) -> Vec<(&'static str, &'static str)> {
        let mut props: Vec<(&'static str, &'static str)> =
            self.variant.map(|v| ("variant", v)).into_iter().collect();
        props.push(("size", self.size.as_str()));
        if let Some(status) = self.status {
            props.push(("status", status.as_str()));
        }
        if self.disabled {
            props.push(("disabled", "disabled"));
        }
        if self.read_only {
            props.push(("readonly", "readonly"));
        }
        props
    }
}

/// The 2px inset ring of a hovered or focused control.
fn inset(color: Hsla) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(2.),
        inset: true,
    }
}

fn status_border(status: StatusType, t: &Tokens) -> Hsla {
    match status {
        StatusType::Error => t.color.border_error,
        StatusType::Warning => t.color.border_warning,
        StatusType::Success => t.color.border_success,
    }
}

/// The height of a single-line control: the element size, or the text line
/// and borders when the text is set larger (FR11).
pub(crate) fn row_height(size: ElementSize, t: &Tokens, rem: Pixels) -> Pixels {
    let border = t.border.width.to_pixels(rem);
    let line = t.text.body.size.to_pixels(rem) * t.text.body.leading + border * 2.;
    element_height(size, t).to_pixels(rem).max(line)
}

/// The control box: border, surface, padding and the status, focus, hover
/// and disabled looks. `part` is the theme with the target's token
/// re-definitions, which apply to the box itself as CSS custom properties do.
pub(crate) fn control_box(
    chrome: &Chrome,
    part: &ResolvedTheme,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let t = part.tokens();
    let rem = window.rem_size();
    let rest = match chrome.status {
        Some(status) => status_border(status, t),
        None => t.color.border_emphasized,
    };
    // A disabled field shows focus only for the keyboard (FR6).
    let ring = if chrome.disabled {
        chrome.focus_visible
    } else {
        chrome.focused
    };
    let duration: Duration = t.duration.fast;
    let eased = transition(
        format!("{:?}-focus", chrome.id),
        ring,
        duration,
        t.ease.standard,
        window,
        cx,
    );
    // A status keeps its border color while focused.
    let focused_border = match chrome.status {
        Some(_) => rest,
        None => t.color.accent,
    };
    let border = mix_srgb(rest, focused_border, eased);
    let selector = chrome.id.to_string();
    let mut el = div()
        .id(chrome.id.clone())
        .debug_selector(|| selector)
        .relative()
        .flex()
        .flex_row()
        .gap(t.spacing.s2)
        .py(t.spacing.s1)
        .px(t.spacing.s2)
        .border(t.border.width)
        .border_color(border)
        .rounded(t.radius.element)
        .bg(t.color.background_surface)
        .min_w_0();
    if chrome.row {
        el = el.items_center().h(row_height(chrome.size, t, rem));
    } else {
        el = el.items_start();
    }
    if eased > 0. {
        let mut muted = t.color.accent_muted;
        muted.a *= eased;
        el.style().box_shadow = Some(vec![inset(muted)]);
    } else if !chrome.disabled {
        let hover = match chrome.status {
            Some(StatusType::Error) => t.shadow.inset_error.layers(),
            Some(StatusType::Warning) => t.shadow.inset_warning.layers(),
            Some(StatusType::Success) => t.shadow.inset_success.layers(),
            None => vec![inset(mix_srgb(
                Hsla::transparent_black(),
                t.color.border_emphasized,
                0.3,
            ))],
        };
        el = el.hover(move |s| {
            let mut s = s;
            s.box_shadow = Some(hover.clone());
            s
        });
    }
    if chrome.disabled {
        el = el.opacity(0.5);
    }
    part.refine(el.style(), chrome.target, &chrome.props(), None);
    if let Some(place) = chrome.place {
        // In a group the box fills the row, joined to its neighbours, and
        // its focus ring paints above them (upstream `groupStyles.inGroup`).
        let radius = t.radius.element.to_pixels(rem);
        let width = t.border.width.to_pixels(rem);
        el = join(el, place, radius, width, chrome.dir).flex_1();
        el.style().size.height = Some(gpui::relative(1.).into());
        if eased > 0. {
            el = el.child(raised_ring(border, width, place, radius, chrome.dir));
        }
    }
    el
}

/// The trailing status glyph of a control (FR9), upstream
/// `useInputStatusIcon`.
pub(crate) fn status_icon(status: StatusType, part: &ResolvedTheme) -> Icon {
    let color = match status {
        StatusType::Error => IconColor::Error,
        StatusType::Warning => IconColor::Warning,
        StatusType::Success => IconColor::Success,
    };
    let mut icon = Icon::new(status.as_str()).size(IconSize::Md).color(color);
    part.refine(
        icon.style(),
        "input-status-icon",
        &[("size", "md"), ("status", status.as_str())],
        None,
    );
    icon
}

/// A field's accessible description, in upstream's order (ORD3): its own
/// description, the status message (an error without one still says it is
/// invalid), and the reason it is disabled.
pub(crate) fn described_by(
    description: Option<&SharedString>,
    status: Option<&InputStatus>,
    disabled_reason: Option<&SharedString>,
    i18n: &astryx_i18n::I18n,
) -> Vec<SharedString> {
    let mut parts: Vec<SharedString> = description.into_iter().cloned().collect();
    match status {
        Some(InputStatus {
            message: Some(message),
            ..
        }) => parts.push(message.clone()),
        Some(InputStatus {
            status: StatusType::Error,
            message: None,
        }) => parts.push(i18n.t("@astryx.field.errorFallback")),
        _ => {}
    }
    parts.extend(disabled_reason.cloned());
    parts
}

/// The status glyph a control shows: under `tooltip` placement with a
/// message, a focusable button whose tooltip carries the message (upstream
/// `useInputStatusIcon`, FR9); otherwise a plain icon.
pub(crate) fn status_indicator(
    id: &gpui::ElementId,
    status: StatusType,
    placement: crate::field::StatusPlacement,
    message: Option<&SharedString>,
    part: &ResolvedTheme,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    use gpui::{InteractiveElement as _, IntoElement as _, StatefulInteractiveElement as _};
    let icon = status_icon(status, part);
    let Some(message) = message.filter(|_| placement == crate::field::StatusPlacement::Tooltip)
    else {
        return icon.into_any_element();
    };
    let button_id = gpui::ElementId::Name(format!("{id}-status-button").into());
    let focus = astryx_primitives::control_focus(&button_id, window, cx);
    let label = astryx_i18n::current(cx).t(match status {
        StatusType::Error => "@astryx.input.statusButton.error",
        StatusType::Warning => "@astryx.input.statusButton.warning",
        StatusType::Success => "@astryx.input.statusButton.success",
    });
    let rem = window.rem_size();
    let mut button = div()
        .id(button_id)
        .track_focus(&focus)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .role(gpui::Role::Button)
        .aria_label(label)
        .aria_description(message.clone())
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(icon);
    if astryx_primitives::is_focus_visible(&focus, window) {
        button = button.child(crate::focus::focus_ring(part.tokens(), px(999.), rem));
    }
    crate::overlay::Tooltip::new(
        gpui::ElementId::Name(format!("{id}-status-tooltip").into()),
        message.clone(),
    )
    .trigger_acts(false)
    .child(button)
    .into_any_element()
}
