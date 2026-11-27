//! ChatMessageList, upstream `Chat/ChatMessageList.tsx`: the conversation's log of
//! messages, with its density, spacing and older-message loading.

use std::rc::Rc;

use gpui::Refineable as _;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, Task, Window, canvas, div,
};
use rustryx_primitives::{
    control_focus, is_focus_visible, provide_in_layer, provide_live_container,
};
use rustryx_theme::current;
use smallvec::SmallVec;

use super::context::{ChatDensity, ChatListContext, use_chat_layout_context};
use crate::focus::focus_ring_inset;
use crate::layout::SpacingStep;
use crate::status::Spinner;

/// Upstream `align`: where a short list rests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatListAlign {
    Top,
    /// At the block end, under a spacer (FR5).
    #[default]
    Bottom,
}

type ActionFn = Rc<dyn Fn(&mut Window, &mut App) -> Task<()>>;

/// Upstream `ChatMessageList`.
///
/// ```ignore
/// ChatMessageList::new("log").children(messages).streaming(is_streaming)
/// ```
#[derive(IntoElement)]
pub struct ChatMessageList {
    id: ElementId,
    empty_state: Option<AnyElement>,
    scroll_to_top_action: Option<ActionFn>,
    density: ChatDensity,
    gap: Option<SpacingStep>,
    align: ChatListAlign,
    streaming: bool,
    label: Option<SharedString>,
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 8]>,
}

impl ChatMessageList {
    pub fn new(id: impl Into<ElementId>) -> Self {
        ChatMessageList {
            id: id.into(),
            empty_state: None,
            scroll_to_top_action: None,
            density: ChatDensity::Balanced,
            gap: None,
            align: ChatListAlign::Bottom,
            streaming: false,
            label: None,
            style: StyleRefinement::default(),
            children: SmallVec::new(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `density`: spacing, and the default for its messages.
        density: ChatDensity,
        /// Upstream `gap`: the row gap alone (FR3).
        ?gap: SpacingStep,
        /// Upstream `align`.
        align: ChatListAlign,
        /// Upstream `isStreaming`: the log is busy while a reply streams.
        !streaming,
        /// Upstream `aria-label`: the log's accessible name.
        ?label: SharedString,
    }

    /// Upstream `emptyState`: shown when there are no messages.
    pub fn empty_state(mut self, empty_state: impl IntoElement) -> Self {
        self.empty_state = Some(empty_state.into_any_element());
        self
    }

    /// Upstream `scrollToTopAction`: run when the top of the list comes into view, to
    /// load older messages; a spinner shows until its task finishes (FR6).
    pub fn scroll_to_top_action(
        mut self,
        action: impl Fn(&mut Window, &mut App) -> Task<()> + 'static,
    ) -> Self {
        self.scroll_to_top_action = Some(Rc::new(action));
        self
    }
}

impl Styled for ChatMessageList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ChatMessageList {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// The older-message loader's state.
#[derive(Default)]
struct TopLoader {
    pending: bool,
    /// Whether the top was in view at the last frame, so only coming into view loads.
    visible: bool,
}

impl RenderOnce for ChatMessageList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let i18n = rustryx_i18n::current(cx);
        let focus = control_focus(&self.id, window, cx);
        let loader = window.use_keyed_state(
            ElementId::Name(format!("{}-top-loader", self.id).into()),
            cx,
            |_, _| TopLoader::default(),
        );
        let pending = loader.read(cx).pending;
        if pending {
            // FR6, as upstream's busy announcement.
            rustryx_primitives::announce(
                i18n.t("@astryx.spinner.loading"),
                rustryx_primitives::Politeness::Polite,
                window,
                cx,
            );
        }
        let scrollport = use_chat_layout_context(cx).map(|layout| layout.scroll.scrollport());

        let mut root = div()
            .id(self.id.clone())
            .role(Role::Log)
            .aria_busy(self.streaming)
            .when_some(self.label.clone(), |el, label| el.aria_label(label))
            .track_focus(&focus)
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0();
        theme.refine(
            root.style(),
            "chat-message-list",
            &[("density", self.density.as_str())],
            None,
        );
        root.style().refine(&self.style);

        let (gap, pad_block, pad_inline) = match self.density {
            ChatDensity::Compact => (t.spacing.s2, t.spacing.s2, t.spacing.s3),
            ChatDensity::Balanced => (t.spacing.s4, t.spacing.s4, t.spacing.s4),
            ChatDensity::Spacious => (t.spacing.s6, t.spacing.s6, t.spacing.s6),
        };
        let gap = self.gap.map_or(gap, |step| step.token(t));
        let sentinel = self.scroll_to_top_action.clone().map(|action| {
            let loader = loader.clone();
            canvas(
                move |bounds: Bounds<Pixels>, window, cx| {
                    let view = scrollport.unwrap_or_else(|| {
                        Bounds::new(
                            gpui::point(gpui::px(0.), gpui::px(0.)),
                            window.viewport_size(),
                        )
                    });
                    let visible = bounds.top() <= view.bottom() && bounds.bottom() >= view.top();
                    let start = loader.update(cx, |l, _| {
                        let entered = visible && !l.visible;
                        l.visible = visible;
                        entered && !l.pending
                    });
                    if start {
                        loader.update(cx, |l, _| l.pending = true);
                        let task = action(window, cx);
                        let loader = loader.clone();
                        window
                            .spawn(cx, async move |cx| {
                                task.await;
                                cx.update(|window, cx| {
                                    loader.update(cx, |l, _| l.pending = false);
                                    window.refresh();
                                })
                                .ok();
                            })
                            .detach();
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .w_full()
            .h(gpui::px(1.))
        });
        let has_children = !self.children.is_empty();
        let inner = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(gap)
            .py(pad_block)
            .px(pad_inline)
            .children(sentinel)
            .children(pending.then(|| {
                div()
                    .flex()
                    .justify_center()
                    .py(t.spacing.s3)
                    .child(Spinner::new(ElementId::Name(
                        format!("{}-loading", self.id).into(),
                    )))
            }))
            .children((self.align == ChatListAlign::Bottom).then(|| div().flex_1().min_h_0()))
            .map(|inner| {
                if has_children {
                    inner.children(self.children)
                } else {
                    inner.children(self.empty_state.map(|empty| {
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .items_center()
                            .justify_center()
                            .child(empty)
                    }))
                }
            });
        let ring = is_focus_visible(&focus, window).then(|| focus_ring_inset(t, gpui::px(0.), rem));
        let root = root.child(inner).children(ring);
        // The log is a live container: it delivers the messages and hints added inside
        // it, so what renders inside leaves announcing to it.
        let root = provide_live_container(root, cx);
        provide_in_layer(
            ChatListContext {
                density: self.density,
            },
            root,
            cx,
        )
    }
}
