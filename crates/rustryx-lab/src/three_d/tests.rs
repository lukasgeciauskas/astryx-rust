//! The 3D chart's tests: upstream lab `projection.test.ts`, `ThreeDChart.test.tsx` and each mark's
//! tests. Upstream reads the rendered SVG; these read the geometry the marks paint, and drive
//! the camera in a window.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Modifiers, MouseButton, Render, TestAppContext, VisualTestContext, Window,
    div, point, px,
};

use super::chart::CHARTS;
use super::*;
use crate::lab_chart::Datum;

fn rows(points: &[(f64, f64, f64)]) -> Vec<Datum> {
    points
        .iter()
        .map(|(x, y, z)| Datum::new().with("x", *x).with("y", *y).with("z", *z))
        .collect()
}

/// A chart's context at upstream's test size (600 by 400, the default camera).
fn ctx_of(points: &[(f64, f64, f64)]) -> ThreeDContext {
    let data = rows(points);
    let domains = [
        compute_domain(&data, "x"),
        compute_domain(&data, "y"),
        compute_domain(&data, "z"),
    ];
    ThreeDContext::detached(
        data.into(),
        ["x", "y", "z"],
        domains,
        Camera::new(35., 25.),
        600.,
        400.,
    )
}

// ---- projection.test.ts ------------------------------------------------------------------

/// The shader's steps as upstream's test writes them out, in 64-bit floats.
fn project_shader(n: (f64, f64, f64), az: f64, el: f64, width: f64, height: f64) -> ProjectedPoint {
    let (az, el) = (
        (az * std::f64::consts::PI) / 180.,
        (el * std::f64::consts::PI) / 180.,
    );
    let (cos_az, sin_az, cos_el, sin_el) = (az.cos(), az.sin(), el.cos(), el.sin());
    let scale = width.min(height) * 0.35;
    let (cx, cy) = (width / 2., height / 2.);
    let (x, y, z) = (n.0 - 0.5, n.1 - 0.5, n.2 - 0.5);
    let x1 = x * cos_az + z * sin_az;
    let z1 = -x * sin_az + z * cos_az;
    let y1 = y * cos_el - z1 * sin_el;
    let z2 = y * sin_el + z1 * cos_el;
    ProjectedPoint {
        px: cx + x1 * scale,
        py: cy - y1 * scale,
        depth: z2,
    }
}

#[test]
fn projection_parity_tier_1() {
    let points = [
        (0., 0., 0.),
        (1., 1., 1.),
        (0.5, 0.5, 0.5),
        (0., 1., 0.5),
        (1., 0., 0.),
        (0.3, 0.7, 0.9),
    ];
    let cameras = [
        (0., 0.),
        (35., 25.),
        (90., 0.),
        (45., 45.),
        (-30., 15.),
        (180., -20.),
    ];
    for (az, el) in cameras {
        let projection = Projection::new(&Camera::new(az, el), 600., 400.);
        for n in points {
            let js = projection.project(n.0, n.1, n.2);
            let gl = project_shader(n, az, el, 600., 400.);
            let close = |a: f64, b: f64| (a - b).abs() < 0.5e-6;
            assert!(
                close(js.px, gl.px) && close(js.py, gl.py) && close(js.depth, gl.depth),
                "{n:?} at az {az}, el {el}"
            );
            // The GPU's 32-bit floats agree to well under a pixel.
            let gpu = projection.project_gl([n.0 as f32, n.1 as f32, n.2 as f32]);
            assert!(
                (gpu.px - js.px).abs() < 1e-3
                    && (gpu.py - js.py).abs() < 1e-3
                    && (gpu.depth - js.depth).abs() < 1e-6
            );
        }
    }
}

#[test]
fn normalize_and_domains() {
    assert_eq!(normalize(5., [0., 10.]), 0.5);
    assert_eq!(normalize(7., [3., 3.]), 0.5, "a single-value domain");
    let data = rows(&[(0., 0., 0.), (10., 20., 30.)]);
    assert_eq!(compute_domain(&data, "z"), [0., 30.]);
    assert_eq!(compute_domain(&data, "missing"), [0., 1.]);
}

