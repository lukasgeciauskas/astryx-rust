//! What one call becomes under Reduce Motion, upstream
//! `motion/reducedMotionPlan.ts` (design:reduced-motion DR1–DR4).
//!
//! With `fade` (a coarse primary pointer, or a touch drag's release), a call
//! becomes an opacity-only cross-fade whose travel lands while the element
//! is transparent: an entrance from 0 lands at once and fades in, an exit to
//! 0 fades out in place and then lands, and any other call that moves dips
//! (fades out, lands, fades back in). Without `fade`, everything lands at
//! once. When no call owns the opacity, a dip or a fade by role starts from
//! and returns to the element's own opacity, which the plan reports as
//! `borrowed`. The planner is pure: it reads the animator only through
//! [`ReducedMotionView`].

use std::collections::BTreeMap;

use super::MotionProperty;
use super::uikit::timing::REDUCED_MOTION_CROSS_FADE;

/// The properties that move an element rather than fade it (upstream
/// `TRAVEL_PROPERTIES`).
pub const TRAVEL_PROPERTIES: [MotionProperty; 5] = MotionProperty::TRAVEL;

/// Which way a substituted cross-fade goes when the call's own opacity does
/// not say (upstream `crossfadeAs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossfadeAs {
    /// Lands the travel at once and fades the opacity in from 0.
    Entrance,
    /// Fades out in place, then lands the travel and restores the opacity.
    Exit,
}

/// A point of the opacity fade: ms after the call's start, and the value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FadePoint {
    pub at: f64,
    pub value: f64,
}

/// The fade back in that follows a landing (a dip).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FadeIn {
    pub to: f64,
    pub ms: f64,
    /// It returns to the element's own opacity, which owns it again once it
    /// ends.
    pub is_borrowed: bool,
}

/// The travel a plan writes later, while the element is transparent
/// (upstream `PlannedLanding`).
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedLanding {
    /// ms after the call's start.
    pub at: f64,
    pub values: BTreeMap<MotionProperty, f64>,
    pub fade_in: Option<FadeIn>,
    /// The opacity written with the landing (an exit by role).
    pub restore: Option<f64>,
    /// The restore returns to the element's own opacity.
    pub restore_is_borrowed: bool,
}

/// What the planner reads of the animator at the call's time (upstream
/// `ReducedMotionView`).
pub trait ReducedMotionView {
    fn value_at(&self, property: MotionProperty, time: f64) -> f64;
    /// Where the opacity is heading (a running fade's end, else its value).
    fn opacity_heading(&self) -> f64;
    /// The element's own opacity, where the animator writes none.
    fn own_opacity(&self) -> f64;
    /// Whether the animator writes the element's opacity itself.
    fn writes_own_opacity(&self) -> bool;
    /// The element's own opacity a running dip borrowed, if any.
    fn borrowed_opacity(&self) -> Option<f64>;
    fn has_opacity_curve(&self) -> bool;
    fn resting_opacity(&self) -> f64;
    /// Whether a move of an earlier call still waits to land.
    fn has_pending_landing(&self) -> bool;
    /// Whether an earlier dip's fade back in still waits on its landing.
    fn has_waiting_dip(&self) -> bool;
}

/// What Reduce Motion makes of a call (upstream `ReducedMotionPlan`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ReducedMotionPlan {
    /// Properties written at once, with no curve.
    pub at_once: Vec<MotionProperty>,
    /// The opacity fade, when the call fades.
    pub opacity_fade: Option<Vec<FadePoint>>,
    /// Values that stay where they are until the landing writes them.
    pub holds: BTreeMap<MotionProperty, f64>,
    pub landing: Option<PlannedLanding>,
    /// The element's own opacity a dip borrowed (no call owns opacity).
    pub borrowed: Option<f64>,
    /// The call joins the waiting dip: it names the opacity the dip returns
    /// to and moves nothing, so the fade goes on as it was.
    pub joins: bool,
}

/// The opacity no call has changed.
const IDENTITY_OPACITY: f64 = 1.;

