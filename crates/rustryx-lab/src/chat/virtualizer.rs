//! `ChatVirtualizer`: a windowed message list for long transcripts (upstream lab
//! `ChatVirtualizer`): only the rows near the viewport render, and spacers stand for the
//! rest, so a thread of thousands of messages costs a viewport of elements.
//!
//! It owns the read position. By default it follows the newest message (the end mode); a
//! reader's scroll up past `end_threshold` of their own movement disengages it into the
//! anchor mode, which holds the row at the viewport's top in place while rows above it
//! measure, load or resize (a rotation, a width drag), and a scroll that lands within
//! `end_threshold` of the bottom re-engages follow. A [`ChatVirtualizerHandle`] declares
//! either mode. Rows are measured as they lay out, and the measurements price unmeasured
//! rows through running averages, per item type with `get_item_type`.
//!
//! Built the way GPUI's own `List` is: the content block is one element that renders the
//! window's rows in prepaint, lays each out as a root to measure it, runs the correction
//! pass against that layout ([`Machine`](super::virtualizer_engine::Machine)), and paints
//! the rows where the pass leaves them, so a correction lands in the frame that needs it.
//! The list writes no position while a finger scrolls (the finger owns it until its scroll
//! ends, momentum included). It renders its own focusable scroll container, or one content
//! block inside a container the caller owns (`scroll_element`, attach mode).

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::{Rc, Weak};

use gpui::prelude::*;
use gpui::{
    A11ySubtreeBuilder, AlignSelf, AnyElement, App, AvailableSpace, Bounds, DispatchPhase, Element,
    ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, MouseDownEvent, Pixels,
    Role, ScrollEvent, ScrollHandle, ScrollSource, SharedString, Style, StyleRefinement,
    Subscription, TouchPhase, Window, div, point, px, relative, size,
};
use rustryx_i18n::current as i18n;
use rustryx_primitives::gestures::finger_scroll::{
    FingerScrollWatch, is_finger_scroll, watch_finger_scrolls,
};
use rustryx_primitives::gestures::host::{GestureAttachment, gesture_area, gesture_host};
use rustryx_primitives::gestures::native_scroll::NativeScroller;
use rustryx_primitives::scroll_area::{
    ScrollAxis, ScrollKeyboardAccess, ScrollRole, UseScrollableAreaOptions, use_scrollable_area,
};

use super::virtualizer_engine::{Child, Dom, Machine, Props};
use super::virtualizer_model::Mode;

/// The size an unmeasured row is priced at (upstream `estimatedItemSize`).
pub enum ItemSize<T> {
    /// A static estimate, px, which the running averages refine. Bias it low: an estimate
    /// too small lets content grow below the reader as it measures, leaving the position
    /// valid; one too large makes it shrink, and the position clamps into a range that
    /// just got smaller.
    Fixed(f64),
    /// An authoritative estimate per row, never overridden by the averages.
    Fn(EstimateFn<T>),
}

/// A per-row estimate: the row and its index to its price, px.
pub type EstimateFn<T> = Rc<dyn Fn(&T, usize) -> f64>;

impl<T> Clone for ItemSize<T> {
    fn clone(&self) -> Self {
        match self {
            ItemSize::Fixed(v) => ItemSize::Fixed(*v),
            ItemSize::Fn(f) => ItemSize::Fn(f.clone()),
        }
    }
}

/// The imperative declarations (upstream `ChatVirtualizerHandle`): both declare where the
/// list should be rather than scroll; the next frame's pass converges on them. A
/// declaration made before the list first renders waits for it.
#[derive(Clone, Default)]
pub struct ChatVirtualizerHandle(Rc<RefCell<HandleInner>>);

#[derive(Default)]
struct HandleInner {
    machine: Weak<RefCell<Machine>>,
    pending: Option<Mode>,
}

