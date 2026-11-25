//! Text and Heading styling against upstream's rules (`text.stylex.ts`).

use std::borrow::Cow;
use std::sync::Arc;

use astryx_theme::{
    Appearance, ComponentRule, Declaration, Pointer, ResolvedTheme, StyleProp, Theme,
};
use gpui::{DefiniteLength, Hsla, Styled};

use super::*;

fn neutral() -> ResolvedTheme {
    ResolvedTheme::new(
        Arc::new(astryx_themes::neutral::theme()),
        Appearance::Light,
        1280.0,
        Pointer::Fine,
    )
}

fn red() -> Hsla {
    gpui::rgba(0xff0000ff).into()
}

#[test]
fn supporting_text_defaults_to_secondary_color() {
    let theme = neutral();
    let mut el = Text::new("x").text_type(TextType::Supporting).root(&theme);
    assert_eq!(
        el.style().text.color,
        Some(theme.tokens().color.text_secondary)
    );
    let mut el = Text::new("x").root(&theme);
    assert_eq!(
        el.style().text.color,
        Some(theme.tokens().color.text_primary)
    );
}

#[test]
fn explicit_color_and_weight_win() {
    let theme = neutral();
    let t = theme.tokens();
    let mut el = Text::new("x")
        .text_type(TextType::Supporting)
        .color(TextColor::Accent)
        .weight(TextWeight::Bold)
        .root(&theme);
    assert_eq!(el.style().text.color, Some(t.color.text_accent));
    assert_eq!(el.style().text.font_weight, Some(t.font.weight.bold));
}

#[test]
fn type_sets_size_and_leading_and_code_its_family() {
    let theme = neutral();
    let t = theme.tokens();
    let mut el = Text::new("x").text_type(TextType::Code).root(&theme);
    let style = el.style();
    assert_eq!(style.text.font_size, Some(t.text.code.size));
    assert_eq!(
        style.text.line_height,
        Some(DefiniteLength::Fraction(t.text.code.leading))
    );
    let family = style.text.font_family.clone().map(|f| f.to_string());
    let code_first = t.font.family.code.named().next().map(str::to_string);
    assert_eq!(family, code_first);
}

#[test]
fn explicit_size_keeps_the_type_weight() {
    let theme = neutral();
    let t = theme.tokens();
    let mut el = Text::new("x")
        .text_type(TextType::Label)
        .size(TextSize::Xl2)
        .root(&theme);
    assert_eq!(el.style().text.font_size, Some(t.font.size.s2xl));
    assert_eq!(el.style().text.font_weight, Some(t.text.label.weight));
}

#[test]
fn max_lines_clamps_and_one_line_truncates() {
    let theme = neutral();
    let mut el = Text::new("x").max_lines(2).root(&theme);
    assert_eq!(el.style().text.line_clamp, Some(2));
    let mut el = Text::new("x").max_lines(1).root(&theme);
    assert!(el.style().text.text_overflow.is_some());
}

#[test]
fn theme_rules_apply_after_defaults_and_caller_style_last() {
    let rule = ComponentRule {
        target: Cow::Borrowed("text"),
        when: Cow::Borrowed(&[(Cow::Borrowed("type"), Cow::Borrowed("label"))]),
        state: None,
        light: Cow::Owned(vec![Declaration::Style(StyleProp::TextColor(red()))]),
        dark: Cow::Owned(vec![]),
    };
    let theme = ResolvedTheme::new(
        Arc::new(Theme::default().with_rules([rule])),
        Appearance::Light,
        1280.0,
        Pointer::Fine,
    );
    let mut el = Text::new("x").text_type(TextType::Label).root(&theme);
    assert_eq!(el.style().text.color, Some(red()));
    let mut body = Text::new("x").root(&theme);
    assert_ne!(body.style().text.color, Some(red()));

    let blue: Hsla = gpui::rgba(0x0000ffff).into();
    let mut el = Text::new("x")
        .text_type(TextType::Label)
        .text_color(blue)
        .root(&theme);
    assert_eq!(
        el.style().text.color,
        Some(blue),
        "caller style wins over theme rules"
    );
}