/// Plans one call under Reduce Motion (upstream `planReducedMotion`): with
/// `fade`, an opacity-only cross-fade whose travel lands while the element
/// is transparent; without, everything at once. `role` gives a call that
/// names no opacity its direction.
pub fn plan_reduced_motion(
    view: &dyn ReducedMotionView,
    time: f64,
    targets: &BTreeMap<MotionProperty, f64>,
    properties: &[MotionProperty],
    fade: bool,
    role: Option<CrossfadeAs>,
) -> ReducedMotionPlan {
    let mut holds = BTreeMap::new();
    if !fade {
        return ReducedMotionPlan {
            at_once: properties.to_vec(),
            ..Default::default()
        };
    }
    let at_once: Vec<MotionProperty> = properties
        .iter()
        .copied()
        .filter(|p| p.is_travel())
        .collect();
    let opacity_now = view.value_at(MotionProperty::Opacity, time);
    let opacity_to = targets
        .get(&MotionProperty::Opacity)
        .copied()
        .unwrap_or_else(|| view.opacity_heading());
    let travel: Vec<MotionProperty> = properties
        .iter()
        .copied()
        .filter(|p| p.is_travel() && (targets[p] - view.value_at(*p, time)).abs() > 1e-9)
        .collect();
    let named = targets.contains_key(&MotionProperty::Opacity);
    let mut entering = named && opacity_to > opacity_now + 1e-6;
    let mut leaving = named && opacity_to < opacity_now - 1e-6;
    let mut fade_from = opacity_now;
    let mut fade_to = opacity_to;
    let mut restore = None;
    let role_borrow = (role.is_some()
        && !named
        && view.borrowed_opacity().is_none()
        && !view.has_opacity_curve()
        && view.resting_opacity() == IDENTITY_OPACITY
        && !view.writes_own_opacity())
    .then(|| view.own_opacity());
    if !entering && !leaving && role == Some(CrossfadeAs::Entrance) {
        entering = true;
        fade_from = if view.has_opacity_curve() {
            opacity_now
        } else {
            0.
        };
        fade_to = role_borrow.unwrap_or(opacity_to);
    } else if !entering && !leaving && role == Some(CrossfadeAs::Exit) {
        leaving = true;
        fade_from = role_borrow.unwrap_or(opacity_now);
        fade_to = 0.;
        restore = Some(role_borrow.unwrap_or(opacity_to));
    }
    let pending = view.has_pending_landing();
    let ms = REDUCED_MOTION_CROSS_FADE.duration_ms;
    let lands_later =
        |at: f64, fade_in: Option<FadeIn>, holds: &mut BTreeMap<MotionProperty, f64>| {
            let mut values = BTreeMap::new();
            for property in &travel {
                holds.insert(*property, view.value_at(*property, time));
                values.insert(*property, targets[property]);
            }
            PlannedLanding {
                at,
                values,
                fade_in,
                restore,
                restore_is_borrowed: role_borrow.is_some() && restore.is_some(),
            }
        };
    if named
        && view.has_waiting_dip()
        && !properties.iter().any(|p| p.is_travel())
        && (opacity_to - view.opacity_heading()).abs() < 1e-6
    {
        return ReducedMotionPlan {
            joins: true,
            ..Default::default()
        };
    }
    let carries = !travel.is_empty() || pending;
    let fade_points = |from: f64, to: f64, at: f64| {
        Some(vec![
            FadePoint {
                at: 0.,
                value: from,
            },
            FadePoint { at, value: to },
        ])
    };
    if entering && !pending && (!carries || fade_from <= 1e-6) {
        return ReducedMotionPlan {
            at_once,
            opacity_fade: fade_points(fade_from, fade_to, ms),
            holds,
            landing: None,
            borrowed: role_borrow,
            joins: false,
        };
    }
    if leaving && (!carries || fade_to <= 1e-6) {
        let landing = (!travel.is_empty() || restore.is_some() || pending)
            .then(|| lands_later(ms, None, &mut holds));
        return ReducedMotionPlan {
            at_once,
            opacity_fade: fade_points(fade_from, fade_to, ms),
            holds,
            landing,
            borrowed: None,
            joins: false,
        };
    }
    if carries {
        let prior_borrow = view.borrowed_opacity();
        let owns_opacity = named
            || (prior_borrow.is_none()
                && (view.has_opacity_curve()
                    || view.resting_opacity() != IDENTITY_OPACITY
                    || view.writes_own_opacity()));
        let borrowed = if owns_opacity {
            None
        } else {
            Some(prior_borrow.unwrap_or_else(|| view.own_opacity()))
        };
        if borrowed.is_some_and(|b| b <= 1e-6) {
            return ReducedMotionPlan {
                at_once,
                holds,
                ..Default::default()
            };
        }
        let open = if view.has_opacity_curve() {
            opacity_now
        } else {
            borrowed.unwrap_or(opacity_now)
        };
        let landing = lands_later(
            ms / 2.,
            Some(FadeIn {
                to: borrowed.unwrap_or(opacity_to),
                ms: ms / 2.,
                is_borrowed: borrowed.is_some(),
            }),
            &mut holds,
        );
        return ReducedMotionPlan {
            at_once,
            opacity_fade: fade_points(open, 0., ms / 2.),
            holds,
            landing: Some(landing),
            borrowed,
            joins: false,
        };
    }
    let mut at_once = at_once;
    if named {
        at_once.push(MotionProperty::Opacity);
    }
    ReducedMotionPlan {
        at_once,
        holds,
        ..Default::default()
    }
}