impl ChatVirtualizerHandle {
    /// A handle to pass to [`ChatVirtualizer::handle`].
    pub fn new() -> Self {
        Self::default()
    }

    fn declare(&self, mode: Mode, window: &mut Window) {
        let mut inner = self.0.borrow_mut();
        match inner.machine.upgrade() {
            Some(machine) => {
                machine.borrow_mut().declare(mode);
                window.refresh();
            }
            None => inner.pending = Some(mode),
        }
    }

    /// The list rendering with this handle: it takes the declarations from now on.
    fn attach(&self, machine: &Rc<RefCell<Machine>>) {
        let mut inner = self.0.borrow_mut();
        inner.machine = Rc::downgrade(machine);
        if let Some(mode) = inner.pending.take() {
            machine.borrow_mut().declare(mode);
        }
    }

    /// Declares "be `px` above the bottom edge"; 0 follows the end (upstream
    /// `scrollToDistanceFromBottomPx`).
    pub fn scroll_to_distance_from_bottom_px(&self, px: f64, window: &mut Window) {
        self.declare(Mode::End { distance: px }, window);
    }

    /// Holds the row `key` at `viewport_offset` px from the viewport's top (0: its top edge
    /// at the viewport's top, the "pin the new prompt to the top" pattern; upstream
    /// `anchorToKey`). The position clamps to the scrollable range, and the key must be in
    /// the current data: a missing key falls back to following the end on the next pass.
    pub fn anchor_to_key(
        &self,
        key: impl Into<SharedString>,
        viewport_offset: f64,
        window: &mut Window,
    ) {
        self.declare(
            Mode::Anchor {
                key: key.into(),
                viewport_offset,
            },
            window,
        );
    }
}

type KeyFn<T> = Rc<dyn Fn(&T, usize) -> SharedString>;
type RenderFn<T> = Rc<dyn Fn(&T, usize, &mut Window, &mut App) -> AnyElement>;

/// Which scroller the list writes.
#[derive(Clone)]
enum Scroller {
    /// Its own (the default).
    Own(ScrollHandle),
    /// The caller's, or none yet (upstream's `scrollElement={null}`: attach mode waiting).
    Attach(Option<ScrollHandle>),
}

