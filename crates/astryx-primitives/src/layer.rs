//! Layers: the stack every floating surface joins (popovers, menus, dialogs,
//! hover cards), upstream `Layer/layerStack.ts`, `useLayerDismissal` and
//! `outsidePress`.
//!
//! [`layer`] draws its content above the rest of the window (a GPUI
//! `deferred` draw, the counterpart of a portal) and puts it on the window's
//! layer stack while it is drawn. The stack orders layers by nesting depth,
//! then by when they opened, and the topmost one:
//!
//! - closes on Escape (`EscapeBehavior::Close`), or swallows it (`Block`);
//! - closes on a press outside it and outside the layers nested in it.
//!
//! [`anchored_layer`] wraps a trigger without changing its layout and draws a
//! layer on one side of it, placed by [`crate::anchored::position_layer`].
//! Focus trapping and entrance motion come with the components that need
//! them (Popover, Dialog, Tooltip).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::panic::Location;
use std::rc::{Rc, Weak};

use gpui::{
    Anchor, AnyElement, App, Bounds, DispatchPhase, Element, ElementId, Global, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, MouseDownEvent, ParentElement as _, Pixels, Point,
    Styled as _, Subscription, Window, WindowId, anchored, deferred,
};

use crate::anchored::{LayerOptions, LayerPosition, position_layer};
use crate::{provide, use_context};

/// What Escape does to the topmost layer, upstream `LayerEscapeBehavior`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EscapeBehavior {
    /// Dismisses it.
    #[default]
    Close,
    /// Keeps it open and stops Escape there, for a layer that must be
    /// answered (an alert dialog).
    Block,
}

type Dismiss = Rc<dyn Fn(&mut Window, &mut App)>;

struct Entry {
    depth: usize,
    seq: u64,
    escape: Cell<EscapeBehavior>,
    dismiss: RefCell<Dismiss>,
    /// Where it was drawn last frame, in window coordinates.
    bounds: Cell<Option<Bounds<Pixels>>>,
    /// Pressing these does not count as outside (the trigger).
    inside: RefCell<Vec<Bounds<Pixels>>>,
}

#[derive(Default)]
struct WindowStack {
    entries: Vec<Weak<Entry>>,
    next_seq: u64,
    _escape: Option<Subscription>,
}

impl WindowStack {
    fn live(&mut self) -> Vec<Rc<Entry>> {
        self.entries.retain(|entry| entry.strong_count() > 0);
        self.entries.iter().filter_map(Weak::upgrade).collect()
    }

    fn topmost(&mut self) -> Option<Rc<Entry>> {
        self.live().into_iter().max_by_key(|e| (e.depth, e.seq))
    }
}

#[derive(Default)]
struct LayerStacks(HashMap<WindowId, Rc<RefCell<WindowStack>>>);

impl Global for LayerStacks {}

fn window_stack(window: &Window, cx: &mut App) -> Rc<RefCell<WindowStack>> {
    let id = window.window_handle().window_id();
    let stacks = cx.default_global::<LayerStacks>();
    if let Some(stack) = stacks.0.get(&id) {
        return stack.clone();
    }
    let stack = Rc::new(RefCell::new(WindowStack::default()));
    stacks.0.insert(id, stack.clone());
    let weak = Rc::downgrade(&stack);
    let handle = window.window_handle();
    let subscription = cx.observe_keystrokes(move |event, window, cx| {
        if window.window_handle() != handle || event.keystroke.key != "escape" {
            return;
        }
        if event.keystroke.modifiers.modified() {
            return;
        }
        let Some(stack) = weak.upgrade() else { return };
        let top = stack.borrow_mut().topmost();
        if let Some(top) = top
            && top.escape.get() == EscapeBehavior::Close
        {
            let dismiss = top.dismiss.borrow().clone();
            dismiss(window, cx);
        }
    });
    stack.borrow_mut()._escape = Some(subscription);
    stack
}

/// The least depth of a modal layer, above any non-modal nesting.
const MODAL_DEPTH: usize = 100;

/// The nesting depth a layer gives its content.
#[derive(Clone, Copy)]
pub(crate) struct LayerDepth(pub(crate) usize);

/// Whether a layer is open in `window`, upstream `hasOpenLayer`.
pub fn has_open_layer(window: &Window, cx: &App) -> bool {
    cx.try_global::<LayerStacks>()
        .and_then(|stacks| stacks.0.get(&window.window_handle().window_id()))
        .is_some_and(|stack| !stack.borrow_mut().live().is_empty())
}

