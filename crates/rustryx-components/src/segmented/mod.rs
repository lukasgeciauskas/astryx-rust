//! SegmentedControl. Upstream: `SegmentedControl/`.
//!
//! A change of selection slides the selected surface, one lens, from the old segment to
//! the new one ([`lens`], FR9 and FR10). A press is tracked as UIKit's segmented control
//! tracks it (FR11-FR14, upstream `useSegmentedTouchTracking.ts`): a press tracker reports
//! the press from landing to lift, and a horizontal pan carries the lens. Neither claims
//! a vertical drag, which still scrolls the page. A tap selects through the segment's
//! click; a drag commits at the lift on the segment nearest the pointer, counted on the
//! control's center line; a cancel or Escape commits nothing.

pub mod examples;
mod lens;
mod touch;

#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod touch_tests;

use std::rc::Rc;

use std::cell::RefCell;

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StyleRefinement, Styled, Toggled, Window, div, prelude::*, px, relative,
};
use rustryx_i18n::use_direction;
use rustryx_primitives::gestures::host::{
    GestureAttachment, after_gesture, gesture_area, gesture_host,
};
use rustryx_primitives::gestures::{
    ControlEvent, GestureState, GestureTouch, PanDirection, PanGestureRecognizer, PanOptions,
    PressTracker, PressTrackerOptions, RecognizerOptions,
};
use rustryx_primitives::{
    ElementSize, ListFocus, ListKeys, is_focus_visible, provide, resolve_size,
};
use rustryx_theme::{ThemeStyled, current};

use crate::button::element_height;
use crate::choice::row_toggles;
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::focus::focus_ring;
use crate::icon::{IconDefaultSize, IconSize};

/// Upstream `layout`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SegmentLayout {
    /// Each segment as wide as its content, capped at the container.
    #[default]
    Hug,
    /// Segments share the full width equally.
    Fill,
}

impl SegmentLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            SegmentLayout::Hug => "hug",
            SegmentLayout::Fill => "fill",
        }
    }
}

/// One segment, upstream `SegmentedControlItem`.
pub struct SegmentedControlItem {
    value: SharedString,
    label: SharedString,
    label_hidden: bool,
    icon: Option<AnyElement>,
    disabled: bool,
}

impl SegmentedControlItem {
    /// Upstream `value` and `label`.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        SegmentedControlItem {
            value: value.into(),
            label: label.into(),
            label_hidden: false,
            icon: None,
            disabled: false,
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `isLabelHidden`: icon only; the label names it.
        !label_hidden,
        /// Upstream `isDisabled`.
        !disabled,
    }

    /// Upstream `icon`: before the label.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }
}

type SelectFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// What the press's listeners read of the latest frame.
#[derive(Default)]
struct PressContext {
    lens: Option<gpui::Entity<lens::LensState>>,
    selected: Option<usize>,
    disabled: Vec<bool>,
    values: Vec<SharedString>,
    on_change: Option<SelectFn>,
    /// The touch presentation (a coarse primary pointer).
    presenting: bool,
    /// The track's padding box, in window px.
    track: gpui::Bounds<gpui::Pixels>,
}

/// The control's press tracking: a press tracker and a horizontal pan on the track.
struct PressTracking {
    attachment: GestureAttachment,
    context: Rc<RefCell<PressContext>>,
    _tracker: PressTracker,
    _pan: PanGestureRecognizer,
}

fn track_point(context: &PressContext, x: f64, y: f64) -> (f32, f32) {
    (
        x as f32 - f32::from(context.track.origin.x),
        y as f32 - f32::from(context.track.origin.y),
    )
}

fn track_area(context: &PressContext) -> touch::Area {
    let size = context.track.size;
    touch::Area {
        left: 0.,
        top: 0.,
        right: f32::from(size.width),
        bottom: f32::from(size.height),
    }
}

