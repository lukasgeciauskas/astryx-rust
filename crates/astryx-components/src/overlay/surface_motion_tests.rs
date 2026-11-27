//! The touch presentation's growth, after upstream `Popover/popoverSurfaceMotion.test.ts`
//! and `Layer/layerEntranceSide.test.ts`.

use std::time::Duration;

use super::*;

fn b(left: f32, top: f32, width: f32, height: f32) -> SurfaceBox {
    SurfaceBox {
        left,
        top,
        width,
        height,
    }
}

/// A 320 x 253 popover below a 120 x 44 source centred at x 201, 4 px apart.
fn geometry() -> (SurfaceBox, SurfaceBox) {
    (b(41., 204., 320., 253.), b(141., 156., 120., 44.))
}

fn close(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() < eps
}

#[test]
fn grows_a_popover_below_its_source_out_of_the_sources_bottom_centre() {
    let (surface, source) = geometry();
    let s = popover_collapsed_state(surface, source);
    assert_eq!((s.origin_x, s.origin_y, s.axis), (160., 0., GrowAxis::Y));
    assert_eq!(s.scale, POPOVER_COLLAPSED_SCALE);
    assert!(close(s.length, 100. / 253., 1e-6));
}

#[test]
fn turns_the_same_motion_toward_a_source_above_or_beside_the_popover() {
    let above = popover_collapsed_state(b(41., 100., 320., 200.), b(141., 310., 120., 44.));
    assert_eq!(
        (above.origin_x, above.origin_y, above.axis),
        (160., 200., GrowAxis::Y)
    );
    let right = popover_collapsed_state(b(200., 100., 180., 240.), b(20., 150., 160., 40.));
    assert_eq!(
        (right.origin_x, right.origin_y, right.axis),
        (0., 70., GrowAxis::X)
    );
    assert!(close(right.length, 100. / 180., 1e-6));
    let left = popover_collapsed_state(b(10., 100., 180., 240.), b(200., 150., 160., 40.));
    assert_eq!(
        (left.origin_x, left.origin_y, left.axis),
        (180., 70., GrowAxis::X)
    );
}

#[test]
fn keeps_the_point_on_the_popover_when_its_source_is_wider_and_never_grows_a_short_popover() {
    let s = popover_collapsed_state(b(300., 60., 90., 80.), b(0., 10., 402., 44.));
    assert_eq!((s.origin_x, s.origin_y), (0., 0.));
    assert_eq!(s.length, 1.);
}

#[test]
fn keeps_every_frame_on_uikits_path_the_length_growing_from_100_pt() {
    let (surface, source) = geometry();
    let s = popover_collapsed_state(surface, source);
    for p in [0., 0.1, 0.3126, 0.5, 0.9, 1., 1.0152] {
        let f = popover_frame(s, p);
        let scale = POPOVER_COLLAPSED_SCALE + (1. - POPOVER_COLLAPSED_SCALE) * p;
        assert!(close(320. * f.scale_x, 320. * scale, 1e-3));
        assert!(close(253. * f.scale_y, scale * (100. + 153. * p), 1e-3));
        assert!(close(f.opacity, p, 1e-6));
    }
}

#[test]
fn matches_the_popover_views_measured_frames() {
    let (surface, source) = geometry();
    let s = popover_collapsed_state(surface, source);
    for (scale, width, height) in [
        (0.0865, 27.69, 9.68),
        (0.3195, 102.25, 47.24),
        (0.6664, 213.26, 134.25),
        (0.9632, 308.22, 238.2),
        (1.015, 324.79, 259.14),
    ] {
        let p = (scale - POPOVER_COLLAPSED_SCALE) / (1. - POPOVER_COLLAPSED_SCALE);
        let f = popover_frame(s, p);
        assert!(close(320. * f.scale_x, width, 0.06), "{width}");
        assert!(close(253. * f.scale_y, height, 0.06), "{height}");
    }
}

#[test]
fn caps_the_side_of_a_layer_flush_against_its_trigger_and_gives_nothing_without_size() {
    assert!(layer_entrance_side(b(0., 0., 0., 10.), b(0., 0., 10., 10.)).is_none());
    let side = layer_entrance_side(b(0., 44., 100., 100.), b(0., 0., 100., 44.)).unwrap();
    assert_eq!((side.x, side.y), (0, -1));
}

fn timing(reduced: bool) -> SurfaceTiming {
    let linear = astryx_theme::CubicBezier(0., 0., 1., 1.);
    SurfaceTiming {
        enter: (
            Duration::from_millis(if reduced { 200 } else { 225 }),
            linear,
        ),
        exit: (
            Duration::from_millis(if reduced { 200 } else { 150 }),
            linear,
        ),
        reduced,
    }
}

