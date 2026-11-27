//! The page-control table (upstream `pageControlTable.ts`: UIPageControl's measured
//! geometry, alphas, gestures, window, fades and jog, from a stock UIPageControl on the
//! iOS 26.2 simulator, in points) and the pure math over it (upstream
//! `pageControlModel.ts`).
//!
//! Coordinates are along the row, in pixels, from the centre of the window's first
//! slot. Right-to-left callers mirror them before the call.

use astryx_theme::authoring::js::{js_max, js_min, js_round};

/// Diameter of one page indicator (a 5-page control's dots measure 9.67 pt).
pub const INDICATOR_SIZE: f64 = 9.67;
/// Space between neighbouring indicators (an 80.3 pt strip is 5 × 9.67 + 4 × 8).
pub const INDICATOR_SPACING: f64 = 8.;
/// Space between the strip and the control's leading and trailing edges.
pub const HORIZONTAL_PADDING: f64 = 14.;
/// The control's height (intrinsicContentSize 108.3 × 25.67 at 5 pages).
pub const HEIGHT: f64 = 25.67;
/// The control is never wider than this; past it the indicators scroll in a window.
pub const MAX_WIDTH: f64 = 161.3;
/// Opacity of the current page's indicator, of the indicator colour.
pub const CURRENT_INDICATOR_ALPHA: f64 = 1.;
/// Opacity UIKit draws every other page's indicator at.
pub const OTHER_INDICATOR_ALPHA: f64 = 0.45;
/// Pages a tap moves: one toward the side of the current indicator it landed on,
/// reported when the finger lifts.
pub const TAP_STEP: f64 = 1.;
/// How far a finger travels from where it landed before the control scrubs (the
/// first page change comes at 16.3 and 16.7 pt). Movement alone starts it.
pub const SCRUB_START_DISTANCE: f64 = 16.;
/// While scrubbing the page switches at the midpoint between two indicators' centres.
pub const SCRUB_SWITCH_POINT: f64 = 0.5;
/// A held finger turns the control interactive after this many milliseconds, moving
/// or not: the platter shows and the other indicators dim.
pub const INTERACTION_DELAY_MS: f64 = 155.;
/// Opacity of the indicators other than the current one while a finger is on the
/// control.
pub const INTERACTING_OTHER_INDICATOR_ALPHA: f64 = 0.25;
/// At most this many indicator slots show; past it the row is a window.
pub const VISIBLE_SLOTS: usize = 10;
/// Where pages continue past the window, its last two slots on that side shrink to
/// these diameters, outermost last.
pub const EDGE_INDICATOR_SIZES: [f64; 2] = [7.33, 4.31];
/// The spring every indicator slides on when the window moves (UIView spring,
/// damping ratio 1, response 0.15).
pub const WINDOW_SLIDE_RESPONSE: f64 = 0.15;
/// The window starts to move this long after the page changes.
pub const WINDOW_SLIDE_DELAY_MS: f64 = 50.;
/// After a tap, the indicator that stops being current fades over this long.
pub const TAP_FADE_OUT_MS: f64 = 125.;
/// After any other page change it fades over this long.
pub const FADE_OUT_MS: f64 = 250.;
/// The curve of both fades out.
pub const FADE_OUT_CURVE: [f64; 4] = [0., 0., 0.58, 1.];
/// Any other opacity change, a switch, takes this long.
pub const SWITCH_MS: f64 = 15.625;
/// The curve of those switches.
pub const SWITCH_CURVE: [f64; 4] = [0.42, 0., 0.58, 1.];
/// Past the window's edge while scrubbing toward the last page, pages per second for
/// each point the finger is past the content's edge.
pub const JOG_RATE_PER_POINT: f64 = 0.173;
/// The same toward the first page, where the run is slower.
pub const JOG_RATE_PER_POINT_BACKWARD: f64 = 0.13;
/// The fastest the control steps pages past the window's edge, pages per second.
pub const JOG_MAX_RATE: f64 = 15.;

