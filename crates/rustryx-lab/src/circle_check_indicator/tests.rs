use gpui::{TestAppContext, transparent_black};

use super::*;

#[test]
fn the_geometry_is_uikits_table() {
    // circleCheck.test.tsx: md is the Large text size, sm the xSmall one, with the two
    // derived sm values.
    let large = CircleCheckGeometry::LARGE;
    assert_eq!(
        (large.diameter, large.stroke, large.check_size),
        (21.8, 1.54, 9.)
    );
    assert_eq!((large.check_stroke, large.check_offset_y), (1.35, 1.));
    let small = CircleCheckGeometry::X_SMALL;
    assert_eq!(
        (small.diameter, small.stroke, small.check_size),
        (18., 1.27, 8.)
    );
    let round2 = |n: f32| (n * 100.).round() / 100.;
    assert_eq!(
        small.check_stroke,
        round2(large.check_stroke * small.check_size / large.check_size)
    );
    assert_eq!(
        small.check_offset_y,
        round2(large.check_offset_y * small.diameter / large.diameter)
    );
    assert_eq!(CircleCheckGeometry::of(ChoiceSize::Sm), small);
    assert_eq!(circle_diameter(ChoiceSize::Md), px(21.8));
}

fn tokens() -> rustryx_theme::Tokens {
    rustryx_theme::ResolvedTheme::new(
        std::sync::Arc::new(rustryx_themes::neutral::theme()),
        rustryx_theme::Appearance::Light,
        1280.,
        rustryx_theme::Pointer::Fine,
    )
    .tokens()
    .clone()
}

#[test]
fn the_empty_circle_is_a_ring_and_the_filled_one_has_none() {
    let t = tokens();
    let geometry = CircleCheckGeometry::LARGE;
    let empty = Look::of(IndicatorState::Unchecked, geometry, &t);
    assert_eq!(empty.fill, transparent_black());
    assert_eq!(
        (empty.ring, empty.ring_width),
        (t.color.border_emphasized, 1.54)
    );
    for state in [IndicatorState::Checked, IndicatorState::Indeterminate] {
        let filled = Look::of(state, geometry, &t);
        assert_eq!(filled.fill, t.color.accent);
        assert_eq!(filled.ring_width, 0.);
    }
}

#[test]
fn a_fade_mixes_the_fill_and_shrinks_the_ring() {
    let t = tokens();
    let geometry = CircleCheckGeometry::LARGE;
    let (empty, filled) = (
        Look::of(IndicatorState::Unchecked, geometry, &t),
        Look::of(IndicatorState::Checked, geometry, &t),
    );
    assert_eq!(empty.mix(filled, 0.), empty);
    let mid = empty.mix(filled, 0.5);
    assert!((mid.ring_width - 0.77).abs() < 1e-6);
    // Premultiplied, as CSS mixes a transparent colour: the accent's hue at half alpha.
    let (a, m) = (t.color.accent.to_rgb(), mid.fill.to_rgb());
    assert!((m.a - 0.5).abs() < 1e-4 && (m.r - a.r).abs() < 1e-4 && (m.b - a.b).abs() < 1e-4);
    assert_eq!(empty.mix(filled, 1.), filled);
}

struct Host(IndicatorState, Option<&'static str>);

impl gpui::Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        let mut circle = CircleCheckIndicator::new(self.0);
        if let Some(child) = self.1 {
            circle = circle.child(div().debug_selector(move || child.into()).size(px(4.)));
        }
        div().child(circle)
    }
}

fn drawn(
    cx: &mut TestAppContext,
    state: IndicatorState,
    child: Option<&'static str>,
) -> (bool, bool, bool) {
    cx.update(rustryx_theme::init);
    let (_, cx) = cx.add_window_view(move |_, _| Host(state, child));
    cx.run_until_parked();
    (
        cx.debug_bounds("circle-check-indicator").is_some(),
        cx.debug_bounds("circle-check-indicator-check").is_some(),
        cx.debug_bounds("circle-check-indicator-dash").is_some(),
    )
}

#[gpui::test]
fn an_unchecked_circle_still_draws_its_ring(cx: &mut TestAppContext) {
    assert_eq!(
        drawn(cx, IndicatorState::Unchecked, None),
        (true, false, false)
    );
}

