//! The panels of a row's swipe actions, upstream `Swipe/SwipeActionsLayer.tsx`: both
//! sides' panels rendered into the row's own root, and the gesture that drives it.
//!
//! A panel is two boxes and the entries. The window sits over the row's resting box
//! (it moves back by the travel the root moves) and clips to it and its corners; it
//! takes no pointer. The track inside it sits beside the window at rest, so not a pixel
//! of it shows, and moves with the row: the part inside the window is exactly the space
//! the row vacates. The entries run from the track's edge next to the row, each moved
//! back so it stays pinned at the row's outer edge until the row has uncovered them all;
//! past the open width they ride the row and the outermost entry's color, painted
//! beside it, fills the rest out to that edge. On a confirmed full swipe the outermost
//! entry slides to the row's edge. While an entry covers the panel, its color fills the
//! track and the others hide at once, so no entry's color shows behind or between
//! others (DEC-9).
//!
//! Under `reveal` the entries are real buttons, named by their labels; a panel is inert
//! (out of the tab order, the pointer's reach and the accessibility tree, fork patch
//! R20) except while the row is open on its side, and under `commit` always (AR1).

use astryx_primitives::gestures::host::gesture_area;
use astryx_primitives::{control_focus, is_focus_visible};
use astryx_theme::{ThemeStyled, color::composite, current};
use gpui::{
    AnyElement, App, Bounds, DispatchPhase, Div, ElementId, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, MouseDownEvent, ParentElement, Pixels, Role, Stateful, Styled,
    TouchEvent, TouchPhase, Window, canvas, div, prelude::*, px, relative,
};

use super::release::{SwipeSide, physical_sign};
#[cfg(test)]
use super::row::{SideGeometry, SwipeState};
use super::row::{SwipeRow, is_swipe_open};
use super::{ItemSwipeActionVariant, ItemSwipeBehavior, SwipeEntry};
use crate::focus::focus_ring;

/// The least width of an entry, px (upstream `minWidth: 72`).
const ENTRY_MIN_WIDTH: f32 = 72.;

#[cfg(test)]
thread_local! {
    /// What each swiped row drew last, by its id, for tests.
    pub(crate) static DRAWN: std::cell::RefCell<std::collections::HashMap<String, Drawn>> =
        std::cell::RefCell::default();
}

/// What a row drew, for tests.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct Drawn {
    pub travel: f64,
    pub confirm: f64,
    pub state: SwipeState,
    pub opacity: f64,
    pub geometry: [SideGeometry; 2],
    pub armed: bool,
}

fn variant_colors(
    t: &astryx_theme::Tokens,
    variant: ItemSwipeActionVariant,
) -> (gpui::Hsla, gpui::Hsla) {
    match variant {
        ItemSwipeActionVariant::Neutral => (t.color.neutral, t.color.text_primary),
        ItemSwipeActionVariant::Accent => (t.color.accent, t.color.on_accent),
        ItemSwipeActionVariant::Destructive => (t.color.error, t.color.on_error),
    }
}

