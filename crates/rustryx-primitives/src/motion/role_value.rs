//! The value half of the motion roles, upstream `motion/roleValue.ts`: role
//! ids, their CSS variable names and a role value's CSS form.

use super::{DurationBounce, SpringDescription};

/// The three kinds of value a role can hold (upstream `MotionRoleKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MotionRoleKind {
    Spring,
    Duration,
    Curve,
}

/// Cubic-bézier control points `[x1, y1, x2, y2]` (upstream
/// `MotionCurvePoints`).
pub type MotionCurvePoints = [f64; 4];

/// A role's value: a spring, a duration in ms, or a curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoleValue {
    Spring(SpringDescription),
    DurationMs(f64),
    Curve(MotionCurvePoints),
}

impl RoleValue {
    /// The kind of value this is.
    pub fn kind(&self) -> MotionRoleKind {
        match self {
            Self::Spring(_) => MotionRoleKind::Spring,
            Self::DurationMs(_) => MotionRoleKind::Duration,
            Self::Curve(_) => MotionRoleKind::Curve,
        }
    }
}

fn is_camel_word(part: &str) -> bool {
    let mut chars = part.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_alphanumeric())
}

/// Whether `id` is a well-formed role name, a dotted table path such as
/// `springs.settle` (upstream `isMotionRoleId`).
pub fn is_motion_role_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() >= 2 && parts.iter().all(|part| is_camel_word(part))
}

/// Whether `id` is one camelCase word, a table id.
pub(crate) fn is_table_id(id: &str) -> bool {
    is_camel_word(id)
}

/// The CSS custom property for a role: `--motion-` and the path in kebab
/// case (upstream `motionRoleVariable`). `Err` for an id that is no
/// dotted table path.
pub fn motion_role_variable(id: &str) -> Result<String, String> {
    if !is_motion_role_id(id) {
        return Err(format!(
            "motion role \"{id}\" must be a dotted table path such as \"springs.sheetPresent\""
        ));
    }
    let mut out = String::from("--motion");
    for part in id.split('.') {
        out.push('-');
        for c in part.chars() {
            if c.is_ascii_uppercase() {
                out.push('-');
                out.push(c.to_ascii_lowercase());
            } else {
                out.push(c);
            }
        }
    }
    Ok(out)
}

/// Six significant digits, as JavaScript prints them (upstream `short`).
pub fn short(n: f64) -> String {
    if n == 0. || !n.is_finite() {
        return format!("{}", if n == 0. { 0. } else { n });
    }
    let rounded: f64 = format!("{n:.5e}").parse().unwrap_or(n);
    format!("{rounded}")
}

/// Why a role value has no CSS form.
#[derive(Clone, Debug, PartialEq)]
pub enum RoleValueError {
    /// The value is not of the role's kind (upstream's `TypeError`).
    Type(String),
    /// The value is out of its kind's range (upstream's `RangeError`).
    Range(String),
}

impl std::fmt::Display for RoleValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Type(message) | Self::Range(message) => f.write_str(message),
        }
    }
}

/// A role value as its CSS custom property holds it (upstream
/// `formatMotionRoleValue`): a spring as SwiftUI's duration and bounce
/// (`0.5s 0.15`), a duration as `250ms`, a curve as `cubic-bezier(…)`.
pub fn format_motion_role_value(
    kind: MotionRoleKind,
    value: RoleValue,
) -> Result<String, RoleValueError> {
    match (kind, value) {
        (MotionRoleKind::Spring, RoleValue::Spring(spring)) => {
            let DurationBounce { duration, bounce } = spring.to_spring().duration_bounce();
            if bounce.is_nan() || bounce >= 1. {
                return Err(RoleValueError::Range(
                    "a spring must settle: its bounce must be below 1 (a damping ratio above 0)".into(),
                ));
            }
            Ok(format!("{}s {}", short(duration), short(bounce)))
        }
        (MotionRoleKind::Spring, _) => Err(RoleValueError::Type(
            "a spring is {response, dampingRatio}, {duration, bounce} or {mass?, stiffness, damping}".into(),
        )),
        (MotionRoleKind::Duration, RoleValue::DurationMs(ms)) if ms.is_finite() && ms >= 0. => {
            Ok(format!("{}ms", short(ms)))
        }
        (MotionRoleKind::Duration, _) => Err(RoleValueError::Type(
            "a duration is a finite number of milliseconds ≥ 0".into(),
        )),
        (MotionRoleKind::Curve, RoleValue::Curve(points)) if points.iter().all(|n| n.is_finite()) => {
            if points[0] < 0. || points[0] > 1. || points[2] < 0. || points[2] > 1. {
                return Err(RoleValueError::Range("a curve's x1 and x2 must lie in [0, 1]".into()));
            }
            let parts: Vec<String> = points.iter().map(|n| short(*n)).collect();
            Ok(format!("cubic-bezier({})", parts.join(", ")))
        }
        (MotionRoleKind::Curve, _) => Err(RoleValueError::Type(
            "a curve is four finite numbers [x1, y1, x2, y2]".into(),
        )),
    }
}
