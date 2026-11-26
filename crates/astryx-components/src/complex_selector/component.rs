//! The ComplexSelector component, upstream
//! `ComplexSelector/ComplexSelector.tsx`: a combobox trigger and a dialog
//! popover (the ported Popover) around the caller's content.

use std::rc::Rc;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{
    ElementSize, LayerAlign, Placement, control_focus, is_focus_visible, prefers_reduced_motion,
    provide, resolve_size,
};
use astryx_theme::{ThemeStyled, current, scope};
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, Entity, FocusHandle, HasPopup, IntoElement,
    KeyDownEvent, Length, MouseButton, ParentElement, RenderOnce, Role, SharedString, Stateful,
    StyleRefinement, Styled, Task, Window, div, prelude::*, px,
};

use crate::button::element_height;
use crate::choice::action::{ActionFn, ChangeFn, Optimistic};
use crate::field::{Field, InputStatus, StatusPlacement, StatusType};
use crate::focus::focus_ring;
use crate::icon::{Icon, IconColor, IconDefaultSize, IconSize};
use crate::input::control::{Chrome, control_box, described_by, status_indicator};
use crate::layout::SpacingStep;
use crate::motion::transition;
use crate::overlay::{Popover, PopoverController};
use crate::selector::{SelectorVariant, trigger_line};
use crate::status::{Spinner, SpinnerSize};

/// The commit helper the content receives (upstream `onChange` argument):
/// `on_change`, then `change_action` with the value shown while it runs.
pub type ComplexCommit<V> = Rc<dyn Fn(V, &mut Window, &mut App)>;
type CloseFn = Rc<dyn Fn(&mut Window, &mut App)>;
type ContentFn<V> = Rc<
    dyn Fn(
        &V,
        ComplexCommit<V>,
        CloseFn,
        ComplexSelectorState,
        &mut Window,
        &mut App,
    ) -> AnyElement,
>;
type OpenFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type KeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;
type HandledKeyFn = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App) -> bool>;
type TriggerFn = Rc<dyn Fn(ComplexTrigger, &mut Window, &mut App) -> AnyElement>;

/// The shell's live state, upstream `ComplexSelectorRenderState`.
#[derive(Clone)]
pub struct ComplexSelectorState {
    /// Upstream `isOpen`.
    pub open: bool,
    /// Upstream `isBusy`: `loading` or a running `change_action`.
    pub busy: bool,
    /// The trigger's focus (upstream `triggerId`, the element it names).
    pub trigger: FocusHandle,
}

/// What a caller-rendered trigger takes (upstream
/// `ComplexSelectorRenderTriggerProps`): pass the control through
/// [`ComplexTrigger::attach`].
#[derive(Clone)]
pub struct ComplexTrigger {
    /// The control's focus (upstream `ref`): focus returns here on close.
    pub focus: FocusHandle,
    pub expanded: bool,
    pub busy: bool,
    pub required: bool,
    pub disabled: bool,
    label: SharedString,
    toggle: CloseFn,
    key: HandledKeyFn,
    on_click: Option<ClickFn>,
    on_key_down: Option<KeyFn>,
}

impl ComplexTrigger {
    /// Gives `el` the combobox role, name, popup states, focus, press and
    /// keys (FR10).
    pub fn attach(self, el: Stateful<Div>) -> Stateful<Div> {
        let (toggle, key, disabled) = (self.toggle.clone(), self.key.clone(), self.disabled);
        let (on_click, on_key) = (self.on_click.clone(), self.on_key_down.clone());
        el.track_focus(&self.focus)
            .role(Role::ComboBox)
            .aria_label(self.label.clone())
            .aria_has_popup(HasPopup::Dialog)
            .aria_expanded(self.expanded)
            .aria_busy(self.busy)
            .aria_required(self.required)
            .aria_disabled(self.disabled)
            .on_click(move |event, window, cx| {
                // FR5: a disabled control's press reaches no handler.
                if disabled {
                    cx.stop_propagation();
                    return;
                }
                toggle(window, cx);
                if let Some(f) = &on_click {
                    f(event, window, cx);
                }
            })
            .on_key_down(move |event, window, cx| {
                if key(event, window, cx) {
                    cx.stop_propagation();
                } else if !disabled && let Some(f) = &on_key {
                    f(event, window, cx);
                }
            })
    }
}

