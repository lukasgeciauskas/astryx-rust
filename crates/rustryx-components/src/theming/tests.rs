//! Theme and MediaTheme tests.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Hsla, IntoElement, ParentElement, RenderOnce, TestAppContext, div, hsla};
use rustryx_theme::{Appearance, ThemeMode};

use super::examples::seeded_theme;
use super::*;
use crate::segmented::tests::host;

type Seen = Rc<RefCell<Vec<(Appearance, Hsla)>>>;

/// Records the theme it draws with: its appearance and accent.
#[derive(IntoElement)]
struct Probe(Seen);

impl RenderOnce for Probe {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let theme = rustryx_theme::current(window, cx);
        self.0
            .borrow_mut()
            .push((theme.appearance, theme.tokens().color.accent));
        div()
    }
}

#[gpui::test]
fn a_theme_provider_draws_its_subtree_with_its_theme_and_mode(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let violet = seeded_theme("violet-test", "#5B3CC4");
    let expected = violet.tokens(Appearance::Dark).color.accent;
    let _cx = host(cx, {
        let seen = seen.clone();
        move || {
            div()
                .child(Probe(seen.clone()))
                .child(theme_provider(violet.clone(), Probe(seen.clone())).mode(ThemeMode::Dark))
                .into_any_element()
        }
    });
    let seen = seen.borrow();
    let (outer, inner) = (seen[seen.len() - 2], seen[seen.len() - 1]);
    assert_eq!(inner, (Appearance::Dark, expected));
    assert_ne!(outer.1, inner.1, "the app's theme keeps its own accent");
}

#[gpui::test]
fn system_mode_keeps_the_surrounding_appearance(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let violet = seeded_theme("violet-test", "#5B3CC4");
    let _cx = host(cx, {
        let seen = seen.clone();
        move || {
            div()
                .child(Probe(seen.clone()))
                .child(theme_provider(violet.clone(), Probe(seen.clone())))
                .into_any_element()
        }
    });
    let seen = seen.borrow();
    assert_eq!(seen[seen.len() - 2].0, seen[seen.len() - 1].0);
}

#[gpui::test]
fn media_theme_takes_the_surface_side(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let _cx = host(cx, {
        let seen = seen.clone();
        move || {
            div()
                .child(media_theme(MediaThemeMode::Dark, Probe(seen.clone())))
                .child(media_theme(MediaThemeMode::Light, Probe(seen.clone())))
                .child(media_theme(MediaThemeMode::Off, Probe(seen.clone())))
                // An unknown surface takes the fallback.
                .child(
                    media_theme(MediaThemeMode::Auto, Probe(seen.clone()))
                        .fallback(Appearance::Light),
                )
                .into_any_element()
        }
    });
    let seen = seen.borrow();
    let last: Vec<Appearance> = seen[seen.len() - 4..].iter().map(|(a, _)| *a).collect();
    assert_eq!(last[0], Appearance::Dark);
    assert_eq!(last[1], Appearance::Light);
    assert_eq!(last[3], Appearance::Light);
}

#[test]
fn auto_picks_the_side_that_reads_or_none() {
    let black = hsla(0., 0., 0., 1.);
    let white = hsla(0., 0., 1., 1.);
    let near_black_text = hsla(0., 0., 0.05, 1.);
    // A dark surface with dark page text: the light on-color reads, so dark side.
    assert_eq!(
        pick_media_mode(hsla(0., 0., 0.1, 1.), Some(near_black_text), white, black),
        Some(Appearance::Dark)
    );
    // A pale surface the page text already reads on: no media context.
    assert_eq!(
        pick_media_mode(hsla(0., 0., 0.9, 1.), Some(near_black_text), white, black),
        None
    );
    // Without the page text, a pale surface takes the light side.
    assert_eq!(
        pick_media_mode(hsla(0., 0., 0.9, 1.), None, white, black),
        Some(Appearance::Light)
    );
}

#[gpui::test]
fn a_themes_component_icons_reach_the_slots(cx: &mut TestAppContext) {
    use rustryx_theme::authoring::define::{
        DefineThemeInput, ThemeComponentIcon, ThemeComponentIconRequest, define_theme,
    };

    use crate::icon::{IconSource, TABLE_ROW_EXPAND, TABLE_SORT_INDICATOR, component_icon};

    let input = DefineThemeInput {
        component_icons: Some(
            [
                (TABLE_SORT_INDICATOR, ThemeComponentIcon::Hidden),
                (
                    TABLE_ROW_EXPAND,
                    ThemeComponentIcon::ByState(
                        [(
                            "expanded",
                            Some(ThemeComponentIconRequest {
                                icon: Some(Some("chevronUp".into())),
                                variant: None,
                                size: None,
                            }),
                        )]
                        .into(),
                    ),
                ),
            ]
            .into(),
        ),
        ..DefineThemeInput::new("slots")
    };
    let theme = define_theme(&input).unwrap();
    cx.update(|cx| {
        apply_component_icons(&theme, cx);
        let fallback = || IconSource::Named("chevronDown".into());
        assert_eq!(
            component_icon(TABLE_SORT_INDICATOR, "none", fallback(), cx),
            None
        );
        assert_eq!(
            component_icon(TABLE_ROW_EXPAND, "expanded", fallback(), cx),
            Some(IconSource::Named("chevronUp".into()))
        );
        // A state the theme does not list keeps the component's own icon.
        assert_eq!(
            component_icon(TABLE_ROW_EXPAND, "collapsed", fallback(), cx),
            Some(fallback())
        );
    });
}
