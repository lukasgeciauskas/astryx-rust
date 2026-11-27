//! ChatLayout, upstream `Chat/ChatLayout.tsx`: a full chat's shell. Messages scroll in
//! the page flow, following the newest one, and the composer stays docked at the bottom
//! with the scroll-to-bottom button above it.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::Refineable as _;
use gpui::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    ScrollHandle, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, canvas, div,
    px,
};
use rustryx_primitives::provide;
use rustryx_theme::current;
use smallvec::SmallVec;

use super::context::{ChatDensity, ChatLayoutContext, MessageRegistry};
use super::new_messages::use_chat_new_messages;
use super::scroll_button::ChatLayoutScrollButton;
use super::stream_scroll::{ChatScrollToBottomOptions, use_chat_stream_scroll};

/// Upstream `ChatLayout`.
///
/// ```ignore
/// ChatLayout::new("chat", ChatComposer::new("composer", |text, _, _| send(text)))
///     .child(ChatMessageList::new("log").children(messages))
/// ```
#[derive(IntoElement)]
pub struct ChatLayout {
    id: ElementId,
    composer: AnyElement,
    empty_state: Option<AnyElement>,
    /// `None`: the default button; `Some(None)`: none.
    scroll_button: Option<Option<AnyElement>>,
    scroll_handle: Option<ScrollHandle>,
    density: ChatDensity,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 2]>,
}

impl ChatLayout {
    /// Upstream `composer`: docked at the bottom.
    pub fn new(id: impl Into<ElementId>, composer: impl IntoElement) -> Self {
        ChatLayout {
            id: id.into(),
            composer: composer.into_any_element(),
            empty_state: None,
            scroll_button: None,
            scroll_handle: None,
            density: ChatDensity::Balanced,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `density`: the dock's padding and the message column's width.
        density: ChatDensity,
    }

    /// Upstream `emptyState`: shown while there are no children.
    pub fn empty_state(mut self, empty_state: impl IntoElement) -> Self {
        self.empty_state = Some(empty_state.into_any_element());
        self
    }

    /// Upstream `scrollButton`: replaces the default ChatLayoutScrollButton.
    pub fn scroll_button(mut self, button: impl IntoElement) -> Self {
        self.scroll_button = Some(Some(button.into_any_element()));
        self
    }

    /// Upstream `scrollButton={null}`: no scroll-to-bottom button.
    pub fn no_scroll_button(mut self) -> Self {
        self.scroll_button = Some(None);
        self
    }

    /// Upstream `scrollRef`: an ancestor scroller's handle. The layout then does not
    /// scroll itself; it follows the newest message in that scroller, and its dock sticks
    /// to that scroller's bottom.
    pub fn scroll_handle(mut self, handle: ScrollHandle) -> Self {
        self.scroll_handle = Some(handle);
        self
    }
}

impl Styled for ChatLayout {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ChatLayout {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ChatLayout {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let i18n = rustryx_i18n::current(cx);
        let self_scrolling = self.scroll_handle.is_none();
        let handle = self.scroll_handle.clone().unwrap_or_else(|| {
            window
                .use_keyed_state(
                    ElementId::Name(format!("{}-chat-scroll", self.id).into()),
                    cx,
                    |_, _| ScrollHandle::new(),
                )
                .read(cx)
                .clone()
        });
        let registry = window
            .use_keyed_state(
                ElementId::Name(format!("{}-chat-messages", self.id).into()),
                cx,
                |_, _| Rc::new(RefCell::new(MessageRegistry::default())),
            )
            .read(cx)
            .clone();
        // The messages register again as they render this frame.
        *registry.borrow_mut() = MessageRegistry::default();
        let scroll = use_chat_stream_scroll(
            self.id.clone(),
            handle.clone(),
            Default::default(),
            window,
            cx,
        )
        .with_messages(registry.clone());
        let news = use_chat_new_messages(self.id.clone(), window, cx);
        // The composer sits on the software keyboard (upstream `useChatKeyboard`).
        let keyboard = super::keyboard::use_chat_keyboard(
            &self.id,
            &scroll,
            &handle,
            self_scrolling,
            window,
            cx,
        );

        let scroll_button = match self.scroll_button {
            Some(button) => button,
            None => {
                let has_new = news.has_new_messages(cx);
                let visible = scroll.is_scrolled_up(cx) || has_new;
                let (scroll, news) = (scroll.clone(), news.clone());
                let mut button = ChatLayoutScrollButton::new(
                    ElementId::Name(format!("{}-scroll-button", self.id).into()),
                    visible,
                    move |_, window, cx| {
                        news.dismiss(cx);
                        scroll.scroll_to_bottom(ChatScrollToBottomOptions::default(), window, cx);
                    },
                );
                if has_new {
                    button = button.label(i18n.t("@astryx.chatLayout.newMessages"));
                }
                Some(button.into_any_element())
            }
        };

