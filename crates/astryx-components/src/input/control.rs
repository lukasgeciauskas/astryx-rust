//! The bordered control box text fields share, upstream `inputWrapperStyles`
//! with the status, disabled-focus and row-size styles of
//! `Field/inputStyles.stylex.ts`.

use std::time::Duration;

use astryx_primitives::ElementSize;
use astryx_theme::color::mix_srgb;
use astryx_theme::{ResolvedTheme, Tokens};
use gpui::{
    App, BoxShadow, Div, ElementId, Hsla, InteractiveElement, Pixels, Stateful, Styled, Window,
    div, point, px,
};

use crate::button::element_height;
use crate::field::StatusType;
use crate::icon::{Icon, IconColor, IconSize};
use crate::motion::transition;

/// The state a control box draws.
pub(crate) struct Chrome {
    pub id: ElementId,
    /// The theming target: `text-input`, `text-area`, `number-input`.
    pub target: &'static str,
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
}

impl Chrome {
    /// The `(prop, value)` pairs themes match on (FR14: `data-size`,
    /// `data-status`, `data-disabled`, `data-readonly`).
    pub fn props(&self) -> Vec<(&'static str, &'static str)> {
        let mut props = vec![("size", self.size.as_str())];
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
    let mut el = div()
        .id(chrome.id.clone())
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
