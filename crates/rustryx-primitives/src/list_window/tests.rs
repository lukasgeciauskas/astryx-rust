//! The engine's tests, after upstream `listWindow.test.ts`, and the
//! adapter's, after `useListWindow.test.tsx` and the record's FR1-FR4.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render, ScrollHandle,
    StatefulInteractiveElement, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::*;

fn defaults() -> ListWindowOptions {
    ListWindowOptions {
        row_count: 100,
        row_height: 20.,
        viewport_height: 100.,
        scroll_top: 0.,
        scroll_height: None,
        start_offset: 0.,
        is_measured: true,
        focus_index: None,
    }
}

fn rows(plan: &ListWindowPlan) -> Vec<usize> {
    plan.segments
        .iter()
        .flat_map(|s| match s {
            ListWindowSegment::Rows { start, end } => (*start..*end).collect(),
            ListWindowSegment::Gap { .. } => Vec::new(),
        })
        .collect()
}

fn window(o: ListWindowOptions) -> ListWindowPlan {
    calculate_list_window(&o).expect("a finite window")
}

#[test]
fn seeds_the_first_rows_before_measuring() {
    let plan = window(ListWindowOptions {
        is_measured: false,
        scroll_top: 800.,
        ..defaults()
    });
    assert_eq!(rows(&plan), (0..9).collect::<Vec<_>>());
    assert_eq!(
        plan.segments,
        [
            ListWindowSegment::Rows { start: 0, end: 9 },
            ListWindowSegment::Gap {
                start: 9,
                end: 100,
                size: 1820.
            },
        ]
    );
}

#[test]
fn intersects_partial_rows_then_overscans_four_per_side() {
    let plan = window(ListWindowOptions {
        scroll_top: 210.,
        ..defaults()
    });
    assert_eq!(rows(&plan), (6..20).collect::<Vec<_>>());
    assert_eq!((plan.first_visible_index, plan.offset), (10, 10.));
}

#[test]
fn reports_the_negative_distance_to_the_first_row_in_leading_content() {
    let plan = window(ListWindowOptions {
        start_offset: 40.,
        scroll_top: 25.,
        ..defaults()
    });
    assert_eq!((plan.first_visible_index, plan.offset), (0, -15.));
    assert_eq!(
        window(ListWindowOptions {
            start_offset: 40.,
            ..defaults()
        })
        .offset,
        -40.
    );
}

#[test]
fn accounts_for_the_start_offset_and_a_partly_visible_list() {
    assert_eq!(
        rows(&window(ListWindowOptions {
            start_offset: 80.,
            ..defaults()
        })),
        [0, 1, 2, 3, 4]
    );
    assert_eq!(
        rows(&window(ListWindowOptions {
            start_offset: 80.,
            scroll_top: 100.,
            ..defaults()
        })),
        (0..10).collect::<Vec<_>>()
    );
}

#[test]
fn retains_one_focused_row_far_below_with_truthful_gaps() {
    let plan = window(ListWindowOptions {
        scroll_top: 400.,
        focus_index: Some(80),
        ..defaults()
    });
    let mut expected: Vec<usize> = (16..29).collect();
    expected.push(80);
    assert_eq!(rows(&plan), expected);
    let gaps = plan
        .segments
        .iter()
        .filter(|s| matches!(s, ListWindowSegment::Gap { .. }))
        .count();
    assert_eq!(gaps, 3);
    let extent: f64 = plan
        .segments
        .iter()
        .map(|s| match s {
            ListWindowSegment::Gap { size, .. } => *size,
            ListWindowSegment::Rows { start, end } => (end - start) as f64 * 20.,
        })
        .sum();
    assert_eq!(extent, 2000.);
}

#[test]
fn does_not_duplicate_focus_within_or_beside_the_range() {
    for focus in [28, 29] {
        let plan = window(ListWindowOptions {
            scroll_top: 400.,
            focus_index: Some(focus),
            ..defaults()
        });
        let runs = plan
            .segments
            .iter()
            .filter(|s| matches!(s, ListWindowSegment::Rows { .. }))
            .count();
        assert_eq!(runs, 1, "focus {focus}");
    }
}

