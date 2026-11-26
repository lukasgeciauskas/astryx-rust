//! The resize model's pure parts, upstream `Resizable/resizableModel.ts` and
//! `utils.ts`: sizes in pixels or a share of a basis, clamping and snapping, the
//! keyboard's snap steps, and the persisted entry.

use astryx_primitives::storage;
use gpui::App;

pub const DEFAULT_MIN: f32 = 50.;
pub const DEFAULT_COLLAPSED_SIZE: f32 = 40.;
pub const DEFAULT_SIZE_FALLBACK: f32 = 250.;
/// The basis a container percentage resolves against until the container is measured.
pub const SERVER_BASIS: f32 = 1200.;
pub const STORAGE_PREFIX: &str = "astryx-resizable:";
/// How far an arrow key moves the separator; Shift moves it further.
pub const KEYBOARD_STEP: f32 = 10.;
pub const KEYBOARD_LARGE_STEP: f32 = 50.;

/// Upstream `ResizableDirection`: the axis a region resizes along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizeDirection {
    /// Its width.
    #[default]
    Horizontal,
    /// Its height.
    Vertical,
}

impl ResizeDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            ResizeDirection::Horizontal => "horizontal",
            ResizeDirection::Vertical => "vertical",
        }
    }
}

/// Upstream `ResizableSize`: pixels, an exact share of the basis, or a share with
/// one pixel floor or ceiling (`percent(value, {min | max: pixel(value)})`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResizableSize {
    Pixels(f32),
    Percent {
        value: f32,
        min: Option<f32>,
        max: Option<f32>,
    },
}

impl ResizableSize {
    /// Upstream `pixel(value)`.
    pub fn pixel(value: f32) -> Self {
        ResizableSize::Pixels(value)
    }

    /// An exact `"N%"`: a share of the basis with no pixel bound.
    pub fn percent(value: f32) -> Self {
        ResizableSize::Percent {
            value,
            min: None,
            max: None,
        }
    }

    /// `percent(value, {min: pixel(floor)})`.
    pub fn percent_at_least(value: f32, floor: f32) -> Self {
        ResizableSize::Percent {
            value,
            min: Some(floor),
            max: None,
        }
    }

    /// `percent(value, {max: pixel(ceiling)})`.
    pub fn percent_at_most(value: f32, ceiling: f32) -> Self {
        ResizableSize::Percent {
            value,
            min: None,
            max: Some(ceiling),
        }
    }

    /// Upstream's exact strings: `"Npx"` or `"N%"` (0 to 100).
    pub fn parse(text: &str) -> Option<Self> {
        if let Some(px) = text.strip_suffix("px") {
            let value: f32 = px.parse().ok()?;
            return (value.is_finite() && value >= 0. && !px.starts_with('+'))
                .then_some(ResizableSize::Pixels(value));
        }
        let percent = text.strip_suffix('%')?;
        let value: f32 = percent.parse().ok()?;
        (value.is_finite() && (0. ..=100.).contains(&value) && !percent.starts_with('+'))
            .then(|| ResizableSize::percent(value))
    }

    /// Whether the size is one upstream accepts.
    pub fn is_valid(&self) -> bool {
        let pixel = |v: f32| v.is_finite() && v >= 0.;
        match *self {
            ResizableSize::Pixels(v) => pixel(v),
            ResizableSize::Percent { value, min, max } => {
                value.is_finite()
                    && (0. ..=100.).contains(&value)
                    && min.is_none_or(pixel)
                    && max.is_none_or(pixel)
                    && !(min.is_some() && max.is_some())
            }
        }
    }

    /// Whether it must be recomputed when its basis changes.
    pub fn depends_on_basis(&self) -> bool {
        matches!(self, ResizableSize::Percent { .. })
    }

    /// Upstream `toPixels`: resolved against `basis`, then its one pixel bound.
    pub fn to_pixels(&self, basis: Option<f32>) -> Option<f32> {
        match *self {
            ResizableSize::Pixels(v) => Some(v),
            ResizableSize::Percent { value, min, max } => {
                let share = (value / 100. * basis?).round();
                Some(match (min, max) {
                    (Some(min), _) => share.max(min),
                    (_, Some(max)) => share.min(max),
                    _ => share,
                })
            }
        }
    }
}

impl From<f32> for ResizableSize {
    fn from(px: f32) -> Self {
        ResizableSize::Pixels(px)
    }
}

/// Upstream `resolveBound`: a configured value in pixels, or its role's fallback.
pub fn resolve_bound(
    raw: Option<ResizableSize>,
    basis: f32,
    fallback: f32,
    label: &str,
    source: &str,
) -> f32 {
    let Some(raw) = raw else {
        return fallback;
    };
    if !raw.is_valid() {
        let key = format!("{source}:{label}:{raw:?}");
        astryx_primitives::diagnostics::dev_warn_once(&key, || {
            format!(
                "{source}: {label} {raw:?} is not a size. Use non-negative pixels, a share \
                 from 0 to 100%, or a share with one pixel floor or ceiling. Falling back \
                 to {fallback}."
            )
        });
        return fallback;
    }
    raw.to_pixels(Some(basis)).unwrap_or(fallback)
}

