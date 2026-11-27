//! Upstream lab `SVGIcon.test.tsx` and `WeightedSVGIcon.test.tsx`, the parts that are
//! not DOM attributes, plus the resolved variation table and the weight rules.

use std::collections::BTreeMap;

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, FontWeight, Render, TestAppContext, Window, div, px};

use super::build::WeightedDrawing;
use super::weighted::{Sprites, current_icon_weight, layer_document};
use super::*;

fn docs(icon: &SVGIconDef, variation: SVGIconVariation) -> Vec<LayerDocument> {
    layer_documents(
        icon,
        &icon_vars(variation, SVGIconSize::Md, None),
        variation,
    )
}

fn groups(docs: &[LayerDocument]) -> Vec<&'static str> {
    docs.iter().map(|d| d.group).collect()
}

#[test]
fn renders_each_shape_with_its_attributes_inside_the_default_view_box() {
    let docs = docs(&x_icon(), SVGIconVariation::Linear);
    assert_eq!(groups(&docs), ["primary-stroke"]);
    let svg = &docs[0].svg;
    assert!(svg.contains("viewBox=\"0 0 24 24\""), "{svg}");
    assert_eq!(svg.matches("<line").count(), 2);
    assert!(
        svg.contains("<line x1=\"18\" y1=\"6\" x2=\"6\" y2=\"18\"/>"),
        "{svg}"
    );
}

#[test]
fn splits_fill_role_and_stroke_role_shapes_into_separate_layer_groups() {
    // Primary fill (the frame), primary stroke (two pegs), secondary stroke (the
    // divider): no secondary fill group.
    let docs = docs(&calendar_icon(), SVGIconVariation::Linear);
    assert_eq!(
        groups(&docs),
        ["primary-fill", "primary-stroke", "secondary-stroke"]
    );
    assert_eq!(docs[0].svg.matches("<rect").count(), 1);
    assert!(docs[0].svg.contains("width=\"18\""));
    assert_eq!(docs[1].svg.matches("<line").count(), 2);
    assert_eq!(docs[2].svg.matches("<line").count(), 1);
    assert!(docs[2].svg.contains("y1=\"10\""));
}

#[test]
fn knocks_secondary_shapes_out_of_the_primary_layer_with_a_mask_in_bold() {
    let docs = docs(&home_icon(), SVGIconVariation::Bold);
    // The secondary layer is dropped: it only exists as the knockout.
    assert_eq!(groups(&docs), ["primary-fill"]);
    let svg = &docs[0].svg;
    assert!(svg.contains("<defs><mask id=\"knockout\""), "{svg}");
    assert!(svg.contains("<rect width=\"24\" height=\"24\" fill=\"white\"/>"));
    let knockout = &svg[svg.find("<path d=\"M15").expect("the door knockout")..];
    assert!(
        knockout.contains(
            "fill=\"black\" stroke=\"black\" stroke-width=\"0.75\" stroke-linecap=\"round\""
        ),
        "{knockout}"
    );
    assert!(svg.contains("mask=\"url(#knockout)\""));
    // Bold fills the primary layer.
    assert!(svg.contains("<g fill=\"black\" stroke=\"none\""));
}

#[test]
fn a_stroke_role_knockout_covers_its_stroke_and_the_gap() {
    let docs = docs(&bell_icon(), SVGIconVariation::Bold);
    let svg = &docs[0].svg;
    // 1.5 + 0.75 * 2, and no fill for a stroke-role shape.
    assert!(
        svg.contains("fill=\"none\" stroke=\"black\" stroke-width=\"3\""),
        "{svg}"
    );
}

#[test]
fn keeps_the_secondary_layer_visible_without_a_mask_outside_bold() {
    let docs = docs(&home_icon(), SVGIconVariation::Linear);
    assert_eq!(groups(&docs), ["primary-fill", "secondary-fill"]);
    assert!(!docs[0].svg.contains("mask"));
    assert!(
        docs[1]
            .svg
            .contains("d=\"M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8\"")
    );
}