        let (area_max, area_pad, dock_inline, dock_block, blur) = match self.density {
            ChatDensity::Compact => (None, px(0.).into(), t.spacing.s2, t.spacing.s2, 80.),
            ChatDensity::Balanced => (None, px(0.).into(), t.spacing.s3, t.spacing.s3, 100.),
            ChatDensity::Spacious => (
                Some(px(800.)),
                t.spacing.s4,
                t.spacing.s4,
                t.spacing.s3,
                120.,
            ),
        };
        let mut root = div()
            .id(self.id.clone())
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0();
        if self_scrolling {
            root = root.overflow_y_scroll().track_scroll(&handle);
        }
        theme.refine(
            root.style(),
            "chat-layout",
            &[("density", self.density.as_str())],
            None,
        );
        root.style().refine(&self.style);

        let mut area = div()
            .debug_selector({
                let selector = format!("{}-messages", self.id);
                move || selector
            })
            .flex()
            .flex_col()
            .mx_auto()
            .flex_grow(1.)
            .flex_shrink_0()
            // The keyboard's lift adds to the end room, so the newest message clears it.
            .pb(t.spacing.s6.to_pixels(window.rem_size()) + px(keyboard.lift))
            .w_full()
            .max_w_full()
            .px(area_pad);
        if let Some(max) = area_max {
            area = area.max_w(max);
        }
        if keyboard.messages_y != 0. {
            area = area.translate(px(0.), px(keyboard.messages_y));
        }
        let area = if self.children.is_empty() && self.empty_state.is_some() {
            area.child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .min_h(px(200.))
                    .children(self.empty_state),
            )
        } else {
            area.children(self.children)
        };

        let mut dock_inner = div().mx_auto().w_full().max_w_full();
        if let Some(max) = area_max {
            dock_inner = dock_inner.max_w(max);
        }
        // The dock sticks to the scroller's bottom (fork patch R18). Its frosted layer is
        // the blur's height, masked in from its top edge as upstream (fork patch R3's
        // `mask_linear`); GPUI has no backdrop blur (carried to fork patch R2), so the
        // layer paints nothing yet, and a layer that paints nothing costs nothing.
        // The composer region is a keyboard target, and Done parks focus on it.
        let composer = rustryx_primitives::keyboard::provide_keyboard_target(
            keyboard.target,
            div()
                .debug_selector({
                    let selector = format!("{}-composer-region", self.id);
                    move || selector
                })
                .track_focus(&keyboard.region)
                .child(self.composer),
            cx,
        );
        let mut dock = div()
            .debug_selector({
                let selector = format!("{}-dock", self.id);
                move || selector
            })
            // The dock rests above the keyboard: its sticky offset is the lift.
            .sticky_bottom(px(keyboard.lift))
            .flex_shrink_0()
            .relative()
            .children(scroll_button)
            .child(
                div()
                    .debug_selector({
                        let selector = format!("{}-blur-layer", self.id);
                        move || selector
                    })
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(blur))
                    .mask_linear(blur_mask(self.density)),
            )
            .child(
                div()
                    .relative()
                    .px(dock_inline)
                    .pb(dock_block)
                    .child(dock_inner.child(composer)),
            );
        if keyboard.dock_y != 0. {
            dock = dock.translate(px(0.), px(keyboard.dock_y));
        }

        // After layout: the scroller is read, the spring steps, and a new last message
        // is noticed.
        let observer = {
            let (scroll, news, registry) = (scroll.clone(), news.clone(), registry.clone());
            canvas(
                move |_, window, cx| {
                    scroll.observe(window, cx);
                    let last = registry.borrow().ids.last().cloned();
                    let locked = scroll.is_locked(cx);
                    news.observe(last.as_ref(), locked, window, cx);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_0()
        };

        let context = ChatLayoutContext {
            scroll,
            messages: registry,
        };
        // A self-scrolling layout is its messages' scroller.
        let root = if self_scrolling {
            root.child(crate::gestures::scroller::scroll_begin(
                self.id.clone(),
                Some(&handle),
                (false, true),
            ))
            .child(area)
            .child(crate::gestures::scroller::scroll_end())
        } else {
            root.child(area)
        };
        provide(context, root.child(dock).child(observer))
    }
}

/// The frosted layer's mask, upstream `linear-gradient(to bottom, transparent, black <n>)`:
/// 24 px compact, 36 px balanced, 48 px spacious.
pub(crate) fn blur_mask(density: ChatDensity) -> gpui::LinearMask {
    let length = match density {
        ChatDensity::Compact => 24.,
        ChatDensity::Balanced => 36.,
        ChatDensity::Spacious => 48.,
    };
    rustryx_effects::edge_mask(
        rustryx_effects::MaskAxis::TopToBottom,
        Some(&rustryx_effects::edge_fade_stops(px(0.), px(length))),
        None,
    )
}

#[cfg(test)]
mod blur_mask_tests {
    use super::*;
    use gpui::MaskStop;

    #[test]
    fn the_frosted_layer_fades_in_from_its_top_edge_by_density() {
        for (density, length) in [
            (ChatDensity::Compact, 24.),
            (ChatDensity::Balanced, 36.),
            (ChatDensity::Spacious, 48.),
        ] {
            let mask = blur_mask(density);
            assert_eq!(mask.angle, 180., "to bottom");
            assert_eq!(mask.stops[0], MaskStop::px(px(0.), 0.));
            assert_eq!(mask.stops[1], MaskStop::px(px(length), 1.));
            assert_eq!(mask.stops[2], MaskStop::from_end(px(0.), 1.));
        }
    }
}
