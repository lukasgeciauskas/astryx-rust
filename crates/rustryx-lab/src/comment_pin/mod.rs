//! `CommentPin`: the avatar-first annotation anchor (upstream lab `CommentPin`,
//! component:CommentPin). One button opens one thread or a caller-defined group of
//! threads; the caller owns grouping, coordinates and what opens.
//!
//! - A single pin shows its lead participant; a grouped pin (more than one thread) shows
//!   its distinct participants as an AvatarGroup of at most three circles (two and an
//!   ellipsis beyond three people) and the exact thread total (FR2, FR3).
//! - Unread activity is an accent dot after the total, or after the participant (FR10).
//! - The silhouette is a speech bubble: half-height corners and a fixed 2 px logical
//!   bottom-start corner, which a host maps its anchor point to (FR8, FR9).
//! - Selected, dimmed, resolved and disabled are independent caller states (FR5): an
//!   unselected resolved pin renders nothing, and a disabled pin keeps its tab stop but
//!   never activates (spec:AST-085).
//! - Every visual part is decorative; the caller's `label` names the button and shows
//!   as its tooltip (AR1, AR3, AR4).

pub mod examples;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, BoxShadow, ClickEvent, ElementId, FontFeatures, Hsla, Pixels, Role, SharedString,
    StyleRefinement, Toggled, Window, div, point, px, relative,
};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_components::focus::focus_ring;
use rustryx_components::gestures::use_press_feedback;
use rustryx_components::icon::{Icon, IconSize};
use rustryx_components::overlay::{Tooltip, TouchTrigger};
use rustryx_components::people::{Avatar, AvatarGroup, AvatarGroupOverflow, AvatarSize};
use rustryx_components::status::{StatusDot, StatusDotVariant};
use rustryx_i18n::{LogicalStyled, use_direction};
use rustryx_primitives::{control_focus, dev_warn_once, is_focus_visible};
use rustryx_theme::ThemeStyled;

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Pin heights on the shared element-size scale (upstream `CommentPinSize`): 28, 32
/// or 36 px.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommentPinSize {
    #[default]
    Sm,
    Md,
    Lg,
}

impl CommentPinSize {
    pub fn as_str(self) -> &'static str {
        match self {
            CommentPinSize::Sm => "sm",
            CommentPinSize::Md => "md",
            CommentPinSize::Lg => "lg",
        }
    }

    /// The participant circle's diameter (upstream `AVATAR_SIZE`): 22, 24 or 28 px.
    pub fn avatar_px(self) -> f32 {
        match self {
            CommentPinSize::Sm => 22.,
            CommentPinSize::Md => 24.,
            CommentPinSize::Lg => 28.,
        }
    }

    /// The overflow ellipsis's size (upstream `OVERFLOW_ICON_SIZE`).
    pub fn overflow_icon(self) -> IconSize {
        match self {
            CommentPinSize::Sm | CommentPinSize::Md => IconSize::Xsm,
            CommentPinSize::Lg => IconSize::Sm,
        }
    }
}

/// Identity data for one representative inside a pin (upstream
/// `CommentPinParticipant`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentPinParticipant {
    /// Upstream `id`: the stable identity that removes duplicate representatives.
    pub id: SharedString,
    /// Upstream `name`: Avatar's fallback initials.
    pub name: SharedString,
    /// Upstream `avatarSrc`: the photo.
    pub avatar_src: Option<SharedString>,
}

impl CommentPinParticipant {
    /// A participant with its `id` and `name`.
    pub fn new(id: impl Into<SharedString>, name: impl Into<SharedString>) -> Self {
        CommentPinParticipant {
            id: id.into(),
            name: name.into(),
            avatar_src: None,
        }
    }

    /// Upstream `avatarSrc`.
    pub fn avatar_src(mut self, src: impl Into<SharedString>) -> Self {
        self.avatar_src = Some(src.into());
        self
    }
}

/// Grouped pins never show more than this many circles, the overflow included (upstream
/// `MAX_GROUP_CIRCLES`).
pub const MAX_GROUP_CIRCLES: usize = 3;

