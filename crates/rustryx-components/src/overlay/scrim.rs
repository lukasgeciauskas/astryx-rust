//! Overlay, upstream `Overlay/`: content on a scrim over a base surface (an
//! image, a video, a card), shown always, on hover or on focus within.
//!
//! - The scrim fills the base, or sits along its bottom or top edge,
//!   padded by `spacing-3` with its content `spacing-2` apart, aligned to
//!   the start, center or end (the default).
//! - `Dark` (the default) dims with `--color-overlay` and draws its content
//!   in the theme's dark media appearance; `Light` brightens with 60% white
//!   and draws it light; no scrim leaves the content as it is.
//! - Shown on hover, it also shows while focus is within the base, so the
//!   keyboard reaches it; a finger, with no hover, toggles it with a tap.
//!   It fades (and, along an edge, slides in) over `--duration-fast`.
//! - `open` overrides all of that.

use std::time::Duration;

use gpui::{
    AnyElement, App, ElementId, FocusHandle, IntoElement, ParentElement, RenderOnce,
    StyleRefinement, Styled, Window, div, prelude::*, px, relative,
};
use rustryx_primitives::prefers_reduced_motion;
use rustryx_theme::{Appearance, Pointer, current, scope};
use smallvec::SmallVec;
use web_time::Instant;

/// Upstream `showOn`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayShowOn {
    #[default]
    Always,
    /// On hover, and while focus is within (keyboard access).
    Hover,
    /// While focus is within.
    Focus,
    /// The same as `Hover`.
    HoverOrFocus,
}

/// Upstream `scrim`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayScrim {
    #[default]
    Dark,
    Light,
    /// No scrim: the content alone.
    None,
}

/// Upstream `position`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayPosition {
    #[default]
    Fill,
    Bottom,
    Top,
}

/// Upstream `align`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayAlign {
    Start,
    Center,
    #[default]
    End,
}

struct OverlayState {
    within: FocusHandle,
    hovered: bool,
    touch_open: bool,
    /// Visible last frame, and since when it last changed.
    shown: bool,
    changed_at: Option<Instant>,
}

/// Content on a scrim over a base surface, upstream `Overlay`.
///
/// ```ignore
/// Overlay::new("cover")
///     .show_on(OverlayShowOn::Hover)
///     .position(OverlayPosition::Bottom)
///     .content(Text::new("Sunset over the bay"))
///     .child(Image::new(...))
/// ```
#[derive(IntoElement)]
pub struct Overlay {
    id: ElementId,
    content: Option<AnyElement>,
    children: SmallVec<[AnyElement; 1]>,
    show_on: OverlayShowOn,
    open: Option<bool>,
    scrim: OverlayScrim,
    position: OverlayPosition,
    align: OverlayAlign,
    style: StyleRefinement,
}

impl Overlay {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Overlay {
            id: id.into(),
            content: None,
            children: SmallVec::new(),
            show_on: OverlayShowOn::Always,
            open: None,
            scrim: OverlayScrim::Dark,
            position: OverlayPosition::Fill,
            align: OverlayAlign::End,
            style: StyleRefinement::default(),
        }
    }

    rustryx_primitives::setters! {
        /// Upstream `showOn`.
        show_on: OverlayShowOn,
        /// Upstream `scrim`.
        scrim: OverlayScrim,
        /// Upstream `position`.
        position: OverlayPosition,
        /// Upstream `align`.
        align: OverlayAlign,
    }

    /// Upstream `content`: what the scrim shows.
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }

    /// Upstream `isOpen`: forces it shown or hidden.
    pub fn open(mut self, open: impl Into<Option<bool>>) -> Self {
        self.open = open.into();
        self
    }
}