#[test]
fn grows_the_popover_out_of_its_source_on_the_entrance_tokens_measuring_it_first() {
    let now = Instant::now();
    let mut m = SurfaceMotion::default();
    m.enter(now);
    // The frame that opens it has no box yet: nothing shows.
    let (f, _, moving) = m.frame(None, timing(false), now).unwrap();
    assert_eq!(f.opacity, 0.);
    assert!(moving);
    let (f, origin, _) = m.frame(Some(geometry()), timing(false), now).unwrap();
    assert!(close(f.scale_x, POPOVER_COLLAPSED_SCALE, 1e-6));
    assert!(close(origin.0, 0.5, 1e-6) && origin.1 == 0.);
    assert_eq!(
        f.opacity, ENTRANCE_OPACITY_FLOOR,
        "it fades in from the floor"
    );
    let (f, _, moving) = m
        .frame(
            Some(geometry()),
            timing(false),
            now + Duration::from_millis(225),
        )
        .unwrap();
    assert_eq!((f.scale_x, f.scale_y, f.opacity), (1., 1., 1.));
    assert!(!moving);
}

#[test]
fn shrinks_back_into_its_source_on_the_exit_tokens_and_is_gone_when_it_ends() {
    let now = Instant::now();
    let mut m = SurfaceMotion::default();
    m.enter(now);
    m.frame(
        Some(geometry()),
        timing(false),
        now + Duration::from_millis(300),
    );
    let closed = now + Duration::from_millis(400);
    assert!(m.exit(closed));
    let (f, _, _) = m
        .frame(
            Some(geometry()),
            timing(false),
            closed + Duration::from_millis(75),
        )
        .unwrap();
    assert!(close(f.opacity, 0.5, 1e-4));
    assert!(
        m.frame(
            Some(geometry()),
            timing(false),
            closed + Duration::from_millis(150)
        )
        .is_none()
    );
}

#[test]
fn dismissed_during_the_growth_shrinks_from_the_frame_on_screen() {
    let now = Instant::now();
    let mut m = SurfaceMotion::default();
    m.enter(now);
    let (half, _, _) = m
        .frame(
            Some(geometry()),
            timing(false),
            now + Duration::from_millis(112),
        )
        .unwrap();
    let closed = now + Duration::from_millis(112);
    m.exit(closed);
    let (f, _, _) = m.frame(Some(geometry()), timing(false), closed).unwrap();
    assert!(close(f.scale_x, half.scale_x, 1e-4) && close(f.opacity, half.opacity, 1e-4));
}

#[test]
fn presented_again_during_the_dismissal_grows_back_from_the_frame_on_screen() {
    let now = Instant::now();
    let mut m = SurfaceMotion::default();
    m.enter(now);
    m.frame(
        Some(geometry()),
        timing(false),
        now + Duration::from_millis(300),
    );
    let closed = now + Duration::from_millis(300);
    m.exit(closed);
    let (mid, _, _) = m
        .frame(
            Some(geometry()),
            timing(false),
            closed + Duration::from_millis(75),
        )
        .unwrap();
    m.enter(closed + Duration::from_millis(75));
    let (f, _, _) = m
        .frame(
            Some(geometry()),
            timing(false),
            closed + Duration::from_millis(75),
        )
        .unwrap();
    assert!(close(f.opacity, mid.opacity, 1e-4) && close(f.scale_y, mid.scale_y, 1e-4));
}

#[test]
fn a_dismissal_with_no_touch_presentation_before_it_does_nothing() {
    let mut m = SurfaceMotion::default();
    assert!(!m.exit(Instant::now()));
}

#[test]
fn reduce_motion_cross_fades_in_at_its_size_and_out_where_it_stands_in_200_ms() {
    let now = Instant::now();
    let mut m = SurfaceMotion::default();
    m.enter(now);
    let (f, _, _) = m
        .frame(
            Some(geometry()),
            timing(true),
            now + Duration::from_millis(100),
        )
        .unwrap();
    assert_eq!((f.scale_x, f.scale_y), (1., 1.));
    assert!(close(f.opacity, 0.5, 1e-4));
    m.frame(
        Some(geometry()),
        timing(true),
        now + Duration::from_millis(200),
    );
    let closed = now + Duration::from_millis(300);
    m.exit(closed);
    let (f, _, _) = m
        .frame(
            Some(geometry()),
            timing(true),
            closed + Duration::from_millis(100),
        )
        .unwrap();
    assert_eq!((f.scale_x, f.scale_y), (1., 1.));
    assert!(close(f.opacity, 0.5, 1e-4));
}