/// The windowed message list (upstream lab `ChatVirtualizer`).
///
/// ```ignore
/// ChatVirtualizer::new("thread", messages, |m, _| m.id.clone(), |m, _, _, _| row(m))
/// ```
#[derive(IntoElement)]
pub struct ChatVirtualizer<T: 'static> {
    id: ElementId,
    data: Rc<Vec<T>>,
    get_row_key: KeyFn<T>,
    render_item: RenderFn<T>,
    estimated: ItemSize<T>,
    handle: Option<ChatVirtualizerHandle>,
    get_item_type: Option<KeyFn<T>>,
    scroll_element: Option<Option<ScrollHandle>>,
    end_threshold: f64,
    overscan_top: f64,
    overscan_bottom: f64,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl<T: 'static> ChatVirtualizer<T> {
    /// A list of `data`, oldest first (upstream `data`: changes are told by identity, so
    /// replace the `Rc` rather than mutate it), each row identified by `get_row_key` (upstream
    /// `getRowKey`: stable, never derived from the index, since the size cache and the
    /// averages are keyed by it) and drawn by `render_item` (upstream `renderItem`).
    pub fn new(
        id: impl Into<ElementId>,
        data: impl Into<Rc<Vec<T>>>,
        get_row_key: impl Fn(&T, usize) -> SharedString + 'static,
        render_item: impl Fn(&T, usize, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        ChatVirtualizer {
            id: id.into(),
            data: data.into(),
            get_row_key: Rc::new(get_row_key),
            render_item: Rc::new(render_item),
            estimated: ItemSize::Fixed(120.),
            handle: None,
            get_item_type: None,
            scroll_element: None,
            end_threshold: 24.,
            overscan_top: 1600.,
            overscan_bottom: 600.,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `estimatedItemSize` (a fixed 120 px by default).
    pub fn estimated_item_size(mut self, size: ItemSize<T>) -> Self {
        self.estimated = size;
        self
    }

    /// Upstream `handleRef`: where the list takes its declarations.
    pub fn handle(mut self, handle: &ChatVirtualizerHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }

    /// Upstream `getItemType`: measured sizes feed a running average per type, so a
    /// corpus mixing tool-call stubs with long messages prices each shape honestly.
    pub fn get_item_type(mut self, f: impl Fn(&T, usize) -> SharedString + 'static) -> Self {
        self.get_item_type = Some(Rc::new(f));
        self
    }

    /// Upstream `scrollElement`, attach mode: the scroll container the caller owns (the
    /// handle its scroller tracks), one that runs no follow of its own; the list renders
    /// one content block into it. `None` is attach mode waiting for its scroller: only
    /// the spacers render, and no own container is made.
    pub fn scroll_element(mut self, handle: Option<&ScrollHandle>) -> Self {
        self.scroll_element = Some(handle.cloned());
        self
    }

    /// Upstream `endThreshold` (24 px): how close to the bottom a scroll must land to
    /// re-engage follow, and how far the reader's own upward movement disengages it.
    pub fn end_threshold(mut self, px: f64) -> Self {
        self.end_threshold = px;
        self
    }

    /// Upstream `overscanTop` (1600 px of rows rendered above the viewport).
    pub fn overscan_top(mut self, px: f64) -> Self {
        self.overscan_top = px;
        self
    }

    /// Upstream `overscanBottom` (600 px below it).
    pub fn overscan_bottom(mut self, px: f64) -> Self {
        self.overscan_bottom = px;
        self
    }

    /// Upstream `label`, own-container mode: the scroll container's accessible name, which
    /// keyboard users can focus and scroll ("Messages", localized, by default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl<T: 'static> Styled for ChatVirtualizer<T> {
    /// Upstream `style` and `className`, own-container mode: the scroll container.
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// A list's state that outlives frames: the machine, and what this layer adds around it.
pub(crate) struct ListState {
    pub(crate) machine: Rc<RefCell<Machine>>,
    shell: Rc<RefCell<Shell>>,
}

/// The frame-to-frame record the element keeps beside the machine.
#[derive(Default)]
pub(crate) struct Shell {
    /// The last frame's content block, for the events that precede this frame's commit.
    dom: Option<Dom>,
    /// The position the last frame left.
    last_scroll_top: Option<f64>,
    last_viewport: Option<(f64, f64)>,
    last_window: Option<(f64, f64)>,
    scroller: Option<(bool, ScrollHandle)>,
    watch: Option<FingerScrollWatch>,
    /// The finger's scroll ended since the last frame.
    flush_pending: Rc<RefCell<bool>>,
    keystrokes: Option<Subscription>,
    /// An area with no recognizers, so the window's gesture host hears the touches and
    /// scrolls the finger-scroll watch reads.
    attachment: Option<GestureAttachment>,
    /// For tests: each painted row's top relative to the viewport, and its height.
    #[cfg(test)]
    pub(crate) painted: Vec<(SharedString, f64, f64)>,
}

impl Shell {
    /// For tests: the content block's children as the last frame left them.
    #[cfg(test)]
    pub(crate) fn dom_children(&self) -> Vec<Child> {
        self.dom
            .as_ref()
            .map(|d| d.children.clone())
            .unwrap_or_default()
    }
}

/// A list's machine and shell.
#[cfg(test)]
type ListEntry = (Weak<RefCell<Machine>>, Weak<RefCell<Shell>>);

#[cfg(test)]
thread_local! {
    /// Each list's state by its id, for tests.
    static LISTS: RefCell<HashMap<String, ListEntry>> = RefCell::default();
}

/// A list's machine and shell, held.
#[cfg(test)]
pub(crate) type ListRefs = (Rc<RefCell<Machine>>, Rc<RefCell<Shell>>);

/// The machine and shell of the list `id` names, for tests.
#[cfg(test)]
pub(crate) fn list_of(id: &str) -> Option<ListRefs> {
    LISTS.with(|l| {
        let l = l.borrow();
        let (m, s) = l.get(id)?;
        Some((m.upgrade()?, s.upgrade()?))
    })
}

impl<T: 'static> RenderOnce for ChatVirtualizer<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(
            ElementId::Name(format!("{:?}-virtualizer", self.id).into()),
            cx,
            |_, _| ListState {
                machine: Rc::new(RefCell::new(Machine::default())),
                shell: Rc::new(RefCell::new(Shell::default())),
            },
        );
        let (machine, shell) = {
            let s = state.read(cx);
            (s.machine.clone(), s.shell.clone())
        };
        #[cfg(test)]
        LISTS.with(|l| {
            l.borrow_mut().insert(
                self.id.to_string(),
                (Rc::downgrade(&machine), Rc::downgrade(&shell)),
            )
        });
        if let Some(handle) = &self.handle {
            handle.attach(&machine);
        }
        let attachment = shell
            .borrow_mut()
            .attachment
            .get_or_insert_with(|| gesture_host(window, cx).attach(&[]))
            .clone();
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| i18n(cx).t("@astryx.chatVirtualizer.label"));
        let area = self.scroll_element.is_none().then(|| {
            use_scrollable_area(
                &ElementId::Name(format!("{:?}-scroller", self.id).into()),
                UseScrollableAreaOptions::new(
                    ScrollAxis::Block,
                    ScrollKeyboardAccess::Viewport {
                        label: label.clone(),
                        role: ScrollRole::Group,
                    },
                ),
                window,
                cx,
            )
        });
        let scroller = match (&self.scroll_element, &area) {
            (Some(attach), _) => Scroller::Attach(attach.clone()),
            (None, Some(area)) => Scroller::Own(area.scroll_handle().clone()),
            (None, None) => Scroller::Attach(None),
        };
        let block = Block {
            id: ElementId::Name(format!("{}-content", self.id).into()),
            list_id: self.id.to_string(),
            data: self.data,
            get_row_key: self.get_row_key,
            render_item: self.render_item,
            estimated: self.estimated,
            get_item_type: self.get_item_type,
            scroller,
            end_threshold: self.end_threshold,
            overscan_top: self.overscan_top,
            overscan_bottom: self.overscan_bottom,
            machine,
            shell,
        };
        match area {
            // Attach mode: one content block in the caller's scroller.
            None => gesture_area(&attachment, block).into_any_element(),
            Some(area) => {
                // Keyboard users reach and scroll the scroller, a group the scrollable
                // area names with this list's label.
                let selector = format!("{}-scroller", self.id);
                let mut viewport = area.viewport(
                    div()
                        .debug_selector(move || selector)
                        .relative()
                        .size_full(),
                );
                viewport.style().refine(&self.style);
                let viewport = viewport.child(area.content(div()).child(block));
                gesture_area(&attachment, viewport).into_any_element()
            }
        }
    }
}