// ---- ThreeDChart --------------------------------------------------------------------------

#[test]
fn chart_names_itself_from_its_keys() {
    assert_eq!(
        accessible_label(None, "x", "y", "z"),
        "3D chart of y by x and z"
    );
    assert_eq!(
        accessible_label(Some("Revenue by region and quarter"), "x", "y", "z"),
        "Revenue by region and quarter"
    );
}

#[test]
fn arrow_keys_turn_the_camera_and_clamp_its_pitch() {
    let c = Camera::new(35., 25.);
    assert_eq!(camera_after_key(c, "right").unwrap().azimuth, 45.);
    assert_eq!(
        camera_after_key(camera_after_key(c, "left").unwrap(), "left")
            .unwrap()
            .azimuth,
        15.
    );
    assert_eq!(camera_after_key(c, "up").unwrap().elevation, 35.);
    assert_eq!(
        camera_after_key(Camera::new(0., 85.), "up")
            .unwrap()
            .elevation,
        89.
    );
    assert_eq!(
        camera_after_key(Camera::new(0., 89.), "up")
            .unwrap()
            .elevation,
        89.
    );
    assert_eq!(
        camera_after_key(Camera::new(0., -85.), "down")
            .unwrap()
            .elevation,
        -89.
    );
    assert!(
        camera_after_key(c, "enter").is_none(),
        "other keys are left alone"
    );
    assert_eq!(camera_after_drag(c, 20., -40.), Camera::new(45., 45.));
    assert_eq!(camera_after_drag(c, 0., -400.).elevation, 89.);
    assert!((rotation_step(1., 50.) - 50. / 16.67).abs() < 1e-12);
}

type Build = Rc<dyn Fn() -> AnyElement>;

struct Host(Build);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(600.)).child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    // The test pointer rests at the window's origin, over the chart; move it off.
    cx.simulate_mouse_move(point(px(650.), px(450.)), None, Modifiers::none());
    settle(cx);
    cx
}

/// Focuses a chart through the handle it last rendered with.
fn focus_chart(cx: &mut VisualTestContext, id: &str) {
    let handle = CHARTS
        .with(|c| c.borrow().get(id).and_then(|d| d.focus.clone()))
        .unwrap();
    cx.update(|window, cx| window.focus(&handle, cx));
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

/// Keeps the context a chart gives its marks.
#[derive(IntoElement)]
struct Probe(Rc<RefCell<Option<ThreeDContext>>>);

impl RenderOnce for Probe {
    fn render(self, _: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        *self.0.borrow_mut() = use_3d(cx);
        div()
    }
}

fn chart(
    id: &'static str,
    seen: &Rc<RefCell<Option<ThreeDContext>>>,
    f: impl Fn(ThreeDChart) -> ThreeDChart + 'static,
) -> impl Fn() -> AnyElement + 'static {
    let seen = seen.clone();
    move || {
        f(ThreeDChart::new(
            id,
            rows(&[(0., 0., 0.), (10., 20., 30.)]),
            "x",
            "y",
            "z",
        ))
        .child(Probe(seen.clone()))
        .into_any_element()
    }
}

fn camera(seen: &Rc<RefCell<Option<ThreeDContext>>>) -> Camera {
    seen.borrow()
        .as_ref()
        .expect("the marks read the chart's context")
        .camera
}

#[gpui::test]
fn chart_sizes_itself_and_is_no_tab_stop_when_static(cx: &mut TestAppContext) {
    let seen = Rc::default();
    let _cx = host(cx, chart("static", &seen, |c| c));
    let drawn = CHARTS.with(|c| c.borrow().get("static").cloned()).unwrap();
    assert_eq!(drawn.label, "3D chart of y by x and z");
    assert_eq!(
        drawn.size,
        Some((600., 400.)),
        "the container's width, the default height"
    );
    assert!(!drawn.focusable);
    let ctx = seen.borrow().clone().unwrap();
    assert_eq!(
        (ctx.x_domain, ctx.y_domain, ctx.z_domain),
        ([0., 10.], [0., 20.], [0., 30.])
    );
}