#[test]
fn clamps_negative_and_stale_end_scrolls_and_takes_a_fractional_pitch() {
    assert_eq!(
        rows(&window(ListWindowOptions {
            scroll_top: -100.,
            ..defaults()
        })),
        rows(&window(defaults()))
    );
    assert_eq!(
        rows(&window(ListWindowOptions {
            scroll_top: 1e9,
            ..defaults()
        })),
        (91..100).collect::<Vec<_>>()
    );
    let plan = window(ListWindowOptions {
        row_height: 12.5,
        viewport_height: 31.25,
        scroll_top: 131.25,
        ..defaults()
    });
    assert_eq!((plan.first_visible_index, plan.offset), (10, 6.25));
}

#[test]
fn an_empty_list_has_no_segments_and_a_short_one_is_bounded() {
    assert!(
        window(ListWindowOptions {
            row_count: 0,
            ..defaults()
        })
        .segments
        .is_empty()
    );
    assert_eq!(
        rows(&window(ListWindowOptions {
            row_count: 3,
            ..defaults()
        })),
        [0, 1, 2]
    );
}

#[test]
fn rejects_invalid_arithmetic() {
    for row_height in [
        0.,
        -1.,
        f64::NAN,
        f64::INFINITY,
        f64::MIN_POSITIVE * f64::EPSILON,
    ] {
        assert!(
            calculate_list_window(&ListWindowOptions {
                row_height,
                ..defaults()
            })
            .is_none(),
            "{row_height}"
        );
    }
    for viewport_height in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(
            calculate_list_window(&ListWindowOptions {
                viewport_height,
                ..defaults()
            })
            .is_none()
        );
    }
    assert!(
        calculate_list_window(&ListWindowOptions {
            scroll_top: f64::NAN,
            ..defaults()
        })
        .is_none()
    );
    assert!(
        calculate_list_window(&ListWindowOptions {
            start_offset: f64::INFINITY,
            ..defaults()
        })
        .is_none()
    );
    assert!(
        calculate_list_window(&ListWindowOptions {
            row_height: 1e308,
            ..defaults()
        })
        .is_none()
    );
    assert!(
        calculate_list_window(&ListWindowOptions {
            row_count: 1,
            row_height: 1e308,
            start_offset: 1e308,
            ..defaults()
        })
        .is_none()
    );
    assert!(
        calculate_list_window(&ListWindowOptions {
            row_count: 1_000_000,
            row_height: 40.,
            ..defaults()
        })
        .is_some()
    );
}

#[test]
fn clamps_to_a_larger_scroll_height_and_rejects_an_invalid_one() {
    let base = ListWindowOptions {
        row_count: 10,
        scroll_top: 250.,
        ..defaults()
    };
    let rows_only = window(base.clone());
    let trailing = window(ListWindowOptions {
        scroll_height: Some(300.),
        ..base.clone()
    });
    assert!(trailing.first_visible_index > rows_only.first_visible_index);
    assert!(
        calculate_list_window(&ListWindowOptions {
            scroll_height: Some(-1.),
            ..base.clone()
        })
        .is_none()
    );
    assert!(
        calculate_list_window(&ListWindowOptions {
            scroll_height: Some(f64::NAN),
            ..base
        })
        .is_none()
    );
}

