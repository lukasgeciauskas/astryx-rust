//! NumberInput. Upstream: `NumberInput/` (with `numberInputCommit.ts`).
//!
//! Typing edits a draft; nothing is reported until the draft commits on
//! blur or Enter, clamped to the range. Arrow keys, the wheel and the
//! steppers step along a grid and commit at once. A draft that does not
//! read as a number (or a fraction when integer-only, or any number when
//! `min` is above `max`) stays in the field, and at commit shows the
//! invalid-number status.

use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::{I18n, LogicalStyled, current as i18n, use_direction};
use astryx_primitives::editing::{EditorStyle, FieldSettings, FocusFn, TextEditState, text_editor};
use astryx_primitives::{
    ElementSize, Politeness, announce, is_focus_visible, prefers_reduced_motion, provide,
    resolve_size,
};
use astryx_theme::color::composite;
use astryx_theme::{ThemeStyled, current, scope};
use gpui::{
    AnyElement, App, ElementId, Entity, Focusable, KeyDownEvent, Length, MouseButton, Role,
    SharedString, StyleRefinement, Styled, Task, Window, div, prelude::*, px,
};

use super::control::{Chrome, control_box, described_by, status_indicator};
use super::group::{group_context, group_disabled, group_place, inside_member};
use super::text_input::selection_color;
use crate::button::{ButtonVariant, IconButton};
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::icon::{Icon, IconColor, IconDefaultSize, IconSize};

/// Upstream `STEPPER_HOLD`: a held stepper repeats after half a second,
/// then faster.
const HOLD_DELAY: Duration = Duration::from_millis(500);
const HOLD_FIRST_INTERVAL: f64 = 250.;
const HOLD_ACCELERATION: f64 = 0.9;
const HOLD_MIN_INTERVAL: f64 = 50.;

/// Upstream `NumberInputEntryError` `kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryErrorKind {
    /// The text is not a number.
    Unreadable,
    /// A fraction in an integer-only field.
    Fractional,
    /// Any number while `min` is above `max`.
    OutOfRange,
}

/// Upstream `NumberInputEntryError`: why the typed text was not committed.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberInputEntryError {
    pub kind: EntryErrorKind,
    /// The text as typed.
    pub text: SharedString,
    /// Its number, for `Fractional` and `OutOfRange`.
    pub value: Option<f64>,
}

/// The range and kind of number a field accepts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rules {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub integer_only: bool,
}

impl Rules {
    /// Upstream `resolveCommitRange`: an integer-only range rounds inward.
    fn commit_range(self) -> (Option<f64>, Option<f64>) {
        let round = |bound: Option<f64>, f: fn(f64) -> f64| {
            bound.map(|b| if self.integer_only { f(b) } else { b })
        };
        (round(self.min, f64::ceil), round(self.max, f64::floor))
    }

    fn inverted(self) -> bool {
        matches!(self.commit_range(), (Some(min), Some(max)) if min > max)
    }
}

/// Upstream `parseNumericInput`: a number of the right kind, or `None`.
fn parse_kind(text: &str, rules: Rules, i18n: &I18n) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value = i18n.parse_number(trimmed)?;
    (!rules.integer_only || value.fract() == 0.).then_some(value)
}

/// Upstream `parseNumberInput`: a number of the right kind inside the range.
pub(crate) fn parse_in_range(text: &str, rules: Rules, i18n: &I18n) -> Option<f64> {
    let value = parse_kind(text, rules, i18n)?;
    let below = rules.min.is_some_and(|min| value < min);
    let above = rules.max.is_some_and(|max| value > max);
    (!below && !above).then_some(value)
}

/// Upstream `getNumberInputRejection`: why typed text cannot commit.
pub(crate) fn rejection(text: &str, rules: Rules, i18n: &I18n) -> Option<NumberInputEntryError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let error = |kind, value| {
        Some(NumberInputEntryError {
            kind,
            text: SharedString::from(text.to_string()),
            value,
        })
    };
    let Some(value) = i18n.parse_number(trimmed) else {
        return error(EntryErrorKind::Unreadable, None);
    };
    if rules.integer_only && value.fract() != 0. {
        return error(EntryErrorKind::Fractional, Some(value));
    }
    if rules.inverted() {
        return error(EntryErrorKind::OutOfRange, Some(value));
    }
    None
}

/// Upstream `NumberInputCommitDecision`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Decision {
    Commit { value: f64, clamped: bool },
    Clear,
    Revert,
}

