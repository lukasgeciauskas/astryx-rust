//! The source of an Outline's items, upstream `useOutlineFromDOM`: the headings with ids
//! inside a container. GPUI has no DOM to query, so the container collects them: each
//! Heading with an id inside it registers its level and text as it renders, and the
//! items are the ones the last frame collected.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{AnyElement, App, ElementId, IntoElement, RenderOnce, Window};
use rustryx_primitives::provide;

use super::OutlineItem;

/// Where a container's headings register this frame.
#[derive(Clone, Default)]
pub(crate) struct OutlineCollect(Rc<RefCell<Vec<OutlineItem>>>);

impl OutlineCollect {
    pub(crate) fn push(&self, item: OutlineItem) {
        self.0.borrow_mut().push(item);
    }
}

#[derive(Default)]
struct SourceState {
    collecting: OutlineCollect,
    items: Vec<OutlineItem>,
}

/// Upstream `useOutlineFromDOM`'s result, and the container it reads.
#[derive(Clone)]
pub struct OutlineSource {
    state: gpui::Entity<SourceState>,
}

/// Upstream `useOutlineFromDOM`: the outline of the headings inside the element that
/// [`OutlineSource::container`] wraps. A heading without an id is omitted; with no
/// container drawn the outline is empty.
pub fn use_outline_from_container(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> OutlineSource {
    let id: ElementId = id.into();
    OutlineSource {
        state: window.use_keyed_state(
            ElementId::Name(format!("{id}-outline-source").into()),
            cx,
            |_, _| SourceState::default(),
        ),
    }
}

impl OutlineSource {
    /// The items: the headings the container held last frame, in order.
    pub fn items(&self, cx: &App) -> Vec<OutlineItem> {
        self.state.read(cx).items.clone()
    }

    /// Wraps the element that holds the headings (upstream `containerRef`).
    pub fn container(&self, child: impl IntoElement) -> AnyElement {
        Container {
            state: self.state.clone(),
            child: child.into_any_element(),
        }
        .into_any_element()
    }
}

#[derive(IntoElement)]
struct Container {
    state: gpui::Entity<SourceState>,
    child: AnyElement,
}

impl RenderOnce for Container {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Last frame's collection is complete: it becomes the items, and this frame
        // collects anew. A change draws once more, so the Outline follows.
        let changed = self.state.update(cx, |s, _| {
            let collected = std::mem::take(&mut *s.collecting.0.borrow_mut());
            let changed = collected != s.items;
            if changed {
                s.items = collected;
            }
            changed
        });
        if changed {
            window.refresh();
        }
        let collect = self.state.read(cx).collecting.clone();
        provide(collect, self.child)
    }
}
