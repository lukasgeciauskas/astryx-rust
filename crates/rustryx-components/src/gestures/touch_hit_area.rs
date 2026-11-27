//! The touch hit area, upstream `utils/touchHitArea.ts` and `utils/hitArea.ts`'s
//! `watchHitArea`: under a coarse primary pointer a small control's target grows toward
//! 44 px through an invisible hit layer, each side stopping at half the gap to a
//! neighbour, so a tap always goes to the nearer target. Under a fine pointer nothing is
//! measured and the insets are 0.
//!
//! The neighbours are the pressable surfaces the window drew last frame (every control,
//! row, card, tab and nav item registers one with the press controller, and each hit-area
//! owner registers its own box): GPUI keeps no list of a page's interactive elements, so
//! the surfaces that take presses stand in for upstream's `INTERACTIVE_SELECTOR`.
//!
//! Upstream keeps it internal (`@astryxdesign/core/internal/touch`'s `useTouchHitArea`
//! and `touchHitAreaStyles`), for the packages built on Core; so is this module, public
//! for Lab and hidden from the docs.

use rustryx_primitives::gestures::hit_area::{
    HitInsets, HitRect, MIN_TOUCH_TARGET, NO_HIT_INSETS, clamped_hit_insets,
};
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use web_time::Instant;

/// Notes a pressable target's box this frame (in paint).
pub fn note_target(window: &Window, key: u64, bounds: Bounds<Pixels>, now: Instant) {
    rustryx_primitives::gestures::hit_area::note_target(window, key, bounds, now);
}

/// How far the target `key` (drawn last frame) may grow on each side now: toward 44 px,
/// stopping halfway to each live neighbour. 0 until it has been drawn once.
pub fn hit_insets(window: &Window, key: u64, now: Instant) -> HitInsets {
    hit_insets_with(window, key, now, None)
}

/// An owner's activation, run by a tap on its hit layer.
pub type ActivateFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// A stable key for the hit area of the element `id` names.
pub fn key_of(id: &ElementId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{id:?}-hit-area").hash(&mut hasher);
    hasher.finish()
}

/// A probe (absolute, its parent's full size) that notes an interactive surface's box
/// under a coarse primary pointer, so the hit areas around it stop halfway to it.
pub fn target_probe(key: u64) -> impl IntoElement {
    rustryx_primitives::gestures::hit_area::target_probe(key)
}

/// Upstream `useTouchHitArea` with `touchHitAreaStyles.layer`: children for a positioned
/// control's root. They note its box, and under a coarse primary pointer an invisible
/// layer grown toward 44 px (each side stopping halfway to a neighbour) takes the taps
/// just past it and runs `activate`. `full` replaces the centered growth (a group's
/// box grows by its edge members, upstream `groupGrowth`).
pub fn hit_area(
    id: &ElementId,
    enabled: bool,
    activate: Option<ActivateFn>,
    full: Option<HitInsets>,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    hit_area_with(id, enabled, activate, full, HitLinks::default(), window, cx)
}

/// What a hit layer also belongs to: a gesture attachment's area (a group's slide, a
/// handle's hold) and a press surface whose paint a finger on it shows.
#[derive(Clone, Copy, Default)]
pub struct HitLinks<'a> {
    /// The gesture attachment whose recognizers a finger on the layer reaches.
    pub gestures: Option<&'a rustryx_primitives::gestures::host::GestureAttachment>,
    /// The press surface whose paint a finger on the layer shows.
    pub press: Option<&'a super::PressFeedback>,
}

/// [`hit_area`] whose layer is also part of a gesture attachment's area, so a finger
/// that lands on it starts the owner's gestures (a group's slide, a handle's hold).
pub fn hit_area_with(
    id: &ElementId,
    enabled: bool,
    activate: Option<ActivateFn>,
    full: Option<HitInsets>,
    links: HitLinks<'_>,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let HitLinks { gestures, press } = links;
    let key = key_of(id);
    let mut children = vec![target_probe(key).into_any_element()];
    if !enabled || !rustryx_primitives::coarse_primary_pointer(window, cx) {
        return children;
    }
    let now = cx.background_executor().now();
    let insets = hit_insets_with(window, key, now, full);
    if insets == NO_HIT_INSETS {
        return children;
    }
    let layer = div()
        .id(ElementId::Name(format!("{id:?}-hit-layer").into()))
        .absolute()
        .top(px(-insets.top))
        .left(px(-insets.left))
        .right(px(-insets.right))
        .bottom(px(-insets.bottom))
        // A finger on the grown area presses the control and shows its paint.
        .children(press.map(|p| p.grown_area_probe().absolute().top_0().left_0().size_full()));
    let layer = match activate {
        Some(activate) => layer
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                activate(window, cx);
            })
            .into_any_element(),
        None => layer.into_any_element(),
    };
    children.push(match gestures {
        Some(attachment) => {
            rustryx_primitives::gestures::host::gesture_area(attachment, layer).into_any_element()
        }
        None => layer,
    });
    children
}

/// [`hit_insets`] with a group's full growth in place of the centered one.
pub fn hit_insets_with(
    window: &Window,
    key: u64,
    now: Instant,
    full: Option<HitInsets>,
) -> HitInsets {
    match rustryx_primitives::gestures::hit_area::target_and_neighbours(window, key, now) {
        Some((own, neighbours)) => clamped_hit_insets(&own, &neighbours, MIN_TOUCH_TARGET, full),
        None => NO_HIT_INSETS,
    }
}

/// The box last noted for `key`.
pub(crate) fn noted_rect(window: &Window, key: u64) -> Option<HitRect> {
    let now = web_time::Instant::now();
    rustryx_primitives::gestures::hit_area::target_and_neighbours(window, key, now).map(|(r, _)| r)
}

/// Upstream `PART_REACH`: toward a field's part beside it (its text, spinner or status
/// icon), a clear button's band reaches at most this far, px.
pub(crate) const PART_REACH: f32 = 2.;

/// Given to a field's clear button (upstream `clearButtonGrowth`): which sides face
/// another part of the field, where its band stops at [`PART_REACH`].
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FieldPartReach {
    pub start: bool,
    pub end: bool,
}

impl FieldPartReach {
    /// Caps the insets toward the parts (`rtl` flips the inline sides).
    pub(crate) fn cap(self, mut insets: HitInsets, rtl: bool) -> HitInsets {
        let (left, right) = if rtl {
            (self.end, self.start)
        } else {
            (self.start, self.end)
        };
        if left {
            insets.left = insets.left.min(PART_REACH);
        }
        if right {
            insets.right = insets.right.min(PART_REACH);
        }
        insets
    }
}
