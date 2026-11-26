//! The motion-role registry, upstream `motion/roles.ts`: a role is one value
//! of a motion table (a spring, a duration whose key ends in `Ms`, or a
//! curve), named by its table path such as `springs.settle`. A table lists
//! its roles when it registers; every other value stays an internal
//! constant. The roles are the ones spec:AST-094 FR4 names.
//!
//! Upstream tells a role apart by the identity of the leaf object a call
//! hands in; here a role-carrying spring is a [`MotionSpring`] with its
//! role id, and a duration or curve is named by its path. Every shared UIKit
//! table is registered from the start (upstream registers each as the first
//! touch module that reads it loads); [`register_motion_table`] adds more.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use super::role_value::is_table_id;
pub use super::role_value::{
    MotionCurvePoints, MotionRoleKind, RoleValue, RoleValueError, format_motion_role_value,
    is_motion_role_id, motion_role_variable,
};
use super::uikit;
use super::{DurationBounce, MotionSpring, SpringDescription};

/// One registered role (upstream `MotionRole`).
#[derive(Clone, Debug, PartialEq)]
pub struct MotionRole {
    /// The table path, e.g. `springs.sheetDetent`.
    pub id: String,
    pub kind: MotionRoleKind,
    /// The CSS custom property that would carry the role's value.
    pub variable: String,
    /// The table's value.
    pub value: RoleValue,
}

/// A value in a motion table, as the registry walks it.
#[derive(Clone, Debug, PartialEq)]
pub enum TableValue {
    Spring(SpringDescription),
    Number(f64),
    /// A list of numbers: a curve when there are four.
    Numbers(Vec<f64>),
    Text(&'static str),
}

/// What [`register_motion_table`] registers of a table.
#[derive(Clone, Copy, Debug, Default)]
pub struct RegisterOptions<'a> {
    /// The role names, relative to the table, that become roles.
    pub roles: &'a [&'a str],
    /// Paths, relative to the table, that are not motion.
    pub exclude: &'a [&'a str],
    /// A path, relative to the table, and the role name it registers under.
    pub rename: &'a [(&'a str, &'a str)],
}

/// A shared table as the registry is seeded with it: its id, leaves, roles
/// and excluded paths.
type SeedTable = (
    &'static str,
    Vec<(String, TableValue)>,
    &'static [&'static str],
    &'static [&'static str],
);

#[derive(Default)]
struct Registry {
    roles: BTreeMap<String, MotionRole>,
    /// A table path to the role id it is registered under, when renamed.
    role_id_of_path: HashMap<String, String>,
}

impl Registry {
    fn seeded() -> Self {
        let mut registry = Self::default();
        let tables: [SeedTable; 10] = [
            (
                "springs",
                uikit::springs::leaves(),
                &uikit::springs::ROLES,
                &[],
            ),
            (
                "timing",
                uikit::timing::leaves(),
                &uikit::timing::ROLES,
                &uikit::timing::EXCLUDE,
            ),
            (
                "scroll",
                uikit::scroll::leaves(),
                &[],
                &uikit::scroll::EXCLUDE,
            ),
            (
                "picker",
                uikit::picker::leaves(),
                &uikit::picker::ROLES,
                &[],
            ),
            ("navigation", uikit::navigation::leaves(), &[], &[]),
            (
                "progress",
                uikit::progress_bar::leaves(),
                &uikit::progress_bar::ROLES,
                &[],
            ),
            (
                "segmented",
                uikit::segmented::leaves(),
                &uikit::segmented::ROLES,
                &[],
            ),
            (
                "switchControl",
                uikit::switch_control::leaves(),
                &uikit::switch_control::ROLES,
                &[],
            ),
            (
                "zoomMedia",
                uikit::zoom_media::leaves(),
                &uikit::zoom_media::ROLES,
                &[],
            ),
            (
                "paging",
                uikit::paging::leaves(),
                &[],
                &uikit::paging::EXCLUDE,
            ),
        ];
        for (id, leaves, roles, exclude) in tables {
            registry
                .register(
                    id,
                    &leaves,
                    RegisterOptions {
                        roles,
                        exclude,
                        rename: &[],
                    },
                )
                .expect("the shared tables register");
        }
        registry
    }