#[gpui::test]
fn chart_takes_its_label_prop(cx: &mut TestAppContext) {
    let seen = Rc::default();
    let _cx = host(
        cx,
        chart("named", &seen, |c| c.label("Revenue by region and quarter")),
    );
    assert_eq!(
        CHARTS
            .with(|c| c.borrow().get("named").cloned())
            .unwrap()
            .label,
        "Revenue by region and quarter"
    );
}

#[gpui::test]
fn chart_draws_nothing_until_it_has_a_width(cx: &mut TestAppContext) {
    let seen: Rc<RefCell<Option<ThreeDContext>>> = Rc::default();
    let inner = chart("narrow", &seen, |c| c);
    let _cx = host(cx, move || {
        div().w(px(0.)).child(inner()).into_any_element()
    });
    assert_eq!(
        CHARTS
            .with(|c| c.borrow().get("narrow").cloned())
            .unwrap()
            .size,
        None
    );
    assert!(seen.borrow().is_none(), "no marks rendered");
}

#[gpui::test]
fn chart_takes_its_height_prop(cx: &mut TestAppContext) {
    let seen = Rc::default();
    let _cx = host(cx, chart("short", &seen, |c| c.height(250.)));
    assert_eq!(
        CHARTS
            .with(|c| c.borrow().get("short").cloned())
            .unwrap()
            .size,
        Some((600., 250.))
    );
}

#[gpui::test]
fn auto_rotation_turns_the_camera_and_makes_the_chart_focusable(cx: &mut TestAppContext) {
    let seen = Rc::default();
    let cx = host(cx, chart("spin", &seen, |c| c.auto_rotate(1.)));
    assert!(camera(&seen).azimuth > 35.);
    let drawn = CHARTS.with(|c| c.borrow().get("spin").cloned()).unwrap();
    assert!(drawn.focusable && drawn.rotating && !drawn.hovered);
    // A step every 50 ms, normalized to 60 fps.
    let before = camera(&seen).azimuth;
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(50));
    settle(cx);
    assert!((camera(&seen).azimuth - (before + rotation_step(1., 50.))).abs() < 1e-9);
}

#[gpui::test]
fn reduced_motion_never_starts_the_rotation(cx: &mut TestAppContext) {
    cx.update(|cx| rustryx_primitives::set_reduced_motion(true, cx));
    let seen = Rc::default();
    let _cx = host(cx, chart("calm", &seen, |c| c.auto_rotate(1.)));
    assert_eq!(camera(&seen).azimuth, 35.);
    assert!(
        !CHARTS
            .with(|c| c.borrow().get("calm").cloned())
            .unwrap()
            .rotating
    );
}

#[gpui::test]
fn rotation_pauses_while_hovered_and_resumes_on_leave(cx: &mut TestAppContext) {
    let seen = Rc::default();
    let cx = host(cx, chart("hover", &seen, |c| c.auto_rotate(1.)));
    let plot = cx.debug_bounds("three-d-plot").unwrap();
    cx.simulate_mouse_move(plot.center(), None, Modifiers::none());
    settle(cx);
    assert!(
        !CHARTS
            .with(|c| c.borrow().get("hover").cloned())
            .unwrap()
            .rotating
    );
    let before = camera(&seen).azimuth;
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    settle(cx);
    assert_eq!(camera(&seen).azimuth, before, "no rotation while hovered");
    cx.simulate_mouse_move(
        point(plot.right() + px(50.), plot.bottom() + px(50.)),
        None,
        Modifiers::none(),
    );
    settle(cx);
    assert!(
        CHARTS
            .with(|c| c.borrow().get("hover").cloned())
            .unwrap()
            .rotating
    );
}

#[gpui::test]
fn rotation_pauses_while_focused(cx: &mut TestAppContext) {
    let seen = Rc::default();
    let cx = host(cx, chart("focus", &seen, |c| c.auto_rotate(1.)));
    focus_chart(cx, "focus");
    settle(cx);
    assert!(
        !CHARTS
            .with(|c| c.borrow().get("focus").cloned())
            .unwrap()
            .rotating
    );
    cx.update(|window, cx| window.blur(cx));
    settle(cx);
    assert!(
        CHARTS
            .with(|c| c.borrow().get("focus").cloned())
            .unwrap()
            .rotating,
        "resumes on blur"
    );
}