/// The content block: the spacers and the windowed rows (upstream's content `div`).
struct Block<T: 'static> {
    id: ElementId,
    list_id: String,
    data: Rc<Vec<T>>,
    get_row_key: KeyFn<T>,
    render_item: RenderFn<T>,
    estimated: ItemSize<T>,
    get_item_type: Option<KeyFn<T>>,
    scroller: Scroller,
    end_threshold: f64,
    overscan_top: f64,
    overscan_bottom: f64,
    machine: Rc<RefCell<Machine>>,
    shell: Rc<RefCell<Shell>>,
}

/// What prepaint leaves for paint.
pub(crate) struct Painted {
    rows: Vec<AnyElement>,
    busy: bool,
    viewport: Option<Bounds<Pixels>>,
    page: bool,
}

/// The same scroll handle: GPUI's handle has no identity to compare, so a probe written
/// through one is read back through the other and then undone.
fn same_handle(a: &ScrollHandle, b: &ScrollHandle) -> bool {
    let saved = a.offset();
    let probe = point(saved.x + px(0.25), saved.y);
    a.set_offset(probe);
    let same = b.offset() == probe;
    a.set_offset(saved);
    same
}

fn key_of(id: &ElementId) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    "chat-virtualizer".hash(&mut hasher);
    hasher.finish()
}