impl ParentElement for Overlay {
    /// Upstream `children`: the base surface.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Overlay {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Overlay {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-overlay", self.id).into()),
            cx,
            |_, cx| OverlayState {
                within: cx.focus_handle().tab_stop(false),
                hovered: false,
                touch_open: false,
                shown: false,
                changed_at: None,
            },
        );
        let within = state.read(cx).within.clone();
        let focus_within = within.contains_focused(window, cx);
        let hover_mode = matches!(
            self.show_on,
            OverlayShowOn::Hover | OverlayShowOn::HoverOrFocus
        );
        let visible = match self.open {
            Some(open) => open,
            None => match self.show_on {
                OverlayShowOn::Always => true,
                OverlayShowOn::Focus => focus_within,
                OverlayShowOn::Hover | OverlayShowOn::HoverOrFocus => {
                    let s = state.read(cx);
                    s.hovered || focus_within || s.touch_open
                }
            },
        };

        // A fade over --duration-fast from the last change.
        let now = cx.background_executor().now();
        let changed_at = state.update(cx, |s, _| {
            if s.shown != visible {
                s.shown = visible;
                s.changed_at = Some(now);
            }
            s.changed_at
        });
        let duration = if prefers_reduced_motion(cx) {
            Duration::ZERO
        } else {
            t.duration.fast
        };
        let progress = match changed_at {
            Some(at) if !duration.is_zero() => {
                (now.saturating_duration_since(at).as_secs_f32() / duration.as_secs_f32()).min(1.)
            }
            _ => 1.,
        };
        if progress < 1. {
            window.request_animation_frame();
        }
        let eased = t.ease.standard.ease(progress);
        let shown = if visible { eased } else { 1. - eased };

        let scrim = self.content.map(|content| {
            // The content draws in the scrim's media appearance.
            let content = match self.scrim {
                OverlayScrim::None => content,
                mode => {
                    let appearance = if mode == OverlayScrim::Dark {
                        Appearance::Dark
                    } else {
                        Appearance::Light
                    };
                    let width = f32::from(window.viewport_size().width);
                    scope(
                        theme.on_surface(appearance, width, Pointer::Fine),
                        div().flex().flex_col().gap(t.spacing.s2).child(content),
                    )
                    .into_any_element()
                }
            };
            let mut el = div()
                .absolute()
                .flex()
                .flex_col()
                .gap(t.spacing.s2)
                .p(t.spacing.s3)
                .opacity(shown);
            el = match self.position {
                OverlayPosition::Fill => el.top_0().bottom_0().left_0().right_0(),
                OverlayPosition::Bottom => el.left_0().right_0().bottom_0(),
                OverlayPosition::Top => el.left_0().right_0().top_0(),
            };
            el = match self.align {
                OverlayAlign::Start => el.items_start().justify_start(),
                OverlayAlign::Center => el.items_center().justify_center(),
                OverlayAlign::End => el.items_end().justify_start(),
            };
            el = match self.scrim {
                OverlayScrim::Dark => el.bg(t.color.overlay),
                OverlayScrim::Light => el.bg(gpui::hsla(0., 0., 1., 0.6)),
                OverlayScrim::None => el,
            };
            // Along an edge it slides in from that edge.
            let travel = 1. - shown;
            el = match self.position {
                OverlayPosition::Bottom => el.translate(px(0.), relative(travel)),
                OverlayPosition::Top => el.translate(px(0.), relative(-travel)),
                OverlayPosition::Fill => el,
            };
            let mut el = el.child(content);
            theme.refine(
                el.style(),
                "overlay-scrim",
                &[(
                    "position",
                    match self.position {
                        OverlayPosition::Fill => "fill",
                        OverlayPosition::Bottom => "bottom",
                        OverlayPosition::Top => "top",
                    },
                )],
                None,
            );
            // Drawn while shown, and while it fades out.
            (visible || shown > 0.).then_some(el)
        });

        let touch_toggles = hover_mode && self.open.is_none();
        let mut root = div()
            .id(self.id.clone())
            .relative()
            .overflow_hidden()
            .track_focus(&within)
            .on_hover({
                let state = state.clone();
                move |hovered, window, cx| {
                    // A touch device (upstream `(hover: none)`) has no hover: a finger's
                    // input, or a device declared touch-first.
                    if window.last_input_was_touch()
                        || rustryx_primitives::modality::declared_primary_pointer(cx) == Some(true)
                    {
                        return;
                    }
                    state.update(cx, |s, _| s.hovered = *hovered);
                    window.refresh();
                }
            })
            .children(self.children)
            .children(scrim.flatten());
        if touch_toggles {
            root = root.on_click({
                let state = state.clone();
                move |_, window, cx| {
                    if window.last_input_was_touch()
                        || rustryx_primitives::modality::declared_primary_pointer(cx) == Some(true)
                    {
                        state.update(cx, |s, _| s.touch_open = !s.touch_open);
                        window.refresh();
                    }
                }
            });
        }
        theme.refine(root.style(), "overlay", &[], None);
        root.style().refine(&self.style);
        root
    }
}
