//! The 3D chart gallery (upstream's `ThreeDChart` stories, and `ThreeD-Showcase`'s `SVGvsWebGL` and
//! `Gallery` and `ThreeD-Backdrop`'s split backdrops): a scatter, bars, a surface and its
//! wireframe, all draggable; the SVG and WebGL scatters side by side; point-cloud shapes on
//! colored cards; and a custom mark that reads the chart through [`use_3d`]. The stories'
//! `Math.random()` is a seeded generator here.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, FontWeight, Hsla, Pixels, SharedString, Window, bounds, canvas, div,
    fill, point, px, rgb, size,
};
use rustryx_charts::marks::fill_circle;
use rustryx_components::layout::{SpacingStep, Stack};
use rustryx_components::text::{Text, TextColor, TextType};
use rustryx_components::theming::{MediaThemeMode, media_theme};
use std::f64::consts::PI;
use std::rc::Rc;

use super::{
    ThreeDAxis, ThreeDBar, ThreeDChart, ThreeDGrid, ThreeDScatter, ThreeDScatterGl, ThreeDSurface,
    use_3d,
};
use crate::lab_chart::{Datum, SequentialHue, use_chart_colors};

/// A seeded stand-in for the stories' `Math.random()`.
struct Seeded(f64);

impl Seeded {
    fn next(&mut self) -> f64 {
        self.0 = (self.0 * 1_664_525. + 1_013_904_223.) % 4_294_967_296.;
        self.0 / 4_294_967_296.
    }
}

fn xyz(x: f64, y: f64, z: f64) -> Datum {
    Datum::new().with("x", x).with("y", y).with("z", z)
}

fn scatter_data() -> Vec<Datum> {
    let mut r = Seeded(1.);
    (0..200)
        .map(|_| xyz(r.next() * 100., r.next() * 100., r.next() * 100.))
        .collect()
}

fn bar_data() -> Vec<Datum> {
    [
        (0, 0, 42.),
        (1, 0, 58.),
        (2, 0, 35.),
        (0, 1, 65.),
        (1, 1, 48.),
        (2, 1, 72.),
        (0, 2, 30.),
        (1, 2, 55.),
        (2, 2, 40.),
    ]
    .into_iter()
    .map(|(product, region, sales)| {
        Datum::new()
            .with("product", product)
            .with("region", region)
            .with("sales", sales)
    })
    .collect()
}

/// The stories' `sin(x) * cos(z)` surface on a 21 by 21 grid.
fn surface_data() -> Vec<Datum> {
    let mut rows = Vec::new();
    for x in 0..=20 {
        for z in 0..=20 {
            let (xn, zn) = (f64::from(x) / 20., f64::from(z) / 20.);
            let y = (xn * PI * 2.).sin() * (zn * PI * 2.).cos() * 50. + 50.;
            rows.push(Datum::new().with("x", x).with("y", y.round()).with("z", z));
        }
    }
    rows
}

/// Points evenly spread over a sphere (a Fibonacci lattice).
fn sphere(n: usize) -> Vec<Datum> {
    let phi = (1. + 5f64.sqrt()) / 2.;
    (0..n)
        .map(|i| {
            let i = i as f64;
            let theta = (1. - (2. * (i + 0.5)) / n as f64).acos();
            let p = (2. * PI * i) / phi;
            xyz(theta.sin() * p.cos(), theta.sin() * p.sin(), theta.cos())
        })
        .collect()
}

fn torus_knot(n: usize) -> Vec<Datum> {
    (0..n)
        .map(|i| {
            let t = (i as f64 / n as f64) * PI * 2.;
            let r = (3. * t).cos() + 2.;
            xyz(r * (2. * t).cos(), r * (2. * t).sin(), (3. * t).sin())
        })
        .collect()
}

fn teapot(n: usize, r: &mut Seeded) -> Vec<Datum> {
    let n = n as f64;
    let mut pts = Vec::new();
    let mut i = 0.;
    while i < n * 0.45 {
        let (u, v) = (r.next() * PI * 2., r.next() * PI);
        let profile = v.sin() * (1.2 + 0.2 * (2. * v).sin());
        pts.push(xyz(profile * u.cos(), v.cos() * 0.7, profile * u.sin()));
        i += 1.;
    }
    i = 0.;
    while i < n * 0.18 {
        let (t, a) = (r.next(), r.next() * PI * 2.);
        let rr = 0.09 * (1. - t * 0.4);
        pts.push(xyz(
            1.1 + t * 0.6 + t * t * 0.2,
            -0.1 + t * 0.6 + t * t * 0.2,
            rr * a.cos(),
        ));
        i += 1.;
    }
    i = 0.;
    while i < n * 0.17 {
        let (t, a) = (r.next() * PI, r.next() * PI * 2.);
        pts.push(xyz(
            -1.1 - 0.5 * t.sin(),
            -0.35 + 0.7 * t.sin(),
            0.07 * a.cos(),
        ));
        i += 1.;
    }
    i = 0.;
    while i < n * 0.15 {
        let (rr, a) = (r.next() * 0.6, r.next() * PI * 2.);
        pts.push(xyz(rr * a.cos(), 0.72, rr * a.sin()));
        i += 1.;
    }
    i = 0.;
    while i < n * 0.05 {
        let (u, v) = (r.next() * PI * 2., r.next() * PI);
        pts.push(xyz(
            0.1 * v.sin() * u.cos(),
            0.82 + 0.1 * v.cos(),
            0.1 * v.sin() * u.sin(),
        ));
        i += 1.;
    }
    pts
}

