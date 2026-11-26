//! Which press is inside a scroller, upstream `utils/scrollView.ts`'s `isInsideScrollView`: a box
//! the platform can scroll, overflowing by more than a pixel, around the pressed element.
//! GPUI has no DOM to walk, so each scrolling container marks its content: a
//! [`scroll_begin`] child before it and a [`scroll_end`] child after it (or a
//! [`ScrollerScope`] around it). While its content prepaints the scroller stands on a stack
//! the press surfaces inside read ([`in_scroller`]), and it registers itself with the
//! window's gesture host (`register_native_scroller`) for the recognizers' native-scroll
//! probes. An anchored or deferred overlay prepaints after its scroller: it is not inside.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};

use astryx_primitives::gestures::host::gesture_host;
use astryx_primitives::gestures::native_scroll::NativeScroller;
use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, ScrollHandle, Styled, Window, px,
};

/// How far a box must overflow before it counts as scrolling, px.
const OVERFLOW_TOLERANCE: f32 = 1.;

thread_local! {
    /// The scrollers around the element prepainting now: whether each scrolls, and where it
    /// shows.
    static STACK: RefCell<Vec<(bool, Option<Bounds<Pixels>>)>> = const { RefCell::new(Vec::new()) };
}

/// Whether the element prepainting now sits inside a scroller that scrolls.
pub fn in_scroller() -> bool {
    STACK.with(|s| s.borrow().iter().any(|(scrolls, _)| *scrolls))
}

/// Where the innermost scroller that scrolls around the element prepainting now shows.
pub fn innermost_scroller() -> Option<Bounds<Pixels>> {
    STACK.with(|s| {
        s.borrow()
            .iter()
            .rev()
            .find(|(scrolls, _)| *scrolls)
            .and_then(|(_, bounds)| *bounds)
    })
}

/// Whether a scroller with this handle scrolls on these axes now.
fn overflows(handle: Option<&ScrollHandle>, (x, y): (bool, bool)) -> bool {
    match handle {
        // A box that never scrolls marks a scroller's place with no axes.
        None => x || y,
        Some(handle) => {
            let max = handle.max_offset();
            (x && max.x > px(OVERFLOW_TOLERANCE)) || (y && max.y > px(OVERFLOW_TOLERANCE))
        }
    }
}

fn key_of(id: &ElementId) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

/// The start of a scroller's content: a child of the scrolling element placed before its
/// content (absolute, so it takes no room). `axes` are (x, y); a box that does not scroll
/// now passes none, so a [`scroll_end`] after its content still balances.
pub fn scroll_begin(
    id: impl Into<ElementId>,
    handle: Option<&ScrollHandle>,
    axes: (bool, bool),
) -> impl IntoElement {
    let (key, handle) = (key_of(&id.into()), handle.cloned());
    gpui::canvas(
        move |bounds, window, cx| {
            let _ = bounds;
            // Inside the scroller, the clip is the scroller's own box.
            let mut shown = window.content_mask().bounds;
            if let Some(handle) = &handle {
                let viewport = handle.bounds();
                if viewport.size.width > px(0.) {
                    shown = viewport;
                }
                let mut scroller = NativeScroller::new(key, shown, handle.clone());
                scroller.scrolls_x = axes.0;
                scroller.scrolls_y = axes.1;
                gesture_host(window, cx).register_native_scroller(scroller);
            }
            STACK.with(|s| {
                s.borrow_mut()
                    .push((overflows(handle.as_ref(), axes), Some(shown)))
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0()
}

/// The end of a scroller's content: a child after it.
pub fn scroll_end() -> impl IntoElement {
    gpui::canvas(
        |_, _, _| {
            STACK.with(|s| {
                s.borrow_mut().pop();
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0()
}

/// A scrolling container: wrap the element that scrolls. With its `ScrollHandle`, it counts
/// as scrolling only while its content overflows; without one, always.
pub struct ScrollerScope {
    id: ElementId,
    handle: Option<ScrollHandle>,
    axes: (bool, bool),
    child: AnyElement,
}

/// Wraps `child`, a container that scrolls on the vertical axis.
pub fn scroller_scope(
    id: impl Into<ElementId>,
    handle: Option<&ScrollHandle>,
    child: impl IntoElement,
) -> ScrollerScope {
    ScrollerScope {
        id: id.into(),
        handle: handle.cloned(),
        axes: (false, true),
        child: child.into_any_element(),
    }
}

impl ScrollerScope {
    /// Which axes it scrolls (x, y).
    pub fn axes(mut self, x: bool, y: bool) -> Self {
        self.axes = (x, y);
        self
    }

    fn scrolls(&self) -> bool {
        overflows(self.handle.as_ref(), self.axes)
    }
}

impl Element for ScrollerScope {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let scrolls = self.scrolls();
        if let Some(handle) = &self.handle {
            let mut scroller = NativeScroller::new(key_of(&self.id), bounds, handle.clone());
            scroller.scrolls_x = self.axes.0;
            scroller.scrolls_y = self.axes.1;
            gesture_host(window, cx).register_native_scroller(scroller);
        }
        STACK.with(|s| s.borrow_mut().push((scrolls, Some(bounds))));
        self.child.prepaint(window, cx);
        STACK.with(|s| {
            s.borrow_mut().pop();
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

/// A scroll handle kept for the container `id` names, so its overflow can be read: track it on
/// the container and hand it to [`scroll_begin`].
pub fn use_scroll_handle(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> ScrollHandle {
    let id = id.into();
    window
        .use_keyed_state(
            ElementId::Name(format!("{id:?}-scroller").into()),
            cx,
            |_, _| ScrollHandle::new(),
        )
        .read(cx)
        .clone()
}
