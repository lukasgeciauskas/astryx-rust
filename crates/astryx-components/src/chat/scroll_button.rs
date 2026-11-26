//! ChatLayoutScrollButton, upstream `Chat/ChatLayoutScrollButton.tsx`: the floating
//! scroll-to-bottom button, which fades in while the reader is scrolled up and widens to
//! a label for new messages.

use std::rc::Rc;

use astryx_theme::ThemeStyled as _;
use astryx_theme::current;
use gpui::Refineable as _;
use gpui::{
    App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, relative,
};
use web_time::Instant;

use crate::button::{Button, ButtonVariant};
use crate::icon::{Icon, IconSize, IconSource};
use astryx_primitives::ElementSize;

type ClickFn = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Upstream `ChatLayoutScrollButton`.
///
/// ```ignore
/// ChatLayoutScrollButton::new("to-bottom", scrolled_up, |_, window, cx| scroll.scroll_to_bottom(..))
/// ```
#[derive(IntoElement)]
pub struct ChatLayoutScrollButton {
    id: ElementId,
    visible: bool,
    label: Option<SharedString>,
    on_click: ClickFn,
    style: StyleRefinement,
}

impl ChatLayoutScrollButton {
    /// Upstream `isVisible` and `onClick`.
    pub fn new(
        id: impl Into<ElementId>,
        visible: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        ChatLayoutScrollButton {
            id: id.into(),
            visible,
            label: None,
            on_click: Rc::new(on_click),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `label`: widens the button to show it (such as "New messages").
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Styled for ChatLayoutScrollButton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The fade's last change of direction.
struct Fade {
    from: f32,
    target: f32,
    at: Option<Instant>,
}

impl RenderOnce for ChatLayoutScrollButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let i18n = astryx_i18n::current(cx);
        let now = cx.background_executor().now();
        let duration = if astryx_primitives::prefers_reduced_motion(cx) {
            std::time::Duration::ZERO
        } else {
            t.duration.fast_max
        };
        let goal = if self.visible { 1. } else { 0. };
        let fade = window.use_keyed_state(
            ElementId::Name(format!("{}-fade", self.id).into()),
            cx,
            |_, _| Fade {
                from: goal,
                target: goal,
                at: None,
            },
        );
        let opacity = fade.update(cx, |f, _| {
            let value = |f: &Fade| match f.at {
                Some(at) if !duration.is_zero() => {
                    let p = (now.duration_since(at).as_secs_f32() / duration.as_secs_f32()).min(1.);
                    f.from + (f.target - f.from) * t.ease.standard.ease(p)
                }
                _ => f.target,
            };
            if f.target != goal {
                f.from = value(f);
                f.target = goal;
                f.at = Some(now);
            }
            value(f)
        });
        if opacity != goal {
            window.request_animation_frame();
        }
        let name = self
            .label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.chatLayoutScrollButton.scrollToBottom"));
        let on_click = self.on_click.clone();
        let mut button = Button::new(ElementId::Name(format!("{}-button", self.id).into()), name)
            .variant(ButtonVariant::Ghost)
            .size(ElementSize::Md)
            .icon(Icon::new(IconSource::Named("chevronDown".into())).size(IconSize::Md))
            .icon_only(self.label.is_none())
            .rounded(t.radius.full)
            .px(t.spacing.s2)
            .whitespace_nowrap()
            .on_click(move |event, window, cx| on_click(event, window, cx));
        if self.label.is_some() {
            button = button.pr(t.spacing.s3);
        }
        let mut container = div()
            .id(self.id.clone())
            .debug_selector({
                let selector = format!("{}", self.id);
                move || selector
            })
            .overflow_hidden()
            .rounded(t.radius.full)
            .bg(t.color.background_popover)
            .shadow_token(&t.shadow.med)
            .h(t.size.element_md)
            .opacity(opacity)
            .max_w(if self.label.is_some() {
                gpui::px(200.).into()
            } else {
                relative(1.)
            })
            .child(button);
        theme.refine(container.style(), "chat-layout-scroll-button", &[], None);
        if opacity == 0. {
            // Hidden: out of the pointer's and keyboard's reach (upstream `inert`).
            container.style().visibility = Some(gpui::Visibility::Hidden);
        }
        let mut wrapper = div().flex().justify_center().pb(t.spacing.s3);
        wrapper.style().refine(&self.style);
        wrapper.child(container)
    }
}