#[test]
fn keeps_ordered_coverage_and_bounds_across_fractional_and_end_cases() {
    for row_count in [0usize, 1, 9, 100, 100_000] {
        for row_height in [12.5, 20., 48.] {
            for viewport_height in [1., 100., 240., 1000.] {
                let total = row_count as f64 * row_height;
                for scroll_top in [0., 1., 100., total, total * 10.] {
                    for focus in [0, row_count / 2, row_count.saturating_sub(1)] {
                        let plan = window(ListWindowOptions {
                            row_count,
                            row_height,
                            viewport_height,
                            scroll_top,
                            start_offset: 37.,
                            is_measured: true,
                            focus_index: Some(focus),
                            scroll_height: None,
                        });
                        let indices = rows(&plan);
                        assert!(indices.windows(2).all(|w| w[0] < w[1]));
                        assert!(
                            indices.len()
                                <= (viewport_height / row_height).ceil() as usize + 1 + 8 + 1
                        );
                        let gaps = plan
                            .segments
                            .iter()
                            .filter(|s| matches!(s, ListWindowSegment::Gap { .. }))
                            .count();
                        assert!(gaps <= 3);
                        let (mut end, mut extent) = (0, 0.);
                        for s in &plan.segments {
                            assert_eq!(s.start(), end);
                            assert!(s.end() > s.start());
                            end = s.end();
                            extent += match s {
                                ListWindowSegment::Gap { size, .. } => *size,
                                ListWindowSegment::Rows { start, end } => {
                                    (end - start) as f64 * row_height
                                }
                            };
                        }
                        assert_eq!(end, row_count);
                        assert!((extent - total).abs() < 1e-6);
                    }
                }
            }
        }
    }
}

#[test]
fn bounds_work_at_100k() {
    let plan = window(ListWindowOptions {
        row_count: 100_000,
        scroll_top: 1_000_010.,
        focus_index: Some(99_999),
        ..defaults()
    });
    assert!(rows(&plan).len() <= 5 + 1 + 8 + 1);
    assert!(plan.segments.len() <= 5);
}

