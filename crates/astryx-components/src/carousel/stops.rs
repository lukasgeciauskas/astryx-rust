//! The row's snap stops, upstream `SnapPaging/snapPagingValue.ts`: each item's start edge less
//! the scroll padding, clamped where the row ends; stops within half a pixel are one; a
//! stop within half its preceding step of the end joins the end's page.

/// The stops for items starting at `starts` (content coordinates), with `padding` of
/// scroll padding, in a row that scrolls to `end`.
pub(crate) fn snap_stops(starts: &[f32], padding: f32, end: f32) -> Vec<f32> {
    let mut stops: Vec<f32> = Vec::new();
    for start in starts {
        let stop = (start - padding).clamp(0., end.max(0.));
        if stops.last().is_none_or(|last| stop > last + 0.5) {
            stops.push(stop);
        }
    }
    let n = stops.len();
    if n >= 3
        && stops[n - 1] >= end - 0.5
        && stops[n - 1] - stops[n - 2] < (stops[n - 2] - stops[n - 3]) / 2.
    {
        stops.remove(n - 2);
    }
    if stops.is_empty() {
        stops.push(0.);
    }
    stops
}

/// Upstream `nearestStop`.
pub(crate) fn nearest_stop(stops: &[f32], at: f32) -> f32 {
    stops
        .iter()
        .copied()
        .fold(stops.first().copied().unwrap_or(at), |best, stop| {
            if (stop - at).abs() < (best - at).abs() {
                stop
            } else {
                best
            }
        })
}

fn nearest_index(stops: &[f32], at: f32) -> usize {
    let mut best = 0;
    for (i, stop) in stops.iter().enumerate() {
        if (stop - at).abs() < (stops[best] - at).abs() {
            best = i;
        }
    }
    best
}

/// Upstream `viewPages`: from stop `anchor`, each page lies on the stop nearest one view on
/// from the last, and at least one stop on.
pub(crate) fn view_pages(stops: &[f32], anchor: usize, width: f32) -> Vec<f32> {
    let mut pages = vec![stops[anchor]];
    let mut i = anchor;
    while i < stops.len() - 1 {
        i = nearest_index(stops, stops[i] + width).max(i + 1);
        pages.push(stops[i]);
    }
    let mut i = anchor;
    while i > 0 {
        i = nearest_index(stops, stops[i] - width).min(i - 1);
        pages.insert(0, stops[i]);
    }
    pages
}

/// An edge button's step (Carousel FR7): about one view, the viewport width less half an
/// item, landing on the nearest stop and moving at least one stop.
pub(crate) fn step_stop(stops: &[f32], at: f32, width: f32, item: f32, direction: f32) -> f32 {
    let target = at + direction * (width - item / 2.).max(item);
    let mut stop = nearest_stop(stops, target);
    if direction > 0. && stop <= at + 0.5 {
        stop = stops
            .iter()
            .copied()
            .find(|s| *s > at + 0.5)
            .unwrap_or(stop);
    } else if direction < 0. && stop >= at - 0.5 {
        stop = stops
            .iter()
            .rev()
            .copied()
            .find(|s| *s < at - 0.5)
            .unwrap_or(stop);
    }
    stop
}

/// AST-094 FR3's paging rule: from the page the drag started on, the release rests at
/// most one page away, the page nearest the flick's projected landing.
pub(crate) fn rest_page(pages: &[f32], start: f32, projected: f32) -> f32 {
    let i = nearest_index(pages, start);
    let lo = i.saturating_sub(1);
    let hi = (i + 1).min(pages.len() - 1);
    nearest_stop(&pages[lo..=hi], projected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_merge_clamp_and_join_the_end_page() {
        // Items every 100px, padding 0, a row that scrolls to 250.
        let stops = snap_stops(&[0., 100., 200., 300.], 0., 250.);
        assert_eq!(stops, vec![0., 100., 200., 250.]);
        // A last stop within half its preceding step of the end joins the end's page.
        let stops = snap_stops(&[0., 100., 200., 300.], 0., 220.);
        assert_eq!(stops, vec![0., 100., 220.]);
    }

    #[test]
    fn a_flick_moves_at_most_one_page() {
        let pages = vec![0., 300., 600., 900.];
        assert_eq!(rest_page(&pages, 300., 2000.), 600.);
        assert_eq!(rest_page(&pages, 300., 320.), 300.);
        assert_eq!(rest_page(&pages, 300., -900.), 0.);
    }

    #[test]
    fn view_pages_step_about_one_view() {
        let stops = vec![0., 100., 200., 300., 400., 500.];
        assert_eq!(view_pages(&stops, 0, 300.), vec![0., 300., 500.]);
        // 250 is as near 200 as 300: the nearer stop first in the row wins.
        assert_eq!(step_stop(&stops, 0., 300., 100., 1.), 200.);
        assert_eq!(step_stop(&stops, 0., 40., 100., 1.), 100.);
    }
}
