//! The starter icon sets (upstream lab `SVGIcon/icons.ts` and `weightedIcons.ts`),
//! generated from upstream (`spec/svg-icons.json`): the fifteen Lucide-based starter
//! icons with per-shape roles, and the five-weight filled drawings WeightedSVGIcon
//! demonstrates with.

use super::build::{WeightedIconDef, WeightedIconLayer, WeightedIconPath};
use super::generated::{GenIconDef, GenShape, GenWeightedDef, STARTER_ICONS, WEIGHTED_ICONS};
use super::{IconShape, IconShapeRole, IconShapeType, SVGIconDef};

fn shape(s: &GenShape) -> IconShape {
    IconShape {
        ty: IconShapeType::from_tag(s.ty).unwrap_or(IconShapeType::Path),
        attrs: s
            .attrs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        role: s.role.map(|r| {
            if r == "stroke" {
                IconShapeRole::Stroke
            } else {
                IconShapeRole::Fill
            }
        }),
    }
}

fn icon(def: &GenIconDef) -> SVGIconDef {
    SVGIconDef {
        name: def.name.into(),
        view_box: def.view_box.map(Into::into),
        primary: def.primary.iter().map(shape).collect(),
        secondary: def.secondary.map(|s| s.iter().map(shape).collect()),
    }
}

fn weighted(def: &GenWeightedDef) -> WeightedIconDef {
    WeightedIconDef {
        name: def.name.into(),
        view_box: def.view_box.map(Into::into),
        layers: def
            .layers
            .iter()
            .map(|l| WeightedIconLayer {
                from: l.from,
                to: l.to,
                paths: l
                    .paths
                    .iter()
                    .map(|(d, even_odd)| WeightedIconPath {
                        d: (*d).into(),
                        even_odd: even_odd.then_some(true),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn starter(export: &str) -> SVGIconDef {
    icon(
        STARTER_ICONS
            .iter()
            .find(|i| i.export == export)
            .expect("a generated starter icon"),
    )
}

fn weighted_starter(export: &str) -> WeightedIconDef {
    weighted(
        WEIGHTED_ICONS
            .iter()
            .find(|i| i.export == export)
            .expect("a generated weighted icon"),
    )
}

/// Upstream `starterIcons`: every starter icon, in order.
pub fn starter_icons() -> Vec<SVGIconDef> {
    STARTER_ICONS.iter().map(icon).collect()
}

/// Upstream `weightedStarterIcons`: every weighted demo icon, in order.
pub fn weighted_starter_icons() -> Vec<WeightedIconDef> {
    WEIGHTED_ICONS.iter().map(weighted).collect()
}

macro_rules! starters {
    ($($fn_name:ident => $export:literal, $doc:literal;)*) => {
        $(
            #[doc = $doc]
            pub fn $fn_name() -> SVGIconDef {
                starter($export)
            }
        )*
    };
}

macro_rules! weighted_starters {
    ($($fn_name:ident => $export:literal, $doc:literal;)*) => {
        $(
            #[doc = $doc]
            pub fn $fn_name() -> WeightedIconDef {
                weighted_starter($export)
            }
        )*
    };
}

starters! {
    x_icon => "xIcon", "Upstream `xIcon`: X / Close, two stroke-role lines.";
    check_icon => "checkIcon", "Upstream `checkIcon`: a stroke-role check.";
    bell_icon => "bellIcon", "Upstream `bellIcon`: a fill-role body and a stroke-role clapper.";
    home_icon => "homeIcon", "Upstream `homeIcon`: a fill-role house and door (masked out in bold).";
    settings_icon => "settingsIcon", "Upstream `settingsIcon`: a fill-role gear and center circle.";
    calendar_icon => "calendarIcon", "Upstream `calendarIcon`: a fill-role frame, stroke-role pegs and divider.";
    menu_icon => "menuIcon", "Upstream `menuIcon`: three stroke-role lines.";
    heart_icon => "heartIcon", "Upstream `heartIcon`: one curved fill-role path.";
    eye_icon => "eyeIcon", "Upstream `eyeIcon`: a fill-role outline and pupil.";
    star_icon => "starIcon", "Upstream `starIcon`: one fill-role path.";
    folder_icon => "folderIcon", "Upstream `folderIcon`.";
    shield_icon => "shieldIcon", "Upstream `shieldIcon`.";
    search_icon => "searchIcon", "Upstream `searchIcon`: a stroke-role glass and handle.";
    mail_icon => "mailIcon", "Upstream `mailIcon`: a fill-role body and a stroke-role flap.";
    lock_icon => "lockIcon", "Upstream `lockIcon`: a fill-role body and a stroke-role shackle.";
}

weighted_starters! {
    x_weighted_icon => "xWeightedIcon", "Upstream `xWeightedIcon`.";
    check_weighted_icon => "checkWeightedIcon", "Upstream `checkWeightedIcon`.";
    bell_weighted_icon => "bellWeightedIcon", "Upstream `bellWeightedIcon`.";
    home_weighted_icon => "homeWeightedIcon", "Upstream `homeWeightedIcon`.";
    settings_weighted_icon => "settingsWeightedIcon", "Upstream `settingsWeightedIcon`.";
    calendar_weighted_icon => "calendarWeightedIcon", "Upstream `calendarWeightedIcon`.";
    menu_weighted_icon => "menuWeightedIcon", "Upstream `menuWeightedIcon`.";
    heart_weighted_icon => "heartWeightedIcon", "Upstream `heartWeightedIcon`.";
    eye_weighted_icon => "eyeWeightedIcon", "Upstream `eyeWeightedIcon`.";
    star_weighted_icon => "starWeightedIcon", "Upstream `starWeightedIcon`.";
    folder_weighted_icon => "folderWeightedIcon", "Upstream `folderWeightedIcon`.";
    shield_weighted_icon => "shieldWeightedIcon", "Upstream `shieldWeightedIcon`.";
    search_weighted_icon => "searchWeightedIcon", "Upstream `searchWeightedIcon`.";
    mail_weighted_icon => "mailWeightedIcon", "Upstream `mailWeightedIcon`.";
    lock_weighted_icon => "lockWeightedIcon", "Upstream `lockWeightedIcon`.";
}
