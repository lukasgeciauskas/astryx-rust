//! Logical-axis scrolling, upstream `useScrollableArea` (AST-025): a caller-owned
//! viewport and content box with live effective-axis and edge state, keyboard
//! access while the viewport scrolls, and overscroll containment on the axes that
//! scroll. GPUI's scroll containers (with fork patch R17) supply the native part:
//! wheel and touch scrolling, chaining and the scrollbars.

use gpui::{
    App, Div, ElementId, FocusHandle, InteractiveElement, KeyDownEvent, Pixels, Point, Role,
    ScrollHandle, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, point, px,
};

use crate::{control_focus, is_focus_visible};

/// Upstream `ScrollAxis`: the logical axes where scrolling is allowed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollAxis {
    /// The inline axis (horizontal in horizontal writing).
    Inline,
    /// The block axis (vertical in horizontal writing).
    #[default]
    Block,
    /// Both.
    Both,
}

impl ScrollAxis {
    pub fn inline(self) -> bool {
        matches!(self, ScrollAxis::Inline | ScrollAxis::Both)
    }

    pub fn block(self) -> bool {
        matches!(self, ScrollAxis::Block | ScrollAxis::Both)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ScrollAxis::Inline => "inline",
            ScrollAxis::Block => "block",
            ScrollAxis::Both => "both",
        }
    }
}

/// Upstream `ScrollOverscroll`: whether a scroll at an edge goes on to the ancestors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollOverscroll {
    /// Chains to the ancestors (the default).
    #[default]
    Allow,
    /// Stays here on the axes that scroll.
    Contain,
}

/// Upstream `ScrollStickyContainment`: whether a fitting viewport stays a sticky
/// boundary. GPUI has no sticky positioning, so both values behave alike
/// (KNOWN_ISSUES).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollStickyContainment {
    #[default]
    WhenScrollable,
    Always,
}

/// The semantics of a named viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollRole {
    #[default]
    Group,
    Region,
}

/// Upstream `ScrollKeyboardAccess`: who keyboard users reach the scrolling through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScrollKeyboardAccess {
    /// The caller's content: the viewport never takes focus.
    Content,
    /// A named viewport that joins the tab order while it scrolls.
    Viewport {
        label: SharedString,
        role: ScrollRole,
    },
    /// As `Viewport`; upstream also hands forward Tab entry to a lone leading
    /// button or link, which GPUI cannot recognise (KNOWN_ISSUES).
    ContentOrViewport {
        label: SharedString,
        role: ScrollRole,
    },
}

/// Upstream `ScrollAxisState`: one requested axis's effective state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollAxisState {
    /// More than a pixel of content past the viewport on a requested axis.
    pub is_scrollable: bool,
    pub at_start: bool,
    pub at_end: bool,
}

impl Default for ScrollAxisState {
    fn default() -> Self {
        ScrollAxisState {
            is_scrollable: false,
            at_start: true,
            at_end: true,
        }
    }
}

/// Upstream `ScrollableAreaState`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollableAreaState {
    pub inline: ScrollAxisState,
    pub block: ScrollAxisState,
}

impl ScrollableAreaState {
    /// Some requested axis scrolls.
    pub fn is_scrollable(&self) -> bool {
        self.inline.is_scrollable || self.block.is_scrollable
    }
}

/// Upstream `UseScrollableAreaOptions`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseScrollableAreaOptions {
    pub axis: ScrollAxis,
    pub keyboard_access: ScrollKeyboardAccess,
    pub overscroll: ScrollOverscroll,
    pub sticky_containment: ScrollStickyContainment,
}

impl UseScrollableAreaOptions {
    pub fn new(axis: ScrollAxis, keyboard_access: ScrollKeyboardAccess) -> Self {
        UseScrollableAreaOptions {
            axis,
            keyboard_access,
            overscroll: ScrollOverscroll::Allow,
            sticky_containment: ScrollStickyContainment::WhenScrollable,
        }
    }
}

/// How far an arrow key scrolls.
pub const ARROW_STEP: Pixels = px(40.);
/// The share of the viewport a page key scrolls.
pub const PAGE_FRACTION: f32 = 0.875;
/// Overflow up to this is still fitting.
const FIT_TOLERANCE: Pixels = px(1.);