fn heart(n: usize, r: &mut Seeded) -> Vec<Datum> {
    (0..n)
        .map(|_| {
            let (u, v) = (r.next() * PI * 2., r.next() * PI);
            let x = v.sin() * (15. * u.sin() - 4. * (3. * u).sin());
            let y = 8. * v.cos();
            let z = v.sin()
                * (15. * u.cos() - 5. * (2. * u).cos() - 2. * (3. * u).cos() - (4. * u).cos());
            xyz(x / 16., y / 16., z / 16.)
        })
        .collect()
}

fn seashell(n: usize, r: &mut Seeded) -> Vec<Datum> {
    (0..n)
        .map(|_| {
            let (u, v) = (r.next() * 4. * PI, r.next() * PI * 2.);
            let rr = (0.15 * u).exp();
            let opening = 0.3 + 0.3 * v.cos();
            let (x, z) = (rr * u.cos() * opening, rr * u.sin() * opening);
            let y = rr * (0.08 * u + 0.2 * v.sin());
            xyz(x * 0.1, y * 0.1 - 0.5, z * 0.1)
        })
        .collect()
}

fn klein_bottle(n: usize, r: &mut Seeded) -> Vec<Datum> {
    (0..n)
        .map(|_| {
            let (u, v) = (r.next() * PI * 2., r.next() * PI * 2.);
            let rr = 4. * (1. - u.cos() / 2.);
            let x = if u < PI {
                6. * u.cos() * (1. + u.sin()) + rr * u.cos() * v.cos()
            } else {
                6. * u.cos() * (1. + u.sin()) + rr * (v + PI).cos()
            };
            let z = rr * u.sin() * v.cos();
            let y = -16. * u.sin() + rr * v.sin();
            xyz(x / 22., y / 22., z / 22.)
        })
        .collect()
}

fn star(n: usize, r: &mut Seeded) -> Vec<Datum> {
    (0..n)
        .map(|_| {
            let u = r.next() * PI * 2.;
            let v = (2. * r.next() - 1.).acos();
            let spike = 0.6 + 0.6 * ((5. * u) / 2.).cos().abs().powi(8);
            xyz(
                spike * v.sin() * u.cos(),
                spike * v.cos(),
                spike * v.sin() * u.sin(),
            )
        })
        .collect()
}

fn tree(n: usize, r: &mut Seeded) -> Vec<Datum> {
    let n = n as f64;
    let mut pts = Vec::new();
    let mut i = 0.;
    while i < n * 0.15 {
        let (h, a) = (r.next(), r.next() * PI * 2.);
        let rr = 0.1 * (1. - h * 0.4);
        pts.push(xyz(rr * a.cos(), h, rr * a.sin()));
        i += 1.;
    }
    i = 0.;
    while i < n * 0.85 {
        let layer = r.next();
        let h = 0.8 + layer * 1.4;
        let max_r = 0.8 * (1. - layer * 0.7);
        let rr = r.next().sqrt() * max_r;
        let a = r.next() * PI * 2.;
        let (jx, jy, jz) = (
            (r.next() - 0.5) * 0.05,
            (r.next() - 0.5) * 0.15,
            (r.next() - 0.5) * 0.05,
        );
        pts.push(xyz(rr * a.cos() + jx, h + jy, rr * a.sin() + jz));
        i += 1.;
    }
    pts
}

fn example(title: &'static str, note: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(Text::new(title).text_type(TextType::Large))
        .when(!note.is_empty(), |s| {
            s.child(
                Text::new(note)
                    .text_type(TextType::Supporting)
                    .color(TextColor::Secondary),
            )
        })
        .child(body)
        .into_any_element()
}

/// The showcase's card caption: 11-pixel medium uppercase in the card's primary text color.
#[derive(IntoElement)]
struct Caption(SharedString);

impl RenderOnce for Caption {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = rustryx_theme::current(window, cx).tokens().clone();
        div()
            .px(px(12.))
            .py(px(6.))
            .text_size(px(11.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.color.text_primary)
            .text_center()
            .child(self.0.to_uppercase())
    }
}