pub(crate) struct ShellState {
    disabled: bool,
}

/// Opens, closes and toggles a ComplexSelector from code, upstream
/// `ComplexSelectorHandle`. Make it with the selector's id in the render
/// that draws it, and pass it to [`ComplexSelector::controller`].
#[derive(Clone)]
pub struct ComplexSelectorController {
    popover: PopoverController,
    shell: Entity<ShellState>,
}

impl ComplexSelectorController {
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id: ElementId = id.into();
        ComplexSelectorController {
            popover: PopoverController::new(popup_id(&id), window, cx),
            shell: window.use_keyed_state(
                ElementId::Name(format!("{id}-shell").into()),
                cx,
                |_, _| ShellState { disabled: false },
            ),
        }
    }

    /// Upstream `open()`: nothing while disabled (FR5).
    pub fn open(&self, window: &mut Window, cx: &mut App) {
        if !self.shell.read(cx).disabled {
            self.popover.show(window, cx);
        }
    }

    /// Upstream `close()`.
    pub fn close(&self, window: &mut Window, cx: &mut App) {
        self.popover.hide(window, cx);
    }

    /// Upstream `toggle()`: nothing while disabled (FR5).
    pub fn toggle(&self, window: &mut Window, cx: &mut App) {
        if !self.shell.read(cx).disabled {
            self.popover.toggle(window, cx);
        }
    }

    /// Upstream `isOpen()`.
    pub fn is_open(&self, cx: &App) -> bool {
        self.popover.is_open(cx)
    }
}

fn popup_id(id: &ElementId) -> ElementId {
    ElementId::Name(format!("{id}-popup").into())
}

/// A trigger and a dialog popover around rich content, upstream
/// `ComplexSelector`. The content owns its semantics.
///
/// ```ignore
/// ComplexSelector::new("priority", "Priority", priority.clone())
///     .trigger_label(priority.clone())
///     .content(|value, commit, close, _, _, _| {
///         RadioList::new("priority-list", "Priority", value.clone())
///             .on_change(move |next, window, cx| { commit(next, window, cx); close(window, cx) })
///             .into_any_element()
///     })
/// ```
#[derive(IntoElement)]
pub struct ComplexSelector<V: Clone + 'static> {
    id: ElementId,
    label: SharedString,
    value: V,
    on_change: Option<ChangeFn<V>>,
    change_action: Option<ActionFn<V>>,
    content: Option<ContentFn<V>>,
    trigger_label: Option<AnyElement>,
    placeholder: Option<AnyElement>,
    label_hidden: bool,
    description: Option<SharedString>,
    label_tooltip: Option<SharedString>,
    optional: bool,
    required: bool,
    disabled: bool,
    disabled_message: Option<SharedString>,
    loading: bool,
    status: Option<InputStatus>,
    status_placement: Option<StatusPlacement>,
    size: Option<ElementSize>,
    variant: SelectorVariant,
    start_icon: Option<AnyElement>,
    width: Option<Length>,
    placement: Placement,
    alignment: LayerAlign,
    render_trigger: Option<TriggerFn>,
    controller: Option<ComplexSelectorController>,
    on_open_change: Option<OpenFn>,
    content_style: StyleRefinement,
    on_click: Option<ClickFn>,
    on_key_down: Option<KeyFn>,
    control_style: StyleRefinement,
    style: StyleRefinement,
}