/// Draws `content`, the layer's surface, as a layer that `on_dismiss`
/// closes. Keep rendering it while the layer is open; stop to close it.
///
/// Place the surface with [`Layer::anchor`], or give it absolute
/// positioning itself (a full-window dialog), so it takes no space in the
/// flow. Presses outside the surface's box dismiss.
#[track_caller]
pub fn layer(
    content: impl IntoElement,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> Layer {
    Layer {
        content: Some(content.into_any_element()),
        dismiss: Rc::new(on_dismiss),
        escape: EscapeBehavior::Close,
        inside: Vec::new(),
        anchor: None,
        source: Location::caller(),
        depth: 0,
        min_depth: 0,
        light_dismiss: true,
    }
}

/// The element [`layer`] returns.
pub struct Layer {
    content: Option<AnyElement>,
    dismiss: Dismiss,
    escape: EscapeBehavior,
    inside: Vec<Bounds<Pixels>>,
    anchor: Option<(Point<Pixels>, Anchor)>,
    source: &'static Location<'static>,
    depth: usize,
    min_depth: usize,
    light_dismiss: bool,
}

impl Layer {
    /// Places the surface's `corner` at `position` in window coordinates,
    /// moved to stay inside the window (GPUI `anchored`).
    pub fn anchor(mut self, position: Point<Pixels>, corner: Anchor) -> Self {
        self.anchor = Some((position, corner));
        self
    }

    /// What Escape does while this layer is topmost.
    pub fn escape(mut self, behavior: EscapeBehavior) -> Self {
        self.escape = behavior;
        self
    }

    /// Whether a press outside the layer dismisses it; on by default. A
    /// layer that covers the window takes its own presses.
    pub fn light_dismiss(mut self, light_dismiss: bool) -> Self {
        self.light_dismiss = light_dismiss;
        self
    }

    /// A modal layer (a dialog): it draws above every layer that opened
    /// before it, and layers opened inside it draw above it.
    pub fn modal(mut self) -> Self {
        self.min_depth = MODAL_DEPTH;
        self
    }

    /// Areas that do not count as outside, such as the trigger, in window
    /// coordinates.
    pub fn inside(mut self, bounds: impl IntoIterator<Item = Bounds<Pixels>>) -> Self {
        self.inside.extend(bounds);
        self
    }
}

struct Registration(Rc<Entry>);

/// A layer's place on the stack, kept between its prepaint and paint.
pub struct LayerState(Rc<Entry>);

/// Draws the content and records where it went.
struct Measured {
    entry: Rc<Entry>,
    child: AnyElement,
}

impl Element for Measured {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.entry
            .bounds
            .set(Some(window.element_transform().apply_bounds(bounds)));
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

impl Element for Layer {
    type RequestLayoutState = AnyElement;
    type PrepaintState = LayerState;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::CodeLocation(*self.source))
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        Some(self.source)
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        self.depth = use_context::<LayerDepth>(cx)
            .map_or(1, |d| d.0 + 1)
            .max(self.min_depth);
        let id = id.expect("layers have an element id");
        let entry = window.with_element_state::<Registration, _>(id, |registration, window| {
            let registration = registration.unwrap_or_else(|| {
                let stack = window_stack(window, cx);
                let mut stack = stack.borrow_mut();
                let entry = Rc::new(Entry {
                    depth: self.depth,
                    seq: stack.next_seq,
                    escape: Cell::new(self.escape),
                    dismiss: RefCell::new(self.dismiss.clone()),
                    bounds: Cell::new(None),
                    inside: RefCell::new(Vec::new()),
                });
                stack.next_seq += 1;
                stack.entries.push(Rc::downgrade(&entry));
                Registration(entry)
            });
            let entry = registration.0.clone();
            entry.escape.set(self.escape);
            *entry.dismiss.borrow_mut() = self.dismiss.clone();
            *entry.inside.borrow_mut() = std::mem::take(&mut self.inside);
            (entry, registration)
        });
        let content = self.content.take().expect("a layer is drawn once");
        let surface = Measured {
            entry,
            child: content,
        };
        let placed = match self.anchor {
            Some((position, corner)) => anchored()
                .position(position)
                .anchor(corner)
                .snap_to_window()
                .child(surface)
                .into_any_element(),
            None => surface.into_any_element(),
        };
        let mut child = deferred(provide(LayerDepth(self.depth), placed))
            .with_priority(self.depth)
            .into_any_element();
        let layout_id = child.request_layout(window, cx);
        (layout_id, child)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> LayerState {
        child.prepaint(window, cx);
        let id = id.expect("layers have an element id");
        window.with_element_state::<Registration, _>(id, |registration, _| {
            let registration = registration.expect("registered in request_layout");
            (LayerState(registration.0.clone()), registration)
        })
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        state: &mut LayerState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let stack = window_stack(window, cx);
        let entry = Rc::downgrade(&state.0);
        let light_dismiss = self.light_dismiss;
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture || !light_dismiss {
                return;
            }
            let Some(entry) = entry.upgrade() else { return };
            let live = stack.borrow_mut().live();
            let Some(top) = live.iter().max_by_key(|e| (e.depth, e.seq)) else {
                return;
            };
            if !Rc::ptr_eq(top, &entry) {
                return;
            }
            // Inside this layer, or a layer above it in the stack, or a
            // registered trigger: not outside.
            let position = event.position;
            let contains = |e: &Rc<Entry>| e.bounds.get().is_some_and(|b| b.contains(&position));
            if contains(&entry) || entry.inside.borrow().iter().any(|b| b.contains(&position)) {
                return;
            }
            let dismiss = entry.dismiss.borrow().clone();
            dismiss(window, cx);
        });
        child.paint(window, cx);
    }
}

