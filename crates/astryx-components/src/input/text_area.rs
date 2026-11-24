//! TextArea. Upstream: `TextArea/`.
//!
//! Controlled by `value`, like TextInput. It shows `rows` lines and scrolls
//! past them; a grip at its bottom end resizes it vertically, as the web's
//! `resize: vertical` does.

use std::cell::Cell;
use std::rc::Rc;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::editing::{
    EditorStyle, FieldSettings, FocusFn, InsertFilter, PasteFn, TextEditState, text_editor,
};
use astryx_primitives::{
    ElementSize, Politeness, announce, is_focus_visible, prefers_reduced_motion, provide,
    resolve_size,
};
use astryx_theme::{ThemeStyled, current, scope};
use gpui::{
    AnyElement, App, Bounds, CursorStyle, DispatchPhase, ElementId, Focusable, Length, MouseButton,
    MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, Role, SharedString, StyleRefinement, Styled,
    Task, Window, canvas, div, point, prelude::*, px,
};
use unicode_segmentation::UnicodeSegmentation;

use super::control::{Chrome, control_box, described_by, status_icon};
use super::text_input::selection_color;
use crate::choice::action::{ActionFn, ChangeFn, Optimistic};
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::icon::{Icon, IconDefaultSize, IconSize};
use crate::status::{Spinner, SpinnerSize};

/// Upstream `COUNTER_WARNING_THRESHOLD`: the last 20% of the limit.
const NEAR_LIMIT: f64 = 0.8;
/// The resize grip's square.
const GRIP: f32 = 12.;

/// Where the length is against `maxLength` (FR6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    Under,
    Near,
    Over,
}

fn zone(length: usize, limit: usize) -> Zone {
    if length > limit {
        Zone::Over
    } else if length as f64 >= limit as f64 * NEAR_LIMIT {
        Zone::Near
    } else {
        Zone::Under
    }
}

/// User-perceived characters: an emoji of several code points counts once.
fn characters(text: &str) -> usize {
    text.graphemes(true).count()
}

#[cfg(test)]
pub(crate) fn characters_for_test(text: &str) -> usize {
    characters(text)
}

/// The field's own state beside its editing state.
#[derive(Default)]
struct AreaState {
    /// The zone of the last edit; unset until the first (FR6).
    zone: Option<Zone>,
    /// The text box's height once the user resized it.
    height: Option<Pixels>,
    /// While resizing: the pointer's y and the height when it began.
    resizing: Option<(Pixels, Pixels)>,
}

