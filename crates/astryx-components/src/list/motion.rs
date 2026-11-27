//! List FR12 with an `animated_layout` handle (AST-105, upstream `listLayoutUpdates.ts`):
//! each row inserted, removed or moved between two renders is reported to the handle, so
//! an inserted row enters (it reveals from its block-end edge), a removed row collapses in
//! place, inert, until it is gone, a row that crosses rows moving the other way rides
//! above them on the list's surface, and the rows after a change glide (fork patch R19,
//! keyed by each row's stable element id). A row's identity is its ListItem's
//! `item_key`; a row without one changes at once. Without a handle nothing here runs.
//!
//! As upstream, only a row drawn in the area's visible part plays its exit; a row removed
//! out of view goes at once.
//!
//! GPUI draws an element once a frame and keeps none from the last, so each keyed row
//! keeps its last frame as an offscreen layer (fork patch R3), and a removed row's exit
//! draws that snapshot: the row as it was, start and end content included. A renderer
//! without offscreen layers (Metal) keeps no frame; there the exit draws what the List
//! kept of the row: its label, description, alignment, selection and divider.

use std::collections::{HashMap, HashSet};

use gpui::{AnyElement, App, Bounds, ElementId, Entity, Pixels, ViewElement, Window, px};
use web_time::Instant;

use super::item::{ListItem, RowGhost};
use crate::animated_layout::{AnimatedLayout, EdgeReveal, LayoutMotion, use_edge_reveal};
use crate::reorderable::ItemKey;

/// A removed row while it collapses.
#[derive(Clone)]
pub(super) struct Exiting {
    pub key: ItemKey,
    pub ghost: RowGhost,
    /// How many keyed rows come before it: it keeps its place after them.
    pub before: usize,
}

#[derive(Default)]
struct MotionState {
    started: bool,
    order: Vec<ItemKey>,
    ghosts: HashMap<ItemKey, RowGhost>,
    exiting: Vec<Exiting>,
    /// Each keyed row's top and height when last drawn.
    places: HashMap<ItemKey, (Pixels, Pixels)>,
    /// The keyed rows last drawn in the visible part (inside every clip above the list).
    in_view: HashSet<ItemKey>,
    riders: HashSet<ItemKey>,
    riders_until: Option<Instant>,
}

/// One place in the list: a row (a child, by its index) or a removed row's exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Slot {
    Row(usize),
    Exit(usize),
}

/// This frame's motion of a List.
pub(super) struct ListMotion {
    state: Entity<MotionState>,
    /// Each child's key, when it is a keyed ListItem.
    pub keys: Vec<Option<ItemKey>>,
    /// Each child's selection, for the rider's surface.
    pub selected: Vec<bool>,
    /// Each keyed row's reveal (an entrance, else at rest).
    pub reveals: HashMap<ItemKey, EdgeReveal>,
    /// The removed rows still collapsing, with their conceals (taken as they draw).
    pub exits: Vec<Option<(Exiting, EdgeReveal)>>,
    /// The rows and exits in order.
    pub slots: Vec<Slot>,
    /// Rows that ride above the rows they cross.
    pub riders: HashSet<ItemKey>,
}

/// The element id that names a keyed row from frame to frame (its reveal and its glide).
pub(super) fn row_id(list: &ElementId, key: &ItemKey) -> ElementId {
    ElementId::Name(format!("{list}-row-{key:?}").into())
}

/// The id a keyed row's last frame is kept under, for its exit's copy (fork patch R3).
pub(super) fn row_copy_id(list: &ElementId, key: &ItemKey) -> ElementId {
    ElementId::Name(format!("{list}-row-copy-{key:?}").into())
}

/// The order of rows and exits: an exit sits before the keyed row that has `before`
/// keyed rows ahead of it, or at the end.
fn sequence(keys: &[Option<ItemKey>], exits: &[Exiting]) -> Vec<Slot> {
    let mut slots = Vec::with_capacity(keys.len() + exits.len());
    let mut placed = vec![false; exits.len()];
    let mut keyed = 0usize;
    for (i, key) in keys.iter().enumerate() {
        if key.is_some() {
            for (j, exit) in exits.iter().enumerate() {
                if !placed[j] && exit.before <= keyed {
                    placed[j] = true;
                    slots.push(Slot::Exit(j));
                }
            }
            keyed += 1;
        }
        slots.push(Slot::Row(i));
    }
    for (j, done) in placed.into_iter().enumerate() {
        if !done {
            slots.push(Slot::Exit(j));
        }
    }
    slots
}