/// Upstream `resolveNumberInputCommit`.
pub(crate) fn resolve_commit(text: &str, rules: Rules, clear: bool, i18n: &I18n) -> Decision {
    if text.trim().is_empty() {
        return if clear {
            Decision::Clear
        } else {
            Decision::Revert
        };
    }
    let Some(value) = parse_kind(text, rules, i18n) else {
        return Decision::Revert;
    };
    if rules.inverted() {
        return Decision::Revert;
    }
    let (min, max) = rules.commit_range();
    let committed = match (min, max) {
        (Some(min), _) if value < min => min,
        (_, Some(max)) if value > max => max,
        _ => value,
    };
    Decision::Commit {
        value: committed,
        clamped: committed != value,
    }
}

/// Upstream `getDecimalPlaces`: digits after the point, exponents included.
fn decimal_places(value: f64) -> i32 {
    let text = format!("{value}");
    text.split_once('.')
        .map_or(0, |(_, frac)| frac.len() as i32)
}

/// Upstream `getEffectiveStep`.
fn effective_step(step: Option<f64>, integer_only: bool) -> f64 {
    match step {
        Some(step) if step.is_finite() && step > 0. && (!integer_only || step.fract() == 0.) => {
            step
        }
        _ => 1.,
    }
}

/// Upstream `getSteppedValue`: one grid position up (`1`) or down (`-1`)
/// from `current`, the grid starting at `min` (or 0), rounded to the step's
/// precision and clamped. Returns `current` when no step fits.
pub(crate) fn stepped(
    current: Option<f64>,
    direction: i8,
    rules: Rules,
    step: Option<f64>,
) -> Option<f64> {
    let step = effective_step(step, rules.integer_only);
    let base = match rules.min {
        Some(min) if !rules.integer_only || min.fract() == 0. => min,
        _ => 0.,
    };
    let mut next = match current {
        None => {
            let start = if direction > 0 {
                rules.min.unwrap_or(0.)
            } else {
                rules.max.unwrap_or(0.)
            };
            match (rules.integer_only, direction > 0) {
                (true, true) => start.ceil(),
                (true, false) => start.floor(),
                _ => start,
            }
        }
        Some(current) => {
            let position = (current - base) / step;
            let tolerance = f64::EPSILON * position.abs().max(1.) * 4.;
            let next_position = if direction > 0 {
                (position + tolerance).floor() + 1.
            } else {
                (position - tolerance).ceil() - 1.
            };
            base + next_position * step
        }
    };
    let precision = decimal_places(step).max(decimal_places(base)).min(12);
    next = format!("{next:.prec$}", prec = precision as usize)
        .parse()
        .unwrap_or(next);
    if let Some(min) = rules.min {
        next = next.max(min);
    }
    if let Some(max) = rules.max {
        next = next.min(max);
    }
    if !next.is_finite() || (rules.integer_only && next.fract() != 0.) {
        return current;
    }
    Some(if next == 0. { 0. } else { next })
}

/// A held stepper.
struct Hold {
    direction: i8,
    held: Option<f64>,
    /// Stops the field committing when it turned disabled or read-only.
    frozen: bool,
    _task: Option<Task<()>>,
}

/// The field's state between frames.
#[derive(Default)]
struct NumberState {
    /// Upstream `pendingInput`: the text being typed.
    draft: Option<String>,
    /// The value a held stepper shows.
    hold_draft: Option<f64>,
    hold: Option<Hold>,
    /// The rejection shown since the last commit.
    shown: Option<NumberInputEntryError>,
    /// Blur kept a rejected draft.
    kept_at_blur: bool,
    /// The value and rules of the last frame, to notice new ones.
    last_value: Option<Option<f64>>,
    judged: Option<Rules>,
    /// The last error told to `on_entry_error_change`.
    reported: Option<NumberInputEntryError>,
}

type ValueFn = Rc<dyn Fn(Option<f64>, &mut Window, &mut App)>;
type EntryErrorFn = Rc<dyn Fn(Option<NumberInputEntryError>, &mut Window, &mut App)>;
type FormatFn = Rc<dyn Fn(f64) -> String>;
type KeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;