/// A multi-line text field, upstream `TextArea`.
#[derive(IntoElement)]
pub struct TextArea {
    id: ElementId,
    label: SharedString,
    value: SharedString,
    on_change: Option<ChangeFn<SharedString>>,
    change_action: Option<ActionFn<SharedString>>,
    label_hidden: bool,
    description: Option<SharedString>,
    optional: bool,
    required: bool,
    disabled: bool,
    read_only: bool,
    disabled_message: Option<SharedString>,
    loading: bool,
    placeholder: Option<SharedString>,
    rows: usize,
    max_length: Option<usize>,
    html_max_length: Option<usize>,
    status: Option<InputStatus>,
    status_placement: StatusPlacement,
    label_tooltip: Option<SharedString>,
    start_icon: Option<AnyElement>,
    auto_focus: bool,
    size: Option<ElementSize>,
    on_paste: Option<PasteFn>,
    on_focus: Option<FocusFn>,
    on_blur: Option<FocusFn>,
    width: Option<Length>,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl TextArea {
    /// Upstream `label` (always rendered for assistive technology) and the
    /// controlled `value`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        TextArea {
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
            read_only: false,
            disabled_message: None,
            loading: false,
            placeholder: None,
            rows: 3,
            max_length: None,
            html_max_length: None,
            status: None,
            status_placement: StatusPlacement::Attached,
            label_tooltip: None,
            start_icon: None,
            auto_focus: false,
            size: None,
            on_paste: None,
            on_focus: None,
            on_blur: None,
            width: None,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `isLabelHidden`: hidden visually, still the field's name.
        !label_hidden,
        /// Upstream `description`: between the label and the field.
        ?description: SharedString,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isDisabled`: dimmed and read-only, but still a tab stop.
        !disabled,
        /// Upstream `isReadOnly`: focusable and selectable, never changed.
        !read_only,
        /// Upstream `disabledMessage`: why the field is disabled. It
        /// describes the field now; its tooltip lands with Tooltip (M5).
        ?disabled_message: SharedString,
        /// Upstream `isLoading`: a spinner, and the field reports busy.
        !loading,
        /// Upstream `placeholder`.
        ?placeholder: SharedString,
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `statusVariant` (`tooltip` lands with Tooltip, M5).
        status_placement: StatusPlacement,
        /// Upstream `labelTooltip`. The InfoTip lands with Tooltip (M5).
        ?label_tooltip: SharedString,
        /// Upstream `hasAutoFocus`: focuses the field when it first renders.
        !auto_focus,
        /// Upstream `size`: `lg` adds block padding; height follows `rows`.
        ?size: ElementSize,
        /// Upstream `width`: the whole field.
        ?width: Length,
    }

    /// Upstream `rows`: the lines shown (default 3).
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows.max(1);
        self
    }

    /// Upstream `maxLength`: a counter of characters against the limit. It
    /// warns past the limit and never blocks typing.
    pub fn max_length(mut self, limit: usize) -> Self {
        self.max_length = Some(limit);
        self
    }

    /// Upstream `htmlMaxLength`: a hard limit in UTF-16 code units, as the
    /// native attribute: typing and pasting stop there.
    pub fn html_max_length(mut self, limit: usize) -> Self {
        self.html_max_length = Some(limit);
        self
    }

    /// Upstream `onChange`: the new text after each edit.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`. The field shows
    /// the new text and is busy until the task settles, then shows `value`.
    pub fn change_action(
        mut self,
        f: impl Fn(SharedString, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `onPaste`: the pasted text, before it lands.
    pub fn on_paste(mut self, f: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_paste = Some(Rc::new(f));
        self
    }

    /// Upstream `onFocus`.
    pub fn on_focus(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_focus = Some(Rc::new(f));
        self
    }

    /// Upstream `onBlur`.
    pub fn on_blur(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_blur = Some(Rc::new(f));
        self
    }

    /// Upstream `startIcon`: drawn small and secondary at the top start.
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `controlXstyle`: styles the bordered box around the text.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl Styled for TextArea {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The hard limit's filter: inserted text is cut so the whole stays within
/// `limit` UTF-16 code units, never splitting a character.
pub(crate) fn utf16_limit(limit: usize) -> InsertFilter {
    Rc::new(
        move |text: &str, replaced: std::ops::Range<usize>, inserted: &str| {
            let kept: usize = text[..replaced.start].encode_utf16().count()
                + text[replaced.end..].encode_utf16().count();
            let mut room = limit.saturating_sub(kept);
            let mut end = 0;
            for (i, ch) in inserted.char_indices() {
                let units = ch.len_utf16();
                if units > room {
                    break;
                }
                room -= units;
                end = i + ch.len_utf8();
            }
            inserted[..end].to_string()
        },
    )
}

/// Two short diagonal strokes in the bottom end corner, as browsers draw a
/// textarea's resize grip.
fn paint_grip(bounds: Bounds<Pixels>, color: gpui::Hsla, rtl: bool, window: &mut Window) {
    let corner = |dx: f32, dy: f32| {
        let x = if rtl {
            bounds.origin.x + px(dx)
        } else {
            bounds.origin.x + bounds.size.width - px(dx)
        };
        point(x, bounds.origin.y + bounds.size.height - px(dy))
    };
    for reach in [9., 5.] {
        let mut stroke = PathBuilder::stroke(px(1.));
        stroke.move_to(corner(3., reach + 3.));
        stroke.line_to(corner(reach + 3., 3.));
        if let Ok(path) = stroke.build() {
            window.paint_path(path, color);
        }
    }
}

impl RenderOnce for TextArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let i18n = i18n(cx);
        let dir = use_direction(cx);
        let rem = window.rem_size();

        let area = window.use_keyed_state(
            ElementId::Name(format!("{:?}-area", self.id).into()),
            cx,
            |_, _| AreaState::default(),
        );

        // FR4: onChange, then the action with the proposed text shown.
        let optimistic = Optimistic::<SharedString>::new(&self.id, window, cx);
        let shown = optimistic.shown(self.value.clone(), cx);
        let busy = self.loading || shown != self.value;
        optimistic.announce_busy(busy, i18n.t("@astryx.spinner.loading"), window, cx);
        let change: ChangeFn<SharedString> =
            Rc::new(optimistic.change(self.on_change, self.change_action));
        // FR6: each edit that lands in another zone of the limit announces
        // the characters left (near) or over (assertively).
        let change: ChangeFn<SharedString> = match self.max_length {
            None => change,
            Some(limit) => {
                let area = area.clone();
                Rc::new(
                    move |text: SharedString, window: &mut Window, cx: &mut App| {
                        let length = characters(&text);
                        let now = zone(length, limit);
                        let before = area.update(cx, |a, _| a.zone.replace(now));
                        if before != Some(now) {
                            let i18n = astryx_i18n::current(cx);
                            match now {
                                Zone::Over => announce(
                                    i18n.format(
                                        "@astryx.textArea.charactersOverLimit",
                                        &[("count", ((length - limit) as f64).into())],
                                    ),
                                    Politeness::Assertive,
                                    window,
                                    cx,
                                ),
                                Zone::Near => announce(
                                    i18n.format(
                                        "@astryx.textArea.charactersRemaining",
                                        &[("count", ((limit - length) as f64).into())],
                                    ),
                                    Politeness::Polite,
                                    window,
                                    cx,
                                ),
                                Zone::Under => {}
                            }
                        }
                        change(text, window, cx);
                    },
                )
            }
        };

        let editable = !self.disabled && !self.read_only;
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-editor", self.id).into()),
            cx,
            |window, cx| TextEditState::reporting(shown.to_string(), true, window, cx),
        );
        state.update(cx, |s, cx| {
            s.set_text(&shown, cx);
            s.set_settings(FieldSettings {
                disabled: self.disabled,
                read_only: self.read_only,
                masked: false,
                filter: self.html_max_length.map(utf16_limit),
            });
            s.on_change = editable.then(|| {
                let change = change.clone();
                Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                    change(text.into(), window, cx)
                }) as astryx_primitives::editing::ChangeFn
            });
            // FR9: a disabled field runs no caller handlers but focus and blur.
            s.on_paste = self.on_paste.clone().filter(|_| editable);
            s.on_focus = self.on_focus.clone();
            s.on_blur = self.on_blur.clone();
        });
        let focus = state.read(cx).focus_handle(cx);
        if self.auto_focus {
            let done = window.use_keyed_state(
                ElementId::Name(format!("{:?}-autofocused", self.id).into()),
                cx,
                |_, _| false,
            );
            if !*done.read(cx) {
                done.update(cx, |done, _| *done = true);
                let focus = focus.clone();
                window.defer(cx, move |window, cx| window.focus(&focus, cx));
            }
        }

        let status = self.status.as_ref().map(|s| s.status);
        let chrome = Chrome {
            id: ElementId::Name(format!("{:?}-control", self.id).into()),
            target: "text-area",
            size,
            status,
            disabled: self.disabled,
            read_only: self.read_only,
            focused: focus.is_focused(window),
            focus_visible: is_focus_visible(&focus, window),
            row: false,
        };
        let part_override = theme.for_part("text-area", &chrome.props());
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let mut control = control_box(&chrome, &part, window, cx).p_0().block();
        control.style().refine(&self.control_style);
        // FR11: a click on the box outside the text focuses it, unless the
        // field is disabled or busy.
        if !self.disabled && !busy {
            let focus = focus.clone();
            control = control.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            });
        }

        let length = characters(&shown);
        let over = self.max_length.is_some_and(|limit| length > limit);
        let status_glyph = status
            .filter(|_| self.status_placement == StatusPlacement::Attached)
            .map(|status| status_icon(status, &part));

        // FR7: the text keeps clear of the icons, spinner and counter.
        let inline = t.spacing.s2.to_pixels(rem);
        let slot = t.spacing.s6.to_pixels(rem);
        let block = if size == ElementSize::Lg {
            t.spacing.s2
        } else {
            t.spacing.s1
        };
        let end_padding = match (busy, status_glyph.is_some()) {
            (true, true) => inline + t.spacing.s12.to_pixels(rem),
            (false, false) => inline,
            _ => inline + slot,
        };
        let start_padding = if self.start_icon.is_some() {
            inline + slot
        } else {
            inline
        };
        let line = t.text.body.size.to_pixels(rem) * t.text.body.leading;
        let user_height = area.read(cx).height;

        let mut described = described_by(
            self.description.as_ref(),
            self.status.as_ref(),
            self.disabled
                .then_some(self.disabled_message.as_ref())
                .flatten(),
            &i18n,
        );
        let counter_text = self.max_length.map(|limit| {
            i18n.format(
                "@astryx.textArea.counter",
                &[
                    ("count", (length as f64).into()),
                    ("limit", (limit as f64).into()),
                ],
            )
        });
        if let Some(counter) = &counter_text {
            described.push(counter.clone());
        }

        let style = EditorStyle {
            text_color: t.color.text_primary,
            placeholder: self.placeholder.clone(),
            placeholder_color: t.color.text_secondary,
            selection_color: selection_color(t),
            caret_color: t.color.text_primary,
            multiline: true,
            min_lines: self.rows,
            max_lines: Some(self.rows),
            height: user_height,
            reduced_motion: prefers_reduced_motion(cx),
        };
        let mut editor = text_editor("editor", &state, style, cx)
            .py(block)
            .padding_start(dir, start_padding)
            .padding_end(dir, end_padding)
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .role(Role::MultilineTextInput)
            .aria_label(self.label.clone())
            .aria_value(shown.clone())
            .aria_disabled(self.disabled)
            .aria_read_only(!editable)
            .aria_required(self.required)
            .aria_invalid(status == Some(StatusType::Error) || over)
            .aria_busy(busy);
        if self.max_length.is_some() {
            editor = editor.pb(t.spacing.s7);
        }
        if let Some(placeholder) = &self.placeholder {
            editor = editor.aria_placeholder(placeholder.clone());
        }
        if !described.is_empty() {
            editor = editor.aria_description(SharedString::from(described.join(" ")));
        }
        control = control.child(editor);

        if let Some(icon) = self.start_icon {
            control = control.child(
                div()
                    .absolute()
                    .top(t.spacing.s2)
                    .inset_start(dir, inline)
                    .flex()
                    .text_color(t.color.icon_secondary)
                    .child(provide(IconDefaultSize(IconSize::Sm), icon)),
            );
        }
        if busy || status_glyph.is_some() {
            control = control.child(
                div()
                    .absolute()
                    .top(t.spacing.s2)
                    .inset_end(dir, inline)
                    .flex()
                    .flex_row_logical(dir)
                    .items_center()
                    .gap(t.spacing.s2)
                    .when(busy, |el| {
                        el.child(Spinner::new("spinner").size(SpinnerSize::Sm).decorative())
                    })
                    .children(status_glyph),
            );
        }
        if let Some(text) = counter_text {
            let mut counter = div()
                .absolute()
                .bottom(t.spacing.s1)
                .inset_end(dir, inline + px(GRIP))
                .flex()
                .flex_row_logical(dir)
                .items_center()
                .gap(t.spacing.s1)
                .font_stack(&t.font.family.body)
                .text_style(&t.text.supporting)
                .text_color(if over {
                    t.color.text_error
                } else {
                    t.color.text_secondary
                })
                .when(over, |el| el.child(Icon::new("warning").size(IconSize::Sm)))
                .child(text);
            part.refine(counter.style(), "text-area-counter", &[], None);
            control = control.child(counter);
        }

        // The resize grip: dragging it sets the text box's height, at least
        // one line.
        let grip_color = t.color.icon_secondary;
        let rtl = dir.is_rtl();
        let min_height = line;
        let resizing = Rc::new(Cell::new(area.read(cx).resizing));
        control = control.child(
            div()
                .id("grip")
                .absolute()
                .bottom_0()
                .inset_end(dir, px(0.))
                .size(px(GRIP))
                .cursor(CursorStyle::ResizeUpDown)
                .on_mouse_down(MouseButton::Left, {
                    let (area, state) = (area.clone(), state.clone());
                    move |event, window, cx| {
                        cx.stop_propagation();
                        let height = state
                            .read(cx)
                            .bounds()
                            .map_or(line * 3., |bounds| bounds.size.height);
                        area.update(cx, |a, _| a.resizing = Some((event.position.y, height)));
                        window.refresh();
                    }
                })
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| paint_grip(bounds, grip_color, rtl, window),
                    )
                    .size_full(),
                ),
        );
        if resizing.get().is_some() {
            let area = area.clone();
            control = control.child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let moved = area.clone();
                        window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble {
                                return;
                            }
                            moved.update(cx, |a, _| {
                                if let Some((from, height)) = a.resizing {
                                    a.height = Some((height + e.position.y - from).max(min_height));
                                }
                            });
                            window.refresh();
                        });
                        let ended = area.clone();
                        window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                                ended.update(cx, |a, _| a.resizing = None);
                                window.refresh();
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            );
        }

        let control = match part_override {
            Some(part) => scope(part, control).into_any_element(),
            None => control.into_any_element(),
        };
        let mut field = Field::new(self.id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(self.disabled)
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
        field.child(control)
    }
}