#[gpui::test]
fn a_checked_circle_draws_the_check(cx: &mut TestAppContext) {
    assert_eq!(
        drawn(cx, IndicatorState::Checked, None),
        (true, true, false)
    );
}

#[gpui::test]
fn a_partial_circle_draws_the_bar(cx: &mut TestAppContext) {
    assert_eq!(
        drawn(cx, IndicatorState::Indeterminate, None),
        (true, false, true)
    );
}

#[gpui::test]
fn children_replace_the_mark(cx: &mut TestAppContext) {
    assert_eq!(
        drawn(cx, IndicatorState::Checked, Some("busy")),
        (true, false, false)
    );
}

#[gpui::test]
fn the_circle_is_the_size_of_its_table(cx: &mut TestAppContext) {
    cx.update(rustryx_theme::init);
    let (_, cx) = cx.add_window_view(|_, _| Host(IndicatorState::Checked, None));
    cx.run_until_parked();
    let bounds = cx.debug_bounds("circle-check-indicator").unwrap();
    // Layout rounds boxes to whole pixels.
    assert!(
        (f32::from(bounds.size.width) - 21.8).abs() <= 0.5,
        "{:?}",
        bounds.size
    );
    let check = cx.debug_bounds("circle-check-indicator-check").unwrap();
    assert!(
        (f32::from(check.size.width) - 9.).abs() < 0.5,
        "{:?}",
        check.size
    );
}

// ---------------------------------------------------------------------------
// Upstream "CircleCheckIndicator mapped to `checkbox` by a theme"

type Drawn = std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>;

/// The theme mapping `checkbox` to the circle, recording each state it draws.
fn circle_theme(cx: &mut gpui::App) -> Drawn {
    use rustryx_components::indicator::{CHECKBOX, set_theme_indicator};
    let drawn = Drawn::default();
    let log = drawn.clone();
    set_theme_indicator(
        CHECKBOX,
        move |request, _, _| {
            log.borrow_mut().push(request.state);
            CircleCheckIndicator::from_request(request).into_any_element()
        },
        cx,
    );
    drawn
}

struct Controlled(std::rc::Rc<std::cell::Cell<bool>>);

impl gpui::Render for Controlled {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let (value, view) = (self.0.clone(), cx.entity());
        div().p(px(20.)).child(
            rustryx_components::choice::CheckboxInput::new("photo", "Select photo", value.get())
                .on_change(move |next, _, cx| {
                    value.set(next);
                    view.update(cx, |_, cx| cx.notify());
                }),
        )
    }
}

#[gpui::test]
fn mapped_to_checkbox_it_draws_the_circle_in_a_checkbox_input_which_keeps_the_checkbox_semantics(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let drawn = cx.update(circle_theme);
    let value = std::rc::Rc::new(std::cell::Cell::new(false));
    let (_, cx) = cx.add_window_view({
        let value = value.clone();
        move |_, _| Controlled(value)
    });
    cx.run_until_parked();
    let circle = cx
        .debug_bounds("circle-check-indicator")
        .expect("the circle draws");
    assert_eq!(drawn.borrow().last(), Some(&"unchecked"));
    assert!(cx.debug_bounds("circle-check-indicator-check").is_none());
    cx.simulate_click(circle.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(value.get(), "the input toggles");
    assert_eq!(drawn.borrow().last(), Some(&"checked"));
    assert!(cx.debug_bounds("circle-check-indicator-check").is_some());
}

struct Photos;

impl gpui::Render for Photos {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        use rustryx_components::choice::{CheckboxList, CheckboxListItem};
        div().w(px(400.)).child(
            CheckboxList::new("photos", "Photos")
                .value(["a"])
                .on_change(|_, _, _| {})
                .item(CheckboxListItem::new("Beach").value("a"))
                .item(CheckboxListItem::new("Mountains").value("b")),
        )
    }
}

#[gpui::test]
fn mapped_to_checkbox_it_draws_one_circle_per_checkbox_list_row(cx: &mut TestAppContext) {
    cx.update(|cx| {
        rustryx_theme::init(cx);
        rustryx_primitives::focus::init(cx);
    });
    let drawn = cx.update(circle_theme);
    let (_, cx) = cx.add_window_view(|_, _| Photos);
    cx.run_until_parked();
    drawn.borrow_mut().clear();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(*drawn.borrow(), ["checked", "unchecked"]);
}