// ----------------------------------------------------------------------------
// Anchored layers

type PlacedFn = Rc<dyn Fn(LayerPosition, &mut Window, &mut App)>;
/// Places a layer: the anchor's bounds, the layer's size and the viewport,
/// with the layer's options.
pub type PositionFn =
    Rc<dyn Fn(Bounds<Pixels>, gpui::Size<Pixels>, Bounds<Pixels>, &LayerOptions) -> LayerPosition>;

/// A trigger and, while open, a layer on one side of it. See
/// [`anchored_layer`].
pub struct AnchoredLayer {
    id: ElementId,
    trigger: AnyElement,
    surface: Option<(AnyElement, Dismiss, LayerOptions)>,
    escape: EscapeBehavior,
    on_placed: Option<PlacedFn>,
    position: Option<PositionFn>,
    depth: usize,
    light_dismiss: bool,
    match_anchor_width: bool,
}

/// Wraps `trigger` without changing its layout: it is laid out as if
/// unwrapped, and its bounds anchor the layer [`AnchoredLayer::surface`]
/// opens. The layer joins the window's layer stack like [`layer`]: Escape
/// and presses outside it and its trigger call its `on_dismiss`.
pub fn anchored_layer(id: impl Into<ElementId>, trigger: impl IntoElement) -> AnchoredLayer {
    AnchoredLayer {
        id: id.into(),
        trigger: trigger.into_any_element(),
        surface: None,
        escape: EscapeBehavior::Close,
        on_placed: None,
        position: None,
        depth: 0,
        light_dismiss: true,
        match_anchor_width: false,
    }
}

impl AnchoredLayer {
    /// Opens `content` as a layer next to the trigger. Keep passing it while
    /// the layer is open; stop to close it.
    pub fn surface(
        mut self,
        content: impl IntoElement,
        options: LayerOptions,
        on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.surface = Some((content.into_any_element(), Rc::new(on_dismiss), options));
        self
    }

    /// What Escape does while this layer is topmost.
    pub fn escape(mut self, behavior: EscapeBehavior) -> Self {
        self.escape = behavior;
        self
    }

    /// Whether a press outside the layer and its trigger dismisses it
    /// (upstream `hasLightDismiss`); on by default.
    pub fn light_dismiss(mut self, light_dismiss: bool) -> Self {
        self.light_dismiss = light_dismiss;
        self
    }

    /// Makes the layer at least as wide as its trigger.
    pub fn match_anchor_width(mut self, match_width: bool) -> Self {
        self.match_anchor_width = match_width;
        self
    }

    /// Places the layer with `f` instead of [`position_layer`]: a Selector
    /// puts its selected option over the trigger.
    pub fn position_with(
        mut self,
        f: impl Fn(Bounds<Pixels>, gpui::Size<Pixels>, Bounds<Pixels>, &LayerOptions) -> LayerPosition
        + 'static,
    ) -> Self {
        self.position = Some(Rc::new(f));
        self
    }

