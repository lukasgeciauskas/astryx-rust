//! Tree-scoped context values, the GPUI counterpart of React context.
//!
//! GPUI has no context mechanism of its own. A [`Provider`] pushes its value
//! onto a per-type stack while its child runs `request_layout`, `prepaint`
//! and `paint`, and pops it after. A [`RenderOnce`](gpui::RenderOnce)
//! component renders during its parent's `request_layout`, and list elements
//! render their items during `prepaint`, so every descendant that reads the
//! context in its render sees the nearest provider's value.
//!
//! Event handlers run outside the draw phases. Read the context during
//! render and capture the value in the handler; do not read it inside the
//! handler.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::panic::Location;

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, Global, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Window,
};

#[derive(Default)]
struct ContextStacks(HashMap<TypeId, Vec<Box<dyn Any>>>);

impl Global for ContextStacks {}

fn push<T: Clone + 'static>(value: &T, cx: &mut App) {
    cx.default_global::<ContextStacks>()
        .0
        .entry(TypeId::of::<T>())
        .or_default()
        .push(Box::new(value.clone()));
}

fn pop<T: 'static>(cx: &mut App) {
    let stack = cx
        .default_global::<ContextStacks>()
        .0
        .get_mut(&TypeId::of::<T>());
    let popped = stack.and_then(Vec::pop);
    debug_assert!(popped.is_some(), "context stack underflow");
}

/// Returns the value of the nearest [`Provider<T>`] above the element that is
/// rendering, or `None` outside every provider of `T`.
pub fn use_context<T: Clone + 'static>(cx: &App) -> Option<T> {
    cx.try_global::<ContextStacks>()?
        .0
        .get(&TypeId::of::<T>())?
        .last()?
        .downcast_ref::<T>()
        .cloned()
}

/// Runs `f` with `value` provided, for code that renders elements directly.
pub fn with_context<T: Clone + 'static, R>(
    value: &T,
    cx: &mut App,
    f: impl FnOnce(&mut App) -> R,
) -> R {
    push(value, cx);
    let result = f(cx);
    pop::<T>(cx);
    result
}

/// Provides `value` to every descendant of `child`.
#[track_caller]
pub fn provide<T: Clone + 'static>(value: T, child: impl IntoElement) -> Provider<T> {
    Provider {
        value,
        child: child.into_any_element(),
        source: Location::caller(),
    }
}

/// An element that provides a context value to its child. It has no layout
/// of its own: it takes its child's box.
pub struct Provider<T> {
    value: T,
    child: AnyElement,
    source: &'static Location<'static>,
}

impl<T: Clone + 'static> Element for Provider<T> {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        Some(self.source)
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        push(&self.value, cx);
        let layout_id = self.child.request_layout(window, cx);
        pop::<T>(cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        push(&self.value, cx);
        self.child.prepaint(window, cx);
        pop::<T>(cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        push(&self.value, cx);
        self.child.paint(window, cx);
        pop::<T>(cx);
    }
}