fn press_tracking(
    id: &ElementId,
    window: &mut Window,
    cx: &mut App,
) -> (GestureAttachment, Rc<RefCell<PressContext>>) {
    let host = gesture_host(window, cx);
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-press-tracking").into()),
        cx,
        move |_, _| {
            let context: Rc<RefCell<PressContext>> = Rc::default();
            let pan = PanGestureRecognizer::new(PanOptions {
                base: RecognizerOptions {
                    name: Some("segmented-control-drag".into()),
                    cancels_touches_in_view: false,
                    // A hold on the track does something: no title is selectable under it.
                    selection_lock: Some(Some(rustryx_primitives::gestures::lock_selection())),
                    ..RecognizerOptions::default()
                },
                direction: PanDirection::Horizontal,
                ..PanOptions::default()
            });
            let attachment = host.attach(&[pan.any()]);
            {
                let context = context.clone();
                pan.add_listener(move |r| {
                    let state = r.state();
                    let velocity = r.velocity().x;
                    if state == GestureState::Ended {
                        let context = context.clone();
                        after_gesture(move |_, cx| {
                            if let Some(lens) = context.borrow().lens.clone() {
                                lens::drag_velocity(&lens, velocity, true, cx);
                            }
                        });
                        return;
                    }
                    if !matches!(state, GestureState::Began | GestureState::Changed) {
                        return;
                    }
                    let location = r.location();
                    let context = context.clone();
                    after_gesture(move |_, cx| {
                        let c = context.borrow();
                        let Some(lens) = c.lens.clone() else {
                            return;
                        };
                        if !lens::pressing(&lens, cx) {
                            return;
                        }
                        lens::drag_velocity(&lens, velocity, false, cx);
                        // The lens chases the pointer; the tracker's lift decides.
                        let (x, _) = track_point(&c, location.x, location.y);
                        let presenting = c.presenting;
                        drop(c);
                        lens::drag(&lens, x, presenting, cx);
                    });
                });
            }
            let mut options = PressTrackerOptions::new(attachment.bounds_reader());
            {
                let context = context.clone();
                options.on_highlight_change = Some(Rc::new(move |highlighted| {
                    if !highlighted {
                        return;
                    }
                    let context = context.clone();
                    after_gesture(move |_, cx| {
                        let c = context.borrow();
                        if let Some(lens) = c.lens.clone() {
                            let presenting = c.presenting;
                            drop(c);
                            lens::highlighted(&lens, presenting, cx);
                        }
                    });
                }));
            }
            {
                let context = context.clone();
                options.on_control_event = Some(Rc::new(move |event, touch: &GestureTouch| {
                    let (x, y, finger) = (
                        touch.x(),
                        touch.y(),
                        touch.pointer_type() != rustryx_primitives::gestures::PointerKind::Mouse,
                    );
                    let context = context.clone();
                    after_gesture(move |window, cx| {
                        let c = context.borrow();
                        let Some(lens) = c.lens.clone() else {
                            return;
                        };
                        let (tx, ty) = track_point(&c, x, y);
                        match event {
                            ControlEvent::TouchDown => {
                                let (area, selected, disabled) =
                                    (track_area(&c), c.selected, c.disabled.clone());
                                drop(c);
                                lens::begin(
                                    &lens,
                                    lens::Landing {
                                        x: tx,
                                        y: ty,
                                        track: area,
                                        selected,
                                        disabled,
                                        finger,
                                    },
                                    cx,
                                );
                            }
                            ControlEvent::TouchUpInside | ControlEvent::TouchUpOutside => {
                                let (values, on_change) = (c.values.clone(), c.on_change.clone());
                                drop(c);
                                // The lift decides, wherever the press began; a tap commits
                                // through the segment's own click.
                                if let Some(target) = lens::release(&lens, tx, ty, (tx, ty), cx)
                                    && let (Some(value), Some(f)) = (values.get(target), on_change)
                                {
                                    f(value.clone(), window, cx);
                                }
                            }
                            ControlEvent::TouchCancel => {
                                drop(c);
                                lens::cancel(&lens, cx);
                            }
                            _ => {}
                        }
                        window.refresh();
                    });
                }));
            }
            let tracker = PressTracker::new(options, host.clock());
            attachment.set_consumer(Some(tracker.consumer()));
            PressTracking {
                attachment,
                context,
                _tracker: tracker,
                _pan: pan,
            }
        },
    );
    let s = state.read(cx);
    (s.attachment.clone(), s.context.clone())
}

/// A single choice among a few options shown side by side, upstream
/// `SegmentedControl`.
///
/// ```ignore
/// SegmentedControl::new("view", "View", "list")
///     .item(SegmentedControlItem::new("list", "List"))
///     .item(SegmentedControlItem::new("grid", "Grid"))
///     .on_change(|value, _, _| { /* store it */ })
/// ```
#[derive(IntoElement)]
pub struct SegmentedControl {
    id: ElementId,
    label: SharedString,
    value: SharedString,
    on_change: Option<SelectFn>,
    size: Option<ElementSize>,
    layout: SegmentLayout,
    disabled: bool,
    disabled_message: Option<SharedString>,
    items: Vec<SegmentedControlItem>,
    style: StyleRefinement,
}