/// A numeric field, upstream `NumberInput`.
#[derive(IntoElement)]
pub struct NumberInput {
    id: ElementId,
    label: SharedString,
    value: Option<f64>,
    on_change: Option<ValueFn>,
    /// Each edit's committable number, or none (PowerSearch's sheet stages it).
    on_input: Option<ValueFn>,
    size: Option<ElementSize>,
    label_hidden: bool,
    description: Option<SharedString>,
    optional: bool,
    required: bool,
    on_key_down: Option<KeyFn>,
    disabled: bool,
    read_only: bool,
    disabled_message: Option<SharedString>,
    placeholder: Option<SharedString>,
    label_tooltip: Option<SharedString>,
    start_icon: Option<AnyElement>,
    label_icon: Option<AnyElement>,
    status: Option<InputStatus>,
    on_entry_error_change: Option<EntryErrorFn>,
    status_placement: StatusPlacement,
    min: Option<f64>,
    max: Option<f64>,
    step: Option<f64>,
    format_value: Option<FormatFn>,
    wheel_enabled: bool,
    number_steppers: bool,
    units: Option<SharedString>,
    integer_only: bool,
    clear: bool,
    width: Option<Length>,
    auto_focus: bool,
    on_focus: Option<FocusFn>,
    on_blur: Option<FocusFn>,
    on_enter: Option<FocusFn>,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl NumberInput {
    /// Upstream `label` (always rendered for assistive technology) and the
    /// controlled `value` (`None` when empty).
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<Option<f64>>,
    ) -> Self {
        NumberInput {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            on_change: None,
            on_input: None,
            size: None,
            label_hidden: false,
            description: None,
            optional: false,
            required: false,
            on_key_down: None,
            disabled: false,
            read_only: false,
            disabled_message: None,
            placeholder: None,
            label_tooltip: None,
            start_icon: None,
            label_icon: None,
            status: None,
            on_entry_error_change: None,
            status_placement: StatusPlacement::Attached,
            min: None,
            max: None,
            step: None,
            format_value: None,
            wheel_enabled: true,
            number_steppers: false,
            units: None,
            integer_only: false,
            clear: false,
            width: None,
            auto_focus: false,
            on_focus: None,
            on_blur: None,
            on_enter: None,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `size`: explicit, else the nearest size provider, else `md`.
        ?size: ElementSize,
        /// Upstream `isLabelHidden`: hidden visually, still the field's name.
        !label_hidden,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `isOptional`.
        !optional,
        /// Upstream `isRequired`.
        !required,
        /// Upstream `isDisabled`: dimmed, no drafts or steps, still a tab stop.
        !disabled,
        /// Upstream `isReadOnly`: focusable, never edited or stepped.
        !read_only,
        /// Upstream `disabledMessage`: why the field is disabled, in a
        /// tooltip on hover and focus; it also describes the field.
        ?disabled_message: SharedString,
        /// Upstream `placeholder`.
        ?placeholder: SharedString,
        /// Upstream `labelTooltip`: an InfoTip beside the label.
        ?label_tooltip: SharedString,
        /// Upstream `status`. An `error` with a message shows instead of the
        /// field's own invalid-number message.
        ?status: InputStatus,
        /// Upstream `statusVariant`; `tooltip` puts the message in the status
        /// icon's tooltip.
        status_placement: StatusPlacement,
        /// Upstream `min`.
        ?min: f64,
        /// Upstream `max`.
        ?max: f64,
        /// Upstream `step` (default 1).
        ?step: f64,
        /// Upstream `isWheelEnabled` (default on): the wheel over the focused
        /// field steps it.
        !wheel_enabled,
        /// Upstream `hasNumberSteppers`: increment and decrement buttons.
        !number_steppers,
        /// Upstream `units`: secondary text after the number, such as "%".
        ?units: SharedString,
        /// Upstream `isIntegerOnly`.
        !integer_only,
        /// Upstream `hasClear`: a clear button while there is a value; it
        /// reports `None`.
        !clear,
        /// Upstream `width`: the whole field.
        ?width: Length,
        /// Upstream `hasAutoFocus`.
        !auto_focus,
    }

    /// Upstream `onChange`: the committed number, or `None` when cleared.
    pub fn on_change(mut self, f: impl Fn(Option<f64>, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Every edit, with the number it would commit (none while empty or
    /// not a number): upstream's `onInput` with `resolveNumberInputCommit`.
    pub(crate) fn on_input(
        mut self,
        f: impl Fn(Option<f64>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_input = Some(Rc::new(f));
        self
    }

    /// Upstream `onEntryErrorChange`: the shown rejection whenever it
    /// changes, `None` when it ends.
    pub fn on_entry_error_change(
        mut self,
        f: impl Fn(Option<NumberInputEntryError>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_entry_error_change = Some(Rc::new(f));
        self
    }

    /// Upstream `formatValue`: how the committed number shows at rest. While
    /// focused, the plain number shows.
    pub fn format_value(mut self, f: impl Fn(f64) -> String + 'static) -> Self {
        self.format_value = Some(Rc::new(f));
        self
    }

    /// Upstream `onKeyDown`: every key, before the field handles it.
    pub fn on_key_down(
        mut self,
        f: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_key_down = Some(Rc::new(f));
        self
    }

    /// Upstream `onEnter`: every Enter outside an IME composition, after the
    /// draft commits.
    pub fn on_enter(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_enter = Some(Rc::new(f));
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

    /// Upstream `startIcon`.
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `labelIcon`: an icon before the label text.
    pub fn label_icon(mut self, icon: impl IntoElement) -> Self {
        self.label_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `controlXstyle`: styles the bordered control box.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl Styled for NumberInput {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What the field's handlers share: its state and settings this frame.
#[derive(Clone)]
struct Ctx {
    state: Entity<NumberState>,
    value: Option<f64>,
    rules: Rules,
    step: Option<f64>,
    clear: bool,
    disabled: bool,
    read_only: bool,
    on_change: Option<ValueFn>,
    on_entry_error: Option<EntryErrorFn>,
    /// A caller error with a message shows instead of the field's own.
    caller_error: bool,
    i18n: I18n,
}

impl Ctx {
    fn change(&self, value: Option<f64>, window: &mut Window, cx: &mut App) {
        if let Some(f) = &self.on_change {
            f(value, window, cx);
        }
        window.refresh();
    }

    /// Tells `on_entry_error_change` about a new shown rejection, once.
    fn report(&self, error: Option<NumberInputEntryError>, window: &mut Window, cx: &mut App) {
        let changed = self.state.update(cx, |s, _| {
            let changed = s.reported != error;
            s.reported = error.clone();
            changed
        });
        if changed && let Some(f) = &self.on_entry_error {
            f(error, window, cx);
        }
    }

    /// Upstream `endDraft`.
    fn end_draft(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| {
            s.draft = None;
            s.shown = None;
            s.kept_at_blur = false;
        });
        self.report(None, window, cx);
    }

    /// The number steps start from: a valid draft, else the value; an
    /// emptied draft steps from nothing.
    fn stepping_from(&self, cx: &App) -> Option<f64> {
        match &self.state.read(cx).draft {
            None => self.value,
            Some(draft) if draft.trim().is_empty() => None,
            Some(draft) => parse_in_range(draft, self.rules, &self.i18n).or(self.value),
        }
    }

    fn next(&self, direction: i8, cx: &App) -> Option<f64> {
        stepped(self.stepping_from(cx), direction, self.rules, self.step)
    }

    /// Upstream `stepValue`: steps and commits at once.
    fn step(&self, direction: i8, window: &mut Window, cx: &mut App) {
        if self.disabled || self.read_only {
            return;
        }
        let Some(next) = self.next(direction, cx) else {
            return;
        };
        self.end_draft(window, cx);
        if Some(next) != self.value {
            self.change(Some(next), window, cx);
        }
        window.refresh();
    }

    /// Upstream `commitPendingInput`.
    fn commit(&self, blur: bool, window: &mut Window, cx: &mut App) {
        let Some(draft) = self.state.read(cx).draft.clone() else {
            return;
        };
        if self.disabled {
            self.end_draft(window, cx);
            return;
        }
        if let Some(error) = rejection(&draft, self.rules, &self.i18n) {
            self.state.update(cx, |s, _| {
                s.shown = Some(error.clone());
                if blur {
                    s.kept_at_blur = true;
                }
            });
            self.report(Some(error), window, cx);
            // The field status announces its own message; a caller's error
            // shown in its place leaves the rejection to announce here.
            if self.caller_error {
                announce(
                    self.i18n.t("@astryx.numberInput.invalidNumber"),
                    Politeness::Assertive,
                    window,
                    cx,
                );
            }
            window.refresh();
            return;
        }
        self.state.update(cx, |s, _| {
            s.shown = None;
            s.kept_at_blur = false;
        });
        self.report(None, window, cx);
        let decision = resolve_commit(&draft, self.rules, self.clear, &self.i18n);
        if blur || matches!(decision, Decision::Commit { clamped: true, .. }) {
            self.state.update(cx, |s, _| s.draft = None);
        }
        match decision {
            Decision::Clear if self.clear && self.value.is_some() => self.change(None, window, cx),
            Decision::Commit { value, .. } if Some(value) != self.value => {
                self.change(Some(value), window, cx)
            }
            _ => {}
        }
        window.refresh();
    }

    /// Starts a stepper hold: after a delay the stepper repeats, showing each
    /// value as a draft, until the pointer lifts.
    fn start_hold(&self, direction: i8, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, _| {
            s.hold = None;
            s.hold_draft = None;
        });
        if self.disabled || self.read_only {
            return;
        }
        let Some(first) = self.next(direction, cx) else {
            return;
        };
        let (state, rules, step) = (self.state.clone(), self.rules, self.step);
        let timer = cx.background_executor().clone();
        let task = window.spawn(cx, async move |cx| {
            timer.timer(HOLD_DELAY).await;
            let mut interval = HOLD_FIRST_INTERVAL;
            loop {
                let more = cx
                    .update(|window, cx| {
                        state
                            .update(cx, |s, _| {
                                let Some(hold) = &mut s.hold else {
                                    return false;
                                };
                                let next = match hold.held {
                                    None => Some(first),
                                    Some(held) => stepped(Some(held), direction, rules, step),
                                };
                                match next {
                                    Some(next) if Some(next) != hold.held => {
                                        hold.held = Some(next);
                                        s.hold_draft = Some(next);
                                        true
                                    }
                                    _ => false,
                                }
                            })
                            .then(|| window.refresh())
                            .is_some()
                    })
                    .unwrap_or(false);
                if !more {
                    break;
                }
                timer.timer(Duration::from_millis(interval as u64)).await;
                interval = (interval * HOLD_ACCELERATION).max(HOLD_MIN_INTERVAL);
            }
        });
        self.state.update(cx, |s, _| {
            s.hold = Some(Hold {
                direction,
                held: None,
                frozen: false,
                _task: Some(task),
            })
        });
    }

    /// Ends a stepper hold. A hold that repeated commits its last value once;
    /// one released before repeating, on the button, is a click: one step.
    fn end_hold(&self, direction: i8, released_on_button: bool, window: &mut Window, cx: &mut App) {
        // The other stepper's release-outside sees this hold too; only the
        // stepper that started it ends it.
        let hold = self.state.update(cx, |s, _| {
            if s.hold.as_ref().is_some_and(|h| h.direction == direction) {
                s.hold_draft = None;
                s.hold.take()
            } else {
                None
            }
        });
        let Some(hold) = hold else {
            return;
        };
        match hold.held {
            None if released_on_button => self.step(direction, window, cx),
            None => {}
            Some(held) => {
                if hold.frozen || self.disabled || self.read_only {
                    window.refresh();
                    return;
                }
                self.end_draft(window, cx);
                if Some(held) != self.value {
                    self.change(Some(held), window, cx);
                }
            }
        }
        window.refresh();
    }
}

fn bits(value: Option<f64>) -> Option<u64> {
    value.map(f64::to_bits)
}

impl RenderOnce for NumberInput {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // FR6: an enclosing InputGroup's disabled state, never re-enabling.
        self.disabled |= group_disabled(cx);
        let place = group_place(cx);
        let group = place.and_then(|_| group_context(cx));
        let dir = use_direction(cx);
        let theme = current(window, cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let i18n = i18n(cx);
        let rem = window.rem_size();
        let rules = Rules {
            min: self.min,
            max: self.max,
            integer_only: self.integer_only,
        };
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-number", self.id).into()),
            cx,
            |_, _| NumberState::default(),
        );
        let editable = !self.disabled && !self.read_only;

        // Reconcile the draft with this frame's props: disabling drops it, a
        // new value replaces a rejected or kept one, new rules re-judge it.
        let report = state.update(cx, |s, _| {
            let mut report = None;
            if self.disabled && (s.draft.is_some() || s.shown.is_some() || s.kept_at_blur) {
                s.draft = None;
                s.shown = None;
                s.kept_at_blur = false;
                report = Some(None);
            }
            if !editable && s.hold_draft.is_some() {
                s.hold_draft = None;
            }
            if !editable && let Some(hold) = &mut s.hold {
                hold.frozen = true;
                hold._task = None;
            }
            let new_value = s
                .last_value
                .is_some_and(|last| bits(last) != bits(self.value));
            s.last_value = Some(self.value);
            let mut replaced = false;
            if new_value && (s.shown.is_some() || s.kept_at_blur) {
                s.draft = None;
                s.shown = None;
                s.kept_at_blur = false;
                replaced = true;
                report = Some(None);
            }
            if s.judged != Some(rules) {
                s.judged = Some(rules);
                if !replaced
                    && s.shown.is_some()
                    && let Some(draft) = &s.draft
                {
                    let judged = rejection(draft, rules, &i18n);
                    if judged != s.shown {
                        s.shown = judged.clone();
                        report = Some(judged);
                    }
                }
            }
            report
        });
        let caller_error = matches!(
            &self.status,
            Some(InputStatus {
                status: StatusType::Error,
                message: Some(_),
            })
        );
        let ctx = Ctx {
            state: state.clone(),
            value: self.value,
            rules,
            step: self.step,
            clear: self.clear,
            disabled: self.disabled,
            read_only: self.read_only,
            on_change: self.on_change.clone(),
            on_entry_error: self.on_entry_error_change.clone(),
            caller_error,
            i18n: i18n.clone(),
        };
        if let Some(error) = report {
            let ctx = ctx.clone();
            window.defer(cx, move |window, cx| ctx.report(error, window, cx));
        }

        let (draft, hold_draft, shown) = {
            let s = state.read(cx);
            (s.draft.clone(), s.hold_draft, s.shown.clone())
        };
        // A held stepper hides the rejection while it shows its values.
        let shown_error = if self.disabled || hold_draft.is_some() {
            None
        } else {
            shown
        };
        let field_status = if shown_error.is_some() && !caller_error {
            Some(InputStatus::error(
                i18n.t("@astryx.numberInput.invalidNumber"),
            ))
        } else {
            self.status.clone()
        };

        let editor_state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-editor", self.id).into()),
            cx,
            |window, cx| TextEditState::reporting("", false, window, cx),
        );
        let focus = editor_state.read(cx).focus_handle(cx);
        let focused = focus.is_focused(window);
        let formatted = self
            .value
            .map(|v| match &self.format_value {
                Some(format) => format(v),
                None => format!("{v}"),
            })
            .unwrap_or_default();
        let display = match (hold_draft, &draft, self.value) {
            (Some(held), _, _) => i18n.editable_number(held),
            (None, Some(draft), _) => draft.clone(),
            (None, None, None) => String::new(),
            (None, None, Some(value)) if focused => i18n.editable_number(value),
            (None, None, Some(_)) => formatted.clone(),
        };

        editor_state.update(cx, |s, cx| {
            s.set_text(&display, cx);
            s.set_settings(FieldSettings {
                disabled: self.disabled,
                read_only: self.read_only,
                masked: false,
                filter: None,
                // A number pad: digits only for integers (upstream inputMode).
                spell_check: false,
                keyboard: astryx_primitives::editing::KeyboardHints {
                    input_mode: Some(if rules.integer_only {
                        gpui::TextInputMode::Numeric
                    } else {
                        gpui::TextInputMode::Decimal
                    }),
                    ..Default::default()
                },
            });
            // FR2: typing only edits the draft.
            s.on_change = editable.then(|| {
                let ctx = ctx.clone();
                let on_input = self.on_input.clone();
                Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                    if let Some(f) = &on_input {
                        let value = match resolve_commit(&text, ctx.rules, false, &ctx.i18n) {
                            Decision::Commit { value, .. } => Some(value),
                            _ => None,
                        };
                        f(value, window, cx);
                    }
                    let clears = ctx.state.update(cx, |s, _| {
                        s.kept_at_blur = false;
                        let clears =
                            s.shown.is_some() && rejection(&text, ctx.rules, &ctx.i18n).is_none();
                        if clears {
                            s.shown = None;
                        }
                        s.draft = Some(text);
                        clears
                    });
                    if clears {
                        ctx.report(None, window, cx);
                    }
                    window.refresh();
                }) as astryx_primitives::editing::ChangeFn
            });
            // FR3: Enter commits, then every Enter reaches onEnter.
            s.on_enter = (!self.disabled).then(|| {
                let (ctx, on_enter) = (ctx.clone(), self.on_enter.clone());
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    ctx.commit(false, window, cx);
                    if let Some(f) = &on_enter {
                        f(window, cx);
                    }
                }) as FocusFn
            });
            s.on_blur = Some({
                let (ctx, on_blur) = (ctx.clone(), self.on_blur.clone());
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    ctx.commit(true, window, cx);
                    if let Some(f) = &on_blur {
                        f(window, cx);
                    }
                })
            });
            s.on_focus = self.on_focus.clone();
            // FR4: unmodified arrows step; other keys reach onKeyDown. A
            // composing IME keeps its arrows (FR11).
            s.on_key_down = (!self.disabled).then(|| {
                let (ctx, on_key_down, editor) =
                    (ctx.clone(), self.on_key_down.clone(), cx.entity());
                Rc::new(
                    move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                        if let Some(f) = &on_key_down {
                            f(event, window, cx);
                        }
                        let keystroke = &event.keystroke;
                        let arrow = match keystroke.key.as_str() {
                            "up" => 1,
                            "down" => -1,
                            _ => return,
                        };
                        let composing = editor.read(cx).buffer().marked().is_some();
                        if composing || keystroke.modifiers.modified() {
                            return;
                        }
                        cx.stop_propagation();
                        ctx.step(arrow, window, cx);
                    },
                ) as astryx_primitives::editing::KeyDownFn
            });
        });
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

