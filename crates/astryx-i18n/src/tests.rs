use gpui::{AppContext as _, TestAppContext, div};

use super::*;

#[test]
fn english_is_the_last_fallback() {
    let en = I18n::new("en");
    assert_eq!(en.t("@astryx.pagination.next"), "Go to next page");
    let de = I18n::new("de-DE");
    assert_eq!(de.t("@astryx.pagination.label"), "Seitennummerierung");
    // A key German lacks falls back to English.
    let english_only = catalog::KEYS
        .iter()
        .find(|key| catalog::shipped("de-DE", key).is_none())
        .expect("translations cover a subset of keys");
    assert_eq!(de.message(english_only), en.message(english_only));
}

#[test]
fn missing_keys_render_as_the_key() {
    assert_eq!(english().t("@astryx.nope"), "@astryx.nope");
}

#[test]
fn shipped_catalogs_match_full_tags_along_the_chain() {
    // `de-DE-u-nu-latn` walks to `de-DE`, which is shipped.
    assert_eq!(
        I18n::new("de-DE-u-nu-latn").t("@astryx.pagination.label"),
        "Seitennummerierung"
    );
    // Plain `de` has no `de` catalog upstream either: English.
    assert_eq!(I18n::new("de").t("@astryx.pagination.label"), "Pagination");
}

#[test]
fn overrides_beat_app_catalogs_which_beat_shipped_ones() {
    let fr = I18n::new("fr-FR")
        .messages("fr-FR", [("@astryx.pagination.next", "Suivante (app)")])
        .messages("fr", [("@astryx.pagination.previous", "Précédente (app)")]);
    assert_eq!(fr.t("@astryx.pagination.next"), "Suivante (app)");
    // The shipped `fr-FR` entry is more specific than the app's `fr` one.
    assert_ne!(fr.t("@astryx.pagination.previous"), "Précédente (app)");
    let fr = fr.overrides("fr", [("@astryx.pagination.next", "Suivant")]);
    assert_eq!(fr.t("@astryx.pagination.next"), "Suivant");
}

#[test]
fn formats_plurals_and_numbers_per_locale() {
    let en = I18n::new("en");
    assert_eq!(
        en.format("@astryx.pagination.previousBy", &[("step", 3.into())]),
        "Go back 3 pages"
    );
    assert_eq!(
        en.format("@astryx.pagination.previousBy", &[("step", 1.into())]),
        "Go back 1 page"
    );
    assert_eq!(
        en.format(
            "@astryx.fileInput.fileSize.kilobytes",
            &[("size", 1536.04.into())]
        ),
        "1536.0 KB"
    );
    let de = I18n::new("de-DE");
    assert_eq!(de.number(1234.5), "1.234,5");
}

#[test]
fn every_shipped_message_parses() {
    for key in catalog::KEYS {
        let english = catalog::english(key).unwrap();
        assert!(Message::parse(english).is_ok(), "en {key}: {english}");
        for tag in catalog::shipped_locales() {
            if let Some(message) = catalog::shipped(tag, key) {
                assert!(Message::parse(message).is_ok(), "{tag} {key}: {message}");
            }
        }
    }
}

#[test]
fn direction_comes_from_the_locale_unless_forced() {
    assert_eq!(I18n::new("ar-SA").direction(), Direction::Rtl);
    assert_eq!(
        I18n::new("en").dir(Direction::Rtl).direction(),
        Direction::Rtl
    );
    assert_eq!(
        I18n::new("he-IL").dir(Direction::Ltr).direction(),
        Direction::Ltr
    );
}

#[gpui::test]
fn providers_scope_the_locale(cx: &mut TestAppContext) {
    use std::cell::RefCell;
    use std::rc::Rc;

    let seen = Rc::new(RefCell::new(Vec::new()));
    cx.update(|cx| set_i18n(I18n::new("fr-FR"), cx));

    #[derive(gpui::IntoElement)]
    struct Probe(Rc<RefCell<Vec<(String, Direction)>>>);

    impl gpui::RenderOnce for Probe {
        fn render(self, _: &mut gpui::Window, cx: &mut App) -> impl IntoElement {
            let i18n = current(cx);
            self.0
                .borrow_mut()
                .push((i18n.locale().to_string(), i18n.direction()));
            div()
        }
    }

    let probe_seen = seen.clone();
    let window = cx.add_window(move |_, _| Root(probe_seen));

    struct Root(Rc<RefCell<Vec<(String, Direction)>>>);

    impl gpui::Render for Root {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            use gpui::ParentElement as _;
            div()
                .child(Probe(self.0.clone()))
                .child(provider(I18n::new("ar-SA"), Probe(self.0.clone())))
        }
    }

    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
    })
    .unwrap();
    let seen = seen.borrow();
    assert_eq!(
        seen[..2],
        [
            ("fr-FR".to_string(), Direction::Ltr),
            ("ar-SA".to_string(), Direction::Rtl)
        ]
    );
}

#[test]
fn logical_styles_mirror_in_rtl() {
    use gpui::{Styled as _, px};

    let ltr = div()
        .padding_start(Direction::Ltr, px(4.))
        .inset_end(Direction::Ltr, px(2.));
    let rtl = div()
        .padding_start(Direction::Rtl, px(4.))
        .inset_end(Direction::Rtl, px(2.));
    let mut ltr = ltr;
    let mut rtl = rtl;
    assert_eq!(ltr.style().padding.left, Some(px(4.).into()));
    assert_eq!(ltr.style().inset.right, Some(px(2.).into()));
    assert_eq!(rtl.style().padding.right, Some(px(4.).into()));
    assert_eq!(rtl.style().inset.left, Some(px(2.).into()));
    let mut row = div().flex_row_logical(Direction::Rtl);
    assert_eq!(
        row.style().flex_direction,
        Some(gpui::FlexDirection::RowReverse)
    );
    let mut icon = div().mirror_in_rtl(Direction::Rtl);
    assert_eq!(icon.style().scale, Some(gpui::point(-1., 1.)));
}