/// Distance between neighbouring indicators' centres (upstream `PITCH`).
pub const PITCH: f64 = INDICATOR_SIZE + INDICATOR_SPACING;

/// Slots rendered on each side of the window, so a slide has its dots (upstream
/// `RENDER_MARGIN`).
pub const RENDER_MARGIN: usize = 3;

/// UIKit draws the other indicators at 45 %. At that alpha, Core's primary text
/// colour on its page background measures 2.97:1, under the 3:1 that WCAG 1.4.11 asks
/// of the only page-count cue; 48 % gives 3.25:1 (upstream `INACTIVE_ALPHA`).
pub const INACTIVE_ALPHA: f64 = 0.48;

/// `page` rounded into 0 to `count - 1` (upstream `clampPage`).
pub fn clamp_page(page: f64, count: f64) -> f64 {
    js_max(0., js_min(count - 1., js_round(page)))
}

/// The control's size for a page count (upstream `PageControlLayout`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageControlLayout {
    /// The control's width, px.
    pub width: f64,
    /// How many indicator slots show.
    pub slots: usize,
    /// Width of the row of shown slots, px; may exceed `width`.
    pub row_width: f64,
    /// More pages than slots, so the row is a window.
    pub is_windowed: bool,
}

/// The control's size for `count` pages, UIPageControl's as measured (upstream
/// `pageControlLayout`).
pub fn page_control_layout(count: f64) -> PageControlLayout {
    let n = js_max(0., count.floor());
    let slots = js_min(n, VISIBLE_SLOTS as f64);
    let row_width = if slots == 0. {
        0.
    } else {
        slots * PITCH - INDICATOR_SPACING
    };
    let natural = (if n == 0. {
        0.
    } else {
        n * PITCH - INDICATOR_SPACING
    }) + 2. * HORIZONTAL_PADDING;
    let hundredths = |value: f64| js_round(value * 100.) / 100.;
    PageControlLayout {
        width: hundredths(js_min(natural, MAX_WIDTH)),
        slots: slots as usize,
        row_width: hundredths(row_width),
        is_windowed: n > VISIBLE_SLOTS as f64,
    }
}

/// The pages a windowed row renders, `[first, last)`: the window's slots and
/// `RENDER_MARGIN` slots on each side (upstream `renderedPages`).
pub fn rendered_pages(count: usize, window_start: usize) -> (usize, usize) {
    let slots = page_control_layout(count as f64).slots;
    (
        window_start.saturating_sub(RENDER_MARGIN),
        count.min(window_start + slots + RENDER_MARGIN),
    )
}

/// With a fine pointer the row shows every page at full size, cropped by the control
/// when it is wider: how far the row shifts toward its start, in px, keeping the
/// current indicator in view (centred where it can be, pinned at either end; upstream
/// `croppedRowOffset`).
pub fn cropped_row_offset(count: f64, current: f64) -> f64 {
    let n = js_max(0., count.floor());
    let row = if n == 0. {
        0.
    } else {
        n * PITCH - INDICATOR_SPACING
    };
    let visible = page_control_layout(n).width - 2. * HORIZONTAL_PADDING;
    if row <= visible {
        return 0.;
    }
    let center = current * PITCH + INDICATOR_SIZE / 2.;
    js_max(0., js_min(row - visible, center - visible / 2.))
}

/// Diameter of page `page`'s indicator while the window starts at `window_start`
/// (upstream `indicatorSize`): full inside the window and 0 outside it; where pages
/// continue, the last two slots on that side take the two edge sizes.
pub fn indicator_size(count: usize, window_start: usize, page: isize) -> f64 {
    if count <= VISIBLE_SLOTS {
        return if page >= 0 && (page as usize) < count {
            INDICATOR_SIZE
        } else {
            0.
        };
    }
    let slot = page - window_start as isize;
    if slot < 0 || slot >= VISIBLE_SLOTS as isize {
        return 0.;
    }
    let [near, far] = EDGE_INDICATOR_SIZES;
    if window_start > 0 {
        if slot == 0 {
            return far;
        }
        if slot == 1 {
            return near;
        }
    }
    if window_start + VISIBLE_SLOTS < count {
        if slot == VISIBLE_SLOTS as isize - 1 {
            return far;
        }
        if slot == VISIBLE_SLOTS as isize - 2 {
            return near;
        }
    }
    INDICATOR_SIZE
}