#[test]
fn heading_uses_level_style_and_heading_family() {
    let theme = neutral();
    let t = theme.tokens();
    let mut el = Heading::new(HeadingLevel::H2, "x").root(&theme);
    let style = el.style();
    assert_eq!(style.text.font_size, Some(t.text.heading_2.size));
    assert_eq!(style.text.font_weight, Some(t.text.heading_2.weight));
    let family = style.text.font_family.clone().map(|f| f.to_string());
    assert_eq!(
        family,
        t.font.family.heading.named().next().map(str::to_string)
    );

    let mut el = Heading::new(HeadingLevel::H2, "x")
        .display_type(DisplayType::Display1)
        .root(&theme);
    assert_eq!(el.style().text.font_size, Some(t.text.display_1.size));
}

// ----------------------------------------------------------------------------
// Inline flow

mod inline_flow {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        AnyElement, InteractiveElement as _, IntoElement, Modifiers, ParentElement, Pixels, Styled,
        TestAppContext, VisualTestContext, div, point, px,
    };

    use crate::link::Link;
    use crate::segmented::tests::{host, press};
    use crate::text::{Text, TextWeight};

    fn height(cx: &mut VisualTestContext, id: &'static str) -> Pixels {
        cx.debug_bounds(id)
            .expect("the paragraph is drawn")
            .size
            .height
    }

    fn paragraph(width: f32, clicks: Rc<RefCell<usize>>) -> impl Fn() -> AnyElement + 'static {
        move || {
            let clicks = clicks.clone();
            div()
                .w(px(width))
                .debug_selector(|| "para".into())
                .child(
                    Text::new("")
                        .child(
                            Link::new("docs", "the documentation")
                                .on_click(move |_, _, _| *clicks.borrow_mut() += 1),
                        )
                        .child(" explains every ")
                        .child(Text::new("option").weight(TextWeight::Semibold))
                        .child(" in detail."),
                )
                .into_any_element()
        }
    }

    #[gpui::test]
    fn spans_and_links_share_one_line_when_it_fits(cx: &mut TestAppContext) {
        let cx = host(cx, paragraph(2000., Rc::default()));
        let one = height(cx, "para");
        assert!(one > px(0.));
        assert!(one < px(40.), "one line, not {one:?}");
    }

    #[gpui::test]
    fn capsize_trims_the_box_to_cap_height_and_baseline(cx: &mut TestAppContext) {
        let cx = host(cx, || {
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .debug_selector(|| "plain".into())
                        .child(Text::new("Revenue")),
                )
                .child(
                    div()
                        .debug_selector(|| "trimmed".into())
                        .child(Text::new("Revenue").capsize(true)),
                )
                .into_any_element()
        });
        let (plain, trimmed) = (height(cx, "plain"), height(cx, "trimmed"));
        assert!(trimmed < plain, "{trimmed:?} < {plain:?}");
        // What is left is about the cap height: well over a third of the line.
        assert!(trimmed > plain * 0.35, "{trimmed:?} of {plain:?}");
    }

    #[gpui::test]
    fn a_narrow_paragraph_wraps_through_its_link(cx: &mut TestAppContext) {
        let cx = host(cx, paragraph(90., Rc::default()));
        assert!(height(cx, "para") > px(60.), "several lines");
    }

    #[gpui::test]
    fn a_link_in_a_paragraph_takes_focus_and_activates(cx: &mut TestAppContext) {
        let clicks = Rc::new(RefCell::new(0));
        let cx = host(cx, paragraph(2000., clicks.clone()));
        press(cx, "tab");
        press(cx, "enter");
        assert_eq!(*clicks.borrow(), 1, "the keyboard reaches the link");
        // The host pads by 20px: the link's text starts there.
        cx.simulate_click(point(px(30.), px(28.)), Modifiers::none());
        assert_eq!(*clicks.borrow(), 2, "so does the pointer");
        cx.simulate_click(point(px(1500.), px(28.)), Modifiers::none());
        assert_eq!(*clicks.borrow(), 2, "but not past the text");
    }
}