fn keys(list: &[&'static str]) -> Vec<ListWindowKey> {
    list.iter().map(|k| ListWindowKey::from(*k)).collect()
}

fn anchor_with<'a>(
    previous: &'a [ListWindowKey],
    next: &'a [ListWindowKey],
    f: impl FnOnce(&mut ListAnchorOptions<'a>),
) -> f64 {
    let mut o = ListAnchorOptions {
        previous_keys: previous,
        keys: next,
        first_visible_index: 1,
        offset: 5.,
        row_height: 20.,
        viewport_height: 20.,
        start_offset: 10.,
        ..ListAnchorOptions::default()
    };
    f(&mut o);
    reconcile_list_anchor(&o)
}

#[test]
fn anchors_the_visible_key_and_fraction() {
    let previous = keys(&["a", "b", "c", "d"]);
    assert_eq!(
        anchor_with(&previous, &keys(&["x", "a", "b", "c", "d"]), |_| {}),
        55.
    );
    assert_eq!(
        anchor_with(&previous, &keys(&["a", "b", "c", "d", "e"]), |_| {}),
        35.
    );
}

#[test]
fn a_gone_anchor_picks_a_successor_then_a_predecessor_then_the_start() {
    let previous = keys(&["a", "b", "c", "d"]);
    assert_eq!(anchor_with(&previous, &keys(&["d", "c"]), |_| {}), 30.);
    assert_eq!(anchor_with(&previous, &keys(&["a"]), |_| {}), 10.);
    assert_eq!(
        anchor_with(&previous, &keys(&["z", "a"]), |o| o.viewport_height = 1.),
        35.
    );
    assert_eq!(anchor_with(&previous, &keys(&["x", "y"]), |_| {}), 10.);
}

#[test]
fn numbers_and_strings_are_different_keys_and_the_new_extent_clamps() {
    let previous = vec![
        ListWindowKey::from(0i64),
        ListWindowKey::from("0"),
        ListWindowKey::from(1i64),
    ];
    let next = vec![
        ListWindowKey::from("0"),
        ListWindowKey::from(0i64),
        ListWindowKey::from(1i64),
    ];
    assert_eq!(anchor_with(&previous, &next, |_| {}), 15.);
    let previous = keys(&["a", "b", "c", "d"]);
    assert_eq!(
        anchor_with(&previous, &keys(&["b"]), |o| o.viewport_height = 100.),
        0.
    );
}

#[test]
fn keeps_a_position_inside_the_leading_content() {
    let previous = keys(&["a", "b"]);
    let top = |next: &[&'static str], offset: f64| {
        anchor_with(&previous, &keys(next), |o| {
            o.first_visible_index = 0;
            o.start_offset = 40.;
            o.offset = offset;
        })
    };
    assert_eq!(top(&["a", "b", "c"], -40.), 0.);
    assert_eq!(top(&["a", "b", "c"], -15.), 25.);
    assert_eq!(top(&["x", "a", "b"], -15.), 45.);
    assert_eq!(top(&["x", "y", "z"], -40.), 0.);
    assert_eq!(top(&["x", "y", "z"], -15.), 25.);
    assert_eq!(top(&["x", "y", "z"], 10.), 40.);
}

#[test]
fn clamps_to_trailing_content_and_reads_a_supplied_key_index() {
    let previous = keys(&["a", "b"]);
    assert_eq!(anchor_with(&previous, &previous, |_| {}), 30.);
    assert_eq!(
        anchor_with(&previous, &previous, |o| o.trailing_content_height = 20.),
        35.
    );
    let map = HashMap::from([(ListWindowKey::from("b"), 3)]);
    assert_eq!(
        anchor_with(&previous, &previous, |o| {
            o.trailing_content_height = 100.;
            o.key_indices = Some(&map);
        }),
        75.
    );
}

const SIZES: [f64; 10] = [100., 50., 150., 100., 100., 100., 100., 100., 100., 100.];

fn sized() -> Vec<f64> {
    let mut offsets = vec![0.];
    for s in SIZES {
        offsets.push(offsets.last().unwrap() + s);
    }
    offsets
}

fn range(f: impl FnOnce(&mut VariableListRangeOptions)) -> VariableListRange {
    let offsets = sized();
    let mut o = VariableListRangeOptions {
        offsets: &offsets,
        viewport_size: 250.,
        scroll_offset: 0.,
        is_measured: true,
        overscan: 2,
    };
    f(&mut o);
    calculate_variable_list_range(&o)
}

#[test]
fn variable_ranges() {
    let se = |r: VariableListRange| (r.start, r.end);
    assert_eq!(se(range(|o| o.is_measured = false)), (0, 5));
    assert_eq!(se(range(|o| o.scroll_offset = 320.)), (1, 8));
    let visible = range(|o| o.scroll_offset = 320.);
    assert_eq!((visible.first_visible_index, visible.offset), (3, 20.));
    assert_eq!(se(range(|o| o.scroll_offset = 99999.)), (5, 10));
    assert_eq!(se(range(|o| o.viewport_size = 2000.)), (0, 10));
    assert_eq!(
        se(range(|o| {
            o.viewport_size = 0.;
            o.scroll_offset = 320.;
        })),
        (1, 6)
    );
    let empty = calculate_variable_list_range(&VariableListRangeOptions {
        offsets: &[0.],
        viewport_size: 250.,
        scroll_offset: 0.,
        is_measured: true,
        overscan: 2,
    });
    assert_eq!(se(empty), (0, 0));
}

#[test]
fn anchoring_with_item_offsets() {
    let previous = keys(&["a", "b", "c", "d"]);
    let run = |next: &[&'static str], offsets: &[f64], viewport: f64| {
        let next = keys(next);
        reconcile_list_anchor(&ListAnchorOptions {
            previous_keys: &previous,
            keys: &next,
            first_visible_index: 2,
            offset: 0.5,
            offsets: Some(offsets),
            viewport_height: viewport,
            ..ListAnchorOptions::default()
        })
    };
    assert_eq!(run(&["b", "c", "d"], &[0., 100., 250., 300.], 50.), 175.);
    assert_eq!(run(&["b", "d"], &[0., 100., 150.], 10.), 125.);
    assert_eq!(run(&["b", "c", "d"], &[0., 100., 250., 300.], 200.), 100.);
}

#[test]
fn fr1_the_end_and_a_given_item_anchor() {
    let previous = keys(&["a", "b", "c", "d"]);
    let next = keys(&["a", "b", "c", "d", "e", "f"]);
    let o = ListAnchorOptions {
        previous_keys: &previous,
        keys: &next,
        first_visible_index: 2,
        offset: 0.,
        row_height: 20.,
        viewport_height: 40.,
        ..ListAnchorOptions::default()
    };
    // At the end, it follows the end; away from it, the first row holds.
    assert_eq!(
        reconcile_anchor(&ListAnchor::End, &o, 40., 40., |_| None),
        80.
    );
    assert_eq!(
        reconcile_anchor(&ListAnchor::End, &o, 20., 40., |_| None),
        40.
    );
    // A given item keeps its distance from the viewport's start.
    let item = ListAnchor::Item("c".into());
    assert_eq!(
        reconcile_anchor(&item, &o, 30., 40., |_| Some((40., 80.))),
        70.
    );
}

#[test]
fn inline_scroll_reads_from_the_inline_start() {
    assert_eq!(logical_inline_scroll(-30., 100., false), 30.);
    assert_eq!(logical_inline_scroll(-30., 100., true), 70.);
    assert_eq!(physical_inline_scroll(70., 100., true), -30.);
    assert_eq!(physical_inline_scroll(30., 100., false), -30.);
}

// ---- The adapter -------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// A 200px container scroller holding the list.
    Container,
    /// A page: a 200px scroller, 100px of leading content, then the list,
    /// with the scroller provided as the page's host.
    Page,
}

