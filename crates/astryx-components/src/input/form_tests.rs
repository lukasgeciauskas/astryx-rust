//! FormLayout tests, numbered by the upstream spec's FR items.

use super::*;
use crate::field::{FormDirection, FormLayout};
use crate::field::{Optionality, resolved};

#[test]
fn fr5_only_the_exception_shows_a_marker() {
    // No default: each field shows its own marker.
    let r = resolved(None, true, false);
    assert!(r.required && r.shows_required && !r.shows_optional);
    // Optional by default: no "Optional", "Required" still shows.
    let r = resolved(Some(Optionality::Optional), false, true);
    assert!(!r.required && !r.shows_optional);
    let r = resolved(Some(Optionality::Optional), true, false);
    assert!(r.required && r.shows_required);
    // Required by default: unmarked fields are required, unmarked.
    let r = resolved(Some(Optionality::Required), false, false);
    assert!(r.required && !r.shows_required && !r.shows_optional);
    let r = resolved(Some(Optionality::Required), false, true);
    assert!(!r.required && r.shows_optional);
    // Optional wins over required.
    let r = resolved(None, true, true);
    assert!(!r.required && r.shows_optional && !r.shows_required);
}

struct FormHost(Rc<dyn Fn() -> FormLayout>);

impl Render for FormHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(20.)).w(px(600.)).child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> FormLayout + 'static,
) -> &mut VisualTestContext {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (_, cx) = cx.add_window_view(move |_, _| FormHost(Rc::new(build)));
    cx.run_until_parked();
    cx
}

fn control(cx: &mut VisualTestContext, id: &'static str) -> gpui::Bounds<gpui::Pixels> {
    cx.debug_bounds(id)
        .unwrap_or_else(|| panic!("no control {id}"))
}

#[gpui::test]
fn fr2_horizontal_fields_share_the_row_evenly(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        FormLayout::new("f")
            .direction(FormDirection::Horizontal)
            .child(TextInput::new("a", "First name", ""))
            .child(TextInput::new("b", "Last name", ""))
    });
    let (a, b) = (control(cx, "a-control"), control(cx, "b-control"));
    assert_eq!(a.origin.y, b.origin.y);
    assert!(b.origin.x > a.origin.x + a.size.width);
    assert!((a.size.width - b.size.width).abs() < px(1.));
}

#[gpui::test]
fn fr1_vertical_fields_stack(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        FormLayout::new("f")
            .child(TextInput::new("a", "First name", ""))
            .child(TextInput::new("b", "Last name", ""))
    });
    let (a, b) = (control(cx, "a-control"), control(cx, "b-control"));
    assert_eq!(a.origin.x, b.origin.x);
    assert!(b.origin.y > a.origin.y + a.size.height);
}

#[gpui::test]
fn fr3_labels_share_one_column_beside_their_controls(cx: &mut TestAppContext) {
    let cx = host(cx, || {
        FormLayout::new("f")
            .direction(FormDirection::HorizontalLabels)
            .child(TextInput::new("a", "Name", ""))
            .child(TextInput::new("b", "A much longer label", ""))
    });
    let (a, b) = (control(cx, "a-control"), control(cx, "b-control"));
    // Both controls start after the widest label, on their label's row.
    assert_eq!(a.origin.x, b.origin.x);
    assert!(a.origin.x > px(120.), "past the longer label");
    assert!(b.origin.y > a.origin.y + a.size.height);
}