/// The first and last pages shown at full size (upstream `fullPages`).
pub fn full_pages(count: usize, window_start: usize) -> (isize, isize) {
    if count <= VISIBLE_SLOTS {
        return (0, count as isize - 1);
    }
    let first = window_start + if window_start > 0 { 2 } else { 0 };
    let last = window_start + VISIBLE_SLOTS
        - 1
        - if window_start + VISIBLE_SLOTS < count {
            2
        } else {
            0
        };
    (first as isize, last as isize)
}

/// The window after the current page changes (upstream `windowFor`): unchanged while
/// the current page is at full size, otherwise moved the least that makes it so.
pub fn window_for(count: usize, window_start: usize, current: usize) -> usize {
    if count <= VISIBLE_SLOTS {
        return 0;
    }
    let max_start = count - VISIBLE_SLOTS;
    let mut start = window_start.min(max_start);
    let current = current as isize;
    for _ in 0..=count {
        let (first, last) = full_pages(count, start);
        if current > last && start < max_start {
            start += 1;
        } else if current < first && start > 0 {
            start -= 1;
        } else {
            break;
        }
    }
    start
}

/// The page a tap moves to (upstream `tapTarget`): one page toward the side of the
/// current indicator the tap landed on (trailing is forward), held to the pages. `x`
/// and `current_center` are in the same horizontal coordinates.
pub fn tap_target(x: f64, current_center: f64, current: f64, count: f64, rtl: bool) -> f64 {
    let trailing = if rtl {
        x < current_center
    } else {
        x > current_center
    };
    let next = current + if trailing { TAP_STEP } else { -TAP_STEP };
    js_max(0., js_min(count - 1., next))
}

/// The page under a scrubbing finger `x` px along the row from the centre of the
/// window's first slot (upstream `scrubTarget`): it switches at the midpoint between
/// indicators and stays within the pages shown at full size.
pub fn scrub_target(x: f64, count: usize, window_start: usize) -> f64 {
    let slot = (x / PITCH + SCRUB_SWITCH_POINT).floor();
    let start = if count <= VISIBLE_SLOTS {
        0
    } else {
        window_start
    };
    let (first, last) = full_pages(count, start);
    js_max(first as f64, js_min(last as f64, start as f64 + slot))
}

/// Seconds until the next page while a scrubbing finger is `distance` px past the edge
/// of the control's content (upstream `jogInterval`): infinite when not past it.
/// `direction` is 1 toward the last page and -1 toward the first; UIKit's run toward
/// the first page is slower.
pub fn jog_interval(distance: f64, direction: f64) -> f64 {
    if distance.is_nan() || distance <= 0. {
        return f64::INFINITY;
    }
    let per_point = if direction < 0. {
        JOG_RATE_PER_POINT_BACKWARD
    } else {
        JOG_RATE_PER_POINT
    };
    1. / js_min(JOG_MAX_RATE, per_point * distance)
}

/// The page count: 0 for a total that is not a finite number (upstream
/// `pageCountOf`).
pub fn page_count_of(total_pages: f64) -> usize {
    if total_pages.is_finite() {
        js_max(0., total_pages.floor()) as usize
    } else {
        0
    }
}

/// The current page, 0-based, from the 1-based `page` (pages count from 1 outside
/// and from 0 inside).
pub fn current_of(page: f64, pages: usize) -> usize {
    if pages == 0 || !page.is_finite() {
        0
    } else {
        clamp_page(page - 1., pages as f64) as usize
    }
}
