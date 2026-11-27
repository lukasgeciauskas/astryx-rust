//! Screen reader announcements, upstream `useAnnounce`.
//!
//! Upstream writes messages into two visually hidden ARIA live regions on
//! `document.body`. Here [`announcer`] draws the two regions as AccessKit
//! nodes (a polite `status` and an assertive `alert`, live and atomic);
//! render it once at the root of each window. [`announce`] writes to them:
//!
//! - Messages written in the same frame are read together, in order; an
//!   identical repeat is kept once.
//! - The text clears 2 s later, so stale status text leaves the tree.

use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::Location;
use std::rc::Rc;
use std::time::Duration;

use accesskit::{Live, Role};
use gpui::{
    App, Bounds, Element, ElementId, Global, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, ParentElement as _, Pixels, SharedString, Style, Styled as _, Window, WindowId, div,
};

/// How urgently a message is read, upstream `AnnouncePoliteness`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Politeness {
    /// When the screen reader is idle. The default.
    #[default]
    Polite,
    /// Interrupts the current speech. For errors and time-critical alerts.
    Assertive,
}

/// How long a message stays in the tree, upstream `CLEAR_DELAY_MS`.
const CLEAR_DELAY: Duration = Duration::from_millis(2000);

#[derive(Default)]
struct Region {
    text: SharedString,
    /// Messages of the frame being built, joined into `text`.
    pending: Vec<SharedString>,
    generation: u64,
}

#[derive(Default)]
struct Channel {
    polite: Region,
    assertive: Region,
}

impl Channel {
    fn region(&mut self, politeness: Politeness) -> &mut Region {
        match politeness {
            Politeness::Polite => &mut self.polite,
            Politeness::Assertive => &mut self.assertive,
        }
    }
}

#[derive(Default)]
struct Channels(HashMap<WindowId, Rc<RefCell<Channel>>>);

impl Global for Channels {}

fn channel(window: &Window, cx: &mut App) -> Rc<RefCell<Channel>> {
    cx.default_global::<Channels>()
        .0
        .entry(window.window_handle().window_id())
        .or_default()
        .clone()
}

/// Reads `message` to screen reader users, upstream `announce(message,
/// {politeness})`. Needs an [`announcer`] in the window.
pub fn announce(
    message: impl Into<SharedString>,
    politeness: Politeness,
    window: &mut Window,
    cx: &mut App,
) {
    let message = message.into();
    if message.trim().is_empty() {
        return;
    }
    let channel = channel(window, cx);
    let weak = Rc::downgrade(&channel);
    let generation = {
        let mut channel = channel.borrow_mut();
        let region = channel.region(politeness);
        if !region.pending.contains(&message) {
            region.pending.push(message);
        }
        region.text = region.pending.join(" ").into();
        region.generation += 1;
        region.generation
    };
    window.refresh();

    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(CLEAR_DELAY).await;
            let Some(channel) = weak.upgrade() else {
                return;
            };
            let cleared = {
                let mut channel = channel.borrow_mut();
                let region = channel.region(politeness);
                if region.generation == generation {
                    region.text = SharedString::default();
                    region.pending.clear();
                    true
                } else {
                    false
                }
            };
            if cleared {
                cx.update(|window, _| window.refresh()).ok();
            }
        })
        .detach();
}

/// Marks a subtree as a live container: an element that delivers what is added inside
/// it to screen reader users itself (upstream's live containers, an ancestor with role
/// `log`, `status` or `alert`, or `aria-live`), so a component inside one leaves its
/// own announcement to it instead of saying the same words again. Provided with
/// [`provide_live_container`]; a layer opened from inside one (a popover, a dialog) is
/// outside it, as a portal is upstream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveContainer;

/// Marks `child` and its descendants in this layer as inside a [`LiveContainer`]. A
/// container with a live role renders its content through this.
#[track_caller]
pub fn provide_live_container(child: impl IntoElement, cx: &App) -> impl IntoElement {
    crate::context::provide_in_layer(LiveContainer, child, cx)
}

/// Whether the element rendering now sits inside a [`LiveContainer`] of its layer
/// (upstream's `closest(LIVE_CONTAINER_SELECTOR) != null`).
pub fn in_live_container(cx: &App) -> bool {
    crate::context::use_layer_context::<LiveContainer>(cx).is_some()
}

/// The text a live region holds now, for tests.
pub fn announcement(politeness: Politeness, window: &Window, cx: &mut App) -> SharedString {
    channel(window, cx)
        .borrow_mut()
        .region(politeness)
        .text
        .clone()
}

/// The two live regions of a window. Render it once, at the root.
#[track_caller]
pub fn announcer() -> impl IntoElement {
    div()
        .absolute()
        .size_0()
        .overflow_hidden()
        .child(LiveRegion {
            politeness: Politeness::Polite,
            text: SharedString::default(),
            source: Location::caller(),
        })
        .child(LiveRegion {
            politeness: Politeness::Assertive,
            text: SharedString::default(),
            source: Location::caller(),
        })
}

/// One live region: an empty box with an accessibility node.
struct LiveRegion {
    politeness: Politeness,
    text: SharedString,
    source: &'static Location<'static>,
}

impl Element for LiveRegion {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::Name(match self.politeness {
            Politeness::Polite => "astryx-live-region-polite".into(),
            Politeness::Assertive => "astryx-live-region-assertive".into(),
        }))
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        Some(self.source)
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // Drawing the region exposes the messages written since the last
        // frame; messages after it replace them.
        let channel = channel(window, cx);
        let mut channel = channel.borrow_mut();
        let region = channel.region(self.politeness);
        self.text = region.text.clone();
        region.pending.clear();
        drop(channel);
        (window.request_layout(Style::default(), [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn a11y_role(&self) -> Option<Role> {
        Some(match self.politeness {
            Politeness::Polite => Role::Status,
            Politeness::Assertive => Role::Alert,
        })
    }

    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        node.set_live(match self.politeness {
            Politeness::Polite => Live::Polite,
            Politeness::Assertive => Live::Assertive,
        });
        node.set_live_atomic();
        if !self.text.is_empty() {
            node.set_label(self.text.to_string());
        }
    }
}
