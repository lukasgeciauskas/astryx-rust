//! Pictures a theme supplies for the indicators Core reads but does not ship
//! (upstream `IndicatorMap`, Indicator FR7): `spinner` (state `busy`),
//! which Spinner draws, and `tree-disclosure` (states `collapsed` and
//! `expanded`), which TreeList's chevron draws. With no entry, each host
//! draws its own picture.
//!
//! ```ignore
//! set_theme_indicator(SPINNER, |request, _, _| {
//!     svg().path("icons/dots.svg").size(request.size).into_any_element()
//! }, cx);
//! ```

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{AnyElement, App, Global, Pixels, Window};

/// The `spinner` indicator (family `activity`).
pub const SPINNER: &str = "spinner";
/// The `tree-disclosure` indicator (family `disclosure`).
pub const TREE_DISCLOSURE: &str = "tree-disclosure";

/// What a host asks a theme's indicator to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IndicatorRequest {
    /// `busy`, `collapsed` or `expanded`.
    pub state: &'static str,
    /// The box the host gives it.
    pub size: Pixels,
}

/// Draws a theme's indicator.
pub type ThemeIndicatorFn = Rc<dyn Fn(IndicatorRequest, &mut Window, &mut App) -> AnyElement>;

#[derive(Default)]
struct ThemeIndicators(HashMap<&'static str, ThemeIndicatorFn>);

impl Global for ThemeIndicators {}

/// Gives `name` (`SPINNER` or `TREE_DISCLOSURE`) the theme's picture.
pub fn set_theme_indicator(
    name: &'static str,
    draw: impl Fn(IndicatorRequest, &mut Window, &mut App) -> AnyElement + 'static,
    cx: &mut App,
) {
    cx.default_global::<ThemeIndicators>()
        .0
        .insert(name, Rc::new(draw));
    cx.refresh_windows();
}

/// Removes the theme's picture for `name`: its host draws its own again.
pub fn clear_theme_indicator(name: &'static str, cx: &mut App) {
    if let Some(indicators) = cx.try_global::<ThemeIndicators>()
        && indicators.0.contains_key(name)
    {
        cx.global_mut::<ThemeIndicators>().0.remove(name);
        cx.refresh_windows();
    }
}

/// The theme's picture for `name`, upstream `useThemeIndicator`.
pub(crate) fn theme_indicator(name: &str, cx: &App) -> Option<ThemeIndicatorFn> {
    cx.try_global::<ThemeIndicators>()
        .and_then(|i| i.0.get(name).cloned())
}

#[cfg(test)]
mod tests {
    use gpui::{
        Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, TestAppContext,
        Window, div,
    };

    use super::*;
    use crate::status::Spinner;

    struct Host;

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().child(Spinner::new("s"))
        }
    }

    #[gpui::test]
    fn fr7_a_theme_spinner_replaces_the_ring(cx: &mut TestAppContext) {
        cx.update(astryx_theme::init);
        let (_, cx) = cx.add_window_view(|_, _| Host);
        cx.run_until_parked();
        assert!(cx.debug_bounds("theme-spinner").is_none());
        cx.update(|_, cx| {
            set_theme_indicator(
                SPINNER,
                |request, _, _| {
                    assert_eq!(request.state, "busy");
                    div()
                        .debug_selector(|| "theme-spinner".into())
                        .size(request.size)
                        .into_any_element()
                },
                cx,
            )
        });
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        assert!(cx.debug_bounds("theme-spinner").is_some());
        cx.update(|_, cx| clear_theme_indicator(SPINNER, cx));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        assert!(cx.debug_bounds("theme-spinner").is_none());
    }
}