        let status = field_status.as_ref().map(|s| s.status);
        let chrome = Chrome {
            id: ElementId::Name(format!("{}-control", self.id).into()),
            target: "number-input",
            variant: None,
            size,
            status,
            disabled: self.disabled,
            read_only: self.read_only,
            focused,
            focus_visible: is_focus_visible(&focus, window),
            row: true,
            place,
            dir,
        };
        let part_override = theme.for_part("number-input", &chrome.props());
        let part = part_override.clone().unwrap_or_else(|| theme.clone());
        let t = part.tokens();
        let mut control = control_box(&chrome, &part, window, cx);
        control.style().refine(&self.control_style);
        // The steppers span the box's height and sit flush with its end.
        let pad_top = control.style().padding.top;
        let pad_bottom = control.style().padding.bottom;
        if self.number_steppers {
            control = control.padding_end(dir, px(0.));
        }
        if !self.disabled {
            let focus = focus.clone();
            control = control.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            });
        }
        // The browser's text-field context menu.
        control = super::field_menu::with_field_menu(
            control,
            &self.id,
            &editor_state,
            &focus,
            self.disabled,
            window,
            cx,
        );
        // FR4: the wheel over the focused field steps it instead of scrolling.
        if self.wheel_enabled && editable && focused {
            let ctx = ctx.clone();
            control = control.on_scroll(move |event, window, cx| {
                let delta = event.delta.pixel_delta(px(1.)).y;
                if delta == px(0.) || event.modifiers.modified() {
                    return;
                }
                cx.stop_propagation();
                ctx.step(if delta > px(0.) { 1 } else { -1 }, window, cx);
            });
        }
        if let Some(icon) = self.start_icon {
            control = control.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .text_color(t.color.icon_secondary)
                    .child(provide(IconDefaultSize(IconSize::Sm), icon)),
            );
        }

        let mut described = described_by(
            self.description.as_ref(),
            field_status.as_ref(),
            self.disabled
                .then_some(self.disabled_message.as_ref())
                .flatten(),
            &i18n,
        );
        described.extend(self.units.clone());
        // FR2: a grouped field is named by the group and itself, and
        // described by the group first.
        let (name, described) = match &group {
            Some(group) => (
                SharedString::from(format!("{} {}", group.label, self.label)),
                group.described.iter().cloned().chain(described).collect(),
            ),
            None => (self.label.clone(), described),
        };
        let invalid = status == Some(StatusType::Error) || shown_error.is_some();
        let style = EditorStyle {
            // FR15: a rejected draft greys while its message shows.
            text_color: if shown_error.is_some() {
                t.color.text_secondary
            } else {
                t.color.text_primary
            },
            placeholder: self.placeholder.clone(),
            placeholder_color: t.color.text_secondary,
            selection_color: selection_color(t),
            caret_color: t.color.text_primary,
            multiline: false,
            min_lines: 1,
            max_lines: Some(1),
            height: None,
            reduced_motion: prefers_reduced_motion(cx),
            spelling_color: t.color.error,
        };
        let mut editor = text_editor("editor", &editor_state, style, cx)
            .flex_1()
            .min_w_0()
            .font_stack(&t.font.family.body)
            .text_style(&t.text.body)
            .role(Role::SpinButton)
            .aria_label(name)
            .aria_value(SharedString::from(display.clone()))
            .aria_disabled(self.disabled)
            .aria_read_only(!editable)
            .aria_required(crate::field::resolve(self.required, self.optional, cx).required)
            .aria_invalid(invalid);
        if let Some(value) = self.value {
            editor = editor.aria_numeric_value(value);
        }
        if let Some(min) = self.min {
            editor = editor.aria_min_numeric_value(min);
        }
        if let Some(max) = self.max {
            editor = editor.aria_max_numeric_value(max);
        }
        if let Some(placeholder) = &self.placeholder {
            editor = editor.aria_placeholder(placeholder.clone());
        }
        if !described.is_empty() {
            editor = editor.aria_description(SharedString::from(described.join(" ")));
        }
        control = control.child(editor);

        if let Some(units) = self.units {
            control = control.child(
                div()
                    .flex_none()
                    .font_stack(&t.font.family.body)
                    .text_style(&t.text.body)
                    .text_color(t.color.text_secondary)
                    .child(units),
            );
        }
        if self.clear && self.value.is_some() && editable {
            let label = i18n.format(
                "@astryx.numberInput.clearLabel",
                &[("label", self.label.as_ref().into())],
            );
            let (ctx, focus) = (ctx.clone(), focus.clone());
            control = control.child(
                IconButton::new(
                    "clear",
                    label,
                    Icon::new("close")
                        .size(IconSize::Sm)
                        .color(IconColor::Secondary),
                )
                .variant(ButtonVariant::Ghost)
                .size(ElementSize::Sm)
                .tooltip(i18n.t("@astryx.inputClearButton.tooltip"))
                .h(px(20.))
                .on_click(move |_, window, cx| {
                    ctx.change(None, window, cx);
                    ctx.end_draft(window, cx);
                    window.focus(&focus, cx);
                }),
            );
        }
        if let Some(status) = status
            && self.status_placement != StatusPlacement::Detached
            && place.is_none()
        {
            let message = field_status.as_ref().and_then(|s| s.message.clone());
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
        if self.number_steppers {
            let base = hold_draft.or_else(|| ctx.stepping_from(cx));
            let can = |direction| stepped(base, direction, rules, self.step) != base;
            let mut column = div()
                .flex()
                .flex_col()
                .flex_none()
                .self_stretch()
                .w(t.spacing.s4)
                .border_start(dir, t.border.width)
                .border_color(t.color.border_emphasized)
                .rounded_end(dir, t.radius.element)
                .overflow_hidden();
            if let Some(top) = pad_top {
                column.style().margin.top = Some(negate(top, rem));
            }
            if let Some(bottom) = pad_bottom {
                column.style().margin.bottom = Some(negate(bottom, rem));
            }
            for direction in [1i8, -1] {
                let blocked = !editable || !can(direction);
                let (name, key) = if direction > 0 {
                    ("increment", "@astryx.numberInput.incrementLabel")
                } else {
                    ("decrement", "@astryx.numberInput.decrementLabel")
                };
                let surface = t.color.background_surface;
                let (hover, pressed) = (
                    composite(surface, t.color.overlay_hover),
                    composite(surface, t.color.overlay_pressed),
                );
                let glyph = Icon::new("numberInput:stepperDown")
                    .size(IconSize::Xsm)
                    .color(IconColor::Inherit);
                // The increment glyph is the decrement turned half a turn.
                let glyph = if direction > 0 {
                    div()
                        .flex()
                        .scale_xy(-1., -1.)
                        .child(glyph)
                        .into_any_element()
                } else {
                    glyph.into_any_element()
                };
                let mut button = div()
                    .id(name)
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .bg(surface)
                    .text_color(if blocked {
                        t.color.icon_disabled
                    } else {
                        t.color.icon_secondary
                    })
                    .role(Role::Button)
                    .aria_label(i18n.format(key, &[("label", self.label.as_ref().into())]))
                    .aria_disabled(blocked)
                    .child(glyph);
                if direction < 0 {
                    button = button
                        .border_t(t.border.width)
                        .border_color(t.color.border_emphasized);
                }
                if !blocked {
                    let (down, up, out) = (ctx.clone(), ctx.clone(), ctx.clone());
                    let focus = focus.clone();
                    button = crate::press::touch_press(
                        button,
                        ElementId::Name(format!("{:?}-step-{direction}", self.id).into()),
                        false,
                        gpui::transparent_black(),
                        pressed,
                        window,
                        cx,
                    );
                    button = button
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            // The field keeps focus while its steppers work.
                            cx.stop_propagation();
                            window.focus(&focus, cx);
                            down.start_hold(direction, window, cx);
                        })
                        .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                            up.end_hold(direction, true, window, cx)
                        })
                        .on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                            out.end_hold(direction, false, window, cx)
                        });
                }
                part.refine(
                    button.style(),
                    "number-input-stepper",
                    &[("direction", name)],
                    None,
                );
                column = column.child(button);
            }
            control = control.child(column);
        }

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
        // In a group the control is the member: no Field of its own.
        if place.is_some() {
            return inside_member(control).into_any_element();
        }
        let mut field = Field::new(self.id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(self.disabled)
            .optional(self.optional)
            .required(self.required)
            .status_placement(self.status_placement);
        if let Some(icon) = self.label_icon {
            field = field.label_icon(icon);
        }
        if let Some(status) = field_status {
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

/// A padding length as a negative margin.
fn negate(length: gpui::DefiniteLength, rem: gpui::Pixels) -> gpui::Length {
    let pixels = match length {
        gpui::DefiniteLength::Absolute(absolute) => absolute.to_pixels(rem),
        gpui::DefiniteLength::Fraction(_) => px(0.),
    };
    gpui::Length::Definite((-pixels).into())
}
