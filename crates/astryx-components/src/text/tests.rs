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