    fn register(
        &mut self,
        id: &str,
        leaves: &[(String, TableValue)],
        options: RegisterOptions<'_>,
    ) -> Result<(), String> {
        if !is_table_id(id) {
            return Err(format!(
                "motion table id \"{id}\" must be one camelCase word"
            ));
        }
        for (relative, value) in leaves {
            if options.exclude.contains(&relative.as_str()) {
                continue;
            }
            let key = relative.rsplit('.').next().unwrap_or(relative);
            let value = match value {
                TableValue::Spring(spring) => RoleValue::Spring(*spring),
                TableValue::Numbers(numbers) if numbers.len() == 4 => {
                    RoleValue::Curve([numbers[0], numbers[1], numbers[2], numbers[3]])
                }
                TableValue::Number(ms) if key.ends_with("Ms") => RoleValue::DurationMs(*ms),
                _ => continue,
            };
            let path = format!("{id}.{relative}");
            let name = options
                .rename
                .iter()
                .find(|(from, _)| *from == relative)
                .map_or(relative.as_str(), |(_, to)| *to);
            if !options.roles.contains(&name) {
                continue;
            }
            let role_id = format!("{id}.{name}");
            if role_id != path {
                self.role_id_of_path.insert(path, role_id.clone());
            }
            let variable = motion_role_variable(&role_id)?;
            self.roles.insert(
                role_id.clone(),
                MotionRole {
                    id: role_id,
                    kind: value.kind(),
                    variable,
                    value,
                },
            );
        }
        Ok(())
    }
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::seeded());
}

/// Registers a motion table (upstream `registerMotionTable`): each spring,
/// `…Ms` duration and four-number curve whose path `options.roles` lists
/// becomes a role named by its path under `id`. `Err` for an id that is
/// not one camelCase word.
pub fn register_motion_table(
    id: &str,
    leaves: &[(String, TableValue)],
    options: RegisterOptions<'_>,
) -> Result<(), String> {
    REGISTRY.with(|registry| registry.borrow_mut().register(id, leaves, options))
}

/// Registers `spring` as the spring role `id` and returns it as a
/// role-carrying spring (upstream `registerSpringRole`).
pub fn register_spring_role(
    id: &'static str,
    spring: SpringDescription,
) -> Result<MotionSpring, String> {
    if !is_motion_role_id(id) {
        return Err(format!(
            "motion role \"{id}\" must be a dotted table path such as \"springs.settle\""
        ));
    }
    let variable = motion_role_variable(id)?;
    REGISTRY.with(|registry| {
        registry.borrow_mut().roles.insert(
            id.to_string(),
            MotionRole {
                id: id.to_string(),
                kind: MotionRoleKind::Spring,
                variable,
                value: RoleValue::Spring(spring),
            },
        )
    });
    Ok(MotionSpring::role(id, spring))
}

/// The role a role-carrying spring plays, if its role is registered
/// (upstream `motionRoleOf`).
pub fn motion_role_of(spring: MotionSpring) -> Option<MotionRole> {
    spring
        .role
        .and_then(motion_role)
        .filter(|role| role.kind == MotionRoleKind::Spring)
}

/// The role the value at table path `path` plays, following a rename
/// (upstream `motionRoleIn`: the way to name a duration).
pub fn motion_role_in(path: &str) -> Option<MotionRole> {
    REGISTRY.with(|registry| {
        let registry = registry.borrow();
        let id = registry
            .role_id_of_path
            .get(path)
            .map_or(path, String::as_str);
        registry.roles.get(id).cloned()
    })
}

/// The role with this id, once its table is registered (upstream
/// `motionRole`).
pub fn motion_role(id: &str) -> Option<MotionRole> {
    REGISTRY.with(|registry| registry.borrow().roles.get(id).cloned())
}

