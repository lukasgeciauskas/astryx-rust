//! Graduated customization: app rules, token values and subtree token scopes.

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, AvailableSpace, Hsla, IntoElement, RenderOnce, TestAppContext, Window, div, point,
    prelude::*, px, size,
};

use crate::*;

fn red() -> Hsla {
    gpui::rgba(0xff0000ff).into()
}

#[test]
fn app_rules_apply_after_theme_rules() {
    let rule = ComponentRule {
        target: Cow::Borrowed("button"),
        when: Cow::Borrowed(&[(Cow::Borrowed("variant"), Cow::Borrowed("primary"))]),
        state: None,
        light: Cow::Owned(vec![Declaration::Style(StyleProp::Background(red()))]),
        dark: Cow::Owned(vec![]),
    };
    let theme = Arc::new(Theme::default().with_rules([rule]));
    let resolved = ResolvedTheme::new(theme, Appearance::Light, 1280.0, Pointer::Fine);
    let mut style = gpui::StyleRefinement::default();
    resolved.refine(&mut style, "button", &[("variant", "primary")], None);
    assert_eq!(style.background, Some(gpui::Fill::from(red())));

    let mut other = gpui::StyleRefinement::default();
    resolved.refine(&mut other, "button", &[("variant", "secondary")], None);
    assert_eq!(other.background, None);
}

#[test]
fn token_values_set_one_or_both_modes() {
    let theme = Theme::default()
        .with_token_values(
            Some(Appearance::Dark),
            [("--color-accent", TokenValue::Color(red()))],
        )
        .unwrap();
    assert_eq!(theme.dark.color.accent, red());
    assert_ne!(theme.light.color.accent, red());
    assert!(
        Theme::default()
            .with_token_values(None, [("--color-nope", TokenValue::Color(red()))])
            .is_err()
    );
}

#[derive(IntoElement)]
struct AccentProbe(Rc<RefCell<Vec<Hsla>>>);

impl RenderOnce for AccentProbe {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.0
            .borrow_mut()
            .push(current(window, cx).tokens().color.accent);
        div().size(px(4.))
    }
}

#[gpui::test]
fn with_tokens_scopes_a_subtree(cx: &mut TestAppContext) {
    cx.update(init);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let cx = cx.add_empty_window();
    cx.draw(
        point(px(0.), px(0.)),
        size(
            AvailableSpace::Definite(px(100.)),
            AvailableSpace::Definite(px(100.)),
        ),
        move |window, cx| {
            div()
                .child(AccentProbe(s.clone()))
                .child(with_tokens(
                    [("--color-accent", TokenValue::Color(red()))],
                    AccentProbe(s.clone()),
                    window,
                    cx,
                ))
                .child(AccentProbe(s))
        },
    );
    let seen = seen.borrow();
    assert_eq!(seen.len(), 3);
    assert_ne!(seen[0], red());
    assert_eq!(seen[1], red());
    assert_eq!(seen[2], seen[0]);
}