/// The rows that cross another moving the other way, each pair's longer move riding
/// above (upstream `markCrossings`), from where each was and where it lands.
fn crossings(moves: &[(ItemKey, Pixels, Pixels)]) -> HashSet<ItemKey> {
    let n = moves.len();
    let mut crosses = vec![false; n];
    let mut lowest = f32::NEG_INFINITY;
    for (i, (_, _, to)) in moves.iter().enumerate() {
        crosses[i] = lowest > f32::from(*to);
        lowest = lowest.max(f32::from(*to));
    }
    let mut highest = f32::INFINITY;
    for i in (0..n).rev() {
        let to = f32::from(moves[i].2);
        crosses[i] |= highest < to;
        highest = highest.min(to);
    }
    let crossing: Vec<&(ItemKey, Pixels, Pixels)> = moves
        .iter()
        .zip(crosses)
        .filter_map(|(m, c)| c.then_some(m))
        .collect();
    let mut riders = HashSet::new();
    for a in 0..crossing.len() {
        for b in a + 1..crossing.len() {
            let (upper, lower) = (crossing[a], crossing[b]);
            if upper.2 > lower.2 {
                let travel = |m: &(ItemKey, Pixels, Pixels)| (m.2 - m.1).abs();
                // On a tie, the row moving up rides above.
                let rider = if travel(upper) > travel(lower) {
                    upper
                } else {
                    lower
                };
                if travel(rider) >= px(0.5) {
                    riders.insert(rider.0.clone());
                }
            }
        }
    }
    riders
}

/// Reads the List's children and reports this frame's changes to `layout`.
pub(super) fn begin(
    list: &ElementId,
    children: &mut [AnyElement],
    layout: &AnimatedLayout,
    gap: Pixels,
    window: &mut Window,
    cx: &mut App,
) -> ListMotion {
    let mut keys = Vec::with_capacity(children.len());
    let mut selected = Vec::with_capacity(children.len());
    let mut ghosts = HashMap::new();
    let count = children.len();
    for (i, child) in children.iter_mut().enumerate() {
        let item = child
            .downcast_mut::<ViewElement<ListItem>>()
            .and_then(|v| v.view_mut());
        let key = item.as_ref().and_then(|item| item.motion_key());
        if let (Some(item), Some(key)) = (item.as_ref(), key.as_ref()) {
            ghosts.insert(key.clone(), item.ghost(i + 1 == count));
        }
        selected.push(item.as_ref().is_some_and(|item| item.is_selected()));
        keys.push(key);
    }
    let state = window.use_keyed_state(
        ElementId::Name(format!("{list}-list-motion").into()),
        cx,
        |_, _| MotionState::default(),
    );
    let now = cx.background_executor().now();
    let current: Vec<ItemKey> = keys.iter().flatten().cloned().collect();
    let (started, prev, kept_ghosts, places, in_view) = {
        let s = state.read(cx);
        (
            s.started,
            s.order.clone(),
            s.ghosts.clone(),
            s.places.clone(),
            s.in_view.clone(),
        )
    };
    let changed = started && prev != current;
    let present: HashSet<&ItemKey> = current.iter().collect();
    let before_set: HashSet<&ItemKey> = prev.iter().collect();
    let mut exiting = state.read(cx).exiting.clone();
    // A row that comes back mid-exit enters again from what it shows.
    exiting.retain(|e| !present.contains(&e.key));
    let mut entering: HashSet<ItemKey> = HashSet::new();
    if changed {
        for (at, key) in prev.iter().enumerate() {
            if present.contains(key) {
                continue;
            }
            // Only a row drawn in view exits; any other goes with the change.
            if !in_view.contains(key) {
                continue;
            }
            if let Some(ghost) = kept_ghosts.get(key) {
                let before = prev[..at].iter().filter(|k| present.contains(k)).count();
                exiting.push(Exiting {
                    key: key.clone(),
                    ghost: ghost.clone(),
                    before,
                });
            }
        }
        entering = current
            .iter()
            .filter(|k| !before_set.contains(k))
            .cloned()
            .collect();
        layout.report_change(window, cx);
    }

    // Each keyed row's reveal: an inserted row enters from nothing.
    let mut reveals = HashMap::new();
    for key in &current {
        let id = row_id(list, key);
        if entering.contains(key) {
            use_edge_reveal(id.clone(), false, Some(layout), window, cx);
        }
        let reveal = use_edge_reveal(id, true, Some(layout), window, cx);
        reveals.insert(key.clone(), reveal);
    }
    // Each removed row's conceal, while it lasts.
    let mut exits = Vec::new();
    for exit in exiting {
        let reveal = use_edge_reveal(row_id(list, &exit.key), false, Some(layout), window, cx);
        if reveal.mounted {
            exits.push(Some((exit, reveal)));
        }
    }
    let kept: Vec<Exiting> = exits.iter().flatten().map(|(e, _)| e.clone()).collect();
    let slots = sequence(&keys, &kept);

    // The rows that cross: where each staying row was and where it lands.
    let mut riders = state.read(cx).riders.clone();
    let mut riders_until = state.read(cx).riders_until;
    if changed && !places.is_empty() {
        let origin = places
            .values()
            .map(|(top, _)| *top)
            .fold(Pixels::MAX, Pixels::min);
        let mut at = origin;
        let mut moves = Vec::new();
        for slot in &slots {
            let (key, height, staying) = match slot {
                Slot::Row(i) => match &keys[*i] {
                    Some(key) => {
                        let height = if entering.contains(key) {
                            px(0.)
                        } else {
                            places.get(key).map_or(px(0.), |p| p.1)
                        };
                        (Some(key.clone()), height, !entering.contains(key))
                    }
                    None => (None, px(0.), false),
                },
                Slot::Exit(j) => {
                    let key = &kept[*j].key;
                    (None, places.get(key).map_or(px(0.), |p| p.1), false)
                }
            };
            if let (Some(key), true) = (key, staying)
                && let Some((from, _)) = places.get(&key)
            {
                moves.push((key, *from, at));
            }
            at += height + gap;
        }
        // In their old order, as upstream reads them.
        moves.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let crossing = crossings(&moves);
        if !crossing.is_empty() {
            let motion = LayoutMotion::read(window, cx);
            riders = crossing;
            riders_until = Some(now + motion.duration);
        }
    }
    // For the length of its glide: a later change in the area can carry it on.
    if riders_until.is_some_and(|until| now >= until) && !layout.is_running() {
        riders.clear();
        riders_until = None;
    }
    if riders_until.is_some() {
        window.request_animation_frame();
    }

    state.update(cx, |s, _| {
        s.started = true;
        s.order = current;
        for (key, ghost) in ghosts {
            s.ghosts.insert(key, ghost);
        }
        let live: HashSet<ItemKey> = s
            .order
            .iter()
            .cloned()
            .chain(kept.iter().map(|e| e.key.clone()))
            .collect();
        s.ghosts.retain(|k, _| live.contains(k));
        s.places.retain(|k, _| live.contains(k));
        s.exiting = kept;
        s.riders = riders.clone();
        s.riders_until = riders_until;
    });
    ListMotion {
        state,
        keys,
        selected,
        reveals,
        exits,
        slots,
        riders,
    }
}

