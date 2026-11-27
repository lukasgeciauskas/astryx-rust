//! The weighted-icon build step (upstream lab `SVGIcon/buildWeightedIcon.ts`): one SVG
//! drawing per weight becomes one layer per distinct drawing, each covering a contiguous
//! weight range. Identical drawings at neighboring weights share one layer; passing
//! `weights` keeps only the drawings a product can reach, and every other weight shows
//! the nearest kept drawing, so the icon never renders empty.

use std::collections::BTreeMap;

use fancy_regex::Regex;
use gpui::SharedString;

/// Upstream `ICON_WEIGHTS`.
pub const ICON_WEIGHTS: [IconWeight; 5] = [300, 400, 500, 600, 700];

/// Upstream `IconWeight`: 300, 400, 500, 600 or 700.
pub type IconWeight = u16;

/// Upstream `WeightedIconPath`: one filled path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WeightedIconPath {
    /// SVG path data.
    pub d: SharedString,
    /// Upstream `evenOdd`: the even-odd fill rule, when set (`Some(true)`). Kept as
    /// given, so drawings compare as upstream's JSON does.
    pub even_odd: Option<bool>,
}

/// Upstream `WeightedIconLayer`: a drawing and the weights it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightedIconLayer {
    /// The lightest weight it covers (inclusive).
    pub from: IconWeight,
    /// The heaviest weight it covers (inclusive).
    pub to: IconWeight,
    /// The filled paths of the drawing.
    pub paths: Vec<WeightedIconPath>,
}

/// Upstream `WeightedIconDef`: an icon whose drawing changes with the surrounding font
/// weight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightedIconDef {
    /// Unique name; also the sprite symbol id's suffix.
    pub name: SharedString,
    /// The view box (`0 0 24 24` when unset).
    pub view_box: Option<SharedString>,
    /// Drawings from lightest to heaviest, together covering 300 to 700.
    pub layers: Vec<WeightedIconLayer>,
}

/// Upstream `BuildWeightedIconOptions`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildWeightedIconOptions {
    /// The view box (`0 0 24 24` when unset).
    pub view_box: Option<SharedString>,
    /// The weights a product can reach (all five when unset): other weights show the
    /// nearest kept drawing, the heavier one on a tie.
    pub weights: Option<Vec<IconWeight>>,
}

/// One weight's drawing: SVG markup to read the paths from, or the paths themselves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WeightedDrawing {
    Svg(String),
    Paths(Vec<WeightedIconPath>),
}

/// Upstream `parseSVGPaths`: the filled paths of a single-color SVG drawing.
pub fn parse_svg_paths(svg: &str) -> Vec<WeightedIconPath> {
    thread_local! {
        static PATH: Regex = Regex::new(r"<path\b([^>]*)>").expect("the path pattern");
        static D: Regex = Regex::new(r#"\sd=(["'])([^"']+)\1"#).expect("the d pattern");
        static EVEN_ODD: Regex = Regex::new(r#"fill-rule=["']evenodd["']"#).expect("the fill-rule pattern");
    }
    let mut out = Vec::new();
    PATH.with(|path| {
        for found in path.captures_iter(svg).flatten() {
            let attrs = found.get(1).map_or("", |m| m.as_str());
            let Some(d) = D.with(|d| {
                d.captures(attrs)
                    .ok()
                    .flatten()
                    .and_then(|c| c.get(2).map(|m| m.as_str().to_string()))
            }) else {
                continue;
            };
            let even_odd = EVEN_ODD.with(|e| e.is_match(attrs).unwrap_or(false));
            out.push(WeightedIconPath {
                d: d.into(),
                even_odd: even_odd.then_some(true),
            });
        }
    });
    out
}

/// Upstream `nearestKept`: the kept weight nearest `weight`, the heavier on a tie.
fn nearest_kept(weight: IconWeight, kept: &[IconWeight]) -> IconWeight {
    let mut best = kept[0];
    for &candidate in kept {
        let distance = (i32::from(candidate) - i32::from(weight)).abs();
        let best_distance = (i32::from(best) - i32::from(weight)).abs();
        if distance < best_distance || (distance == best_distance && candidate > best) {
            best = candidate;
        }
    }
    best
}

/// Upstream `buildWeightedIcon`: a weighted icon from one drawing per weight. Fails
/// as upstream throws when no kept weight has a drawing.
pub fn build_weighted_icon(
    name: impl Into<SharedString>,
    drawings: &BTreeMap<IconWeight, WeightedDrawing>,
    options: &BuildWeightedIconOptions,
) -> Result<WeightedIconDef, String> {
    let name: SharedString = name.into();
    let available: Vec<IconWeight> = ICON_WEIGHTS
        .iter()
        .copied()
        .filter(|w| drawings.contains_key(w))
        .collect();
    let kept: Vec<IconWeight> = options
        .weights
        .clone()
        .unwrap_or_else(|| ICON_WEIGHTS.to_vec())
        .into_iter()
        .filter(|w| available.contains(w))
        .collect();
    if kept.is_empty() {
        return Err(format!(
            "buildWeightedIcon({name}): no drawing for any kept weight"
        ));
    }
    let paths_for = |w: IconWeight| -> Vec<WeightedIconPath> {
        match drawings.get(&nearest_kept(w, &kept)) {
            Some(WeightedDrawing::Svg(svg)) => parse_svg_paths(svg),
            Some(WeightedDrawing::Paths(paths)) => paths.clone(),
            None => Vec::new(),
        }
    };
    let mut layers: Vec<WeightedIconLayer> = Vec::new();
    for weight in ICON_WEIGHTS {
        let paths = paths_for(weight);
        match layers.last_mut() {
            Some(previous) if previous.paths == paths => previous.to = weight,
            _ => layers.push(WeightedIconLayer {
                from: weight,
                to: weight,
                paths,
            }),
        }
    }
    Ok(WeightedIconDef {
        name,
        view_box: options.view_box.clone(),
        layers,
    })
}