impl<V: Clone + 'static> ComplexSelector<V> {
    /// Upstream `label` and `value`.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, value: V) -> Self {
        ComplexSelector {
            id: id.into(),
            label: label.into(),
            value,
            on_change: None,
            change_action: None,
            content: None,
            trigger_label: None,
            placeholder: None,
            label_hidden: false,
            description: None,
            label_tooltip: None,
            optional: false,
            required: false,
            disabled: false,
            disabled_message: None,
            loading: false,
            status: None,
            status_placement: None,
            size: None,
            variant: SelectorVariant::Input,
            start_icon: None,
            width: None,
            placement: Placement::Below,
            alignment: LayerAlign::Start,
            render_trigger: None,
            controller: None,
            on_open_change: None,
            content_style: StyleRefinement::default(),
            on_click: None,
            on_key_down: None,
            control_style: StyleRefinement::default(),
            style: StyleRefinement::default(),
        }
    }

    astryx_primitives::setters! {
        /// Upstream `isLabelHidden`.
        !label_hidden,
        /// Upstream `description`.
        ?description: SharedString,
        /// Upstream `labelTooltip`.
        ?label_tooltip: SharedString,
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
        /// Upstream `status`.
        ?status: InputStatus,
        /// Upstream `statusVariant`.
        ?status_placement: StatusPlacement,
        /// Upstream `size`.
        ?size: ElementSize,
        /// Upstream `variant`.
        variant: SelectorVariant,
        /// Upstream `width`.
        ?width: Length,
        /// Upstream `placement`: below by default.
        placement: Placement,
        /// Upstream `alignment`: start by default.
        alignment: LayerAlign,
    }

    /// Upstream `onChange`.
    pub fn on_change(mut self, f: impl Fn(V, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `changeAction`: async work after `on_change`; the content
    /// sees the new value and `busy` while it runs.
    pub fn change_action(
        mut self,
        f: impl Fn(V, &mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.change_action = Some(Rc::new(f));
        self
    }

    /// Upstream `children`: the popup's content from the value, the commit
    /// helper, the close helper and the shell's state (FR1).
    pub fn content(
        mut self,
        f: impl Fn(
            &V,
            ComplexCommit<V>,
            CloseFn,
            ComplexSelectorState,
            &mut Window,
            &mut App,
        ) -> AnyElement
        + 'static,
    ) -> Self {
        self.content = Some(Rc::new(f));
        self
    }

    /// Upstream `triggerLabel`: what the trigger shows for the value.
    pub fn trigger_label(mut self, label: impl IntoElement) -> Self {
        self.trigger_label = Some(label.into_any_element());
        self
    }

    /// Upstream `placeholder`: shown without a `trigger_label`.
    pub fn placeholder(mut self, placeholder: impl IntoElement) -> Self {
        self.placeholder = Some(placeholder.into_any_element());
        self
    }

    /// Upstream `startIcon`.
    pub fn start_icon(mut self, icon: impl IntoElement) -> Self {
        self.start_icon = Some(icon.into_any_element());
        self
    }

    /// Upstream `renderTrigger`: the caller's control, which
    /// [`ComplexTrigger::attach`] wires; no field renders (FR10).
    pub fn render_trigger(
        mut self,
        f: impl Fn(ComplexTrigger, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_trigger = Some(Rc::new(f));
        self
    }

    /// Upstream `handleRef`.
    pub fn controller(mut self, controller: ComplexSelectorController) -> Self {
        self.controller = Some(controller);
        self
    }

    /// Upstream `onOpenChange`: every open and close (FR8).
    pub fn on_open_change(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_change = Some(Rc::new(f));
        self
    }

    /// Upstream `contentXstyle`: the content's box.
    pub fn content_style(mut self, style: StyleRefinement) -> Self {
        self.content_style = style;
        self
    }

    /// Upstream `onClick`: before the toggle; `window.prevent_default()`
    /// keeps the popup closed (FR11).
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Upstream `onKeyDown`: after the built-in keys.
    pub fn on_key_down(
        mut self,
        f: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_key_down = Some(Rc::new(f));
        self
    }

    /// Upstream `controlXstyle`.
    pub fn control_style(mut self, style: StyleRefinement) -> Self {
        self.control_style = style;
        self
    }
}

impl<V: Clone + 'static> Styled for ComplexSelector<V> {
    /// Upstream `containerXstyle`.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<V: Clone + 'static> RenderOnce for ComplexSelector<V> {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t18 = i18n(cx);
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let input = self.variant == SelectorVariant::Input;
        let disabled = self.disabled;
        let status_placement = match self.status_placement.unwrap_or(StatusPlacement::Attached) {
            StatusPlacement::Attached if !input => StatusPlacement::Detached,
            p => p,
        };

        // FR1, FR6: the value shown (an action's while it runs) and busy.
        let optimistic = Optimistic::<V>::new(&self.id, window, cx);
        let value = optimistic.shown(self.value.clone(), cx);
        let busy = self.loading || optimistic.pending(cx);
        optimistic.announce_busy(busy, t18.t("@astryx.spinner.loading"), window, cx);
        let controller = self
            .controller
            .clone()
            .unwrap_or_else(|| ComplexSelectorController::new(self.id.clone(), window, cx));
        controller.shell.update(cx, |s, _| s.disabled = disabled);
        let pipeline =
            Rc::new(optimistic.change(self.on_change.clone(), self.change_action.clone()));
        let commit: ComplexCommit<V> = {
            let shell = controller.shell.clone();
            Rc::new(move |next, window, cx| {
                if !shell.read(cx).disabled {
                    pipeline(next, window, cx);
                }
            })
        };
        let trigger = control_focus(
            &ElementId::Name(format!("{}-trigger", self.id).into()),
            window,
            cx,
        );
        let open = controller.is_open(cx);
        let close: CloseFn = {
            let popover = controller.popover.clone();
            Rc::new(move |window, cx| popover.hide(window, cx))
        };
        let toggle: CloseFn = {
            let controller = controller.clone();
            Rc::new(move |window, cx| controller.toggle(window, cx))
        };
        // FR8: ArrowDown opens a closed popup.
        let key_fn: HandledKeyFn = {
            let controller = controller.clone();
            Rc::new(move |event, window, cx| {
                let closed = !controller.is_open(cx) && !controller.shell.read(cx).disabled;
                if event.keystroke.key == "down" && closed {
                    controller.open(window, cx);
                    return true;
                }
                false
            })
        };
        let external = self.render_trigger.is_some();
        let status = self.status.as_ref().map(|s| s.status);

        // The trigger: the caller's control (FR10), or the built-in one.
        let trigger_el: AnyElement = match self.render_trigger.clone() {
            Some(render) => render(
                ComplexTrigger {
                    focus: trigger.clone(),
                    expanded: open,
                    busy,
                    required: self.required,
                    disabled,
                    label: self.label.clone(),
                    toggle: toggle.clone(),
                    key: key_fn.clone(),
                    on_click: self.on_click.clone(),
                    on_key_down: self.on_key_down.clone(),
                },
                window,
                cx,
            ),
            None => {
                let theme = theme.clone();
                let mut props: Vec<(&'static str, &'static str)> =
                    vec![("variant", self.variant.as_str()), ("size", size.as_str())];
                if let Some(status) = status {
                    props.push(("status", status.as_str()));
                }
                if disabled {
                    props.push(("disabled", "disabled"));
                }
                let part_override = theme.for_part("complex-selector", &props);
                let part = part_override.clone().unwrap_or_else(|| theme.clone());
                let t = part.tokens();
                let focused = trigger.is_focused(window);
                let focus_visible = is_focus_visible(&trigger, window);
                let height = element_height(size, t).to_pixels(rem);
                let (line, pad) = trigger_line(size, t, rem, input);
                let control_id = ElementId::Name(format!("{}-control", self.id).into());
                let mut control = if input {
                    control_box(
                        &Chrome {
                            id: control_id.clone(),
                            target: "complex-selector",
                            variant: Some("input"),
                            size,
                            status,
                            disabled,
                            read_only: false,
                            focused,
                            focus_visible,
                            row: false,
                            place: None,
                            dir,
                        },
                        &part,
                        window,
                        cx,
                    )
                } else {
                    let fill = match self.variant {
                        SelectorVariant::Secondary => t.color.neutral,
                        _ => gpui::transparent_black(),
                    };
                    let mut el = div()
                        .id(control_id.clone())
                        .debug_selector({
                            let id = control_id.to_string();
                            move || id
                        })
                        .relative()
                        .flex()
                        .flex_row()
                        .self_start()
                        .rounded(t.radius.element)
                        .bg(fill)
                        .font_weight(t.font.weight.medium);
                    if !disabled {
                        let hover = astryx_theme::color::composite(fill, t.color.overlay_hover);
                        el = el.hover(move |s| s.bg(hover));
                        // A finger's press paints as UIKit tracks a control (upstream `usePressFeedback`).
                        el = crate::press::touch_press(
                            el,
                            ElementId::Name(format!("{control_id:?}-trigger-press").into()),
                            false,
                            fill,
                            t.color.overlay_pressed,
                            window,
                            cx,
                        );
                        if !open && !prefers_reduced_motion(cx) && !window.last_input_was_touch() {
                            el = el.active(|s| s.scale(0.98));
                        }
                    } else {
                        el = el.opacity(0.5);
                    }
                    part.refine(el.style(), "complex-selector", &props, None);
                    if focus_visible {
                        el = el.child(focus_ring(t, t.radius.element.to_pixels(rem), rem));
                    }
                    el
                };
                let has_label = self.trigger_label.is_some();
                control = control
                    .track_focus(&trigger)
                    .items_center()
                    .justify_between()
                    .gap(t.spacing.s2)
                    .px(t.spacing.s3)
                    .py(pad)
                    .min_h(height)
                    .font_stack(&t.font.family.body)
                    .text_size(t.text.label.size)
                    .line_height(line)
                    .text_color(if has_label {
                        t.color.text_primary
                    } else {
                        t.color.text_secondary
                    });
                if input {
                    control = control.w_full();
                }
                control = if disabled {
                    control.cursor_default()
                } else {
                    control.cursor_pointer()
                };
                control.style().refine(&self.control_style);
                if let Some(icon) = self.start_icon.take() {
                    control = control.child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .text_color(t.color.icon_secondary)
                            .child(provide(IconDefaultSize(IconSize::Sm), icon)),
                    );
                }
                let shown: AnyElement = match self.trigger_label.take() {
                    Some(label) => label,
                    None => self.placeholder.take().unwrap_or_else(|| {
                        t18.t("@astryx.selector.placeholder").into_any_element()
                    }),
                };
                let value_selector = format!("{}-value", self.id);
                control = control.child(
                    div()
                        .debug_selector(move || value_selector)
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(shown),
                );
                if busy {
                    let busy_selector = format!("{}-busy", self.id);
                    control = control.child(
                        div()
                            .flex()
                            .flex_none()
                            .debug_selector(move || busy_selector)
                            .child(
                                Spinner::new(ElementId::Name(
                                    format!("{}-spinner", self.id).into(),
                                ))
                                .size(SpinnerSize::Sm)
                                .decorative(),
                            ),
                    );
                }
                // FR7: a status icon or tip takes the chevron's place.
                if let Some(status) =
                    status.filter(|_| status_placement != StatusPlacement::Detached)
                {
                    let message = self.status.as_ref().and_then(|s| s.message.clone());
                    control = control.child(status_indicator(
                        &self.id,
                        status,
                        status_placement,
                        message.as_ref(),
                        &part,
                        window,
                        cx,
                    ));
                } else {
                    let turn = transition(
                        format!("{}-chevron", self.id),
                        open,
                        t.duration.fast,
                        t.ease.standard,
                        window,
                        cx,
                    );
                    let mut chevron = Icon::new("chevronDown")
                        .size(IconSize::Sm)
                        .color(IconColor::Secondary)
                        .rotation(std::f32::consts::PI * turn);
                    part.refine(
                        chevron.style(),
                        "complex-selector-indicator-icon",
                        &[("state", if open { "expanded" } else { "collapsed" })],
                        None,
                    );
                    let indicator = format!("{}-indicator", self.id);
                    control = control.child(
                        div()
                            .flex()
                            .flex_none()
                            .debug_selector(move || indicator)
                            .child(chevron),
                    );
                }
                // FR4, FR11: the combobox's name, popup and states.
                let described = described_by(
                    self.description.as_ref(),
                    self.status.as_ref(),
                    disabled.then_some(self.disabled_message.as_ref()).flatten(),
                    &t18,
                );
                control = control
                    .role(Role::ComboBox)
                    .aria_label(self.label.clone())
                    .aria_has_popup(HasPopup::Dialog)
                    .aria_expanded(open)
                    .aria_required(self.required)
                    .aria_invalid(status == Some(StatusType::Error))
                    .aria_busy(busy)
                    .aria_disabled(disabled);
                if !described.is_empty() {
                    control = control.aria_description(SharedString::from(described.join(" ")));
                }
                {
                    let trigger = trigger.clone();
                    control = control.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        trigger.focus(window, cx);
                    });
                }
                {
                    let (toggle, on_click) = (toggle.clone(), self.on_click.clone());
                    control = control.on_click(move |event, window, cx| {
                        // FR5: a disabled trigger's press reaches no handler.
                        if disabled {
                            cx.stop_propagation();
                            return;
                        }
                        // FR11: the caller's onClick first; it can keep the
                        // popup closed.
                        if let Some(f) = &on_click {
                            f(event, window, cx);
                            if window.default_prevented() {
                                return;
                            }
                        }
                        toggle(window, cx);
                    });
                }
                {
                    let (key_fn, on_key) = (key_fn.clone(), self.on_key_down.clone());
                    control = control.on_key_down(move |event, window, cx| {
                        if key_fn(event, window, cx) {
                            cx.stop_propagation();
                        }
                        if !disabled && let Some(f) = &on_key {
                            f(event, window, cx);
                        }
                    });
                }
                match part_override {
                    Some(part) => scope(part, control).into_any_element(),
                    None => control.into_any_element(),
                }
            }
        };

        // The popup: a dialog popover around the content (FR2, FR9).
        let content = self.content.clone();
        let content_style = self.content_style.clone();
        let shell_state = ComplexSelectorState {
            open,
            busy,
            trigger: trigger.clone(),
        };
        let content_id = ElementId::Name(format!("{}-content", self.id).into());
        let content_fn = move |window: &mut Window, cx: &mut App| -> AnyElement {
            let theme = current(window, cx);
            let t = theme.tokens();
            let cap = crate::overlay::viewport_cap(t, window);
            let body = content
                .as_ref()
                .map(|f| {
                    f(
                        &value,
                        commit.clone(),
                        close.clone(),
                        shell_state.clone(),
                        window,
                        cx,
                    )
                })
                .unwrap_or_else(|| div().into_any_element());
            let scroller_handle =
                crate::gestures::scroller::use_scroll_handle(content_id.clone(), window, cx);
            let mut wrap = div()
                .id(content_id.clone())
                .debug_selector({
                    let id = content_id.to_string();
                    move || id
                })
                .p(t.spacing.s3)
                .max_h(px(480.).min(cap.height))
                .overflow_y_scroll()
                .track_scroll(&scroller_handle)
                .child(crate::gestures::scroller::scroll_begin(
                    content_id.clone(),
                    Some(&scroller_handle),
                    (false, true),
                ))
                .child(body)
                .child(crate::gestures::scroller::scroll_end());
            wrap.style().refine(&content_style);
            wrap.into_any_element()
        };
        let report = self.on_open_change.clone();
        let return_to = trigger.clone();
        let popover = Popover::new(popup_id(&self.id), self.label.clone())
            .controller(controller.popover.clone())
            .enabled(false)
            .close_button(false)
            .auto_focus(true)
            .placement(self.placement)
            .alignment(self.alignment)
            .padding(SpacingStep::S0)
            .surface_target("complex-selector-popup")
            .return_focus(trigger.clone())
            .on_open_change(move |open, window, cx| {
                // A caller's control takes focus back on every close.
                if !open && external {
                    return_to.focus(window, cx);
                }
                if let Some(f) = &report {
                    f(open, window, cx);
                }
            })
            .content_fn(content_fn)
            .child(trigger_el);
        // FR13: a disabled selector's reason, on hover and focus.
        let control = crate::overlay::disabled_reason(
            &self.id,
            disabled,
            self.disabled_message.as_ref().filter(|m| !m.is_empty()),
            false,
            popover,
        );
        if external {
            return control;
        }
        let mut field = Field::new(self.id, self.label)
            .description_opt(self.description)
            .label_hidden(self.label_hidden)
            .disabled(disabled)
            .optional(self.optional)
            .required(self.required)
            .status_placement(status_placement);
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