/// The state that the handle's last layout and offset describe.
pub fn measure(axis: ScrollAxis, handle: &ScrollHandle) -> ScrollableAreaState {
    let max = handle.max_offset();
    let offset = handle.offset();
    let axis_state = |requested: bool, max: Pixels, offset: Pixels| {
        if !requested || max <= FIT_TOLERANCE {
            return ScrollAxisState::default();
        }
        let scrolled = -offset;
        ScrollAxisState {
            is_scrollable: true,
            at_start: scrolled <= px(0.5),
            at_end: scrolled >= max - px(0.5),
        }
    };
    ScrollableAreaState {
        inline: axis_state(axis.inline(), max.x, offset.x),
        block: axis_state(axis.block(), max.y, offset.y),
    }
}

/// The offset a scrolling key moves to, or `None` when the key does not scroll.
/// `viewport_focused`: Space pages only from the viewport itself (on a button it
/// presses).
#[allow(clippy::too_many_arguments)]
pub fn key_target(
    key: &str,
    shift: bool,
    viewport_focused: bool,
    axis: ScrollAxis,
    state: &ScrollableAreaState,
    offset: Point<Pixels>,
    max: Point<Pixels>,
    viewport: gpui::Size<Pixels>,
) -> Option<Point<Pixels>> {
    let block = axis.block() && state.block.is_scrollable;
    let inline = axis.inline() && state.inline.is_scrollable;
    let page_y = viewport.height * PAGE_FRACTION;
    let (x, y) = (offset.x, offset.y);
    let target = match key {
        "down" if block => point(x, y - ARROW_STEP),
        "up" if block => point(x, y + ARROW_STEP),
        "right" if inline => point(x - ARROW_STEP, y),
        "left" if inline => point(x + ARROW_STEP, y),
        "pagedown" if block => point(x, y - page_y),
        "pageup" if block => point(x, y + page_y),
        "space" if block && viewport_focused => {
            point(x, if shift { y + page_y } else { y - page_y })
        }
        "home" if block => point(x, px(0.)),
        "end" if block => point(x, -max.y),
        "home" if inline => point(px(0.), y),
        "end" if inline => point(-max.x, y),
        _ => return None,
    };
    Some(point(
        target.x.clamp(-max.x, px(0.)),
        target.y.clamp(-max.y, px(0.)),
    ))
}

/// The published state of one area, between frames.
#[derive(Default)]
struct Published {
    state: ScrollableAreaState,
}

/// What `use_scrollable_area` returns: the state and the two getters.
#[derive(Clone)]
pub struct ScrollableArea {
    id: ElementId,
    options: UseScrollableAreaOptions,
    handle: ScrollHandle,
    focus: FocusHandle,
    published: gpui::Entity<Published>,
    /// The state the last layout published.
    pub state: ScrollableAreaState,
}

/// Upstream `useScrollableArea`, keyed by `id`.
pub fn use_scrollable_area(
    id: &ElementId,
    options: UseScrollableAreaOptions,
    window: &mut Window,
    cx: &mut App,
) -> ScrollableArea {
    let handle = window
        .use_keyed_state(
            ElementId::Name(format!("{id:?}-scroll-handle").into()),
            cx,
            |_, _| ScrollHandle::new(),
        )
        .read(cx)
        .clone();
    let published = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-scroll-state").into()),
        cx,
        |_, _| Published::default(),
    );
    let focus = control_focus(id, window, cx);
    let state = published.read(cx).state;
    ScrollableArea {
        id: id.clone(),
        options,
        handle,
        focus,
        published,
        state,
    }
}

impl ScrollableArea {
    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.handle
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// The viewport owns keyboard access and shows a focus ring now.
    pub fn is_focus_visible(&self, window: &Window) -> bool {
        !matches!(self.options.keyboard_access, ScrollKeyboardAccess::Content)
            && is_focus_visible(&self.focus, window)
    }