/// Upstream `clampSize`: within the bounds, then the nearest snap point.
pub fn clamp_size(size: f32, min: f32, max: f32, snaps: &[f32]) -> f32 {
    let clamped = size.max(min).min(max);
    match snaps
        .iter()
        .copied()
        .min_by(|a, b| (clamped - a).abs().total_cmp(&(clamped - b).abs()))
    {
        Some(nearest) => nearest.max(min).min(max),
        None => clamped,
    }
}

/// The snap point `count` points away from `size` toward `grow` (upstream
/// `adjacentSnapPoint`); a point past a bound counts as the bound.
pub fn adjacent_snap_point(
    size: f32,
    min: f32,
    max: f32,
    snaps: &[f32],
    grow: bool,
    count: usize,
) -> f32 {
    let mut points: Vec<f32> = snaps.iter().map(|p| p.max(min).min(max)).collect();
    points.sort_by(f32::total_cmp);
    points.dedup();
    let ahead: Vec<f32> = if grow {
        points.into_iter().filter(|p| *p > size).collect()
    } else {
        points.into_iter().filter(|p| *p < size).rev().collect()
    };
    match ahead.len() {
        0 => size,
        n => ahead[count.min(n) - 1],
    }
}

/// Upstream `PersistedResizableState`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Persisted {
    /// The expanded size, when the entry carries a usable one.
    pub size: Option<f32>,
    /// The collapse state, or `None` for a legacy width-only entry.
    pub is_collapsed: Option<bool>,
}

/// Upstream `loadPersistedState`: the current `{size, isCollapsed}` form, a legacy
/// width, or a legacy `0` (collapsed, no size).
pub fn load_persisted(key: &str, cx: &mut App) -> Option<Persisted> {
    let raw = storage::read(cx, &format!("{STORAGE_PREFIX}{key}"))?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if let Some(n) = parsed.as_f64() {
        if !n.is_finite() {
            return None;
        }
        return Some(if n == 0. {
            Persisted {
                size: None,
                is_collapsed: Some(true),
            }
        } else {
            Persisted {
                size: Some(n as f32),
                is_collapsed: None,
            }
        });
    }
    let size = parsed
        .get("size")
        .and_then(|s| s.as_f64())
        .filter(|s| s.is_finite() && *s > 0.)
        .map(|s| s as f32);
    let collapsed = parsed.get("isCollapsed").and_then(|c| c.as_bool()) == Some(true);
    (size.is_some() || collapsed).then_some(Persisted {
        size,
        is_collapsed: Some(collapsed),
    })
}

/// Upstream `persistState`.
pub fn persist(key: &str, size: f32, is_collapsed: bool, cx: &mut App) {
    let value = serde_json::json!({ "size": size, "isCollapsed": is_collapsed });
    storage::write(cx, &format!("{STORAGE_PREFIX}{key}"), &value.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr5_sizes_parse_and_resolve_against_one_basis() {
        assert_eq!(
            ResizableSize::parse("240px"),
            Some(ResizableSize::Pixels(240.))
        );
        assert_eq!(
            ResizableSize::parse("30%"),
            Some(ResizableSize::percent(30.))
        );
        assert_eq!(ResizableSize::parse("120%"), None);
        assert_eq!(ResizableSize::parse("30 %"), None);
        assert_eq!(
            ResizableSize::percent(30.).to_pixels(Some(1000.)),
            Some(300.)
        );
        assert_eq!(
            ResizableSize::percent_at_least(10., 200.).to_pixels(Some(1000.)),
            Some(200.)
        );
        assert_eq!(
            ResizableSize::percent_at_most(50., 300.).to_pixels(Some(1000.)),
            Some(300.)
        );
        let both = ResizableSize::Percent {
            value: 10.,
            min: Some(1.),
            max: Some(2.),
        };
        assert!(!both.is_valid(), "a descriptor with both bounds is invalid");
        assert_eq!(resolve_bound(Some(both), 1000., 50., "minSize", "t"), 50.);
    }

    #[test]
    fn clamping_snaps_to_the_nearest_point_within_the_bounds() {
        assert_eq!(clamp_size(330., 100., 600., &[200., 400.]), 400.);
        assert_eq!(clamp_size(700., 100., 600., &[]), 600.);
        assert_eq!(
            adjacent_snap_point(300., 0., 1000., &[200., 400., 600.], true, 2),
            600.
        );
        assert_eq!(
            adjacent_snap_point(300., 0., 1000., &[200., 400.], false, 1),
            200.
        );
        assert_eq!(
            adjacent_snap_point(600., 0., 1000., &[200., 400.], true, 1),
            600.
        );
    }
}