impl SegmentedControl {
    /// Upstream `label` (its accessible name, never shown) and `value`.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        SegmentedControl {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            on_change: None,
            size: None,
            layout: SegmentLayout::Hug,
            disabled: false,
            disabled_message: None,
            items: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `size`: else the nearest size provider, else `md`.
        ?size: ElementSize,
        /// Upstream `layout`.
        layout: SegmentLayout,
        /// Upstream `isDisabled`: every segment.
        !disabled,
        /// Upstream `disabledMessage`: the reason, in a tooltip on hover and focus.
        ?disabled_message: SharedString,
    }

    /// Upstream `onChange`: the newly selected value.
    pub fn on_change(mut self, f: impl Fn(SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `children`: one segment.
    pub fn item(mut self, item: SegmentedControlItem) -> Self {
        self.items.push(item);
        self
    }

    /// Upstream `children`: segments.
    pub fn items(mut self, items: impl IntoIterator<Item = SegmentedControlItem>) -> Self {
        self.items.extend(items);
        self
    }
}

impl Styled for SegmentedControl {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SegmentedControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let size = resolve_size(self.size, ElementSize::Md, cx);
        let disabled: Vec<bool> = self
            .items
            .iter()
            .map(|i| i.disabled || self.disabled)
            .collect();
        let values: Rc<[SharedString]> = self.items.iter().map(|i| i.value.clone()).collect();
        let selected = values.iter().position(|v| *v == self.value);

        // Arrows move focus and select (upstream: selection follows focus).
        let on_change = self.on_change.clone();
        let move_values = values.clone();
        let list = ListFocus::new(
            &self.id,
            self.items.len(),
            ListKeys::horizontal(),
            window,
            cx,
        )
        .disabled(disabled.iter().copied())
        .rtl(dir.is_rtl())
        .on_move(move |index, window, cx| {
            if let Some(f) = &on_change {
                f(move_values[index].clone(), window, cx);
            }
        });
        let tab_stop = list.tab_stop(selected, window);
        // A composing component's focus request for a segment
        // (`request_focus` with the segment's id).
        for (index, value) in values.iter().enumerate() {
            let item_id = ElementId::Name(format!("{:?}-{}", self.id, value).into());
            if rustryx_primitives::take_focus_request(&item_id, cx) {
                let handle = list.handle(index).clone();
                window.defer(cx, move |window, cx| window.focus(&handle, cx));
            }
        }
        let hint = crate::keyboard_hint::use_keyboard_hint(
            &self.id,
            crate::keyboard_hint::KeyboardHintOptions::default().enabled(!self.disabled),
            list.focused(window).is_some(),
            window,
            cx,
        );

        let pad = t.spacing.s0_5.to_pixels(rem);
        let radius = t.radius.element.to_pixels(rem);
        let inner_radius = (radius - pad).max(px(0.));
        let height = element_height(size, t).to_pixels(rem) - px(4.);
        let (text, inline, icon_size) = match size {
            ElementSize::Sm => (&t.text.supporting, t.spacing.s2, px(14.)),
            ElementSize::Md => (&t.text.label, t.spacing.s3, px(16.)),
            ElementSize::Lg => (&t.text.label, t.spacing.s3, px(18.)),
        };
        let fill = self.layout == SegmentLayout::Fill;
        let (hover, pressed) = (t.color.overlay_hover, t.color.overlay_pressed);

        // FR9, FR10: the lens slides from the old segment to the new one.
        let lens_state = lens::use_lens(&self.id, window, cx);
        let touch = rustryx_primitives::coarse_primary_pointer(window, cx);
        lens::select(
            &lens_state,
            selected,
            touch,
            t.duration.fast,
            t.ease.standard,
            cx,
        );
        let lens_frame = lens::frame(&lens_state, selected, window, cx);
        let carried = lens_frame.lens.is_some();
        // A held lens's face (FR12) and a drag's target from elsewhere (FR13).
        lens::follow(&lens_state, cx);
        let (face, drag_target) = {
            let s = lens_state.read(cx);
            (s.face, s.drag_target)
        };
        let (attachment, press_context) = press_tracking(&self.id, window, cx);
        let track_bounds = Rc::new(std::cell::Cell::new(press_context.borrow().track));
        {
            let mut c = press_context.borrow_mut();
            c.lens = Some(lens_state.clone());
            c.selected = selected;
            c.disabled = disabled.clone();
            c.values = values.to_vec();
            c.on_change = self.on_change.clone();
            c.presenting = touch;
        }
        let presses = !self.disabled;
        let count = self.items.len();
        // The selected surface, as the lens paints it: the theme's selected segment.
        let surface = |b: lens::SegmentBox, opacity: f32, selector: &'static str| {
            let mut layer = div()
                .rounded(inner_radius)
                .bg(t.color.background_surface)
                .shadow_token(&t.shadow.low);
            theme.refine(
                layer.style(),
                "segmented-control-item",
                &[("size", size.as_str()), ("selected", "selected")],
                None,
            );
            let id = format!("{}-{selector}", self.id);
            layer
                .absolute()
                .left(px(b.left))
                .top(px(b.top))
                .w(px(b.width))
                .h(px(b.height))
                .p_0()
                .opacity(opacity)
                .debug_selector(move || id.clone())
        };
        let leaving_layer = lens_frame
            .leaving
            .map(|(b, opacity)| surface(b, opacity, "leaving"));
        let lens_layer = lens_frame
            .lens
            .map(|(b, opacity)| surface(b, opacity, "lens"));

        let segments = self
            .items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let is_selected = selected == Some(i);
                let item_disabled = disabled[i];
                let handle = list.item_handle(i, tab_stop);
                let line = text.size.to_pixels(rem) * text.leading;
                let mut seg = div()
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap(t.spacing.s1)
                    .h(height.max(line))
                    .rounded(inner_radius)
                    .text_size(text.size)
                    .line_height(relative(text.leading))
                    .font_weight(t.font.weight.medium)
                    .text_color(t.color.text_secondary)
                    .whitespace_nowrap()
                    .min_w_0();
                seg = if item.label_hidden {
                    seg.aspect_ratio(1.).flex_none()
                } else {
                    seg.px(inline)
                };
                if fill {
                    seg = seg.flex_1();
                }
                if is_selected {
                    seg = seg
                        .text_color(t.color.text_primary)
                        .font_weight(t.font.weight.semibold)
                        .bg(t.color.background_surface)
                        .shadow_token(&t.shadow.low);
                }
                if item_disabled {
                    seg = seg.text_color(t.color.text_disabled);
                }
                // FR12: the selected face follows a dragging lens; the weights change at the lift.
                if face == Some(i) {
                    seg = seg.text_color(t.color.text_primary);
                } else if is_selected && face.is_some() {
                    seg = seg.text_color(t.color.text_secondary);
                }
                theme.refine(
                    seg.style(),
                    "segmented-control-item",
                    &[
                        ("size", size.as_str()),
                        ("selected", if is_selected { "selected" } else { "" }),
                        ("disabled", if item_disabled { "disabled" } else { "" }),
                    ],
                    None,
                );
                if is_selected && carried {
                    // The lens carries the surface: the destination paints none of its own.
                    seg = seg.bg(gpui::transparent_black()).shadow(Vec::new());
                }
                let icon_el = item.icon.map(|icon| {
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(icon_size)
                        .child(provide(IconDefaultSize(IconSize::Sm), icon))
                });
                let label = (!item.label_hidden).then(|| {
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(item.label.clone())
                });
                let selector = format!("{}-{}", self.id, item.value);
                let mut seg = seg
                    .id(ElementId::Name(
                        format!("{:?}-{}", self.id, item.value).into(),
                    ))
                    .debug_selector(move || selector.clone())
                    .track_focus(&handle)
                    .children(icon_el)
                    .children(label);
                if item_disabled {
                    seg = seg.on_click(|_, _, cx| cx.stop_propagation());
                } else {
                    let mut seg_press = None;
                    if !is_selected {
                        // FR11: a held segment paints the pressed overlay under a finger as under
                        // a mouse; a drag from elsewhere lights the segment it would commit.
                        seg = seg.hover(move |s| s.bg(hover));
                        let press = crate::gestures::use_press_feedback(
                            ElementId::Name(format!("{:?}-{}-press", self.id, item.value).into()),
                            window,
                            cx,
                        );
                        seg = crate::press::touch_press_with(
                            seg,
                            &press,
                            gpui::transparent_black(),
                            pressed,
                            window,
                            cx,
                        );
                        seg_press = Some(press);
                        if drag_target == Some(i) {
                            seg = seg.bg(pressed);
                        }
                    }
                    // Upstream SegmentedControlItem's `useTouchHitArea`.
                    {
                        let (select, value) = (self.on_change.clone(), item.value.clone());
                        let stray = lens_state.clone();
                        seg = seg.relative().children(
                            crate::gestures::touch_hit_area::hit_area_with(
                                &ElementId::Name(format!("{:?}-{}", self.id, item.value).into()),
                                true,
                                Some(std::rc::Rc::new(move |window, cx| {
                                    // The lift already answered this press (a drag, or a cancel).
                                    if lens::take_stray_click(&stray, cx) {
                                        return;
                                    }
                                    if !is_selected && let Some(f) = &select {
                                        f(value.clone(), window, cx);
                                    }
                                })),
                                None,
                                crate::gestures::touch_hit_area::HitLinks {
                                    gestures: None,
                                    press: seg_press.as_ref(),
                                },
                                window,
                                cx,
                            ),
                        );
                    }
                    let select = self.on_change.clone();
                    let value = item.value.clone();
                    let stray = lens_state.clone();
                    seg = seg.cursor_pointer().on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        // The lift already answered this press (a drag, or a cancel).
                        if !event.is_keyboard() && lens::take_stray_click(&stray, cx) {
                            return;
                        }
                        if row_toggles(event)
                            && !is_selected
                            && let Some(f) = &select
                        {
                            f(value.clone(), window, cx);
                        }
                    });
                }
                if is_focus_visible(&handle, window) {
                    seg = seg.child(focus_ring(t, inner_radius, rem));
                }
                seg.role(Role::RadioButton)
                    .aria_disabled(item_disabled)
                    .aria_label(item.label)
                    .aria_toggled(if is_selected {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
            })
            .collect::<Vec<_>>();

        // Upstream: `inline-flex`, and `fit-content` so a stretching parent never
        // widens a hugging control.
        let mut el = div()
            .inline_flex()
            .flex_row()
            .items_center()
            .gap(t.spacing.s0_5)
            .p(t.spacing.s0_5)
            .rounded(t.radius.element)
            .bg(t.color.neutral);
        el = if fill {
            el.w_full()
        } else {
            el.flex_none().w_fit().max_w_full()
        };
        if self.disabled {
            el = el.opacity(0.5);
        }
        theme.refine(
            el.style(),
            "segmented-control",
            &[("size", size.as_str()), ("layout", self.layout.as_str())],
            None,
        );
        el.style().refine(&self.style);
        // The track's padding box, where the lens is placed, and the segments' boxes.
        let origin = Rc::new(std::cell::Cell::new(gpui::Point::<gpui::Pixels>::default()));
        let probe = {
            let (origin, track_bounds, context) =
                (origin.clone(), track_bounds.clone(), press_context.clone());
            gpui::canvas(
                move |bounds, _, _| {
                    origin.set(bounds.origin);
                    track_bounds.set(bounds);
                    context.borrow_mut().track = bounds;
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };
        let boxes = lens_state.read(cx).boxes.clone();
        let el = el
            .relative()
            .child(probe)
            .children(leaving_layer)
            .children(lens_layer)
            .children(segments)
            .on_children_prepainted(move |children, window, cx| {
                if lens::measure(&boxes, origin.get(), &children, count) {
                    window.defer(cx, |window, _| window.refresh());
                }
            })
            .id(self.id.clone())
            .on_key_down({
                // Escape cancels a press in progress: nothing commits (FR3).
                let (keys, lens) = (list.key_down_listener(), lens_state.clone());
                move |event, window, cx| {
                    if event.keystroke.key == "escape" && lens::pressing(&lens, cx) {
                        cx.stop_propagation();
                        lens::cancel(&lens, cx);
                        window.refresh();
                        return;
                    }
                    keys(event, window, cx)
                }
            })
            .role(Role::RadioGroup)
            .aria_label(self.label);
        let el = hint.wrap(el, window, cx);
        // A disabled control attaches nothing.
        let el = if presses {
            gesture_area(&attachment, el).into_any_element()
        } else {
            el
        };
        crate::overlay::disabled_reason(
            &self.id,
            self.disabled,
            self.disabled_message.as_ref(),
            false,
            el,
        )
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const SEGMENTED_CONTROL_DOC: ComponentDoc = ComponentDoc {
    name: "SegmentedControl",
    rust_type: "rustryx::components::segmented::SegmentedControl",
    props: &[
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("label", RustApi::Argument("label")),
        p("size", RustApi::Method("size")),
        p("layout", RustApi::Method("layout")),
        p("isDisabled", RustApi::Method("disabled")),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("children", RustApi::Method("item / items")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const SEGMENTED_CONTROL_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "SegmentedControlItem",
    rust_type: "rustryx::components::segmented::SegmentedControlItem",
    props: &[
        p("value", RustApi::Argument("value")),
        p("label", RustApi::Argument("label")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("icon", RustApi::Method("icon")),
        p("isDisabled", RustApi::Method("disabled")),
    ],
    example: include_str!("examples.rs"),
};