/// Every registered role, by id (upstream `listMotionRoles`).
pub fn list_motion_roles() -> Vec<MotionRole> {
    REGISTRY.with(|registry| registry.borrow().roles.values().cloned().collect())
}

/// A number as upstream's `NUMBER` pattern reads it:
/// `-?(\d+\.?\d*|\.\d+)(e-?\d+)?`, returning the value and the rest.
fn number(text: &str) -> Option<(f64, &str)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    if bytes.first() == Some(&b'-') {
        i += 1;
    }
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let int = digits(i);
    i += int;
    if int > 0 {
        if bytes.get(i) == Some(&b'.') {
            i += 1;
            i += digits(i);
        }
    } else {
        if bytes.get(i) != Some(&b'.') {
            return None;
        }
        let fraction = digits(i + 1);
        if fraction == 0 {
            return None;
        }
        i += 1 + fraction;
    }
    if bytes.get(i) == Some(&b'e') {
        let mut j = i + 1;
        if bytes.get(j) == Some(&b'-') {
            j += 1;
        }
        let exponent = digits(j);
        if exponent > 0 {
            i = j + exponent;
        }
    }
    let value: f64 = text[..i].trim_end_matches('.').parse().ok()?;
    Some((value, &text[i..]))
}

fn parse_spring(value: &str) -> Option<DurationBounce> {
    let (duration, rest) = number(value)?;
    let rest = rest.strip_prefix('s')?;
    let trimmed = rest.trim_start();
    if trimmed.len() == rest.len() {
        return None;
    }
    let (bounce, rest) = number(trimmed)?;
    if !rest.is_empty() {
        return None;
    }
    (duration > 0. && bounce > -1. && bounce < 1.).then_some(DurationBounce { duration, bounce })
}

fn parse_duration(value: &str) -> Option<f64> {
    let (n, rest) = number(value)?;
    let ms = match rest {
        "ms" => n,
        "s" => n * 1000.,
        _ => return None,
    };
    (ms >= 0.).then_some(ms)
}

fn parse_curve(value: &str) -> Option<MotionCurvePoints> {
    let mut rest = value.strip_prefix("cubic-bezier(")?;
    let mut points = [0.; 4];
    for (i, point) in points.iter_mut().enumerate() {
        let (n, after) = number(rest.trim_start())?;
        *point = n;
        let after = after.trim_start();
        rest = if i < 3 {
            after.strip_prefix(',')?
        } else {
            after.strip_prefix(')')?
        };
    }
    if !rest.is_empty() {
        return None;
    }
    let [x1, _, x2, _] = points;
    ((0. ..=1.).contains(&x1) && (0. ..=1.).contains(&x2)).then_some(points)
}

/// A role value read back from its CSS form (upstream
/// `parseMotionRoleValue`): `None` for an empty or malformed value. A
/// spring reads as SwiftUI's duration and bounce.
pub fn parse_motion_role_value(kind: MotionRoleKind, text: &str) -> Option<RoleValue> {
    let value = text.trim();
    if value.is_empty() {
        return None;
    }
    match kind {
        MotionRoleKind::Spring => parse_spring(value).map(|DurationBounce { duration, bounce }| {
            RoleValue::Spring(SpringDescription::DurationBounce { duration, bounce })
        }),
        MotionRoleKind::Duration => parse_duration(value).map(RoleValue::DurationMs),
        MotionRoleKind::Curve => parse_curve(value).map(RoleValue::Curve),
    }
}

/// Reads a CSS easing, a keyword or a `cubic-bezier()`, as a curve role's
/// value is read (upstream `parseCssEasing`); `None` for any other easing.
pub fn parse_css_easing(text: &str) -> Option<MotionCurvePoints> {
    let value = text.trim();
    match value {
        "linear" => Some([0., 0., 1., 1.]),
        "ease" => Some([0.25, 0.1, 0.25, 1.]),
        "ease-in" => Some([0.42, 0., 1., 1.]),
        "ease-out" => Some([0., 0., 0.58, 1.]),
        "ease-in-out" => Some([0.42, 0., 0.58, 1.]),
        _ => parse_curve(value),
    }
}
