//! Pictures a theme supplies for the named indicators (upstream
//! `IndicatorMap` and `IndicatorRegistry`, Indicator FR7). Every name upstream
//! lists:
//!
//! - `check` (family `singleSelection`: `unchecked`, `checked`): the mark on a
//!   chosen option, CheckIndicator by default;
//! - `radio` (`singleSelection`): the circle of a radio control, RadioIndicator by
//!   default;
//! - `checkbox` (`multiSelection`: `unchecked`, `checked`, `indeterminate`): the
//!   box of a checkbox control, CheckboxIndicator by default;
//! - `spinner` (`activity`: `busy`): no built-in, Spinner draws its own ring;
//! - `tree-disclosure` (`disclosure`: `collapsed`, `expanded`): no built-in,
//!   TreeList's row draws its rotating chevron.
//!
//! A theme's entry overrides the built-in by name, so every component that renders
//! that indicator follows (upstream `getIndicator`: the theme's override, else the
//! default). With no entry, each host draws its own picture.
//!
//! ```ignore
//! set_theme_indicator(SPINNER, |request, _, _| {
//!     svg().path("icons/dots.svg").size(request.size).into_any_element()
//! }, cx);
//! ```

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{AnyElement, App, Global, Pixels, SharedString, Window};

use super::ChoiceSize;

/// The `check` indicator (family `singleSelection`): the mark on a chosen option.
pub const CHECK: &str = "check";
/// The `radio` indicator (family `singleSelection`): the circle of a radio control.
pub const RADIO: &str = "radio";
/// The `checkbox` indicator (family `multiSelection`): the box of a checkbox
/// control, including its partial state.
pub const CHECKBOX: &str = "checkbox";
/// The `spinner` indicator (family `activity`).
pub const SPINNER: &str = "spinner";
/// The `tree-disclosure` indicator (family `disclosure`).
pub const TREE_DISCLOSURE: &str = "tree-disclosure";

/// What a host asks a theme's indicator to draw (upstream `IndicatorProps`).
#[derive(Default)]
pub struct IndicatorRequest {
    /// Upstream `state`, in its family's words: `unchecked`, `checked` or
    /// `indeterminate`; `busy`; `collapsed` or `expanded`.
    pub state: &'static str,
    /// The box the host gives it.
    pub size: Pixels,
    /// Upstream `size` of a choice indicator (`check`, `radio`, `checkbox`): the
    /// owning control's size. `Md` for the others.
    pub control_size: ChoiceSize,
    /// Upstream `isDisabled`: the owning control is disabled. Purely visual: the owner
    /// keeps the disabled semantics.
    pub disabled: bool,
    /// The owner row's GPUI group: hovering the row tints the indicator (upstream
    /// `indicatorScope`, the CSS ancestor marker).
    pub hover_group: Option<SharedString>,
    /// Upstream `children`: content drawn inside the indicator instead of the state
    /// mark, such as a busy spinner.
    pub children: Vec<AnyElement>,
}

/// Draws a theme's indicator.
pub type ThemeIndicatorFn = Rc<dyn Fn(IndicatorRequest, &mut Window, &mut App) -> AnyElement>;

#[derive(Default)]
struct ThemeIndicators(HashMap<&'static str, ThemeIndicatorFn>);

impl Global for ThemeIndicators {}

/// Gives `name` (`CHECK`, `RADIO`, `CHECKBOX`, `SPINNER` or `TREE_DISCLOSURE`) the
/// theme's picture (upstream `defineTheme({indicators})`).
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

/// The theme's picture for `name`, upstream `useThemeIndicator` (the theme's entry
/// alone; a host with a built-in falls back to it, as upstream `useIndicator` does).
pub fn theme_indicator(name: &str, cx: &App) -> Option<ThemeIndicatorFn> {
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

    #[test]
    fn the_registry_names_every_indicator_upstream_lists() {
        // Upstream `IndicatorMap`: check, radio, checkbox, spinner, tree-disclosure.
        assert_eq!(
            [CHECK, RADIO, CHECKBOX, SPINNER, TREE_DISCLOSURE],
            ["check", "radio", "checkbox", "spinner", "tree-disclosure"]
        );
    }

    type Drawn = std::rc::Rc<std::cell::RefCell<Vec<(&'static str, bool, ChoiceSize)>>>;

    /// A theme picture for `name` that records each request it draws.
    fn record(name: &'static str, drawn: &Drawn, cx: &mut gpui::VisualTestContext) {
        let drawn = drawn.clone();
        cx.update(|_, cx| {
            set_theme_indicator(
                name,
                move |request, _, _| {
                    drawn.borrow_mut().push((
                        request.state,
                        request.disabled,
                        request.control_size,
                    ));
                    div().size(request.size).into_any_element()
                },
                cx,
            )
        });
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }

    struct Choices;

    impl Render for Choices {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            use crate::choice::{CheckboxInput, RadioList, RadioListItem};
            use crate::indicator::{CheckIndicator, IndicatorState};
            div()
                .child(
                    CheckboxInput::new("a", "Select photo", IndicatorState::Indeterminate)
                        .disabled(true),
                )
                .child(
                    RadioList::new("r", "Plan", "pro")
                        .item(RadioListItem::new("basic", "Basic"))
                        .item(RadioListItem::new("pro", "Pro")),
                )
                .child(CheckIndicator::new(true).size(ChoiceSize::Sm))
        }
    }

    #[gpui::test]
    fn fr7_a_theme_entry_replaces_the_built_in_choice_indicators_in_every_host(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            astryx_theme::init(cx);
            astryx_primitives::focus::init(cx);
        });
        let (_, cx) = cx.add_window_view(|_, _| Choices);
        cx.run_until_parked();
        let (checkbox, radio, check) = (Drawn::default(), Drawn::default(), Drawn::default());
        record(CHECKBOX, &checkbox, cx);
        record(RADIO, &radio, cx);
        record(CHECK, &check, cx);
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        assert_eq!(
            checkbox.borrow().last(),
            Some(&("indeterminate", true, ChoiceSize::Md))
        );
        let radios = radio.borrow().clone();
        let last_two = &radios[radios.len() - 2..];
        assert_eq!(
            last_two,
            [
                ("unchecked", false, ChoiceSize::Md),
                ("checked", false, ChoiceSize::Md)
            ]
        );
        assert_eq!(
            check.borrow().last(),
            Some(&("checked", false, ChoiceSize::Sm))
        );
        for name in [CHECKBOX, RADIO, CHECK] {
            cx.update(|_, cx| clear_theme_indicator(name, cx));
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