    /// Told where the layer landed each frame (the side, after any flip).
    pub fn on_placed(mut self, f: impl Fn(LayerPosition, &mut Window, &mut App) + 'static) -> Self {
        self.on_placed = Some(Rc::new(f));
        self
    }
}

/// What an anchored layer keeps from layout to prepaint.
pub struct AnchoredState {
    surface: Option<(AnyElement, Rc<Entry>, LayerOptions)>,
}

impl Element for AnchoredLayer {
    type RequestLayoutState = AnchoredState;
    type PrepaintState = Option<LayerState>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnchoredState) {
        // Transparent: the trigger's node stands in the parent's layout.
        let layout_id = self.trigger.request_layout(window, cx);
        let Some((content, dismiss, options)) = self.surface.take() else {
            return (layout_id, AnchoredState { surface: None });
        };
        self.depth = use_context::<LayerDepth>(cx).map_or(1, |d| d.0 + 1);
        let id = id.expect("anchored layers have an element id");
        let escape = self.escape;
        let depth = self.depth;
        let entry = window.with_element_state::<Registration, _>(id, |registration, window| {
            let registration = registration.unwrap_or_else(|| {
                let stack = window_stack(window, cx);
                let mut stack = stack.borrow_mut();
                let entry = Rc::new(Entry {
                    depth,
                    seq: stack.next_seq,
                    escape: Cell::new(escape),
                    dismiss: RefCell::new(dismiss.clone()),
                    bounds: Cell::new(None),
                    inside: RefCell::new(Vec::new()),
                });
                stack.next_seq += 1;
                stack.entries.push(Rc::downgrade(&entry));
                Registration(entry)
            });
            let entry = registration.0.clone();
            entry.escape.set(escape);
            *entry.dismiss.borrow_mut() = dismiss.clone();
            (entry, registration)
        });
        let surface = provide(
            LayerDepth(depth),
            Measured {
                entry: entry.clone(),
                child: content,
            },
        )
        .into_any_element();
        (
            layout_id,
            AnchoredState {
                surface: Some((surface, entry, options)),
            },
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut AnchoredState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<LayerState> {
        self.trigger.prepaint(window, cx);
        let (mut surface, entry, options) = state.surface.take()?;
        // The trigger is never outside its own layer.
        *entry.inside.borrow_mut() = vec![window.element_transform().apply_bounds(bounds)];
        if self.match_anchor_width {
            surface = gpui::div()
                .min_w(bounds.size.width)
                .child(surface)
                .into_any_element();
        }
        let measured = surface.layout_as_root(
            gpui::size(
                gpui::AvailableSpace::MaxContent,
                gpui::AvailableSpace::MaxContent,
            ),
            window,
            cx,
        );
        let viewport = Bounds::new(Point::default(), window.viewport_size());
        let position = match &self.position {
            Some(place) => place(bounds, measured, viewport, &options),
            None => position_layer(bounds, measured, viewport, &options),
        };
        if let Some(f) = &self.on_placed {
            f(position, window, cx);
        }
        window.defer_draw(
            surface,
            position.origin,
            options.priority.max(self.depth),
            None,
        );
        Some(LayerState(entry))
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut AnchoredState,
        entry: &mut Option<LayerState>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(entry) = entry.as_ref()
            && self.light_dismiss
        {
            dismiss_on_outside_press(&entry.0, window, cx);
        }
        self.trigger.paint(window, cx);
    }
}

/// Closes the layer of `entry` on a press outside it, its trigger, and the
/// layers above it, while it is topmost.
fn dismiss_on_outside_press(entry: &Rc<Entry>, window: &mut Window, cx: &mut App) {
    let stack = window_stack(window, cx);
    let entry = Rc::downgrade(entry);
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture {
            return;
        }
        let Some(entry) = entry.upgrade() else { return };
        let live = stack.borrow_mut().live();
        let Some(top) = live.iter().max_by_key(|e| (e.depth, e.seq)) else {
            return;
        };
        if !Rc::ptr_eq(top, &entry) {
            return;
        }
        let position = event.position;
        let inside = entry.bounds.get().is_some_and(|b| b.contains(&position))
            || entry.inside.borrow().iter().any(|b| b.contains(&position));
        if inside {
            return;
        }
        let dismiss = entry.dismiss.borrow().clone();
        dismiss(window, cx);
    });
}