#[gpui::test]
fn arrow_keys_turn_a_focused_interactive_chart(cx: &mut TestAppContext) {
    let seen = Rc::default();
    let cx = host(cx, chart("keys", &seen, |c| c.interactive(true)));
    assert!(
        CHARTS
            .with(|c| c.borrow().get("keys").cloned())
            .unwrap()
            .focusable
    );
    focus_chart(cx, "keys");
    settle(cx);
    cx.simulate_keystrokes("right");
    settle(cx);
    assert_eq!(camera(&seen).azimuth, 45.);
    cx.simulate_keystrokes("up");
    settle(cx);
    assert_eq!(camera(&seen).elevation, 35.);
}

#[gpui::test]
fn arrow_keys_stop_at_the_chart(cx: &mut TestAppContext) {
    let seen = Rc::default();
    let reached = Rc::new(std::cell::Cell::new(false));
    let (inner, flag) = (
        chart("stop", &seen, |c| c.interactive(true)),
        reached.clone(),
    );
    let cx = host(cx, move || {
        let flag = flag.clone();
        div()
            .on_key_down(move |_, _, _| flag.set(true))
            .child(inner())
            .into_any_element()
    });
    focus_chart(cx, "stop");
    settle(cx);
    cx.simulate_keystrokes("left");
    settle(cx);
    assert_eq!(camera(&seen).azimuth, 25.);
    assert!(
        !reached.get(),
        "the arrow key goes no further (upstream's preventDefault)"
    );
}

#[gpui::test]
fn arrow_keys_do_nothing_when_not_interactive(cx: &mut TestAppContext) {
    cx.update(|cx| rustryx_primitives::set_reduced_motion(true, cx));
    let seen = Rc::default();
    let cx = host(cx, chart("idle", &seen, |c| c.auto_rotate(1.)));
    focus_chart(cx, "idle");
    settle(cx);
    cx.simulate_keystrokes("right");
    settle(cx);
    assert_eq!(camera(&seen).azimuth, 35.);
}