/// The row with its swipe: the root moves by the travel, its panels are drawn into it,
/// and the gesture's areas wrap it. `own_focus` is the row's own tab stop, where focus
/// goes when a panel closes under it.
pub(crate) fn decorate(
    row: &SwipeRow,
    root: Stateful<Div>,
    id: &ElementId,
    radii: gpui::CornersRefinement<gpui::AbsoluteLength>,
    own_focus: Option<FocusHandle>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    row.advance(window, cx);
    if row.take_refocus() {
        // A panel closing under focus hands it to the row's content.
        let focused_entry = {
            let st = row.st();
            st.entry_focus
                .iter()
                .flatten()
                .any(|f| f.is_focused(window))
        };
        if focused_entry {
            match &own_focus {
                Some(focus) => focus.focus(window, cx),
                None => window.blur(cx),
            }
        }
    }
    let travel = row.presented_x();
    let confirm = row.confirm_of(travel);
    let opacity = row.opacity();
    let state = row.state();
    let (host, geometry) = {
        let st = row.st();
        (st.host.clone(), st.geometry)
    };
    #[cfg(test)]
    DRAWN.with(|d| {
        d.borrow_mut().insert(
            id.to_string(),
            Drawn {
                travel,
                confirm,
                state,
                opacity,
                geometry,
                armed: row.armed(),
            },
        )
    });
    let open = is_swipe_open(&state);
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let viewport = window.viewport_size().width;

    let mut root = root;
    if travel != 0. {
        root = root.translate(px(travel as f32), px(0.));
    }
    if opacity < 0.999 {
        // A Reduce Motion cross-fade fades the row as one group (fork patch R3).
        root = root.layer_opacity(opacity as f32);
    }
    // The bridge: Escape closes an open row with focus inside it.
    root = root.on_key_down({
        let row = row.clone();
        move |event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" && row.is_open() {
                cx.stop_propagation();
                row.close(true);
                window.refresh();
            }
        }
    });
    // The row's resting box, and the closes of an open row: a pointer landing outside
    // it, and a scroll that moves it.
    let probe = canvas(
        {
            let row = row.clone();
            move |bounds: Bounds<Pixels>, window: &mut Window, _: &mut App| {
                row.record_bounds(bounds, window);
            }
        },
        {
            let row = row.clone();
            move |bounds: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
                if !row.is_open() {
                    return;
                }
                let mouse = row.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
                    if phase == DispatchPhase::Capture && !bounds.contains(&event.position) {
                        mouse.close(true);
                        window.refresh();
                    }
                });
                let touch = row.clone();
                window.on_touch_event(move |event: &TouchEvent, window, _| {
                    if event.phase == TouchPhase::Started && !bounds.contains(&event.position) {
                        touch.close(true);
                        window.refresh();
                    }
                    false
                });
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();
    root = root.child(probe);

    let behavior = host.behavior;
    let mut focus_handles: [Vec<FocusHandle>; 2] = [Vec::new(), Vec::new()];
    for side in [SwipeSide::Leading, SwipeSide::Trailing] {
        let entries = host.entries(side);
        if entries.is_empty() {
            continue;
        }
        let inline_sign = row.st().inline_sign;
        let sign = physical_sign(side, inline_sign);
        let geom = geometry[side.index()];
        let commit = behavior == ItemSwipeBehavior::Commit;
        let live = open && state.side == side && !commit;
        let cover = state
            .cover
            .filter(|c| state.side == side && *c < entries.len());
        let outer_index = entries.len() - 1;
        let fill_entry = &entries[cover.unwrap_or(outer_index)];
        let surface = t.color.background_surface;
        let track_bg = match cover {
            Some(_) => composite(surface, variant_colors(t, fill_entry.variant).0),
            None => surface,
        };
        let entries_x = geom.entries_x(travel);
        let mut track = div()
            .absolute()
            .top_0()
            .bottom_0()
            .w(relative(2.))
            .overflow_hidden()
            .flex()
            .bg(track_bg)
            .translate(px(travel as f32), px(0.));
        track.style().corner_radii = radii.clone();
        // The outermost entry is the last, at the outer edge.
        track = if sign > 0. {
            track.right(relative(1.)).flex_row_reverse()
        } else {
            track.left(relative(1.)).flex_row()
        };
        theme.refine(
            track.style(),
            "item-swipe-panel",
            &[("side", side.as_str())],
            None,
        );
        let row_key = id.to_string();
        let widths_row = row.clone();
        let track = track.on_children_prepainted(move |bounds: Vec<Bounds<Pixels>>, _, _| {
            widths_row.st_mut().widths[side.index()] =
                bounds.iter().map(|b| f64::from(b.size.width)).collect();
        });
        let mut children = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let is_outer = index == outer_index;
            let x = if is_outer {
                entries_x + geom.slide * confirm
            } else {
                entries_x
            };
            let entry_id =
                ElementId::Name(format!("{row_key}-swipe-{}-{index}", side.as_str()).into());
            let el = entry_element(
                entry,
                EntryLook {
                    is_outer,
                    sign,
                    x,
                    covered: cover.is_some_and(|c| c != index),
                    viewport,
                },
                window,
                cx,
            );
            let selector = format!("{row_key}-swipe-{}-{index}", side.as_str());
            let el = el.debug_selector(move || selector.clone());
            if commit {
                children.push(el.into_any_element());
                continue;
            }
            let focus = control_focus(&entry_id, window, cx);
            focus_handles[side.index()].push(focus.clone());
            let mut button = el
                .id(entry_id)
                .track_focus(&focus)
                .role(Role::Button)
                .aria_label(entry.label.clone());
            if is_focus_visible(&focus, window) {
                button = button.child(focus_ring(t, t.radius.element.to_pixels(rem), rem));
            }
            if entry.disabled {
                // Focusable while the row rests open, but inert: no `on_activate`, and
                // its click does not reach the row.
                button = button
                    .aria_disabled(true)
                    .on_click(|_, _, cx| cx.stop_propagation());
            } else {
                let (row, fire, holds_out) =
                    (row.clone(), entry.on_activate.clone(), entry.has_removal);
                button = button.cursor_pointer().on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if row.activate_entry(fire.clone(), holds_out, index) {
                        let key = row.st().host.item_key.clone();
                        fire(key.as_ref(), window, cx);
                    }
                    window.refresh();
                });
            }
            children.push(button.into_any_element());
        }
        let selector = format!("{row_key}-swipe-panel-{}", side.as_str());
        let mut panel = div()
            .id(ElementId::Name(
                format!("{row_key}-swipe-panel-{}", side.as_str()).into(),
            ))
            .debug_selector(move || selector.clone())
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .overflow_hidden()
            .inert(!live)
            .child(track.children(children));
        panel.style().corner_radii = radii.clone();
        if travel != 0. {
            panel = panel.translate(px(-travel as f32), px(0.));
        }
        root = root.child(panel);
    }
    row.st_mut().entry_focus = focus_handles;

    // The gesture: the dismissing tap inside the pan's area.
    let inner: AnyElement = match row.dismiss_attachment() {
        Some(attachment) => gesture_area(&attachment, root).into_any_element(),
        None => root.into_any_element(),
    };
    match row.snap() {
        Some(snap) => snap.wrap(inner),
        None => inner,
    }
}