impl<T: 'static> Element for Block<T> {
    type RequestLayoutState = ();
    type PrepaintState = Painted;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn a11y_role(&self) -> Option<Role> {
        Some(Role::GenericContainer)
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // As tall as the last pass left the block; a column-flex host must not shrink it.
        let height = self
            .shell
            .borrow()
            .dom
            .as_ref()
            .map_or(0., |d| d.block_height());
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(height as f32).into();
        style.flex_shrink = 0.;
        style.align_self = Some(AlignSelf::Stretch);
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Painted {
        let host = gesture_host(window, cx);
        let now = host.now();
        let data: Rc<dyn Any> = self.data.clone();
        let keys: Vec<SharedString> = self
            .data
            .iter()
            .enumerate()
            .map(|(i, item)| (self.get_row_key)(item, i))
            .collect();
        let types: Option<Vec<SharedString>> = self.get_item_type.as_ref().map(|f| {
            self.data
                .iter()
                .enumerate()
                .map(|(i, item)| f(item, i))
                .collect()
        });
        let estimate: Option<Box<dyn Fn(usize) -> f64>> = match &self.estimated {
            ItemSize::Fixed(_) => None,
            ItemSize::Fn(f) => {
                let (f, items) = (f.clone(), self.data.clone());
                Some(Box::new(move |i| f(&items[i], i)))
            }
        };
        let props = Props {
            data: &data,
            keys: &keys,
            types: types.as_deref(),
            fixed: match &self.estimated {
                ItemSize::Fixed(v) => Some(*v),
                ItemSize::Fn(_) => None,
            },
            estimate_fn: estimate.as_deref(),
            end_threshold: self.end_threshold,
            overscan_top: self.overscan_top,
            overscan_bottom: self.overscan_bottom,
        };

        let mut shell = self.shell.borrow_mut();
        let mut m = self.machine.borrow_mut();
        // The scroller this frame: its position as the scroller applied it, its viewport,
        // and whether it is the page's (a scroller spanning the whole window).
        let (attach, handle) = match &self.scroller {
            Scroller::Own(h) => (false, Some(h.clone())),
            Scroller::Attach(h) => (true, h.clone()),
        };
        let viewport = handle.as_ref().map(|h| h.bounds());
        let applied = handle.as_ref().map_or(0., |h| -f64::from(h.offset().y));
        let window_size = window.viewport_size();
        let page = attach
            && viewport.is_some_and(|v| v.origin == point(px(0.), px(0.)) && v.size == window_size);
        m.is_page_viewport = page;
        let replaced = match (&shell.scroller, &handle) {
            (Some((was_attach, old)), Some(new)) => *was_attach != attach || !same_handle(old, new),
            _ => false,
        };
        if replaced {
            m.on_scroller_replaced();
            shell.last_scroll_top = None;
            shell.last_viewport = None;
            shell.watch = None;
        }
        shell.scroller = handle.clone().map(|h| (attach, h));
        let scroller_id = handle
            .as_ref()
            .map_or(0, |_| key_of(&self.id) ^ u64::from(attach));

        // Core's finger-scroll watch on this scroller.
        let key = key_of(&self.id);
        if let (Some(h), Some(v)) = (&handle, viewport) {
            let mut native = NativeScroller::new(key, v, h.clone());
            native.scrolls_x = false;
            host.register_native_scroller(native);
            if shell.watch.is_none() {
                let flag = shell.flush_pending.clone();
                shell.watch = Some(watch_finger_scrolls(
                    &host,
                    key,
                    Some(Rc::new(move || *flag.borrow_mut() = true)),
                ));
            }
        }
        let finger = handle.is_some() && is_finger_scroll(&host, key, None);

        // The content block as the last frame left it, at this frame's position and size.
        let client = viewport.map_or(0., |v| f64::from(v.size.height));
        let max = handle
            .as_ref()
            .map_or(0., |h| f64::from(h.max_offset().y).max(0.));
        let layout_height = shell.dom.as_ref().map_or(0., |d| d.block_height());
        let origin = viewport.map_or(0., |v| f64::from(bounds.origin.y - v.origin.y) + applied);
        let below = if max > 0. {
            (client + max - origin - layout_height).max(0.)
        } else {
            0.
        };
        let mut dom = shell.dom.clone().unwrap_or_default();
        dom.spacers = m.spacers;
        dom.scroll_top = applied;
        dom.client_height = client;
        dom.origin = origin;
        dom.below = below;
        dom.attach = attach;
        dom.scroller = handle.is_some();

        // The frame's events, against the last commit: a resize, the container's new size,
        // a scroll, the finger's scroll ending; then the animation-frame callbacks.
        let win_size = (f64::from(window_size.width), f64::from(window_size.height));
        if page && shell.last_window.is_some_and(|w| w != win_size) {
            m.on_viewport_resize(now);
            // The viewport-sized box's entry.
            m.observe_container();
        }
        shell.last_window = Some(win_size);
        let vp_size = viewport.map(|v| (f64::from(v.size.width), f64::from(v.size.height)));
        if shell.last_viewport.is_some() && shell.last_viewport != vp_size {
            m.observe_container();
        }
        shell.last_viewport = vp_size;
        if let Some(last) = shell.last_scroll_top
            && (last - applied).abs() > 0.5
        {
            m.on_scroll(&mut dom, &props, now, finger);
        }
        if std::mem::take(&mut *shell.flush_pending.borrow_mut()) {
            m.flush_gesture(&mut dom, &props);
        }
        let tasks = std::mem::take(&mut m.tasks);
        m.run_frame_tasks(tasks, &mut dom, &props);

        // The commit: render the rows, measure them, run the pass; again while the pass
        // moves the window, as a re-render would.
        let width = bounds.size.width;
        let index_of: HashMap<&SharedString, usize> =
            keys.iter().enumerate().map(|(i, k)| (k, i)).collect();
        let mut built: HashMap<SharedString, (AnyElement, f64)> = HashMap::new();
        m.note_data(&props);
        for _ in 0..8 {
            let children = m.render_children(&props);
            let mut heights = HashMap::new();
            for child in &children {
                let Child::Row(row_key) = child else { continue };
                if !built.contains_key(row_key) {
                    let i = index_of[row_key];
                    let selector = format!("{}-row-{row_key}", self.list_id);
                    let mut element = div()
                        .id(ElementId::Name(format!("row-{row_key}").into()))
                        .debug_selector(move || selector)
                        .w(width)
                        .child((self.render_item)(&self.data[i], i, window, cx))
                        .into_any_element();
                    let laid = element.layout_as_root(
                        size(AvailableSpace::Definite(width), AvailableSpace::MinContent),
                        window,
                        cx,
                    );
                    built.insert(row_key.clone(), (element, f64::from(laid.height)));
                }
                let h = built[row_key].1;
                heights.insert(row_key.clone(), h);
                // A known row that changed size (the resize-observer path); new rows are
                // the pass's to measure.
                if m.sizes.contains_key(row_key) {
                    m.observe_row(row_key, h);
                }
            }
            dom.children = children;
            dom.heights = heights;
            if !m.commit(&mut dom, &props, now, scroller_id) {
                break;
            }
        }

        // Paint the rows where the pass left them.
        let scroll_top = dom.scroll_top();
        if let Some(h) = &handle
            && (scroll_top - applied).abs() > 0.01
        {
            h.set_offset(point(h.offset().x, px(-(scroll_top as f32))));
        }
        let shift = applied - scroll_top;
        let mut y = 0.;
        let mut rows = Vec::new();
        let mut focused = None;
        #[cfg(test)]
        shell.painted.clear();
        #[cfg(test)]
        let viewport_top = viewport.map_or(f64::from(bounds.origin.y), |v| f64::from(v.origin.y));
        for child in &dom.children {
            let h = dom.child_height(child);
            if let Child::Row(row_key) = child
                && let Some((mut element, _)) = built.remove(row_key)
            {
                let origin = point(bounds.origin.x, bounds.origin.y + px((y + shift) as f32));
                if element.prepaint_at(origin, window, cx).is_some() {
                    focused = Some(row_key.clone());
                }
                #[cfg(test)]
                shell
                    .painted
                    .push((row_key.clone(), f64::from(origin.y) - viewport_top, h));
                rows.push(element);
            }
            y += h;
        }
        if m.focused_key != focused {
            m.focused_key = focused;
            window.request_animation_frame();
        }
        shell.last_scroll_top = Some(scroll_top);
        m.spacers = dom.spacers;
        let busy = m.busy;
        if m.wants_frame() || *shell.flush_pending.borrow() {
            window.request_animation_frame();
        }
        if page && shell.keystrokes.is_none() {
            // A key ends the time in which a scroll counts as the platform's, even one
            // whose handler stops it.
            let weak = Rc::downgrade(&self.machine);
            let window_handle = window.window_handle();
            shell.keystrokes = Some(cx.observe_keystrokes(move |_, window, _| {
                if window.window_handle() == window_handle
                    && let Some(m) = weak.upgrade()
                    && let Ok(mut m) = m.try_borrow_mut()
                {
                    m.end_viewport_move();
                }
            }));
        }
        shell.dom = Some(dom);
        Painted {
            rows,
            busy,
            viewport: handle.as_ref().and(viewport),
            page,
        }
    }

    fn a11y_synthetic_children(&mut self, painted: &mut Painted, builder: &mut A11ySubtreeBuilder) {
        // History mounting into the log is not news: the block is busy for that commit
        // and the next frame.
        if painted.busy {
            builder.parent_node().set_busy();
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut (),
        painted: &mut Painted,
        window: &mut Window,
        cx: &mut App,
    ) {
        for row in &mut painted.rows {
            row.paint(window, cx);
        }
        let Some(viewport) = painted.viewport else {
            return;
        };
        // A wheel is an intent signal: it breaks convergence.
        let weak = Rc::downgrade(&self.machine);
        window.on_mouse_event(move |event: &ScrollEvent, phase, _, _| {
            if phase == DispatchPhase::Capture
                && event.source == ScrollSource::Pointer
                && viewport.contains(&event.position)
                && let Some(m) = weak.upgrade()
            {
                m.borrow_mut().on_wheel();
            }
        });
        if painted.page {
            // A press ends the time in which a scroll counts as the platform's.
            let weak = Rc::downgrade(&self.machine);
            window.on_mouse_event(move |_: &MouseDownEvent, phase, _, _| {
                if phase == DispatchPhase::Capture
                    && let Some(m) = weak.upgrade()
                {
                    m.borrow_mut().end_viewport_move();
                }
            });
            let weak = Rc::downgrade(&self.machine);
            window.on_touch_event(move |event, _, _| {
                if event.phase == TouchPhase::Started
                    && let Some(m) = weak.upgrade()
                {
                    m.borrow_mut().end_viewport_move();
                }
                false
            });
        }
    }
}