#[gpui::test]
fn dragging_turns_the_camera_half_a_degree_a_pixel(cx: &mut TestAppContext) {
    let seen = Rc::default();
    let cx = host(cx, chart("drag", &seen, |c| c.interactive(true)));
    let at = cx.debug_bounds("three-d-plot").unwrap().center();
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        at + point(px(20.), px(-40.)),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        at + point(px(20.), px(-40.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    settle(cx);
    assert_eq!(camera(&seen), Camera::new(45., 45.));
}

#[test]
fn use_3d_reads_nothing_outside_a_chart() {
    let app = gpui::TestAppContext::single();
    app.update(|cx| assert!(use_3d(cx).is_none()));
}

// ---- marks --------------------------------------------------------------------------------

#[test]
fn axes_radiate_from_one_origin_with_key_and_domain_labels() {
    let ctx = ctx_of(&[(0., 0., 0.), (2., 2., 2.)]);
    let (origin, ends) = axis_geometry(&ctx, true);
    assert!(origin.px.is_finite() && origin.py.is_finite());
    assert!(
        ends.iter()
            .all(|(e, _)| e.px.is_finite() && e.py.is_finite()),
        "three lines from the origin"
    );
    let labels: Vec<String> = ends
        .iter()
        .map(|(_, l)| l.clone().unwrap().to_string())
        .collect();
    assert_eq!(labels, ["x [0-2]", "y [0-2]", "z [0-2]"]);
}

#[test]
fn axes_can_go_unlabelled() {
    let ctx = ctx_of(&[(0., 0., 0.), (2., 2., 2.)]);
    assert!(
        axis_geometry(&ctx, false)
            .1
            .iter()
            .all(|(_, l)| l.is_none())
    );
}

#[test]
fn grid_draws_two_lines_per_boundary() {
    let ctx = ctx_of(&[(0., 0., 0.), (1., 1., 1.)]);
    assert_eq!(grid_lines(&ctx, 5).len(), 12);
    assert_eq!(grid_lines(&ctx, 2).len(), 6);
    assert!(
        grid_lines(&ctx, 5)
            .iter()
            .all(|l| [l.x1, l.y1, l.x2, l.y2].iter().all(|v| v.is_finite()))
    );
    let depths: Vec<f64> = grid_lines(&ctx, 5).iter().map(|l| l.depth).collect();
    assert!(depths.windows(2).all(|w| w[0] <= w[1]), "far to near");
}

#[test]
fn scatter_draws_far_points_first_sized_by_depth() {
    let ctx = ctx_of(&[(0., 0., 0.), (2., 2., 2.), (1., 2., 0.)]);
    let points = scatter_points(&ctx);
    assert_eq!(points.len(), 3);
    let radii: Vec<f64> = points.iter().map(|p| 4. * depth_factor(p.depth)).collect();
    assert!(
        radii[0] < radii[1] && radii[1] < radii[2],
        "radius ascends with depth"
    );
    for p in &points {
        let (r, o) = (10. * depth_factor(p.depth), 0.5 * depth_factor(p.depth));
        assert!(r > 5. && r <= 10. && o > 0.25 && o <= 0.5);
        assert!(p.px.is_finite() && p.py.is_finite());
    }
}

#[test]
fn bars_have_three_shaded_faces_of_four_corners() {
    let ctx = ctx_of(&[(0., 1., 0.), (2., 2., 2.)]);
    let bars = bar_geometry(&ctx, 0.06, 0.06);
    assert_eq!(bars.len(), 2);
    let faces: Vec<_> = bars.iter().flat_map(|b| b.faces()).collect();
    assert_eq!(faces.len(), 6);
    assert_eq!(bars[0].faces().map(|(_, o)| o), [0.9, 0.7, 1.]);
    assert!(
        faces
            .iter()
            .all(|(c, _)| c.iter().all(|p| p.px.is_finite() && p.py.is_finite()))
    );
}

fn grid3x3() -> Vec<(f64, f64, f64)> {
    let mut g = Vec::new();
    for x in [0., 1., 2.] {
        for z in [0., 1., 2.] {
            g.push((x, x + z, z));
        }
    }
    g
}

#[test]
fn surface_meshes_its_grid() {
    let blue = gpui::rgb(0x336699).into();
    assert_eq!(surface_faces(&ctx_of(&grid3x3()), &[blue]).len(), 4);
    assert_eq!(
        surface_faces(
            &ctx_of(&[(0., 0., 0.), (1., 1., 0.), (0., 1., 1.), (1., 2., 1.)]),
            &[blue]
        )
        .len(),
        1
    );
    assert!(
        surface_faces(&ctx_of(&grid3x3()), &[blue])
            .iter()
            .all(|f| f.color == Some(blue))
    );
    assert!(
        surface_faces(
            &ctx_of(&[(0., 1., 0.), (1., 2., 0.), (2., 1., 0.)]),
            &[blue]
        )
        .is_empty(),
        "one z value"
    );
    let holed: Vec<_> = grid3x3()
        .into_iter()
        .filter(|(x, _, z)| !(*x == 2. && *z == 2.))
        .collect();
    assert_eq!(
        surface_faces(&ctx_of(&holed), &[blue]).len(),
        3,
        "the face touching the missing corner is dropped"
    );
}

#[test]
fn surface_picks_the_nearest_ramp_color() {
    let (low, high): (gpui::Hsla, gpui::Hsla) =
        (gpui::rgb(0xaaaaaa).into(), gpui::rgb(0xbbbbbb).into());
    let faces = surface_faces(&ctx_of(&grid3x3()), &[low, high]);
    assert_eq!(faces.iter().filter(|f| f.color == Some(low)).count(), 1);
    assert_eq!(faces.iter().filter(|f| f.color == Some(high)).count(), 3);
    assert_eq!(
        lerp_color(&[], 0.5),
        None,
        "an empty ramp paints SVG's default"
    );
    assert_eq!(lerp_color(&[low, high], 0.49), Some(low));
    assert_eq!(lerp_color(&[low, high], 0.5), Some(high));
}

#[test]
fn surface_wireframe_draws_unfilled_outlines() {
    let c: gpui::Hsla = gpui::rgb(0x336699).into();
    let wire = face_style(Some(c), true, 0.8);
    assert_eq!(wire.fill, None, "fill none");
    assert_eq!(wire.stroke, Some((c.opacity(0.8), 1.)));
    let filled = face_style(Some(c), false, 0.8);
    assert_eq!(filled.fill, Some(c.opacity(0.8)));
    assert_eq!(filled.stroke, Some((c.opacity(0.3), 0.5)));
    assert_eq!(
        face_style(None, false, 0.8).fill,
        Some(gpui::black().opacity(0.8)),
        "no ramp color: SVG's black"
    );
}

#[test]
fn gl_points_match_the_svg_projection_and_clip_outside_the_cube() {
    let ctx = ctx_of(&[(0., 0., 0.), (2., 2., 2.), (1., 2., 0.)]);
    let positions = gl_positions(&ctx);
    let points = gl_points(&ctx, &positions, 4., 0.85, 1.);
    assert_eq!(points.len(), 3, "in data order, unsorted");
    for (gl, d) in points.iter().zip(ctx.data.iter()) {
        let (nx, ny, nz) = ctx.normalized(d);
        let svg = ctx.project(nx, ny, nz);
        assert!((gl.at.px - svg.px).abs() < 1e-3 && (gl.at.py - svg.py).abs() < 1e-3);
        assert!(
            (gl.radius - 4. * depth_factor(svg.depth) * 0.48 / 0.96).abs() < 1e-4,
            "the SVG radius"
        );
    }
    let far = [[40f32, 0.5, 0.5]];
    assert!(
        gl_points(&ctx, &far, 4., 0.85, 1.).is_empty(),
        "a center outside the clip volume"
    );
}

#[test]
fn js_sort_matches_v8_for_nan_orders() {
    // A NaN order is +0: V8 keeps [2, NaN, 1] as one ascending run, then inserts.
    let mut v = vec![2., f64::NAN, 1.];
    js_sort(&mut v, |a: &f64, b: &f64| a - b);
    assert_eq!(v[0], 2.);
    assert!(v[1].is_nan());
    assert_eq!(v[2], 1.);
    // 150 depths with NaNs among them, past the insertion-sort size, so the runs merge and
    // gallop; the expected order is Node's `sort((a, b) => a.d - b.d)`.
    let mut w: Vec<(f64, usize)> = (0..150)
        .map(|i| {
            let d = if i % 13 == 0 {
                f64::NAN
            } else {
                f64::from((i * 37) % 101) / 10. - if i % 7 == 0 { 20. } else { 0. }
            };
            (d, i as usize)
        })
        .collect();
    js_sort(&mut w, |a, b| a.0 - b.0);
    let v8: [usize; 150] = [
        0, 14, 28, 7, 21, 35, 11, 22, 33, 3, 25, 36, 6, 17, 9, 20, 31, 1, 12, 23, 34, 4, 15, 37,
        18, 29, 10, 32, 2, 24, 5, 16, 8, 13, 19, 26, 27, 38, 39, 63, 42, 56, 70, 49, 71, 41, 74,
        44, 55, 66, 47, 58, 69, 50, 61, 72, 53, 64, 75, 45, 67, 48, 59, 40, 51, 62, 73, 43, 54, 46,
        52, 57, 60, 65, 112, 77, 105, 84, 98, 101, 82, 93, 96, 88, 76, 78, 85, 107, 99, 110, 80,
        102, 113, 83, 94, 86, 97, 108, 89, 100, 111, 81, 92, 103, 95, 106, 87, 109, 79, 90, 91,
        104, 126, 140, 119, 133, 147, 142, 123, 134, 145, 115, 137, 148, 118, 129, 121, 132, 124,
        135, 146, 116, 127, 138, 149, 141, 144, 114, 117, 122, 125, 128, 120, 130, 136, 68, 139,
        30, 131, 143,
    ];
    assert_eq!(w.iter().map(|p| p.1).collect::<Vec<_>>(), v8);
}