struct Host {
    keys: Rc<RefCell<Vec<ListWindowKey>>>,
    sizes: Rc<RefCell<HashMap<ListWindowKey, f32>>>,
    pitch: ListPitch,
    mode: Mode,
    anchor: ListAnchor,
    can_window: bool,
    axis: WindowedListAxis,
    rtl: bool,
    insets: (f32, f32),
    scroll: ScrollHandle,
    renders: Rc<std::cell::Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let keys: Rc<[ListWindowKey]> = self.keys.borrow().clone().into();
        let (sizes, inline) = (self.sizes.clone(), self.axis == WindowedListAxis::Inline);
        let row_keys = keys.clone();
        let list = WindowedList::new("list", keys, self.pitch, move |i, _, _| {
            let key = row_keys[i].clone();
            let size = sizes.borrow().get(&key).copied().unwrap_or(20.);
            let name = match &key {
                ListWindowKey::Number(n) => n.to_string(),
                ListWindowKey::Text(t) => t.to_string(),
            };
            let row = div().id(gpui::ElementId::Name(format!("row-{name}").into()));
            let row = if inline {
                row.h(px(20.)).w(px(size))
            } else {
                row.w_full().h(px(size))
            };
            row.debug_selector(move || format!("row-{name}"))
                .focusable()
                .into_any_element()
        })
        .anchor(self.anchor.clone())
        .can_window(self.can_window)
        .axis(self.axis)
        .rtl(self.rtl)
        .insets(px(self.insets.0), px(self.insets.1))
        .initial_viewport(px(200.));
        let list = if inline { list } else { list.w_full() };
        let scroller = div()
            .id("scroller")
            .track_scroll(&self.scroll)
            .debug_selector(|| "scroller-view".into());
        let scroller = if inline {
            scroller.flex().overflow_x_scroll().w(px(200.)).h(px(40.))
        } else {
            scroller.overflow_y_scroll().h(px(200.)).w(px(300.))
        };
        match self.mode {
            Mode::Container => scroller.child(list.scroll(&self.scroll)),
            Mode::Page => scroller.child(crate::provide(
                ScrollHost(self.scroll.clone()),
                div()
                    .flex()
                    .flex_col()
                    .child(div().h(px(100.)).flex_none())
                    .child(list)
                    .child(div().h(px(100.)).flex_none()),
            )),
        }
    }
}

