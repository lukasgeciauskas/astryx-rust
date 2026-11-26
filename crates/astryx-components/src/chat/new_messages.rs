//! useChatNewMessages, upstream `Chat/useChatNewMessages.ts`: notices a message that
//! arrives while the reader is scrolled away from the newest one.

use gpui::{App, ElementId, Entity, Window};

#[derive(Default)]
struct NewMessagesState {
    has_new: bool,
    last: Option<ElementId>,
}

/// Upstream `UseChatNewMessagesReturn`.
#[derive(Clone)]
pub struct ChatNewMessages {
    state: Entity<NewMessagesState>,
}

/// Upstream `useChatNewMessages`. ChatLayout feeds it its newest message each frame; a
/// custom layout calls [`ChatNewMessages::observe`] with its own.
pub fn use_chat_new_messages(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> ChatNewMessages {
    let id = id.into();
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-chat-new-messages").into()),
        cx,
        |_, _| NewMessagesState::default(),
    );
    ChatNewMessages { state }
}

impl ChatNewMessages {
    /// Upstream `hasNewMessages`.
    pub fn has_new_messages(&self, cx: &App) -> bool {
        self.state.read(cx).has_new
    }

    /// Upstream `dismiss`.
    pub fn dismiss(&self, cx: &mut App) {
        self.state.update(cx, |s, _| s.has_new = false);
    }

    /// Upstream's content observer: the newest message is `last`; one that changed while
    /// the view is not following (`locked` false) is new.
    pub fn observe(
        &self,
        last: Option<&ElementId>,
        locked: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let changed = self.state.update(cx, |s, _| {
            if last.is_none() || s.last.as_ref() == last {
                return false;
            }
            s.last = last.cloned();
            if !locked && !s.has_new {
                s.has_new = true;
                return true;
            }
            false
        });
        if changed {
            window.refresh();
        }
    }
}