/// What a pin draws for its participants and counts (upstream's ORD1–ORD3).
#[derive(Clone, Debug, PartialEq)]
pub struct CommentPinDisplay {
    /// More than one thread: the AvatarGroup and the total.
    pub grouped: bool,
    /// The thread total, after an invalid value falls back to one.
    pub thread_count: f64,
    /// The unread count, after an invalid value is ignored and a larger one capped.
    pub unread_count: f64,
    /// The participants drawn as Avatars, in order; the anonymous fallback when none.
    pub shown: Vec<CommentPinParticipant>,
    /// The ellipsis overflow circle's count of distinct participants not drawn.
    pub overflow: usize,
}

impl CommentPinDisplay {
    /// Whether the unread dot shows.
    pub fn unread(&self) -> bool {
        self.unread_count > 0.
    }
}

/// Upstream `isValidThreadCount`: a finite integer of at least one.
fn valid_thread_count(n: f64) -> bool {
    n.is_finite() && n.fract() == 0. && n >= 1.
}

/// Upstream `isValidUnreadCount`: a finite non-negative integer.
fn valid_unread_count(n: f64) -> bool {
    n.is_finite() && n.fract() == 0. && n >= 0.
}

/// Upstream `takeDistinctParticipants`: caller order, duplicate ids removed, at most
/// `limit`.
fn take_distinct(
    participants: &[CommentPinParticipant],
    limit: usize,
) -> Vec<CommentPinParticipant> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in participants {
        if !seen.insert(p.id.clone()) {
            continue;
        }
        out.push(p.clone());
        if out.len() == limit {
            break;
        }
    }
    out
}

/// What a pin draws (upstream's count normalization, participant selection and count
/// display).
pub fn comment_pin_display(
    participants: &[CommentPinParticipant],
    thread_count: f64,
    unread_count: f64,
) -> CommentPinDisplay {
    let thread_count = if valid_thread_count(thread_count) {
        thread_count
    } else {
        1.
    };
    let grouped = thread_count > 1.;
    let unread_count = if valid_unread_count(unread_count) {
        unread_count.min(thread_count)
    } else {
        0.
    };
    let distinct = take_distinct(participants, if grouped { usize::MAX } else { 1 });
    let has_overflow = grouped && distinct.len() > MAX_GROUP_CIRCLES;
    let visible: Vec<_> = if has_overflow {
        distinct[..MAX_GROUP_CIRCLES - 1].to_vec()
    } else {
        distinct.clone()
    };
    let overflow = distinct.len() - visible.len();
    let shown = if visible.is_empty() {
        vec![CommentPinParticipant::new("__anonymous__", "")]
    } else {
        visible
    };
    CommentPinDisplay {
        grouped,
        thread_count,
        unread_count,
        shown,
        overflow,
    }
}

/// JavaScript's `String.prototype.trim()` leaves nothing: every character is
/// ECMAScript white space or a line terminator.
fn js_blank(s: &str) -> bool {
    s.chars().all(|c| {
        matches!(
            c,
            '\t' | '\u{b}'
                | '\u{c}'
                | ' '
                | '\u{a0}'
                | '\u{feff}'
                | '\n'
                | '\r'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
        )
    })
}

/// The development warnings a pin's props raise (upstream's `useDevWarning` calls), in
/// upstream order.
pub fn comment_pin_warnings(
    participants: &[CommentPinParticipant],
    thread_count: f64,
    unread_count: f64,
    label: &str,
) -> Vec<&'static str> {
    let resolved = if valid_thread_count(thread_count) {
        thread_count
    } else {
        1.
    };
    let mut out = Vec::new();
    if participants.is_empty() {
        out.push(
            "`participants` should contain at least one participant. Pass the lead participant for a single thread or every distinct participant for a group.",
        );
    }
    if !valid_thread_count(thread_count) {
        out.push("`threadCount` must be a finite positive integer. Invalid values fall back to 1.");
    }
    if !valid_unread_count(unread_count) || unread_count > resolved {
        out.push(
            "`unreadCount` must be a finite non-negative integer no greater than `threadCount`. Invalid values are ignored and larger values are capped.",
        );
    }
    if js_blank(label) {
        out.push("`label` must be a non-empty accessible action name.");
    }
    out
}