#[test]
fn skips_the_mask_in_bold_when_there_is_nothing_to_knock_out() {
    let docs = docs(&x_icon(), SVGIconVariation::Bold);
    assert!(docs.iter().all(|d| !d.svg.contains("mask")));
    assert_eq!(
        docs.iter()
            .map(|d| d.svg.matches("<line").count())
            .sum::<usize>(),
        2
    );
}

#[test]
fn pins_the_stroke_width_when_set() {
    assert_eq!(
        icon_vars(SVGIconVariation::Linear, SVGIconSize::Md, Some(3.)).stroke_width,
        3.
    );
    // Without the override the width stays with the size preset.
    assert_eq!(
        icon_vars(SVGIconVariation::Linear, SVGIconSize::Xsm, None).stroke_width,
        2.
    );
    assert_eq!(
        icon_vars(SVGIconVariation::Linear, SVGIconSize::Sm, None).stroke_width,
        1.75
    );
    let vars = icon_vars(SVGIconVariation::Linear, SVGIconSize::Md, Some(3.));
    let docs = layer_documents(&home_icon(), &vars, SVGIconVariation::Linear);
    assert!(docs[0].svg.contains("stroke-width=\"3\""));
}

#[test]
fn honors_a_custom_view_box() {
    let dot = SVGIconDef {
        name: "Dot".into(),
        view_box: Some("0 0 32 32".into()),
        primary: vec![IconShape::new(
            IconShapeType::Circle,
            [("cx", "16"), ("cy", "16"), ("r", "8")],
        )],
        secondary: None,
    };
    let docs = docs(&dot, SVGIconVariation::Linear);
    // The role defaults to fill: the circle lands in the fill-role group.
    assert_eq!(groups(&docs), ["primary-fill"]);
    assert!(docs[0].svg.contains("viewBox=\"0 0 32 32\""));
    assert!(docs[0].svg.contains("r=\"8\""));
}

#[test]
fn each_variation_sets_upstreams_layer_properties() {
    let v = |variation| icon_vars(variation, SVGIconSize::Md, None);
    let twotone = v(SVGIconVariation::Twotone);
    assert_eq!(
        (
            twotone.primary.opacity,
            twotone.primary.stroke_role_opacity,
            twotone.secondary.opacity
        ),
        (1., 0.4, 0.4)
    );
    let bold = v(SVGIconVariation::Bold);
    assert_eq!(
        (bold.primary.fill, bold.primary.stroke),
        (IconPaint::CurrentColor, IconPaint::None)
    );
    assert_eq!(
        (
            bold.primary.stroke_role_width,
            bold.secondary.stroke_role_width
        ),
        (2., 2.5)
    );
    let broken = v(SVGIconVariation::Broken);
    assert_eq!(
        (
            broken.secondary.opacity,
            broken.secondary.stroke_role_opacity
        ),
        (0., 0.)
    );
    let bulk = v(SVGIconVariation::Bulk);
    assert_eq!(
        (bulk.secondary.fill, bulk.secondary.opacity),
        (IconPaint::CurrentColor, 0.4)
    );
    let linear = v(SVGIconVariation::Linear);
    assert_eq!((linear.primary.stroke_role_width, linear.gap), (1.5, 0.75));
}

#[test]
fn camel_case_attributes_become_svg_attributes() {
    let shape = IconShape::new(
        IconShapeType::Path,
        [("d", "M0 0"), ("fillRule", "evenodd"), ("pathLength", "1")],
    );
    let icon = SVGIconDef {
        name: "P".into(),
        view_box: None,
        primary: vec![shape],
        secondary: None,
    };
    let svg = &docs(&icon, SVGIconVariation::Linear)[0].svg;
    assert!(
        svg.contains("fill-rule=\"evenodd\" pathLength=\"1\""),
        "{svg}"
    );
}

