use astryx_i18n::Direction;
use gpui::{FlexDirection, Styled as _};

use super::*;

fn theme() -> astryx_theme::ResolvedTheme {
    astryx_theme::ResolvedTheme::new(
        std::sync::Arc::new(astryx_themes::neutral::theme()),
        astryx_theme::Appearance::Light,
        1280.0,
        astryx_theme::Pointer::Fine,
    )
}

#[test]
fn horizontal_stacks_reverse_in_rtl() {
    let theme = theme();
    let mut ltr = Stack::horizontal().root(&theme, Direction::Ltr);
    let mut rtl = Stack::horizontal().root(&theme, Direction::Rtl);
    assert_eq!(ltr.style().flex_direction, Some(FlexDirection::Row));
    assert_eq!(rtl.style().flex_direction, Some(FlexDirection::RowReverse));
}

#[test]
fn alignment_resolves_like_upstream() {
    use gpui::{AlignItems, JustifyContent};
    let theme = theme();
    // HStack: hAlign is the main axis.
    let mut el = Stack::horizontal()
        .h_align(StackAlign::Between)
        .v_align(StackAlign::Center)
        .root(&theme, Direction::Ltr);
    assert_eq!(
        el.style().justify_content,
        Some(JustifyContent::SpaceBetween)
    );
    assert_eq!(el.style().align_items, Some(AlignItems::Center));
    // VStack: hAlign is the cross axis, and start is the right in RTL.
    let mut el = Stack::vertical()
        .h_align(StackAlign::Start)
        .root(&theme, Direction::Rtl);
    assert_eq!(el.style().align_items, Some(AlignItems::FlexEnd));
    // hAlign wins over justify/align.
    let mut el = Stack::vertical()
        .align(StackAlign::End)
        .h_align(StackAlign::Center)
        .root(&theme, Direction::Ltr);
    assert_eq!(el.style().align_items, Some(AlignItems::Center));
}

#[test]
fn padding_resolves_per_edge_and_mirrors() {
    let theme = theme();
    let t = theme.tokens();
    let mut el = Stack::vertical()
        .padding(SpacingStep::S2)
        .padding_inline(SpacingStep::S4)
        .padding_inline_start(SpacingStep::S6)
        .root(&theme, Direction::Rtl);
    let padding = el.style().padding.clone();
    // Inline start (the right in RTL) is the most specific; end takes the
    // axis value; block takes `padding`.
    assert_eq!(padding.right, Some(t.spacing.s6.into()));
    assert_eq!(padding.left, Some(t.spacing.s4.into()));
    assert_eq!(padding.top, Some(t.spacing.s2.into()));
}

#[test]
fn center_axes() {
    use gpui::{AlignItems, JustifyContent};
    let theme = theme();
    let mut both = Center::new().root(&theme, Direction::Ltr);
    assert_eq!(both.style().justify_content, Some(JustifyContent::Center));
    assert_eq!(both.style().align_items, Some(AlignItems::Center));
    let mut vertical = Center::new()
        .axis(CenterAxis::Vertical)
        .root(&theme, Direction::Ltr);
    assert_eq!(vertical.style().justify_content, None);
    assert_eq!(vertical.style().align_items, Some(AlignItems::Center));
}

#[test]
fn every_layout_doc_maps_its_upstream_props() {
    for doc in [
        STACK_DOC,
        CENTER_DOC,
        GRID_DOC,
        DIVIDER_DOC,
        ASPECT_RATIO_DOC,
        VISUALLY_HIDDEN_DOC,
    ] {
        assert!(!doc.props.is_empty(), "{} has no props", doc.name);
    }
}