/// A count as JavaScript writes the number (the thread total is a JS number upstream):
/// integers below 10²¹ in full, from the shortest digits that round-trip, larger ones
/// in exponent form.
pub fn js_count_text(n: f64) -> String {
    let exp = format!("{n:e}");
    let (mantissa, power) = exp.split_once('e').unwrap_or((&exp, "0"));
    let power: i32 = power.parse().unwrap_or(0);
    if power >= 21 {
        return format!("{mantissa}e+{power}");
    }
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let width = (power + 1).max(0) as usize;
    if digits.len() >= width {
        digits
    } else {
        format!("{digits}{}", "0".repeat(width - digits.len()))
    }
}

/// An avatar-first annotation anchor for one thread or a caller-defined group (upstream
/// lab `CommentPin`).
///
/// ```ignore
/// CommentPin::new("pin-1", "Open annotation from Ada", [ada])
///     .on_click(|_, _, cx| open_thread("thread-1", cx))
/// ```
#[derive(IntoElement)]
pub struct CommentPin {
    id: ElementId,
    label: SharedString,
    participants: Vec<CommentPinParticipant>,
    size: CommentPinSize,
    thread_count: f64,
    unread_count: f64,
    selected: bool,
    dimmed: bool,
    resolved: bool,
    disabled: bool,
    on_click: Option<ClickFn>,
    style: StyleRefinement,
}