#[test]
fn the_starter_sets_are_upstreams() {
    let names: Vec<String> = starter_icons().iter().map(|i| i.name.to_string()).collect();
    assert_eq!(
        names,
        [
            "X", "Check", "Bell", "Home", "Settings", "Calendar", "Menu", "Heart", "Eye", "Star",
            "Folder", "Shield", "Search", "Mail", "Lock"
        ]
    );
    assert_eq!(weighted_starter_icons().len(), 15);
    assert_eq!(bell_icon().secondary.as_ref().map(Vec::len), Some(1));
}

// buildWeightedIcon and parseSVGPaths, against upstream's output under Node.

fn svg(d: &str) -> WeightedDrawing {
    WeightedDrawing::Svg(format!(
        "<svg viewBox=\"0 0 24 24\"><path d=\"{d}\"/></svg>"
    ))
}

fn ranges(icon: &WeightedIconDef) -> Vec<(u16, u16, String)> {
    icon.layers
        .iter()
        .map(|l| {
            (
                l.from,
                l.to,
                l.paths.first().map(|p| p.d.to_string()).unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn merges_identical_drawings_at_neighboring_weights_into_one_layer() {
    let drawings = BTreeMap::from([
        (300, svg("M1 1")),
        (400, svg("M1 1")),
        (500, svg("M2 2")),
        (600, svg("M3 3")),
        (700, svg("M3 3")),
    ]);
    let icon = build_weighted_icon("dot", &drawings, &Default::default()).unwrap();
    assert_eq!(
        ranges(&icon),
        [
            (300, 400, "M1 1".into()),
            (500, 500, "M2 2".into()),
            (600, 700, "M3 3".into())
        ]
    );
}

#[test]
fn covers_every_weight_with_the_nearest_kept_drawing_when_weights_are_pruned() {
    let drawings = BTreeMap::from([
        (300, svg("A")),
        (400, svg("B")),
        (500, svg("C")),
        (600, svg("D")),
        (700, svg("E")),
    ]);
    let options = BuildWeightedIconOptions {
        weights: Some(vec![400, 600]),
        ..Default::default()
    };
    // 300 to 400 (nearest), 500 to 600 (a tie goes heavier), 700 to 600.
    let icon = build_weighted_icon("dot", &drawings, &options).unwrap();
    assert_eq!(
        ranges(&icon),
        [(300, 400, "B".into()), (500, 700, "D".into())]
    );
    // A kept weight with no drawing is dropped: 500 ties between 300 and 700.
    let drawings = BTreeMap::from([(300, svg("A")), (700, svg("B"))]);
    let options = BuildWeightedIconOptions {
        weights: Some(vec![300, 500, 700]),
        ..Default::default()
    };
    let icon = build_weighted_icon("dot", &drawings, &options).unwrap();
    assert_eq!(
        ranges(&icon),
        [(300, 400, "A".into()), (500, 700, "B".into())]
    );
}

#[test]
fn fails_as_upstream_throws_with_no_kept_drawing() {
    let drawings = BTreeMap::from([(400, svg("A"))]);
    let options = BuildWeightedIconOptions {
        weights: Some(vec![700]),
        ..Default::default()
    };
    assert_eq!(
        build_weighted_icon("dot", &drawings, &options),
        Err("buildWeightedIcon(dot): no drawing for any kept weight".into())
    );
}

#[test]
fn keeps_the_view_box_and_compares_drawings_as_upstreams_json_does() {
    let drawings = BTreeMap::from([(500, svg("X"))]);
    let options = BuildWeightedIconOptions {
        view_box: Some("0 0 32 32".into()),
        ..Default::default()
    };
    let icon = build_weighted_icon("dot", &drawings, &options).unwrap();
    assert_eq!(icon.view_box.as_deref(), Some("0 0 32 32"));
    assert_eq!(ranges(&icon), [(300, 700, "X".into())]);
    let path = |even_odd| WeightedIconPath {
        d: "Q".into(),
        even_odd,
    };
    // `{d, evenOdd: false}` and `{d}` are different JSON: two layers.
    let drawings = BTreeMap::from([
        (300, WeightedDrawing::Paths(vec![path(Some(false))])),
        (400, WeightedDrawing::Paths(vec![path(None)])),
    ]);
    let icon = build_weighted_icon("dot", &drawings, &Default::default()).unwrap();
    assert_eq!(
        icon.layers
            .iter()
            .map(|l| (l.from, l.to))
            .collect::<Vec<_>>(),
        [(300, 300), (400, 700)]
    );
}

#[test]
fn reads_paths_as_upstreams_patterns_do() {
    let parse = |s: &str| {
        parse_svg_paths(s)
            .into_iter()
            .map(|p| (p.d.to_string(), p.even_odd))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        parse("<svg><path d=\"M0 0\"/></svg>"),
        [("M0 0".into(), None)]
    );
    assert_eq!(
        parse("<svg><path d='M1 1 L2 2'/></svg>"),
        [("M1 1 L2 2".into(), None)]
    );
    assert_eq!(
        parse(
            "<svg><path fill-rule=\"evenodd\" d=\"M0 0\"/><path d=\"M3 3\" fill-rule='evenodd'/></svg>"
        ),
        [("M0 0".into(), Some(true)), ("M3 3".into(), Some(true))]
    );
    // A path with no `d` is skipped; so is an empty `d`.
    assert_eq!(
        parse("<svg><path id=\"x\"/><path  d=\"Z\" /></svg>"),
        [("Z".into(), None)]
    );
    assert_eq!(
        parse("<svg><path d=\"\"/><path d=\"M2\"/></svg>"),
        [("M2".into(), None)]
    );
    // `<pathx>` is not a path; a line break before `d` is whitespace.
    assert_eq!(
        parse("<svg><pathx d=\"M9\"/><path\nd=\"M5 5\"/></svg>"),
        [("M5 5".into(), None)]
    );
    assert_eq!(
        parse("<svg><path data-d=\"no\" d=\"M7 7\"></path></svg>"),
        [("M7 7".into(), None)]
    );
    assert_eq!(
        parse("<svg><path d=\"M1&quot;2\"/></svg>"),
        [("M1&quot;2".into(), None)]
    );
    assert_eq!(
        parse("<svg><path fill-rule=\"nonzero\" d=\"M8\"/></svg>"),
        [("M8".into(), None)]
    );
    assert_eq!(parse("<svg><path\td=\"M6\"/></svg>"), [("M6".into(), None)]);
    assert!(parse("").is_empty());
}

#[test]
fn demo_icons_cover_all_five_weights_without_gaps() {
    for icon in weighted_starter_icons() {
        let covered: Vec<u16> = icon
            .layers
            .iter()
            .flat_map(|l| (l.from..=l.to).step_by(100))
            .collect();
        assert_eq!(covered, ICON_WEIGHTS, "{}", icon.name);
    }
}

// The weight probe and the layer rule.

#[test]
fn the_probe_reads_upstreams_bands() {
    for (weight, read) in [
        (100., Some(300)),
        (300., Some(300)),
        (349., Some(300)),
        (350., Some(400)),
        (449.9, Some(400)),
        (450., Some(500)),
        (550., Some(600)),
        (650., Some(700)),
        (1000., Some(700)),
        (1100., None),
        (0., None),
        (-50., None),
    ] {
        assert_eq!(read_weight_probe(weight), read, "{weight}");
    }
}

#[test]
fn the_current_weight_snaps_to_a_drawing() {
    assert_eq!(effective_icon_weight(None, None), 400);
    assert_eq!(
        effective_icon_weight(Some(450.), Some(300)),
        500,
        "CSS round() rounds half up"
    );
    assert_eq!(effective_icon_weight(Some(449.), None), 400);
    assert_eq!(effective_icon_weight(Some(250.), None), 300);
    assert_eq!(effective_icon_weight(Some(900.), None), 700);
    assert_eq!(effective_icon_weight(None, Some(600)), 600);
    assert_eq!(layer_opacity(300, 400, 400), 1.);
    assert_eq!(layer_opacity(300, 400, 500), 0.);
    assert_eq!(layer_opacity(500, 700, 500), 1.);
    assert_eq!(layer_opacity(500, 700, 400), 0.);
}

#[test]
fn draws_one_document_per_distinct_drawing() {
    let bell = bell_weighted_icon();
    let docs: Vec<String> = bell
        .layers
        .iter()
        .map(|l| layer_document(&bell, l))
        .collect();
    assert_eq!(docs.len(), bell.layers.len());
    for weight in ICON_WEIGHTS {
        let shown = bell
            .layers
            .iter()
            .filter(|l| layer_opacity(l.from, l.to, weight) == 1.)
            .count();
        assert_eq!(shown, 1, "exactly one drawing at {weight}");
    }
    assert!(docs[0].starts_with(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><g fill=\"black\"><path d="
    ));
}

#[test]
fn the_sprite_symbol_id_is_upstreams() {
    assert_eq!(
        weighted_icon_symbol_id(&bell_weighted_icon()),
        "astryx-weighted-icon-bell"
    );
}

// In a window.

type BuildFn = std::rc::Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Host(BuildFn);

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size(px(400.)).child((self.0)(window, cx))
    }
}

/// Records the weight a weighted icon would draw at, where it renders.
#[derive(IntoElement)]
struct Probe(std::rc::Rc<std::cell::Cell<u16>>, Option<u16>);

impl RenderOnce for Probe {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.0.set(current_icon_weight(self.1, window, cx));
        div()
    }
}

fn window(
    cx: &mut TestAppContext,
    f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> &mut gpui::VisualTestContext {
    cx.update(rustryx_theme::init);
    let (_, cx) = cx.add_window_view(move |_, _| Host(std::rc::Rc::new(f)));
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx
}

#[gpui::test]
fn a_weighted_icon_follows_the_surrounding_font_weight(cx: &mut TestAppContext) {
    let cell = || std::rc::Rc::new(std::cell::Cell::new(0u16));
    let (b, l, p, s) = (cell(), cell(), cell(), cell());
    let (b2, l2, p2, s2) = (b.clone(), l.clone(), p.clone(), s.clone());
    window(cx, move |_, _| {
        div()
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .child(Probe(b2.clone(), None)),
            )
            .child(
                div()
                    .font_weight(FontWeight::LIGHT)
                    .child(Probe(l2.clone(), None)),
            )
            .child(
                div()
                    .font_weight(FontWeight::LIGHT)
                    .child(Probe(p2.clone(), Some(700))),
            )
            .child(pin_icon_weight(
                300.,
                div()
                    .font_weight(FontWeight::BOLD)
                    .child(Probe(s2.clone(), None)),
            ))
            .into_any_element()
    });
    assert_eq!((b.get(), l.get(), p.get(), s.get()), (700, 300, 700, 300));
}

#[gpui::test]
fn icons_draw_at_their_size_and_sprites_register_their_symbols(cx: &mut TestAppContext) {
    let cx = window(cx, |_, _| {
        div()
            .child(
                SVGIcon::new(home_icon())
                    .variation(SVGIconVariation::Bold)
                    .size(SVGIconSize::Lg),
            )
            .child(WeightedIconSprite::new([bell_weighted_icon()]))
            .child(
                WeightedSVGIcon::new(bell_weighted_icon())
                    .sprite(true)
                    .size(SVGIconSize::Sm),
            )
            .into_any_element()
    });
    let icon = cx.debug_bounds("svg-icon-Home").expect("the icon is drawn");
    assert_eq!(icon.size, gpui::size(px(24.), px(24.)));
    let weighted = cx
        .debug_bounds("weighted-svg-icon-bell")
        .expect("the weighted icon is drawn");
    assert_eq!(weighted.size, gpui::size(px(16.), px(16.)));
    let held = cx.update(|_, cx| {
        cx.try_global::<Sprites>()
            .is_some_and(|s| s.0.contains_key("astryx-weighted-icon-bell"))
    });
    assert!(held, "the sprite holds the bell's symbol");
}