impl ListMotion {
    /// Keeps where each keyed row was drawn, from the list's children's bounds in slot
    /// order, and which of them are in the visible part (the clip of every scroller above
    /// the list, within the window); returns the bounds of the rows alone (not the
    /// exits), in child order.
    pub fn record(
        &self,
    ) -> impl Fn(&[Bounds<Pixels>], &mut Window, &mut App) -> Vec<Bounds<Pixels>> + 'static {
        let state = self.state.clone();
        let slots = self.slots.clone();
        let keys = self.keys.clone();
        move |bounds, window, cx| {
            let visible = window.content_mask().bounds;
            let mut rows = Vec::with_capacity(keys.len());
            let mut places = Vec::new();
            let mut in_view = HashSet::new();
            for (slot, b) in slots.iter().zip(bounds) {
                if let Slot::Row(i) = slot {
                    rows.push(*b);
                    if let Some(key) = &keys[*i] {
                        places.push((key.clone(), (b.top(), b.size.height)));
                        if visible.intersects(b) {
                            in_view.insert(key.clone());
                        }
                    }
                }
            }
            // Past the slots: the reorder overlay and anything after it.
            rows.extend(bounds.iter().skip(slots.len()).copied());
            state.update(cx, |s, _| {
                for (key, place) in places {
                    s.places.insert(key, place);
                }
                s.in_view = in_view;
            });
            rows
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exit(key: &'static str, before: usize) -> Exiting {
        Exiting {
            key: key.into(),
            ghost: RowGhost::default(),
            before,
        }
    }

    #[test]
    fn a_removed_row_keeps_its_place_among_the_keyed_rows() {
        let keys = vec![Some("a".into()), None, Some("c".into())];
        assert_eq!(
            sequence(&keys, &[exit("b", 1)]),
            [Slot::Row(0), Slot::Row(1), Slot::Exit(0), Slot::Row(2)]
        );
        assert_eq!(
            sequence(&keys, &[exit("z", 5)]),
            [Slot::Row(0), Slot::Row(1), Slot::Row(2), Slot::Exit(0)]
        );
    }

    #[test]
    fn of_two_rows_that_cross_the_longer_move_rides_above() {
        // a moves from the top to the bottom past b and c, which move up one row each.
        let moves = vec![
            ("a".into(), px(0.), px(80.)),
            ("b".into(), px(40.), px(0.)),
            ("c".into(), px(80.), px(40.)),
        ];
        let riders = crossings(&moves);
        assert!(riders.contains(&ItemKey::from("a")));
        assert_eq!(riders.len(), 1);
        // Rows that only shift (an insert above) cross nothing.
        let shift = vec![
            ("a".into(), px(0.), px(40.)),
            ("b".into(), px(40.), px(80.)),
        ];
        assert!(crossings(&shift).is_empty());
    }
}