struct Setup {
    renders: Rc<std::cell::Cell<usize>>,
    keys: Rc<RefCell<Vec<ListWindowKey>>>,
    sizes: Rc<RefCell<HashMap<ListWindowKey, f32>>>,
    scroll: ScrollHandle,
}

fn host(
    cx: &mut TestAppContext,
    count: usize,
    pitch: ListPitch,
    mode: Mode,
    configure: impl FnOnce(&mut Host),
) -> (&mut VisualTestContext, Setup) {
    let keys = Rc::new(RefCell::new(
        (0..count).map(ListWindowKey::from).collect::<Vec<_>>(),
    ));
    let sizes: Rc<RefCell<HashMap<ListWindowKey, f32>>> = Rc::default();
    let scroll = ScrollHandle::new();
    let renders: Rc<std::cell::Cell<usize>> = Rc::default();
    let renders_in = renders.clone();
    let setup = Setup {
        keys: keys.clone(),
        sizes: sizes.clone(),
        scroll: scroll.clone(),
        renders,
    };
    let (_, cx) = cx.add_window_view(move |_, _| {
        let mut host = Host {
            keys,
            sizes,
            pitch,
            mode,
            anchor: ListAnchor::FirstVisible,
            can_window: true,
            axis: WindowedListAxis::Block,
            rtl: false,
            insets: (0., 0.),
            scroll,
            renders: renders_in,
        };
        configure(&mut host);
        host
    });
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (cx, setup)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn mounted(cx: &mut VisualTestContext, count: usize) -> Vec<usize> {
    (0..count)
        .filter(|i| {
            cx.debug_bounds(Box::leak(format!("row-{i}").into_boxed_str()))
                .is_some()
        })
        .collect()
}

fn scroll_to(cx: &mut VisualTestContext, s: &Setup, y: f32) {
    s.scroll.set_offset(point(px(0.), px(-y)));
    settle(cx);
}

#[gpui::test]
fn mounts_only_the_visible_rows_plus_four_per_side_in_a_10k_list(cx: &mut TestAppContext) {
    let (cx, s) = host(
        cx,
        10_000,
        ListPitch::Fixed(px(20.)),
        Mode::Container,
        |_| {},
    );
    // 200px of 20px rows: ten rows, then four past the end.
    assert_eq!(mounted(cx, 40), (0..14).collect::<Vec<_>>());
    scroll_to(cx, &s, 2000.);
    let now = mounted(cx, 200);
    assert_eq!(
        now,
        (96..114).collect::<Vec<_>>(),
        "rows 100-109 show; four each side"
    );
    // The list is as tall as every row.
    let list = cx.debug_bounds("row-100").unwrap();
    assert_eq!(f32::from(list.size.height), 20.);
}

#[gpui::test]
fn keeps_the_focused_row_mounted_after_it_scrolls_out(cx: &mut TestAppContext) {
    let (cx, s) = host(
        cx,
        10_000,
        ListPitch::Fixed(px(20.)),
        Mode::Container,
        |_| {},
    );
    let row = cx.debug_bounds("row-2").unwrap();
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    settle(cx);
    scroll_to(cx, &s, 4000.);
    let now = mounted(cx, 400);
    assert!(now.contains(&2), "the focused row stays: {now:?}");
    assert!(now.contains(&200) && !now.contains(&3));
}

#[gpui::test]
fn keeps_the_first_visible_key_in_place_when_rows_arrive_above(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, 1000, ListPitch::Fixed(px(20.)), Mode::Container, |_| {});
    scroll_to(cx, &s, 405.);
    let before = cx.debug_bounds("row-20").unwrap().origin.y;
    {
        let mut keys = s.keys.borrow_mut();
        for i in 0..5 {
            keys.insert(0, ListWindowKey::from(format!("new-{i}")));
        }
    }
    settle(cx);
    let after = cx.debug_bounds("row-20").unwrap().origin.y;
    assert_eq!(before, after, "row 20 holds its place");
    assert_eq!(f32::from(s.scroll.offset().y), -505.);
}