    /// Upstream `getViewportProps`: overflow on the requested axes, containment on
    /// the ones that scroll, a named viewport (its role and label) that is a tab stop
    /// while it scrolls (FR12, AR1) and keeps focus when the overflow goes (AR2), the scrolling keys, and the measurement
    /// that publishes the state once a frame when it changes (FR5, FR9, PR1).
    /// The viewport takes the hook's id.
    pub fn viewport(&self, el: Div) -> Stateful<Div> {
        let axis = self.options.axis;
        let state = self.state;
        let (handle, published) = (self.handle.clone(), self.published.clone());
        let el = el.on_children_prepainted(move |_, window, cx| {
            let now = measure(axis, &handle);
            if published.read(cx).state != now {
                published.update(cx, |p, _| p.state = now);
                window.defer(cx, |window, _| window.refresh());
            }
        });
        let mut el = el.id(self.id.clone()).track_scroll(&self.handle);
        el = match axis {
            ScrollAxis::Inline => el.overflow_x_scroll(),
            ScrollAxis::Block => el.overflow_y_scroll(),
            ScrollAxis::Both => el.overflow_scroll(),
        };
        if self.options.overscroll == ScrollOverscroll::Contain {
            if state.inline.is_scrollable {
                el = el.overscroll_x_contain();
            }
            if state.block.is_scrollable {
                el = el.overscroll_y_contain();
            }
        }
        match &self.options.keyboard_access {
            ScrollKeyboardAccess::Content => {}
            ScrollKeyboardAccess::Viewport { label, role }
            | ScrollKeyboardAccess::ContentOrViewport { label, role } => {
                // A tab stop only while it scrolls (AR1); named, with its role, always,
                // as upstream's viewport props are.
                el = el
                    .track_focus(&self.focus.clone().tab_stop(state.is_scrollable()))
                    .role(match role {
                        ScrollRole::Group => Role::Group,
                        ScrollRole::Region => Role::Region,
                    })
                    .aria_label(label.clone());
            }
        }
        // The scrolling keys: from the viewport, or bubbling from content that left
        // them alone. A key that cannot move here goes on to the ancestors (FR14).
        let (handle, focus) = (self.handle.clone(), self.focus.clone());
        el = el.on_key_down(move |event: &KeyDownEvent, window, cx| {
            let keys = &event.keystroke;
            if keys.modifiers.control || keys.modifiers.alt || keys.modifiers.platform {
                return;
            }
            let state = measure(axis, &handle);
            let (offset, max) = (handle.offset(), handle.max_offset());
            let Some(target) = key_target(
                &keys.key,
                keys.modifiers.shift,
                focus.is_focused(window),
                axis,
                &state,
                offset,
                max,
                handle.bounds().size,
            ) else {
                return;
            };
            if target != offset {
                handle.set_offset(target);
                cx.stop_propagation();
                window.refresh();
            }
        });
        el
    }

    /// Upstream `getContentProps`: the real content box, at least the viewport's
    /// size, and as wide as its content when the inline axis scrolls (FR2).
    pub fn content(&self, el: Div) -> Div {
        let el = el.min_w_full().min_h_full();
        if self.options.axis.inline() {
            el.w_max().flex_none()
        } else {
            el
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    fn scrolling(inline: bool, block: bool) -> ScrollableAreaState {
        let axis = |on| ScrollAxisState {
            is_scrollable: on,
            at_start: true,
            at_end: !on,
        };
        ScrollableAreaState {
            inline: axis(inline),
            block: axis(block),
        }
    }

    #[test]
    fn ast025_fr12_keys_scroll_the_requested_axes_and_clamp() {
        let state = scrolling(false, true);
        let max = point(px(0.), px(300.));
        let view = size(px(200.), px(100.));
        let at = |key: &str, offset: Point<Pixels>, focused| {
            key_target(
                key,
                false,
                focused,
                ScrollAxis::Block,
                &state,
                offset,
                max,
                view,
            )
        };
        let top = point(px(0.), px(0.));
        assert_eq!(at("down", top, false), Some(point(px(0.), px(-40.))));
        assert_eq!(at("pagedown", top, false), Some(point(px(0.), px(-87.5))));
        assert_eq!(at("end", top, false), Some(point(px(0.), px(-300.))));
        assert_eq!(at("up", top, false), Some(top), "clamped at the start");
        assert_eq!(
            at("right", top, false),
            None,
            "the inline axis does not scroll"
        );
        assert_eq!(
            at("space", top, false),
            None,
            "Space presses a focused control"
        );
        assert_eq!(at("space", top, true), Some(point(px(0.), px(-87.5))));
    }

    #[test]
    fn ast025_fr4_a_fitting_or_unrequested_axis_is_inactive() {
        let handle = ScrollHandle::new();
        let state = measure(ScrollAxis::Inline, &handle);
        assert_eq!(state, ScrollableAreaState::default());
        assert!(!state.is_scrollable());
    }
}