struct EntryLook {
    is_outer: bool,
    /// The side's physical sign: 1 on the left.
    sign: f64,
    /// Its translation against the track.
    x: f64,
    /// Another entry covers the panel: it hides at once.
    covered: bool,
    viewport: Pixels,
}

/// An entry's surface: its icon above its label on its variant's fill.
fn entry_element(entry: &SwipeEntry, look: EntryLook, window: &mut Window, cx: &mut App) -> Div {
    let theme = current(window, cx);
    let t = theme.tokens();
    let (fill, text) = variant_colors(t, entry.variant);
    let mut el = div()
        .relative()
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(t.spacing.s1)
        .min_w(px(ENTRY_MIN_WIDTH))
        .h_full()
        .px(t.spacing.s3)
        .bg(fill)
        .text_color(text)
        .font_stack(&t.font.family.body)
        .text_size(t.text.supporting.size)
        .line_height(relative(t.text.supporting.leading))
        .font_weight(t.font.weight.medium)
        .whitespace_nowrap();
    if look.x != 0. {
        el = el.translate(px(look.x as f32), px(0.));
    }
    if look.is_outer {
        // Its color painted beside it out to the row's outer edge: it fills whatever
        // the row uncovers past the open width, and the row on a slide-out.
        let beside = div()
            .absolute()
            .top_0()
            .bottom_0()
            .w(look.viewport)
            .bg(fill);
        el = el.child(if look.sign > 0. {
            beside.right(relative(1.))
        } else {
            beside.left(relative(1.))
        });
    }
    if let Some(icon) = &entry.icon {
        el = el.child(icon(window, cx));
    }
    el = el.child(div().child(entry.label.clone()));
    if entry.disabled {
        // Half strength as one group, upstream's `filter: opacity(0.5)` (fork patch R3),
        // so the entry's own opacity stays free for the cover.
        el = el.layer_opacity(0.5);
    }
    if look.covered {
        el = el.opacity(0.);
    }
    theme.refine(
        el.style(),
        "item-swipe-action",
        &[("variant", entry.variant.as_str())],
        None,
    );
    el
}