#[gpui::test]
fn renders_every_row_when_it_may_not_window(cx: &mut TestAppContext) {
    let (cx, _) = host(cx, 300, ListPitch::Fixed(px(20.)), Mode::Container, |h| {
        h.can_window = false
    });
    assert_eq!(mounted(cx, 300).len(), 300);
}

#[gpui::test]
fn fr3_rows_of_varying_size_are_measured(cx: &mut TestAppContext) {
    let (cx, s) = host(
        cx,
        1000,
        ListPitch::Estimated(px(20.)),
        Mode::Container,
        |_| {},
    );
    // Every row measures 40px: the estimate was half that.
    let (_, cx) = (0, cx);
    {
        let mut sizes = s.sizes.borrow_mut();
        for i in 0..1000usize {
            sizes.insert(ListWindowKey::from(i), 40.);
        }
    }
    settle(cx);
    scroll_to(cx, &s, 4000.);
    // Rows not yet measured count at the estimate; whatever the window
    // mounts covers the viewport, and each mounted row has its own size.
    let now = mounted(cx, 1000);
    let view = cx.debug_bounds("scroller-view").unwrap();
    let bounds: Vec<_> = now
        .iter()
        .map(|i| {
            cx.debug_bounds(Box::leak(format!("row-{i}").into_boxed_str()))
                .unwrap()
        })
        .collect();
    assert!(bounds.iter().all(|b| f32::from(b.size.height) == 40.));
    let top = bounds
        .iter()
        .map(|b| b.top())
        .fold(view.bottom(), |a, b| a.min(b));
    let bottom = bounds
        .iter()
        .map(|b| b.bottom())
        .fold(view.top(), |a, b| a.max(b));
    assert!(
        top <= view.top() && bottom >= view.bottom(),
        "the mounted rows cover the viewport: {now:?}"
    );
    assert!(now.len() <= 5 + 1 + 8 + 1, "{now:?}");
}

#[gpui::test]
fn fr2_a_resize_above_keeps_the_first_visible_row(cx: &mut TestAppContext) {
    let (cx, s) = host(
        cx,
        1000,
        ListPitch::Estimated(px(20.)),
        Mode::Container,
        |_| {},
    );
    scroll_to(cx, &s, 400.);
    let before = cx.debug_bounds("row-20").unwrap().origin.y;
    // Rows 0-9 grow, all above the viewport.
    {
        let mut sizes = s.sizes.borrow_mut();
        for i in 0..10usize {
            sizes.insert(ListWindowKey::from(i), 30.);
        }
    }
    settle(cx);
    assert_eq!(cx.debug_bounds("row-20").unwrap().origin.y, before);
}

#[gpui::test]
fn inv4_windows_on_a_scrolling_page(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, 10_000, ListPitch::Fixed(px(20.)), Mode::Page, |_| {});
    // The list starts 100px down: rows 0-4 show, then four more.
    assert_eq!(mounted(cx, 40), (0..9).collect::<Vec<_>>());
    scroll_to(cx, &s, 2100.);
    let now = mounted(cx, 200);
    assert_eq!(now, (96..114).collect::<Vec<_>>());
    // The page's host lets it hold its anchor too.
    let before = cx.debug_bounds("row-100").unwrap().origin.y;
    s.keys.borrow_mut().insert(0, ListWindowKey::from("new"));
    settle(cx);
    assert_eq!(cx.debug_bounds("row-100").unwrap().origin.y, before);
}

#[gpui::test]
fn fr1_the_end_anchor_follows_new_rows(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, 100, ListPitch::Fixed(px(20.)), Mode::Container, |h| {
        h.anchor = ListAnchor::End
    });
    scroll_to(cx, &s, 1800.);
    assert!(mounted(cx, 100).contains(&99));
    s.keys
        .borrow_mut()
        .extend((100..103usize).map(ListWindowKey::from));
    settle(cx);
    assert_eq!(f32::from(s.scroll.offset().y), -1860.);
    assert!(mounted(cx, 103).contains(&102), "the new last row shows");
}