/// A rounded card of one color, its content in a media theme of `mode`.
fn card(bg: u32, mode: MediaThemeMode, content: impl IntoElement) -> impl IntoElement {
    div()
        .bg(rgb(bg))
        .rounded(px(16.))
        .overflow_hidden()
        .child(media_theme(mode, content))
}

/// `ThreeD-Showcase`'s `Cell`: an auto-rotating WebGL point cloud over its label.
fn cell(
    bg: u32,
    mode: MediaThemeMode,
    data: Rc<[Datum]>,
    label: &'static str,
    azimuth: f64,
    elevation: f64,
) -> impl IntoElement {
    let color = if mode == MediaThemeMode::Dark {
        0xdfe2e5
    } else {
        0x0a1317
    };
    card(
        bg,
        mode,
        div()
            .flex()
            .flex_col()
            .child(
                div().flex_1().p(px(4.)).child(
                    ThreeDChart::new(
                        SharedString::from(format!("three-d-cell-{label}")),
                        data,
                        "x",
                        "y",
                        "z",
                    )
                    .height(220.)
                    .azimuth(azimuth)
                    .elevation(elevation)
                    .interactive(true)
                    .auto_rotate(0.3)
                    .child(
                        ThreeDScatterGl::new(rgb(color).into())
                            .size(1.5)
                            .opacity(0.6),
                    ),
                ),
            )
            .child(Caption(label.into())),
    )
}

/// `ThreeD-Backdrop`'s `BackdropInner`: a custom mark. Through [`use_3d`] it paints the plot's halves
/// in two colors, then the points far to near, each colored by the half it projects onto.
#[derive(IntoElement)]
struct SplitBackdrop {
    backgrounds: [Hsla; 2],
    dots: [Hsla; 2],
}

