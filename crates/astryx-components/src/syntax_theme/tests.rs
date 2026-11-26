//! SyntaxTheme tests.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{IntoElement, ParentElement, RenderOnce, TestAppContext, div};

use super::*;
use crate::segmented::tests::host;

type Seen = Rc<RefCell<Vec<(SyntaxColors, bool)>>>;

#[derive(IntoElement)]
struct Probe(Seen);

impl RenderOnce for Probe {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let colors = syntax_colors(window, cx);
        let hooked = use_syntax_theme(window, cx).is_some();
        self.0.borrow_mut().push((colors, hooked));
        div()
    }
}

#[gpui::test]
fn a_syntax_theme_colors_its_subtree_and_the_theme_colors_the_rest(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let cx = host(cx, {
        let seen = seen.clone();
        move || {
            seen.borrow_mut().clear();
            div()
                .child(Probe(seen.clone()))
                .child(syntax_theme(astryx_syntax::dracula(), Probe(seen.clone())))
                .into_any_element()
        }
    });
    cx.run_until_parked();
    let seen = seen.borrow();
    let (outside, outside_hook) = seen[0];
    let (inside, inside_hook) = seen[1];
    assert!(
        !outside_hook,
        "useSyntaxTheme is None outside a SyntaxTheme"
    );
    assert!(inside_hook);
    let pink: gpui::Hsla = gpui::Rgba::try_from("#ff79c6").unwrap().into();
    assert_eq!(inside.keyword, pink);
    assert_ne!(outside.keyword, pink);
    // Dracula sets no diff markers: they follow its punctuation.
    assert_eq!(inside.added_marker, inside.punctuation);
    assert_eq!(inside.diff_header, gpui::transparent_black());
}