#[gpui::test]
fn fr1_a_given_item_opens_in_view(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, 1000, ListPitch::Fixed(px(20.)), Mode::Container, |h| {
        h.anchor = ListAnchor::Item(ListWindowKey::from(500usize));
    });
    assert_eq!(f32::from(s.scroll.offset().y), -10_000.);
    assert!(mounted(cx, 600).contains(&500));
}

#[gpui::test]
fn a_focused_row_that_leaves_hands_focus_to_the_list(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, 100, ListPitch::Fixed(px(20.)), Mode::Container, |_| {});
    let row = cx.debug_bounds("row-3").unwrap();
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    settle(cx);
    s.keys.borrow_mut().remove(3);
    settle(cx);
    let focused: Option<FocusHandle> = cx.update(|window, cx| window.focused(cx));
    assert!(focused.is_some(), "focus stays in the list");
}

#[gpui::test]
fn inline_items_window_from_the_inline_start(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, 500, ListPitch::Fixed(px(20.)), Mode::Container, |h| {
        h.axis = WindowedListAxis::Inline
    });
    assert_eq!(mounted(cx, 40), (0..14).collect::<Vec<_>>());
    s.scroll.set_offset(point(px(-1000.), px(0.)));
    settle(cx);
    assert_eq!(mounted(cx, 100), (46..64).collect::<Vec<_>>());
}

#[gpui::test]
fn inline_items_in_right_to_left_run_from_the_right(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, 500, ListPitch::Fixed(px(20.)), Mode::Container, |h| {
        h.axis = WindowedListAxis::Inline;
        h.rtl = true;
    });
    // Unscrolled, the scroller shows the left: the inline end.
    assert_eq!(mounted(cx, 500), (486..500).collect::<Vec<_>>());
    // At the right edge it shows the inline start.
    s.scroll.set_offset(point(px(-9800.), px(0.)));
    settle(cx);
    assert_eq!(mounted(cx, 500), (0..14).collect::<Vec<_>>());
    let first = cx.debug_bounds("row-0").unwrap();
    let view = cx.debug_bounds("scroller-view").unwrap();
    assert_eq!(
        first.right(),
        view.right(),
        "item 0 sits at the inline start"
    );
}

#[gpui::test]
fn inline_insets_cover_the_viewport_edges(cx: &mut TestAppContext) {
    let (cx, s) = host(cx, 500, ListPitch::Fixed(px(20.)), Mode::Container, |h| {
        h.axis = WindowedListAxis::Inline;
        h.insets = (40., 40.);
    });
    s.scroll.set_offset(point(px(-1000.), px(0.)));
    settle(cx);
    // Items 52-57 show between the insets; four more each side.
    assert_eq!(mounted(cx, 500), (48..62).collect::<Vec<_>>());
}

#[gpui::test]
fn settles_without_redrawing(cx: &mut TestAppContext) {
    for anchor in [
        ListAnchor::FirstVisible,
        ListAnchor::Item(ListWindowKey::from(500usize)),
    ] {
        let (cx, s) = host(
            cx,
            1000,
            ListPitch::Estimated(px(48.)),
            Mode::Container,
            |h| h.anchor = anchor.clone(),
        );
        {
            let mut sizes = s.sizes.borrow_mut();
            for i in 0..1000usize {
                sizes.insert(ListWindowKey::from(i), 56.);
            }
        }
        settle(cx);
        settle(cx);
        // Once settled, each forced frame draws once: the list asks for none.
        let before = s.renders.get();
        settle(cx);
        assert_eq!(
            s.renders.get() - before,
            4,
            "{anchor:?} keeps asking for frames"
        );
        assert!(mounted(cx, 1000).len() <= 5 + 1 + 8, "{anchor:?}");
    }
}