impl RenderOnce for SplitBackdrop {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_3d(cx) else {
            return div().into_any_element();
        };
        let mut points: Vec<_> = ctx
            .data
            .iter()
            .map(|d| {
                let (nx, ny, nz) = ctx.normalized(d);
                ctx.project(nx, ny, nz)
            })
            .collect();
        points.sort_by(|a, b| a.depth.total_cmp(&b.depth));
        let (width, height) = (ctx.width, ctx.height);
        let (backgrounds, dots) = (self.backgrounds, self.dots);
        canvas(
            |_, _, _| {},
            move |area: Bounds<Pixels>, _, window, _| {
                let half = px((width / 2.) as f32);
                let h = px(height as f32);
                window.paint_quad(fill(bounds(area.origin, size(half, h)), backgrounds[0]));
                window.paint_quad(fill(
                    bounds(area.origin + point(half, px(0.)), size(half, h)),
                    backgrounds[1],
                ));
                for p in &points {
                    let depth_factor = 0.5 + (p.depth + 0.5) * 0.5;
                    let color = if p.px < width / 2. { dots[0] } else { dots[1] };
                    fill_circle(
                        window,
                        area.origin,
                        (p.px, p.py),
                        1.2 * depth_factor,
                        color.opacity((0.65 * depth_factor) as f32),
                    );
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}

fn backdrop(
    id: &'static str,
    data: Rc<[Datum]>,
    auto_rotate: f64,
    backgrounds: [u32; 2],
    dots: [u32; 2],
) -> impl IntoElement {
    div()
        .rounded(px(16.))
        .overflow_hidden()
        .max_w(px(600.))
        .child(
            ThreeDChart::new(id, data, "x", "y", "z")
                .height(400.)
                .interactive(true)
                .auto_rotate(auto_rotate)
                .child(SplitBackdrop {
                    backgrounds: backgrounds.map(|c| rgb(c).into()),
                    dots: dots.map(|c| rgb(c).into()),
                }),
        )
}

/// The stories' data, built once (their `useMemo`s): the page renders again for every step
/// of the turning charts.
struct StoryData {
    scatter: Rc<[Datum]>,
    bars: Rc<[Datum]>,
    surface: Rc<[Datum]>,
    sphere: Rc<[Datum]>,
    torus_knot: Rc<[Datum]>,
    shapes: [(Rc<[Datum]>, &'static str, f64, f64); 6],
}

impl StoryData {
    fn new() -> Self {
        let mut r = Seeded(7.);
        StoryData {
            scatter: scatter_data().into(),
            bars: bar_data().into(),
            surface: surface_data().into(),
            sphere: sphere(2000).into(),
            torus_knot: torus_knot(2500).into(),
            shapes: [
                (teapot(3000, &mut r).into(), "Teapot", 30., 25.),
                (heart(2500, &mut r).into(), "Heart", 0., 15.),
                (seashell(3000, &mut r).into(), "Seashell", 45., 30.),
                (klein_bottle(2500, &mut r).into(), "Klein Bottle", 50., 25.),
                (star(2500, &mut r).into(), "Star", 30., 35.),
                (tree(3000, &mut r).into(), "Tree", 35., 15.),
            ],
        }
    }
}

thread_local! {
    static STORY_DATA: Rc<StoryData> = Rc::new(StoryData::new());
}

/// Upstream's stories, top to bottom.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let colors = use_chart_colors(window, cx);
    let c = colors.categorical(1)[0];
    let data = STORY_DATA.with(Rc::clone);
    let shapes = data.shapes.clone();
    let palettes = [
        (0x0064e0, MediaThemeMode::Dark),
        (0xe3193b, MediaThemeMode::Dark),
        (0xfbce03, MediaThemeMode::Light),
        (0x6b1efd, MediaThemeMode::Dark),
        (0x0b991f, MediaThemeMode::Dark),
        (0xeb6e00, MediaThemeMode::Dark),
    ];
    let sphere_side = |id: &'static str, label: &'static str, gl: bool| {
        card(
            0x0064e0,
            MediaThemeMode::Dark,
            div().child(Caption(label.into())).child(
                ThreeDChart::new(id, data.sphere.clone(), "x", "y", "z")
                    .height(300.)
                    .azimuth(30.)
                    .elevation(20.)
                    .interactive(true)
                    .auto_rotate(0.3)
                    .when(gl, |chart| {
                        chart.child(
                            ThreeDScatterGl::new(rgb(0xdfe2e5).into())
                                .size(3.)
                                .opacity(0.85),
                        )
                    })
                    .when(!gl, |chart| {
                        chart.child(
                            ThreeDScatter::new(rgb(0xdfe2e5).into())
                                .radius(1.5)
                                .opacity(0.85),
                        )
                    }),
            ),
        )
    };
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example(
            "3D Scatter Plot",
            "200 points. Drag to rotate. Depth encoded via size and opacity.",
            ThreeDChart::new("three-d-scatter", data.scatter.clone(), "x", "y", "z")
                .height(400.)
                .interactive(true)
                .child(ThreeDGrid::new())
                .child(ThreeDAxis::new())
                .child(ThreeDScatter::new(c).radius(4.)),
        ))
        .child(example(
            "3D Bar Chart",
            "Sales by product x region. Drag to rotate.",
            ThreeDChart::new(
                "three-d-bars",
                data.bars.clone(),
                "product",
                "sales",
                "region",
            )
            .height(400.)
            .interactive(true)
            .child(ThreeDGrid::new().divisions(3))
            .child(ThreeDAxis::new())
            .child(ThreeDBar::new(c).bar_width(0.12).bar_depth(0.12)),
        ))
        .child(example(
            "3D Surface",
            "sin(x) * cos(z) surface. Drag to rotate. Color maps to height.",
            ThreeDChart::new("three-d-surface", data.surface.clone(), "x", "y", "z")
                .height(450.)
                .interactive(true)
                .child(ThreeDGrid::new())
                .child(ThreeDAxis::new())
                .child(ThreeDSurface::new(
                    colors.sequential(SequentialHue::Blue, 5),
                )),
        ))
        .child(example(
            "3D Wireframe",
            "",
            ThreeDChart::new("three-d-wireframe", data.surface.clone(), "x", "y", "z")
                .height(450.)
                .interactive(true)
                .child(ThreeDGrid::new())
                .child(
                    ThreeDSurface::new(colors.sequential(SequentialHue::Teal, 5)).wireframe(true),
                ),
        ))
        .child(example(
            "SVG vs WebGL",
            "The same sphere, camera and depth cues as SVG circles and WebGL points.",
            div()
                .grid()
                .grid_cols(2)
                .gap(px(12.))
                .max_w(px(780.))
                .child(sphere_side("three-d-svg", "SVG (ThreeDScatter)", false))
                .child(sphere_side("three-d-gl", "WebGL (ThreeDScatterGL)", true)),
        ))
        .child(example(
            "Gallery",
            "",
            div()
                .grid()
                .grid_cols(3)
                .gap(px(12.))
                .max_w(px(780.))
                .children(shapes.into_iter().zip(palettes).map(
                    |((data, label, az, el), (bg, mode))| cell(bg, mode, data, label, az, el),
                )),
        ))
        .child(example(
            "Split backdrop",
            "A custom mark: points take the color of the half they project onto.",
            Stack::vertical()
                .gap(SpacingStep::S4)
                .child(backdrop(
                    "three-d-split",
                    data.sphere.clone(),
                    0.15,
                    [0x0a1317, 0xf1f4f7],
                    [0xdfe2e5, 0x0a1317],
                ))
                .child(backdrop(
                    "three-d-split-bold",
                    data.torus_knot.clone(),
                    0.2,
                    [0x0064e0, 0xe3193b],
                    [0xffffff, 0xffffff],
                )),
        ))
        .into_any_element()
}