impl CommentPin {
    /// A pin named `label` (upstream `label`: the accessible action name and the
    /// tooltip; include grouped, unread or resolved context when it changes the action)
    /// for `participants` (upstream `participants`: every distinct participant, lead
    /// first).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        participants: impl IntoIterator<Item = CommentPinParticipant>,
    ) -> Self {
        CommentPin {
            id: id.into(),
            label: label.into(),
            participants: participants.into_iter().collect(),
            size: CommentPinSize::Sm,
            thread_count: 1.,
            unread_count: 0.,
            selected: false,
            dimmed: false,
            resolved: false,
            disabled: false,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `size` (`Sm` by default).
    pub fn size(mut self, size: CommentPinSize) -> Self {
        self.size = size;
        self
    }

    /// Upstream `threadCount`: the threads this pin opens (one by default); above one
    /// it is grouped and shows the total. Invalid values (not a finite integer of at
    /// least one) fall back to one and warn in development.
    pub fn thread_count(mut self, count: impl Into<f64>) -> Self {
        self.thread_count = count.into();
        self
    }

    /// Upstream `unreadCount`: threads with unread activity (none by default); above
    /// zero an accent dot shows. Invalid values are ignored and values above the total
    /// capped, with a development warning.
    pub fn unread_count(mut self, count: impl Into<f64>) -> Self {
        self.unread_count = count.into();
        self
    }

    /// Upstream `isSelected`: the current annotation target (`aria-pressed`).
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Upstream `isDimmed`: another pin is selected; this one dims but stays available.
    pub fn dimmed(mut self, dimmed: bool) -> Self {
        self.dimmed = dimmed;
        self
    }

    /// Upstream `isResolved`: every thread is resolved; renders nothing unless selected.
    pub fn resolved(mut self, resolved: bool) -> Self {
        self.resolved = resolved;
        self
    }

    /// Upstream `isDisabled`: keeps its tab stop and is announced unavailable, but
    /// never activates.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Upstream `onClick` (a forwarded button prop): pointer and keyboard activation.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Styled for CommentPin {
    /// Upstream `xstyle`, `className` and `style`: the marker button (a host places it
    /// here).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The fill with the hover or pressed overlay composited over it.
fn over(fill: Hsla, overlay: Hsla) -> Hsla {
    rustryx_theme::color::composite(fill, overlay)
}

impl RenderOnce for CommentPin {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let key = self.id.to_string();
        for message in comment_pin_warnings(
            &self.participants,
            self.thread_count,
            self.unread_count,
            &self.label,
        ) {
            dev_warn_once(&format!("{key}:{message}"), || {
                format!("CommentPin: {message}")
            });
        }
        // Resolved discussion leaves the content; a selected pin keeps its anchor.
        if self.resolved && !self.selected {
            return div().hidden().into_any_element();
        }
        let theme = rustryx_theme::current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let display = comment_pin_display(&self.participants, self.thread_count, self.unread_count);
        let size = self.size;
        let height = match size {
            CommentPinSize::Sm => t.size.element_sm,
            CommentPinSize::Md => t.size.element_md,
            CommentPinSize::Lg => t.size.element_lg,
        }
        .to_pixels(rem);
        let border = t.border.width.to_pixels(rem);
        let circle = px(size.avatar_px());
        // One inset on every side: the one that centers a circle in the height.
        let inset: Pixels = (height - circle) / 2. - border;
        let round = height / 2.;
        let anchor = px(2.);

        let focus = control_focus(&self.id, window, cx);
        let feedback = use_press_feedback(self.id.clone(), window, cx)
            .disabled(self.disabled)
            .space_activates(&focus);
        let fill = t.color.background_popover;
        let fill = match feedback.overlay(t.color.overlay_pressed, window, cx) {
            Some(pressed) if !self.disabled => over(fill, pressed),
            _ => fill,
        };

        let mut el = div()
            .relative()
            // Upstream's `inline-flex`: in a block parent it flows with the text.
            .inline_flex()
            .flex_row_logical(dir)
            .flex_none()
            .items_center()
            .justify_center()
            .min_w(height)
            .h(height)
            .p(inset)
            .border(t.border.width)
            .border_color(gpui::transparent_black())
            .bg(fill)
            .text_color(t.color.text_secondary)
            .font_stack(&t.font.family.body)
            .text_size(t.text.supporting.size)
            .font_weight(t.font.weight.semibold)
            .line_height(relative(t.text.supporting.leading))
            .whitespace_nowrap()
            .cursor_pointer();
        // The speech bubble: three half-height corners and the 2 px bottom-start one,
        // mirrored in RTL.
        el = el.rounded_tl(round).rounded_tr(round);
        el = if dir.is_rtl() {
            el.rounded_bl(round).rounded_br(anchor)
        } else {
            el.rounded_br(round).rounded_bl(anchor)
        };
        let mut shadow = t.shadow.med.layers();
        if self.selected {
            // One solid 2 px accent ring outside the pin over its resting elevation; the
            // border stays transparent, so nothing shifts.
            shadow.insert(
                0,
                BoxShadow {
                    color: t.color.accent,
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(2.),
                    inset: false,
                },
            );
        }
        el.style().box_shadow = Some(shadow);
        if display.grouped || display.unread() {
            el = el.justify_start().gap(t.spacing.s1);
        }
        // Dimmed, then selected (full prominence), then disabled.
        let opacity = if self.disabled {
            0.45
        } else if self.selected || !self.dimmed {
            1.
        } else {
            0.4
        };
        el = el.opacity(opacity);
        if self.disabled {
            el = el.cursor_default();
        }
        let hover = over(t.color.background_popover, t.color.overlay_hover);
        let pressed = over(t.color.background_popover, t.color.overlay_pressed);
        theme.refine(
            el.style(),
            "comment-pin",
            &[
                ("size", size.as_str()),
                ("grouped", if display.grouped { "grouped" } else { "" }),
                ("unread", if display.unread() { "unread" } else { "" }),
                ("selected", if self.selected { "selected" } else { "" }),
                ("dimmed", if self.dimmed { "dimmed" } else { "" }),
                ("resolved", if self.resolved { "resolved" } else { "" }),
                ("disabled", if self.disabled { "disabled" } else { "" }),
            ],
            None,
        );
        el.style().refine(&self.style);

        // The participants: decorative (inert keeps them out of the accessibility tree and
        // lets presses reach the button).
        let avatar = |p: &CommentPinParticipant, i: usize| {
            let mut a = Avatar::new(ElementId::Name(format!("{key}-avatar-{i}").into()))
                .name(p.name.clone())
                .size(AvatarSize::Px(size.avatar_px()));
            if let Some(src) = &p.avatar_src {
                a = a.src(src.clone());
            }
            a
        };
        let rail = if display.grouped {
            let mut group = AvatarGroup::new(ElementId::Name(format!("{key}-group").into()))
                .size(AvatarSize::Px(size.avatar_px()))
                .children(display.shown.iter().enumerate().map(|(i, p)| avatar(p, i)));
            if display.overflow > 0 {
                group = group.child(
                    AvatarGroupOverflow::new(display.overflow)
                        .px_0()
                        .child(Icon::new("moreHorizontal").size(size.overflow_icon())),
                );
            }
            // The first ring sits in the start padding.
            div()
                .margin_start(dir, px(-2.))
                .child(group)
                .into_any_element()
        } else {
            div()
                .children(display.shown.iter().enumerate().map(|(i, p)| avatar(p, i)))
                .into_any_element()
        };
        let rail = div()
            .id(ElementId::Name(format!("{key}-rail").into()))
            .inert(true)
            .flex()
            .flex_none()
            .items_center()
            .debug_selector({
                let name = format!("comment-pin-rail-{key}");
                move || name
            })
            .child(rail);
        let trailing = (display.grouped || display.unread()).then(|| {
            div()
                .id(ElementId::Name(format!("{key}-trailing").into()))
                .inert(true)
                .flex()
                .flex_row_logical(dir)
                .items_center()
                .gap(t.spacing.s1)
                .padding_end(dir, t.spacing.s1)
                .children(display.grouped.then(|| {
                    div()
                        .font_features(FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])))
                        .debug_selector({
                            let name = format!("comment-pin-count-{key}");
                            move || name
                        })
                        .child(js_count_text(display.thread_count))
                }))
                .children(display.unread().then(|| {
                    div()
                        .debug_selector({
                            let name = format!("comment-pin-unread-{key}");
                            move || name
                        })
                        .child(StatusDot::new(
                            ElementId::Name(format!("{key}-unread").into()),
                            StatusDotVariant::Accent,
                            "",
                        ))
                }))
        });

        let on_click = self.on_click.clone();
        let disabled = self.disabled;
        let mut el = el
            .id(self.id.clone())
            .debug_selector({
                let name = format!("comment-pin-{key}");
                move || name
            })
            .track_focus(&focus)
            // The hover and pressed overlays, never on a disabled pin.
            .when(!disabled, |el| {
                el.hover(move |s| s.bg(hover))
                    .active(move |s| s.bg(pressed))
            })
            .role(Role::Button)
            .aria_label(self.label.clone())
            .aria_toggled(if self.selected {
                Toggled::True
            } else {
                Toggled::False
            })
            .aria_disabled(disabled)
            // A disabled pin blocks Enter and Space itself; other keys reach the caller.
            .on_key_down(move |event, window, cx| {
                let key = event.keystroke.key.as_str();
                if disabled && (key == "enter" || key == "space") {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            })
            .on_click(move |event, window, cx| {
                // Never the caller's handler, nor a clickable ancestor's, while disabled.
                cx.stop_propagation();
                if disabled {
                    return;
                }
                if let Some(f) = &on_click {
                    f(event, window, cx);
                }
            })
            .child(rail)
            .children(trailing);
        if is_focus_visible(&focus, window) {
            let grow = t.focus.outline_width.to_pixels(rem) + t.focus.outline_offset.to_pixels(rem);
            let ring = focus_ring(t, round, rem);
            let ring = if dir.is_rtl() {
                ring.rounded_br(anchor + grow)
            } else {
                ring.rounded_bl(anchor + grow)
            };
            el = el.child(ring);
        }
        let el = feedback.surface(el);
        let has_tooltip = !self.disabled && !js_blank(&self.label);
        Tooltip::new(ElementId::Name(format!("{key}-tooltip").into()), self.label)
            .enabled(has_tooltip)
            // Activation stays the primary touch behavior (AR4).
            .touch_trigger(TouchTrigger::None)
            .child(el)
            .into_any_element()
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `CommentPin`.
pub const COMMENT_PIN_DOC: ComponentDoc = ComponentDoc {
    name: "CommentPin",
    rust_type: "rustryx::lab::comment_pin::CommentPin",
    props: &[
        p("label", RustApi::Argument("label")),
        p(
            "participants",
            RustApi::Argument("participants (CommentPinParticipant)"),
        ),
        p("size", RustApi::Method("size")),
        p("threadCount", RustApi::Method("thread_count")),
        p("unreadCount", RustApi::Method("unread_count")),
        p("isSelected", RustApi::Method("selected")),
        p("isDimmed", RustApi::Method("dimmed")),
        p("isResolved", RustApi::Method("resolved")),
        p("isDisabled", RustApi::Method("disabled")),
        p("onClick", RustApi::Method("on_click")),
        p(
            "ref",
            RustApi::Waived("GPUI elements have no refs; focus and place the pin by its id"),
        ),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
